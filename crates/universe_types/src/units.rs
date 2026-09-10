//! Model units. Resource arithmetic uses micro-units only.

pub const MICRO_PER_UNIT: u64 = 1_000_000;
pub const CHEM_E_PER_MU: u64 = 10;

pub type MicroMass = u64;
pub type MicroEnergy = u64;

#[inline]
pub const fn mu_to_micro(mu: u64) -> MicroMass {
    mu.saturating_mul(MICRO_PER_UNIT)
}

#[inline]
pub const fn eu_to_micro(eu: u64) -> MicroEnergy {
    eu.saturating_mul(MICRO_PER_UNIT)
}

#[inline]
pub fn checked_mul_u128(a: u128, b: u128, what: &'static str) -> Result<u128, crate::error::NumericFault> {
    a.checked_mul(b)
        .ok_or(crate::error::NumericFault::Overflow(what))
}

#[inline]
pub fn checked_add_u128(a: u128, b: u128, what: &'static str) -> Result<u128, crate::error::NumericFault> {
    a.checked_add(b)
        .ok_or(crate::error::NumericFault::Overflow(what))
}

#[inline]
pub fn sum_u64_to_u128(xs: impl IntoIterator<Item = u64>) -> Result<u128, crate::error::NumericFault> {
    let mut acc: u128 = 0;
    for x in xs {
        acc = checked_add_u128(acc, u128::from(x), "sum_u64")?;
    }
    Ok(acc)
}

/// T01 expected closed-world initial totals for EU-Core-1.
pub const T01_INITIAL_M_MICRO: u128 = 8_396_800_000;
pub const T01_INITIAL_U_MICRO: u128 = 2_585_600_000;
