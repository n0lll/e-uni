//! Shared error types. World-internal action failures are not `SimError`.

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConfigError {
    #[error("unknown configuration key or illegal YAML: {0}")]
    UnknownOrMalformed(String),
    #[error("illegal rule combination: {0}")]
    IllegalCombination(String),
    #[error("field {field} out of range: {detail}")]
    OutOfRange { field: &'static str, detail: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NumericFault {
    #[error("non-finite f64")]
    NonFinite,
    #[error("integer overflow: {0}")]
    Overflow(&'static str),
    #[error("range sample exhausted 32 reserved u32 slots")]
    RangeSampleExhausted,
    #[error("contact solver unresolved")]
    ContactSolverUnresolved,
    #[error("random field overflow: {0}")]
    RngFieldOverflow(&'static str),
    #[error("capacity or account would go negative")]
    NegativeAccount,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SimError {
    #[error("rule violation: {0}")]
    RuleViolation(String),
    #[error(transparent)]
    Numeric(#[from] NumericFault),
    #[error("archive failure: {0}")]
    ArchiveFailure(String),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("safety pause: {0}")]
    SafetyPause(String),
    #[error("experiment setup: {0}")]
    ExperimentSetup(String),
}

/// Normal world-internal failures. They do not pause the universe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ActionFailure {
    NoSource = 1,
    NoCapacity = 2,
    NoContact = 3,
    NoCapability = 4,
    InsufficientOldEnergy = 5,
    NotMature = 6,
    EmbryoExists = 7,
    ParentDead = 8,
    NoSpace = 9,
    IdempotentReplay = 10,
}

impl ActionFailure {
    pub fn code(self) -> u8 {
        self as u8
    }
}
