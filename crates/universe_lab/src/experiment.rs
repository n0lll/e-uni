//! Canonical experiment definition, source identity, and restorable runtime packs.

use std::fs;
use std::path::{Path, PathBuf};

use universe_core::probe::ControlMode;
use universe_core::world::World;
use universe_types::{
    hex_hash, rules_hash, sha256, state_hash, CanonWriter, Hash32, LightingRules, RuleSpec,
    WorldState,
};

/// Algorithm id for the file-list execution digest (not `git diff HEAD`).
pub const SOURCE_DIGEST_ALGO: &str = "eu-source-digest-v1";

/// Workspace paths that can change C5 / lab execution. Results under `docs/`
/// are excluded so writing artefacts does not invalidate a frozen batch.
pub const EXECUTION_SOURCE_ROOTS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "crates/universe_types",
    "crates/universe_core",
    "crates/universe_archive",
    "crates/universe_lab",
    "apps/narrator",
];

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
pub struct SourceFileEntry {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug)]
pub struct ExecutionSourceManifest {
    pub algorithm: String,
    pub digest: String,
    pub files: Vec<SourceFileEntry>,
    pub git_commit: String,
    pub git_dirty: bool,
    pub git_porcelain: String,
    pub execution_source_differs_from_head: bool,
    pub rustc: String,
    pub cargo: String,
}

pub fn workspace_root() -> Result<PathBuf, String> {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let crates = manifest
        .parent()
        .ok_or_else(|| "universe_lab Cargo.toml has no parent".to_string())?;
    crates
        .parent()
        .ok_or_else(|| "crates/ has no parent".to_string())
        .map(Path::to_path_buf)
}

fn skip_dir_name(name: &std::ffi::OsStr) -> bool {
    name == "target" || name == ".git"
}

fn collect_rel_paths(root: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for rel in EXECUTION_SOURCE_ROOTS {
        let path = root.join(rel);
        if !path.exists() {
            return Err(format!("execution source missing: {rel}"));
        }
        if path.is_file() {
            out.push((*rel).to_string());
            continue;
        }
        let mut stack = vec![path];
        while let Some(dir) = stack.pop() {
            let rd = fs::read_dir(&dir).map_err(|e| format!("read_dir {}: {e}", dir.display()))?;
            for ent in rd {
                let ent = ent.map_err(|e| e.to_string())?;
                if skip_dir_name(&ent.file_name()) {
                    continue;
                }
                let p = ent.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    let rel_path = p
                        .strip_prefix(root)
                        .map_err(|e| e.to_string())?
                        .to_string_lossy()
                        .replace('\\', "/");
                    if rel_path.starts_with("docs/") {
                        continue;
                    }
                    out.push(rel_path);
                }
            }
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn git_output(args: &[&str], cwd: &Path) -> Result<Vec<u8>, String> {
    let o = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&o.stderr)
        ));
    }
    Ok(o.stdout)
}

pub fn execution_source_manifest() -> Result<ExecutionSourceManifest, String> {
    let root = workspace_root()?;
    let paths = collect_rel_paths(&root)?;
    let mut files = Vec::with_capacity(paths.len());
    let mut canon = Vec::new();
    canon.extend_from_slice(SOURCE_DIGEST_ALGO.as_bytes());
    canon.push(b'\n');
    for rel in &paths {
        let bytes = fs::read(root.join(rel)).map_err(|e| format!("read {rel}: {e}"))?;
        let h = hex_hash(&sha256(&bytes));
        canon.extend_from_slice(rel.as_bytes());
        canon.push(b'\t');
        canon.extend_from_slice(h.as_bytes());
        canon.push(b'\n');
        files.push(SourceFileEntry {
            path: rel.clone(),
            sha256: h,
        });
    }
    let digest = hex_hash(&sha256(&canon));
    let porcelain = String::from_utf8_lossy(&git_output(&["status", "--porcelain"], &root)?)
        .replace("\r\n", "\n");
    let mut diff_args = vec!["diff", "--name-only", "HEAD", "--"];
    diff_args.extend(EXECUTION_SOURCE_ROOTS.iter().copied());
    let changed = String::from_utf8_lossy(&git_output(&diff_args, &root)?).replace("\r\n", "\n");
    let mut untracked_args = vec!["ls-files", "--others", "--exclude-standard", "--"];
    untracked_args.extend(EXECUTION_SOURCE_ROOTS.iter().copied());
    let untracked =
        String::from_utf8_lossy(&git_output(&untracked_args, &root)?).replace("\r\n", "\n");
    let execution_source_differs_from_head =
        changed.lines().any(|l| !l.is_empty()) || untracked.lines().any(|l| !l.is_empty());
    let rustc = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    let cargo = std::process::Command::new("cargo")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    Ok(ExecutionSourceManifest {
        algorithm: SOURCE_DIGEST_ALGO.into(),
        digest,
        files,
        git_commit: crate::d1::git_commit(),
        git_dirty: !porcelain.trim().is_empty(),
        git_porcelain: porcelain,
        execution_source_differs_from_head,
        rustc,
        cargo,
    })
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
