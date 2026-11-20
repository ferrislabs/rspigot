use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, TryRecvError};

use rspigot_domain::entity::{Entity, EntityState, RemovalReason, thrown};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{EntityId, Vec3, mth};
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;

use crate::plugins::events::EntityRemoveCause;
use crate::ports::ClientLink;
use crate::server::Server;

const SIGNAL_RANGE: f64 = 12.0;
const SIGNAL_RISE: f64 = 8.0;
const LIFE_TICKS: i32 = 80;
const START_SPEED: f64 = 0.3;
const SPEED_LERP: f64 = 0.0025;
const HOVER_RANGE: f64 = 1.0;
const HOVER_DAMPING: f64 = 0.8;
const CLIMB_LERP: f64 = 0.015;
const SURVIVE_ODDS: i32 = 5;
const PICKUP_DELAY: u16 = 10;
const SHATTER_EVENT: i32 = 2003;
const LOCATE_TIMEOUT: Duration = Duration::from_secs(30);
const DEATH_SOUND: &str = "minecraft:entity.ender_eye.death";

pub(crate) fn signal_target(from: Vec3, target: Vec3) -> Vec3 {
    let (dx, dz) = (target.x - from.x, target.z - from.z);
    let distance = (dx * dx + dz * dz).sqrt();
    if distance > SIGNAL_RANGE {
        Vec3::new(
            from.x + dx / distance * SIGNAL_RANGE,
            from.y + SIGNAL_RISE,
            from.z + dz / distance * SIGNAL_RANGE,
        )
    } else {
        target
    }
}

pub(crate) struct PendingEye {
    player: PlayerId,
    hand: crate::events::Hand,
    origin: Vec3,
    dimension: crate::world_state::DimensionKey,
    started: Instant,
    answer: Receiver<Option<Vec3>>,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn use_eye_of_ender(&mut self, id: PlayerId, hand: crate::events::Hand) -> bool {
        let Some(origin) = self.players.get(&id).map(|p| p.position) else {
            return false;
        };
        if self.pending_eyes.iter().any(|pending| pending.player == id) {
            return true;
        }
        let answer = self
            .workers
            .request_stronghold(&self.world.dimension, origin);
        self.pending_eyes.push(PendingEye {
            player: id,
            hand,
            origin,
            dimension: self.world.dimension.key.clone(),
            started: Instant::now(),
            answer,
        });
        true
    }

    pub(crate) fn tick_pending_eyes(&mut self) {
        if self.pending_eyes.is_empty() {
            return;
        }
        let mut ready = Vec::new();
        let dimension = &self.world.dimension.key;
        self.pending_eyes.retain(|pending| {
            if &pending.dimension != dimension {
                return true;
            }
            match pending.answer.try_recv() {
                Err(TryRecvError::Empty) => true,
                Err(TryRecvError::Disconnected) | Ok(None) => false,
                Ok(Some(target)) => {
                    ready.push((pending.player, pending.hand, pending.origin, target));
                    false
                }
            }
        });
        for (player, hand, origin, target) in ready {
            let here = self
                .players
                .get(&player)
                .is_some_and(|p| p.dim == self.world.dimension.key);
            if here {
                self.launch_eye_from(player, hand, origin, target);
            }
        }
    }

    pub(crate) fn expire_pending_eyes(&mut self) {
        self.pending_eyes
            .retain(|pending| pending.started.elapsed() < LOCATE_TIMEOUT);
    }

    pub(crate) fn forget_pending_eyes(&mut self, id: PlayerId) {
        self.pending_eyes.retain(|pending| pending.player != id);
    }

    pub(crate) fn tick_eye(&mut self, id: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Thrown(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        base.old_position = base.position;
        base.tick_count = base.tick_count.wrapping_add(1);
        base.position = base.position + base.velocity;
        base.needs_sync = true;
        if let Some(target) = state.target {
            let (dx, dz) = (target.x - base.position.x, target.z - base.position.z);
            let remaining = (dx * dx + dz * dz).sqrt();
            let speed =
                (base.velocity.x * base.velocity.x + base.velocity.z * base.velocity.z).sqrt();
            let angle = mth::atan2(dz, dx);
            let mut horizontal = speed + (remaining - speed) * SPEED_LERP;
            let mut vertical = base.velocity.y;
            if remaining < HOVER_RANGE {
                horizontal *= HOVER_DAMPING;
                vertical *= HOVER_DAMPING;
            }
            let climb = if base.position.y < target.y {
                1.0
            } else {
                -1.0
            };
            base.velocity = Vec3::new(
                angle.cos() * horizontal,
                vertical + (climb - vertical) * CLIMB_LERP,
                angle.sin() * horizontal,
            );
        }
        thrown::update_rotation(base);
        if base.tick_count > LIFE_TICKS {
            self.finish_eye(id);
        }
    }

    fn finish_eye(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let position = entity.base.position;
        let stack = thrown::item(&entity.base);
        self.play_sound(None, position, DEATH_SOUND, SoundSource::Neutral, 1.0, 1.0);
        self.remove_entity(
            id,
            RemovalReason::Discarded,
            Some(EntityRemoveCause::Despawn),
        );
        let survives = rand::Rng::random_range(&mut self.rng, 0..SURVIVE_ODDS) > 0;
        match (survives, stack) {
            (true, Some(stack)) => {
                self.spawn_item_entity(position, Vec3::ZERO, PICKUP_DELAY, stack);
            }
            _ => self.level_event(SHATTER_EVENT, position.block(), 0),
        }
    }

    #[cfg(test)]
    pub(crate) fn launch_eye(
        &mut self,
        id: PlayerId,
        hand: crate::events::Hand,
        target: Vec3,
    ) -> bool {
        let Some(position) = self.players.get(&id).map(|p| p.position) else {
            return false;
        };
        self.launch_eye_from(id, hand, position, target)
    }

    pub(crate) fn launch_eye_from(
        &mut self,
        id: PlayerId,
        hand: crate::events::Hand,
        position: Vec3,
        target: Vec3,
    ) -> bool {
        let Some(stack) = self
            .held(id, hand)
            .filter(|stack| stack.item.kind() == Some(ItemKind::EnderEye))
        else {
            return false;
        };
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let origin = Vec3::new(position.x, position.y + player.height() * 0.5, position.z);
        let owner = player.profile.id.0;
        let aim = signal_target(origin, target);
        let mut base = self.new_entity_base(rspigot_entities::EntityKind::EyeOfEnder);
        base.position = origin;
        base.old_position = origin;
        let seed = rand::Rng::random::<i64>(&mut self.rng);
        let mut state = thrown::ThrownState::new(&mut base, seed);
        thrown::set_item(&mut base, &stack);
        state.owner = Some(owner);
        state.target = Some(aim);
        let angle = mth::atan2(aim.z - origin.z, aim.x - origin.x);
        base.velocity = Vec3::new(angle.cos() * START_SPEED, 0.0, angle.sin() * START_SPEED);
        base.needs_sync = true;
        let eye = Entity::new(base, EntityState::Thrown(Box::new(state)));
        self.spawn_entity(
            eye,
            crate::plugins::steps::Birth::of(crate::plugins::events::CreatureSpawnReason::Default)
                .by(id)
                .then(crate::plugins::steps::Spawned::Thrown {
                    player: id,
                    hand,
                    stack,
                }),
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use rspigot_domain::item::{HOTBAR_START, ItemStack};
    use rspigot_domain::player::{ClientSettings, GameMode, GameProfile};
    use rspigot_domain::text::Component;
    use rspigot_entities::EntityKind as Kind;
    use rspigot_items::ItemKind;

    use super::*;
    use crate::config::{GameConfig, LevelData};
    use crate::events::{ClientEvent, Hand, Inbound};
    use crate::ports::ChunkGenerator;
    use crate::status::StatusBoard;
    use crate::world_state::tests::Quiet;
    use rspigot_domain::ChunkPos;
    use rspigot_domain::chunk::ChunkColumn;
    use rspigot_domain::world::Dimension;

    #[derive(Clone)]
    struct Mute;

    impl ClientLink for Mute {
        fn send(&self, _: ClientEvent) {}

        fn disconnect(&self, _: Component) {}

        fn is_connected(&self) -> bool {
            true
        }
    }

    struct Gate {
        open: std::sync::Mutex<bool>,
        signal: std::sync::Condvar,
    }

    impl Gate {
        fn new(open: bool) -> Arc<Self> {
            Arc::new(Self {
                open: std::sync::Mutex::new(open),
                signal: std::sync::Condvar::new(),
            })
        }

        fn release(&self) {
            *self.open.lock().unwrap() = true;
            self.signal.notify_all();
        }

        fn wait(&self) {
            let mut open = self.open.lock().unwrap();
            while !*open {
                open = self.signal.wait(open).unwrap();
            }
        }
    }

    struct Release(Arc<Gate>);

    impl Drop for Release {
        fn drop(&mut self) {
            self.0.release();
        }
    }

    struct Beacon {
        target: Option<Vec3>,
        gate: Arc<Gate>,
        lookups: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl ChunkGenerator for Beacon {
        fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
            Quiet.generate(dimension, pos)
        }

        fn spawn_point(&self, dimension: &Dimension) -> Vec3 {
            Quiet.spawn_point(dimension)
        }

        fn nearest_stronghold(&self, _: &Dimension, _: Vec3) -> Option<Vec3> {
            self.lookups
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.gate.wait();
            self.target
        }
    }

    fn arrive(mode: GameMode) -> (Server<Mute>, PlayerId) {
        arrive_with(mode, None)
    }

    fn arrive_with(mode: GameMode, target: Option<Vec3>) -> (Server<Mute>, PlayerId) {
        let (server, id, _, _) = arrive_gated(mode, target, true);
        (server, id)
    }

    type Lookups = Arc<std::sync::atomic::AtomicUsize>;

    fn arrive_gated(
        mode: GameMode,
        target: Option<Vec3>,
        open: bool,
    ) -> (Server<Mute>, PlayerId, Arc<Gate>, Lookups) {
        let gate = Gate::new(open);
        let lookups = Lookups::default();
        let beacon = Beacon {
            target,
            gate: Arc::clone(&gate),
            lookups: Arc::clone(&lookups),
        };
        let config = GameConfig {
            chunk_workers: 2,
            spawn_chunk_radius: 1,
            ..GameConfig::default()
        };
        let mut server = Server::new(
            config,
            LevelData::default(),
            Arc::new(Quiet),
            Arc::new(beacon),
            StatusBoard::new(1),
        )
        .with_rng_seed(5);
        server.wait_for_spawn(Duration::from_secs(10));
        let id = rspigot_domain::player::PlayerId::offline("Alice");
        server.handle(Inbound::Join {
            profile: GameProfile {
                id,
                name: "Alice".to_owned(),
                properties: Vec::new(),
            },
            settings: ClientSettings::default(),
            link: Mute,
            address: [127, 0, 0, 1].into(),
            session: 1,
        });
        server.deliver();
        let player = server.players.get_mut(&id).unwrap();
        player.game_mode = mode;
        player.inventory.set(
            HOTBAR_START,
            Some(ItemStack::new(
                ItemKind::EnderEye.id(),
                std::num::NonZeroU8::new(2).unwrap(),
            )),
        );
        (server, id, gate, lookups)
    }

    fn tick_until_eye(server: &mut Server<Mute>) -> bool {
        for _ in 0..400 {
            server.tick();
            server.deliver();
            if eyes(server) > 0 {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    fn count(server: &Server<Mute>, id: PlayerId) -> Option<u8> {
        server.players[&id]
            .inventory
            .get(HOTBAR_START)
            .map(|stack| stack.count.get())
    }

    fn eyes(server: &Server<Mute>) -> usize {
        server.entities_of(Kind::EyeOfEnder).len()
    }

    #[test]
    fn a_far_target_is_pulled_to_twelve_blocks_and_raised() {
        let from = Vec3::new(0.0, 64.0, 0.0);
        let near = Vec3::new(3.0, 20.0, 4.0);
        assert_eq!(signal_target(from, near), near);
        let far = signal_target(from, Vec3::new(100.0, 0.0, 0.0));
        assert_eq!(far, Vec3::new(12.0, 72.0, 0.0));
    }

    #[test]
    fn a_thrown_eye_spends_one_item_flies_toward_the_target_then_ends() {
        let (mut server, id) = arrive(GameMode::Survival);
        let start = server.players[&id].position;
        assert!(server.launch_eye(id, Hand::Main, Vec3::new(start.x + 40.0, 64.0, start.z)));
        server.deliver();
        assert_eq!(eyes(&server), 1);
        assert_eq!(
            server.players[&id]
                .inventory
                .get(HOTBAR_START)
                .map(|stack| stack.count.get()),
            Some(1)
        );
        for _ in 0..20 {
            server.tick();
        }
        let flown = server.entities_of(Kind::EyeOfEnder)[0].position.x - start.x;
        assert!(flown > 3.0 && flown <= SIGNAL_RANGE + 1.0, "{flown}");
        for _ in 0..70 {
            server.tick();
            server.deliver();
        }
        assert_eq!(eyes(&server), 0);
    }

    #[test]
    fn creative_players_keep_their_eye() {
        let (mut server, id) = arrive(GameMode::Creative);
        assert!(server.launch_eye(id, Hand::Main, Vec3::new(30.0, 64.0, 0.0)));
        server.deliver();
        assert_eq!(
            server.players[&id]
                .inventory
                .get(HOTBAR_START)
                .map(|stack| stack.count.get()),
            Some(2)
        );
    }

    #[test]
    fn using_the_eye_flies_it_toward_the_nearest_stronghold() {
        let target = Vec3::new(300.0, 0.0, 9.0);
        let (mut server, id) = arrive_with(GameMode::Survival, Some(target));
        let start = server.players[&id].position;
        assert!(server.use_eye_of_ender(id, Hand::Main));
        assert!(tick_until_eye(&mut server));
        for _ in 0..20 {
            server.tick();
        }
        let flown = server.entities_of(Kind::EyeOfEnder)[0].position.x - start.x;
        assert!(flown > 3.0 && flown <= SIGNAL_RANGE + 1.0, "{flown}");
        assert_eq!(count(&server, id), Some(1));
    }

    #[test]
    fn using_the_eye_does_not_wait_and_launches_once_the_answer_is_ready() {
        let target = Vec3::new(300.0, 32.0, 9.0);
        let (mut server, id, gate, lookups) = arrive_gated(GameMode::Survival, Some(target), false);
        let _release = Release(Arc::clone(&gate));
        let started = std::time::Instant::now();
        assert!(server.use_eye_of_ender(id, Hand::Main));
        server.tick();
        server.deliver();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(eyes(&server), 0);
        assert_eq!(count(&server, id), Some(2));
        gate.release();
        assert!(tick_until_eye(&mut server));
        assert_eq!(count(&server, id), Some(1));
        assert_eq!(lookups.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn a_second_use_while_pending_starts_no_second_lookup() {
        let target = Vec3::new(300.0, 32.0, 9.0);
        let (mut server, id, gate, lookups) = arrive_gated(GameMode::Survival, Some(target), false);
        let _release = Release(Arc::clone(&gate));
        assert!(server.use_eye_of_ender(id, Hand::Main));
        assert!(server.use_eye_of_ender(id, Hand::Main));
        assert_eq!(server.pending_eyes.len(), 1);
        gate.release();
        assert!(tick_until_eye(&mut server));
        for _ in 0..5 {
            server.tick();
            server.deliver();
        }
        assert_eq!(eyes(&server), 1);
        assert_eq!(lookups.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(count(&server, id), Some(1));
    }

    #[test]
    fn without_a_stronghold_the_eye_does_nothing_and_is_kept() {
        let (mut server, id, gate, _) = arrive_gated(GameMode::Survival, None, false);
        let _release = Release(Arc::clone(&gate));
        assert!(server.use_eye_of_ender(id, Hand::Main));
        gate.release();
        for _ in 0..100 {
            server.tick();
            server.deliver();
            if server.pending_eyes.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(server.pending_eyes.is_empty());
        assert_eq!(eyes(&server), 0);
        assert_eq!(count(&server, id), Some(2));
    }

    #[test]
    fn a_player_who_left_before_the_answer_launches_nothing() {
        let target = Vec3::new(300.0, 32.0, 9.0);
        let (mut server, id, gate, _) = arrive_gated(GameMode::Survival, Some(target), false);
        let _release = Release(Arc::clone(&gate));
        assert!(server.use_eye_of_ender(id, Hand::Main));
        server.remove_player(id, false);
        gate.release();
        for _ in 0..40 {
            server.tick();
            server.deliver();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(eyes(&server), 0);
        assert!(server.pending_eyes.is_empty());
    }

    #[test]
    fn a_vanished_item_before_the_answer_launches_nothing() {
        let target = Vec3::new(300.0, 32.0, 9.0);
        let (mut server, id, gate, _) = arrive_gated(GameMode::Survival, Some(target), false);
        let _release = Release(Arc::clone(&gate));
        assert!(server.use_eye_of_ender(id, Hand::Main));
        server.set_held(id, Hand::Main, None);
        gate.release();
        for _ in 0..40 {
            server.tick();
            server.deliver();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(eyes(&server), 0);
        assert!(server.pending_eyes.is_empty());
    }
}
