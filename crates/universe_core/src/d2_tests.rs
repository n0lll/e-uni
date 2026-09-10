//! D2 world-level regressions. These fail on 4f237b0 (late feedback, double
//! motion normalization, no readout-update fault path).

use super::*;
use crate::physics::tick_dt;
use crate::probe::{ControlMode, FaultPoint};
use universe_types::{
    rules_hash, state_hash, FutureInput, NumericFault, RuleSpec, SimError,
};

fn reversal_spec() -> RuleSpec {
    let mut spec = RuleSpec::eu_core_1();
    spec.world.grid_width = 16;
    spec.world.grid_height = 16;
    spec.world.width_lu = 16.0;
    spec.world.height_lu = 16.0;
    spec.lighting.left_x0 = 3;
    spec.lighting.left_x1 = 5;
    spec.lighting.right_x0 = 3;
    spec.lighting.right_x1 = 5;
    spec.lighting.y0 = 7;
    spec.lighting.y1 = 9;
    spec.init.initial_count = 1;
    spec.validate_lab().expect("lab spec");
    spec
}

fn lamp_world(seed: u64, mode: ControlMode) -> World {
    let spec = reversal_spec();
    let mut st = initialize_reversal_world(&spec, seed).unwrap();
    st.organisms[0].geom.x = 4.5;
    st.organisms[0].geom.y = 8.5;
    st.organisms[0].geom.theta = 0.0;
    let mut w = World::from_state(spec, st);
    w.disable_reproduction = true;
    w.freeze_learning = true;
    w.control_mode = mode;
    w.enable_probe = true;
    w
}

fn diag_at(w: &World, tick: u64) -> &crate::probe::DecisionDiag {
    w.last_decisions
        .iter()
        .find(|d| d.tick == tick)
        .unwrap_or_else(|| panic!("missing decision at tick {tick}"))
}

/// FB01: after steps 0–4, the tick=5 neural input must contain [0,5) photo.
#[test]
fn fb01_first_complete_feedback_on_decision() {
    let mut w = lamp_world(11, ControlMode::Stationary);
    for _ in 0..6 {
        w.step().unwrap();
    }
    assert_eq!(w.state.tick, 6);
    let d0 = diag_at(&w, 0);
    assert_eq!(d0.feedback_ticks, 0, "first decision has no prior interval");
    assert!(d0.obs[32 + 31] < 0.5, "first-decision photo valid bit must be 0");

    let d5 = diag_at(&w, 5);
    assert_eq!(d5.feedback_start_tick, 0);
    assert_eq!(d5.feedback_end_tick, 5);
    assert_eq!(d5.feedback_ticks, 5);
    assert_eq!(d5.observation_state_tick, 5);
    assert!(
        d5.obs[32 + 31] > 0.5,
        "tick=5 must read sealed [0,5) photo valid=1, got valid={}",
        d5.obs[32 + 31]
    );
    assert!(
        d5.obs[31] > 0.0,
        "tick=5 photo channel must be >0 under the lamp, got {}",
        d5.obs[31]
    );
    assert!(
        d5.actual_forward_displacement.abs() < 1e-9,
        "stationary interval must not report paid-control as motion"
    );
}

/// FB02: tick=10 reads [5,10), not the previous [0,5) photo.
#[test]
fn fb02_interval_switches() {
    let mut w = lamp_world(11, ControlMode::Stationary);
    for _ in 0..6 {
        w.step().unwrap();
    }
    let photo_05 = diag_at(&w, 5).obs[31];
    assert!(photo_05 > 0.0);
    w.state.organisms[0].geom.x = 0.4;
    w.state.organisms[0].geom.y = 0.4;
    for _ in 0..5 {
        w.step().unwrap();
    }
    let d10 = diag_at(&w, 10);
    assert_eq!(d10.feedback_start_tick, 5);
    assert_eq!(d10.feedback_end_tick, 10);
    assert_eq!(d10.feedback_ticks, 5);
    assert!(
        d10.obs[31] + 1e-9 < photo_05,
        "tick=10 must not reuse [0,5) photo {} vs {}",
        d10.obs[31],
        photo_05
    );
}

/// FB03: a skipped decision stretches interval_d; recovery updates once.
#[test]
fn fb03_skipped_decision_consumes_once() {
    let mut w = lamp_world(11, ControlMode::Neural);
    w.freeze_learning = false;
    for _ in 0..5 {
        w.step().unwrap();
    }
    assert_eq!(w.state.tick, 5);
    let maint = maintenance_cost(&w.state.organisms[0], &w.spec, false);
    w.state.organisms[0].accounts.e_old = maint + 100;
    w.state.organisms[0].accounts.e_new = 0;
    w.step().unwrap();
    assert_eq!(w.state.tick, 6);
    assert!(
        w.last_skips.iter().any(|s| s.tick == 5),
        "tick 5 must be recorded as a skipped decision, skips={:?}",
        w.last_skips
            .iter()
            .map(|s| (s.tick, s.reason.clone()))
            .collect::<Vec<_>>()
    );
    assert!(
        w.last_decisions.iter().all(|d| d.tick != 5),
        "skipped tick must not consume the learning interval"
    );
    w.state.organisms[0].accounts.e_old = 8_000_000;
    for _ in 0..5 {
        w.step().unwrap();
    }
    let d10 = diag_at(&w, 10);
    assert_eq!(d10.interval_d, 10, "open interval must span the skipped boundary");
    assert_eq!(d10.feedback_ticks, 10);
    let updates: Vec<u64> = w
        .last_decisions
        .iter()
        .filter(|d| d.applied_update)
        .map(|d| d.tick)
        .collect();
    assert_eq!(updates, vec![10], "readout must fire once after the skip, got {updates:?}");
}

/// FB04: Neural and Reflex receive the same observation bytes and interval.
#[test]
fn fb04_same_frame_neural_and_reflex() {
    let mut base = lamp_world(11, ControlMode::Stationary);
    for _ in 0..5 {
        base.step().unwrap();
    }
    let bytes = base.state.encode_canonical().unwrap();
    let restored = universe_types::WorldState::decode_canonical(&bytes).unwrap();
    let mut neural = World::from_state(base.spec.clone(), restored.clone());
    neural.disable_reproduction = true;
    neural.freeze_learning = true;
    neural.control_mode = ControlMode::Neural;
    neural.enable_probe = true;
    let mut reflex = World::from_state(base.spec.clone(), restored);
    reflex.disable_reproduction = true;
    reflex.freeze_learning = true;
    reflex.control_mode = ControlMode::ReflexQ;
    reflex.enable_probe = true;
    neural.step().unwrap();
    reflex.step().unwrap();
    let fn_ = neural.last_observation.get(&1).expect("neural frame");
    let fr = reflex.last_observation.get(&1).expect("reflex frame");
    assert_eq!(fn_.feedback_start_tick, fr.feedback_start_tick);
    assert_eq!(fn_.feedback_end_tick, fr.feedback_end_tick);
    assert_eq!(fn_.feedback_ticks, fr.feedback_ticks);
    assert_eq!(fn_.obs, fr.obs);
}

/// MV01: full-speed free motion normalizes to ~1 after a sealed interval.
#[test]
fn mv01_open_field_full_thrust() {
    let spec = reversal_spec();
    let mut st = initialize_reversal_world(&spec, 9).unwrap();
    st.organisms[0].geom.x = 8.0;
    st.organisms[0].geom.y = 8.0;
    st.organisms[0].geom.theta = 0.0;
    st.organisms[0].accounts.e_old = 8_000_000;
    let mut w = World::from_state(spec, st);
    w.disable_reproduction = true;
    w.freeze_learning = true;
    w.control_mode = ControlMode::Forced {
        thrust: 1,
        turn: 0,
    };
    w.enable_probe = true;
    for _ in 0..6 {
        w.step().unwrap();
    }
    let d5 = diag_at(&w, 5);
    assert!((d5.u_effective - 1.0).abs() < 1e-12);
    let dur = tick_dt(&w.spec) * d5.feedback_ticks as f64;
    let speed = d5.actual_forward_displacement / dur;
    let norm = speed / w.spec.physics.max_linear_speed;
    assert!(
        (norm - 1.0).abs() < 0.05,
        "expected ~1 forward sense, got {norm} disp={} dur={dur}",
        d5.actual_forward_displacement
    );
    assert!((d5.obs[28] - 1.0).abs() < 0.05, "obs[28]={}", d5.obs[28]);
}

/// MV02: wall contact: paid thrust stays 1, actual forward ~ 0.
#[test]
fn mv02_wall_paid_but_no_progress() {
    let spec = reversal_spec();
    let mut st = initialize_reversal_world(&spec, 9).unwrap();
    st.organisms[0].geom.x = 0.3;
    st.organisms[0].geom.y = 8.0;
    st.organisms[0].geom.theta = std::f64::consts::PI;
    st.organisms[0].accounts.e_old = 8_000_000;
    let mut w = World::from_state(spec, st);
    w.disable_reproduction = true;
    w.freeze_learning = true;
    w.control_mode = ControlMode::Forced {
        thrust: 1,
        turn: 0,
    };
    w.enable_probe = true;
    for _ in 0..6 {
        w.step().unwrap();
    }
    let d5 = diag_at(&w, 5);
    assert!((d5.u_effective - 1.0).abs() < 1e-12);
    assert!(d5.paid_motion_energy > 0);
    let dur = tick_dt(&w.spec) * d5.feedback_ticks as f64;
    let norm = (d5.actual_forward_displacement / dur) / w.spec.physics.max_linear_speed;
    assert!(
        norm.abs() < 0.05,
        "wall should zero actual forward, got {norm} disp={}",
        d5.actual_forward_displacement
    );
    assert!(d5.obs[28].abs() < 0.05, "obs[28]={}", d5.obs[28]);
}

/// MV03: full turn across ±π stays ~±1 with no 2π jump in last_dtheta.
#[test]
fn mv03_full_turn_unwraps() {
    let spec = reversal_spec();
    let mut st = initialize_reversal_world(&spec, 9).unwrap();
    st.organisms[0].geom.x = 8.0;
    st.organisms[0].geom.y = 8.0;
    st.organisms[0].geom.theta = std::f64::consts::PI - 0.05;
    st.organisms[0].accounts.e_old = 8_000_000;
    let mut w = World::from_state(spec, st);
    w.disable_reproduction = true;
    w.freeze_learning = true;
    w.control_mode = ControlMode::Forced {
        thrust: 0,
        turn: 1,
    };
    w.enable_probe = true;
    let mut sum = 0.0;
    for _ in 0..12 {
        w.step().unwrap();
        let dth = w.state.organisms[0].last_dtheta;
        assert!(
            dth > 0.0 && dth < 1.0,
            "unwrapped dtheta jumped: {dth} theta={}",
            w.state.organisms[0].geom.theta
        );
        sum += dth;
    }
    let d5 = diag_at(&w, 5);
    assert!((d5.w_effective - 1.0).abs() < 1e-12);
    assert!((d5.obs[29] - 1.0).abs() < 0.05, "obs[29]={}", d5.obs[29]);
    let expect = 12.0 * tick_dt(&w.spec) * w.spec.physics.max_angular_speed;
    assert!((sum - expect).abs() < 1e-6, "sum={sum} expect={expect}");
}

/// MV04: P3 partial pay is logged as 0.5, e_new unused.
#[test]
fn mv04_partial_pay_logged() {
    let spec = reversal_spec();
    let mut st = initialize_reversal_world(&spec, 1).unwrap();
    st.organisms[0].geom.x = 8.0;
    st.organisms[0].geom.y = 8.0;
    let mut w = World::from_state(spec, st);
    w.disable_reproduction = true;
    w.freeze_learning = true;
    w.control_mode = ControlMode::Forced {
        thrust: 1,
        turn: 1,
    };
    w.enable_probe = true;
    for _ in 0..5 {
        w.step().unwrap();
    }
    let maint = maintenance_cost(&w.state.organisms[0], &w.spec, false);
    let cog = w.spec.cognition_cost_b();
    w.state.organisms[0].accounts.e_old = maint + cog + 600;
    w.state.organisms[0].accounts.e_new = 50_000;
    w.step().unwrap();
    let d5 = diag_at(&w, 5);
    assert!((d5.u_effective - 0.5).abs() < 1e-12, "u_eff={}", d5.u_effective);
    assert!((d5.w_effective - 0.5).abs() < 1e-12, "w_eff={}", d5.w_effective);
    assert_eq!(d5.paid_motion_energy, 600);
}

/// AT01: pending readout actually changes A/v; AfterP2 inject restores bytes.
#[test]
fn at01_readout_update_then_injected_fault_restores() {
    let spec = reversal_spec();
    let st = initialize_reversal_world(&spec, 21).unwrap();
    let a0 = st.organisms[0].brain.a;
    let v0 = st.organisms[0].brain.v;
    let mut live = World::from_state(spec.clone(), st.clone());
    live.disable_reproduction = true;
    live.freeze_learning = false;
    live.control_mode = ControlMode::Neural;
    live.enable_probe = true;
    for _ in 0..5 {
        live.step().unwrap();
    }
    assert!(live.state.organisms[0].brain.has_pending);
    let (b0, h0) = (
        live.state.encode_canonical().unwrap(),
        state_hash(&live.state).unwrap(),
    );
    let mut injected = live.clone();
    injected.inject_fault = Some(FaultPoint::AfterP2);
    match injected.step().unwrap_err() {
        SimError::Numeric(NumericFault::Overflow(tag)) => assert_eq!(tag, "inject_p2"),
        other => panic!("{other:?}"),
    }
    assert_eq!(injected.state.encode_canonical().unwrap(), b0);
    assert_eq!(state_hash(&injected.state).unwrap(), h0);
    assert!(injected.last_events.is_empty());

    let mut reference = live.clone();
    injected.inject_fault = None;
    injected.step().unwrap();
    reference.step().unwrap();
    assert_eq!(
        state_hash(&injected.state).unwrap(),
        state_hash(&reference.state).unwrap()
    );
    assert!(
        injected.last_decisions.iter().any(|d| d.tick == 5 && d.applied_update),
        "tick=5 must apply a readout update"
    );
    let changed_a = injected.state.organisms[0].brain.a != a0;
    let changed_v = injected.state.organisms[0].brain.v != v0;
    assert!(changed_a || changed_v, "A/v must move on the uninjected path");
}

#[test]
fn ex03_nonempty_future_inputs_rejected() {
    let spec = reversal_spec();
    let mut st = initialize_reversal_world(&spec, 3).unwrap();
    st.future_inputs.push(FutureInput {
        effective_tick: 0,
        seq: 0,
        kind: 1,
        payload: vec![1],
    });
    let mut w = World::from_state(spec, st);
    match w.step().unwrap_err() {
        SimError::ExperimentSetup(msg) => assert!(msg.contains("future_inputs")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn low_stock_retargets_m0_u0() {
    let spec = reversal_spec();
    let mut st = initialize_reversal_world(&spec, 3).unwrap();
    let u0 = st.audit.initial_u;
    st.organisms[0].accounts.c = 2_000;
    retarget_audit_baseline(&mut st, &spec).unwrap();
    let mu = compute_mu(&st, spec.reactions.chem_e_per_organic).unwrap();
    assert_eq!(st.audit.initial_u, mu.u);
    assert_eq!(st.audit.initial_m, mu.m);
    assert_ne!(st.audit.initial_u, u0);
}

#[test]
fn lighting_change_syncs_rules_hash() {
    let spec = reversal_spec();
    let h0 = rules_hash(&spec).unwrap();
    let st = initialize_reversal_world(&spec, 3).unwrap();
    let mut w = World::from_state(spec, st);
    assert_eq!(w.state.rules_hash, h0);
    let mut lit = w.spec.lighting.clone();
    lit.left_x0 = 10;
    lit.left_x1 = 12;
    lit.right_x0 = 10;
    lit.right_x1 = 12;
    w.apply_lighting(lit).unwrap();
    let h1 = rules_hash(&w.spec).unwrap();
    assert_ne!(h0, h1);
    assert_eq!(w.state.rules_hash, h1);
}

#[test]
fn p0_energy_not_post_maintenance() {
    let mut w = lamp_world(11, ControlMode::Stationary);
    let e0 = w.state.organisms[0].accounts.e_old;
    let emax = w.state.organisms[0].e_max;
    w.step().unwrap();
    let frame = w.last_observation.get(&1).expect("frame");
    assert_eq!(frame.p0_e_old, e0);
    let want = (e0 as f64 / emax as f64).clamp(0.0, 1.0);
    assert!((frame.obs[20] - want).abs() < 1e-12);
    assert!(w.last_probe.maint_paid + w.last_probe.cognition > 0);
}
