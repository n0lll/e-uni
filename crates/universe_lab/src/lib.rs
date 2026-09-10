//! Experiment runners and diagnostic fixtures.

use universe_core::brain::{apply_readout_update, sample_policy_masked, store_grads};
use universe_core::init::initialize_reversal_world;
use universe_core::rng::random_unit;
use universe_core::world::World;
use universe_types::{BrainState, RuleSpec, PHI, POLICY_LOGITS};

pub mod d1;
pub mod d2;
pub mod d2d;
pub mod experiment;

/// Chapter 42.4 readout reversal, one seed.
/// Returns last-5k correct-direction fraction and mean last-step fixture reward.
pub fn readout_reversal_one_seed(seed: u64, freeze: bool) -> (f64, f64) {
    let spec = RuleSpec::eu_core_1();
    let mut brain = BrainState::zeroed();
    let mut phi = [0.0; PHI];
    phi[0] = 1.0;
    brain.phi_prev = phi;
    let a0 = [[0.0; PHI]; POLICY_LOGITS];
    brain.a = a0;
    let n = 40_000u64;
    let mut correct_last = 0u64;
    let mut reward_last = 0.0;
    for i in 0..n {
        let left_good = i < 20_000;
        let env_u = random_unit(seed.wrapping_mul(2).wrapping_add(2), 99, i, 0, 0, 0).unwrap();
        let success_left = if left_good { 0.8 } else { 0.2 };
        let env_left_success = env_u < success_left;
        let hs = sample_policy_masked(
            &brain.a,
            &phi,
            seed.wrapping_mul(2).wrapping_add(1),
            i,
            1,
            true,
            true,
        )
        .unwrap();
        let chose_left = hs.thrust < 0;
        let correct_dir = chose_left == left_good;
        let success = (chose_left && env_left_success) || (!chose_left && !env_left_success);
        store_grads(&mut brain, &phi, &hs, true);
        brain.r = 0.0;
        brain.gamma = 1.0;
        brain.trace_decay = 1.0;
        brain.interval_d = 0;
        for _ in 0..4 {
            brain.r += brain.gamma * 0.0;
            brain.gamma *= spec.brain.gamma_tick;
            brain.trace_decay *= spec.brain.gamma_tick * spec.brain.lambda_trace;
            brain.interval_d += 1;
        }
        let r_last = if success { 0.02 } else { 0.0 };
        brain.r += brain.gamma * r_last;
        brain.gamma *= spec.brain.gamma_tick;
        brain.trace_decay *= spec.brain.gamma_tick * spec.brain.lambda_trace;
        brain.interval_d += 1;
        if i >= n - 5000 {
            if correct_dir {
                correct_last += 1;
            }
            reward_last += r_last;
        }
        if !freeze {
            apply_readout_update(&mut brain, &a0, &phi, &spec);
        }
        brain.phi_prev = phi;
    }
    (correct_last as f64 / 5000.0, reward_last / 5000.0)
}

pub fn reversal_spec() -> RuleSpec {
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

pub(crate) fn light_right(spec: &mut RuleSpec) {
    spec.lighting.left_x0 = 10;
    spec.lighting.left_x1 = 12;
    spec.lighting.right_x0 = 10;
    spec.lighting.right_x1 = 12;
}

#[derive(Clone, Debug)]
pub struct ReversalMetrics {
    pub seed: u64,
    pub freeze: bool,
    pub second_half_metab_e: u64,
    pub survived_ticks: u64,
    pub final_e: u64,
    pub entered_new_light: bool,
}

pub fn run_resource_reversal(seed: u64, freeze: bool, half_ticks: u64) -> ReversalMetrics {
    let spec = reversal_spec();
    let st = initialize_reversal_world(&spec, seed).unwrap();
    let mut w = World::from_state(spec, st);
    w.freeze_learning = freeze;
    w.disable_reproduction = true;
    let mut second = 0u64;
    let mut survived = 0u64;
    let mut entered = false;
    let total = half_ticks.saturating_mul(2);
    for t in 0..total {
        if t == half_ticks {
            light_right(&mut w.spec);
        }
        if w.state.organisms.is_empty() {
            break;
        }
        if w.step().is_err() {
            break;
        }
        survived = w.state.tick;
        if t >= half_ticks {
            // metab_e_new = diss * (8/2)
            second = second.saturating_add(w.last_metab_diss.saturating_mul(4));
            if let Some(org) = w.state.organisms.first() {
                if org.geom.x >= 10.0 && org.geom.x <= 13.0 && org.geom.y >= 7.0 && org.geom.y <= 10.0
                {
                    entered = true;
                }
            }
        }
    }
    let final_e = w
        .state
        .organisms
        .first()
        .map(|o| o.accounts.e_old)
        .unwrap_or(0);
    ReversalMetrics {
        seed,
        freeze,
        second_half_metab_e: second,
        survived_ticks: survived,
        final_e,
        entered_new_light: entered,
    }
}

/// Independent-seed paired bootstrap of learn-minus-freeze differences.
pub fn paired_bootstrap(diffs: &[f64], stat_seed: u64, draws: usize) -> (f64, f64, f64) {
    if diffs.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let n = diffs.len();
    let mean: f64 = diffs.iter().sum::<f64>() / n as f64;
    let mut rng = SplitMix64 { s: stat_seed | 1 };
    let mut means = Vec::with_capacity(draws);
    for _ in 0..draws {
        let mut s = 0.0;
        for _ in 0..n {
            let j = (rng.next() as usize) % n;
            s += diffs[j];
        }
        means.push(s / n as f64);
    }
    means.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let lo = means[(0.025 * draws as f64) as usize];
    let hi = means[((0.975 * draws as f64) as usize).min(draws - 1)];
    (mean, lo, hi)
}

struct SplitMix64 {
    s: u64,
}

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.s = self.s.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }
}

pub const REVERSAL_SEEDS: [u64; 20] = [
    101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114, 115, 116, 117, 118, 119,
    120,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readout_smoke_one_hundred_not_nan() {
        let spec = RuleSpec::eu_core_1();
        let mut brain = BrainState::zeroed();
        let mut phi = [0.0; PHI];
        phi[0] = 1.0;
        brain.phi_prev = phi;
        let a0 = [[0.0; PHI]; POLICY_LOGITS];
        for i in 0..20u64 {
            let hs = sample_policy_masked(&brain.a, &phi, 1, i, 1, true, true).unwrap();
            store_grads(&mut brain, &phi, &hs, true);
            brain.r = 0.02;
            apply_readout_update(&mut brain, &a0, &phi, &spec);
            assert!(brain.a[0][0].is_finite());
        }
    }

    #[test]
    fn reversal_map_inits() {
        let spec = reversal_spec();
        let st = initialize_reversal_world(&spec, 7).unwrap();
        assert_eq!(st.organisms.len(), 1);
        assert_eq!(st.fields.width, 16);
        assert!((st.organisms[0].geom.x - 8.0).abs() < 1e-9);
    }

    #[test]
    fn bootstrap_recovers_constant_diff() {
        let diffs = [1.0; 20];
        let (m, lo, hi) = paired_bootstrap(&diffs, 99, 2000);
        assert!((m - 1.0).abs() < 1e-12);
        assert!((lo - 1.0).abs() < 1e-12);
        assert!((hi - 1.0).abs() < 1e-12);
    }

    #[test]
    fn measure_short_reversal_runtime() {
        let t0 = std::time::Instant::now();
        let m = run_resource_reversal(101, true, 50);
        eprintln!(
            "reversal_map 100 ticks freeze: {:?} survive={} metab_e={}",
            t0.elapsed(),
            m.survived_ticks,
            m.second_half_metab_e
        );
    }

    #[test]
    fn reversal_short_smoke() {
        let a = run_resource_reversal(101, false, 20);
        let b = run_resource_reversal(101, true, 20);
        assert!(a.survived_ticks > 0);
        assert!(b.survived_ticks > 0);
    }

    #[test]
    fn readout_one_seed_finite() {
        let (c, r) = readout_reversal_one_seed(21, false);
        let (cf, rf) = readout_reversal_one_seed(21, true);
        eprintln!("readout seed=21 learn_last5k={c:.4} reward={r:.5} freeze_last5k={cf:.4} reward={rf:.5}");
        assert!(c.is_finite() && r.is_finite());
        assert!(cf.is_finite() && rf.is_finite());
    }

    #[test]
    #[ignore]
    fn resource_reversal_20_seeds_full() {
        let half = 10_000u64;
        let mut diffs = Vec::new();
        for seed in REVERSAL_SEEDS {
            let learn = run_resource_reversal(seed, false, half);
            let freeze = run_resource_reversal(seed, true, half);
            diffs.push(learn.second_half_metab_e as f64 - freeze.second_half_metab_e as f64);
            eprintln!(
                "seed {} learn_e={} freeze_e={} survive_l={} survive_f={}",
                seed,
                learn.second_half_metab_e,
                freeze.second_half_metab_e,
                learn.survived_ticks,
                freeze.survived_ticks
            );
        }
        let (mean, lo, hi) = paired_bootstrap(&diffs, 20260910, 2000);
        eprintln!("paired mean={mean} ci=[{lo}, {hi}]");
        if lo <= 0.0 {
            eprintln!("尚未观察到可靠收益");
        }
    }

    #[test]
    fn archive_batch_and_replay_short() {
        use universe_archive::Archive;
        use universe_types::state_hash;
        let mut spec = reversal_spec();
        spec.archive.state_hash_every_ticks = 10;
        spec.archive.checkpoint_every_ticks = 10;
        spec.archive.aggregate_every_ticks = 10;
        let dir = std::env::temp_dir().join(format!(
            "eu-lab-arch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut arch = Archive::open(&dir, &spec).unwrap();
        let st = initialize_reversal_world(&spec, 3).unwrap();
        let mut w = World::from_state(spec.clone(), st);
        w.disable_reproduction = true;
        w.disable_active_motion = true;
        for _ in 0..25 {
            w.step().unwrap();
            arch.record_committed(&w.state, &w.last_events).unwrap();
        }
        let target = w.state.tick;
        let expect = state_hash(&w.state).unwrap();
        let (snap_tick, path, h) = arch
            .latest_snapshot_tick(target)
            .unwrap()
            .expect("snapshot");
        let loaded = Archive::load_snapshot_file(&path, Some(h)).unwrap();
        let mut w2 = World::from_state(spec, loaded);
        w2.disable_reproduction = true;
        w2.disable_active_motion = true;
        while w2.state.tick < target {
            w2.step().unwrap();
        }
        assert_eq!(snap_tick % 10, 0);
        assert_eq!(state_hash(&w2.state).unwrap(), expect);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn narrator_does_not_change_state_hash() {
        use narrator::run_window;
        use universe_archive::Archive;
        use universe_types::{state_hash, PresentationSpec};
        let spec = reversal_spec();
        let st = initialize_reversal_world(&spec, 5).unwrap();
        let mut w = World::from_state(spec.clone(), st);
        w.disable_reproduction = true;
        w.disable_active_motion = true;
        for _ in 0..8 {
            w.step().unwrap();
        }
        let h0 = state_hash(&w.state).unwrap();
        let dir = std::env::temp_dir().join(format!(
            "eu-iso-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let arch = Archive::open(&dir, &spec).unwrap();
        arch.set_durable_tick(w.state.tick).unwrap();
        let present = PresentationSpec::default();
        let _ = run_window(&arch, &spec, &present, 0, w.state.tick);
        assert_eq!(state_hash(&w.state).unwrap(), h0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
