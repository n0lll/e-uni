//! Side-channel tick and run ledgers. Never encoded into `state_hash`.

/// Diagnostic available-energy proxy in micro-units: `P = E + 8*C`.
pub fn budget_p(e_old: u64, e_new: u64, c: u64) -> u128 {
    u128::from(e_old)
        .saturating_add(u128::from(e_new))
        .saturating_add(8u128.saturating_mul(u128::from(c)))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TickProbe {
    pub maint_paid: u64,
    pub debt_repay: u64,
    pub cognition: u64,
    pub thrust_attempt: u64,
    pub turn_attempt: u64,
    pub repair: u64,
    pub repro_start: u64,
    pub repro_build: u64,
    pub photo_extent: u64,
    pub uptake_n: u64,
    pub uptake_o: u64,
    pub metab_e_new: u64,
    pub abs_dtheta: u64,
    /// First living body, or 0 if none.
    pub p: u128,
    pub e_old: u64,
    pub e_new: u64,
    pub c: u64,
    pub n: u64,
    pub b: u64,
    pub debt: u64,
    pub x_bits: u64,
    pub y_bits: u64,
    pub living: u32,
}

impl TickProbe {
    pub fn motion_paid(&self) -> u64 {
        self.thrust_attempt.saturating_add(self.turn_attempt)
    }

    pub fn uptake_no(&self) -> u64 {
        self.uptake_n.saturating_add(self.uptake_o)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunLedger {
    pub maint_paid: u64,
    pub debt_repay: u64,
    pub cognition: u64,
    pub thrust_attempt: u64,
    pub turn_attempt: u64,
    pub repair: u64,
    pub repro_start: u64,
    pub repro_build: u64,
    pub photo_extent: u64,
    pub uptake_n: u64,
    pub uptake_o: u64,
    pub metab_e_new: u64,
    pub abs_dtheta_times_1e9: u64,
    pub photo_before_switch: u64,
    pub photo_after_switch: u64,
    pub uptake_o_before_switch: u64,
    pub uptake_o_after_switch: u64,
    pub metab_e_before_switch: u64,
    pub metab_e_after_switch: u64,
    pub p_initial: Option<u128>,
    pub p_at_switch: Option<u128>,
    pub p_before_death_or_end: Option<u128>,
    pub e_at_switch: Option<u64>,
    pub c_at_switch: Option<u64>,
    pub n_at_switch: Option<u64>,
    pub debt_at_switch: Option<u64>,
    pub death_tick: Option<u64>,
    pub death_cause: Option<String>,
    pub first_old_light_entry: Option<u64>,
    pub first_new_light_entry: Option<u64>,
    pub old_light_residence_before: u64,
    pub new_light_residence_after: u64,
    pub decision_count: u64,
    pub learning_update_count: u64,
    pub executed_ticks: u64,
}

impl RunLedger {
    pub fn accumulate(&mut self, t: &TickProbe) {
        self.maint_paid = self.maint_paid.saturating_add(t.maint_paid);
        self.debt_repay = self.debt_repay.saturating_add(t.debt_repay);
        self.cognition = self.cognition.saturating_add(t.cognition);
        self.thrust_attempt = self.thrust_attempt.saturating_add(t.thrust_attempt);
        self.turn_attempt = self.turn_attempt.saturating_add(t.turn_attempt);
        self.repair = self.repair.saturating_add(t.repair);
        self.repro_start = self.repro_start.saturating_add(t.repro_start);
        self.repro_build = self.repro_build.saturating_add(t.repro_build);
        self.photo_extent = self.photo_extent.saturating_add(t.photo_extent);
        self.uptake_n = self.uptake_n.saturating_add(t.uptake_n);
        self.uptake_o = self.uptake_o.saturating_add(t.uptake_o);
        self.metab_e_new = self.metab_e_new.saturating_add(t.metab_e_new);
        self.abs_dtheta_times_1e9 = self.abs_dtheta_times_1e9.saturating_add(t.abs_dtheta);
        self.executed_ticks = self.executed_ticks.saturating_add(1);
    }
}

#[derive(Clone, Debug, Default)]
pub struct DecisionDiag {
    pub tick: u64,
    pub entity_id: u64,
    pub applied_update: bool,
    pub interval_d: u64,
    pub r: f64,
    pub delta: f64,
    pub delta_used: f64,
    pub clipped: bool,
    pub entropy: f64,
    pub a_minus_a0_frob: f64,
    pub trace_a_frob: f64,
    pub h_l2_delta: f64,
    pub input_change: f64,
    pub da_bias_abs: f64,
    pub da_feat_abs: f64,
    pub v_prev: f64,
    pub v_next: f64,
    pub intent_thrust: i8,
    pub intent_turn: i8,
    pub paid_thrust: i8,
    pub paid_turn: i8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ControlMode {
    #[default]
    Neural,
    Stationary,
    Forced { thrust: i8, turn: i8 },
    OracleGotoLight,
    ReflexQ,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaultPoint {
    AfterP1,
    AfterP2,
    AfterP4,
    AfterP5a,
    AfterP5f,
    AfterP6Alloc,
    BeforeP7,
}

impl FaultPoint {
    pub fn tag(self) -> &'static str {
        match self {
            Self::AfterP1 => "inject_p1",
            Self::AfterP2 => "inject_p2",
            Self::AfterP4 => "inject_p4",
            Self::AfterP5a => "inject_p5a",
            Self::AfterP5f => "inject_p5f",
            Self::AfterP6Alloc => "inject_p6",
            Self::BeforeP7 => "inject_p7",
        }
    }

    pub fn all() -> [FaultPoint; 7] {
        [
            Self::AfterP1,
            Self::AfterP2,
            Self::AfterP4,
            Self::AfterP5a,
            Self::AfterP5f,
            Self::AfterP6Alloc,
            Self::BeforeP7,
        ]
    }
}
