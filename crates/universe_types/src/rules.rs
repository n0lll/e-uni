//! Complete EU-Core-1 RuleSpec. YAML must list every field; unknown keys are rejected.

use crate::canon::CanonWriter;
use crate::enums::{
    Boundary, LearningKind, MutationKind, ReproductionKind, RuleProfile, HIDDEN, INPUTS,
    LOGICAL_CHANNELS, POLICY_LOGITS,
};
use crate::error::ConfigError;
use crate::units::{T01_INITIAL_M_MICRO, T01_INITIAL_U_MICRO};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleSpec {
    pub profile: RuleProfile,
    pub time: TimeRules,
    pub world: WorldRules,
    pub lighting: LightingRules,
    pub init: InitRules,
    pub rates: RateRules,
    pub physics: PhysicsRules,
    pub physiology: PhysiologyRules,
    pub brain: BrainRules,
    pub memory: MemoryRules,
    pub evolution: EvolutionRules,
    pub reactions: ReactionRules,
    pub archive: ArchiveRules,
    pub safety: SafetyRules,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeRules {
    pub dt_numerator: u64,
    pub dt_denominator: u64,
    pub decision_every_ticks: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldRules {
    pub width_lu: f64,
    pub height_lu: f64,
    pub grid_width: u32,
    pub grid_height: u32,
    pub boundary: Boundary,
    pub n_divisor: u64,
    pub o_divisor: u64,
    pub s_divisor: u64,
    pub q_divisor: u64,
    pub s_decay_divisor: u64,
    pub q_loss_divisor: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LightingRules {
    pub left_x0: u32,
    pub left_x1: u32,
    pub right_x0: u32,
    pub right_x1: u32,
    pub y0: u32,
    pub y1: u32,
    pub initial_q_lit: u64,
    pub radiation_input_per_cell: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitRules {
    pub initial_n_per_cell: u64,
    pub initial_o_per_cell: u64,
    pub initial_s_per_cell: u64,
    pub initial_count: u32,
    pub initial_b: u64,
    pub initial_c: u64,
    pub initial_n: u64,
    pub initial_e: u64,
    pub n_max: u64,
    pub c_max: u64,
    pub e_max: u64,
    pub b_target: u64,
    pub genome_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateRules {
    pub n_uptake_base: u64,
    pub n_uptake_boost: u64,
    pub photo_base: u64,
    pub photo_boost: u64,
    pub metab_base: u64,
    pub metab_boost: u64,
    pub o_uptake_when_on: u64,
    pub repair_when_on: u64,
    pub metab_base_e_target: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicsRules {
    pub radius_lu: f64,
    pub max_linear_speed: f64,
    /// Must be exactly `std::f64::consts::PI` for EU-Core-1.
    pub max_angular_speed: f64,
    pub substeps: u32,
    pub max_projection_iters: u32,
    pub contact_tolerance: f64,
    pub action_contact_slop: f64,
    pub thrust_cost: u64,
    pub turn_cost: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysiologyRules {
    pub b_div_for_maintenance: u64,
    pub cost_per_capability: u64,
    pub capability_count: u64,
    pub hidden_h: u64,
    pub used_memory_entries: u64,
    pub debt_repay_per_tick: u64,
    pub death_debt: u64,
    pub mature_age: u64,
    pub cognition_div: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrainRules {
    pub core_inputs: u32,
    pub initial_hidden: u32,
    pub logical_action_channels: u32,
    pub policy_logits: u32,
    pub value_outputs: u32,
    pub learning: LearningKind,
    pub writable_state_per_individual: bool,
    pub leak_old: f64,
    pub leak_new: f64,
    pub gamma_tick: f64,
    pub lambda_trace: f64,
    pub actor_lr: f64,
    pub critic_lr: f64,
    pub entropy_coef: f64,
    pub baseline_coef: f64,
    pub delta_clip: f64,
    pub e_a_radius: f64,
    pub e_v_radius: f64,
    pub d_a_radius: f64,
    pub d_v_radius: f64,
    pub a_row_radius: f64,
    pub v_radius: f64,
    pub w_rec_l1: f64,
    pub reward_bias: f64,
    pub reward_hdev: f64,
    pub reward_birth: f64,
    pub reward_death: f64,
    pub reward_e_target: u64,
    pub reward_c_target: u64,
    pub sense_c_n: u64,
    pub sense_c_o: u64,
    pub sense_c_s: u64,
    pub sense_c_q: u64,
    pub sense_range_lu: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryRules {
    pub associative_enabled: bool,
    pub experience_inheritance: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvolutionRules {
    pub reproduction: ReproductionKind,
    pub mutation_profile: MutationKind,
    pub morphology_mutation: bool,
    pub min_age: u64,
    pub min_b_over_target_numer: u64,
    pub min_b_over_target_denom: u64,
    pub min_c: u64,
    pub min_e_old: u64,
    pub start_fee: u64,
    pub target_extent: u64,
    pub max_extent_per_tick: u64,
    pub extra_maintenance: u64,
    pub timeout_ticks: u64,
    pub ready_place_ticks: u64,
    pub place_every_ticks: u64,
    pub place_angles: u32,
    pub mutation_p_numer: u32,
    pub mutation_p_denom: u32,
    pub mutation_delta: i32,
    pub q_win_init_abs: i32,
    pub q_wrec_diag: i32,
    pub q_wrec_off_abs: i32,
    pub signal_enabled: bool,
    pub transfer_enabled: bool,
    pub attack_enabled: bool,
    pub adhere_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReactionRules {
    pub chem_e_per_organic: u64,
    pub photo_q_per: u64,
    pub photo_dissipate: u64,
    pub metab_e_new: u64,
    pub metab_dissipate: u64,
    pub repair_e_old: u64,
    pub repair_dissipate: u64,
    pub attack_e_old: u64,
    pub attack_dissipate: u64,
    pub signal_e_old: u64,
    pub signal_dissipate: u64,
    pub transfer_e_old: u64,
    pub transfer_dissipate: u64,
    pub embryo_c_in: u64,
    pub embryo_e_old: u64,
    pub embryo_b_out: u64,
    pub embryo_c_out: u64,
    pub embryo_e_out: u64,
    pub embryo_dissipate: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveRules {
    pub checkpoint_every_ticks: u64,
    pub state_hash_every_ticks: u64,
    pub aggregate_every_ticks: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SafetyRules {
    pub max_living: u32,
    pub max_contact_pairs: u32,
    pub max_unwritten_archive_bytes: u64,
}

impl LightingRules {
    pub fn is_lit(&self, x: u32, y: u32) -> bool {
        if y < self.y0 || y > self.y1 {
            return false;
        }
        (x >= self.left_x0 && x <= self.left_x1) || (x >= self.right_x0 && x <= self.right_x1)
    }

    pub fn lit_cell_count(&self) -> u64 {
        let ys = u64::from(self.y1 - self.y0 + 1);
        let left = u64::from(self.left_x1 - self.left_x0 + 1);
        let right = u64::from(self.right_x1 - self.right_x0 + 1);
        ys * (left + right)
    }
}

impl RuleSpec {
    pub fn eu_core_1() -> Self {
        Self {
            profile: RuleProfile::EuCore1,
            time: TimeRules {
                dt_numerator: 1,
                dt_denominator: 10,
                decision_every_ticks: 5,
            },
            world: WorldRules {
                width_lu: 128.0,
                height_lu: 128.0,
                grid_width: 128,
                grid_height: 128,
                boundary: Boundary::Closed,
                n_divisor: 64,
                o_divisor: 128,
                s_divisor: 16,
                q_divisor: 16,
                s_decay_divisor: 1000,
                q_loss_divisor: 16,
            },
            lighting: LightingRules {
                left_x0: 28,
                left_x1: 35,
                right_x0: 92,
                right_x1: 99,
                y0: 60,
                y1: 67,
                initial_q_lit: 200_000,
                radiation_input_per_cell: 20_000,
            },
            init: InitRules {
                initial_n_per_cell: 500_000,
                initial_o_per_cell: 0,
                initial_s_per_cell: 0,
                initial_count: 128,
                initial_b: 1_000_000,
                initial_c: 500_000,
                initial_n: 100_000,
                initial_e: 5_000_000,
                n_max: 250_000,
                c_max: 4_000_000,
                e_max: 10_000_000,
                b_target: 1_000_000,
                genome_name: "minimal_autotroph_core1".to_string(),
            },
            rates: RateRules {
                n_uptake_base: 2_000,
                n_uptake_boost: 2_000,
                photo_base: 2_000,
                photo_boost: 2_000,
                metab_base: 1_000,
                metab_boost: 1_000,
                o_uptake_when_on: 2_000,
                repair_when_on: 1_000,
                metab_base_e_target: 6_000_000,
            },
            physics: PhysicsRules {
                radius_lu: 0.3,
                max_linear_speed: 1.0,
                max_angular_speed: std::f64::consts::PI,
                substeps: 4,
                max_projection_iters: 32,
                contact_tolerance: 1e-6,
                action_contact_slop: 1e-6,
                thrust_cost: 1000,
                turn_cost: 200,
            },
            physiology: PhysiologyRules {
                b_div_for_maintenance: 500,
                cost_per_capability: 20,
                capability_count: 8,
                hidden_h: 32,
                used_memory_entries: 0,
                debt_repay_per_tick: 2_000,
                death_debt: 200_000,
                mature_age: 500,
                cognition_div: 8,
            },
            brain: BrainRules {
                core_inputs: INPUTS as u32,
                initial_hidden: HIDDEN as u32,
                logical_action_channels: LOGICAL_CHANNELS as u32,
                policy_logits: POLICY_LOGITS as u32,
                value_outputs: 1,
                learning: LearningKind::IndependentReadoutActorCriticV1,
                writable_state_per_individual: true,
                leak_old: 0.75,
                leak_new: 0.25,
                gamma_tick: 0.999,
                lambda_trace: 0.99,
                actor_lr: 0.05,
                critic_lr: 0.02,
                entropy_coef: 0.001,
                baseline_coef: 0.002,
                delta_clip: 2.0,
                e_a_radius: 10.0,
                e_v_radius: 10.0,
                d_a_radius: 0.01,
                d_v_radius: 0.01,
                a_row_radius: 24.0,
                v_radius: 16.0,
                w_rec_l1: 0.9,
                reward_bias: 0.001,
                reward_hdev: 0.002,
                reward_birth: 0.25,
                reward_death: 2.0,
                reward_e_target: 6_000_000,
                reward_c_target: 500_000,
                sense_c_n: 500_000,
                sense_c_o: 500_000,
                sense_c_s: 10_000,
                sense_c_q: 200_000,
                sense_range_lu: 4.0,
            },
            memory: MemoryRules {
                associative_enabled: false,
                experience_inheritance: false,
            },
            evolution: EvolutionRules {
                reproduction: ReproductionKind::AsexualEmbryoV1,
                mutation_profile: MutationKind::BoundedParameterMutationV1,
                morphology_mutation: false,
                min_age: 500,
                min_b_over_target_numer: 9,
                min_b_over_target_denom: 10,
                min_c: 1_500_000,
                min_e_old: 4_000_000,
                start_fee: 2_000,
                target_extent: 250_000,
                max_extent_per_tick: 2_500,
                extra_maintenance: 200,
                timeout_ticks: 1_000,
                ready_place_ticks: 500,
                place_every_ticks: 10,
                place_angles: 16,
                mutation_p_numer: 1,
                mutation_p_denom: 256,
                mutation_delta: 64,
                q_win_init_abs: 4096,
                q_wrec_diag: 26214,
                q_wrec_off_abs: 64,
                signal_enabled: false,
                transfer_enabled: false,
                attack_enabled: false,
                adhere_enabled: false,
            },
            reactions: ReactionRules {
                chem_e_per_organic: 10,
                photo_q_per: 20,
                photo_dissipate: 10,
                metab_e_new: 8,
                metab_dissipate: 2,
                repair_e_old: 2,
                repair_dissipate: 2,
                attack_e_old: 2,
                attack_dissipate: 2,
                signal_e_old: 1,
                signal_dissipate: 1,
                transfer_e_old: 1,
                transfer_dissipate: 1,
                embryo_c_in: 5,
                embryo_e_old: 12,
                embryo_b_out: 4,
                embryo_c_out: 1,
                embryo_e_out: 8,
                embryo_dissipate: 4,
            },
            archive: ArchiveRules {
                checkpoint_every_ticks: 10_000,
                state_hash_every_ticks: 100,
                aggregate_every_ticks: 100,
            },
            safety: SafetyRules {
                max_living: 4_096,
                max_contact_pairs: 65_536,
                max_unwritten_archive_bytes: 256 * 1024 * 1024,
            },
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.time.dt_numerator != 1 || self.time.dt_denominator != 10 {
            return Err(ConfigError::IllegalCombination(
                "dt must be 1/10 s".into(),
            ));
        }
        if self.time.decision_every_ticks != 5 {
            return Err(ConfigError::IllegalCombination(
                "decision_every_ticks must be 5".into(),
            ));
        }
        self.validate_core_map()?;
        match self.profile {
            RuleProfile::EuCore1 => self.validate_core1()?,
            RuleProfile::EuBody1 | RuleProfile::EuSocial1 | RuleProfile::EuSocialPair1 => {
                return Err(ConfigError::IllegalCombination(
                    "E/S/pair profiles are reserved interfaces; this executor implements EU-Core-1 only"
                        .into(),
                ));
            }
        }
        if self.physics.max_angular_speed.to_bits() != std::f64::consts::PI.to_bits() {
            return Err(ConfigError::IllegalCombination(
                "max_angular_speed must be the fixed π constant".into(),
            ));
        }
        Ok(())
    }

    fn validate_core_map(&self) -> Result<(), ConfigError> {
        if self.world.grid_width != 128 || self.world.grid_height != 128 {
            return Err(ConfigError::IllegalCombination(
                "grid must be 128x128 for EU-Core-1".into(),
            ));
        }
        if self.world.boundary != Boundary::Closed {
            return Err(ConfigError::IllegalCombination(
                "boundary must be closed".into(),
            ));
        }
        if self.lighting.lit_cell_count() != 128 {
            return Err(ConfigError::IllegalCombination(format!(
                "lighting cells must be 128, got {}",
                self.lighting.lit_cell_count()
            )));
        }
        Ok(())
    }

    /// Lab maps (ch. 42.5) keep Core-1 algorithms but may shrink the grid and lighting.
    pub fn validate_lab(&self) -> Result<(), ConfigError> {
        if self.time.dt_numerator != 1 || self.time.dt_denominator != 10 {
            return Err(ConfigError::IllegalCombination(
                "dt must be 1/10 s".into(),
            ));
        }
        if self.time.decision_every_ticks != 5 {
            return Err(ConfigError::IllegalCombination(
                "decision_every_ticks must be 5".into(),
            ));
        }
        if self.world.boundary != Boundary::Closed {
            return Err(ConfigError::IllegalCombination(
                "boundary must be closed".into(),
            ));
        }
        if self.world.grid_width == 0 || self.world.grid_height == 0 {
            return Err(ConfigError::IllegalCombination("empty grid".into()));
        }
        match self.profile {
            RuleProfile::EuCore1 => self.validate_core1()?,
            RuleProfile::EuBody1 | RuleProfile::EuSocial1 | RuleProfile::EuSocialPair1 => {
                return Err(ConfigError::IllegalCombination(
                    "E/S/pair profiles are reserved interfaces; this executor implements EU-Core-1 only"
                        .into(),
                ));
            }
        }
        Ok(())
    }

    fn validate_core1(&self) -> Result<(), ConfigError> {
        if self.brain.core_inputs != 64
            || self.brain.initial_hidden != 32
            || self.brain.policy_logits != 16
            || self.brain.value_outputs != 1
            || self.brain.logical_action_channels != 12
        {
            return Err(ConfigError::IllegalCombination(
                "B-version network shape must be I=64 H=32 L=16 V=1".into(),
            ));
        }
        if self.brain.learning != LearningKind::IndependentReadoutActorCriticV1 {
            return Err(ConfigError::IllegalCombination(
                "EU-Core-1 must use independent_readout_actor_critic_v1".into(),
            ));
        }
        if !self.brain.writable_state_per_individual {
            return Err(ConfigError::IllegalCombination(
                "writable_state_per_individual must be true".into(),
            ));
        }
        if self.memory.associative_enabled || self.memory.experience_inheritance {
            return Err(ConfigError::IllegalCombination(
                "EU-Core-1 forbids associative memory and experience inheritance".into(),
            ));
        }
        if self.evolution.morphology_mutation {
            return Err(ConfigError::IllegalCombination(
                "EU-Core-1 forbids morphology_mutation".into(),
            ));
        }
        if self.evolution.reproduction != ReproductionKind::AsexualEmbryoV1 {
            return Err(ConfigError::IllegalCombination(
                "EU-Core-1 reproduction must be asexual_embryo_v1".into(),
            ));
        }
        if self.evolution.signal_enabled
            || self.evolution.transfer_enabled
            || self.evolution.attack_enabled
            || self.evolution.adhere_enabled
        {
            return Err(ConfigError::IllegalCombination(
                "EU-Core-1 social/adhere capabilities must be off".into(),
            ));
        }
        Ok(())
    }

    pub fn expected_initial_m(&self) -> u128 {
        let cells = u128::from(self.world.grid_width) * u128::from(self.world.grid_height);
        let env = cells * u128::from(self.init.initial_n_per_cell)
            + cells * u128::from(self.init.initial_o_per_cell)
            + cells * u128::from(self.init.initial_s_per_cell);
        let living = u128::from(self.init.initial_count)
            * (u128::from(self.init.initial_n)
                + u128::from(self.init.initial_c)
                + u128::from(self.init.initial_b));
        env + living
    }

    pub fn expected_initial_u(&self) -> u128 {
        let chem = self.reactions.chem_e_per_organic;
        let cells = u128::from(self.world.grid_width) * u128::from(self.world.grid_height);
        let env_q = u128::from(self.lighting.lit_cell_count()) * u128::from(self.lighting.initial_q_lit);
        let env_o = cells * u128::from(self.init.initial_o_per_cell);
        let living_e = u128::from(self.init.initial_count) * u128::from(self.init.initial_e);
        let living_cb = u128::from(self.init.initial_count)
            * (u128::from(self.init.initial_c) + u128::from(self.init.initial_b));
        env_q + living_e + u128::from(chem) * (env_o + living_cb)
    }

    pub fn t01_matches_doc(&self) -> bool {
        self.expected_initial_m() == T01_INITIAL_M_MICRO
            && self.expected_initial_u() == T01_INITIAL_U_MICRO
    }

    pub fn maintenance_cost_b(&self) -> u64 {
        // ceil(B_target / 500) + 20*8 + 32 + 0 = 2000 + 160 + 32 = 2192
        let b = self.init.b_target;
        let struct_cost = (b + self.physiology.b_div_for_maintenance - 1)
            / self.physiology.b_div_for_maintenance;
        struct_cost
            + self.physiology.cost_per_capability * self.physiology.capability_count
            + self.physiology.hidden_h
            + 2 * self.physiology.used_memory_entries
    }

    pub fn cognition_cost_b(&self) -> u64 {
        // K = H*(I+H+1) + 6*(L+1)*(H+1); cost = ceil(K/8)
        let h = u64::from(self.brain.initial_hidden);
        let i = u64::from(self.brain.core_inputs);
        let l = u64::from(self.brain.policy_logits);
        let k = h * (i + h + 1) + 6 * (l + 1) * (h + 1);
        (k + self.physiology.cognition_div - 1) / self.physiology.cognition_div
    }

    pub fn encode_canonical(&self) -> Result<Vec<u8>, crate::error::NumericFault> {
        let mut w = CanonWriter::with_magic_rules();
        w.u8(self.profile.code());
        w.u64(self.time.dt_numerator);
        w.u64(self.time.dt_denominator);
        w.u64(self.time.decision_every_ticks);
        w.f64_canonical(self.world.width_lu)?;
        w.f64_canonical(self.world.height_lu)?;
        w.u32(self.world.grid_width);
        w.u32(self.world.grid_height);
        w.u8(self.world.boundary as u8);
        w.u64(self.world.n_divisor);
        w.u64(self.world.o_divisor);
        w.u64(self.world.s_divisor);
        w.u64(self.world.q_divisor);
        w.u64(self.world.s_decay_divisor);
        w.u64(self.world.q_loss_divisor);
        w.u32(self.lighting.left_x0);
        w.u32(self.lighting.left_x1);
        w.u32(self.lighting.right_x0);
        w.u32(self.lighting.right_x1);
        w.u32(self.lighting.y0);
        w.u32(self.lighting.y1);
        w.u64(self.lighting.initial_q_lit);
        w.u64(self.lighting.radiation_input_per_cell);
        w.u64(self.init.initial_n_per_cell);
        w.u64(self.init.initial_o_per_cell);
        w.u64(self.init.initial_s_per_cell);
        w.u32(self.init.initial_count);
        w.u64(self.init.initial_b);
        w.u64(self.init.initial_c);
        w.u64(self.init.initial_n);
        w.u64(self.init.initial_e);
        w.u64(self.init.n_max);
        w.u64(self.init.c_max);
        w.u64(self.init.e_max);
        w.u64(self.init.b_target);
        w.u64(self.init.genome_name.len() as u64);
        w.raw(self.init.genome_name.as_bytes());
        w.u64(self.rates.n_uptake_base);
        w.u64(self.rates.n_uptake_boost);
        w.u64(self.rates.photo_base);
        w.u64(self.rates.photo_boost);
        w.u64(self.rates.metab_base);
        w.u64(self.rates.metab_boost);
        w.u64(self.rates.o_uptake_when_on);
        w.u64(self.rates.repair_when_on);
        w.u64(self.rates.metab_base_e_target);
        w.f64_canonical(self.physics.radius_lu)?;
        w.f64_canonical(self.physics.max_linear_speed)?;
        w.f64_canonical(self.physics.max_angular_speed)?;
        w.u32(self.physics.substeps);
        w.u32(self.physics.max_projection_iters);
        w.f64_canonical(self.physics.contact_tolerance)?;
        w.f64_canonical(self.physics.action_contact_slop)?;
        w.u64(self.physics.thrust_cost);
        w.u64(self.physics.turn_cost);
        w.u64(self.physiology.b_div_for_maintenance);
        w.u64(self.physiology.cost_per_capability);
        w.u64(self.physiology.capability_count);
        w.u64(self.physiology.hidden_h);
        w.u64(self.physiology.used_memory_entries);
        w.u64(self.physiology.debt_repay_per_tick);
        w.u64(self.physiology.death_debt);
        w.u64(self.physiology.mature_age);
        w.u64(self.physiology.cognition_div);
        w.u32(self.brain.core_inputs);
        w.u32(self.brain.initial_hidden);
        w.u32(self.brain.logical_action_channels);
        w.u32(self.brain.policy_logits);
        w.u32(self.brain.value_outputs);
        w.u8(self.brain.learning as u8);
        w.bool(self.brain.writable_state_per_individual);
        w.f64_canonical(self.brain.leak_old)?;
        w.f64_canonical(self.brain.leak_new)?;
        w.f64_canonical(self.brain.gamma_tick)?;
        w.f64_canonical(self.brain.lambda_trace)?;
        w.f64_canonical(self.brain.actor_lr)?;
        w.f64_canonical(self.brain.critic_lr)?;
        w.f64_canonical(self.brain.entropy_coef)?;
        w.f64_canonical(self.brain.baseline_coef)?;
        w.f64_canonical(self.brain.delta_clip)?;
        w.f64_canonical(self.brain.e_a_radius)?;
        w.f64_canonical(self.brain.e_v_radius)?;
        w.f64_canonical(self.brain.d_a_radius)?;
        w.f64_canonical(self.brain.d_v_radius)?;
        w.f64_canonical(self.brain.a_row_radius)?;
        w.f64_canonical(self.brain.v_radius)?;
        w.f64_canonical(self.brain.w_rec_l1)?;
        w.f64_canonical(self.brain.reward_bias)?;
        w.f64_canonical(self.brain.reward_hdev)?;
        w.f64_canonical(self.brain.reward_birth)?;
        w.f64_canonical(self.brain.reward_death)?;
        w.u64(self.brain.reward_e_target);
        w.u64(self.brain.reward_c_target);
        w.u64(self.brain.sense_c_n);
        w.u64(self.brain.sense_c_o);
        w.u64(self.brain.sense_c_s);
        w.u64(self.brain.sense_c_q);
        w.f64_canonical(self.brain.sense_range_lu)?;
        w.bool(self.memory.associative_enabled);
        w.bool(self.memory.experience_inheritance);
        w.u8(self.evolution.reproduction as u8);
        w.u8(self.evolution.mutation_profile as u8);
        w.bool(self.evolution.morphology_mutation);
        w.u64(self.evolution.min_age);
        w.u64(self.evolution.min_b_over_target_numer);
        w.u64(self.evolution.min_b_over_target_denom);
        w.u64(self.evolution.min_c);
        w.u64(self.evolution.min_e_old);
        w.u64(self.evolution.start_fee);
        w.u64(self.evolution.target_extent);
        w.u64(self.evolution.max_extent_per_tick);
        w.u64(self.evolution.extra_maintenance);
        w.u64(self.evolution.timeout_ticks);
        w.u64(self.evolution.ready_place_ticks);
        w.u64(self.evolution.place_every_ticks);
        w.u32(self.evolution.place_angles);
        w.u32(self.evolution.mutation_p_numer);
        w.u32(self.evolution.mutation_p_denom);
        w.i32(self.evolution.mutation_delta);
        w.i32(self.evolution.q_win_init_abs);
        w.i32(self.evolution.q_wrec_diag);
        w.i32(self.evolution.q_wrec_off_abs);
        w.bool(self.evolution.signal_enabled);
        w.bool(self.evolution.transfer_enabled);
        w.bool(self.evolution.attack_enabled);
        w.bool(self.evolution.adhere_enabled);
        w.u64(self.reactions.chem_e_per_organic);
        w.u64(self.reactions.photo_q_per);
        w.u64(self.reactions.photo_dissipate);
        w.u64(self.reactions.metab_e_new);
        w.u64(self.reactions.metab_dissipate);
        w.u64(self.reactions.repair_e_old);
        w.u64(self.reactions.repair_dissipate);
        w.u64(self.reactions.attack_e_old);
        w.u64(self.reactions.attack_dissipate);
        w.u64(self.reactions.signal_e_old);
        w.u64(self.reactions.signal_dissipate);
        w.u64(self.reactions.transfer_e_old);
        w.u64(self.reactions.transfer_dissipate);
        w.u64(self.reactions.embryo_c_in);
        w.u64(self.reactions.embryo_e_old);
        w.u64(self.reactions.embryo_b_out);
        w.u64(self.reactions.embryo_c_out);
        w.u64(self.reactions.embryo_e_out);
        w.u64(self.reactions.embryo_dissipate);
        w.u64(self.archive.checkpoint_every_ticks);
        w.u64(self.archive.state_hash_every_ticks);
        w.u64(self.archive.aggregate_every_ticks);
        w.u32(self.safety.max_living);
        w.u32(self.safety.max_contact_pairs);
        w.u64(self.safety.max_unwritten_archive_bytes);
        Ok(w.into_inner())
    }
}

pub fn parse_rule_yaml(text: &str) -> Result<RuleSpec, ConfigError> {
    let spec: RuleSpec = serde_yaml::from_str(text)
        .map_err(|e| ConfigError::UnknownOrMalformed(e.to_string()))?;
    spec.validate()?;
    Ok(spec)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExperimentSpec {
    pub root_seed: Option<u64>,
    pub stop_tick: Option<u64>,
    pub freeze_learning: bool,
    pub disable_active_motion: bool,
    pub disable_reproduction: bool,
}

impl Default for ExperimentSpec {
    fn default() -> Self {
        Self {
            root_seed: None,
            stop_tick: None,
            freeze_learning: false,
            disable_active_motion: false,
            disable_reproduction: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationSpec {
    pub narrative_enabled: bool,
    pub narrative_mode: String,
    pub window_ticks: u64,
    pub backend_profile: Option<String>,
    pub fallback: String,
}

impl Default for PresentationSpec {
    fn default() -> Self {
        Self {
            narrative_enabled: true,
            narrative_mode: "scheduled_chronicle".into(),
            window_ticks: 3000,
            backend_profile: None,
            fallback: "deterministic_templates".into(),
        }
    }
}
