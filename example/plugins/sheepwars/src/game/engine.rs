use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, MutexGuard, PoisonError};

use rspigot_plugin::{
    Attribute, BossBar, ChunkLoad, Entity, EntityType, GameMode, ItemRemoval, ItemStack, Location,
    ParticleBurst, Player, Position, Rotation, SoundCue, Title, Uuid, Vector,
};

use super::capture::{CAPTURE_PARTICLE, CAPTURE_PARTICLES, CAPTURE_SOUND, Capture};
use super::display::lives_line;
use super::flight::{BOARDING_RIDER_OFFSET, EYE_HEIGHT, Flight, Observation, Step, can_launch};
use super::intent::Intent;
use super::registry::{Airborne, Registry};
use super::session::{ForceStartError, Session, VoidVerdict, launch_announcement};
use crate::arena::{Arena, Point};
use crate::domain::{PlayerId, SheepKind, TeamId};
use crate::hud::{Hud, bossbar_progress, bossbar_title};
use crate::sheep::{
    Combatant, SEEKER_RANGE, Vec3, apply, display_name, homes_in_flight, launch_power,
    launch_velocity, nearest_enemy, paint, plan, plan_pulse, sheep_wool, targetable,
};

pub const HUD_REFRESH_TICKS: u64 = 5;
pub const ACTION_BAR_REFRESH_TICKS: u64 = 20;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    NotSheep,
    Denied,
    Struck,
}

#[derive(Debug)]
pub enum Death {
    NotSheep,
    Slain {
        kind: Option<SheepKind>,
        cue: bool,
        at: Position,
    },
}

pub fn player_id(uuid: Uuid) -> PlayerId {
    PlayerId(uuid.as_u128())
}

struct State {
    session: Session,
    players: HashMap<PlayerId, Player>,
    flights: Registry<Entity>,
    hud: Hud,
    bar: BossBar,
    ticks: u64,
}

pub struct Game {
    arena: Arena,
    state: Mutex<State>,
}

impl Game {
    pub fn new(arena: Arena, session: Session, hud: Hud, bar: BossBar) -> Self {
        Self {
            arena,
            state: Mutex::new(State {
                session,
                players: HashMap::new(),
                flights: Registry::default(),
                hud,
                bar,
                ticks: 0,
            }),
        }
    }

    pub async fn admit(&self, player: Player) {
        let id = player_id(player.uuid);
        let intents = {
            let mut state = self.lock();
            state.bar.add_player(&player);
            state.players.insert(id, player);
            state.session.join(id)
        };
        self.run(intents).await;
    }

    pub fn on_chunk_load(&self, event: &ChunkLoad) {
        if event.world().id() != self.arena.world.id() {
            return;
        }
        let chunk = event.chunk();
        match self.arena.place_chunk(chunk.x, chunk.z) {
            Ok(0) => {}
            Ok(placed) => {
                tracing::debug!("placed {placed} blocks in chunk ({}, {})", chunk.x, chunk.z)
            }
            Err(error) => {
                tracing::warn!("could not place chunk ({}, {}): {error}", chunk.x, chunk.z)
            }
        }
    }

    pub async fn release(&self, player: &Player) {
        let id = player_id(player.uuid);
        let intents = {
            let mut state = self.lock();
            state.bar.remove_player(player);
            state.session.leave(id)
        };
        self.run(intents).await;
        self.lock().players.remove(&id);
    }

    pub async fn join(&self, player: &Player) {
        let id = player_id(player.uuid);
        let intents = {
            let mut state = self.lock();
            state.players.insert(id, player.clone());
            state.session.join(id)
        };
        self.run(intents).await;
    }

    pub async fn quit(&self, player: &Player) {
        let intents = self.lock().session.quit(player_id(player.uuid));
        self.run(intents).await;
    }

    pub async fn died(&self, player: &Player) {
        let intents = self.lock().session.die(player_id(player.uuid));
        self.run(intents).await;
    }

    pub async fn force_start(&self) -> Result<(), ForceStartError> {
        let intents = self.lock().session.force_start()?;
        self.run(intents).await;
        Ok(())
    }

    pub async fn stop(&self) {
        let intents = self.lock().session.stop();
        self.run(intents).await;
    }

    pub fn respawn_location(&self, player: &Player) -> Location {
        let point = self.lock().session.respawn_point(player_id(player.uuid));
        self.location(point)
    }

    pub fn blocks_damage_to(&self, player: &Player, shieldable: bool) -> bool {
        self.lock()
            .session
            .blocks_damage_to(player_id(player.uuid), shieldable)
    }

    pub fn blocks_hit(&self, attacker: &Player, victim: &Player) -> bool {
        self.lock()
            .session
            .blocks_hit(player_id(attacker.uuid), player_id(victim.uuid))
    }

    pub fn void_verdict(&self, player: &Player, y: f64) -> VoidVerdict {
        self.lock().session.void_verdict(player_id(player.uuid), y)
    }

    pub async fn rescue(&self, player: &Player) {
        let lobby = self.lock().session.layout().lobby();
        self.teleport(player, lobby).await;
    }

    pub async fn throw(&self, player: &Player, kind: SheepKind) -> bool {
        let id = player_id(player.uuid);
        let team = {
            let state = self.lock();
            let standing = state.session.standing();
            let Some((_, team)) = standing.into_iter().find(|(member, _)| *member == id) else {
                return false;
            };
            if !state.session.is_playing() || !can_launch(state.flights.len()) {
                return false;
            }
            team
        };
        let eye = Position {
            x: player.position.x,
            y: player.position.y + EYE_HEIGHT,
            z: player.position.z,
        };
        let sheep = match self.arena.world.spawn(EntityType::Sheep, eye).await {
            Ok(sheep) => sheep,
            Err(error) => {
                tracing::warn!("could not spawn a sheep for {}: {error}", player.name);
                return false;
            }
        };
        let velocity = launch_velocity(
            f64::from(player.rotation.yaw),
            f64::from(player.rotation.pitch),
            launch_power(kind),
        );
        paint(&sheep, kind);
        sheep.set_velocity(vector(velocity));
        player
            .inventory
            .remove(ItemRemoval::new(sheep_wool(kind), 1));
        self.lock()
            .flights
            .insert(sheep.entity_id, Flight::new(kind, team, id), sheep);
        if let Some(announcement) = launch_announcement(kind, team) {
            self.run(vec![announcement]).await;
        }
        true
    }

    pub fn strike(&self, attacker: &Player, sheep: &Entity) -> Hit {
        let mut state = self.lock();
        let now = state.ticks;
        let attacker = player_id(attacker.uuid);
        let State {
            session, flights, ..
        } = &mut *state;
        let Some(airborne) = flights.get_mut(sheep.entity_id) else {
            return Hit::NotSheep;
        };
        if session.capture(attacker, &airborne.flight) == Capture::Denied {
            return Hit::Denied;
        }
        airborne.flight.record_hit(attacker, now);
        Hit::Struck
    }

    pub fn slain(&self, sheep: &Entity) -> Death {
        let mut state = self.lock();
        let Some(airborne) = state.flights.take(sheep.entity_id) else {
            return Death::NotSheep;
        };
        if airborne.flight.kind == SheepKind::Boarding {
            state.session.shield(airborne.flight.thrower);
        }
        let (kind, cue) = match airborne.flight.loot() {
            Capture::Denied => (None, false),
            Capture::Fragment => (None, true),
            Capture::Taken { kind } => (Some(kind), true),
        };
        Death::Slain {
            kind,
            cue,
            at: sheep.position,
        }
    }

    pub fn play_capture(&self, at: Position) {
        let world = &self.arena.world;
        world.sound.play(SoundCue::new(CAPTURE_SOUND).at(at));
        world
            .particle
            .spawn(ParticleBurst::new(CAPTURE_PARTICLE, at).count(CAPTURE_PARTICLES));
    }

    pub async fn tick(&self) {
        let intents = self.lock().session.tick();
        self.run(intents).await;
        let roster = self.roster().await;
        self.advance_flights(&roster).await;
        self.refresh();
    }

    pub fn close(&self) {
        let mut state = self.lock();
        for airborne in state.flights.drain() {
            airborne.entity.remove();
        }
        state.hud.close();
        state.bar.remove();
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn player(&self, id: PlayerId) -> Option<Player> {
        self.lock().players.get(&id).cloned()
    }

    fn everyone(&self) -> Vec<Player> {
        self.lock().players.values().cloned().collect()
    }

    fn location(&self, point: Point) -> Location {
        Location {
            world: self.arena.world.id().to_owned(),
            position: Position {
                x: point.x,
                y: point.y,
                z: point.z,
            },
            rotation: Rotation::default(),
        }
    }

    async fn teleport(&self, player: &Player, point: Point) {
        if let Err(error) = player.location.teleport_to(&self.location(point)).await {
            tracing::warn!("could not teleport {}: {error}", player.name);
        }
    }

    async fn run(&self, intents: Vec<Intent>) {
        let mut queue: VecDeque<Intent> = intents.into();
        while let Some(intent) = queue.pop_front() {
            queue.extend(self.perform(intent).await);
        }
    }

    async fn perform(&self, intent: Intent) -> Vec<Intent> {
        match intent {
            Intent::Broadcast(text) => {
                for player in self.everyone() {
                    player.message.send(text.as_str());
                }
            }
            Intent::Tell { player, text } => {
                if let Some(player) = self.player(player) {
                    player.message.send(text);
                }
            }
            Intent::Title { text, subtitle } => {
                for player in self.everyone() {
                    player
                        .title
                        .send(Title::new(text.as_str()).subtitle(subtitle.as_str()));
                }
            }
            Intent::Enroll { player, team } => {
                if let Some(player) = self.player(player) {
                    self.lock().hud.add_player(team, &player.name);
                    player.game_mode.set(GameMode::Survival);
                    player.inventory.clear();
                }
            }
            Intent::Unenroll { player, team } => {
                if let Some(player) = self.player(player) {
                    self.lock().hud.remove_player(team, &player.name);
                }
            }
            Intent::SyncTeams => self.sync_teams(),
            Intent::Teleport { player, to } => {
                if let Some(player) = self.player(player) {
                    self.teleport(&player, to).await;
                }
            }
            Intent::ClearInventory { player } => {
                if let Some(player) = self.player(player) {
                    player.inventory.clear();
                }
            }
            Intent::Give { player, kind } => {
                if let Some(player) = self.player(player) {
                    player
                        .inventory
                        .add(ItemStack::new(sheep_wool(kind), 1).name(display_name(kind)));
                }
            }
            Intent::Respawn { player } => {
                if let Some(player) = self.player(player) {
                    player.health.respawn();
                }
            }
            Intent::Spectate { player } => {
                if let Some(player) = self.player(player) {
                    player.game_mode.set(GameMode::Spectator);
                }
            }
            Intent::Reset => return self.reset(),
        }
        Vec::new()
    }

    fn sync_teams(&self) {
        let state = self.lock();
        for (id, player) in &state.players {
            for team in TeamId::ALL {
                state.hud.remove_player(team, &player.name);
            }
            if let Some(team) = state.session.round().team_of(*id) {
                state.hud.add_player(team, &player.name);
            }
        }
    }

    fn reset(&self) -> Vec<Intent> {
        let mut state = self.lock();
        for airborne in state.flights.drain() {
            airborne.entity.remove();
        }
        for player in state.players.values() {
            for team in TeamId::ALL {
                state.hud.remove_player(team, &player.name);
            }
        }
        let mut ids: Vec<PlayerId> = state.players.keys().copied().collect();
        ids.sort();
        ids.into_iter()
            .flat_map(|id| state.session.join(id))
            .collect()
    }

    fn refresh(&self) {
        let mut state = self.lock();
        state.ticks += 1;
        let ticks = state.ticks;
        let state = &mut *state;
        let round = state.session.round();
        if ticks.is_multiple_of(HUD_REFRESH_TICKS) {
            state.hud.render(round, None);
            state.bar.set_title(bossbar_title(round));
            state.bar.set_progress(bossbar_progress(round));
        }
        if ticks.is_multiple_of(ACTION_BAR_REFRESH_TICKS) {
            for (id, player) in &state.players {
                let lives = round
                    .team_of(*id)
                    .and_then(|team| round.team(team).lives_of(*id));
                if let Some(lives) = lives {
                    player.action_bar.send(lives_line(lives));
                }
            }
        }
    }

    async fn roster(&self) -> Vec<Combatant> {
        let (players, teams) = {
            let state = self.lock();
            if state.flights.is_empty() {
                return Vec::new();
            }
            let teams: HashMap<PlayerId, TeamId> = state.session.standing().into_iter().collect();
            let players: Vec<(PlayerId, Player)> = state
                .players
                .iter()
                .filter(|(id, _)| teams.contains_key(id))
                .map(|(id, player)| (*id, player.clone()))
                .collect();
            (players, teams)
        };
        let mut roster = Vec::with_capacity(players.len());
        for (id, player) in players {
            let state = match player.get_state().await {
                Ok(state) => state,
                Err(error) => {
                    tracing::warn!("could not read {}: {error}", player.name);
                    continue;
                }
            };
            let Some(team) = targetable(teams.get(&id).copied(), state.dead) else {
                continue;
            };
            roster.push(Combatant {
                position: vec3(state.player.position),
                player,
                team,
            });
        }
        roster
    }

    async fn advance_flights(&self, roster: &[Combatant]) {
        let handles = self.lock().flights.handles();
        let mut children = Vec::new();
        for (id, entity) in handles {
            let sheep = entity.get_state().await;
            let Some(mut airborne) = self.lock().flights.take(id) else {
                continue;
            };
            let sheep = match sheep {
                Ok(sheep) => sheep,
                Err(error) => {
                    tracing::debug!("a sheep is gone: {error}");
                    continue;
                }
            };
            let position = vec3(sheep.entity.position);
            let observation = Observation {
                on_ground: sheep.on_ground,
                position,
                vertical_speed: sheep.velocity.y,
                enemy: Self::enemy_near(&airborne.flight, position, roster),
            };
            let outcome = airborne.flight.step(observation);
            let step = outcome.step;
            let thrower = airborne.flight.thrower;
            if matches!(step, Step::Detonate | Step::Release) {
                if outcome.pulse {
                    self.pulse(position).await;
                }
                self.lock().session.shield(thrower);
                children.extend(self.land(&airborne, position, roster).await);
                airborne.entity.remove();
                continue;
            }
            let handle = airborne.entity.clone();
            self.lock()
                .flights
                .insert(id, airborne.flight, airborne.entity);
            if outcome.pulse {
                self.pulse(position).await;
            }
            match step {
                Step::Swell(scale) => handle.attributes.set(Attribute::Scale, scale),
                Step::Steer(velocity) => handle.set_velocity(vector(velocity)),
                Step::Carry(at) => self.carry(thrower, at).await,
                Step::Keep | Step::Detonate | Step::Release => {}
            }
        }
        self.lock().flights.extend_capped(children);
    }

    async fn land(
        &self,
        airborne: &Airborne<Entity>,
        at: Vec3,
        roster: &[Combatant],
    ) -> Vec<Airborne<Entity>> {
        let Flight {
            kind,
            team,
            thrower,
            ..
        } = airborne.flight;
        let actions = plan(kind, at, team);
        match apply(&self.arena.world, &actions, roster).await {
            Ok(applied) => applied
                .spawned
                .into_iter()
                .map(|(kind, entity)| Airborne {
                    id: entity.entity_id,
                    flight: Flight::fragment(kind, team, thrower),
                    entity,
                })
                .collect(),
            Err(error) => {
                tracing::warn!("a {kind:?} sheep failed to land: {error}");
                Vec::new()
            }
        }
    }

    fn enemy_near(flight: &Flight, from: Vec3, roster: &[Combatant]) -> Option<Vec3> {
        if !homes_in_flight(flight.kind) {
            return None;
        }
        nearest_enemy(roster, from, SEEKER_RANGE, flight.team)
    }

    async fn pulse(&self, at: Vec3) {
        let actions = plan_pulse(at);
        if let Err(error) = apply(&self.arena.world, &actions, &[]).await {
            tracing::warn!("a sheep fuse failed to pulse: {error}");
        }
    }

    async fn carry(&self, thrower: PlayerId, at: Vec3) {
        self.lock().session.shield(thrower);
        if let Some(rider) = self.player(thrower) {
            let to = Position {
                x: at.x,
                y: at.y + BOARDING_RIDER_OFFSET,
                z: at.z,
            };
            if let Err(error) = rider.location.teleport(to).await {
                tracing::warn!("could not carry {}: {error}", rider.name);
            }
        }
    }
}

const fn vec3(position: Position) -> Vec3 {
    Vec3 {
        x: position.x,
        y: position.y,
        z: position.z,
    }
}

const fn vector(vec: Vec3) -> Vector {
    Vector {
        x: vec.x,
        y: vec.y,
        z: vec.z,
    }
}
