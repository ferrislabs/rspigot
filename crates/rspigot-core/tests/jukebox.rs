mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::events::{ClientEvent, Inbound};
use rspigot_core::plugins::{Answer, Question};
use rspigot_domain::BlockPos;
use screen::use_block;
use seeded::{Seed, world};

const JUKEBOX: BlockPos = BlockPos::new(3, 64, 3);
const LAMP: BlockPos = BlockPos::new(4, 64, 3);

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn scene_with_disc(extra: Vec<Seed>, disc: &str) -> (Scene, Player) {
    let mut seeds = vec![Seed::block(JUKEBOX, BlockKind::Jukebox)];
    seeds.extend(extra);
    let mut s = world(seeds, Vec::new());
    let alice = s.players[0].clone();
    console(&mut s, "gamemode survival Alice");
    if !disc.is_empty() {
        console(&mut s, &format!("give Alice {disc}"));
    }
    alice.link.take();
    (s, alice)
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

#[test]
fn a_disc_used_on_a_jukebox_starts_its_song_and_is_spent() {
    let (mut s, alice) = scene_with_disc(Vec::new(), "minecraft:music_disc_13");
    use_block(&mut s, &alice, JUKEBOX);
    s.tick(1);
    assert_eq!(prop(&mut s, JUKEBOX, "has_record"), "true");
    assert!(level_events(&alice).contains(&(1010, 1)));
    let held = match s.ask(Question::Inventory(alice.id)) {
        Answer::Inventory(inventory) => inventory
            .slots
            .iter()
            .flatten()
            .filter(|item| item.key == "minecraft:music_disc_13")
            .count(),
        other => panic!("expected the inventory, got {other:?}"),
    };
    assert_eq!(held, 0);
}

#[test]
fn a_playing_jukebox_powers_its_neighbours() {
    let (mut s, alice) = scene_with_disc(
        vec![Seed::block(LAMP, BlockKind::RedstoneLamp)],
        "minecraft:music_disc_13",
    );
    assert_eq!(prop(&mut s, LAMP, "lit"), "false");
    use_block(&mut s, &alice, JUKEBOX);
    s.tick(3);
    assert_eq!(prop(&mut s, LAMP, "lit"), "true");
}

#[test]
fn the_song_ends_one_second_after_its_length() {
    let (mut s, alice) = scene_with_disc(
        vec![Seed::block(LAMP, BlockKind::RedstoneLamp)],
        "minecraft:music_disc_11",
    );
    use_block(&mut s, &alice, JUKEBOX);
    s.tick(1400);
    assert!(!level_events(&alice).contains(&(1011, 0)));
    s.tick(60);
    assert!(level_events(&alice).contains(&(1011, 0)));
    assert_eq!(prop(&mut s, JUKEBOX, "has_record"), "true");
    s.tick(5);
    assert_eq!(prop(&mut s, LAMP, "lit"), "false");
}

#[test]
fn an_empty_hand_pops_the_disc_out_and_stops_the_music() {
    let (mut s, alice) = scene_with_disc(Vec::new(), "minecraft:music_disc_13");
    use_block(&mut s, &alice, JUKEBOX);
    s.tick(2);
    alice.link.take();
    console(&mut s, "clear Alice");
    use_block(&mut s, &alice, JUKEBOX);
    s.tick(1);
    assert_eq!(prop(&mut s, JUKEBOX, "has_record"), "false");
    assert!(level_events(&alice).contains(&(1011, 0)));
    let ground = s.server.ground_items();
    assert_eq!(ground.len(), 1);
    assert_eq!(ground[0].1.item, rspigot_items::ItemKind::MusicDisc13.id());
}

#[test]
fn breaking_a_jukebox_drops_the_disc() {
    let (mut s, alice) = scene_with_disc(Vec::new(), "minecraft:music_disc_13");
    use_block(&mut s, &alice, JUKEBOX);
    s.tick(2);
    alice.link.take();
    s.act(rspigot_core::plugins::Order::Block {
        pos: JUKEBOX,
        order: rspigot_core::plugins::BlockOrder::Set("minecraft:air".to_owned()),
    });
    s.tick(1);
    assert!(level_events(&alice).contains(&(1011, 0)));
    assert!(
        s.server
            .ground_items()
            .iter()
            .any(|(_, stack)| stack.item == rspigot_items::ItemKind::MusicDisc13.id())
    );
}

#[test]
fn a_hopper_feeds_a_disc_to_a_jukebox_and_takes_it_back() {
    let hopper = BlockPos::new(3, 65, 3);
    let (mut s, alice) = scene_with_disc(
        vec![
            Seed::block(hopper, BlockKind::Hopper)
                .with("facing", "down")
                .holding("minecraft:music_disc_13", 1),
        ],
        "",
    );
    s.tick(2);
    assert_eq!(prop(&mut s, JUKEBOX, "has_record"), "true");
    assert!(level_events(&alice).contains(&(1010, 1)));
}

#[test]
fn a_hopper_refuses_an_item_that_is_not_a_disc() {
    let hopper = BlockPos::new(3, 65, 3);
    let (mut s, _alice) = scene_with_disc(
        vec![
            Seed::block(hopper, BlockKind::Hopper)
                .with("facing", "down")
                .holding("minecraft:stone", 1),
        ],
        "",
    );
    s.tick(2);
    assert_eq!(prop(&mut s, JUKEBOX, "has_record"), "false");
}

#[test]
fn a_comparator_reads_the_song_of_the_disc() {
    let comparator = BlockPos::new(4, 64, 3);
    let lamp = BlockPos::new(5, 64, 3);
    let (mut s, alice) = scene_with_disc(
        vec![
            Seed::block(comparator, BlockKind::Comparator).with("facing", "west"),
            Seed::block(lamp, BlockKind::RedstoneLamp),
        ],
        "minecraft:music_disc_13",
    );
    use_block(&mut s, &alice, JUKEBOX);
    s.tick(4);
    assert_eq!(prop(&mut s, comparator, "powered"), "true");
    assert_eq!(prop(&mut s, lamp, "lit"), "true");
}
