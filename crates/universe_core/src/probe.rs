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

#[derive(Clone, Debug)]
pub struct ObservationFrame {
    pub entity_id: u64,
    pub observation_state_tick: u64,
    pub feedback_start_tick: u64,
    pub feedback_end_tick: u64,
    pub feedback_ticks: u64,
    pub obs: [f64; universe_types::INPUTS],
    pub p0_e_old: u64,
    pub p0_c: u64,
}

impl Default for ObservationFrame {
    fn default() -> Self {
        Self {
            entity_id: 0,
            observation_state_tick: 0,
            feedback_start_tick: 0,
            feedback_end_tick: 0,
            feedback_ticks: 0,
            obs: [0.0; universe_types::INPUTS],
            p0_e_old: 0,
            p0_c: 0,
        }
    }
}

impl ObservationFrame {
    pub fn obs_valid(&self, i: usize) -> bool {
        i < 32 && self.obs[32 + i] > 0.5
    }
}

#[derive(Clone, Debug, Default)]
pub struct DecisionSkip {
    pub tick: u64,
    pub entity_id: u64,
    pub reason: String,
    pub e_old: u64,
    pub open_feedback_ticks: u64,
}

#[derive(Clone, Debug)]
pub struct DecisionDiag {
    pub tick: u64,
    pub entity_id: u64,
    pub observation_state_tick: u64,
    pub feedback_start_tick: u64,
    pub feedback_end_tick: u64,
    pub feedback_ticks: u64,
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
    pub v_norm: f64,
    pub intent_thrust: i8,
    pub intent_turn: i8,
    pub binary_intent: [bool; 10],
    pub p_thr: [f64; 3],
    pub p_turn: [f64; 3],
    pub p_bin: [f64; 10],
    pub paid_thrust: i8,
    pub paid_turn: i8,
    pub u_effective: f64,
    pub w_effective: f64,
    pub paid_motion_energy: u64,
    pub actual_forward_displacement: f64,
    pub actual_unwrapped_turn: f64,
    pub obs: [f64; universe_types::INPUTS],
    pub h_before: [f64; universe_types::HIDDEN],
    pub h_after: [f64; universe_types::HIDDEN],
    pub phi: [f64; universe_types::PHI],
    pub actor_task_gradient_norm: f64,
    pub entropy_term_norm: f64,
    pub baseline_term_norm: f64,
    pub trace_norm_before_projection: f64,
    pub trace_norm_after_projection: f64,
    pub update_norm_before_projection: f64,
    pub update_norm_after_projection: f64,
}

impl Default for DecisionDiag {
    fn default() -> Self {
        Self {
            tick: 0,
            entity_id: 0,
            observation_state_tick: 0,
            feedback_start_tick: 0,
            feedback_end_tick: 0,
            feedback_ticks: 0,
            applied_update: false,
            interval_d: 0,
            r: 0.0,
            delta: 0.0,
            delta_used: 0.0,
            clipped: false,
            entropy: 0.0,
            a_minus_a0_frob: 0.0,
            trace_a_frob: 0.0,
            h_l2_delta: 0.0,
            input_change: 0.0,
            da_bias_abs: 0.0,
            da_feat_abs: 0.0,
            v_prev: 0.0,
            v_next: 0.0,
            v_norm: 0.0,
            intent_thrust: 0,
            intent_turn: 0,
            binary_intent: [false; 10],
            p_thr: [0.0; 3],
            p_turn: [0.0; 3],
            p_bin: [0.0; 10],
            paid_thrust: 0,
            paid_turn: 0,
            u_effective: 0.0,
            w_effective: 0.0,
            paid_motion_energy: 0,
            actual_forward_displacement: 0.0,
            actual_unwrapped_turn: 0.0,
            obs: [0.0; universe_types::INPUTS],
            h_before: [0.0; universe_types::HIDDEN],
            h_after: [0.0; universe_types::HIDDEN],
            phi: [0.0; universe_types::PHI],
            actor_task_gradient_norm: 0.0,
            entropy_term_norm: 0.0,
            baseline_term_norm: 0.0,
            trace_norm_before_projection: 0.0,
            trace_norm_after_projection: 0.0,
            update_norm_before_projection: 0.0,
            update_norm_after_projection: 0.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DecisionAggregate {
    pub n: u64,
    pub n_updates: u64,
    pub n_clipped: u64,
    pub mean_delta: f64,
    pub mean_delta_used: f64,
    pub mean_entropy: f64,
    pub mean_interval_d: f64,
    pub mean_r: f64,
    pub mean_input_change: f64,
    pub mean_h_l2: f64,
    pub mean_a_minus_a0: f64,
    pub mean_u_effective: f64,
    pub mean_w_effective: f64,
    pub n_obs31_valid: u64,
    pub mean_obs31: f64,
}

pub fn aggregate_decisions(diags: &[DecisionDiag]) -> DecisionAggregate {
    let n = diags.len() as u64;
    if n == 0 {
        return DecisionAggregate::default();
    }
    let mut a = DecisionAggregate { n, ..DecisionAggregate::default() };
    for d in diags {
        a.n_updates += u64::from(d.applied_update);
        a.n_clipped += u64::from(d.clipped);
        a.mean_delta += d.delta;
        a.mean_delta_used += d.delta_used;
        a.mean_entropy += d.entropy;
        a.mean_interval_d += d.interval_d as f64;
        a.mean_r += d.r;
        a.mean_input_change += d.input_change;
        a.mean_h_l2 += d.h_l2_delta;
        a.mean_a_minus_a0 += d.a_minus_a0_frob;
        a.mean_u_effective += d.u_effective;
        a.mean_w_effective += d.w_effective;
        if d.obs[32 + 31] > 0.5 {
            a.n_obs31_valid += 1;
        }
        a.mean_obs31 += d.obs[31];
    }
    let inv = n as f64;
    a.mean_delta /= inv;
    a.mean_delta_used /= inv;
    a.mean_entropy /= inv;
    a.mean_interval_d /= inv;
    a.mean_r /= inv;
    a.mean_input_change /= inv;
    a.mean_h_l2 /= inv;
    a.mean_a_minus_a0 /= inv;
    a.mean_u_effective /= inv;
    a.mean_w_effective /= inv;
    a.mean_obs31 /= inv;
    a
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

impl ControlMode {
    pub fn code(self) -> u8 {
        match self {
            Self::Neural => 0,
            Self::Stationary => 1,
            Self::Forced { .. } => 2,
            Self::OracleGotoLight => 3,
            Self::ReflexQ => 4,
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Neural => "Neural".into(),
            Self::Stationary => "Stationary".into(),
            Self::Forced { thrust, turn } => format!("Forced({thrust},{turn})"),
            Self::OracleGotoLight => "OracleGotoLight".into(),
            Self::ReflexQ => "ReflexQ".into(),
        }
    }
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
