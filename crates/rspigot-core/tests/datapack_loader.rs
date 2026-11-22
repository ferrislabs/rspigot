mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::*;
use rspigot_anvil::DirectoryDatapacks;
use rspigot_blocks::BlockKind;
use rspigot_core::events::{ClientEvent, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, Question};
use rspigot_domain::entity::hanging;
use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Tag;
use seeded::{Seed, world, world_spawning};

const META: &str = r#"{"pack":{"description":"t","pack_format":94}}"#;
const SPAWNER: BlockPos = BlockPos::new(4, 64, 4);
const JUKEBOX: BlockPos = BlockPos::new(3, 64, 3);
const GEM: &str = r#"{"wants":{"id":"minecraft:emerald","count":3},"gives":{"id":"minecraft:diamond","count":2},"max_uses":7}"#;
const HUSKS: &str = r#"{"total_mobs":6.0,"simultaneous_mobs":2.0,"total_mobs_added_per_player":2.0,"simultaneous_mobs_added_per_player":1.0,"ticks_between_spawn":10,"spawn_potentials":[{"data":{"entity":{"id":"minecraft:villager"}},"weight":1}]}"#;
const ARMORER: &str = "{NoAI:1b,VillagerDataFinalized:1b,VillagerData:{type:\"minecraft:plains\",profession:\"minecraft:armorer\",level:1}}";
const SHAPELESS: &str = r#"{"type":"minecraft:crafting_shapeless","ingredients":["minecraft:stick"],"result":{"id":"minecraft:diamond"}}"#;
const IMPOSSIBLE: &str = r#"{"criteria":{"c":{"trigger":"minecraft:impossible"}}}"#;

fn temp(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("rspigot-loader-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn folder_pack(root: &Path) {
    write(root, "p/pack.mcmeta", META);
    for (relative, text) in files() {
        write(root, &format!("p/{relative}"), text);
    }
}

fn zip_pack(root: &Path) {
    let file = std::fs::File::create(root.join("p.zip")).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("pack.mcmeta", options).unwrap();
    writer.write_all(META.as_bytes()).unwrap();
    for (relative, text) in files() {
        writer.start_file(relative, options).unwrap();
        writer.write_all(text.as_bytes()).unwrap();
    }
    writer.finish().unwrap();
}

fn files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("data/t/trial_spawner/husks.json", HUSKS),
        ("data/t/villager_trade/gem.json", GEM),
        (
            "data/minecraft/trade_set/armorer/level_1.json",
            r#"{"amount":1,"trades":["t:gem"]}"#,
        ),
        (
            "data/t/jukebox_song/tune.json",
            r#"{"length_in_seconds":2.0,"comparator_output":3}"#,
        ),
        (
            "data/minecraft/jukebox_song/11.json",
            r#"{"length_in_seconds":2.0,"comparator_output":3}"#,
        ),
        (
            "data/t/painting_variant/wide.json",
            r#"{"width":4,"height":2}"#,
        ),
        (
            "data/minecraft/painting_variant/alban.json",
            r#"{"width":2,"height":3}"#,
        ),
        (
            "data/minecraft/tags/painting_variant/placeable.json",
            r#"{"values":["t:wide"]}"#,
        ),
        ("data/t/advancement/hello.json", IMPOSSIBLE),
        ("data/t/recipe/sticks.json", SHAPELESS),
    ]
}

fn attach(s: Scene, root: &Path) -> Scene {
    Scene {
        server: s
            .server
            .with_datapacks(Arc::new(DirectoryDatapacks::new(root))),
        plugin: s.plugin,
        players: s.players,
    }
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn prop(s: &mut Scene, pos: BlockPos, name: &str) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state
            .properties
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_default(),
        other => panic!("expected a block, got {other:?}"),
    }
}

fn level_events(alice: &Player) -> Vec<(i32, i32)> {
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

fn check_registries_through_the_server(root: &Path) {
    let mut s = attach(world(Vec::new(), Vec::new()), root);
    let alban = hanging::variant_by_name("minecraft:alban").unwrap();
    assert_eq!(hanging::variant_size(alban), (2, 3));
    let wide = hanging::variant_by_name("t:wide").unwrap();
    assert_eq!(hanging::variant_size(wide), (4, 2));
    assert!(hanging::placeable_variants().contains(&wide));
    let id = s.players[0].id;
    assert!(s.server.award_advancement(id, "t:hello"));
    assert_eq!(s.server.award_recipes(id, &["t:sticks"]), 1);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    console(
        &mut s,
        &format!("summon minecraft:villager 2.5 64 0.5 {ARMORER}"),
    );
    let villager = s.server.entities_of(Kind::Villager)[0].id;
    let alice = s.players[0].clone();
    alice.link.take();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target: villager,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
    let offers = alice
        .link
        .take()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::MerchantOffers { offers, .. } => Some(offers),
            _ => None,
        })
        .expect("offers");
    assert_eq!(offers.len(), 1);
    assert_eq!(
        offers[0].result.item.kind().unwrap().name(),
        "minecraft:diamond"
    );
}

fn check_jukebox(root: &Path) {
    let s = world(vec![Seed::block(JUKEBOX, BlockKind::Jukebox)], Vec::new());
    let mut s = attach(s, root);
    let alice = s.players[0].clone();
    console(&mut s, "gamemode survival Alice");
    console(&mut s, "give Alice minecraft:music_disc_11");
    alice.link.take();
    screen::use_block(&mut s, &alice, JUKEBOX);
    s.tick(30);
    assert!(!level_events(&alice).contains(&(1011, 0)));
    s.tick(60);
    assert!(level_events(&alice).contains(&(1011, 0)));
}

fn check_running_trial_spawner(root: &Path) {
    let seed = Seed::block(SPAWNER, BlockKind::TrialSpawner)
        .carrying("normal_config", Tag::String("t:husks".to_owned()))
        .carrying("ominous_config", Tag::String("t:husks".to_owned()))
        .carrying("target_cooldown_length", 100)
        .carrying("required_player_range", 14);
    let s = world_spawning(vec![seed], Vec::new(), true);
    let mut s = attach(s, root);
    console(&mut s, "gamemode survival Alice");
    s.tick(120);
    assert_eq!(prop(&mut s, SPAWNER, "trial_spawner_state"), "active");
    assert_eq!(s.server.entities_of(Kind::Villager).len(), 2);
}

#[test]
fn every_new_registry_kind_loads_from_a_pack_folder() {
    let root = temp("folder");
    folder_pack(&root);
    check_registries_through_the_server(&root);
    check_jukebox(&root);
    check_running_trial_spawner(&root);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn every_new_registry_kind_loads_from_a_zipped_pack() {
    let root = temp("zip");
    zip_pack(&root);
    check_registries_through_the_server(&root);
    check_jukebox(&root);
    check_running_trial_spawner(&root);
    std::fs::remove_dir_all(&root).unwrap();
}
