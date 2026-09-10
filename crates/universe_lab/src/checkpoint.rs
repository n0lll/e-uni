//! Versioned F2 experiment checkpoint. Archive is not used.

use universe_core::probe::{ControlMode, DecisionAggregate, RunLedger, TickProbe};
use universe_core::world::World;
use universe_core::{budget_p_i128, validate_loaded_world};
use universe_types::{
    hex_hash, parse_lab_yaml, rules_hash, sha256, state_hash, CanonReader, CanonWriter, Hash32,
    LightingRules, NumericFault, SimError, WorldState, RuleSpec,
};

use crate::t_recover::{RecoverDetector, TickSample};

pub const CHECKPOINT_MAGIC: &[u8; 4] = b"EUCK";
pub const CHECKPOINT_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct LightEvent {
    pub tick: u64,
    pub lighting: LightingRules,
}

#[derive(Clone, Debug)]
pub struct ExperimentCheckpoint {
    pub definition_hash: Hash32,
    pub spec: RuleSpec,
    pub state: WorldState,
    pub freeze_learning: bool,
    pub disable_reproduction: bool,
    pub disable_active_motion: bool,
    pub control_mode: ControlMode,
    pub enable_probe: bool,
    pub probe_frame_limit: usize,
    pub schedule: Vec<LightEvent>,
    pub next_event: usize,
    pub t0_world: u64,
    pub run_ledger: RunLedger,
    pub last_probe: TickProbe,
    pub detector: RecoverDetector,
    pub probe_aggregate: DecisionAggregate,
    pub last_metab_diss: u64,
    pub last_metab_e_new: u64,
}

impl ExperimentCheckpoint {
    pub fn from_session(s: &ExperimentSession) -> Self {
        Self {
            definition_hash: s.definition_hash,
            spec: s.world.spec.clone(),
            state: s.world.state.clone(),
            freeze_learning: s.world.freeze_learning,
            disable_reproduction: s.world.disable_reproduction,
            disable_active_motion: s.world.disable_active_motion,
            control_mode: s.world.control_mode,
            enable_probe: s.world.enable_probe,
            probe_frame_limit: s.world.probe_frame_limit,
            schedule: s.schedule.clone(),
            next_event: s.next_event,
            t0_world: s.t0_world,
            run_ledger: s.world.run_ledger.clone(),
            last_probe: s.world.last_probe.clone(),
            detector: s.detector.clone(),
            probe_aggregate: s.world.probe_aggregate.clone(),
            last_metab_diss: s.world.last_metab_diss,
            last_metab_e_new: s.world.last_metab_e_new,
        }
    }

    pub fn into_session(self) -> Result<ExperimentSession, SimError> {
        validate_loaded_world(&self.spec, &self.state)?;
        let mut world = World::from_state_checked(self.spec, self.state)?;
        world.apply_runtime(
            self.freeze_learning,
            self.disable_active_motion,
            self.disable_reproduction,
            self.control_mode,
        );
        world.enable_probe = self.enable_probe;
        world.probe_frame_limit = self.probe_frame_limit;
        world.run_ledger = self.run_ledger;
        world.last_probe = self.last_probe;
        world.probe_aggregate = self.probe_aggregate;
        world.last_metab_diss = self.last_metab_diss;
        world.last_metab_e_new = self.last_metab_e_new;
        Ok(ExperimentSession {
            world,
            schedule: self.schedule,
            next_event: self.next_event,
            t0_world: self.t0_world,
            detector: self.detector,
            definition_hash: self.definition_hash,
            tick_log: Vec::new(),
            record_ticks: false,
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, SimError> {
        let yaml = serde_yaml::to_string(&self.spec)
            .map_err(|e| SimError::ExperimentSetup(format!("spec yaml: {e}")))?;
        let state = self
            .state
            .encode_canonical()
            .map_err(SimError::from)?;
        let mut w = CanonWriter::new();
        w.raw(CHECKPOINT_MAGIC);
        w.u32(CHECKPOINT_VERSION);
        w.bytes32(&self.definition_hash);
        w.blob(yaml.as_bytes());
        w.blob(&state);
        w.bool(self.freeze_learning);
        w.bool(self.disable_reproduction);
        w.bool(self.disable_active_motion);
        self.control_mode.encode_canonical(&mut w);
        w.bool(self.enable_probe);
        w.u64(self.probe_frame_limit as u64);
        w.u64(self.schedule.len() as u64);
        for ev in &self.schedule {
            w.u64(ev.tick);
            encode_lighting(&mut w, &ev.lighting);
        }
        w.u64(self.next_event as u64);
        w.u64(self.t0_world);
        encode_ledger(&mut w, &self.run_ledger);
        encode_probe(&mut w, &self.last_probe);
        self.detector.encode(&mut w);
        encode_agg(&mut w, &self.probe_aggregate)?;
        w.u64(self.last_metab_diss);
        w.u64(self.last_metab_e_new);
        Ok(w.into_inner())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, SimError> {
        let mut r = CanonReader::new(bytes);
        let mag = r
            .raw(4)
            .map_err(SimError::from)?;
        if mag != CHECKPOINT_MAGIC {
            return Err(SimError::ExperimentSetup("checkpoint magic".into()));
        }
        let ver = r.u32().map_err(SimError::from)?;
        if ver != CHECKPOINT_VERSION {
            return Err(SimError::ExperimentSetup(format!(
                "unsupported checkpoint version {ver}"
            )));
        }
        let definition_hash = r.bytes32().map_err(SimError::from)?;
        let yaml = std::str::from_utf8(r.blob().map_err(SimError::from)?)
            .map_err(|_| SimError::ExperimentSetup("spec yaml utf8".into()))?;
        let spec = parse_lab_yaml(yaml).map_err(SimError::from)?;
        let state_bytes = r.blob().map_err(SimError::from)?;
        let state = WorldState::decode_canonical(state_bytes).map_err(SimError::from)?;
        let freeze_learning = r.bool().map_err(SimError::from)?;
        let disable_reproduction = r.bool().map_err(SimError::from)?;
        let disable_active_motion = r.bool().map_err(SimError::from)?;
        let control_mode = ControlMode::decode_canonical(&mut r).map_err(SimError::from)?;
        let enable_probe = r.bool().map_err(SimError::from)?;
        let probe_frame_limit = r.u64().map_err(SimError::from)? as usize;
        let n_ev = r.u64().map_err(SimError::from)? as usize;
        let mut schedule = Vec::with_capacity(n_ev);
        for _ in 0..n_ev {
            schedule.push(LightEvent {
                tick: r.u64().map_err(SimError::from)?,
                lighting: decode_lighting(&mut r).map_err(SimError::from)?,
            });
        }
        let next_event = r.u64().map_err(SimError::from)? as usize;
        let t0_world = r.u64().map_err(SimError::from)?;
        let run_ledger = decode_ledger(&mut r).map_err(SimError::from)?;
        let last_probe = decode_probe(&mut r).map_err(SimError::from)?;
        let detector = RecoverDetector::decode(&mut r).map_err(SimError::from)?;
        let probe_aggregate = decode_agg(&mut r).map_err(SimError::from)?;
        let last_metab_diss = r.u64().map_err(SimError::from)?;
        let last_metab_e_new = r.u64().map_err(SimError::from)?;
        r.finish().map_err(SimError::from)?;
        if next_event > schedule.len() {
            return Err(SimError::ExperimentSetup("schedule cursor past end".into()));
        }
        Ok(Self {
            definition_hash,
            spec,
            state,
            freeze_learning,
            disable_reproduction,
            disable_active_motion,
            control_mode,
            enable_probe,
            probe_frame_limit,
            schedule,
            next_event,
            t0_world,
            run_ledger,
            last_probe,
            detector,
            probe_aggregate,
            last_metab_diss,
            last_metab_e_new,
        })
    }

    pub fn content_hash(&self) -> Result<Hash32, SimError> {
        Ok(sha256(&self.encode()?))
    }
}

#[derive(Clone, Debug)]
pub struct ExperimentSession {
    pub world: World,
    pub schedule: Vec<LightEvent>,
    pub next_event: usize,
    pub t0_world: u64,
    pub detector: RecoverDetector,
    pub definition_hash: Hash32,
    pub tick_log: Vec<TickSample>,
    pub record_ticks: bool,
}

impl ExperimentSession {
    pub fn new(
        mut world: World,
        schedule: Vec<LightEvent>,
        l: u64,
        w: u64,
        h: u64,
        definition_hash: Hash32,
        record_ticks: bool,
    ) -> Self {
        let t0_world = world.state.tick;
        let (p0, living0) = match world.state.organisms.iter().find(|o| !o.marked_dead) {
            Some(o) => (
                budget_p_i128(o.accounts.e_old, o.accounts.e_new, o.accounts.c),
                true,
            ),
            None => (0, false),
        };
        let detector = RecoverDetector::new(l, w, h, t0_world, p0, living0);
        world.last_events.clear();
        Self {
            world,
            schedule,
            next_event: 0,
            t0_world,
            detector,
            definition_hash,
            tick_log: Vec::new(),
            record_ticks,
        }
    }

    pub fn t_rel(&self) -> u64 {
        self.world.state.tick.saturating_sub(self.t0_world)
    }

    fn apply_due_lighting(&mut self) -> Result<(), SimError> {
        while self.next_event < self.schedule.len() {
            let ev = &self.schedule[self.next_event];
            if ev.tick != self.world.state.tick {
                break;
            }
            let lighting = ev.lighting.clone();
            self.world.apply_lighting(lighting)?;
            self.next_event += 1;
        }
        Ok(())
    }

    pub fn advance(&mut self) -> Result<(), SimError> {
        let snap = self.clone();
        if let Err(e) = self.apply_due_lighting() {
            *self = snap;
            return Err(e);
        }
        match self.world.step() {
            Ok(()) => {
                self.after_step();
                Ok(())
            }
            Err(e) => {
                *self = snap;
                Err(e)
            }
        }
    }

    fn after_step(&mut self) {
        let t_rel = self.t_rel();
        let started = self.world.state.tick.saturating_sub(1);
        let every = self.world.spec.time.decision_every_ticks.max(1);
        let decision_boundary = started % every == 0;
        let org = self.world.state.organisms.iter().find(|o| !o.marked_dead);
        let living = org.is_some();
        let p = org
            .map(|o| budget_p_i128(o.accounts.e_old, o.accounts.e_new, o.accounts.c))
            .unwrap_or(0);
        let debt = org.map(|o| o.maintenance_debt).unwrap_or(0);
        let probe = &self.world.last_probe;
        let sample = TickSample {
            t_rel,
            world_tick_after: self.world.state.tick,
            living,
            photo_extent: probe.photo_extent,
            p,
            debt,
            debt_accrued: probe.debt_accrued,
            maint_due: probe.maint_due,
            maint_paid: probe.maint_paid,
            maint_unpaid: probe.maint_unpaid,
            cognition_due: probe.cognition_due,
            cognition_paid: probe.cognition,
            decision_boundary,
        };
        self.detector.on_tick(sample.clone());
        if self.record_ticks {
            self.tick_log.push(sample);
        }
    }

    pub fn run_until(&mut self, t_rel_end: u64) -> Result<(), SimError> {
        while self.t_rel() < t_rel_end {
            if self.world.state.organisms.is_empty() {
                break;
            }
            self.advance()?;
        }
        Ok(())
    }

    pub fn checkpoint(&self) -> ExperimentCheckpoint {
        ExperimentCheckpoint::from_session(self)
    }

    pub fn public_state_hash(&self) -> Result<String, SimError> {
        Ok(hex_hash(&state_hash(&self.world.state)?))
    }

    pub fn rules_hash_hex(&self) -> Result<String, SimError> {
        Ok(hex_hash(&rules_hash(&self.world.spec)?))
    }
}

fn encode_lighting(w: &mut CanonWriter, l: &LightingRules) {
    w.u32(l.left_x0);
    w.u32(l.left_x1);
    w.u32(l.right_x0);
    w.u32(l.right_x1);
    w.u32(l.y0);
    w.u32(l.y1);
    w.u64(l.initial_q_lit);
    w.u64(l.radiation_input_per_cell);
}

fn decode_lighting(r: &mut CanonReader) -> Result<LightingRules, NumericFault> {
    Ok(LightingRules {
        left_x0: r.u32()?,
        left_x1: r.u32()?,
        right_x0: r.u32()?,
        right_x1: r.u32()?,
        y0: r.u32()?,
        y1: r.u32()?,
        initial_q_lit: r.u64()?,
        radiation_input_per_cell: r.u64()?,
    })
}

fn opt_u128(w: &mut CanonWriter, v: Option<u128>) {
    match v {
        None => w.u8(0),
        Some(x) => {
            w.u8(1);
            w.u128(x);
        }
    }
}

fn opt_u128_r(r: &mut CanonReader) -> Result<Option<u128>, NumericFault> {
    match r.u8()? {
        0 => Ok(None),
        1 => Ok(Some(r.u128()?)),
        _ => Err(NumericFault::Overflow("opt_u128")),
    }
}

fn encode_ledger(w: &mut CanonWriter, l: &RunLedger) {
    w.u64(l.maint_paid);
    w.u64(l.debt_repay);
    w.u64(l.cognition);
    w.u64(l.thrust_attempt);
    w.u64(l.turn_attempt);
    w.u64(l.repair);
    w.u64(l.repro_start);
    w.u64(l.repro_build);
    w.u64(l.photo_extent);
    w.u64(l.uptake_n);
    w.u64(l.uptake_o);
    w.u64(l.metab_e_new);
    w.u64(l.abs_dtheta_times_1e9);
    w.u64(l.photo_before_switch);
    w.u64(l.photo_after_switch);
    w.u64(l.uptake_o_before_switch);
    w.u64(l.uptake_o_after_switch);
    w.u64(l.metab_e_before_switch);
    w.u64(l.metab_e_after_switch);
    opt_u128(w, l.p_initial);
    opt_u128(w, l.p_at_switch);
    opt_u128(w, l.p_before_death_or_end);
    w.option_u64(l.e_at_switch);
    w.option_u64(l.c_at_switch);
    w.option_u64(l.n_at_switch);
    w.option_u64(l.debt_at_switch);
    w.option_u64(l.death_tick);
    match &l.death_cause {
        None => w.u8(0),
        Some(s) => {
            w.u8(1);
            w.blob(s.as_bytes());
        }
    }
    w.option_u64(l.first_old_light_entry);
    w.option_u64(l.first_new_light_entry);
    w.u64(l.old_light_residence_before);
    w.u64(l.new_light_residence_after);
    w.u64(l.decision_count);
    w.u64(l.learning_update_count);
    w.u64(l.executed_ticks);
}

fn decode_ledger(r: &mut CanonReader) -> Result<RunLedger, NumericFault> {
    Ok(RunLedger {
        maint_paid: r.u64()?,
        debt_repay: r.u64()?,
        cognition: r.u64()?,
        thrust_attempt: r.u64()?,
        turn_attempt: r.u64()?,
        repair: r.u64()?,
        repro_start: r.u64()?,
        repro_build: r.u64()?,
        photo_extent: r.u64()?,
        uptake_n: r.u64()?,
        uptake_o: r.u64()?,
        metab_e_new: r.u64()?,
        abs_dtheta_times_1e9: r.u64()?,
        photo_before_switch: r.u64()?,
        photo_after_switch: r.u64()?,
        uptake_o_before_switch: r.u64()?,
        uptake_o_after_switch: r.u64()?,
        metab_e_before_switch: r.u64()?,
        metab_e_after_switch: r.u64()?,
        p_initial: opt_u128_r(r)?,
        p_at_switch: opt_u128_r(r)?,
        p_before_death_or_end: opt_u128_r(r)?,
        e_at_switch: r.option_u64()?,
        c_at_switch: r.option_u64()?,
        n_at_switch: r.option_u64()?,
        debt_at_switch: r.option_u64()?,
        death_tick: r.option_u64()?,
        death_cause: match r.u8()? {
            0 => None,
            1 => Some(
                String::from_utf8(r.blob()?.to_vec())
                    .map_err(|_| NumericFault::Overflow("death_cause"))?,
            ),
            _ => return Err(NumericFault::Overflow("death_cause_tag")),
        },
        first_old_light_entry: r.option_u64()?,
        first_new_light_entry: r.option_u64()?,
        old_light_residence_before: r.u64()?,
        new_light_residence_after: r.u64()?,
        decision_count: r.u64()?,
        learning_update_count: r.u64()?,
        executed_ticks: r.u64()?,
    })
}

fn encode_probe(w: &mut CanonWriter, p: &TickProbe) {
    w.u64(p.maint_paid);
    w.u64(p.debt_repay);
    w.u64(p.cognition);
    w.u64(p.thrust_attempt);
    w.u64(p.turn_attempt);
    w.u64(p.repair);
    w.u64(p.repro_start);
    w.u64(p.repro_build);
    w.u64(p.photo_extent);
    w.u64(p.uptake_n);
    w.u64(p.uptake_o);
    w.u64(p.metab_e_new);
    w.u64(p.abs_dtheta);
    w.u64(p.maint_due);
    w.u64(p.maint_unpaid);
    w.u64(p.cognition_due);
    w.u64(p.debt_accrued);
    w.u64(p.e_old);
    w.u64(p.e_new);
    w.u64(p.c);
    w.u64(p.n);
    w.u64(p.b);
    w.u64(p.debt);
    w.u128(p.p);
    w.u64(p.x_bits);
    w.u64(p.y_bits);
    w.u32(p.living);
}

fn decode_probe(r: &mut CanonReader) -> Result<TickProbe, NumericFault> {
    Ok(TickProbe {
        maint_paid: r.u64()?,
        debt_repay: r.u64()?,
        cognition: r.u64()?,
        thrust_attempt: r.u64()?,
        turn_attempt: r.u64()?,
        repair: r.u64()?,
        repro_start: r.u64()?,
        repro_build: r.u64()?,
        photo_extent: r.u64()?,
        uptake_n: r.u64()?,
        uptake_o: r.u64()?,
        metab_e_new: r.u64()?,
        abs_dtheta: r.u64()?,
        maint_due: r.u64()?,
        maint_unpaid: r.u64()?,
        cognition_due: r.u64()?,
        debt_accrued: r.u64()?,
        e_old: r.u64()?,
        e_new: r.u64()?,
        c: r.u64()?,
        n: r.u64()?,
        b: r.u64()?,
        debt: r.u64()?,
        p: r.u128()?,
        x_bits: r.u64()?,
        y_bits: r.u64()?,
        living: r.u32()?,
    })
}

fn encode_agg(w: &mut CanonWriter, a: &DecisionAggregate) -> Result<(), NumericFault> {
    w.u64(a.n);
    w.u64(a.n_updates);
    w.u64(a.n_clipped);
    w.f64_canonical(a.mean_delta)?;
    w.f64_canonical(a.mean_delta_used)?;
    w.f64_canonical(a.mean_entropy)?;
    w.f64_canonical(a.mean_interval_d)?;
    w.f64_canonical(a.mean_r)?;
    w.f64_canonical(a.mean_input_change)?;
    w.f64_canonical(a.mean_h_l2)?;
    w.f64_canonical(a.mean_a_minus_a0)?;
    w.f64_canonical(a.mean_u_effective)?;
    w.f64_canonical(a.mean_w_effective)?;
    w.u64(a.n_obs31_valid);
    w.f64_canonical(a.mean_obs31)?;
    Ok(())
}

fn decode_agg(r: &mut CanonReader) -> Result<DecisionAggregate, NumericFault> {
    Ok(DecisionAggregate {
        n: r.u64()?,
        n_updates: r.u64()?,
        n_clipped: r.u64()?,
        mean_delta: r.f64_canonical()?,
        mean_delta_used: r.f64_canonical()?,
        mean_entropy: r.f64_canonical()?,
        mean_interval_d: r.f64_canonical()?,
        mean_r: r.f64_canonical()?,
        mean_input_change: r.f64_canonical()?,
        mean_h_l2: r.f64_canonical()?,
        mean_a_minus_a0: r.f64_canonical()?,
        mean_u_effective: r.f64_canonical()?,
        mean_w_effective: r.f64_canonical()?,
        n_obs31_valid: r.u64()?,
        mean_obs31: r.f64_canonical()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use universe_core::init::initialize_reversal_world;
    use universe_core::probe::ControlMode;
    use universe_core::world::World;
    use universe_types::{rules_hash, state_hash};

    use crate::experiment::light_right_schedule;
    use crate::reversal_spec;

    fn session_at_left(seed: u64, control: ControlMode, l: u64, w: u64, h: u64) -> ExperimentSession {
        let spec = reversal_spec();
        let st = initialize_reversal_world(&spec, seed).unwrap();
        let mut world = World::from_state_checked(spec, st).unwrap();
        world.disable_reproduction = true;
        world.control_mode = control;
        world.freeze_learning = true;
        ExperimentSession::new(world, Vec::new(), l, w, h, [0; 32], true)
    }

    fn hashes_match(a: &ExperimentSession, b: &ExperimentSession) {
        assert_eq!(
            state_hash(&a.world.state).unwrap(),
            state_hash(&b.world.state).unwrap()
        );
        assert_eq!(
            rules_hash(&a.world.spec).unwrap(),
            rules_hash(&b.world.spec).unwrap()
        );
        assert_eq!(a.world.run_ledger.decision_count, b.world.run_ledger.decision_count);
        assert_eq!(a.world.run_ledger.learning_update_count, b.world.run_ledger.learning_update_count);
        assert_eq!(a.detector.first_recover_t(), b.detector.first_recover_t());
        assert_eq!(a.next_event, b.next_event);
    }

    #[test]
    fn yaml_lab_spec_roundtrip() {
        let spec = reversal_spec();
        let y = serde_yaml::to_string(&spec).unwrap();
        let back = parse_lab_yaml(&y).unwrap();
        assert_eq!(spec, back);
    }

    #[test]
    fn lighting_switch_before_and_after_restore() {
        let spec = reversal_spec();
        let st = initialize_reversal_world(&spec, 9).unwrap();
        let right = light_right_schedule(&spec);
        let mut live = World::from_state_checked(spec.clone(), st.clone()).unwrap();
        live.disable_reproduction = true;
        live.freeze_learning = true;
        live.control_mode = ControlMode::Stationary;
        let mut live_s = ExperimentSession::new(
            live,
            vec![LightEvent {
                tick: 5,
                lighting: right.clone(),
            }],
            0,
            4,
            40,
            [1; 32],
            true,
        );
        let mut pre = live_s.clone();
        pre.run_until(4).unwrap();
        assert_eq!(pre.world.state.tick, 4);
        let bytes = pre.checkpoint().encode().unwrap();
        let mut restored = ExperimentCheckpoint::decode(&bytes)
            .unwrap()
            .into_session()
            .unwrap();
        restored.record_ticks = true;
        live_s.run_until(8).unwrap();
        restored.run_until(8).unwrap();
        hashes_match(&live_s, &restored);
        assert_eq!(live_s.next_event, 1);
        assert_eq!(restored.next_event, 1);
        assert_eq!(live_s.world.spec.lighting, right);

        let mut after = World::from_state_checked(spec, st).unwrap();
        after.disable_reproduction = true;
        after.freeze_learning = true;
        after.control_mode = ControlMode::Stationary;
        let mut after_s = ExperimentSession::new(
            after,
            vec![LightEvent {
                tick: 5,
                lighting: right,
            }],
            0,
            4,
            40,
            [1; 32],
            false,
        );
        after_s.run_until(6).unwrap();
        let bytes = after_s.checkpoint().encode().unwrap();
        let mut r2 = ExperimentCheckpoint::decode(&bytes)
            .unwrap()
            .into_session()
            .unwrap();
        after_s.run_until(12).unwrap();
        r2.run_until(12).unwrap();
        hashes_match(&after_s, &r2);
    }

    #[test]
    fn decision_and_non_decision_restore() {
        let mut live = session_at_left(11, ControlMode::Neural, 200, 100, 4000);
        live.world.freeze_learning = false;
        live.run_until(5).unwrap();
        assert_eq!(live.world.state.tick % 5, 0);
        let b_dec = live.checkpoint().encode().unwrap();
        live.run_until(6).unwrap();
        let b_nd = live.checkpoint().encode().unwrap();
        let mut cont = live.clone();
        cont.run_until(20).unwrap();
        let mut r_dec = ExperimentCheckpoint::decode(&b_dec)
            .unwrap()
            .into_session()
            .unwrap();
        r_dec.world.freeze_learning = false;
        r_dec.run_until(20).unwrap();
        hashes_match(&cont, &r_dec);
        let mut r_nd = ExperimentCheckpoint::decode(&b_nd)
            .unwrap()
            .into_session()
            .unwrap();
        r_nd.world.freeze_learning = false;
        r_nd.run_until(20).unwrap();
        hashes_match(&cont, &r_nd);
    }

    #[test]
    fn skipped_cognition_restore_pays_once_later() {
        let mut s = session_at_left(7, ControlMode::Stationary, 200, 100, 4000);
        s.world.state.organisms[0].accounts.e_old = 2_500;
        s.run_until(1).unwrap();
        assert!(s.world.run_ledger.decision_count == 0 || s.world.last_skips.iter().any(|k| k.tick == 0));
        let bytes = s.checkpoint().encode().unwrap();
        let mut r = ExperimentCheckpoint::decode(&bytes)
            .unwrap()
            .into_session()
            .unwrap();
        s.run_until(8).unwrap();
        r.run_until(8).unwrap();
        hashes_match(&s, &r);
        assert_eq!(s.world.run_ledger.decision_count, r.world.run_ledger.decision_count);
    }

    #[test]
    fn t_recover_partial_and_after_first_restore() {
        let spec = reversal_spec();
        let mut st = initialize_reversal_world(&spec, 5).unwrap();
        st.organisms[0].geom.x = 11.5;
        st.organisms[0].geom.y = 8.5;
        let right = light_right_schedule(&spec);
        let mut world = World::from_state_checked(spec, st).unwrap();
        world.disable_reproduction = true;
        world.freeze_learning = true;
        world.control_mode = ControlMode::Stationary;
        world.apply_lighting(right).unwrap();
        let mut live = ExperimentSession::new(world, Vec::new(), 0, 6, 40, [2; 32], true);
        live.run_until(3).unwrap();
        let mid = live.checkpoint().encode().unwrap();
        live.run_until(20).unwrap();
        let first = live.detector.first_recover_t();
        let mut r = ExperimentCheckpoint::decode(&mid)
            .unwrap()
            .into_session()
            .unwrap();
        r.run_until(20).unwrap();
        assert_eq!(first, r.detector.first_recover_t());
        hashes_match(&live, &r);
        if let Some(t) = first {
            assert!(t >= 6);
            let bytes = live.checkpoint().encode().unwrap();
            let mut after = ExperimentCheckpoint::decode(&bytes)
                .unwrap()
                .into_session()
                .unwrap();
            let mut cont = live.clone();
            cont.run_until(30).unwrap();
            after.run_until(30).unwrap();
            assert_eq!(cont.detector.first_recover_t(), Some(t));
            assert_eq!(after.detector.first_recover_t(), Some(t));
            hashes_match(&cont, &after);
        }
    }

    #[test]
    fn checkpoint_replay_twice_and_reject_corrupt() {
        let mut s = session_at_left(3, ControlMode::Forced { thrust: 1, turn: 0 }, 200, 100, 4000);
        s.run_until(4).unwrap();
        let bytes = s.checkpoint().encode().unwrap();
        let mut a = ExperimentCheckpoint::decode(&bytes)
            .unwrap()
            .into_session()
            .unwrap();
        let mut b = ExperimentCheckpoint::decode(&bytes)
            .unwrap()
            .into_session()
            .unwrap();
        a.run_until(15).unwrap();
        b.run_until(15).unwrap();
        hashes_match(&a, &b);
        let mut bad = bytes.clone();
        bad.truncate(bytes.len() / 2);
        assert!(ExperimentCheckpoint::decode(&bad).is_err());
        if let Some(x) = bad.get_mut(0) {
            *x ^= 0xff;
        }
        assert!(ExperimentCheckpoint::decode(&bad).is_err());
        let mut mismatch = s.checkpoint();
        mismatch.state.rules_hash = [9; 32];
        assert!(mismatch.into_session().is_err());
    }
}

