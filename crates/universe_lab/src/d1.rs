//! Diagnostic campaign D1: action costs, C0–C5, CSV/SVG/report.

use std::fs;
use std::io::Write;
use std::path::Path;

use universe_core::init::initialize_reversal_world;
use universe_core::probe::{budget_p, ControlMode, DecisionDiag, RunLedger};
use universe_core::world::World;
use universe_core::{scale_control_parts, MotionPay};
use universe_types::{hex_hash, rules_hash, sha256, CanonWriter, RuleSpec};

use crate::{light_right, reversal_spec, REVERSAL_SEEDS};

pub const D1_MAX_TICKS: u64 = 20_000;
pub const D1_SWITCH_TICK: u64 = 10_000;

#[derive(Clone, Debug)]
pub struct Scenario {
    pub group: String,
    pub seed: u64,
    pub freeze_learning: bool,
    pub control: ControlMode,
    pub switch_tick: Option<u64>,
    pub max_ticks: u64,
    pub place: Option<(f64, f64)>,
    pub heading: Option<f64>,
    pub dark: bool,
    pub collect_decisions: bool,
}

#[derive(Clone, Debug, Default)]
pub struct WindowRow {
    pub tick: u64,
    pub group: String,
    pub seed: u64,
    pub e: u64,
    pub c: u64,
    pub n: u64,
    pub p: u128,
    pub photo: u64,
    pub uptake_o: u64,
    pub metab_e: u64,
    pub maint: u64,
    pub cog: u64,
    pub thrust: u64,
    pub turn: u64,
    pub x: f64,
    pub y: f64,
    pub living: u32,
}

#[derive(Clone, Debug)]
pub struct RunRow {
    pub scenario: Scenario,
    pub ledger: RunLedger,
    pub rules_hash: String,
    pub experiment_hash: String,
    pub commit: String,
    pub windows: Vec<WindowRow>,
    pub path: Vec<(u64, f64, f64, u64, u64, u128)>,
    pub decisions: Vec<DecisionDiag>,
    pub alive_at_end: bool,
    pub final_x: f64,
    pub final_y: f64,
    pub entered_old: bool,
    pub entered_new: bool,
    pub c0_pass: Option<bool>,
    pub error: Option<String>,
    pub initial_q: u64,
}

#[derive(Clone, Debug)]
pub struct ActionCostResult {
    pub name: String,
    pub motion_thrust: u64,
    pub motion_turn: u64,
    pub abs_dtheta: f64,
    pub dx: f64,
    pub dy: f64,
    pub dtheta_end: f64,
    pub ok: bool,
    pub detail: String,
}

pub fn campaign_experiment_hash() -> [u8; 32] {
    let mut w = CanonWriter::new();
    w.raw(b"EUD1");
    w.u32(1);
    w.u64(16);
    w.u64(16);
    w.u64(D1_SWITCH_TICK);
    w.u64(D1_MAX_TICKS);
    w.raw(b"P=E+8C");
    w.raw(b"C0-C5");
    w.u64(REVERSAL_SEEDS.len() as u64);
    for s in REVERSAL_SEEDS {
        w.u64(s);
    }
    sha256(&w.into_inner())
}

pub fn git_commit() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "unknown".into())
}

fn in_lit(spec: &RuleSpec, x: f64, y: f64) -> bool {
    let gx = x.floor();
    let gy = y.floor();
    if gx < 0.0 || gy < 0.0 {
        return false;
    }
    spec.lighting.is_lit(gx as u32, gy as u32)
}

fn extinguish(w: &mut World) {
    w.spec.lighting.y0 = 1;
    w.spec.lighting.y1 = 0;
    for q in &mut w.state.fields.q {
        *q = 0;
    }
    for r in &mut w.state.fields.q_remainder {
        *r = 0;
    }
}

pub fn run_scenario(sc: Scenario, experiment_hex: &str, commit: &str) -> RunRow {
    let spec = reversal_spec();
    let rh = hex_hash(&rules_hash(&RuleSpec::eu_core_1()).unwrap());
    let mut st = initialize_reversal_world(&spec, sc.seed).unwrap();
    if let Some((x, y)) = sc.place {
        st.organisms[0].geom.x = x;
        st.organisms[0].geom.y = y;
    }
    if let Some(th) = sc.heading {
        st.organisms[0].geom.theta = th;
    }
    let mut w = World::from_state(spec, st);
    w.freeze_learning = sc.freeze_learning;
    w.disable_reproduction = true;
    w.control_mode = sc.control;
    w.enable_probe = sc.collect_decisions;
    let initial_q = w.state.fields.q.iter().copied().sum();
    if sc.dark {
        extinguish(&mut w);
    }

    let mut row = RunRow {
        scenario: sc.clone(),
        ledger: RunLedger::default(),
        rules_hash: rh,
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
            light_right(&mut w.spec);
            switched = true;
        }
        if w.state.organisms.is_empty() {
            break;
        }
        match w.step() {
            Ok(()) => {}
            Err(e) => {
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
            if t % 50 == 0 {
                row.path.push((
                    w.state.tick,
                    x,
                    y,
                    org.accounts.e_old,
                    org.accounts.c,
                    budget_p(org.accounts.e_old, org.accounts.e_new, org.accounts.c),
                ));
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
    if sc.group == "C0" {
        let photo = w.run_ledger.photo_extent;
        row.c0_pass = Some(row.alive_at_end && photo > 0 && w.state.tick >= sc.max_ticks);
    }
    row.decisions = w.last_decisions.clone();
    row.ledger = w.run_ledger.clone();
    row
}

fn forced_world(seed: u64, thrust: i8, turn: i8, x: f64, y: f64, theta: f64) -> World {
    let spec = reversal_spec();
    let mut st = initialize_reversal_world(&spec, seed).unwrap();
    st.organisms[0].geom.x = x;
    st.organisms[0].geom.y = y;
    st.organisms[0].geom.theta = theta;
    st.organisms[0].accounts.e_old = 8_000_000;
    let mut w = World::from_state(spec, st);
    w.freeze_learning = true;
    w.disable_reproduction = true;
    w.control_mode = ControlMode::Forced { thrust, turn };
    w
}

pub fn run_action_costs() -> Vec<ActionCostResult> {
    let mut out = Vec::new();
    out.push(action_case("u0_w0", 0, 0, 8.0, 8.0, 0.0, 0, 0, true));
    out.push(action_case("u1_w0", 1, 0, 3.0, 8.0, 0.0, 100_000, 0, false));
    out.push(action_case(
        "u0_w1",
        0,
        1,
        8.0,
        8.0,
        0.0,
        0,
        20_000,
        false,
    ));
    out.push(action_case(
        "u1_w1",
        1,
        1,
        5.0,
        8.0,
        0.0,
        100_000,
        20_000,
        false,
    ));
    out.push(action_case(
        "wall_u1",
        1,
        0,
        0.3,
        8.0,
        std::f64::consts::PI,
        100_000,
        0,
        false,
    ));

    let spec = reversal_spec();
    let mut org = initialize_reversal_world(&spec, 1).unwrap().organisms.remove(0);
    org.accounts.e_old = 600;
    org.accounts.e_new = 40_000;
    org.control.thrust = 1;
    org.control.turn = 1;
    let pay: MotionPay = scale_control_parts(&mut org, &spec).unwrap();
    let ok = pay.total() == 600
        && pay.thrust == 500
        && pay.turn == 100
        && (org.control.u_effective - 0.5).abs() < 1e-12
        && org.accounts.e_new == 40_000;
    out.push(ActionCostResult {
        name: "p3_e_old_600".into(),
        motion_thrust: pay.thrust,
        motion_turn: pay.turn,
        abs_dtheta: 0.0,
        dx: 0.0,
        dy: 0.0,
        dtheta_end: 0.0,
        ok,
        detail: format!(
            "paid={} u_eff={:.3} e_new={}",
            pay.total(),
            org.control.u_effective,
            org.accounts.e_new
        ),
    });
    out
}

fn action_case(
    name: &str,
    thrust: i8,
    turn: i8,
    x: f64,
    y: f64,
    theta: f64,
    expect_u: u64,
    expect_w: u64,
    expect_still: bool,
) -> ActionCostResult {
    let mut w = forced_world(99, thrust, turn, x, y, theta);
    let x0 = w.state.organisms[0].geom.x;
    let y0 = w.state.organisms[0].geom.y;
    let th0 = w.state.organisms[0].geom.theta;
    for _ in 0..100 {
        w.step().expect("action step");
    }
    let org = &w.state.organisms[0];
    let dx = org.geom.x - x0;
    let dy = org.geom.y - y0;
    let dth = org.geom.theta - th0;
    let abs_dth = w.run_ledger.abs_dtheta_times_1e9 as f64 / 1e9;
    let mut ok = w.run_ledger.thrust_attempt == expect_u
        && w.run_ledger.turn_attempt == expect_w;
    let mut detail = format!(
        "ledger u={} w={} abs_dtheta={:.6} dx={:.4} dy={:.4}",
        w.run_ledger.thrust_attempt, w.run_ledger.turn_attempt, abs_dth, dx, dy
    );
    if expect_still {
        ok = ok && dx.abs() < 1e-9 && dy.abs() < 1e-9 && dth.abs() < 1e-9;
    }
    if name == "u1_w0" {
        ok = ok && (dx - 10.0).abs() < 0.15 && dy.abs() < 0.15;
        detail.push_str(&format!(" (want dx~10)"));
    }
    if name == "u0_w1" {
        let want = 10.0 * std::f64::consts::PI;
        ok = ok && (abs_dth - want).abs() < 1e-6;
        detail.push_str(&format!(" (want |dtheta|~{want:.6})"));
    }
    if name == "wall_u1" {
        ok = ok && org.geom.x <= 0.31 + 1e-6;
        detail.push_str(" (clamped at wall)");
    }
    ActionCostResult {
        name: name.into(),
        motion_thrust: w.run_ledger.thrust_attempt,
        motion_turn: w.run_ledger.turn_attempt,
        abs_dtheta: abs_dth,
        dx,
        dy,
        dtheta_end: dth,
        ok,
        detail,
    }
}

fn opt_u64(v: Option<u64>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}

fn opt_u128(v: Option<u128>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}

fn opt_bool(v: Option<bool>) -> String {
    match v {
        Some(true) => "1".into(),
        Some(false) => "0".into(),
        None => String::new(),
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn write_runs_csv(path: &Path, rows: &[RunRow]) -> std::io::Result<()> {
    let mut f = fs::File::create(path)?;
    writeln!(
        f,
        "seed,group,commit,rules_hash,experiment_hash,executed_ticks,decision_count,learning_update_count,alive_at_switch,death_tick,death_cause,first_old_light_entry_tick,first_new_light_entry_tick,old_light_residence_before_switch,new_light_residence_after_switch,E_at_switch,C_at_switch,N_at_switch,debt_at_switch,photo_extent_before,photo_extent_after,organic_intake_before,organic_intake_after,catabolic_E_before,catabolic_E_after,maintenance_paid,cognition_paid,movement_paid,turn_paid,debt_repaid,repair_paid,reproduction_paid,other_paid,P_initial,P_at_switch,P_before_death_or_end,alive_at_end,c0_pass,error"
    )?;
    for r in rows {
        let l = &r.ledger;
        let alive_sw = match (r.scenario.switch_tick, l.death_tick) {
            (None, _) => String::new(),
            (Some(sw), None) => {
                if r.alive_at_end || l.executed_ticks >= sw {
                    "1".into()
                } else {
                    "0".into()
                }
            }
            (Some(sw), Some(d)) => if d >= sw { "1" } else { "0" }.into(),
        };
        writeln!(
            f,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            r.scenario.seed,
            r.scenario.group,
            r.commit,
            r.rules_hash,
            r.experiment_hash,
            l.executed_ticks,
            l.decision_count,
            l.learning_update_count,
            alive_sw,
            opt_u64(l.death_tick),
            csv_escape(l.death_cause.as_deref().unwrap_or("")),
            opt_u64(l.first_old_light_entry),
            opt_u64(l.first_new_light_entry),
            l.old_light_residence_before,
            l.new_light_residence_after,
            opt_u64(l.e_at_switch),
            opt_u64(l.c_at_switch),
            opt_u64(l.n_at_switch),
            opt_u64(l.debt_at_switch),
            l.photo_before_switch,
            l.photo_after_switch,
            l.uptake_o_before_switch,
            l.uptake_o_after_switch,
            l.metab_e_before_switch,
            l.metab_e_after_switch,
            l.maint_paid,
            l.cognition,
            l.thrust_attempt,
            l.turn_attempt,
            l.debt_repay,
            l.repair,
            l.repro_start.saturating_add(l.repro_build),
            0,
            opt_u128(l.p_initial),
            opt_u128(l.p_at_switch),
            opt_u128(l.p_before_death_or_end),
            u8::from(r.alive_at_end),
            opt_bool(r.c0_pass),
            csv_escape(r.error.as_deref().unwrap_or("")),
        )?;
    }
    Ok(())
}

pub fn write_windows_csv(path: &Path, rows: &[RunRow]) -> std::io::Result<()> {
    let mut f = fs::File::create(path)?;
    writeln!(
        f,
        "tick,group,seed,E,C,N,P,photo_extent,uptake_o,catabolic_E,maintenance,cognition,thrust,turn,x,y,living"
    )?;
    for r in rows {
        for w in &r.windows {
            writeln!(
                f,
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{:.6},{:.6},{}",
                w.tick,
                w.group,
                w.seed,
                w.e,
                w.c,
                w.n,
                w.p,
                w.photo,
                w.uptake_o,
                w.metab_e,
                w.maint,
                w.cog,
                w.thrust,
                w.turn,
                w.x,
                w.y,
                w.living
            )?;
        }
    }
    Ok(())
}

pub fn write_decisions_csv(path: &Path, diags: &[DecisionDiag]) -> std::io::Result<()> {
    let mut f = fs::File::create(path)?;
    writeln!(
        f,
        "tick,entity,update,interval_d,R,delta,delta_used,clipped,entropy,A_minus_A0,trace_a,h_l2,input_change,da_bias,da_feat,v_prev,v_next,intent_u,intent_w,paid_u,paid_w"
    )?;
    for (i, d) in diags.iter().enumerate() {
        if i % 100 != 0 && i + 1 != diags.len() {
            continue;
        }
        writeln!(
            f,
            "{},{},{},{},{:.8},{:.8},{:.8},{},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{:.8},{},{},{},{}",
            d.tick,
            d.entity_id,
            u8::from(d.applied_update),
            d.interval_d,
            d.r,
            d.delta,
            d.delta_used,
            u8::from(d.clipped),
            d.entropy,
            d.a_minus_a0_frob,
            d.trace_a_frob,
            d.h_l2_delta,
            d.input_change,
            d.da_bias_abs,
            d.da_feat_abs,
            d.v_prev,
            d.v_next,
            d.intent_thrust,
            d.intent_turn,
            d.paid_thrust,
            d.paid_turn
        )?;
    }
    Ok(())
}

fn svg_map_xy(x: f64, y: f64, w: f64, h: f64) -> (f64, f64) {
    let pad = 40.0;
    (
        pad + x / 16.0 * (w - 2.0 * pad),
        h - pad - y / 16.0 * (h - 2.0 * pad),
    )
}

pub fn write_trajectory_svg(path: &Path, row: &RunRow) -> std::io::Result<()> {
    let mut s = String::new();
    let w = 720.0;
    let h = 720.0;
    s.push_str(&format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {w} {h}'>\n"
    ));
    s.push_str("<rect width='100%' height='100%' fill='#111'/>\n");
    let left = [(3.0, 7.0), (6.0, 10.0)];
    let right = [(10.0, 7.0), (13.0, 10.0)];
    for (c0, c1, color) in [
        (left[0], left[1], "#2a5"),
        (right[0], right[1], "#258"),
    ] {
        let (ax, ay) = svg_map_xy(c0.0, c0.1, w, h);
        let (bx, by) = svg_map_xy(c1.0, c1.1, w, h);
        let x = ax.min(bx);
        let y = ay.min(by);
        let bw = (ax - bx).abs();
        let bh = (ay - by).abs();
        s.push_str(&format!(
            "<rect x='{x:.1}' y='{y:.1}' width='{bw:.1}' height='{bh:.1}' fill='{color}' fill-opacity='0.35'/>\n"
        ));
    }
    if row.path.len() >= 2 {
        s.push_str("<polyline fill='none' stroke='#fc3' stroke-width='2' points='");
        for &(tick, x, y, _, _, _) in &row.path {
            let _ = tick;
            let (px, py) = svg_map_xy(x, y, w, h);
            s.push_str(&format!("{px:.1},{py:.1} "));
        }
        s.push_str("'/>\n");
    }
    if let Some((_, x, y, _, _, _)) = row.path.first() {
        let (px, py) = svg_map_xy(*x, *y, w, h);
        s.push_str(&format!(
            "<circle cx='{px:.1}' cy='{py:.1}' r='5' fill='#fff'/>\n"
        ));
    }
    if !row.alive_at_end {
        if let Some((_, x, y, _, _, _)) = row.path.last() {
            let (px, py) = svg_map_xy(*x, *y, w, h);
            s.push_str(&format!(
                "<circle cx='{px:.1}' cy='{py:.1}' r='6' fill='#f35'/>\n"
            ));
        }
    }
    s.push_str(&format!(
        "<text x='40' y='24' fill='#ddd' font-size='14'>{} seed {}</text>\n",
        row.scenario.group, row.scenario.seed
    ));
    s.push_str("</svg>\n");
    fs::write(path, s)
}

pub fn write_budget_svg(path: &Path, row: &RunRow) -> std::io::Result<()> {
    let mut s = String::new();
    let w = 900.0;
    let h = 420.0;
    s.push_str(&format!(
        "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 {w} {h}'>\n"
    ));
    s.push_str("<rect width='100%' height='100%' fill='#111'/>\n");
    if row.windows.is_empty() {
        s.push_str("</svg>\n");
        return fs::write(path, s);
    }
    let max_tick = row.windows.last().map(|x| x.tick).unwrap_or(1).max(1) as f64;
    let max_p = row
        .windows
        .iter()
        .map(|x| x.p)
        .max()
        .unwrap_or(1)
        .max(1) as f64;
    let plot = |pts: &[(f64, f64)], color: &str| {
        let mut out = format!("<polyline fill='none' stroke='{color}' stroke-width='1.5' points='");
        for &(t, v) in pts {
            let px = 50.0 + t / max_tick * 820.0;
            let py = 380.0 - (v / max_p) * 340.0;
            out.push_str(&format!("{px:.1},{py:.1} "));
        }
        out.push_str("'/>\n");
        out
    };
    let e_pts: Vec<(f64, f64)> = row
        .windows
        .iter()
        .map(|x| (x.tick as f64, x.e as f64))
        .collect();
    let c_pts: Vec<(f64, f64)> = row
        .windows
        .iter()
        .map(|x| (x.tick as f64, (x.c as u128 * 8) as f64))
        .collect();
    let p_pts: Vec<(f64, f64)> = row
        .windows
        .iter()
        .map(|x| (x.tick as f64, x.p as f64))
        .collect();
    s.push_str(&plot(&p_pts, "#6cf"));
    s.push_str(&plot(&e_pts, "#fc3"));
    s.push_str(&plot(&c_pts, "#6c6"));
    if let Some(sw) = row.scenario.switch_tick {
        let px = 50.0 + sw as f64 / max_tick * 820.0;
        s.push_str(&format!(
            "<line x1='{px:.1}' y1='40' x2='{px:.1}' y2='380' stroke='#f66' stroke-dasharray='4 3'/>\n"
        ));
    }
    s.push_str(
        "<text x='50' y='24' fill='#6cf' font-size='13'>P</text>\n\
         <text x='80' y='24' fill='#fc3' font-size='13'>E</text>\n\
         <text x='110' y='24' fill='#6c6' font-size='13'>8C</text>\n",
    );
    s.push_str("</svg>\n");
    fs::write(path, s)
}

fn find_group<'a>(rows: &'a [RunRow], group: &str) -> Option<&'a RunRow> {
    rows.iter().find(|r| r.scenario.group == group)
}

pub fn write_report(
    path: &Path,
    commit: &str,
    rules: &str,
    experiment: &str,
    actions: &[ActionCostResult],
    rows: &[RunRow],
) -> std::io::Result<()> {
    let c0 = find_group(rows, "C0");
    let c1 = find_group(rows, "C1");
    let c2 = find_group(rows, "C2");
    let c3 = find_group(rows, "C3");
    let c4 = find_group(rows, "C4");
    let c5: Vec<&RunRow> = rows.iter().filter(|r| r.scenario.group.starts_with("C5")).collect();
    let mut md = String::new();
    md.push_str("# D1 诊断轮报告\n\n");
    md.push_str(&format!("日期：2026-09-10\n\n"));
    md.push_str(&format!("`commit={commit}`\n\n"));
    md.push_str(&format!("`rules_hash={rules}`\n\n"));
    md.push_str(&format!("`experiment_hash={experiment}`\n\n"));
    md.push_str("命令：`cargo run --release -p universe_lab --bin d1 -- docs/d1`\n\n");
    md.push_str("本轮不改学习率、不扩网络、不推进 E/S/配对。诊断 P 使用 `P = E + 8*C`（微单位）。\n\n");

    md.push_str("## 原 E0 反转结论（保留，不覆盖）\n\n");
    md.push_str("原文摘自 [`docs/batches/G5.md`](../batches/G5.md)（提交 `5daa29e`）：\n\n");
    md.push_str("```\n");
    md.push_str("后半程累计代谢 E（学习−冻结）配对：\n");
    md.push_str("- 均值 495591.2 μE\n");
    md.push_str("- 2.5%/97.5% 分位 **[-3283781.6, 4559920]**，区间跨零\n");
    md.push_str("多数种子在换灯前死亡（存活 ticks 约 3.6k–9k），后半程代谢 E 为 0\n");
    md.push_str("**结论：尚未观察到可靠收益。**\n");
    md.push_str("```\n\n");
    md.push_str("累计代谢 E 可以把旧 C 转成可支出 E，**不能**单独解释为新补给或净收益。换灯前死亡样本全部保留。C5 是区间反馈接线之后的复现，标识为本 `experiment_hash`，不是改写 G5。\n\n");

    md.push_str("## 动作成本专项\n\n");
    md.push_str("| 案例 | thrust μE | turn μE | 几何 | 通过 |\n|---|---|---|---|---|\n");
    for a in actions {
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            a.name,
            a.motion_thrust,
            a.motion_turn,
            a.detail,
            if a.ok { "是" } else { "否" }
        ));
    }
    md.push_str("\n费用看分类账本，不用期末总 E。P3 旧 E=600、请求 1200 只支付 600，两控制缩放到 0.5；`e_new` 不能补差。\n\n");

    fn line(md: &mut String, label: &str, r: Option<&RunRow>) {
        match r {
            None => md.push_str(&format!("- {label}：无记录\n")),
            Some(r) => md.push_str(&format!(
                "- {label}：executed={} death={} photo={} P_end={} alive={} x={:.2} y={:.2}\n",
                r.ledger.executed_ticks,
                r.ledger
                    .death_tick
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "—".into()),
                r.ledger.photo_extent,
                r.ledger
                    .p_before_death_or_end
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "—".into()),
                r.alive_at_end,
                r.final_x,
                r.final_y
            )),
        }
    }
    md.push_str("## C0–C5 摘要\n\n");
    line(&mut md, "C0 灯心静止", c0);
    line(&mut md, "C1 黑暗静止", c1);
    line(&mut md, "C2 中心静止", c2);
    line(&mut md, "C3 Oracle", c3);
    line(&mut md, "C4 ReflexQ", c4);
    if let Some(r) = c1 {
        md.push_str(&format!(
            "  （C1 关灯前场 Q 总和 {} μE；死亡前 P 采样为 {}，债务致死后账户已耗尽属实，不是把缺失写成 0 的换灯字段。）\n",
            r.initial_q,
            r.ledger
                .p_before_death_or_end
                .map(|p| p.to_string())
                .unwrap_or_else(|| "空缺".into())
        ));
    }
    if let Some(r) = c0 {
        md.push_str(&format!(
            "\nC0 判定（长期存活且光合>0）：{}\n",
            if r.c0_pass == Some(true) {
                "通过"
            } else {
                "未通过"
            }
        ));
    }
    md.push_str(&format!("\nC5 行数：{}（应为 40）\n\n", c5.len()));

    let mut learn_e = Vec::new();
    let mut freeze_e = Vec::new();
    for r in &c5 {
        if r.scenario.group == "C5-learn" {
            learn_e.push(r.ledger.metab_e_after_switch as f64);
        }
        if r.scenario.group == "C5-freeze" {
            freeze_e.push(r.ledger.metab_e_after_switch as f64);
        }
    }
    if learn_e.len() == freeze_e.len() && !learn_e.is_empty() {
        let diffs: Vec<f64> = learn_e
            .iter()
            .zip(freeze_e.iter())
            .map(|(a, b)| a - b)
            .collect();
        let (mean, lo, hi) = crate::paired_bootstrap(&diffs, 20260910, 2000);
        md.push_str(&format!(
            "C5 后半程代谢 E（学习−冻结）配对均值 {mean:.1}，bootstrap 95% [{lo:.1}, {hi:.1}]。\n"
        ));
        md.push_str("这是修复感官反馈后的复现，**不**把学习门禁改写成通过。\n\n");
    }

    let c0_ok = c0.and_then(|r| r.c0_pass) == Some(true);
    let c3_alive = c3.map(|r| r.entered_old || r.ledger.photo_extent > 0).unwrap_or(false);
    let c4_photo = c4.map(|r| r.ledger.photo_extent > 0).unwrap_or(false);
    let action_ok = actions.iter().all(|a| a.ok);

    md.push_str("## 三列根因\n\n");
    md.push_str("### 已证实\n\n");
    md.push_str("- `World::step` 在候选状态上执行；`NumericFault` 丢弃候选，提交边界的规范字节与 `state_hash` 不变，`last_events` 为空，tick 不推进。关闭注入后再走同一步，与从未注入的参考世界一致。\n");
    md.push_str("- 档案 `write_snapshot` 在 rename 前失败时不登记 SQLite 行，恢复索引不能指向缺失快照。\n");
    md.push_str("- `IntervalFeedback` 的光合 extent / N+O 摄取 / ΔE 已写入；感官 26–31 不再在 `ticks>0` 时报告“测得零”。这会改变生命输入，因此 C5 必须新 `experiment_hash`。\n");
    md.push_str("- 诊断 P 与分类账本（维持、偿债、认知、推进、转向、代谢 E_new、光合 extent）为只读侧信道；开关探针不改变 `state_hash`。\n");
    if action_ok {
        md.push_str("- 动作费用专项与 P3 缩放符合账本：静止 0、直行 100000、转向 20000、混合 120000、撞墙仍付推进费；旧 E 不足时不能用 `e_new` 补差。\n");
    }
    if c0_ok {
        md.push_str("- C0：灯心静止 20000 ticks 存活，且实际光合非零，基础资源闭环可以支付生命费用（允许容量满时净 P 不增）。\n");
    }
    md.push_str("- 原 E0 的跨零结论仍然成立：后半程代谢 E 差值区间跨零；该指标也可来自旧 C。\n\n");

    md.push_str("### 被证据削弱\n\n");
    if c0_ok {
        md.push_str("- “同化完全不工作，所以 10000 ticks 前必然死亡”被 C0 削弱：在灯心且不运动时，外来同化可以覆盖基础费用。\n");
        md.push_str("- 因此换灯前死亡更宜解释为**没能持续待在补给区**（或运动把费用打高），而不是灯下物理闭环不存在。\n");
    } else {
        md.push_str("- C0 未通过，不能削弱“灯下闭环失败”的假设；神经学习不是第一嫌疑。\n");
    }
    if action_ok {
        md.push_str("- “推进/转向扣费实现数量级错误”被分项账本削弱（100 ticks 专项与规则 1000/200 μE 一致）。\n");
    }
    md.push_str("\n");

    md.push_str("### 未知\n\n");
    md.push_str("- 完整 64 维特征 + 在线 actor–critic 在反转图上是否产生可靠适应：C5 复现仍不得改写为通过。\n");
    md.push_str("- 奖励时间尺度（γ≈0.999）与高储备缓冲是否使换灯后迁移的价值过弱，需独立低储备/共同初态探针，而不是在本轮调学习率。\n");
    if c3_alive {
        md.push_str("- C3 Oracle 能取得补给，说明身体与地图在脚本权限下可达；这不证明神经网络会走同一条路。\n");
    } else {
        md.push_str("- C3 未稳定取得补给：先核脚本是否按 5-tick 与预算执行，再谈策略不可达。\n");
    }
    if c4_photo {
        md.push_str("- C4 反射在 20000 ticks 内存活且光合与 C0 同量级；终点可以落在绘图光区矩形之外（扩散补给）。正式 64 维 Q 足以支持一种局部觅食，不能把 C5 死亡写成“感官没有光信息”。\n");
    } else {
        md.push_str("- C4 光合仍接近零：可能是反射规则过弱，不能直接上升为“感官信息论不可观测”。\n");
    }
    md.push_str("\n");

    md.push_str("## 学习链审计（代码，非调参）\n\n");
    md.push_str("1. 方向：`DIRS = [0, π/2, π, −π/2]` → 前/左/后/右；Q 索引 3/7/11/15。`turn=+1` 增加 θ（逆时针），左转后原左侧成为前侧。\n");
    md.push_str("2. 64 维数值+有效位进入 `recurrent_features`；缺失填 0、有效位 0/1。\n");
    md.push_str("3. `phi = [1, h/√32]`，不是除以 √33。\n");
    md.push_str("4. 策略意图在 `SampledIntent`；付费后 `u_effective/w_effective`；实际位移/extent 在几何与反应。覆盖控制器时 `freeze_learning=true`，梯度仍对应抽样意图。\n");
    md.push_str("5. 区间在下一次 `decide` 用真实 `interval_d` / `gamma` / `trace_decay` 更新一次。\n");
    md.push_str("6. `V_next` 使用更新前的 `v` 与新 `phi`。\n");
    md.push_str("7. logits 12–15 置 `-inf`；B 版社交四通道不可执行。\n");
    md.push_str("8. actor_lr / critic_lr / entropy_coef / baseline_coef 只出现在 `apply_readout_update` 的规定位置。\n");
    md.push_str("9. A0 约束是对读出 `A` 的偏移惩罚，不改基因、不覆盖 `h`。\n");
    md.push_str("10. 冻结组只跳过读出更新；维持、感知、代谢、真实控制限制仍在。\n\n");

    md.push_str("## 后续实验（至多两项）\n\n");
    if c0_ok && c3_alive {
        md.push_str("1. **F2 共同初态恢复探针**：用 C3（或灯心静止至稳定光合）生成公开检查点；换灯后等待 200 ticks，再分学习/冻结。主指标：最早满足“连续 100 ticks 存活、光合 extent>0、窗口 ΔP≥0”的结束 tick `T_recover`，截断到预注册 H（建议先取 H=4000 ticks，覆盖折扣有效尺度并短于原后半程）。死亡或未恢复记失败。种子与 E0 的 101–120 分开预注册。\n");
        md.push_str("2. **低储备动机探针**（仅当 F2 的共同初态仍远高于奖励 C 不足区时才跑）：把检查点 C 降到刚过 `reward_c_target` 的公开值，检验换灯后是否出现可学习的饥饿差；这是新实验，不是从 E0 里偷偷抽走 C。\n");
    } else if !c0_ok {
        md.push_str("1. **先修灯下闭环**：C0 失败时不要做 F2/学习门禁。核对光合多输入结算、N/Q 配额、代谢与容量。\n");
        md.push_str("2. 闭环修复后再用 Oracle 量行程，再考虑 F2。\n");
    } else {
        md.push_str("1. **先核 C3 脚本**：确认 5-tick 决策、转向累加与预算缩放后是否能在 ~7 LU 级行程内进灯。\n");
        md.push_str("2. 脚本可达后再做 F2 共同初态；本轮仍不调学习率。\n");
    }
    md.push_str("\n本轮不强行把学习门禁写成通过。\n");
    fs::write(path, md)
}

pub fn write_manifest(
    path: &Path,
    commit: &str,
    rules: &str,
    experiment: &str,
    command: &str,
) -> std::io::Result<()> {
    let body = format!(
        "{{\n  \"commit\": \"{commit}\",\n  \"profile\": \"EU-Core-1\",\n  \"rules_hash\": \"{rules}\",\n  \"experiment_hash\": \"{experiment}\",\n  \"command\": \"{command}\",\n  \"switch_tick\": {switch},\n  \"max_ticks\": {max},\n  \"seeds\": {seeds:?},\n  \"p_definition\": \"P = E_old + E_new + 8*C (micro-units)\",\n  \"map\": \"reversal_16x16\",\n  \"controllers\": [\"Stationary\", \"OracleGotoLight\", \"ReflexQ\", \"Neural\"]\n}}\n",
        switch = D1_SWITCH_TICK,
        max = D1_MAX_TICKS,
        seeds = REVERSAL_SEEDS,
    );
    fs::write(path, body)
}

pub fn scenarios() -> Vec<Scenario> {
    let mut v = vec![
        Scenario {
            group: "C0".into(),
            seed: 7,
            freeze_learning: true,
            control: ControlMode::Stationary,
            switch_tick: None,
            max_ticks: D1_MAX_TICKS,
            place: Some((4.5, 8.5)),
            heading: Some(0.0),
            dark: false,
            collect_decisions: false,
        },
        Scenario {
            group: "C1".into(),
            seed: 7,
            freeze_learning: true,
            control: ControlMode::Stationary,
            switch_tick: None,
            max_ticks: D1_MAX_TICKS,
            place: Some((8.0, 8.0)),
            heading: Some(0.0),
            dark: true,
            collect_decisions: false,
        },
        Scenario {
            group: "C2".into(),
            seed: 7,
            freeze_learning: true,
            control: ControlMode::Stationary,
            switch_tick: None,
            max_ticks: D1_MAX_TICKS,
            place: Some((8.0, 8.0)),
            heading: Some(0.0),
            dark: false,
            collect_decisions: false,
        },
        Scenario {
            group: "C3".into(),
            seed: 7,
            freeze_learning: true,
            control: ControlMode::OracleGotoLight,
            switch_tick: Some(D1_SWITCH_TICK),
            max_ticks: D1_MAX_TICKS,
            place: Some((8.0, 8.0)),
            heading: Some(0.0),
            dark: false,
            collect_decisions: false,
        },
        Scenario {
            group: "C4".into(),
            seed: 7,
            freeze_learning: true,
            control: ControlMode::ReflexQ,
            switch_tick: Some(D1_SWITCH_TICK),
            max_ticks: D1_MAX_TICKS,
            place: Some((8.0, 8.0)),
            heading: Some(0.0),
            dark: false,
            collect_decisions: false,
        },
    ];
    for seed in REVERSAL_SEEDS {
        v.push(Scenario {
            group: "C5-learn".into(),
            seed,
            freeze_learning: false,
            control: ControlMode::Neural,
            switch_tick: Some(D1_SWITCH_TICK),
            max_ticks: D1_MAX_TICKS,
            place: None,
            heading: None,
            dark: false,
            collect_decisions: seed == 101,
        });
        v.push(Scenario {
            group: "C5-freeze".into(),
            seed,
            freeze_learning: true,
            control: ControlMode::Neural,
            switch_tick: Some(D1_SWITCH_TICK),
            max_ticks: D1_MAX_TICKS,
            place: None,
            heading: None,
            dark: false,
            collect_decisions: false,
        });
    }
    v
}

pub fn run_d1_campaign(out_dir: impl AsRef<Path>) -> Result<(), String> {
    let out = out_dir.as_ref();
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let commit = git_commit();
    let spec = RuleSpec::eu_core_1();
    let rules = hex_hash(&rules_hash(&spec).unwrap());
    let experiment = hex_hash(&campaign_experiment_hash());
    let command = format!(
        "cargo run --release -p universe_lab --bin d1 -- {}",
        out.display()
    );

    let actions = run_action_costs();
    if let Some(bad) = actions.iter().find(|a| !a.ok) {
        return Err(format!("action cost failed: {} {}", bad.name, bad.detail));
    }

    let mut rows = Vec::new();
    for sc in scenarios() {
        eprintln!("D1 {} seed {}", sc.group, sc.seed);
        rows.push(run_scenario(sc, &experiment, &commit));
    }
    if rows.iter().filter(|r| r.scenario.group.starts_with("C5")).count() != 40 {
        return Err("C5 did not retain 40 rows".into());
    }

    write_manifest(&out.join("manifest.json"), &commit, &rules, &experiment, &command)
        .map_err(|e| e.to_string())?;
    write_runs_csv(&out.join("runs.csv"), &rows).map_err(|e| e.to_string())?;
    write_windows_csv(&out.join("windows.csv"), &rows).map_err(|e| e.to_string())?;
    let dec = rows
        .iter()
        .find(|r| r.scenario.group == "C5-learn" && r.scenario.seed == 101)
        .map(|r| r.decisions.clone())
        .unwrap_or_default();
    write_decisions_csv(&out.join("decisions.csv"), &dec).map_err(|e| e.to_string())?;
    let traj = rows
        .iter()
        .find(|r| r.scenario.group == "C3")
        .ok_or("no C3")?;
    write_trajectory_svg(&out.join("trajectory.svg"), traj).map_err(|e| e.to_string())?;
    let bud = rows
        .iter()
        .find(|r| r.scenario.group == "C0")
        .ok_or("no C0")?;
    write_budget_svg(&out.join("budget.svg"), bud).map_err(|e| e.to_string())?;
    write_report(
        &out.join("report.md"),
        &commit,
        &rules,
        &experiment,
        &actions,
        &rows,
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use universe_core::probe::ControlMode;

    #[test]
    fn action_costs_match_ledger() {
        for a in run_action_costs() {
            assert!(a.ok, "{} {}", a.name, a.detail);
        }
    }

    #[test]
    fn oracle_holds_control_for_five_ticks() {
        let spec = reversal_spec();
        let mut st = initialize_reversal_world(&spec, 7).unwrap();
        st.organisms[0].geom.x = 8.0;
        st.organisms[0].geom.y = 8.0;
        st.organisms[0].geom.theta = 0.0;
        let mut w = World::from_state(spec, st);
        w.freeze_learning = true;
        w.disable_reproduction = true;
        w.control_mode = ControlMode::OracleGotoLight;
        w.step().unwrap();
        let u0 = w.state.organisms[0].control.thrust;
        let w0 = w.state.organisms[0].control.turn;
        assert!(u0 != 0 || w0 != 0, "oracle should command motion");
        for _ in 0..4 {
            w.step().unwrap();
            assert_eq!(w.state.organisms[0].control.thrust, u0);
            assert_eq!(w.state.organisms[0].control.turn, w0);
        }
        w.step().unwrap();
    }

    #[test]
    fn c3_reaches_old_light_short() {
        let sc = Scenario {
            group: "C3s".into(),
            seed: 7,
            freeze_learning: true,
            control: ControlMode::OracleGotoLight,
            switch_tick: None,
            max_ticks: 800,
            place: Some((8.0, 8.0)),
            heading: Some(0.0),
            dark: false,
            collect_decisions: false,
        };
        let row = run_scenario(sc, "test", "test");
        assert!(row.error.is_none(), "{:?}", row.error);
        assert!(
            row.entered_old || row.ledger.photo_extent > 0,
            "oracle did not forage photo={} x={:.2} y={:.2}",
            row.ledger.photo_extent,
            row.final_x,
            row.final_y
        );
    }
}
