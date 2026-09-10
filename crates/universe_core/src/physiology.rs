//! Maintenance, cognition eligibility, death deposition (ch. 35.3–35.4).

use crate::physics::cell_xy;
use crate::ledger::spend_old_energy;
use universe_types::{NumericFault, Organism, RuleSpec, WorldState};

pub fn maintenance_cost(org: &Organism, spec: &RuleSpec, has_embryo: bool) -> u64 {
    let bdiv = spec.physiology.b_div_for_maintenance;
    let struct_cost = (org.b_target + bdiv - 1) / bdiv;
    let extra = if has_embryo {
        spec.evolution.extra_maintenance
    } else {
        0
    };
    struct_cost
        + spec.physiology.cost_per_capability * spec.physiology.capability_count
        + spec.physiology.hidden_h
        + 2 * spec.physiology.used_memory_entries
        + extra
}

pub fn is_irrecoverable(org: &Organism, spec: &RuleSpec) -> bool {
    5 * org.accounts.b < org.b_target || org.maintenance_debt >= spec.physiology.death_debt
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MaintPay {
    pub maint: u64,
    pub repay: u64,
}

impl MaintPay {
    pub fn total(self) -> u64 {
        self.maint.saturating_add(self.repay)
    }
}

pub fn pay_maintenance(
    org: &mut Organism,
    spec: &RuleSpec,
    has_embryo: bool,
) -> Result<u64, NumericFault> {
    Ok(pay_maintenance_parts(org, spec, has_embryo)?.total())
}

pub fn pay_maintenance_parts(
    org: &mut Organism,
    spec: &RuleSpec,
    has_embryo: bool,
) -> Result<MaintPay, NumericFault> {
    let cost = maintenance_cost(org, spec, has_embryo);
    let pay = org.accounts.e_old.min(cost);
    if pay > 0 {
        org.accounts.e_old -= pay;
    }
    let unpaid = cost - pay;
    org.maintenance_debt = org
        .maintenance_debt
        .checked_add(unpaid)
        .ok_or(NumericFault::Overflow("debt"))?;
    let mut repay = 0u64;
    if unpaid == 0 && org.maintenance_debt > 0 && org.accounts.e_old > 0 {
        repay = org
            .accounts
            .e_old
            .min(spec.physiology.debt_repay_per_tick)
            .min(org.maintenance_debt);
        org.accounts.e_old -= repay;
        org.maintenance_debt -= repay;
    }
    let _ = spend_old_energy; // accounts already updated
    Ok(MaintPay { maint: pay, repay })
}

pub fn deposit_death(state: &mut WorldState, spec: &RuleSpec, idx: usize) -> Result<u64, NumericFault> {
    let org = &state.organisms[idx];
    let n = org.accounts.n;
    let o_add = org
        .accounts
        .c
        .checked_add(org.accounts.b)
        .ok_or(NumericFault::Overflow("death_o"))?;
    let e_diss = org
        .accounts
        .e_old
        .checked_add(org.accounts.e_new)
        .ok_or(NumericFault::Overflow("death_e"))?;
    let (cx, cy) = cell_xy(
        org.geom.x,
        org.geom.y,
        spec.world.grid_width,
        spec.world.grid_height,
    )
    .ok_or(NumericFault::NonFinite)?;
    let i = state.fields.idx(cx, cy);
    state.fields.n[i] = state.fields.n[i]
        .checked_add(n)
        .ok_or(NumericFault::Overflow("death_n_field"))?;
    state.fields.o[i] = state.fields.o[i]
        .checked_add(o_add)
        .ok_or(NumericFault::Overflow("death_o_field"))?;
    let org = &mut state.organisms[idx];
    org.accounts.n = 0;
    org.accounts.c = 0;
    org.accounts.b = 0;
    org.accounts.e_old = 0;
    org.accounts.e_new = 0;
    org.marked_dead = true;
    org.brain = Box::new(universe_types::BrainState::zeroed());
    Ok(e_diss)
}

pub fn attempt_motion_cost(thrust: i8, turn: i8, spec: &RuleSpec) -> u64 {
    let u = thrust.abs() as u64;
    let w = turn.abs() as u64;
    // ceil(1000*|u|+200*|w|) with integer |u|,|w| in {0,1} is exact
    spec.physics.thrust_cost * u + spec.physics.turn_cost * w
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MotionPay {
    pub thrust: u64,
    pub turn: u64,
}

impl MotionPay {
    pub fn total(self) -> u64 {
        self.thrust.saturating_add(self.turn)
    }
}

pub fn scale_control(org: &mut Organism, spec: &RuleSpec) -> Result<u64, NumericFault> {
    Ok(scale_control_parts(org, spec)?.total())
}

/// P3: pay from `e_old` only. `e_new` cannot cover a shortfall. Partial pay scales both controls equally.
pub fn scale_control_parts(org: &mut Organism, spec: &RuleSpec) -> Result<MotionPay, NumericFault> {
    if org.marked_dead {
        return Ok(MotionPay::default());
    }
    let u = org.control.thrust.unsigned_abs() as u64;
    let w = org.control.turn.unsigned_abs() as u64;
    let cost_u = spec.physics.thrust_cost.saturating_mul(u);
    let cost_w = spec.physics.turn_cost.saturating_mul(w);
    let cost = cost_u.saturating_add(cost_w);
    let s = if cost == 0 {
        1.0
    } else {
        (org.accounts.e_old as f64 / cost as f64).min(1.0)
    };
    let paid = org.accounts.e_old.min(cost);
    org.accounts.e_old -= paid;
    org.control.u_effective = s * f64::from(org.control.thrust);
    org.control.w_effective = s * f64::from(org.control.turn);
    let (thrust, turn) = if cost == 0 {
        (0, 0)
    } else {
        let t = paid.saturating_mul(cost_u) / cost;
        (t, paid.saturating_sub(t))
    };
    Ok(MotionPay { thrust, turn })
}
