use std::collections::{BTreeMap, BTreeSet};

use rspigot_entities::EntityKind;
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use crate::boss_event::{BossColor, BossMessages, BossOverlay, ServerBossEvent};
use crate::entity::save::{read_uuid, uuid_tag};
use crate::player::PlayerId;
use crate::random::{LegacyRandom, RandomSource};
use crate::text::Component;
use crate::world::Difficulty;
use crate::{BlockPos, EntityId, SectionPos, mth};

pub const MAX_RAID_OMEN_LEVEL: i32 = 5;
pub const RAID_COOLDOWN_TICKS: i32 = 300;
pub const RAID_TIMEOUT_TICKS: i64 = 48_000;
pub const POST_RAID_TICKS: i32 = 40;
pub const CELEBRATION_TICKS: i32 = 600;
pub const RAIDER_RADIUS_SQR: f64 = 12_544.0;
pub const NEARBY_RAID_DISTANCE_SQR: f64 = 9_216.0;
pub const RAIDER_STALE_TICKS: i32 = 600;
pub const RAIDER_IDLE_TICKS: i32 = 2_400;
pub const RAIDER_OUTSIDE_LIMIT: i32 = 29;
pub const HERO_OF_THE_VILLAGE_TICKS: i32 = 48_000;
pub const OMEN_EFFECT_TICKS: i32 = 600;
pub const POI_SEARCH_RANGE: i32 = 64;
pub const HORN_DISTANCE: f64 = 13.0;
pub const HORN_HEARING_RANGE: f64 = 64.0;
pub const HORN_SOUND: &str = "minecraft:event.raid.horn";
pub const RAID_NAME_KEY: &str = "event.minecraft.raid";
pub const RAID_VICTORY_KEY: &str = "event.minecraft.raid.victory.full";
pub const RAID_DEFEAT_KEY: &str = "event.minecraft.raid.defeat.full";
pub const RAIDERS_REMAINING_KEY: &str = "event.minecraft.raid.raiders_remaining";
pub const RAID_FILE: &str = "raids";
pub const RAID_ID_KEY: &str = "RaidId";
const FIRST_SPAWN_TRIES: i32 = 8;
const RETRY_SPAWN_TRIES: i32 = 20;
const UNSPAWNABLE_LIMIT: i32 = 6;
const VILLAGE_SECTION_RADIUS: i32 = 2;
const SPAWN_HEIGHT_RANGE: i32 = 97;
const SPAWN_CHUNK_MARGIN: i32 = 10;

pub fn raid_name() -> Component {
    Component::translatable(RAID_NAME_KEY, Vec::new())
}

fn raid_name_with_raiders(alive: i32) -> Component {
    raid_name()
        .append(Component::text(" - "))
        .append(Component::translatable(
            RAIDERS_REMAINING_KEY,
            vec![Component::text(alive.to_string())],
        ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaidStatus {
    Ongoing,
    Victory,
    Loss,
    Stopped,
}

impl RaidStatus {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Ongoing => "ongoing",
            Self::Victory => "victory",
            Self::Loss => "loss",
            Self::Stopped => "stopped",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "ongoing" => Some(Self::Ongoing),
            "victory" => Some(Self::Victory),
            "loss" => Some(Self::Loss),
            "stopped" => Some(Self::Stopped),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaidStopReason {
    Peace,
    Timeout,
    Finished,
    Unspawnable,
    NotInVillage,
    Plugin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaiderType {
    Vindicator,
    Evoker,
    Pillager,
    Witch,
    Ravager,
}

impl RaiderType {
    pub const ALL: [Self; 5] = [
        Self::Vindicator,
        Self::Evoker,
        Self::Pillager,
        Self::Witch,
        Self::Ravager,
    ];

    pub const fn kind(self) -> EntityKind {
        match self {
            Self::Vindicator => EntityKind::Vindicator,
            Self::Evoker => EntityKind::Evoker,
            Self::Pillager => EntityKind::Pillager,
            Self::Witch => EntityKind::Witch,
            Self::Ravager => EntityKind::Ravager,
        }
    }

    const fn spawns_before_bonus(self) -> [i32; 8] {
        match self {
            Self::Vindicator => [0, 0, 2, 0, 1, 4, 2, 5],
            Self::Evoker => [0, 0, 0, 0, 0, 1, 1, 2],
            Self::Pillager => [0, 4, 3, 3, 4, 4, 4, 2],
            Self::Witch => [0, 0, 0, 0, 3, 0, 0, 1],
            Self::Ravager => [0, 0, 0, 1, 0, 1, 0, 2],
        }
    }

    const fn extra_per_extended_wave(self, extra_waves: i32) -> i32 {
        match self {
            Self::Vindicator => extra_waves / 3,
            Self::Evoker => extra_waves / 4,
            Self::Pillager => extra_waves / 2,
            Self::Witch => extra_waves / 5,
            Self::Ravager => extra_waves / 6,
        }
    }

    pub const fn default_spawns_for_wave(self, wave: i32) -> i32 {
        if wave < 0 {
            return 0;
        }
        let table = self.spawns_before_bonus();
        if (wave as usize) < table.len() {
            return table[wave as usize];
        }
        let last = table.len() as i32 - 1;
        table[last as usize] + self.extra_per_extended_wave(wave - last)
    }

    fn potential_bonus_spawns(
        self,
        random: &mut impl RandomSource,
        wave: i32,
        difficulty: Difficulty,
        bonus_wave: bool,
    ) -> i32 {
        let easy = difficulty == Difficulty::Easy;
        let normal = difficulty == Difficulty::Normal;
        let bound = match self {
            Self::Vindicator | Self::Pillager => {
                if easy {
                    random.next_int(2)
                } else if normal {
                    1
                } else {
                    2
                }
            }
            Self::Evoker => return 0,
            Self::Witch => {
                if easy || wave < 3 || wave == 4 {
                    return 0;
                }
                1
            }
            Self::Ravager => i32::from(!easy && bonus_wave),
        };
        if bound < 1 {
            0
        } else {
            random.next_int(bound + 1)
        }
    }
}

pub const fn groups_for(difficulty: Difficulty) -> i32 {
    match difficulty {
        Difficulty::Peaceful => 0,
        Difficulty::Easy => 3,
        Difficulty::Normal => 5,
        Difficulty::Hard => 7,
    }
}

pub fn rider_of_ravager(
    wave: i32,
    ravager_index: i32,
    normal_groups: i32,
    hard_groups: i32,
) -> Option<EntityKind> {
    if wave == normal_groups {
        Some(EntityKind::Pillager)
    } else if hard_groups <= wave {
        Some(if ravager_index == 0 {
            EntityKind::Evoker
        } else {
            EntityKind::Vindicator
        })
    } else {
        None
    }
}

pub fn enchant_odds(omen_level: i32) -> f32 {
    match omen_level {
        2 => 0.1,
        3 => 0.25,
        4 => 0.5,
        5 => 0.75,
        _ => 0.0,
    }
}

pub fn vindicator_sharpness(wave: i32) -> u8 {
    if groups_for(Difficulty::Normal) < wave {
        2
    } else {
        1
    }
}

pub fn pillager_quick_charge(wave: i32) -> Option<u8> {
    if groups_for(Difficulty::Normal) < wave {
        Some(2)
    } else if groups_for(Difficulty::Easy) < wave {
        Some(1)
    } else {
        None
    }
}

pub const fn can_be_leader(kind: EntityKind) -> bool {
    !matches!(kind, EntityKind::Witch | EntityKind::Ravager)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RaiderStatus {
    pub removed: bool,
    pub same_dimension: bool,
    pub position: BlockPos,
    pub tick_count: i32,
    pub no_action_time: i32,
    pub ticks_outside_raid: i32,
    pub health: f32,
    pub registered: bool,
    pub wave: i32,
    pub leader: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerStatus {
    pub id: PlayerId,
    pub position: BlockPos,
    pub alive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NewRaider {
    pub id: EntityId,
    pub health: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JoinInfo {
    pub raid: i32,
    pub wave: i32,
    pub place_at: Option<BlockPos>,
    pub leader: bool,
    pub enchant_odds: f32,
}

pub trait RaidWorld {
    fn difficulty(&self) -> Difficulty;
    fn has_chunk_at(&self, pos: BlockPos) -> bool;
    fn has_chunks_around(&self, pos: BlockPos, margin: i32) -> bool;
    fn is_village(&self, pos: BlockPos) -> bool;
    fn is_position_entity_ticking(&self, pos: BlockPos) -> bool;
    fn surface_height(&self, x: i32, z: i32) -> i32;
    fn spawn_position_ok(&self, pos: BlockPos) -> bool;
    fn snow_on_air(&self, pos: BlockPos) -> bool;
    fn players(&self) -> Vec<PlayerStatus>;
    fn raider(&self, id: EntityId) -> Option<RaiderStatus>;
    fn detach_raider(&mut self, id: EntityId);
    fn set_ticks_outside_raid(&mut self, id: EntityId, ticks: i32);
    fn create_raider(&mut self, kind: EntityKind) -> Option<NewRaider>;
    fn join_raider(&mut self, id: EntityId, info: JoinInfo);
    fn apply_hero_of_the_village(&mut self, uuid: Uuid, amplifier: i32) -> Option<PlayerId>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum RaidEvent {
    Horn(BlockPos),
    Finish {
        winners: Vec<PlayerId>,
    },
    Stop(RaidStopReason),
    SpawnWave {
        leader: Option<EntityId>,
        raiders: Vec<EntityId>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Raid {
    pub id: i32,
    started: bool,
    active: bool,
    ticks_active: i64,
    raid_omen_level: i32,
    groups_spawned: i32,
    raid_cooldown_ticks: i32,
    post_raid_ticks: i32,
    total_health: f32,
    num_groups: i32,
    status: RaidStatus,
    center: BlockPos,
    heroes: BTreeSet<Uuid>,
    celebration_ticks: i32,
    wave_spawn_pos: Option<BlockPos>,
    groups: BTreeMap<i32, BTreeSet<EntityId>>,
    leaders: BTreeMap<i32, EntityId>,
    random: LegacyRandom,
    bar: ServerBossEvent,
    bar_messages: BossMessages,
    events: Vec<RaidEvent>,
}

impl Raid {
    pub fn new(center: BlockPos, difficulty: Difficulty, bar_id: Uuid, seed: i64) -> Self {
        let mut raid = Self::blank(bar_id, seed);
        raid.active = true;
        raid.raid_cooldown_ticks = RAID_COOLDOWN_TICKS;
        raid.center = center;
        raid.num_groups = groups_for(difficulty);
        raid.status = RaidStatus::Ongoing;
        raid.bar.set_progress(0.0);
        raid
    }

    fn blank(bar_id: Uuid, seed: i64) -> Self {
        Self {
            id: 0,
            started: false,
            active: false,
            ticks_active: 0,
            raid_omen_level: 0,
            groups_spawned: 0,
            raid_cooldown_ticks: 0,
            post_raid_ticks: 0,
            total_health: 0.0,
            num_groups: 0,
            status: RaidStatus::Ongoing,
            center: BlockPos::new(0, 0, 0),
            heroes: BTreeSet::new(),
            celebration_ticks: 0,
            wave_spawn_pos: None,
            groups: BTreeMap::new(),
            leaders: BTreeMap::new(),
            random: LegacyRandom::new(seed),
            bar: ServerBossEvent::new(bar_id, raid_name(), BossColor::Red, BossOverlay::Notched10),
            bar_messages: Vec::new(),
            events: Vec::new(),
        }
    }

    pub const fn is_started(&self) -> bool {
        self.started
    }

    pub const fn is_active(&self) -> bool {
        self.active
    }

    pub const fn status(&self) -> RaidStatus {
        self.status
    }

    pub const fn is_stopped(&self) -> bool {
        matches!(self.status, RaidStatus::Stopped)
    }

    pub const fn is_victory(&self) -> bool {
        matches!(self.status, RaidStatus::Victory)
    }

    pub const fn is_loss(&self) -> bool {
        matches!(self.status, RaidStatus::Loss)
    }

    pub const fn is_in_progress(&self) -> bool {
        matches!(self.status, RaidStatus::Ongoing)
    }

    pub const fn is_over(&self) -> bool {
        self.is_victory() || self.is_loss()
    }

    pub const fn center(&self) -> BlockPos {
        self.center
    }

    pub const fn groups_spawned(&self) -> i32 {
        self.groups_spawned
    }

    pub const fn num_groups(&self) -> i32 {
        self.num_groups
    }

    pub const fn total_health(&self) -> f32 {
        self.total_health
    }

    pub const fn raid_omen_level(&self) -> i32 {
        self.raid_omen_level
    }

    pub const fn cooldown_ticks(&self) -> i32 {
        self.raid_cooldown_ticks
    }

    pub const fn ticks_active(&self) -> i64 {
        self.ticks_active
    }

    pub const fn wave_spawn_pos(&self) -> Option<BlockPos> {
        self.wave_spawn_pos
    }

    pub fn set_raid_omen_level(&mut self, level: i32) {
        self.raid_omen_level = level;
    }

    pub fn heroes(&self) -> impl Iterator<Item = Uuid> + '_ {
        self.heroes.iter().copied()
    }

    pub fn bar(&self) -> &ServerBossEvent {
        &self.bar
    }

    pub fn take_bar_messages(&mut self) -> BossMessages {
        std::mem::take(&mut self.bar_messages)
    }

    pub fn take_events(&mut self) -> Vec<RaidEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn has_first_wave_spawned(&self) -> bool {
        0 < self.groups_spawned
    }

    pub fn is_between_waves(&self) -> bool {
        self.has_first_wave_spawned()
            && self.total_raiders_alive() == 0
            && 0 < self.raid_cooldown_ticks
    }

    pub fn total_raiders_alive(&self) -> i32 {
        self.groups.values().map(|group| group.len() as i32).sum()
    }

    pub fn raiders(&self) -> BTreeSet<EntityId> {
        self.groups.values().flatten().copied().collect()
    }

    pub fn leader(&self, wave: i32) -> Option<EntityId> {
        self.leaders.get(&wave).copied()
    }

    pub fn set_leader(&mut self, wave: i32, id: EntityId) {
        self.leaders.insert(wave, id);
    }

    pub fn remove_leader(&mut self, wave: i32) {
        self.leaders.remove(&wave);
    }

    pub fn add_hero_of_the_village(&mut self, uuid: Uuid) {
        self.heroes.insert(uuid);
    }

    pub fn enchant_odds(&self) -> f32 {
        enchant_odds(self.raid_omen_level)
    }

    pub fn groups_for(&self, difficulty: Difficulty) -> i32 {
        groups_for(difficulty)
    }

    fn has_bonus_wave(&self) -> bool {
        1 < self.raid_omen_level
    }

    fn has_spawned_bonus_wave(&self) -> bool {
        self.num_groups < self.groups_spawned
    }

    fn is_final_wave(&self) -> bool {
        self.groups_spawned == self.num_groups
    }

    fn should_spawn_bonus_group(&self) -> bool {
        self.is_final_wave() && self.total_raiders_alive() == 0 && self.has_bonus_wave()
    }

    fn has_more_waves(&self) -> bool {
        if self.has_bonus_wave() {
            !self.has_spawned_bonus_wave()
        } else {
            !self.is_final_wave()
        }
    }

    fn should_spawn_group(&self) -> bool {
        self.raid_cooldown_ticks == 0
            && (self.groups_spawned < self.num_groups || self.should_spawn_bonus_group())
            && self.total_raiders_alive() == 0
    }

    pub fn absorb_raid_omen(&mut self, amplifier: i32) {
        self.raid_omen_level += amplifier + 1;
        self.raid_omen_level = self.raid_omen_level.clamp(0, MAX_RAID_OMEN_LEVEL);
    }

    pub fn stop(&mut self) {
        self.active = false;
        let messages = self.bar.remove_all_players();
        self.bar_messages.extend(messages);
        self.status = RaidStatus::Stopped;
    }

    pub fn stop_for(&mut self, reason: RaidStopReason) {
        self.events.push(RaidEvent::Stop(reason));
        self.stop();
    }

    fn bar_progress(&mut self, progress: f32) {
        let messages = self.bar.set_progress(progress);
        self.bar_messages.extend(messages);
    }

    fn bar_name(&mut self, name: Component) {
        let messages = self.bar.set_name(name);
        self.bar_messages.extend(messages);
    }

    fn bar_visible(&mut self, visible: bool) {
        let messages = self.bar.set_visible(visible);
        self.bar_messages.extend(messages);
    }

    pub fn update_bossbar(&mut self, health_of_living: f32) {
        let progress = (health_of_living / self.total_health).clamp(0.0, 1.0);
        self.bar_progress(progress);
    }

    pub fn is_nearest(&self, pos: BlockPos, others: &[(i32, BlockPos)]) -> bool {
        if !self.active {
            return false;
        }
        let own = distance_sqr(self.center, pos);
        own < NEARBY_RAID_DISTANCE_SQR
            && others.iter().all(|(_, center)| {
                let distance = distance_sqr(*center, pos);
                distance >= own
            })
    }

    fn update_players(&mut self, world: &impl RaidWorld, others: &[(i32, BlockPos)]) {
        let valid: BTreeSet<PlayerId> = world
            .players()
            .into_iter()
            .filter(|player| player.alive && self.is_nearest(player.position, others))
            .map(|player| player.id)
            .collect();
        let current: BTreeSet<PlayerId> = self.bar.players().collect();
        for player in valid.difference(&current) {
            let messages = self.bar.add_player(*player);
            self.bar_messages.extend(messages);
        }
        for player in current.difference(&valid) {
            let messages = self.bar.remove_player(*player);
            self.bar_messages.extend(messages);
        }
    }

    fn move_center_to_nearby_village_section(&mut self, world: &impl RaidWorld) {
        let origin = SectionPos::of(self.center);
        let mut best: Option<(f64, BlockPos)> = None;
        for z in -VILLAGE_SECTION_RADIUS..=VILLAGE_SECTION_RADIUS {
            for y in -VILLAGE_SECTION_RADIUS..=VILLAGE_SECTION_RADIUS {
                for x in -VILLAGE_SECTION_RADIUS..=VILLAGE_SECTION_RADIUS {
                    let section = SectionPos::new(origin.x + x, origin.y + y, origin.z + z);
                    let center = section.center();
                    if !world.is_village(center) {
                        continue;
                    }
                    let distance = distance_sqr(center, self.center);
                    if best.is_none_or(|(smallest, _)| distance < smallest) {
                        best = Some((distance, center));
                    }
                }
            }
        }
        if let Some((_, center)) = best {
            self.center = center;
        }
    }

    fn find_random_spawn_pos(
        &mut self,
        world: &impl RaidWorld,
        max_tries: i32,
    ) -> Option<BlockPos> {
        let seconds = self.raid_cooldown_ticks / 20;
        let radius = seconds as f32 * 0.22 - 0.24;
        let start = self.random.next_float() * 6.283_185_5;
        for attempt in 0..max_tries {
            let angle = start + (attempt as f32 * std::f32::consts::PI) / 8.0;
            let x = self.center.x
                + mth::floor(f64::from(mth::cos(f64::from(angle)) * 32.0 * radius))
                + self.random.next_int(3) * mth::floor(f64::from(radius));
            let z = self.center.z
                + mth::floor(f64::from(mth::sin(f64::from(angle)) * 32.0 * radius))
                + self.random.next_int(3) * mth::floor(f64::from(radius));
            let y = world.surface_height(x, z);
            if (y - self.center.y).abs() >= SPAWN_HEIGHT_RANGE {
                continue;
            }
            let pos = BlockPos::new(x, y, z);
            if world.is_village(pos) && seconds >= 8 {
                continue;
            }
            if !world.has_chunks_around(pos, SPAWN_CHUNK_MARGIN)
                || !world.is_position_entity_ticking(pos)
            {
                continue;
            }
            if world.spawn_position_ok(pos) || world.snow_on_air(pos) {
                return Some(pos);
            }
        }
        None
    }

    fn health_of_living_raiders(&self, world: &impl RaidWorld) -> f32 {
        self.groups
            .values()
            .flatten()
            .filter_map(|id| world.raider(*id))
            .map(|raider| raider.health)
            .sum()
    }

    pub fn refresh_bossbar(&mut self, world: &impl RaidWorld) {
        let health = self.health_of_living_raiders(world);
        self.update_bossbar(health);
    }

    pub fn add_wave_mob(
        &mut self,
        wave: i32,
        id: EntityId,
        health: f32,
        update_health: bool,
        world: &impl RaidWorld,
    ) {
        self.groups.entry(wave).or_default().insert(id);
        if update_health {
            self.total_health += health;
        }
        self.refresh_bossbar(world);
    }

    pub fn remove_from_raid(
        &mut self,
        world: &mut impl RaidWorld,
        id: EntityId,
        wave: i32,
        remove_from_total_health: bool,
        health: f32,
    ) {
        let removed = self
            .groups
            .get_mut(&wave)
            .is_some_and(|group| group.remove(&id));
        if removed {
            if remove_from_total_health {
                self.total_health -= health;
            }
            world.detach_raider(id);
            self.refresh_bossbar(world);
        }
    }

    pub fn join_raid(
        &mut self,
        world: &mut impl RaidWorld,
        wave: i32,
        raider: NewRaider,
        place_at: Option<BlockPos>,
        leader: bool,
    ) {
        self.add_wave_mob(wave, raider.id, raider.health, true, world);
        world.join_raider(
            raider.id,
            JoinInfo {
                raid: self.id,
                wave,
                place_at,
                leader,
                enchant_odds: self.enchant_odds(),
            },
        );
    }

    fn update_raiders(&mut self, world: &mut impl RaidWorld) {
        let mut stale: Vec<(EntityId, RaiderStatus)> = Vec::new();
        let all: Vec<EntityId> = self.groups.values().flatten().copied().collect();
        for id in all {
            let Some(status) = world.raider(id) else {
                stale.push((
                    id,
                    RaiderStatus {
                        removed: true,
                        same_dimension: false,
                        position: self.center,
                        tick_count: 0,
                        no_action_time: 0,
                        ticks_outside_raid: 0,
                        health: 0.0,
                        registered: false,
                        wave: self.wave_of(id),
                        leader: false,
                    },
                ));
                continue;
            };
            let near = !status.removed
                && status.same_dimension
                && distance_sqr(self.center, status.position) < RAIDER_RADIUS_SQR;
            if !near {
                stale.push((id, status));
                continue;
            }
            if status.tick_count > RAIDER_STALE_TICKS {
                let mut outside = status.ticks_outside_raid;
                let mut drop = !status.registered;
                if !world.is_village(status.position) && status.no_action_time > RAIDER_IDLE_TICKS {
                    outside += 1;
                    world.set_ticks_outside_raid(id, outside);
                }
                if outside > RAIDER_OUTSIDE_LIMIT {
                    drop = true;
                }
                if drop {
                    stale.push((id, status));
                }
            }
        }
        for (id, status) in stale {
            let wave = if status.wave == 0 {
                self.wave_of(id)
            } else {
                status.wave
            };
            self.remove_from_raid(world, id, wave, true, status.health);
            if status.leader || self.leaders.get(&wave) == Some(&id) {
                self.remove_leader(wave);
            }
        }
    }

    fn wave_of(&self, id: EntityId) -> i32 {
        self.groups
            .iter()
            .find(|(_, group)| group.contains(&id))
            .map_or(0, |(wave, _)| *wave)
    }

    fn spawn_group(&mut self, world: &mut impl RaidWorld, pos: BlockPos) {
        let wave = self.groups_spawned + 1;
        self.total_health = 0.0;
        let difficulty = world.difficulty();
        let bonus = self.should_spawn_bonus_group();
        let mut leader_taken = false;
        let mut leader = None;
        let mut spawned: Vec<EntityId> = Vec::new();
        for kind in RaiderType::ALL {
            let count = kind.default_spawns_for_wave(if bonus { self.num_groups } else { wave })
                + kind.potential_bonus_spawns(&mut self.random, wave, difficulty, bonus);
            let mut ravagers = 0;
            for _ in 0..count {
                let Some(raider) = world.create_raider(kind.kind()) else {
                    break;
                };
                let is_leader = !leader_taken && can_be_leader(kind.kind());
                if is_leader {
                    leader_taken = true;
                    leader = Some(raider.id);
                    self.set_leader(wave, raider.id);
                }
                self.join_raid(world, wave, raider, Some(pos), is_leader);
                spawned.push(raider.id);
                if kind != RaiderType::Ravager {
                    continue;
                }
                let rider = rider_of_ravager(
                    wave,
                    ravagers,
                    groups_for(Difficulty::Normal),
                    groups_for(Difficulty::Hard),
                )
                .and_then(|rider| world.create_raider(rider));
                ravagers += 1;
                if let Some(rider) = rider {
                    self.join_raid(world, wave, rider, Some(pos), false);
                    spawned.push(rider.id);
                }
            }
        }
        self.wave_spawn_pos = None;
        self.groups_spawned += 1;
        self.refresh_bossbar(world);
        self.events.push(RaidEvent::SpawnWave {
            leader,
            raiders: spawned,
        });
    }

    pub fn tick(&mut self, world: &mut impl RaidWorld, others: &[(i32, BlockPos)]) {
        if self.is_stopped() {
            return;
        }
        if self.status != RaidStatus::Ongoing {
            if self.is_over() {
                self.celebration_ticks += 1;
                if self.celebration_ticks >= CELEBRATION_TICKS {
                    self.stop_for(RaidStopReason::Finished);
                    return;
                }
                if self.celebration_ticks % 20 == 0 {
                    self.update_players(world, others);
                    self.bar_visible(true);
                    if self.is_victory() {
                        self.bar_progress(0.0);
                        self.bar_name(Component::translatable(RAID_VICTORY_KEY, Vec::new()));
                    } else {
                        self.bar_name(Component::translatable(RAID_DEFEAT_KEY, Vec::new()));
                    }
                }
            }
            return;
        }
        let was_active = self.active;
        self.active = world.has_chunk_at(self.center);
        if world.difficulty() == Difficulty::Peaceful {
            self.stop_for(RaidStopReason::Peace);
            return;
        }
        if was_active != self.active {
            self.bar_visible(self.active);
        }
        if !self.active {
            return;
        }
        if !world.is_village(self.center) {
            self.move_center_to_nearby_village_section(world);
        }
        if !world.is_village(self.center) {
            if self.groups_spawned < 1 {
                self.stop_for(RaidStopReason::NotInVillage);
            } else {
                self.status = RaidStatus::Loss;
                self.events.push(RaidEvent::Finish {
                    winners: Vec::new(),
                });
            }
        }
        self.ticks_active += 1;
        if self.ticks_active >= RAID_TIMEOUT_TICKS {
            self.stop_for(RaidStopReason::Timeout);
            return;
        }
        let alive = self.total_raiders_alive();
        if alive == 0 && self.has_more_waves() {
            if self.raid_cooldown_ticks < 1 {
                if self.raid_cooldown_ticks == 0 && 0 < self.groups_spawned {
                    self.raid_cooldown_ticks = RAID_COOLDOWN_TICKS;
                    self.bar_name(raid_name());
                    return;
                }
            } else {
                let present = self.wave_spawn_pos.is_some();
                let mut refind = !present && self.raid_cooldown_ticks % 5 == 0;
                if let Some(pos) = self.wave_spawn_pos
                    && !world.is_position_entity_ticking(pos)
                {
                    refind = true;
                }
                if refind {
                    self.wave_spawn_pos = self.find_random_spawn_pos(world, FIRST_SPAWN_TRIES);
                }
                if self.raid_cooldown_ticks == RAID_COOLDOWN_TICKS
                    || self.raid_cooldown_ticks % 20 == 0
                {
                    self.update_players(world, others);
                }
                self.raid_cooldown_ticks -= 1;
                let progress = ((RAID_COOLDOWN_TICKS - self.raid_cooldown_ticks) as f32
                    / RAID_COOLDOWN_TICKS as f32)
                    .clamp(0.0, 1.0);
                self.bar_progress(progress);
            }
        }
        if self.ticks_active % 20 == 0 {
            self.update_players(world, others);
            self.update_raiders(world);
            if !(1..3).contains(&alive) {
                self.bar_name(raid_name());
            } else {
                self.bar_name(raid_name_with_raiders(alive));
            }
        }
        let mut horn = false;
        let mut failures = 0;
        while self.should_spawn_group() {
            let pos = match self.wave_spawn_pos {
                Some(pos) => Some(pos),
                None => self.find_random_spawn_pos(world, RETRY_SPAWN_TRIES),
            };
            match pos {
                None => failures += 1,
                Some(pos) => {
                    self.started = true;
                    self.spawn_group(world, pos);
                    if !horn {
                        self.events.push(RaidEvent::Horn(pos));
                        horn = true;
                    }
                }
            }
            if failures >= UNSPAWNABLE_LIMIT {
                self.stop_for(RaidStopReason::Unspawnable);
                break;
            }
        }
        if self.started && !self.has_more_waves() && alive == 0 {
            if self.post_raid_ticks < POST_RAID_TICKS {
                self.post_raid_ticks += 1;
            } else {
                self.status = RaidStatus::Victory;
                let amplifier = self.raid_omen_level - 1;
                let heroes: Vec<Uuid> = self.heroes.iter().copied().collect();
                let winners = heroes
                    .into_iter()
                    .filter_map(|uuid| world.apply_hero_of_the_village(uuid, amplifier))
                    .collect();
                self.events.push(RaidEvent::Finish { winners });
            }
        }
    }

    pub fn to_nbt(&self) -> Compound {
        let heroes = self
            .heroes
            .iter()
            .map(|uuid| uuid_tag(*uuid))
            .collect::<Vec<_>>();
        Compound::new()
            .with("started", self.started)
            .with("active", self.active)
            .with("ticks_active", self.ticks_active)
            .with("raid_omen_level", self.raid_omen_level)
            .with("groups_spawned", self.groups_spawned)
            .with("cooldown_ticks", self.raid_cooldown_ticks)
            .with("post_raid_ticks", self.post_raid_ticks)
            .with("total_health", self.total_health)
            .with("group_count", self.num_groups)
            .with("status", self.status.name())
            .with(
                "center",
                Tag::IntArray(vec![self.center.x, self.center.y, self.center.z]),
            )
            .with("heroes_of_the_village", Tag::List(heroes))
    }

    pub fn from_nbt(nbt: &Compound, bar_id: Uuid, seed: i64) -> Option<Self> {
        let mut raid = Self::blank(bar_id, seed);
        raid.started = nbt.get_bool("started")?;
        raid.active = nbt.get_bool("active")?;
        raid.ticks_active = nbt.get_i64("ticks_active")?;
        raid.raid_omen_level = nbt.get_i32("raid_omen_level")?;
        raid.groups_spawned = nbt.get_i32("groups_spawned")?;
        raid.raid_cooldown_ticks = nbt.get_i32("cooldown_ticks")?;
        raid.post_raid_ticks = nbt.get_i32("post_raid_ticks")?;
        raid.total_health = match nbt.get("total_health")? {
            Tag::Float(value) => *value,
            other => other.as_f64()? as f32,
        };
        raid.num_groups = nbt.get_i32("group_count")?;
        raid.status = RaidStatus::parse(nbt.get_str("status")?)?;
        raid.center = match nbt.get_int_array("center")? {
            [x, y, z] => BlockPos::new(*x, *y, *z),
            _ => return None,
        };
        raid.heroes = nbt
            .get_list("heroes_of_the_village")
            .unwrap_or_default()
            .iter()
            .filter_map(read_uuid)
            .collect();
        raid.bar.set_progress(0.0);
        Some(raid)
    }
}

fn distance_sqr(a: BlockPos, b: BlockPos) -> f64 {
    let dx = f64::from(a.x - b.x);
    let dy = f64::from(a.y - b.y);
    let dz = f64::from(a.z - b.z);
    dx * dx + dy * dy + dz * dz
}

#[derive(Debug, Clone, PartialEq)]
pub struct Raids {
    raids: BTreeMap<i32, Raid>,
    next_id: i32,
    tick: i32,
    dirty: bool,
}

impl Default for Raids {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    Refused,
    Unchanged { raid: i32 },
    Triggered { raid: i32, first_wave: bool },
}

#[derive(Debug, Clone, Copy)]
pub struct Omen<'a> {
    pub pos: BlockPos,
    pub village_pois: &'a [BlockPos],
    pub difficulty: Difficulty,
    pub bar_id: Uuid,
    pub seed: i64,
    pub amplifier: Option<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RaidOutput {
    pub raid: i32,
    pub bar_id: Uuid,
    pub messages: BossMessages,
    pub events: Vec<RaidEvent>,
}

impl Raids {
    pub fn new() -> Self {
        Self {
            raids: BTreeMap::new(),
            next_id: 1,
            tick: 0,
            dirty: true,
        }
    }

    pub fn get(&self, id: i32) -> Option<&Raid> {
        self.raids.get(&id)
    }

    pub fn get_mut(&mut self, id: i32) -> Option<&mut Raid> {
        self.raids.get_mut(&id)
    }

    pub fn ids(&self) -> Vec<i32> {
        self.raids.keys().copied().collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Raid> {
        self.raids.values()
    }

    pub fn is_empty(&self) -> bool {
        self.raids.is_empty()
    }

    pub const fn next_id(&self) -> i32 {
        self.next_id
    }

    pub const fn tick_count(&self) -> i32 {
        self.tick
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    pub fn set_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn nearby_raid(&self, pos: BlockPos, max_distance_sqr: i32) -> Option<i32> {
        let mut best = None;
        let mut limit = f64::from(max_distance_sqr);
        for (id, raid) in &self.raids {
            let distance = distance_sqr(raid.center, pos);
            if raid.active && distance < limit {
                best = Some(*id);
                limit = distance;
            }
        }
        best
    }

    pub fn raid_at(&self, pos: BlockPos) -> Option<i32> {
        self.nearby_raid(pos, NEARBY_RAID_DISTANCE_SQR as i32)
    }

    pub fn raid_centers_in_chunk(&self, chunk: crate::ChunkPos) -> Vec<BlockPos> {
        self.raids
            .values()
            .map(|raid| raid.center)
            .filter(|center| center.chunk() == chunk)
            .collect()
    }

    pub fn can_join_raid(alive: bool, can_join: bool, no_action_time: i32) -> bool {
        alive && can_join && no_action_time < RAIDER_IDLE_TICKS + 1
    }

    pub fn create_or_extend(
        &mut self,
        omen: &Omen<'_>,
        allow: impl FnOnce(&Raid) -> bool,
    ) -> Trigger {
        let Omen {
            pos: omen_pos,
            village_pois,
            difficulty,
            bar_id,
            seed,
            amplifier: omen_amplifier,
        } = *omen;
        let mut pos = omen_pos;
        if !village_pois.is_empty() {
            let sum = village_pois.iter().fold((0.0, 0.0, 0.0), |sum, poi| {
                (
                    sum.0 + f64::from(poi.x),
                    sum.1 + f64::from(poi.y),
                    sum.2 + f64::from(poi.z),
                )
            });
            let scale = 1.0 / village_pois.len() as f64;
            pos = BlockPos::new(
                mth::floor(sum.0 * scale),
                mth::floor(sum.1 * scale),
                mth::floor(sum.2 * scale),
            );
        }
        let existing = self.raid_at(pos);
        let mut raid = existing
            .and_then(|id| self.raids.remove(&id))
            .unwrap_or_else(|| Raid::new(pos, difficulty, bar_id, seed));
        let stored = existing.is_some();
        self.dirty = true;
        if raid.started && (!raid.is_in_progress() || MAX_RAID_OMEN_LEVEL <= raid.raid_omen_level) {
            let id = raid.id;
            self.raids.insert(id, raid);
            return Trigger::Unchanged { raid: id };
        }
        if !allow(&raid) {
            if stored {
                let id = raid.id;
                self.raids.insert(id, raid);
            }
            return Trigger::Refused;
        }
        if !stored {
            self.next_id += 1;
            raid.id = self.next_id;
        }
        let first_wave = omen_amplifier.is_some() && !raid.has_first_wave_spawned();
        if let Some(amplifier) = omen_amplifier {
            raid.absorb_raid_omen(amplifier);
        }
        let id = raid.id;
        self.raids.insert(id, raid);
        Trigger::Triggered {
            raid: id,
            first_wave,
        }
    }

    pub fn tick(&mut self, world: &mut impl RaidWorld, enabled: bool) -> Vec<RaidOutput> {
        self.tick += 1;
        let mut outputs = Vec::new();
        for id in self.ids() {
            let Some(mut raid) = self.raids.remove(&id) else {
                continue;
            };
            if !enabled {
                raid.stop();
            }
            let already_stopped = raid.is_stopped();
            if !already_stopped {
                let others: Vec<(i32, BlockPos)> = self
                    .raids
                    .iter()
                    .filter(|(_, other)| other.active)
                    .map(|(other_id, other)| (*other_id, other.center))
                    .collect();
                raid.tick(world, &others);
            }
            let output = RaidOutput {
                raid: id,
                bar_id: raid.bar.id(),
                messages: raid.take_bar_messages(),
                events: raid.take_events(),
            };
            if !output.messages.is_empty() || !output.events.is_empty() {
                outputs.push(output);
            }
            if already_stopped {
                self.dirty = true;
            } else {
                self.raids.insert(id, raid);
            }
        }
        if self.tick % 200 == 0 {
            self.dirty = true;
        }
        outputs
    }

    pub fn to_nbt(&self) -> Compound {
        let raids = self
            .raids
            .iter()
            .map(|(id, raid)| {
                let mut entry = raid.to_nbt();
                entry.insert("id", *id);
                Tag::Compound(entry)
            })
            .collect();
        Compound::new()
            .with("raids", Tag::List(raids))
            .with("next_id", self.next_id)
            .with("tick", self.tick)
    }

    pub fn from_nbt(nbt: &Compound, mut bar_id: impl FnMut() -> Uuid, seed: i64) -> Self {
        let mut raids = BTreeMap::new();
        for entry in nbt.get_list("raids").unwrap_or_default() {
            let Some(entry) = entry.as_compound() else {
                continue;
            };
            let Some(id) = entry.get_i32("id") else {
                continue;
            };
            if let Some(mut raid) =
                Raid::from_nbt(entry, bar_id(), seed.wrapping_add(i64::from(id)))
            {
                raid.id = id;
                raids.insert(id, raid);
            }
        }
        Self {
            raids,
            next_id: nbt.get_i32("next_id").unwrap_or(1),
            tick: nbt.get_i32("tick").unwrap_or(0),
            dirty: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Mock {
        difficulty: Difficulty,
        village: bool,
        loaded: bool,
        players: Vec<PlayerStatus>,
        raiders: BTreeMap<EntityId, RaiderStatus>,
        next_id: i32,
        created: Vec<EntityKind>,
        joined: Vec<(EntityId, JoinInfo)>,
        detached: Vec<EntityId>,
        outside: Vec<(EntityId, i32)>,
        heroes: Vec<(Uuid, i32)>,
        refuse: Vec<EntityKind>,
        spawnable: bool,
    }

    impl Mock {
        fn new() -> Self {
            Self {
                difficulty: Difficulty::Normal,
                village: true,
                loaded: true,
                players: Vec::new(),
                raiders: BTreeMap::new(),
                next_id: 100,
                created: Vec::new(),
                joined: Vec::new(),
                detached: Vec::new(),
                outside: Vec::new(),
                heroes: Vec::new(),
                refuse: vec![EntityKind::Evoker],
                spawnable: true,
            }
        }
    }

    impl RaidWorld for Mock {
        fn difficulty(&self) -> Difficulty {
            self.difficulty
        }

        fn has_chunk_at(&self, _pos: BlockPos) -> bool {
            self.loaded
        }

        fn has_chunks_around(&self, _pos: BlockPos, _margin: i32) -> bool {
            true
        }

        fn is_village(&self, _pos: BlockPos) -> bool {
            self.village
        }

        fn is_position_entity_ticking(&self, _pos: BlockPos) -> bool {
            true
        }

        fn surface_height(&self, _x: i32, _z: i32) -> i32 {
            64
        }

        fn spawn_position_ok(&self, _pos: BlockPos) -> bool {
            self.spawnable
        }

        fn snow_on_air(&self, _pos: BlockPos) -> bool {
            false
        }

        fn players(&self) -> Vec<PlayerStatus> {
            self.players.clone()
        }

        fn raider(&self, id: EntityId) -> Option<RaiderStatus> {
            self.raiders.get(&id).copied()
        }

        fn detach_raider(&mut self, id: EntityId) {
            self.detached.push(id);
        }

        fn set_ticks_outside_raid(&mut self, id: EntityId, ticks: i32) {
            self.outside.push((id, ticks));
            if let Some(raider) = self.raiders.get_mut(&id) {
                raider.ticks_outside_raid = ticks;
            }
        }

        fn create_raider(&mut self, kind: EntityKind) -> Option<NewRaider> {
            if self.refuse.contains(&kind) {
                return None;
            }
            self.next_id += 1;
            let id = EntityId(self.next_id);
            self.created.push(kind);
            self.raiders.insert(
                id,
                RaiderStatus {
                    removed: false,
                    same_dimension: true,
                    position: BlockPos::new(0, 64, 0),
                    tick_count: 0,
                    no_action_time: 0,
                    ticks_outside_raid: 0,
                    health: 20.0,
                    registered: true,
                    wave: 0,
                    leader: false,
                },
            );
            Some(NewRaider { id, health: 20.0 })
        }

        fn join_raider(&mut self, id: EntityId, info: JoinInfo) {
            self.joined.push((id, info));
            if let Some(raider) = self.raiders.get_mut(&id) {
                raider.wave = info.wave;
                raider.leader = info.leader;
            }
        }

        fn apply_hero_of_the_village(&mut self, uuid: Uuid, amplifier: i32) -> Option<PlayerId> {
            self.heroes.push((uuid, amplifier));
            Some(PlayerId(uuid))
        }
    }

    fn raid(difficulty: Difficulty) -> Raid {
        let mut raid = Raid::new(BlockPos::new(0, 64, 0), difficulty, Uuid::from_u128(1), 7);
        raid.id = 2;
        raid
    }

    fn run(raid: &mut Raid, world: &mut Mock, ticks: i32) -> Vec<RaidEvent> {
        let mut events = Vec::new();
        for _ in 0..ticks {
            raid.tick(world, &[]);
            events.extend(raid.take_events());
        }
        events
    }

    fn kill_all(raid: &mut Raid, world: &mut Mock) {
        let alive: Vec<EntityId> = raid.raiders().into_iter().collect();
        for id in alive {
            let wave = world.raiders[&id].wave;
            raid.remove_from_raid(world, id, wave, false, 20.0);
            world.raiders.remove(&id);
        }
    }

    #[test]
    fn the_wave_tables_match_vanilla() {
        assert_eq!(RaiderType::Vindicator.default_spawns_for_wave(2), 2);
        assert_eq!(RaiderType::Pillager.default_spawns_for_wave(1), 4);
        assert_eq!(RaiderType::Witch.default_spawns_for_wave(4), 3);
        assert_eq!(RaiderType::Ravager.default_spawns_for_wave(3), 1);
        assert_eq!(RaiderType::Evoker.default_spawns_for_wave(7), 2);
        assert_eq!(RaiderType::Vindicator.default_spawns_for_wave(-1), 0);
    }

    #[test]
    fn waves_past_the_table_grow_by_their_divisor() {
        assert_eq!(RaiderType::Vindicator.default_spawns_for_wave(10), 5 + 1);
        assert_eq!(RaiderType::Pillager.default_spawns_for_wave(9), 2 + 1);
        assert_eq!(RaiderType::Witch.default_spawns_for_wave(12), 1 + 1);
        assert_eq!(RaiderType::Ravager.default_spawns_for_wave(13), 2 + 1);
        assert_eq!(RaiderType::Evoker.default_spawns_for_wave(11), 2 + 1);
    }

    #[test]
    fn the_group_count_depends_on_difficulty() {
        assert_eq!(groups_for(Difficulty::Peaceful), 0);
        assert_eq!(groups_for(Difficulty::Easy), 3);
        assert_eq!(groups_for(Difficulty::Normal), 5);
        assert_eq!(groups_for(Difficulty::Hard), 7);
    }

    #[test]
    fn bonus_spawns_follow_the_difficulty_rules() {
        let mut random = LegacyRandom::new(1);
        assert_eq!(
            RaiderType::Evoker.potential_bonus_spawns(&mut random, 5, Difficulty::Hard, true),
            0
        );
        assert_eq!(
            RaiderType::Witch.potential_bonus_spawns(&mut random, 4, Difficulty::Hard, false),
            0
        );
        assert_eq!(
            RaiderType::Witch.potential_bonus_spawns(&mut random, 5, Difficulty::Easy, false),
            0
        );
        assert_eq!(
            RaiderType::Ravager.potential_bonus_spawns(&mut random, 5, Difficulty::Normal, false),
            0
        );
        let mut seen = BTreeSet::new();
        for _ in 0..200 {
            seen.insert(RaiderType::Pillager.potential_bonus_spawns(
                &mut random,
                2,
                Difficulty::Hard,
                false,
            ));
        }
        assert_eq!(seen, BTreeSet::from([0, 1, 2]));
        let mut mirror = LegacyRandom::new(5);
        let mut tested = LegacyRandom::new(5);
        let expected = {
            let bound = mirror.next_int(2);
            if bound < 1 {
                0
            } else {
                mirror.next_int(bound + 1)
            }
        };
        assert_eq!(
            RaiderType::Vindicator.potential_bonus_spawns(&mut tested, 3, Difficulty::Easy, false),
            expected
        );
    }

    #[test]
    fn ravager_riders_depend_on_the_wave() {
        assert_eq!(rider_of_ravager(5, 0, 5, 7), Some(EntityKind::Pillager));
        assert_eq!(rider_of_ravager(7, 0, 5, 7), Some(EntityKind::Evoker));
        assert_eq!(rider_of_ravager(7, 1, 5, 7), Some(EntityKind::Vindicator));
        assert_eq!(rider_of_ravager(4, 0, 5, 7), None);
        assert_eq!(rider_of_ravager(6, 0, 5, 7), None);
    }

    #[test]
    fn enchant_odds_climb_with_the_omen_level() {
        assert_eq!(enchant_odds(1), 0.0);
        assert_eq!(enchant_odds(2), 0.1);
        assert_eq!(enchant_odds(5), 0.75);
        assert_eq!(vindicator_sharpness(5), 1);
        assert_eq!(vindicator_sharpness(6), 2);
        assert_eq!(pillager_quick_charge(3), None);
        assert_eq!(pillager_quick_charge(4), Some(1));
        assert_eq!(pillager_quick_charge(6), Some(2));
        assert!(!can_be_leader(EntityKind::Witch));
        assert!(!can_be_leader(EntityKind::Ravager));
        assert!(can_be_leader(EntityKind::Pillager));
    }

    #[test]
    fn a_fresh_raid_waits_the_cooldown_then_spawns_the_first_wave_with_a_horn() {
        let mut world = Mock::new();
        let mut raid = raid(Difficulty::Normal);
        let events = run(&mut raid, &mut world, 299);
        assert!(events.is_empty());
        assert!(!raid.is_started());
        let events = run(&mut raid, &mut world, 2);
        assert!(raid.is_started());
        assert_eq!(raid.groups_spawned(), 1);
        assert!(
            events
                .iter()
                .any(|event| matches!(event, RaidEvent::Horn(_)))
        );
        let wave = events
            .iter()
            .find_map(|event| match event {
                RaidEvent::SpawnWave { leader, raiders } => Some((*leader, raiders.clone())),
                _ => None,
            })
            .unwrap();
        assert!(wave.0.is_some());
        assert_eq!(wave.1.len() as i32, raid.total_raiders_alive());
        assert!(
            world
                .created
                .iter()
                .filter(|kind| **kind == EntityKind::Pillager)
                .count()
                >= 4
        );
        assert!(raid.total_health() > 0.0);
        assert!(world.joined.iter().all(|(_, info)| info.wave == 1));
        assert_eq!(
            world.joined.iter().filter(|(_, info)| info.leader).count(),
            1
        );
    }

    #[test]
    fn the_leader_is_the_first_raider_that_can_lead() {
        let mut world = Mock::new();
        let mut raid = raid(Difficulty::Normal);
        run(&mut raid, &mut world, 301);
        let leader = raid.leader(1).unwrap();
        let (_, info) = world.joined.iter().find(|(id, _)| *id == leader).unwrap();
        assert!(info.leader);
        assert_eq!(world.joined[0].0, leader);
    }

    #[test]
    fn the_bar_progress_follows_the_cooldown_then_the_raider_health() {
        let mut world = Mock::new();
        let mut raid = raid(Difficulty::Normal);
        run(&mut raid, &mut world, 150);
        assert!((raid.bar().progress() - 0.5).abs() < 0.01);
        run(&mut raid, &mut world, 151);
        assert!((raid.bar().progress() - 1.0).abs() < 0.01);
        let id = *raid.raiders().iter().next().unwrap();
        world.raiders.get_mut(&id).unwrap().health = 0.0;
        raid.refresh_bossbar(&world);
        assert!(raid.bar().progress() < 1.0);
    }

    #[test]
    fn killing_every_raider_starts_the_next_wave_after_the_cooldown() {
        let mut world = Mock::new();
        let mut raid = raid(Difficulty::Normal);
        run(&mut raid, &mut world, 301);
        kill_all(&mut raid, &mut world);
        assert_eq!(raid.total_raiders_alive(), 0);
        run(&mut raid, &mut world, 1);
        assert_eq!(raid.cooldown_ticks(), RAID_COOLDOWN_TICKS);
        let events = run(&mut raid, &mut world, 301);
        assert_eq!(raid.groups_spawned(), 2);
        assert!(
            events
                .iter()
                .any(|event| matches!(event, RaidEvent::Horn(_)))
        );
    }

    #[test]
    fn clearing_the_last_wave_wins_after_the_post_raid_delay_and_rewards_the_heroes() {
        let mut world = Mock::new();
        let mut raid = raid(Difficulty::Easy);
        let hero = Uuid::from_u128(77);
        raid.add_hero_of_the_village(hero);
        raid.set_raid_omen_level(1);
        let mut finish = None;
        for _ in 0..3 {
            run(&mut raid, &mut world, 301);
            kill_all(&mut raid, &mut world);
            run(&mut raid, &mut world, 1);
        }
        assert_eq!(raid.groups_spawned(), 3);
        for _ in 0..60 {
            raid.tick(&mut world, &[]);
            for event in raid.take_events() {
                if let RaidEvent::Finish { winners } = event {
                    finish = Some(winners);
                }
            }
        }
        assert!(raid.is_victory());
        assert_eq!(finish.unwrap(), vec![PlayerId(hero)]);
        assert_eq!(world.heroes, vec![(hero, 0)]);
    }

    #[test]
    fn a_bonus_wave_follows_the_final_wave_with_an_omen_of_two_or_more() {
        let mut world = Mock::new();
        let mut raid = raid(Difficulty::Easy);
        raid.set_raid_omen_level(2);
        for _ in 0..3 {
            run(&mut raid, &mut world, 301);
            kill_all(&mut raid, &mut world);
            run(&mut raid, &mut world, 1);
        }
        assert_eq!(raid.groups_spawned(), 3);
        run(&mut raid, &mut world, 301);
        assert_eq!(raid.groups_spawned(), 4);
        assert!(!raid.is_victory());
        kill_all(&mut raid, &mut world);
        run(&mut raid, &mut world, 60);
        assert!(raid.is_victory());
    }

    #[test]
    fn a_raid_on_peaceful_stops_with_the_peace_reason() {
        let mut world = Mock::new();
        world.difficulty = Difficulty::Peaceful;
        let mut raid = raid(Difficulty::Normal);
        let events = run(&mut raid, &mut world, 1);
        assert_eq!(events, vec![RaidEvent::Stop(RaidStopReason::Peace)]);
        assert!(raid.is_stopped());
    }

    #[test]
    fn a_raid_that_never_found_a_village_stops_and_a_started_one_is_lost() {
        let mut world = Mock::new();
        world.village = false;
        let mut raid = raid(Difficulty::Normal);
        let events = run(&mut raid, &mut world, 1);
        assert_eq!(events, vec![RaidEvent::Stop(RaidStopReason::NotInVillage)]);
        let mut world = Mock::new();
        let mut raid = raid_started(&mut world);
        world.village = false;
        let events = run(&mut raid, &mut world, 1);
        assert!(raid.is_loss());
        assert_eq!(
            events,
            vec![RaidEvent::Finish {
                winners: Vec::new()
            }]
        );
    }

    fn raid_started(world: &mut Mock) -> Raid {
        let mut raid = raid(Difficulty::Normal);
        run(&mut raid, world, 301);
        raid
    }

    #[test]
    fn a_lost_raid_celebrates_then_stops_after_thirty_seconds() {
        let mut world = Mock::new();
        let mut raid = raid_started(&mut world);
        world.village = false;
        run(&mut raid, &mut world, 1);
        assert!(raid.is_loss());
        let events = run(&mut raid, &mut world, 598);
        assert!(!raid.is_stopped(), "{events:?}");
        let events = run(&mut raid, &mut world, 2);
        assert!(raid.is_stopped());
        assert_eq!(events, vec![RaidEvent::Stop(RaidStopReason::Finished)]);
    }

    #[test]
    fn a_raid_times_out_after_forty_eight_thousand_ticks() {
        let mut world = Mock::new();
        let mut raid = raid(Difficulty::Normal);
        world.spawnable = false;
        let mut events = Vec::new();
        while !raid.is_stopped() {
            raid.tick(&mut world, &[]);
            events.extend(raid.take_events());
            if raid.ticks_active() > 50_000 {
                break;
            }
        }
        assert_eq!(
            events,
            vec![RaidEvent::Stop(RaidStopReason::Unspawnable)],
            "{}",
            raid.ticks_active()
        );
        let mut world = Mock::new();
        let mut raid = raid_started(&mut world);
        for raider in world.raiders.values_mut() {
            raider.tick_count = 0;
        }
        raid.ticks_active = RAID_TIMEOUT_TICKS - 1;
        let events = run(&mut raid, &mut world, 1);
        assert_eq!(events, vec![RaidEvent::Stop(RaidStopReason::Timeout)]);
    }

    #[test]
    fn six_failed_spawn_searches_in_one_tick_stop_the_raid() {
        let mut world = Mock::new();
        world.spawnable = false;
        let mut raid = raid(Difficulty::Normal);
        let events = run(&mut raid, &mut world, 301);
        assert_eq!(events, vec![RaidEvent::Stop(RaidStopReason::Unspawnable)]);
        assert!(raid.is_stopped());
        assert!(!raid.is_started());
    }

    #[test]
    fn raiders_that_wandered_far_or_are_gone_leave_the_raid_and_a_dead_leader_is_dropped() {
        let mut world = Mock::new();
        let mut raid = raid_started(&mut world);
        let leader = raid.leader(1).unwrap();
        let before = raid.total_raiders_alive();
        world.raiders.get_mut(&leader).unwrap().position = BlockPos::new(500, 64, 0);
        let gone = *raid.raiders().iter().find(|id| **id != leader).unwrap();
        world.raiders.remove(&gone);
        let mut ticks = 0;
        while raid.ticks_active() % 20 != 0 || ticks == 0 {
            raid.tick(&mut world, &[]);
            ticks += 1;
        }
        assert_eq!(raid.total_raiders_alive(), before - 2);
        assert_eq!(raid.leader(1), None);
        assert!(world.detached.contains(&leader));
    }

    #[test]
    fn idle_raiders_outside_a_village_are_dropped_after_thirty_checks() {
        let mut world = Mock::new();
        let mut raid = raid_started(&mut world);
        world.village = false;
        raid.center = BlockPos::new(0, 64, 0);
        let ids: Vec<_> = raid.raiders().into_iter().collect();
        for id in &ids {
            let raider = world.raiders.get_mut(id).unwrap();
            raider.tick_count = 700;
            raider.no_action_time = 2_500;
        }
        raid.groups_spawned = 1;
        world.village = true;
        let village_center = raid.center;
        let mut village = Mock::new();
        village.raiders = world.raiders.clone();
        village.village = false;
        let mut copy = raid.clone();
        copy.status = RaidStatus::Ongoing;
        for _ in 0..40 {
            copy.update_raiders(&mut village);
        }
        assert_eq!(copy.total_raiders_alive(), 0);
        assert_eq!(copy.center(), village_center);
    }

    #[test]
    fn only_alive_players_whose_nearest_raid_is_this_one_see_the_bar() {
        let mut world = Mock::new();
        let near = PlayerStatus {
            id: PlayerId(Uuid::from_u128(1)),
            position: BlockPos::new(10, 64, 0),
            alive: true,
        };
        let dead = PlayerStatus {
            id: PlayerId(Uuid::from_u128(2)),
            position: BlockPos::new(10, 64, 0),
            alive: false,
        };
        let far = PlayerStatus {
            id: PlayerId(Uuid::from_u128(3)),
            position: BlockPos::new(200, 64, 0),
            alive: true,
        };
        let closer_elsewhere = PlayerStatus {
            id: PlayerId(Uuid::from_u128(4)),
            position: BlockPos::new(60, 64, 0),
            alive: true,
        };
        world.players = vec![near, dead, far, closer_elsewhere];
        let mut raid = raid(Difficulty::Normal);
        let others = [(9, BlockPos::new(70, 64, 0))];
        raid.update_players(&world, &others);
        let seen: Vec<PlayerId> = raid.bar().players().collect();
        assert_eq!(seen, vec![near.id]);
        let messages = raid.take_bar_messages();
        assert_eq!(messages.len(), 1);
        world.players.clear();
        raid.update_players(&world, &others);
        assert_eq!(raid.bar().players().count(), 0);
    }

    #[test]
    fn stopping_removes_every_player_from_the_bar() {
        let mut raid = raid(Difficulty::Normal);
        let player = PlayerId(Uuid::from_u128(1));
        raid.bar.add_player(player);
        raid.stop();
        assert_eq!(raid.bar().players().count(), 0);
        assert_eq!(
            raid.take_bar_messages(),
            vec![(player, crate::boss_event::BossUpdate::Remove)]
        );
        assert!(!raid.is_active());
    }

    #[test]
    fn spawn_positions_circle_the_center_and_shrink_with_the_cooldown() {
        let mut world = Mock::new();
        world.village = false;
        let mut raid = raid(Difficulty::Normal);
        let pos = raid.find_random_spawn_pos(&world, 8).unwrap();
        assert_eq!(pos.y, 64);
        let far = distance_sqr(pos, raid.center());
        assert!((90.0 * 90.0..110.0 * 110.0).contains(&far), "{far}");
        raid.raid_cooldown_ticks = 0;
        let near = raid.find_random_spawn_pos(&world, 8).unwrap();
        assert!(distance_sqr(near, raid.center()) < 16.0 * 16.0);
    }

    #[test]
    fn a_spawn_search_gives_up_when_nothing_is_valid() {
        let mut world = Mock::new();
        world.spawnable = false;
        let mut raid = raid(Difficulty::Normal);
        assert_eq!(raid.find_random_spawn_pos(&world, 20), None);
    }

    #[test]
    fn the_center_moves_to_the_nearest_village_section() {
        struct Villages(BlockPos);
        impl RaidWorld for Villages {
            fn difficulty(&self) -> Difficulty {
                Difficulty::Normal
            }
            fn has_chunk_at(&self, _: BlockPos) -> bool {
                true
            }
            fn has_chunks_around(&self, _: BlockPos, _: i32) -> bool {
                true
            }
            fn is_village(&self, pos: BlockPos) -> bool {
                pos == self.0
            }
            fn is_position_entity_ticking(&self, _: BlockPos) -> bool {
                true
            }
            fn surface_height(&self, _: i32, _: i32) -> i32 {
                64
            }
            fn spawn_position_ok(&self, _: BlockPos) -> bool {
                true
            }
            fn snow_on_air(&self, _: BlockPos) -> bool {
                false
            }
            fn players(&self) -> Vec<PlayerStatus> {
                Vec::new()
            }
            fn raider(&self, _: EntityId) -> Option<RaiderStatus> {
                None
            }
            fn detach_raider(&mut self, _: EntityId) {}
            fn set_ticks_outside_raid(&mut self, _: EntityId, _: i32) {}
            fn create_raider(&mut self, _: EntityKind) -> Option<NewRaider> {
                None
            }
            fn join_raider(&mut self, _: EntityId, _: JoinInfo) {}
            fn apply_hero_of_the_village(&mut self, _: Uuid, _: i32) -> Option<PlayerId> {
                None
            }
        }
        let target = SectionPos::of(BlockPos::new(20, 64, 0)).center();
        let world = Villages(target);
        let mut raid = raid(Difficulty::Normal);
        raid.move_center_to_nearby_village_section(&world);
        assert_eq!(raid.center(), target);
    }

    fn omen<'a>(pos: BlockPos, pois: &'a [BlockPos], amplifier: Option<i32>) -> Omen<'a> {
        Omen {
            pos,
            village_pois: pois,
            difficulty: Difficulty::Normal,
            bar_id: Uuid::from_u128(5),
            seed: 3,
            amplifier,
        }
    }

    #[test]
    fn a_bad_omen_creates_a_raid_centred_on_the_average_of_the_occupied_pois() {
        let mut raids = Raids::new();
        let pois = [BlockPos::new(10, 64, 0), BlockPos::new(30, 66, 20)];
        let result =
            raids.create_or_extend(&omen(BlockPos::new(0, 64, 0), &pois, Some(0)), |_| true);
        let Trigger::Triggered { raid, first_wave } = result else {
            panic!("{result:?}");
        };
        assert_eq!(raid, 2);
        assert!(first_wave);
        let created = raids.get(2).unwrap();
        assert_eq!(created.center(), BlockPos::new(20, 65, 10));
        assert_eq!(created.raid_omen_level(), 1);
        assert_eq!(created.num_groups(), 5);
        assert_eq!(raids.next_id(), 2);
    }

    #[test]
    fn a_second_omen_in_the_same_village_raises_the_level_up_to_five() {
        let mut raids = Raids::new();
        let at = BlockPos::new(0, 64, 0);
        for expected in [2, 4, 5, 5] {
            raids.create_or_extend(&omen(at, &[], Some(1)), |_| true);
            assert_eq!(raids.get(2).unwrap().raid_omen_level(), expected);
        }
        assert_eq!(raids.ids(), vec![2]);
    }

    #[test]
    fn a_started_raid_at_the_maximum_level_is_left_unchanged() {
        let mut raids = Raids::new();
        let at = BlockPos::new(0, 64, 0);
        raids.create_or_extend(&omen(at, &[], Some(4)), |_| true);
        raids.get_mut(2).unwrap().started = true;
        let result = raids.create_or_extend(&omen(at, &[], Some(1)), |_| panic!("asked"));
        assert_eq!(result, Trigger::Unchanged { raid: 2 });
    }

    #[test]
    fn a_refused_trigger_leaves_no_raid_behind() {
        let mut raids = Raids::new();
        let result =
            raids.create_or_extend(&omen(BlockPos::new(0, 64, 0), &[], Some(0)), |_| false);
        assert_eq!(result, Trigger::Refused);
        assert!(raids.is_empty());
        assert_eq!(raids.next_id(), 1);
    }

    #[test]
    fn the_raid_at_a_position_is_the_nearest_active_raid_within_ninety_six_blocks() {
        let mut raids = Raids::new();
        raids.create_or_extend(&omen(BlockPos::new(0, 64, 0), &[], Some(0)), |_| true);
        raids.create_or_extend(&omen(BlockPos::new(200, 64, 0), &[], Some(0)), |_| true);
        assert_eq!(raids.raid_at(BlockPos::new(10, 64, 0)), Some(2));
        assert_eq!(raids.raid_at(BlockPos::new(190, 64, 0)), Some(3));
        assert_eq!(raids.raid_at(BlockPos::new(100, 64, 0)), None);
        raids.get_mut(2).unwrap().active = false;
        assert_eq!(raids.raid_at(BlockPos::new(10, 64, 0)), None);
    }

    #[test]
    fn disabling_raids_stops_them_and_removes_them_on_the_next_tick() {
        let mut raids = Raids::new();
        raids.create_or_extend(&omen(BlockPos::new(0, 64, 0), &[], Some(0)), |_| true);
        let mut world = Mock::new();
        raids.tick(&mut world, false);
        assert!(raids.get(2).is_some() || raids.is_empty());
        raids.tick(&mut world, false);
        assert!(raids.is_empty());
    }

    #[test]
    fn the_manager_ticks_every_raid_and_reports_the_outputs() {
        let mut raids = Raids::new();
        raids.create_or_extend(&omen(BlockPos::new(0, 64, 0), &[], Some(0)), |_| true);
        let mut world = Mock::new();
        let mut events = Vec::new();
        for _ in 0..301 {
            for output in raids.tick(&mut world, true) {
                events.extend(output.events);
            }
        }
        assert!(
            events
                .iter()
                .any(|event| matches!(event, RaidEvent::Horn(_)))
        );
        assert_eq!(raids.tick_count(), 301);
        assert!(raids.take_dirty());
    }

    #[test]
    fn raids_round_trip_through_the_vanilla_saved_data_layout() {
        let mut raids = Raids::new();
        let mut world = Mock::new();
        raids.create_or_extend(&omen(BlockPos::new(5, 64, 5), &[], Some(2)), |_| true);
        for _ in 0..301 {
            raids.tick(&mut world, true);
        }
        raids
            .get_mut(2)
            .unwrap()
            .add_hero_of_the_village(Uuid::from_u128(0xAABB));
        let nbt = raids.to_nbt();
        let list = nbt.get_list("raids").unwrap();
        let entry = list[0].as_compound().unwrap();
        for key in [
            "id",
            "started",
            "active",
            "ticks_active",
            "raid_omen_level",
            "groups_spawned",
            "cooldown_ticks",
            "post_raid_ticks",
            "total_health",
            "group_count",
            "status",
            "center",
            "heroes_of_the_village",
        ] {
            assert!(entry.contains_key(key), "{key}");
        }
        assert_eq!(entry.get_str("status"), Some("ongoing"));
        assert_eq!(entry.get_int_array("center"), Some(&[5, 64, 5][..]));
        assert_eq!(nbt.get_i32("next_id"), Some(2));
        assert_eq!(nbt.get_i32("tick"), Some(301));
        let loaded = Raids::from_nbt(&nbt, || Uuid::from_u128(9), 1);
        let original = raids.get(2).unwrap();
        let read = loaded.get(2).unwrap();
        assert_eq!(read.center(), original.center());
        assert_eq!(read.raid_omen_level(), original.raid_omen_level());
        assert_eq!(read.groups_spawned(), 1);
        assert_eq!(read.total_health(), original.total_health());
        assert_eq!(
            read.heroes().collect::<Vec<_>>(),
            vec![Uuid::from_u128(0xAABB)]
        );
        assert_eq!(read.status(), RaidStatus::Ongoing);
        assert!(read.is_started());
        assert_eq!(loaded.next_id(), 2);
        assert_eq!(loaded.tick_count(), 301);
        assert_eq!(read.total_raiders_alive(), 0);
    }

    #[test]
    fn corrupt_raid_entries_are_skipped_on_load() {
        let nbt = Compound::new().with(
            "raids",
            Tag::List(vec![Tag::Compound(Compound::new().with("id", 4))]),
        );
        let loaded = Raids::from_nbt(&nbt, Uuid::nil, 0);
        assert!(loaded.is_empty());
        assert_eq!(loaded.next_id(), 1);
    }

    #[test]
    fn the_status_names_round_trip() {
        for status in [
            RaidStatus::Ongoing,
            RaidStatus::Victory,
            RaidStatus::Loss,
            RaidStatus::Stopped,
        ] {
            assert_eq!(RaidStatus::parse(status.name()), Some(status));
        }
        assert_eq!(RaidStatus::parse("nope"), None);
    }

    #[test]
    fn joining_requires_an_alive_willing_raider_not_idle_for_too_long() {
        assert!(Raids::can_join_raid(true, true, 2_400));
        assert!(!Raids::can_join_raid(true, true, 2_401));
        assert!(!Raids::can_join_raid(false, true, 0));
        assert!(!Raids::can_join_raid(true, false, 0));
    }

    #[test]
    fn evokers_that_cannot_be_created_are_skipped_without_blocking_the_wave() {
        let mut world = Mock::new();
        let mut raid = raid(Difficulty::Hard);
        for _ in 0..6 {
            run(&mut raid, &mut world, 301);
            kill_all(&mut raid, &mut world);
            run(&mut raid, &mut world, 1);
        }
        assert!(raid.groups_spawned() >= 6);
        assert!(!world.created.contains(&EntityKind::Evoker));
        assert!(world.created.contains(&EntityKind::Ravager));
        assert!(world.created.contains(&EntityKind::Witch));
    }
}
