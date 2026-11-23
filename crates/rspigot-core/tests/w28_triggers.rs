mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{Scene, calm_server, scene_on, send_input};
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_core::ports::{DatapackError, DatapackSource, PackContents, PackExtras, PackFile};
use rspigot_domain::{Identifier, Rotation};

struct Packs(Vec<PackFile>);

impl DatapackSource for Packs {
    fn read_packs(&self) -> Result<Vec<PackContents>, DatapackError> {
        Ok(vec![PackContents {
            name: "main".to_owned(),
            ..PackContents::default()
        }])
    }

    fn read_extras(&self) -> Result<Vec<PackExtras>, DatapackError> {
        Ok(vec![PackExtras {
            name: "main".to_owned(),
            advancements: self.0.clone(),
            ..PackExtras::default()
        }])
    }
}

const PLAYER: &str =
    r#""player":{"type":"minecraft:entity_properties","entity":"this","predicate":{}}"#;

fn advancement(id: &str, trigger: &str, conditions: &str) -> PackFile {
    PackFile {
        id: Identifier::parse(id).expect("id"),
        text: format!(
            r#"{{"criteria":{{"c":{{"trigger":"{trigger}","conditions":{{{conditions}}}}}}}}}"#
        ),
    }
}

fn scene() -> Scene {
    let files = vec![
        advancement(
            "t:hoard",
            "minecraft:inventory_changed",
            &format!(
                r#"{PLAYER},"items":[{{"items":"minecraft:diamond"}}],"slots":{{"occupied":{{"min":1}}}}"#
            ),
        ),
        advancement(
            "t:snack",
            "minecraft:consume_item",
            &format!(r#"{PLAYER},"item":{{"items":"minecraft:golden_apple"}}"#),
        ),
    ];
    let mut server = calm_server().with_datapacks(Arc::new(Packs(files)));
    server.start(Duration::from_secs(10));
    scene_on(server, &["Alice"], Vec::new())
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

#[test]
fn a_datapack_inventory_changed_with_extra_conditions_is_granted() {
    let mut s = scene();
    let alice = s.players[0].id;
    assert!(!s.server.advancement_is_done(alice, "t:hoard"));
    console(&mut s, "give Alice minecraft:emerald 1");
    s.tick(3);
    assert!(!s.server.advancement_is_done(alice, "t:hoard"));
    console(&mut s, "give Alice minecraft:diamond 1");
    s.tick(3);
    assert!(s.server.advancement_is_done(alice, "t:hoard"));
}

#[test]
fn a_datapack_consume_item_with_extra_conditions_is_granted() {
    let mut s = scene();
    let alice = s.players[0].clone();
    console(&mut s, "give Alice minecraft:golden_apple 1");
    s.tick(3);
    assert!(!s.server.advancement_is_done(alice.id, "t:snack"));
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, 0.0),
        },
    );
    s.tick(70);
    assert!(s.server.advancement_is_done(alice.id, "t:snack"));
}
