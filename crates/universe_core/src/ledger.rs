//! Atomic reaction extents. U03 is the only writer of mass/energy accounts.

use universe_types::{NumericFault, ReactionRules};

#[derive(Debug, Clone, Copy, Default)]
pub struct CellView {
    pub n: u64,
    pub o: u64,
    pub s: u64,
    pub q: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BodyView {
    pub n: u64,
    pub c: u64,
    pub b: u64,
    pub e_old: u64,
    pub e_new: u64,
    pub n_max: u64,
    pub c_max: u64,
    pub e_max: u64,
    pub b_target: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LedgerDelta {
    pub dissipation: u64,
}

fn sub(acc: &mut u64, x: u64) -> Result<(), NumericFault> {
    *acc = acc.checked_sub(x).ok_or(NumericFault::NegativeAccount)?;
    Ok(())
}

fn add(acc: &mut u64, x: u64) -> Result<(), NumericFault> {
    *acc = acc.checked_add(x).ok_or(NumericFault::Overflow("ledger_add"))?;
    Ok(())
}

/// Photosynthesis: N:1 + Q:20 → C:1 + dissipate 10. Uses current self N and local Q.
pub fn photosynth(
    body: &mut BodyView,
    cell: &mut CellView,
    extent: u64,
    rules: &ReactionRules,
) -> Result<LedgerDelta, NumericFault> {
    let n_in = extent;
    let q_in = extent
        .checked_mul(rules.photo_q_per)
        .ok_or(NumericFault::Overflow("photo_q"))?;
    let diss = extent
        .checked_mul(rules.photo_dissipate)
        .ok_or(NumericFault::Overflow("photo_d"))?;
    sub(&mut body.n, n_in)?;
    sub(&mut cell.q, q_in)?;
    add(&mut body.c, extent)?;
    if body.c > body.c_max {
        return Err(NumericFault::Overflow("c_max"));
    }
    Ok(LedgerDelta { dissipation: diss })
}

/// O uptake: env O → self C. Chemical energy moves with mass; no extra E.
pub fn o_uptake(body: &mut BodyView, cell: &mut CellView, extent: u64) -> Result<LedgerDelta, NumericFault> {
    sub(&mut cell.o, extent)?;
    add(&mut body.c, extent)?;
    if body.c > body.c_max {
        return Err(NumericFault::Overflow("c_max"));
    }
    Ok(LedgerDelta { dissipation: 0 })
}

/// N uptake: env N → self N.
pub fn n_uptake(body: &mut BodyView, cell: &mut CellView, extent: u64) -> Result<LedgerDelta, NumericFault> {
    sub(&mut cell.n, extent)?;
    add(&mut body.n, extent)?;
    if body.n > body.n_max {
        return Err(NumericFault::Overflow("n_max"));
    }
    Ok(LedgerDelta { dissipation: 0 })
}

/// Catabolism: C:1 → local N:1 + E_new:8 + dissipate 2. New E is not spendable this tick.
pub fn catabolism(
    body: &mut BodyView,
    cell: &mut CellView,
    extent: u64,
    rules: &ReactionRules,
) -> Result<LedgerDelta, NumericFault> {
    let e_new = extent
        .checked_mul(rules.metab_e_new)
        .ok_or(NumericFault::Overflow("metab_e"))?;
    let diss = extent
        .checked_mul(rules.metab_dissipate)
        .ok_or(NumericFault::Overflow("metab_d"))?;
    sub(&mut body.c, extent)?;
    add(&mut cell.n, extent)?;
    add(&mut body.e_new, e_new)?;
    if body.e_old.saturating_add(body.e_new) > body.e_max {
        return Err(NumericFault::Overflow("e_max"));
    }
    Ok(LedgerDelta { dissipation: diss })
}

/// Repair spends E_old only.
pub fn repair(
    body: &mut BodyView,
    extent: u64,
    rules: &ReactionRules,
) -> Result<LedgerDelta, NumericFault> {
    let e = extent
        .checked_mul(rules.repair_e_old)
        .ok_or(NumericFault::Overflow("repair_e"))?;
    let diss = extent
        .checked_mul(rules.repair_dissipate)
        .ok_or(NumericFault::Overflow("repair_d"))?;
    let room = body.b_target.saturating_sub(body.b);
    if extent > room {
        return Err(NumericFault::Overflow("repair_over_target"));
    }
    sub(&mut body.c, extent)?;
    sub(&mut body.e_old, e)?;
    add(&mut body.b, extent)?;
    Ok(LedgerDelta { dissipation: diss })
}

/// Spend old E into dissipation (maintenance, motion, cognition).
pub fn spend_old_energy(body: &mut BodyView, amount: u64) -> Result<LedgerDelta, NumericFault> {
    sub(&mut body.e_old, amount)?;
    Ok(LedgerDelta { dissipation: amount })
}

pub fn commit_e_new_into_old(body: &mut BodyView) -> Result<(), NumericFault> {
    add(&mut body.e_old, body.e_new)?;
    body.e_new = 0;
    if body.e_old > body.e_max {
        return Err(NumericFault::Overflow("e_max_commit"));
    }
    Ok(())
}

/// Transfer C: giver C+E_old → receiver C + dissipate. For T06 unit tests.
pub fn transfer_c(
    giver: &mut BodyView,
    receiver: &mut BodyView,
    extent: u64,
    rules: &ReactionRules,
) -> Result<LedgerDelta, NumericFault> {
    let e = extent
        .checked_mul(rules.transfer_e_old)
        .ok_or(NumericFault::Overflow("xfer_e"))?;
    let diss = extent
        .checked_mul(rules.transfer_dissipate)
        .ok_or(NumericFault::Overflow("xfer_d"))?;
    sub(&mut giver.c, extent)?;
    sub(&mut giver.e_old, e)?;
    add(&mut receiver.c, extent)?;
    if receiver.c > receiver.c_max {
        return Err(NumericFault::Overflow("xfer_c_max"));
    }
    Ok(LedgerDelta { dissipation: diss })
}
