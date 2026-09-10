//! D2 diagnostic campaign: structured results, no hardcoded pass prose.

use std::fs;
use std::io::Write;
use std::path::Path;

use universe_core::init::{initialize_reversal_world, retarget_audit_baseline};
use universe_core::probe::{aggregate_decisions, ControlMode, DecisionAggregate, DecisionDiag};
use universe_core::world::World;
use universe_core::{compute_mu, scale_control_parts};
use universe_types::{hex_hash, rules_hash};

use crate::experiment::{
    close_light_keep_q, light_right_schedule, source_identity, ExperimentDef, RuntimePack,
    SourceIdentity,
};
use crate::reversal_spec;

#[derive(Clone, Debug)]
pub struct CaseResult {
    pub id: String,
    pub ok: bool,
    pub detail: String,
    pub experiment_hash: String,
    pub rules_hash: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct D2Report {
    pub source: SourceIdentity,
    pub cases: Vec<CaseResult>,
    pub aggregates: Vec<(String, DecisionAggregate)>,
    pub sample_frames: Vec<DecisionDiag>,
    pub open_issues: Vec<String>,
}

impl D2Report {
    pub fn has_experiment_error(&self) -> bool {
        self.cases.iter().any(|c| c.error.is_some() || !c.ok)
    }
}

fn def_base(id: &str, seed: u64, control: ControlMode, freeze: bool) -> ExperimentDef {
    ExperimentDef {
        name: id.into(),
        version: 2,
        spec: reversal_spec(),
        controller: control,
        freeze_learning: freeze,
        disable_reproduction: true,
        disable_active_motion: false,
        start_x: None,
        start_y: None,
        heading: None,
        dark_quench_q: false,
        dark_close_input: false,
        switch_tick: None,
        max_ticks: 12,
        seed,
        metric_name: "d2_short".into(),
        stat_seed: 20260910,
    }
}

fn world_from_def(def: &ExperimentDef) -> Result<World, String> {
    let mut st = initialize_reversal_world(&def.spec, def.seed).map_err(|e| e.to_string())?;
    if let Some(x) = def.start_x {
        st.organisms[0].geom.x = x;
    }
    if let Some(y) = def.start_y {
        st.organisms[0].geom.y = y;
    }
    if let Some(h) = def.heading {
        st.organisms[0].geom.theta = h;
    }
    if def.dark_quench_q {
        for q in &mut st.fields.q {
            *q = 0;
        }
        for r in &mut st.fields.q_remainder {
            *r = 0;
        }
        retarget_audit_baseline(&mut st, &def.spec).map_err(|e| e.to_string())?;
    }
    let mut w = World::from_state(def.spec.clone(), st);
    w.apply_runtime(
        def.freeze_learning,
        def.disable_active_motion,
        def.disable_reproduction,
        def.controller,
    );
    w.enable_probe = true;
    if def.dark_close_input {
        w.apply_lighting(close_light_keep_q(&w.spec))
            .map_err(|e| e.to_string())?;
    }
    Ok(w)
}

fn run_steps(w: &mut World, n: u64) -> Result<(), String> {
    for _ in 0..n {
        w.step().map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub fn run_d2_cases() -> D2Report {
    let source = source_identity();
    let mut cases = Vec::new();
    let mut aggregates = Vec::new();
    let mut sample_frames = Vec::new();

    // FB01 short
    {
        let mut def = def_base("FB01", 11, ControlMode::Stationary, true);
        def.start_x = Some(4.5);
        def.start_y = Some(8.5);
        def.heading = Some(0.0);
        let eh = hex_hash(&def.experiment_hash());
        let rh = hex_hash(&def.rules_hash());
        let mut w = match world_from_def(&def) {
            Ok(w) => w,
            Err(e) => {
                cases.push(CaseResult {
                    id: "FB01".into(),
                    ok: false,
                    detail: String::new(),
                    experiment_hash: eh,
                    rules_hash: rh,
                    error: Some(e),
                });
                return finish(source, cases, aggregates, sample_frames);
            }
        };
        match run_steps(&mut w, 6) {
            Ok(()) => {
                let d5 = w.last_decisions.iter().find(|d| d.tick == 5);
                let ok = d5
                    .map(|d| d.feedback_ticks == 5 && d.obs[32 + 31] > 0.5 && d.obs[31] > 0.0)
                    .unwrap_or(false);
                cases.push(CaseResult {
                    id: "FB01".into(),
                    ok,
                    detail: format!("decisions={} ", w.last_decisions.len()),
                    experiment_hash: eh,
                    rules_hash: rh,
                    error: None,
                });
                aggregates.push(("FB01".into(), aggregate_decisions(&w.last_decisions)));
                sample_frames = w.last_decisions.clone();
            }
            Err(e) => cases.push(CaseResult {
                id: "FB01".into(),
                ok: false,
                detail: String::new(),
                experiment_hash: eh,
                rules_hash: rh,
                error: Some(e),
            }),
        }
    }

    // EX02: hash sensitivity
    {
        let a = def_base("EX02a", 11, ControlMode::Neural, false);
        let mut b = a.clone();
        b.seed = 12;
        let mut c = a.clone();
        c.controller = ControlMode::Stationary;
        let ha = hex_hash(&a.experiment_hash());
        let hb = hex_hash(&b.experiment_hash());
        let hc = hex_hash(&c.experiment_hash());
        let ok = ha != hb && ha != hc && hb != hc;
        cases.push(CaseResult {
            id: "EX02".into(),
            ok,
            detail: format!("a={ha} b={hb} c={hc}"),
            experiment_hash: ha,
            rules_hash: hex_hash(&a.rules_hash()),
            error: None,
        });
    }

    // runtime pack restores flags
    {
        let def = def_base("AR02pack", 7, ControlMode::OracleGotoLight, true);
        let mut w = world_from_def(&def).expect("world");
        let _ = run_steps(&mut w, 4);
        w.apply_lighting(light_right_schedule(&w.spec)).ok();
        let pack = RuntimePack::from_world(&w);
        let h0 = pack.content_hash();
        let mut w2 = pack.into_world();
        w2.enable_probe = true;
        let ok = w2.freeze_learning
            && w2.control_mode == ControlMode::OracleGotoLight
            && w2.disable_reproduction
            && w2.state.rules_hash == rules_hash(&w2.spec).unwrap();
        cases.push(CaseResult {
            id: "AR02".into(),
            ok,
            detail: format!("pack={}", hex_hash(&h0)),
            experiment_hash: hex_hash(&def.experiment_hash()),
            rules_hash: hex_hash(&def.rules_hash()),
            error: None,
        });
    }

    // dark keep-Q vs quench: public new initial U0
    {
        let spec = reversal_spec();
        let keep = initialize_reversal_world(&spec, 7).unwrap();
        let u_keep0 = keep.audit.initial_u;
        let q_keep = keep.fields.q.iter().copied().sum::<u64>();
        let mut wkeep = World::from_state(spec.clone(), keep);
        wkeep.apply_runtime(true, false, true, ControlMode::Stationary);
        wkeep.apply_lighting(close_light_keep_q(&wkeep.spec)).unwrap();
        let mut quench = initialize_reversal_world(&spec, 7).unwrap();
        for q in &mut quench.fields.q {
            *q = 0;
        }
        for r in &mut quench.fields.q_remainder {
            *r = 0;
        }
        retarget_audit_baseline(&mut quench, &spec).unwrap();
        let mu = compute_mu(&quench, spec.reactions.chem_e_per_organic).unwrap();
        let ok = q_keep > 0 && quench.audit.initial_u == mu.u && quench.audit.initial_u != u_keep0;
        cases.push(CaseResult {
            id: "R04-dark".into(),
            ok,
            detail: format!("keep_q={q_keep} quench_u0={} keep_u0={u_keep0}", quench.audit.initial_u),
            experiment_hash: hex_hash(&def_base("R04-dark", 7, ControlMode::Stationary, true).experiment_hash()),
            rules_hash: hex_hash(&rules_hash(&spec).unwrap()),
            error: None,
        });
    }

    // P3 still scales
    {
        let spec = reversal_spec();
        let mut org = initialize_reversal_world(&spec, 1).unwrap().organisms.remove(0);
        org.accounts.e_old = 600;
        org.accounts.e_new = 40_000;
        org.control.thrust = 1;
        org.control.turn = 1;
        let pay = scale_control_parts(&mut org, &spec).unwrap();
        let ok = pay.total() == 600 && (org.control.u_effective - 0.5).abs() < 1e-12;
        cases.push(CaseResult {
            id: "MV04-unit".into(),
            ok,
            detail: format!("paid={}", pay.total()),
            experiment_hash: String::new(),
            rules_hash: hex_hash(&rules_hash(&spec).unwrap()),
            error: None,
        });
    }

    finish(source, cases, aggregates, sample_frames)
}

fn finish(
    source: SourceIdentity,
    cases: Vec<CaseResult>,
    aggregates: Vec<(String, DecisionAggregate)>,
    sample_frames: Vec<DecisionDiag>,
) -> D2Report {
    D2Report {
        source,
        cases,
        aggregates,
        sample_frames,
        open_issues: open_issue_lines(),
    }
}

pub fn open_issue_lines() -> Vec<String> {
    vec![
        "R08 P2: 共享配额平局仍固定 priority=0，未接入 QuotaTie；单体 F2 无竞争，可隔离。".into(),
        "R09 P2: repair 反应未接入世界步进，属 B 版缺失而非 E/S。F2 默认满结构、无损伤、无出生，可隔离；完整 B 版须补世界级 repair。出生当步仍累计新生体反馈，繁殖验收前处理。".into(),
        "R10 P2: validate 未拒绝 q_loss_divisor=0 / substeps=0 等；部分入哈希参数仍有硬编码路径。参数扫描前修复。".into(),
        "R11 P2: enable_probe 时 last_decisions 无界且每步深拷贝。长时间探针运行前须加采样预算。".into(),
        "AT02/AT03: 胚体构造与出生 ID 分配故障注入列入繁殖验收批次；本轮单体关繁殖 F2 延期。".into(),
        "AR03 档案事件事务已补幂等主键；长期档案产品其余能力不以通过论。".into(),
    ]
}

pub fn write_d2_artefacts(out_dir: impl AsRef<Path>, report: &D2Report) -> Result<(), String> {
    let out = out_dir.as_ref();
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    write_manifest(out, report)?;
    write_cases_csv(out, report)?;
    write_aggregates_csv(out, report)?;
    write_frames_csv(out, report)?;
    write_open_issues(out, report)?;
    write_report_md(out, report)?;
    Ok(())
}

fn write_manifest(out: &Path, report: &D2Report) -> Result<(), String> {
    let failed = report.has_experiment_error();
    let body = format!(
        "{{\n  \"campaign\": \"D2\",\n  \"commit\": \"{}\",\n  \"dirty\": {},\n  \"source_digest\": \"{}\",\n  \"rustc\": \"{}\",\n  \"base_rules_hash\": \"{}\",\n  \"status\": \"{}\",\n  \"note\": \"docs/d1 and docs/batches/G5.md are historical; this directory is D2 only.\"\n}}\n",
        report.source.commit,
        report.source.dirty,
        report.source.source_digest,
        report.source.rustc.replace('"', "'"),
        hex_hash(&ExperimentDef::base_rules_hash()),
        if failed { "experiment_or_case_failed" } else { "cases_recorded" }
    );
    fs::write(out.join("manifest.json"), body).map_err(|e| e.to_string())
}

fn write_cases_csv(out: &Path, report: &D2Report) -> Result<(), String> {
    let mut f = fs::File::create(out.join("cases.csv")).map_err(|e| e.to_string())?;
    writeln!(f, "id,ok,error,rules_hash,experiment_hash,detail").map_err(|e| e.to_string())?;
    for c in &report.cases {
        writeln!(
            f,
            "{},{},{},{},{},{}",
            c.id,
            u8::from(c.ok),
            csv_escape(c.error.as_deref().unwrap_or("")),
            c.rules_hash,
            c.experiment_hash,
            csv_escape(&c.detail)
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn write_aggregates_csv(out: &Path, report: &D2Report) -> Result<(), String> {
    let mut f = fs::File::create(out.join("decision_aggregates.csv")).map_err(|e| e.to_string())?;
    writeln!(
        f,
        "case,n,n_updates,n_clipped,mean_delta,mean_entropy,mean_interval_d,mean_u_effective,n_obs31_valid,mean_obs31"
    )
    .map_err(|e| e.to_string())?;
    for (name, a) in &report.aggregates {
        writeln!(
            f,
            "{},{},{},{},{:.8},{:.8},{:.8},{:.8},{},{:.8}",
            name,
            a.n,
            a.n_updates,
            a.n_clipped,
            a.mean_delta,
            a.mean_entropy,
            a.mean_interval_d,
            a.mean_u_effective,
            a.n_obs31_valid,
            a.mean_obs31
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn write_frames_csv(out: &Path, report: &D2Report) -> Result<(), String> {
    let mut f = fs::File::create(out.join("frames.csv")).map_err(|e| e.to_string())?;
    writeln!(
        f,
        "entity_id,decision_tick,observation_state_tick,feedback_start_tick,feedback_end_tick,feedback_ticks,interval_d,R,u_effective,w_effective,paid_motion_energy,actual_forward_displacement,actual_unwrapped_turn,intent_u,intent_w,p_thr0,p_thr1,p_thr2,p_turn0,p_turn1,p_turn2,obs31,obs31_valid,phi0,phi1"
    )
    .map_err(|e| e.to_string())?;
    for d in &report.sample_frames {
        writeln!(
            f,
            "{},{},{},{},{},{},{},{:.8},{:.8},{:.8},{},{:.8},{:.8},{},{},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{},{:.6},{:.6}",
            d.entity_id,
            d.tick,
            d.observation_state_tick,
            d.feedback_start_tick,
            d.feedback_end_tick,
            d.feedback_ticks,
            d.interval_d,
            d.r,
            d.u_effective,
            d.w_effective,
            d.paid_motion_energy,
            d.actual_forward_displacement,
            d.actual_unwrapped_turn,
            d.intent_thrust,
            d.intent_turn,
            d.p_thr[0],
            d.p_thr[1],
            d.p_thr[2],
            d.p_turn[0],
            d.p_turn[1],
            d.p_turn[2],
            d.obs[31],
            u8::from(d.obs[32 + 31] > 0.5),
            d.phi[0],
            d.phi[1]
        )
        .map_err(|e| e.to_string())?;
    }
    let mut full = fs::File::create(out.join("frames_obs64.csv")).map_err(|e| e.to_string())?;
    write!(full, "decision_tick").map_err(|e| e.to_string())?;
    for i in 0..64 {
        write!(full, ",obs{i}").map_err(|e| e.to_string())?;
    }
    writeln!(full).map_err(|e| e.to_string())?;
    for d in &report.sample_frames {
        write!(full, "{}", d.tick).map_err(|e| e.to_string())?;
        for v in d.obs {
            write!(full, ",{v:.8}").map_err(|e| e.to_string())?;
        }
        writeln!(full).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn write_open_issues(out: &Path, report: &D2Report) -> Result<(), String> {
    let mut s = String::from("# D2 未解决问题\n\n");
    for line in &report.open_issues {
        s.push_str("- ");
        s.push_str(line);
        s.push('\n');
    }
    fs::write(out.join("open_issues.md"), s).map_err(|e| e.to_string())
}

fn write_report_md(out: &Path, report: &D2Report) -> Result<(), String> {
    let failed = report.has_experiment_error();
    let mut md = String::new();
    md.push_str("# D2 修复报告\n\n");
    md.push_str(&format!(
        "源码提交 `{}` dirty={} source_digest=`{}`\n\n",
        report.source.commit, report.source.dirty, report.source.source_digest
    ));
    md.push_str("本报告由 `docs/d2/` 结构化结果生成。`docs/d1/` 与 `docs/batches/G5.md` 保持原件。\n\n");
    if failed {
        md.push_str("**状态：存在失败或实验错误，不得解读为普通通过。**\n\n");
    } else {
        md.push_str("**状态：所列短对照均记录为 ok；这不是学习门禁通过，也不是 F2 正式跑数。**\n\n");
    }
    md.push_str("## 对照\n\n");
    for c in &report.cases {
        md.push_str(&format!(
            "- {}：ok={} error={} {}\n",
            c.id,
            c.ok,
            c.error.as_deref().unwrap_or("—"),
            c.detail
        ));
    }
    md.push_str("\n## 实际规则哈希\n\n");
    md.push_str(&format!(
        "- 128×128 基础 RuleSpec：`{}`\n",
        hex_hash(&ExperimentDef::base_rules_hash())
    ));
    if let Some(c) = report.cases.iter().find(|c| c.id == "FB01") {
        md.push_str(&format!("- 本轮 16×16 实验 RuleSpec（FB01）：`{}`\n", c.rules_hash));
    }
    md.push_str("\n学习率、奖励系数与网络规模未改。未推进 E/S/配对。\n\n");
    md.push_str("F2 的 200-tick 等待只限制指标窗口，不推迟学习/冻结分支时刻。正式种子冻结前不跑正式 F2。\n\n");
    md.push_str("## 未解决问题\n\n");
    for line in &report.open_issues {
        md.push_str("- ");
        md.push_str(line);
        md.push('\n');
    }
    fs::write(out.join("report.md"), md).map_err(|e| e.to_string())
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn run_d2_campaign(out_dir: impl AsRef<Path>) -> Result<(), String> {
    let report = run_d2_cases();
    write_d2_artefacts(out_dir, &report)?;
    if report.has_experiment_error() {
        return Err("D2 cases recorded failures or experiment errors".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn experiment_hash_changes_with_seed_and_controller() {
        let a = def_base("t", 1, ControlMode::Neural, false);
        let mut b = a.clone();
        b.seed = 2;
        let mut c = a.clone();
        c.controller = ControlMode::ReflexQ;
        assert_ne!(a.experiment_hash(), b.experiment_hash());
        assert_ne!(a.experiment_hash(), c.experiment_hash());
    }

    #[test]
    fn report_refuses_pass_on_error() {
        let mut r = run_d2_cases();
        r.cases.push(CaseResult {
            id: "EX01".into(),
            ok: false,
            detail: String::new(),
            experiment_hash: String::new(),
            rules_hash: String::new(),
            error: Some("injected".into()),
        });
        assert!(r.has_experiment_error());
    }

    #[test]
    fn d2_short_cases_ok() {
        let r = run_d2_cases();
        for c in &r.cases {
            assert!(c.error.is_none(), "{} {:?}", c.id, c.error);
            assert!(c.ok, "{} {}", c.id, c.detail);
        }
        assert!(r.sample_frames.iter().any(|d| d.tick == 5 && d.obs[31] > 0.0));
    }

    #[test]
    fn s0_restore_replays_without_periodic_snapshot() {
        use universe_archive::Archive;
        use universe_core::init::initialize_reversal_world;
        use universe_types::state_hash;
        let spec = reversal_spec();
        let dir = std::env::temp_dir().join(format!(
            "eu-d2-s0-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut arch = Archive::open(&dir, &spec).unwrap();
        let st = initialize_reversal_world(&spec, 3).unwrap();
        arch.record_initial(&st).unwrap();
        let mut w = World::from_state(spec.clone(), st);
        w.disable_reproduction = true;
        w.freeze_learning = true;
        w.control_mode = ControlMode::Stationary;
        for _ in 0..7 {
            w.step().unwrap();
            arch.record_committed(&w.state, &w.last_events).unwrap();
        }
        let target = w.state.tick;
        let expect = state_hash(&w.state).unwrap();
        let (snap_tick, path, h) = arch.latest_snapshot_tick(target).unwrap().unwrap();
        assert_eq!(snap_tick, 0);
        let loaded = Archive::load_snapshot_file(&path, Some(h)).unwrap();
        let mut w2 = World::from_state(spec, loaded);
        w2.disable_reproduction = true;
        w2.freeze_learning = true;
        w2.control_mode = ControlMode::Stationary;
        while w2.state.tick < target {
            w2.step().unwrap();
        }
        assert_eq!(state_hash(&w2.state).unwrap(), expect);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
