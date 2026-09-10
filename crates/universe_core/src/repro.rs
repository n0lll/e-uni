//! Asexual embryo construction, mutation, and spatial birth (ch. 38).

use crate::genome::{decode_a0, decode_v0};
use crate::physics::cell_xy;
use crate::quota::{common_extents, MultiInputRequest, RequestId};
use crate::rng::{random_range, random_u32, random_unit};
use universe_types::{
    Embryo, EmbryoStage, Geometry, LifeEvent, LifeEventKind, NumericFault, Organism, Phase,
    RngDomain, RuleSpec, WorldState, HIDDEN, INPUTS, PHI, POLICY_LOGITS,
};

fn q_index_count() -> usize {
    HIDDEN * INPUTS + HIDDEN * HIDDEN + HIDDEN + POLICY_LOGITS * PHI + PHI
}

fn genome_qs(g: &universe_types::Genome) -> Vec<i32> {
    let mut v = Vec::with_capacity(q_index_count());
    v.extend_from_slice(&g.q_win);
    v.extend_from_slice(&g.q_wrec);
    v.extend_from_slice(&g.q_bh);
    v.extend_from_slice(&g.q_a0);
    v.extend_from_slice(&g.q_v0);
    v
}

fn qs_to_genome(qs: &[i32]) -> universe_types::Genome {
    let mut i = 0;
    let take = |i: &mut usize, n: usize| {
        let s = qs[*i..*i + n].to_vec();
        *i += n;
        s
    };
    let mut g = universe_types::Genome {
        hash: [0; 32],
        q_win: take(&mut i, HIDDEN * INPUTS),
        q_wrec: take(&mut i, HIDDEN * HIDDEN),
        q_bh: take(&mut i, HIDDEN),
        q_a0: take(&mut i, POLICY_LOGITS * PHI),
        q_v0: take(&mut i, PHI),
    };
    g.recompute_hash();
    g
}

fn clip_q(idx: usize, q: i32) -> i32 {
    // W_in, then W_rec+b_h, then A0, then v0
    let n_win = HIDDEN * INPUTS;
    let n_rec = HIDDEN * HIDDEN + HIDDEN;
    let n_a0 = POLICY_LOGITS * PHI;
    if idx < n_win {
        q.clamp(-16384, 16384)
    } else if idx < n_win + n_rec {
        q.clamp(-32768, 32768)
    } else if idx < n_win + n_rec + n_a0 {
        q.clamp(-786432, 786432)
    } else {
        q.clamp(-524288, 524288)
    }
}

pub fn mutate_from_parent(
    parent: &universe_types::Genome,
    root_seed: u64,
    parent_id: u64,
    attempt_id: u64,
    spec: &RuleSpec,
    tick: u64,
) -> Result<universe_types::Genome, NumericFault> {
    let mut qs = genome_qs(parent);
    for (pi, q) in qs.iter_mut().enumerate() {
        let u = random_range(
            root_seed,
            RngDomain::Mutation.code(),
            tick,
            parent_id,
            attempt_id,
            (pi as u64) * 32,
            spec.evolution.mutation_p_denom,
        )?;
        if u < spec.evolution.mutation_p_numer {
            let sign = random_range(
                root_seed,
                RngDomain::Mutation.code(),
                tick,
                parent_id,
                attempt_id,
                (pi as u64) * 32 + 16,
                2,
            )?;
            let d = spec.evolution.mutation_delta;
            let nq = if sign == 0 { *q - d } else { *q + d };
            *q = clip_q(pi, nq);
        }
    }
    Ok(qs_to_genome(&qs))
}

fn can_start(org: &Organism, spec: &RuleSpec, has_embryo: bool) -> bool {
    org.age >= spec.evolution.min_age
        && org.accounts.b * spec.evolution.min_b_over_target_denom
            >= spec.evolution.min_b_over_target_numer * org.b_target
        && org.maintenance_debt == 0
        && org.accounts.c >= spec.evolution.min_c
        && org.accounts.e_old >= spec.evolution.min_e_old
        && org.control.reproduce
        && !has_embryo
}

pub fn phase_start_embryos(
    state: &mut WorldState,
    spec: &RuleSpec,
) -> Result<(u64, Vec<LifeEvent>), NumericFault> {
    let mut diss = 0u64;
    let mut events = Vec::new();
    let n = state.organisms.len();
    for i in 0..n {
        if state.organisms[i].marked_dead {
            continue;
        }
        let id = state.organisms[i].entity_id;
        let has = state.embryos.iter().any(|e| e.parent_id == id);
        if !can_start(&state.organisms[i], spec, has) {
            continue;
        }
        if state.organisms[i].accounts.e_old < spec.evolution.start_fee {
            continue;
        }
        state.organisms[i].accounts.e_old -= spec.evolution.start_fee;
        diss += spec.evolution.start_fee;
        state.organisms[i].attempt_count += 1;
        let attempt = state.organisms[i].attempt_count;
        let gh = state.organisms[i].genome_hash;
        let parent_g = state
            .genomes
            .iter()
            .find(|g| g.hash == gh)
            .cloned()
            .ok_or(NumericFault::Overflow("parent_genome"))?;
        let child = mutate_from_parent(&parent_g, state.root_seed, id, attempt, spec, state.tick)?;
        let heading = 2.0
            * std::f64::consts::PI
            * random_unit(
                state.root_seed,
                RngDomain::EmbryoHeading.code(),
                state.tick,
                id,
                attempt,
                0,
            )?;
        if !state.genomes.iter().any(|g| g.hash == child.hash) {
            state.genomes.push(child.clone());
        }
        state.embryos.push(Embryo {
            parent_id: id,
            attempt_id: attempt,
            genome_hash: child.hash,
            extent: 0,
            target_extent: spec.evolution.target_extent,
            b: 0,
            c: 0,
            e: 0,
            created_tick: state.tick,
            heading0: heading,
            ready_tick: None,
            stage: EmbryoStage::Building,
        });
        events.push(LifeEvent {
            tick: state.tick,
            kind: LifeEventKind::EmbryoStart,
            entity_a: id,
            entity_b: attempt,
            amount: spec.evolution.start_fee,
        });
    }
    Ok((diss, events))
}

pub fn phase_build_embryos(state: &mut WorldState, spec: &RuleSpec) -> Result<u64, NumericFault> {
    // Next tick after creation: created_tick < current tick.
    let mut diss = 0u64;
    let r = &spec.reactions;
    let mut i = 0;
    while i < state.embryos.len() {
        if state.embryos[i].stage != EmbryoStage::Building {
            i += 1;
            continue;
        }
        if state.embryos[i].created_tick >= state.tick {
            i += 1;
            continue;
        }
        let pid = state.embryos[i].parent_id;
        let Some(pidx) = state.organisms.iter().position(|o| o.entity_id == pid && !o.marked_dead)
        else {
            i += 1;
            continue;
        };
        let remain = state.embryos[i]
            .target_extent
            .saturating_sub(state.embryos[i].extent);
        let max_e = remain.min(spec.evolution.max_extent_per_tick);
        let c_need = r.embryo_c_in;
        let e_need = r.embryo_e_old;
        let parent = &state.organisms[pidx];
        let req = MultiInputRequest {
            id: RequestId {
                phase: Phase::P5fEmbryo.code(),
                actor: pid,
                purpose: 1,
                target: state.embryos[i].attempt_id,
            },
            max_extent: max_e,
            coeff: vec![u128::from(c_need), u128::from(e_need)],
            priority_by_pool: vec![0, 0],
        };
        let pools = [
            u128::from(parent.accounts.c),
            u128::from(parent.accounts.e_old),
        ];
        let extents = common_extents(&pools, &[req])?;
        let extent = extents[0];
        if extent > 0 {
            state.organisms[pidx].accounts.c -= extent * c_need;
            state.organisms[pidx].accounts.e_old -= extent * e_need;
            state.embryos[i].b += extent * r.embryo_b_out;
            state.embryos[i].c += extent * r.embryo_c_out;
            state.embryos[i].e += extent * r.embryo_e_out;
            state.embryos[i].extent += extent;
            diss += extent * r.embryo_dissipate;
        }
        if state.embryos[i].extent >= state.embryos[i].target_extent {
            state.embryos[i].stage = EmbryoStage::Ready;
            state.embryos[i].ready_tick = Some(state.tick);
        }
        i += 1;
    }
    Ok(diss)
}

pub fn cancel_embryo(state: &mut WorldState, spec: &RuleSpec, ei: usize) -> Result<u64, NumericFault> {
    let e = &state.embryos[ei];
    let mass = e.b.saturating_add(e.c);
    let ediss = e.e;
    let parent = state.organisms.iter().find(|o| o.entity_id == e.parent_id);
    let (x, y) = if let Some(p) = parent {
        cell_xy(p.geom.x, p.geom.y, spec.world.grid_width, spec.world.grid_height)
            .unwrap_or((0, 0))
    } else {
        (0, 0)
    };
    let idx = state.fields.idx(x, y);
    state.fields.o[idx] = state.fields.o[idx]
        .checked_add(mass)
        .ok_or(NumericFault::Overflow("emb_o"))?;
    state.embryos.remove(ei);
    Ok(ediss)
}

fn overlaps(x: f64, y: f64, others: &[(f64, f64)], r: f64, w: f64, h: f64) -> bool {
    if x < r || y < r || x > w - r || y > h - r {
        return true;
    }
    let min_d = 2.0 * r + 2e-6;
    for &(ox, oy) in others {
        let dx = x - ox;
        let dy = y - oy;
        if dx * dx + dy * dy < min_d * min_d {
            return true;
        }
    }
    false
}

pub fn phase_births(
    state: &mut WorldState,
    spec: &RuleSpec,
) -> Result<(u64, Vec<LifeEvent>), NumericFault> {
    let r = spec.physics.radius_lu;
    let w = spec.world.width_lu;
    let h = spec.world.height_lu;
    let mut events = Vec::new();
    let mut cancel = Vec::new();
    for (i, e) in state.embryos.iter().enumerate() {
        if state.tick.saturating_sub(e.created_tick) > spec.evolution.timeout_ticks
            && e.stage == EmbryoStage::Building
        {
            cancel.push(i);
        }
        if e.stage == EmbryoStage::Ready {
            if let Some(rt) = e.ready_tick {
                if state.tick.saturating_sub(rt) > spec.evolution.ready_place_ticks {
                    cancel.push(i);
                }
            }
        }
    }
    cancel.sort_unstable();
    cancel.dedup();
    let mut diss = 0u64;
    for i in cancel.into_iter().rev() {
        let parent = state.embryos[i].parent_id;
        let amt = state.embryos[i].b.saturating_add(state.embryos[i].c);
        diss += cancel_embryo(state, spec, i)?;
        events.push(LifeEvent {
            tick: state.tick,
            kind: LifeEventKind::EmbryoCancel,
            entity_a: parent,
            entity_b: 0,
            amount: amt,
        });
    }
    let mut ready: Vec<usize> = state
        .embryos
        .iter()
        .enumerate()
        .filter(|(_, e)| e.stage == EmbryoStage::Ready)
        .map(|(i, _)| i)
        .collect();
    let mut pri: Vec<(u32, u64, usize)> = Vec::new();
    for i in ready.drain(..) {
        let e = &state.embryos[i];
        if let Some(rt) = e.ready_tick {
            if (state.tick - rt) % spec.evolution.place_every_ticks != 0 && state.tick != rt {
                continue;
            }
        }
        let p = random_u32(
            state.root_seed,
            RngDomain::BirthPriority.code(),
            state.tick,
            e.parent_id,
            e.attempt_id,
            0,
        )?;
        pri.push((p, e.parent_id, i));
    }
    pri.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut accepted: Vec<(f64, f64)> = state
        .organisms
        .iter()
        .filter(|o| !o.marked_dead)
        .map(|o| (o.geom.x, o.geom.y))
        .collect();
    let mut born = Vec::new();
    for &(_, _, ei) in &pri {
        let e = state.embryos[ei].clone();
        let Some(parent) = state
            .organisms
            .iter()
            .find(|o| o.entity_id == e.parent_id && !o.marked_dead)
        else {
            continue;
        };
        let px = parent.geom.x;
        let py = parent.geom.y;
        let dist = 2.0 * r + 2e-6;
        let n_ang = spec.evolution.place_angles;
        let mut placed = None;
        for k in 0..n_ang {
            let th = e.heading0 + 2.0 * std::f64::consts::PI * f64::from(k) / f64::from(n_ang);
            let x = px + dist * libm::cos(th);
            let y = py + dist * libm::sin(th);
            if !overlaps(x, y, &accepted, r, w, h) {
                placed = Some((x, y, th));
                break;
            }
        }
        if let Some((x, y, th)) = placed {
            born.push((ei, x, y, th, e));
            accepted.push((x, y));
        }
    }
    born.sort_by_key(|b| std::cmp::Reverse(b.0));
    for (ei, x, y, th, e) in born {
        let id = state.next_entity_id;
        state.next_entity_id = state
            .next_entity_id
            .checked_add(1)
            .ok_or(NumericFault::Overflow("next_id"))?;
        let g = state
            .genomes
            .iter()
            .find(|g| g.hash == e.genome_hash)
            .cloned()
            .ok_or(NumericFault::Overflow("child_genome"))?;
        let mut org = Organism::new(
            id,
            Geometry {
                x,
                y,
                theta: crate::physics::wrap_angle(th),
            },
            g.hash,
            spec,
        );
        org.accounts.n = 0;
        org.accounts.c = e.c;
        org.accounts.b = e.b;
        org.accounts.e_old = e.e;
        org.accounts.e_new = 0;
        org.brain.a = decode_a0(&g);
        org.brain.v = decode_v0(&g);
        org.age = 0;
        state.organisms.push(org);
        state.embryos.remove(ei);
        if let Some(p) = state
            .organisms
            .iter_mut()
            .find(|o| o.entity_id == e.parent_id)
        {
            p.feedback.birth_count = p.feedback.birth_count.saturating_add(1);
        }
        events.push(LifeEvent {
            tick: state.tick,
            kind: LifeEventKind::Birth,
            entity_a: id,
            entity_b: e.parent_id,
            amount: e.extent,
        });
        events.push(LifeEvent {
            tick: state.tick,
            kind: LifeEventKind::Lineage,
            entity_a: e.parent_id,
            entity_b: id,
            amount: e.attempt_id,
        });
    }
    state.sort_authoritative();
    Ok((diss, events))
}

/// Apply `extent` construction units to an embryo without a parent debit (T10 arithmetic).
pub fn materialize_extent(e: &mut Embryo, spec: &RuleSpec, extent: u64) {
    let r = &spec.reactions;
    e.b += extent * r.embryo_b_out;
    e.c += extent * r.embryo_c_out;
    e.e += extent * r.embryo_e_out;
    e.extent += extent;
}

/// T10 helper: 5000 extents → B=20000 C=5000 E=40000; cancel adds O=25000 and dissipates 40000.
pub fn t10_accounts_after_5000() -> (u64, u64, u64) {
    (20_000, 5_000, 40_000)
}

