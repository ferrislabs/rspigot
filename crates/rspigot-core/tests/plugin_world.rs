mod common;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::admin::ListEntry;
use rspigot_core::commands::Weather;
use rspigot_core::events::{ClientEvent, GameEvent, SoundSource};
use rspigot_core::plugins::{
    Answer, BlockOrder, BlockState, GameRule, Order, ParticleDataSpec, ParticleSpec, Question,
    ServerOrder, ServerState, SoundSpec, WorldOrder, WorldState,
};
use rspigot_domain::particle::{Particle, ParticleData};
use rspigot_domain::player::GameProfile;
use rspigot_domain::text::Component;
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, Vec3};
use rspigot_items::ItemKind;

fn block(pos: BlockPos, order: BlockOrder) -> Order {
    Order::Block { pos, order }
}

fn set(key: &str) -> BlockOrder {
    BlockOrder::Set(key.into())
}

fn block_state(s: &mut Scene, pos: BlockPos) -> BlockState {
    let Answer::Block(state) = s.ask(Question::Block(pos)) else {
        panic!("expected the block at {pos:?}");
    };
    state
}

fn names(s: &mut Scene, question: Question) -> Vec<ListEntry> {
    let Answer::Names(names) = s.ask(question) else {
        panic!("expected a list of names");
    };
    names
}

fn server_state(s: &mut Scene) -> ServerState {
    let Answer::Server(state) = s.ask(Question::Server) else {
        panic!("expected the state of the server");
    };
    state
}

fn world_state(s: &mut Scene) -> WorldState {
    let Answer::World(state) = s.ask(Question::World) else {
        panic!("expected the state of the world");
    };
    state
}

fn on_the_ground(s: &Scene, item: ItemKind) -> Vec<u8> {
    s.server
        .ground_items()
        .into_iter()
        .filter(|(_, stack)| stack.item == item.id())
        .map(|(_, stack)| stack.count.get())
        .collect()
}

fn sound(key: &str) -> SoundSpec {
    SoundSpec {
        key: key.into(),
        volume: 1.0,
        pitch: 1.0,
    }
}

fn particle(key: &str, position: Vec3) -> ParticleSpec {
    ParticleSpec {
        key: key.into(),
        data: ParticleDataSpec::None,
        position,
        count: 4,
        spread: Vec3::new(0.25, 0.25, 0.25),
        speed: 0.1,
    }
}

fn sounds(player: &Player) -> Vec<(&'static str, SoundSource, Vec3, f32, f32)> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Sound {
                sound,
                source,
                position,
                volume,
                pitch,
                ..
            } => Some((sound, source, position, volume, pitch)),
            _ => None,
        })
        .collect()
}

fn particles(player: &Player) -> Vec<Particle> {
    player
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Particle(particle) => Some(particle),
            _ => None,
        })
        .collect()
}

#[test]
fn broadcast_dispatch_and_shutdown() {
    let mut s = scene(&["Alice", "Bob"]);
    let players = s.players.clone();
    assert_eq!(
        ServerOrder::Broadcast(String::new()).name(),
        "server.broadcast"
    );
    s.act(Order::Server(ServerOrder::Broadcast("§eHi".into())));
    for player in &players {
        let chats: Vec<ClientEvent> = player
            .link
            .take()
            .into_iter()
            .filter(|event| matches!(event, ClientEvent::SystemChat { .. }))
            .collect();
        assert_eq!(
            chats,
            [ClientEvent::system(Component::from_section_codes("§eHi"))]
        );
    }
    s.act(Order::Server(ServerOrder::DispatchCommand(
        "time set 6000".into(),
    )));
    for player in &players {
        assert!(
            player
                .link
                .take()
                .iter()
                .any(|event| matches!(event, ClientEvent::Time { day_time: 6000, .. }))
        );
    }
    s.act(Order::Server(ServerOrder::Save));
    assert!(s.server.is_running());
    s.act(Order::Server(ServerOrder::Shutdown));
    assert!(!s.server.is_running());
    assert!(s.warnings().is_empty());
}

#[test]
fn whitelist_and_operators_are_listed() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    s.act(Order::Server(ServerOrder::WhitelistAdd("Carol".into())));
    s.act(Order::Server(ServerOrder::OperatorAdd("Alice".into())));
    assert_eq!(
        names(&mut s, Question::Whitelist),
        [ListEntry {
            name: "Carol".into(),
            uuid: None,
            level: None,
            bypasses_player_limit: false,
        }]
    );
    let profile = GameProfile {
        id: alice.id,
        name: "Alice".into(),
        properties: Vec::new(),
    };
    assert_eq!(
        names(&mut s, Question::Operators),
        [ListEntry::of(&profile).operator(4)]
    );
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::OperatorLevel { level: 4, .. }))
    );

    assert!(!server_state(&mut s).whitelist_enabled);
    s.act(Order::Server(ServerOrder::SetWhitelistEnabled(true)));
    assert!(server_state(&mut s).whitelist_enabled);

    s.act(Order::Server(ServerOrder::WhitelistRemove("carol".into())));
    assert_eq!(names(&mut s, Question::Whitelist), []);
    s.act(Order::Server(ServerOrder::OperatorRemove("Alice".into())));
    assert_eq!(names(&mut s, Question::Operators), []);
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::OperatorLevel { level: 0, .. }))
    );
    assert!(s.warnings().is_empty());
}

#[test]
fn blank_names_are_refused_with_one_warning() {
    let mut s = scene(&["Alice", "Bob"]);
    s.act(Order::Server(ServerOrder::WhitelistAdd("Carol".into())));
    s.act(Order::Server(ServerOrder::OperatorAdd("Alice".into())));
    let whitelist = names(&mut s, Question::Whitelist);
    let operators = names(&mut s, Question::Operators);
    let bad_orders = [
        ServerOrder::WhitelistAdd(String::new()),
        ServerOrder::WhitelistRemove(" ".into()),
        ServerOrder::OperatorAdd("  ".into()),
        ServerOrder::OperatorRemove("\t".into()),
    ];
    for bad in bad_orders {
        let name = bad.name();
        let before = s.warnings().len();
        s.act(Order::Server(bad));
        let warnings = s.warnings();
        assert_eq!(warnings.len(), before + 1, "{name}");
        assert_eq!(
            warnings.last().map(String::as_str),
            Some(format!("{name} was not applied: name cannot be empty").as_str())
        );
    }
    assert_eq!(names(&mut s, Question::Whitelist), whitelist);
    assert_eq!(names(&mut s, Question::Operators), operators);
}

#[test]
fn server_state_and_named_players() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    let state = server_state(&mut s);
    assert_eq!(state.max_players, 20);
    assert_eq!(state.online, 2);
    assert_eq!(state.tps, [20.0; 3]);
    assert!(!state.whitelist_enabled);

    let Answer::Players(everyone) = s.ask(Question::Players) else {
        panic!("expected the online players");
    };
    let snapshot = everyone
        .into_iter()
        .find(|player| player.id == alice.id)
        .expect("alice is online");
    assert_eq!(
        s.ask(Question::PlayerNamed("alice".into())),
        Answer::Players(vec![snapshot])
    );
    assert_eq!(
        s.ask(Question::PlayerNamed("nobody".into())),
        Answer::Players(Vec::new())
    );
}

#[test]
fn world_orders_reach_every_client_and_read_back() {
    let mut s = scene(&["Alice", "Bob"]);
    let players = s.players.clone();
    let spawn = BlockPos::new(5, 64, 5);
    assert_eq!(WorldOrder::SetTime(0).name(), "world.set_time");

    s.act(Order::World(WorldOrder::SetTime(6000)));
    for player in &players {
        assert!(player.link.take().iter().any(|event| matches!(
            event,
            ClientEvent::Time {
                day_time: 6000,
                advance_time: true,
                ..
            }
        )));
    }
    s.act(Order::World(WorldOrder::SetWeather(Weather::Thunder)));
    for player in &players {
        let events = player.link.take();
        assert!(events.contains(&ClientEvent::GameEvent(GameEvent::StartRaining)));
        assert!(events.contains(&ClientEvent::GameEvent(GameEvent::ThunderLevelChange(1.0))));
    }
    s.act(Order::World(WorldOrder::SetDifficulty(Difficulty::Hard)));
    for player in &players {
        assert!(player.link.take().contains(&ClientEvent::Difficulty {
            difficulty: Difficulty::Hard,
            locked: false
        }));
    }
    s.act(Order::World(WorldOrder::SetSpawn(spawn)));
    for player in &players {
        assert!(
            player.link.take().iter().any(
                |event| matches!(event, ClientEvent::SpawnPosition { pos, .. } if *pos == spawn)
            )
        );
    }
    s.act(Order::World(WorldOrder::SetGameRule(
        GameRule::ImmediateRespawn,
        true,
    )));
    for player in &players {
        assert!(
            player
                .link
                .take()
                .contains(&ClientEvent::GameEvent(GameEvent::ImmediateRespawn(true)))
        );
    }
    s.act(Order::World(WorldOrder::SetGameRule(
        GameRule::DaylightCycle,
        false,
    )));
    for player in &players {
        assert!(player.link.take().iter().any(|event| matches!(
            event,
            ClientEvent::Time {
                day_time: 6000,
                advance_time: false,
                ..
            }
        )));
    }
    s.act(Order::World(WorldOrder::SetPvp(false)));
    s.act(Order::World(WorldOrder::Save));

    let state = world_state(&mut s);
    assert_eq!(state.name, "world");
    assert_eq!(state.uuid, None);
    assert_eq!(state.seed, 0);
    assert_eq!(state.day_time, 6000);
    assert!(state.raining);
    assert!(state.thundering);
    assert_eq!(state.difficulty, Difficulty::Hard);
    assert_eq!(state.spawn, spawn);
    assert!(!state.pvp);
    assert_eq!(state.rules.len(), 11);
    for rule in [
        (GameRule::DaylightCycle, false),
        (GameRule::ImmediateRespawn, true),
        (GameRule::KeepInventory, false),
    ] {
        assert!(state.rules.contains(&rule), "{rule:?}");
    }
    assert!(s.warnings().is_empty());
}

#[test]
fn world_sound_and_particles_reach_players_nearby() {
    let mut s = scene(&["Alice", "Bob"]);
    let players = s.players.clone();
    let at = s.server.player_location(players[0].id).unwrap();
    let far = Vec3::new(1000.0, 64.0, 1000.0);

    s.act(Order::World(WorldOrder::PlaySound {
        sound: sound("minecraft:entity.cow.hurt"),
        position: at,
    }));
    for player in &players {
        assert_eq!(
            sounds(player),
            [(
                "minecraft:entity.cow.hurt",
                SoundSource::Master,
                at,
                1.0,
                1.0
            )]
        );
    }
    s.act(Order::World(WorldOrder::PlaySound {
        sound: sound("minecraft:entity.cow.hurt"),
        position: far,
    }));
    for player in &players {
        assert!(sounds(player).is_empty());
    }

    s.act(Order::World(WorldOrder::SpawnParticle(particle(
        "minecraft:happy_villager",
        at,
    ))));
    for player in &players {
        assert_eq!(
            particles(player),
            [Particle {
                kind: "minecraft:happy_villager",
                data: ParticleData::None,
                position: at,
                count: 4,
                spread: Vec3::new(0.25, 0.25, 0.25),
                speed: 0.1,
            }]
        );
    }
    s.act(Order::World(WorldOrder::SpawnParticle(particle(
        "minecraft:happy_villager",
        far,
    ))));
    for player in &players {
        assert!(particles(player).is_empty());
    }
    assert!(s.warnings().is_empty());
}

#[test]
fn a_block_is_set_read_back_and_broken() {
    let mut s = scene(&["Alice", "Bob"]);
    let players = s.players.clone();
    let pos = BlockPos::new(2, 64, 2);
    let stone = BlockKind::from_name("minecraft:stone")
        .unwrap()
        .default_state();
    assert_eq!(set("minecraft:stone").name(), "block.set");
    stream_chunk_of(&mut s, pos);

    s.act(block(pos, set("minecraft:stone")));
    for player in &players {
        assert!(
            player
                .link
                .take()
                .contains(&ClientEvent::BlockUpdate { pos, state: stone })
        );
    }
    assert_eq!(
        block_state(&mut s, pos),
        BlockState {
            kind: "minecraft:stone".into(),
            properties: Vec::new()
        }
    );

    s.act(block(
        pos,
        BlockOrder::SetWith {
            kind: "minecraft:oak_stairs".into(),
            properties: vec![("facing".into(), "east".into())],
        },
    ));
    let stairs = block_state(&mut s, pos);
    assert_eq!(stairs.kind, "minecraft:oak_stairs");
    assert!(
        stairs
            .properties
            .contains(&("facing".into(), "east".into()))
    );

    s.act(block(pos, set("minecraft:stone")));
    s.act(block(pos, BlockOrder::BreakNaturally));
    assert_eq!(block_state(&mut s, pos).kind, "minecraft:air");
    assert_eq!(s.server.ground_items().len(), 1);
    assert_eq!(on_the_ground(&s, ItemKind::Cobblestone), [1]);

    s.act(block(pos, set("minecraft:dirt")));
    s.act(block(pos, BlockOrder::BreakNaturally));
    assert_eq!(block_state(&mut s, pos).kind, "minecraft:air");
    assert_eq!(s.server.ground_items().len(), 2);
    assert_eq!(on_the_ground(&s, ItemKind::Dirt), [1]);
    assert!(s.warnings().is_empty());
}

#[test]
fn without_block_drops_a_broken_block_drops_nothing() {
    let mut s = scene(&["Alice", "Bob"]);
    let pos = BlockPos::new(2, 64, 2);
    s.act(Order::World(WorldOrder::SetGameRule(
        GameRule::BlockDrops,
        false,
    )));
    s.act(block(pos, set("minecraft:stone")));
    s.act(block(pos, BlockOrder::BreakNaturally));
    assert_eq!(block_state(&mut s, pos).kind, "minecraft:air");
    assert!(s.server.ground_items().is_empty());
    assert!(s.warnings().is_empty());
}

#[test]
fn set_time_moves_forward_to_that_time_of_day() {
    let mut s = scene(&["Alice", "Bob"]);
    let players = s.players.clone();
    let time_set = |s: &mut Scene, line: &str| {
        s.act(Order::Server(ServerOrder::DispatchCommand(line.into())));
    };
    time_set(&mut s, "time set 30000");
    assert_eq!(world_state(&mut s).day_time, 30000);

    s.act(Order::World(WorldOrder::SetTime(1000)));
    assert_eq!(world_state(&mut s).day_time, 49000);
    for player in &players {
        assert!(player.link.take().iter().any(|event| matches!(
            event,
            ClientEvent::Time {
                day_time: 49000,
                ..
            }
        )));
    }

    time_set(&mut s, "time set 30000");
    s.act(Order::World(WorldOrder::SetTime(-1000)));
    assert_eq!(world_state(&mut s).day_time, 47000);
    s.act(Order::World(WorldOrder::SetTime(47000)));
    assert_eq!(world_state(&mut s).day_time, 47000);
    assert!(s.warnings().is_empty());
}

#[test]
fn a_block_in_an_unloaded_chunk_is_not_found() {
    let mut s = scene(&["Alice", "Bob"]);
    let far = BlockPos::new(10_000, 64, 10_000);
    assert_eq!(s.ask(Question::Block(far)), Answer::NotFound);
    s.act(block(far, set("minecraft:stone")));
    assert_eq!(
        s.warnings(),
        ["block.set was not applied: the chunk is not loaded"]
    );
    assert_eq!(s.ask(Question::Block(far)), Answer::NotFound);
    assert_eq!(
        s.ask(Question::Block(BlockPos::new(2, 400, 2))),
        Answer::NotFound
    );
}

#[test]
fn explode_breaks_blocks_only_when_asked() {
    let mut s = scene(&["Alice", "Bob"]);
    let players = s.players.clone();
    let pos = BlockPos::new(3, 64, 3);
    let center = Vec3::new(3.5, 64.5, 3.5);
    let explode = |breaks_blocks| {
        Order::World(WorldOrder::Explode {
            position: center,
            power: 2.0,
            breaks_blocks,
        })
    };

    s.act(block(pos, set("minecraft:stone")));
    s.act(explode(false));
    assert_eq!(block_state(&mut s, pos).kind, "minecraft:stone");
    for player in &players {
        assert!(
            player
                .link
                .take()
                .iter()
                .any(|event| matches!(event, ClientEvent::Explode { .. }))
        );
    }

    s.act(explode(true));
    assert_eq!(block_state(&mut s, pos).kind, "minecraft:air");

    s.act(Order::World(WorldOrder::SetGameRule(
        GameRule::MobGriefing,
        false,
    )));
    s.act(block(pos, set("minecraft:stone")));
    s.act(explode(true));
    assert_eq!(block_state(&mut s, pos).kind, "minecraft:air");
    assert!(s.warnings().is_empty());
}

#[test]
fn invalid_world_orders_warn() {
    let mut s = scene(&["Alice", "Bob"]);
    let players = s.players.clone();
    let pos = BlockPos::new(2, 64, 2);
    let center = Vec3::new(2.5, 64.5, 2.5);
    let stairs = |name: &str, value: &str| BlockOrder::SetWith {
        kind: "minecraft:oak_stairs".into(),
        properties: vec![(name.into(), value.into())],
    };
    let explode = |position, power| {
        Order::World(WorldOrder::Explode {
            position,
            power,
            breaks_blocks: true,
        })
    };
    let bad_orders = [
        Order::World(WorldOrder::PlaySound {
            sound: sound("minecraft:nope"),
            position: center,
        }),
        Order::World(WorldOrder::SpawnParticle(particle(
            "minecraft:nope",
            center,
        ))),
        block(pos, set("minecraft:nope")),
        block(BlockPos::new(2, 400, 2), set("minecraft:stone")),
        block(pos, stairs("facing", "sideways")),
        block(pos, stairs("color", "red")),
        explode(center, -1.0),
        explode(center, f32::NAN),
        explode(Vec3::new(f64::NAN, 64.5, 2.5), 2.0),
    ];
    for order in bad_orders {
        let name = order.name();
        let before = s.warnings().len();
        s.act(order);
        let warnings = s.warnings();
        assert_eq!(warnings.len(), before + 1, "{name}");
        let last = warnings.last().unwrap();
        assert!(
            last.starts_with(&format!("{name} was not applied: ")),
            "{last}"
        );
    }
    assert_eq!(block_state(&mut s, pos).kind, "minecraft:air");
    for player in &players {
        assert!(!player.link.take().iter().any(|event| matches!(
            event,
            ClientEvent::Sound { .. }
                | ClientEvent::Particle(_)
                | ClientEvent::BlockUpdate { .. }
                | ClientEvent::Explode { .. }
        )));
    }
}
