//! Canonical experiment definition, source identity, and restorable runtime packs.

use universe_core::probe::ControlMode;
use universe_core::world::World;
use universe_types::{
    hex_hash, rules_hash, sha256, state_hash, CanonWriter, Hash32, LightingRules, RuleSpec,
    WorldState,
};

#[derive(Clone, Debug)]
pub struct SourceIdentity {
    pub commit: String,
    pub dirty: bool,
    pub source_digest: String,
    pub rustc: String,
}

pub fn source_identity() -> SourceIdentity {
    let commit = crate::d1::git_commit();
    let dirty = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(true);
    let diff = std::process::Command::new("git")
        .args(["diff", "HEAD"])
        .output()
        .ok()
        .map(|o| o.stdout)
        .unwrap_or_default();
    let source_digest = hex_hash(&sha256(&diff));
    let rustc = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    SourceIdentity {
        commit,
        dirty,
        source_digest,
        rustc,
    }
}

#[derive(Clone, Debug)]
pub struct ExperimentDef {
    pub name: String,
    pub version: u32,
    pub spec: RuleSpec,
    pub controller: ControlMode,
    pub freeze_learning: bool,
    pub disable_reproduction: bool,
    pub disable_active_motion: bool,
    pub start_x: Option<f64>,
    pub start_y: Option<f64>,
    pub heading: Option<f64>,
    pub dark_quench_q: bool,
    pub dark_close_input: bool,
    pub switch_tick: Option<u64>,
    pub max_ticks: u64,
    pub seed: u64,
    pub metric_name: String,
    pub stat_seed: u64,
}

impl ExperimentDef {
    pub fn rules_hash(&self) -> Hash32 {
        rules_hash(&self.spec).expect("rules")
    }

    pub fn base_rules_hash() -> Hash32 {
        rules_hash(&RuleSpec::eu_core_1()).expect("base")
    }

    pub fn encode_canonical(&self) -> Vec<u8> {
        let mut w = CanonWriter::new();
        w.raw(b"EUD2");
        w.u32(self.version);
        w.raw(self.name.as_bytes());
        w.u8(0);
        w.raw(&self.spec.encode_canonical().expect("spec"));
        w.u8(self.controller.code());
        match self.controller {
            ControlMode::Forced { thrust, turn } => {
                w.i8(thrust);
                w.i8(turn);
            }
            _ => {
                w.i8(0);
                w.i8(0);
            }
        }
        w.bool(self.freeze_learning);
        w.bool(self.disable_reproduction);
        w.bool(self.disable_active_motion);
        w.bool(self.start_x.is_some());
        if let Some(x) = self.start_x {
            w.f64_canonical(x).expect("x");
        }
        w.bool(self.start_y.is_some());
        if let Some(y) = self.start_y {
            w.f64_canonical(y).expect("y");
        }
        w.bool(self.heading.is_some());
        if let Some(h) = self.heading {
            w.f64_canonical(h).expect("h");
        }
        w.bool(self.dark_quench_q);
        w.bool(self.dark_close_input);
        w.bool(self.switch_tick.is_some());
        if let Some(t) = self.switch_tick {
            w.u64(t);
        }
        w.u64(self.max_ticks);
        w.u64(self.seed);
        w.raw(self.metric_name.as_bytes());
        w.u8(0);
        w.u64(self.stat_seed);
        w.into_inner()
    }

    pub fn experiment_hash(&self) -> Hash32 {
        sha256(&self.encode_canonical())
    }
}

#[derive(Clone, Debug)]
pub struct RuntimePack {
    pub spec: RuleSpec,
    pub state: WorldState,
    pub freeze_learning: bool,
    pub disable_reproduction: bool,
    pub disable_active_motion: bool,
    pub control_mode: ControlMode,
}

impl RuntimePack {
    pub fn from_world(w: &World) -> Self {
        Self {
            spec: w.spec.clone(),
            state: w.state.clone(),
            freeze_learning: w.freeze_learning,
            disable_reproduction: w.disable_reproduction,
            disable_active_motion: w.disable_active_motion,
            control_mode: w.control_mode,
        }
    }

    pub fn into_world(self) -> World {
        let mut w = World::from_state(self.spec, self.state);
        w.apply_runtime(
            self.freeze_learning,
            self.disable_active_motion,
            self.disable_reproduction,
            self.control_mode,
        );
        w
    }

    pub fn content_hash(&self) -> Hash32 {
        let mut w = CanonWriter::new();
        w.raw(&self.spec.encode_canonical().expect("spec"));
        w.raw(&self.state.encode_canonical().expect("state"));
        w.bool(self.freeze_learning);
        w.bool(self.disable_reproduction);
        w.bool(self.disable_active_motion);
        w.u8(self.control_mode.code());
        sha256(&w.into_inner())
    }
}

pub fn light_right_schedule(spec: &RuleSpec) -> LightingRules {
    let mut l = spec.lighting.clone();
    l.left_x0 = 10;
    l.left_x1 = 12;
    l.right_x0 = 10;
    l.right_x1 = 12;
    l
}

pub fn close_light_keep_q(spec: &RuleSpec) -> LightingRules {
    let mut l = spec.lighting.clone();
    l.y0 = 1;
    l.y1 = 0;
    l.radiation_input_per_cell = 0;
    l
}

pub fn state_fingerprint(state: &WorldState) -> String {
    hex_hash(&state_hash(state).expect("state hash"))
}
