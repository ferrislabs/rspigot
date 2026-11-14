use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, EntityId, Identifier, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::config::{GameConfig, LevelData};
use crate::events::{ClientEvent, Inbound};
use crate::ports::{ChunkGenerator, ClientLink};
use crate::server::Server;
use crate::status::StatusBoard;
use crate::travel::Destination;
use crate::world_state::tests::Quiet;

#[derive(Clone, Default)]
pub(crate) struct Tap(Arc<Mutex<Vec<ClientEvent>>>);

impl Tap {
    pub(crate) fn take(&self) -> Vec<ClientEvent> {
        self.0
            .lock()
            .map(|mut events| std::mem::take(&mut *events))
            .unwrap_or_default()
    }
}

impl ClientLink for Tap {
    fn send(&self, event: ClientEvent) {
        if let Ok(mut events) = self.0.lock() {
            events.push(event);
        }
    }

    fn disconnect(&self, _: Component) {}

    fn is_connected(&self) -> bool {
        true
    }
}

pub(crate) struct Isles;

impl ChunkGenerator for Isles {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, dimension.height, rspigot_domain::BiomeId(0));
        for x in 0..16 {
            for z in 0..16 {
                let at = BlockPos::new(pos.min_block_x() + x, 62, pos.min_block_z() + z);
                chunk.set_block(at, BlockKind::EndStone.default_state());
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }
}

pub(crate) fn end() -> Identifier {
    Identifier::minecraft("the_end")
}

pub(crate) fn provide(server: &mut Server<Tap>, key: &Identifier) {
    if *key == end() {
        server.load_dimension(
            key.clone(),
            Dimension::end(),
            Arc::new(Isles),
            Arc::new(Quiet),
        );
    }
}

pub(crate) fn arrive() -> (Server<Tap>, PlayerId, Tap) {
    let config = GameConfig {
        chunk_workers: 2,
        spawn_chunk_radius: 1,
        ..GameConfig::default()
    };
    let mut server = Server::new(
        config,
        LevelData::default(),
        Arc::new(Quiet),
        Arc::new(Quiet),
        StatusBoard::new(1),
    )
    .with_rng_seed(5)
    .with_dimension_provider(provide);
    server.wait_for_spawn(Duration::from_secs(10));
    let link = Tap::default();
    let id = PlayerId::offline("Alice");
    server.handle(Inbound::Join {
        profile: GameProfile {
            id,
            name: "Alice".to_owned(),
            properties: Vec::new(),
        },
        settings: ClientSettings::default(),
        link: link.clone(),
        address: [127, 0, 0, 1].into(),
        session: 1,
    });
    server.deliver();
    link.take();
    (server, id, link)
}

pub(crate) fn enter_end(server: &mut Server<Tap>, id: PlayerId) {
    server.change_dimension_now(id, Destination::end_spawn());
    server.deliver();
}

pub(crate) fn run_until(
    server: &mut Server<Tap>,
    limit: usize,
    done: impl Fn(&Server<Tap>) -> bool,
) {
    for _ in 0..limit {
        if done(server) {
            return;
        }
        server.tick();
        std::thread::sleep(Duration::from_millis(2));
    }
}

pub(crate) fn end_dragons(server: &Server<Tap>) -> usize {
    server.dormant[&end()]
        .entities
        .store
        .iter()
        .filter(|entity| entity.base.kind == Kind::EnderDragon)
        .count()
}

#[test]
fn the_end_starts_its_fight_when_a_player_arrives() {
    let (mut server, id, _) = arrive();
    enter_end(&mut server, id);
    assert!(server.dormant[&end()].entities.fight.is_some());
    run_until(&mut server, 3000, |server| end_dragons(server) == 1);
    assert_eq!(end_dragons(&server), 1);
    let fight = server.dormant[&end()]
        .entities
        .fight
        .as_ref()
        .expect("a fight");
    assert!(fight.data.dragon_uuid.is_some());
    assert!(!fight.data.needs_state_scanning);
    assert!(fight.data.exit_portal_location.is_some());
    assert!(fight.bar.players().any(|player| player == id));
}

#[test]
fn the_overworld_has_no_fight_of_its_own() {
    let (server, _, _) = arrive();
    assert!(server.entities.fight.is_none());
}

fn the_dragon(server: &Server<Tap>) -> Option<EntityId> {
    server.dormant[&end()]
        .entities
        .store
        .iter()
        .find(|entity| entity.base.kind == Kind::EnderDragon)
        .map(|entity| entity.base.id)
}

fn count_in_column(server: &Server<Tap>, kind: BlockKind) -> usize {
    (40..90)
        .filter(|y| {
            server.dormant[&end()]
                .world
                .chunks
                .block(BlockPos::new(0, *y, 0))
                .and_then(BlockStateId::kind)
                == Some(kind)
        })
        .count()
}

fn kill_the_dragon(server: &mut Server<Tap>) -> i32 {
    let dragon = the_dragon(server).expect("a dragon");
    server.in_dimension(&end(), |server| {
        let reward = server.dragon_experience_reward(dragon);
        server.dragon_kill(dragon);
        reward
    })
}

#[test]
fn killing_the_dragon_leaves_the_exit_portal_and_one_egg_only_the_first_time() {
    let (mut server, id, _) = arrive();
    enter_end(&mut server, id);
    run_until(&mut server, 3000, |server| end_dragons(server) == 1);
    assert_eq!(kill_the_dragon(&mut server), 12_000);
    assert_eq!(count_in_column(&server, BlockKind::DragonEgg), 1);
    assert!(count_in_column(&server, BlockKind::Bedrock) >= 4);
    let fight = server.dormant[&end()]
        .entities
        .fight
        .as_ref()
        .expect("fight");
    assert!(fight.data.dragon_killed && fight.data.previously_killed);
    assert_eq!(fight.data.gateways.len(), 19);
    let egg = (40..90)
        .map(|y| BlockPos::new(0, y, 0))
        .find(|pos| {
            server.dormant[&end()]
                .world
                .chunks
                .block(*pos)
                .and_then(BlockStateId::kind)
                == Some(BlockKind::DragonEgg)
        })
        .expect("the egg");
    server.in_dimension(&end(), |server| {
        server.set_block(egg, BlockStateId::AIR);
        if let Some(fight) = server.entities.fight.as_mut() {
            fight.data.dragon_killed = false;
            fight.data.dragon_uuid = None;
        }
    });
    run_until(&mut server, 200, |server| end_dragons(server) == 1);
    assert_eq!(kill_the_dragon(&mut server), 500);
    assert_eq!(count_in_column(&server, BlockKind::DragonEgg), 0);
    let fight = server.dormant[&end()]
        .entities
        .fight
        .as_ref()
        .expect("fight");
    assert_eq!(fight.data.gateways.len(), 18);
}
