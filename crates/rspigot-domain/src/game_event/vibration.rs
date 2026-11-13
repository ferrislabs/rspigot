use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use super::event::{GameEvent, GameEventTag};
use crate::entity::physics::{BlockView, has_tag};
use crate::entity::save::{read_uuid, uuid_tag};
use crate::{BlockPos, BlockStateId, EntityId, Vec3, mth};

pub const OCCLUDES_VIBRATION_SIGNALS: &str = "minecraft:occludes_vibration_signals";
pub const DAMPENS_VIBRATIONS: &str = "minecraft:dampens_vibrations";
pub const VIBRATION_RESONATORS: &str = "minecraft:vibration_resonators";
pub const MAX_REDSTONE_STRENGTH: i32 = 15;
const OCCLUSION_OFFSET: f64 = 9.999_999_747_378_752e-6;
const TRAVERSE_EPSILON: f64 = -1.0e-7;
const EVENT: &str = "event";
const SELECTOR: &str = "selector";
const EVENT_DELAY: &str = "event_delay";
const GAME_EVENT: &str = "game_event";
const DISTANCE: &str = "distance";
const POS: &str = "pos";
const SOURCE: &str = "source";
const PROJECTILE_OWNER: &str = "projectile_owner";
const TICK: &str = "tick";
const NO_TICK: i64 = -1;
const AXIS_OFFSETS: [(f64, f64, f64); 6] = [
    (0.0, -OCCLUSION_OFFSET, 0.0),
    (0.0, OCCLUSION_OFFSET, 0.0),
    (0.0, 0.0, -OCCLUSION_OFFSET),
    (0.0, 0.0, OCCLUSION_OFFSET),
    (-OCCLUSION_OFFSET, 0.0, 0.0),
    (OCCLUSION_OFFSET, 0.0, 0.0),
];

#[derive(Debug, Clone, PartialEq)]
pub struct VibrationInfo {
    pub event: GameEvent,
    pub distance: f32,
    pub pos: Vec3,
    pub source: Option<Uuid>,
    pub projectile_owner: Option<Uuid>,
    pub entity: Option<EntityId>,
}

impl VibrationInfo {
    pub fn new(
        event: GameEvent,
        distance: f32,
        pos: Vec3,
        source: Option<Uuid>,
        projectile_owner: Option<Uuid>,
        entity: Option<EntityId>,
    ) -> Self {
        Self {
            event,
            distance,
            pos,
            source,
            projectile_owner,
            entity,
        }
    }

    pub fn to_nbt(&self) -> Compound {
        let mut out = Compound::new();
        out.insert(GAME_EVENT, self.event.name());
        out.insert(DISTANCE, self.distance);
        out.insert(
            POS,
            Tag::List(vec![
                Tag::Double(self.pos.x),
                Tag::Double(self.pos.y),
                Tag::Double(self.pos.z),
            ]),
        );
        if let Some(source) = self.source {
            out.insert(SOURCE, uuid_tag(source));
        }
        if let Some(owner) = self.projectile_owner {
            out.insert(PROJECTILE_OWNER, uuid_tag(owner));
        }
        out
    }

    pub fn from_nbt(nbt: &Compound) -> Option<Self> {
        let event = GameEvent::from_name(nbt.get_str(GAME_EVENT)?)?;
        let distance = match nbt.get(DISTANCE)? {
            Tag::Float(value) if *value >= 0.0 => *value,
            _ => return None,
        };
        let coordinates = nbt.get_list(POS).filter(|list| list.len() == 3)?;
        let pos = Vec3::new(
            coordinates[0].as_f64()?,
            coordinates[1].as_f64()?,
            coordinates[2].as_f64()?,
        );
        Some(Self::new(
            event,
            distance,
            pos,
            nbt.get(SOURCE).and_then(read_uuid),
            nbt.get(PROJECTILE_OWNER).and_then(read_uuid),
            None,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VibrationSelector {
    current: Option<(VibrationInfo, i64)>,
}

impl VibrationSelector {
    pub fn new(current: Option<VibrationInfo>, tick: i64) -> Self {
        Self {
            current: current.map(|info| (info, tick)),
        }
    }

    pub fn current(&self) -> Option<&VibrationInfo> {
        self.current.as_ref().map(|(info, _)| info)
    }

    fn should_replace(&self, candidate: &VibrationInfo, tick: i64) -> bool {
        let Some((current, at)) = &self.current else {
            return true;
        };
        if tick != *at {
            return false;
        }
        if candidate.distance < current.distance {
            return true;
        }
        if current.distance < candidate.distance {
            return false;
        }
        current.event.frequency() < candidate.event.frequency()
    }

    pub fn add_candidate(&mut self, candidate: VibrationInfo, tick: i64) {
        if self.should_replace(&candidate, tick) {
            self.current = Some((candidate, tick));
        }
    }

    pub fn chosen_candidate(&self, time: i64) -> Option<&VibrationInfo> {
        self.current
            .as_ref()
            .filter(|(_, tick)| *tick < time)
            .map(|(info, _)| info)
    }

    pub fn start_over(&mut self) {
        self.current = None;
    }

    pub fn to_nbt(&self) -> Compound {
        let mut out = Compound::new();
        if let Some((info, _)) = &self.current {
            out.insert(EVENT, info.to_nbt());
        }
        out.insert(
            TICK,
            self.current.as_ref().map_or(NO_TICK, |(_, tick)| *tick),
        );
        out
    }

    pub fn from_nbt(nbt: &Compound) -> Option<Self> {
        let tick = nbt.get(TICK).and_then(Tag::as_i64)?;
        let info = nbt.get_compound(EVENT).and_then(VibrationInfo::from_nbt);
        Some(Self::new(info, tick))
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VibrationData {
    pub current: Option<VibrationInfo>,
    pub selector: VibrationSelector,
    pub travel_time_in_ticks: i32,
    pub reload_particle: bool,
}

impl VibrationData {
    pub fn decrement_travel_time(&mut self) {
        self.travel_time_in_ticks = (self.travel_time_in_ticks - 1).max(0);
    }

    pub fn to_nbt(&self) -> Compound {
        let mut out = Compound::new();
        if let Some(info) = &self.current {
            out.insert(EVENT, info.to_nbt());
        }
        out.insert(SELECTOR, self.selector.to_nbt());
        out.insert(EVENT_DELAY, self.travel_time_in_ticks);
        out
    }

    pub fn from_nbt(nbt: &Compound) -> Option<Self> {
        let selector = VibrationSelector::from_nbt(nbt.get_compound(SELECTOR)?)?;
        let delay = match nbt.get(EVENT_DELAY) {
            None => 0,
            Some(Tag::Int(value)) if *value >= 0 => *value,
            Some(_) => return None,
        };
        Some(Self {
            current: nbt.get_compound(EVENT).and_then(VibrationInfo::from_nbt),
            selector,
            travel_time_in_ticks: delay,
            reload_particle: true,
        })
    }

    pub fn read_or_default(nbt: Option<&Compound>) -> Self {
        nbt.and_then(Self::from_nbt).unwrap_or_default()
    }
}

pub fn redstone_strength_for_distance(distance: f32, listener_radius: i32) -> i32 {
    let scaled = mth::floor((15.0 / f64::from(listener_radius)) * f64::from(distance));
    (MAX_REDSTONE_STRENGTH - scaled).max(1)
}

pub fn travel_time_in_ticks(distance: f32) -> i32 {
    mth::floor(f64::from(distance))
}

pub fn distance_between_in_blocks(origin: BlockPos, dest: BlockPos) -> f32 {
    let (dx, dy, dz) = (
        f64::from(origin.x - dest.x),
        f64::from(origin.y - dest.y),
        f64::from(origin.z - dest.z),
    );
    (dx * dx + dy * dy + dz * dz).sqrt() as f32
}

pub fn is_postable(listener: Vec3, source: Vec3, listener_radius: i32) -> bool {
    let (a, b) = (listener.block(), source.block());
    let (dx, dy, dz) = (
        f64::from(a.x - b.x),
        f64::from(a.y - b.y),
        f64::from(a.z - b.z),
    );
    dx * dx + dy * dy + dz * dz <= f64::from(listener_radius * listener_radius)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceFacts {
    pub spectator: bool,
    pub stepping_carefully: bool,
    pub dampens_vibrations: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validity {
    Valid,
    Invalid,
    AvoidedBySneaking,
}

pub fn validate(
    event: GameEvent,
    listenable: GameEventTag,
    source: Option<SourceFacts>,
    affected: Option<BlockStateId>,
    can_trigger_avoid: bool,
) -> Validity {
    if !event.is_in(listenable) {
        return Validity::Invalid;
    }
    if let Some(source) = source {
        if source.spectator {
            return Validity::Invalid;
        }
        if source.stepping_carefully && event.is_in(GameEventTag::IgnoreVibrationsSneaking) {
            return if can_trigger_avoid {
                Validity::AvoidedBySneaking
            } else {
                Validity::Invalid
            };
        }
        if source.dampens_vibrations {
            return Validity::Invalid;
        }
    }
    if affected.is_some_and(|state| has_tag(state, DAMPENS_VIBRATIONS)) {
        return Validity::Invalid;
    }
    Validity::Valid
}

fn traverse_blocks(from: Vec3, to: Vec3, mut hit: impl FnMut(BlockPos) -> bool) -> bool {
    if from == to {
        return false;
    }
    let to_x = mth::lerp(TRAVERSE_EPSILON, to.x, from.x);
    let to_y = mth::lerp(TRAVERSE_EPSILON, to.y, from.y);
    let to_z = mth::lerp(TRAVERSE_EPSILON, to.z, from.z);
    let from_x = mth::lerp(TRAVERSE_EPSILON, from.x, to.x);
    let from_y = mth::lerp(TRAVERSE_EPSILON, from.y, to.y);
    let from_z = mth::lerp(TRAVERSE_EPSILON, from.z, to.z);
    let (mut x, mut y, mut z) = (mth::floor(from_x), mth::floor(from_y), mth::floor(from_z));
    if hit(BlockPos::new(x, y, z)) {
        return true;
    }
    let (dx, dy, dz) = (to_x - from_x, to_y - from_y, to_z - from_z);
    let (sx, sy, sz) = (mth::sign(dx), mth::sign(dy), mth::sign(dz));
    let step = |sign: i32, delta: f64| {
        if sign == 0 {
            f64::MAX
        } else {
            f64::from(sign) / delta
        }
    };
    let (delta_x, delta_y, delta_z) = (step(sx, dx), step(sy, dy), step(sz, dz));
    let first = |sign: i32, delta: f64, from: f64| {
        delta
            * if sign > 0 {
                1.0 - mth::frac(from)
            } else {
                mth::frac(from)
            }
    };
    let mut t_x = first(sx, delta_x, from_x);
    let mut t_y = first(sy, delta_y, from_y);
    let mut t_z = first(sz, delta_z, from_z);
    while t_x <= 1.0 || t_y <= 1.0 || t_z <= 1.0 {
        if t_x < t_y {
            if t_x < t_z {
                x += sx;
                t_x += delta_x;
            } else {
                z += sz;
                t_z += delta_z;
            }
        } else if t_y < t_z {
            y += sy;
            t_y += delta_y;
        } else {
            z += sz;
            t_z += delta_z;
        }
        if hit(BlockPos::new(x, y, z)) {
            return true;
        }
    }
    false
}

fn block_center(pos: Vec3) -> Vec3 {
    Vec3::new(
        f64::from(mth::floor(pos.x)) + 0.5,
        f64::from(mth::floor(pos.y)) + 0.5,
        f64::from(mth::floor(pos.z)) + 0.5,
    )
}

pub fn is_occluded(view: &impl BlockView, origin: Vec3, dest: Vec3) -> bool {
    let origin = block_center(origin);
    let dest = block_center(dest);
    AXIS_OFFSETS.iter().all(|(dx, dy, dz)| {
        let from = Vec3::new(origin.x + dx, origin.y + dy, origin.z + dz);
        traverse_blocks(from, dest, |pos| {
            view.block(pos)
                .is_some_and(|state| has_tag(state, OCCLUDES_VIBRATION_SIGNALS))
        })
    })
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::BlockKind;
    use rustc_hash::FxHashMap;

    use super::*;

    struct Blocks(FxHashMap<BlockPos, BlockStateId>);

    impl BlockView for Blocks {
        fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
            Some(self.0.get(&pos).copied().unwrap_or(BlockStateId::AIR))
        }
    }

    fn info(event: GameEvent, distance: f32) -> VibrationInfo {
        VibrationInfo::new(
            event,
            distance,
            Vec3::new(1.5, 64.0, -2.5),
            None,
            None,
            None,
        )
    }

    #[test]
    fn registry_order_matches_the_game_event_registry() {
        for (index, event) in GameEvent::ALL.iter().enumerate() {
            let entries = rspigot_data::registries::static_entries(super::super::event::REGISTRY)
                .expect("registry");
            assert_eq!(entries[index], event.name());
            assert_eq!(GameEvent::from_name(event.name()), Some(*event));
        }
        assert_eq!(GameEvent::ALL.len(), 61);
    }

    #[test]
    fn frequencies_follow_the_vanilla_table() {
        assert_eq!(GameEvent::Step.frequency(), 1);
        assert_eq!(GameEvent::ProjectileLand.frequency(), 2);
        assert_eq!(GameEvent::ItemInteractFinish.frequency(), 3);
        assert_eq!(GameEvent::EntityAction.frequency(), 4);
        assert_eq!(GameEvent::Equip.frequency(), 5);
        assert_eq!(GameEvent::Shear.frequency(), 6);
        assert_eq!(GameEvent::EntityDamage.frequency(), 7);
        assert_eq!(GameEvent::Eat.frequency(), 8);
        assert_eq!(GameEvent::ContainerClose.frequency(), 9);
        assert_eq!(GameEvent::BlockActivate.frequency(), 10);
        assert_eq!(GameEvent::BlockChange.frequency(), 11);
        assert_eq!(GameEvent::BlockDestroy.frequency(), 12);
        assert_eq!(GameEvent::BlockPlace.frequency(), 13);
        assert_eq!(GameEvent::Teleport.frequency(), 14);
        assert_eq!(GameEvent::Explode.frequency(), 15);
        assert_eq!(GameEvent::Resonate7.frequency(), 7);
        assert_eq!(GameEvent::Shriek.frequency(), 0);
        assert_eq!(GameEvent::SculkSensorTendrilsClicking.frequency(), 0);
        assert_eq!(GameEvent::JukeboxPlay.frequency(), 0);
    }

    #[test]
    fn every_vibration_event_but_the_silent_ones_has_a_frequency() {
        for event in GameEvent::ALL {
            let vibrates = event.is_in(GameEventTag::Vibrations);
            assert_eq!(vibrates, event.frequency() > 0, "{}", event.name());
        }
    }

    #[test]
    fn notification_radii_default_to_sixteen() {
        assert_eq!(GameEvent::Step.notification_radius(), 16);
        assert_eq!(GameEvent::JukeboxPlay.notification_radius(), 10);
        assert_eq!(GameEvent::JukeboxStopPlay.notification_radius(), 10);
        assert_eq!(GameEvent::Shriek.notification_radius(), 32);
    }

    #[test]
    fn resonance_events_map_back_from_their_frequency() {
        assert_eq!(
            GameEvent::resonance_by_frequency(1),
            Some(GameEvent::Resonate1)
        );
        assert_eq!(
            GameEvent::resonance_by_frequency(15),
            Some(GameEvent::Resonate15)
        );
        assert_eq!(GameEvent::resonance_by_frequency(0), None);
        assert_eq!(GameEvent::resonance_by_frequency(16), None);
    }

    #[test]
    fn the_listener_tags_split_the_events() {
        assert!(GameEvent::Shriek.is_in(GameEventTag::WardenCanListen));
        assert!(!GameEvent::Shriek.is_in(GameEventTag::Vibrations));
        assert!(GameEvent::SculkSensorTendrilsClicking.is_in(GameEventTag::ShriekerCanListen));
        assert!(GameEvent::SculkSensorTendrilsClicking.is_in(GameEventTag::WardenCanListen));
        assert!(!GameEvent::Step.is_in(GameEventTag::ShriekerCanListen));
        assert!(GameEvent::Step.is_in(GameEventTag::IgnoreVibrationsSneaking));
        assert!(GameEvent::NoteBlockPlay.is_in(GameEventTag::AllayCanListen));
        assert!(!GameEvent::Explode.is_in(GameEventTag::IgnoreVibrationsSneaking));
        assert!(
            !GameEvent::Resonate1.is_in(GameEventTag::WardenCanListen)
                || GameEvent::Resonate1.is_in(GameEventTag::Vibrations)
        );
    }

    #[test]
    fn redstone_strength_falls_off_with_the_distance() {
        assert_eq!(redstone_strength_for_distance(0.0, 8), 15);
        assert_eq!(redstone_strength_for_distance(4.0, 8), 8);
        assert_eq!(redstone_strength_for_distance(8.0, 8), 1);
        assert_eq!(redstone_strength_for_distance(16.0, 8), 1);
        assert_eq!(redstone_strength_for_distance(8.0, 16), 8);
    }

    #[test]
    fn travel_time_is_the_floored_distance() {
        assert_eq!(travel_time_in_ticks(0.0), 0);
        assert_eq!(travel_time_in_ticks(7.9), 7);
        assert_eq!(travel_time_in_ticks(8.0), 8);
    }

    #[test]
    fn a_listener_hears_events_within_its_block_radius_only() {
        let listener = Vec3::new(0.5, 64.5, 0.5);
        assert!(is_postable(listener, Vec3::new(8.2, 64.5, 0.5), 8));
        assert!(!is_postable(listener, Vec3::new(9.2, 64.5, 0.5), 8));
        assert!(is_postable(listener, Vec3::new(5.5, 64.5, 5.5), 8));
        assert!(!is_postable(listener, Vec3::new(6.5, 64.5, 6.5), 8));
    }

    #[test]
    fn distances_between_blocks_are_euclidean() {
        let origin = BlockPos::new(0, 0, 0);
        assert_eq!(
            distance_between_in_blocks(origin, BlockPos::new(3, 4, 0)),
            5.0
        );
        assert_eq!(distance_between_in_blocks(origin, origin), 0.0);
    }

    #[test]
    fn the_selector_keeps_the_nearest_then_the_loudest_vibration_of_a_tick() {
        let mut selector = VibrationSelector::default();
        selector.add_candidate(info(GameEvent::Step, 5.0), 10);
        selector.add_candidate(info(GameEvent::Eat, 6.0), 10);
        assert_eq!(selector.current().map(|i| i.event), Some(GameEvent::Step));
        selector.add_candidate(info(GameEvent::Eat, 5.0), 10);
        assert_eq!(selector.current().map(|i| i.event), Some(GameEvent::Eat));
        selector.add_candidate(info(GameEvent::Step, 5.0), 10);
        assert_eq!(selector.current().map(|i| i.event), Some(GameEvent::Eat));
        selector.add_candidate(info(GameEvent::Step, 1.0), 11);
        assert_eq!(selector.current().map(|i| i.event), Some(GameEvent::Eat));
    }

    #[test]
    fn a_candidate_is_chosen_once_its_tick_has_passed() {
        let mut selector = VibrationSelector::default();
        selector.add_candidate(info(GameEvent::Step, 5.0), 10);
        assert!(selector.chosen_candidate(10).is_none());
        assert!(selector.chosen_candidate(11).is_some());
        selector.start_over();
        assert!(selector.chosen_candidate(12).is_none());
    }

    #[test]
    fn vibration_data_round_trips_through_nbt() {
        let source = Uuid::from_u128(0x1234_5678_9abc_def0_1234_5678_9abc_def0);
        let mut current = info(GameEvent::BlockPlace, 3.5);
        current.source = Some(source);
        current.projectile_owner = Some(Uuid::from_u128(9));
        let data = VibrationData {
            current: Some(current.clone()),
            selector: VibrationSelector::new(Some(info(GameEvent::Eat, 1.5)), 77),
            travel_time_in_ticks: 4,
            reload_particle: false,
        };
        let loaded = VibrationData::from_nbt(&data.to_nbt()).expect("data");
        assert_eq!(loaded.current, Some(current));
        assert_eq!(loaded.selector, data.selector);
        assert_eq!(loaded.travel_time_in_ticks, 4);
        assert!(loaded.reload_particle);
    }

    #[test]
    fn vibration_data_uses_the_vanilla_layout() {
        let data = VibrationData {
            current: Some(info(GameEvent::Step, 2.0)),
            ..VibrationData::default()
        };
        let nbt = data.to_nbt();
        let event = nbt.get_compound("event").expect("event");
        assert_eq!(event.get_str("game_event"), Some("minecraft:step"));
        assert_eq!(event.get("distance"), Some(&Tag::Float(2.0)));
        assert_eq!(event.get_list("pos").map(<[Tag]>::len), Some(3));
        let selector = nbt.get_compound("selector").expect("selector");
        assert_eq!(selector.get("tick"), Some(&Tag::Long(-1)));
        assert_eq!(nbt.get("event_delay"), Some(&Tag::Int(0)));
    }

    #[test]
    fn broken_vibration_data_falls_back_to_the_default() {
        let mut nbt = Compound::new();
        nbt.insert("event_delay", 3);
        assert_eq!(
            VibrationData::read_or_default(Some(&nbt)),
            VibrationData::default()
        );
        let mut bad = VibrationData::default().to_nbt();
        bad.insert("event_delay", -1);
        assert_eq!(
            VibrationData::read_or_default(Some(&bad)),
            VibrationData::default()
        );
        assert_eq!(
            VibrationData::read_or_default(None),
            VibrationData::default()
        );
    }

    #[test]
    fn a_broken_event_is_dropped_but_the_rest_is_kept() {
        let data = VibrationData {
            travel_time_in_ticks: 6,
            ..VibrationData::default()
        };
        let mut nbt = data.to_nbt();
        nbt.insert("event", Compound::new());
        let loaded = VibrationData::from_nbt(&nbt).expect("data");
        assert_eq!(loaded.current, None);
        assert_eq!(loaded.travel_time_in_ticks, 6);
    }

    #[test]
    fn travel_time_never_goes_below_zero() {
        let mut data = VibrationData {
            travel_time_in_ticks: 1,
            ..VibrationData::default()
        };
        data.decrement_travel_time();
        data.decrement_travel_time();
        assert_eq!(data.travel_time_in_ticks, 0);
    }

    fn wool() -> BlockStateId {
        BlockKind::WhiteWool.default_state()
    }

    #[test]
    fn wool_on_the_whole_line_occludes_a_vibration() {
        let mut map = FxHashMap::default();
        for dx in -1..=1 {
            for dy in -1..=1 {
                map.insert(BlockPos::new(3, 64 + dy, dx), wool());
            }
        }
        let view = Blocks(map);
        let origin = Vec3::new(0.5, 64.5, 0.5);
        let dest = Vec3::new(6.5, 64.5, 0.5);
        assert!(is_occluded(&view, origin, dest));
    }

    #[test]
    fn a_single_wool_block_on_the_line_occludes_a_vibration() {
        let mut map = FxHashMap::default();
        map.insert(BlockPos::new(3, 64, 0), wool());
        let origin = Vec3::new(0.5, 64.5, 0.5);
        let dest = Vec3::new(6.5, 64.5, 0.5);
        assert!(is_occluded(&Blocks(map), origin, dest));
    }

    #[test]
    fn wool_beside_the_line_lets_the_vibration_through() {
        let mut map = FxHashMap::default();
        map.insert(BlockPos::new(3, 65, 0), wool());
        map.insert(BlockPos::new(3, 64, 1), wool());
        let origin = Vec3::new(0.5, 64.5, 0.5);
        let dest = Vec3::new(6.5, 64.5, 0.5);
        assert!(!is_occluded(&Blocks(map), origin, dest));
        assert!(!is_occluded(&Blocks(FxHashMap::default()), origin, dest));
    }

    #[test]
    fn stone_does_not_occlude_vibrations() {
        let mut map = FxHashMap::default();
        for dy in -1..=1 {
            for dz in -1..=1 {
                map.insert(
                    BlockPos::new(3, 64 + dy, dz),
                    BlockKind::Stone.default_state(),
                );
            }
        }
        let origin = Vec3::new(0.5, 64.5, 0.5);
        let dest = Vec3::new(6.5, 64.5, 0.5);
        assert!(!is_occluded(&Blocks(map), origin, dest));
    }

    fn walker() -> SourceFacts {
        SourceFacts {
            spectator: false,
            stepping_carefully: false,
            dampens_vibrations: false,
        }
    }

    #[test]
    fn a_vibration_must_be_in_the_listenable_tag() {
        assert_eq!(
            validate(
                GameEvent::Step,
                GameEventTag::Vibrations,
                Some(walker()),
                None,
                false
            ),
            Validity::Valid
        );
        assert_eq!(
            validate(
                GameEvent::Shriek,
                GameEventTag::Vibrations,
                Some(walker()),
                None,
                false
            ),
            Validity::Invalid
        );
        assert_eq!(
            validate(
                GameEvent::Shriek,
                GameEventTag::WardenCanListen,
                None,
                None,
                false
            ),
            Validity::Valid
        );
    }

    #[test]
    fn spectators_and_dampening_sources_are_silent() {
        let spectator = SourceFacts {
            spectator: true,
            ..walker()
        };
        let muffled = SourceFacts {
            dampens_vibrations: true,
            ..walker()
        };
        assert_eq!(
            validate(
                GameEvent::Step,
                GameEventTag::Vibrations,
                Some(spectator),
                None,
                false
            ),
            Validity::Invalid
        );
        assert_eq!(
            validate(
                GameEvent::Step,
                GameEventTag::Vibrations,
                Some(muffled),
                None,
                false
            ),
            Validity::Invalid
        );
    }

    #[test]
    fn sneaking_avoids_the_quiet_events_and_is_reported_to_sensors() {
        let sneaking = SourceFacts {
            stepping_carefully: true,
            ..walker()
        };
        assert_eq!(
            validate(
                GameEvent::Step,
                GameEventTag::Vibrations,
                Some(sneaking),
                None,
                true
            ),
            Validity::AvoidedBySneaking
        );
        assert_eq!(
            validate(
                GameEvent::Step,
                GameEventTag::Vibrations,
                Some(sneaking),
                None,
                false
            ),
            Validity::Invalid
        );
        assert_eq!(
            validate(
                GameEvent::Eat,
                GameEventTag::Vibrations,
                Some(sneaking),
                None,
                true
            ),
            Validity::Valid
        );
    }

    #[test]
    fn a_wool_block_dampens_the_vibration_it_changed() {
        assert_eq!(
            validate(
                GameEvent::BlockPlace,
                GameEventTag::Vibrations,
                None,
                Some(wool()),
                false
            ),
            Validity::Invalid
        );
        assert_eq!(
            validate(
                GameEvent::BlockPlace,
                GameEventTag::Vibrations,
                None,
                Some(BlockKind::Stone.default_state()),
                false
            ),
            Validity::Valid
        );
    }
}
