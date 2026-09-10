//! Single-threaded reference stepper. Phase order is P0–P7 / P5a–P5f (ch. 34.3).

use crate::audit::{compute_mu, energy_identity_holds};
use crate::brain::{accumulate_interval, decide, tick_reward};
use crate::fields::phase_p5a;
use crate::ledger::{commit_e_new_into_old, BodyView};
use crate::physics::physics_substeps;
use crate::physiology::{deposit_death, is_irrecoverable, pay_maintenance, scale_control};
use crate::react::{phase_p5b_uptake, phase_p5c_photo, phase_p5d_metab};
use crate::repro::{phase_births, phase_build_embryos, phase_start_embryos};
use crate::sense::sense;
use universe_types::{
    LifeEvent, LifeEventKind, NumericFault, Organism, SimError, WorldState, RuleSpec,
};

#[derive(Clone, Debug)]
pub struct World {
    pub spec: RuleSpec,
    pub state: WorldState,
    pub freeze_learning: bool,
    pub disable_active_motion: bool,
    pub disable_reproduction: bool,
    /// Side channel: never encoded into `state_hash`.
    pub last_events: Vec<LifeEvent>,
    pub last_metab_diss: u64,
}

impl World {
    pub fn from_state(spec: RuleSpec, state: WorldState) -> Self {
        Self {
            spec,
            state,
            freeze_learning: false,
            disable_active_motion: false,
            disable_reproduction: false,
            last_events: Vec::new(),
            last_metab_diss: 0,
        }
    }

    pub fn step(&mut self) -> Result<(), SimError> {
        let living = self
            .state
            .organisms
            .iter()
            .filter(|o| !o.marked_dead)
            .count();
        if living > self.spec.safety.max_living as usize {
            return Err(SimError::SafetyPause(format!(
                "living {} exceeds {}",
                living, self.spec.safety.max_living
            )));
        }
        match self.step_apply() {
            Ok(()) => Ok(()),
            Err(e) => {
                if matches!(&e, SimError::Numeric(NumericFault::Overflow("contact_pairs"))) {
                    return Err(SimError::SafetyPause(
                        "candidate contact pairs exceed safety threshold".into(),
                    ));
                }
                Err(e)
            }
        }
    }

    fn step_apply(&mut self) -> Result<(), SimError> {
        self.last_events.clear();
        self.state.audit.step_dissipation = 0;
        self.state.audit.step_radiation_input = 0;
        self.state.audit.step_radiation_loss = 0;
        let chem = self.spec.reactions.chem_e_per_organic;
        let u_old = compute_mu(&self.state, chem)?.u;
        let m_old = compute_mu(&self.state, chem)?.m;

        // P0: future inputs (none unless queued).
        // P1: maintenance.
        let present: Vec<u64> = self
            .state
            .organisms
            .iter()
            .filter(|o| !o.marked_dead)
            .map(|o| o.entity_id)
            .collect();
        let mut maint_diss = 0u64;
        let mut maint_ok = vec![false; self.state.organisms.len()];
        for i in 0..self.state.organisms.len() {
            if self.state.organisms[i].marked_dead {
                continue;
            }
            let id = self.state.organisms[i].entity_id;
            let has_emb = self.state.embryos.iter().any(|e| e.parent_id == id);
            let org = &mut self.state.organisms[i];
            org.last_dx = 0.0;
            org.last_dy = 0.0;
            org.last_dtheta = 0.0;
            let debt0 = org.maintenance_debt;
            let cost = crate::physiology::maintenance_cost(org, &self.spec, has_emb);
            let d = pay_maintenance(org, &self.spec, has_emb)?;
            maint_diss = maint_diss
                .checked_add(d)
                .ok_or(NumericFault::Overflow("maint_sum"))?;
            let unpaid_now = org.maintenance_debt > debt0;
            maint_ok[i] = !unpaid_now && (d >= cost || cost == 0);
            org.cognition_paid = false;
        }
        self.add_diss(maint_diss)?;

        // Cognition fee on decision ticks.
        let decision = self.state.tick % self.spec.time.decision_every_ticks == 0;
        let cog = self.spec.cognition_cost_b();
        if decision {
            for (i, org) in self.state.organisms.iter_mut().enumerate() {
                if org.marked_dead {
                    continue;
                }
                if maint_ok[i] && org.accounts.e_old >= cog {
                    org.accounts.e_old -= cog;
                    self.state.audit.step_dissipation = self
                        .state
                        .audit
                        .step_dissipation
                        .checked_add(cog)
                        .ok_or(NumericFault::Overflow("cog"))?;
                    org.cognition_paid = true;
                }
            }
        }

        // P2
        if decision {
            let n = self.state.organisms.len();
            for i in 0..n {
                if self.state.organisms[i].marked_dead {
                    continue;
                }
                if self.state.organisms[i].cognition_paid {
                    let gh = self.state.organisms[i].genome_hash;
                    let g = self
                        .state
                        .genomes
                        .iter()
                        .find(|g| g.hash == gh)
                        .cloned();
                    if let Some(g) = g {
                        let obs = sense(&self.state, &self.spec, &self.state.organisms[i]);
                        let org = &self.state.organisms[i];
                        let allow_repro = org.age >= self.spec.evolution.min_age
                            && org.accounts.b * self.spec.evolution.min_b_over_target_denom
                                >= self.spec.evolution.min_b_over_target_numer * org.b_target
                            && org.maintenance_debt == 0
                            && !self
                                .state
                                .embryos
                                .iter()
                                .any(|e| e.parent_id == org.entity_id)
                            && !self.disable_reproduction;
                        let seed = self.state.root_seed;
                        let tick = self.state.tick;
                        let freeze = self.freeze_learning;
                        decide(
                            &self.spec,
                            &mut self.state.organisms[i],
                            &g,
                            &obs,
                            seed,
                            tick,
                            freeze,
                            allow_repro,
                        )?;
                    }
                } else {
                    zero_control(&mut self.state.organisms[i]);
                }
            }
        } else {
            for (i, org) in self.state.organisms.iter_mut().enumerate() {
                if org.marked_dead {
                    continue;
                }
                if !maint_ok[i] {
                    zero_control(org);
                }
            }
        }

        if self.disable_active_motion {
            for org in &mut self.state.organisms {
                org.control.thrust = 0;
                org.control.turn = 0;
                org.control.u_effective = 0.0;
                org.control.w_effective = 0.0;
                org.control.n_boost = false;
                org.control.o_uptake = false;
                org.control.photo_boost = false;
                org.control.metab_boost = false;
                org.control.repair = false;
                org.control.reproduce = false;
            }
        }

        // P3: pay motion attempt.
        let mut motion_diss = 0u64;
        for org in &mut self.state.organisms {
            if org.marked_dead {
                continue;
            }
            motion_diss = motion_diss
                .checked_add(scale_control(org, &self.spec)?)
                .ok_or(NumericFault::Overflow("motion_sum"))?;
        }
        self.add_diss(motion_diss)?;

        // P4
        physics_substeps(&mut self.state, &self.spec)?;

        // P5a
        let (input, loss) = phase_p5a(&mut self.state.fields, &self.spec)?;
        self.state.audit.step_radiation_input = input;
        self.state.audit.step_radiation_loss = loss;

        // P5b–P5d
        phase_p5b_uptake(&mut self.state, &self.spec)?;
        let d = phase_p5c_photo(&mut self.state, &self.spec)?;
        self.add_diss(d)?;
        let d = phase_p5d_metab(&mut self.state, &self.spec)?;
        self.last_metab_diss = d;
        self.add_diss(d)?;

        // P5f: embryo start then construction. Birth is P6.
        if !self.disable_reproduction {
            let (d, ev) = phase_start_embryos(&mut self.state, &self.spec)?;
            self.add_diss(d)?;
            self.last_events.extend(ev);
            let d = phase_build_embryos(&mut self.state, &self.spec)?;
            self.add_diss(d)?;
        }

        // P6: death and cancel, then births, then rewards on the final available accounts.
        let mut to_kill = Vec::new();
        for (i, org) in self.state.organisms.iter().enumerate() {
            if org.marked_dead {
                continue;
            }
            if is_irrecoverable(org, &self.spec) {
                to_kill.push(i);
            }
        }
        let tick = self.state.tick;
        for i in to_kill.iter().copied().rev() {
            let pid = self.state.organisms[i].entity_id;
            let mut ei = 0;
            while ei < self.state.embryos.len() {
                if self.state.embryos[ei].parent_id == pid {
                    let amt = self.state.embryos[ei].b.saturating_add(self.state.embryos[ei].c);
                    let d = crate::repro::cancel_embryo(&mut self.state, &self.spec, ei)?;
                    self.add_diss(d)?;
                    self.last_events.push(LifeEvent {
                        tick,
                        kind: LifeEventKind::EmbryoCancel,
                        entity_a: pid,
                        entity_b: 0,
                        amount: amt,
                    });
                } else {
                    ei += 1;
                }
            }
            // Reward dying bodies before wiping accounts (ch. 37.4).
            let r = tick_reward(&self.state.organisms[i], &self.spec, 0, true);
            accumulate_interval(&mut self.state.organisms[i].brain, r, &self.spec);
            let u_eff = self.state.organisms[i].control.u_effective;
            let w_eff = self.state.organisms[i].control.w_effective;
            {
                let fb = &mut self.state.organisms[i].feedback;
                fb.ticks += 1;
                fb.thrust_sum += u_eff;
                fb.turn_sum += w_eff;
                fb.died = true;
            }
            let d = deposit_death(&mut self.state, &self.spec, i)?;
            self.add_diss(d)?;
            self.last_events.push(LifeEvent {
                tick,
                kind: LifeEventKind::Death,
                entity_a: pid,
                entity_b: 0,
                amount: d,
            });
        }
        let mut births_this: std::collections::BTreeMap<u64, u64> = std::collections::BTreeMap::new();
        if !self.disable_reproduction {
            let (d, ev) = phase_births(&mut self.state, &self.spec)?;
            self.add_diss(d)?;
            for e in &ev {
                if e.kind == LifeEventKind::Birth {
                    *births_this.entry(e.entity_b).or_insert(0) += 1;
                }
                if e.kind == LifeEventKind::EmbryoCancel {
                    // already counted
                }
            }
            self.last_events.extend(ev);
        }
        for i in 0..self.state.organisms.len() {
            if self.state.organisms[i].marked_dead {
                continue;
            }
            let id = self.state.organisms[i].entity_id;
            let births = births_this.get(&id).copied().unwrap_or(0);
            let r = tick_reward(&self.state.organisms[i], &self.spec, births, false);
            accumulate_interval(&mut self.state.organisms[i].brain, r, &self.spec);
            let u_eff = self.state.organisms[i].control.u_effective;
            let w_eff = self.state.organisms[i].control.w_effective;
            let fb = &mut self.state.organisms[i].feedback;
            fb.ticks += 1;
            fb.thrust_sum += u_eff;
            fb.turn_sum += w_eff;
        }
        self.state.organisms.retain(|o| !o.marked_dead);
        gc_unreferenced_genomes(&mut self.state);
        for org in &mut self.state.organisms {
            if present.contains(&org.entity_id) {
                org.age = org.age.saturating_add(1);
            }
        }

        // P7 commit E_new
        for org in &mut self.state.organisms {
            let mut body = BodyView {
                n: org.accounts.n,
                c: org.accounts.c,
                b: org.accounts.b,
                e_old: org.accounts.e_old,
                e_new: org.accounts.e_new,
                n_max: org.n_max,
                c_max: org.c_max,
                e_max: org.e_max,
                b_target: org.b_target,
            };
            commit_e_new_into_old(&mut body)?;
            org.accounts.e_old = body.e_old;
            org.accounts.e_new = 0;
        }

        let mu = compute_mu(&self.state, chem)?;
        if mu.m != m_old {
            return Err(SimError::RuleViolation(format!(
                "mass not conserved: {m_old} -> {}",
                mu.m
            )));
        }
        if !energy_identity_holds(
            u_old,
            mu.u,
            u128::from(self.state.audit.step_dissipation),
            u128::from(self.state.audit.step_radiation_loss),
            u128::from(self.state.audit.step_radiation_input),
        )? {
            return Err(SimError::RuleViolation("energy identity failed".into()));
        }
        self.state.audit.cumulative_radiation_input = self
            .state
            .audit
            .cumulative_radiation_input
            .checked_add(u128::from(input))
            .ok_or(NumericFault::Overflow("cum_in"))?;
        self.state.audit.cumulative_radiation_loss = self
            .state
            .audit
            .cumulative_radiation_loss
            .checked_add(u128::from(loss))
            .ok_or(NumericFault::Overflow("cum_loss"))?;
        self.state.audit.cumulative_dissipation = self
            .state
            .audit
            .cumulative_dissipation
            .checked_add(u128::from(self.state.audit.step_dissipation))
            .ok_or(NumericFault::Overflow("cum_diss"))?;
        self.state.tick = self
            .state
            .tick
            .checked_add(1)
            .ok_or(NumericFault::Overflow("tick"))?;
        Ok(())
    }

    fn add_diss(&mut self, d: u64) -> Result<(), NumericFault> {
        self.state.audit.step_dissipation = self
            .state
            .audit
            .step_dissipation
            .checked_add(d)
            .ok_or(NumericFault::Overflow("step_diss"))?;
        Ok(())
    }
}

fn gc_unreferenced_genomes(state: &mut WorldState) {
    let mut keep = std::collections::BTreeSet::new();
    for o in &state.organisms {
        keep.insert(o.genome_hash);
    }
    for e in &state.embryos {
        keep.insert(e.genome_hash);
    }
    state.genomes.retain(|g| keep.contains(&g.hash));
    state.genomes.sort_by(|a, b| a.hash.cmp(&b.hash));
}

fn zero_control(org: &mut Organism) {
    org.control.thrust = 0;
    org.control.turn = 0;
    org.control.u_effective = 0.0;
    org.control.w_effective = 0.0;
    org.control.n_boost = false;
    org.control.o_uptake = false;
    org.control.photo_boost = false;
    org.control.metab_boost = false;
    org.control.repair = false;
    org.control.reproduce = false;
}
