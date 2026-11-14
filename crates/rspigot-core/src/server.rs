use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crossbeam_channel::{Receiver, Sender, select};
use rspigot_domain::item::HOTBAR_START;
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::{Difficulty, Dimension};
use rspigot_domain::{ChunkPos, EntityId, Identifier, Rotation, Vec3};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::admin::{AdminLists, Ban, format_ban_date};
use crate::chunks::{ChunkJob, ChunkManager, ChunkWorkers, MobGeneration, Priority};
use crate::commands::{CommandTree, build_tree};
use crate::config::{GameConfig, LevelData, hashed_seed};
use rand::SeedableRng;
use rand::rngs::StdRng;

use crate::entities::EntityWorld;
use crate::events::{
    ClientEvent, EntityKind, EquipmentSlot, GameEvent, Inbound, JoinInfo, MetadataEntry,
    MetadataValue, PlayerListEntry, Pose, SpawnInfo,
};
use crate::falling::ScheduledFalls;
use crate::player::Player;
use crate::player_io::PlayerIo;
use crate::plugins::events::PlayerLoginResult;
use crate::plugins::{
    Arrival, Broadcasts, Decisions, Impacts, Lane, PluginInfo, PluginIntro, PluginInventories,
    Refusal, Registry, ToPlugin, Visibility,
};
use crate::ports::{
    ChunkGenerator, ChunkStore, ClientLink, EntityStore, PlayerStore, PluginOutbox,
};
use crate::spawning::Spawning;
use crate::status::{PingHook, PlayerSample, StatusBoard};
use crate::streaming::ChunkStreamer;

const KICK_DATE: &str = "%Y-%m-%d at %H:%M:%S UTC";

pub(crate) enum Audience {
    One(PlayerId),
    All,
    Trackers(EntityId),
    TrackersExcept(EntityId, PlayerId),
    ChunkViewers(ChunkPos),
    Dimension(crate::world_state::DimensionKey),
    Nearby {
        position: Vec3,
        range: f64,
        except: Option<PlayerId>,
    },
}

pub struct World {
    pub dimension: Arc<Dimension>,
    pub chunks: ChunkManager,
    pub level: LevelData,
    pub spawn: Vec3,
    pub difficulty: Difficulty,
}

struct TpsMeter {
    last_sample: Instant,
    recent: [f64; 3],
}

impl TpsMeter {
    const SAMPLE_TICKS: u64 = 100;
    const DECAY: [f64; 3] = [0.92, 0.9835, 0.9945];

    fn new() -> Self {
        Self {
            last_sample: Instant::now(),
            recent: [20.0; 3],
        }
    }

    fn record(&mut self, tick: u64) {
        if !tick.is_multiple_of(Self::SAMPLE_TICKS) {
            return;
        }
        let now = Instant::now();
        let elapsed = now
            .duration_since(self.last_sample)
            .as_secs_f64()
            .max(f64::EPSILON);
        self.last_sample = now;
        let current = (Self::SAMPLE_TICKS as f64 / elapsed).min(20.0);
        for (avg, decay) in self.recent.iter_mut().zip(Self::DECAY) {
            *avg = *avg * decay + current * (1.0 - decay);
        }
    }
}

pub struct Server<L> {
    pub(crate) config: GameConfig,
    pub(crate) world: World,
    pub(crate) workers: ChunkWorkers,
    pub(crate) players: FxHashMap<PlayerId, Player<L>>,
    pub(crate) outbox: Vec<(Audience, ClientEvent)>,
    pub(crate) admin: AdminLists,
    pub(crate) admin_sink: Option<Sender<AdminLists>>,
    pub(crate) player_io: Option<PlayerIo>,
    pub(crate) running: bool,
    pub(crate) entities: EntityWorld,
    pub(crate) falls: ScheduledFalls,
    pub(crate) game_events: crate::game_events::GameEvents,
    pub(crate) fluids: crate::fluids::ScheduledFluids,
    pub(crate) rng: StdRng,
    pub(crate) spawning: Spawning,
    pub(crate) raids: crate::raids::RaidRuntime,
    pub(crate) plugins: Registry,
    pub(crate) decisions: Decisions,
    pub(crate) broadcasts: Broadcasts,
    pub(crate) visibility: Visibility,
    pub(crate) plugin_inventories: PluginInventories,
    pub(crate) impacts: Impacts,
    pub(crate) dormant: FxHashMap<crate::world_state::DimensionKey, crate::world_state::WorldState>,
    pub(crate) dimension_provider: Option<crate::travel::DimensionProvider<L>>,
    arrivals: FxHashMap<u64, Arrival<L>>,
    startup: Option<Receiver<Inbound<L>>>,
    arrival_counter: u64,
    login_counter: u64,
    inventory_counter: u64,
    clock: fn() -> SystemTime,
    next_entity_id: i32,
    pub(crate) status: StatusBoard,
    operator_tree: Arc<CommandTree>,
    player_tree: Arc<CommandTree>,
    tick: u64,
    tps: TpsMeter,
}

impl<L: ClientLink> Server<L> {
    pub fn new<S: ChunkStore + EntityStore, G: ChunkGenerator>(
        config: GameConfig,
        mut level: LevelData,
        store: Arc<S>,
        generator: Arc<G>,
        status: StatusBoard,
    ) -> Self {
        let dimension = Arc::new(Dimension::overworld());
        let spawn = match level.spawn {
            Some(pos) => pos.center(),
            None => generator.spawn_point(&dimension),
        };
        level.spawn = Some(spawn.block());
        let workers = ChunkWorkers::spawn(config.chunk_workers, store, generator);
        let mut chunks = ChunkManager::new(Arc::clone(&dimension), config.chunks);
        chunks.set_mob_generation(level.spawn_mobs.then_some(MobGeneration {
            seed: level.seed,
            zoom_seed: hashed_seed(level.seed),
            difficulty: config.difficulty,
        }));
        let admin = config.admin.clone();
        status.update(|s| s.max_players = config.max_players);
        let spawning = Spawning::new(level.seed, config.spawning.slime_seed, &dimension);
        let mut server = Self {
            world: World {
                dimension,
                chunks,
                level,
                spawn,
                difficulty: config.difficulty,
            },
            workers,
            players: FxHashMap::default(),
            outbox: Vec::new(),
            admin,
            admin_sink: None,
            player_io: None,
            running: true,
            entities: EntityWorld::default(),
            falls: ScheduledFalls::default(),
            game_events: crate::game_events::GameEvents::default(),
            fluids: crate::fluids::ScheduledFluids::default(),
            rng: StdRng::from_os_rng(),
            spawning,
            raids: crate::raids::RaidRuntime::default(),
            plugins: Registry::default(),
            decisions: Decisions::default(),
            broadcasts: Broadcasts::default(),
            visibility: Visibility::default(),
            plugin_inventories: PluginInventories::default(),
            impacts: Impacts::default(),
            dormant: FxHashMap::default(),
            dimension_provider: None,
            arrivals: FxHashMap::default(),
            startup: None,
            arrival_counter: 0,
            login_counter: 0,
            inventory_counter: 0,
            clock: SystemTime::now,
            next_entity_id: 1,
            status,
            operator_tree: Arc::new(build_tree(true)),
            player_tree: Arc::new(build_tree(false)),
            tick: 0,
            tps: TpsMeter::new(),
            config,
        };
        server.load_raids();
        server.load_dragon_fight();
        let center = spawn.chunk();
        let radius = server.config.spawn_chunk_radius;
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                server.world.chunks.add_ticket(
                    ChunkPos::new(center.x + dx, center.z + dz),
                    Priority::URGENT,
                    &server.workers,
                );
            }
        }
        server
    }

    pub fn with_player_store<S: PlayerStore>(mut self, store: Arc<S>) -> Self {
        self.player_io = Some(PlayerIo::spawn(store));
        self
    }

    pub fn with_admin_sink(mut self, sink: Sender<AdminLists>) -> Self {
        self.admin_sink = Some(sink);
        self
    }

    pub fn with_startup_inbox(mut self, inbox: Receiver<Inbound<L>>) -> Self {
        self.startup = Some(inbox);
        self
    }

    pub(crate) fn drain_startup(&mut self) {
        let Some(inbox) = self.startup.take() else {
            return;
        };
        let waiting: Vec<Inbound<L>> = queued(&inbox).collect();
        for message in waiting {
            self.handle(message);
        }
        self.startup = Some(inbox);
    }

    pub fn start(&mut self, timeout: Duration) -> usize {
        self.world_initialised();
        let loaded = self.wait_for_spawn(timeout);
        self.world_loaded();
        loaded
    }

    pub fn with_clock(mut self, clock: fn() -> SystemTime) -> Self {
        self.clock = clock;
        self
    }

    pub fn with_rng_seed(mut self, seed: u64) -> Self {
        self.rng = StdRng::seed_from_u64(seed);
        self
    }

    pub(crate) fn now(&self) -> SystemTime {
        (self.clock)()
    }

    pub(crate) fn prune_expired_bans(&mut self) -> SystemTime {
        let now = self.now();
        if self.admin.prune_expired(now) {
            self.publish_admin();
        }
        now
    }

    pub(crate) fn publish_admin(&self) {
        if let Some(sink) = &self.admin_sink {
            let _ = sink.send(self.admin.clone());
        }
    }

    pub fn wait_for_spawn(&mut self, timeout: Duration) -> usize {
        let deadline = Instant::now() + timeout;
        while self.world.chunks.stats().pending > 0 && Instant::now() < deadline {
            if let Ok(result) = self
                .workers
                .results()
                .recv_timeout(Duration::from_millis(50))
            {
                self.world.chunks.accept(result);
            }
        }
        self.handle_chunk_events();
        self.world.chunks.stats().loaded
    }

    pub fn run(mut self, inbound: &Receiver<Inbound<L>>) -> LevelData {
        self.startup = None;
        self.handle_waiting(inbound);
        self.server_loaded();
        let tick_length = Duration::from_millis(50);
        let results = self.workers.results().clone();
        let mut next_tick = Instant::now();
        while self.running {
            let now = Instant::now();
            if now >= next_tick {
                self.handle_waiting(inbound);
                if !self.running {
                    break;
                }
                self.tick();
                next_tick += tick_length;
                let behind = Instant::now().saturating_duration_since(next_tick);
                if behind > Duration::from_secs(2) {
                    tracing::warn!(
                        "Can't keep up! Is the server overloaded? Running {}ms or {} ticks behind",
                        behind.as_millis(),
                        behind.as_millis() / 50
                    );
                    next_tick = Instant::now();
                }
                continue;
            }
            select! {
                recv(inbound) -> message => match message {
                    Ok(message) => {
                        self.handle(message);
                        self.deliver();
                    }
                    Err(_) => self.running = false,
                },
                recv(results) -> result => {
                    if let Ok(result) = result {
                        self.world.chunks.accept(result);
                    }
                },
                default(next_tick.saturating_duration_since(now)) => {}
            }
        }
        self.shutdown(inbound)
    }

    fn handle_waiting(&mut self, inbound: &Receiver<Inbound<L>>) {
        let mut waiting = queued(inbound);
        while self.running
            && let Some(message) = waiting.next()
        {
            self.handle(message);
            self.deliver();
        }
    }

    pub fn handle(&mut self, message: Inbound<L>) {
        match message {
            Inbound::PreLogin {
                name,
                id,
                address,
                transferred,
                reply,
            } => self.pre_login(name, id, address, transferred, reply),
            Inbound::Join {
                profile,
                settings,
                link,
                address,
                session,
            } => self.join(profile, settings, link, address, session),
            Inbound::Input { player, input } => match self.dimension_of(player) {
                Some(key) => self.in_dimension(&key, |server| server.handle_input(player, input)),
                None => self.handle_input(player, input),
            },
            Inbound::Leave { player, session } => {
                if self
                    .players
                    .get(&player)
                    .is_some_and(|current| current.session == session)
                {
                    let key = self.dimension_of(player);
                    match key {
                        Some(key) => self.in_dimension(&key, |server| {
                            server.remove_player(player, true);
                        }),
                        None => self.remove_player(player, true),
                    }
                }
            }
            Inbound::Console(line) => self.console_line(line),
            Inbound::StatusPing { ping, reply } => self.status_ping(ping, reply),
            Inbound::PluginConnected {
                connection,
                intro,
                outbox,
            } => self.plugin_connected(connection, intro, outbox),
            Inbound::PluginMessage {
                connection,
                message,
            } => self.plugin_message(connection, message),
            Inbound::PluginDisconnected { connection } => self.plugin_disconnected(connection),
            Inbound::Shutdown => self.running = false,
        }
    }

    fn dimension_of(&self, player: PlayerId) -> Option<crate::world_state::DimensionKey> {
        self.players
            .get(&player)
            .map(|p| p.dim.clone())
            .filter(|key| self.dormant.contains_key(key))
    }

    fn plugin_connected(
        &mut self,
        connection: u64,
        intro: PluginIntro,
        outbox: Box<dyn PluginOutbox>,
    ) {
        let tick = self.current_tick();
        let plugin = PluginInfo {
            name: intro.name.clone(),
            version: intro.version.clone(),
        };
        let replaced = self.plugins.connect(connection, intro, outbox, tick);
        if let Some(replaced) = replaced {
            replaced.outbox.send(ToPlugin::Replaced);
            self.forget_plugin_inventories(replaced.connection);
            self.plugin_disabled(connection, replaced.plugin);
        }
        self.plugins.send(connection, ToPlugin::Accepted);
        self.refresh_ping_hook();
        self.plugin_enabled(connection, plugin);
        self.release_departed_plugins();
    }

    pub(crate) fn current_tick(&self) -> u64 {
        self.tick
    }

    pub(crate) fn allocate_entity_id(&mut self) -> EntityId {
        let id = EntityId(self.next_entity_id);
        self.next_entity_id += 1;
        id
    }

    pub(crate) fn allocate_entity_id_for(
        &mut self,
        kind: rspigot_entities::EntityKind,
    ) -> EntityId {
        let id = self.allocate_entity_id();
        if kind == rspigot_entities::EntityKind::EnderDragon {
            self.next_entity_id += rspigot_domain::dragon::parts::PART_COUNT as i32;
        }
        id
    }

    pub(crate) fn push(&mut self, audience: Audience, event: ClientEvent) {
        self.outbox.push((audience, event));
    }

    pub fn deliver(&mut self) {
        let active = self.world.dimension.key.clone();
        for (audience, event) in self.outbox.drain(..) {
            match audience {
                Audience::One(id) => {
                    if let Some(player) = self.players.get(&id) {
                        player.link.send(event);
                    }
                }
                Audience::All => {
                    for player in self.players.values() {
                        player.link.send(event.clone());
                    }
                }
                Audience::Dimension(key) => {
                    for player in self.players.values().filter(|p| p.dim == key) {
                        player.link.send(event.clone());
                    }
                }
                Audience::Trackers(entity) => {
                    for player in self
                        .players
                        .values()
                        .filter(|p| p.tracked.contains(&entity))
                    {
                        player.link.send(event.clone());
                    }
                }
                Audience::TrackersExcept(entity, excluded) => {
                    for player in self
                        .players
                        .values()
                        .filter(|p| p.id != excluded && p.tracked.contains(&entity))
                    {
                        player.link.send(event.clone());
                    }
                }
                Audience::ChunkViewers(pos) => {
                    for player in self
                        .players
                        .values()
                        .filter(|p| p.dim == active && p.streamer.has_sent(pos))
                    {
                        player.link.send(event.clone());
                    }
                }
                Audience::Nearby {
                    position,
                    range,
                    except,
                } => {
                    for player in self.players.values().filter(|p| {
                        p.dim == active
                            && Some(p.id) != except
                            && except.is_none_or(|source| !self.visibility.hides(p.id, source))
                            && p.position.distance_squared(position) < range * range
                    }) {
                        player.link.send(event.clone());
                    }
                }
            }
        }
    }

    pub(crate) fn list_entry(player: &Player<L>) -> PlayerListEntry {
        PlayerListEntry {
            profile: player.profile.clone(),
            game_mode: player.game_mode,
            latency_ms: 0,
            listed: true,
            display_name: None,
        }
    }

    pub(crate) fn spawn_info(&self, player: &Player<L>) -> SpawnInfo {
        SpawnInfo {
            dimension: (*self.world.dimension).clone(),
            hashed_seed: hashed_seed(self.world.level.seed),
            game_mode: player.game_mode,
            previous_game_mode: player.previous_game_mode,
            is_debug: false,
            is_flat: self.config.is_flat,
            death_location: None,
            portal_cooldown: 0,
        }
    }

    pub(crate) fn command_tree(&self, operator: bool) -> Arc<CommandTree> {
        if operator {
            Arc::clone(&self.operator_tree)
        } else {
            Arc::clone(&self.player_tree)
        }
    }

    pub(crate) fn next_inventory_id(&mut self) -> u64 {
        self.inventory_counter += 1;
        self.inventory_counter
    }

    pub(crate) fn next_login_attempt(&mut self) -> u64 {
        self.login_counter += 1;
        self.login_counter
    }

    pub(crate) fn park_arrival(&mut self, arrival: Arrival<L>) -> u64 {
        self.arrival_counter += 1;
        self.arrivals.insert(self.arrival_counter, arrival);
        self.arrival_counter
    }

    pub(crate) fn take_arrival(&mut self, key: u64) -> Option<Arrival<L>> {
        self.arrivals.remove(&key)
    }

    pub(crate) fn arrival(&self, key: u64) -> Option<&Arrival<L>> {
        self.arrivals.get(&key)
    }

    pub(crate) fn arrival_mut(&mut self, key: u64) -> Option<&mut Arrival<L>> {
        self.arrivals.get_mut(&key)
    }

    fn join(
        &mut self,
        profile: GameProfile,
        settings: ClientSettings,
        link: L,
        address: IpAddr,
        session: u64,
    ) {
        if self.players.contains_key(&profile.id) {
            self.disconnect_player(
                profile.id,
                Component::translatable("multiplayer.disconnect.duplicate_login", Vec::new()),
            );
        }
        self.ask_login(Arrival {
            profile,
            settings,
            link,
            address: address.to_canonical(),
            session,
            refusal: None,
        });
    }

    pub(crate) fn refusal(&mut self, profile: &GameProfile, address: IpAddr) -> Option<Refusal> {
        let now = self.prune_expired_bans();
        if let Some(ban) = self.admin.profile_ban(profile, now) {
            let reason = ban_reason("multiplayer.disconnect.banned", ban);
            if self.admin.learn_uuid(profile) {
                self.publish_admin();
            }
            return Some(Refusal {
                result: PlayerLoginResult::KickBanned,
                reason,
            });
        }
        if !self.admin.may_join(profile) {
            return Some(Refusal {
                result: PlayerLoginResult::KickWhitelist,
                reason: Component::text(self.config.whitelist_message.clone()),
            });
        }
        if let Some(ban) = self.admin.ip_ban(address, now) {
            return Some(Refusal {
                result: PlayerLoginResult::KickBanned,
                reason: ban_reason("multiplayer.disconnect.banned_ip", ban),
            });
        }
        let operator = self.admin.is_operator(profile);
        if self.players.len() as u32 >= self.config.max_players && !operator {
            return Some(Refusal {
                result: PlayerLoginResult::KickFull,
                reason: Component::translatable("multiplayer.disconnect.server_full", Vec::new()),
            });
        }
        None
    }

    pub(crate) fn arrive(&mut self, arrival: Arrival<L>) {
        let Arrival {
            profile,
            settings,
            link,
            address,
            session,
            ..
        } = arrival;
        let operator = self.admin.is_operator(&profile);
        let saved = self.load_saved_player(&profile);
        let entity_id = self.allocate_entity_id();
        let inventory_ids = [self.next_inventory_id(), self.next_inventory_id()];
        let position = saved
            .as_ref()
            .filter(|save| !save.is_dead())
            .map_or(self.world.spawn, |save| save.position);
        let view_distance = self.config.view_distance.min(settings.view_distance.max(2));
        let initial = settings.clone();
        let mut player = Player {
            id: profile.id,
            profile,
            link,
            address,
            session,
            entity_id,
            position,
            rotation: Rotation::new(self.world.level.spawn_angle, 0.0),
            on_ground: false,
            dim: self.world.dimension.key.clone(),
            game_mode: self.config.default_game_mode,
            previous_game_mode: None,
            inventory: Default::default(),
            inventory_state: 0,
            menu: Default::default(),
            drops: Default::default(),
            streamer: ChunkStreamer::new(self.config.stream, position.chunk(), view_distance),
            settings,
            pending_teleport: None,
            teleport_counter: 0,
            sneaking: false,
            sprinting: false,
            flying: false,
            allow_flight: None,
            fly_speed: 0.05,
            walk_speed: 0.1,
            latency: 0,
            tracked: FxHashSet::default(),
            loaded: false,
            operator,
            effects: Default::default(),
            vitals: Default::default(),
            known_movement: Vec3::ZERO,
            moved_this_tick: false,
            using_item: None,
            open_menu: None,
            plugin_menu: None,
            window_counter: 0,
            inventory_ids,
            ender_items: vec![None; 27],
            kick_leave_message: None,
            last_input: Default::default(),
            mining: None,
            delayed_destroy: None,
            use_memo: None,
        };
        if let Some(save) = &saved {
            save.restore(&mut player, self.world.spawn, self.world.level.spawn_angle);
            if save.dimension == self.world.dimension.key {
                player.dim = save.dimension.clone();
            }
        }
        tracing::info!(
            "{}[/{}] logged in with entity id {} at ([{}]{:.1}, {:.1}, {:.1})",
            player.name(),
            player.id,
            entity_id.0,
            self.world.level.name,
            position.x,
            position.y,
            position.z
        );
        self.send_join_sequence(&mut player);
        let entries: Vec<PlayerListEntry> = self
            .players
            .values()
            .map(Self::list_entry)
            .chain(std::iter::once(Self::list_entry(&player)))
            .collect();
        player.link.send(ClientEvent::PlayerInfo(entries));
        self.push(
            Audience::All,
            ClientEvent::PlayerInfo(vec![Self::list_entry(&player)]),
        );
        player
            .streamer
            .start(&mut self.world.chunks, &self.workers, &player.link);
        let id = player.id;
        self.players.insert(id, player);
        self.update_player_view(id);
        if let Some(save) = &saved {
            self.announce_restored(id, save);
        }
        self.refresh_status();
        self.announce_settings(id, &initial);
        self.send_commands(id);
        self.ask_join(id);
    }

    fn send_join_sequence(&self, player: &mut Player<L>) {
        let info = JoinInfo {
            entity_id: player.entity_id,
            hardcore: self.config.hardcore,
            dimension_names: vec![
                Identifier::minecraft("overworld"),
                Identifier::minecraft("the_nether"),
                Identifier::minecraft("the_end"),
            ],
            max_players: self.config.max_players,
            view_distance: self.config.view_distance,
            simulation_distance: self.config.simulation_distance,
            reduced_debug_info: false,
            enable_respawn_screen: !self.config.rules.immediate_respawn,
            do_limited_crafting: false,
            spawn: self.spawn_info(player),
            enforces_secure_chat: false,
        };
        let link = &player.link;
        link.send(ClientEvent::Join(Box::new(info)));
        link.send(ClientEvent::Difficulty {
            difficulty: self.world.difficulty,
            locked: false,
        });
        link.send(ClientEvent::Abilities(player.abilities()));
        link.send(ClientEvent::HeldSlot(player.inventory.selected_hotbar));
        link.send(ClientEvent::RecipeSync);
        link.send(ClientEvent::OperatorLevel {
            entity: player.entity_id,
            level: if player.operator { 4 } else { 0 },
        });
        link.send(ClientEvent::SpawnPosition {
            dimension: self.world.dimension.key.clone(),
            pos: self.world.spawn.block(),
            yaw: self.world.level.spawn_angle,
            pitch: 0.0,
        });
        let teleport = player.next_teleport_id();
        player.link.send(ClientEvent::Teleport {
            id: teleport,
            position: player.position,
            rotation: player.rotation,
        });
        let link = &player.link;
        link.send(self.time_event());
        if self.world.level.raining {
            link.send(ClientEvent::GameEvent(GameEvent::StartRaining));
            link.send(ClientEvent::GameEvent(GameEvent::RainLevelChange(1.0)));
            if self.world.level.thundering {
                link.send(ClientEvent::GameEvent(GameEvent::ThunderLevelChange(1.0)));
            }
        }
        link.send(ClientEvent::GameEvent(GameEvent::LevelChunksLoadStart));
        let (slots, carried) = player.menu.sync_all(&player.inventory);
        link.send(ClientEvent::InventoryContent {
            state_id: 0,
            slots,
            carried,
        });
        link.send(ClientEvent::Health {
            health: player.vitals.health,
            food: player.vitals.food.food,
            saturation: player.vitals.food.saturation,
        });
        link.send(ClientEvent::Experience {
            progress: 0.0,
            level: 0,
            total: 0,
        });
        player.vitals.mark_sent();
    }

    pub(crate) fn time_event(&self) -> ClientEvent {
        ClientEvent::Time {
            game_time: self.world.level.game_time,
            day_time: self.world.level.day_time,
            advance_time: self.world.level.daylight_cycle && !self.world.dimension.has_fixed_time,
        }
    }

    pub(crate) fn remove_player(&mut self, id: PlayerId, announce: bool) {
        if !self.players.contains_key(&id) {
            return;
        }
        self.settle_lane(Lane::Player(id), false);
        if !self.players.contains_key(&id) {
            return;
        }
        self.close_open_menu(id, false);
        if let Some(entity) = self.players.get(&id).map(|player| player.entity_id) {
            self.unride(entity);
        }
        let Some(snapshot) = self.snapshot_of(id) else {
            return;
        };
        let forgotten = self.decisions.forget(id);
        self.notify_undone(forgotten, crate::plugins::Phase::Settled);
        self.visibility.forget(id);
        let Some(mut player) = self.players.remove(&id) else {
            return;
        };
        self.persist_player(&player);
        player.streamer.release_all(&mut self.world.chunks);
        for other in self.players.values_mut() {
            if other.tracked.remove(&player.entity_id) {
                other
                    .link
                    .send(ClientEvent::RemoveEntities(vec![player.entity_id]));
            }
        }
        self.push(Audience::All, ClientEvent::PlayerInfoRemove(vec![id]));
        self.refresh_status();
        self.leave_event(snapshot, player.kick_leave_message.take(), announce);
    }

    fn refresh_status(&self) {
        let sample: Vec<PlayerSample> = self
            .players
            .values()
            .take(12)
            .map(|p| PlayerSample {
                id: p.id,
                name: p.profile.name.clone(),
            })
            .collect();
        let online = self.players.len() as u32;
        self.status.update(|s| {
            s.online = online;
            s.sample = sample;
        });
    }

    pub fn tick(&mut self) {
        self.tick += 1;
        self.tps.record(self.tick);
        self.tick_world(true);
        let mut keys: Vec<crate::world_state::DimensionKey> = self
            .dormant
            .keys()
            .filter(|key| self.players.values().any(|p| &p.dim == *key))
            .cloned()
            .collect();
        keys.sort();
        for key in keys {
            self.tick_dormant(&key);
        }
    }

    fn tick_dormant(&mut self, key: &crate::world_state::DimensionKey) {
        let Some(mut state) = self.dormant.remove(key) else {
            return;
        };
        self.swap_world(&mut state);
        self.drain_world_results();
        self.tick_world(false);
        self.swap_world(&mut state);
        self.dormant.insert(key.clone(), state);
    }

    pub(crate) fn drain_world_results(&mut self) {
        while let Ok(result) = self.workers.results().try_recv() {
            self.world.chunks.accept(result);
        }
    }

    fn tick_world(&mut self, primary: bool) {
        let home = self.world.dimension.key.clone();
        let mut away = self.set_aside_players_outside(&home);
        self.tick_world_here(primary, &home, &mut away);
        self.players.extend(away);
        self.deliver();
    }

    fn set_aside_players_outside(
        &mut self,
        key: &crate::world_state::DimensionKey,
    ) -> Vec<(PlayerId, Player<L>)> {
        let (here, away): (Vec<_>, Vec<_>) = std::mem::take(&mut self.players)
            .into_iter()
            .partition(|(_, player)| &player.dim == key);
        self.players.extend(here);
        away
    }

    fn tick_world_here(
        &mut self,
        primary: bool,
        home: &crate::world_state::DimensionKey,
        away: &mut Vec<(PlayerId, Player<L>)>,
    ) {
        self.world.level.game_time += 1;
        if self.world.level.daylight_cycle && !self.world.dimension.has_fixed_time {
            self.world.level.day_time += 1;
        }
        for pos in self.world.chunks.tick() {
            self.ask_chunk_unload(pos);
        }
        self.handle_chunk_events();
        for player in self.players.values_mut() {
            player.streamer.tick(&self.world.chunks, &player.link);
        }
        self.world.chunks.relight();
        self.send_entity_changes();
        self.tick_health();
        self.tick_eating();
        self.tick_delayed_destroys();
        self.tick_spawning();
        self.tick_patrol_spawner();
        self.tick_phantom_spawner();
        self.tick_trader_spawner();
        self.tick_raids();
        self.tick_dragon_fight();
        self.tick_entities();
        self.tick_ride_players();
        self.tick_golem_chests();
        self.collect_items();
        self.collect_arrows();
        self.collect_orbs();
        self.tick_furnaces();
        self.tick_game_events();
        self.tick_campfires();
        self.tick_beehives();
        self.tick_inventories();
        if primary {
            self.players.extend(away.drain(..));
            self.tick_plugins();
            *away = self.set_aside_players_outside(home);
        }
        self.tick_open_menus();
        self.tick_falling_blocks();
        self.tick_turtle_eggs();
        self.tick_fluids();
        self.update_tracking();
        self.notify_poses();
        self.sync_vitals();
        if self.tick.is_multiple_of(20) {
            let time = self.time_event();
            self.push(Audience::Dimension(self.world.dimension.key.clone()), time);
        }
        let disconnected: Vec<PlayerId> = self
            .players
            .values()
            .filter(|p| !p.link.is_connected())
            .map(|p| p.id)
            .collect();
        for id in disconnected {
            self.remove_player(id, true);
        }
    }

    pub(crate) fn metadata(player: &Player<L>) -> Vec<MetadataEntry> {
        vec![
            MetadataEntry {
                index: 0,
                value: MetadataValue::Byte(player.shared_flags()),
            },
            MetadataEntry {
                index: 6,
                value: MetadataValue::Pose(if player.sneaking && !player.flying {
                    Pose::Crouching
                } else {
                    Pose::Standing
                }),
            },
        ]
        .into_iter()
        .chain((player.vitals.arrow_count > 0).then(|| MetadataEntry {
            index: 12,
            value: MetadataValue::VarInt(player.vitals.arrow_count),
        }))
        .collect()
    }

    fn update_tracking(&mut self) {
        struct Target {
            id: PlayerId,
            entity: EntityId,
            position: Vec3,
            rotation: Rotation,
            chunk: ChunkPos,
            dim: crate::world_state::DimensionKey,
            metadata: Vec<MetadataEntry>,
            held: Option<rspigot_domain::item::ItemStack>,
        }
        let targets: Vec<Target> = self
            .players
            .values()
            .filter(|p| !p.vitals.removed)
            .map(|p| Target {
                id: p.id,
                entity: p.entity_id,
                position: p.position,
                rotation: p.rotation,
                chunk: p.position.chunk(),
                dim: p.dim.clone(),
                metadata: Self::metadata(p)
                    .into_iter()
                    .chain(Self::health_metadata(p))
                    .collect(),
                held: p
                    .inventory
                    .get(HOTBAR_START + usize::from(p.inventory.selected_hotbar))
                    .cloned(),
            })
            .collect();
        let range = self.config.player_tracking_range * self.config.player_tracking_range;
        let visibility = &self.visibility;
        let rides = &self.entities.rides;
        for viewer in self.players.values_mut() {
            for target in &targets {
                if target.id == viewer.id {
                    continue;
                }
                let visible = viewer.dim == target.dim
                    && viewer.position.distance_squared(target.position) <= range
                    && viewer.streamer.has_sent(target.chunk)
                    && !visibility.hides(viewer.id, target.id);
                let tracking = viewer.tracked.contains(&target.entity);
                if visible && !tracking {
                    viewer.tracked.insert(target.entity);
                    viewer.link.send(ClientEvent::SpawnEntity {
                        entity: target.entity,
                        uuid: target.id.0,
                        kind: EntityKind::Player,
                        position: target.position,
                        rotation: target.rotation,
                        head_yaw: target.rotation.yaw,
                        data: 0,
                        velocity: Vec3::ZERO,
                    });
                    viewer.link.send(ClientEvent::EntityHeadRotation {
                        entity: target.entity,
                        yaw: target.rotation.yaw,
                    });
                    viewer.link.send(ClientEvent::EntityMetadata {
                        entity: target.entity,
                        entries: target.metadata.clone(),
                    });
                    if let Some(vehicle) = rides.vehicle_of(target.entity) {
                        viewer.link.send(ClientEvent::Passengers {
                            vehicle,
                            passengers: rides.passengers_of(vehicle).to_vec(),
                        });
                    }
                    if target.held.is_some() {
                        viewer.link.send(ClientEvent::EntityEquipment {
                            entity: target.entity,
                            slots: vec![(EquipmentSlot::MainHand, target.held.clone())],
                        });
                    }
                } else if !visible && tracking {
                    viewer.tracked.remove(&target.entity);
                    viewer
                        .link
                        .send(ClientEvent::RemoveEntities(vec![target.entity]));
                }
            }
        }
    }

    pub(crate) fn tps(&self) -> [f64; 3] {
        self.tps.recent
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn players_online(&self) -> usize {
        self.players.len()
    }

    pub fn chunk_stats(&self) -> crate::chunks::ChunkManagerStats {
        self.world.chunks.stats()
    }

    fn shutdown(mut self, inbound: &Receiver<Inbound<L>>) -> LevelData {
        tracing::info!("Stopping server");
        self.status.update(|status| status.ping = PingHook::Off);
        queued(inbound).for_each(drop);
        self.settle_every_lane();
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for id in ids {
            if let Some(player) = self.players.get(&id) {
                player.link.disconnect(Component::translatable(
                    "multiplayer.disconnect.server_shutdown",
                    Vec::new(),
                ));
            }
            self.remove_player(id, false);
        }
        self.world_saving_at_shutdown();
        self.disable_plugins_at_shutdown();
        self.outbox.clear();
        tracing::info!(
            "Saving chunks for level '{}'/{}",
            self.world.level.name,
            self.world.dimension.key
        );
        self.handle_chunk_events();
        self.save_all_entities();
        self.save_raids();
        self.save_dragon_fight();
        let saved = self.world.chunks.save_dirty(&self.workers);
        self.workers.submit(Priority::BACKGROUND, ChunkJob::Flush);
        let keys: Vec<crate::world_state::DimensionKey> = self.dormant.keys().cloned().collect();
        for key in keys {
            let Some(mut state) = self.dormant.remove(&key) else {
                continue;
            };
            self.swap_world(&mut state);
            self.save_all_entities();
            self.save_world_data();
            self.world.chunks.save_dirty(&self.workers);
            self.workers.submit(Priority::BACKGROUND, ChunkJob::Flush);
            self.swap_world(&mut state);
            state.workers.shutdown();
        }
        self.workers.shutdown();
        if let Some(io) = self.player_io.take() {
            io.shutdown();
        }
        let stats = self.world.chunks.stats();
        tracing::info!(
            "Saved {saved} chunks ({} loaded from disk, {} generated, {} unloaded, {} cancelled, average load {:.2} ms)",
            stats.loaded_from_disk,
            stats.generated,
            stats.unloaded,
            stats.cancelled,
            stats.average_load_time().as_secs_f64() * 1000.0
        );
        self.world.level
    }
}

fn ban_reason<T>(key: &str, ban: &Ban<T>) -> Component {
    let mut reason = Component::translatable(format!("{key}.reason"), vec![ban.reason_message()]);
    if let Some(date) = ban
        .expires
        .and_then(|expires| format_ban_date(expires, KICK_DATE))
    {
        reason = reason.append(Component::translatable(
            format!("{key}.expiration"),
            vec![Component::text(date)],
        ));
    }
    reason
}

fn queued<T>(inbound: &Receiver<T>) -> impl Iterator<Item = T> {
    let waiting = inbound.len();
    inbound.try_iter().take(waiting)
}

#[cfg(test)]
mod tests {
    use crossbeam_channel::unbounded;

    use super::queued;

    #[test]
    fn queued_takes_the_waiting_messages_in_order_and_no_more() {
        let (sender, receiver) = unbounded();
        for message in 1..=3 {
            sender.send(message).expect("open");
        }
        let mut batch = queued(&receiver);
        assert_eq!(batch.next(), Some(1));
        sender.send(4).expect("open");
        assert_eq!(batch.collect::<Vec<_>>(), [2, 3]);
        assert_eq!(queued(&receiver).collect::<Vec<_>>(), [4]);
        assert_eq!(queued(&receiver).count(), 0);
    }
}
