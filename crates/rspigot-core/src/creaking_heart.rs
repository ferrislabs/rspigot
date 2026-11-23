use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::entity::species::creaking;
use rspigot_domain::random::RandomSource;
use rspigot_domain::spawn::SpawnReason;
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Identifier, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_nbt::{Compound, Tag};
use rustc_hash::{FxHashMap, FxHashSet};
use uuid::Uuid;

use rspigot_domain::damage::DamageType;
use rspigot_domain::particle::{Particle, ParticleData};

use crate::events::ClientEvent;
use crate::health::DamageSource;
use crate::mobs::ServerRandom;
use crate::plugins::events::{CreatureSpawnReason, EntityRemoveCause};
use crate::plugins::steps::Birth;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const ENTITY: &str = "creaking_heart";
const LINK_KEY: &str = "creaking";
const STATE: &str = "creaking_heart_state";
const PALE_LOGS: &str = "minecraft:pale_oak_logs";
const ACTIVE_ATTRIBUTE: &str = "minecraft:gameplay/creaking_active";
const OVERWORLD_CLOCK: &str = "minecraft:overworld";
const TICK_BASE: i32 = 20;
const TICK_SPREAD: i32 = 5;
const KEEP_DISTANCE: f64 = 34.0;
const SIGNAL_DISTANCE: f64 = 32.0;
const SPAWN_PLAYER_RANGE: f64 = 32.0;
const SPAWN_ATTEMPTS: i32 = 5;
const SPAWN_HORIZONTAL: i32 = 16;
const SPAWN_VERTICAL: i32 = 8;
const EMITTER_TICKS: i32 = 100;
const CRUMBLE_RANGE: f64 = 32.0;
const RESIN_ATTEMPTS: (i32, i32) = (2, 3);
const RESIN_DEPTH: i32 = 2;
const RESIN_LIMIT: usize = 64;
const SPAWN_SOUND: &str = "minecraft:block.creaking_heart.spawn";
const HURT_SOUND: &str = "minecraft:block.creaking_heart.hurt";
const RESIN_SOUND: &str = "minecraft:block.resin.place";
const CREAKING_SPAWN_SOUND: &str = "minecraft:entity.creaking.spawn";
const CREAKING_DEATH_SOUND: &str = "minecraft:entity.creaking.death";
const FACES: [(&str, (i32, i32, i32)); 6] = [
    ("down", (0, -1, 0)),
    ("up", (0, 1, 0)),
    ("north", (0, 0, -1)),
    ("south", (0, 0, 1)),
    ("west", (-1, 0, 0)),
    ("east", (1, 0, 0)),
];

#[derive(Debug, Clone, Default)]
pub(crate) struct HeartRuntime {
    pub(crate) ticker: i32,
    pub(crate) emitter: i32,
    pub(crate) signal: i32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Hearts {
    pub(crate) runtime: FxHashMap<BlockPos, HeartRuntime>,
}

fn is_heart(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::CreakingHeart)
}

fn uuid_tag(uuid: Uuid) -> Tag {
    let bits = uuid.as_u128();
    Tag::IntArray(vec![
        (bits >> 96) as u32 as i32,
        (bits >> 64) as u32 as i32,
        (bits >> 32) as u32 as i32,
        bits as u32 as i32,
    ])
}

fn uuid_of(tag: &Tag) -> Option<Uuid> {
    let Tag::IntArray(values) = tag else {
        return None;
    };
    let [a, b, c, d] = values.as_slice() else {
        return None;
    };
    let bits = (u128::from(*a as u32) << 96)
        | (u128::from(*b as u32) << 64)
        | (u128::from(*c as u32) << 32)
        | u128::from(*d as u32);
    Some(Uuid::from_u128(bits))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn ensure_creaking_heart_entity(&mut self, pos: BlockPos, state: BlockStateId) {
        if !is_heart(state) {
            return;
        }
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        if chunk.block_entity(pos).is_none() {
            chunk.set_block_entity(BlockEntity {
                pos,
                kind: Identifier::minecraft(ENTITY),
                data: Compound::new(),
            });
        }
    }

    pub(crate) fn creaking_heart_signal(&self, pos: BlockPos) -> i32 {
        self.spawning
            .hearts
            .runtime
            .get(&pos)
            .map_or(0, |runtime| runtime.signal)
    }

    fn heart_link(&self, pos: BlockPos) -> Option<Uuid> {
        self.world
            .chunks
            .get(pos.chunk())?
            .block_entity(pos)?
            .data
            .get(LINK_KEY)
            .and_then(uuid_of)
    }

    fn set_heart_link(&mut self, pos: BlockPos, link: Option<Uuid>) {
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        let Some(existing) = chunk.block_entity(pos) else {
            return;
        };
        let mut entity = existing.clone();
        match link {
            Some(uuid) => {
                entity.data.insert(LINK_KEY, uuid_tag(uuid));
            }
            None => {
                entity.data.remove(LINK_KEY);
            }
        }
        chunk.set_block_entity(entity);
    }

    fn creaking_active_at(&self) -> bool {
        crate::environment_attributes::value(
            ACTIVE_ATTRIBUTE,
            self.world.dimension.key.as_str(),
            &[(OVERWORLD_CLOCK, self.world.level.day_time)],
        )
        .is_some_and(|value| value == rspigot_data::loot::AttributeValue::Bool(true))
    }

    fn has_required_logs(&self, pos: BlockPos, state: BlockStateId) -> bool {
        let axis = state.property("axis").unwrap_or("y");
        let (dx, dy, dz) = match axis {
            "x" => (1, 0, 0),
            "z" => (0, 0, 1),
            _ => (0, 1, 0),
        };
        [(dx, dy, dz), (-dx, -dy, -dz)]
            .into_iter()
            .all(|(x, y, z)| {
                rspigot_domain::entity::physics::has_tag(
                    self.block_or_air(pos.offset(x, y, z)),
                    PALE_LOGS,
                )
            })
    }

    pub(crate) fn tick_creaking_hearts(&mut self) {
        let mut found: Vec<BlockPos> = Vec::new();
        for chunk_pos in self.world.chunks.loaded_positions() {
            let Some(chunk) = self.world.chunks.get(chunk_pos) else {
                continue;
            };
            found.extend(
                chunk
                    .block_entities
                    .iter()
                    .filter(|entity| entity.kind.path() == ENTITY)
                    .map(|entity| entity.pos),
            );
        }
        let alive: FxHashSet<BlockPos> = found.iter().copied().collect();
        self.spawning
            .hearts
            .runtime
            .retain(|pos, _| alive.contains(pos));
        for pos in found {
            self.tick_creaking_heart(pos);
        }
    }

    fn protector_of(&self, pos: BlockPos) -> Option<EntityId> {
        let uuid = self.heart_link(pos)?;
        self.entities.store.by_uuid(uuid)
    }

    fn distance_to_creaking(&self, pos: BlockPos, creaking: EntityId) -> f64 {
        let center = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        );
        self.entities.store.get(creaking).map_or(0.0, |entity| {
            entity.base.position.distance_squared(center).sqrt()
        })
    }

    fn tick_creaking_heart(&mut self, pos: BlockPos) {
        let state = self.block_or_air(pos);
        if !is_heart(state) {
            return;
        }
        let linked = self.heart_link(pos).is_some();
        let protector = self.protector_of(pos);
        let signal = match protector {
            Some(creaking) if linked => {
                let distance = self.distance_to_creaking(pos, creaking);
                15 - ((distance.clamp(0.0, SIGNAL_DISTANCE) / SIGNAL_DISTANCE) * 15.0).floor()
                    as i32
            }
            _ => 0,
        };
        let entry = self.spawning.hearts.runtime.entry(pos).or_default();
        if entry.signal != signal {
            entry.signal = signal;
            self.update_neighbors_at(pos);
        }
        let entry = self.spawning.hearts.runtime.entry(pos).or_default();
        if entry.emitter > 0 {
            entry.emitter -= 1;
        }
        let waiting = entry.ticker;
        entry.ticker -= 1;
        if waiting > -1 {
            return;
        }
        let spread = ServerRandom(&mut self.rng).next_int(TICK_SPREAD);
        if let Some(entry) = self.spawning.hearts.runtime.get_mut(&pos) {
            entry.ticker = TICK_BASE + spread;
        }
        let wanted = if !self.has_required_logs(pos, state) && !linked {
            "uprooted"
        } else if self.creaking_active_at() {
            "awake"
        } else {
            "dormant"
        };
        let mut current = state;
        if state.property(STATE) != Some(wanted)
            && let Some(next) = state.with_property(STATE, wanted)
        {
            self.set_block(pos, next);
            current = next;
            if wanted == "uprooted" {
                return;
            }
        }
        if linked {
            let Some(creaking) = protector else {
                return;
            };
            let persistent = self
                .entities
                .store
                .get(creaking)
                .and_then(|entity| entity.living())
                .and_then(|living| living.mob.as_ref())
                .is_some_and(|mob| mob.persistence_required);
            let distance = self.distance_to_creaking(pos, creaking);
            if (self.creaking_active_at() || persistent)
                && distance <= KEEP_DISTANCE
                && !self.creaking_player_stuck(creaking)
            {
                return;
            }
            self.remove_creaking_protector(pos);
            return;
        }
        if current.property(STATE) != Some("awake") {
            return;
        }
        let monsters =
            self.world.level.spawn_monsters && self.world.difficulty != Difficulty::Peaceful;
        let center = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        );
        let near = self.players.values().any(|player| {
            !player.vitals.is_dead()
                && player.position.distance_squared(center)
                    <= SPAWN_PLAYER_RANGE * SPAWN_PLAYER_RANGE
        });
        if monsters && near {
            self.spawn_creaking_protector(pos);
        }
    }

    fn spawn_creaking_protector(&mut self, pos: BlockPos) {
        for _ in 0..SPAWN_ATTEMPTS {
            let (dx, dz) = {
                let mut random = ServerRandom(&mut self.rng);
                (
                    random.next_int(2 * SPAWN_HORIZONTAL + 1) - SPAWN_HORIZONTAL,
                    random.next_int(2 * SPAWN_HORIZONTAL + 1) - SPAWN_HORIZONTAL,
                )
            };
            let Some(spot) = self.creaking_spot(pos.offset(dx, 0, dz), pos.y) else {
                continue;
            };
            let at = Vec3::new(
                f64::from(spot.x) + 0.5,
                f64::from(spot.y),
                f64::from(spot.z) + 0.5,
            );
            let Ok((mut entity, _)) =
                self.summon_entity(Kind::Creaking, at, None, SpawnReason::Spawner)
            else {
                continue;
            };
            creaking::set_home_pos(&mut entity.base, pos);
            let uuid = entity.base.uuid;
            let id = entity.base.id;
            self.spawn_entity(entity, Birth::of(CreatureSpawnReason::Spawner));
            if self.entities.store.get(id).is_none() {
                continue;
            }
            self.set_heart_link(pos, Some(uuid));
            self.entity_sound(id, CREAKING_SPAWN_SOUND, 1.0, 1.0);
            self.play_sound(None, at, SPAWN_SOUND, SoundSource::Blocks, 1.0, 1.0);
            return;
        }
    }

    fn creaking_spot(&self, column: BlockPos, around: i32) -> Option<BlockPos> {
        for y in (around - SPAWN_VERTICAL..=around + SPAWN_VERTICAL).rev() {
            let at = BlockPos::new(column.x, y, column.z);
            let below = self.block_or_air(at.offset(0, -1, 0));
            let solid = !below.is_air()
                && !below
                    .kind()
                    .is_some_and(|kind| kind.name().ends_with("_leaves"))
                && !rspigot_domain::entity::physics::has_tag(below, "minecraft:leaves");
            if solid
                && self.block_or_air(at).is_air()
                && self.block_or_air(at.offset(0, 1, 0)).is_air()
            {
                return Some(at);
            }
        }
        None
    }

    pub(crate) fn remove_creaking_protector(&mut self, pos: BlockPos) {
        let Some(creaking) = self.protector_of(pos) else {
            self.set_heart_link(pos, None);
            return;
        };
        self.creaking_tear_down(creaking);
        self.set_heart_link(pos, None);
    }

    pub(crate) fn creaking_tear_down(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let at = entity.base.position;
        let bounds = entity.base.bounding_box();
        let center = Vec3::new(
            (bounds.min_x + bounds.max_x) / 2.0,
            (bounds.min_y + bounds.max_y) / 2.0,
            (bounds.min_z + bounds.max_z) / 2.0,
        );
        let spread = Vec3::new(
            (bounds.max_x - bounds.min_x) * creaking::CRUMBLE_SPREAD,
            (bounds.max_y - bounds.min_y) * creaking::CRUMBLE_SPREAD,
            (bounds.max_z - bounds.min_z) * creaking::CRUMBLE_SPREAD,
        );
        let wood = BlockKind::PaleOakWood.default_state();
        let heart = BlockKind::CreakingHeart
            .state_with(&[(STATE, "awake")])
            .unwrap_or_else(|| BlockKind::CreakingHeart.default_state());
        for (state, count) in [
            (wood, creaking::CRUMBLE_WOOD_COUNT),
            (heart, creaking::CRUMBLE_HEART_COUNT),
        ] {
            self.push(
                Audience::Nearby {
                    position: center,
                    range: CRUMBLE_RANGE,
                    except: None,
                },
                ClientEvent::Particle(Particle {
                    data: ParticleData::Block(state),
                    count,
                    spread,
                    ..Particle::single("minecraft:block_crumble", center)
                }),
            );
        }
        self.play_sound(
            None,
            at,
            CREAKING_DEATH_SOUND,
            SoundSource::Hostile,
            1.0,
            1.0,
        );
        self.remove_entity(
            id,
            rspigot_domain::entity::RemovalReason::Discarded,
            Some(EntityRemoveCause::Despawn),
        );
    }

    pub(crate) fn creaking_death_effects(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let at = entity.base.position;
        creaking::set_tearing_down(&mut entity.base);
        self.play_sound(
            None,
            at,
            creaking::TWITCH_SOUND,
            SoundSource::Hostile,
            1.0,
            1.0,
        );
        self.hurt_mob(id, &DamageSource::new(DamageType::GenericKill), f32::MAX);
    }

    fn creaking_bound_to(&self, home: BlockPos) -> Option<EntityId> {
        self.entities
            .store
            .iter()
            .find(|entity| {
                entity.base.kind == Kind::Creaking
                    && !entity.base.is_removed()
                    && creaking::home_pos(&entity.base) == Some(home)
            })
            .map(|entity| entity.base.id)
    }

    pub(crate) fn creaking_heart_will_destroy(&mut self, pos: BlockPos, state: BlockStateId) {
        if !is_heart(state) {
            return;
        }
        let Some(creaking) = self.creaking_bound_to(pos) else {
            return;
        };
        self.creaking_death_effects(creaking);
        self.set_heart_link(pos, None);
    }

    fn creaking_player_stuck(&mut self, id: EntityId) -> bool {
        let Some(bounds) = self
            .entities
            .store
            .get(id)
            .map(|entity| entity.base.bounding_box())
        else {
            return false;
        };
        let eyes: Vec<bool> = self
            .players
            .values()
            .filter(|player| !player.vitals.is_dead())
            .map(|player| {
                let eye = player.eye_position();
                eye.x >= bounds.min_x
                    && eye.x < bounds.max_x
                    && eye.y >= bounds.min_y
                    && eye.y < bounds.max_y
                    && eye.z >= bounds.min_z
                    && eye.z < bounds.max_z
            })
            .collect();
        self.entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
            .and_then(|living| living.mob.as_mut())
            .and_then(|mob| match &mut mob.species {
                rspigot_domain::entity::species::Species::Creaking(state) => {
                    Some(state.player_is_stuck(&eyes))
                }
                _ => None,
            })
            .unwrap_or(false)
    }

    pub(crate) fn creaking_heart_hurt(&mut self, creaking: EntityId) {
        let Some(home) = self
            .entities
            .store
            .get(creaking)
            .and_then(|entity| creaking::home_pos(&entity.base))
        else {
            return;
        };
        let state = self.block_or_air(home);
        if !is_heart(state) || self.protector_of(home) != Some(creaking) {
            return;
        }
        let waiting = self
            .spawning
            .hearts
            .runtime
            .get(&home)
            .is_some_and(|runtime| runtime.emitter > 0);
        if waiting {
            return;
        }
        if state.property(STATE) == Some("awake") {
            let (low, high) = RESIN_ATTEMPTS;
            let rounds = low + self.rng.random_range(0..=(high - low));
            for _ in 0..rounds {
                self.spread_resin(home);
            }
        }
        if let Some(runtime) = self.spawning.hearts.runtime.get_mut(&home) {
            runtime.emitter = EMITTER_TICKS;
        }
        let at = Vec3::new(
            f64::from(home.x) + 0.5,
            f64::from(home.y) + 0.5,
            f64::from(home.z) + 0.5,
        );
        self.play_sound(None, at, HURT_SOUND, SoundSource::Blocks, 1.0, 1.0);
    }

    fn spread_resin(&mut self, origin: BlockPos) {
        let mut queue = std::collections::VecDeque::from([(origin, 0)]);
        let mut seen: FxHashSet<BlockPos> = FxHashSet::default();
        seen.insert(origin);
        let mut visited = 0;
        while let Some((pos, depth)) = queue.pop_front() {
            if visited >= RESIN_LIMIT {
                return;
            }
            visited += 1;
            if self.visit_for_resin(pos) {
                return;
            }
            if depth >= RESIN_DEPTH {
                continue;
            }
            let mut order = FACES;
            for index in (1..order.len()).rev() {
                order.swap(index, self.rng.random_range(0..=index));
            }
            for (_, (x, y, z)) in order {
                let next = pos.offset(x, y, z);
                if rspigot_domain::entity::physics::has_tag(self.block_or_air(next), PALE_LOGS)
                    && seen.insert(next)
                {
                    queue.push_back((next, depth + 1));
                }
            }
        }
    }

    fn visit_for_resin(&mut self, pos: BlockPos) -> bool {
        if !rspigot_domain::entity::physics::has_tag(self.block_or_air(pos), PALE_LOGS) {
            return false;
        }
        let mut order = FACES;
        for index in (1..order.len()).rev() {
            order.swap(index, self.rng.random_range(0..=index));
        }
        for (name, (x, y, z)) in order {
            let target = pos.offset(x, y, z);
            let opposite = FACES
                .iter()
                .find(|(_, (ox, oy, oz))| (*ox, *oy, *oz) == (-x, -y, -z))
                .map_or(name, |(face, _)| *face);
            let state = self.block_or_air(target);
            let clump = if state.is_air() {
                BlockKind::ResinClump.default_state()
            } else if state.kind() == Some(BlockKind::Water) {
                return false;
            } else {
                state
            };
            if clump.kind() == Some(BlockKind::ResinClump)
                && clump.property(opposite) != Some("true")
                && let Some(next) = clump.with_property(opposite, "true")
            {
                self.set_block(target, next);
                self.play_sound(
                    None,
                    Vec3::new(
                        f64::from(target.x) + 0.5,
                        f64::from(target.y) + 0.5,
                        f64::from(target.z) + 0.5,
                    ),
                    RESIN_SOUND,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
                return true;
            }
        }
        false
    }
}
