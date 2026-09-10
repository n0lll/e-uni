//! Integer genome factory and decode (ch. 37.7). Factory uses root_seed=0, domain 2, actor=1.

use crate::rng::random_range;
use universe_types::{
    Genome, NumericFault, RngDomain, RuleSpec, HIDDEN, INPUTS, PHI, POLICY_LOGITS, Q_SCALE,
};

fn round_away_from_zero(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    if x >= 0.0 {
        (x + 0.5).floor() as i32
    } else {
        -((-x + 0.5).floor() as i32)
    }
}

pub fn decode_q(q: i32) -> f64 {
    f64::from(q) / f64::from(Q_SCALE)
}

fn encode_q(w: f64) -> i32 {
    round_away_from_zero(w * f64::from(Q_SCALE))
}

fn uniform_signed(
    abs_max: i32,
    param_index: u64,
) -> Result<i32, NumericFault> {
    let n = (2 * abs_max + 1) as u32;
    let u = random_range(0, RngDomain::InitialGenome.code(), 0, 1, 0, param_index * 32, n)?;
    Ok(u as i32 - abs_max)
}

pub fn make_minimal_autotroph_genome(spec: &RuleSpec) -> Result<Genome, NumericFault> {
    let mut idx = 0u64;
    let mut q_win = Vec::with_capacity(HIDDEN * INPUTS);
    for _ in 0..(HIDDEN * INPUTS) {
        q_win.push(uniform_signed(spec.evolution.q_win_init_abs, idx)?);
        idx += 1;
    }
    let mut q_wrec = vec![0i32; HIDDEN * HIDDEN];
    for i in 0..HIDDEN {
        for j in 0..HIDDEN {
            if i == j {
                q_wrec[i * HIDDEN + j] = spec.evolution.q_wrec_diag;
            } else {
                q_wrec[i * HIDDEN + j] = uniform_signed(spec.evolution.q_wrec_off_abs, idx)?;
                idx += 1;
            }
        }
    }
    // Row L1 constraint on decoded W_rec, then re-quantize.
    for i in 0..HIDDEN {
        let mut sum = 0.0;
        let mut row = [0.0f64; HIDDEN];
        for j in 0..HIDDEN {
            row[j] = decode_q(q_wrec[i * HIDDEN + j]);
            sum += row[j].abs();
        }
        if sum > spec.brain.w_rec_l1 && sum > 0.0 {
            let s = spec.brain.w_rec_l1 / sum;
            for j in 0..HIDDEN {
                q_wrec[i * HIDDEN + j] = encode_q(row[j] * s);
            }
        }
    }
    let q_bh = vec![0i32; HIDDEN];
    let mut q_a0 = vec![0i32; POLICY_LOGITS * PHI];
    let ln8 = libm::log(8.0);
    let ln4 = libm::log(4.0);
    let ln19 = libm::log(19.0);
    // thrust logits 0..2, turn 3..5, binary 6..15
    let thrust = [-ln8, 0.0, -ln8];
    let turn = [-ln8, 0.0, -ln8];
    for (k, &v) in thrust.iter().enumerate() {
        q_a0[k * PHI] = encode_q(v);
    }
    for (k, &v) in turn.iter().enumerate() {
        q_a0[(3 + k) * PHI] = encode_q(v);
    }
    for h in 6..POLICY_LOGITS {
        let logit = if h == 11 {
            // reproduce is logical channel 7 → binary index 5 → logit 6+5=11
            -ln4
        } else {
            -ln19
        };
        q_a0[h * PHI] = encode_q(logit);
    }
    let q_v0 = vec![0i32; PHI];
    let mut g = Genome {
        hash: [0; 32],
        q_win,
        q_wrec,
        q_bh,
        q_a0,
        q_v0,
    };
    g.recompute_hash();
    Ok(g)
}

pub fn decode_win(g: &Genome) -> [[f64; INPUTS]; HIDDEN] {
    let mut w = [[0.0; INPUTS]; HIDDEN];
    for i in 0..HIDDEN {
        for j in 0..INPUTS {
            w[i][j] = decode_q(g.q_win[i * INPUTS + j]);
        }
    }
    w
}

pub fn decode_wrec(g: &Genome) -> [[f64; HIDDEN]; HIDDEN] {
    let mut w = [[0.0; HIDDEN]; HIDDEN];
    for i in 0..HIDDEN {
        for j in 0..HIDDEN {
            w[i][j] = decode_q(g.q_wrec[i * HIDDEN + j]);
        }
        let mut s = 0.0;
        for j in 0..HIDDEN {
            s += w[i][j].abs();
        }
        if s > 0.9 {
            let k = 0.9 / s;
            for j in 0..HIDDEN {
                w[i][j] *= k;
            }
        }
    }
    w
}

pub fn decode_bh(g: &Genome) -> [f64; HIDDEN] {
    let mut b = [0.0; HIDDEN];
    for i in 0..HIDDEN {
        b[i] = decode_q(g.q_bh[i]);
    }
    b
}

pub fn decode_a0(g: &Genome) -> [[f64; PHI]; POLICY_LOGITS] {
    let mut a = [[0.0; PHI]; POLICY_LOGITS];
    for i in 0..POLICY_LOGITS {
        for j in 0..PHI {
            a[i][j] = decode_q(g.q_a0[i * PHI + j]);
        }
        let mut n2 = 0.0;
        for j in 0..PHI {
            n2 += a[i][j] * a[i][j];
        }
        let n = libm::sqrt(n2);
        if n > 24.0 {
            let s = 24.0 / n;
            for j in 0..PHI {
                a[i][j] *= s;
            }
        }
    }
    a
}

pub fn decode_v0(g: &Genome) -> [f64; PHI] {
    let mut v = [0.0; PHI];
    for i in 0..PHI {
        v[i] = decode_q(g.q_v0[i]);
    }
    let mut n2 = 0.0;
    for x in v {
        n2 += x * x;
    }
    let n = libm::sqrt(n2);
    if n > 16.0 {
        let s = 16.0 / n;
        for x in v.iter_mut() {
            *x *= s;
        }
    }
    v
}
