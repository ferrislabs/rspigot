mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::commands::Target;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, Question};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Meadow;

impl ChunkGenerator for Meadow {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                let at = |y: i32| BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                for y in 60..=62 {
                    chunk.set_block(at(y), BlockStateId(1));
                }
                chunk.set_block(at(63), BlockKind::GrassBlock.default_state());
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn ready(seed: u64) -> Scene {
    let config = GameConfig {
        view_distance: 5,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        day_time: 6_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Meadow),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(8, 64, 8));
    s.tick(40);
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, kind: Kind, at: &str, nbt: &str) -> EntityId {
    let known: Vec<EntityId> = s
        .server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .collect();
    console(s, &format!("summon {} {at} {nbt}", kind.name()));
    s.server
        .entities_of(kind)
        .into_iter()
        .map(|entity| entity.id)
        .find(|id| !known.contains(id))
        .expect("a new entity")
}

fn saved(s: &Scene, id: EntityId) -> rspigot_nbt::Compound {
    s.server.saved_entity(id).expect("a saved entity")
}

fn position_of(s: &Scene, id: EntityId) -> Vec3 {
    s.server
        .entities_of(Kind::Allay)
        .into_iter()
        .find(|entity| entity.id == id)
        .expect("the allay")
        .position
}

fn health_of(s: &Scene, id: EntityId) -> f32 {
    s.server
        .entities_of(Kind::Allay)
        .into_iter()
        .find(|entity| entity.id == id)
        .and_then(|entity| entity.health)
        .expect("the allay")
}

fn mainhand(s: &Scene, id: EntityId) -> Option<(String, i32)> {
    let nbt = saved(s, id);
    let hand = nbt.get_compound("equipment")?.get_compound("mainhand")?;
    Some((
        hand.get_str("id")?.to_owned(),
        hand.get_i32("count").unwrap_or(1),
    ))
}

fn interact(s: &mut Scene, target: EntityId) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.5, 0.0),
        },
    );
    s.tick(1);
}

fn select_first_slot(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::HeldSlot(0));
}

fn ground_of(s: &Scene, item: &str) -> usize {
    let kind = match item {
        "apple" => rspigot_items::ItemKind::Apple,
        "carrot" => rspigot_items::ItemKind::Carrot,
        other => panic!("{other}"),
    };
    s.server
        .ground_items()
        .iter()
        .filter(|(_, stack)| stack.item == kind.id())
        .count()
}

fn held_count(s: &mut Scene) -> Option<i32> {
    let player = s.players[0].id;
    match s.ask(Question::Inventory(player)) {
        Answer::Inventory(inventory) => inventory.slots[0].as_ref().map(|stack| stack.count),
        other => panic!("{other:?}"),
    }
}

fn brain_has(s: &Scene, id: EntityId, memory: &str) -> bool {
    saved(s, id)
        .get_compound("Brain")
        .and_then(|brain| brain.get_compound("memories"))
        .is_some_and(|memories| memories.contains_key(memory))
}

#[test]
fn an_allay_floats_without_gravity_and_wanders_in_the_air() {
    let mut s = ready(3);
    let allay = summon(&mut s, Kind::Allay, "8.5 72 8.5", "");
    let mut moved = 0.0f64;
    for _ in 0..200 {
        s.tick(1);
        let at = position_of(&s, allay);
        assert!(at.y > 64.0, "{at:?}");
        moved = moved.max((at.x - 8.5).abs() + (at.z - 8.5).abs() + (at.y - 72.0).abs());
    }
    assert!(moved > 1.0, "{moved}");
}

#[test]
fn an_allay_never_takes_fall_damage_and_keeps_its_cooldown_in_nbt() {
    let mut s = ready(3);
    let allay = summon(&mut s, Kind::Allay, "8.5 120 8.5", "{NoAI:1b}");
    s.tick(80);
    assert_eq!(health_of(&s, allay), 20.0);
    assert_eq!(saved(&s, allay).get_i64("DuplicationCooldown"), Some(0));
    let cooling = summon(
        &mut s,
        Kind::Allay,
        "10.5 70 8.5",
        "{NoAI:1b,DuplicationCooldown:3000L}",
    );
    assert_eq!(
        saved(&s, cooling).get_i64("DuplicationCooldown"),
        Some(3000)
    );
}

#[test]
fn giving_an_item_to_an_allay_makes_it_hold_exactly_one_and_like_the_giver() {
    let mut s = ready(3);
    let allay = summon(&mut s, Kind::Allay, "8.5 65 9.5", "{NoAI:1b}");
    console(&mut s, "give Alice minecraft:apple 3");
    select_first_slot(&mut s);
    s.tick(2);
    interact(&mut s, allay);
    assert_eq!(mainhand(&s, allay), Some(("minecraft:apple".to_owned(), 1)));
    assert!(brain_has(&s, allay, "minecraft:liked_player"));
    s.tick(1);
    assert_eq!(held_count(&mut s), Some(2));
}

#[test]
fn taking_the_item_back_with_an_empty_hand_empties_the_allay_and_forgets_the_player() {
    let mut s = ready(3);
    let allay = summon(&mut s, Kind::Allay, "8.5 65 9.5", "{NoAI:1b}");
    console(&mut s, "give Alice minecraft:apple 1");
    select_first_slot(&mut s);
    s.tick(2);
    interact(&mut s, allay);
    assert!(mainhand(&s, allay).is_some());
    s.tick(1);
    interact(&mut s, allay);
    assert_eq!(mainhand(&s, allay), None);
    assert!(!brain_has(&s, allay, "minecraft:liked_player"));
    assert_eq!(held_count(&mut s), Some(1));
}

#[test]
fn an_allay_that_likes_a_player_cannot_be_hurt_by_them_but_others_can() {
    let mut s = ready(3);
    let allay = summon(&mut s, Kind::Allay, "8.5 65 9.5", "{NoAI:1b}");
    console(&mut s, "give Alice minecraft:apple 1");
    select_first_slot(&mut s);
    s.tick(2);
    interact(&mut s, allay);
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::Attack(allay));
    s.tick(2);
    assert_eq!(health_of(&s, allay), 20.0);
    let foe = summon(&mut s, Kind::Vindicator, "10.5 65 9.5", "{NoAI:1b}");
    s.server.do_hurt_target(foe, Target::Entity(allay));
    assert!(health_of(&s, allay) < 20.0);
}

#[test]
fn an_allay_holding_an_item_picks_up_the_same_item_into_its_inventory() {
    let mut s = ready(3);
    let allay = summon(&mut s, Kind::Allay, "8.5 65 9.5", "{NoAI:1b}");
    console(&mut s, "give Alice minecraft:apple 1");
    select_first_slot(&mut s);
    s.tick(2);
    interact(&mut s, allay);
    s.tick(25);
    console(
        &mut s,
        "summon minecraft:item 8.5 65.2 9.5 {Item:{id:\"minecraft:apple\",count:2}}",
    );
    s.tick(40);
    assert_eq!(ground_of(&s, "apple"), 0);
    let nbt = saved(&s, allay);
    let inventory = nbt.get_list("Inventory").expect("an inventory");
    assert_eq!(inventory.len(), 1);
    let stack = inventory[0].as_compound().unwrap();
    assert_eq!(stack.get_str("id"), Some("minecraft:apple"));
    assert_eq!(stack.get_i32("count"), Some(2));
}

#[test]
fn an_allay_holding_nothing_or_another_item_ignores_dropped_items() {
    let mut s = ready(3);
    summon(&mut s, Kind::Allay, "8.5 65 22.5", "{NoAI:1b}");
    console(
        &mut s,
        "summon minecraft:item 8.5 65.2 22.5 {Item:{id:\"minecraft:apple\",count:1}}",
    );
    s.tick(40);
    assert_eq!(ground_of(&s, "apple"), 1);
    summon(
        &mut s,
        Kind::Allay,
        "12.5 65 22.5",
        "{NoAI:1b,equipment:{mainhand:{id:\"minecraft:carrot\",count:1}}}",
    );
    console(
        &mut s,
        "summon minecraft:item 12.5 65.2 22.5 {Item:{id:\"minecraft:apple\",count:1}}",
    );
    s.tick(40);
    assert_eq!(ground_of(&s, "apple"), 2);
}

#[test]
fn an_allay_carrying_items_throws_them_to_the_player_it_likes() {
    let mut s = ready(3);
    let allay = summon(&mut s, Kind::Allay, "8.5 65 12.5", "");
    console(&mut s, "give Alice minecraft:apple 1");
    select_first_slot(&mut s);
    s.tick(2);
    interact(&mut s, allay);
    assert_eq!(held_count(&mut s), None);
    console(
        &mut s,
        "summon minecraft:item 8.5 65.2 12.5 {Item:{id:\"minecraft:apple\",count:1}}",
    );
    let mut delivered = false;
    for _ in 0..600 {
        s.tick(1);
        if held_count(&mut s) == Some(1) {
            delivered = true;
            break;
        }
    }
    assert!(delivered);
    assert!(
        saved(&s, allay)
            .get_list("Inventory")
            .is_none_or(|list| list.is_empty())
    );
}

#[test]
fn killing_an_allay_drops_its_held_item_and_its_inventory() {
    let mut s = ready(3);
    summon(
        &mut s,
        Kind::Allay,
        "8.5 65 22.5",
        "{NoAI:1b,equipment:{mainhand:{id:\"minecraft:apple\",count:1}},Inventory:[{id:\"minecraft:carrot\",count:3}]}",
    );
    console(&mut s, "kill @e[type=minecraft:allay]");
    s.tick(40);
    assert_eq!(ground_of(&s, "apple"), 1);
    assert_eq!(ground_of(&s, "carrot"), 1);
}

#[test]
fn an_amethyst_shard_does_nothing_to_an_allay_that_is_not_dancing() {
    let mut s = ready(3);
    let allay = summon(&mut s, Kind::Allay, "8.5 65 9.5", "{NoAI:1b}");
    console(&mut s, "give Alice minecraft:amethyst_shard 1");
    select_first_slot(&mut s);
    s.tick(2);
    interact(&mut s, allay);
    assert_eq!(s.server.entities_of(Kind::Allay).len(), 1);
    assert_eq!(
        mainhand(&s, allay),
        Some(("minecraft:amethyst_shard".to_owned(), 1))
    );
}
