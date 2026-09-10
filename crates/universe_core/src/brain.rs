//! Recurrent features and online actor–critic readouts (ch. 37).

use crate::genome::{decode_a0, decode_bh, decode_v0, decode_win, decode_wrec};
use crate::rng::random_unit;
use universe_types::{
    BrainState, Genome, NumericFault, Organism, RngDomain, RuleSpec, SampledIntent, HIDDEN, INPUTS,
    PHI, POLICY_LOGITS,
};

pub fn tanh_vec(z: f64) -> f64 {
    libm::tanh(z)
}

pub fn recurrent_features(
    obs: &[f64; INPUTS],
    h: &[f64; HIDDEN],
    w_in: &[[f64; INPUTS]; HIDDEN],
    w_rec: &[[f64; HIDDEN]; HIDDEN],
    b_h: &[f64; HIDDEN],
) -> ([f64; HIDDEN], [f64; PHI]) {
    let mut h_next = [0.0f64; HIDDEN];
    let s = libm::sqrt(32.0);
    for i in 0..HIDDEN {
        let mut z = b_h[i];
        for j in 0..INPUTS {
            z += w_in[i][j] * obs[j];
        }
        for j in 0..HIDDEN {
            z += w_rec[i][j] * h[j];
        }
        h_next[i] = 0.75 * h[i] + 0.25 * tanh_vec(z);
    }
    let mut phi = [0.0f64; PHI];
    phi[0] = 1.0;
    for i in 0..HIDDEN {
        phi[i + 1] = h_next[i] / s;
    }
    (h_next, phi)
}

fn require_finite(x: f64, _tag: &'static str) -> Result<(), NumericFault> {
    if x.is_finite() {
        Ok(())
    } else {
        Err(NumericFault::NonFinite)
    }
}

fn logits(a: &[[f64; PHI]; POLICY_LOGITS], phi: &[f64; PHI]) -> Result<[f64; POLICY_LOGITS], NumericFault> {
    for row in a {
        for &v in row {
            require_finite(v, "A")?;
        }
    }
    for &v in phi {
        require_finite(v, "phi")?;
    }
    let mut y = [0.0; POLICY_LOGITS];
    for i in 0..POLICY_LOGITS {
        let mut s = 0.0;
        for j in 0..PHI {
            s += a[i][j] * phi[j];
        }
        y[i] = s;
    }
    Ok(y)
}

pub fn value(v: &[f64; PHI], phi: &[f64; PHI]) -> f64 {
    let mut s = 0.0;
    for i in 0..PHI {
        s += v[i] * phi[i];
    }
    s
}

fn softmax3(z: [f64; 3]) -> [f64; 3] {
    let m = z[0].max(z[1]).max(z[2]);
    let e0 = libm::exp(z[0] - m);
    let e1 = libm::exp(z[1] - m);
    let e2 = libm::exp(z[2] - m);
    let s = e0 + e1 + e2;
    [e0 / s, e1 / s, e2 / s]
}

/// Legal class masks may be −∞. NaN or +∞ in a participating logit is a fault.
fn softmax3_checked(z: [f64; 3]) -> Result<[f64; 3], NumericFault> {
    if z.iter().any(|x| x.is_nan() || *x == f64::INFINITY) {
        return Err(NumericFault::NonFinite);
    }
    if !z.iter().any(|x| x.is_finite()) {
        return Err(NumericFault::NonFinite);
    }
    let p = softmax3(z);
    if p.iter().any(|x| !x.is_finite()) {
        return Err(NumericFault::NonFinite);
    }
    Ok(p)
}

fn sigmoid(z: f64) -> f64 {
    1.0 / (1.0 + libm::exp(-z))
}

fn sample_cat3(p: [f64; 3], u: f64) -> usize {
    let mut c = 0.0;
    for i in 0..2 {
        c += p[i];
        if c > u {
            return i;
        }
    }
    2
}

fn project_l2(x: &mut [f64], radius: f64) {
    let mut n2 = 0.0;
    for &v in x.iter() {
        n2 += v * v;
    }
    let n = libm::sqrt(n2);
    if n > radius && n > 0.0 {
        let s = radius / n;
        for v in x.iter_mut() {
            *v *= s;
        }
    }
}

fn frobenius(a: &[[f64; PHI]; POLICY_LOGITS]) -> f64 {
    let mut n2 = 0.0;
    for row in a {
        for &v in row {
            n2 += v * v;
        }
    }
    libm::sqrt(n2)
}

fn project_frob(a: &mut [[f64; PHI]; POLICY_LOGITS], radius: f64) {
    let n = frobenius(a);
    if n > radius && n > 0.0 {
        let s = radius / n;
        for row in a.iter_mut() {
            for v in row.iter_mut() {
                *v *= s;
            }
        }
    }
}

fn project_rows(a: &mut [[f64; PHI]; POLICY_LOGITS], radius: f64) {
    for row in a.iter_mut() {
        project_l2(row, radius);
    }
}

/// Bernoulli log-prob gradient wrt logit is (a-p); times phi for parameters.
pub fn bernoulli_grad_logit(a: f64, p: f64) -> f64 {
    a - p
}

pub fn entropy_grad_bernoulli(p: f64) -> f64 {
    if p <= 0.0 || p >= 1.0 {
        return 0.0;
    }
    p * (1.0 - p) * libm::log((1.0 - p) / p)
}

pub fn entropy_cat(p: [f64; 3]) -> f64 {
    let mut h = 0.0;
    for pi in p {
        if pi > 0.0 {
            h -= pi * libm::log(pi);
        }
    }
    h
}

pub fn entropy_grad_cat(p: [f64; 3]) -> [f64; 3] {
    let h = entropy_cat(p);
    let mut g = [0.0; 3];
    for j in 0..3 {
        if p[j] > 0.0 {
            g[j] = -p[j] * (libm::log(p[j]) + h);
        }
    }
    g
}

#[derive(Clone, Copy)]
pub struct HeadSample {
    pub thrust: i8,
    pub turn: i8,
    pub binary: [bool; 10],
    pub p_thr: [f64; 3],
    pub p_turn: [f64; 3],
    pub p_bin: [f64; 10],
    pub a_thr: usize,
    pub a_turn: usize,
}

fn mask_policy(z: &mut [f64; POLICY_LOGITS], allow_repro: bool) {
    // last four logical channels: signal, transfer, attack, adhere → logits 12..15
    for i in 12..16 {
        z[i] = f64::NEG_INFINITY;
    }
    if !allow_repro {
        z[11] = f64::NEG_INFINITY;
    }
}

pub fn sample_policy(
    a: &[[f64; PHI]; POLICY_LOGITS],
    phi: &[f64; PHI],
    root_seed: u64,
    tick: u64,
    entity: u64,
    allow_repro: bool,
) -> Result<HeadSample, NumericFault> {
    sample_policy_masked(a, phi, root_seed, tick, entity, allow_repro, false)
}

/// `mask_idle_thrust` zeros the stationary thrust class (ch. 42.4 fixture).
pub fn sample_policy_masked(
    a: &[[f64; PHI]; POLICY_LOGITS],
    phi: &[f64; PHI],
    root_seed: u64,
    tick: u64,
    entity: u64,
    allow_repro: bool,
    mask_idle_thrust: bool,
) -> Result<HeadSample, NumericFault> {
    let mut z = logits(a, phi)?;
    for (i, &zi) in z.iter().enumerate() {
        let legally_masked = i >= 12 || (i == 11 && !allow_repro) || (i == 1 && mask_idle_thrust);
        if !legally_masked && !zi.is_finite() {
            return Err(NumericFault::NonFinite);
        }
    }
    mask_policy(&mut z, allow_repro);
    if mask_idle_thrust {
        z[1] = f64::NEG_INFINITY;
    }
    let p_thr = softmax3_checked([z[0], z[1], z[2]])?;
    let p_turn = softmax3_checked([z[3], z[4], z[5]])?;
    let u0 = random_unit(root_seed, RngDomain::PolicySample.code(), tick, entity, 0, 0)?;
    let u1 = random_unit(root_seed, RngDomain::PolicySample.code(), tick, entity, 0, 1)?;
    let a_thr = sample_cat3(p_thr, u0);
    let a_turn = sample_cat3(p_turn, u1);
    let mut binary = [false; 10];
    let mut p_bin = [0.0; 10];
    for h in 0..10 {
        let logit_i = 6 + h;
        let legally_masked = h >= 6 || (h == 5 && !allow_repro);
        let u = random_unit(
            root_seed,
            RngDomain::PolicySample.code(),
            tick,
            entity,
            0,
            2 + h as u64,
        )?;
        if legally_masked {
            p_bin[h] = 0.0;
            binary[h] = false;
            continue;
        }
        if !z[logit_i].is_finite() {
            return Err(NumericFault::NonFinite);
        }
        let p = sigmoid(z[logit_i]);
        if !p.is_finite() {
            return Err(NumericFault::NonFinite);
        }
        p_bin[h] = p;
        binary[h] = u < p;
    }
    Ok(HeadSample {
        thrust: a_thr as i8 - 1,
        turn: a_turn as i8 - 1,
        binary,
        p_thr,
        p_turn,
        p_bin,
        a_thr,
        a_turn,
    })
}

pub fn store_grads(brain: &mut BrainState, phi: &[f64; PHI], hs: &HeadSample, allow_repro: bool) {
    brain.grad_log = [[0.0; PHI]; POLICY_LOGITS];
    brain.grad_ent = [[0.0; PHI]; POLICY_LOGITS];
    let g_thr = [
        (if hs.a_thr == 0 { 1.0 } else { 0.0 }) - hs.p_thr[0],
        (if hs.a_thr == 1 { 1.0 } else { 0.0 }) - hs.p_thr[1],
        (if hs.a_thr == 2 { 1.0 } else { 0.0 }) - hs.p_thr[2],
    ];
    let ge_thr = entropy_grad_cat(hs.p_thr);
    let g_turn = [
        (if hs.a_turn == 0 { 1.0 } else { 0.0 }) - hs.p_turn[0],
        (if hs.a_turn == 1 { 1.0 } else { 0.0 }) - hs.p_turn[1],
        (if hs.a_turn == 2 { 1.0 } else { 0.0 }) - hs.p_turn[2],
    ];
    let ge_turn = entropy_grad_cat(hs.p_turn);
    for j in 0..3 {
        for k in 0..PHI {
            brain.grad_log[j][k] = g_thr[j] * phi[k];
            brain.grad_ent[j][k] = ge_thr[j] * phi[k];
            brain.grad_log[3 + j][k] = g_turn[j] * phi[k];
            brain.grad_ent[3 + j][k] = ge_turn[j] * phi[k];
        }
    }
    for h in 0..10 {
        let masked = h >= 6 || (h == 5 && !allow_repro); // binary 6..9 are social; 5 is reproduce
        // binary heads: 0 Nboost,1 O,2 photo,3 metab,4 repair,5 reproduce,6 signal,7 transfer,8 attack,9 adhere
        let social = h >= 6;
        if social || (h == 5 && !allow_repro) {
            continue;
        }
        let a = if hs.binary[h] { 1.0 } else { 0.0 };
        let p = hs.p_bin[h];
        let gl = bernoulli_grad_logit(a, p);
        let ge = entropy_grad_bernoulli(p);
        for k in 0..PHI {
            brain.grad_log[6 + h][k] = gl * phi[k];
            brain.grad_ent[6 + h][k] = ge * phi[k];
        }
        let _ = masked;
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UpdateStats {
    pub delta: f64,
    pub delta_used: f64,
    pub clipped: bool,
    pub v_prev: f64,
    pub v_next: f64,
    /// ‖actor_lr · δ_used · e_A‖_F after traces, coefficients included.
    pub actor_task_gradient_norm: Option<f64>,
    /// ‖actor_lr · entropy_coef · ∇H‖_F
    pub entropy_term_norm: Option<f64>,
    /// ‖actor_lr · baseline_coef · (A−A0)‖_F
    pub baseline_term_norm: Option<f64>,
    pub trace_norm_before_projection: Option<f64>,
    pub trace_norm_after_projection: Option<f64>,
    pub update_norm_before_projection: Option<f64>,
    pub update_norm_after_projection: Option<f64>,
}

pub fn apply_readout_update(
    brain: &mut BrainState,
    a0: &[[f64; PHI]; POLICY_LOGITS],
    phi_next: &[f64; PHI],
    spec: &RuleSpec,
) -> Result<UpdateStats, NumericFault> {
    for &x in phi_next {
        require_finite(x, "phi_next")?;
    }
    let v_prev = value(&brain.v, &brain.phi_prev);
    let v_next = value(&brain.v, phi_next);
    require_finite(v_prev, "v_prev")?;
    require_finite(v_next, "v_next")?;
    require_finite(brain.r, "R")?;
    require_finite(brain.gamma, "gamma")?;
    let delta = brain.r + brain.gamma * v_next - v_prev;
    require_finite(delta, "delta")?;
    brain.last_delta = delta;
    let du = delta.clamp(-spec.brain.delta_clip, spec.brain.delta_clip);
    for i in 0..POLICY_LOGITS {
        for j in 0..PHI {
            brain.e_a[i][j] = brain.trace_decay * brain.e_a[i][j] + brain.grad_log[i][j];
            require_finite(brain.e_a[i][j], "e_a")?;
        }
    }
    for j in 0..PHI {
        brain.e_v[j] = brain.trace_decay * brain.e_v[j] + brain.phi_prev[j];
        require_finite(brain.e_v[j], "e_v")?;
    }
    let trace_before = frobenius(&brain.e_a);
    project_frob(&mut brain.e_a, spec.brain.e_a_radius);
    project_l2(&mut brain.e_v, spec.brain.e_v_radius);
    let trace_after = frobenius(&brain.e_a);
    let mut task = [[0.0; PHI]; POLICY_LOGITS];
    let mut ent = [[0.0; PHI]; POLICY_LOGITS];
    let mut base = [[0.0; PHI]; POLICY_LOGITS];
    let mut da = [[0.0; PHI]; POLICY_LOGITS];
    for i in 0..POLICY_LOGITS {
        for j in 0..PHI {
            task[i][j] = spec.brain.actor_lr * du * brain.e_a[i][j];
            ent[i][j] = spec.brain.actor_lr * spec.brain.entropy_coef * brain.grad_ent[i][j];
            base[i][j] = spec.brain.actor_lr * spec.brain.baseline_coef * (brain.a[i][j] - a0[i][j]);
            da[i][j] = task[i][j] + ent[i][j] - base[i][j];
            require_finite(da[i][j], "da")?;
        }
    }
    let mut dv = [0.0; PHI];
    for j in 0..PHI {
        dv[j] = spec.brain.critic_lr * du * brain.e_v[j];
        require_finite(dv[j], "dv")?;
    }
    let update_before = frobenius(&da);
    project_frob(&mut da, spec.brain.d_a_radius);
    project_l2(&mut dv, spec.brain.d_v_radius);
    let update_after = frobenius(&da);
    for i in 0..POLICY_LOGITS {
        for j in 0..PHI {
            brain.a[i][j] += da[i][j];
            require_finite(brain.a[i][j], "A")?;
        }
    }
    for j in 0..PHI {
        brain.v[j] += dv[j];
        require_finite(brain.v[j], "v")?;
    }
    project_rows(&mut brain.a, spec.brain.a_row_radius);
    project_l2(&mut brain.v, spec.brain.v_radius);
    Ok(UpdateStats {
        delta,
        delta_used: du,
        clipped: delta.abs() > spec.brain.delta_clip && delta.is_finite(),
        v_prev,
        v_next,
        actor_task_gradient_norm: Some(frobenius(&task)),
        entropy_term_norm: Some(frobenius(&ent)),
        baseline_term_norm: Some(frobenius(&base)),
        trace_norm_before_projection: Some(trace_before),
        trace_norm_after_projection: Some(trace_after),
        update_norm_before_projection: Some(update_before),
        update_norm_after_projection: Some(update_after),
    })
}

pub fn tick_reward(org: &Organism, spec: &RuleSpec, births: u64, died: bool) -> f64 {
    let e = org.accounts.e_old as f64 + org.accounts.e_new as f64;
    let short_e = (1.0 - e / spec.brain.reward_e_target as f64).max(0.0);
    let short_c = (1.0 - org.accounts.c as f64 / spec.brain.reward_c_target as f64).max(0.0);
    let damage = (1.0 - org.accounts.b as f64 / org.b_target as f64).max(0.0);
    let debt = (org.maintenance_debt as f64 / spec.physiology.death_debt as f64).min(1.0);
    let hdev = short_e * short_e + short_c * short_c + 2.0 * damage * damage + debt;
    spec.brain.reward_bias - spec.brain.reward_hdev * hdev
        + spec.brain.reward_birth * births as f64
        - if died { spec.brain.reward_death } else { 0.0 }
}

pub fn accumulate_interval(brain: &mut BrainState, reward: f64, spec: &RuleSpec) {
    brain.r += brain.gamma * reward;
    brain.gamma *= spec.brain.gamma_tick;
    brain.trace_decay *= spec.brain.gamma_tick * spec.brain.lambda_trace;
    brain.interval_d += 1;
}

pub fn reset_interval(brain: &mut BrainState) {
    brain.r = 0.0;
    brain.gamma = 1.0;
    brain.trace_decay = 1.0;
    brain.interval_d = 0;
}

pub fn decide(
    spec: &RuleSpec,
    org: &mut Organism,
    genome: &Genome,
    obs: &[f64; INPUTS],
    root_seed: u64,
    tick: u64,
    freeze: bool,
    allow_repro: bool,
) -> Result<crate::probe::DecisionDiag, NumericFault> {
    let w_in = decode_win(genome);
    let w_rec = decode_wrec(genome);
    let b_h = decode_bh(genome);
    let a0 = decode_a0(genome);
    let interval_d = org.brain.interval_d;
    let r = org.brain.r;
    let h_before = org.brain.h;
    let a_before = org.brain.a;
    let applied = org.brain.has_pending && !freeze;
    let (h_next, phi_next) = recurrent_features(obs, &org.brain.h, &w_in, &w_rec, &b_h);
    for &x in obs {
        require_finite(x, "obs")?;
    }
    for &x in &h_next {
        require_finite(x, "h")?;
    }
    let stats = if applied {
        apply_readout_update(&mut org.brain, &a0, &phi_next, spec)?
    } else {
        UpdateStats::default()
    };
    org.brain.h = h_next;
    let hs = sample_policy(
        &org.brain.a,
        &phi_next,
        root_seed,
        tick,
        org.entity_id,
        allow_repro,
    )?;
    store_grads(&mut org.brain, &phi_next, &hs, allow_repro);
    org.brain.phi_prev = phi_next;
    org.brain.sampled = SampledIntent {
        thrust: hs.thrust,
        turn: hs.turn,
        binary: hs.binary,
        valid: true,
    };
    org.intent = org.brain.sampled;
    org.control.thrust = hs.thrust;
    org.control.turn = hs.turn;
    org.control.n_boost = hs.binary[0];
    org.control.o_uptake = hs.binary[1];
    org.control.photo_boost = hs.binary[2];
    org.control.metab_boost = hs.binary[3];
    org.control.repair = hs.binary[4];
    org.control.reproduce = hs.binary[5] && allow_repro;
    org.brain.has_pending = true;
    reset_interval(&mut org.brain);

    let mut h_l2 = 0.0;
    for i in 0..HIDDEN {
        let d = h_next[i] - h_before[i];
        h_l2 += d * d;
    }
    let mut da_bias = 0.0;
    let mut da_feat = 0.0;
    let mut a_minus_a0 = 0.0;
    let mut trace_a = 0.0;
    for i in 0..POLICY_LOGITS {
        let db = org.brain.a[i][0] - a_before[i][0];
        da_bias += db.abs();
        for j in 0..PHI {
            let d = org.brain.a[i][j] - a_before[i][j];
            if j > 0 {
                da_feat += d.abs();
            }
            let aa = org.brain.a[i][j] - a0[i][j];
            a_minus_a0 += aa * aa;
            trace_a += org.brain.e_a[i][j] * org.brain.e_a[i][j];
        }
    }
    let mut v_norm = 0.0;
    for j in 0..PHI {
        v_norm += org.brain.v[j] * org.brain.v[j];
    }
    let mut entropy = entropy_cat(hs.p_thr) + entropy_cat(hs.p_turn);
    for h in 0..10 {
        let p = hs.p_bin[h];
        if p > 0.0 && p < 1.0 {
            entropy -= p * libm::log(p) + (1.0 - p) * libm::log(1.0 - p);
        }
    }
    Ok(crate::probe::DecisionDiag {
        tick,
        entity_id: org.entity_id,
        applied_update: applied,
        interval_d,
        r,
        delta: stats.delta,
        delta_used: stats.delta_used,
        clipped: stats.clipped,
        entropy,
        a_minus_a0_frob: libm::sqrt(a_minus_a0),
        trace_a_frob: libm::sqrt(trace_a),
        h_l2_delta: libm::sqrt(h_l2),
        input_change: 0.0,
        da_bias_abs: da_bias,
        da_feat_abs: da_feat,
        v_prev: stats.v_prev,
        v_next: stats.v_next,
        v_norm: libm::sqrt(v_norm),
        intent_thrust: hs.thrust,
        intent_turn: hs.turn,
        binary_intent: hs.binary,
        p_thr: hs.p_thr,
        p_turn: hs.p_turn,
        p_bin: hs.p_bin,
        paid_thrust: org.control.thrust,
        paid_turn: org.control.turn,
        h_before,
        h_after: h_next,
        phi: phi_next,
        actor_task_gradient_norm: stats.actor_task_gradient_norm,
        entropy_term_norm: stats.entropy_term_norm,
        baseline_term_norm: stats.baseline_term_norm,
        trace_norm_before_projection: stats.trace_norm_before_projection,
        trace_norm_after_projection: stats.trace_norm_after_projection,
        update_norm_before_projection: stats.update_norm_before_projection,
        update_norm_after_projection: stats.update_norm_after_projection,
        ..crate::probe::DecisionDiag::default()
    })
}

pub fn install_genome_weights(org: &mut Organism, g: &Genome) {
    org.brain.a = decode_a0(g);
    org.brain.v = decode_v0(g);
    org.genome_hash = g.hash;
}
