mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::plugins::{Answer, BlockOrder, EventKind, EventPayload, Order, Question};
use rspigot_domain::{BlockPos, Vec3};
use seeded::{Seed, contents, world};

const DISPENSER: BlockPos = BlockPos::new(3, 64, 3);
const FRONT: BlockPos = BlockPos::new(4, 64, 3);
const POWER: BlockPos = BlockPos::new(3, 64, 2);

fn dispenser(items: &[(&'static str, u8)]) -> Seed {
    let mut seed = Seed::block(DISPENSER, BlockKind::Dispenser).with("facing", "east");
    for (key, count) in items {
        seed = seed.holding(key, *count);
    }
    seed
}

fn power(s: &mut Scene) {
    s.act(Order::Block {
        pos: POWER,
        order: BlockOrder::Set("minecraft:redstone_block".to_owned()),
    });
}

fn state_at(s: &mut Scene, pos: BlockPos) -> rspigot_core::plugins::BlockState {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state,
        other => panic!("expected a block, got {other:?}"),
    }
}

fn prop(s: &mut Scene, pos: BlockPos, name: &str) -> String {
    state_at(s, pos)
        .properties
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
        .unwrap_or_default()
}

fn count_of(s: &mut Scene, kind: &str) -> usize {
    match s.ask(Question::Entities {
        near: None,
        kind: Some(kind.to_owned()),
    }) {
        Answer::Entities(entities) => entities.len(),
        other => panic!("expected entities, got {other:?}"),
    }
}

fn spawn(s: &mut Scene, kind: &str) {
    let answer = s.ask(Question::Spawn {
        kind: kind.to_owned(),
        position: Vec3::new(4.5, 64.0, 3.5),
    });
    assert!(matches!(answer, Answer::Spawned(_)));
}

fn fire(items: &[(&'static str, u8)], extra: Vec<Seed>) -> Scene {
    let mut seeds = vec![dispenser(items)];
    seeds.extend(extra);
    let mut s = world(seeds, Vec::new());
    power(&mut s);
    s.tick(5);
    s
}

fn left_in_dispenser(s: &mut Scene) -> Vec<Option<(&'static str, u8)>> {
    let alice = s.players[0].clone();
    contents(s, &alice, DISPENSER, 1)
}

#[test]
fn honeycomb_waxes_the_copper_block_in_front_and_keeps_the_rest() {
    let mut s = fire(
        &[("minecraft:honeycomb", 2)],
        vec![Seed::block(FRONT, BlockKind::CopperBlock)],
    );
    assert_eq!(state_at(&mut s, FRONT).kind, "minecraft:waxed_copper_block");
    assert_eq!(
        left_in_dispenser(&mut s),
        [Some(("minecraft:honeycomb", 1))]
    );
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn waxing_keeps_the_properties_of_the_block() {
    let mut s = fire(
        &[("minecraft:honeycomb", 1)],
        vec![
            Seed::block(FRONT, BlockKind::ExposedCutCopperStairs)
                .with("facing", "south")
                .with("half", "top"),
        ],
    );
    assert_eq!(
        state_at(&mut s, FRONT).kind,
        "minecraft:waxed_exposed_cut_copper_stairs"
    );
    assert_eq!(prop(&mut s, FRONT, "facing"), "south");
    assert_eq!(prop(&mut s, FRONT, "half"), "top");
}

#[test]
fn honeycomb_in_front_of_anything_else_is_dropped() {
    let mut s = fire(
        &[("minecraft:honeycomb", 2)],
        vec![Seed::block(FRONT, BlockKind::Stone)],
    );
    assert_eq!(state_at(&mut s, FRONT).kind, "minecraft:stone");
    assert_eq!(s.server.ground_items().len(), 1);
}

#[test]
fn glowstone_charges_a_respawn_anchor_by_one() {
    let mut s = fire(
        &[("minecraft:glowstone", 2)],
        vec![Seed::block(FRONT, BlockKind::RespawnAnchor).with("charges", "2")],
    );
    assert_eq!(prop(&mut s, FRONT, "charges"), "3");
    assert_eq!(
        left_in_dispenser(&mut s),
        [Some(("minecraft:glowstone", 1))]
    );
}

#[test]
fn glowstone_stays_in_the_dispenser_when_the_anchor_is_full() {
    let mut s = fire(
        &[("minecraft:glowstone", 2)],
        vec![Seed::block(FRONT, BlockKind::RespawnAnchor).with("charges", "4")],
    );
    assert_eq!(prop(&mut s, FRONT, "charges"), "4");
    assert_eq!(
        left_in_dispenser(&mut s),
        [Some(("minecraft:glowstone", 2))]
    );
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn glowstone_in_front_of_anything_else_is_dropped() {
    let s = fire(&[("minecraft:glowstone", 2)], Vec::new());
    assert_eq!(s.server.ground_items().len(), 1);
}

#[test]
fn a_shulker_box_is_placed_in_front_facing_up_on_solid_ground() {
    let mut s = fire(&[("minecraft:shulker_box", 1)], Vec::new());
    assert_eq!(state_at(&mut s, FRONT).kind, "minecraft:shulker_box");
    assert_eq!(prop(&mut s, FRONT, "facing"), "up");
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn a_shulker_box_faces_the_dispenser_direction_over_a_gap() {
    let mut s = fire(
        &[("minecraft:blue_shulker_box", 1)],
        vec![Seed::block(BlockPos::new(4, 63, 3), BlockKind::Air)],
    );
    assert_eq!(state_at(&mut s, FRONT).kind, "minecraft:blue_shulker_box");
    assert_eq!(prop(&mut s, FRONT, "facing"), "east");
}

#[test]
fn a_shulker_box_that_does_not_fit_stays_in_the_dispenser() {
    let mut s = fire(
        &[("minecraft:shulker_box", 1)],
        vec![Seed::block(FRONT, BlockKind::Stone)],
    );
    assert_eq!(state_at(&mut s, FRONT).kind, "minecraft:stone");
    assert_eq!(
        left_in_dispenser(&mut s),
        [Some(("minecraft:shulker_box", 1))]
    );
    assert!(s.server.ground_items().is_empty());
}

fn soul_sand_base() -> Vec<Seed> {
    let sand = |x, y| Seed::block(BlockPos::new(x, y, 3), BlockKind::SoulSand);
    let air = |x, y| Seed::block(BlockPos::new(x, y, 3), BlockKind::Air);
    vec![
        sand(3, 63),
        sand(4, 63),
        sand(5, 63),
        sand(4, 62),
        air(3, 62),
        air(5, 62),
    ]
}

#[test]
fn a_wither_skull_is_placed_on_a_soul_sand_base_with_the_dispenser_rotation() {
    let mut s = fire(&[("minecraft:wither_skeleton_skull", 1)], soul_sand_base());
    assert_eq!(
        state_at(&mut s, FRONT).kind,
        "minecraft:wither_skeleton_skull"
    );
    assert_eq!(prop(&mut s, FRONT, "rotation"), "4");
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn a_wither_skull_without_a_soul_sand_base_is_not_placed() {
    let mut s = fire(&[("minecraft:wither_skeleton_skull", 1)], Vec::new());
    assert_eq!(state_at(&mut s, FRONT).kind, "minecraft:air");
    assert_eq!(s.server.ground_items().len(), 1);
}

#[test]
fn the_projectile_items_each_launch_their_entity() {
    let cases = [
        ("minecraft:fire_charge", "minecraft:small_fireball"),
        ("minecraft:wind_charge", "minecraft:wind_charge"),
        ("minecraft:firework_rocket", "minecraft:firework_rocket"),
        ("minecraft:spectral_arrow", "minecraft:arrow"),
        ("minecraft:tipped_arrow", "minecraft:arrow"),
        ("minecraft:blue_egg", "minecraft:egg"),
        ("minecraft:brown_egg", "minecraft:egg"),
    ];
    for (item, entity) in cases {
        let mut s = fire(&[(item, 1)], Vec::new());
        assert_eq!(count_of(&mut s, entity), 1, "{item}");
        assert_eq!(left_in_dispenser(&mut s), [None], "{item}");
    }
}

#[test]
fn a_fire_charge_no_longer_lights_the_block_in_front() {
    let mut s = fire(&[("minecraft:fire_charge", 1)], Vec::new());
    assert_eq!(state_at(&mut s, FRONT).kind, "minecraft:air");
}

#[test]
fn flint_and_steel_lights_a_candle_and_an_unlit_campfire() {
    for (kind, name) in [
        (BlockKind::Candle, "candle"),
        (BlockKind::Campfire, "campfire"),
    ] {
        let mut s = fire(
            &[("minecraft:flint_and_steel", 1)],
            vec![Seed::block(FRONT, kind).with("lit", "false")],
        );
        assert_eq!(prop(&mut s, FRONT, "lit"), "true", "{name}");
        assert_eq!(
            left_in_dispenser(&mut s),
            [Some(("minecraft:flint_and_steel", 1))],
            "{name}"
        );
    }
}

#[test]
fn flint_and_steel_on_a_lit_candle_stays_in_the_dispenser() {
    let mut s = fire(
        &[("minecraft:flint_and_steel", 1)],
        vec![Seed::block(FRONT, BlockKind::Candle).with("lit", "true")],
    );
    assert_eq!(
        left_in_dispenser(&mut s),
        [Some(("minecraft:flint_and_steel", 1))]
    );
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn a_brush_scrapes_a_scute_off_an_armadillo_in_front() {
    let mut s = world(
        vec![dispenser(&[("minecraft:brush", 1)])],
        vec![observe(1, EventKind::BlockDispenseArmor)],
    );
    spawn(&mut s, "minecraft:armadillo");
    power(&mut s);
    s.tick(5);
    let heard = settled_for(&s.plugin, EventKind::BlockDispenseArmor);
    let EventPayload::BlockDispenseArmor(event) = &heard.first().expect("an armor event").payload
    else {
        panic!("an armor event");
    };
    assert_eq!(event.target_entity.kind, "minecraft:armadillo");
    assert_eq!(left_in_dispenser(&mut s), [Some(("minecraft:brush", 1))]);
    assert!(!s.server.ground_items().is_empty());
}

#[test]
fn a_brush_with_no_armadillo_in_front_stays_untouched() {
    let mut s = fire(&[("minecraft:brush", 1)], Vec::new());
    assert_eq!(left_in_dispenser(&mut s), [Some(("minecraft:brush", 1))]);
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn a_water_creature_bucket_pours_its_water_and_releases_the_creature() {
    let cases = [
        ("minecraft:salmon_bucket", "minecraft:salmon"),
        ("minecraft:cod_bucket", "minecraft:cod"),
        ("minecraft:pufferfish_bucket", "minecraft:pufferfish"),
        ("minecraft:tropical_fish_bucket", "minecraft:tropical_fish"),
        ("minecraft:axolotl_bucket", "minecraft:axolotl"),
        ("minecraft:tadpole_bucket", "minecraft:tadpole"),
    ];
    for (item, entity) in cases {
        let mut s = fire(&[(item, 1)], Vec::new());
        assert_eq!(state_at(&mut s, FRONT).kind, "minecraft:water", "{item}");
        assert_eq!(count_of(&mut s, entity), 1, "{item}");
        assert_eq!(
            left_in_dispenser(&mut s),
            [Some(("minecraft:bucket", 1))],
            "{item}"
        );
    }
}

#[test]
fn a_creature_bucket_in_front_of_a_wall_is_dropped() {
    let mut s = fire(
        &[("minecraft:salmon_bucket", 1)],
        vec![Seed::block(FRONT, BlockKind::Stone)],
    );
    assert_eq!(count_of(&mut s, "minecraft:salmon"), 0);
    assert_eq!(s.server.ground_items().len(), 1);
}

#[test]
fn shears_harvest_a_full_beehive_in_front() {
    let mut s = fire(
        &[("minecraft:shears", 1)],
        vec![Seed::block(FRONT, BlockKind::Beehive).with("honey_level", "5")],
    );
    assert_eq!(prop(&mut s, FRONT, "honey_level"), "0");
    assert!(!s.server.ground_items().is_empty());
    assert_eq!(left_in_dispenser(&mut s), [Some(("minecraft:shears", 1))]);
}

#[test]
fn shears_leave_a_hive_that_is_not_full_alone() {
    let mut s = fire(
        &[("minecraft:shears", 1)],
        vec![Seed::block(FRONT, BlockKind::Beehive).with("honey_level", "4")],
    );
    assert_eq!(prop(&mut s, FRONT, "honey_level"), "4");
    assert!(s.server.ground_items().is_empty());
}

fn summon_tame(s: &mut Scene, kind: &str, tame: bool) {
    let nbt = if tame { "{Tame:1b}" } else { "{}" };
    s.server
        .handle(rspigot_core::events::Inbound::Console(format!(
            "summon {kind} 4.5 64 3.5 {nbt}"
        )));
    s.server.deliver();
}

#[test]
fn a_chest_is_strapped_onto_a_tame_donkey_in_front() {
    let mut s = world(
        vec![dispenser(&[("minecraft:chest", 1)])],
        vec![observe(1, EventKind::BlockDispenseArmor)],
    );
    summon_tame(&mut s, "minecraft:donkey", true);
    power(&mut s);
    s.tick(5);
    let heard = settled_for(&s.plugin, EventKind::BlockDispenseArmor);
    let EventPayload::BlockDispenseArmor(event) = &heard.first().expect("an armor event").payload
    else {
        panic!("an armor event");
    };
    assert_eq!(event.target_entity.kind, "minecraft:donkey");
    assert_eq!(left_in_dispenser(&mut s), [None]);
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn a_chest_in_front_of_a_wild_donkey_is_dropped() {
    let mut s = world(vec![dispenser(&[("minecraft:chest", 1)])], Vec::new());
    summon_tame(&mut s, "minecraft:donkey", false);
    power(&mut s);
    s.tick(5);
    assert_eq!(s.server.ground_items().len(), 1);
}
