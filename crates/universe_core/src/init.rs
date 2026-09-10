//! EU-Core-1 world initialization (ch. 34.6).

use crate::audit::compute_mu;
use crate::genome::make_minimal_autotroph_genome;
use crate::rng::random_unit;
use universe_types::{
    AuditTotals, Fields, Geometry, NumericFault, Organism, RngDomain, RuleSpec, WorldState,
};

pub fn lighting_cells(spec: &RuleSpec) -> Vec<(u32, u32)> {
    let mut cells = Vec::new();
    let l = &spec.lighting;
    for y in l.y0..=l.y1 {
        for x in l.left_x0..=l.left_x1 {
            cells.push((x, y));
        }
        for x in l.right_x0..=l.right_x1 {
            cells.push((x, y));
        }
    }
    cells.sort_by_key(|&(x, y)| (y, x));
    cells
}

/// Centre of currently lit cells in LU (cell index + 0.5). Empty lighting → (0, 0).
pub fn lighting_centroid(spec: &RuleSpec) -> (f64, f64) {
    let cells = lighting_cells(spec);
    if cells.is_empty() {
        return (0.0, 0.0);
    }
    let n = cells.len() as f64;
    let sx: f64 = cells.iter().map(|&(x, _)| f64::from(x) + 0.5).sum();
    let sy: f64 = cells.iter().map(|&(_, y)| f64::from(y) + 0.5).sum();
    (sx / n, sy / n)
}

pub fn initialize_world(spec: &RuleSpec, root_seed: u64) -> Result<WorldState, NumericFault> {
    spec.validate()
        .map_err(|_| NumericFault::Overflow("invalid_rules"))?;
    let w = spec.world.grid_width;
    let h = spec.world.grid_height;
    let mut fields = Fields::zeros(w, h);
    for y in 0..h {
        for x in 0..w {
            let i = fields.idx(x, y);
            fields.n[i] = spec.init.initial_n_per_cell;
            fields.o[i] = spec.init.initial_o_per_cell;
            fields.s[i] = spec.init.initial_s_per_cell;
            fields.q[i] = if spec.lighting.is_lit(x, y) {
                spec.lighting.initial_q_lit
            } else {
                0
            };
        }
    }

    let cells = lighting_cells(spec);
    if cells.len() != spec.init.initial_count as usize {
        return Err(NumericFault::Overflow("init_count"));
    }
    let mut genome = make_minimal_autotroph_genome(spec)?;
    genome.recompute_hash();
    let genome_hash = genome.hash;
    let mut organisms = Vec::with_capacity(cells.len());
    let mut next_id = 1u64;
    for &(cx, cy) in &cells {
        let u = random_unit(
            root_seed,
            RngDomain::InitialHeading.code(),
            0,
            next_id,
            0,
            0,
        )?;
        let theta = 2.0 * std::f64::consts::PI * u;
        let geom = Geometry {
            x: f64::from(cx) + 0.5,
            y: f64::from(cy) + 0.5,
            theta,
        };
        organisms.push(Organism::new(next_id, geom, genome_hash, spec));
        next_id += 1;
    }

    let mut state = WorldState::empty(spec, root_seed)?;
    state.fields = fields;
    state.organisms = organisms;
    state.genomes.push(genome);
    state.next_entity_id = next_id;
    for org in &mut state.organisms {
        crate::brain::install_genome_weights(org, &state.genomes[0]);
    }
    let mu = compute_mu(&state, spec.reactions.chem_e_per_organic)?;
    state.audit = AuditTotals {
        initial_m: mu.m,
        initial_u: mu.u,
        ..AuditTotals::default()
    };
    state.sort_authoritative();
    Ok(state)
}

/// Chapter 42.5 resource-reversal map: 16×16, one autotroph at the centre.
pub fn initialize_reversal_world(spec: &RuleSpec, root_seed: u64) -> Result<WorldState, NumericFault> {
    spec.validate_lab()
        .map_err(|_| NumericFault::Overflow("invalid_lab_rules"))?;
    let w = spec.world.grid_width;
    let h = spec.world.grid_height;
    let mut fields = Fields::zeros(w, h);
    for y in 0..h {
        for x in 0..w {
            let i = fields.idx(x, y);
            fields.n[i] = spec.init.initial_n_per_cell;
            fields.o[i] = spec.init.initial_o_per_cell;
            fields.s[i] = spec.init.initial_s_per_cell;
            fields.q[i] = if spec.lighting.is_lit(x, y) {
                spec.lighting.initial_q_lit
            } else {
                0
            };
        }
    }
    let mut genome = make_minimal_autotroph_genome(spec)?;
    genome.recompute_hash();
    let genome_hash = genome.hash;
    let u = random_unit(
        root_seed,
        RngDomain::InitialHeading.code(),
        0,
        1,
        0,
        0,
    )?;
    let theta = 2.0 * std::f64::consts::PI * u;
    let geom = Geometry {
        x: spec.world.width_lu / 2.0,
        y: spec.world.height_lu / 2.0,
        theta,
    };
    let mut org = Organism::new(1, geom, genome_hash, spec);
    crate::brain::install_genome_weights(&mut org, &genome);
    let mut state = WorldState::empty(spec, root_seed)?;
    state.fields = fields;
    state.organisms.push(org);
    state.genomes.push(genome);
    state.next_entity_id = 2;
    let mu = compute_mu(&state, spec.reactions.chem_e_per_organic)?;
    state.audit = AuditTotals {
        initial_m: mu.m,
        initial_u: mu.u,
        ..AuditTotals::default()
    };
    state.sort_authoritative();
    Ok(state)
}

/// Recompute M0/U0 after a public experimental initial-state edit.
pub fn retarget_audit_baseline(state: &mut WorldState, spec: &RuleSpec) -> Result<(), NumericFault> {
    let mu = compute_mu(state, spec.reactions.chem_e_per_organic)?;
    state.audit.initial_m = mu.m;
    state.audit.initial_u = mu.u;
    Ok(())
}
