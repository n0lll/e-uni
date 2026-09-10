//! D1 engineering tests: atomic commit, probes, interval feedback, P3 scale.

use super::*;
use crate::probe::{budget_p, ControlMode, FaultPoint};
use universe_types::{state_hash, NumericFault, Organism, RuleSpec, SimError};

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

fn snap(w: &World) -> (Vec<u8>, [u8; 32]) {
    (
        w.state.encode_canonical().unwrap(),
        state_hash(&w.state).unwrap(),
    )
}

fn primed_world() -> World {
    let spec = reversal_spec();
    let st = initialize_reversal_world(&spec, 7).unwrap();
    let mut w = World::from_state(spec, st);
    w.disable_reproduction = true;
    w.freeze_learning = true;
    w.control_mode = ControlMode::Stationary;
    w
}

#[test]
fn p3_old_energy_cannot_use_e_new() {
    let spec = RuleSpec::eu_core_1();
    let st = initialize_reversal_world(&reversal_spec(), 1).unwrap();
    let mut org: Organism = st.organisms[0].clone();
    org.accounts.e_old = 600;
    org.accounts.e_new = 50_000;
    org.control.thrust = 1;
    org.control.turn = 1;
    let pay = scale_control_parts(&mut org, &spec).unwrap();
    assert_eq!(pay.total(), 600);
    assert_eq!(pay.thrust, 500);
    assert_eq!(pay.turn, 100);
    assert!((org.control.u_effective - 0.5).abs() < 1e-12);
    assert!((org.control.w_effective - 0.5).abs() < 1e-12);
    assert_eq!(org.accounts.e_old, 0);
    assert_eq!(org.accounts.e_new, 50_000);
}

#[test]
fn inject_each_fault_point_preserves_committed_bytes() {
    for point in FaultPoint::all() {
        let mut injected = primed_world();
        let mut reference = primed_world();
        let (bytes0, hash0) = snap(&injected);
        injected.inject_fault = Some(point);
        let err = injected.step().unwrap_err();
        match err {
            SimError::Numeric(NumericFault::Overflow(tag)) => assert_eq!(tag, point.tag()),
            other => panic!("point {point:?} got {other:?}"),
        }
        let (bytes1, hash1) = snap(&injected);
        assert_eq!(bytes0, bytes1, "canonical bytes changed at {point:?}");
        assert_eq!(hash0, hash1, "state_hash changed at {point:?}");
        assert!(injected.last_events.is_empty());
        assert_eq!(injected.state.tick, 0);

        injected.inject_fault = None;
        injected.step().unwrap();
        reference.step().unwrap();
        assert_eq!(
            state_hash(&injected.state).unwrap(),
            state_hash(&reference.state).unwrap(),
            "post-inject step diverged at {point:?}"
        );
        assert_eq!(injected.last_events, reference.last_events);
    }
}

#[test]
fn coincident_bodies_unresolved_keeps_boundary() {
    let mut spec = reversal_spec();
    spec.physics.max_projection_iters = 0;
    spec.safety.max_living = 8;
    let mut st = initialize_reversal_world(&spec, 3).unwrap();
    let mut twin = st.organisms[0].clone();
    twin.entity_id = 2;
    twin.geom = st.organisms[0].geom;
    st.organisms.push(twin);
    st.next_entity_id = 3;
    let mut w = World::from_state(spec, st);
    w.disable_reproduction = true;
    w.freeze_learning = true;
    let (b0, h0) = snap(&w);
    let err = w.step().unwrap_err();
    match err {
        SimError::Numeric(NumericFault::ContactSolverUnresolved) => {}
        other => panic!("{other:?}"),
    }
    let (b1, h1) = snap(&w);
    assert_eq!(b0, b1);
    assert_eq!(h0, h1);
    assert!(w.last_events.is_empty());
}

#[test]
fn overcrowded_pairs_pause_keeps_boundary() {
    let mut spec = reversal_spec();
    spec.safety.max_contact_pairs = 1;
    spec.safety.max_living = 8;
    let mut st = initialize_reversal_world(&spec, 4).unwrap();
    for k in 0..3u64 {
        let mut extra = st.organisms[0].clone();
        extra.entity_id = 2 + k;
        extra.geom.x = 0.4 + 0.01 * k as f64;
        extra.geom.y = 0.4 + 0.01 * k as f64;
        st.organisms.push(extra);
    }
    st.next_entity_id = 6;
    let mut w = World::from_state(spec, st);
    w.disable_reproduction = true;
    let (b0, h0) = snap(&w);
    match w.step().unwrap_err() {
        SimError::SafetyPause(_) => {}
        other => panic!("{other:?}"),
    }
    let (b1, h1) = snap(&w);
    assert_eq!(b0, b1);
    assert_eq!(h0, h1);
}

#[test]
fn probe_does_not_change_state_hash() {
    let mut a = primed_world();
    let mut b = primed_world();
    a.enable_probe = false;
    b.enable_probe = true;
    for _ in 0..12 {
        a.step().unwrap();
        b.step().unwrap();
    }
    assert_eq!(
        state_hash(&a.state).unwrap(),
        state_hash(&b.state).unwrap()
    );
    assert!(b.run_ledger.executed_ticks > 0);
    assert!(b.last_decisions.len() >= 2);
}

#[test]
fn interval_feedback_records_photo_in_light() {
    let spec = reversal_spec();
    let mut st = initialize_reversal_world(&spec, 11).unwrap();
    st.organisms[0].geom.x = 4.5;
    st.organisms[0].geom.y = 8.5;
    let mut w = World::from_state(spec, st);
    w.disable_reproduction = true;
    w.freeze_learning = true;
    w.control_mode = ControlMode::Stationary;
    for _ in 0..6 {
        w.step().unwrap();
    }
    let org = &w.state.organisms[0];
    assert!(org.last_feedback.ticks > 0);
    assert!(
        org.last_feedback.photo_extent > 0,
        "photo_extent stayed 0 under the lamp"
    );
    assert_ne!(org.last_feedback.e_net_i128, 0);
    let p = budget_p(org.accounts.e_old, org.accounts.e_new, org.accounts.c);
    assert!(p > 0);
}

#[test]
fn p_equals_e_plus_eight_c() {
    let org_e = 5_000_000u64;
    let org_c = 500_000u64;
    assert_eq!(budget_p(org_e, 0, org_c), 5_000_000 + 8 * 500_000);
}
