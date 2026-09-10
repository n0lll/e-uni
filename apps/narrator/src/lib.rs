//! Periodic narrator (ch. 31). Read-only; never writes life state.

use universe_archive::Archive;
use universe_types::{hex_hash, rules_hash, Hash32, LifeEventKind, PresentationSpec, RuleSpec};

#[derive(Clone, Debug)]
pub struct UniverseCard {
    pub rules_hash: String,
    pub glossary_version: String,
    pub time_units: String,
    pub space_and_boundaries: String,
    pub resource_types_and_conversions: Vec<String>,
    pub organism_capabilities: Vec<String>,
    pub perception_limits: Vec<String>,
    pub neural_learning_and_memory_rules: String,
    pub reproduction_and_inheritance_rules: String,
    pub death_and_recycling_rules: String,
    pub explicitly_absent_mechanisms: Vec<String>,
}

pub fn build_universe_card(spec: &RuleSpec) -> UniverseCard {
    let rh = hex_hash(&rules_hash(spec).unwrap_or([0; 32]));
    UniverseCard {
        rules_hash: rh,
        glossary_version: "eu-core-1-card-v1".into(),
        time_units: "tick 为权威时间；dt=1/10 模拟秒；决策每 5 ticks".into(),
        space_and_boundaries: format!(
            "{}×{} LU 闭合墙，单圆盘半径 {} LU",
            spec.world.grid_width, spec.world.grid_height, spec.physics.radius_lu
        ),
        resource_types_and_conversions: vec![
            "N 为抽象无机营养，不是氮气".into(),
            "O 为有机碎屑账户".into(),
            "C 为体内有机物；Q 为辐射能抽象，不是太阳".into(),
            "光合：N+Q→C+耗散；代谢：C→N+E_new+耗散".into(),
        ],
        organism_capabilities: vec![
            "推进、转向、N/O 摄取、光合增强、代谢增强、修复、无性繁殖".into(),
        ],
        perception_limits: vec!["64 维：32 数值 + 32 有效位；距离射线 4 LU".into()],
        neural_learning_and_memory_rules:
            "循环特征层私有；在线 actor–critic 读出；对象关联记忆未启用；学习痕迹不遗传".into(),
        reproduction_and_inheritance_rules:
            "无性胚体：启动费、逐步构造、空间出生竞争；突变作用于遗传编码 q".into(),
        death_and_recycling_rules:
            "5*B < B_target 或债务≥200000；N→格 N，C+B→格 O，E→耗散；ID 不复用".into(),
        explicitly_absent_mechanisms: vec![
            "没有语言或文本思维".into(),
            "没有货币".into(),
            "没有氧气化学或真实光合作用".into(),
            "未启用转赠".into(),
            "未启用攻击".into(),
            "未启用对象关联记忆".into(),
            "未启用有性繁殖或求偶".into(),
            "未启用形态突变 / EU-Body-1".into(),
            "叙述者文字不是宇宙事实".into(),
        ],
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingRange {
    pub a: u64,
    pub b: u64,
}

pub fn plan_windows(
    narrated_tick: u64,
    durable_tick: u64,
    window: u64,
    include_partial: bool,
) -> Vec<PendingRange> {
    let mut out = Vec::new();
    if window == 0 || durable_tick <= narrated_tick {
        return out;
    }
    let mut a = narrated_tick;
    while a + window <= durable_tick {
        out.push(PendingRange {
            a,
            b: a + window,
        });
        a += window;
        // merge unfrozen contiguous: planner returns the next contiguous range only
        break;
    }
    if include_partial && a < durable_tick && out.is_empty() {
        out.push(PendingRange {
            a,
            b: durable_tick,
        });
    }
    out
}

#[derive(Clone, Debug)]
pub struct WindowFacts {
    pub a: u64,
    pub b: u64,
    pub births: u64,
    pub deaths: u64,
    pub cancels: u64,
    pub lineages: u64,
    pub event_ids: Vec<String>,
    pub missing: Vec<String>,
    pub content_hash: Hash32,
}

pub fn build_window_facts(archive: &Archive, a: u64, b: u64) -> Result<WindowFacts, String> {
    if b > archive.progress().map_err(|e| e.to_string())?.durable_tick {
        return Err("b exceeds durable_tick".into());
    }
    let events = archive.events_in_window(a, b).map_err(|e| e.to_string())?;
    let mut births = 0u64;
    let mut deaths = 0u64;
    let mut cancels = 0u64;
    let mut lineages = 0u64;
    let mut event_ids = Vec::new();
    for (i, e) in events.iter().enumerate() {
        event_ids.push(format!("e{i}:{}:{}", e.tick, e.kind.code()));
        match e.kind {
            LifeEventKind::Birth => births += 1,
            LifeEventKind::Death => deaths += 1,
            LifeEventKind::EmbryoCancel => cancels += 1,
            LifeEventKind::Lineage => lineages += 1,
            LifeEventKind::EmbryoStart => {}
        }
    }
    let mut payload = format!("{a}:{b}:{births}:{deaths}:{cancels}:{lineages}");
    for id in &event_ids {
        payload.push_str(id);
    }
    let content_hash = universe_types::sha256(payload.as_bytes());
    let mut missing = Vec::new();
    if events.is_empty() {
        missing.push("本期无可展开生命事件；仅有场与生理过程".into());
    }
    Ok(WindowFacts {
        a,
        b,
        births,
        deaths,
        cancels,
        lineages,
        event_ids,
        missing,
        content_hash,
    })
}

#[derive(Clone, Debug)]
pub struct NarrativeDraft {
    pub body: String,
    pub mode: String,
    pub claims: Vec<String>,
}

pub fn render_template(card: &UniverseCard, facts: &WindowFacts) -> NarrativeDraft {
    let mut body = String::new();
    body.push_str(&format!(
        "窗口 [{}, {})（{} ticks）。规则哈希 {}。\n",
        facts.a, facts.b, facts.b.saturating_sub(facts.a), card.rules_hash
    ));
    body.push_str(&format!(
        "本期记录：出生 {}，死亡 {}，胚体取消 {}，谱系事件 {}。\n",
        facts.births, facts.deaths, facts.cancels, facts.lineages
    ));
    body.push_str("B 版明确缺失：");
    body.push_str(&card.explicitly_absent_mechanisms.join("；"));
    body.push('\n');
    if facts.births == 0 && facts.deaths == 0 {
        body.push_str("本期人口与资源指标若无达到报告阈值的新过程，则仅作简述；不虚构紧张局势。\n");
    }
    for m in &facts.missing {
        body.push_str("缺证：");
        body.push_str(m);
        body.push('\n');
    }
    let mut claims = vec![
        format!("births={}", facts.births),
        format!("deaths={}", facts.deaths),
    ];
    claims.extend(facts.event_ids.iter().cloned());
    NarrativeDraft {
        body,
        mode: "deterministic_templates".into(),
        claims,
    }
}

/// Null backend: always fail so caller uses the template.
pub fn generate_draft(
    _ctx_hash: Hash32,
    backend_profile: Option<&str>,
) -> Result<NarrativeDraft, String> {
    match backend_profile {
        None => Err("backend_profile=null; use template".into()),
        Some(_) => Err("model timeout or unavailable".into()),
    }
}

pub fn validate_draft(draft: &NarrativeDraft, facts: &WindowFacts) -> Result<(), String> {
    let b = format!("births={}", facts.births);
    let d = format!("deaths={}", facts.deaths);
    if !draft.claims.iter().any(|c| c == &b) {
        return Err("missing births claim".into());
    }
    if !draft.claims.iter().any(|c| c == &d) {
        return Err("missing deaths claim".into());
    }
    if draft.body.contains("转赠成功") || draft.body.contains("有性繁殖已发生") {
        return Err("claims absent mechanism".into());
    }
    Ok(())
}

pub fn publish_once(
    archive: &Archive,
    facts: &WindowFacts,
    draft: &NarrativeDraft,
    predecessor: Option<i64>,
) -> Result<i64, String> {
    archive
        .insert_narrative(
            facts.a,
            facts.b,
            predecessor,
            &draft.body,
            facts.content_hash,
            &draft.mode,
        )
        .map_err(|e| e.to_string())?;
    archive
        .set_narrated_tick(facts.b)
        .map_err(|e| e.to_string())?;
    Ok(archive
        .latest_narrative_body()
        .ok()
        .and_then(|_| Some(1))
        .unwrap_or(1))
}

pub fn run_window(
    archive: &Archive,
    spec: &RuleSpec,
    present: &PresentationSpec,
    a: u64,
    b: u64,
) -> Result<String, String> {
    let prog = archive.progress().map_err(|e| e.to_string())?;
    if b > prog.durable_tick {
        return Err("b > durable_tick".into());
    }
    let card = build_universe_card(spec);
    let facts = build_window_facts(archive, a, b)?;
    let template = render_template(&card, &facts);
    let draft = match generate_draft(facts.content_hash, present.backend_profile.as_deref()) {
        Ok(d) => match validate_draft(&d, &facts) {
            Ok(()) => d,
            Err(_) => template,
        },
        Err(_) => template,
    };
    validate_draft(&draft, &facts)?;
    let _ = publish_once(archive, &facts, &draft, None)?;
    Ok(draft.body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use universe_archive::Archive;
    use universe_types::RuleSpec;

    #[test]
    fn card_lists_absent_mechanisms() {
        let spec = RuleSpec::eu_core_1();
        let card = build_universe_card(&spec);
        assert!(card
            .explicitly_absent_mechanisms
            .iter()
            .any(|s| s.contains("转赠")));
        assert!(card
            .explicitly_absent_mechanisms
            .iter()
            .any(|s| s.contains("有性")));
    }

    #[test]
    fn windows_require_durable() {
        let w = plan_windows(0, 2500, 3000, true);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].b, 2500);
        let w = plan_windows(0, 6000, 3000, false);
        assert_eq!(w[0], PendingRange { a: 0, b: 3000 });
    }

    #[test]
    fn template_and_publish_do_not_need_model() {
        let spec = RuleSpec::eu_core_1();
        let dir = std::env::temp_dir().join(format!(
            "eu-nar-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let arch = Archive::open(&dir, &spec).unwrap();
        arch.set_durable_tick(3000).unwrap();
        let present = PresentationSpec::default();
        let body = run_window(&arch, &spec, &present, 0, 3000).unwrap();
        assert!(body.contains("出生"));
        assert!(body.contains("未启用转赠"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
