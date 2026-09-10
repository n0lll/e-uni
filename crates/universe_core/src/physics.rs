//! Disk kinematics and deterministic contact projection (ch. 35.1–35.2).

use crate::rng::{geometry_draw_index, random_u32};
use universe_types::{NumericFault, RuleSpec, WorldState, RngDomain};

/// Duration of one physical tick, seconds.
pub fn tick_dt(spec: &RuleSpec) -> f64 {
    let den = spec.time.dt_denominator.max(1);
    spec.time.dt_numerator as f64 / den as f64
}

/// Duration of one physics substep, seconds.
pub fn substep_dt(spec: &RuleSpec) -> f64 {
    let nsub = spec.physics.substeps.max(1);
    tick_dt(spec) / f64::from(nsub)
}

pub fn wrap_angle(mut theta: f64) -> f64 {
    let pi = std::f64::consts::PI;
    let two_pi = 2.0 * pi;
    while theta >= pi {
        theta -= two_pi;
    }
    while theta < -pi {
        theta += two_pi;
    }
    if theta.to_bits() == (-0.0f64).to_bits() {
        0.0
    } else {
        theta
    }
}

pub fn cell_xy(x: f64, y: f64, width: u32, height: u32) -> Option<(u32, u32)> {
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    let gx = x.floor();
    let gy = y.floor();
    if gx < 0.0 || gy < 0.0 {
        return None;
    }
    let ix = gx as u32;
    let iy = gy as u32;
    if ix >= width || iy >= height {
        return None;
    }
    Some((ix, iy))
}

fn axis_normal(word: u32) -> (f64, f64) {
    match word % 4 {
        0 => (1.0, 0.0),
        1 => (-1.0, 0.0),
        2 => (0.0, 1.0),
        _ => (0.0, -1.0),
    }
}

/// One unconstrained substep: rotate, then translate along the new heading.
pub fn integrate_substep(
    x: f64,
    y: f64,
    theta: f64,
    u_eff: f64,
    w_eff: f64,
    spec: &RuleSpec,
) -> (f64, f64, f64) {
    let dt = substep_dt(spec);
    let omega = w_eff * spec.physics.max_angular_speed;
    let v = u_eff * spec.physics.max_linear_speed;
    let theta2 = wrap_angle(theta + omega * dt);
    let x2 = x + v * dt * libm::cos(theta2);
    let y2 = y + v * dt * libm::sin(theta2);
    (x2, y2, theta2)
}

fn project_walls(state: &mut WorldState, spec: &RuleSpec) {
    let r = spec.physics.radius_lu;
    let w = spec.world.width_lu;
    let h = spec.world.height_lu;
    for org in &mut state.organisms {
        if org.marked_dead {
            continue;
        }
        org.geom.x = org.geom.x.clamp(r, w - r);
        org.geom.y = org.geom.y.clamp(r, h - r);
    }
}

fn candidate_pairs(state: &WorldState, spec: &RuleSpec) -> Result<Vec<(u64, u64)>, NumericFault> {
    let r = spec.physics.radius_lu;
    let n = state.organisms.len();
    let mut pairs = Vec::new();
    // Uniform 1 LU bins over AABB of each disk.
    use std::collections::BTreeMap;
    let mut bins: BTreeMap<(i32, i32), Vec<usize>> = BTreeMap::new();
    for (i, org) in state.organisms.iter().enumerate() {
        if org.marked_dead {
            continue;
        }
        let minx = (org.geom.x - r).floor() as i32;
        let maxx = (org.geom.x + r).floor() as i32;
        let miny = (org.geom.y - r).floor() as i32;
        let maxy = (org.geom.y + r).floor() as i32;
        for bx in minx..=maxx {
            for by in miny..=maxy {
                bins.entry((bx, by)).or_default().push(i);
            }
        }
    }
    let mut seen = std::collections::BTreeSet::new();
    for idxs in bins.values() {
        for a in 0..idxs.len() {
            for b in (a + 1)..idxs.len() {
                let i = idxs[a];
                let j = idxs[b];
                let ida = state.organisms[i].entity_id;
                let idb = state.organisms[j].entity_id;
                let (lo, hi) = if ida < idb { (ida, idb) } else { (idb, ida) };
                if seen.insert((lo, hi)) {
                    pairs.push((lo, hi));
                }
            }
        }
    }
    if pairs.len() > spec.safety.max_contact_pairs as usize {
        return Err(NumericFault::Overflow("contact_pairs"));
    }
    let _ = n;
    Ok(pairs)
}

fn lookup(state: &WorldState, id: u64) -> usize {
    state
        .organisms
        .iter()
        .position(|o| o.entity_id == id)
        .expect("id")
}

fn separate_pair(
    state: &mut WorldState,
    spec: &RuleSpec,
    ida: u64,
    idb: u64,
    substep: u32,
    iteration: u32,
) -> Result<f64, NumericFault> {
    let r = spec.physics.radius_lu;
    let ia = lookup(state, ida);
    let ib = lookup(state, idb);
    let a = state.organisms[ia].geom;
    let b = state.organisms[ib].geom;
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let d2 = dx * dx + dy * dy;
    let d = libm::sqrt(d2);
    let min_d = 2.0 * r;
    if d >= min_d {
        return Ok(0.0);
    }
    let (nx, ny) = if d == 0.0 {
        let draw = geometry_draw_index(substep, iteration, 0, 0)?;
        let w = random_u32(
            state.root_seed,
            RngDomain::GeometryNormal.code(),
            state.tick,
            ida.min(idb),
            ida.max(idb),
            draw,
        )?;
        axis_normal(w)
    } else {
        (dx / d, dy / d)
    };
    let pen = min_d - d;
    let wa = 1.0 / (state.organisms[ia].b_target as f64);
    let wb = 1.0 / (state.organisms[ib].b_target as f64);
    let wsum = wa + wb;
    let sa = wa / wsum;
    let sb = wb / wsum;
    state.organisms[ia].geom.x -= nx * pen * sa;
    state.organisms[ia].geom.y -= ny * pen * sa;
    state.organisms[ib].geom.x += nx * pen * sb;
    state.organisms[ib].geom.y += ny * pen * sb;
    Ok(pen)
}

fn max_wall_pen(state: &WorldState, spec: &RuleSpec) -> f64 {
    let r = spec.physics.radius_lu;
    let w = spec.world.width_lu;
    let h = spec.world.height_lu;
    let mut m = 0.0f64;
    for org in &state.organisms {
        if org.marked_dead {
            continue;
        }
        m = m.max((r - org.geom.x).max(0.0));
        m = m.max((org.geom.x - (w - r)).max(0.0));
        m = m.max((r - org.geom.y).max(0.0));
        m = m.max((org.geom.y - (h - r)).max(0.0));
    }
    m
}

fn max_pair_pen(state: &WorldState, spec: &RuleSpec, pairs: &[(u64, u64)]) -> f64 {
    let r = spec.physics.radius_lu;
    let mut m = 0.0f64;
    for &(ida, idb) in pairs {
        let a = state.organisms[lookup(state, ida)].geom;
        let b = state.organisms[lookup(state, idb)].geom;
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let d = libm::sqrt(dx * dx + dy * dy);
        m = m.max((2.0 * r - d).max(0.0));
    }
    m
}

pub fn solve_contacts(
    state: &mut WorldState,
    spec: &RuleSpec,
    substep: u32,
) -> Result<(), NumericFault> {
    let tol = spec.physics.contact_tolerance;
    for it in 0..spec.physics.max_projection_iters {
        project_walls(state, spec);
        let mut pairs = candidate_pairs(state, spec)?;
        if it % 2 == 0 {
            pairs.sort_unstable();
        } else {
            pairs.sort_unstable_by(|a, b| b.cmp(a));
        }
        for &(ida, idb) in &pairs {
            separate_pair(state, spec, ida, idb, substep, it)?;
        }
        project_walls(state, spec);
        let pairs2 = candidate_pairs(state, spec)?;
        let pen = max_wall_pen(state, spec).max(max_pair_pen(state, spec, &pairs2));
        if pen <= tol {
            return Ok(());
        }
    }
    Err(NumericFault::ContactSolverUnresolved)
}

pub fn physics_substeps(state: &mut WorldState, spec: &RuleSpec) -> Result<(), NumericFault> {
    let nsub = spec.physics.substeps;
    let dt = substep_dt(spec);
    for s in 0..nsub {
        let mut starts = Vec::with_capacity(state.organisms.len());
        for org in &mut state.organisms {
            if org.marked_dead {
                starts.push(None);
                continue;
            }
            let x0 = org.geom.x;
            let y0 = org.geom.y;
            let omega = org.control.w_effective * spec.physics.max_angular_speed;
            let v = org.control.u_effective * spec.physics.max_linear_speed;
            let dtheta = omega * dt;
            let th2 = wrap_angle(org.geom.theta + dtheta);
            org.geom.x = x0 + v * dt * libm::cos(th2);
            org.geom.y = y0 + v * dt * libm::sin(th2);
            org.geom.theta = th2;
            starts.push(Some((x0, y0, dtheta, th2)));
        }
        solve_contacts(state, spec, s)?;
        for (org, start) in state.organisms.iter_mut().zip(starts.into_iter()) {
            org.geom.theta = wrap_angle(org.geom.theta);
            let Some((x0, y0, dtheta, heading)) = start else {
                continue;
            };
            if org.marked_dead {
                continue;
            }
            let dx = org.geom.x - x0;
            let dy = org.geom.y - y0;
            let forward = dx * libm::cos(heading) + dy * libm::sin(heading);
            org.last_dx += dx;
            org.last_dy += dy;
            org.last_dtheta += dtheta;
            org.feedback.thrust_sum += forward;
            org.feedback.turn_sum += dtheta;
        }
    }
    Ok(())
}

/// T12-style equal-mass projection along x.
pub fn project_two_equal_disks(x0: f64, y0: f64, x1: f64, y1: f64, radius: f64) -> (f64, f64, f64, f64) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let d = libm::sqrt(dx * dx + dy * dy);
    let min_d = 2.0 * radius;
    if d >= min_d || d == 0.0 {
        return (x0, y0, x1, y1);
    }
    let nx = dx / d;
    let ny = dy / d;
    let pen = min_d - d;
    (
        x0 - nx * pen * 0.5,
        y0 - ny * pen * 0.5,
        x1 + nx * pen * 0.5,
        y1 + ny * pen * 0.5,
    )
}
