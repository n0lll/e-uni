//! Single-threaded reference stepper. Phase order is P0–P7 / P5a–P5f (ch. 34.3).

use crate::audit::{compute_mu, energy_identity_holds};
use crate::brain::{accumulate_interval, decide, tick_reward};
use crate::fields::phase_p5a;
use crate::init::lighting_centroid;
use crate::ledger::{commit_e_new_into_old, BodyView};
use crate::physics::{physics_substeps, wrap_angle};
use crate::physiology::{
    deposit_death, is_irrecoverable, maintenance_cost, pay_maintenance_parts, scale_control_parts,
};
use crate::probe::{
    budget_p, ControlMode, DecisionDiag, DecisionSkip, FaultPoint, ObservationFrame, RunLedger,
    TickProbe,
};
use crate::react::{phase_p5b_uptake, phase_p5c_photo, phase_p5d_metab};
use crate::repro::{phase_births, phase_build_embryos, phase_start_embryos};
use crate::sense::sense_observation;
use universe_types::{
    rules_hash, Accounts, LifeEvent, LifeEventKind, LightingRules, NumericFault, Organism, SimError,
    WorldState, RuleSpec, INPUTS,
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
    /// E_new actually produced by catabolism this tick (not an E-balance residual).
    pub last_metab_e_new: u64,
    pub inject_fault: Option<FaultPoint>,
    pub control_mode: ControlMode,
    pub enable_probe: bool,
    pub last_probe: TickProbe,
    pub run_ledger: RunLedger,
    pub last_decisions: Vec<DecisionDiag>,
    pub last_skips: Vec<DecisionSkip>,
    pub last_observation: std::collections::HashMap<u64, ObservationFrame>,
    last_obs: std::collections::HashMap<u64, [f64; INPUTS]>,
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
            last_metab_e_new: 0,
            inject_fault: None,
            control_mode: ControlMode::Neural,
            enable_probe: false,
            last_probe: TickProbe::default(),
            run_ledger: RunLedger::default(),
            last_decisions: Vec::new(),
            last_skips: Vec::new(),
            last_observation: std::collections::HashMap::new(),
            last_obs: std::collections::HashMap::new(),
        }
    }

    pub fn apply_runtime(
        &mut self,
        freeze_learning: bool,
        disable_active_motion: bool,
        disable_reproduction: bool,
        control_mode: ControlMode,
    ) {
        self.freeze_learning = freeze_learning;
        self.disable_active_motion = disable_active_motion;
        self.disable_reproduction = disable_reproduction;
        self.control_mode = control_mode;
    }

    /// Versioned lighting change: executing RuleSpec and state's rules_hash stay aligned.
    pub fn apply_lighting(&mut self, lighting: LightingRules) -> Result<(), SimError> {
        self.spec.lighting = lighting;
        self.state.rules_hash = rules_hash(&self.spec)?;
        Ok(())
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
        let committed = self.state.clone();
        let ledger = self.run_ledger.clone();
        let probe = self.last_probe.clone();
        let decisions = self.last_decisions.clone();
        let skips = self.last_skips.clone();
        let frames = self.last_observation.clone();
        let obs = self.last_obs.clone();
        let metab_diss = self.last_metab_diss;
        let metab_e = self.last_metab_e_new;
        match self.step_apply() {
            Ok(()) => Ok(()),
            Err(e) => {
                self.state = committed;
                self.last_events.clear();
                self.run_ledger = ledger;
                self.last_probe = probe;
                self.last_decisions = decisions;
                self.last_skips = skips;
                self.last_observation = frames;
                self.last_obs = obs;
                self.last_metab_diss = metab_diss;
                self.last_metab_e_new = metab_e;
                if matches!(&e, SimError::Numeric(NumericFault::Overflow("contact_pairs"))) {
                    return Err(SimError::SafetyPause(
                        "candidate contact pairs exceed safety threshold".into(),
                    ));
                }
                Err(e)
            }
        }
    }

    fn maybe_fault(&self, p: FaultPoint) -> Result<(), SimError> {
        if self.inject_fault == Some(p) {
            return Err(NumericFault::Overflow(p.tag()).into());
        }
        Ok(())
    }

    fn step_apply(&mut self) -> Result<(), SimError> {
        self.last_events.clear();
        self.state.audit.step_dissipation = 0;
        self.state.audit.step_radiation_input = 0;
        self.state.audit.step_radiation_loss = 0;
        let mut probe = TickProbe::default();
        let chem = self.spec.reactions.chem_e_per_organic;
        let u_old = compute_mu(&self.state, chem)?.u;
        let m_old = compute_mu(&self.state, chem)?.m;

        let mut e0: std::collections::HashMap<u64, i128> = std::collections::HashMap::new();
        let mut b0: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
        for org in &self.state.organisms {
            if org.marked_dead {
                continue;
            }
            e0.insert(org.entity_id, e_tot(org));
            b0.insert(org.entity_id, org.accounts.b);
        }
        if self.run_ledger.p_initial.is_none() {
            if let Some(org) = self.state.organisms.iter().find(|o| !o.marked_dead) {
                self.run_ledger.p_initial = Some(budget_p(
                    org.accounts.e_old,
                    org.accounts.e_new,
                    org.accounts.c,
                ));
            }
        }

        // P0: unsupported future_inputs are rejected; observation uses this boundary.
        if !self.state.future_inputs.is_empty() {
            return Err(SimError::ExperimentSetup(
                "non-empty future_inputs are not executed and must not be silently ignored".into(),
            ));
        }
        let p0_accounts: std::collections::HashMap<u64, Accounts> = self
            .state
            .organisms
            .iter()
            .filter(|o| !o.marked_dead)
            .map(|o| (o.entity_id, o.accounts))
            .collect();

        // P1: maintenance.
        let present: Vec<u64> = self
            .state
            .organisms
            .iter()
            .filter(|o| !o.marked_dead)
            .map(|o| o.entity_id)
            .collect();
        let decision = self.state.tick % self.spec.time.decision_every_ticks == 0;
        let cog = self.spec.cognition_cost_b();
        let n_org = self.state.organisms.len();
        let mut will_decide = vec![false; n_org];
        if decision {
            self.last_observation.clear();
            self.last_skips.clear();
            for i in 0..n_org {
                if self.state.organisms[i].marked_dead {
                    continue;
                }
                let id = self.state.organisms[i].entity_id;
                let has_emb = self.state.embryos.iter().any(|e| e.parent_id == id);
                if cognition_eligible(&self.state.organisms[i], &self.spec, has_emb) {
                    will_decide[i] = true;
                    {
                        let org = &mut self.state.organisms[i];
                        org.last_feedback = org.feedback;
                        org.feedback = universe_types::IntervalFeedback::default();
                    }
                    let org = &self.state.organisms[i];
                    let tick = self.state.tick;
                    let fb_ticks = org.last_feedback.ticks;
                    let start = tick.saturating_sub(fb_ticks);
                    let obs = sense_observation(
                        &self.state,
                        &self.spec,
                        org,
                        p0_accounts[&id],
                        org.last_feedback,
                    );
                    self.last_observation.insert(
                        id,
                        ObservationFrame {
                            entity_id: id,
                            observation_state_tick: tick,
                            feedback_start_tick: start,
                            feedback_end_tick: tick,
                            feedback_ticks: fb_ticks,
                            obs,
                            p0_e_old: p0_accounts[&id].e_old,
                            p0_c: p0_accounts[&id].c,
                        },
                    );
                } else {
                    let org = &self.state.organisms[i];
                    let reason = if org.accounts.e_old < maintenance_cost(org, &self.spec, has_emb)
                    {
                        "maintenance_unpaid"
                    } else {
                        "no_cognition_energy"
                    };
                    self.last_skips.push(DecisionSkip {
                        tick: self.state.tick,
                        entity_id: id,
                        reason: reason.into(),
                        e_old: org.accounts.e_old,
                        open_feedback_ticks: org.feedback.ticks,
                    });
                }
            }
        }

        let mut maint_diss = 0u64;
        let mut maint_ok = vec![false; n_org];
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
            let cost = maintenance_cost(org, &self.spec, has_emb);
            let pay = pay_maintenance_parts(org, &self.spec, has_emb)?;
            probe.maint_paid = probe
                .maint_paid
                .checked_add(pay.maint)
                .ok_or(NumericFault::Overflow("maint_p"))?;
            probe.debt_repay = probe
                .debt_repay
                .checked_add(pay.repay)
                .ok_or(NumericFault::Overflow("repay_p"))?;
            maint_diss = maint_diss
                .checked_add(pay.total())
                .ok_or(NumericFault::Overflow("maint_sum"))?;
            let unpaid_now = org.maintenance_debt > debt0;
            maint_ok[i] = !unpaid_now && (pay.total() >= cost || cost == 0);
            org.cognition_paid = false;
        }
        self.add_diss(maint_diss)?;

        if decision {
            for (i, org) in self.state.organisms.iter_mut().enumerate() {
                if org.marked_dead {
                    continue;
                }
                if will_decide[i] && org.accounts.e_old >= cog {
                    org.accounts.e_old -= cog;
                    self.state.audit.step_dissipation = self
                        .state
                        .audit
                        .step_dissipation
                        .checked_add(cog)
                        .ok_or(NumericFault::Overflow("cog"))?;
                    org.cognition_paid = true;
                    probe.cognition = probe
                        .cognition
                        .checked_add(cog)
                        .ok_or(NumericFault::Overflow("cog_p"))?;
                    self.run_ledger.decision_count = self.run_ledger.decision_count.saturating_add(1);
                }
            }
        }

        add_interval_delta(&mut self.state, &e0, &b0)?;
        snapshot_eb(&self.state, &mut e0, &mut b0);

        self.maybe_fault(FaultPoint::AfterP1)?;

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
                        let id = self.state.organisms[i].entity_id;
                        let frame = self
                            .last_observation
                            .get(&id)
                            .cloned()
                            .ok_or_else(|| {
                                SimError::ExperimentSetup("missing ObservationFrame for paid cognition".into())
                            })?;
                        let seed = self.state.root_seed;
                        let tick = self.state.tick;
                        let freeze = self.freeze_learning;
                        let allow_repro = {
                            let org = &self.state.organisms[i];
                            org.age >= self.spec.evolution.min_age
                                && org.accounts.b * self.spec.evolution.min_b_over_target_denom
                                    >= self.spec.evolution.min_b_over_target_numer * org.b_target
                                && org.maintenance_debt == 0
                                && !self
                                    .state
                                    .embryos
                                    .iter()
                                    .any(|e| e.parent_id == org.entity_id)
                                && !self.disable_reproduction
                        };
                        let mut diag = decide(
                            &self.spec,
                            &mut self.state.organisms[i],
                            &g,
                            &frame.obs,
                            seed,
                            tick,
                            freeze,
                            allow_repro,
                        )?;
                        diag.observation_state_tick = frame.observation_state_tick;
                        diag.feedback_start_tick = frame.feedback_start_tick;
                        diag.feedback_end_tick = frame.feedback_end_tick;
                        diag.feedback_ticks = frame.feedback_ticks;
                        diag.obs = frame.obs;
                        diag.actual_forward_displacement =
                            self.state.organisms[i].last_feedback.thrust_sum;
                        diag.actual_unwrapped_turn = self.state.organisms[i].last_feedback.turn_sum;
                        if diag.applied_update {
                            self.run_ledger.learning_update_count =
                                self.run_ledger.learning_update_count.saturating_add(1);
                        }
                        if self.enable_probe {
                            diag.input_change = if let Some(prev) = self.last_obs.get(&id) {
                                l2_delta(prev, &frame.obs)
                            } else {
                                0.0
                            };
                            self.last_obs.insert(id, frame.obs);
                            self.last_decisions.push(diag);
                        }
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
                zero_control(org);
            }
        }

        if decision {
            self.apply_diagnostic_control();
        }

        self.maybe_fault(FaultPoint::AfterP2)?;

        // P3: pay motion attempt.
        let mut motion_diss = 0u64;
        let mut paid_rows: Vec<(u64, i8, i8, f64, f64, u64)> = Vec::new();
        for org in &mut self.state.organisms {
            if org.marked_dead {
                continue;
            }
            let pay = scale_control_parts(org, &self.spec)?;
            probe.thrust_attempt = probe
                .thrust_attempt
                .checked_add(pay.thrust)
                .ok_or(NumericFault::Overflow("thr_p"))?;
            probe.turn_attempt = probe
                .turn_attempt
                .checked_add(pay.turn)
                .ok_or(NumericFault::Overflow("turn_p"))?;
            motion_diss = motion_diss
                .checked_add(pay.total())
                .ok_or(NumericFault::Overflow("motion_sum"))?;
            paid_rows.push((
                org.entity_id,
                org.control.thrust,
                org.control.turn,
                org.control.u_effective,
                org.control.w_effective,
                pay.total(),
            ));
        }
        self.add_diss(motion_diss)?;
        let tick_now = self.state.tick;
        for (eid, pu, pw, ue, we, energy) in paid_rows {
            if let Some(last) = self
                .last_decisions
                .iter_mut()
                .rev()
                .find(|d| d.tick == tick_now && d.entity_id == eid)
            {
                last.paid_thrust = pu;
                last.paid_turn = pw;
                last.u_effective = ue;
                last.w_effective = we;
                last.paid_motion_energy = energy;
            }
        }

        // P4
        physics_substeps(&mut self.state, &self.spec)?;
        for org in &self.state.organisms {
            if org.marked_dead {
                continue;
            }
            let scaled = (org.last_dtheta.abs() * 1e9).round().max(0.0) as u64;
            probe.abs_dtheta = probe.abs_dtheta.saturating_add(scaled);
        }
        self.maybe_fault(FaultPoint::AfterP4)?;

        // P5a
        let (input, loss) = phase_p5a(&mut self.state.fields, &self.spec)?;
        self.state.audit.step_radiation_input = input;
        self.state.audit.step_radiation_loss = loss;
        self.maybe_fault(FaultPoint::AfterP5a)?;

        // P5b–P5d
        let (up_n, up_o) = phase_p5b_uptake(&mut self.state, &self.spec)?;
        probe.uptake_n = up_n;
        probe.uptake_o = up_o;
        let (d, photo) = phase_p5c_photo(&mut self.state, &self.spec)?;
        probe.photo_extent = photo;
        self.add_diss(d)?;
        let (d, metab_e) = phase_p5d_metab(&mut self.state, &self.spec)?;
        self.last_metab_diss = d;
        self.last_metab_e_new = metab_e;
        probe.metab_e_new = metab_e;
        self.add_diss(d)?;

        // P5f: embryo start then construction. Birth is P6.
        if !self.disable_reproduction {
            let (d, ev) = phase_start_embryos(&mut self.state, &self.spec)?;
            probe.repro_start = d;
            self.add_diss(d)?;
            self.last_events.extend(ev);
            let d = phase_build_embryos(&mut self.state, &self.spec)?;
            probe.repro_build = d;
            self.add_diss(d)?;
        }
        self.maybe_fault(FaultPoint::AfterP5f)?;

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
            add_one_interval_delta(&mut self.state.organisms[i], &e0, &b0)?;
            let org = &self.state.organisms[i];
            let p_death = budget_p(org.accounts.e_old, org.accounts.e_new, org.accounts.c);
            self.run_ledger.p_before_death_or_end = Some(p_death);
            self.run_ledger.death_tick = Some(tick);
            self.run_ledger.death_cause = Some(if 5 * org.accounts.b < org.b_target {
                "structure".into()
            } else {
                "debt".into()
            });
            // Reward dying bodies before wiping accounts (ch. 37.4).
            let r = tick_reward(&self.state.organisms[i], &self.spec, 0, true);
            accumulate_interval(&mut self.state.organisms[i].brain, r, &self.spec);
            {
                let fb = &mut self.state.organisms[i].feedback;
                fb.ticks += 1;
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
            }
            self.last_events.extend(ev);
        }
        self.maybe_fault(FaultPoint::AfterP6Alloc)?;

        for i in 0..self.state.organisms.len() {
            if self.state.organisms[i].marked_dead {
                continue;
            }
            add_one_interval_delta(&mut self.state.organisms[i], &e0, &b0)?;
            let id = self.state.organisms[i].entity_id;
            let births = births_this.get(&id).copied().unwrap_or(0);
            let r = tick_reward(&self.state.organisms[i], &self.spec, births, false);
            accumulate_interval(&mut self.state.organisms[i].brain, r, &self.spec);
            let fb = &mut self.state.organisms[i].feedback;
            fb.ticks += 1;
        }
        self.state.organisms.retain(|o| !o.marked_dead);
        gc_unreferenced_genomes(&mut self.state);
        for org in &mut self.state.organisms {
            if present.contains(&org.entity_id) {
                org.age = org.age.saturating_add(1);
            }
        }

        self.maybe_fault(FaultPoint::BeforeP7)?;

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

        fill_body_probe(&self.state, &mut probe);
        self.last_probe = probe.clone();
        self.run_ledger.accumulate(&probe);
        if self.run_ledger.death_tick.is_none() {
            self.run_ledger.p_before_death_or_end = Some(probe.p);
        }

        self.state.tick = self
            .state
            .tick
            .checked_add(1)
            .ok_or(NumericFault::Overflow("tick"))?;
        Ok(())
    }

    fn apply_diagnostic_control(&mut self) {
        match self.control_mode {
            ControlMode::Neural => {}
            ControlMode::Stationary => {
                for org in &mut self.state.organisms {
                    if org.cognition_paid {
                        zero_control(org);
                    }
                }
            }
            ControlMode::Forced { thrust, turn } => {
                for org in &mut self.state.organisms {
                    if !org.cognition_paid {
                        continue;
                    }
                    zero_control(org);
                    org.control.thrust = thrust;
                    org.control.turn = turn;
                }
            }
            ControlMode::OracleGotoLight => {
                let target = lighting_centroid(&self.spec);
                for org in &mut self.state.organisms {
                    if !org.cognition_paid {
                        continue;
                    }
                    zero_control(org);
                    oracle_goto(org, target);
                }
            }
            ControlMode::ReflexQ => {
                let n = self.state.organisms.len();
                for i in 0..n {
                    if self.state.organisms[i].marked_dead
                        || !self.state.organisms[i].cognition_paid
                    {
                        continue;
                    }
                    let id = self.state.organisms[i].entity_id;
                    let frame = match self.last_observation.get(&id) {
                        Some(f) => f.clone(),
                        None => continue,
                    };
                    let org = &mut self.state.organisms[i];
                    zero_control(org);
                    reflex_q(org, &self.spec, &frame);
                }
            }
        }
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

fn e_tot(org: &Organism) -> i128 {
    i128::from(org.accounts.e_old) + i128::from(org.accounts.e_new)
}

fn snapshot_eb(
    state: &WorldState,
    e0: &mut std::collections::HashMap<u64, i128>,
    b0: &mut std::collections::HashMap<u64, u64>,
) {
    e0.clear();
    b0.clear();
    for org in &state.organisms {
        if org.marked_dead {
            continue;
        }
        e0.insert(org.entity_id, e_tot(org));
        b0.insert(org.entity_id, org.accounts.b);
    }
}

fn add_interval_delta(
    state: &mut WorldState,
    e0: &std::collections::HashMap<u64, i128>,
    b0: &std::collections::HashMap<u64, u64>,
) -> Result<(), NumericFault> {
    for org in &mut state.organisms {
        if org.marked_dead {
            continue;
        }
        add_one_interval_delta(org, e0, b0)?;
    }
    Ok(())
}

fn add_one_interval_delta(
    org: &mut Organism,
    e0: &std::collections::HashMap<u64, i128>,
    b0: &std::collections::HashMap<u64, u64>,
) -> Result<(), NumericFault> {
    if let Some(&e_prev) = e0.get(&org.entity_id) {
        org.feedback.e_net_i128 += e_tot(org) - e_prev;
    }
    if let Some(&b_prev) = b0.get(&org.entity_id) {
        if org.accounts.b < b_prev {
            let loss = b_prev - org.accounts.b;
            org.feedback.struct_loss = org
                .feedback
                .struct_loss
                .checked_add(loss)
                .ok_or(NumericFault::Overflow("struct_loss"))?;
        }
    }
    Ok(())
}

fn fill_body_probe(state: &WorldState, probe: &mut TickProbe) {
    probe.living = state
        .organisms
        .iter()
        .filter(|o| !o.marked_dead)
        .count() as u32;
    if let Some(org) = state.organisms.iter().find(|o| !o.marked_dead) {
        probe.e_old = org.accounts.e_old;
        probe.e_new = org.accounts.e_new;
        probe.c = org.accounts.c;
        probe.n = org.accounts.n;
        probe.b = org.accounts.b;
        probe.debt = org.maintenance_debt;
        probe.p = budget_p(org.accounts.e_old, org.accounts.e_new, org.accounts.c);
        probe.x_bits = org.geom.x.to_bits();
        probe.y_bits = org.geom.y.to_bits();
    }
}

fn l2_delta(a: &[f64; INPUTS], b: &[f64; INPUTS]) -> f64 {
    let mut s = 0.0;
    for i in 0..INPUTS {
        let d = a[i] - b[i];
        s += d * d;
    }
    libm::sqrt(s)
}

fn oracle_goto(org: &mut Organism, target: (f64, f64)) {
    let dx = target.0 - org.geom.x;
    let dy = target.1 - org.geom.y;
    let dist = libm::sqrt(dx * dx + dy * dy);
    if dist < 0.4 {
        org.control.thrust = 0;
        org.control.turn = 0;
        return;
    }
    let desired = libm::atan2(dy, dx);
    let err = wrap_angle(desired - org.geom.theta);
    if err.abs() > 0.2 {
        org.control.turn = if err > 0.0 { 1 } else { -1 };
        org.control.thrust = if err.abs() < std::f64::consts::FRAC_PI_2 {
            1
        } else {
            0
        };
    } else {
        org.control.turn = 0;
        org.control.thrust = 1;
    }
}

fn reflex_q(org: &mut Organism, spec: &RuleSpec, frame: &ObservationFrame) {
    let obs = &frame.obs;
    let ticks = org.last_feedback.ticks.max(1);
    let photo_per = org.last_feedback.photo_extent as f64 / ticks as f64;
    let p_in = photo_per * 8.0;
    let need = maintenance_cost(org, spec, false) as f64
        + spec.cognition_cost_b() as f64 / spec.time.decision_every_ticks as f64;
    if org.last_feedback.photo_extent > 0 && p_in + 1.0 >= need {
        org.control.thrust = 0;
        org.control.turn = 0;
        return;
    }
    let mut best = 0usize;
    let mut best_q = f64::NEG_INFINITY;
    for d in 0..4 {
        let valid = obs[32 + d * 4 + 3] > 0.5;
        let q = obs[d * 4 + 3];
        if valid && q > best_q {
            best_q = q;
            best = d;
        }
    }
    match best {
        0 => {
            org.control.thrust = 1;
            org.control.turn = 0;
        }
        1 => {
            org.control.thrust = 1;
            org.control.turn = 1;
        }
        2 => {
            org.control.thrust = 0;
            org.control.turn = 1;
        }
        _ => {
            org.control.thrust = 1;
            org.control.turn = -1;
        }
    }
}

fn cognition_eligible(org: &Organism, spec: &RuleSpec, has_emb: bool) -> bool {
    let cost = maintenance_cost(org, spec, has_emb);
    if org.accounts.e_old < cost {
        return false;
    }
    let mut e = org.accounts.e_old - cost;
    let debt = org.maintenance_debt;
    if debt > 0 && e > 0 {
        let repay = e.min(spec.physiology.debt_repay_per_tick).min(debt);
        e -= repay;
    }
    e >= spec.cognition_cost_b()
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
