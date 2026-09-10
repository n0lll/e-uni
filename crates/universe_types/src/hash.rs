use sha2::{Digest, Sha256};

use crate::canon::{MAGIC_STATE, SCHEMA_VERSION};
use crate::error::NumericFault;
use crate::rules::RuleSpec;
use crate::state::WorldState;

pub type Hash32 = [u8; 32];

pub fn sha256(bytes: &[u8]) -> Hash32 {
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    let mut a = [0u8; 32];
    a.copy_from_slice(&out);
    a
}

pub fn hex_hash(h: &Hash32) -> String {
    hex::encode(h)
}

/// `rules_hash = SHA-256(canonical(RuleSpec))`. Never hash YAML text.
pub fn rules_hash(spec: &RuleSpec) -> Result<Hash32, NumericFault> {
    Ok(sha256(&spec.encode_canonical()?))
}

pub fn schema_header_state() -> [u8; 8] {
    let mut b = [0u8; 8];
    b[0..4].copy_from_slice(&MAGIC_STATE);
    b[4..8].copy_from_slice(&SCHEMA_VERSION.to_le_bytes());
    b
}

/// `state_hash = SHA-256(schema_header || rules_hash || canonical_state)`.
pub fn state_hash(state: &WorldState) -> Result<Hash32, NumericFault> {
    let canon = state.encode_canonical()?;
    let mut h = Sha256::new();
    h.update(schema_header_state());
    h.update(state.rules_hash);
    h.update(&canon);
    let out = h.finalize();
    let mut a = [0u8; 32];
    a.copy_from_slice(&out);
    Ok(a)
}

/// First typed path that differs. Empty worlds compare field-by-field.
pub fn diff_states(a: &WorldState, b: &WorldState) -> Option<String> {
    if a.tick != b.tick {
        return Some("tick".into());
    }
    if a.root_seed != b.root_seed {
        return Some("root_seed".into());
    }
    if a.rules_hash != b.rules_hash {
        return Some("rules_hash".into());
    }
    if a.next_entity_id != b.next_entity_id {
        return Some("next_entity_id".into());
    }
    if a.fields != b.fields {
        if a.fields.n != b.fields.n {
            return Some("fields.n".into());
        }
        if a.fields.o != b.fields.o {
            return Some("fields.o".into());
        }
        if a.fields.s != b.fields.s {
            return Some("fields.s".into());
        }
        if a.fields.q != b.fields.q {
            return Some("fields.q".into());
        }
        if a.fields.s_remainder != b.fields.s_remainder {
            return Some("fields.s_remainder".into());
        }
        if a.fields.q_remainder != b.fields.q_remainder {
            return Some("fields.q_remainder".into());
        }
        return Some("fields".into());
    }
    if a.organisms.len() != b.organisms.len() {
        return Some("organisms.len".into());
    }
    for (i, (oa, ob)) in a.organisms.iter().zip(b.organisms.iter()).enumerate() {
        if oa != ob {
            if oa.brain.e_a != ob.brain.e_a {
                return Some(format!("organisms[{i}].brain.e_a"));
            }
            if oa.brain.a != ob.brain.a {
                return Some(format!("organisms[{i}].brain.a"));
            }
            if oa.accounts != ob.accounts {
                return Some(format!("organisms[{i}].accounts"));
            }
            return Some(format!("organisms[{i}]"));
        }
    }
    if a.genomes != b.genomes {
        return Some("genomes".into());
    }
    if a.embryos != b.embryos {
        return Some("embryos".into());
    }
    if a.future_inputs != b.future_inputs {
        return Some("future_inputs".into());
    }
    if a.audit != b.audit {
        return Some("audit_totals".into());
    }
    None
}
