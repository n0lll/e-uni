//! F2 preparation: public initial states, T_recover sessions, development calibration.
//!
//! Formal evaluation is not exposed as a default CLI entry.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use universe_core::init::{initialize_reversal_world, retarget_audit_baseline};
use universe_core::probe::{budget_p_i128, ControlMode};
use universe_core::world::World;
use universe_types::{
    hex_hash, parse_lab_yaml, rules_hash, sha256, state_hash, CanonWriter, Hash32, LightingRules,
    RuleSpec, SimError, WorldState,
};

use crate::checkpoint::{ExperimentCheckpoint, ExperimentSession, LightEvent};
use crate::experiment::{
    close_radiation_keep_geometry, execution_source_manifest, light_right_schedule,
    ExecutionSourceManifest,
};
use crate::t_recover::{first_recover_from_log, RecoverOutcome, RecoverStatus, TickSample};
use crate::{paired_bootstrap, reversal_spec};

pub const F2_PROTOCOL_VERSION: u32 = 1;
pub const F2_GENERATOR: &str = "f2-init-v1";
pub const DEV_SEEDS: [u64; 12] = [
    10001, 10002, 10003, 10004, 10005, 10006, 10007, 10008, 10009, 10010, 10011, 10012,
];
pub const CANDIDATE_L: u64 = 200;
pub const CANDIDATE_W: u64 = 100;
pub const CANDIDATE_H: u64 = 4000;
pub const F2_STAT_SEED: u64 = 20260911;
pub const F2_BOOTSTRAP_DRAWS: usize = 2000;
pub const ROLE_DEV: &str = "development";

#[derive(Clone, Debug)]
pub struct InitRecipe {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub e: Option<u64>,
    pub c: Option<u64>,
    pub n: Option<u64>,
    pub quench_unlit_q: bool,
}

impl InitRecipe {
    pub fn v1_left_body_keep_residual_q() -> Self {
        Self {
            name: "v1_left_body_right_light_keep_residual_q".into(),
            x: 4.5,
            y: 8.5,
            e: None,
            c: None,
            n: None,
            quench_unlit_q: false,
        }
    }

    pub fn v2_quench_unlit_q() -> Self {
        Self {
            name: "v2_left_body_right_light_quench_unlit_q".into(),
            x: 4.5,
            y: 8.5,
            e: None,
            c: None,
            n: None,
            quench_unlit_q: true,
        }
    }

    pub fn v3_closer_quench() -> Self {
        Self {
            name: "v3_x6.5_right_light_quench_unlit_q".into(),
            x: 6.5,
            y: 8.5,
            e: None,
            c: None,
            n: None,
            quench_unlit_q: true,
        }
    }

    pub fn candidates() -> [Self; 3] {
        [
            Self::v1_left_body_keep_residual_q(),
            Self::v2_quench_unlit_q(),
            Self::v3_closer_quench(),
        ]
    }
}

pub fn lab_spec() -> RuleSpec {
    reversal_spec()
}

pub fn protocol_hash(spec: &RuleSpec, recipe: &InitRecipe, l: u64, w: u64, h: u64) -> Hash32 {
    let mut wr = CanonWriter::new();
    wr.raw(b"EUF2");
    wr.u32(F2_PROTOCOL_VERSION);
    wr.raw(F2_GENERATOR.as_bytes());
    wr.raw(&spec.encode_canonical().expect("spec"));
    wr.raw(recipe.name.as_bytes());
    wr.f64_canonical(recipe.x).expect("x");
    wr.f64_canonical(recipe.y).expect("y");
    wr.bool(recipe.quench_unlit_q);
    wr.option_u64(recipe.e);
    wr.option_u64(recipe.c);
    wr.option_u64(recipe.n);
    wr.u64(l);
    wr.u64(w);
    wr.u64(h);
    wr.u64(DEV_SEEDS.len() as u64);
    for s in DEV_SEEDS {
        wr.u64(s);
    }
    wr.raw(ROLE_DEV.as_bytes());
    wr.raw(b"restricted_time_freeze_minus_learn");
    wr.u64(F2_STAT_SEED);
    wr.u64(F2_BOOTSTRAP_DRAWS as u64);
    wr.u8(0); // probe default off for development runs
    wr.bool(true); // disable_reproduction
    wr.u64(0); // initial O
    sha256(&wr.into_inner())
}

pub fn run_def_hash(
    protocol: Hash32,
    seed: u64,
    group: &str,
    control: ControlMode,
    freeze: bool,
    initial_state_hash: Hash32,
) -> Hash32 {
    let mut w = CanonWriter::new();
    w.raw(b"F2RN");
    w.bytes32(&protocol);
    w.u64(seed);
    w.raw(group.as_bytes());
    control.encode_canonical(&mut w);
    w.bool(freeze);
    w.bytes32(&initial_state_hash);
    sha256(&w.into_inner())
}

fn quench_unlit(state: &mut WorldState, lighting: &LightingRules) {
    let w = state.fields.width;
    let h = state.fields.height;
    for y in 0..h {
        for x in 0..w {
            if !lighting.is_lit(x, y) {
                let i = state.fields.idx(x, y);
                state.fields.q[i] = 0;
                state.fields.q_remainder[i] = 0;
            }
        }
    }
}

fn quench_all_q(state: &mut WorldState) {
    state.fields.q.fill(0);
    state.fields.q_remainder.fill(0);
}

pub fn generate_common_state(recipe: &InitRecipe, seed: u64) -> Result<(RuleSpec, WorldState), SimError> {
    let spec0 = lab_spec();
    let mut st = initialize_reversal_world(&spec0, seed)?;
    {
        let org = &mut st.organisms[0];
        org.geom.x = recipe.x;
        org.geom.y = recipe.y;
        org.control = Default::default();
        org.intent = Default::default();
        org.feedback = Default::default();
        org.last_feedback = Default::default();
        org.brain.has_pending = false;
        org.maintenance_debt = 0;
        if let Some(e) = recipe.e {
            org.accounts.e_old = e;
            org.accounts.e_new = 0;
        }
        if let Some(c) = recipe.c {
            org.accounts.c = c;
        }
        if let Some(n) = recipe.n {
            org.accounts.n = n;
        }
    }
    st.fields.o.fill(0);
    st.embryos.clear();
    let mut w = World::from_state_checked(spec0, st)?;
    w.disable_reproduction = true;
    w.apply_lighting(light_right_schedule(&w.spec))?;
    if recipe.quench_unlit_q {
        quench_unlit(&mut w.state, &w.spec.lighting);
    }
    retarget_audit_baseline(&mut w.state, &w.spec)?;
    w.state.rules_hash = rules_hash(&w.spec)?;
    Ok((w.spec, w.state))
}

pub fn generate_residual_state(common_spec: &RuleSpec, common: &WorldState) -> Result<(RuleSpec, WorldState), SimError> {
    let mut w = World::from_state_checked(common_spec.clone(), common.clone())?;
    w.apply_lighting(close_radiation_keep_geometry(&w.spec))?;
    retarget_audit_baseline(&mut w.state, &w.spec)?;
    Ok((w.spec, w.state))
}

pub fn generate_negative_state(common_spec: &RuleSpec, common: &WorldState) -> Result<(RuleSpec, WorldState), SimError> {
    let mut w = World::from_state_checked(common_spec.clone(), common.clone())?;
    w.apply_lighting(close_radiation_keep_geometry(&w.spec))?;
    quench_all_q(&mut w.state);
    retarget_audit_baseline(&mut w.state, &w.spec)?;
    Ok((w.spec, w.state))
}

fn write_state_pair(dir: &Path, seed: u64, spec: &RuleSpec, state: &WorldState) -> Result<Hash32, String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let spec_y = serde_yaml::to_string(spec).map_err(|e| e.to_string())?;
    fs::write(dir.join(format!("seed_{seed}.spec.yaml")), spec_y).map_err(|e| e.to_string())?;
    let bytes = state.encode_canonical().map_err(|e| e.to_string())?;
    fs::write(dir.join(format!("seed_{seed}.eus1")), &bytes).map_err(|e| e.to_string())?;
    let h = state_hash(state).map_err(|e| e.to_string())?;
    let org = &state.organisms[0];
    let meta = format!(
        "seed={seed}\nstate_hash={}\nrules_hash={}\nE_old={}\nE_new={}\nC={}\nN={}\nB={}\ndebt={}\nx={}\ny={}\ntheta={}\nM0={}\nU0={}\n",
        hex_hash(&h),
        hex_hash(&state.rules_hash),
        org.accounts.e_old,
        org.accounts.e_new,
        org.accounts.c,
        org.accounts.n,
        org.accounts.b,
        org.maintenance_debt,
        org.geom.x,
        org.geom.y,
        org.geom.theta,
        state.audit.initial_m,
        state.audit.initial_u,
    );
    fs::write(dir.join(format!("seed_{seed}.meta.txt")), meta).map_err(|e| e.to_string())?;
    Ok(h)
}

pub fn load_state_pair(dir: &Path, seed: u64) -> Result<(RuleSpec, WorldState), String> {
    let yaml = fs::read_to_string(dir.join(format!("seed_{seed}.spec.yaml"))).map_err(|e| e.to_string())?;
    let spec = parse_lab_yaml(&yaml).map_err(|e| e.to_string())?;
    let bytes = fs::read(dir.join(format!("seed_{seed}.eus1"))).map_err(|e| e.to_string())?;
    let state = WorldState::decode_canonical(&bytes).map_err(|e| e.to_string())?;
    World::from_state_checked(spec.clone(), state.clone()).map_err(|e| e.to_string())?;
    Ok((spec, state))
}

#[derive(Clone, Debug)]
pub struct RunRecord {
    pub seed: u64,
    pub group: String,
    pub controller: String,
    pub freeze: bool,
    pub outcome: RecoverOutcome,
    pub experiment_hash: String,
    pub initial_hash: String,
    pub decisions: u64,
    pub updates: u64,
    pub photo: u64,
    pub survived_ticks: u64,
    pub final_p: i128,
    pub final_x: f64,
    pub final_y: f64,
    pub ticks: Vec<TickSample>,
    pub p0: i128,
}

fn open_session(
    spec: RuleSpec,
    state: WorldState,
    control: ControlMode,
    freeze: bool,
    l: u64,
    w: u64,
    h: u64,
    def: Hash32,
    schedule: Vec<LightEvent>,
    disable_motion: bool,
) -> Result<ExperimentSession, SimError> {
    let mut world = World::from_state_checked(spec, state)?;
    world.disable_reproduction = true;
    world.disable_active_motion = disable_motion;
    world.freeze_learning = freeze;
    world.control_mode = control;
    world.enable_probe = false;
    Ok(ExperimentSession::new(world, schedule, l, w, h, def, true))
}

pub fn run_loaded(
    spec: RuleSpec,
    state: WorldState,
    seed: u64,
    group: &str,
    control: ControlMode,
    freeze: bool,
    protocol: Hash32,
    l: u64,
    w: u64,
    h: u64,
    disable_motion: bool,
) -> Result<RunRecord, SimError> {
    let initial_hash = state_hash(&state)?;
    let def = run_def_hash(protocol, seed, group, control, freeze, initial_hash);
    let mut sess = open_session(
        spec,
        state,
        control,
        freeze,
        l,
        w,
        h,
        def,
        Vec::new(),
        disable_motion,
    )?;
    match sess.run_until(h) {
        Ok(()) => {}
        Err(e) => {
            sess.detector.fail_engineering(format!("{e}"));
        }
    }
    let org = sess.world.state.organisms.iter().find(|o| !o.marked_dead);
    let outcome = sess.detector.outcome(sess.t_rel().min(h).max(sess.detector.died_at().unwrap_or(sess.t_rel())));
    let p0 = sess.detector.p0;
    Ok(RunRecord {
        seed,
        group: group.into(),
        controller: control.label(),
        freeze,
        outcome,
        experiment_hash: hex_hash(&def),
        initial_hash: hex_hash(&initial_hash),
        decisions: sess.world.run_ledger.decision_count,
        updates: sess.world.run_ledger.learning_update_count,
        photo: sess.world.run_ledger.photo_extent,
        survived_ticks: sess.world.run_ledger.executed_ticks,
        final_p: org
            .map(|o| budget_p_i128(o.accounts.e_old, o.accounts.e_new, o.accounts.c))
            .unwrap_or(0),
        final_x: org.map(|o| o.geom.x).unwrap_or(0.0),
        final_y: org.map(|o| o.geom.y).unwrap_or(0.0),
        ticks: sess.tick_log,
        p0,
    })
}

fn write_ticks(path: &Path, rec: &RunRecord, p0: i128, l: u64, w: u64, h: u64) -> Result<(), String> {
    let mut f = fs::File::create(path).map_err(|e| e.to_string())?;
    writeln!(f, "# p0={p0} living0=1 L={l} W={w} H={h} seed={} group={}", rec.seed, rec.group)
        .map_err(|e| e.to_string())?;
    writeln!(
        f,
        "t_rel,world_tick_after,seed,group,living,photo,p,debt,debt_accrued,maint_due,maint_paid,maint_unpaid,cog_due,cog_paid,decision"
    )
    .map_err(|e| e.to_string())?;
    for s in &rec.ticks {
        writeln!(
            f,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            s.t_rel,
            s.world_tick_after,
            rec.seed,
            rec.group,
            u8::from(s.living),
            s.photo_extent,
            s.p,
            s.debt,
            s.debt_accrued,
            s.maint_due,
            s.maint_paid,
            s.maint_unpaid,
            s.cognition_due,
            s.cognition_paid,
            u8::from(s.decision_boundary)
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn parse_tick_csv(text: &str) -> Result<(i128, bool, Vec<TickSample>, u64, u64, u64), String> {
    let mut p0 = 0i128;
    let mut living0 = true;
    let mut l = CANDIDATE_L;
    let mut w = CANDIDATE_W;
    let mut h = CANDIDATE_H;
    let mut samples = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.starts_with('#') {
            for part in line.trim_start_matches('#').split_whitespace() {
                if let Some(v) = part.strip_prefix("p0=") {
                    p0 = v.parse().map_err(|e| format!("{e}"))?;
                }
                if let Some(v) = part.strip_prefix("living0=") {
                    living0 = v != "0";
                }
                if let Some(v) = part.strip_prefix("L=") {
                    l = v.parse().map_err(|e| format!("{e}"))?;
                }
                if let Some(v) = part.strip_prefix("W=") {
                    w = v.parse().map_err(|e| format!("{e}"))?;
                }
                if let Some(v) = part.strip_prefix("H=") {
                    h = v.parse().map_err(|e| format!("{e}"))?;
                }
            }
            continue;
        }
        if i == 0 || line.starts_with("t_rel,") {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let c: Vec<&str> = line.split(',').collect();
        if c.len() < 15 {
            return Err(format!("bad tick csv line {}", i + 1));
        }
        samples.push(TickSample {
            t_rel: c[0].parse().map_err(|e| format!("{e}"))?,
            world_tick_after: c[1].parse().map_err(|e| format!("{e}"))?,
            living: c[4] != "0",
            photo_extent: c[5].parse().map_err(|e| format!("{e}"))?,
            p: c[6].parse().map_err(|e| format!("{e}"))?,
            debt: c[7].parse().map_err(|e| format!("{e}"))?,
            debt_accrued: c[8].parse().map_err(|e| format!("{e}"))?,
            maint_due: c[9].parse().map_err(|e| format!("{e}"))?,
            maint_paid: c[10].parse().map_err(|e| format!("{e}"))?,
            maint_unpaid: c[11].parse().map_err(|e| format!("{e}"))?,
            cognition_due: c[12].parse().map_err(|e| format!("{e}"))?,
            cognition_paid: c[13].parse().map_err(|e| format!("{e}"))?,
            decision_boundary: c[14] != "0",
        });
    }
    Ok((p0, living0, samples, l, w, h))
}

pub fn recompute_file(path: &Path) -> Result<Option<u64>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let (p0, living0, samples, l, w, h) = parse_tick_csv(&text)?;
    Ok(first_recover_from_log(&samples, p0, living0, l, w, h))
}

fn status_str(s: RecoverStatus) -> &'static str {
    match s {
        RecoverStatus::Recovered => "Recovered",
        RecoverStatus::Died => "Died",
        RecoverStatus::NotRecoveredByH => "NotRecoveredByH",
        RecoverStatus::EngineeringError => "EngineeringError",
    }
}

pub fn generate_into(out: &Path, recipe: &InitRecipe) -> Result<Hash32, String> {
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let spec_lab = lab_spec();
    let proto = protocol_hash(&spec_lab, recipe, CANDIDATE_L, CANDIDATE_W, CANDIDATE_H);
    let common_dir = out.join("initial_states/common");
    let residual_dir = out.join("initial_states/residual");
    let negative_dir = out.join("initial_states/negative");
    let mut lines = String::from("seed,common_hash,residual_hash,negative_hash\n");
    for seed in DEV_SEEDS {
        let (cspec, cst) = generate_common_state(recipe, seed).map_err(|e| e.to_string())?;
        let ch = write_state_pair(&common_dir, seed, &cspec, &cst)?;
        let (rspec, rst) = generate_residual_state(&cspec, &cst).map_err(|e| e.to_string())?;
        let rh = write_state_pair(&residual_dir, seed, &rspec, &rst)?;
        let (nspec, nst) = generate_negative_state(&cspec, &cst).map_err(|e| e.to_string())?;
        let nh = write_state_pair(&negative_dir, seed, &nspec, &nst)?;
        lines.push_str(&format!(
            "{},{},{},{}\n",
            seed,
            hex_hash(&ch),
            hex_hash(&rh),
            hex_hash(&nh)
        ));
    }
    fs::write(out.join("initial_states/hashes.csv"), lines).map_err(|e| e.to_string())?;
    fs::write(out.join("dev_seeds.txt"), DEV_SEEDS.iter().map(|s| format!("{s}\n")).collect::<String>())
        .map_err(|e| e.to_string())?;
    fs::write(
        out.join("protocol.json"),
        protocol_json(&proto, recipe),
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        out.join("spec.yaml"),
        serde_yaml::to_string(&spec_lab).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(proto)
}

fn protocol_json(proto: &Hash32, recipe: &InitRecipe) -> String {
    format!(
        r#"{{
  "version": {ver},
  "generator": "{gen}",
  "role": "{role}",
  "recipe": "{recipe}",
  "x": {x},
  "y": {y},
  "quench_unlit_q": {q},
  "L": {l},
  "W": {w},
  "H": {h},
  "dev_seeds": {seeds:?},
  "stat_seed": {ss},
  "bootstrap_draws": {bd},
  "primary_metric": "restricted_time_freeze_minus_learn",
  "positive_means": "learning recovered faster",
  "protocol_hash": "{ph}",
  "isolation": {{
    "initial_count": 1,
    "disable_reproduction": true,
    "initial_O": 0,
    "archive": "not_used"
  }}
}}
"#,
        ver = F2_PROTOCOL_VERSION,
        gen = F2_GENERATOR,
        role = ROLE_DEV,
        recipe = recipe.name,
        x = recipe.x,
        y = recipe.y,
        q = recipe.quench_unlit_q,
        l = CANDIDATE_L,
        w = CANDIDATE_W,
        h = CANDIDATE_H,
        seeds = DEV_SEEDS,
        ss = F2_STAT_SEED,
        bd = F2_BOOTSTRAP_DRAWS,
        ph = hex_hash(proto)
    )
}

pub fn validate_dir(out: &Path) -> Result<(), String> {
    let hashes = fs::read_to_string(out.join("initial_states/hashes.csv")).map_err(|e| e.to_string())?;
    for (i, line) in hashes.lines().enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue;
        }
        let seed: u64 = line
            .split(',')
            .next()
            .ok_or("hash csv")?
            .parse()
            .map_err(|e| format!("{e}"))?;
        for kind in ["common", "residual", "negative"] {
            let (spec, st) = load_state_pair(&out.join("initial_states").join(kind), seed)?;
            if spec.init.initial_count != 1 {
                return Err("initial_count != 1".into());
            }
            if !st.embryos.is_empty() {
                return Err("embryos present".into());
            }
            if st.fields.o.iter().any(|&x| x != 0) {
                return Err("O field not zero".into());
            }
            if st.organisms.len() != 1 {
                return Err("not singleton".into());
            }
        }
        let (_, a) = load_state_pair(&out.join("initial_states/common"), seed)?;
        let (_, b) = load_state_pair(&out.join("initial_states/common"), seed)?;
        assert_eq!(state_hash(&a).unwrap(), state_hash(&b).unwrap());
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct DevSummary {
    pub recipe: String,
    pub protocol_hash: String,
    pub c3_recovered: usize,
    pub c4_recovered: usize,
    pub neg_false_positive: usize,
    pub residual_recovered: usize,
    pub learn_recovered: usize,
    pub freeze_recovered: usize,
    pub mean: Option<f64>,
    pub lo: Option<f64>,
    pub hi: Option<f64>,
    pub blocked: bool,
    pub block_reason: String,
    pub engineering: usize,
}

fn count_rec(rs: &[RunRecord]) -> usize {
    rs.iter()
        .filter(|r| r.outcome.status == RecoverStatus::Recovered)
        .count()
}

fn write_runs_csv(path: &Path, rows: &[RunRecord]) -> Result<(), String> {
    let mut f = fs::File::create(path).map_err(|e| e.to_string())?;
    writeln!(
        f,
        "seed,group,controller,freeze,status,success,restricted_time,first_recover,died_at,error,decisions,updates,photo,survived_ticks,final_p,final_x,final_y,initial_hash,experiment_hash"
    )
    .map_err(|e| e.to_string())?;
    for r in rows {
        writeln!(
            f,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            r.seed,
            r.group,
            r.controller,
            r.freeze,
            status_str(r.outcome.status),
            u8::from(r.outcome.success),
            r.outcome.restricted_time,
            r.outcome
                .first_recover_t
                .map(|t| t.to_string())
                .unwrap_or_default(),
            r.outcome
                .died_at
                .map(|t| t.to_string())
                .unwrap_or_default(),
            r.outcome.error.clone().unwrap_or_default().replace(',', ";"),
            r.decisions,
            r.updates,
            r.photo,
            r.survived_ticks,
            r.final_p,
            r.final_x,
            r.final_y,
            r.initial_hash,
            r.experiment_hash
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn run_group(
    out: &Path,
    kind: &str,
    group: &str,
    control: ControlMode,
    freeze: bool,
    proto: Hash32,
    disable_motion: bool,
) -> Result<Vec<RunRecord>, String> {
    let dir = out.join("initial_states").join(kind);
    let tdir = out.join("tick_metrics");
    fs::create_dir_all(&tdir).map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    for seed in DEV_SEEDS {
        let (spec, st) = load_state_pair(&dir, seed)?;
        let rec = run_loaded(
            spec,
            st,
            seed,
            group,
            control,
            freeze,
            proto,
            CANDIDATE_L,
            CANDIDATE_W,
            CANDIDATE_H,
            disable_motion,
        )
        .map_err(|e| e.to_string())?;
        write_ticks(
            &tdir.join(format!("{group}_{seed}.csv")),
            &rec,
            rec.p0,
            CANDIDATE_L,
            CANDIDATE_W,
            CANDIDATE_H,
        )?;
        rows.push(rec);
    }
    Ok(rows)
}

pub fn run_dev(out: &Path) -> Result<DevSummary, String> {
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let mut summary = DevSummary {
        recipe: String::new(),
        protocol_hash: String::new(),
        c3_recovered: 0,
        c4_recovered: 0,
        neg_false_positive: 0,
        residual_recovered: 0,
        learn_recovered: 0,
        freeze_recovered: 0,
        mean: None,
        lo: None,
        hi: None,
        blocked: false,
        block_reason: String::new(),
        engineering: 0,
    };
    let mut all_rows: Vec<RunRecord> = Vec::new();
    let mut locked = false;
    for recipe in InitRecipe::candidates() {
        let proto = generate_into(out, &recipe)?;
        validate_dir(out)?;
        summary.recipe = recipe.name.clone();
        summary.protocol_hash = hex_hash(&proto);
        let residual = run_group(
            out,
            "residual",
            "A_residual",
            ControlMode::Stationary,
            true,
            proto,
            true,
        )?;
        let negative = run_group(
            out,
            "negative",
            "B_negative",
            ControlMode::Stationary,
            true,
            proto,
            true,
        )?;
        let c3 = run_group(
            out,
            "common",
            "C3_oracle",
            ControlMode::OracleGotoLight,
            true,
            proto,
            false,
        )?;
        let c4 = run_group(
            out,
            "common",
            "C4_reflex",
            ControlMode::ReflexQ,
            true,
            proto,
            false,
        )?;
        summary.residual_recovered = count_rec(&residual);
        summary.neg_false_positive = count_rec(&negative);
        summary.c3_recovered = count_rec(&c3);
        summary.c4_recovered = count_rec(&c4);
        all_rows.clear();
        all_rows.extend(residual);
        all_rows.extend(negative.clone());
        all_rows.extend(c3);
        all_rows.extend(c4);
        if summary.neg_false_positive > 0 {
            summary.blocked = true;
            summary.block_reason = format!(
                "负对照误判恢复 {}/{}",
                summary.neg_false_positive,
                DEV_SEEDS.len()
            );
            write_runs_csv(&out.join("runs.csv"), &all_rows)?;
            break;
        }
        if summary.c3_recovered >= 8 {
            locked = true;
            let learn = run_group(
                out,
                "common",
                "learn",
                ControlMode::Neural,
                false,
                proto,
                false,
            )?;
            let freeze = run_group(
                out,
                "common",
                "freeze",
                ControlMode::Neural,
                true,
                proto,
                false,
            )?;
            summary.learn_recovered = count_rec(&learn);
            summary.freeze_recovered = count_rec(&freeze);
            summary.engineering = learn
                .iter()
                .chain(freeze.iter())
                .filter(|r| r.outcome.status == RecoverStatus::EngineeringError)
                .count();
            let mut diffs = Vec::new();
            let mut pair_csv = String::from("seed,restricted_freeze,restricted_learn,diff,learn_status,freeze_status\n");
            for seed in DEV_SEEDS {
                let l = learn.iter().find(|r| r.seed == seed).unwrap();
                let f = freeze.iter().find(|r| r.seed == seed).unwrap();
                if l.outcome.status == RecoverStatus::EngineeringError
                    || f.outcome.status == RecoverStatus::EngineeringError
                {
                    pair_csv.push_str(&format!(
                        "{seed},,,engineering,{},{}\n",
                        status_str(l.outcome.status),
                        status_str(f.outcome.status)
                    ));
                    continue;
                }
                let diff = f.outcome.restricted_time as f64 - l.outcome.restricted_time as f64;
                diffs.push(diff);
                pair_csv.push_str(&format!(
                    "{},{},{},{},{},{}\n",
                    seed,
                    f.outcome.restricted_time,
                    l.outcome.restricted_time,
                    diff,
                    status_str(l.outcome.status),
                    status_str(f.outcome.status)
                ));
            }
            fs::write(out.join("paired_differences.csv"), pair_csv).map_err(|e| e.to_string())?;
            if !diffs.is_empty() && summary.engineering == 0 {
                let (m, lo, hi) = paired_bootstrap(&diffs, F2_STAT_SEED, F2_BOOTSTRAP_DRAWS);
                summary.mean = Some(m);
                summary.lo = Some(lo);
                summary.hi = Some(hi);
                fs::write(
                    out.join("statistics.json"),
                    format!(
                        "{{\n  \"n_pairs\": {},\n  \"mean\": {m},\n  \"lo\": {lo},\n  \"hi\": {hi},\n  \"stat_seed\": {},\n  \"draws\": {},\n  \"quantile\": \"means[(0.025*draws) as usize] .. min((0.975*draws) as usize, draws-1)\"\n}}\n",
                        diffs.len(),
                        F2_STAT_SEED,
                        F2_BOOTSTRAP_DRAWS
                    ),
                )
                .map_err(|e| e.to_string())?;
            }
            all_rows.extend(learn);
            all_rows.extend(freeze);
            write_runs_csv(&out.join("runs.csv"), &all_rows)?;
            break;
        }
        write_runs_csv(&out.join("runs.csv"), &all_rows)?;
    }
    if !locked && !summary.blocked {
        summary.blocked = true;
        summary.block_reason = format!(
            "三组公开初态上 C3 恢复数均为 {}/{}，未达到 8/12 的开发锁定门槛",
            summary.c3_recovered,
            DEV_SEEDS.len()
        );
    }
    Ok(summary)
}

pub fn write_prep_docs(out: &Path, s: &DevSummary, elapsed_ms: u128) -> Result<(), String> {
    let src = write_source_manifest(out)?;
    fs::write(
        out.join("commands.txt"),
        "cargo test --offline --workspace --exclude observer --exclude narrator\n\
cargo run --release --offline -p universe_lab --bin f2 -- generate docs/f2-prep/20260911-f6440c3\n\
cargo run --release --offline -p universe_lab --bin f2 -- validate docs/f2-prep/20260911-f6440c3\n\
cargo run --release --offline -p universe_lab --bin f2 -- dev docs/f2-prep/20260911-f6440c3\n\
cargo run --release --offline -p universe_lab --bin f2 -- recompute docs/f2-prep/20260911-f6440c3\n",
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        out.join("manifest.json"),
        format!(
            "{{\n  \"run_id\": \"20260911-f6440c3\",\n  \"git_commit\": \"{}\",\n  \"git_dirty\": {},\n  \"source_digest\": \"{}\",\n  \"protocol_hash\": \"{}\",\n  \"recipe\": \"{}\",\n  \"elapsed_ms\": {elapsed_ms},\n  \"blocked\": {},\n  \"rustc\": \"{}\",\n  \"cargo\": \"{}\"\n}}\n",
            src.git_commit,
            src.git_dirty,
            src.digest,
            s.protocol_hash,
            s.recipe,
            s.blocked,
            jesc(&src.rustc),
            jesc(&src.cargo)
        ),
    )
    .map_err(|e| e.to_string())?;
    fs::write(out.join("open_issues.md"), open_issues_md()).map_err(|e| e.to_string())?;
    fs::write(out.join("report.md"), prep_report_md(s, &src, elapsed_ms)).map_err(|e| e.to_string())?;
    write_sample_frames(out)?;
    Ok(())
}

fn write_sample_frames(out: &Path) -> Result<(), String> {
    let (spec, st) = load_state_pair(&out.join("initial_states/common"), 10001)?;
    let mut w = World::from_state_checked(spec, st).map_err(|e| e.to_string())?;
    w.disable_reproduction = true;
    w.freeze_learning = false;
    w.control_mode = ControlMode::Neural;
    w.enable_probe = true;
    w.probe_frame_limit = 8;
    for _ in 0..40 {
        w.step().map_err(|e| e.to_string())?;
    }
    let mut s = String::from("# sample decision frames (seed 10001, 40 ticks, limit 8)\n");
    s.push_str(&format!(
        "aggregate_n={} updates={}\n",
        w.probe_aggregate.n, w.probe_aggregate.n_updates
    ));
    for d in &w.last_decisions {
        s.push_str(&format!(
            "tick={} applied={} task_norm={:?} entropy_norm={:?} trace_after={:?} update_after={:?} obs_state_tick={} feedback=[{},{}) ticks={}\n",
            d.tick,
            d.applied_update,
            d.actor_task_gradient_norm,
            d.entropy_term_norm,
            d.trace_norm_after_projection,
            d.update_norm_after_projection,
            d.observation_state_tick,
            d.feedback_start_tick,
            d.feedback_end_tick,
            d.feedback_ticks
        ));
    }
    fs::write(out.join("sample_frames.txt"), s).map_err(|e| e.to_string())?;
    Ok(())
}

fn prep_report_md(s: &DevSummary, src: &ExecutionSourceManifest, elapsed_ms: u128) -> String {
    format!(
        "# F2 准备报告（20260911-f6440c3）\n\n\
基线提交 `f6440c39c42bc544466199fcb405092e0af7a0bd`。本轮未提交代码与 HEAD 并存；执行源摘要见 `source_manifest.json`。\n\n\
## 缺陷处理\n\n\
- F01 非有限 logits：未屏蔽头报 `NumericFault::NonFinite`，世界步回退。合法 −∞ mask 与 RNG 槽保留。\n\
- F02 RuntimePack 使用 `RP01` + `ControlMode::encode_canonical`（含 Forced 参数）。\n\
- F03 新增 `EUCK` 实验 checkpoint（不含 Archive）。\n\
- F04 七个范数为 `Option<f64>`，无更新为 `None`；探针帧上限 256。\n\
- F05/F06 Archive 批次/pending_bytes：**本轮不用 Archive，延期**。\n\
- F07 `validate_lab`/`validate_core1` 拒绝零除数、零子步、硬编码 leak/sense_range/w_rec_l1 变化。\n\
- F08 先诊断控制器再 `zero_motion_only`，只清推进/转向。\n\
- `from_state` 仍无加载保证；F2 使用 `from_state_checked`。\n\n\
## 开发对照\n\n\
- recipe: {}\n\
- protocol_hash: {}\n\
- C3 Oracle 恢复: {}/12\n\
- C4 ReflexQ 恢复: {}/12\n\
- A 残留 Q 恢复: {}/12\n\
- B 负对照误判: {}/12\n\
- 神经学习恢复: {}/12\n\
- 神经冻结恢复: {}/12\n\
- 配对均值: {:?}\n\
- bootstrap 95% : [{:?}, {:?}]\n\
- blocked: {} {}\n\
- engineering_errors: {}\n\
- elapsed_ms: {elapsed_ms}\n\
- git: {} dirty={}\n\
- source_digest: {}\n\n\
主比较（看结果前锁定）：同 seed `受限时间_冻结 − 受限时间_学习`，正值=学习更快。统计种子 {}，{} 次，分位 `means[(0.025*draws) as usize]`。\n\n\
正式 F2 **未执行**。101–120 仍是历史/开发种子。L={}, W={}, H={} 仅在控制条件支持时作为候选。\n",
        s.recipe,
        s.protocol_hash,
        s.c3_recovered,
        s.c4_recovered,
        s.residual_recovered,
        s.neg_false_positive,
        s.learn_recovered,
        s.freeze_recovered,
        s.mean,
        s.lo,
        s.hi,
        s.blocked,
        s.block_reason,
        s.engineering,
        src.git_commit,
        src.git_dirty,
        src.digest,
        F2_STAT_SEED,
        F2_BOOTSTRAP_DRAWS,
        CANDIDATE_L,
        CANDIDATE_W,
        CANDIDATE_H
    )
}

fn open_issues_md() -> String {
    "# F2 准备未解决问题\n\n\
## 本轮隔离\n\n\
- R08：单体 `initial_count=1`，无共享配额竞争。\n\
- R09：`disable_reproduction=true`，无损伤，repair 仍未接入世界步进。AT02/AT03 未跑。\n\
- Archive F05/F06：`close_batch` 同边界覆盖与 `pending_bytes` 失败不回退 **未修**。F2 不使用 Archive。\n\
- `World::from_state` 仍无校验；权威加载入口是 `from_state_checked` / `into_world_checked` / `ExperimentCheckpoint`。\n\
- observer/narrator 未在本轮验收。\n\n\
## 正式 F2 前\n\n\
- 学习门禁仍以开发配对结果为准，不得把 10001–10012 或 101–120 当正式评估集。\n\
- 若负对照误判或 C3 不可达，不得冻结正式 L/W/H。\n".into()
}

pub fn replay_checks_json(out: &Path) -> Result<String, String> {
    let (spec, st) = load_state_pair(&out.join("initial_states/common"), 10001)?;
    let mut world = World::from_state_checked(spec, st).map_err(|e| e.to_string())?;
    world.disable_reproduction = true;
    world.freeze_learning = true;
    world.control_mode = ControlMode::Stationary;
    let mut s = ExperimentSession::new(world, Vec::new(), CANDIDATE_L, CANDIDATE_W, 80, [0; 32], false);
    s.run_until(10).map_err(|e| e.to_string())?;
    let bytes = s.checkpoint().encode().map_err(|e| e.to_string())?;
    fs::write(out.join("sample_checkpoint.euck"), &bytes).map_err(|e| e.to_string())?;
    let mut r1 = ExperimentCheckpoint::decode(&bytes)
        .map_err(|e| e.to_string())?
        .into_session()
        .map_err(|e| e.to_string())?;
    let mut r2 = ExperimentCheckpoint::decode(&bytes)
        .map_err(|e| e.to_string())?
        .into_session()
        .map_err(|e| e.to_string())?;
    r1.run_until(40).map_err(|e| e.to_string())?;
    r2.run_until(40).map_err(|e| e.to_string())?;
    let same = state_hash(&r1.world.state).unwrap() == state_hash(&r2.world.state).unwrap();
    Ok(format!(
        "{{\n  \"same_checkpoint_replayed_twice\": {same},\n  \"end_tick\": {}\n}}\n",
        r1.world.state.tick
    ))
}

pub fn write_source_manifest(out: &Path) -> Result<ExecutionSourceManifest, String> {
    let m = execution_source_manifest()?;
    let mut s = String::from("{\n");
    s.push_str(&format!("  \"algorithm\": \"{}\",\n", m.algorithm));
    s.push_str(&format!("  \"digest\": \"{}\",\n", m.digest));
    s.push_str(&format!("  \"git_commit\": \"{}\",\n", m.git_commit));
    s.push_str(&format!("  \"git_dirty\": {},\n", m.git_dirty));
    s.push_str(&format!(
        "  \"execution_source_differs_from_head\": {},\n",
        m.execution_source_differs_from_head
    ));
    s.push_str(&format!("  \"rustc\": \"{}\",\n", jesc(&m.rustc)));
    s.push_str(&format!("  \"cargo\": \"{}\",\n", jesc(&m.cargo)));
    s.push_str("  \"files\": [\n");
    for (i, f) in m.files.iter().enumerate() {
        s.push_str(&format!(
            "    {{\"path\": \"{}\", \"sha256\": \"{}\"}}{}\n",
            f.path,
            f.sha256,
            if i + 1 == m.files.len() { "" } else { "," }
        ));
    }
    s.push_str("  ]\n}\n");
    fs::write(out.join("source_manifest.json"), s).map_err(|e| e.to_string())?;
    Ok(m)
}

fn jesc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn default_out_dir() -> PathBuf {
    PathBuf::from("docs/f2-prep/20260911-f6440c3")
}

#[cfg(test)]
mod tests {
    use super::*;
    use universe_types::state_hash;

    #[test]
    fn learn_freeze_share_public_bytes() {
        let recipe = InitRecipe::v1_left_body_keep_residual_q();
        let (spec, st) = generate_common_state(&recipe, 10001).unwrap();
        let h = state_hash(&st).unwrap();
        let mut w1 = World::from_state_checked(spec.clone(), st.clone()).unwrap();
        let mut w2 = World::from_state_checked(spec, st).unwrap();
        w1.freeze_learning = false;
        w2.freeze_learning = true;
        w1.disable_reproduction = true;
        w2.disable_reproduction = true;
        assert_eq!(state_hash(&w1.state).unwrap(), h);
        assert_eq!(state_hash(&w2.state).unwrap(), h);
        assert_eq!(w1.state.encode_canonical().unwrap(), w2.state.encode_canonical().unwrap());
    }

    #[test]
    fn forced_params_not_in_public_state() {
        let recipe = InitRecipe::v1_left_body_keep_residual_q();
        let (spec, st) = generate_common_state(&recipe, 10002).unwrap();
        let mut a = World::from_state_checked(spec.clone(), st.clone()).unwrap();
        let mut b = World::from_state_checked(spec, st).unwrap();
        a.control_mode = ControlMode::Forced { thrust: 1, turn: 0 };
        b.control_mode = ControlMode::Forced { thrust: -1, turn: 0 };
        assert_eq!(state_hash(&a.state).unwrap(), state_hash(&b.state).unwrap());
    }
}
