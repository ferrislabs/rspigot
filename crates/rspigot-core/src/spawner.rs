use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::entity::{Aabb, Entity};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::random::RandomSource;
use rspigot_domain::spawn::rules::{check_spawn_obstruction, check_spawn_rules, no_collision};
use rspigot_domain::spawn::{SpawnLevel, SpawnReason, spawn_aabb};
use rspigot_domain::spawner::{self, SpawnData, Spawner};
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, EntityId, Identifier, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

use crate::events::ClientEvent;
use crate::game_events::GameEventContext;
use crate::mobs::ServerRandom;
use crate::plugins::events::{CreatureSpawnReason, SpawnerSpawn};
use crate::plugins::steps::{Birth, Continuation, Spawned, SpawnerRound, SpawnerStep};
use crate::plugins::{EventPayload, Lane, SpawnerData};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const BLOCK_EVENT_RANGE: f64 = 64.0;
const POSITION_KEY: &str = "Pos";

enum Attempt {
    Skip,
    Abort,
    Candidate(Box<Entity>),
}

pub(crate) fn is_spawner_block(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::Spawner)
}

pub(crate) fn middle(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    )
}

pub(crate) fn fixed_position(entity: &Compound) -> Option<Vec3> {
    let [x, y, z] = entity.get_list(POSITION_KEY)? else {
        return None;
    };
    Some(Vec3::new(x.as_f64()?, y.as_f64()?, z.as_f64()?))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn spawner_at(&self, pos: BlockPos) -> Option<Spawner> {
        self.world
            .chunks
            .block(pos)
            .filter(|s| is_spawner_block(*s))?;
        let data = self
            .world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .map(|entity| Spawner::read(&entity.data));
        Some(data.unwrap_or_default())
    }

    fn store_spawner(&mut self, pos: BlockPos, spawner: &Spawner) {
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        let (kind, mut data) = chunk.block_entity(pos).map_or_else(
            || (Identifier::minecraft(spawner::ENTITY), Compound::new()),
            |existing| (existing.kind.clone(), existing.data.clone()),
        );
        spawner.write(&mut data);
        chunk.set_block_entity(BlockEntity { pos, kind, data });
    }

    pub(crate) fn ensure_spawner_entity(&mut self, pos: BlockPos, state: BlockStateId) {
        if !is_spawner_block(state) {
            return;
        }
        let present = self
            .world
            .chunks
            .get(pos.chunk())
            .is_some_and(|chunk| chunk.block_entity(pos).is_some());
        if !present {
            self.store_spawner(pos, &Spawner::default());
        }
    }

    pub(crate) fn tick_spawners(&mut self) {
        let mut found: Vec<BlockPos> = Vec::new();
        for chunk_pos in self.world.chunks.loaded_positions() {
            let Some(chunk) = self.world.chunks.get(chunk_pos) else {
                continue;
            };
            let mut here = chunk
                .block_entities
                .iter()
                .filter(|entity| entity.kind.path() == spawner::ENTITY)
                .map(|entity| entity.pos)
                .peekable();
            if here.peek().is_some() && self.entity_ticking(chunk_pos) {
                found.extend(here);
            }
        }
        for pos in found {
            self.tick_spawner(pos);
        }
    }

    fn tick_spawner(&mut self, pos: BlockPos) {
        let Some(mut spawner) = self.spawner_at(pos) else {
            return;
        };
        if !self.player_near_spawner(pos, spawner.player_range) {
            return;
        }
        if spawner.delay == -1 {
            self.reset_spawner_delay(pos, &mut spawner);
        }
        if spawner.delay >= 1 {
            spawner.delay -= 1;
            self.store_spawner(pos, &spawner);
            return;
        }
        let left = spawner.count;
        self.store_spawner(pos, &spawner);
        self.spawner_round(SpawnerRound {
            pos,
            left,
            spawned: false,
        });
    }

    fn player_near_spawner(&self, pos: BlockPos, range: i32) -> bool {
        let center = middle(pos);
        let reach = f64::from(range);
        self.players.values().any(|player| {
            !player.vitals.removed
                && !player.vitals.dead
                && player.game_mode != GameMode::Spectator
                && player.position.distance_squared(center) < reach * reach
        })
    }

    fn reset_spawner_delay(&mut self, pos: BlockPos, spawner: &mut Spawner) {
        spawner.reset_delay(&mut ServerRandom(&mut self.rng));
        self.store_spawner(pos, spawner);
        self.push(
            Audience::Nearby {
                position: middle(pos),
                range: BLOCK_EVENT_RANGE,
                except: None,
            },
            ClientEvent::BlockEvent {
                pos,
                action: spawner::BLOCK_EVENT,
                data: 0,
                block: BlockKind::Spawner.id().0,
            },
        );
    }

    fn delay_spawner(&mut self, pos: BlockPos) {
        if let Some(mut spawner) = self.spawner_at(pos) {
            self.reset_spawner_delay(pos, &mut spawner);
        }
    }

    fn spawner_round(&mut self, mut round: SpawnerRound) {
        while round.left > 0 {
            round.left -= 1;
            let Some(mut spawner) = self.spawner_at(round.pos) else {
                return;
            };
            let had_next = spawner.next.is_some();
            let attempt = self.spawner_attempt(round.pos, &mut spawner);
            if !had_next {
                self.store_spawner(round.pos, &spawner);
            }
            match attempt {
                Attempt::Skip => {}
                Attempt::Abort => {
                    self.delay_spawner(round.pos);
                    return;
                }
                Attempt::Candidate(entity) => {
                    self.offer_spawn(round, &spawner, *entity);
                    return;
                }
            }
        }
        if round.spawned {
            self.delay_spawner(round.pos);
        }
    }

    fn spawner_attempt(&mut self, pos: BlockPos, spawner: &mut Spawner) -> Attempt {
        let data = spawner.next_data(&mut ServerRandom(&mut self.rng)).clone();
        let Some(kind) = data.entity_id().and_then(Kind::from_name) else {
            return Attempt::Abort;
        };
        let at = self.spawner_spawn_point(pos, spawner.range, &data);
        let block = at.block();
        if !no_collision(self, &spawn_aabb(kind, at.x, at.y, at.z)) {
            return Attempt::Skip;
        }
        if !self.spawner_spot_valid(kind, block, &data) {
            return Attempt::Skip;
        }
        let nbt = (!data.only_names_the_entity()).then(|| data.entity.clone());
        let Ok((mut entity, _)) = self.summon_entity(kind, at, nbt, SpawnReason::Spawner) else {
            return Attempt::Abort;
        };
        if self.spawner_crowded(pos, spawner, kind) {
            return Attempt::Abort;
        }
        let yaw = ServerRandom(&mut self.rng).next_float() * 360.0;
        let position = entity.base.position;
        entity.base.snap_to(position, yaw, 0.0);
        if entity.living().is_some_and(|state| state.is_mob())
            && !self.mob_fits_spawner(&entity, kind, &data)
        {
            return Attempt::Skip;
        }
        Attempt::Candidate(Box::new(entity))
    }

    fn spawner_spawn_point(&mut self, pos: BlockPos, range: i32, data: &SpawnData) -> Vec3 {
        if let Some(fixed) = fixed_position(&data.entity) {
            return fixed;
        }
        let mut random = ServerRandom(&mut self.rng);
        let reach = f64::from(range);
        let x = f64::from(pos.x) + (random.next_double() - random.next_double()) * reach + 0.5;
        let y = f64::from(pos.y + random.next_int(3) - 1);
        let z = f64::from(pos.z) + (random.next_double() - random.next_double()) * reach + 0.5;
        Vec3::new(x, y, z)
    }

    fn spawner_spot_valid(&mut self, kind: Kind, block: BlockPos, data: &SpawnData) -> bool {
        let Some(limits) = data.light_limits() else {
            return check_spawn_rules(self, kind, SpawnReason::Spawner, block);
        };
        let peaceful = self.difficulty() == Difficulty::Peaceful;
        if !kind.category().is_friendly() && peaceful {
            return false;
        }
        let sky = self.sky_light(block) - self.sky_darken();
        limits.admits(self.block_light(block), sky)
    }

    fn spawner_crowded(&self, pos: BlockPos, spawner: &Spawner, kind: Kind) -> bool {
        let area = Aabb::new(
            f64::from(pos.x),
            f64::from(pos.y),
            f64::from(pos.z),
            f64::from(pos.x + 1),
            f64::from(pos.y + 1),
            f64::from(pos.z + 1),
        )
        .inflate(f64::from(spawner.range));
        let from = Vec3::new(area.min_x, 0.0, area.min_z).chunk();
        let to = Vec3::new(area.max_x, 0.0, area.max_z).chunk();
        let mut count = 0;
        for x in from.x..=to.x {
            for z in from.z..=to.z {
                count += self
                    .entities
                    .store
                    .in_chunk(ChunkPos::new(x, z))
                    .iter()
                    .filter_map(|id| self.entities.store.get(*id))
                    .filter(|entity| {
                        !entity.base.is_removed()
                            && entity.base.kind == kind
                            && entity.base.bounding_box().intersects(&area)
                    })
                    .count();
            }
        }
        i32::try_from(count).unwrap_or(i32::MAX) >= spawner.max_nearby
    }

    fn mob_fits_spawner(&self, entity: &Entity, kind: Kind, data: &SpawnData) -> bool {
        let rules_ok = data.rules.is_some()
            || rspigot_domain::spawn::rules::mob_check_spawn_rules(
                self,
                kind,
                entity.base.position,
            );
        rules_ok && check_spawn_obstruction(self, kind, &entity.base.bounding_box())
    }

    fn offer_spawn(&mut self, round: SpawnerRound, spawner: &Spawner, entity: Entity) {
        let view = crate::plugins::spawns::view_of_entity(&entity);
        let location = self.location_of(
            entity.base.position,
            Rotation::new(entity.base.yaw, entity.base.pitch),
        );
        let data = SpawnerData {
            pos: round.pos,
            spawned: spawner.spawned_id().map(str::to_owned),
            delay: spawner.delay,
        };
        let entity_type = view.kind.clone();
        let step = SpawnerStep { round, entity };
        self.fire(
            Lane::World,
            Continuation::SpawnerSpawn(Box::new(step)),
            move |_| {
                Some(EventPayload::SpawnerSpawn(SpawnerSpawn {
                    entity: view,
                    entity_type,
                    location,
                    spawner: data,
                }))
            },
        );
    }

    pub(crate) fn finish_spawner_spawn(&mut self, step: SpawnerStep, cancelled: bool) {
        let SpawnerStep { round, entity } = step;
        if cancelled {
            self.spawner_round(round);
            return;
        }
        self.spawn_entity(
            entity,
            Birth::of(CreatureSpawnReason::Spawner).then(Spawned::Spawner(round)),
        );
    }

    pub(crate) fn finish_spawner_birth(&mut self, round: SpawnerRound, placed: Option<EntityId>) {
        let Some(id) = placed else {
            self.delay_spawner(round.pos);
            return;
        };
        self.level_event(spawner::SPAWN_EFFECT_EVENT, round.pos, 0);
        let at = self
            .entities
            .store
            .get(id)
            .map_or_else(|| middle(round.pos), |entity| entity.base.position);
        self.game_event(
            GameEvent::EntityPlace,
            middle(at.block()),
            GameEventContext::of(Some(id)),
        );
        let is_mob = self
            .entities
            .store
            .get(id)
            .and_then(Entity::living)
            .is_some_and(|state| state.is_mob());
        if is_mob {
            self.spawn_anim(id);
        }
        self.spawner_round(SpawnerRound {
            spawned: true,
            ..round
        });
    }

    pub(crate) fn use_egg_on_spawner(
        &mut self,
        id: PlayerId,
        slot: usize,
        egg: &ItemStack,
        pos: BlockPos,
    ) -> bool {
        let Some(kind) = Kind::from_spawn_egg(egg.item.0) else {
            return false;
        };
        if self
            .world
            .chunks
            .block(pos)
            .is_some_and(crate::trial_spawner::is_trial_spawner)
        {
            if !self.within_reach(id, pos) || !self.use_egg_on_trial_spawner(pos, kind) {
                return false;
            }
            let player = self.players.get(&id).map(|player| player.entity_id);
            self.block_game_event(GameEvent::BlockChange, pos, GameEventContext::of(player));
            self.use_spawn_egg(id, slot, egg);
            return true;
        }
        let Some(mut spawner) = self.spawner_at(pos) else {
            return false;
        };
        if !self.within_reach(id, pos) {
            return false;
        }
        spawner.set_entity(kind.name());
        self.store_spawner(pos, &spawner);
        self.send_spawner_update(pos);
        let player = self.players.get(&id).map(|player| player.entity_id);
        self.block_game_event(GameEvent::BlockChange, pos, GameEventContext::of(player));
        self.use_spawn_egg(id, slot, egg);
        true
    }

    fn send_spawner_update(&mut self, pos: BlockPos) {
        let Some(mut data) = self
            .world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .map(|entity| entity.data.clone())
        else {
            return;
        };
        data.remove("SpawnPotentials");
        self.push(
            Audience::ChunkViewers(pos.chunk()),
            ClientEvent::BlockEntityUpdate {
                pos,
                kind: "minecraft:mob_spawner",
                data,
            },
        );
    }
}
