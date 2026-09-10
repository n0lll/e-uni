//! Proportional quota and multi-input common extent (ch. 34.4). No intra-phase redistribution.

use universe_types::NumericFault;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RequestId {
    pub phase: u8,
    pub actor: u64,
    pub purpose: u8,
    pub target: u64,
}

impl RequestId {
    pub fn as_tuple(self) -> (u8, u64, u8, u64) {
        (self.phase, self.actor, self.purpose, self.target)
    }
}

impl PartialOrd for RequestId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RequestId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_tuple().cmp(&other.as_tuple())
    }
}

#[derive(Debug, Clone)]
pub struct PoolDemand {
    pub id: RequestId,
    pub demand: u128,
    /// Higher value wins remainder ties, then RequestId.
    pub priority: u32,
}

/// Allocate integer pool `available` across demands. Order of `items` must not change grants.
pub fn allocate_pool(available: u128, items: &[PoolDemand]) -> Result<Vec<u128>, NumericFault> {
    let n = items.len();
    let mut grants = vec![0u128; n];
    let mut active: Vec<usize> = Vec::new();
    let mut total = 0u128;
    for (i, it) in items.iter().enumerate() {
        if it.demand == 0 {
            continue;
        }
        total = total
            .checked_add(it.demand)
            .ok_or(NumericFault::Overflow("quota_D"))?;
        active.push(i);
    }
    if total == 0 {
        return Ok(grants);
    }
    if total <= available {
        for i in active {
            grants[i] = items[i].demand;
        }
        return Ok(grants);
    }
    let a = available;
    let d = total;
    let mut remainders: Vec<(u128, u32, RequestId, usize)> = Vec::new();
    let mut sum_g = 0u128;
    for i in active {
        let prod = a
            .checked_mul(items[i].demand)
            .ok_or(NumericFault::Overflow("quota_Ad"))?;
        let g = prod / d;
        let r = prod % d;
        grants[i] = g;
        sum_g = sum_g.checked_add(g).ok_or(NumericFault::Overflow("quota_sum_g"))?;
        remainders.push((r, items[i].priority, items[i].id, i));
    }
    let leftover = a - sum_g;
    remainders.sort_by(|x, y| {
        y.0.cmp(&x.0)
            .then_with(|| y.1.cmp(&x.1))
            .then_with(|| x.2.cmp(&y.2))
    });
    for k in 0..leftover {
        let idx = remainders[k as usize].3;
        grants[idx] += 1;
        if grants[idx] > items[idx].demand {
            return Err(NumericFault::Overflow("quota_gt_demand"));
        }
    }
    Ok(grants)
}

#[derive(Debug, Clone)]
pub struct MultiInputRequest {
    pub id: RequestId,
    pub max_extent: u64,
    /// Per-pool coefficients; 0 means unused.
    pub coeff: Vec<u128>,
    pub priority_by_pool: Vec<u32>,
}

/// For each pool, demand = max_extent * coeff. Then extent = min(max, min_j floor(grant_j / c_j)).
pub fn common_extents(
    pool_available: &[u128],
    requests: &[MultiInputRequest],
) -> Result<Vec<u64>, NumericFault> {
    let n_pools = pool_available.len();
    let n_req = requests.len();
    let mut grants = vec![vec![0u128; n_pools]; n_req];
    for p in 0..n_pools {
        let mut items: Vec<PoolDemand> = Vec::with_capacity(n_req);
        for r in requests {
            let c = r.coeff.get(p).copied().unwrap_or(0);
            let demand = u128::from(r.max_extent)
                .checked_mul(c)
                .ok_or(NumericFault::Overflow("extent_coeff"))?;
            let pr = r.priority_by_pool.get(p).copied().unwrap_or(0);
            items.push(PoolDemand {
                id: r.id,
                demand,
                priority: pr,
            });
        }
        let g = allocate_pool(pool_available[p], &items)?;
        for i in 0..n_req {
            grants[i][p] = g[i];
        }
    }
    let mut extents = vec![0u64; n_req];
    for i in 0..n_req {
        let mut x = u128::from(requests[i].max_extent);
        for p in 0..n_pools {
            let c = requests[i].coeff.get(p).copied().unwrap_or(0);
            if c == 0 {
                continue;
            }
            x = x.min(grants[i][p] / c);
        }
        extents[i] = x as u64;
    }
    Ok(extents)
}
