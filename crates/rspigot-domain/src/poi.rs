use rspigot_blocks::{BlockKind, StateId};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::chunk::ChunkColumn;
use crate::pos::{BlockPos, ChunkPos, SectionPos};
use crate::world::WorldHeight;

pub const MAX_VILLAGE_DISTANCE: i32 = 6;
pub const VILLAGE_SECTION_SIZE: i32 = 1;
pub const NO_VILLAGE: i32 = 7;
const CENTER_REACH: i32 = MAX_VILLAGE_DISTANCE * 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PoiType {
    Armorer,
    Butcher,
    Cartographer,
    Cleric,
    Farmer,
    Fisherman,
    Fletcher,
    Leatherworker,
    Librarian,
    Mason,
    Shepherd,
    Toolsmith,
    Weaponsmith,
    Home,
    Meeting,
    Beehive,
    BeeNest,
    NetherPortal,
    Lodestone,
    TestInstance,
    LightningRod,
}

impl PoiType {
    pub const ALL: [Self; 21] = [
        Self::Armorer,
        Self::Butcher,
        Self::Cartographer,
        Self::Cleric,
        Self::Farmer,
        Self::Fisherman,
        Self::Fletcher,
        Self::Leatherworker,
        Self::Librarian,
        Self::Mason,
        Self::Shepherd,
        Self::Toolsmith,
        Self::Weaponsmith,
        Self::Home,
        Self::Meeting,
        Self::Beehive,
        Self::BeeNest,
        Self::NetherPortal,
        Self::Lodestone,
        Self::TestInstance,
        Self::LightningRod,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Armorer => "minecraft:armorer",
            Self::Butcher => "minecraft:butcher",
            Self::Cartographer => "minecraft:cartographer",
            Self::Cleric => "minecraft:cleric",
            Self::Farmer => "minecraft:farmer",
            Self::Fisherman => "minecraft:fisherman",
            Self::Fletcher => "minecraft:fletcher",
            Self::Leatherworker => "minecraft:leatherworker",
            Self::Librarian => "minecraft:librarian",
            Self::Mason => "minecraft:mason",
            Self::Shepherd => "minecraft:shepherd",
            Self::Toolsmith => "minecraft:toolsmith",
            Self::Weaponsmith => "minecraft:weaponsmith",
            Self::Home => "minecraft:home",
            Self::Meeting => "minecraft:meeting",
            Self::Beehive => "minecraft:beehive",
            Self::BeeNest => "minecraft:bee_nest",
            Self::NetherPortal => "minecraft:nether_portal",
            Self::Lodestone => "minecraft:lodestone",
            Self::TestInstance => "minecraft:test_instance",
            Self::LightningRod => "minecraft:lightning_rod",
        }
    }

    pub const fn max_tickets(self) -> i32 {
        match self {
            Self::Meeting => 32,
            Self::Beehive
            | Self::BeeNest
            | Self::NetherPortal
            | Self::Lodestone
            | Self::TestInstance
            | Self::LightningRod => 0,
            _ => 1,
        }
    }

    pub const fn valid_range(self) -> i32 {
        match self {
            Self::Meeting => 6,
            _ => 1,
        }
    }

    pub const fn is_acquirable_job_site(self) -> bool {
        matches!(
            self,
            Self::Armorer
                | Self::Butcher
                | Self::Cartographer
                | Self::Cleric
                | Self::Farmer
                | Self::Fisherman
                | Self::Fletcher
                | Self::Leatherworker
                | Self::Librarian
                | Self::Mason
                | Self::Shepherd
                | Self::Toolsmith
                | Self::Weaponsmith
        )
    }

    pub const fn is_village(self) -> bool {
        self.is_acquirable_job_site() || matches!(self, Self::Home | Self::Meeting)
    }

    pub const fn is_bee_home(self) -> bool {
        matches!(self, Self::Beehive | Self::BeeNest)
    }

    pub fn of_kind(kind: BlockKind) -> Option<Self> {
        Some(match kind {
            BlockKind::BlastFurnace => Self::Armorer,
            BlockKind::Smoker => Self::Butcher,
            BlockKind::CartographyTable => Self::Cartographer,
            BlockKind::BrewingStand => Self::Cleric,
            BlockKind::Composter => Self::Farmer,
            BlockKind::Barrel => Self::Fisherman,
            BlockKind::FletchingTable => Self::Fletcher,
            BlockKind::Cauldron
            | BlockKind::LavaCauldron
            | BlockKind::WaterCauldron
            | BlockKind::PowderSnowCauldron => Self::Leatherworker,
            BlockKind::Lectern => Self::Librarian,
            BlockKind::Stonecutter => Self::Mason,
            BlockKind::Loom => Self::Shepherd,
            BlockKind::SmithingTable => Self::Toolsmith,
            BlockKind::Grindstone => Self::Weaponsmith,
            BlockKind::WhiteBed
            | BlockKind::OrangeBed
            | BlockKind::MagentaBed
            | BlockKind::LightBlueBed
            | BlockKind::YellowBed
            | BlockKind::LimeBed
            | BlockKind::PinkBed
            | BlockKind::GrayBed
            | BlockKind::LightGrayBed
            | BlockKind::CyanBed
            | BlockKind::PurpleBed
            | BlockKind::BlueBed
            | BlockKind::BrownBed
            | BlockKind::GreenBed
            | BlockKind::RedBed
            | BlockKind::BlackBed => Self::Home,
            BlockKind::Bell => Self::Meeting,
            BlockKind::Beehive => Self::Beehive,
            BlockKind::BeeNest => Self::BeeNest,
            BlockKind::NetherPortal => Self::NetherPortal,
            BlockKind::Lodestone => Self::Lodestone,
            BlockKind::TestInstanceBlock => Self::TestInstance,
            BlockKind::LightningRod
            | BlockKind::ExposedLightningRod
            | BlockKind::WeatheredLightningRod
            | BlockKind::OxidizedLightningRod
            | BlockKind::WaxedLightningRod
            | BlockKind::WaxedExposedLightningRod
            | BlockKind::WaxedWeatheredLightningRod
            | BlockKind::WaxedOxidizedLightningRod => Self::LightningRod,
            _ => return None,
        })
    }

    pub fn of_state(state: StateId) -> Option<Self> {
        let kind = state.kind()?;
        let poi = Self::of_kind(kind)?;
        if poi == Self::Home && state.property("part") != Some("head") {
            return None;
        }
        Some(poi)
    }
}

pub fn has_poi(state: StateId) -> bool {
    PoiType::of_state(state).is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PoiRecord {
    pos: BlockPos,
    kind: PoiType,
    free_tickets: i32,
}

impl PoiRecord {
    pub fn new(pos: BlockPos, kind: PoiType) -> Self {
        Self {
            pos,
            kind,
            free_tickets: kind.max_tickets(),
        }
    }

    pub const fn pos(&self) -> BlockPos {
        self.pos
    }

    pub const fn kind(&self) -> PoiType {
        self.kind
    }

    pub const fn free_tickets(&self) -> i32 {
        self.free_tickets
    }

    pub const fn has_space(&self) -> bool {
        self.free_tickets > 0
    }

    pub const fn is_occupied(&self) -> bool {
        self.free_tickets != self.kind.max_tickets()
    }

    fn acquire_ticket(&mut self) -> bool {
        if self.free_tickets <= 0 {
            return false;
        }
        self.free_tickets -= 1;
        true
    }

    fn release_ticket(&mut self) -> bool {
        if self.free_tickets >= self.kind.max_tickets() {
            return false;
        }
        self.free_tickets += 1;
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Occupancy {
    HasSpace,
    IsOccupied,
    Any,
}

impl Occupancy {
    pub const fn test(self, record: &PoiRecord) -> bool {
        match self {
            Self::HasSpace => record.has_space(),
            Self::IsOccupied => record.is_occupied(),
            Self::Any => true,
        }
    }
}

fn dist_sqr(a: BlockPos, b: BlockPos) -> i64 {
    let dx = i64::from(a.x) - i64::from(b.x);
    let dy = i64::from(a.y) - i64::from(b.y);
    let dz = i64::from(a.z) - i64::from(b.z);
    dx * dx + dy * dy + dz * dz
}

#[derive(Debug, Clone, Default)]
pub struct PoiSection {
    records: FxHashMap<u16, PoiRecord>,
}

impl PoiSection {
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn get(&self, pos: BlockPos) -> Option<&PoiRecord> {
        self.records.get(&SectionPos::relative(pos))
    }

    fn get_mut(&mut self, pos: BlockPos) -> Option<&mut PoiRecord> {
        self.records.get_mut(&SectionPos::relative(pos))
    }

    fn add(&mut self, pos: BlockPos, kind: PoiType) -> bool {
        let key = SectionPos::relative(pos);
        if self.records.get(&key).is_some_and(|old| old.kind == kind) {
            return false;
        }
        self.records.insert(key, PoiRecord::new(pos, kind));
        true
    }

    fn remove(&mut self, pos: BlockPos) -> bool {
        self.records.remove(&SectionPos::relative(pos)).is_some()
    }

    pub fn records(
        &self,
        predicate: impl Fn(PoiType) -> bool,
        occupancy: Occupancy,
    ) -> Vec<PoiRecord> {
        let mut found: Vec<(u16, PoiRecord)> = self
            .records
            .iter()
            .filter(|(_, record)| predicate(record.kind) && occupancy.test(record))
            .map(|(key, record)| (*key, *record))
            .collect();
        found.sort_unstable_by_key(|(key, _)| *key);
        found.into_iter().map(|(_, record)| record).collect()
    }

    fn is_village_center(&self) -> bool {
        self.records
            .values()
            .any(|record| record.kind.is_village() && record.is_occupied())
    }
}

#[derive(Debug, Clone)]
pub struct PoiManager {
    height: WorldHeight,
    sections: FxHashMap<SectionPos, PoiSection>,
    centers: FxHashSet<SectionPos>,
    levels: FxHashMap<SectionPos, u8>,
}

impl PoiManager {
    pub fn new(height: WorldHeight) -> Self {
        Self {
            height,
            sections: FxHashMap::default(),
            centers: FxHashSet::default(),
            levels: FxHashMap::default(),
        }
    }

    fn min_section(&self) -> i32 {
        self.height.min_y >> 4
    }

    fn max_section(&self) -> i32 {
        self.height.max_y() >> 4
    }

    pub fn section_count(&self) -> usize {
        self.sections.len()
    }

    pub fn record_count(&self) -> usize {
        self.sections.values().map(PoiSection::len).sum()
    }

    pub fn add(&mut self, pos: BlockPos, kind: PoiType) -> bool {
        let section = SectionPos::of(pos);
        let added = self.sections.entry(section).or_default().add(pos, kind);
        if added {
            self.refresh_center(section);
        }
        added
    }

    pub fn remove(&mut self, pos: BlockPos) -> bool {
        let section = SectionPos::of(pos);
        let Some(entry) = self.sections.get_mut(&section) else {
            return false;
        };
        let removed = entry.remove(pos);
        if entry.is_empty() {
            self.sections.remove(&section);
        }
        if removed {
            self.refresh_center(section);
        }
        removed
    }

    pub fn on_block_change(&mut self, pos: BlockPos, old: StateId, new: StateId) {
        let old_kind = PoiType::of_state(old);
        let new_kind = PoiType::of_state(new);
        if old_kind == new_kind {
            return;
        }
        if old_kind.is_some() {
            self.remove(pos);
        }
        if let Some(kind) = new_kind {
            self.add(pos, kind);
        }
    }

    pub fn load_chunk(&mut self, chunk: &ChunkColumn) {
        let pos = chunk.pos();
        self.unload_chunk(pos);
        let min_section = chunk.height().min_y >> 4;
        for (index, section) in chunk.sections().iter().enumerate() {
            if !section.blocks().palette().iter().copied().any(has_poi) {
                continue;
            }
            let sy = min_section + index as i32;
            let key = SectionPos::from_chunk(pos, sy);
            let origin = key.origin();
            let mut found = PoiSection::default();
            for y in 0..16usize {
                for z in 0..16usize {
                    for x in 0..16usize {
                        if let Some(kind) = PoiType::of_state(section.block(x, y, z)) {
                            found.add(origin.offset(x as i32, y as i32, z as i32), kind);
                        }
                    }
                }
            }
            if !found.is_empty() {
                self.sections.insert(key, found);
                self.refresh_center(key);
            }
        }
    }

    pub fn unload_chunk(&mut self, pos: ChunkPos) {
        for sy in self.min_section()..=self.max_section() {
            let key = SectionPos::from_chunk(pos, sy);
            if self.sections.remove(&key).is_some() {
                self.refresh_center(key);
            }
        }
    }

    pub fn exists(&self, pos: BlockPos, predicate: impl Fn(PoiType) -> bool) -> bool {
        self.type_at(pos).is_some_and(predicate)
    }

    pub fn type_at(&self, pos: BlockPos) -> Option<PoiType> {
        self.record_at(pos).map(|record| record.kind())
    }

    pub fn record_at(&self, pos: BlockPos) -> Option<PoiRecord> {
        self.sections
            .get(&SectionPos::of(pos))
            .and_then(|section| section.get(pos))
            .copied()
    }

    pub fn in_chunk(
        &self,
        predicate: &impl Fn(PoiType) -> bool,
        chunk: ChunkPos,
        occupancy: Occupancy,
    ) -> Vec<PoiRecord> {
        let mut found = Vec::new();
        for sy in self.min_section()..=self.max_section() {
            if let Some(section) = self.sections.get(&SectionPos::from_chunk(chunk, sy)) {
                found.extend(section.records(predicate, occupancy));
            }
        }
        found
    }

    pub fn in_square(
        &self,
        predicate: impl Fn(PoiType) -> bool,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Vec<PoiRecord> {
        let chunk_radius = radius.div_euclid(16) + 1;
        let origin = center.chunk();
        let mut found = Vec::new();
        for z in origin.z - chunk_radius..=origin.z + chunk_radius {
            for x in origin.x - chunk_radius..=origin.x + chunk_radius {
                found.extend(self.in_chunk(&predicate, ChunkPos::new(x, z), occupancy));
            }
        }
        found.retain(|record| {
            (record.pos.x - center.x).abs() <= radius && (record.pos.z - center.z).abs() <= radius
        });
        found
    }

    pub fn in_range(
        &self,
        predicate: impl Fn(PoiType) -> bool,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Vec<PoiRecord> {
        let radius_sqr = i64::from(radius.wrapping_mul(radius));
        let mut found = self.in_square(predicate, center, radius, occupancy);
        found.retain(|record| dist_sqr(record.pos, center) <= radius_sqr);
        found
    }

    pub fn count_in_range(
        &self,
        predicate: impl Fn(PoiType) -> bool,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> usize {
        self.in_range(predicate, center, radius, occupancy).len()
    }

    pub fn find(
        &self,
        predicate: impl Fn(PoiType) -> bool,
        filter: impl Fn(BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Option<BlockPos> {
        self.in_range(predicate, center, radius, occupancy)
            .into_iter()
            .map(|record| record.pos)
            .find(|pos| filter(*pos))
    }

    pub fn find_closest(
        &self,
        predicate: impl Fn(PoiType) -> bool,
        center: BlockPos,
        radius: i32,
        occupancy: Occupancy,
    ) -> Option<BlockPos> {
        self.in_range(predicate, center, radius, occupancy)
            .into_iter()
            .map(|record| record.pos)
            .min_by_key(|pos| dist_sqr(*pos, center))
    }

    pub fn take(
        &mut self,
        predicate: impl Fn(PoiType) -> bool,
        filter: impl Fn(PoiType, BlockPos) -> bool,
        center: BlockPos,
        radius: i32,
    ) -> Option<BlockPos> {
        let pos = self
            .in_range(predicate, center, radius, Occupancy::HasSpace)
            .into_iter()
            .find(|record| filter(record.kind, record.pos))?
            .pos;
        let section = SectionPos::of(pos);
        self.sections
            .get_mut(&section)?
            .get_mut(pos)?
            .acquire_ticket();
        self.refresh_center(section);
        Some(pos)
    }

    pub fn release(&mut self, pos: BlockPos) -> Option<bool> {
        let section = SectionPos::of(pos);
        let released = self
            .sections
            .get_mut(&section)?
            .get_mut(pos)?
            .release_ticket();
        self.refresh_center(section);
        Some(released)
    }

    pub fn sections_to_village(&self, section: SectionPos) -> i32 {
        self.levels
            .get(&section)
            .map_or(NO_VILLAGE, |level| i32::from(*level))
    }

    fn refresh_center(&mut self, section: SectionPos) {
        let center = self
            .sections
            .get(&section)
            .is_some_and(PoiSection::is_village_center);
        if center == self.centers.contains(&section) {
            return;
        }
        if center {
            self.centers.insert(section);
            self.spread_from(section);
        } else {
            self.centers.remove(&section);
            self.recompute_around(section);
        }
    }

    fn spread_from(&mut self, source: SectionPos) {
        for section in source.cube(MAX_VILLAGE_DISTANCE) {
            let level = source.chebyshev_distance(section) as u8;
            let entry = self.levels.entry(section).or_insert(level);
            if level < *entry {
                *entry = level;
            }
        }
    }

    fn recompute_around(&mut self, removed: SectionPos) {
        let nearby: Vec<SectionPos> = self
            .centers
            .iter()
            .copied()
            .filter(|center| center.chebyshev_distance(removed) <= CENTER_REACH)
            .collect();
        for section in removed.cube(MAX_VILLAGE_DISTANCE) {
            let level = nearby
                .iter()
                .map(|center| center.chebyshev_distance(section))
                .filter(|distance| *distance <= MAX_VILLAGE_DISTANCE)
                .min();
            match level {
                Some(level) => {
                    self.levels.insert(section, level as u8);
                }
                None => {
                    self.levels.remove(&section);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
