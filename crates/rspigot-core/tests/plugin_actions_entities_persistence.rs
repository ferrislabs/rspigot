mod common;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_anvil::{AnvilStore, DimensionLayout};
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::Inbound;
use rspigot_core::plugins::{
    Answer, BlockOrder, EntityOrder, ObjectAnswer, ObjectOrder, ObjectQuestion, Order, PdcChange,
    PdcHolder, PdcValue, Question, WorldOrder,
};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::{BlockPos, EntityId, Vec3};

struct Root(PathBuf);

impl Root {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("rspigot-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a temporary world folder");
        Self(path)
    }
}

impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scene_on_disk(root: &Root) -> Scene {
    let store = AnvilStore::open(&root.0, DimensionLayout::Spigot).expect("an anvil store");
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(store),
        Arc::new(Floor),
        StatusBoard::new(20),
    );
    server.wait_for_spawn(Duration::from_secs(10));
    let mut s = scene_on(server, &["Alice"], Vec::new());
    s.tick(60);
    s
}

fn pdc(s: &mut Scene, holder: PdcHolder) -> Vec<(String, PdcValue)> {
    match s.ask(Question::Object(ObjectQuestion::Pdc(holder))) {
        Answer::Object(ObjectAnswer::Pdc(entries)) => entries.into_iter().collect(),
        other => panic!("expected a container, got {other:?}"),
    }
}

fn set(key: &str, value: PdcValue) -> PdcChange {
    PdcChange::Set {
        key: key.to_owned(),
        value,
    }
}

fn nested() -> PdcValue {
    PdcValue::Container(
        [
            ("n".to_owned(), PdcValue::Long(9)),
            (
                "l".to_owned(),
                PdcValue::List(vec![PdcValue::Int(1), PdcValue::Int(2)]),
            ),
        ]
        .into(),
    )
}

fn spawn_cow(s: &mut Scene) -> EntityId {
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: "minecraft:cow".into(),
        position: Vec3::new(3.5, 64.0, 3.5),
    }) else {
        panic!("expected a cow");
    };
    view.id
}

fn the_cow(s: &mut Scene) -> EntityId {
    match s.ask(Question::Entities {
        near: Some((Vec3::new(3.5, 64.0, 3.5), 4.0)),
        kind: Some("minecraft:cow".to_owned()),
    }) {
        Answer::Entities(found) => found.first().expect("the cow was saved").id,
        other => panic!("expected the entities, got {other:?}"),
    }
}

#[test]
fn block_and_entity_values_survive_a_real_save_and_reload() {
    let root = Root::new("pdc-reload");
    let pos = BlockPos::new(2, 63, 2);
    {
        let mut s = scene_on_disk(&root);
        let cow = spawn_cow(&mut s);
        s.tick(2);
        s.act(Order::Block {
            pos,
            order: BlockOrder::Pdc(set("plugin:owner", PdcValue::Text("alice".to_owned()))),
        });
        s.act(Order::Block {
            pos,
            order: BlockOrder::Pdc(set("plugin:nested", nested())),
        });
        s.act(Order::Entity {
            entity: cow,
            order: EntityOrder::Object(ObjectOrder::Pdc(set("plugin:level", PdcValue::Int(7)))),
        });
        s.act(Order::Entity {
            entity: cow,
            order: EntityOrder::Object(ObjectOrder::Pdc(set(
                "plugin:big",
                PdcValue::Long(1 << 40),
            ))),
        });
        assert!(s.warnings().is_empty());
        s.act(Order::World(WorldOrder::Save));
        s.server
            .handle(Inbound::Console("save-all flush".to_owned()));
        s.server.deliver();
    }

    let mut s = scene_on_disk(&root);
    let block = pdc(&mut s, PdcHolder::Block(pos));
    assert_eq!(
        block,
        vec![
            ("plugin:nested".to_owned(), nested()),
            (
                "plugin:owner".to_owned(),
                PdcValue::Text("alice".to_owned())
            ),
        ]
    );
    let cow = the_cow(&mut s);
    let held = pdc(&mut s, PdcHolder::Entity(cow));
    assert_eq!(
        held,
        vec![
            ("plugin:big".to_owned(), PdcValue::Long(1 << 40)),
            ("plugin:level".to_owned(), PdcValue::Int(7)),
        ]
    );
}
