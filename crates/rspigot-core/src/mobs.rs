use rand::Rng;
use rspigot_ai::{
    AiWorld, BlockView as AiView, Facts, GoalSettings, Hand, LastDamage, Living, MemoryModuleType,
    MobAi, MobBody, MobView, TargetKind,
};
use rspigot_domain::damage::{DamageTag, DamageType};
use rspigot_domain::entity::ageable::{self, AgeableState};
use rspigot_domain::entity::fire;
use rspigot_domain::entity::living::{
    self, ATTACK_DAMAGE, ATTACK_KNOCKBACK, EQUIPMENT_SLOTS, HurtOutcome, HurtSource,
};
use rspigot_domain::entity::neutral;
use rspigot_domain::entity::physics::{Landing, water_height};
use rspigot_domain::entity::reward::EquipmentDrop;
use rspigot_domain::entity::riding::Rides;
use rspigot_domain::entity::species::{self, Species};
use rspigot_domain::entity::tamable;
use rspigot_domain::entity::{
    Aabb, ActivationType, Entities, Entity, EntityState, FLAG_INVISIBLE, RemovalReason,
    activation_type, reward,
};
use rspigot_domain::entity::{EntityBase, LivingState};
use rspigot_domain::item::{HOTBAR_START, ItemStack};
use rspigot_domain::mth;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::poi::{Occupancy, PoiRecord};
use rspigot_domain::random::LegacyRandom;
use rspigot_domain::text::Component;
use rspigot_domain::world::Difficulty;
use rspigot_domain::world::WorldHeight;
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, EntityId, SectionPos, Vec3};
use rspigot_entities::{
    CreatureSound, CreatureSounds, EntityKind as Kind, SoundState, WaterContact,
};
use rspigot_items::EquipmentSlot;
use rspigot_items::ItemKind;
use rustc_hash::FxHashMap;
use uuid::Uuid;

use crate::chunks::ChunkManager;
use crate::commands::Target;
use crate::config::{ActivationRanges, GameRules, LevelData};
use crate::events::{ClientEvent, EntityStatus};
use crate::fall::fall_impact;
use crate::health::{DamageSource, SourceEntity};
use crate::lighting::light_level;
use crate::loot::{self, LootContext, LootDamage, LootEntity, RaiderFacts};
use crate::player::Player;
use crate::plugins::events::EntityRemoveCause;
use crate::plugins::events::{EntityDeath, EntityDeathPatch, EntityKnockbackCause};
use crate::plugins::steps::LootStep;
use crate::plugins::{
    AfterHurt, Continuation, EventKind, EventPayload, Holder, MobAttackFollow, Struck, spec_of,
    stack_of,
};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::spawning::LEASH;

const OFFHAND_SLOT: usize = 45;
const ARMOR_SLOTS: std::ops::Range<usize> = 5..9;
const HEAD_SLOT: usize = 5;
const GAZE_DISGUISE: rspigot_ai::TemptItems =
    rspigot_ai::TemptItems::Tag("minecraft:gaze_disguise_equipment");
const PIGLIN_SAFE_ARMOR: rspigot_ai::TemptItems =
    rspigot_ai::TemptItems::Tag("minecraft:piglin_safe_armor");
const NO_DESPAWN_DISTANCE: f64 = 32.0;
const ACTIVATION_HEIGHT: f64 = 256.0;
const IMMUNITY_RECHECK: i64 = 20;
const INACTIVE_SKIP: i32 = 4;
const WORLD_BORDER: i32 = 30_000_000;
const DAY_LENGTH: i64 = 24_000;
const SKY_LIGHT_KEYS: [(i64, f32); 4] = [
    (133, 1.0),
    (11_867, 1.0),
    (13_670, 0.266_666_68),
    (22_330, 0.266_666_68),
];
const MAX_LIGHT: f32 = 15.0;
const STORM_SKY_LIGHT: f32 = 4.0;
const RAIN_SKY_ALPHA: f32 = 0.3125;
const THUNDER_SKY_ALPHA: f32 = 0.527_343_75;
const BRIGHT_SKY_DARKEN: i32 = 4;
const BRIGHT_LIMIT: f32 = 0.5;
pub(crate) const DAMAGE_SOURCE_MEMORY: i64 = 40;
const RETALIATION_MEMORY: i64 = 100;
const NO_WOLF_RETALIATION: &str = "minecraft:no_wolf_retaliation";
const PANIC_CAUSES: &str = "minecraft:panic_causes";
const PANIC_ENVIRONMENTAL_CAUSES: &str = "minecraft:panic_environmental_causes";
pub const DEFAULT_KNOCKBACK: f64 = 0.400_000_005_960_464_5;
pub const MIN_KNOCKBACK_DIRECTION: f64 = 9.999_999_747_378_752e-6;
const KNOCKBACK_VERTICAL_CAP: f64 = 0.4;
const VOID_DAMAGE: f32 = 4.0;
const VOID_DEPTH: i32 = 64;
const ITEM_PICKUP_DELAY: u16 = 10;
const ITEM_SPREAD: f64 = 0.2;
const ITEM_LIFT: f64 = 0.2;
const ATTACKER_SLOWDOWN: f64 = 0.6;
const BIG_FALL: i32 = 4;
const DYES: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

#[derive(Debug)]
pub(crate) struct MobEntry {
    pub ai: MobAi,
    activated_tick: i64,
    default_active: bool,
    activation: ActivationType,
    pub ambient_sound_time: i32,
}

#[derive(Debug, Default)]
pub(crate) struct MobTable {
    pub entries: FxHashMap<EntityId, MobEntry>,
    pub players: Vec<Living>,
    pub spawn_randoms: FxHashMap<EntityId, LegacyRandom>,
    pub initial_targets: FxHashMap<EntityId, EntityId>,
    pub open_chests: Vec<(EntityId, crate::containers::Backing)>,
    pub golem_chests: FxHashMap<BlockPos, crate::containers::GolemOpening>,
    pub charging_sounds: crate::crossbow::ChargingSoundFlags,
}

fn range_of(activation: ActivationType, ranges: &ActivationRanges) -> i32 {
    match activation {
        ActivationType::Monster => ranges.monsters,
        ActivationType::Animal => ranges.animals,
        ActivationType::Raider => ranges.raiders,
        ActivationType::Misc => ranges.misc,
    }
}

fn default_active(kind: Kind, activation: ActivationType, ranges: &ActivationRanges) -> bool {
    range_of(activation, ranges) == 0 || matches!(kind, Kind::Wither | Kind::EnderDragon)
}

fn inflate(area: &Aabb, horizontal: f64, vertical: f64) -> Aabb {
    rspigot_ai::inflate(area, horizontal, vertical, horizontal)
}

fn immune(entity: &Entity, target: Option<EntityId>) -> bool {
    let base = &entity.base;
    if base.fluid.in_water || base.remaining_fire_ticks > 0 || !base.on_ground {
        return true;
    }
    let Some(state) = entity.living() else {
        return false;
    };
    if state.hurt_time > 0 || !state.effects.is_empty() {
        return true;
    }
    let kind = base.kind;
    if kind.extends("PathfinderMob") && target.is_some() {
        return true;
    }
    if kind.extends("Animal") {
        let in_love = state
            .mob
            .as_ref()
            .and_then(|mob| mob.ageable.as_ref())
            .is_some_and(|ageable| ageable.in_love());
        if entity.is_baby() || in_love {
            return true;
        }
        if kind == Kind::Sheep && species::is_sheared(base) {
            return true;
        }
    }
    kind == Kind::Creeper && species::is_ignited(base)
}

fn armor_cover(filled: usize) -> f32 {
    filled as f32 / ARMOR_SLOTS.len() as f32
}

fn player_target<L>(player: &Player<L>, game_time: i64) -> Living {
    let abilities = player.abilities();
    let held = player
        .inventory
        .get(HOTBAR_START + usize::from(player.inventory.selected_hotbar))
        .map(|stack| stack.item);
    let armor = ARMOR_SLOTS
        .filter(|slot| player.inventory.get(*slot).is_some())
        .count();
    Living {
        id: player.entity_id,
        player: true,
        kind: Kind::Player,
        position: player.position,
        eye_height: player.eye_height(),
        bounds: Aabb::of_size(player.position, 0.6, player.height() as f32),
        alive: !player.vitals.removed && !player.vitals.dead && player.vitals.health > 0.0,
        spectator: player.game_mode == GameMode::Spectator,
        creative: player.game_mode == GameMode::Creative,
        invulnerable: abilities.invulnerable,
        discrete: player.sneaking,
        invisible: player.effects.has("minecraft:invisibility"),
        armor_cover: armor_cover(armor),
        main_hand: held,
        off_hand: player.inventory.get(OFFHAND_SLOT).map(|stack| stack.item),
        yaw: player.rotation.yaw,
        pitch: player.rotation.pitch,
        in_water: false,
        baby: false,
        age: 0,
        in_love: false,
        panicking: false,
        has_target: false,
        scared: false,
        facts: Facts {
            head_yaw: player.rotation.yaw,
            piglin_safe_armor: ARMOR_SLOTS
                .filter(|slot| {
                    player
                        .inventory
                        .get(*slot)
                        .is_some_and(|stack| PIGLIN_SAFE_ARMOR.matches(Some(stack.item)))
                })
                .count()
                > 0,
            cannot_be_hunted: false,
            sees_player_holding_loved_item: false,
            hunted_recently: false,
            sprinting: player.sprinting,
            passenger: false,
            hurt_by: false,
            uuid: player.profile.id.0,
            tamed: false,
            owner: None,
            sleeping: false,
            sleep_timer: 0,
            cat_relaxing: false,
            last_hurt_by_mob: player.vitals.last_hurt_by_mob,
            last_hurt_by_mob_at: player.vitals.last_hurt_by_mob_at,
            last_hurt_mob: player.vitals.last_hurt_mob,
            last_hurt_mob_at: player.vitals.last_hurt_mob_at,
            sitting: false,
            llama_strength: 0,
            retaliable: player
                .vitals
                .last_source
                .is_some_and(|source| retaliable(source, game_time)),
            tick_count: player.vitals.tick_count as i32,
            gaze_disguise: player
                .inventory
                .get(HEAD_SLOT)
                .is_some_and(|stack| GAZE_DISGUISE.matches(Some(stack.item))),
            villager: rspigot_ai::VillagerPeerFacts::default(),
            hero_of_the_village: player.effects.has("minecraft:hero_of_the_village"),
            cube_size: 0,
            sniffer_state: 0,
            school_leader: None,
            school_size: 0,
            swimming: false,
            wool_color: 0,
        },
    }
}

fn retaliable(source: (DamageType, i64), game_time: i64) -> bool {
    let (kind, at) = source;
    game_time - at <= RETALIATION_MEMORY && !kind.in_tag(NO_WOLF_RETALIATION)
}

fn entity_facts(
    entity: &Entity,
    state: &LivingState,
    game_time: i64,
    resolve: &impl Fn(Uuid) -> Option<EntityId>,
) -> Facts {
    let base = &entity.base;
    let tame =
        state.mob.as_ref().is_some_and(|mob| mob.tamable.is_some()) && tamable::is_tame(base);
    Facts {
        uuid: base.uuid,
        head_yaw: base.head_yaw,
        tamed: tame,
        owner: tame
            .then(|| tamable::owner(base))
            .flatten()
            .and_then(resolve),
        sleeping: matches!(
            base.data.get(rspigot_domain::entity::DATA_POSE),
            Some(rspigot_domain::entity::SyncedValue::Pose(
                rspigot_domain::entity::Pose::Sleeping
            ))
        ) || (base.kind == Kind::Fox && species::fox::is_sleeping(base)),
        sleep_timer: 0,
        cat_relaxing: entity.base.kind == Kind::Cat
            && (rspigot_domain::entity::species::cat::is_lying(base)
                || rspigot_domain::entity::species::cat::is_relax_state_one(base)),
        last_hurt_by_mob: state.damage.last_hurt_by_mob,
        last_hurt_by_mob_at: state.damage.last_hurt_by_mob_at,
        last_hurt_mob: state.damage.last_hurt_mob,
        last_hurt_mob_at: state.damage.last_hurt_mob_at,
        sitting: tame && tamable::is_in_sitting_pose(base),
        llama_strength: if base.kind.extends("Llama") {
            entity.extra.get_i32("Strength").unwrap_or(0)
        } else {
            0
        },
        retaliable: state
            .damage
            .last_source
            .is_some_and(|source| retaliable(source, game_time)),
        tick_count: base.tick_count,
        gaze_disguise: false,
        piglin_safe_armor: [
            EquipmentSlot::Feet,
            EquipmentSlot::Legs,
            EquipmentSlot::Chest,
            EquipmentSlot::Head,
        ]
        .into_iter()
        .any(|slot| {
            state
                .equipment(slot)
                .is_some_and(|stack| PIGLIN_SAFE_ARMOR.matches(Some(stack.item)))
        }),
        cannot_be_hunted: matches!(
            state.mob.as_ref().map(|mob| mob.species),
            Some(Species::Hoglin(hoglin)) if hoglin.cannot_be_hunted
        ),
        sees_player_holding_loved_item: false,
        hunted_recently: false,
        sprinting: false,
        passenger: false,
        hurt_by: false,
        villager: if base.kind == Kind::Villager {
            crate::kinds::villager::villager_facts(entity).peer()
        } else {
            rspigot_ai::VillagerPeerFacts::default()
        },
        hero_of_the_village: false,
        sniffer_state: if base.kind == Kind::Sniffer {
            u8::try_from(species::sniffer::state(base).id()).unwrap_or(0)
        } else {
            0
        },
        school_leader: crate::kinds::fish_facts_of(entity).0,
        school_size: crate::kinds::fish_facts_of(entity).1,
        swimming: false,
        wool_color: if base.kind == Kind::Sheep {
            species::wool_color(base)
        } else {
            0
        },
        cube_size: if base.kind.extends("AbstractCubeMob") {
            u8::try_from(rspigot_domain::entity::reward::cube_size(entity).clamp(0, 255))
                .unwrap_or(u8::MAX)
        } else {
            0
        },
    }
}

fn entity_target(
    entity: &Entity,
    game_time: i64,
    resolve: &impl Fn(Uuid) -> Option<EntityId>,
) -> Option<Living> {
    let state = entity.living()?;
    let base = &entity.base;
    let dimensions = base.kind.dimensions();
    let armor = EQUIPMENT_SLOTS
        .iter()
        .filter(|slot| {
            matches!(
                slot,
                EquipmentSlot::Feet
                    | EquipmentSlot::Legs
                    | EquipmentSlot::Chest
                    | EquipmentSlot::Head
            )
        })
        .filter(|slot| state.equipment(**slot).is_some())
        .count();
    Some(Living {
        id: base.id,
        player: false,
        kind: base.kind,
        position: base.position,
        eye_height: f64::from(dimensions.eye_height),
        bounds: base.bounding_box(),
        alive: !base.is_removed() && !state.dead && living::health(base) > 0.0,
        spectator: false,
        creative: false,
        invulnerable: base.invulnerable,
        discrete: false,
        invisible: base
            .data
            .flag(rspigot_domain::entity::DATA_SHARED_FLAGS, FLAG_INVISIBLE),
        armor_cover: armor_cover(armor),
        main_hand: state
            .equipment(EquipmentSlot::Mainhand)
            .map(|stack| stack.item),
        off_hand: state
            .equipment(EquipmentSlot::Offhand)
            .map(|stack| stack.item),
        yaw: base.yaw,
        pitch: base.pitch,
        in_water: base.fluid.in_water,
        baby: entity.is_baby(),
        age: state
            .mob
            .as_ref()
            .and_then(|mob| mob.ageable.as_ref())
            .map_or(0, |ageable| ageable.age),
        in_love: state
            .mob
            .as_ref()
            .and_then(|mob| mob.ageable.as_ref())
            .is_some_and(AgeableState::in_love),
        panicking: false,
        has_target: false,
        scared: false,
        facts: entity_facts(entity, state, game_time, resolve),
    })
}

fn with_mob_facts(
    mut living: Living,
    entries: &FxHashMap<EntityId, MobEntry>,
    rides: &Rides,
) -> Living {
    living.facts.passenger = rides.is_passenger(living.id);
    if let Some(entry) = entries.get(&living.id) {
        living.panicking = entry.ai.is_panicking();
        living.has_target = entry.ai.target().is_some();
        if let Some(brain) = entry.ai.behavior_brain() {
            let memories = brain.memories();
            living.facts.hunted_recently = memories.has_value(MemoryModuleType::HuntedRecently);
            living.facts.hurt_by = memories.has_value(MemoryModuleType::HurtBy);
            living.facts.sees_player_holding_loved_item =
                memories.has_value(MemoryModuleType::NearestPlayerHoldingWantedItem);
            if living.kind == Kind::Villager {
                crate::kinds::villager::with_peer_memory_facts(
                    &mut living.facts.villager,
                    memories,
                );
            }
        }
    }
    living
}

fn lerp(delta: f32, start: f32, end: f32) -> f32 {
    start + delta * (end - start)
}

pub(crate) fn sky_darken(level: &LevelData) -> i32 {
    let mut light = MAX_LIGHT * sky_light_factor(level.day_time);
    let rain = if level.raining { 1.0 } else { 0.0 };
    let thunder = if level.raining && level.thundering {
        1.0
    } else {
        0.0
    };
    let rain_only = rain - thunder;
    if rain_only > 0.0 {
        light = lerp(
            rain_only,
            light,
            lerp(RAIN_SKY_ALPHA, light, STORM_SKY_LIGHT),
        );
    }
    if thunder > 0.0 {
        light = lerp(
            thunder,
            light,
            lerp(THUNDER_SKY_ALPHA, light, STORM_SKY_LIGHT),
        );
    }
    (MAX_LIGHT - light) as i32
}

fn sky_light_factor(day_time: i64) -> f32 {
    let time = day_time.rem_euclid(DAY_LENGTH);
    let count = SKY_LIGHT_KEYS.len();
    let mut factor = SKY_LIGHT_KEYS[0].1;
    for index in 0..count {
        let (start, from) = SKY_LIGHT_KEYS[index];
        let (end, to) = SKY_LIGHT_KEYS[(index + 1) % count];
        let end = if index + 1 == count {
            end + DAY_LENGTH
        } else {
            end
        };
        let at = if time < start {
            time + DAY_LENGTH
        } else {
            time
        };
        if at >= start && at <= end {
            let alpha = (at - start) as f32 / (end - start) as f32;
            factor = from + alpha * (to - from);
            break;
        }
    }
    factor
}

pub(crate) struct ChunkView<'a> {
    pub chunks: &'a ChunkManager,
    pub height: WorldHeight,
}

impl ChunkView<'_> {
    pub(crate) fn sky_light(&self, pos: BlockPos) -> i32 {
        let Some(chunk) = self.chunks.get(pos.chunk()) else {
            return 0;
        };
        if !self.height.contains(pos.y) && pos.y >= self.height.max_y() {
            return MAX_LIGHT as i32;
        }
        let sections = chunk.light.sky.len().max(1);
        let index =
            ((pos.y >> 4) - self.height.min_section() + 1).clamp(0, sections as i32 - 1) as usize;
        chunk
            .light
            .sky
            .get(index)
            .and_then(Option::as_ref)
            .map_or(0, |layer| {
                i32::from(layer.get(
                    (pos.x & 15) as usize,
                    (pos.y & 15) as usize,
                    (pos.z & 15) as usize,
                ))
            })
    }
}

impl AiView for ChunkView<'_> {
    fn state(&self, pos: BlockPos) -> Option<BlockStateId> {
        self.chunks.block(pos)
    }

    fn can_see_sky(&self, pos: BlockPos) -> bool {
        self.sky_light(pos) >= MAX_LIGHT as i32
    }

    fn min_y(&self) -> i32 {
        self.height.min_y
    }

    fn max_y(&self) -> i32 {
        self.height.max_y()
    }

    fn chunk_loaded(&self, chunk: ChunkPos) -> bool {
        self.chunks.is_loaded(chunk)
    }
}

pub(crate) struct MobWorld<'a> {
    view: ChunkView<'a>,
    store: &'a Entities,
    entries: &'a FxHashMap<EntityId, MobEntry>,
    players: &'a [Living],
    open_chests: &'a [(EntityId, crate::containers::Backing)],
    difficulty: i32,
    dimension: &'a str,
    game_time: i64,
    day_time: i64,
    sky_darken: i32,
    rules: GameRules,
    has_day_cycle: bool,
    thundering: bool,
    raining: bool,
    ambient_light: f32,
    sea_level: i32,
    raids: &'a rspigot_domain::raid::Raids,
    rides: &'a Rides,
}

impl MobWorld<'_> {
    fn half_items(&self, pos: BlockPos) -> Vec<rspigot_domain::item::Slot> {
        let size = crate::containers::CHEST_HALF_SLOTS;
        self.view
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .map_or_else(
                || vec![None; size],
                |entity| crate::containers::read_items(&entity.data, size),
            )
    }

    fn chest_view(&self, pos: BlockPos, ignore_blocked: bool) -> Option<rspigot_ai::ContainerView> {
        let state = self.view.state(pos)?;
        if !state.kind().is_some_and(crate::chests::is_pairable) {
            return None;
        }
        let blocked = |at: BlockPos| !ignore_blocked && rspigot_ai::is_chest_blocked_at(self, at);
        if blocked(pos) {
            return None;
        }
        match crate::chests::pair_of(pos, state, |at| self.view.state(at)) {
            Some((right, left)) => {
                let partner = if pos == right { left } else { right };
                if blocked(partner) {
                    return None;
                }
                let mut slots = self.half_items(right);
                slots.extend(self.half_items(left));
                Some(rspigot_ai::ContainerView {
                    slots,
                    compound: true,
                })
            }
            None => Some(rspigot_ai::ContainerView {
                slots: self.half_items(pos),
                compound: false,
            }),
        }
    }

    fn golem_openers(&self, pos: BlockPos) -> Vec<EntityId> {
        self.store
            .iter()
            .filter(|entity| entity.base.kind == Kind::CopperGolem)
            .filter_map(|entity| {
                let Some(Species::CopperGolem(golem)) = entity
                    .living()
                    .and_then(|living| living.mob.as_ref())
                    .map(|mob| mob.species)
                else {
                    return None;
                };
                let opened = golem.opened_chest?;
                let connected = self.view.state(opened).is_some_and(|state| {
                    rspigot_ai::chest_side(state) != rspigot_ai::ChestSide::Single
                        && rspigot_ai::connected_pos(opened, state) == pos
                });
                (opened == pos || connected).then_some(entity.base.id)
            })
            .collect()
    }

    pub(crate) fn resolve(&self, uuid: Uuid) -> Option<EntityId> {
        self.players
            .iter()
            .find(|player| player.facts.uuid == uuid)
            .map(|player| player.id)
            .or_else(|| self.store.by_uuid(uuid))
    }
}

impl AiView for MobWorld<'_> {
    fn state(&self, pos: BlockPos) -> Option<BlockStateId> {
        self.view.state(pos)
    }

    fn min_y(&self) -> i32 {
        self.view.min_y()
    }

    fn max_y(&self) -> i32 {
        self.view.max_y()
    }

    fn chunk_loaded(&self, chunk: ChunkPos) -> bool {
        self.view.chunk_loaded(chunk)
    }

    fn can_see_sky(&self, pos: BlockPos) -> bool {
        self.view.can_see_sky(pos)
    }
}

impl AiWorld for MobWorld<'_> {
    fn players(&self) -> &[Living] {
        self.players
    }

    fn living(&self, id: EntityId) -> Option<Living> {
        if let Some(player) = self.players.iter().find(|player| player.id == id) {
            return Some(*player);
        }
        let resolve = |uuid| self.resolve(uuid);
        self.store
            .get(id)
            .and_then(|entity| entity_target(entity, self.game_time, &resolve))
            .map(|living| with_mob_facts(living, self.entries, self.rides))
    }

    fn living_of(&self, kind: TargetKind, area: &Aabb) -> Vec<Living> {
        if kind == TargetKind::Player {
            return self
                .players
                .iter()
                .filter(|player| player.bounds.intersects(area))
                .copied()
                .collect();
        }
        let resolve = |uuid| self.resolve(uuid);
        let mut found: Vec<Living> = self
            .store
            .iter()
            .filter(|entity| kind.matches(entity.base.kind))
            .filter(|entity| entity.base.bounding_box().intersects(area))
            .filter_map(|entity| entity_target(entity, self.game_time, &resolve))
            .map(|living| with_mob_facts(living, self.entries, self.rides))
            .collect();
        if kind.matches(Kind::Player) {
            found.extend(
                self.players
                    .iter()
                    .filter(|player| player.bounds.intersects(area))
                    .copied(),
            );
        }
        found
    }

    fn chest_open_count(&self, pos: BlockPos) -> i32 {
        let players = self
            .open_chests
            .iter()
            .filter(|(_, backing)| backing.positions().any(|candidate| candidate == pos))
            .count();
        (players + self.golem_openers(pos).len()) as i32
    }

    fn chests_in_chunk(&self, chunk: ChunkPos) -> Option<Vec<BlockPos>> {
        let loaded = self.view.chunks.get(chunk)?;
        let mut chests: Vec<BlockPos> = loaded
            .block_entities
            .iter()
            .filter(|entity| matches!(entity.kind.path(), "chest" | "trapped_chest"))
            .map(|entity| entity.pos)
            .collect();
        chests.sort_unstable_by_key(|pos| (pos.x, pos.y, pos.z));
        Some(chests)
    }

    fn chest_container(
        &self,
        pos: BlockPos,
        ignore_blocked: bool,
    ) -> Option<rspigot_ai::ContainerView> {
        self.chest_view(pos, ignore_blocked)
    }

    fn container_locked(&self, pos: BlockPos) -> bool {
        self.view
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .is_some_and(|entity| entity.data.contains_key(crate::containers::LOCK))
    }

    fn container_openers(&self, pos: BlockPos) -> Vec<EntityId> {
        let mut openers: Vec<EntityId> = self
            .open_chests
            .iter()
            .filter(|(_, backing)| backing.positions().any(|candidate| candidate == pos))
            .map(|(id, _)| *id)
            .collect();
        openers.extend(self.golem_openers(pos));
        openers
    }

    fn main_hand_stack(&self, id: EntityId) -> Option<ItemStack> {
        self.store
            .get(id)?
            .living()?
            .equipment(EquipmentSlot::Mainhand)
            .cloned()
    }

    fn any_entity_in(&self, area: &Aabb, except: EntityId) -> bool {
        self.players
            .iter()
            .any(|player| !player.spectator && player.bounds.intersects(area))
            || self.store.iter().any(|entity| {
                entity.base.id != except
                    && !entity.base.is_removed()
                    && entity.base.bounding_box().intersects(area)
            })
    }

    fn raw_brightness_undarkened(&self, pos: BlockPos) -> i32 {
        if pos.x < -WORLD_BORDER
            || pos.z < -WORLD_BORDER
            || pos.x >= WORLD_BORDER
            || pos.z >= WORLD_BORDER
        {
            return MAX_LIGHT as i32;
        }
        let Some(chunk) = self.view.chunks.get(pos.chunk()) else {
            return 0;
        };
        light_level(chunk, pos, true).max(light_level(chunk, pos, false))
    }

    fn aggressive_panda(&self, id: EntityId) -> bool {
        self.store.get(id).is_some_and(|entity| {
            entity.base.kind == Kind::Panda && species::panda::is_aggressive(&entity.base)
        })
    }

    fn ambient_light(&self) -> f32 {
        self.ambient_light
    }

    fn bright_outside(&self) -> bool {
        self.has_day_cycle && self.sky_darken < BRIGHT_SKY_DARKEN
    }

    fn dark_outside(&self) -> bool {
        self.has_day_cycle && self.sky_darken >= BRIGHT_SKY_DARKEN
    }

    fn villager_reputation(&self, villager: EntityId, player: EntityId) -> i32 {
        let Some(uuid) = self
            .players
            .iter()
            .find(|candidate| candidate.id == player)
            .map(|candidate| candidate.facts.uuid)
        else {
            return 0;
        };
        self.store.get(villager).map_or(0, |entity| {
            crate::kinds::villager::reputation_of(entity, uuid)
        })
    }

    fn poi_type_at(&self, pos: BlockPos) -> Option<rspigot_domain::poi::PoiType> {
        self.view.chunks.poi().type_at(pos)
    }

    fn path_door_nodes(&self, mob: EntityId) -> Option<(BlockPos, BlockPos)> {
        let path = self
            .entries
            .get(&mob)?
            .ai
            .behavior_brain()?
            .memories()
            .get(MemoryModuleType::Path)?
            .as_path()?;
        if path.is_done() {
            return None;
        }
        let previous = path.previous_node()?.pos;
        Some((previous, path.next_node_pos()))
    }

    fn villager_can_reach(&self, villager: EntityId, pos: BlockPos, reach: i32) -> bool {
        let Some(entity) = self.store.get(villager) else {
            return false;
        };
        let Some(state) = entity.living() else {
            return false;
        };
        let Some(entry) = self.entries.get(&villager) else {
            return false;
        };
        let mob = MobView::of(&entity.base, state);
        entry.ai.can_reach_pos(&mob, self, pos, reach)
    }

    fn villager_trades(&self, villager: EntityId) -> Vec<rspigot_ai::TradeView> {
        self.store
            .get(villager)
            .map(crate::kinds::villager::trade_views)
            .unwrap_or_default()
    }

    fn sections_to_village(&self, pos: BlockPos) -> i32 {
        self.view
            .chunks
            .poi()
            .sections_to_village(SectionPos::of(pos))
    }

    fn pois_in_range(&self, center: BlockPos, radius: i32, occupancy: Occupancy) -> Vec<PoiRecord> {
        self.view
            .chunks
            .poi()
            .in_range(|_| true, center, radius, occupancy)
    }

    fn mob_griefing(&self) -> bool {
        self.rules.mob_griefing
    }

    fn raining(&self) -> bool {
        self.raining
    }

    fn bees_stay_in_hive(&self, _pos: BlockPos) -> bool {
        crate::beehive::bees_stay_in_hive(self.raining, self.dimension, self.day_time)
    }

    fn beehive(&self, pos: BlockPos) -> Option<rspigot_ai::HiveView> {
        let state = self.view.chunks.block(pos)?;
        if !crate::beehive::is_hive_block(state) {
            return None;
        }
        let hive = self
            .view
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .map(|entity| rspigot_domain::beehive::Beehive::read(&entity.data))
            .unwrap_or_default();
        let fire_nearby = (-1..=1).any(|dx| {
            (-1..=1).any(|dy| {
                (-1..=1).any(|dz| {
                    self.view
                        .chunks
                        .block(pos.offset(dx, dy, dz))
                        .and_then(BlockStateId::kind)
                        == Some(rspigot_blocks::BlockKind::Fire)
                })
            })
        });
        Some(rspigot_ai::HiveView {
            full: hive.is_full(),
            fire_nearby,
        })
    }

    fn thundering(&self) -> bool {
        self.thundering
    }

    fn items_in(&self, area: &Aabb) -> Vec<rspigot_ai::DroppedItem> {
        self.store
            .iter()
            .filter_map(|entity| {
                let EntityState::Item(state) = &entity.state else {
                    return None;
                };
                let bounds = entity.base.bounding_box();
                bounds.intersects(area).then_some(rspigot_ai::DroppedItem {
                    id: entity.base.id,
                    position: entity.base.position,
                    item: rspigot_domain::entity::item::stack(&entity.base)?.item,
                    bounds,
                    has_pickup_delay: state.pickup_delay > 0,
                    alive: !entity.base.is_removed(),
                })
            })
            .collect()
    }

    fn raid_at(&self, pos: BlockPos) -> Option<rspigot_ai::RaidFacts> {
        self.raids
            .raid_at(pos)
            .and_then(|id| self.raids.get(id))
            .map(crate::raids::facts_of)
    }

    fn is_ominous_banner(&self, id: EntityId) -> bool {
        self.store.get(id).is_some_and(|entity| {
            rspigot_domain::entity::item::stack(&entity.base)
                .is_some_and(crate::raids::is_ominous_banner)
        })
    }

    fn dropped_item(&self, id: EntityId) -> Option<rspigot_ai::DroppedItem> {
        let entity = self.store.get(id)?;
        let EntityState::Item(state) = &entity.state else {
            return None;
        };
        Some(rspigot_ai::DroppedItem {
            id,
            position: entity.base.position,
            item: rspigot_domain::entity::item::stack(&entity.base)?.item,
            bounds: entity.base.bounding_box(),
            has_pickup_delay: state.pickup_delay > 0,
            alive: !entity.base.is_removed(),
        })
    }

    fn dimension(&self) -> &str {
        self.dimension
    }

    fn living_by_uuid(&self, uuid: Uuid) -> Option<Living> {
        self.resolve(uuid).and_then(|id| self.living(id))
    }

    fn forgive_dead_players(&self) -> bool {
        self.rules.forgive_dead_players
    }

    fn within_world_border(&self, pos: BlockPos) -> bool {
        pos.x >= -WORLD_BORDER
            && pos.z >= -WORLD_BORDER
            && pos.x < WORLD_BORDER
            && pos.z < WORLD_BORDER
    }

    fn universal_anger(&self) -> bool {
        self.rules.universal_anger
    }

    fn difficulty(&self) -> i32 {
        self.difficulty
    }

    fn sea_level(&self) -> i32 {
        self.sea_level
    }

    fn game_time(&self) -> i64 {
        self.game_time
    }

    fn clock_time(&self) -> i64 {
        self.day_time
    }

    fn raw_brightness(&self, pos: BlockPos) -> i32 {
        if pos.x < -WORLD_BORDER
            || pos.z < -WORLD_BORDER
            || pos.x >= WORLD_BORDER
            || pos.z >= WORLD_BORDER
        {
            return MAX_LIGHT as i32;
        }
        let Some(chunk) = self.view.chunks.get(pos.chunk()) else {
            return 0;
        };
        (light_level(chunk, pos, true) - self.sky_darken).max(light_level(chunk, pos, false))
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn chunk_view(&self) -> ChunkView<'_> {
        ChunkView {
            chunks: &self.world.chunks,
            height: self.world.dimension.height,
        }
    }

    pub(crate) fn register_mob(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if !entity.living().is_some_and(|state| state.is_mob()) {
            return;
        }
        let kind = entity.base.kind;
        let fox_red = species::fox::variant(&entity.base) == species::fox::Variant::Red;
        let cat_tame = kind == Kind::Cat && tamable::is_tame(&entity.base);
        let activation = activation_type(kind);
        let ranges = self.config.activation;
        let seed = self.rng.random::<i64>();
        let settings = GoalSettings {
            zombie_aggressive_towards_villager: self.config.mobs.zombie_aggressive_towards_villager,
        };
        let mut ai = MobAi::new(kind, seed, settings);
        if let Some(random) = self.entities.mobs.spawn_randoms.remove(&id) {
            ai.brain_mut().random = random;
        }
        if let Some(entity) = self.entities.store.get(id) {
            Self::load_mob_brain(entity, &mut ai);
        }
        let mut restored_target = false;
        if let Some(target) = self.entities.mobs.initial_targets.remove(&id) {
            ai.set_target(Some(target));
            restored_target = true;
            self.kind_target_changed(id, Some(target));
        }
        if kind == Kind::Cat {
            ai.reassess_cat_tame_goals(cat_tame);
        }
        let hard = self.world.difficulty == Difficulty::Hard;
        let game_time = self.world.level.game_time;
        let mut remembered = None;
        if let Some(Entity {
            base,
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(id)
            && let Some(anger) = living.mob.as_mut().and_then(|mob| mob.neutral.as_mut())
        {
            anger.resolve_legacy(game_time);
            neutral::sync(base, anger);
            remembered = anger.target;
        }
        if let Some(uuid) = remembered {
            let found = self
                .players
                .values()
                .find(|player| player.profile.id.0 == uuid)
                .map(|player| player.entity_id)
                .or_else(|| self.entities.store.by_uuid(uuid));
            if let Some(target) = found {
                ai.set_target(Some(target));
                restored_target = true;
                self.kind_target_changed(id, Some(target));
            }
        }
        if let Some(living) = self.entities.store.get_mut(id).and_then(|e| e.living_mut()) {
            let bow = living
                .equipment(EquipmentSlot::Mainhand)
                .is_some_and(|stack| stack.item == ItemKind::Bow.id());
            match living.mob.as_mut().map(|mob| &mut mob.species) {
                Some(Species::Chicken(chicken)) if chicken.egg_time.is_none() => {
                    chicken.egg_time = Some(species::next_egg_time(&mut ai.brain_mut().random));
                }
                Some(Species::Skeleton(_)) => ai.reassess_weapon(bow, hard),
                Some(Species::Fox(fox)) if fox.hunts => {
                    ai.fox_target_goals(fox_red);
                }
                Some(Species::Zombie(zombie))
                    if matches!(
                        kind,
                        Kind::Zombie | Kind::Husk | Kind::ZombieVillager | Kind::ZombifiedPiglin
                    ) =>
                {
                    ai.set_can_break_doors(zombie.can_break_doors);
                }
                _ => {}
            }
        }
        self.kind_registered(id, &mut ai);
        self.entities.mobs.entries.insert(
            id,
            MobEntry {
                ai,
                activated_tick: i64::from(i32::MIN),
                default_active: default_active(kind, activation, &ranges),
                activation,
                ambient_sound_time: 0,
            },
        );
        if restored_target {
            self.kind_acquired_target(id);
        }
    }

    pub(crate) fn refresh_mob_players(&mut self) {
        let game_time = self.world.level.game_time;
        let mut players: Vec<Living> = self
            .players
            .values()
            .filter(|player| !player.vitals.removed)
            .map(|player| {
                let mut target = player_target(player, game_time);
                let in_water = |at: Vec3| {
                    self.world.chunks.block(at.block()).is_some_and(|state| {
                        state.flags().contains(rspigot_blocks::StateFlags::WATER)
                    })
                };
                let eyes = Vec3::new(
                    player.position.x,
                    player.position.y + player.eye_height(),
                    player.position.z,
                );
                target.in_water = in_water(player.position);
                target.facts.swimming = player.sprinting && target.in_water && in_water(eyes);
                target
            })
            .collect();
        players.sort_by_key(|player| player.id);
        self.entities.mobs.players = players;
        self.entities.mobs.open_chests = self
            .players
            .values()
            .filter_map(|player| {
                player
                    .open_menu
                    .as_ref()
                    .map(|open| (player.entity_id, open.backing))
            })
            .filter(|(_, backing)| backing.entity.is_some() || backing.ender)
            .collect();
    }

    pub(crate) fn activate_entities(&mut self) {
        let ranges = self.config.activation;
        let current = self.current_tick() as i64;
        let simulation = i32::from(self.config.simulation_distance) * 16 - 8;
        let max_range = ranges
            .monsters
            .max(ranges.animals)
            .max(ranges.raiders)
            .max(ranges.misc)
            .min(simulation);
        let boxes: Vec<Aabb> = self
            .entities
            .mobs
            .players
            .iter()
            .filter(|player| !(ranges.ignore_spectators && player.spectator))
            .map(|player| player.bounds)
            .collect();
        let store = &self.entities.store;
        for player in boxes {
            let reach = inflate(&player, f64::from(max_range), ACTIVATION_HEIGHT);
            for (id, entry) in &mut self.entities.mobs.entries {
                let Some(entity) = store.get(*id) else {
                    continue;
                };
                let bounds = entity.base.bounding_box();
                if !bounds.intersects(&reach) || current <= entry.activated_tick {
                    continue;
                }
                let range = f64::from(range_of(entry.activation, &ranges));
                if entry.default_active
                    || inflate(&player, range, ACTIVATION_HEIGHT).intersects(&bounds)
                {
                    entry.activated_tick = current;
                }
            }
        }
    }

    pub(crate) fn mob_active(&mut self, id: EntityId) -> bool {
        let current = self.current_tick() as i64;
        let Some(entry) = self.entities.mobs.entries.get_mut(&id) else {
            return true;
        };
        let Some(entity) = self.entities.store.get_mut(id) else {
            return false;
        };
        let position = entity.base.position;
        let near = self
            .entities
            .mobs
            .players
            .iter()
            .filter(|player| !player.spectator)
            .map(|player| player.position.distance_squared(position))
            .reduce(f64::min)
            .is_some_and(|distance| distance < NO_DESPAWN_DISTANCE * NO_DESPAWN_DISTANCE);
        if near && let Some(state) = entity.living_mut() {
            state.no_action_time = 0;
        }
        let target = entry.ai.target();
        let mut active = entry.activated_tick >= current || entry.default_active;
        if !active {
            if (current - entry.activated_tick - 1) % IMMUNITY_RECHECK == 0 {
                if immune(entity, target) {
                    entry.activated_tick = current + IMMUNITY_RECHECK;
                }
                active = true;
            }
        } else if !entry.default_active
            && entity.base.tick_count % INACTIVE_SKIP == 0
            && !immune(entity, target)
        {
            active = false;
        }
        if !active {
            entity.base.tick_count = entity.base.tick_count.wrapping_add(1);
            if let Some(state) = entity.living_mut() {
                state.no_action_time += 1;
            }
        }
        active
    }

    pub(crate) fn mob_world(&self, sky_darken: i32) -> MobWorld<'_> {
        MobWorld {
            view: self.chunk_view(),
            store: &self.entities.store,
            entries: &self.entities.mobs.entries,
            players: &self.entities.mobs.players,
            open_chests: &self.entities.mobs.open_chests,
            difficulty: i32::from(self.world.difficulty.id()),
            dimension: self.world.dimension.key.as_str(),
            game_time: self.world.level.game_time,
            day_time: self.world.level.day_time,
            sky_darken,
            rules: self.config.rules,
            has_day_cycle: self.world.dimension.has_skylight,
            thundering: self.world.level.raining && self.world.level.thundering,
            raining: self.world.level.raining,
            ambient_light: ambient_light_of(&self.world.dimension),
            sea_level: self.world.dimension.sea_level,
            raids: &self.raids.set,
            rides: &self.entities.rides,
        }
    }

    pub(crate) fn light_at_eyes(&self, entity: &Entity) -> f32 {
        let world = self.mob_world(sky_darken(&self.world.level));
        let base = &entity.base;
        if !world.chunk_loaded(base.position.chunk()) {
            return 0.0;
        }
        let eye = base.position.y + f64::from(base.eye_height());
        let pos = Vec3::new(base.position.x, eye, base.position.z).block();
        rspigot_ai::light_magic_value(&world, pos)
    }

    pub(crate) fn mob_view(&self, id: EntityId) -> Option<MobView> {
        let entity = self.entities.store.get(id)?;
        let state = entity.living()?;
        let base = &entity.base;
        let mut view = MobView::of(base, state);
        let mob = state.mob.as_ref()?;
        if let Some(ageable) = &mob.ageable {
            view.age = ageable.age;
            view.in_love = ageable.in_love();
        }
        view.baby = entity.is_baby();
        view.vehicle = self.entities.rides.vehicle_of(id);
        view.passenger = view.vehicle.is_some();
        view.on_fire = fire::is_on_fire(base);
        view.panic = state.damage.last_source.is_some_and(|(kind, at)| {
            self.world.level.game_time - at <= DAMAGE_SOURCE_MEMORY && kind.in_tag(PANIC_CAUSES)
        });
        view.panic_environmental = state.damage.last_source.is_some_and(|(kind, at)| {
            self.world.level.game_time - at <= DAMAGE_SOURCE_MEMORY
                && kind.in_tag(PANIC_ENVIRONMENTAL_CAUSES)
        });
        view.last_damage = state.damage.last_source.and_then(|(kind, at)| {
            (self.world.level.game_time - at <= DAMAGE_SOURCE_MEMORY).then_some(LastDamage {
                kind,
                entity: state.damage.last_source_entity,
            })
        });
        view.using_item = state.using_item.map(|(slot, _)| {
            if slot == EquipmentSlot::Offhand {
                Hand::Off
            } else {
                Hand::Main
            }
        });
        view.ticks_using_item = state.using_item.map_or(0, |(_, ticks)| ticks);
        view.swell_dir = species::swell_dir(base);
        view.light = self.light_at_eyes(entity);
        view.swinging = state.swing.is_some();
        let world = self.mob_world(sky_darken(&self.world.level));
        view.owner = view
            .tamed
            .then(|| tamable::owner(base))
            .flatten()
            .and_then(|uuid| world.resolve(uuid));
        if let Some(neutral) = &mob.neutral {
            view.angry = neutral.is_angry(self.world.level.game_time);
            view.anger_target = neutral.target.and_then(|uuid| world.resolve(uuid));
        }
        view.leashed = entity.extra.contains_key(LEASH);
        self.kind_view(entity, &mut view);
        Some(view)
    }

    pub(crate) fn mob_server_ai_step(&mut self, id: EntityId) {
        if self.kind_is_immobile(id) {
            return;
        }
        let sky_darken = sky_darken(&self.world.level);
        let bright = self.entities.store.get(id).is_some_and(|entity| {
            rspigot_domain::entity::species::raider::idles_extra(
                entity.base.kind,
                entity.base.kind.extends("Monster") && self.light_at_eyes(entity) > BRIGHT_LIMIT,
            )
        });
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        if bright {
            state.no_action_time += 2;
        }
        if living::health(base) <= 0.0 || !living::effective_ai(base, state) {
            return;
        }
        state.no_action_time += 1;
        if !state.mob.as_ref().is_some_and(|mob| mob.aware) {
            return;
        }
        let Some(mob) = self.mob_view(id) else {
            return;
        };
        let Some(mut entry) = self.entities.mobs.entries.remove(&id) else {
            return;
        };
        let mut body = MobBody::of(&mob);
        let deferred = self.defers_targets();
        entry.ai.defer_targets(deferred);
        {
            let world = self.mob_world(sky_darken);
            entry.ai.think(&mob, &world, &mut body);
        }
        let requests = entry.ai.take_target_requests();
        let target_in_water = entry.ai.target().is_some_and(|target| {
            match self.players.values().find(|p| p.entity_id == target) {
                Some(player) => self.feet_in_water(player.id),
                None => self
                    .entities
                    .store
                    .get(target)
                    .is_some_and(|other| other.base.fluid.in_water),
            }
        });
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            apply_body(base, state, &mob, &body);
            state.has_target = entry.ai.target().is_some();
            state.target_in_water = target_in_water;
            if let Some(ageable) = state.mob.as_mut().and_then(|mob| mob.ageable.as_mut()) {
                ageable::custom_server_ai_step(ageable);
            }
        }
        let actions = entry.ai.take_actions();
        self.entities.mobs.entries.insert(id, entry);
        self.mob_target_requests(id, requests, deferred);
        for action in actions {
            self.apply_mob_action(id, action);
        }
        self.kind_custom_ai_step(id);
        let view = ChunkView {
            chunks: &self.world.chunks,
            height: self.world.dimension.height,
        };
        if let (
            Some(entry),
            Some(Entity {
                base,
                state: EntityState::Living(state),
                ..
            }),
        ) = (
            self.entities.mobs.entries.get_mut(&id),
            self.entities.store.get_mut(id),
        ) {
            entry.ai.act(base, state, &view);
        }
        self.cube_after_act(id);
    }

    pub(crate) fn mob_paths_block_changed(
        &mut self,
        pos: BlockPos,
        old: BlockStateId,
        new: BlockStateId,
    ) {
        if old.collision_shape_id() == new.collision_shape_id() {
            return;
        }
        let mut entries = std::mem::take(&mut self.entities.mobs.entries);
        {
            let world = self.mob_world(sky_darken(&self.world.level));
            for (id, entry) in &mut entries {
                let Some(entity) = world.store.get(*id) else {
                    continue;
                };
                let Some(state) = entity.living() else {
                    continue;
                };
                let mob = MobView::of(&entity.base, state);
                entry.ai.block_changed(pos, &mob, &world);
            }
        }
        self.entities.mobs.entries = entries;
    }

    pub(crate) fn mob_can_float(&self, id: EntityId) -> bool {
        self.entities
            .mobs
            .entries
            .get(&id)
            .is_some_and(|entry| entry.ai.can_float())
    }

    pub(crate) fn mob_after_tick(&mut self, id: EntityId) {
        let Some(entry) = self.entities.mobs.entries.get_mut(&id) else {
            return;
        };
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        entry.ai.turn_body(base, state);
        if base.tick_count % rspigot_ai::CONTROL_FLAG_INTERVAL == 0 {
            entry.ai.update_control_flags();
        }
        if base.kind.extends("Spider") {
            let climbing = base.horizontal_collision;
            living::set_climbing(base, climbing);
        }
        self.species_post_tick(id);
    }

    pub fn player_entity_id(&self, player: PlayerId) -> Option<EntityId> {
        self.players.get(&player).map(|player| player.entity_id)
    }

    pub fn mob_target(&self, id: EntityId) -> Option<EntityId> {
        self.entities.mobs.entries.get(&id)?.ai.target()
    }

    pub fn poi(&self) -> &rspigot_domain::poi::PoiManager {
        self.world.chunks.poi()
    }

    pub fn poi_mut(&mut self) -> &mut rspigot_domain::poi::PoiManager {
        self.world.chunks.poi_mut()
    }

    pub fn mob_ai(&self, id: EntityId) -> Option<&MobAi> {
        self.entities.mobs.entries.get(&id).map(|entry| &entry.ai)
    }
}

pub(crate) struct ServerRandom<'a, R: Rng + ?Sized>(pub &'a mut R);

impl<R: Rng + ?Sized> rspigot_domain::random::RandomSource for ServerRandom<'_, R> {
    fn next_float(&mut self) -> f32 {
        self.0.random::<f32>()
    }

    fn next_int(&mut self, bound: i32) -> i32 {
        if bound <= 0 {
            0
        } else {
            self.0.random_range(0..bound)
        }
    }

    fn next_double(&mut self) -> f64 {
        self.0.random::<f64>()
    }

    fn next_boolean(&mut self) -> bool {
        self.0.random::<bool>()
    }

    fn next_gaussian(&mut self) -> f64 {
        rspigot_domain::random::polar_gaussian(&mut || self.0.random::<f64>()).0
    }
}

fn apply_body(base: &mut EntityBase, state: &mut LivingState, mob: &MobView, body: &MobBody) {
    base.velocity = body.velocity;
    if body.velocity != mob.velocity {
        base.needs_sync = true;
    }
    if body.yaw != mob.yaw {
        base.set_yaw(body.yaw);
    }
    if body.body_yaw != mob.body_yaw {
        state.body_yaw = body.body_yaw;
    }
    if body.head_yaw != mob.head_yaw {
        base.head_yaw = body.head_yaw;
    }
    if body.pitch != mob.pitch {
        base.set_pitch(body.pitch);
    }
    base.fall_distance = body.fall_distance;
    if let Some(Species::Armadillo(armadillo)) = state.mob.as_mut().map(|mob| &mut mob.species) {
        species::armadillo::switch_to_state(base, armadillo, body.armadillo.state);
    }
    if base.kind == Kind::Vex {
        species::vex::set_charging(base, body.vex.charging);
    }
    if let Some(Species::Fish(fish)) = state.mob.as_mut().map(|mob| &mut mob.species) {
        fish.inflate_counter = body.fish.inflate_counter;
        fish.deflate_timer = body.fish.deflate_timer;
    }
    if let Some(Species::Squid(squid)) = state.mob.as_mut().map(|mob| &mut mob.species) {
        squid.movement = body.squid.movement;
    }
    if let Some(Species::Zombie(zombie)) = state.mob.as_mut().map(|mob| &mut mob.species) {
        zombie.searching_for_land = body.drowned.searching_for_land;
    }
    if base.kind.extends("Guardian")
        && species::guardian::active_attack_target(base) != body.guardian.attack_target
    {
        species::guardian::set_active_attack_target(base, body.guardian.attack_target);
    }
    if let Some(Species::Turtle(turtle)) = state.mob.as_mut().map(|mob| &mut mob.species) {
        turtle.home_pos = body.turtle.home_pos;
        turtle.lay_egg_counter = body.turtle.lay_egg_counter;
        turtle.going_home = body.turtle.going_home;
        turtle.travel_pos = body.turtle.travel_pos;
    }
    if base.kind == Kind::Turtle {
        if species::turtle::has_egg(base) != body.turtle.has_egg {
            species::turtle::set_has_egg(base, body.turtle.has_egg);
        }
        if species::turtle::is_laying_egg(base) != body.turtle.laying_egg {
            let mut scratch = species::turtle::TurtleState::default();
            species::turtle::set_laying_egg(base, &mut scratch, body.turtle.laying_egg);
        }
    }
    if let Some(Species::Dolphin(dolphin)) = state.mob.as_mut().map(|mob| &mut mob.species) {
        dolphin.treasure_pos = body.dolphin.treasure_pos;
    }
    if base.kind == Kind::Dolphin && species::dolphin::got_fish(base) != body.dolphin.got_fish {
        species::dolphin::set_got_fish(base, body.dolphin.got_fish);
    }
    if let Some(Species::Bee(bee)) = state.mob.as_mut().map(|mob| &mut mob.species) {
        let facts = body.bee;
        bee.hive_pos = facts.hive_pos;
        bee.flower_pos = facts.flower_pos;
        bee.ticks_without_nectar = facts.ticks_without_nectar;
        bee.stay_out_countdown = facts.stay_out_countdown;
        bee.crops_grown = facts.crops_grown;
        bee.locate_hive_cooldown = facts.locate_hive_cooldown;
        bee.locate_flower_cooldown = facts.locate_flower_cooldown;
        if facts.has_nectar != species::bee::has_nectar(base) {
            let ticks = bee.ticks_without_nectar;
            species::bee::set_has_nectar(base, bee, facts.has_nectar);
            if !facts.has_nectar {
                bee.ticks_without_nectar = ticks;
            }
        }
        species::bee::set_has_stung(base, facts.has_stung);
    }
    if let Some(Species::Phantom(phantom)) = state.mob.as_mut().map(|mob| &mut mob.species) {
        phantom.anchor = body.phantom.anchor;
    }
    if base.kind == Kind::Sniffer {
        let baby = state
            .mob
            .as_ref()
            .and_then(|mob| mob.ageable.as_ref())
            .is_some_and(|ageable| ageable::is_baby(base, ageable));
        species::sniffer::set_state(base, baby, body.sniffer.state);
    }
    state.discard_friction = body.discard_friction;
    if body.pose != mob.pose {
        base.set_pose(body.pose);
        let baby = state
            .mob
            .as_ref()
            .and_then(|mob| mob.ageable.as_ref())
            .is_some_and(|ageable| ageable::is_baby(base, ageable));
        ageable::refresh_dimensions(base, baby);
    }
    living::set_aggressive(base, body.aggressive);
    if base.kind == Kind::Creeper {
        species::set_swell_dir(base, body.swell_dir);
    }
    match (mob.using_item, body.using_item) {
        (None, Some(hand)) => living::start_using_item(base, state, hand_slot(hand)),
        (Some(_), None) => living::stop_using_item(base, state),
        _ => {}
    }
}

pub(crate) fn hand_slot(hand: Hand) -> EquipmentSlot {
    match hand {
        Hand::Main => EquipmentSlot::Mainhand,
        Hand::Off => EquipmentSlot::Offhand,
    }
}

pub fn knockback_velocity(current: Vec3, strength: f64, dx: f64, dz: f64, on_ground: bool) -> Vec3 {
    let push = Vec3::new(dx, 0.0, dz).normalize().scale(strength);
    Vec3::new(
        current.x / 2.0 - push.x,
        if on_ground {
            KNOCKBACK_VERTICAL_CAP.min(current.y / 2.0 + strength)
        } else {
            current.y
        },
        current.z / 2.0 - push.z,
    )
}

fn sound_state<V: rspigot_domain::entity::BlockView>(entity: &Entity, view: &V) -> SoundState {
    let base = &entity.base;
    let water = if !base.fluid.in_water {
        WaterContact::Dry
    } else {
        let eye = base.position.y + f64::from(base.eye_height()) - 0.111_111_111_938_953_4;
        let pos = Vec3::new(base.position.x, eye, base.position.z).block();
        let submerged =
            water_height(view, pos).is_some_and(|height| eye < f64::from(pos.y) + height);
        if submerged {
            WaterContact::Submerged
        } else {
            WaterContact::Touching
        }
    };
    SoundState {
        baby: entity.is_baby(),
        tiny: reward::cube_size(entity) <= 1,
        water,
    }
}

fn hurt_source_of(source: &DamageSource, victim: EntityId) -> HurtSource<'_> {
    HurtSource {
        kind: source.kind,
        causing: source.causing.as_ref().map(|c| &c.name),
        direct: source.direct.as_ref().map(|d| &d.name),
        living_cause: source.living_cause(),
        creative_attacker: source.creative_attacker(),
        self_inflicted: source
            .causing
            .as_ref()
            .is_some_and(|causing| causing.entity == victim),
        reflected_fireball: source
            .direct
            .as_ref()
            .is_some_and(|direct| direct.kind == Kind::Fireball)
            && source
                .causing
                .as_ref()
                .is_some_and(|causing| causing.kind == Kind::Player),
    }
}

pub(crate) fn loot_facts_of(entity: &Entity) -> LootEntity {
    loot_facts(entity)
}

fn loot_facts(entity: &Entity) -> LootEntity {
    let base = &entity.base;
    let kind = base.kind;
    let mut facts = LootEntity::of(kind);
    facts.on_fire = !kind.fire_immune() && base.remaining_fire_ticks > 0;
    facts.baby = entity.is_baby();
    if let Some(living) = entity.living() {
        facts.mainhand = living.equipment(EquipmentSlot::Mainhand).cloned();
    }
    if kind == Kind::Sheep {
        let wool = species::wool_color(&entity.base);
        let dye = DYES.get(usize::from(wool)).copied().unwrap_or(DYES[0]);
        facts
            .components
            .push(("minecraft:sheep/color", dye.to_owned()));
        facts.sheared = Some(species::is_sheared(&entity.base));
    }
    if kind == Kind::Chicken {
        let variant =
            species::variant(&entity.base, rspigot_data::variants::VariantFamily::Chicken);
        facts.components.push((
            "minecraft:chicken/variant",
            species::variant_name(rspigot_data::variants::VariantFamily::Chicken, variant)
                .to_owned(),
        ));
    }
    if kind == Kind::Frog {
        facts.components.push((
            "minecraft:frog/variant",
            species::frog::variant_name(&entity.base).to_owned(),
        ));
    }
    if kind.extends("AbstractCubeMob") {
        facts.cube_size = Some(reward::cube_size(entity));
    }
    if kind.extends("Raider") {
        facts.raider = Some(RaiderFacts {
            has_raid: false,
            captain: false,
        });
    }
    facts
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn mob_source(&self, id: EntityId) -> Option<SourceEntity> {
        let entity = self.entities.store.get(id)?;
        Some(SourceEntity {
            entity: id,
            player: None,
            kind: entity.base.kind,
            creative: false,
            name: self.entity_name(id)?,
            position: entity.base.position,
        })
    }

    pub(crate) fn living_entity_alive(&self, id: EntityId) -> bool {
        if let Some(player) = self.players.values().find(|p| p.entity_id == id) {
            return !player.vitals.is_dead();
        }
        self.entities.store.get(id).is_some_and(|e| {
            !e.base.is_removed()
                && e.living()
                    .is_some_and(|l| !living::is_dead_or_dying(&e.base, l))
        })
    }

    fn entity_loot_facts(&self, id: EntityId) -> Option<LootEntity> {
        if let Some(player) = self.players.values().find(|p| p.entity_id == id) {
            let mut facts = LootEntity::of(Kind::Player);
            facts.mainhand = player.inventory.held().cloned();
            return Some(facts);
        }
        self.entities.store.get(id).map(loot_facts)
    }

    pub(crate) fn hurt_mob(&mut self, id: EntityId, source: &DamageSource, amount: f32) -> bool {
        self.strike(None, Struck::Mob(id), source, amount, AfterHurt::Nothing)
    }

    pub(crate) fn ask_hurt_mob(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        let hurt_source = hurt_source_of(source, id);
        self.panda_stand_up(id);
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return None;
        };
        if !living::can_hurt(base, state, &hurt_source) {
            return None;
        }
        state.no_action_time = 0;
        let amount = self.kind_hurt_amount(id, source, living::floored(amount))?;
        Some(amount)
    }

    pub(crate) fn mob_final_damage(
        &self,
        id: EntityId,
        kind: DamageType,
        self_inflicted: bool,
        amount: f32,
    ) -> f32 {
        self.entities
            .store
            .get(id)
            .and_then(Entity::living)
            .map_or(0.0, |state| {
                living::final_damage(&mut state.clone(), kind, self_inflicted, amount)
            })
    }

    pub(crate) fn apply_hurt_mob(
        &mut self,
        id: EntityId,
        source: &DamageSource,
        amount: f32,
    ) -> bool {
        if self.kind_absorbs_hurt(id, source, amount) {
            return false;
        }
        let now = self.current_tick();
        let game_time = self.world.level.game_time;
        let kind = source.kind;
        let causing_living = source
            .causing
            .as_ref()
            .filter(|c| c.kind.living())
            .map(|c| c.entity);
        let hurt_source = hurt_source_of(source, id);
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return false;
        };
        if !living::can_hurt(base, state, &hurt_source) {
            return false;
        }
        let health_before = living::health(base);
        let outcome = living::hurt_settled(base, state, &hurt_source, amount, now);
        if outcome == HurtOutcome::Ignored {
            return false;
        }
        if living::health(base) < health_before
            && let Some(ageable) = state.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
        {
            ageable.reset_love();
        }
        living::remember_attacker(
            state,
            kind,
            causing_living,
            source.attacker(),
            base.tick_count,
        );
        if outcome == HurtOutcome::Full && !kind.is(DamageTag::NoImpact) {
            base.sync_velocity = true;
        }
        state.damage.last_source = Some((kind, game_time));
        state.damage.last_source_entity = causing_living;
        let position = base.position;
        let dead = living::is_dead_or_dying(base, state);
        if let Some(attacker) = source.attacker()
            && let Some(player) = self.players.get_mut(&attacker)
        {
            player.vitals.attack_ticker = 0;
        }
        if outcome == HurtOutcome::Full {
            self.push(
                Audience::Trackers(id),
                ClientEvent::DamageEvent {
                    entity: id,
                    kind,
                    cause: source.causing.as_ref().map(|c| c.entity),
                    direct: source.direct.as_ref().map(|d| d.entity),
                    position: source.position,
                },
            );
            if !kind.is(DamageTag::NoKnockback) {
                let (dx, dz) = source.knockback_from(position);
                self.knock(
                    Holder::Mob(id),
                    DEFAULT_KNOCKBACK,
                    (dx, dz),
                    source.knock_cause(),
                    source.direct.as_ref().map(|direct| direct.entity),
                );
            }
        }
        if dead {
            if outcome == HurtOutcome::Full && !self.kind_death_sound(id) {
                self.make_entity_sound(id, |sounds| sounds.death, |set| set.death);
            }
            self.charged_creeper_drops(source.causing.as_ref().map(|c| c.entity), id);
            self.die_mob(id, source);
        } else if outcome == HurtOutcome::Full {
            self.reset_ambient_sound(id);
            if !self.kind_hurt_sound(id) {
                self.make_entity_sound(id, |sounds| sounds.hurt, |set| set.hurt);
            }
        }
        if !dead {
            self.zombie_reinforcements(id, source);
        }
        self.kind_after_hurt(id, source, dead);
        true
    }

    pub(crate) fn make_entity_sound(
        &mut self,
        id: EntityId,
        pick: fn(CreatureSounds) -> CreatureSound,
        voice: fn(&rspigot_data::variants::SoundSet) -> Option<&'static str>,
    ) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if entity.base.is_silent() {
            return;
        }
        let Some(sounds) = entity.base.kind.sounds() else {
            return;
        };
        let state = sound_state(entity, &self.world.chunks);
        let variant = crate::mob_life::variant_sound(entity, voice);
        let Some(sound) = variant.unwrap_or_else(|| pick(sounds).get(state)) else {
            return;
        };
        let position = entity.base.position;
        let volume = sounds.volume.get(state.baby, reward::cube_size(entity));
        let source = crate::kinds::kind_sound_source(entity).unwrap_or(sounds.source);
        let pitch = self.voice_pitch(sounds.pitch, state.baby);
        self.play_sound(None, position, sound, source, volume, pitch);
    }

    pub(crate) fn entity_sound(
        &mut self,
        id: EntityId,
        sound: &'static str,
        volume: f32,
        pitch: f32,
    ) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        if entity.base.is_silent() {
            return;
        }
        let source = crate::kinds::kind_sound_source(entity).unwrap_or_else(|| {
            entity
                .base
                .kind
                .sounds()
                .map_or(rspigot_entities::SoundSource::Neutral, |s| s.source)
        });
        let position = entity.base.position;
        self.play_sound(None, position, sound, source, volume, pitch);
    }

    pub(crate) fn die_mob(&mut self, id: EntityId, source: &DamageSource) {
        if self.zombie_infects_villager(id, source) {
            return;
        }
        let level = self.world.level.name.clone();
        let credit = self.entities.store.get(id).and_then(|e| {
            let living = e.living()?;
            let player = living.damage.last_hurt_by_player?;
            self.players.get(&player).map(|p| Component::text(p.name()))
        });
        let victim = self.entity_name(id);
        let owner_message = self.owner_death_message(id, credit.as_ref());
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        if base.is_removed() || state.dead || state.dying {
            return;
        }
        if let (Some(name), Some(victim)) = (base.custom_name(), victim) {
            let message = state.damage.combat.death_message(&victim, credit.as_ref());
            tracing::info!(
                "Named entity {}['{}'/{}, l='ServerLevel[{}]', x={:.2}, y={:.2}, z={:.2}] died: {}",
                base.kind.class(),
                name.plain_text(),
                id.0,
                level,
                base.position.x,
                base.position.y,
                base.position.z,
                crate::console::console_text(&message)
            );
        }
        state.dying = true;
        let killer = source
            .causing
            .as_ref()
            .and_then(|causing| {
                self.players
                    .values()
                    .find(|p| p.entity_id == causing.entity)
            })
            .map(|player| player.id);
        self.raider_died(id, killer);
        self.stop_mob_brain(id);
        if let Some((owner, message)) = owner_message {
            self.push(Audience::One(owner), ClientEvent::system(message));
        }
        self.drop_all_death_loot(id, source);
    }

    fn owner_death_message(
        &self,
        id: EntityId,
        credit: Option<&Component>,
    ) -> Option<(PlayerId, Component)> {
        if !self.config.rules.show_death_messages {
            return None;
        }
        let entity = self.entities.store.get(id)?;
        let state = entity.living()?;
        state.mob.as_ref()?.tamable?;
        let owner = tamable::owner(&entity.base)?;
        let player = self.players.values().find(|p| p.profile.id.0 == owner)?;
        let victim = self.entity_name(id)?;
        Some((
            player.id,
            state.damage.combat.death_message(&victim, credit),
        ))
    }

    fn damage_facts(&self, source: &DamageSource) -> LootDamage {
        LootDamage {
            kind: source.kind,
            source: source
                .causing
                .as_ref()
                .and_then(|c| self.entity_loot_facts(c.entity)),
            direct: source
                .direct
                .as_ref()
                .and_then(|d| self.entity_loot_facts(d.entity)),
            is_direct: source.causing.as_ref().map(|c| c.entity)
                == source.direct.as_ref().map(|d| d.entity),
        }
    }

    fn drop_all_death_loot(&mut self, id: EntityId, source: &DamageSource) {
        let mob_drops = self.config.rules.mob_drops;
        let mut held: Vec<ItemStack> = self.kind_death_item(id).into_iter().collect();
        held.extend(self.kind_death_extra_drops(id));
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(living) = entity.living() else {
            return;
        };
        let killed_by_player = living.damage.player_memory > 0;
        let player = living
            .damage
            .last_hurt_by_player
            .filter(|_| killed_by_player)
            .and_then(|p| self.players.get(&p));
        let player_facts = player.map(|p| {
            let mut facts = LootEntity::of(Kind::Player);
            facts.mainhand = p.inventory.held().cloned();
            facts
        });
        let drop_loot = reward::should_drop_loot(entity, mob_drops);
        let table = living
            .mob
            .as_ref()
            .and_then(|mob| mob.death_loot_table.as_deref())
            .and_then(rspigot_data::loot::table)
            .or_else(|| rspigot_data::loot::entity_table(entity.base.kind.id().0));
        let this = loot_facts(entity);
        let damage = self.damage_facts(source);
        let mut drops: Vec<ItemStack> = held;
        let mut slots = Vec::new();
        if drop_loot {
            if let Some(table) = table {
                let context = LootContext {
                    block: None,
                    tool: None,
                    this: Some(&this),
                    damage: Some(&damage),
                    player: player_facts.as_ref(),
                    luck: 0.0,
                    explosion_radius: None,
                };
                drops.extend(loot::table_drops(table, &context, &mut self.rng));
            }
            if let Some(carried) = species::enderman::carried_block(&entity.base) {
                drops.extend(crate::kinds::carried_block_drops(carried, &mut self.rng));
            }
            slots = self.planned_equipment_drops(id, killed_by_player);
            drops.extend(slots.iter().map(|drop| drop.dropped.clone()));
        }
        let experience = self.experience_reward(id, killed_by_player, mob_drops);
        let step = LootStep {
            victim: id,
            drops,
            slots,
            experience,
        };
        let lane = self.lane_for(None, id);
        let described = self
            .listened(EventKind::EntityDeath)
            .then(|| step.drops.iter().map(spec_of).collect::<Vec<_>>());
        self.fire(
            lane,
            Continuation::MobDeath(Box::new(step)),
            move |server| {
                let entity = server.entity_view(id)?;
                Some(EventPayload::EntityDeath(EntityDeath {
                    damage_source: server.damage_view(source),
                    dropped_exp: experience,
                    drops: described?,
                    entity_type: entity.kind.clone(),
                    entity,
                }))
            },
        );
    }

    pub(crate) fn finish_mob_death(&mut self, step: LootStep, patch: EntityDeathPatch) {
        let LootStep {
            victim,
            drops,
            slots,
            experience,
        } = step;
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(victim)
        else {
            return;
        };
        if base.is_removed() || !state.dying {
            return;
        }
        state.dying = false;
        state.dead = true;
        let position = base.position;
        self.clear_dropped_equipment(victim, &slots);
        let drops = patch.drops.apply_with(
            drops,
            |old, spec| stack_of(&spec).unwrap_or(Some(old)),
            |spec| stack_of(&spec).ok().flatten(),
        );
        let extended = self.kind_of(victim) == Some(Kind::Wither);
        for stack in drops {
            let velocity = Vec3::new(
                self.rng.random::<f64>() * ITEM_SPREAD - ITEM_SPREAD / 2.0,
                ITEM_LIFT,
                self.rng.random::<f64>() * ITEM_SPREAD - ITEM_SPREAD / 2.0,
            );
            let nether_star = stack.item == ItemKind::NetherStar.id();
            let dropped = self.spawn_item_entity(position, velocity, ITEM_PICKUP_DELAY, stack);
            if extended && nether_star {
                self.wither_item_dropped(dropped.id);
            }
        }
        self.award_experience(position, patch.dropped_exp.unwrap_or(experience));
        if let Some(entity) = self.entities.store.get_mut(victim) {
            entity.base.set_pose(rspigot_domain::entity::Pose::Dying);
        }
        self.push(
            Audience::Trackers(victim),
            ClientEvent::EntityStatus {
                entity: victim,
                status: EntityStatus::Death,
            },
        );
    }

    fn planned_equipment_drops(
        &mut self,
        id: EntityId,
        killed_by_player: bool,
    ) -> Vec<EquipmentDrop> {
        let rng = &mut self.rng;
        let Some(living) = self.entities.store.get(id).and_then(|e| e.living()) else {
            return Vec::new();
        };
        reward::planned_equipment_drops(living, killed_by_player, &mut ServerRandom(rng))
    }

    fn clear_dropped_equipment(&mut self, id: EntityId, slots: &[EquipmentDrop]) {
        let Some(living) = self.entities.store.get_mut(id).and_then(|e| e.living_mut()) else {
            return;
        };
        let mut cleared = Vec::new();
        for drop in slots {
            let index = living::slot_index(drop.slot);
            if living.equipment[index].as_ref() == Some(&drop.original) {
                living.equipment[index] = None;
                cleared.push((crate::entities::wire_slot(drop.slot), None));
            }
        }
        if !cleared.is_empty() {
            self.push(
                Audience::Trackers(id),
                ClientEvent::EntityEquipment {
                    entity: id,
                    slots: cleared,
                },
            );
        }
    }

    fn experience_reward(&mut self, id: EntityId, killed_by_player: bool, mob_drops: bool) -> i32 {
        let Some(entity) = self.entities.store.get(id) else {
            return 0;
        };
        if !(killed_by_player && reward::should_drop_experience(entity) && mob_drops) {
            return 0;
        }
        reward::base_experience_reward(entity, &mut ServerRandom(&mut self.rng))
    }

    pub(crate) fn kill_entity(&mut self, id: EntityId) -> bool {
        let living = self
            .entities
            .store
            .get(id)
            .is_some_and(|e| e.living().is_some());
        if living {
            self.hurt_mob(id, &DamageSource::new(DamageType::GenericKill), f32::MAX);
        } else {
            self.remove_entity(id, RemovalReason::Killed, Some(EntityRemoveCause::Death));
        }
        true
    }

    pub(crate) fn void_check(&mut self, id: EntityId) {
        let floor = f64::from(self.world.dimension.height.min_y - VOID_DEPTH);
        let below = self
            .entities
            .store
            .get(id)
            .is_some_and(|e| e.base.position.y < floor);
        if below {
            self.hurt_mob(id, &DamageSource::new(DamageType::OutOfWorld), VOID_DAMAGE);
        }
    }

    pub(crate) fn forget_dead_attackers(&mut self, id: EntityId) {
        let now = self.current_tick();
        let (mob, target) = match self.entities.store.get(id).and_then(|e| e.living()) {
            Some(living) => (living.damage.last_hurt_by_mob, living.damage.last_hurt_mob),
            None => return,
        };
        let mob_alive = mob.is_some_and(|m| self.living_entity_alive(m));
        let target_alive = target.is_some_and(|t| self.living_entity_alive(t));
        if let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        {
            if !target_alive {
                state.damage.last_hurt_mob = None;
            }
            if !mob_alive {
                state.damage.last_hurt_by_mob = None;
            }
            let alive = !living::is_dead_or_dying(base, state);
            state.damage.combat.recheck(now, alive);
        }
    }

    pub(crate) fn mob_landed(&mut self, id: EntityId, landing: Landing) {
        if matches!(
            self.kind_of(id),
            Some(Kind::Ghast | Kind::Allay | Kind::Bee)
        ) {
            return;
        }
        if landing.state.kind() == Some(rspigot_blocks::BlockKind::TurtleEgg) {
            self.egg_fallen_on(id, false, landing.pos);
        }
        let Some(impact) = fall_impact(landing.state, landing.distance, false) else {
            return;
        };
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        if let Some(Species::Creeper(creeper)) = state.mob.as_mut().map(|mob| &mut mob.species) {
            species::creeper_fall(creeper, impact.distance);
        }
        let damage = living::fall_damage(base, state, impact.distance, impact.multiplier);
        if damage <= 0 {
            return;
        }
        if let Some(sounds) = base.kind.sounds() {
            let sound = if damage > BIG_FALL {
                sounds.big_fall
            } else {
                sounds.small_fall
            };
            self.entity_sound(id, sound, 1.0, 1.0);
        }
        self.hurt_mob(
            id,
            &impact.source((landing.pos, landing.state)),
            damage as f32,
        );
    }

    pub fn do_hurt_target(&mut self, attacker: EntityId, target: Target) -> bool {
        self.hurt_target_with(attacker, target, None)
    }

    pub(crate) fn hurt_target_with(
        &mut self,
        attacker: EntityId,
        target: Target,
        charge: Option<(f32, f32)>,
    ) -> bool {
        if self.kind_of(attacker) == Some(Kind::Bee) {
            return self.bee_hurt_target(attacker, target);
        }
        if self
            .entities
            .store
            .get(attacker)
            .is_some_and(|entity| entity.base.kind == Kind::IronGolem)
        {
            return self.iron_golem_hurt_target(attacker, target);
        }
        if matches!(
            self.entities
                .store
                .get(attacker)
                .map(|entity| entity.base.kind),
            Some(Kind::Hoglin | Kind::Zoglin)
        ) {
            return self.hoglin_hurt_target(attacker, target);
        }
        if self.kind_of(attacker) == Some(Kind::Ravager) {
            self.ravager_attack(attacker);
        }
        self.panda_bite(attacker);
        let Some(source_entity) = self.mob_source(attacker) else {
            return false;
        };
        let Some(Entity {
            base,
            state: EntityState::Living(state),
            ..
        }) = self.entities.store.get_mut(attacker)
        else {
            return false;
        };
        let damage = state.attribute(ATTACK_DAMAGE) as f32;
        let knockback = match charge {
            Some((strength, _)) => strength,
            None => state.attribute(ATTACK_KNOCKBACK) as f32 / 2.0,
        };
        let yaw = charge.map_or(base.yaw, |(_, yaw)| yaw);
        let source = DamageSource::by(DamageType::MobAttack, source_entity);
        let victim = match target {
            Target::Player(player) => Struck::Player(player),
            Target::Entity(entity) => Struck::Mob(entity),
        };
        let follow = MobAttackFollow {
            attacker,
            target,
            knockback,
            yaw,
            golem: false,
            hoglin: false,
            bee: false,
            puffer: None,
            cube: false,
        };
        self.strike(None, victim, &source, damage, AfterHurt::MobAttack(follow))
    }

    pub(crate) fn finish_mob_attack(&mut self, follow: MobAttackFollow, hurt: bool) {
        let MobAttackFollow {
            attacker,
            target,
            knockback,
            yaw,
            golem,
            hoglin,
            ..
        } = follow;
        if let Some(puff) = follow.puffer {
            if hurt {
                self.pufferfish_finish_sting(attacker, target, puff);
            }
            return;
        }
        if follow.bee {
            self.bee_finish_attack(attacker, target, hurt);
            return;
        }
        if follow.cube {
            self.cube_finish_attack(attacker, hurt);
            return;
        }
        if golem {
            self.iron_golem_finish_attack(attacker, target, hurt);
            return;
        }
        if hoglin {
            self.hoglin_finish_attack(attacker, target, hurt);
            return;
        }
        if !hurt {
            return;
        }
        self.panda_attack_sound(attacker);
        if knockback > 0.0 {
            let (dx, dz) = mth::yaw_direction(yaw);
            let living_target = match target {
                Target::Player(_) => true,
                Target::Entity(entity) => self
                    .entities
                    .store
                    .get(entity)
                    .is_some_and(|e| e.living().is_some()),
            };
            if living_target {
                self.knockback_target(
                    target,
                    f64::from(knockback),
                    (dx, dz),
                    EntityKnockbackCause::EntityAttack,
                    attacker,
                );
            }
            if living_target && let Some(entity) = self.entities.store.get_mut(attacker) {
                entity.base.velocity =
                    entity
                        .base
                        .velocity
                        .multiply(ATTACKER_SLOWDOWN, 1.0, ATTACKER_SLOWDOWN);
            }
        }
        let target_entity = match target {
            Target::Player(player) => self.players.get(&player).map(|p| p.entity_id),
            Target::Entity(entity) => Some(entity),
        };
        let tick_count = self
            .entities
            .store
            .get(attacker)
            .map_or(0, |e| e.base.tick_count);
        if let Some(living) = self
            .entities
            .store
            .get_mut(attacker)
            .and_then(|e| e.living_mut())
        {
            living.damage.last_hurt_mob = target_entity;
            living.damage.last_hurt_mob_at = tick_count;
        }
        self.kind_attack_sound(attacker);
        self.zombie_attack_effects(attacker, target);
    }

    pub fn mob_damage(&self, id: EntityId) -> Option<MobDamageSnapshot> {
        let entity = self.entities.store.get(id)?;
        let living = entity.living()?;
        Some(MobDamageSnapshot {
            health: living::health(&entity.base),
            hurt_time: living.hurt_time,
            death_time: living.death_time,
            dead: living.dead,
            cooldown: living.damage.cooldown,
            last_hurt_by_player: living.damage.last_hurt_by_player,
            player_memory: living.damage.player_memory,
            last_hurt_by_mob: living.damage.last_hurt_by_mob,
            equipment: EQUIPMENT_SLOTS
                .iter()
                .filter_map(|slot| living.equipment(*slot).map(|stack| (*slot, stack.clone())))
                .collect(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MobDamageSnapshot {
    pub health: f32,
    pub hurt_time: i32,
    pub death_time: i32,
    pub dead: bool,
    pub cooldown: i32,
    pub last_hurt_by_player: Option<PlayerId>,
    pub player_memory: i32,
    pub last_hurt_by_mob: Option<EntityId>,
    pub equipment: Vec<(rspigot_items::EquipmentSlot, ItemStack)>,
}

pub(crate) fn ambient_light_of(dimension: &rspigot_domain::world::Dimension) -> f32 {
    crate::spawning::dimension_spawns(dimension).ambient_light
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ambient_light_follows_the_dimension() {
        use rspigot_domain::world::Dimension;
        assert_eq!(ambient_light_of(&Dimension::overworld()), 0.0);
        assert_eq!(ambient_light_of(&Dimension::nether()), 0.1);
        assert_eq!(ambient_light_of(&Dimension::end()), 0.25);
    }

    #[test]
    fn sky_darkens_at_night_following_the_day_timeline() {
        let at = |day_time| LevelData {
            day_time,
            ..LevelData::default()
        };
        assert_eq!(sky_darken(&at(6000)), 0);
        assert_eq!(sky_darken(&at(18_000)), 11);
        assert_eq!(sky_darken(&at(0)), sky_darken(&at(24_000)));
        assert!(sky_darken(&at(12_768)) > 0 && sky_darken(&at(12_768)) < 11);
    }

    #[test]
    fn rain_and_thunder_darken_the_day_sky() {
        let weather = |raining, thundering, day_time| LevelData {
            day_time,
            raining,
            thundering,
            ..LevelData::default()
        };
        assert_eq!(sky_darken(&weather(true, false, 6000)), 3);
        assert_eq!(sky_darken(&weather(true, true, 6000)), 5);
        assert_eq!(sky_darken(&weather(false, true, 6000)), 0);
        assert_eq!(sky_darken(&weather(true, true, 18_000)), 11);
    }

    #[test]
    fn knockback_halves_motion_and_pushes_away_from_the_source() {
        let pushed = knockback_velocity(Vec3::ZERO, DEFAULT_KNOCKBACK, 1.0, 0.0, true);
        assert_eq!(pushed, Vec3::new(-DEFAULT_KNOCKBACK, 0.4, 0.0));
        let airborne = knockback_velocity(Vec3::new(0.2, -0.3, 0.0), 0.5, 0.0, 2.0, false);
        assert_eq!(airborne, Vec3::new(0.1, -0.3, -0.5));
        let resisted = knockback_velocity(Vec3::new(0.4, 0.2, -0.4), 0.0, 1.0, 1.0, true);
        assert_eq!(resisted, Vec3::new(0.2, 0.1, -0.2));
    }

    #[test]
    fn sheep_colour_and_cube_size_become_loot_facts() {
        use rspigot_domain::entity::{EntityBase, LivingState};
        let mut base = EntityBase::new(EntityId(1), uuid::Uuid::from_u128(1), Kind::Sheep);
        let living = LivingState::new(&mut base, 0.0);
        let mut sheep = Entity::new(base, EntityState::Living(Box::new(living)));
        species::set_wool_color(&mut sheep.base, 14);
        let facts = loot_facts(&sheep);
        assert_eq!(
            facts.components,
            vec![("minecraft:sheep/color", "red".to_owned())]
        );
        assert_eq!(facts.sheared, Some(false));
        let mut base = EntityBase::new(EntityId(2), uuid::Uuid::from_u128(2), Kind::Slime);
        let mut living = LivingState::new(&mut base, 0.0);
        species::cube::set_size(&mut base, &mut living, 4, true);
        let slime = Entity::new(base, EntityState::Living(Box::new(living)));
        assert_eq!(loot_facts(&slime).cube_size, Some(4));
    }
}
