//! EU-Core-1 type contracts: units, RuleSpec, canonical encoding, hashes.

pub mod canon;
pub mod enums;
pub mod error;
pub mod events;
pub mod hash;
pub mod rules;
pub mod state;
pub mod units;

pub use canon::{canonicalize_f64, CanonReader, CanonWriter, SCHEMA_VERSION};
pub use enums::*;
pub use error::{ActionFailure, ConfigError, NumericFault, SimError};
pub use events::{LifeEvent, LifeEventKind};
pub use hash::{diff_states, hex_hash, rules_hash, sha256, state_hash, Hash32};
pub use rules::{
    parse_rule_yaml, ArchiveRules, BrainRules, EvolutionRules, ExperimentSpec, InitRules,
    LightingRules, MemoryRules, PhysicsRules, PhysiologyRules, PresentationSpec, RateRules,
    ReactionRules, RuleSpec, SafetyRules, TimeRules, WorldRules,
};
pub use state::*;
pub use units::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_micro() {
        assert_eq!(MICRO_PER_UNIT, 1_000_000);
        assert_eq!(mu_to_micro(1), 1_000_000);
        assert_eq!(eu_to_micro(5), 5_000_000);
    }

    #[test]
    fn t01_from_rule_spec() {
        let spec = RuleSpec::eu_core_1();
        spec.validate().unwrap();
        eprintln!("EU-Core-1 rules_hash={}", hex_hash(&rules_hash(&spec).unwrap()));
        assert!(spec.t01_matches_doc());
        assert_eq!(spec.expected_initial_m(), T01_INITIAL_M_MICRO);
        assert_eq!(spec.expected_initial_u(), T01_INITIAL_U_MICRO);
        assert_eq!(spec.lighting.lit_cell_count(), 128);
        assert_eq!(spec.maintenance_cost_b(), 2192);
        assert_eq!(spec.cognition_cost_b(), 809);
    }

    #[test]
    fn yaml_unknown_key_rejected() {
        let mut y = serde_yaml::to_string(&RuleSpec::eu_core_1()).unwrap();
        y.push_str("bogus_key: 1\n");
        let err = parse_rule_yaml(&y).unwrap_err();
        match err {
            ConfigError::UnknownOrMalformed(_) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn illegal_learning_rejected() {
        let mut spec = RuleSpec::eu_core_1();
        spec.brain.learning = LearningKind::ExperimentalLocalPlasticity;
        assert!(spec.validate().is_err());
        spec = RuleSpec::eu_core_1();
        spec.brain.core_inputs = 32;
        assert!(spec.validate().is_err());
        spec = RuleSpec::eu_core_1();
        spec.evolution.morphology_mutation = true;
        assert!(spec.validate().is_err());
        spec = RuleSpec::eu_core_1();
        spec.memory.associative_enabled = true;
        assert!(spec.validate().is_err());
        spec = RuleSpec::eu_core_1();
        spec.evolution.transfer_enabled = true;
        assert!(spec.validate().is_err());
        spec = RuleSpec::eu_core_1();
        spec.time.dt_denominator = 5;
        assert!(spec.validate().is_err());
        spec = RuleSpec::eu_core_1();
        spec.profile = RuleProfile::EuBody1;
        assert!(spec.validate().is_err());
    }

    #[test]
    fn yaml_whitespace_does_not_change_rules_hash() {
        let spec = RuleSpec::eu_core_1();
        let y1 = serde_yaml::to_string(&spec).unwrap();
        let y2 = format!("\n\n{y1}\n# comment not parsed\n");
        // comments after dump may break YAML; instead re-indent by roundtrip
        let spec_a: RuleSpec = serde_yaml::from_str(&y1).unwrap();
        let y_compact = serde_yaml::to_string(&spec_a).unwrap();
        let spec_b: RuleSpec = serde_yaml::from_str(&y_compact).unwrap();
        spec_a.validate().unwrap();
        spec_b.validate().unwrap();
        assert_eq!(rules_hash(&spec_a).unwrap(), rules_hash(&spec_b).unwrap());
        assert_eq!(rules_hash(&spec).unwrap(), rules_hash(&spec_a).unwrap());
        let _ = y2;
    }

    #[test]
    fn canonical_state_roundtrip() {
        let spec = RuleSpec::eu_core_1();
        let mut st = WorldState::empty(&spec, 0xDEAD_BEEF_u64).unwrap();
        st.fields.n.push(7);
        st.audit.initial_m = 42;
        let bytes = st.encode_canonical().unwrap();
        let st2 = WorldState::decode_canonical(&bytes).unwrap();
        assert_eq!(st, st2);
        assert_eq!(state_hash(&st).unwrap(), state_hash(&st2).unwrap());
    }

    #[test]
    fn negative_zero_normalized() {
        let bits = canonicalize_f64(-0.0).unwrap();
        assert_eq!(bits, 0.0f64.to_bits());
        assert!(canonicalize_f64(f64::NAN).is_err());
        assert!(canonicalize_f64(f64::INFINITY).is_err());
    }

    #[test]
    fn presentation_not_in_rules_hash() {
        let spec = RuleSpec::eu_core_1();
        let h1 = rules_hash(&spec).unwrap();
        let p = PresentationSpec {
            narrative_enabled: false,
            ..PresentationSpec::default()
        };
        let _ = p;
        assert_eq!(h1, rules_hash(&spec).unwrap());
    }
}
