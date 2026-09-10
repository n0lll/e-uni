//! Stable numeric encodings for rule and random domains.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[repr(u8)]
pub enum RuleProfile {
    #[serde(rename = "EU-Core-1")]
    EuCore1 = 1,
    #[serde(rename = "EU-Body-1")]
    EuBody1 = 2,
    #[serde(rename = "EU-Social-1")]
    EuSocial1 = 3,
    #[serde(rename = "EU-Social-Pair-1")]
    EuSocialPair1 = 4,
}

impl RuleProfile {
    pub fn code(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Boundary {
    Closed = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum LearningKind {
    IndependentReadoutActorCriticV1 = 1,
    ExperimentalLocalPlasticity = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum ReproductionKind {
    AsexualEmbryoV1 = 1,
    SexualPairV1 = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum MutationKind {
    BoundedParameterMutationV1 = 1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RngDomain {
    InitialHeading = 1,
    InitialGenome = 2,
    PolicySample = 3,
    QuotaTie = 4,
    Mutation = 5,
    EmbryoHeading = 6,
    BirthPriority = 7,
    GeometryNormal = 8,
    SocialSenseTie = 9,
    StructureMutation = 10,
    SexualRecombination = 11,
    EmbryoCarrier = 12,
}

impl RngDomain {
    pub fn code(self) -> u8 {
        self as u8
    }
}

/// P0–P7 and P5 substages. `phase_code < 32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Phase {
    P0 = 0,
    P1 = 1,
    P2 = 2,
    P3 = 3,
    P4 = 4,
    P5a = 5,
    P5b = 6,
    P5c = 7,
    P5d = 8,
    P5e = 9,
    P5fRepair = 10,
    P5fEmbryo = 11,
    P5fTransfer = 12,
    P5fSignal = 13,
    P6 = 14,
    P7 = 15,
}

impl Phase {
    pub fn code(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PoolType {
    EnvN = 0,
    EnvO = 1,
    EnvS = 2,
    EnvQ = 3,
    SelfN = 4,
    SelfC = 5,
    SelfB = 6,
    SelfEOld = 7,
    CapN = 8,
    CapC = 9,
    CapE = 10,
    TargetB = 11,
    ReceiverC = 12,
}

impl PoolType {
    pub fn code(self) -> u8 {
        self as u8
    }
}

pub const TICK_BITS: u32 = 40;
pub const ACTOR_BITS: u32 = 44;
pub const OBJECT_BITS: u32 = 44;
pub const BLOCK_BITS: u32 = 24;
pub const DOMAIN_BITS: u32 = 8;

pub const TICK_MAX: u64 = (1u64 << TICK_BITS) - 1;
pub const ACTOR_MAX: u64 = (1u64 << ACTOR_BITS) - 1;
pub const OBJECT_MAX: u64 = (1u64 << OBJECT_BITS) - 1;
pub const BLOCK_MAX: u64 = (1u64 << BLOCK_BITS) - 1;

pub const HIDDEN: usize = 32;
pub const INPUTS: usize = 64;
pub const POLICY_LOGITS: usize = 16;
pub const PHI: usize = HIDDEN + 1;
pub const BINARY_HEADS: usize = 10;
pub const LOGICAL_CHANNELS: usize = 12;

pub const Q_SCALE: i32 = 32768;
