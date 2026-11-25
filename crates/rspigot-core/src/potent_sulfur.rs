use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
use rspigot_domain::entity::Aabb;
use rspigot_domain::entity::raycast::clip_blocks;
use rspigot_domain::potent_sulfur::{self as potent, GeyserRandom, PotentState, Surroundings};
use rspigot_domain::spike;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Identifier, Vec3};
use rspigot_entities::{EntityKind, SoundSource};
use rspigot_nbt::Compound;

use crate::mobs::ServerRandom;
use crate::plugins::events::EntityPotionEffectCause;
use crate::ports::ClientLink;
use crate::server::Server;

const ENTITY: &str = "minecraft:potent_sulfur";
const COUNTDOWN: &str = "countdown";
const NO_COUNTDOWN: i32 = -1;
const CAUSES_CONTINUOUS: &str = "minecraft:causes_continuous_geyser_eruptions";
const CAUSES_PERIODIC: &str = "minecraft:causes_periodic_geyser_eruptions";

fn block_in_tag(state: BlockStateId, tag: &str) -> bool {
    state.kind().is_some_and(|kind| {
        rspigot_data::tags::tag("minecraft:block", tag)
            .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
    })
}

fn is_source_or_dry(state: BlockStateId) -> bool {
    match state.kind() {
        Some(BlockKind::Water | BlockKind::Lava) => state.property("level") == Some("0"),
        _ => true,
    }
}

fn is_water_source_at(state: BlockStateId) -> bool {
    match state.kind() {
        Some(BlockKind::Water) => state.property("level") == Some("0"),
        Some(_) => state.flags().contains(rspigot_blocks::StateFlags::WATER),
        None => false,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn ensure_potent_sulfur_entity(&mut self, pos: BlockPos, state: BlockStateId) {
        if state.kind() != Some(BlockKind::PotentSulfur) {
            return;
        }
        if let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk())
            && chunk.block_entity(pos).is_none()
        {
            let mut data = Compound::new();
            data.insert(COUNTDOWN, NO_COUNTDOWN);
            chunk.set_block_entity(BlockEntity {
                pos,
                kind: Identifier::minecraft("potent_sulfur"),
                data,
            });
        }
        self.potent_sulfur_refresh(pos);
    }

    pub(crate) fn potent_sulfur_neighbour_changed(&mut self, pos: BlockPos) {
        self.potent_sulfur_refresh(pos);
    }

    fn potent_surroundings(&self, pos: BlockPos) -> Surroundings {
        let above = self.block_or_air(pos.offset(0, 1, 0));
        let below = self.block_or_air(pos.offset(0, -1, 0));
        Surroundings {
            water_above: is_water_source_at(above),
            below_continuous: block_in_tag(below, CAUSES_CONTINUOUS),
            below_periodic: block_in_tag(below, CAUSES_PERIODIC),
            below_fluid_ok: is_source_or_dry(below),
        }
    }

    fn potent_countdown(&self, pos: BlockPos) -> i32 {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .and_then(|entity| entity.data.get_i32(COUNTDOWN))
            .unwrap_or(NO_COUNTDOWN)
    }

    fn set_potent_countdown(&mut self, pos: BlockPos, countdown: i32) {
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        let mut data = Compound::new();
        data.insert(COUNTDOWN, countdown);
        chunk.set_block_entity(BlockEntity {
            pos,
            kind: Identifier::minecraft("potent_sulfur"),
            data,
        });
    }

    fn set_potent_state(&mut self, pos: BlockPos, state: PotentState) {
        let next = potent::with_state(state);
        if self.block_or_air(pos) == next {
            return;
        }
        self.set_block(pos, next);
        self.update_neighbour_shapes(pos);
        if state.is_erupting() {
            let sound = if state == PotentState::Continuous {
                potent::CONTINUOUS_START_SOUND
            } else {
                potent::ERUPTION_START_SOUND
            };
            self.play_sound(None, pos.center(), sound, SoundSource::Blocks, 1.0, 1.0);
        }
    }

    fn potent_sulfur_refresh(&mut self, pos: BlockPos) {
        let Some(current) = potent::state_of(self.block_or_air(pos)) else {
            return;
        };
        let valid = potent::valid_state(current, self.potent_surroundings(pos));
        if valid.reset_countdown {
            self.set_potent_countdown(pos, NO_COUNTDOWN);
        }
        self.set_potent_state(pos, valid.state);
    }

    pub(crate) fn tick_potent_sulfur(&mut self) {
        let mut found: Vec<BlockPos> = Vec::new();
        for chunk_pos in self
            .world
            .chunks
            .chunks_with_block_entities(|kind| kind.as_str() == ENTITY)
        {
            let Some(chunk) = self.world.chunks.get(chunk_pos) else {
                continue;
            };
            found.extend(
                chunk
                    .block_entities
                    .iter()
                    .filter(|entity| entity.kind.as_str() == ENTITY)
                    .map(|entity| entity.pos),
            );
        }
        for pos in found {
            let Some(state) = potent::state_of(self.block_or_air(pos)) else {
                continue;
            };
            match state {
                PotentState::Dry => {}
                PotentState::Wet => self.potent_nausea(pos),
                PotentState::Dormant => {
                    self.potent_countdown_tick(pos, state);
                    self.potent_nausea(pos);
                }
                PotentState::Erupting => {
                    self.potent_launch(pos);
                    self.potent_countdown_tick(pos, state);
                }
                PotentState::Continuous => self.potent_launch(pos),
            }
        }
    }

    fn gas_source(&self, pos: BlockPos) -> Option<BlockPos> {
        let block = |at: BlockPos| self.block_or_air(at);
        potent::find_gas_source(&block, pos)
    }

    fn potent_countdown_tick(&mut self, pos: BlockPos, state: PotentState) {
        let now = self.world.level.game_time;
        if now % potent::COUNTDOWN_INTERVAL_TICKS != 0 {
            return;
        }
        let Some(source) = self.gas_source(pos) else {
            return;
        };
        let mut countdown = self.potent_countdown(pos);
        if countdown < 1 {
            let height = potent::geyser_height(pos, source);
            let mut random = GeyserRandom::at(self.world.level.seed, pos);
            countdown = potent::next_countdown(state, height, &mut random);
        }
        if countdown > 0 {
            countdown -= 1;
        }
        if countdown == 0 {
            let next = if state == PotentState::Dormant {
                PotentState::Erupting
            } else {
                PotentState::Dormant
            };
            self.set_potent_countdown(pos, countdown);
            self.set_potent_state(pos, next);
            return;
        }
        self.set_potent_countdown(pos, countdown);
    }

    fn nearby_living(&self, area: &Aabb) -> Vec<EntityId> {
        let mut found: Vec<EntityId> = Vec::new();
        for player in self.players.values() {
            if player.vitals.is_dead()
                || player.game_mode == rspigot_domain::player::GameMode::Spectator
            {
                continue;
            }
            if self
                .entity_box(player.entity_id)
                .is_some_and(|(_, _, bounds)| bounds.intersects(area))
            {
                found.push(player.entity_id);
            }
        }
        for entity in self.entities.store.iter() {
            if entity.base.is_removed()
                || entity.living().is_none()
                || !self.living_entity_alive(entity.base.id)
            {
                continue;
            }
            if entity.base.bounding_box().intersects(area) {
                found.push(entity.base.id);
            }
        }
        found
    }

    fn eye_of(&self, id: EntityId) -> Option<Vec3> {
        if let Some(player) = self.players.values().find(|p| p.entity_id == id) {
            return Some(player.eye_position());
        }
        let entity = self.entities.store.get(id)?;
        Some(entity.base.position + Vec3::new(0.0, f64::from(entity.base.eye_height()), 0.0))
    }

    fn potent_nausea(&mut self, pos: BlockPos) {
        if self.world.level.game_time % potent::NAUSEA_INTERVAL_TICKS != 0 {
            return;
        }
        let Some(source) = self.gas_source(pos) else {
            return;
        };
        let (sx, sy, sz) = (
            f64::from(source.x),
            f64::from(source.y),
            f64::from(source.z),
        );
        let reach = potent::NEARBY_INFLATE_XZ;
        let area = Aabb {
            min_x: sx - reach,
            min_y: sy,
            min_z: sz - reach,
            max_x: sx + 1.0 + reach,
            max_y: sy + 1.0,
            max_z: sz + 1.0 + reach,
        };
        let Some(effect) = EffectId::from_name("minecraft:nausea") else {
            return;
        };
        for id in self.nearby_living(&area) {
            let Some(eye) = self.eye_of(id) else {
                continue;
            };
            let eye_open = potent::is_passable(self.block_or_air(eye.block()));
            let lower = Vec3::new(eye.x, eye.y - 1.0, eye.z);
            let water = is_water_source_at(self.block_or_air(lower.block()));
            let from = source.offset(0, -1, 0).center();
            let sight = clip_blocks(&self.world.chunks, from, lower).is_none();
            if !potent::gas_reaches(source, eye, eye_open, water, sight) {
                continue;
            }
            let instance = EffectInstance {
                ambient: true,
                ..EffectInstance::new(
                    effect,
                    EffectDuration::Ticks(potent::NAUSEA_DURATION_TICKS),
                    0,
                    true,
                )
            };
            self.add_effect_to(id, instance, EntityPotionEffectCause::Unknown);
        }
    }

    fn potent_launch(&mut self, pos: BlockPos) {
        let Some(source) = self.gas_source(pos) else {
            return;
        };
        let water = potent::geyser_height(pos, source);
        let above = pos.offset(0, 1, 0);
        let count = {
            let block = |at: BlockPos| self.block_or_air(at);
            potent::unobstructed_blocks(&block, above, water)
        };
        let area = Aabb {
            min_x: f64::from(above.x),
            min_y: f64::from(above.y),
            min_z: f64::from(above.z),
            max_x: f64::from(above.x) + 1.0,
            max_y: f64::from(above.y) + 1.0 + f64::from((count - 1).max(0)),
            max_z: f64::from(above.z) + 1.0,
        };
        let mut players: Vec<rspigot_domain::player::PlayerId> = Vec::new();
        for player in self.players.values() {
            if player.vitals.is_dead()
                || player.game_mode == rspigot_domain::player::GameMode::Spectator
                || player.flying
                || self.entities.rides.is_passenger(player.entity_id)
            {
                continue;
            }
            if self
                .entity_box(player.entity_id)
                .is_some_and(|(_, _, bounds)| bounds.intersects(&area))
                && potent::launches(player.vitals.velocity.y, water)
            {
                players.push(player.id);
            }
        }
        for id in players {
            if let Some(player) = self.players.get_mut(&id) {
                player.vitals.velocity =
                    player.vitals.velocity + Vec3::new(0.0, potent::LAUNCH_IMPULSE, 0.0);
                player.vitals.sync_velocity = true;
            }
        }
        let ids: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|entity| {
                !entity.base.is_removed()
                    && entity.base.kind != EntityKind::EnderDragon
                    && entity.base.bounding_box().intersects(&area)
                    && !self.entities.rides.is_passenger(entity.base.id)
                    && (entity.living().is_none() || self.living_entity_alive(entity.base.id))
                    && potent::launches(entity.base.velocity.y, water)
            })
            .map(|entity| entity.base.id)
            .collect();
        for id in ids {
            if let Some(entity) = self.entities.store.get_mut(id) {
                if entity.base.velocity.y > -0.5 && entity.base.fall_distance > 1.0 {
                    entity.base.fall_distance = 1.0;
                }
                entity.base.velocity =
                    entity.base.velocity + Vec3::new(0.0, potent::LAUNCH_IMPULSE, 0.0);
                entity.base.sync_velocity = true;
            }
        }
    }

    pub(crate) fn sulfur_spike_random_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        let growth_roll: f32 = self.rng.random();
        if growth_roll >= spike::GROWTH_CHANCE {
            return;
        }
        let above = self.block_or_air(pos.offset(0, 1, 0));
        if !spike::is_stalactite_start(state, above) {
            return;
        }
        let seed: u64 = self.rng.random();
        let placed = {
            let block = |at: BlockPos| self.block_or_air(at);
            let mut rng = StdRng::seed_from_u64(seed);
            spike::plan_sulfur_growth(&block, pos, state, &mut ServerRandom(&mut rng))
        };
        for (at, grown) in &placed {
            self.set_block(*at, *grown);
        }
        for (at, _) in &placed {
            self.update_neighbour_shapes(*at);
        }
    }
}
