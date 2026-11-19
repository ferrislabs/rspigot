#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SheepKind {
    Lightning,
    Incendiary,
    Boarding,
    Healing,
    Seeker,
    Splitting,
    Darkness,
    Frozen,
    Quake,
    Warp,
    Intergalactic,
}

impl SheepKind {
    pub const ALL: [Self; 11] = [
        Self::Lightning,
        Self::Incendiary,
        Self::Boarding,
        Self::Healing,
        Self::Seeker,
        Self::Splitting,
        Self::Darkness,
        Self::Frozen,
        Self::Quake,
        Self::Warp,
        Self::Intergalactic,
    ];
}
