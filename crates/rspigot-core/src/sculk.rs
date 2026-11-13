use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::game_event::{GameEvent, VIBRATION_RESONATORS, redstone_strength_for_distance};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::warden::{self, WardenSpawnTracker};
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::{EntityKind, SoundSource};
use rspigot_nbt::Tag;

use crate::game_events::{GameEventContext, TimerKind};
use crate::ports::ClientLink;
use crate::server::Server;

const SENSOR_ACTIVE_TICKS: u64 = 30;
const CALIBRATED_ACTIVE_TICKS: u64 = 10;
const COOLDOWN_TICKS: u64 = 10;
const SHRIEK_TICKS: u64 = 90;
const SHRIEK_LEVEL_EVENT: i32 = 3007;
const WARNING_KEY: &str = "warning_level";
const FREQUENCY_KEY: &str = "last_vibration_frequency";
const CLICKING: &str = "minecraft:block.sculk_sensor.clicking";
const CLICKING_STOP: &str = "minecraft:block.sculk_sensor.clicking_stop";
const RESONATE: &str = "minecraft:block.amethyst_block.resonate";
const NOTES: [i32; 16] = [0, 0, 2, 4, 6, 7, 9, 10, 12, 14, 15, 18, 19, 21, 22, 24];
const SUMMON_ATTEMPTS: i32 = 20;
const SUMMON_HORIZONTAL: i32 = 5;
const SUMMON_VERTICAL: i32 = 6;
const SUMMON_LEVEL: i32 = 4;
const SHRIEKER_DARKNESS_RADIUS: i32 = 40;
const WARNING_SOUNDS: [&str; 4] = [
    "minecraft:entity.warden.nearby_close",
    "minecraft:entity.warden.nearby_closer",
    "minecraft:entity.warden.nearby_closest",
    "minecraft:entity.warden.listening_angry",
];

fn note_pitch(note: i32) -> f32 {
    2.0_f32.powf((note - 12) as f32 / 12.0)
}

fn center(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + 0.5,
        f64::from(pos.z) + 0.5,
    )
}

fn with(state: BlockStateId, name: &str, value: &str) -> BlockStateId {
    state.with_property(name, value).unwrap_or(state)
}

impl<L: ClientLink> Server<L> {
    fn sensor_state(&self, pos: BlockPos) -> Option<BlockStateId> {
        self.world.chunks.block(pos).filter(|state| {
            matches!(
                state.kind(),
                Some(BlockKind::SculkSensor | BlockKind::CalibratedSculkSensor)
            )
        })
    }

    pub(crate) fn sensor_receive(
        &mut self,
        pos: BlockPos,
        calibrated: bool,
        event: GameEvent,
        source: Option<EntityId>,
        distance: f32,
    ) {
        let Some(state) = self.sensor_state(pos) else {
            return;
        };
        if state.property("sculk_sensor_phase") != Some("inactive") {
            return;
        }
        let frequency = event.frequency();
        if let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk())
            && let Some(mut entity) = chunk.block_entity(pos).cloned()
        {
            entity.data.insert(FREQUENCY_KEY, frequency);
            chunk.set_block_entity(entity);
        }
        let radius = if calibrated { 16 } else { 8 };
        let power = redstone_strength_for_distance(distance, radius);
        self.sensor_activate(pos, state, calibrated, source, power, frequency);
    }

    fn sensor_activate(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        calibrated: bool,
        source: Option<EntityId>,
        power: i32,
        frequency: i32,
    ) {
        let active = with(
            with(state, "sculk_sensor_phase", "active"),
            "power",
            &power.to_string(),
        );
        self.set_block(pos, active);
        let ticks = if calibrated {
            CALIBRATED_ACTIVE_TICKS
        } else {
            SENSOR_ACTIVE_TICKS
        };
        self.schedule_sculk(pos, ticks, TimerKind::Sensor);
        self.try_resonate(source, pos, frequency);
        self.block_game_event(
            GameEvent::SculkSensorTendrilsClicking,
            pos,
            GameEventContext::of(source),
        );
        if state.property("waterlogged") != Some("true") {
            let pitch = self.rng.random::<f32>() * 0.2 + 0.8;
            self.play_sound(None, center(pos), CLICKING, SoundSource::Blocks, 1.0, pitch);
        }
    }

    fn try_resonate(&mut self, source: Option<EntityId>, pos: BlockPos, frequency: i32) {
        let Some(event) = GameEvent::resonance_by_frequency(frequency) else {
            return;
        };
        for (dx, dy, dz) in [
            (0, -1, 0),
            (0, 1, 0),
            (0, 0, -1),
            (0, 0, 1),
            (-1, 0, 0),
            (1, 0, 0),
        ] {
            let at = pos.offset(dx, dy, dz);
            let Some(state) = self.world.chunks.block(at) else {
                continue;
            };
            if !rspigot_domain::entity::physics::has_tag(state, VIBRATION_RESONATORS) {
                continue;
            }
            self.block_game_event(
                event,
                at,
                GameEventContext {
                    entity: source,
                    state: Some(state),
                },
            );
            let pitch = note_pitch(NOTES[frequency as usize]);
            self.play_sound(None, center(at), RESONATE, SoundSource::Blocks, 1.0, pitch);
        }
    }

    pub(crate) fn sensor_tick(&mut self, pos: BlockPos) {
        let Some(state) = self.sensor_state(pos) else {
            return;
        };
        match state.property("sculk_sensor_phase") {
            Some("active") => {
                let cooled = with(with(state, "sculk_sensor_phase", "cooldown"), "power", "0");
                self.set_block(pos, cooled);
                self.schedule_sculk(pos, COOLDOWN_TICKS, TimerKind::Sensor);
            }
            Some("cooldown") => {
                self.set_block(pos, with(state, "sculk_sensor_phase", "inactive"));
                if state.property("waterlogged") != Some("true") {
                    let pitch = self.rng.random::<f32>() * 0.2 + 0.8;
                    self.play_sound(
                        None,
                        center(pos),
                        CLICKING_STOP,
                        SoundSource::Blocks,
                        1.0,
                        pitch,
                    );
                }
            }
            _ => {}
        }
    }

    pub(crate) fn player_step(&mut self, id: PlayerId, from: Vec3, to: Vec3, on_ground: bool) {
        const STEP_FACTOR: f64 = 0.6;
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if !on_ground || player.flying || player.game_mode == GameMode::Spectator {
            return;
        }
        let (dx, dz) = (to.x - from.x, to.z - from.z);
        let horizontal = (dx * dx + dz * dz).sqrt();
        if horizontal <= 0.0 {
            return;
        }
        let entity = player.entity_id;
        let vitals = &mut player.vitals;
        vitals.move_dist += (horizontal * STEP_FACTOR) as f32;
        let stepping = vitals.move_dist > vitals.next_step;
        if stepping {
            vitals.next_step = vitals.move_dist + 1.0;
        }
        let below = BlockPos::new(
            to.x.floor() as i32,
            (to.y - 0.2).floor() as i32,
            to.z.floor() as i32,
        );
        self.sculk_on_step(below, entity);
        if stepping
            && self
                .world
                .chunks
                .block(below)
                .is_some_and(|state| !state.is_air())
        {
            self.game_event(GameEvent::Step, to, GameEventContext::of(Some(entity)));
        }
    }

    pub(crate) fn sculk_on_step(&mut self, pos: BlockPos, entity: EntityId) {
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        match state.kind() {
            Some(BlockKind::SculkSensor | BlockKind::CalibratedSculkSensor) => {
                if self.warden_entity(entity)
                    || state.property("sculk_sensor_phase") != Some("inactive")
                {
                    return;
                }
                let calibrated = state.kind() == Some(BlockKind::CalibratedSculkSensor);
                let at = self
                    .sculk_entity_position(entity)
                    .unwrap_or_else(|| center(pos));
                self.handle_forced_step(pos, calibrated, at, entity);
            }
            Some(BlockKind::SculkShrieker) => {
                let player = self.shrieker_player(Some(entity));
                self.shrieker_try_shriek(pos, player);
            }
            _ => {}
        }
    }

    fn handle_forced_step(&mut self, pos: BlockPos, calibrated: bool, at: Vec3, entity: EntityId) {
        let mut data = self.listener_data(crate::game_events::Listener::Sensor { pos, calibrated });
        if data.current.is_some() {
            return;
        }
        let distance = rspigot_domain::game_event::distance_between_in_blocks(at.block(), pos);
        let info = rspigot_domain::game_event::VibrationInfo::new(
            GameEvent::Step,
            distance,
            at,
            None,
            None,
            Some(entity),
        );
        data.selector
            .add_candidate(info, self.world.level.game_time);
        self.store_listener_data(
            crate::game_events::Listener::Sensor { pos, calibrated },
            data,
        );
    }

    fn warden_entity(&self, id: EntityId) -> bool {
        self.entities
            .store
            .get(id)
            .is_some_and(|entity| entity.base.kind == EntityKind::Warden)
    }

    pub(crate) fn sculk_entity_position(&self, id: EntityId) -> Option<Vec3> {
        if let Some(player) = self.players.values().find(|p| p.entity_id == id) {
            return Some(player.position);
        }
        self.entity_position(id)
    }

    pub(crate) fn shrieker_player(&self, entity: Option<EntityId>) -> Option<PlayerId> {
        let id = entity?;
        if let Some(player) = self.players.values().find(|p| p.entity_id == id) {
            return Some(player.id);
        }
        let owner = match &self.entities.store.get(id)?.state {
            rspigot_domain::entity::EntityState::Arrow(state) => state.owner,
            rspigot_domain::entity::EntityState::Thrown(state) => state.owner,
            rspigot_domain::entity::EntityState::Hurting(state) => state.owner,
            rspigot_domain::entity::EntityState::Bullet(state) => state.owner,
            rspigot_domain::entity::EntityState::Item(state) => state.owner,
            _ => None,
        }?;
        self.players
            .values()
            .find(|player| player.profile.id.0 == owner)
            .map(|player| player.id)
    }

    fn can_respond(&self, pos: BlockPos) -> bool {
        self.world
            .chunks
            .block(pos)
            .is_some_and(|state| state.property("can_summon") == Some("true"))
            && self.world.difficulty != Difficulty::Peaceful
    }

    fn warning_level_at(&self, pos: BlockPos) -> i32 {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .and_then(|entity| entity.data.get_i32(WARNING_KEY))
            .unwrap_or(0)
    }

    fn set_warning_level_at(&mut self, pos: BlockPos, level: i32) {
        if let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk())
            && let Some(mut entity) = chunk.block_entity(pos).cloned()
        {
            entity.data.insert(WARNING_KEY, Tag::Int(level));
            chunk.set_block_entity(entity);
        }
    }

    pub(crate) fn shrieker_try_shriek(&mut self, pos: BlockPos, player: Option<PlayerId>) {
        let Some(player) = player else {
            return;
        };
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        if state.property("shrieking") == Some("true") {
            return;
        }
        self.set_warning_level_at(pos, 0);
        if self.can_respond(pos) && !self.shrieker_warn(pos, player) {
            return;
        }
        self.shriek(pos, state, player);
    }

    fn nearby_wardens(&self, pos: BlockPos) -> bool {
        let at = center(pos);
        let reach = warden::NEARBY_WARDEN_BOX / 2.0;
        self.entities.store.iter().any(|entity| {
            entity.base.kind == EntityKind::Warden
                && (entity.base.position.x - at.x).abs() <= reach
                && (entity.base.position.y - at.y).abs() <= reach
                && (entity.base.position.z - at.z).abs() <= reach
        })
    }

    fn shrieker_warn(&mut self, pos: BlockPos, trigger: PlayerId) -> bool {
        if self.nearby_wardens(pos) {
            return false;
        }
        let at = center(pos);
        let mut ids: Vec<PlayerId> = self
            .players
            .values()
            .filter(|player| {
                player.game_mode != GameMode::Spectator
                    && !player.vitals.dead
                    && warden::closer_than_warning_radius(at, player.position)
            })
            .map(|player| player.id)
            .collect();
        if !ids.contains(&trigger) {
            ids.push(trigger);
        }
        let mut trackers: Vec<WardenSpawnTracker> = ids
            .iter()
            .filter_map(|id| self.players.get(id))
            .map(|player| player.vitals.warden_tracker)
            .collect();
        let Some(level) = warden::try_warn(&mut trackers) else {
            return false;
        };
        for (id, tracker) in ids.iter().zip(trackers) {
            if let Some(player) = self.players.get_mut(id) {
                player.vitals.warden_tracker = tracker;
            }
        }
        self.set_warning_level_at(pos, level);
        true
    }

    fn shriek(&mut self, pos: BlockPos, state: BlockStateId, player: PlayerId) {
        self.set_block(pos, with(state, "shrieking", "true"));
        self.schedule_sculk(pos, SHRIEK_TICKS, TimerKind::Shrieker);
        self.level_event(SHRIEK_LEVEL_EVENT, pos, 0);
        let source = self.players.get(&player).map(|p| p.entity_id);
        self.block_game_event(GameEvent::Shriek, pos, GameEventContext::of(source));
    }

    pub(crate) fn shrieker_tick(&mut self, pos: BlockPos) {
        let Some(state) = self
            .world
            .chunks
            .block(pos)
            .filter(|state| state.kind() == Some(BlockKind::SculkShrieker))
        else {
            return;
        };
        if state.property("shrieking") == Some("true") {
            self.set_block(pos, with(state, "shrieking", "false"));
            self.shrieker_respond(pos);
        }
    }

    pub(crate) fn shrieker_respond(&mut self, pos: BlockPos) {
        if !self.can_respond(pos) {
            return;
        }
        let level = self.warning_level_at(pos);
        if level <= 0 {
            return;
        }
        if !self.try_summon_warden(pos) {
            self.warden_reply_sound(pos, level);
        }
        self.apply_darkness_around(center(pos), None, SHRIEKER_DARKNESS_RADIUS);
    }

    fn warden_reply_sound(&mut self, pos: BlockPos, level: i32) {
        let Some(sound) = usize::try_from(level - 1)
            .ok()
            .and_then(|index| WARNING_SOUNDS.get(index))
        else {
            return;
        };
        let offset = |rng: &mut rand::rngs::StdRng| f64::from(rng.random_range(-10..=10));
        let at = Vec3::new(
            f64::from(pos.x) + offset(&mut self.rng),
            f64::from(pos.y) + offset(&mut self.rng),
            f64::from(pos.z) + offset(&mut self.rng),
        );
        self.play_sound(None, at, sound, SoundSource::Hostile, 5.0, 1.0);
    }

    fn try_summon_warden(&mut self, pos: BlockPos) -> bool {
        if self.warning_level_at(pos) < SUMMON_LEVEL {
            return false;
        }
        for _ in 0..SUMMON_ATTEMPTS {
            let dx = self
                .rng
                .random_range(-SUMMON_HORIZONTAL..=SUMMON_HORIZONTAL);
            let dz = self
                .rng
                .random_range(-SUMMON_HORIZONTAL..=SUMMON_HORIZONTAL);
            let Some(spot) = self.summon_spot(pos.offset(dx, SUMMON_VERTICAL, dz)) else {
                continue;
            };
            let yaw = self.rng.random::<f32>() * 360.0;
            let at = Vec3::new(
                f64::from(spot.x) + 0.5,
                f64::from(spot.y),
                f64::from(spot.z) + 0.5,
            );
            if self.spawn_triggered_warden(at, yaw) {
                return true;
            }
        }
        false
    }

    fn summon_spot(&self, top: BlockPos) -> Option<BlockPos> {
        let mut at = top;
        for _ in 0..=(SUMMON_VERTICAL * 2) {
            let below = self.world.chunks.block(at.offset(0, -1, 0))?;
            let solid = !below.collision_shape().is_empty();
            let clear = (0..3).all(|up| {
                self.world
                    .chunks
                    .block(at.offset(0, up, 0))
                    .is_some_and(|state| {
                        state.collision_shape().is_empty() && !state.flags().has_fluid()
                    })
            });
            if solid && clear {
                return Some(at);
            }
            at = at.offset(0, -1, 0);
        }
        None
    }

    pub(crate) fn apply_darkness_around(
        &mut self,
        at: Vec3,
        source: Option<EntityId>,
        radius: i32,
    ) {
        use rspigot_domain::effect::{EffectDuration, EffectId, EffectInstance};
        let Some(effect) = EffectId::from_name("minecraft:darkness") else {
            return;
        };
        let instance = EffectInstance::new(effect, EffectDuration::Ticks(DARKNESS_TICKS), 0, false);
        let reach = f64::from(radius);
        let targets: Vec<PlayerId> = self
            .players
            .values()
            .filter(|player| player.game_mode == GameMode::Survival)
            .filter(|player| at.distance_squared(player.position) < reach * reach)
            .filter(|player| {
                player.effects.get(effect).is_none_or(|current| {
                    current.amplifier < instance.amplifier
                        || matches!(current.duration, EffectDuration::Ticks(left) if left < DARKNESS_DISPLAY_LIMIT)
                })
            })
            .map(|player| player.id)
            .collect();
        let _ = source;
        for id in targets {
            self.apply_effect(
                id,
                instance,
                Some(crate::plugins::events::EntityPotionEffectCause::Warden),
            );
        }
    }
}

const DARKNESS_TICKS: u32 = 260;
const DARKNESS_DISPLAY_LIMIT: u32 = 200;
