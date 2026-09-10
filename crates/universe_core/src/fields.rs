//! Integer four-neighbor transport and S/Q remainder decay (ch. 34.5).

use universe_types::{Fields, NumericFault, RuleSpec};

pub fn signed_flux(xi: u64, xj: u64, divisor: u64) -> Result<i128, NumericFault> {
    if divisor == 0 {
        return Err(NumericFault::Overflow("transport_divisor"));
    }
    if xi == xj {
        return Ok(0);
    }
    let (hi, lo) = if xi > xj { (xi, xj) } else { (xj, xi) };
    let mag = (hi - lo) / divisor;
    if xi > xj {
        Ok(i128::from(mag))
    } else {
        Ok(-i128::from(mag))
    }
}

/// Right and up undirected edges only. Deltas computed from a frozen snapshot.
pub fn transport_deltas(
    field: &[u64],
    width: u32,
    height: u32,
    divisor: u64,
) -> Result<Vec<i128>, NumericFault> {
    let w = width as usize;
    let h = height as usize;
    let n = w * h;
    if field.len() != n {
        return Err(NumericFault::Overflow("field_len"));
    }
    let mut delta = vec![0i128; n];
    for y in 0..h {
        for x in 0..(w.saturating_sub(1)) {
            let i = y * w + x;
            let j = i + 1;
            let flux = signed_flux(field[i], field[j], divisor)?;
            delta[i] -= flux;
            delta[j] += flux;
        }
    }
    for y in 0..(h.saturating_sub(1)) {
        for x in 0..w {
            let i = y * w + x;
            let j = i + w;
            let flux = signed_flux(field[i], field[j], divisor)?;
            delta[i] -= flux;
            delta[j] += flux;
        }
    }
    Ok(delta)
}

fn apply_delta(field: &mut [u64], delta: &[i128]) -> Result<(), NumericFault> {
    for (slot, d) in field.iter_mut().zip(delta.iter()) {
        let next = i128::from(*slot) + *d;
        if next < 0 {
            return Err(NumericFault::NegativeAccount);
        }
        if next > i128::from(u64::MAX) {
            return Err(NumericFault::Overflow("field_cell"));
        }
        *slot = next as u64;
    }
    Ok(())
}

pub fn apply_radiation_input(fields: &mut Fields, spec: &RuleSpec) -> Result<u64, NumericFault> {
    let mut added = 0u64;
    let inc = spec.lighting.radiation_input_per_cell;
    for y in 0..fields.height {
        for x in 0..fields.width {
            if spec.lighting.is_lit(x, y) {
                let i = fields.idx(x, y);
                fields.q[i] = fields.q[i]
                    .checked_add(inc)
                    .ok_or(NumericFault::Overflow("q_input"))?;
                added = added
                    .checked_add(inc)
                    .ok_or(NumericFault::Overflow("q_input_sum"))?;
            }
        }
    }
    Ok(added)
}

pub fn transport_all_fields(fields: &mut Fields, spec: &RuleSpec) -> Result<(), NumericFault> {
    let w = fields.width;
    let h = fields.height;
    let dn = transport_deltas(&fields.n, w, h, spec.world.n_divisor)?;
    let d_o = transport_deltas(&fields.o, w, h, spec.world.o_divisor)?;
    let ds = transport_deltas(&fields.s, w, h, spec.world.s_divisor)?;
    let dq = transport_deltas(&fields.q, w, h, spec.world.q_divisor)?;
    apply_delta(&mut fields.n, &dn)?;
    apply_delta(&mut fields.o, &d_o)?;
    apply_delta(&mut fields.s, &ds)?;
    apply_delta(&mut fields.q, &dq)?;
    Ok(())
}

/// After diffusion. Returns (n_from_s, q_radiation_loss).
pub fn decay_s_q(fields: &mut Fields, spec: &RuleSpec) -> Result<(u64, u64), NumericFault> {
    let n = fields.len();
    if fields.s_remainder.len() != n || fields.q_remainder.len() != n {
        return Err(NumericFault::Overflow("remainder_len"));
    }
    let sdiv = spec.world.s_decay_divisor;
    let qdiv = spec.world.q_loss_divisor;
    let mut n_gain = 0u64;
    let mut q_loss = 0u64;
    for i in 0..n {
        let s_acc = u64::from(fields.s[i]) + u64::from(fields.s_remainder[i]);
        let s_decay = s_acc / sdiv;
        fields.s_remainder[i] = (s_acc % sdiv) as u16;
        if s_decay > fields.s[i] {
            return Err(NumericFault::NegativeAccount);
        }
        fields.s[i] -= s_decay;
        fields.n[i] = fields.n[i]
            .checked_add(s_decay)
            .ok_or(NumericFault::Overflow("s_to_n"))?;
        n_gain = n_gain
            .checked_add(s_decay)
            .ok_or(NumericFault::Overflow("n_gain"))?;

        let q_acc = u64::from(fields.q[i]) + u64::from(fields.q_remainder[i]);
        let loss = q_acc / qdiv;
        fields.q_remainder[i] = (q_acc % qdiv) as u16;
        if loss > fields.q[i] {
            return Err(NumericFault::NegativeAccount);
        }
        fields.q[i] -= loss;
        q_loss = q_loss
            .checked_add(loss)
            .ok_or(NumericFault::Overflow("q_loss"))?;
    }
    Ok((n_gain, q_loss))
}

/// P5a: radiation input, four-field transport, S inactivation, Q loss.
pub fn phase_p5a(fields: &mut Fields, spec: &RuleSpec) -> Result<(u64, u64), NumericFault> {
    let input = apply_radiation_input(fields, spec)?;
    transport_all_fields(fields, spec)?;
    let (_n, loss) = decay_s_q(fields, spec)?;
    Ok((input, loss))
}
