//! Deterministic EU-Core-1 simulation kernel.

pub mod audit;
pub mod brain;
pub mod fields;
pub mod genome;
pub mod init;
pub mod ledger;
pub mod physics;
pub mod physiology;
pub mod probe;
pub mod quota;
pub mod react;
pub mod repro;
pub mod rng;
pub mod sense;
pub mod world;

pub use audit::{compute_mu, energy_identity_holds, MuTotals};
pub use fields::{decay_s_q, phase_p5a, transport_all_fields, transport_deltas};
pub use init::{initialize_reversal_world, initialize_world, lighting_cells, lighting_centroid};
pub use physiology::{
    attempt_motion_cost, is_irrecoverable, maintenance_cost, pay_maintenance, pay_maintenance_parts,
    scale_control, scale_control_parts, MaintPay, MotionPay,
};
pub use probe::{
    budget_p, ControlMode, DecisionDiag, FaultPoint, RunLedger, TickProbe,
};
pub use ledger::{
    catabolism, commit_e_new_into_old, n_uptake, o_uptake, photosynth, repair, spend_old_energy,
    transfer_c, BodyView, CellView,
};
pub use quota::{allocate_pool, common_extents, MultiInputRequest, PoolDemand, RequestId};
pub use rng::{
    encode_key_counter, geometry_draw_index, philox4x32_10, quota_draw_index, random_range,
    random_u32, random_u32_domain, random_unit, ObserverRng,
};
pub use world::World;


#[cfg(test)]
mod g1_tests {
    use super::*;
    use universe_types::{Fields, RuleSpec, T01_INITIAL_M_MICRO, T01_INITIAL_U_MICRO};

    fn spec() -> RuleSpec {
        RuleSpec::eu_core_1()
    }

    #[test]
    fn t01_initialization_totals() {
        let spec = spec();
        let st = initialize_world(&spec, 0).unwrap();
        let mu = compute_mu(&st, spec.reactions.chem_e_per_organic).unwrap();
        assert_eq!(mu.m, T01_INITIAL_M_MICRO);
        assert_eq!(mu.u, T01_INITIAL_U_MICRO);
        assert_eq!(st.audit.initial_m, T01_INITIAL_M_MICRO);
        assert_eq!(st.organisms.len(), 128);
        assert_eq!(st.organisms[0].entity_id, 1);
        // IDs assigned y then x; first lighting row y=60, x=28.
        assert_eq!(st.organisms[0].geom.x, 28.5);
        assert_eq!(st.organisms[0].geom.y, 60.5);
    }

    #[test]
    fn t02_photosynthesis() {
        let rules = spec().reactions;
        let mut body = BodyView {
            n: 100,
            c: 0,
            c_max: 10_000,
            ..BodyView::default()
        };
        let mut cell = CellView {
            q: 10_000,
            ..CellView::default()
        };
        let d = photosynth(&mut body, &mut cell, 7, &rules).unwrap();
        assert_eq!(body.n, 93);
        assert_eq!(cell.q, 10_000 - 140);
        assert_eq!(body.c, 7);
        assert_eq!(d.dissipation, 70);
    }

    #[test]
    fn t03_organic_uptake() {
        let mut body = BodyView {
            c: 0,
            e_old: 9,
            e_new: 0,
            c_max: 10_000,
            ..BodyView::default()
        };
        let mut cell = CellView {
            o: 20,
            ..CellView::default()
        };
        o_uptake(&mut body, &mut cell, 13).unwrap();
        assert_eq!(cell.o, 7);
        assert_eq!(body.c, 13);
        assert_eq!(body.e_old, 9);
        assert_eq!(body.e_new, 0);
    }

    #[test]
    fn t04_catabolism() {
        let rules = spec().reactions;
        let mut body = BodyView {
            c: 20,
            e_old: 0,
            e_new: 0,
            e_max: 10_000,
            ..BodyView::default()
        };
        let mut cell = CellView::default();
        let d = catabolism(&mut body, &mut cell, 9, &rules).unwrap();
        assert_eq!(body.c, 11);
        assert_eq!(cell.n, 9);
        assert_eq!(body.e_new, 72);
        assert_eq!(d.dissipation, 18);
    }

    #[test]
    fn t05_single_pool_quota_order_independent() {
        let ids = [
            RequestId {
                phase: 1,
                actor: 10,
                purpose: 0,
                target: 0,
            },
            RequestId {
                phase: 1,
                actor: 11,
                purpose: 0,
                target: 0,
            },
            RequestId {
                phase: 1,
                actor: 12,
                purpose: 0,
                target: 0,
            },
        ];
        let demands = [5u128, 3, 2];
        let mut items: Vec<PoolDemand> = (0..3)
            .map(|i| PoolDemand {
                id: ids[i],
                demand: demands[i],
                priority: 0,
            })
            .collect();
        let g = allocate_pool(7, &items).unwrap();
        assert_eq!(g, vec![4, 2, 1]);
        items.reverse();
        let g2 = allocate_pool(7, &items).unwrap();
        assert_eq!(g2, vec![1, 2, 4]);
    }

    #[test]
    fn t06_multi_input_no_second_allocation() {
        // pools: 0 receiver C capacity, 1 giver0 E, 2 giver1 E, 3 giver0 C, 4 giver1 C
        let a = RequestId {
            phase: 12,
            actor: 1,
            purpose: 1,
            target: 9,
        };
        let b = RequestId {
            phase: 12,
            actor: 2,
            purpose: 1,
            target: 9,
        };
        let reqs = [
            MultiInputRequest {
                id: a,
                max_extent: 3,
                coeff: vec![1, 1, 0, 1, 0],
                priority_by_pool: vec![2, 0, 0, 0, 0],
            },
            MultiInputRequest {
                id: b,
                max_extent: 3,
                coeff: vec![1, 0, 1, 0, 1],
                priority_by_pool: vec![1, 0, 0, 0, 0],
            },
        ];
        let avail = [3u128, 1, 3, 100, 100];
        let x = common_extents(&avail, &reqs).unwrap();
        assert_eq!(x, vec![1, 1]);
        // unused capacity 1 is not reallocated this phase
        assert_eq!(x[0] + x[1], 2);
    }

    #[test]
    fn t07_no_spend_of_new_energy() {
        let rules = spec().reactions;
        let mut body = BodyView {
            c: 2,
            b: 0,
            e_old: 0,
            e_new: 0,
            e_max: 10_000,
            b_target: 1_000,
            c_max: 10_000,
            ..BodyView::default()
        };
        let mut cell = CellView::default();
        catabolism(&mut body, &mut cell, 1, &rules).unwrap();
        assert_eq!(body.e_new, 8);
        assert_eq!(body.c, 1);
        let repair_attempt = repair(&mut body, 1, &rules);
        assert!(repair_attempt.is_err());
        assert_eq!(body.b, 0);
        assert_eq!(body.e_new, 8);
        assert_eq!(body.e_old, 0);
    }

    #[test]
    fn t08_diffusion_center() {
        let spec = spec();
        let mut f = Fields::zeros(128, 128);
        let cx = 64u32;
        let cy = 64u32;
        let i = f.idx(cx, cy);
        f.n[i] = 128;
        let d = transport_deltas(&f.n, 128, 128, spec.world.n_divisor).unwrap();
        let mut next = f.n.clone();
        for (slot, delta) in next.iter_mut().zip(d.iter()) {
            *slot = (*slot as i128 + *delta) as u64;
        }
        assert_eq!(next[i], 120);
        assert_eq!(next[f.idx(cx + 1, cy)], 2);
        assert_eq!(next[f.idx(cx - 1, cy)], 2);
        assert_eq!(next[f.idx(cx, cy + 1)], 2);
        assert_eq!(next[f.idx(cx, cy - 1)], 2);
        assert_eq!(next.iter().copied().sum::<u64>(), 128);
    }

    #[test]
    fn t09_decay_remainders() {
        let spec = spec();
        let mut f = Fields::zeros(1, 1);
        f.s[0] = 1;
        f.s_remainder[0] = 999;
        f.q[0] = 1;
        f.q_remainder[0] = 15;
        let (n_gain, q_loss) = decay_s_q(&mut f, &spec).unwrap();
        assert_eq!(f.s[0], 0);
        assert_eq!(f.s_remainder[0], 0);
        assert_eq!(n_gain, 1);
        assert_eq!(f.n[0], 1);
        assert_eq!(f.q[0], 0);
        assert_eq!(q_loss, 1);
        assert_eq!(f.q_remainder[0], 0);
    }

    #[test]
    fn lifeless_field_mass_energy_identity() {
        let spec = spec();
        let mut st = initialize_world(&spec, 1).unwrap();
        st.organisms.clear();
        st.embryos.clear();
        let mut world = World::from_state(spec.clone(), st);
        let m0 = compute_mu(&world.state, spec.reactions.chem_e_per_organic)
            .unwrap()
            .m;
        for _ in 0..16 {
            world.step().unwrap();
            let mu = compute_mu(&world.state, spec.reactions.chem_e_per_organic).unwrap();
            assert_eq!(mu.m, m0);
        }
    }

    #[test]
    fn seed_does_not_change_t01_totals() {
        let spec = spec();
        let a = initialize_world(&spec, 1).unwrap();
        let b = initialize_world(&spec, 2).unwrap();
        let ma = compute_mu(&a, spec.reactions.chem_e_per_organic).unwrap();
        let mb = compute_mu(&b, spec.reactions.chem_e_per_organic).unwrap();
        assert_eq!(ma, mb);
    }
}

#[cfg(test)]
mod g2_tests {
    use super::*;
    use crate::physics::{project_two_equal_disks, wrap_angle};
    use crate::physiology::{deposit_death, is_irrecoverable};
    use universe_types::{Accounts, Geometry, RuleSpec};

    #[test]
    fn t12_two_disk_projection() {
        let (x0, y0, x1, y1) = project_two_equal_disks(10.0, 10.0, 10.5, 10.0, 0.3);
        assert!((x0 - 9.95).abs() <= 1e-12);
        assert!((x1 - 10.55).abs() <= 1e-12);
        assert!((y0 - 10.0).abs() <= 1e-12);
        assert!((y1 - 10.0).abs() <= 1e-12);
    }

    #[test]
    fn t11_death_deposit() {
        let spec = RuleSpec::eu_core_1();
        let mut st = initialize_world(&spec, 0).unwrap();
        st.organisms.truncate(1);
        {
            let o = &mut st.organisms[0];
            o.accounts = Accounts {
                n: 13,
                c: 7,
                b: 11,
                e_old: 3,
                e_new: 5,
            };
            o.geom = Geometry {
                x: 10.5,
                y: 10.5,
                theta: 0.0,
            };
        }
        let i = st.fields.idx(10, 10);
        let n0 = st.fields.n[i];
        let o0 = st.fields.o[i];
        let d = deposit_death(&mut st, &spec, 0).unwrap();
        assert_eq!(d, 8);
        assert_eq!(st.fields.n[i], n0 + 13);
        assert_eq!(st.fields.o[i], o0 + 18);
        assert!(st.organisms[0].marked_dead);
        assert_eq!(st.organisms[0].accounts.n, 0);
        // second deposit would be empty
        let d2 = deposit_death(&mut st, &spec, 0).unwrap();
        assert_eq!(d2, 0);
    }

    #[test]
    fn death_threshold_integer() {
        let spec = RuleSpec::eu_core_1();
        let mut st = initialize_world(&spec, 0).unwrap();
        st.organisms[0].accounts.b = spec.init.b_target / 5 - 1;
        assert!(is_irrecoverable(&st.organisms[0], &spec));
        st.organisms[0].accounts.b = spec.init.b_target / 5;
        st.organisms[0].maintenance_debt = 0;
        assert!(!is_irrecoverable(&st.organisms[0], &spec));
    }

    #[test]
    fn wrap_angle_range() {
        let p = std::f64::consts::PI;
        assert!(wrap_angle(p) < p);
        assert!(wrap_angle(-p) >= -p);
        assert_eq!(wrap_angle(-0.0).to_bits(), 0.0f64.to_bits());
    }

    #[test]
    fn stationary_autotroph_survives_in_light() {
        let spec = RuleSpec::eu_core_1();
        let mut st = initialize_world(&spec, 7).unwrap();
        st.organisms.truncate(1);
        let mut w = World::from_state(spec, st);
        w.disable_active_motion = true;
        w.disable_reproduction = true;
        for _ in 0..400 {
            w.step().unwrap();
        }
        assert_eq!(w.state.organisms.len(), 1);
        assert!(w.state.organisms[0].accounts.e_old > 100_000);
    }

    #[test]
    fn dark_world_consumes_finite_reserve() {
        let mut spec = RuleSpec::eu_core_1();
        spec.lighting.radiation_input_per_cell = 0;
        spec.lighting.initial_q_lit = 0;
        let mut st = initialize_world(&spec, 3).unwrap();
        st.organisms.truncate(1);
        st.fields.q.fill(0);
        let start_e = st.organisms[0].accounts.e_old;
        let mut w = World::from_state(spec, st);
        w.disable_active_motion = true;
        for _ in 0..2500 {
            w.step().unwrap();
            if w.state.organisms.is_empty() {
                return;
            }
        }
        let e = w.state.organisms[0].accounts.e_old;
        assert!(
            e < start_e,
            "dark reserve must eventually fall: {start_e} -> {e}"
        );
    }

    #[test]
    fn replay_hash_matches() {
        let spec = RuleSpec::eu_core_1();
        let st = initialize_world(&spec, 11).unwrap();
        let mut a = World::from_state(spec.clone(), st.clone());
        a.disable_active_motion = true;
        for _ in 0..5 {
            a.step().unwrap();
        }
        let mut b = World::from_state(spec, st);
        b.disable_active_motion = true;
        for _ in 0..5 {
            b.step().unwrap();
        }
        assert_eq!(
            universe_types::state_hash(&a.state).unwrap(),
            universe_types::state_hash(&b.state).unwrap()
        );
    }

    #[test]
    fn safety_pause_on_living_cap() {
        let mut spec = RuleSpec::eu_core_1();
        spec.safety.max_living = 4;
        let st = initialize_world(&spec, 0).unwrap();
        let mut w = World::from_state(spec, st);
        match w.step() {
            Err(universe_types::SimError::SafetyPause(_)) => {}
            other => panic!("expected pause, got {other:?}"),
        }
    }
}

#[cfg(test)]
mod g3_tests {
    use super::*;
    use crate::brain::{apply_readout_update, bernoulli_grad_logit};
    use universe_types::{BrainState, PHI, POLICY_LOGITS, RuleSpec};

    #[test]
    fn t13_bernoulli_grad() {
        let p = 0.25;
        let a = 1.0;
        let g = bernoulli_grad_logit(a, p);
        assert!((g - 0.75).abs() < 1e-15);
        let spec = RuleSpec::eu_core_1();
        let mut brain = BrainState::zeroed();
        brain.phi_prev[0] = 1.0;
        brain.grad_log[6][0] = 0.75;
        brain.has_pending = true;
        brain.r = 1.0;
        brain.gamma = 0.0;
        let a0 = [[0.0; PHI]; POLICY_LOGITS];
        let phi_next = {
            let mut p = [0.0; PHI];
            p[0] = 1.0;
            p
        };
        let p_before = 1.0 / (1.0 + libm::exp(-brain.a[6][0]));
        apply_readout_update(&mut brain, &a0, &phi_next, &spec);
        let p_after = 1.0 / (1.0 + libm::exp(-brain.a[6][0]));
        assert!(p_after > p_before);
    }

    #[test]
    fn td_numeric_example() {
        let mut gamma = 1.0;
        for _ in 0..5 {
            gamma *= 0.999;
        }
        assert!((gamma - 0.995009990004999_f64).abs() < 1e-12);
        let r = 0.2;
        let v_prev = 0.5;
        let v_next = 0.2;
        let delta = r + gamma * v_next - v_prev;
        assert!((delta + 0.100998001999_f64).abs() < 1e-12);
        let du = delta.clamp(-2.0, 2.0);
        let dv0 = 0.02 * du * 1.0;
        assert!((dv0 + 0.00201996003998).abs() < 1e-12);
    }

    #[test]
    fn t14_brain_isolation() {
        let spec = RuleSpec::eu_core_1();
        let mut st = initialize_world(&spec, 0).unwrap();
        assert!(st.organisms.len() >= 2);
        let b_before = st.organisms[1].brain.clone();
        let a0 = st.organisms[0].brain.a;
        st.organisms[0].brain.has_pending = true;
        st.organisms[0].brain.r = 0.5;
        st.organisms[0].brain.phi_prev[0] = 1.0;
        st.organisms[0].brain.grad_log[0][0] = 0.1;
        let phi = {
            let mut p = [0.0; PHI];
            p[0] = 1.0;
            p
        };
        apply_readout_update(&mut st.organisms[0].brain, &a0, &phi, &spec);
        assert_ne!(st.organisms[0].brain.a[0][0].to_bits(), a0[0][0].to_bits());
        assert_eq!(st.organisms[1].brain.as_ref(), b_before.as_ref());
    }

    #[test]
    fn genome_factory_deterministic() {
        let spec = RuleSpec::eu_core_1();
        let g1 = crate::genome::make_minimal_autotroph_genome(&spec).unwrap();
        let g2 = crate::genome::make_minimal_autotroph_genome(&spec).unwrap();
        assert_eq!(g1.hash, g2.hash);
        assert!(g1.param_count_ok());
    }

    #[test]
    fn decision_boundary_snapshot_restore() {
        let spec = RuleSpec::eu_core_1();
        let st = initialize_world(&spec, 19).unwrap();
        let mut cont = World::from_state(spec.clone(), st.clone());
        cont.disable_active_motion = true;
        cont.disable_reproduction = true;
        let mut hashes = Vec::new();
        for _ in 0..12 {
            cont.step().unwrap();
            hashes.push(universe_types::state_hash(&cont.state).unwrap());
        }
        // Restore at tick 5 (decision) via canonical bytes.
        let mut a = World::from_state(spec.clone(), st.clone());
        a.disable_active_motion = true;
        a.disable_reproduction = true;
        for _ in 0..5 {
            a.step().unwrap();
        }
        assert_eq!(a.state.tick, 5);
        let bytes = a.state.encode_canonical().unwrap();
        let restored = universe_types::WorldState::decode_canonical(&bytes).unwrap();
        let mut b = World::from_state(spec.clone(), restored);
        b.disable_active_motion = true;
        b.disable_reproduction = true;
        for _ in 0..7 {
            b.step().unwrap();
        }
        assert_eq!(
            universe_types::state_hash(&b.state).unwrap(),
            hashes[11]
        );
        // Restore at tick 6 (non-decision).
        let mut c = World::from_state(spec.clone(), st);
        c.disable_active_motion = true;
        c.disable_reproduction = true;
        for _ in 0..6 {
            c.step().unwrap();
        }
        let bytes = c.state.encode_canonical().unwrap();
        let restored = universe_types::WorldState::decode_canonical(&bytes).unwrap();
        let mut d = World::from_state(spec, restored);
        d.disable_active_motion = true;
        d.disable_reproduction = true;
        for _ in 0..6 {
            d.step().unwrap();
        }
        assert_eq!(
            universe_types::state_hash(&d.state).unwrap(),
            hashes[11]
        );
    }
}

#[cfg(test)]
mod g4_tests {
    use super::*;
    use crate::physiology::maintenance_cost;
    use crate::repro::{cancel_embryo, materialize_extent, mutate_from_parent, t10_accounts_after_5000};
    use universe_types::{Embryo, EmbryoStage, RuleSpec};

    #[test]
    fn t10_embryo_cancel_deposit() {
        let spec = RuleSpec::eu_core_1();
        let mut st = initialize_world(&spec, 0).unwrap();
        st.organisms.truncate(1);
        let (b, c, e) = t10_accounts_after_5000();
        let mut emb = Embryo {
            parent_id: st.organisms[0].entity_id,
            attempt_id: 1,
            genome_hash: st.genomes[0].hash,
            extent: 0,
            target_extent: spec.evolution.target_extent,
            b: 0,
            c: 0,
            e: 0,
            created_tick: 0,
            heading0: 0.0,
            ready_tick: None,
            stage: EmbryoStage::Building,
        };
        materialize_extent(&mut emb, &spec, 5_000);
        assert_eq!(emb.b, b);
        assert_eq!(emb.c, c);
        assert_eq!(emb.e, e);
        let (cx, cy) = crate::physics::cell_xy(
            st.organisms[0].geom.x,
            st.organisms[0].geom.y,
            spec.world.grid_width,
            spec.world.grid_height,
        )
        .unwrap();
        let i = st.fields.idx(cx, cy);
        let o0 = st.fields.o[i];
        st.embryos.push(emb);
        let diss = cancel_embryo(&mut st, &spec, 0).unwrap();
        assert_eq!(diss, 40_000);
        assert_eq!(st.fields.o[i], o0 + 25_000);
        assert!(st.embryos.is_empty());
        // construction dissipation 20_000 is already spent; cancel does not refund it
        assert_eq!(5_000 * spec.reactions.embryo_dissipate, 20_000);
    }

    #[test]
    fn extra_maintenance_with_embryo() {
        let spec = RuleSpec::eu_core_1();
        let st = initialize_world(&spec, 0).unwrap();
        let org = &st.organisms[0];
        assert_eq!(maintenance_cost(org, &spec, false), 2_192);
        assert_eq!(maintenance_cost(org, &spec, true), 2_392);
    }

    #[test]
    fn mutation_copies_encoding_not_weights() {
        let spec = RuleSpec::eu_core_1();
        let parent = crate::genome::make_minimal_autotroph_genome(&spec).unwrap();
        let child = mutate_from_parent(&parent, 7, 1, 1, &spec, 10).unwrap();
        // Child A/v are not taken from a living brain; they decode from (possibly mutated) q.
        let a0p = crate::genome::decode_a0(&parent);
        let a0c = crate::genome::decode_a0(&child);
        let _ = (a0p, a0c);
        assert!(child.param_count_ok());
        assert_eq!(child.q_win.len(), parent.q_win.len());
    }

    #[test]
    fn start_fee_not_refunded_on_cancel() {
        let spec = RuleSpec::eu_core_1();
        let mut st = initialize_world(&spec, 0).unwrap();
        st.organisms.truncate(1);
        st.organisms[0].age = spec.evolution.min_age;
        st.organisms[0].accounts.c = spec.evolution.min_c;
        st.organisms[0].accounts.e_old = spec.evolution.min_e_old;
        st.organisms[0].accounts.b = spec.init.b_target;
        st.organisms[0].control.reproduce = true;
        st.organisms[0].maintenance_debt = 0;
        let e0 = st.organisms[0].accounts.e_old;
        let (diss, ev) = crate::repro::phase_start_embryos(&mut st, &spec).unwrap();
        assert_eq!(diss, spec.evolution.start_fee);
        assert_eq!(st.organisms[0].accounts.e_old, e0 - spec.evolution.start_fee);
        assert_eq!(ev.len(), 1);
        let _ = cancel_embryo(&mut st, &spec, 0).unwrap();
        assert_eq!(st.organisms[0].accounts.e_old, e0 - spec.evolution.start_fee);
        assert!(st.embryos.is_empty());
    }

    #[test]
    fn full_extent_accounts() {
        let spec = RuleSpec::eu_core_1();
        let mut e = Embryo {
            parent_id: 1,
            attempt_id: 1,
            genome_hash: [0; 32],
            extent: 0,
            target_extent: spec.evolution.target_extent,
            b: 0,
            c: 0,
            e: 0,
            created_tick: 0,
            heading0: 0.0,
            ready_tick: None,
            stage: EmbryoStage::Building,
        };
        materialize_extent(&mut e, &spec, spec.evolution.target_extent);
        assert_eq!(e.b, 1_000_000);
        assert_eq!(e.c, 250_000);
        assert_eq!(e.e, 2_000_000);
    }
}

#[cfg(test)]
mod d1_tests;
