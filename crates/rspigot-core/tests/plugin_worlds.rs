mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use common::*;
use crossbeam_channel::{Receiver, unbounded};
use rspigot_core::commands::Weather;
use rspigot_core::events::ClientEvent;
use rspigot_core::plugins::{
    ActionError, ActionErrorKind, Answer, FromPlugin, Order, PlayerOrder, Question, ServerOrder,
    ToPlugin, WorldOrder, WorldState,
};
use rspigot_core::server::Server;
use rspigot_core::worlds::{
    GeneratorSpec, LayerSpec, MAX_CREATED_WORLDS, WorldKind, WorldSpec, WorldsChange,
};
use rspigot_domain::{BlockPos, Identifier, Vec3};
use rspigot_worldgen::WorldGenerator;

fn factory(server: &mut Server<Link>, spec: &WorldSpec) -> Result<(), String> {
    let generator = WorldGenerator::for_world(spec)?;
    server.load_world(spec, Arc::new(generator), Arc::new(NoStore));
    Ok(())
}

fn key(name: &str) -> Identifier {
    Identifier::parse(&format!("rspigot:{name}")).expect("a valid identifier")
}

fn spec(name: &str, generator: GeneratorSpec) -> WorldSpec {
    WorldSpec {
        name: name.to_owned(),
        kind: WorldKind::Overworld,
        seed: 42,
        generator,
        structures: false,
    }
}

fn layers() -> GeneratorSpec {
    GeneratorSpec::Flat(vec![
        LayerSpec {
            block: "minecraft:bedrock".to_owned(),
            height: 1,
        },
        LayerSpec {
            block: "minecraft:stone".to_owned(),
            height: 3,
        },
    ])
}

fn scene_with_worlds() -> (Scene, Receiver<WorldsChange>) {
    let (sink, changes) = unbounded();
    let server = configured(|_| {})
        .with_world_factory(factory)
        .with_worlds_sink(sink);
    (scene_on(server, &["Alice"], Vec::new()), changes)
}

fn act(s: &mut Scene, order: Order) -> Answer {
    let request = next_request();
    common::order(&mut s.server, 1, FromPlugin::Action { request, order });
    s.plugin
        .received()
        .into_iter()
        .find_map(|message| match message {
            ToPlugin::Answer {
                request: answered,
                answer,
            } if answered == request => Some(answer),
            _ => None,
        })
        .expect("the server answers the action")
}

fn next_request() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(50_000);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn create(s: &mut Scene, spec: WorldSpec) -> Answer {
    act(s, Order::Server(ServerOrder::CreateWorld(spec)))
}

fn failure(answer: Answer) -> ActionError {
    match answer {
        Answer::Failed(error) => error,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn in_world(name: &str, order: Order) -> Order {
    Order::InWorld {
        world: key(name),
        order: Box::new(order),
    }
}

fn block_in(s: &mut Scene, name: &str, pos: BlockPos) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let answer = s.ask(Question::InWorld {
            world: key(name),
            question: Box::new(Question::Block(pos)),
        });
        match answer {
            Answer::Block(state) => return state.kind,
            Answer::NotFound if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            other => panic!("expected the block, got {other:?}"),
        }
    }
}

fn state_of(s: &mut Scene, name: &str) -> WorldState {
    match s.ask(Question::InWorld {
        world: key(name),
        question: Box::new(Question::World),
    }) {
        Answer::World(state) => state,
        other => panic!("expected a world state, got {other:?}"),
    }
}

fn travel(s: &mut Scene, world: &str) {
    let player = s.players[0].id;
    s.act(Order::Player {
        player,
        order: PlayerOrder::TeleportTo {
            world: world.to_owned(),
            position: Vec3::new(0.5, 80.0, 0.5),
            rotation: None,
        },
    });
}

#[test]
fn a_void_world_has_no_blocks() {
    let (mut s, _) = scene_with_worlds();
    assert_eq!(
        create(&mut s, spec("void", GeneratorSpec::Void)),
        Answer::WorldCreated("rspigot:void".to_owned())
    );
    for pos in [
        BlockPos::new(8, 63, 8),
        BlockPos::new(8, 0, 8),
        BlockPos::new(-9, -60, 5),
    ] {
        assert_eq!(block_in(&mut s, "void", pos), "minecraft:air", "{pos:?}");
    }
}

#[test]
fn a_flat_world_has_the_requested_layers() {
    let (mut s, _) = scene_with_worlds();
    create(&mut s, spec("flat", layers()));
    assert_eq!(
        block_in(&mut s, "flat", BlockPos::new(3, -64, 3)),
        "minecraft:bedrock"
    );
    for y in -63..=-61 {
        assert_eq!(
            block_in(&mut s, "flat", BlockPos::new(3, y, 3)),
            "minecraft:stone"
        );
    }
    assert_eq!(
        block_in(&mut s, "flat", BlockPos::new(3, -60, 3)),
        "minecraft:air"
    );
}

#[test]
fn deleting_a_world_with_a_player_is_refused() {
    let (mut s, _) = scene_with_worlds();
    create(&mut s, spec("arena", layers()));
    travel(&mut s, "rspigot:arena");
    assert_eq!(
        s.server.player_dimension(s.players[0].id),
        Some(key("arena"))
    );
    let refused = failure(act(
        &mut s,
        in_world("arena", Order::World(WorldOrder::Delete { files: false })),
    ));
    assert_eq!(refused.kind, ActionErrorKind::IncompatibleState);
    travel(&mut s, "minecraft:overworld");
    assert_eq!(
        act(
            &mut s,
            in_world("arena", Order::World(WorldOrder::Delete { files: false })),
        ),
        Answer::Done
    );
    let gone = s.ask(Question::InWorld {
        world: key("arena"),
        question: Box::new(Question::World),
    });
    assert_eq!(gone, Answer::NotFound);
}

#[test]
fn the_default_world_cannot_be_deleted() {
    let (mut s, _) = scene_with_worlds();
    let refused = failure(act(
        &mut s,
        Order::World(WorldOrder::Delete { files: true }),
    ));
    assert_eq!(refused.kind, ActionErrorKind::IncompatibleState);
}

#[test]
fn deleting_with_files_asks_the_adapter_to_erase_them() {
    let (mut s, changes) = scene_with_worlds();
    create(&mut s, spec("arena", layers()));
    assert_eq!(
        act(
            &mut s,
            in_world("arena", Order::World(WorldOrder::Delete { files: true })),
        ),
        Answer::Done
    );
    let last = changes.try_iter().last().expect("a change was published");
    assert!(last.worlds.is_empty());
    assert_eq!(last.erased, Some(key("arena")));
}

#[test]
fn a_created_world_survives_a_restart() {
    let (mut s, changes) = scene_with_worlds();
    create(&mut s, spec("arena", layers()));
    let saved = changes.try_iter().last().expect("the list was published");
    assert_eq!(saved.worlds, vec![spec("arena", layers())]);
    assert_eq!(saved.erased, None);
    drop(s);
    let mut server = configured(|_| {}).with_world_factory(factory);
    server.restore_worlds(saved.worlds);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    assert_eq!(
        block_in(&mut s, "arena", BlockPos::new(3, -64, 3)),
        "minecraft:bedrock"
    );
    assert_eq!(state_of(&mut s, "arena").seed, 42);
}

#[test]
fn names_must_be_valid_and_unique() {
    let (mut s, _) = scene_with_worlds();
    for bad in ["", "Arena", "a b", "../x", "x:y"] {
        let refused = failure(create(&mut s, spec(bad, GeneratorSpec::Void)));
        assert_eq!(refused.kind, ActionErrorKind::InvalidArgument, "{bad:?}");
    }
    create(&mut s, spec("arena", GeneratorSpec::Void));
    let refused = failure(create(&mut s, spec("arena", GeneratorSpec::Void)));
    assert_eq!(refused.kind, ActionErrorKind::InvalidArgument);
}

#[test]
fn the_number_of_created_worlds_is_limited() {
    let (mut s, _) = scene_with_worlds();
    for index in 0..MAX_CREATED_WORLDS {
        let name = format!("w{index}");
        assert!(matches!(
            create(&mut s, spec(&name, GeneratorSpec::Void)),
            Answer::WorldCreated(_)
        ));
    }
    let refused = failure(create(&mut s, spec("extra", GeneratorSpec::Void)));
    assert_eq!(refused.kind, ActionErrorKind::LimitReached);
}

#[test]
fn orders_and_reads_target_the_world_they_name() {
    let (mut s, _) = scene_with_worlds();
    create(&mut s, spec("arena", GeneratorSpec::Void));
    s.act(in_world(
        "arena",
        Order::World(WorldOrder::SetWeather(Weather::Rain)),
    ));
    assert!(state_of(&mut s, "arena").raining);
    let Answer::World(default) = s.ask(Question::World) else {
        panic!("expected the default world");
    };
    assert!(!default.raining);
    assert_eq!(state_of(&mut s, "arena").name, "arena");
}

#[test]
fn a_player_travels_between_worlds_and_back() {
    let (mut s, _) = scene_with_worlds();
    create(&mut s, spec("arena", layers()));
    let alice = s.players[0].clone();
    travel(&mut s, "rspigot:arena");
    assert_eq!(s.server.player_dimension(alice.id), Some(key("arena")));
    let respawns = |events: &[ClientEvent]| {
        events
            .iter()
            .filter(|event| matches!(event, ClientEvent::Respawn { .. }))
            .count()
    };
    assert_eq!(respawns(&alice.link.take()), 1);
    travel(&mut s, "minecraft:overworld");
    assert_eq!(
        s.server.player_dimension(alice.id),
        Some(Identifier::minecraft("overworld"))
    );
    assert_eq!(respawns(&alice.link.take()), 1);
}
