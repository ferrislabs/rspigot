use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::beehive::{
    self, Beehive, ENTER_SOUND, EXIT_SOUND, Occupant, ReleaseStatus, SHEAR_SOUND, WORK_SOUND,
};
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::entity::species::{Species, bee};
use rspigot_domain::entity::{Aabb, Entity, EntityState, RemovalReason, ageable, save};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Identifier, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;
use rspigot_nbt::Compound;

use crate::events::Hand;
use crate::game_events::GameEventContext;
use crate::mobs::ServerRandom;
use crate::plugins::events::{CreatureSpawnReason, EntityEnterBlock, EntityRemoveCause};
use crate::plugins::steps::{Birth, Continuation, HiveRelease, Spawned};
use crate::plugins::{EventPayload, Lane};
use crate::ports::ClientLink;
use crate::server::Server;
use rspigot_ai::TargetReason;

const HIVE_ENTITY: &str = "beehive";
const CAMPFIRES: &str = "minecraft:campfires";
const HIVE_INHABITORS: &str = "minecraft:beehive_inhabitors";
const ENCHANTMENT_REGISTRY: &str = "minecraft:enchantment";
const PREVENTS_BEE_SPAWNS: &str = "minecraft:prevents_bee_spawns_when_mining";
const HONEY_LEVEL: &str = "honey_level";
const FACING: &str = "facing";
const LIT: &str = "lit";
const FULL_HONEY: i32 = beehive::MAX_HONEY_LEVEL;
const SMOKE_DEPTH: i32 = 5;
const POST_MIN: f64 = 7.0 / 16.0;
const POST_MAX: f64 = 9.0 / 16.0;
const FIRE_REACH: i32 = 1;
const HARVEST_TABLE: &str = "minecraft:harvest/beehive";

pub(crate) fn is_hive_block(state: BlockStateId) -> bool {
    matches!(state.kind(), Some(BlockKind::Beehive | BlockKind::BeeNest))
}

pub(crate) fn bees_stay_in_hive(raining: bool, dimension: &str, day_time: i64) -> bool {
    if raining {
        return true;
    }
    if dimension != "minecraft:overworld" {
        return false;
    }
    let time = day_time.rem_euclid(24_000);
    (bee::STAY_IN_HIVE_FROM..bee::STAY_IN_HIVE_UNTIL).contains(&time)
}

pub(crate) fn honey_level(state: BlockStateId) -> i32 {
    state
        .property(HONEY_LEVEL)
        .and_then(|level| level.parse().ok())
        .unwrap_or(0)
}

fn facing_step(state: BlockStateId) -> (i32, i32) {
    match state.property(FACING) {
        Some("south") => (0, 1),
        Some("east") => (1, 0),
        Some("west") => (-1, 0),
        _ => (0, -1),
    }
}

fn block_in_tag(state: Option<BlockStateId>, tag: &str) -> bool {
    state.and_then(BlockStateId::kind).is_some_and(|kind| {
        rspigot_data::tags::tag("minecraft:block", tag)
            .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
    })
}

fn is_lit_campfire(state: Option<BlockStateId>) -> bool {
    state.is_some_and(|state| {
        state.property(LIT) == Some("true") && block_in_tag(Some(state), CAMPFIRES)
    })
}

fn intersects_post(state: BlockStateId) -> bool {
    state.collision_shape().iter().any(|shape| {
        shape.min_x < POST_MAX
            && shape.max_x > POST_MIN
            && shape.min_z < POST_MAX
            && shape.max_z > POST_MIN
            && shape.max_y > 0.0
            && shape.min_y < 1.0
    })
}

fn prevents_bee_spawns(tool: Option<&ItemStack>) -> bool {
    let Some(members) = rspigot_data::tags::tag(ENCHANTMENT_REGISTRY, PREVENTS_BEE_SPAWNS) else {
        return false;
    };
    tool.is_some_and(|stack| {
        stack.enchantments().iter().any(|(name, level)| {
            *level > 0
                && rspigot_data::registries::synced_index(ENCHANTMENT_REGISTRY, name)
                    .is_some_and(|id| members.binary_search(&id).is_ok())
        })
    })
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn hive_at(&self, pos: BlockPos) -> Option<Beehive> {
        let state = self.world.chunks.block(pos).filter(|s| is_hive_block(*s))?;
        let _ = state;
        let data = self
            .world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .map(|entity| Beehive::read(&entity.data));
        Some(data.unwrap_or_default())
    }

    fn store_hive(&mut self, pos: BlockPos, hive: &Beehive) {
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        let (kind, mut data) = chunk.block_entity(pos).map_or_else(
            || (Identifier::minecraft(HIVE_ENTITY), Compound::new()),
            |existing| (existing.kind.clone(), existing.data.clone()),
        );
        hive.write(&mut data);
        chunk.set_block_entity(BlockEntity { pos, kind, data });
    }

    pub(crate) fn ensure_beehive_entity(&mut self, pos: BlockPos, state: BlockStateId) {
        if !is_hive_block(state) {
            return;
        }
        let present = self
            .world
            .chunks
            .get(pos.chunk())
            .is_some_and(|chunk| chunk.block_entity(pos).is_some());
        if !present {
            self.store_hive(pos, &Beehive::default());
        }
    }

    pub(crate) fn fire_near_hive(&self, pos: BlockPos) -> bool {
        for dx in -FIRE_REACH..=FIRE_REACH {
            for dy in -FIRE_REACH..=FIRE_REACH {
                for dz in -FIRE_REACH..=FIRE_REACH {
                    if self
                        .world
                        .chunks
                        .block(pos.offset(dx, dy, dz))
                        .and_then(BlockStateId::kind)
                        == Some(BlockKind::Fire)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub(crate) fn is_smokey_pos(&self, pos: BlockPos) -> bool {
        for depth in 1..=SMOKE_DEPTH {
            let below = pos.offset(0, -depth, 0);
            let state = self.world.chunks.block(below);
            if is_lit_campfire(state) {
                return true;
            }
            if state.is_some_and(intersects_post) {
                return is_lit_campfire(self.world.chunks.block(below.offset(0, -1, 0)));
            }
        }
        false
    }

    pub(crate) fn tick_beehives(&mut self) {
        let mut found: Vec<BlockPos> = Vec::new();
        for chunk_pos in self.world.chunks.loaded_positions() {
            let Some(chunk) = self.world.chunks.get(chunk_pos) else {
                continue;
            };
            found.extend(
                chunk
                    .block_entities
                    .iter()
                    .filter(|entity| entity.kind.as_str() == "minecraft:beehive")
                    .map(|entity| entity.pos),
            );
        }
        for pos in found {
            self.tick_beehive(pos);
        }
    }

    fn tick_beehive(&mut self, pos: BlockPos) {
        let Some(state) = self.world.chunks.block(pos).filter(|s| is_hive_block(*s)) else {
            return;
        };
        let Some(mut hive) = self.hive_at(pos) else {
            return;
        };
        if hive.is_empty() {
            return;
        }
        let due = hive.tick_occupants();
        let mut released = Vec::new();
        for index in due {
            let occupant = hive.bees[index].clone();
            let status = occupant.status();
            if self.release_occupant(pos, state, &occupant, status, hive.flower_pos, false, None) {
                released.push(index);
            } else {
                hive.defer(index);
            }
        }
        hive.remove_all(&released);
        self.store_hive(pos, &hive);
        if !hive.is_empty() && self.rng.random::<f64>() < beehive::WORK_SOUND_CHANCE {
            let at = Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y),
                f64::from(pos.z) + 0.5,
            );
            self.play_sound(None, at, WORK_SOUND, SoundSource::Blocks, 1.0, 1.0);
        }
    }

    pub(crate) fn bee_enters_hive(&mut self, id: EntityId, pos: BlockPos) {
        let Some(view) = self.entity_view(id) else {
            return;
        };
        if !self.hive_at(pos).is_some_and(|hive| hive.has_space()) {
            return;
        }
        if self
            .decisions
            .anywhere(|then| matches!(then, Continuation::EnterHive { bee, .. } if *bee == id))
        {
            return;
        }
        let lane: Lane = self.lane_for(None, id);
        self.fire(lane, Continuation::EnterHive { bee: id, pos }, move |_| {
            Some(EventPayload::EntityEnterBlock(EntityEnterBlock {
                block: pos,
                entity_type: view.kind.clone(),
                entity: view,
            }))
        });
    }

    pub(crate) fn finish_enter_hive(&mut self, id: EntityId, pos: BlockPos, cancelled: bool) {
        if cancelled {
            if let Some((_, state)) = self.bee_parts(id) {
                state.stay_out_countdown = bee::STAY_OUT_AFTER_REFUSAL;
            }
            return;
        }
        let Some(mut hive) = self.hive_at(pos).filter(Beehive::has_space) else {
            return;
        };
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(tag) = save::save_entity(entity, None) else {
            return;
        };
        let flower = match entity
            .living()
            .and_then(|l| l.mob.as_ref())
            .map(|m| m.species)
        {
            Some(Species::Bee(state)) => state.flower_pos,
            _ => None,
        };
        hive.store(Occupant::of(&tag));
        let mut random = ServerRandom(&mut self.rng);
        hive.adopt_flower(flower, &mut random);
        self.store_hive(pos, &hive);
        let at = Vec3::new(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
        self.play_sound(None, at, ENTER_SOUND, SoundSource::Blocks, 1.0, 1.0);
        let state = self.world.chunks.block(pos).unwrap_or_default();
        self.block_game_event(
            GameEvent::BlockChange,
            pos,
            GameEventContext::entity_in(Some(id), state),
        );
        self.remove_entity(
            id,
            RemovalReason::Discarded,
            Some(EntityRemoveCause::EnterBlock),
        );
        self.hive_changed(pos);
    }

    pub(crate) fn hive_changed(&mut self, pos: BlockPos) {
        if self.fire_near_hive(pos)
            && let Some(state) = self.world.chunks.block(pos).filter(|s| is_hive_block(*s))
        {
            self.empty_hive(pos, state, None, ReleaseStatus::Emergency);
        }
    }

    pub(crate) fn empty_hive(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        player: Option<PlayerId>,
        status: ReleaseStatus,
    ) {
        let Some(mut hive) = self.hive_at(pos) else {
            return;
        };
        if hive.is_empty() {
            return;
        }
        let mut released = Vec::new();
        for (index, occupant) in hive.bees.clone().iter().enumerate() {
            if self.release_occupant(pos, state, occupant, status, hive.flower_pos, false, player) {
                released.push(index);
            }
        }
        hive.remove_all(&released);
        self.store_hive(pos, &hive);
        if !released.is_empty() {
            self.hive_changed(pos);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn release_occupant(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        occupant: &Occupant,
        status: ReleaseStatus,
        flower: Option<BlockPos>,
        force: bool,
        angered_by: Option<PlayerId>,
    ) -> bool {
        if !force && self.bees_stay_in_hive_at(pos) && status != ReleaseStatus::Emergency {
            return false;
        }
        let (step_x, step_z) = facing_step(state);
        let front = pos.offset(step_x, 0, step_z);
        let blocked = self
            .world
            .chunks
            .block(front)
            .is_some_and(|state| !state.collision_shape().is_empty());
        if blocked && status != ReleaseStatus::Emergency {
            return false;
        }
        let id = self.allocate_entity_id();
        let uuid = self.random_uuid();
        let Ok(mut entity) = save::load_entity(
            &occupant.entity_tag().with("id", Kind::Bee.name()),
            id,
            uuid,
        ) else {
            return false;
        };
        if entity.base.kind != Kind::Bee || !inhabits_hives(entity.base.kind) {
            return false;
        }
        entity.base.set_no_gravity(true);
        let ticks = occupant.ticks_in_hive;
        let mut random = ServerRandom(&mut self.rng);
        let flower_kept = flower.is_some() && random.next_float() > beehive::FLOWER_KEEP_ODDS;
        if let EntityState::Living(living) = &mut entity.state
            && let Some(mob) = living.mob.as_mut()
        {
            if let Some(ageable) = mob.ageable.as_mut()
                && !ageable::is_age_locked(&entity.base)
            {
                let age = beehive::age_after_stay(ageable.age, false, ticks);
                ageable::set_age(&mut entity.base, ageable, age);
                if let Some(love) = ageable.love.as_mut() {
                    love.ticks = beehive::love_after_stay(love.ticks, ticks);
                }
            }
            if let Species::Bee(state) = &mut mob.species {
                state.hive_pos = Some(pos);
                if flower_kept && state.flower_pos.is_none() {
                    state.flower_pos = flower;
                }
                if status == ReleaseStatus::HoneyDelivered {
                    bee::drop_off_nectar(&mut entity.base, state);
                }
            }
        }
        let width = entity.base.dimensions.width;
        let height = entity.base.dimensions.height;
        let offset = beehive::exit_offset(blocked, width);
        let position = Vec3::new(
            f64::from(pos.x) + 0.5 + offset * f64::from(step_x),
            f64::from(pos.y) + 0.5 - f64::from(height) / 2.0,
            f64::from(pos.z) + 0.5 + offset * f64::from(step_z),
        );
        let (yaw, pitch) = (entity.base.yaw, entity.base.pitch);
        entity.base.snap_to(position, yaw, pitch);
        let release = HiveRelease {
            pos,
            occupant: occupant.clone(),
            status,
            angered_by,
        };
        self.spawn_entity(
            entity,
            Birth::of(CreatureSpawnReason::Beehive).then(Spawned::Hive(Box::new(release))),
        );
        true
    }

    pub(crate) fn finish_hive_release(&mut self, release: HiveRelease, placed: Option<EntityId>) {
        let HiveRelease {
            pos,
            mut occupant,
            status,
            angered_by,
        } = release;
        let Some(bee_id) = placed else {
            if let Some(mut hive) = self.hive_at(pos).filter(Beehive::has_space) {
                occupant.ticks_in_hive = occupant.min_ticks_in_hive / 2;
                hive.store(occupant);
                self.store_hive(pos, &hive);
            }
            return;
        };
        let state = self.world.chunks.block(pos).unwrap_or_default();
        if status == ReleaseStatus::HoneyDelivered
            && is_hive_block(state)
            && state.property(HONEY_LEVEL).is_some()
        {
            let mut random = ServerRandom(&mut self.rng);
            if let Some(level) = beehive::honey_after_delivery(honey_level(state), &mut random)
                && let Some(next) = state.with_property(HONEY_LEVEL, &level.to_string())
            {
                self.set_block(pos, next);
            }
        }
        let at = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y) + 0.5,
            f64::from(pos.z) + 0.5,
        );
        self.play_sound(None, at, EXIT_SOUND, SoundSource::Blocks, 1.0, 1.0);
        let state = self.world.chunks.block(pos).unwrap_or_default();
        self.block_game_event(
            GameEvent::BlockChange,
            pos,
            GameEventContext::entity_in(Some(bee_id), state),
        );
        if let Some(player) = angered_by {
            self.anger_released_bee(bee_id, pos, player);
        }
    }

    fn anger_released_bee(&mut self, bee_id: EntityId, pos: BlockPos, player: PlayerId) {
        let Some(found) = self.players.get(&player) else {
            return;
        };
        let target = found.entity_id;
        let player_position = found.position;
        let Some(bee) = self.entities.store.get(bee_id) else {
            return;
        };
        if player_position.distance_squared(bee.base.position) > beehive::ANGRY_PLAYER_DISTANCE_SQR
        {
            return;
        }
        if self.is_smokey_pos(pos) {
            if let Some((_, state)) = self.bee_parts(bee_id) {
                state.stay_out_countdown = beehive::SEDATED_STAY_OUT_TICKS;
            }
        } else if let Some(view) = self.entity_view(bee_id) {
            self.request_target(view, target, TargetReason::ClosestPlayer);
        }
    }

    fn bees_stay_in_hive_at(&self, pos: BlockPos) -> bool {
        self.bees_stay_in_hive(pos)
    }

    pub(crate) fn bees_stay_in_hive(&self, _pos: BlockPos) -> bool {
        bees_stay_in_hive(
            self.world.level.raining,
            self.world.dimension.key.as_str(),
            self.world.level.day_time,
        )
    }

    pub(crate) fn anger_nearby_bees(&mut self, pos: BlockPos) {
        let (rx, ry, rz) = beehive::ANGER_RADIUS;
        let area = Aabb::new(
            f64::from(pos.x) - rx,
            f64::from(pos.y) - ry,
            f64::from(pos.z) - rz,
            f64::from(pos.x) + 1.0 + rx,
            f64::from(pos.y) + 1.0 + ry,
            f64::from(pos.z) + 1.0 + rz,
        );
        let bees: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|entity| {
                entity.base.kind == Kind::Bee && entity.base.bounding_box().intersects(&area)
            })
            .map(|entity| entity.base.id)
            .collect();
        if bees.is_empty() {
            return;
        }
        let players: Vec<EntityId> = self
            .players
            .values()
            .filter(|player| {
                Aabb::of_size(player.position, 0.6, player.height() as f32).intersects(&area)
            })
            .map(|player| player.entity_id)
            .collect();
        if players.is_empty() {
            return;
        }
        for bee_id in bees {
            if self.mob_target(bee_id).is_some() {
                continue;
            }
            let pick = self.rng.random_range(0..players.len());
            if let Some(view) = self.entity_view(bee_id) {
                self.request_target(view, players[pick], TargetReason::ClosestPlayer);
            }
        }
    }

    pub(crate) fn hive_snapshot(&self, pos: BlockPos) -> Option<Beehive> {
        self.hive_at(pos)
    }

    pub(crate) fn hive_broken(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        mut hive: Beehive,
        player: Option<PlayerId>,
        tool: Option<&ItemStack>,
    ) {
        if !is_hive_block(state) || prevents_bee_spawns(tool) {
            return;
        }
        let bees: Vec<Occupant> = std::mem::take(&mut hive.bees);
        for occupant in &bees {
            self.release_occupant(
                pos,
                state,
                occupant,
                ReleaseStatus::Emergency,
                hive.flower_pos,
                false,
                player,
            );
        }
        self.anger_nearby_bees(pos);
    }

    pub(crate) fn hive_exploded(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        hive: Option<Beehive>,
        harmful_source: bool,
    ) {
        if !is_hive_block(state) {
            return;
        }
        self.anger_nearby_bees(pos);
        if !harmful_source {
            return;
        }
        let Some(hive) = hive else {
            return;
        };
        for occupant in &hive.bees {
            self.release_occupant(
                pos,
                state,
                occupant,
                ReleaseStatus::Emergency,
                hive.flower_pos,
                false,
                None,
            );
        }
    }

    pub(crate) fn hive_fire_placed(&mut self, pos: BlockPos) {
        for (dx, dy, dz) in [
            (1, 0, 0),
            (-1, 0, 0),
            (0, 1, 0),
            (0, -1, 0),
            (0, 0, 1),
            (0, 0, -1),
        ] {
            let hive = pos.offset(dx, dy, dz);
            if let Some(state) = self.world.chunks.block(hive).filter(|s| is_hive_block(*s)) {
                self.empty_hive(hive, state, None, ReleaseStatus::Emergency);
            }
        }
    }

    pub(crate) fn use_beehive(
        &mut self,
        id: PlayerId,
        hand: Hand,
        held: Option<&ItemStack>,
        pos: BlockPos,
        clicked: BlockStateId,
        sequence: i32,
    ) -> bool {
        if !is_hive_block(clicked) || honey_level(clicked) < FULL_HONEY {
            return false;
        }
        let Some(stack) = held else {
            return false;
        };
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let at = player.position;
        let entity_id = player.entity_id;
        let used = match stack.item.kind() {
            Some(ItemKind::Shears) => {
                self.drop_honeycomb(pos);
                self.play_sound(None, at, SHEAR_SOUND, SoundSource::Blocks, 1.0, 1.0);
                self.hurt_held(id, hand, 1);
                self.block_game_event(
                    GameEvent::Shear,
                    pos,
                    GameEventContext::entity_in(Some(entity_id), clicked),
                );
                true
            }
            Some(ItemKind::GlassBottle) => {
                self.shrink_held(id, hand, 1);
                self.play_sound(
                    None,
                    at,
                    beehive::BOTTLE_FILL_SOUND,
                    SoundSource::Blocks,
                    1.0,
                    1.0,
                );
                let honey = ItemStack::new(ItemKind::HoneyBottle.id(), std::num::NonZeroU8::MIN);
                if self.held(id, hand).is_none() {
                    self.set_held(id, hand, Some(honey));
                } else {
                    self.give_or_drop(id, honey);
                }
                self.block_game_event(
                    GameEvent::FluidPickup,
                    pos,
                    GameEventContext::entity_in(Some(entity_id), clicked),
                );
                true
            }
            _ => false,
        };
        if !used {
            return false;
        }
        if self.is_smokey_pos(pos) {
            self.reset_honey_level(pos, clicked);
        } else {
            let has_bees = self.hive_at(pos).is_some_and(|hive| !hive.is_empty());
            if has_bees {
                self.anger_nearby_bees(pos);
            }
            self.reset_honey_level(pos, clicked);
            self.empty_hive(pos, clicked, Some(id), ReleaseStatus::Emergency);
        }
        self.refresh_hands(id);
        self.player_swing(id, hand, false);
        self.ack_block(id, sequence);
        true
    }

    fn reset_honey_level(&mut self, pos: BlockPos, state: BlockStateId) {
        if let Some(next) = state.with_property(HONEY_LEVEL, "0") {
            self.set_block(pos, next);
        }
    }

    fn drop_honeycomb(&mut self, pos: BlockPos) {
        let Some(table) = rspigot_data::loot::table(HARVEST_TABLE) else {
            return;
        };
        let context = crate::loot::LootContext::default();
        for stack in crate::loot::table_drops(table, &context, &mut self.rng) {
            self.spawn_item(pos, stack);
        }
    }

    pub(crate) fn bee_parts(
        &mut self,
        id: EntityId,
    ) -> Option<(&mut rspigot_domain::entity::EntityBase, &mut bee::BeeState)> {
        let Entity {
            base,
            state: EntityState::Living(living),
            ..
        } = self.entities.store.get_mut(id)?
        else {
            return None;
        };
        match living.mob.as_mut().map(|mob| &mut mob.species) {
            Some(Species::Bee(state)) => Some((base, state)),
            _ => None,
        }
    }
}

fn inhabits_hives(kind: Kind) -> bool {
    rspigot_data::tags::tag("minecraft:entity_type", HIVE_INHABITORS)
        .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
}
