//! Non-authoritative life events. They are archived, never hashed into WorldState.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LifeEventKind {
    Death = 1,
    Birth = 2,
    Lineage = 3,
    EmbryoCancel = 4,
    EmbryoStart = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifeEvent {
    pub tick: u64,
    pub kind: LifeEventKind,
    pub entity_a: u64,
    pub entity_b: u64,
    pub amount: u64,
}

impl LifeEventKind {
    pub fn code(self) -> u8 {
        self as u8
    }

    pub fn from_code(c: u8) -> Option<Self> {
        match c {
            1 => Some(Self::Death),
            2 => Some(Self::Birth),
            3 => Some(Self::Lineage),
            4 => Some(Self::EmbryoCancel),
            5 => Some(Self::EmbryoStart),
            _ => None,
        }
    }
}
