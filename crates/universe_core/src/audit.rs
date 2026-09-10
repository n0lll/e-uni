//! Independent mass/energy totals. Must not share code with the debit path used to mutate accounts.

use universe_types::{checked_add_u128, checked_mul_u128, NumericFault, WorldState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MuTotals {
    pub m: u128,
    pub u: u128,
}

pub fn compute_mu(state: &WorldState, chem_e: u64) -> Result<MuTotals, NumericFault> {
    let mut m: u128 = 0;
    let mut organic: u128 = 0;
    let mut qe: u128 = 0;

    for &v in &state.fields.n {
        m = checked_add_u128(m, u128::from(v), "env.n")?;
    }
    for &v in &state.fields.o {
        m = checked_add_u128(m, u128::from(v), "env.o")?;
        organic = checked_add_u128(organic, u128::from(v), "env.o.chem")?;
    }
    for &v in &state.fields.s {
        m = checked_add_u128(m, u128::from(v), "env.s")?;
    }
    for &v in &state.fields.q {
        qe = checked_add_u128(qe, u128::from(v), "env.q")?;
    }

    let mut living_e: u128 = 0;
    for o in &state.organisms {
        m = checked_add_u128(m, u128::from(o.accounts.n), "org.n")?;
        m = checked_add_u128(m, u128::from(o.accounts.c), "org.c")?;
        m = checked_add_u128(m, u128::from(o.accounts.b), "org.b")?;
        organic = checked_add_u128(organic, u128::from(o.accounts.c), "org.c.chem")?;
        organic = checked_add_u128(organic, u128::from(o.accounts.b), "org.b.chem")?;
        living_e = checked_add_u128(living_e, u128::from(o.accounts.e_old), "org.e_old")?;
        living_e = checked_add_u128(living_e, u128::from(o.accounts.e_new), "org.e_new")?;
    }
    for e in &state.embryos {
        m = checked_add_u128(m, u128::from(e.b), "emb.b")?;
        m = checked_add_u128(m, u128::from(e.c), "emb.c")?;
        organic = checked_add_u128(organic, u128::from(e.b), "emb.b.chem")?;
        organic = checked_add_u128(organic, u128::from(e.c), "emb.c.chem")?;
        living_e = checked_add_u128(living_e, u128::from(e.e), "emb.e")?;
    }

    let chem = checked_mul_u128(organic, u128::from(chem_e), "chem_e")?;
    let u = checked_add_u128(checked_add_u128(qe, living_e, "u.qe_e")?, chem, "u.chem")?;
    Ok(MuTotals { m, u })
}

pub fn energy_identity_holds(
    u_old: u128,
    u_next: u128,
    dissipation_step: u128,
    radiation_loss_step: u128,
    radiation_input_step: u128,
) -> Result<bool, NumericFault> {
    let left = checked_add_u128(
        checked_add_u128(u_next, dissipation_step, "id.left1")?,
        radiation_loss_step,
        "id.left2",
    )?;
    let right = checked_add_u128(u_old, radiation_input_step, "id.right")?;
    Ok(left == right)
}
