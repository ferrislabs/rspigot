mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{Scene, calm_server, scene_on};
use rspigot_core::plugins::{Answer, Question};
use rspigot_core::ports::{
    DatapackError, DatapackSource, PackContents, PackExtras, PackFile, PackMeta,
};
use rspigot_domain::Identifier;

struct Packs(Mutex<(Vec<PackContents>, Vec<PackExtras>)>);

impl DatapackSource for Packs {
    fn read_packs(&self) -> Result<Vec<PackContents>, DatapackError> {
        Ok(self.0.lock().expect("packs").0.clone())
    }

    fn read_extras(&self) -> Result<Vec<PackExtras>, DatapackError> {
        Ok(self.0.lock().expect("packs").1.clone())
    }
}

fn file(id: &str, text: &str) -> PackFile {
    PackFile {
        id: Identifier::parse(id).expect("id"),
        text: text.to_owned(),
    }
}

const GEMS: &str =
    r#"{"pools":[{"rolls":1,"entries":[{"type":"minecraft:item","name":"minecraft:diamond"}]}]}"#;
const REWARDING: &str = r#"{"criteria":{"c":{"trigger":"minecraft:impossible"}},"rewards":{"loot":["t:gems"],"function":"t:hello"}}"#;

fn scene() -> Scene {
    let pack = PackContents {
        name: "main".to_owned(),
        meta: PackMeta::default(),
        functions: vec![file("t:hello", "give Alice minecraft:apple 1")],
        loot_tables: vec![file("t:gems", GEMS)],
        ..PackContents::default()
    };
    let extras = PackExtras {
        name: "main".to_owned(),
        advancements: vec![file("t:reward", REWARDING)],
        ..PackExtras::default()
    };
    let packs = Arc::new(Packs(Mutex::new((vec![pack], vec![extras]))));
    let mut server = calm_server().with_datapacks(packs);
    server.start(Duration::from_secs(10));
    scene_on(server, &["Alice"], Vec::new())
}

fn held(s: &mut Scene) -> Vec<String> {
    let alice = s.players[0].id;
    let Answer::Inventory(state) = s.ask(Question::Inventory(alice)) else {
        panic!("an inventory");
    };
    state
        .slots
        .into_iter()
        .flatten()
        .map(|spec| spec.key)
        .collect()
}

#[test]
fn completing_an_advancement_gives_its_loot_and_runs_its_function() {
    let mut s = scene();
    let alice = s.players[0].id;
    assert!(s.server.award_advancement(alice, "t:reward"));
    s.tick(2);
    let stacks = held(&mut s);
    assert!(
        stacks.contains(&"minecraft:diamond".to_owned()),
        "{stacks:?}"
    );
    assert!(stacks.contains(&"minecraft:apple".to_owned()), "{stacks:?}");
}
