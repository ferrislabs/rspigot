mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::*;
use rspigot_core::commands::{CommandOutcome, CommandSender, execute};
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{Hand, PlayerInput};
use rspigot_core::ports::{PlayerStore, StoreError};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, Rotation};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::{Compound, Tag};

#[derive(Default)]
struct Saves(Mutex<HashMap<PlayerId, Compound>>);

impl Saves {
    fn player(&self, id: PlayerId) -> Option<Compound> {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(found) = self.0.lock().unwrap().get(&id).cloned() {
                return Some(found);
            }
            if std::time::Instant::now() > deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn put_player(&self, id: PlayerId, data: Compound) {
        self.0.lock().unwrap().insert(id, data);
    }
}

impl PlayerStore for Saves {
    fn load_player(&self, id: PlayerId) -> Result<Option<Compound>, StoreError> {
        Ok(self.0.lock().unwrap().get(&id).cloned())
    }

    fn save_player(&self, id: PlayerId, data: &Compound) -> Result<(), StoreError> {
        self.0.lock().unwrap().insert(id, data.clone());
        Ok(())
    }
}

fn console(s: &mut Scene, line: &str) -> CommandOutcome {
    let outcome = execute(&mut s.server, CommandSender::Console, line);
    s.server.deliver();
    outcome
}

fn throw(s: &mut Scene) {
    console(s, "gamemode survival Alice");
    console(s, "give Alice ender_pearl 5");
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, -80.0),
        },
    );
    s.tick(1);
}

fn scene_with(saves: &Arc<Saves>) -> Scene {
    let server = server(PluginSettings::default()).with_player_store(Arc::clone(saves));
    scene_on(server, &["Alice"], Vec::new())
}

fn move_pearls(data: &mut Compound, to: [f64; 3]) {
    let Some(Tag::List(pearls)) = data.get("ender_pearls").cloned() else {
        panic!("no pearls saved");
    };
    let moved: Vec<Tag> = pearls
        .into_iter()
        .map(|pearl| {
            let Tag::Compound(mut pearl) = pearl else {
                panic!("a compound");
            };
            pearl.insert(
                "Pos",
                Tag::List(to.iter().map(|v| Tag::Double(*v)).collect()),
            );
            pearl.insert("Motion", Tag::List(vec![Tag::Double(0.0); 3]));
            Tag::Compound(pearl)
        })
        .collect();
    data.insert("ender_pearls", Tag::List(moved));
}

#[test]
fn a_flying_pearl_is_saved_with_its_owner() {
    let saves = Arc::new(Saves::default());
    let mut s = scene_with(&saves);
    throw(&mut s);
    assert_eq!(s.server.entities_of(Kind::EnderPearl).len(), 1);
    assert_eq!(console(&mut s, "save-all"), CommandOutcome::Success);
    let alice = PlayerId::offline("Alice");
    let data = saves.player(alice).expect("saved");
    let Some(Tag::List(pearls)) = data.get("ender_pearls") else {
        panic!("no ender_pearls list");
    };
    assert_eq!(pearls.len(), 1);
    let Tag::Compound(pearl) = &pearls[0] else {
        panic!("a compound");
    };
    assert_eq!(pearl.get_str("id"), Some("minecraft:ender_pearl"));
    assert_eq!(
        pearl.get_str("ender_pearl_dimension"),
        Some("minecraft:overworld")
    );
}

#[test]
fn a_saved_pearl_returns_and_loads_its_chunk() {
    let saves = Arc::new(Saves::default());
    let mut first = scene_with(&saves);
    throw(&mut first);
    console(&mut first, "save-all");
    let alice = PlayerId::offline("Alice");
    let mut data = saves.player(alice).expect("saved");
    move_pearls(&mut data, [400.5, 90.0, 400.5]);
    saves.put_player(alice, data);
    let mut second = scene_with(&saves);
    second.server.wait_for_spawn(Duration::from_secs(10));
    second.tick(2);
    second.server.wait_for_spawn(Duration::from_secs(10));
    second.tick(2);
    let pearls = second.server.entities_of(Kind::EnderPearl);
    assert_eq!(pearls.len(), 1, "the pearl came back");
    assert!((pearls[0].position.x - 400.5).abs() < 2.0);
    assert!(
        second
            .server
            .block_state_at(BlockPos::new(400, 64, 400))
            .is_some(),
        "its chunk was loaded"
    );
}
