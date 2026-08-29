#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct Entity {
    pub id: u32,
    pub generation: u32,
}

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[repr(C)]
pub struct EntityRecord {
    pub generation: u32,
    pub archetype_id: u32,
    pub row: u32,
}

impl EntityRecord {
    pub const DEAD: Self = Self {
        generation: 0,
        archetype_id: u32::MAX,
        row: 0,
    };
}
