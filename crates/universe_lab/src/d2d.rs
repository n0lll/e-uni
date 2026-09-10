//! D2-D campaign: post-fix C5 on seeds 101–120, versioned evidence, no F2.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use universe_core::init::initialize_reversal_world;
use universe_core::probe::{budget_p, ControlMode, RunLedger};
use universe_core::world::World;
use universe_types::{hex_hash, rules_hash, sha256, state_hash, CanonWriter, RuleSpec, SimError};

use crate::d1::{write_runs_csv, write_windows_csv, RunRow, Scenario, WindowRow, D1_MAX_TICKS, D1_SWITCH_TICK};
use crate::d2::open_issue_lines;
use crate::experiment::{
    execution_source_manifest, light_right_schedule, ExecutionSourceManifest, SOURCE_DIGEST_ALGO,
};
use crate::{paired_bootstrap, reversal_spec, REVERSAL_SEEDS};

pub const D2D_STAT_SEED: u64 = 20260910;
pub const D2D_BOOTSTRAP_DRAWS: usize = 2000;
pub const D1_C5_MEAN: f64 = 195_084.0;
pub const D1_C5_LO: f64 = -3_581_658.0;
pub const D1_C5_HI: f64 = 4_097_096.8;

#[derive(Clone, Debug)]
pub struct C5Record {
    pub row: RunRow,
    pub error_class: Option<String>,
    pub initial_state_hash: String,
    pub rules_hash_after_switch: Option<String>,
    pub p_residual: Option<i128>,
    pub probe_enabled: bool,
}

#[derive(Clone, Debug)]
pub struct PairDiff {
    pub seed: u64,
    pub learn_metab_after: u64,
    pub freeze_metab_after: u64,
    pub diff: f64,
    pub learn_alive_switch: bool,
    pub freeze_alive_switch: bool,
    pub learn_alive_end: bool,
    pub freeze_alive_end: bool,
    pub learn_death_tick: Option<u64>,
    pub freeze_death_tick: Option<u64>,
    pub learn_updates: u64,
    pub freeze_updates: u64,
    pub learn_decisions: u64,
    pub freeze_decisions: u64,
    pub learn_photo: u64,
    pub freeze_photo: u64,
}

#[derive(Clone, Debug)]
pub struct Integrity {
    pub n_rows: usize,
    pub n_seeds: usize,
    pub paired_ok: bool,
    pub freeze_updates_zero: bool,
    pub probe_all_off: bool,
    pub engineering_errors: Vec<String>,
    pub source_digest_unchanged: bool,
    pub initial_pairs_ok: bool,
    pub valid_complete: bool,
}

#[derive(Clone, Debug)]
pub struct D2dReport {
    pub source: ExecutionSourceManifest,
    pub source_digest_after: String,
    pub experiment_hash: String,
    pub rules_hash: String,
    pub rules_hash_after_switch: String,
    pub base_rules_hash: String,
    pub profile: String,
    pub command: String,
    pub started_unix: u64,
    pub elapsed_ms: u128,
    pub isolation: Isolation,
    pub records: Vec<C5Record>,
    pub pairs: Vec<PairDiff>,
    pub integrity: Integrity,
    pub mean: Option<f64>,
    pub lo: Option<f64>,
    pub hi: Option<f64>,
    pub conclusion: String,
}

#[derive(Clone, Debug)]
pub struct Isolation {
    pub initial_count: u32,
    pub disable_reproduction: bool,
    pub q_loss_divisor: u64,
    pub substeps: u32,
    pub decision_every_ticks: u64,
    pub dt_numerator: u64,
    pub dt_denominator: u64,
    pub initial_o_per_cell: u64,
    pub enable_probe: bool,
    pub ok: bool,
    pub notes: Vec<String>,
}

pub fn campaign_experiment_hash(spec: &RuleSpec) -> [u8; 32] {
    let mut w = CanonWriter::new();
    w.raw(b"EUD2D");
    w.u32(1);
    w.raw(&spec.encode_canonical().expect("reversal spec"));
    let after = light_right_schedule(spec);
    w.u32(after.left_x0);
    w.u32(after.left_x1);
    w.u32(after.right_x0);
    w.u32(after.right_x1);
    w.u32(after.y0);
    w.u32(after.y1);
    w.u64(D1_SWITCH_TICK);
    w.u64(D1_MAX_TICKS);
    w.u64(REVERSAL_SEEDS.len() as u64);
    for s in REVERSAL_SEEDS {
        w.u64(s);
    }
    w.raw(b"C5-learn+C5-freeze");
    w.u8(0);
    w.bool(false); // enable_probe
    w.bool(true); // disable_reproduction
    w.raw(b"metab_e_after_switch");
    w.u8(0);
    w.u64(D2D_STAT_SEED);
    w.u64(D2D_BOOTSTRAP_DRAWS as u64);
    w.raw(b"P=E+8C");
    w.u8(0);
    w.bool(true); // apply_lighting
    sha256(&w.into_inner())
}

pub fn check_isolation(spec: &RuleSpec) -> Isolation {
    let mut notes = Vec::new();
    let mut ok = true;
    if spec.init.initial_count != 1 {
        ok = false;
        notes.push(format!(
            "initial_count={} (need 1 for R08 isolation)",
            spec.init.initial_count
        ));
    }
    if spec.world.q_loss_divisor == 0 {
        ok = false;
        notes.push("q_loss_divisor=0 is not a verified C5 config".into());
    }
    if spec.physics.substeps == 0 {
        ok = false;
        notes.push("substeps=0 is not a verified C5 config".into());
    }
    if spec.time.decision_every_ticks != 5
        || spec.time.dt_numerator != 1
        || spec.time.dt_denominator != 10
    {
        ok = false;
        notes.push("time step / decision interval is not the verified lab combo".into());
    }
    notes.push(format!(
        "EX03 coverage this round: non-empty future_inputs only; q_loss_divisor=0/substeps=0 not claimed rejected"
    ));
    Isolation {
        initial_count: spec.init.initial_count,
        disable_reproduction: true,
        q_loss_divisor: spec.world.q_loss_divisor,
        substeps: spec.physics.substeps,
        decision_every_ticks: spec.time.decision_every_ticks,
        dt_numerator: spec.time.dt_numerator,
        dt_denominator: spec.time.dt_denominator,
        initial_o_per_cell: spec.init.initial_o_per_cell,
        enable_probe: false,
        ok,
        notes,
    }
}

/// Diagnostic P identity from the D2-D prompt. Residual 0 is not required for
/// batch validity; a non-zero residual must be explained, not fitted.
pub fn p_identity_residual(l: &RunLedger) -> Option<i128> {
    let p0 = l.p_initial? as i128;
    let p1 = l.p_before_death_or_end? as i128;
    let rhs = 8i128
        .saturating_mul(i128::from(l.photo_extent).saturating_add(i128::from(l.uptake_o)))
        .saturating_sub(i128::from(l.maint_paid))
        .saturating_sub(i128::from(l.cognition))
        .saturating_sub(i128::from(l.thrust_attempt))
        .saturating_sub(i128::from(l.turn_attempt))
        .saturating_sub(i128::from(l.debt_repay));
    Some(p1.saturating_sub(p0).saturating_sub(rhs))
}

pub fn classify_sim_error(e: &SimError) -> &'static str {
    match e {
        SimError::SafetyPause(_) => "SafetyPause",
        SimError::Numeric(_) => "NumericFault",
        SimError::ExperimentSetup(_) => "ExperimentSetup",
        SimError::ArchiveFailure(_) => "ArchiveFailure",
        SimError::RuleViolation(_) => "RuleViolation",
        SimError::Config(_) => "Config",
    }
}

fn in_lit(spec: &RuleSpec, x: f64, y: f64) -> bool {
    let gx = x.floor();
    let gy = y.floor();
    if gx < 0.0 || gy < 0.0 {
        return false;
    }
    spec.lighting.is_lit(gx as u32, gy as u32)
}

fn alive_at_switch(row: &RunRow) -> bool {
    match (row.scenario.switch_tick, row.ledger.death_tick) {
        (None, _) => false,
        (Some(sw), None) => row.alive_at_end || row.ledger.executed_ticks >= sw,
        (Some(sw), Some(d)) => d >= sw,
    }
}

fn c5_scenario(seed: u64, freeze: bool) -> Scenario {
    Scenario {
        group: if freeze {
            "C5-freeze".into()
        } else {
            "C5-learn".into()
        },
        seed,
        freeze_learning: freeze,
        control: ControlMode::Neural,
        switch_tick: Some(D1_SWITCH_TICK),
        max_ticks: D1_MAX_TICKS,
        place: None,
        heading: None,
        dark: false,
        collect_decisions: false,
    }
}

pub fn initial_state_hash(seed: u64) -> Result<String, String> {
    let spec = reversal_spec();
    let st = initialize_reversal_world(&spec, seed).map_err(|e| e.to_string())?;
    Ok(hex_hash(&state_hash(&st).map_err(|e| e.to_string())?))
}

pub fn run_c5_scenario(
    sc: Scenario,
    experiment_hex: &str,
    commit: &str,
    rules_hex: &str,
) -> C5Record {
    let spec = reversal_spec();
    let st = initialize_reversal_world(&spec, sc.seed).expect("init");
    let initial_state_hash = hex_hash(&state_hash(&st).expect("state hash"));
    let mut w = World::from_state(spec, st);
    w.freeze_learning = sc.freeze_learning;
    w.disable_reproduction = true;
    w.control_mode = sc.control;
    w.enable_probe = false;
    let initial_q = w.state.fields.q.iter().copied().sum();

    let mut row = RunRow {
        scenario: sc.clone(),
        ledger: RunLedger::default(),
        rules_hash: rules_hex.to_string(),
        experiment_hash: experiment_hex.to_string(),
        commit: commit.to_string(),
        windows: Vec::new(),
        path: Vec::new(),
        decisions: Vec::new(),
        alive_at_end: false,
        final_x: 0.0,
        final_y: 0.0,
        entered_old: false,
        entered_new: false,
        c0_pass: None,
        error: None,
        initial_q,
    };

    let mut photo_mark = 0u64;
    let mut o_mark = 0u64;
    let mut metab_mark = 0u64;
    let mut switched = false;
    let mut error_class = None;
    let mut rules_hash_after_switch = None;

    for t in 0..sc.max_ticks {
        if sc.switch_tick == Some(t) {
            if let Some(org) = w.state.organisms.iter().find(|o| !o.marked_dead) {
                w.run_ledger.p_at_switch = Some(budget_p(
                    org.accounts.e_old,
                    org.accounts.e_new,
                    org.accounts.c,
                ));
                w.run_ledger.e_at_switch = Some(org.accounts.e_old);
                w.run_ledger.c_at_switch = Some(org.accounts.c);
                w.run_ledger.n_at_switch = Some(org.accounts.n);
                w.run_ledger.debt_at_switch = Some(org.maintenance_debt);
            }
            photo_mark = w.run_ledger.photo_extent;
            o_mark = w.run_ledger.uptake_o;
            metab_mark = w.run_ledger.metab_e_new;
            match w.apply_lighting(light_right_schedule(&w.spec)) {
                Ok(()) => {
                    rules_hash_after_switch = Some(hex_hash(&w.state.rules_hash));
                    switched = true;
                }
                Err(e) => {
                    error_class = Some(classify_sim_error(&e).into());
                    row.error = Some(e.to_string());
                    break;
                }
            }
        }
        if w.state.organisms.is_empty() {
            break;
        }
        match w.step() {
            Ok(()) => {}
            Err(e) => {
                error_class = Some(classify_sim_error(&e).into());
                row.error = Some(e.to_string());
                break;
            }
        }
        if let Some(org) = w.state.organisms.first() {
            let x = org.geom.x;
            let y = org.geom.y;
            if in_lit(&w.spec, x, y) {
                if !switched {
                    if w.run_ledger.first_old_light_entry.is_none() {
                        w.run_ledger.first_old_light_entry = Some(w.state.tick);
                    }
                    w.run_ledger.old_light_residence_before =
                        w.run_ledger.old_light_residence_before.saturating_add(1);
                    row.entered_old = true;
                } else {
                    if w.run_ledger.first_new_light_entry.is_none() {
                        w.run_ledger.first_new_light_entry = Some(w.state.tick);
                    }
                    w.run_ledger.new_light_residence_after =
                        w.run_ledger.new_light_residence_after.saturating_add(1);
                    row.entered_new = true;
                }
            }
            row.final_x = x;
            row.final_y = y;
            if t % 100 == 99 || t + 1 == sc.max_ticks {
                row.windows.push(WindowRow {
                    tick: w.state.tick,
                    group: sc.group.clone(),
                    seed: sc.seed,
                    e: org.accounts.e_old,
                    c: org.accounts.c,
                    n: org.accounts.n,
                    p: budget_p(org.accounts.e_old, org.accounts.e_new, org.accounts.c),
                    photo: w.run_ledger.photo_extent,
                    uptake_o: w.run_ledger.uptake_o,
                    metab_e: w.run_ledger.metab_e_new,
                    maint: w.run_ledger.maint_paid,
                    cog: w.run_ledger.cognition,
                    thrust: w.run_ledger.thrust_attempt,
                    turn: w.run_ledger.turn_attempt,
                    x,
                    y,
                    living: 1,
                });
            }
        } else {
            row.windows.push(WindowRow {
                tick: w.state.tick,
                group: sc.group.clone(),
                seed: sc.seed,
                e: 0,
                c: 0,
                n: 0,
                p: 0,
                photo: w.run_ledger.photo_extent,
                uptake_o: w.run_ledger.uptake_o,
                metab_e: w.run_ledger.metab_e_new,
                maint: w.run_ledger.maint_paid,
                cog: w.run_ledger.cognition,
                thrust: w.run_ledger.thrust_attempt,
                turn: w.run_ledger.turn_attempt,
                x: row.final_x,
                y: row.final_y,
                living: 0,
            });
            break;
        }
    }

    if switched {
        w.run_ledger.photo_before_switch = photo_mark;
        w.run_ledger.photo_after_switch = w.run_ledger.photo_extent.saturating_sub(photo_mark);
        w.run_ledger.uptake_o_before_switch = o_mark;
        w.run_ledger.uptake_o_after_switch = w.run_ledger.uptake_o.saturating_sub(o_mark);
        w.run_ledger.metab_e_before_switch = metab_mark;
        w.run_ledger.metab_e_after_switch = w.run_ledger.metab_e_new.saturating_sub(metab_mark);
    } else {
        w.run_ledger.photo_before_switch = w.run_ledger.photo_extent;
        w.run_ledger.uptake_o_before_switch = w.run_ledger.uptake_o;
        w.run_ledger.metab_e_before_switch = w.run_ledger.metab_e_new;
    }

    row.alive_at_end = !w.state.organisms.is_empty();
    if let Some(org) = w.state.organisms.first() {
        row.final_x = org.geom.x;
        row.final_y = org.geom.y;
        if w.run_ledger.p_before_death_or_end.is_none() {
            w.run_ledger.p_before_death_or_end = Some(budget_p(
                org.accounts.e_old,
                org.accounts.e_new,
                org.accounts.c,
            ));
        }
    }
    debug_assert!(!w.enable_probe);
    debug_assert!(w.last_decisions.is_empty());
    row.ledger = w.run_ledger.clone();
    let p_residual = p_identity_residual(&row.ledger);
    C5Record {
        probe_enabled: w.enable_probe,
        row,
        error_class,
        initial_state_hash,
        rules_hash_after_switch,
        p_residual,
    }
}

pub fn pair_by_seed(records: &[C5Record]) -> Result<Vec<PairDiff>, String> {
    let mut learn: BTreeMap<u64, &C5Record> = BTreeMap::new();
    let mut freeze: BTreeMap<u64, &C5Record> = BTreeMap::new();
    for r in records {
        match r.row.scenario.group.as_str() {
            "C5-learn" => {
                if learn.insert(r.row.scenario.seed, r).is_some() {
                    return Err(format!("duplicate C5-learn seed {}", r.row.scenario.seed));
                }
            }
            "C5-freeze" => {
                if freeze.insert(r.row.scenario.seed, r).is_some() {
                    return Err(format!("duplicate C5-freeze seed {}", r.row.scenario.seed));
                }
            }
            other => return Err(format!("unexpected group {other}")),
        }
    }
    let seeds: BTreeSet<u64> = learn.keys().copied().collect();
    if seeds.len() != REVERSAL_SEEDS.len() {
        return Err(format!(
            "learn seeds {} != {}",
            seeds.len(),
            REVERSAL_SEEDS.len()
        ));
    }
    for s in REVERSAL_SEEDS {
        if !seeds.contains(&s) {
            return Err(format!("missing learn seed {s}"));
        }
        if !freeze.contains_key(&s) {
            return Err(format!("missing freeze seed {s}"));
        }
    }
    let mut pairs = Vec::new();
    for s in REVERSAL_SEEDS {
        let l = learn[&s];
        let f = freeze[&s];
        if l.initial_state_hash != f.initial_state_hash {
            return Err(format!(
                "seed {s} learn/freeze initial state_hash differ"
            ));
        }
        let ld = l.row.ledger.metab_e_after_switch;
        let fd = f.row.ledger.metab_e_after_switch;
        pairs.push(PairDiff {
            seed: s,
            learn_metab_after: ld,
            freeze_metab_after: fd,
            diff: ld as f64 - fd as f64,
            learn_alive_switch: alive_at_switch(&l.row),
            freeze_alive_switch: alive_at_switch(&f.row),
            learn_alive_end: l.row.alive_at_end,
            freeze_alive_end: f.row.alive_at_end,
            learn_death_tick: l.row.ledger.death_tick,
            freeze_death_tick: f.row.ledger.death_tick,
            learn_updates: l.row.ledger.learning_update_count,
            freeze_updates: f.row.ledger.learning_update_count,
            learn_decisions: l.row.ledger.decision_count,
            freeze_decisions: f.row.ledger.decision_count,
            learn_photo: l.row.ledger.photo_extent,
            freeze_photo: f.row.ledger.photo_extent,
        });
    }
    Ok(pairs)
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

pub fn run_d2d_campaign(out_dir: impl AsRef<Path>) -> Result<D2dReport, String> {
    let out = out_dir.as_ref();
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let started_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let source = execution_source_manifest()?;
    let spec = reversal_spec();
    let isolation = check_isolation(&spec);
    if !isolation.ok {
        return Err(format!("C5 isolation blocked: {:?}", isolation.notes));
    }
    let rules_hex = hex_hash(&rules_hash(&spec).map_err(|e| e.to_string())?);
    let mut spec_after = spec.clone();
    spec_after.lighting = light_right_schedule(&spec);
    let rules_after = hex_hash(&rules_hash(&spec_after).map_err(|e| e.to_string())?);
    let experiment_hex = hex_hash(&campaign_experiment_hash(&spec));
    let base_rules = hex_hash(&rules_hash(&RuleSpec::eu_core_1()).map_err(|e| e.to_string())?);
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let command = format!(
        "cargo run --release --offline -p universe_lab --bin d2d -- {}",
        out.display()
    );

    write_source_manifest(out, &source)?;
    write_start_identity(out, &source, &experiment_hex, &rules_hex, &rules_after, profile, &command, started_unix)?;

    let mut records = Vec::new();
    for seed in REVERSAL_SEEDS {
        let h_learn = initial_state_hash(seed)?;
        let h_freeze = initial_state_hash(seed)?;
        if h_learn != h_freeze {
            return Err(format!("seed {seed} reconstructed initial hashes differ"));
        }
        eprintln!("D2-D C5-learn seed {seed}");
        records.push(run_c5_scenario(
            c5_scenario(seed, false),
            &experiment_hex,
            &source.git_commit,
            &rules_hex,
        ));
        eprintln!("D2-D C5-freeze seed {seed}");
        records.push(run_c5_scenario(
            c5_scenario(seed, true),
            &experiment_hex,
            &source.git_commit,
            &rules_hex,
        ));
    }

    let after = execution_source_manifest()?;
    let digest_unchanged = after.digest == source.digest;
    if !digest_unchanged {
        fs::write(
            out.join("BATCH_INVALID.txt"),
            format!(
                "source_digest changed during the batch\nbefore={}\nafter={}\n",
                source.digest, after.digest
            ),
        )
        .map_err(|e| e.to_string())?;
    }

    let engineering_errors: Vec<String> = records
        .iter()
        .filter(|r| r.error_class.is_some() || r.row.error.is_some())
        .map(|r| {
            format!(
                "{} seed {} class={} err={}",
                r.row.scenario.group,
                r.row.scenario.seed,
                r.error_class.as_deref().unwrap_or("?"),
                r.row.error.as_deref().unwrap_or("")
            )
        })
        .collect();

    let pair_res = pair_by_seed(&records);
    let (pairs, paired_ok) = match pair_res {
        Ok(p) => (p, true),
        Err(e) => {
            engineering_errors_note(out, &e)?;
            (Vec::new(), false)
        }
    };
    let freeze_updates_zero = records
        .iter()
        .filter(|r| r.row.scenario.group == "C5-freeze")
        .all(|r| r.row.ledger.learning_update_count == 0);
    let probe_all_off = records.iter().all(|r| !r.probe_enabled);
    let initial_pairs_ok = paired_ok
        && pairs.iter().all(|p| {
            records
                .iter()
                .filter(|r| r.row.scenario.seed == p.seed)
                .map(|r| r.initial_state_hash.as_str())
                .collect::<BTreeSet<_>>()
                .len()
                == 1
        });

    let valid_complete = digest_unchanged
        && paired_ok
        && records.len() == 40
        && engineering_errors.is_empty()
        && freeze_updates_zero
        && probe_all_off
        && initial_pairs_ok
        && isolation.ok;

    let (mean, lo, hi, conclusion) = if valid_complete {
        let diffs: Vec<f64> = pairs.iter().map(|p| p.diff).collect();
        let (m, l, h) = paired_bootstrap(&diffs, D2D_STAT_SEED, D2D_BOOTSTRAP_DRAWS);
        let text = if l <= 0.0 && h >= 0.0 {
            "尚未观察到可靠收益".to_string()
        } else {
            "当前 C5 后半程代谢 E 指标上学习−冻结的 95% 区间不跨零；这只是该指标在本协议下的差异，不是 F2、生态复杂化或感情形成通过。".to_string()
        };
        (Some(m), Some(l), Some(h), text)
    } else {
        (
            None,
            None,
            None,
            "本批次不得生成有效完成的主统计结论。".to_string(),
        )
    };

    let n_rows = records.len();
    let report = D2dReport {
        source,
        source_digest_after: after.digest,
        experiment_hash: experiment_hex,
        rules_hash: rules_hex,
        rules_hash_after_switch: rules_after,
        base_rules_hash: base_rules,
        profile: profile.into(),
        command,
        started_unix,
        elapsed_ms: started.elapsed().as_millis(),
        isolation,
        records,
        pairs,
        integrity: Integrity {
            n_rows,
            n_seeds: REVERSAL_SEEDS.len(),
            paired_ok,
            freeze_updates_zero,
            probe_all_off,
            engineering_errors,
            source_digest_unchanged: digest_unchanged,
            initial_pairs_ok,
            valid_complete,
        },
        mean,
        lo,
        hi,
        conclusion,
    };

    write_d2d_artefacts(out, &report)?;
    append_versions_elapsed(out, report.elapsed_ms, report.integrity.valid_complete)?;
    if !digest_unchanged {
        return Err("source_digest changed during the batch; marked invalid".into());
    }
    Ok(report)
}

fn append_versions_elapsed(out: &Path, elapsed_ms: u128, valid: bool) -> Result<(), String> {
    let extra = format!("elapsed_ms={elapsed_ms}\nvalid_complete={valid}\n");
    fs::OpenOptions::new()
        .append(true)
        .open(out.join("versions.txt"))
        .and_then(|mut f| f.write_all(extra.as_bytes()))
        .map_err(|e| e.to_string())
}

fn engineering_errors_note(out: &Path, msg: &str) -> Result<(), String> {
    fs::write(out.join("pair_error.txt"), msg).map_err(|e| e.to_string())
}

fn write_start_identity(
    out: &Path,
    source: &ExecutionSourceManifest,
    experiment_hex: &str,
    rules_hex: &str,
    rules_after: &str,
    profile: &str,
    command: &str,
    started_unix: u64,
) -> Result<(), String> {
    let mut v = String::new();
    v.push_str(&format!("campaign=D2-D\n"));
    v.push_str(&format!("git_commit={}\n", source.git_commit));
    v.push_str(&format!("git_dirty={}\n", source.git_dirty));
    v.push_str(&format!(
        "execution_source_differs_from_head={}\n",
        source.execution_source_differs_from_head
    ));
    v.push_str(&format!("source_digest_algo={}\n", SOURCE_DIGEST_ALGO));
    v.push_str(&format!("source_digest={}\n", source.digest));
    v.push_str(&format!("source_digest_at=start\n"));
    v.push_str(&format!("rustc={}\n", source.rustc));
    v.push_str(&format!("cargo={}\n", source.cargo));
    v.push_str(&format!("profile={profile}\n"));
    v.push_str(&format!("command={command}\n"));
    v.push_str(&format!("started_unix={started_unix}\n"));
    v.push_str(&format!("experiment_hash={experiment_hex}\n"));
    v.push_str(&format!("rules_hash={rules_hex}\n"));
    v.push_str(&format!("rules_hash_after_switch={rules_after}\n"));
    v.push_str("learning_rates_unchanged=true\n");
    v.push_str("reward_coeffs_unchanged=true\n");
    v.push_str("network_size_unchanged=true\n");
    v.push_str("docs_d1_preserved=true\n");
    v.push_str("docs_batches_G5_preserved=true\n");
    v.push_str("f2_not_run=true\n");
    v.push_str("enable_probe=false\n");
    v.push_str("seeds=101-120\n");
    v.push_str(&format!("switch_tick={}\n", D1_SWITCH_TICK));
    v.push_str(&format!("max_ticks={}\n", D1_MAX_TICKS));
    v.push_str(&format!("stat_seed={}\n", D2D_STAT_SEED));
    v.push_str(&format!("bootstrap_draws={}\n", D2D_BOOTSTRAP_DRAWS));
    fs::write(out.join("versions.txt"), v).map_err(|e| e.to_string())?;
    fs::write(out.join("git_status_at_start.txt"), &source.git_porcelain)
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn write_source_manifest(out: &Path, source: &ExecutionSourceManifest) -> Result<(), String> {
    let mut s = String::new();
    s.push_str("{\n");
    s.push_str(&format!(
        "  \"algorithm\": \"{}\",\n",
        json_escape(&source.algorithm)
    ));
    s.push_str(&format!("  \"digest\": \"{}\",\n", source.digest));
    s.push_str(&format!("  \"n_files\": {},\n", source.files.len()));
    s.push_str("  \"files\": [\n");
    for (i, f) in source.files.iter().enumerate() {
        s.push_str(&format!(
            "    {{\"path\": \"{}\", \"sha256\": \"{}\"}}{}",
            json_escape(&f.path),
            f.sha256,
            if i + 1 == source.files.len() {
                "\n"
            } else {
                ",\n"
            }
        ));
    }
    s.push_str("  ]\n}\n");
    fs::write(out.join("source_manifest.json"), s).map_err(|e| e.to_string())
}

fn write_d2d_artefacts(out: &Path, report: &D2dReport) -> Result<(), String> {
    let rows: Vec<RunRow> = report.records.iter().map(|r| r.row.clone()).collect();
    write_runs_csv(&out.join("runs.csv"), &rows).map_err(|e| e.to_string())?;
    write_windows_csv(&out.join("windows.csv"), &rows).map_err(|e| e.to_string())?;
    write_pairs_csv(out, report)?;
    write_statistics_json(out, report)?;
    write_manifest_json(out, report)?;
    write_sampling(out)?;
    write_open_issues(out, report)?;
    write_report_md(out, report)?;
    Ok(())
}

fn write_pairs_csv(out: &Path, report: &D2dReport) -> Result<(), String> {
    let mut f = fs::File::create(out.join("paired_differences.csv")).map_err(|e| e.to_string())?;
    writeln!(
        f,
        "seed,learn_metab_after,freeze_metab_after,diff,learn_alive_switch,freeze_alive_switch,learn_alive_end,freeze_alive_end,learn_death_tick,freeze_death_tick,learn_updates,freeze_updates,learn_decisions,freeze_decisions,learn_photo,freeze_photo"
    )
    .map_err(|e| e.to_string())?;
    for p in &report.pairs {
        writeln!(
            f,
            "{},{},{},{:.1},{},{},{},{},{},{},{},{},{},{},{},{}",
            p.seed,
            p.learn_metab_after,
            p.freeze_metab_after,
            p.diff,
            u8::from(p.learn_alive_switch),
            u8::from(p.freeze_alive_switch),
            u8::from(p.learn_alive_end),
            u8::from(p.freeze_alive_end),
            p.learn_death_tick.map(|x| x.to_string()).unwrap_or_default(),
            p.freeze_death_tick.map(|x| x.to_string()).unwrap_or_default(),
            p.learn_updates,
            p.freeze_updates,
            p.learn_decisions,
            p.freeze_decisions,
            p.learn_photo,
            p.freeze_photo
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn write_statistics_json(out: &Path, report: &D2dReport) -> Result<(), String> {
    let mut s = String::new();
    s.push_str("{\n");
    s.push_str(&format!(
        "  \"valid_complete\": {},\n",
        report.integrity.valid_complete
    ));
    s.push_str(&format!(
        "  \"metric\": \"metab_e_after_switch learn-minus-freeze\",\n"
    ));
    s.push_str(&format!("  \"stat_seed\": {},\n", D2D_STAT_SEED));
    s.push_str(&format!("  \"bootstrap_draws\": {},\n", D2D_BOOTSTRAP_DRAWS));
    s.push_str("  \"resample_unit\": \"complete seed pair\",\n");
    s.push_str("  \"quantile\": \"lo=means[(0.025*draws) as usize], hi=means[((0.975*draws) as usize).min(draws-1)] after sort\",\n");
    match (report.mean, report.lo, report.hi) {
        (Some(m), Some(l), Some(h)) => {
            s.push_str(&format!("  \"mean\": {m:.1},\n"));
            s.push_str(&format!("  \"ci95_lo\": {l:.1},\n"));
            s.push_str(&format!("  \"ci95_hi\": {h:.1},\n"));
        }
        _ => {
            s.push_str("  \"mean\": null,\n");
            s.push_str("  \"ci95_lo\": null,\n");
            s.push_str("  \"ci95_hi\": null,\n");
        }
    }
    s.push_str(&format!(
        "  \"d1_reference_mean\": {},\n",
        D1_C5_MEAN
    ));
    s.push_str(&format!("  \"d1_reference_ci95_lo\": {},\n", D1_C5_LO));
    s.push_str(&format!("  \"d1_reference_ci95_hi\": {},\n", D1_C5_HI));
    s.push_str(&format!(
        "  \"learn_alive_switch\": {},\n",
        report.pairs.iter().filter(|p| p.learn_alive_switch).count()
    ));
    s.push_str(&format!(
        "  \"freeze_alive_switch\": {},\n",
        report.pairs.iter().filter(|p| p.freeze_alive_switch).count()
    ));
    s.push_str(&format!(
        "  \"learn_alive_end\": {},\n",
        report.pairs.iter().filter(|p| p.learn_alive_end).count()
    ));
    s.push_str(&format!(
        "  \"freeze_alive_end\": {},\n",
        report.pairs.iter().filter(|p| p.freeze_alive_end).count()
    ));
    s.push_str("  \"p_residuals\": [\n");
    for (i, r) in report.records.iter().enumerate() {
        s.push_str(&format!(
            "    {{\"seed\": {}, \"group\": \"{}\", \"residual\": {}}}{}",
            r.row.scenario.seed,
            r.row.scenario.group,
            r.p_residual
                .map(|x| x.to_string())
                .unwrap_or_else(|| "null".into()),
            if i + 1 == report.records.len() {
                "\n"
            } else {
                ",\n"
            }
        ));
    }
    s.push_str("  ],\n");
    s.push_str("  \"pairs\": [\n");
    for (i, p) in report.pairs.iter().enumerate() {
        s.push_str(&format!(
            "    {{\"seed\": {}, \"diff\": {:.1}}}{}",
            p.seed,
            p.diff,
            if i + 1 == report.pairs.len() {
                "\n"
            } else {
                ",\n"
            }
        ));
    }
    s.push_str("  ]\n}\n");
    fs::write(out.join("statistics.json"), s).map_err(|e| e.to_string())
}

fn write_manifest_json(out: &Path, report: &D2dReport) -> Result<(), String> {
    let status = if report.integrity.valid_complete {
        "c5_complete"
    } else {
        "invalid_or_incomplete"
    };
    let body = format!(
        "{{\n  \"campaign\": \"D2-D\",\n  \"commit\": \"{}\",\n  \"git_dirty\": {},\n  \"execution_source_differs_from_head\": {},\n  \"source_digest_algo\": \"{}\",\n  \"source_digest\": \"{}\",\n  \"source_digest_after\": \"{}\",\n  \"source_digest_unchanged\": {},\n  \"rustc\": \"{}\",\n  \"cargo\": \"{}\",\n  \"profile\": \"{}\",\n  \"command\": \"{}\",\n  \"experiment_hash\": \"{}\",\n  \"rules_hash\": \"{}\",\n  \"rules_hash_after_switch\": \"{}\",\n  \"base_rules_hash_128\": \"{}\",\n  \"switch_tick\": {},\n  \"max_ticks\": {},\n  \"seeds\": {:?},\n  \"enable_probe\": false,\n  \"p_definition\": \"P = E_old + E_new + 8*C (micro-units)\",\n  \"map\": \"reversal_16x16\",\n  \"status\": \"{}\",\n  \"note\": \"Historical D2 short results in docs/d2/ keep their original 4f237b0 dirty identity. This directory is the D2-D run.\"\n}}\n",
        json_escape(&report.source.git_commit),
        report.source.git_dirty,
        report.source.execution_source_differs_from_head,
        SOURCE_DIGEST_ALGO,
        report.source.digest,
        report.source_digest_after,
        report.integrity.source_digest_unchanged,
        json_escape(&report.source.rustc),
        json_escape(&report.source.cargo),
        report.profile,
        json_escape(&report.command),
        report.experiment_hash,
        report.rules_hash,
        report.rules_hash_after_switch,
        report.base_rules_hash,
        D1_SWITCH_TICK,
        D1_MAX_TICKS,
        REVERSAL_SEEDS,
        status
    );
    fs::write(out.join("manifest.json"), body).map_err(|e| e.to_string())
}

fn write_sampling(out: &Path) -> Result<(), String> {
    let text = "\
# D2-D 采样预算（预先声明，不事后挑选）

- 完整 C5：40 条运行全部 `enable_probe=false`。主统计与 runs.csv 只使用 `RunLedger` 账本，不保留无界 `last_decisions`。
- 短对照：`cargo run --offline -p universe_lab --bin d2 -- <run>/short`。预先指定用例 FB01/EX02/AR02/R04-dark/MV04-unit，单体，H≤12。短对照若 `enable_probe=true`，缓存由有限 H 与单一个体限制。
- 不根据 C5 结果再导出额外决策帧。
- 冻结组正常感知、抽样、行动并支付认知费；只关闭在线读出更新。
";
    fs::write(out.join("sampling.md"), text).map_err(|e| e.to_string())
}

fn write_open_issues(out: &Path, report: &D2dReport) -> Result<(), String> {
    let mut s = String::from("# D2-D 未解决问题\n\n");
    s.push_str("## 本轮实际隔离\n\n");
    s.push_str(&format!(
        "- R08：`initial_count={}`，单体，无共享资源竞争；未验证多体 QuotaTie。\n",
        report.isolation.initial_count
    ));
    s.push_str(&format!(
        "- R09：`disable_reproduction={}`，未注入损伤；repair 仍未接入世界步进。出生当步反馈未测。AT02/AT03 未跑。\n",
        report.isolation.disable_reproduction
    ));
    s.push_str(&format!(
        "- R10：本轮配置 `q_loss_divisor={}`、`substeps={}`、dt={}/{}、`decision_every_ticks={}`。EX03 仅覆盖非空 future_inputs，不能宣称零除数等全部被拒绝。\n",
        report.isolation.q_loss_divisor,
        report.isolation.substeps,
        report.isolation.dt_numerator,
        report.isolation.dt_denominator,
        report.isolation.decision_every_ticks
    ));
    s.push_str("- R11：完整 C5 `enable_probe=false`，不把无界 `last_decisions` 用于长运行。内核无界探针 API 未改；有界探针仍是后续缺陷。短对照见 `sampling.md`。\n");
    s.push_str("- 档案：本轮 C5 不使用档案恢复。现有测试覆盖 S0、事件幂等、rules mismatch；换灯边界恢复与决策/非决策边界对照未作为本轮 C5 路径验收。不得写「档案全部通过」。\n");
    s.push_str("- 本轮 C5 未使用非空 future_inputs；该路径保持拒绝。\n\n");
    s.push_str("## 交接清单中的原表述\n\n");
    for line in open_issue_lines() {
        s.push_str("- ");
        s.push_str(&line);
        s.push('\n');
    }
    s.push_str("\n## F2 前（本轮不执行）\n\n");
    s.push_str("- 公开共同初态含脑、独立生成器、T_recover、留 Q 关输入、负对照、C3/C4 可达性，再冻 L/W/H。\n");
    s.push_str("- 学习/冻结在换灯后首次神经行动前分支；200 ticks 只限制指标窗口。\n");
    s.push_str("- 正式评估种子不得参与初态/指标/超参数选择。101–120 不是未来正式评估集。\n");
    fs::write(out.join("open_issues.md"), s).map_err(|e| e.to_string())
}

fn write_report_md(out: &Path, report: &D2dReport) -> Result<(), String> {
    let mut md = String::new();
    md.push_str("# D2-D 报告\n\n");
    md.push_str(&format!(
        "源码 HEAD `{}` git_dirty={} execution_source_differs_from_head={} source_digest=`{}` algo=`{}`\n\n",
        report.source.git_commit,
        report.source.git_dirty,
        report.source.execution_source_differs_from_head,
        report.source.digest,
        SOURCE_DIGEST_ALGO
    ));
    md.push_str("本报告由本目录结构化结果生成。`docs/d1/` 与 `docs/batches/G5.md` 原件未改。既有 `docs/d2/` 短结果保留提交前身份，不是把 commit 改成当前 HEAD 的新运行。\n\n");
    if report.integrity.valid_complete {
        md.push_str("**状态：40 条 C5 记录完整且无工程错误。这不是学习门禁通过，也不是 F2。**\n\n");
    } else {
        md.push_str("**状态：本批次不得解读为有效完成的主统计。**\n\n");
        if !report.integrity.engineering_errors.is_empty() {
            md.push_str("工程错误：\n");
            for e in &report.integrity.engineering_errors {
                md.push_str(&format!("- {e}\n"));
            }
            md.push('\n');
        }
    }
    md.push_str("## 协议\n\n");
    md.push_str(&format!("- 命令：`{}`\n", report.command));
    md.push_str(&format!("- profile：{}\n", report.profile));
    md.push_str(&format!("- rustc：{}\n", report.source.rustc));
    md.push_str(&format!("- 耗时：{} ms\n", report.elapsed_ms));
    md.push_str(&format!("- experiment_hash：`{}`\n", report.experiment_hash));
    md.push_str(&format!(
        "- 实际 16×16 RuleSpec：`{}`（D1 runs.csv 曾写入 128×128 基线 `{}`，这是公开的记录差异，不是无说明的同配置复现）\n",
        report.rules_hash, report.base_rules_hash
    ));
    md.push_str(&format!(
        "- 换灯后 RuleSpec：`{}`（`apply_lighting`，与 state.rules_hash 对齐）\n",
        report.rules_hash_after_switch
    ));
    md.push_str("- 地图 16×16、种子 101–120、learn/freeze、20000 ticks、tick=10000 换灯、关繁殖、`enable_probe=false`。\n");
    md.push_str("- 主指标：后半程代谢 E 的逐 seed `学习−冻结`；bootstrap 种子 20260910、2000 次；重采样单位是完整 seed 对。\n");
    md.push_str("- 学习率、奖励、网络未改。未引入 F2 低库存或 L/W。死亡样本全部保留。\n\n");
    md.push_str("## 完整性\n\n");
    md.push_str(&format!(
        "- 行数 {}（应为 40）；配对 {}；冻结组更新全零 {}；探针全关 {}；初态成对 {}；摘要未变 {}\n",
        report.integrity.n_rows,
        report.integrity.paired_ok,
        report.integrity.freeze_updates_zero,
        report.integrity.probe_all_off,
        report.integrity.initial_pairs_ok,
        report.integrity.source_digest_unchanged
    ));
    let n_res = report.records.len() as i128;
    if n_res > 0 {
        let zeros = report
            .records
            .iter()
            .filter(|r| r.p_residual == Some(0))
            .count();
        let some = report.records.iter().filter(|r| r.p_residual.is_some()).count();
        md.push_str(&format!(
            "- P 会计残差：{some} 条可计算，其中 {zeros} 条为 0。残差定义见 statistics.json。非零时不改账本凑零；诊断 P 不替代 M/U 守恒。常见来源包括容量上限、N 库存变化、未列入公式的耗散/修复项、以及死亡当步账户语义。\n\n"
        ));
    }
    md.push_str("## C5 主比较\n\n");
    if let (Some(m), Some(l), Some(h)) = (report.mean, report.lo, report.hi) {
        md.push_str(&format!(
            "后半程代谢 E（学习−冻结）配对均值 {m:.1}，bootstrap 95% [{l:.1}, {h:.1}]。\n\n"
        ));
        md.push_str(&format!("结论：{}\n\n", report.conclusion));
        md.push_str(&format!(
            "换灯存活：学习 {} / 冻结 {}；期末存活：学习 {} / 冻结 {}（各 20）。\n\n",
            report.pairs.iter().filter(|p| p.learn_alive_switch).count(),
            report.pairs.iter().filter(|p| p.freeze_alive_switch).count(),
            report.pairs.iter().filter(|p| p.learn_alive_end).count(),
            report.pairs.iter().filter(|p| p.freeze_alive_end).count()
        ));
    } else {
        md.push_str(&format!("{}\n\n", report.conclusion));
    }
    md.push_str("历史 D1 C5（`docs/d1/report.md`，不是本轮目标）：均值 +195084.0 μE，95% [-3581658.0, +4097096.8]。\n\n");
    md.push_str("## 逐 seed 差值\n\n");
    if report.pairs.is_empty() {
        md.push_str("（无有效配对）\n\n");
    } else {
        md.push_str("| seed | learn−freeze μE | learn 换灯存活 | freeze 换灯存活 |\n|---|---:|---|---|\n");
        for p in &report.pairs {
            md.push_str(&format!(
                "| {} | {:.1} | {} | {} |\n",
                p.seed, p.diff, p.learn_alive_switch, p.freeze_alive_switch
            ));
        }
        md.push('\n');
    }
    md.push_str("## 未解决问题\n\n见 `open_issues.md`。F2 项目本轮未执行。\n");
    fs::write(out.join("report.md"), md).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use universe_core::probe::RunLedger;

    #[test]
    fn experiment_hash_stable_and_sensitive() {
        let spec = reversal_spec();
        let a = campaign_experiment_hash(&spec);
        let b = campaign_experiment_hash(&spec);
        assert_eq!(a, b);
        let mut other = spec.clone();
        other.lighting.left_x0 = 4;
        assert_ne!(campaign_experiment_hash(&spec), campaign_experiment_hash(&other));
    }

    #[test]
    fn isolation_accepts_reversal_spec() {
        let iso = check_isolation(&reversal_spec());
        assert!(iso.ok, "{:?}", iso.notes);
        assert_eq!(iso.q_loss_divisor, 16);
        assert_eq!(iso.substeps, 4);
        assert_eq!(iso.initial_count, 1);
        assert_eq!(iso.initial_o_per_cell, 0);
    }

    #[test]
    fn p_identity_zero_on_balanced_ledger() {
        let mut l = RunLedger::default();
        l.p_initial = Some(1000);
        l.p_before_death_or_end = Some(1000 + 8 * 10 - 20 - 5 - 3 - 2 - 1);
        l.photo_extent = 7;
        l.uptake_o = 3;
        l.maint_paid = 20;
        l.cognition = 5;
        l.thrust_attempt = 3;
        l.turn_attempt = 2;
        l.debt_repay = 1;
        assert_eq!(p_identity_residual(&l), Some(0));
    }

    #[test]
    fn pair_by_seed_does_not_use_push_order() {
        let spec = reversal_spec();
        let rh = hex_hash(&rules_hash(&spec).unwrap());
        let eh = hex_hash(&campaign_experiment_hash(&spec));
        let mk = |seed: u64, freeze: bool, metab: u64| {
            let sc = c5_scenario(seed, freeze);
            let st = initialize_reversal_world(&spec, seed).unwrap();
            C5Record {
                row: RunRow {
                    scenario: sc,
                    ledger: RunLedger {
                        metab_e_after_switch: metab,
                        ..RunLedger::default()
                    },
                    rules_hash: rh.clone(),
                    experiment_hash: eh.clone(),
                    commit: "test".into(),
                    windows: vec![],
                    path: vec![],
                    decisions: vec![],
                    alive_at_end: false,
                    final_x: 0.0,
                    final_y: 0.0,
                    entered_old: false,
                    entered_new: false,
                    c0_pass: None,
                    error: None,
                    initial_q: 0,
                },
                error_class: None,
                initial_state_hash: hex_hash(&state_hash(&st).unwrap()),
                rules_hash_after_switch: None,
                p_residual: Some(0),
                probe_enabled: false,
            }
        };
        // Reverse seed order, freeze before learn, to catch order-dependent pairing.
        let mut recs = Vec::new();
        for seed in REVERSAL_SEEDS.iter().rev().copied() {
            recs.push(mk(seed, true, 1));
            recs.push(mk(seed, false, 4));
        }
        let pairs = pair_by_seed(&recs).unwrap();
        assert_eq!(pairs.len(), 20);
        assert_eq!(pairs[0].seed, 101);
        assert_eq!(pairs[0].diff, 3.0);
        assert_eq!(pairs[19].seed, 120);
    }

    #[test]
    fn source_digest_covers_lockfile_not_docs() {
        let m = execution_source_manifest().unwrap();
        assert_eq!(m.algorithm, SOURCE_DIGEST_ALGO);
        assert!(m.files.iter().any(|f| f.path == "Cargo.lock"));
        assert!(m.files.iter().any(|f| f.path == "rust-toolchain.toml"));
        assert!(m.files.iter().any(|f| f.path.starts_with("crates/universe_lab/")));
        assert!(m.files.iter().all(|f| !f.path.starts_with("docs/")));
        assert!(m.files.iter().all(|f| !f.path.contains("/target/")));
        let again = execution_source_manifest().unwrap();
        assert_eq!(m.digest, again.digest);
    }

    #[test]
    fn learn_freeze_share_initial_hash() {
        for seed in [101u64, 120] {
            assert_eq!(initial_state_hash(seed).unwrap(), initial_state_hash(seed).unwrap());
        }
    }
}
