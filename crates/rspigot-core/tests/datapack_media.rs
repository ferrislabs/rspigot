mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use std::sync::{Arc, Mutex};

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::ClientEvent;
use rspigot_core::ports::{
    DatapackError, DatapackSource, PackContents, PackExtras, PackFile, PackRegistryFile,
};
use rspigot_domain::entity::hanging;
use rspigot_domain::{BlockPos, Identifier};
use screen::use_block;
use seeded::{Seed, world};

const JUKEBOX: BlockPos = BlockPos::new(3, 64, 3);

#[derive(Default)]
struct Packs(Mutex<Vec<PackExtras>>);

impl DatapackSource for Packs {
    fn read_packs(&self) -> Result<Vec<PackContents>, DatapackError> {
        Ok(vec![PackContents {
            name: "main".to_owned(),
            ..PackContents::default()
        }])
    }

    fn read_extras(&self) -> Result<Vec<PackExtras>, DatapackError> {
        Ok(self.0.lock().expect("packs").clone())
    }
}

fn packs(files: &[(&str, &str, &str)]) -> Arc<Packs> {
    Arc::new(Packs(Mutex::new(vec![PackExtras {
        name: "main".to_owned(),
        registries: files
            .iter()
            .map(|(registry, id, text)| PackRegistryFile {
                registry: (*registry).to_owned(),
                file: PackFile {
                    id: Identifier::parse(id).expect("id"),
                    text: (*text).to_owned(),
                },
            })
            .collect(),
        ..PackExtras::default()
    }])))
}

fn with_packs(s: Scene, source: Arc<Packs>) -> Scene {
    Scene {
        server: s.server.with_datapacks(source),
        plugin: s.plugin,
        players: s.players,
    }
}

fn events(alice: &Player) -> Vec<(i32, i32)> {
    alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::LevelEvent { event, data, .. } => Some((event, data)),
            _ => None,
        })
        .collect()
}

fn jukebox_scene(source: Arc<Packs>, disc: &str) -> (Scene, Player) {
    let s = world(vec![Seed::block(JUKEBOX, BlockKind::Jukebox)], Vec::new());
    let mut s = with_packs(s, source);
    let alice = s.players[0].clone();
    let hand = if disc.contains('[') {
        format!("item replace entity Alice weapon.mainhand with {disc}")
    } else {
        format!("give Alice {disc}")
    };
    for line in ["gamemode survival Alice".to_owned(), hand] {
        s.server
            .handle(rspigot_core::events::Inbound::Console(line));
        s.server.deliver();
    }
    alice.link.take();
    (s, alice)
}

#[test]
fn a_datapack_song_overrides_the_length_of_a_built_in_disc() {
    let source = packs(&[(
        "jukebox_song",
        "minecraft:11",
        r#"{"length_in_seconds":2.0,"comparator_output":3}"#,
    )]);
    let (mut s, alice) = jukebox_scene(source, "minecraft:music_disc_11");
    use_block(&mut s, &alice, JUKEBOX);
    s.tick(30);
    assert!(!events(&alice).contains(&(1011, 0)));
    s.tick(60);
    assert!(events(&alice).contains(&(1011, 0)));
}

#[test]
fn a_datapack_song_played_from_a_playable_disc_uses_its_new_registry_id() {
    let source = packs(&[(
        "jukebox_song",
        "t:tune",
        r#"{"length_in_seconds":2.0,"comparator_output":3}"#,
    )]);
    let (mut s, alice) = jukebox_scene(
        source,
        "minecraft:music_disc_13[minecraft:jukebox_playable=\"t:tune\"]",
    );
    use_block(&mut s, &alice, JUKEBOX);
    s.tick(1);
    let base = rspigot_data::registries::synced_entries("minecraft:jukebox_song")
        .unwrap()
        .len();
    let seen = events(&alice);
    assert!(
        seen.contains(&(1010, i32::try_from(base).unwrap())),
        "{seen:?}"
    );
}

#[test]
fn datapack_paintings_resize_and_extend_the_variant_table() {
    let source = packs(&[
        ("painting_variant", "t:wide", r#"{"width":4,"height":2}"#),
        (
            "painting_variant",
            "minecraft:alban",
            r#"{"width":2,"height":3}"#,
        ),
        (
            "tags/painting_variant",
            "minecraft:placeable",
            r#"{"values":["t:wide"]}"#,
        ),
    ]);
    let _s = with_packs(world(Vec::new(), Vec::new()), source);
    let alban = hanging::variant_by_name("minecraft:alban").unwrap();
    assert_eq!(hanging::variant_size(alban), (2, 3));
    let wide = hanging::variant_by_name("t:wide").unwrap();
    assert_eq!(hanging::variant_size(wide), (4, 2));
    assert_eq!(hanging::variant_name(wide), Some("t:wide"));
    assert!(hanging::placeable_variants().contains(&wide));
}
