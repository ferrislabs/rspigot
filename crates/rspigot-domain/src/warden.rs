use rspigot_nbt::{Compound, Tag};
use rustc_hash::FxHashMap;
use uuid::Uuid;

use crate::EntityId;
use crate::entity::save::{read_uuid, uuid_tag};

pub const CALM_ANGER: i32 = 0;
pub const AGITATED_ANGER: i32 = 40;
pub const ANGRY_ANGER: i32 = 80;
pub const MAX_ANGER: i32 = 150;
pub const DEFAULT_ANGER_INCREASE: i32 = 35;
pub const PROJECTILE_ANGER_INCREASE: i32 = 10;
pub const HURT_ANGER_BONUS: i32 = 20;
pub const ROAR_ANGER_INCREASE: i32 = 20;
pub const TICKS_BEFORE_NEXT_WARNING: i32 = 12_000;
pub const WARNING_COOLDOWN_TICKS: i32 = 200;
pub const MAX_WARNING_LEVEL: i32 = 4;
pub const WARNING_RADIUS: f64 = 16.0;
pub const NEARBY_WARDEN_BOX: f64 = 48.0;
pub const DIGGING_DURATION: i32 = 100;
pub const EMERGE_DURATION: i32 = 134;
pub const ROAR_DURATION: i32 = 84;
pub const SNIFFING_DURATION: i32 = 84;
pub const DIG_COOLDOWN_TICKS: i64 = 1200;
pub const INITIAL_DIG_COOLDOWN_TICKS: i64 = 1200;
pub const ANGER_KEY: &str = "anger";
pub const LISTENER_KEY: &str = "listener";
pub const TRACKER_KEY: &str = "warden_spawn_tracker";
const SUSPECTS: &str = "suspects";
const UUID: &str = "uuid";
const ANGER: &str = "anger";
const TICKS_SINCE_LAST_WARNING: &str = "ticks_since_last_warning";
const WARNING_LEVEL: &str = "warning_level";
const COOLDOWN_TICKS: &str = "cooldown_ticks";
const AMBIENT: [&str; 3] = [
    "minecraft:entity.warden.ambient",
    "minecraft:entity.warden.agitated",
    "minecraft:entity.warden.angry",
];
const LISTENING: [&str; 3] = [
    "minecraft:entity.warden.listening",
    "minecraft:entity.warden.listening_angry",
    "minecraft:entity.warden.listening_angry",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AngerLevel {
    Calm,
    Agitated,
    Angry,
}

impl AngerLevel {
    pub const fn minimum_anger(self) -> i32 {
        match self {
            Self::Calm => CALM_ANGER,
            Self::Agitated => AGITATED_ANGER,
            Self::Angry => ANGRY_ANGER,
        }
    }

    pub const fn ambient_sound(self) -> &'static str {
        AMBIENT[self as usize]
    }

    pub const fn listening_sound(self) -> &'static str {
        LISTENING[self as usize]
    }

    pub fn by_anger(anger: i32) -> Self {
        [Self::Angry, Self::Agitated]
            .into_iter()
            .find(|level| level.minimum_anger() <= anger)
            .unwrap_or(Self::Calm)
    }

    pub const fn is_angry(self) -> bool {
        matches!(self, Self::Angry)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    Valid,
    Invalid,
    Removed { keeps_anger: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Suspect {
    pub id: EntityId,
    pub uuid: Uuid,
    pub player: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AngerManagement {
    conversion_delay: i32,
    suspects: Vec<Suspect>,
    anger_by_suspect: FxHashMap<EntityId, i32>,
    anger_by_uuid: Vec<(Uuid, i32)>,
    highest_anger: i32,
}

impl AngerManagement {
    pub fn new(conversion_delay: i32, pending: Vec<(Uuid, i32)>) -> Self {
        let mut anger_by_uuid: Vec<(Uuid, i32)> = Vec::with_capacity(pending.len());
        for (uuid, anger) in pending {
            match anger_by_uuid.iter_mut().find(|(known, _)| *known == uuid) {
                Some(entry) => entry.1 = anger,
                None => anger_by_uuid.push((uuid, anger)),
            }
        }
        Self {
            conversion_delay,
            suspects: Vec::new(),
            anger_by_suspect: FxHashMap::default(),
            anger_by_uuid,
            highest_anger: 0,
        }
    }

    pub fn empty(conversion_delay: i32) -> Self {
        Self::new(conversion_delay, Vec::new())
    }

    pub fn suspects(&self) -> &[Suspect] {
        &self.suspects
    }

    pub fn pending(&self) -> &[(Uuid, i32)] {
        &self.anger_by_uuid
    }

    pub fn anger_of(&self, id: EntityId) -> i32 {
        self.anger_by_suspect.get(&id).copied().unwrap_or(0)
    }

    pub fn highest_anger(&self) -> i32 {
        self.highest_anger
    }

    pub fn tick(
        &mut self,
        mut standing: impl FnMut(Suspect) -> Standing,
        mut resolve: impl FnMut(Uuid) -> Option<Suspect>,
    ) {
        self.conversion_delay -= 1;
        if self.conversion_delay < 1 {
            self.convert_from_uuids(&mut resolve);
            self.conversion_delay = 2;
        }
        self.anger_by_uuid.retain_mut(|(_, anger)| {
            if *anger < 2 {
                false
            } else {
                *anger -= 1;
                true
            }
        });
        let listed: Vec<Suspect> = self.suspects.clone();
        for suspect in listed {
            let Some(anger) = self.anger_by_suspect.get(&suspect.id).copied() else {
                continue;
            };
            let state = standing(suspect);
            if anger < 2 || state != Standing::Valid {
                self.suspects.retain(|known| known.id != suspect.id);
                self.anger_by_suspect.remove(&suspect.id);
                if anger > 1 && matches!(state, Standing::Removed { keeps_anger: true }) {
                    self.put_pending(suspect.uuid, anger - 1);
                }
            } else {
                self.anger_by_suspect.insert(suspect.id, anger - 1);
            }
        }
        self.sort_and_update_highest_anger();
    }

    fn put_pending(&mut self, uuid: Uuid, anger: i32) {
        match self
            .anger_by_uuid
            .iter_mut()
            .find(|(known, _)| *known == uuid)
        {
            Some(entry) => entry.1 = anger,
            None => self.anger_by_uuid.push((uuid, anger)),
        }
    }

    fn convert_from_uuids(&mut self, resolve: &mut impl FnMut(Uuid) -> Option<Suspect>) {
        let pending = std::mem::take(&mut self.anger_by_uuid);
        for (uuid, anger) in pending {
            match resolve(uuid) {
                Some(suspect) => {
                    self.anger_by_suspect.insert(suspect.id, anger);
                    self.suspects.push(suspect);
                }
                None => self.anger_by_uuid.push((uuid, anger)),
            }
        }
    }

    fn sort_and_update_highest_anger(&mut self) {
        let anger = &self.anger_by_suspect;
        self.suspects.sort_by(|first, second| {
            if first.id == second.id {
                return std::cmp::Ordering::Equal;
            }
            let one = anger.get(&first.id).copied().unwrap_or(0);
            let two = anger.get(&second.id).copied().unwrap_or(0);
            let one_angry = AngerLevel::by_anger(one).is_angry();
            let two_angry = AngerLevel::by_anger(two).is_angry();
            if one_angry != two_angry {
                return if one_angry {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }
            if first.player == second.player {
                return two.cmp(&one);
            }
            if first.player {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        });
        self.highest_anger = self
            .suspects
            .iter()
            .map(|suspect| anger.get(&suspect.id).copied().unwrap_or(0))
            .max()
            .unwrap_or(0);
    }

    pub fn increase_anger(&mut self, suspect: Suspect, increment: i32) -> i32 {
        let known = self.anger_by_suspect.contains_key(&suspect.id);
        let mut anger = (self.anger_of(suspect.id) + increment).min(MAX_ANGER);
        if !known {
            let pending = self
                .anger_by_uuid
                .iter()
                .position(|(uuid, _)| *uuid == suspect.uuid)
                .map_or(0, |index| self.anger_by_uuid.remove(index).1);
            anger += pending;
            self.suspects.push(suspect);
        }
        self.anger_by_suspect.insert(suspect.id, anger);
        self.sort_and_update_highest_anger();
        anger
    }

    pub fn clear_anger(&mut self, id: EntityId) {
        self.anger_by_suspect.remove(&id);
        self.suspects.retain(|suspect| suspect.id != id);
        self.sort_and_update_highest_anger();
    }

    pub fn active_anger(&self, current_target: Option<EntityId>) -> i32 {
        match current_target {
            None => self.highest_anger,
            Some(target) => self.anger_of(target),
        }
    }

    pub fn active_entity(&self, mut valid: impl FnMut(EntityId) -> bool) -> Option<EntityId> {
        self.suspects
            .iter()
            .find(|suspect| valid(suspect.id))
            .map(|suspect| suspect.id)
    }

    pub fn to_nbt(&self) -> Compound {
        let pairs = self
            .suspects
            .iter()
            .map(|suspect| (suspect.uuid, self.anger_of(suspect.id)))
            .chain(self.anger_by_uuid.iter().copied());
        let list = pairs
            .map(|(uuid, anger)| {
                let mut entry = Compound::new();
                entry.insert(UUID, uuid_tag(uuid));
                entry.insert(ANGER, anger);
                Tag::Compound(entry)
            })
            .collect();
        let mut out = Compound::new();
        out.insert(SUSPECTS, Tag::List(list));
        out
    }

    pub fn from_nbt(nbt: &Compound, conversion_delay: i32) -> Self {
        let pairs = nbt
            .get_list(SUSPECTS)
            .map(|list| {
                list.iter()
                    .map(|tag| {
                        let entry = tag.as_compound()?;
                        let uuid = entry.get(UUID).and_then(read_uuid)?;
                        let anger = match entry.get(ANGER)? {
                            Tag::Int(value) if *value >= 0 => *value,
                            _ => return None,
                        };
                        Some((uuid, anger))
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .unwrap_or_else(|| Some(Vec::new()));
        Self::new(conversion_delay, pairs.unwrap_or_default())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WardenSpawnTracker {
    pub ticks_since_last_warning: i32,
    pub warning_level: i32,
    pub cooldown_ticks: i32,
}

impl WardenSpawnTracker {
    pub fn tick(&mut self) {
        if self.ticks_since_last_warning < TICKS_BEFORE_NEXT_WARNING {
            self.ticks_since_last_warning += 1;
        } else {
            self.set_warning_level(self.warning_level - 1);
            self.ticks_since_last_warning = 0;
        }
        if self.cooldown_ticks > 0 {
            self.cooldown_ticks -= 1;
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn on_cooldown(&self) -> bool {
        self.cooldown_ticks > 0
    }

    pub fn set_warning_level(&mut self, level: i32) {
        self.warning_level = level.clamp(0, MAX_WARNING_LEVEL);
    }

    fn increase_warning_level(&mut self) {
        if !self.on_cooldown() {
            self.ticks_since_last_warning = 0;
            self.cooldown_ticks = WARNING_COOLDOWN_TICKS;
            self.set_warning_level(self.warning_level + 1);
        }
    }

    pub fn to_nbt(&self) -> Compound {
        let mut out = Compound::new();
        out.insert(TICKS_SINCE_LAST_WARNING, self.ticks_since_last_warning);
        out.insert(WARNING_LEVEL, self.warning_level);
        out.insert(COOLDOWN_TICKS, self.cooldown_ticks);
        out
    }

    pub fn from_nbt(nbt: &Compound) -> Option<Self> {
        let field = |key: &str| match nbt.get(key) {
            None => Some(0),
            Some(Tag::Int(value)) if *value >= 0 => Some(*value),
            Some(_) => None,
        };
        Some(Self {
            ticks_since_last_warning: field(TICKS_SINCE_LAST_WARNING)?,
            warning_level: field(WARNING_LEVEL)?,
            cooldown_ticks: field(COOLDOWN_TICKS)?,
        })
    }

    pub fn read_or_default(nbt: Option<&Compound>) -> Self {
        nbt.and_then(Self::from_nbt).unwrap_or_default()
    }
}

pub fn try_warn(trackers: &mut [WardenSpawnTracker]) -> Option<i32> {
    if trackers.iter().any(WardenSpawnTracker::on_cooldown) {
        return None;
    }
    let mut best: Option<usize> = None;
    for (index, tracker) in trackers.iter().enumerate() {
        if best.is_none_or(|top| tracker.warning_level > trackers[top].warning_level) {
            best = Some(index);
        }
    }
    let top = best?;
    trackers[top].increase_warning_level();
    let updated = trackers[top];
    for tracker in trackers.iter_mut() {
        *tracker = updated;
    }
    Some(updated.warning_level)
}

pub fn closer_than_warning_radius(from: crate::Vec3, to: crate::Vec3) -> bool {
    from.distance_squared(to) < WARNING_RADIUS * WARNING_RADIUS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suspect(id: i32, player: bool) -> Suspect {
        Suspect {
            id: EntityId(id),
            uuid: Uuid::from_u128(id as u128),
            player,
        }
    }

    fn all_valid(_: Suspect) -> Standing {
        Standing::Valid
    }

    fn nobody(_: Uuid) -> Option<Suspect> {
        None
    }

    #[test]
    fn anger_levels_split_at_forty_and_eighty() {
        assert_eq!(AngerLevel::by_anger(0), AngerLevel::Calm);
        assert_eq!(AngerLevel::by_anger(39), AngerLevel::Calm);
        assert_eq!(AngerLevel::by_anger(40), AngerLevel::Agitated);
        assert_eq!(AngerLevel::by_anger(79), AngerLevel::Agitated);
        assert_eq!(AngerLevel::by_anger(80), AngerLevel::Angry);
        assert_eq!(AngerLevel::by_anger(150), AngerLevel::Angry);
        assert_eq!(AngerLevel::by_anger(-5), AngerLevel::Calm);
        assert!(AngerLevel::Angry.is_angry());
        assert!(!AngerLevel::Agitated.is_angry());
    }

    #[test]
    fn anger_levels_carry_their_sounds() {
        assert_eq!(
            AngerLevel::Calm.ambient_sound(),
            "minecraft:entity.warden.ambient"
        );
        assert_eq!(
            AngerLevel::Agitated.ambient_sound(),
            "minecraft:entity.warden.agitated"
        );
        assert_eq!(
            AngerLevel::Angry.ambient_sound(),
            "minecraft:entity.warden.angry"
        );
        assert_eq!(
            AngerLevel::Calm.listening_sound(),
            "minecraft:entity.warden.listening"
        );
        assert_eq!(
            AngerLevel::Agitated.listening_sound(),
            "minecraft:entity.warden.listening_angry"
        );
        assert_eq!(
            AngerLevel::Angry.listening_sound(),
            "minecraft:entity.warden.listening_angry"
        );
        for level in [AngerLevel::Calm, AngerLevel::Agitated, AngerLevel::Angry] {
            for sound in [level.ambient_sound(), level.listening_sound()] {
                assert!(
                    rspigot_data::registries::static_index("minecraft:sound_event", sound)
                        .is_some(),
                    "{sound}"
                );
            }
        }
    }

    #[test]
    fn anger_is_capped_at_one_hundred_fifty() {
        let mut anger = AngerManagement::empty(5);
        let player = suspect(1, true);
        assert_eq!(anger.increase_anger(player, 100), 100);
        assert_eq!(anger.increase_anger(player, 100), 150);
        assert_eq!(anger.increase_anger(player, 1), 150);
        assert_eq!(anger.highest_anger(), 150);
    }

    #[test]
    fn the_angriest_player_comes_first_and_sets_the_highest_anger() {
        let mut anger = AngerManagement::empty(5);
        anger.increase_anger(suspect(1, false), 90);
        anger.increase_anger(suspect(2, true), 30);
        anger.increase_anger(suspect(3, true), 50);
        let order: Vec<i32> = anger.suspects().iter().map(|s| s.id.0).collect();
        assert_eq!(order, vec![1, 3, 2]);
        assert_eq!(anger.highest_anger(), 90);
        assert_eq!(anger.active_entity(|_| true), Some(EntityId(1)));
        assert_eq!(
            anger.active_entity(|id| id != EntityId(1)),
            Some(EntityId(3))
        );
    }

    #[test]
    fn players_outrank_other_mobs_below_the_angry_threshold() {
        let mut anger = AngerManagement::empty(5);
        anger.increase_anger(suspect(1, false), 60);
        anger.increase_anger(suspect(2, true), 10);
        let order: Vec<i32> = anger.suspects().iter().map(|s| s.id.0).collect();
        assert_eq!(order, vec![2, 1]);
    }

    #[test]
    fn the_active_anger_follows_the_current_target() {
        let mut anger = AngerManagement::empty(5);
        anger.increase_anger(suspect(1, true), 70);
        anger.increase_anger(suspect(2, true), 20);
        assert_eq!(anger.active_anger(None), 70);
        assert_eq!(anger.active_anger(Some(EntityId(2))), 20);
        assert_eq!(anger.active_anger(Some(EntityId(9))), 0);
    }

    #[test]
    fn clearing_a_suspect_forgets_it() {
        let mut anger = AngerManagement::empty(5);
        anger.increase_anger(suspect(1, true), 70);
        anger.clear_anger(EntityId(1));
        assert!(anger.suspects().is_empty());
        assert_eq!(anger.highest_anger(), 0);
        assert_eq!(anger.anger_of(EntityId(1)), 0);
    }

    #[test]
    fn anger_decays_one_point_per_tick_and_a_suspect_below_two_is_dropped() {
        let mut anger = AngerManagement::empty(5);
        anger.increase_anger(suspect(1, true), 3);
        anger.tick(all_valid, nobody);
        assert_eq!(anger.anger_of(EntityId(1)), 2);
        anger.tick(all_valid, nobody);
        assert_eq!(anger.anger_of(EntityId(1)), 1);
        anger.tick(all_valid, nobody);
        assert!(anger.suspects().is_empty());
    }

    #[test]
    fn an_invalid_suspect_is_dropped_without_keeping_its_anger() {
        let mut anger = AngerManagement::empty(5);
        anger.increase_anger(suspect(1, true), 50);
        anger.tick(|_| Standing::Invalid, nobody);
        assert!(anger.suspects().is_empty());
        assert!(anger.pending().is_empty());
    }

    #[test]
    fn a_suspect_that_unloaded_keeps_its_anger_by_uuid() {
        let mut anger = AngerManagement::empty(5);
        anger.increase_anger(suspect(1, true), 50);
        anger.tick(|_| Standing::Removed { keeps_anger: true }, nobody);
        assert!(anger.suspects().is_empty());
        assert_eq!(anger.pending(), &[(Uuid::from_u128(1), 49)]);
    }

    #[test]
    fn a_dead_suspect_loses_its_anger() {
        let mut anger = AngerManagement::empty(5);
        anger.increase_anger(suspect(1, true), 50);
        anger.tick(|_| Standing::Removed { keeps_anger: false }, nobody);
        assert!(anger.pending().is_empty());
    }

    #[test]
    fn pending_anger_returns_when_the_entity_is_found_again() {
        let mut anger = AngerManagement::new(1, vec![(Uuid::from_u128(7), 40)]);
        anger.tick(all_valid, |uuid| {
            (uuid == Uuid::from_u128(7)).then(|| suspect(7, true))
        });
        assert_eq!(anger.anger_of(EntityId(7)), 39);
        assert!(anger.pending().is_empty());
        assert_eq!(anger.highest_anger(), 39);
    }

    #[test]
    fn pending_anger_is_added_when_a_new_anger_is_raised_before_the_conversion() {
        let mut anger = AngerManagement::new(5, vec![(Uuid::from_u128(7), 40)]);
        assert_eq!(anger.increase_anger(suspect(7, true), 35), 75);
        assert!(anger.pending().is_empty());
    }

    #[test]
    fn the_uuid_conversion_waits_for_its_delay() {
        let mut anger = AngerManagement::new(2, vec![(Uuid::from_u128(7), 40)]);
        let resolve = |_| Some(suspect(7, true));
        anger.tick(all_valid, resolve);
        assert!(anger.suspects().is_empty());
        anger.tick(all_valid, resolve);
        assert_eq!(anger.suspects().len(), 1);
    }

    #[test]
    fn anger_round_trips_through_nbt_with_loaded_and_pending_suspects() {
        let mut anger = AngerManagement::new(5, vec![(Uuid::from_u128(9), 12)]);
        anger.increase_anger(suspect(1, true), 55);
        let nbt = anger.to_nbt();
        let list = nbt.get_list("suspects").expect("suspects");
        assert_eq!(list.len(), 2);
        let first = list[0].as_compound().expect("entry");
        assert_eq!(first.get_i32("anger"), Some(55));
        assert_eq!(first.get_int_array("uuid").map(<[i32]>::len), Some(4));
        let loaded = AngerManagement::from_nbt(&nbt, 3);
        assert_eq!(
            loaded.pending(),
            &[(Uuid::from_u128(1), 55), (Uuid::from_u128(9), 12)]
        );
    }

    #[test]
    fn broken_anger_nbt_loads_as_empty() {
        let mut nbt = Compound::new();
        nbt.insert("suspects", Tag::List(vec![Tag::Compound(Compound::new())]));
        assert!(AngerManagement::from_nbt(&nbt, 0).pending().is_empty());
        assert!(
            AngerManagement::from_nbt(&Compound::new(), 0)
                .pending()
                .is_empty()
        );
    }

    #[test]
    fn a_tracker_counts_up_then_forgets_one_warning_every_ten_minutes() {
        let mut tracker = WardenSpawnTracker {
            ticks_since_last_warning: 0,
            warning_level: 2,
            cooldown_ticks: 0,
        };
        for _ in 0..TICKS_BEFORE_NEXT_WARNING {
            tracker.tick();
        }
        assert_eq!(tracker.warning_level, 2);
        tracker.tick();
        assert_eq!(tracker.warning_level, 1);
        assert_eq!(tracker.ticks_since_last_warning, 0);
    }

    #[test]
    fn the_warning_level_never_leaves_zero_to_four() {
        let mut tracker = WardenSpawnTracker::default();
        tracker.set_warning_level(9);
        assert_eq!(tracker.warning_level, 4);
        tracker.set_warning_level(-3);
        assert_eq!(tracker.warning_level, 0);
    }

    #[test]
    fn the_cooldown_counts_down() {
        let mut tracker = WardenSpawnTracker {
            cooldown_ticks: 2,
            ..WardenSpawnTracker::default()
        };
        assert!(tracker.on_cooldown());
        tracker.tick();
        tracker.tick();
        assert!(!tracker.on_cooldown());
        tracker.tick();
        assert_eq!(tracker.cooldown_ticks, 0);
    }

    #[test]
    fn a_warning_raises_the_level_and_starts_the_cooldown_for_everyone_nearby() {
        let mut trackers = [
            WardenSpawnTracker::default(),
            WardenSpawnTracker {
                warning_level: 2,
                ticks_since_last_warning: 40,
                cooldown_ticks: 0,
            },
        ];
        assert_eq!(try_warn(&mut trackers), Some(3));
        for tracker in trackers {
            assert_eq!(tracker.warning_level, 3);
            assert_eq!(tracker.cooldown_ticks, 200);
            assert_eq!(tracker.ticks_since_last_warning, 0);
        }
    }

    #[test]
    fn a_warning_is_refused_while_anyone_nearby_is_on_cooldown() {
        let mut trackers = [
            WardenSpawnTracker::default(),
            WardenSpawnTracker {
                cooldown_ticks: 5,
                ..WardenSpawnTracker::default()
            },
        ];
        assert_eq!(try_warn(&mut trackers), None);
        assert_eq!(trackers[0].warning_level, 0);
    }

    #[test]
    fn a_fourth_warning_reaches_level_four_and_a_fifth_stays_capped() {
        let mut trackers = [WardenSpawnTracker::default()];
        let mut seen = Vec::new();
        for _ in 0..5 {
            seen.push(try_warn(&mut trackers));
            trackers[0].cooldown_ticks = 0;
        }
        assert_eq!(seen, vec![Some(1), Some(2), Some(3), Some(4), Some(4)]);
    }

    #[test]
    fn no_players_nearby_means_no_warning() {
        assert_eq!(try_warn(&mut []), None);
    }

    #[test]
    fn the_tracker_round_trips_through_nbt() {
        let tracker = WardenSpawnTracker {
            ticks_since_last_warning: 120,
            warning_level: 3,
            cooldown_ticks: 44,
        };
        assert_eq!(
            WardenSpawnTracker::from_nbt(&tracker.to_nbt()),
            Some(tracker)
        );
        assert_eq!(
            WardenSpawnTracker::read_or_default(None),
            WardenSpawnTracker::default()
        );
        let mut bad = Compound::new();
        bad.insert("warning_level", -2);
        assert_eq!(
            WardenSpawnTracker::read_or_default(Some(&bad)),
            WardenSpawnTracker::default()
        );
        assert_eq!(
            WardenSpawnTracker::from_nbt(&Compound::new()),
            Some(WardenSpawnTracker::default())
        );
    }

    #[test]
    fn the_durations_are_the_ceilings_of_the_animation_lengths() {
        assert_eq!(DIGGING_DURATION, 100);
        assert_eq!(EMERGE_DURATION, 134);
        assert_eq!(ROAR_DURATION, 84);
        assert_eq!(SNIFFING_DURATION, 84);
    }

    #[test]
    fn warnings_reach_players_within_sixteen_blocks_only() {
        use crate::Vec3;
        let origin = Vec3::new(0.5, 64.5, 0.5);
        assert!(closer_than_warning_radius(
            origin,
            Vec3::new(15.0, 64.5, 0.5)
        ));
        assert!(!closer_than_warning_radius(
            origin,
            Vec3::new(16.5, 64.5, 0.5)
        ));
    }
}
