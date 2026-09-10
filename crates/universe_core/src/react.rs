//! P5b–P5d basal and boosted uptake / photo / catabolism with shared-pool quota.

use crate::ledger::{catabolism, n_uptake, o_uptake, photosynth, BodyView, CellView};
use crate::physics::cell_xy;
use crate::quota::{allocate_pool, common_extents, MultiInputRequest, PoolDemand, RequestId};
use universe_types::{NumericFault, Organism, Phase, PoolType, RuleSpec, WorldState};

fn org_cell(org: &Organism, spec: &RuleSpec) -> Result<(u32, u32), NumericFault> {
    cell_xy(
        org.geom.x,
        org.geom.y,
        spec.world.grid_width,
        spec.world.grid_height,
    )
    .ok_or(NumericFault::NonFinite)
}

fn body_of(org: &Organism) -> BodyView {
    BodyView {
        n: org.accounts.n,
        c: org.accounts.c,
        b: org.accounts.b,
        e_old: org.accounts.e_old,
        e_new: org.accounts.e_new,
        n_max: org.n_max,
        c_max: org.c_max,
        e_max: org.e_max,
        b_target: org.b_target,
    }
}

fn write_body(org: &mut Organism, b: BodyView) {
    org.accounts.n = b.n;
    org.accounts.c = b.c;
    org.accounts.b = b.b;
    org.accounts.e_old = b.e_old;
    org.accounts.e_new = b.e_new;
}

fn write_cell(state: &mut WorldState, x: u32, y: u32, c: CellView) {
    let i = state.fields.idx(x, y);
    state.fields.n[i] = c.n;
    state.fields.o[i] = c.o;
    state.fields.s[i] = c.s;
    state.fields.q[i] = c.q;
}

fn read_cell(state: &WorldState, x: u32, y: u32) -> CellView {
    let i = state.fields.idx(x, y);
    CellView {
        n: state.fields.n[i],
        o: state.fields.o[i],
        s: state.fields.s[i],
        q: state.fields.q[i],
    }
}

/// Returns `(n_extent_sum, o_extent_sum)` actually applied.
pub fn phase_p5b_uptake(state: &mut WorldState, spec: &RuleSpec) -> Result<(u64, u64), NumericFault> {
    // N uptake: shared env N + private capacity.
    let living: Vec<usize> = state
        .organisms
        .iter()
        .enumerate()
        .filter(|(_, o)| !o.marked_dead)
        .map(|(i, _)| i)
        .collect();
    let mut n_sum = 0u64;
    let mut o_sum = 0u64;

    // Group by cell for env N.
    use std::collections::BTreeMap;
    let mut by_cell: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for &i in &living {
        let (x, y) = org_cell(&state.organisms[i], spec)?;
        let idx = y * spec.world.grid_width + x;
        by_cell.entry(idx).or_default().push(i);
    }
    for (cell_i, members) in by_cell {
        let x = cell_i % spec.world.grid_width;
        let y = cell_i / spec.world.grid_width;
        let avail_n = u128::from(state.fields.n[cell_i as usize]);
        let mut reqs = Vec::new();
        for (slot, &oi) in members.iter().enumerate() {
            let org = &state.organisms[oi];
            let mut max_e = spec.rates.n_uptake_base;
            if org.control.n_boost {
                max_e += spec.rates.n_uptake_boost;
            }
            let cap = org.n_max.saturating_sub(org.accounts.n);
            max_e = max_e.min(cap);
            reqs.push(MultiInputRequest {
                id: RequestId {
                    phase: Phase::P5b.code(),
                    actor: org.entity_id,
                    purpose: 1,
                    target: u64::from(cell_i),
                },
                max_extent: max_e,
                coeff: vec![1, 1],
                priority_by_pool: vec![0, 0],
            });
            let _ = slot;
        }
        // pools: 0 = env N, 1 = dummy private (use huge avail; real cap already in max_extent)
        let extents = common_extents(&[avail_n, u128::MAX / 4], &reqs)?;
        for (k, &oi) in members.iter().enumerate() {
            let x_e = extents[k];
            if x_e == 0 {
                continue;
            }
            let mut body = body_of(&state.organisms[oi]);
            let mut cell = read_cell(state, x, y);
            n_uptake(&mut body, &mut cell, x_e)?;
            write_body(&mut state.organisms[oi], body);
            write_cell(state, x, y, cell);
            n_sum = n_sum
                .checked_add(x_e)
                .ok_or(NumericFault::Overflow("n_up_sum"))?;
            state.organisms[oi].feedback.uptake_no = state.organisms[oi]
                .feedback
                .uptake_no
                .checked_add(x_e)
                .ok_or(NumericFault::Overflow("fb_up_n"))?;
        }
    }

    // O uptake only if intent on (B version default off).
    let mut by_cell_o: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for &i in &living {
        if !state.organisms[i].control.o_uptake {
            continue;
        }
        let (x, y) = org_cell(&state.organisms[i], spec)?;
        let idx = y * spec.world.grid_width + x;
        by_cell_o.entry(idx).or_default().push(i);
    }
    for (cell_i, members) in by_cell_o {
        let x = cell_i % spec.world.grid_width;
        let y = cell_i / spec.world.grid_width;
        let avail = u128::from(state.fields.o[cell_i as usize]);
        let items: Vec<PoolDemand> = members
            .iter()
            .map(|&oi| {
                let org = &state.organisms[oi];
                let cap = org.c_max.saturating_sub(org.accounts.c);
                let max_e = spec.rates.o_uptake_when_on.min(cap);
                PoolDemand {
                    id: RequestId {
                        phase: Phase::P5b.code(),
                        actor: org.entity_id,
                        purpose: 2,
                        target: u64::from(cell_i),
                    },
                    demand: u128::from(max_e),
                    priority: 0,
                }
            })
            .collect();
        let g = allocate_pool(avail, &items)?;
        for (k, &oi) in members.iter().enumerate() {
            let x_e = g[k] as u64;
            if x_e == 0 {
                continue;
            }
            let mut body = body_of(&state.organisms[oi]);
            let mut cell = read_cell(state, x, y);
            o_uptake(&mut body, &mut cell, x_e)?;
            write_body(&mut state.organisms[oi], body);
            write_cell(state, x, y, cell);
            o_sum = o_sum
                .checked_add(x_e)
                .ok_or(NumericFault::Overflow("o_up_sum"))?;
            state.organisms[oi].feedback.uptake_no = state.organisms[oi]
                .feedback
                .uptake_no
                .checked_add(x_e)
                .ok_or(NumericFault::Overflow("fb_up_o"))?;
        }
    }
    let _ = PoolType::EnvN;
    Ok((n_sum, o_sum))
}

/// Returns `(dissipation, photo_extent_sum)`.
pub fn phase_p5c_photo(state: &mut WorldState, spec: &RuleSpec) -> Result<(u64, u64), NumericFault> {
    let living: Vec<usize> = state
        .organisms
        .iter()
        .enumerate()
        .filter(|(_, o)| !o.marked_dead)
        .map(|(i, _)| i)
        .collect();
    use std::collections::BTreeMap;
    let mut by_cell: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for &i in &living {
        let (x, y) = org_cell(&state.organisms[i], spec)?;
        by_cell
            .entry(y * spec.world.grid_width + x)
            .or_default()
            .push(i);
    }
    let mut diss_all = 0u64;
    let mut extent_all = 0u64;
    for (cell_i, members) in by_cell {
        let x = cell_i % spec.world.grid_width;
        let y = cell_i / spec.world.grid_width;
        let q = state.fields.q[cell_i as usize];
        // pools: 0=Q (shared), 1=self N via huge separate pools encoded as coeff on distinct virtual
        // Use sequential apply after Q quota only, then N/C limits on extent.
        // Correct multi-input: Q shared, each self N and C capacity are private pools.
        let mut reqs = Vec::new();
        let mut priv_n = Vec::new();
        let mut priv_c = Vec::new();
        for &oi in &members {
            let org = &state.organisms[oi];
            let mut max_e = spec.rates.photo_base;
            if org.control.photo_boost {
                max_e += spec.rates.photo_boost;
            }
            max_e = max_e
                .min(org.accounts.n)
                .min(org.c_max.saturating_sub(org.accounts.c));
            priv_n.push(u128::from(org.accounts.n));
            priv_c.push(u128::from(org.c_max.saturating_sub(org.accounts.c)));
            let mut coeff = vec![u128::from(spec.reactions.photo_q_per)]; // Q
            for _ in 0..members.len() {
                coeff.push(0);
            }
            reqs.push(MultiInputRequest {
                id: RequestId {
                    phase: Phase::P5c.code(),
                    actor: org.entity_id,
                    purpose: 3,
                    target: u64::from(cell_i),
                },
                max_extent: max_e,
                coeff,
                priority_by_pool: vec![0; 1 + members.len()],
            });
        }
        // private N pools
        for (k, &oi) in members.iter().enumerate() {
            reqs[k].coeff[1 + k] = 1;
            let _ = oi;
        }
        let mut avail = vec![u128::from(q)];
        avail.extend(priv_n);
        let extents = common_extents(&avail, &reqs)?;
        for (k, &oi) in members.iter().enumerate() {
            let x_e = extents[k];
            if x_e == 0 {
                continue;
            }
            let mut body = body_of(&state.organisms[oi]);
            let mut cell = read_cell(state, x, y);
            let d = photosynth(&mut body, &mut cell, x_e, &spec.reactions)?;
            diss_all = diss_all
                .checked_add(d.dissipation)
                .ok_or(NumericFault::Overflow("photo_d"))?;
            write_body(&mut state.organisms[oi], body);
            write_cell(state, x, y, cell);
            extent_all = extent_all
                .checked_add(x_e)
                .ok_or(NumericFault::Overflow("photo_x"))?;
            state.organisms[oi].feedback.photo_extent = state.organisms[oi]
                .feedback
                .photo_extent
                .checked_add(x_e)
                .ok_or(NumericFault::Overflow("fb_photo"))?;
        }
        let _ = priv_c;
    }
    Ok((diss_all, extent_all))
}

/// Returns `(dissipation, E_new produced by the reaction)`.
pub fn phase_p5d_metab(state: &mut WorldState, spec: &RuleSpec) -> Result<(u64, u64), NumericFault> {
    let mut diss_all = 0u64;
    let mut e_new_all = 0u64;
    let n = state.organisms.len();
    for oi in 0..n {
        if state.organisms[oi].marked_dead {
            continue;
        }
        let org = &state.organisms[oi];
        let mut max_e = 0u64;
        let e_tot = org.accounts.e_old.saturating_add(org.accounts.e_new);
        if e_tot < spec.rates.metab_base_e_target {
            let room_e = spec.rates.metab_base_e_target - e_tot;
            let by_e = room_e / spec.reactions.metab_e_new;
            max_e = spec.rates.metab_base.min(by_e);
        }
        if org.control.metab_boost {
            let room_e = org.e_max.saturating_sub(e_tot);
            let by_e = room_e / spec.reactions.metab_e_new;
            max_e = (spec.rates.metab_base + spec.rates.metab_boost).min(by_e);
        }
        max_e = max_e.min(org.accounts.c);
        if max_e == 0 {
            continue;
        }
        let (x, y) = org_cell(org, spec)?;
        let mut body = body_of(org);
        let mut cell = read_cell(state, x, y);
        let d = catabolism(&mut body, &mut cell, max_e, &spec.reactions)?;
        diss_all = diss_all
            .checked_add(d.dissipation)
            .ok_or(NumericFault::Overflow("metab_d"))?;
        let produced = max_e
            .checked_mul(spec.reactions.metab_e_new)
            .ok_or(NumericFault::Overflow("metab_e_sum"))?;
        e_new_all = e_new_all
            .checked_add(produced)
            .ok_or(NumericFault::Overflow("metab_e_all"))?;
        write_body(&mut state.organisms[oi], body);
        write_cell(state, x, y, cell);
    }
    Ok((diss_all, e_new_all))
}
