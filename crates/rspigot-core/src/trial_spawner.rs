use std::num::NonZeroU8;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::effect::{EffectDuration, EffectInstance, EffectKind};
use rspigot_domain::entity::Entity;
use rspigot_domain::entity::raycast::clip_blocks;
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::random::RandomSource;
use rspigot_domain::spawn::rules::{check_spawn_obstruction, check_spawn_rules, no_collision};
use rspigot_domain::spawn::{SpawnLevel, SpawnReason, spawn_aabb};
use rspigot_domain::spawner::SpawnData;
use rspigot_domain::trial_spawner::{self as trial, Data, FullConfig, State};
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Identifier, Rotation, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_nbt::Compound;
use uuid::Uuid;

use crate::events::ClientEvent;
use crate::game_events::GameEventContext;
use crate::items::PICKUP_DELAY;
use crate::loot::{self, LootContext};
use crate::mobs::ServerRandom;
use crate::plugins::events::{
    BlockDispenseLoot, CreatureSpawnReason, EntityRemoveCause, TrialSpawnerSpawn,
};
use crate::plugins::steps::{Birth, Continuation, Spawned};
use crate::plugins::{EventPayload, Lane, TrialSpawnerData, spec_of};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::spawner::{fixed_position, middle};
use rspigot_domain::entity::RemovalReason;

const PLAYER_HEIGHT: f64 = 1.8;
const STATE: &str = "trial_spawner_state";
const OMINOUS: &str = "ominous";
const BLOCK_ENTITY: &str = "minecraft:trial_spawner";

pub(crate) fn is_trial_spawner(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::TrialSpawner)
}

pub(crate) fn state_of(block: BlockStateId) -> State {
    block
        .property(STATE)
        .and_then(State::from_name)
        .unwrap_or(State::Inactive)
}

pub(crate) fn is_ominous(block: BlockStateId) -> bool {
    block.property(OMINOUS) == Some("true")
}

pub(crate) fn as_long(pos: BlockPos) -> i64 {
    ((i64::from(pos.x) & 0x3FF_FFFF) << 38)
        | ((i64::from(pos.z) & 0x3FF_FFFF) << 12)
        | (i64::from(pos.y) & 0xFFF)
}

#[derive(Debug)]
pub(crate) struct TrialRun {
    pos: BlockPos,
    full: FullConfig,
    data: Data,
    ominous: bool,
    dirty: bool,
}

#[derive(Debug)]
pub(crate) struct TrialSpawnStep {
    pos: BlockPos,
    entity: Entity,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Dispensing {
    Trial,
    Vault,
}

#[derive(Debug)]
pub(crate) struct LootDispenseStep {
    pub(crate) pos: BlockPos,
    pub(crate) player: Option<PlayerId>,
    pub(crate) stacks: Vec<ItemStack>,
    pub(crate) then: Dispensing,
}

struct Target {
    position: Vec3,
    height: f64,
}

impl TrialSpawnStep {
    pub(crate) fn entity(&self) -> &Entity {
        &self.entity
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn ensure_trial_spawner_entity(&mut self, pos: BlockPos, state: BlockStateId) {
        if !is_trial_spawner(state) {
            return;
        }
        let present = self
            .world
            .chunks
            .get(pos.chunk())
            .is_some_and(|chunk| chunk.block_entity(pos).is_some());
        if !present {
            self.store_trial(&TrialRun {
                pos,
                full: FullConfig::default(),
                data: Data::default(),
                ominous: false,
                dirty: false,
            });
        }
    }

    pub(crate) fn trial_at(&self, pos: BlockPos) -> Option<(FullConfig, Data)> {
        self.world
            .chunks
            .block(pos)
            .filter(|state| is_trial_spawner(*state))?;
        let entity = self.world.chunks.get(pos.chunk())?.block_entity(pos)?;
        Some((FullConfig::read(&entity.data), Data::read(&entity.data)))
    }

    fn store_trial(&mut self, run: &TrialRun) {
        let Some(chunk) = self.world.chunks.chunk_mut(run.pos.chunk()) else {
            return;
        };
        let (kind, mut data) = chunk.block_entity(run.pos).map_or_else(
            || (Identifier::minecraft(trial::ENTITY), Compound::new()),
            |existing| (existing.kind.clone(), existing.data.clone()),
        );
        run.full.write(&mut data);
        run.data.write(&mut data);
        chunk.set_block_entity(BlockEntity {
            pos: run.pos,
            kind,
            data,
        });
    }

    fn send_trial_update(&mut self, pos: BlockPos) {
        let Some(block) = self.world.chunks.block(pos) else {
            return;
        };
        let Some((_, data)) = self.trial_at(pos) else {
            return;
        };
        self.push(
            Audience::ChunkViewers(pos.chunk()),
            ClientEvent::BlockEntityUpdate {
                pos,
                kind: BLOCK_ENTITY,
                data: data.update_tag(state_of(block)),
            },
        );
    }

    pub(crate) fn tick_trial_spawners(&mut self) {
        let mut found: Vec<BlockPos> = Vec::new();
        for chunk_pos in self
            .world
            .chunks
            .chunks_with_block_entities(|kind| kind.path() == trial::ENTITY)
        {
            let Some(chunk) = self.world.chunks.get(chunk_pos) else {
                continue;
            };
            let mut here = chunk
                .block_entities
                .iter()
                .filter(|entity| entity.kind.path() == trial::ENTITY)
                .map(|entity| entity.pos)
                .peekable();
            if here.peek().is_some() && self.entity_ticking(chunk_pos) {
                found.extend(here);
            }
        }
        for pos in found {
            self.tick_trial_spawner(pos);
        }
    }

    fn tick_trial_spawner(&mut self, pos: BlockPos) {
        let Some(block) = self
            .world
            .chunks
            .block(pos)
            .filter(|state| is_trial_spawner(*state))
        else {
            return;
        };
        let Some((full, data)) = self.trial_at(pos) else {
            return;
        };
        let state = state_of(block);
        let mut run = TrialRun {
            pos,
            full,
            data,
            ominous: is_ominous(block),
            dirty: false,
        };
        let now = self.world.level.game_time;
        let active = run.full.active(run.ominous);
        let before = run.data.current_mobs.len();
        let tracked: Vec<Uuid> = run
            .data
            .current_mobs
            .iter()
            .copied()
            .filter(|mob| self.trial_mob_tracked(*mob, pos))
            .collect();
        run.data.current_mobs = tracked;
        if run.data.current_mobs.len() != before {
            run.data.lost_track_of_a_mob(now, &active);
        }
        let next = self.trial_next_state(&mut run, state, &active);
        self.store_trial(&run);
        if run.dirty {
            self.send_trial_update(pos);
        }
        if next != state {
            self.set_trial_state(pos, next);
        }
    }

    fn trial_mob_tracked(&self, mob: Uuid, pos: BlockPos) -> bool {
        let Some(id) = self.entities.store.by_uuid(mob) else {
            return false;
        };
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        if entity.base.is_removed() || entity.living().is_some_and(|living| living.dead) {
            return false;
        }
        let block = entity.base.position.block();
        let (dx, dy, dz) = (block.x - pos.x, block.y - pos.y, block.z - pos.z);
        dx * dx + dy * dy + dz * dz <= trial::MOB_TRACKING_DISTANCE_SQR
    }

    fn set_trial_state(&mut self, pos: BlockPos, next: State) {
        let Some(block) = self.world.chunks.block(pos) else {
            return;
        };
        if let Some(changed) = block.with_property(STATE, next.name()) {
            self.set_block(pos, changed);
        }
    }

    fn set_trial_ominous(&mut self, pos: BlockPos, ominous: bool) {
        let Some(block) = self.world.chunks.block(pos) else {
            return;
        };
        let value = if ominous { "true" } else { "false" };
        if let Some(changed) = block.with_property(OMINOUS, value) {
            self.set_block(pos, changed);
        }
    }

    fn trial_can_spawn(&self) -> bool {
        self.world.difficulty != Difficulty::Peaceful && self.world.level.spawn_mobs
    }

    fn trial_next_state(
        &mut self,
        run: &mut TrialRun,
        state: State,
        active: &trial::Config,
    ) -> State {
        let pos = run.pos;
        let now = self.world.level.game_time;
        let cooldown = run.full.target_cooldown_length;
        match state {
            State::Inactive => {
                run.dirty |= run
                    .data
                    .ensure_next_spawn(active, &mut ServerRandom(&mut self.rng));
                let named = run
                    .data
                    .next_spawn_data
                    .as_ref()
                    .and_then(SpawnData::entity_id)
                    .is_some();
                if named {
                    State::WaitingForPlayers
                } else {
                    state
                }
            }
            State::WaitingForPlayers => {
                if !self.trial_can_spawn() {
                    run.data.reset_statistics();
                    return state;
                }
                let created = run.data.next_spawn_data.is_none();
                let has_mob = run
                    .data
                    .has_mob_to_spawn(active, &mut ServerRandom(&mut self.rng));
                run.dirty |= created;
                if !has_mob {
                    return State::Inactive;
                }
                self.trial_detect_players(run, state);
                if run.data.detected_players.is_empty() {
                    state
                } else {
                    State::Active
                }
            }
            State::Active => {
                if !self.trial_can_spawn() {
                    run.data.reset_statistics();
                    return State::WaitingForPlayers;
                }
                let created = run.data.next_spawn_data.is_none();
                let has_mob = run
                    .data
                    .has_mob_to_spawn(active, &mut ServerRandom(&mut self.rng));
                run.dirty |= created;
                if !has_mob {
                    return State::Inactive;
                }
                let additional = run.data.count_additional_players();
                self.trial_detect_players(run, state);
                if run.ominous {
                    self.trial_spawn_ominous_item(run);
                }
                if run.data.has_finished_spawning_all_mobs(active, additional) {
                    if run.data.have_all_current_mobs_died() {
                        run.data.finish_spawning_wave(now, cooldown);
                        return State::WaitingForRewardEjection;
                    }
                } else if run.data.is_ready_to_spawn_next_mob(now, active, additional) {
                    self.trial_try_spawn(run);
                }
                state
            }
            State::WaitingForRewardEjection => {
                if run
                    .data
                    .is_ready_to_open_shutter(now, trial::SHUTTER_DELAY, cooldown)
                {
                    self.play_sound(
                        None,
                        middle(pos),
                        trial::OPEN_SHUTTER_SOUND,
                        SoundSource::Blocks,
                        1.0,
                        1.0,
                    );
                    return State::EjectingReward;
                }
                state
            }
            State::EjectingReward => {
                if !run
                    .data
                    .is_ready_to_eject_items(now, trial::EJECTION_INTERVAL, cooldown)
                {
                    return state;
                }
                if run.data.detected_players.is_empty() {
                    self.play_sound(
                        None,
                        middle(pos),
                        trial::CLOSE_SHUTTER_SOUND,
                        SoundSource::Blocks,
                        1.0,
                        1.0,
                    );
                    run.data.ejecting_loot_table = None;
                    return State::Cooldown;
                }
                if run.data.ejecting_loot_table.is_none() {
                    run.data.ejecting_loot_table = active
                        .pick_loot_table(&mut ServerRandom(&mut self.rng))
                        .cloned();
                }
                let table = run.data.ejecting_loot_table.clone();
                run.data.eject_next_player();
                if let Some(table) = table {
                    self.store_trial(run);
                    self.trial_eject_reward(pos, &table);
                    self.reload_trial_data(run);
                }
                state
            }
            State::Cooldown => {
                self.trial_detect_players(run, state);
                if run.data.detected_players.is_empty() {
                    if run.data.is_cooldown_finished(now) {
                        self.set_trial_ominous(pos, false);
                        run.ominous = false;
                        run.data.reset();
                        return State::WaitingForPlayers;
                    }
                } else {
                    run.data.restart_wave();
                    return State::Active;
                }
                state
            }
        }
    }

    fn trial_detect(
        &self,
        pos: BlockPos,
        range: i32,
        line_of_sight: bool,
        creative: bool,
    ) -> Vec<Uuid> {
        let center = middle(pos);
        let limit = i64::from(range) * i64::from(range);
        let view = &self.world.chunks;
        self.players
            .values()
            .filter(|player| {
                if player.vitals.removed || player.vitals.dead {
                    return false;
                }
                if player.game_mode == GameMode::Spectator
                    || (!creative && player.game_mode == GameMode::Creative)
                {
                    return false;
                }
                let at = player.position.block();
                let (dx, dy, dz) = (
                    i64::from(at.x - pos.x),
                    i64::from(at.y - pos.y),
                    i64::from(at.z - pos.z),
                );
                if dx * dx + dy * dy + dz * dz >= limit {
                    return false;
                }
                !line_of_sight
                    || clip_blocks(view, player.eye_position(), center)
                        .is_none_or(|hit| hit.pos == pos)
            })
            .map(|player| player.id.0)
            .collect()
    }

    fn trial_omen_candidate(&self, players: &[Uuid]) -> Option<(PlayerId, bool)> {
        let mut bad = None;
        for uuid in players {
            let id = PlayerId(*uuid);
            let Some(player) = self.players.get(&id) else {
                continue;
            };
            if player.effects.get(EffectKind::TrialOmen.id()).is_some() {
                return Some((id, false));
            }
            if player.effects.get(EffectKind::BadOmen.id()).is_some() {
                bad = Some((id, true));
            }
        }
        bad
    }

    fn trial_detect_players(&mut self, run: &mut TrialRun, state: State) {
        let pos = run.pos;
        let now = self.world.level.game_time;
        if (as_long(pos) + now) % trial::DETECT_INTERVAL != 0 {
            return;
        }
        if state == State::Cooldown && run.ominous {
            return;
        }
        let range = run.full.required_player_range;
        let seen = self.trial_detect(pos, range, true, false);
        let mut became_ominous = false;
        if !run.ominous
            && !seen.is_empty()
            && let Some((player, bad_omen)) = self.trial_omen_candidate(&seen)
        {
            if bad_omen {
                self.transform_bad_omen(player);
            }
            let eye = self
                .players
                .get(&player)
                .map_or_else(|| middle(pos), |p| p.eye_position());
            self.level_event(trial::BECOME_OMINOUS_EVENT, eye.block(), 0);
            self.trial_become_ominous(run);
            became_ominous = true;
        }
        if state == State::Cooldown && !became_ominous {
            return;
        }
        let found = if run.data.detected_players.is_empty() {
            seen
        } else {
            self.trial_detect(pos, range, false, false)
        };
        if run.data.detected(found, now) && !became_ominous {
            let event = if run.ominous {
                trial::DETECT_OMINOUS_PLAYER_EVENT
            } else {
                trial::DETECT_PLAYER_EVENT
            };
            let count = i32::try_from(run.data.detected_players.len()).unwrap_or(i32::MAX);
            self.level_event(event, pos, count);
        }
    }

    fn transform_bad_omen(&mut self, player: PlayerId) {
        let Some(amplifier) = self
            .players
            .get(&player)
            .and_then(|p| p.effects.get(EffectKind::BadOmen.id()))
            .map(|effect| effect.amplifier)
        else {
            return;
        };
        let ticks = (i32::from(amplifier) + 1) * trial::BAD_OMEN_TO_TRIAL_OMEN_TICKS;
        self.remove_effect_now(player, EffectKind::BadOmen.id());
        let omen = EffectInstance::new(
            EffectKind::TrialOmen.id(),
            EffectDuration::Ticks(ticks.cast_unsigned()),
            0,
            false,
        );
        self.apply_effect_now(player, omen);
    }

    fn trial_become_ominous(&mut self, run: &mut TrialRun) {
        let pos = run.pos;
        let now = self.world.level.game_time;
        self.set_trial_ominous(pos, true);
        self.level_event(trial::BECOME_OMINOUS_EVENT, pos, 1);
        run.ominous = true;
        let mobs = std::mem::take(&mut run.data.current_mobs);
        for mob in mobs {
            let Some(id) = self.entities.store.by_uuid(mob) else {
                continue;
            };
            if let Some(at) = self.entities.store.get(id).map(|e| e.base.position.block()) {
                self.level_event(trial::SPAWN_MOB_EVENT, at, trial::Flame::Normal.encode());
            }
            self.remove_entity(
                id,
                RemovalReason::Discarded,
                Some(EntityRemoveCause::Despawn),
            );
        }
        let ominous = run.full.ominous.resolve();
        run.data.reset_after_becoming_ominous(now, &ominous);
        run.dirty = true;
    }

    fn trial_try_spawn(&mut self, run: &mut TrialRun) {
        let pos = run.pos;
        let active = run.full.active(run.ominous);
        let Some(data) = run.data.next_spawn_data.clone() else {
            return;
        };
        let Some(entity) = self.trial_spawn_candidate(pos, &active, &data) else {
            return;
        };
        self.store_trial(run);
        self.offer_trial_spawn(pos, run.ominous, entity);
        self.reload_trial_data(run);
    }

    fn reload_trial_data(&self, run: &mut TrialRun) {
        if let Some((_, data)) = self.trial_at(run.pos) {
            run.data = data;
        }
    }

    fn trial_spawn_candidate(
        &mut self,
        pos: BlockPos,
        config: &trial::Config,
        data: &SpawnData,
    ) -> Option<Entity> {
        let kind = data.entity_id().and_then(Kind::from_name)?;
        let at = match fixed_position(&data.entity) {
            Some(fixed) => fixed,
            None => {
                let mut random = ServerRandom(&mut self.rng);
                let reach = f64::from(config.spawn_range);
                let x =
                    f64::from(pos.x) + (random.next_double() - random.next_double()) * reach + 0.5;
                let y = f64::from(pos.y + random.next_int(3) - 1);
                let z =
                    f64::from(pos.z) + (random.next_double() - random.next_double()) * reach + 0.5;
                Vec3::new(x, y, z)
            }
        };
        if !no_collision(self, &spawn_aabb(kind, at.x, at.y, at.z)) {
            return None;
        }
        let view = &self.world.chunks;
        let visible = clip_blocks(view, at, middle(pos)).is_none_or(|hit| hit.pos == pos);
        if !visible {
            return None;
        }
        let block = at.block();
        if !check_spawn_rules(self, kind, SpawnReason::TrialSpawner, block) {
            return None;
        }
        if let Some(limits) = data.light_limits() {
            let sky = self.sky_light(block) - self.sky_darken();
            if !limits.admits(self.block_light(block), sky) {
                return None;
            }
        }
        let nbt = (!data.only_names_the_entity()).then(|| data.entity.clone());
        let (mut entity, _) = self
            .summon_entity(kind, at, nbt, SpawnReason::TrialSpawner)
            .ok()?;
        self.equip_from_spawn_data(&mut entity, data);
        let yaw = ServerRandom(&mut self.rng).next_float() * 360.0;
        let position = entity.base.position;
        entity.base.snap_to(position, yaw, 0.0);
        if let Some(mob) = entity.living_mut().and_then(|living| living.mob.as_mut()) {
            mob.persistence_required = true;
        }
        if entity.living().is_some_and(|state| state.is_mob())
            && !check_spawn_obstruction(self, kind, &entity.base.bounding_box())
        {
            return None;
        }
        Some(entity)
    }

    fn equip_from_spawn_data(&mut self, entity: &mut Entity, data: &SpawnData) {
        let Some(equipment) = &data.equipment else {
            return;
        };
        let Some(table) = equipment
            .get_str("loot_table")
            .and_then(rspigot_data::loot::table)
        else {
            return;
        };
        let chance = equipment.get_f64("slot_drop_chances").map(|c| c as f32);
        let facts = crate::mobs::loot_facts_of(entity);
        let context = LootContext {
            this: Some(&facts),
            ..LootContext::default()
        };
        let stacks = loot::table_drops(table, &context, &mut self.rng);
        if let Some(living) = entity.living_mut() {
            rspigot_domain::entity::spawn::equip_from_table(living, stacks, chance);
        }
    }

    fn offer_trial_spawn(&mut self, pos: BlockPos, ominous: bool, entity: Entity) {
        let view = crate::plugins::spawns::view_of_entity(&entity);
        let location = self.location_of(
            entity.base.position,
            Rotation::new(entity.base.yaw, entity.base.pitch),
        );
        let entity_type = view.kind.clone();
        self.fire(
            Lane::World,
            Continuation::TrialSpawn(Box::new(TrialSpawnStep { pos, entity })),
            move |_| {
                Some(EventPayload::TrialSpawnerSpawn(TrialSpawnerSpawn {
                    entity: view,
                    entity_type,
                    location,
                    trial_spawner: TrialSpawnerData { pos, ominous },
                }))
            },
        );
    }

    pub(crate) fn finish_trial_spawn(&mut self, step: TrialSpawnStep, cancelled: bool) {
        if cancelled {
            return;
        }
        let TrialSpawnStep { pos, entity } = step;
        self.spawn_entity(
            entity,
            Birth::of(CreatureSpawnReason::TrialSpawner).then(Spawned::TrialSpawner(pos)),
        );
    }

    pub(crate) fn finish_trial_birth(&mut self, pos: BlockPos, placed: Option<EntityId>) {
        let Some(id) = placed else {
            return;
        };
        let Some((uuid, at)) = self
            .entities
            .store
            .get(id)
            .map(|entity| (entity.base.uuid, entity.base.position))
        else {
            return;
        };
        let Some(block) = self
            .world
            .chunks
            .block(pos)
            .filter(|s| is_trial_spawner(*s))
        else {
            return;
        };
        let flame = trial::Flame::of(is_ominous(block)).encode();
        self.level_event(trial::SPAWN_MOB_AT_EVENT, pos, flame);
        self.level_event(trial::SPAWN_MOB_EVENT, at.block(), flame);
        self.game_event(
            GameEvent::EntityPlace,
            middle(at.block()),
            GameEventContext::of(Some(id)),
        );
        let Some((full, mut data)) = self.trial_at(pos) else {
            return;
        };
        let now = self.world.level.game_time;
        let active = full.active(is_ominous(block));
        data.mob_spawned(uuid, now, &active, &mut ServerRandom(&mut self.rng));
        self.store_trial(&TrialRun {
            pos,
            full,
            data,
            ominous: is_ominous(block),
            dirty: false,
        });
        self.send_trial_update(pos);
    }

    fn trial_eject_reward(&mut self, pos: BlockPos, table: &str) {
        let Some(table) = rspigot_data::loot::table(table) else {
            return;
        };
        let context = LootContext::default();
        let stacks = loot::table_drops(table, &context, &mut self.rng);
        if stacks.is_empty() {
            return;
        }
        self.offer_loot_dispense(LootDispenseStep {
            pos,
            player: None,
            stacks,
            then: Dispensing::Trial,
        });
    }

    pub(crate) fn offer_loot_dispense(&mut self, step: LootDispenseStep) {
        let pos = step.pos;
        let player = step.player;
        let loot: Vec<_> = step.stacks.iter().map(spec_of).collect();
        self.fire(
            Lane::World,
            Continuation::DispenseLoot(Box::new(step)),
            move |server| {
                Some(EventPayload::BlockDispenseLoot(BlockDispenseLoot {
                    block: pos,
                    dispensed_loot: loot,
                    player: player.and_then(|id| server.snapshot_of(id)),
                }))
            },
        );
    }

    pub(crate) fn finish_loot_dispense(
        &mut self,
        step: LootDispenseStep,
        cancelled: bool,
        stacks: Vec<ItemStack>,
    ) {
        if cancelled {
            return;
        }
        match step.then {
            Dispensing::Trial => {
                for stack in stacks {
                    self.eject_item_above(step.pos, stack);
                }
                self.level_event(trial::EJECT_ITEM_EVENT, step.pos, 0);
            }
            Dispensing::Vault => self.vault_unlock(step.pos, step.player, stacks),
        }
    }

    pub(crate) fn eject_item_above(&mut self, pos: BlockPos, stack: ItemStack) {
        let base = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y),
            f64::from(pos.z) + 0.5,
        );
        let position = Vec3::new(base.x, base.y + 1.2 - 0.125, base.z);
        let spread = 0.0172275 * 2.0;
        let mut random = ServerRandom(&mut self.rng);
        let speed = random.next_double() * 0.1 + 0.2;
        let velocity = Vec3::new(
            random.triangle(0.0 * speed, spread),
            random.triangle(0.2, spread),
            random.triangle(0.0 * speed, spread),
        );
        self.spawn_item_entity(position, velocity, PICKUP_DELAY, stack);
    }

    fn trial_dispensing_items(&mut self, pos: BlockPos, table: &str) -> Vec<(ItemStack, i32)> {
        let Some(table) = rspigot_data::loot::table(table) else {
            return Vec::new();
        };
        let (x, y, z) = trial::low_resolution_key(pos.x, pos.y, pos.z);
        let seed = self
            .world
            .level
            .seed
            .wrapping_add(as_long(BlockPos::new(x, y, z)));
        let mut rng = StdRng::seed_from_u64(seed.cast_unsigned());
        loot::table_drops(table, &LootContext::default(), &mut rng)
            .into_iter()
            .map(|stack| {
                let weight = i32::from(stack.count.get());
                let mut single = stack;
                single.count = NonZeroU8::MIN;
                (single, weight)
            })
            .collect()
    }

    fn trial_spawn_ominous_item(&mut self, run: &mut TrialRun) {
        let pos = run.pos;
        let now = self.world.level.game_time;
        if run.data.cooldown_ends_at > now {
            return;
        }
        let table = run.full.ominous.resolve().items_to_drop_when_ominous;
        let items = self.trial_dispensing_items(pos, &table);
        let total: i32 = items.iter().map(|(_, weight)| *weight).sum();
        if total <= 0 {
            return;
        }
        let mut roll = self.rng.random_range(0..total);
        let Some(stack) = items.into_iter().find_map(|(stack, weight)| {
            roll -= weight;
            (roll < 0).then_some(stack)
        }) else {
            return;
        };
        let Some(position) = self.ominous_item_position(run) else {
            return;
        };
        self.spawn_ominous_item_spawner(position, stack);
        let pitch = (self.rng.random::<f32>() - self.rng.random::<f32>()) * 0.2 + 1.0;
        self.play_sound(
            None,
            position,
            trial::SPAWN_ITEM_BEGIN_SOUND,
            SoundSource::Blocks,
            1.0,
            pitch,
        );
        run.data.cooldown_ends_at = now + trial::ITEM_SPAWNER_INTERVAL;
    }

    fn ominous_item_position(&mut self, run: &TrialRun) -> Option<Vec3> {
        let center = middle(run.pos);
        let range = f64::from(run.full.required_player_range);
        let limit = range * range;
        let players: Vec<Target> = run
            .data
            .detected_players
            .iter()
            .filter_map(|uuid| self.players.get(&PlayerId(*uuid)))
            .filter(|player| {
                player.game_mode != GameMode::Creative
                    && player.game_mode != GameMode::Spectator
                    && !player.vitals.dead
                    && !player.vitals.removed
                    && player.position.distance_squared(center) <= limit
            })
            .map(|player| Target {
                position: player.position,
                height: PLAYER_HEIGHT,
            })
            .collect();
        if players.is_empty() {
            return None;
        }
        let mobs: Vec<Target> = run
            .data
            .current_mobs
            .iter()
            .filter_map(|uuid| self.entities.store.by_uuid(*uuid))
            .filter_map(|id| self.entities.store.get(id))
            .filter(|entity| {
                !entity.base.is_removed()
                    && !entity.living().is_some_and(|living| living.dead)
                    && entity.base.position.distance_squared(center) <= limit
            })
            .map(|entity| Target {
                position: entity.base.position,
                height: entity.base.bounding_box().max_y - entity.base.bounding_box().min_y,
            })
            .collect();
        let pool = if ServerRandom(&mut self.rng).next_boolean() {
            mobs
        } else {
            players
        };
        let chosen = match pool.len() {
            0 => return None,
            1 => &pool[0],
            len => &pool[self.rng.random_range(0..len)],
        };
        let lift = chosen.height + 2.0 + f64::from(ServerRandom(&mut self.rng).next_int(4));
        let top = Vec3::new(
            chosen.position.x,
            chosen.position.y + lift,
            chosen.position.z,
        );
        let view = &self.world.chunks;
        let stop =
            clip_blocks(view, chosen.position, top).map_or_else(|| top.block(), |hit| hit.pos);
        let below = stop.center();
        let spot = Vec3::new(below.x, below.y - 1.0, below.z);
        let empty = self
            .world
            .chunks
            .block(spot.block())
            .is_none_or(|state| state.collision_shape().is_empty());
        empty.then_some(spot)
    }

    pub(crate) fn use_egg_on_trial_spawner(&mut self, pos: BlockPos, kind: Kind) -> bool {
        let Some(block) = self
            .world
            .chunks
            .block(pos)
            .filter(|s| is_trial_spawner(*s))
        else {
            return false;
        };
        let Some((full, mut data)) = self.trial_at(pos) else {
            return false;
        };
        data.reset();
        let run = TrialRun {
            pos,
            full: full.overriding_entity(kind.name()),
            data,
            ominous: is_ominous(block),
            dirty: false,
        };
        self.store_trial(&run);
        self.set_trial_state(pos, State::Inactive);
        true
    }
}
