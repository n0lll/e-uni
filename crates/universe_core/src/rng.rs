//! Philox4x32-10 and the EU-Core-1 `random_u32` counter encoding (ch. 33.2).

use universe_types::{
    NumericFault, RngDomain, ACTOR_MAX, BLOCK_MAX, OBJECT_MAX, TICK_MAX,
};

/// Random123 Philox4x32 multipliers and Weyl constants.
pub const PHILOX_M0: u32 = 0xD251_1F53;
pub const PHILOX_M1: u32 = 0xCD9E_8D57;
pub const PHILOX_W0: u32 = 0x9E37_79B9;
pub const PHILOX_W1: u32 = 0xBB67_AE85;

#[inline]
fn mulhilo32(a: u32, b: u32) -> (u32, u32) {
    let p = u64::from(a).wrapping_mul(u64::from(b));
    ((p >> 32) as u32, p as u32)
}

#[inline]
fn philox4x32_round(ctr: [u32; 4], key: [u32; 2]) -> [u32; 4] {
    let (hi0, lo0) = mulhilo32(PHILOX_M0, ctr[0]);
    let (hi1, lo1) = mulhilo32(PHILOX_M1, ctr[2]);
    [
        hi1 ^ ctr[1] ^ key[0],
        lo1,
        hi0 ^ ctr[3] ^ key[1],
        lo0,
    ]
}

/// Random123 `philox4x32_R(10, ctr, key)`.
pub fn philox4x32_10(mut ctr: [u32; 4], mut key: [u32; 2]) -> [u32; 4] {
    ctr = philox4x32_round(ctr, key);
    for _ in 1..10 {
        key[0] = key[0].wrapping_add(PHILOX_W0);
        key[1] = key[1].wrapping_add(PHILOX_W1);
        ctr = philox4x32_round(ctr, key);
    }
    ctr
}

pub fn encode_key_counter(
    root_seed: u64,
    domain: u8,
    tick: u64,
    actor: u64,
    object: u64,
    draw_index: u64,
) -> Result<([u32; 2], [u32; 4], usize), NumericFault> {
    if tick > TICK_MAX {
        return Err(NumericFault::RngFieldOverflow("tick"));
    }
    if actor > ACTOR_MAX {
        return Err(NumericFault::RngFieldOverflow("actor"));
    }
    if object > OBJECT_MAX {
        return Err(NumericFault::RngFieldOverflow("object"));
    }
    let block = draw_index / 4;
    if block > BLOCK_MAX {
        return Err(NumericFault::RngFieldOverflow("block"));
    }
    let stream = (u64::from(domain) << 24) | block;
    let key64 = root_seed ^ (stream << 32);
    let key = [key64 as u32, (key64 >> 32) as u32];
    let ctr128 = (u128::from(tick) << 88) | (u128::from(actor) << 44) | u128::from(object);
    let ctr = [
        ctr128 as u32,
        (ctr128 >> 32) as u32,
        (ctr128 >> 64) as u32,
        (ctr128 >> 96) as u32,
    ];
    let lane = (draw_index % 4) as usize;
    Ok((key, ctr, lane))
}

pub fn random_u32(
    root_seed: u64,
    domain: u8,
    tick: u64,
    actor: u64,
    object: u64,
    draw_index: u64,
) -> Result<u32, NumericFault> {
    let (key, ctr, lane) = encode_key_counter(root_seed, domain, tick, actor, object, draw_index)?;
    let words = philox4x32_10(ctr, key);
    Ok(words[lane])
}

pub fn random_u32_domain(
    root_seed: u64,
    domain: RngDomain,
    tick: u64,
    actor: u64,
    object: u64,
    draw_index: u64,
) -> Result<u32, NumericFault> {
    random_u32(root_seed, domain.code(), tick, actor, object, draw_index)
}

/// Uniform in `[0, 1)` from two u32 draws at `2*j` and `2*j+1`.
pub fn random_unit(
    root_seed: u64,
    domain: u8,
    tick: u64,
    actor: u64,
    object: u64,
    j: u64,
) -> Result<f64, NumericFault> {
    let a = random_u32(root_seed, domain, tick, actor, object, 2 * j)? >> 5;
    let b = random_u32(root_seed, domain, tick, actor, object, 2 * j + 1)? >> 6;
    let num = u64::from(a) * 67_108_864 + u64::from(b);
    Ok((num as f64) / 9_007_199_254_740_992.0)
}

/// Integer in `[0, n)` using rejection sampling. Uses 32 reserved consecutive u32 slots
/// starting at `slot_base`. Does not consume the next event's slots.
pub fn random_range(
    root_seed: u64,
    domain: u8,
    tick: u64,
    actor: u64,
    object: u64,
    slot_base: u64,
    n: u32,
) -> Result<u32, NumericFault> {
    if n == 0 {
        return Err(NumericFault::Overflow("random_range_n"));
    }
    // Keep the 2^32 comparison in u64: `(2^32 - rem) as u32` wraps to 0 when rem == 0.
    let modulus = 1u64 << 32;
    let rem = modulus % u64::from(n);
    let limit = modulus - rem;
    for k in 0..32u64 {
        let x = u64::from(random_u32(root_seed, domain, tick, actor, object, slot_base + k)?);
        if x < limit {
            return Ok((x % u64::from(n)) as u32);
        }
    }
    Err(NumericFault::RangeSampleExhausted)
}

/// Quota remainder tie-break draw_index (ch. 33.2).
pub fn quota_draw_index(
    phase_code: u32,
    pool_type: u32,
    pool_subindex: u32,
    local_request_slot: u32,
) -> Result<u64, NumericFault> {
    if phase_code >= 32 {
        return Err(NumericFault::RngFieldOverflow("phase_code"));
    }
    if pool_type >= 16 {
        return Err(NumericFault::RngFieldOverflow("pool_type"));
    }
    if pool_subindex >= 8 {
        return Err(NumericFault::RngFieldOverflow("pool_subindex"));
    }
    if local_request_slot >= 256 {
        return Err(NumericFault::RngFieldOverflow("local_request_slot"));
    }
    Ok(
        (((((u64::from(phase_code) * 16 + u64::from(pool_type)) * 8)
            + u64::from(pool_subindex))
            * 256)
            + u64::from(local_request_slot))
            * 4,
    )
}

pub fn geometry_draw_index(
    substep: u32,
    iteration: u32,
    node_a: u32,
    node_b: u32,
) -> Result<u64, NumericFault> {
    if substep >= 32 || iteration >= 32 || node_a >= 8 || node_b >= 8 {
        return Err(NumericFault::RngFieldOverflow("geometry_draw"));
    }
    Ok(
        (((((u64::from(substep) * 32 + u64::from(iteration)) * 8) + u64::from(node_a)) * 8)
            + u64::from(node_b))
            * 4,
    )
}

/// Observer/UI RNG. Independent of Philox world streams (splitmix64).
#[derive(Clone, Debug)]
pub struct ObserverRng {
    state: u64,
}

impl ObserverRng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed | 1,
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn philox_kat_zero() {
        assert_eq!(
            philox4x32_10([0, 0, 0, 0], [0, 0]),
            [0x6627_e8d5, 0xe169_c58d, 0xbc57_ac4c, 0x9b00_dbd8]
        );
    }

    #[test]
    fn philox_kat_ones() {
        assert_eq!(
            philox4x32_10([0xffff_ffff; 4], [0xffff_ffff, 0xffff_ffff]),
            [0x408f_276d, 0x41c8_3b0e, 0xa20b_c7c6, 0x6d54_51fd]
        );
    }

    #[test]
    fn philox_kat_pi() {
        assert_eq!(
            philox4x32_10(
                [0x243f_6a88, 0x85a3_08d3, 0x1319_8a2e, 0x0370_7344],
                [0xa409_3822, 0x299f_31d0]
            ),
            [0xd16c_fe09, 0x94fd_cceb, 0x5001_e420, 0x2412_6ea1]
        );
    }

    #[test]
    fn field_overflow_rejected() {
        let err = random_u32(0, 1, 1u64 << 40, 0, 0, 0).unwrap_err();
        assert_eq!(err, NumericFault::RngFieldOverflow("tick"));
        let err = random_u32(0, 1, 0, 1u64 << 44, 0, 0).unwrap_err();
        assert_eq!(err, NumericFault::RngFieldOverflow("actor"));
        let err = random_u32(0, 1, 0, 0, 1u64 << 44, 0).unwrap_err();
        assert_eq!(err, NumericFault::RngFieldOverflow("object"));
        let err = random_u32(0, 1, 0, 0, 0, 4 * (1u64 << 24)).unwrap_err();
        assert_eq!(err, NumericFault::RngFieldOverflow("block"));
    }

    #[test]
    fn encoding_matches_spec_example() {
        let root = 0x0123_4567_89ab_cdef;
        let (key, ctr, lane) = encode_key_counter(root, 3, 10, 7, 0, 5).unwrap();
        // block = 5/4 = 1, lane = 1
        assert_eq!(lane, 1);
        let stream = (3u64 << 24) | 1;
        let key64 = root ^ (stream << 32);
        assert_eq!(key[0], key64 as u32);
        assert_eq!(key[1], (key64 >> 32) as u32);
        let ctr128 = (u128::from(10u64) << 88) | (u128::from(7u64) << 44);
        assert_eq!(ctr[0], ctr128 as u32);
        assert_eq!(ctr[1], (ctr128 >> 32) as u32);
        let v = random_u32(root, 3, 10, 7, 0, 5).unwrap();
        assert_eq!(v, philox4x32_10(ctr, key)[1]);
    }

    #[test]
    fn unit_interval_in_range() {
        let u = random_unit(1, 1, 0, 1, 0, 0).unwrap();
        assert!(u >= 0.0 && u < 1.0);
        let a = random_u32(1, 1, 0, 1, 0, 0).unwrap() >> 5;
        let b = random_u32(1, 1, 0, 1, 0, 1).unwrap() >> 6;
        let expect = (u64::from(a) * 67_108_864 + u64::from(b)) as f64 / 9_007_199_254_740_992.0;
        assert_eq!(u.to_bits(), expect.to_bits());
    }

    #[test]
    fn range_uses_reserved_slots() {
        let x = random_range(99, 2, 0, 1, 0, 0, 10).unwrap();
        assert!(x < 10);
        // n=1 always 0
        assert_eq!(random_range(99, 2, 0, 1, 0, 0, 1).unwrap(), 0);
    }

    #[test]
    fn observer_rng_does_not_change_philox() {
        let a = random_u32(5, 3, 1, 2, 0, 0).unwrap();
        let mut obs = ObserverRng::new(123);
        let _ = obs.next_u64();
        let b = random_u32(5, 3, 1, 2, 0, 0).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn quota_draw_index_formula() {
        let d = quota_draw_index(5, 2, 1, 3).unwrap();
        assert_eq!(d, ((((5u64 * 16 + 2) * 8 + 1) * 256 + 3) * 4));
        assert!(quota_draw_index(32, 0, 0, 0).is_err());
    }
}
