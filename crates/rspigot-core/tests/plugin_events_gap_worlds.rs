mod common;

use std::sync::Arc;

use common::*;
use crossbeam_channel::{Receiver, unbounded};
use rspigot_core::plugins::{
    ActionErrorKind, Answer, EventKind, EventPayload, FromPlugin, Order, ServerOrder, ToPlugin,
    WorldOrder,
};
use rspigot_core::server::Server;
use rspigot_core::worlds::{GeneratorSpec, LayerSpec, WorldKind, WorldSpec, WorldsChange};
use rspigot_domain::Identifier;
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

fn scene_with_worlds(
    handlers: Vec<rspigot_core::plugins::HandlerSpec>,
) -> (Scene, Receiver<WorldsChange>) {
    let (sink, changes) = unbounded();
    let server = configured(|_| {})
        .with_world_factory(factory)
        .with_worlds_sink(sink);
    (scene_on(server, &["Alice"], handlers), changes)
}

fn next_request() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(70_000);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn send(s: &mut Scene, order: Order) -> u64 {
    let request = next_request();
    common::order(&mut s.server, 1, FromPlugin::Action { request, order });
    request
}

fn answer_of(s: &Scene, request: u64) -> Option<Answer> {
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
}

fn delete_arena() -> Order {
    Order::InWorld {
        world: key("arena"),
        order: Box::new(Order::World(WorldOrder::Delete { files: false })),
    }
}

#[test]
fn deleting_a_world_asks_world_unload_before_it_goes() {
    let (mut s, _) = scene_with_worlds(vec![observe(1, EventKind::WorldUnload)]);
    send(
        &mut s,
        Order::Server(ServerOrder::CreateWorld(spec("arena", layers()))),
    );
    let request = send(&mut s, delete_arena());
    assert_eq!(answer_of(&s, request), Some(Answer::Done));
    let asked = settled_for(&s.plugin, EventKind::WorldUnload);
    assert_eq!(asked.len(), 1);
    let EventPayload::WorldUnload(unload) = &asked[0].payload else {
        panic!("expected a WorldUnload");
    };
    assert_eq!(unload.world, "rspigot:arena");
}

#[test]
fn a_cancelled_world_unload_keeps_the_world() {
    let (mut s, changes) = scene_with_worlds(vec![deciding(1, EventKind::WorldUnload)]);
    send(
        &mut s,
        Order::Server(ServerOrder::CreateWorld(spec("arena", layers()))),
    );
    let request = send(&mut s, delete_arena());
    assert_eq!(answer_of(&s, request), None);
    let asked = last_asked(&s.plugin, EventKind::WorldUnload);
    changes.try_iter().for_each(drop);
    judge(&mut s.server, 1, asked.id, 1, true, None);
    let Some(Answer::Failed(error)) = answer_of(&s, request) else {
        panic!("expected a refusal");
    };
    assert_eq!(error.kind, ActionErrorKind::Cancelled);
    assert!(changes.try_iter().last().is_none());
}

#[test]
fn an_allowed_world_unload_deletes_the_world() {
    let (mut s, changes) = scene_with_worlds(vec![deciding(1, EventKind::WorldUnload)]);
    send(
        &mut s,
        Order::Server(ServerOrder::CreateWorld(spec("arena", layers()))),
    );
    let request = send(&mut s, delete_arena());
    let asked = last_asked(&s.plugin, EventKind::WorldUnload);
    judge(&mut s.server, 1, asked.id, 1, false, None);
    assert_eq!(answer_of(&s, request), Some(Answer::Done));
    let last = changes.try_iter().last().expect("a change was published");
    assert!(last.worlds.is_empty());
}

#[test]
fn a_refused_delete_asks_nothing() {
    let (mut s, _) = scene_with_worlds(vec![observe(1, EventKind::WorldUnload)]);
    let request = send(&mut s, delete_arena());
    assert!(matches!(answer_of(&s, request), Some(Answer::Failed(_))));
    assert!(settled_for(&s.plugin, EventKind::WorldUnload).is_empty());
}
