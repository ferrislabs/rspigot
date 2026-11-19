#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SheepKind {
    Explosive,
    Incendiary,
    Frozen,
    Seeker,
    Lightning,
    Healing,
    Boarding,
    Quake,
    Darkness,
    Splitting,
}

impl SheepKind {
    pub const ALL: [Self; 10] = [
        Self::Explosive,
        Self::Incendiary,
        Self::Frozen,
        Self::Seeker,
        Self::Lightning,
        Self::Healing,
        Self::Boarding,
        Self::Quake,
        Self::Darkness,
        Self::Splitting,
    ];
}
