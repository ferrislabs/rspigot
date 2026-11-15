use rspigot_blocks::{BlockKind, Face, StateFlags};
use rspigot_domain::bed::{self, BedRule, Problem, Sleeper};
use rspigot_domain::damage::DamageType;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::{BlockPos, BlockStateId, Identifier, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_entities::SoundSource;

use crate::events::{Animation, ClientEvent};
use crate::explosion::BlockInteraction;
use crate::health::DamageSource;
use crate::plugins::events::{
    EventResult, PlayerBedEnter, PlayerBedEnterPatch, PlayerBedEnterResult, PlayerBedLeave,
    PlayerSpawnChange, PlayerSpawnChangeCause, PlayerSpawnChangePatch, PlayerTeleportCause,
    TimeSkipReason,
};
use crate::plugins::{Continuation, EventPayload, Location};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

#[derive(Debug)]
pub(crate) struct BedEnterStep {
    player: PlayerId,
    kind: BlockKind,
    state: BlockStateId,
    rule: BedRule,
    pos: BlockPos,
    result: Result<(), Problem>,
}

#[derive(Debug)]
pub(crate) struct BedLeaveStep {
    player: PlayerId,
    forceful: bool,
    update: bool,
}

#[derive(Debug)]
pub(crate) struct SpawnChangeStep {
    player: PlayerId,
    config: Option<bed::RespawnConfig>,
    show_message: bool,
}

const fn enter_result(result: Result<(), Problem>) -> PlayerBedEnterResult {
    match result {
        Ok(()) => PlayerBedEnterResult::Ok,
        Err(Problem::TooFarAway) => PlayerBedEnterResult::TooFarAway,
        Err(Problem::NotSafe) => PlayerBedEnterResult::NotSafe,
        Err(Problem::NotPossibleHere) => PlayerBedEnterResult::NotPossibleHere,
        Err(Problem::NotPossibleNow(_)) => PlayerBedEnterResult::NotPossibleNow,
        Err(Problem::Obstructed | Problem::Other) => PlayerBedEnterResult::OtherProblem,
    }
}

const EXPLOSION_POWER: f32 = 5.0;
const STRAW_BED_LEAVE_SOUND: &str = "minecraft:block.straw_bed.break_leave";
const WAKE_COUNTER: i32 = 100;
const COOLDOWN_LIMIT: i32 = 110;

impl<L: ClientLink> Server<L> {
    pub(crate) fn bed_neighbour_changed(
        &mut self,
        pos: BlockPos,
        state: BlockStateId,
        towards: Face,
    ) {
        let neighbour = self
            .world
            .chunks
            .block(bed::step(pos, towards))
            .unwrap_or(BlockStateId::AIR);
        let (Some(part), Some(facing)) = (bed::part_of(state), bed::facing_of(state)) else {
            return;
        };
        if towards != bed::neighbour_direction(part, facing) {
            return;
        }
        let next = if bed::is_partner(state, neighbour) {
            bed::heal_from_partner(state, neighbour)
        } else {
            BlockStateId::AIR
        };
        if next != state {
            self.set_block(pos, next);
        }
    }

    pub(crate) fn set_respawn_position(
        &mut self,
        id: PlayerId,
        config: Option<bed::RespawnConfig>,
        show_message: bool,
        cause: PlayerSpawnChangeCause,
    ) {
        let step = SpawnChangeStep {
            player: id,
            config: config.clone(),
            show_message,
        };
        self.fire(
            id,
            Continuation::SpawnChange(Box::new(step)),
            move |server| {
                let location = config.as_ref().map(|config| Location {
                    world: config.dimension.to_string(),
                    position: Vec3::new(
                        f64::from(config.pos.x),
                        f64::from(config.pos.y),
                        f64::from(config.pos.z),
                    ),
                    rotation: Rotation::new(config.yaw, config.pitch),
                });
                Some(EventPayload::PlayerSpawnChange(PlayerSpawnChange {
                    cause,
                    forced: config.as_ref().is_some_and(|config| config.forced),
                    new_spawn: location,
                    player: server.snapshot_of(id)?,
                }))
            },
        );
    }

    pub(crate) fn finish_spawn_change(
        &mut self,
        step: SpawnChangeStep,
        cancelled: bool,
        patch: PlayerSpawnChangePatch,
    ) {
        if cancelled {
            return;
        }
        let config = match patch.new_spawn {
            Some(None) => None,
            Some(Some(location)) => {
                Identifier::parse(&location.world)
                    .ok()
                    .map(|dimension| bed::RespawnConfig {
                        dimension,
                        pos: location.position.block(),
                        yaw: location.rotation.yaw,
                        pitch: location.rotation.pitch,
                        forced: patch
                            .forced
                            .unwrap_or_else(|| step.config.as_ref().is_some_and(|c| c.forced)),
                    })
            }
            None => step.config.map(|config| bed::RespawnConfig {
                forced: patch.forced.unwrap_or(config.forced),
                ..config
            }),
        };
        let Some(player) = self.players.get_mut(&step.player) else {
            return;
        };
        let moved = match (&config, &player.respawn) {
            (Some(new), Some(old)) => !new.is_same_position(old),
            (Some(_), None) => true,
            _ => false,
        };
        player.respawn = config;
        if step.show_message && moved {
            self.push(
                Audience::One(step.player),
                ClientEvent::SystemChat {
                    message: Component::translatable(bed::SPAWN_SET_MESSAGE, Vec::new()),
                    overlay: false,
                },
            );
        }
    }

    fn bed_rule_of(&self, kind: BlockKind) -> BedRule {
        bed::rule_of(
            &self.world.dimension.dimension_type,
            kind == BlockKind::StrawBed,
        )
    }

    pub(crate) fn dark_outside(&self) -> bool {
        !self.world.dimension.has_fixed_time && crate::mobs::sky_darken(&self.world.level) > 3
    }

    fn can_sleep_through_nights(&self) -> bool {
        self.config.rules.players_sleeping_percentage < 101
    }

    fn overlay(&mut self, id: PlayerId, key: &str) {
        self.push(
            Audience::One(id),
            ClientEvent::SystemChat {
                message: Component::translatable(key, Vec::new()),
                overlay: true,
            },
        );
    }

    pub(crate) fn use_bed(&mut self, id: PlayerId, pos: BlockPos, clicked: BlockStateId) -> bool {
        let Some(kind) = bed::bed_of(clicked) else {
            return false;
        };
        let Some(partner) = bed::partner_pos(pos, clicked) else {
            return true;
        };
        if !bed::is_partner(clicked, self.block_or_air(partner)) {
            return true;
        }
        let Some(head) = bed::head_pos(pos, clicked) else {
            return true;
        };
        let state = self.block_or_air(head);
        let rule = self.bed_rule_of(kind);
        if bed::is_occupied(state) {
            if !self.kick_villager_out_of_bed(head) {
                self.overlay(id, bed::OCCUPIED_MESSAGE);
            }
            return true;
        }
        self.start_sleep_in_bed(id, kind, state, rule, head);
        true
    }

    fn kick_villager_out_of_bed(&mut self, head: BlockPos) -> bool {
        let sleeper = self
            .entities
            .store
            .iter()
            .find(|entity| {
                entity.base.kind == Kind::Villager
                    && entity.base.pose() == rspigot_domain::entity::Pose::Sleeping
                    && entity.base.position.block() == head
            })
            .map(|entity| entity.base.id);
        let Some(villager) = sleeper else {
            return false;
        };
        self.villager_stop_sleeping(villager);
        true
    }

    fn suffocating(&self, pos: BlockPos) -> bool {
        let flags = self.block_or_air(pos).flags();
        flags.contains(StateFlags::BLOCKS_MOTION) && flags.contains(StateFlags::FULL_COLLISION_CUBE)
    }

    fn monster_prevents_rest(&self, id: PlayerId, pos: BlockPos) -> bool {
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let area = bed::monster_search_area(pos);
        let game_time = self.world.level.game_time;
        let universal = self.config.rules.universal_anger;
        let uuid = player.profile.id.0;
        self.entities.store.iter().any(|entity| {
            if entity.base.is_removed()
                || !entity.base.kind.extends("Monster")
                || !entity.base.bounding_box().intersects(&area)
            {
                return false;
            }
            if entity.base.kind != Kind::ZombifiedPiglin {
                return true;
            }
            entity
                .living()
                .and_then(|living| living.mob.as_ref())
                .and_then(|mob| mob.neutral)
                .is_some_and(|anger| {
                    anger.is_angry(game_time)
                        && (anger.target == Some(uuid)
                            || anger.angry_at_all_players(game_time, universal))
                })
        })
    }

    fn bed_result(
        &mut self,
        id: PlayerId,
        rule: BedRule,
        pos: BlockPos,
        facing: Face,
    ) -> Result<(), Problem> {
        let Some(player) = self.players.get(&id) else {
            return Err(Problem::Other);
        };
        if player.sleeping.is_some() || player.vitals.is_dead() {
            return Err(Problem::Other);
        }
        let dark = self.dark_outside();
        let can_sleep = rule.can_sleep.test(dark);
        let can_set_spawn = rule.can_set_spawn.test(dark);
        let (position, creative, rotation) = (
            player.position,
            player.game_mode == GameMode::Creative,
            player.rotation,
        );
        let behind = bed::step(pos, facing.opposite());
        if !bed::in_reach(position, pos) && !bed::in_reach(position, behind) {
            return Err(Problem::TooFarAway);
        }
        let above = pos.offset(0, 1, 0);
        if self.suffocating(above) || self.suffocating(bed::step(above, facing.opposite())) {
            return Err(Problem::Obstructed);
        }
        if can_set_spawn {
            self.set_respawn_position(
                id,
                Some(bed::RespawnConfig {
                    dimension: self.world.dimension.key.clone(),
                    pos,
                    yaw: rotation.yaw,
                    pitch: rotation.pitch,
                    forced: false,
                }),
                true,
                PlayerSpawnChangeCause::Bed,
            );
        }
        if !can_sleep {
            return Err(rule.problem());
        }
        if !creative && self.monster_prevents_rest(id, pos) {
            return Err(Problem::NotSafe);
        }
        Ok(())
    }

    fn start_sleep_in_bed(
        &mut self,
        id: PlayerId,
        kind: BlockKind,
        state: BlockStateId,
        rule: BedRule,
        pos: BlockPos,
    ) {
        let Some(facing) = bed::facing_of(state) else {
            return;
        };
        let result = self.bed_result(id, rule, pos, facing);
        if result == Err(Problem::Other) {
            self.bed_problem(id, kind, rule, pos, state, Problem::Other);
            return;
        }
        let step = BedEnterStep {
            player: id,
            kind,
            state,
            rule,
            pos,
            result,
        };
        self.fire(id, Continuation::BedEnter(Box::new(step)), move |server| {
            Some(EventPayload::PlayerBedEnter(PlayerBedEnter {
                bed: pos,
                bed_enter_result: enter_result(result),
                player: server.snapshot_of(id)?,
                use_bed: EventResult::Default,
            }))
        });
    }

    pub(crate) fn finish_bed_enter_event(
        &mut self,
        step: BedEnterStep,
        cancelled: bool,
        patch: PlayerBedEnterPatch,
    ) {
        let verdict = if cancelled {
            EventResult::Deny
        } else {
            patch.use_bed.unwrap_or(EventResult::Default)
        };
        let result = match verdict {
            EventResult::Allow => Ok(()),
            EventResult::Deny => Err(Problem::Other),
            EventResult::Default => step.result,
        };
        self.finish_bed_enter(
            step.player,
            step.kind,
            step.state,
            step.rule,
            step.pos,
            result,
        );
    }

    pub(crate) fn finish_bed_enter(
        &mut self,
        id: PlayerId,
        kind: BlockKind,
        state: BlockStateId,
        rule: BedRule,
        pos: BlockPos,
        result: Result<(), Problem>,
    ) {
        if let Err(problem) = result {
            self.bed_problem(id, kind, rule, pos, state, problem);
            return;
        }
        if !self.begin_sleeping(id, kind, pos) {
            self.bed_problem(id, kind, rule, pos, state, Problem::Other);
            return;
        }
        let slept_in = if kind == BlockKind::StrawBed {
            crate::stats::custom::SLEEP_IN_STRAW_BED
        } else {
            crate::stats::custom::SLEEP_IN_BED
        };
        self.award_custom(id, slept_in, 1);
        self.trigger_slept_in_bed(id);
        if !self.can_sleep_through_nights() {
            self.overlay(id, bed::SLEEP_NOT_POSSIBLE_MESSAGE);
        }
        self.update_sleeping_list();
    }

    fn bed_problem(
        &mut self,
        id: PlayerId,
        kind: BlockKind,
        rule: BedRule,
        pos: BlockPos,
        state: BlockStateId,
        problem: Problem,
    ) {
        if rule.destroy_on_use {
            self.destroy_bed_on_use(kind, pos, state);
        } else if let Some(message) = problem.message() {
            self.overlay(id, message);
        }
    }

    fn destroy_bed_on_use(&mut self, kind: BlockKind, pos: BlockPos, state: BlockStateId) {
        if kind == BlockKind::StrawBed {
            self.destroy_straw_bed(pos);
            return;
        }
        self.set_block(pos, BlockStateId::AIR);
        if let Some(facing) = bed::facing_of(state) {
            let other = bed::step(pos, facing.opposite());
            if self.block_or_air(other).kind() == Some(kind) {
                self.set_block(other, BlockStateId::AIR);
            }
        }
        let damage = DamageSource::new(DamageType::BadRespawnPoint);
        self.explode(
            None,
            &damage,
            pos.center() + Vec3::new(0.0, 0.5, 0.0),
            EXPLOSION_POWER,
            BlockInteraction::DestroyWithDecay,
            true,
            None,
        );
    }

    fn destroy_straw_bed(&mut self, pos: BlockPos) {
        self.play_sound(
            None,
            pos.center(),
            STRAW_BED_LEAVE_SOUND,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        self.set_block(pos, BlockStateId::AIR);
    }

    fn sleep_spot_exists(&self, kind: BlockKind, pos: BlockPos) -> bool {
        let state = self.block_or_air(pos);
        if state.kind() != Some(kind) {
            return false;
        }
        if kind != BlockKind::StrawBed || bed::part_of(state) != Some(bed::Part::Head) {
            return true;
        }
        bed::partner_pos(pos, state).is_some_and(|foot| {
            let other = self.block_or_air(foot);
            other.kind() == Some(kind) && bed::part_of(other) == Some(bed::Part::Foot)
        })
    }

    fn begin_sleeping(&mut self, id: PlayerId, kind: BlockKind, pos: BlockPos) -> bool {
        if !self.sleep_spot_exists(kind, pos) || bed::is_occupied(self.block_or_air(pos)) {
            return false;
        }
        let Some(entity) = self
            .players
            .get(&id)
            .filter(|player| player.sleeping.is_none() && !player.vitals.is_dead())
            .map(|player| player.entity_id)
        else {
            return false;
        };
        self.stop_riding_forced(entity);
        let state = self.block_or_air(pos);
        self.set_block(pos, bed::with_occupied(state, true));
        let spot = bed::sleep_position(pos, kind);
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        player.position = spot;
        player.sleeping = Some(pos);
        player.sleep_counter = 0;
        player.sneaking = false;
        player.known_movement = Vec3::ZERO;
        self.broadcast_metadata(id);
        self.push_own_metadata(id);
        self.ask_teleport(id, spot, None, PlayerTeleportCause::Unknown, None);
        true
    }

    fn push_own_metadata(&mut self, id: PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let entity = player.entity_id;
        let entries = Self::metadata(player);
        self.push(
            Audience::One(id),
            ClientEvent::EntityMetadata { entity, entries },
        );
    }

    pub(crate) fn stop_sleep_in_bed(&mut self, id: PlayerId, forceful: bool, update: bool) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let Some(sleeping) = player.sleeping else {
            return;
        };
        if player.waking {
            return;
        }
        player.waking = true;
        let step = BedLeaveStep {
            player: id,
            forceful,
            update,
        };
        self.fire(id, Continuation::BedLeave(step), move |server| {
            Some(EventPayload::PlayerBedLeave(PlayerBedLeave {
                bed: sleeping,
                player: server.snapshot_of(id)?,
                set_spawn_location: true,
            }))
        });
    }

    pub(crate) fn finish_bed_leave_event(&mut self, step: BedLeaveStep, cancelled: bool) {
        let Some(player) = self.players.get_mut(&step.player) else {
            return;
        };
        player.waking = false;
        if cancelled || player.sleeping.is_none() {
            return;
        }
        self.finish_bed_leave(step.player, step.forceful, step.update);
    }

    pub(crate) fn finish_bed_leave(&mut self, id: PlayerId, forceful: bool, update: bool) {
        let Some(entity) = self.players.get(&id).map(|player| player.entity_id) else {
            return;
        };
        let event = ClientEvent::EntityAnimation {
            entity,
            animation: Animation::WakeUp,
        };
        self.push(Audience::One(id), event.clone());
        self.push(Audience::Trackers(entity), event);
        self.stand_up(id);
        if update {
            self.update_sleeping_list();
        }
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        player.sleep_counter = if forceful { 0 } else { WAKE_COUNTER };
        let (position, rotation) = (player.position, player.rotation);
        self.ask_teleport(
            id,
            position,
            Some(rotation),
            PlayerTeleportCause::ExitBed,
            None,
        );
    }

    pub(crate) fn stand_up(&mut self, id: PlayerId) {
        let Some((pos, yaw)) = self
            .players
            .get(&id)
            .and_then(|player| player.sleeping.map(|pos| (pos, player.rotation.yaw)))
        else {
            return;
        };
        let state = self.block_or_air(pos);
        let mut placed = None;
        if let Some(kind) = bed::bed_of(state)
            && let Some(facing) = bed::facing_of(state)
        {
            self.set_block(pos, bed::with_occupied(state, false));
            if kind == BlockKind::StrawBed && self.bed_rule_of(kind).destroy_on_leave {
                self.destroy_straw_bed(pos);
            }
            let spot = bed::stand_up_or_fallback(&self.world.chunks, pos, facing, yaw);
            placed = Some((spot, bed::look_at_yaw(spot, pos)));
        }
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if let Some((spot, yaw)) = placed {
            player.position = spot;
            player.rotation = Rotation::new(yaw, 0.0);
        }
        player.sleeping = None;
        self.broadcast_metadata(id);
        self.push_own_metadata(id);
    }

    pub(crate) fn leave_bed_for_good(&mut self, id: PlayerId) {
        if self
            .players
            .get(&id)
            .is_some_and(|player| player.sleeping.is_some())
        {
            self.stand_up(id);
        }
    }

    fn sleepers(&self) -> Vec<Sleeper> {
        self.players
            .values()
            .map(|player| Sleeper {
                spectator: player.game_mode == GameMode::Spectator,
                sleeping: player.sleeping.is_some(),
                deep: player.sleeping.is_some() && player.sleep_counter >= bed::DEEP_SLEEP_TICKS,
            })
            .collect()
    }

    pub(crate) fn update_sleeping_list(&mut self) {
        if self.players.is_empty() {
            return;
        }
        let sleepers = self.sleepers();
        if self.world.sleep.update(&sleepers) {
            self.announce_sleep_status();
        }
    }

    fn announce_sleep_status(&mut self) {
        if !self.can_sleep_through_nights() {
            return;
        }
        let percent = self.config.rules.players_sleeping_percentage;
        let status = self.world.sleep;
        let message = if status.enough_sleeping(percent) {
            Component::translatable(bed::SLEEP_SKIPPING_MESSAGE, Vec::new())
        } else {
            Component::translatable(
                bed::SLEEP_COUNT_MESSAGE,
                vec![
                    Component::text(status.sleeping.to_string()),
                    Component::text(bed::sleepers_needed(status.active, percent).to_string()),
                ],
            )
        };
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for id in ids {
            self.push(
                Audience::One(id),
                ClientEvent::SystemChat {
                    message: message.clone(),
                    overlay: true,
                },
            );
        }
    }

    fn wake_up_all_players(&mut self) {
        self.world.sleep.clear();
        let sleeping: Vec<PlayerId> = self
            .players
            .values()
            .filter(|player| player.sleeping.is_some())
            .map(|player| player.id)
            .collect();
        for id in sleeping {
            self.stop_sleep_in_bed(id, false, false);
        }
    }

    pub(crate) fn finish_night_skip(&mut self) {
        self.wake_up_all_players();
    }

    pub(crate) fn tick_sleeping(&mut self) {
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for id in ids {
            self.tick_sleeper(id);
        }
        self.skip_the_night();
    }

    fn tick_sleeper(&mut self, id: PlayerId) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let Some(pos) = player.sleeping else {
            if player.sleep_counter > 0 {
                player.sleep_counter += 1;
                if player.sleep_counter > COOLDOWN_LIMIT {
                    player.sleep_counter = 0;
                }
            }
            return;
        };
        player.sleep_counter = (player.sleep_counter + 1).min(bed::DEEP_SLEEP_TICKS);
        let at = player.position.block();
        if let Some(kind) = bed::bed_of(self.block_or_air(at))
            && !self.bed_rule_of(kind).can_sleep.test(self.dark_outside())
        {
            self.stop_sleep_in_bed(id, false, true);
            return;
        }
        let exists = bed::bed_of(self.block_or_air(pos))
            .is_some_and(|kind| self.sleep_spot_exists(kind, pos));
        if !exists {
            self.stop_sleep_in_bed(id, true, true);
        }
    }

    fn skip_the_night(&mut self) {
        let percent = self.config.rules.players_sleeping_percentage;
        let status = self.world.sleep;
        let sleepers = self.sleepers();
        if !status.enough_sleeping(percent) || !status.enough_deep_sleeping(percent, &sleepers) {
            return;
        }
        if self.world.level.daylight_cycle && !self.world.dimension.has_fixed_time {
            let target = bed::skip_target(self.world.level.day_time);
            self.change_day_time_waking(target, TimeSkipReason::NightSkip);
        } else {
            self.finish_night_skip();
        }
        if self.world.level.raining {
            self.change_weather(false, false);
        }
    }
}
