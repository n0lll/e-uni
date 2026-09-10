//! Sense 64-vector: 32 values + 32 validity flags (ch. 36.1).

use crate::physics::cell_xy;
use universe_types::{Organism, RuleSpec, WorldState, INPUTS};

const DIRS: [f64; 4] = [0.0, std::f64::consts::FRAC_PI_2, std::f64::consts::PI, -std::f64::consts::FRAC_PI_2];
const RANGES: [f64; 3] = [1.0, 2.0, 4.0];

fn sat01(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

fn sat11(x: f64) -> f64 {
    x.clamp(-1.0, 1.0)
}

fn squash(x: f64, c: f64) -> f64 {
    if c == 0.0 {
        0.0
    } else {
        x / (x + c)
    }
}

fn ray_wall(x: f64, y: f64, dx: f64, dy: f64, w: f64, h: f64) -> f64 {
    let mut t: f64 = 4.0;
    if dx > 0.0 {
        t = t.min((w - x) / dx);
    } else if dx < 0.0 {
        t = t.min((0.0 - x) / dx);
    }
    if dy > 0.0 {
        t = t.min((h - y) / dy);
    } else if dy < 0.0 {
        t = t.min((0.0 - y) / dy);
    }
    t.max(0.0).min(4.0)
}

fn ray_disk(ox: f64, oy: f64, dx: f64, dy: f64, cx: f64, cy: f64, r: f64) -> Option<f64> {
    let fx = ox - cx;
    let fy = oy - cy;
    let b = fx * dx + fy * dy;
    let c = fx * fx + fy * fy - r * r;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let s = libm::sqrt(disc);
    let t0 = -b - s;
    let t1 = -b + s;
    let t = if t0 >= 0.0 {
        t0
    } else if t1 >= 0.0 {
        t1
    } else {
        return None;
    };
    if t <= 4.0 {
        Some(t)
    } else {
        None
    }
}

pub fn sense(state: &WorldState, spec: &RuleSpec, org: &Organism) -> [f64; INPUTS] {
    let mut o = [0.0f64; INPUTS];
    let theta = org.geom.theta;
    let rself = spec.physics.radius_lu;
    // 0..15 fields
    for (di, &rel) in DIRS.iter().enumerate() {
        let dir = theta + rel;
        let dx = libm::cos(dir);
        let dy = libm::sin(dir);
        let mut acc = [0.0f64; 4];
        let mut cnt = [0u32; 4];
        for &dist in &RANGES {
            let px = org.geom.x + dist * dx;
            let py = org.geom.y + dist * dy;
            if let Some((cx, cy)) = cell_xy(px, py, spec.world.grid_width, spec.world.grid_height) {
                let i = state.fields.idx(cx, cy);
                acc[0] += state.fields.n[i] as f64;
                acc[1] += state.fields.o[i] as f64;
                acc[2] += state.fields.s[i] as f64;
                acc[3] += state.fields.q[i] as f64;
                for c in cnt.iter_mut() {
                    *c += 1;
                }
            }
        }
        for ch in 0..4 {
            let idx = di * 4 + ch;
            if cnt[ch] > 0 {
                let mean = acc[ch] / f64::from(cnt[ch]);
                let c = match ch {
                    0 => spec.brain.sense_c_n as f64,
                    1 => spec.brain.sense_c_o as f64,
                    2 => spec.brain.sense_c_s as f64,
                    _ => spec.brain.sense_c_q as f64,
                };
                o[idx] = squash(mean, c);
                o[32 + idx] = 1.0;
            }
        }
    }
    // 16..19 distances
    for (di, &rel) in DIRS.iter().enumerate() {
        let dir = theta + rel;
        let dx = libm::cos(dir);
        let dy = libm::sin(dir);
        let mut best = ray_wall(
            org.geom.x,
            org.geom.y,
            dx,
            dy,
            spec.world.width_lu,
            spec.world.height_lu,
        );
        for other in &state.organisms {
            if other.entity_id == org.entity_id || other.marked_dead {
                continue;
            }
            if let Some(t) = ray_disk(
                org.geom.x,
                org.geom.y,
                dx,
                dy,
                other.geom.x,
                other.geom.y,
                rself,
            ) {
                // surface distance: hit time already to surface if r is radius
                best = best.min(t);
            }
        }
        o[16 + di] = (best.min(4.0) / 4.0).clamp(0.0, 1.0);
        o[32 + 16 + di] = 1.0;
    }
    o[20] = sat01(org.accounts.e_old as f64 / org.e_max as f64);
    o[21] = sat01(org.accounts.c as f64 / org.c_max as f64);
    o[22] = sat01(org.accounts.b as f64 / org.b_target as f64);
    o[23] = sat01(org.maintenance_debt as f64 / spec.physiology.death_debt as f64);
    o[24] = sat01(org.age as f64 / spec.physiology.mature_age as f64);
    o[32 + 20] = 1.0;
    o[32 + 21] = 1.0;
    o[32 + 22] = 1.0;
    o[32 + 23] = 1.0;
    o[32 + 24] = 1.0;
    let has_emb = state
        .embryos
        .iter()
        .any(|e| e.parent_id == org.entity_id);
    if has_emb {
        let e = state
            .embryos
            .iter()
            .find(|e| e.parent_id == org.entity_id)
            .unwrap();
        o[25] = sat01(e.extent as f64 / e.target_extent.max(1) as f64);
    } else {
        o[25] = 0.0;
    }
    o[32 + 25] = 1.0;
    let fb = &org.last_feedback;
    if fb.ticks == 0 {
        for i in 26..32 {
            o[i] = 0.0;
            o[32 + i] = 0.0;
        }
    } else {
        o[26] = sat01(fb.struct_loss as f64 / org.b_target as f64);
        o[27] = sat11(fb.e_net_i128 as f64 / org.e_max as f64);
        let inv = fb.ticks as f64;
        o[28] = sat11((fb.thrust_sum / inv) / spec.physics.max_linear_speed);
        o[29] = sat11((fb.turn_sum / inv) / spec.physics.max_angular_speed);
        let max_up = spec.rates.n_uptake_base.saturating_add(spec.rates.n_uptake_boost)
            + spec.rates.o_uptake_when_on;
        let max_up = max_up.saturating_mul(fb.ticks);
        o[30] = sat01(fb.uptake_no as f64 / max_up.max(1) as f64);
        let max_ph = spec.rates.photo_base.saturating_add(spec.rates.photo_boost) * fb.ticks;
        o[31] = sat01(fb.photo_extent as f64 / max_ph.max(1) as f64);
        for i in 26..32 {
            o[32 + i] = 1.0;
        }
    }
    o
}
