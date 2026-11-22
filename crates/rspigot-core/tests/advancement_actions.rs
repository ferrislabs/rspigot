mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClickInput, DigAction, Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::events::{PlayerFishState, PlayerTeleportCause};
use rspigot_core::plugins::{EventKind, EventPayload};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Identifier, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;
use screen::*;
use seeded::Seed;

struct Land {
    floor: std::ops::RangeInclusive<i32>,
    blocks: Vec<(BlockPos, BlockStateId)>,
}

impl ChunkGenerator for Land {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                for y in self.floor.clone() {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockKind::Stone.default_state(),
                    );
                }
            }
        }
        for (at, state) in &self.blocks {
            if at.chunk() == pos {
                chunk.set_block(*at, *state);
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

fn land(
    floor: std::ops::RangeInclusive<i32>,
    blocks: Vec<(BlockPos, BlockStateId)>,
) -> Server<Link> {
    let config = GameConfig {
        view_distance: 3,
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
        Arc::new(Land { floor, blocks }),
        StatusBoard::new(20),
    )
    .with_rng_seed(7);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn scene_of(server: Server<Link>) -> Scene {
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(9, 64, 9));
    s
}

fn meadow(blocks: Vec<(BlockPos, BlockStateId)>) -> Scene {
    scene_of(land(60..=63, blocks))
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn done(s: &Scene, advancement: &str) -> bool {
    let alice = s.players[0].id;
    s.server.done_advancements(alice).contains(&advancement)
}

fn nether_server() -> Scene {
    let mut server = configured(|_| {});
    server.load_dimension(
        Identifier::minecraft("the_nether"),
        Dimension::nether(),
        Arc::new(Floor),
        Arc::new(NoStore),
    );
    scene_on(server, &["Alice"], Vec::new())
}

fn there_and_back(back_to: Vec3) -> Scene {
    let mut s = nether_server();
    let alice = s.players[0].clone();
    s.server.change_dimension(
        alice.id,
        Identifier::minecraft("the_nether"),
        Vec3::new(8.5, 70.0, 8.5),
        None,
        PlayerTeleportCause::NetherPortal,
    );
    s.server.deliver();
    s.tick(3);
    s.server.change_dimension(
        alice.id,
        Identifier::minecraft("overworld"),
        back_to,
        None,
        PlayerTeleportCause::NetherPortal,
    );
    s.server.deliver();
    s.tick(3);
    s
}

#[test]
fn leaving_the_nether_seven_thousand_blocks_from_the_entry_completes_fast_travel() {
    let s = there_and_back(Vec3::new(7_508.5, 70.0, 8.5));
    assert!(done(&s, "minecraft:nether/fast_travel"));
}

#[test]
fn leaving_the_nether_close_to_the_entry_does_not_complete_fast_travel() {
    let s = there_and_back(Vec3::new(508.5, 70.0, 8.5));
    assert!(!done(&s, "minecraft:nether/fast_travel"));
}

fn ride(s: &mut Scene, rider: &str, vehicle: &str) {
    console(s, &format!("ride {rider} mount {vehicle}"));
    s.tick(2);
}

#[test]
fn boarding_a_boat_that_carries_a_goat_completes_its_advancement() {
    let mut s = meadow(Vec::new());
    console(&mut s, "summon oak_boat 9.5 64 9.5");
    console(&mut s, "summon goat 9.5 64 9.5");
    s.tick(2);
    ride(&mut s, "@e[type=goat]", "@e[type=oak_boat]");
    ride(&mut s, "Alice", "@e[type=oak_boat]");
    assert!(done(&s, "minecraft:husbandry/ride_a_boat_with_a_goat"));
}

#[test]
fn boarding_an_empty_boat_does_not_complete_the_goat_advancement() {
    let mut s = meadow(Vec::new());
    console(&mut s, "summon oak_boat 9.5 64 9.5");
    s.tick(2);
    ride(&mut s, "Alice", "@e[type=oak_boat]");
    assert!(!done(&s, "minecraft:husbandry/ride_a_boat_with_a_goat"));
}

fn skull() -> BlockStateId {
    BlockKind::WitherSkeletonSkull.default_state()
}

fn soul_sand() -> BlockStateId {
    BlockKind::SoulSand.default_state()
}

fn wither_frame(skulls: &[i32]) -> Vec<(BlockPos, BlockStateId)> {
    let mut blocks = vec![(BlockPos::new(11, 64, 11), soul_sand())];
    for dx in 0..3 {
        blocks.push((BlockPos::new(10 + dx, 65, 11), soul_sand()));
        if skulls.contains(&dx) {
            blocks.push((BlockPos::new(10 + dx, 66, 11), skull()));
        }
    }
    blocks
}

fn place_skull(s: &mut Scene, against: BlockPos) {
    console(s, "give Alice wither_skeleton_skull 1");
    s.tick(2);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: against,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 1,
        },
    );
    s.tick(3);
}

#[test]
fn building_a_wither_next_to_a_player_completes_its_advancement() {
    let mut s = meadow(wither_frame(&[0, 2]));
    console(&mut s, "difficulty hard");
    place_skull(&mut s, BlockPos::new(11, 65, 11));
    assert_eq!(s.server.entities_of(Kind::Wither).len(), 1);
    assert!(done(&s, "minecraft:nether/summon_wither"));
}

#[test]
fn an_unfinished_wither_frame_completes_nothing() {
    let mut s = meadow(wither_frame(&[0]));
    console(&mut s, "difficulty hard");
    place_skull(&mut s, BlockPos::new(11, 65, 11));
    assert!(s.server.entities_of(Kind::Wither).is_empty());
    assert!(!done(&s, "minecraft:nether/summon_wither"));
}

fn walk(s: &mut Scene, position: Vec3, on_ground: bool) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: Some(position),
            rotation: None,
            on_ground,
        },
    );
}

fn drop_to(s: &mut Scene, from: f64, to: f64) {
    walk(s, Vec3::new(8.5, from, 8.5), false);
    let mut y = from;
    while y - 1.0 > to {
        y -= 1.0;
        walk(s, Vec3::new(8.5, y, 8.5), false);
        if (from - y) as i64 % 4 == 0 {
            s.tick(1);
        }
    }
    walk(s, Vec3::new(8.5, to, 8.5), true);
    s.tick(2);
}

fn pit() -> Scene {
    let mut s = scene_of(land(-64..=-61, Vec::new()));
    console(&mut s, "gamemode creative Alice");
    let alice = s.players[0].clone();
    alice.link.take();
    console(&mut s, "tp Alice 8.5 319.5 8.5");
    s.tick(2);
    let confirm = alice.link.take().into_iter().find_map(|event| match event {
        rspigot_core::events::ClientEvent::Teleport { id, .. } => Some(id),
        _ => None,
    });
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::TeleportConfirm(confirm.expect("a teleport")),
    );
    s.tick(2);
    s
}

#[test]
fn falling_from_the_build_limit_to_the_bottom_completes_its_advancement() {
    let mut s = pit();
    drop_to(&mut s, 319.5, -60.0);
    assert!(done(&s, "minecraft:adventure/fall_from_world_height"));
}

#[test]
fn a_short_fall_does_not_complete_the_world_height_advancement() {
    let mut s = pit();
    drop_to(&mut s, 319.5, 200.0);
    assert!(!done(&s, "minecraft:adventure/fall_from_world_height"));
}

fn release(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Dig {
            action: DigAction::ReleaseUseItem,
            pos: BlockPos::new(0, 0, 0),
            face: Direction::Down,
            sequence: 0,
        },
    );
}

fn shoot_at_target(away_before_impact: bool) -> Scene {
    let target = (BlockPos::new(8, 65, 12), BlockKind::Target.default_state());
    let mut s = meadow(vec![target]);
    console(&mut s, "give Alice bow 1");
    console(&mut s, "give Alice arrow 5");
    s.tick(2);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, 0.0),
        },
    );
    s.tick(25);
    release(&mut s);
    if away_before_impact {
        console(&mut s, "tp Alice 8.5 64 -30.5");
    }
    s.tick(6);
    s
}

#[test]
fn an_arrow_hitting_a_target_center_from_afar_completes_bullseye() {
    let s = shoot_at_target(true);
    assert!(done(&s, "minecraft:adventure/bullseye"));
}

#[test]
fn an_arrow_hitting_a_target_from_close_does_not_complete_bullseye() {
    let s = shoot_at_target(false);
    assert!(!done(&s, "minecraft:adventure/bullseye"));
}

const STAND: BlockPos = BlockPos::new(0, 64, 2);
const HOTBAR_ZERO: i16 = 32;

fn water() -> rspigot_domain::item::ItemStack {
    rspigot_domain::potion::stack(rspigot_items::ItemKind::Potion, "minecraft:water")
}

fn move_stack(
    s: &mut Scene,
    alice: &Player,
    view: &mut View,
    from: i16,
    to: i16,
    item: rspigot_domain::item::ItemStack,
) {
    let slot = Some(item);
    let take = predict(
        view,
        press(from, 0, ClickInput::Pickup),
        &[(from, None)],
        slot.clone(),
    );
    send_click(s, alice, view, take);
    let put = predict(view, press(to, 0, ClickInput::Pickup), &[(to, slot)], None);
    send_click(s, alice, view, put);
}

fn bottle_in_a_stand(take_it_out: bool) -> Scene {
    let mut s = scene_with(&["Alice"], Vec::new());
    let alice = s.players[0].clone();
    set_block(&mut s, STAND, "minecraft:brewing_stand");
    s.act(rspigot_core::plugins::Order::Inventory {
        player: alice.id,
        order: rspigot_core::plugins::InventoryOrder::Set {
            slot: 0,
            item: Some({
                let mut spec = spec("minecraft:potion", 1);
                let contents = rspigot_nbt::Compound::new().with(
                    "minecraft:potion_contents",
                    rspigot_nbt::Compound::new().with("potion", "minecraft:water"),
                );
                rspigot_nbt::write_network(
                    &mut spec.components,
                    &rspigot_nbt::Tag::Compound(contents),
                );
                spec
            }),
        },
    });
    alice.link.take();
    let mut view = open_block(&mut s, &alice, STAND);
    refresh(&alice, &mut view);
    move_stack(&mut s, &alice, &mut view, HOTBAR_ZERO, 0, water());
    if take_it_out {
        move_stack(&mut s, &alice, &mut view, 0, HOTBAR_ZERO, water());
    }
    s.tick(2);
    s
}

#[test]
fn taking_a_potion_out_of_a_brewing_stand_completes_local_brewery() {
    let s = bottle_in_a_stand(true);
    assert!(done(&s, "minecraft:nether/brew_potion"));
}

#[test]
fn leaving_the_potion_in_the_brewing_stand_completes_nothing() {
    let s = bottle_in_a_stand(false);
    assert!(!done(&s, "minecraft:nether/brew_potion"));
}

const OMINOUS_BANNER: &str = r#"{id:"minecraft:white_banner",count:1,components:{"minecraft:banner_patterns":[{pattern:"minecraft:rhombus",color:"cyan"},{pattern:"minecraft:stripe_bottom",color:"light_gray"},{pattern:"minecraft:stripe_center",color:"gray"},{pattern:"minecraft:border",color:"light_gray"},{pattern:"minecraft:stripe_middle",color:"black"},{pattern:"minecraft:half_horizontal",color:"light_gray"},{pattern:"minecraft:circle",color:"light_gray"},{pattern:"minecraft:border",color:"black"}],"minecraft:tooltip_display":{hidden_components:["minecraft:banner_patterns"]},"minecraft:item_name":{translate:"block.minecraft.ominous_banner"},"minecraft:rarity":"uncommon"}}"#;

fn strike_first(s: &mut Scene, kind: Kind) {
    s.tick(25);
    let alice = s.players[0].clone();
    let target = s.server.entities_of(kind)[0].id;
    send_input(&mut s.server, &alice, PlayerInput::Attack(target));
    s.tick(30);
}

fn killing_a_pillager(head: Option<&str>) -> Scene {
    let mut s = meadow(Vec::new());
    let equipment = head.map_or(String::new(), |banner| {
        format!(",equipment:{{head:{banner}}}")
    });
    console(
        &mut s,
        &format!("summon pillager 9.5 64 9.5 {{NoAI:1b,Health:1f{equipment}}}"),
    );
    strike_first(&mut s, Kind::Pillager);
    assert!(s.server.entities_of(Kind::Pillager).is_empty());
    s
}

#[test]
fn killing_a_banner_carrying_captain_completes_voluntary_exile() {
    let s = killing_a_pillager(Some(OMINOUS_BANNER));
    assert!(done(&s, "minecraft:adventure/voluntary_exile"));
}

#[test]
fn killing_a_pillager_without_the_banner_does_not_complete_voluntary_exile() {
    let s = killing_a_pillager(None);
    assert!(!done(&s, "minecraft:adventure/voluntary_exile"));
}

fn shoot_a_cow() -> Scene {
    let mut s = meadow(Vec::new());
    console(&mut s, "summon cow 8.5 64 12.5 {NoAI:1b}");
    console(&mut s, "give Alice bow 1");
    console(&mut s, "give Alice arrow 5");
    s.tick(2);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Move {
            position: None,
            rotation: Some(Rotation::new(0.0, 8.0)),
            on_ground: true,
        },
    );
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, 8.0),
        },
    );
    s.tick(25);
    release(&mut s);
    s.tick(8);
    s
}

#[test]
fn an_arrow_that_hurts_a_cow_completes_take_aim() {
    let s = shoot_a_cow();
    assert!(done(&s, "minecraft:adventure/shoot_arrow"));
}

#[test]
fn a_punch_on_a_cow_does_not_complete_take_aim() {
    let mut s = meadow(Vec::new());
    console(&mut s, "summon cow 9.5 64 9.5 {NoAI:1b}");
    strike_first(&mut s, Kind::Cow);
    assert!(!done(&s, "minecraft:adventure/shoot_arrow"));
}

fn lightning_near_a_villager(difficulty: &str, villager: bool) -> Scene {
    let mut s = meadow(Vec::new());
    console(&mut s, &format!("difficulty {difficulty}"));
    if villager {
        console(&mut s, "summon villager 11.5 64 9.5 {NoAI:1b}");
    }
    console(&mut s, "summon lightning_bolt 12.5 64 9.5");
    s.tick(6);
    s
}

#[test]
fn lightning_that_sets_nothing_on_fire_beside_a_villager_completes_its_advancement() {
    let s = lightning_near_a_villager("easy", true);
    assert!(done(
        &s,
        "minecraft:adventure/lightning_rod_with_villager_no_fire"
    ));
}

#[test]
fn lightning_that_starts_a_fire_does_not_complete_its_advancement() {
    let s = lightning_near_a_villager("hard", true);
    assert!(!done(
        &s,
        "minecraft:adventure/lightning_rod_with_villager_no_fire"
    ));
}

#[test]
fn lightning_without_a_villager_nearby_does_not_complete_its_advancement() {
    let s = lightning_near_a_villager("easy", false);
    assert!(!done(
        &s,
        "minecraft:adventure/lightning_rod_with_villager_no_fire"
    ));
}

const ALL_POTIONS: [&str; 17] = [
    "fire_resistance",
    "infested",
    "invisibility",
    "jump_boost",
    "night_vision",
    "oozing",
    "poison",
    "regeneration",
    "resistance",
    "slow_falling",
    "slowness",
    "speed",
    "strength",
    "water_breathing",
    "weakness",
    "weaving",
    "wind_charged",
];

#[test]
fn holding_every_potion_effect_at_once_completes_a_furious_cocktail() {
    let mut s = meadow(Vec::new());
    for effect in ALL_POTIONS {
        console(&mut s, &format!("effect give Alice {effect} 1000"));
    }
    s.tick(2);
    assert!(done(&s, "minecraft:nether/all_potions"));
}

#[test]
fn a_missing_potion_effect_leaves_a_furious_cocktail_undone() {
    let mut s = meadow(Vec::new());
    for effect in &ALL_POTIONS[1..] {
        console(&mut s, &format!("effect give Alice {effect} 1000"));
    }
    s.tick(2);
    assert!(!done(&s, "minecraft:nether/all_potions"));
}

const CHEST: BlockPos = BlockPos::new(3, 64, 3);

fn opening_a_chest(table: &str) -> Scene {
    let mut s = seeded::world(
        vec![
            Seed::block(CHEST, BlockKind::Chest)
                .carrying("LootTable", table.to_owned())
                .carrying("LootTableSeed", 7i64),
        ],
        Vec::new(),
    );
    let alice = s.players[0].clone();
    let mut view = open_block(&mut s, &alice, CHEST);
    close_window(&mut s, &alice, &mut view);
    s.tick(2);
    s
}

#[test]
fn opening_a_bastion_chest_completes_the_loot_criterion() {
    let s = opening_a_chest("minecraft:chests/bastion_treasure");
    assert_eq!(
        s.server
            .obtained_criteria(s.players[0].id, "minecraft:nether/loot_bastion"),
        vec!["loot_bastion_treasure"]
    );
}

#[test]
fn opening_another_loot_chest_leaves_the_bastion_criteria_empty() {
    let s = opening_a_chest("minecraft:chests/simple_dungeon");
    assert!(
        s.server
            .obtained_criteria(s.players[0].id, "minecraft:nether/loot_bastion")
            .is_empty()
    );
}

#[test]
fn standing_in_water_unlocks_the_boat_recipe_criterion() {
    let water = BlockKind::Water.default_state();
    let mut s = meadow(vec![
        (BlockPos::new(8, 64, 8), water),
        (BlockPos::new(8, 65, 8), water),
    ]);
    s.tick(3);
    assert_eq!(
        s.server
            .obtained_criteria(s.players[0].id, "minecraft:recipes/transportation/oak_boat"),
        vec!["in_water"]
    );
}

#[test]
fn standing_on_dry_land_leaves_the_boat_recipe_criterion_open() {
    let mut s = meadow(Vec::new());
    s.tick(3);
    assert!(
        s.server
            .obtained_criteria(s.players[0].id, "minecraft:recipes/transportation/oak_boat")
            .is_empty()
    );
}

fn levitate_up(height: f64) -> Scene {
    let mut s = meadow(Vec::new());
    console(&mut s, "effect give Alice levitation 1000 0");
    s.tick(3);
    let alice = s.players[0].clone();
    alice.link.take();
    console(&mut s, &format!("tp Alice 8.5 {} 8.5", 64.0 + height));
    s.tick(3);
    s
}

#[test]
fn rising_fifty_blocks_under_levitation_completes_great_view_from_up_here() {
    let s = levitate_up(60.0);
    assert!(done(&s, "minecraft:end/levitate"));
}

#[test]
fn a_small_rise_under_levitation_does_not_complete_the_levitation_advancement() {
    let s = levitate_up(10.0);
    assert!(!done(&s, "minecraft:end/levitate"));
}

fn lake() -> Vec<(BlockPos, BlockStateId)> {
    let mut blocks = Vec::new();
    for x in 0..16 {
        for z in 12..16 {
            for y in 62..=63 {
                blocks.push((BlockPos::new(x, y, z), BlockKind::Water.default_state()));
            }
        }
    }
    blocks
}

fn cast(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, 15.0),
        },
    );
    s.tick(1);
}

fn fishing_events(s: &Scene) -> Vec<(u64, PlayerFishState)> {
    asked_for(&s.plugin, EventKind::PlayerFish)
        .into_iter()
        .filter_map(|asked| match asked.payload {
            EventPayload::PlayerFish(event) => Some((asked.id, event.state)),
            _ => None,
        })
        .collect()
}

fn allow_until(s: &mut Scene, ticks: u32, wanted: PlayerFishState) -> bool {
    let mut ruled = std::collections::HashSet::new();
    for _ in 0..ticks {
        s.tick(1);
        for (id, state) in fishing_events(s) {
            if state == wanted {
                return true;
            }
            if ruled.insert(id) {
                rule_on(&mut s.server, 1, id, false, None);
            }
        }
    }
    false
}

fn fished(seed: u64) -> Scene {
    let server = land(60..=61, lake()).with_rng_seed(seed);
    let mut s = scene_on(server, &["Alice"], vec![deciding(1, EventKind::PlayerFish)]);
    stream_chunk_of(&mut s, BlockPos::new(9, 64, 9));
    console(&mut s, "give Alice fishing_rod");
    cast(&mut s);
    s.tick(40);
    assert!(allow_until(&mut s, 900, PlayerFishState::Bite));
    let bite = last_asked(&s.plugin, EventKind::PlayerFish);
    rule_on(&mut s.server, 1, bite.id, false, None);
    s.tick(1);
    cast(&mut s);
    let asked = last_asked(&s.plugin, EventKind::PlayerFish);
    rule_on(&mut s.server, 1, asked.id, false, None);
    s.tick(2);
    s
}

#[test]
fn reeling_in_a_cod_completes_its_fishy_business_criterion() {
    let s = fished(2);
    assert_eq!(
        s.server
            .obtained_criteria(s.players[0].id, "minecraft:husbandry/fishy_business"),
        vec!["cod"]
    );
}

#[test]
fn reeling_in_a_pufferfish_completes_its_own_criterion() {
    let s = fished(3);
    assert_eq!(
        s.server
            .obtained_criteria(s.players[0].id, "minecraft:husbandry/fishy_business"),
        vec!["pufferfish"]
    );
}

#[test]
fn reeling_in_something_else_completes_no_fish_criterion() {
    let s = fished(1);
    assert_eq!(s.server.entities_of(Kind::Item).len(), 1);
    assert!(
        s.server
            .obtained_criteria(s.players[0].id, "minecraft:husbandry/fishy_business")
            .is_empty()
    );
}

fn lava_road() -> Vec<(BlockPos, BlockStateId)> {
    let mut blocks = Vec::new();
    for x in -40..40 {
        for z in 4..13 {
            for y in 62..=63 {
                blocks.push((BlockPos::new(x, y, z), BlockKind::Lava.default_state()));
            }
        }
    }
    blocks
}

fn strider_ride(distance: f64) -> Scene {
    let mut s = scene_of(land(60..=61, lava_road()));
    console(&mut s, "gamemode creative Alice");
    console(&mut s, "give Alice warped_fungus_on_a_stick 1");
    console(
        &mut s,
        r#"summon strider -34.5 63.5 8.5 {NoAI:1b,equipment:{saddle:{id:"minecraft:saddle",count:1}}}"#,
    );
    s.tick(2);
    ride(&mut s, "Alice", "@e[type=strider]");
    let alice = s.players[0].clone();
    let mut x = -34.5;
    while x < -34.5 + distance {
        x += 1.0;
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::MoveVehicle {
                position: Vec3::new(x, 63.5, 8.5),
                rotation: Rotation::new(0.0, 0.0),
                on_ground: false,
            },
        );
        s.tick(1);
    }
    s.tick(2);
    s
}

#[test]
fn riding_a_strider_fifty_blocks_through_lava_completes_feet_dont_fail_me_now() {
    let s = strider_ride(60.0);
    assert!(done(&s, "minecraft:nether/ride_strider_in_overworld_lava"));
}

#[test]
fn riding_a_strider_a_few_blocks_through_lava_completes_nothing() {
    let s = strider_ride(10.0);
    assert!(!done(&s, "minecraft:nether/ride_strider_in_overworld_lava"));
}

fn arrow_at_alice(shield: bool) -> Scene {
    let mut s = meadow(Vec::new());
    if shield {
        console(&mut s, "give Alice shield 1");
        s.tick(2);
        let alice = s.players[0].clone();
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::UseItem {
                hand: Hand::Main,
                sequence: 1,
                rotation: Rotation::new(0.0, 0.0),
            },
        );
        s.tick(8);
    }
    console(
        &mut s,
        "summon arrow 8.5 65.0 11.5 {Motion:[0.0d,0.0d,-1.0d]}",
    );
    s.tick(6);
    s
}

#[test]
fn a_shield_that_stops_an_arrow_completes_not_today_thank_you() {
    let s = arrow_at_alice(true);
    assert!(done(&s, "minecraft:story/deflect_arrow"));
}

#[test]
fn an_arrow_that_hits_without_a_shield_does_not_complete_not_today_thank_you() {
    let s = arrow_at_alice(false);
    assert!(!done(&s, "minecraft:story/deflect_arrow"));
}

fn crossbow_arrow_kills_a_pillager(weapon: &str) -> Scene {
    let mut s = meadow(Vec::new());
    let alice = s.players[0].id.0.as_u128();
    let owner = format!(
        "[I;{},{},{},{}]",
        (alice >> 96) as u32 as i32,
        (alice >> 64) as u32 as i32,
        (alice >> 32) as u32 as i32,
        alice as u32 as i32
    );
    console(&mut s, "summon pillager 8.5 64 12.5 {NoAI:1b,Health:1f}");
    s.tick(2);
    console(
        &mut s,
        &format!(
            r#"summon arrow 8.5 64.8 10.5 {{Motion:[0.0d,0.0d,1.0d],Owner:{owner},weapon:{{id:"minecraft:{weapon}",count:1}}}}"#
        ),
    );
    s.tick(40);
    assert!(s.server.entities_of(Kind::Pillager).is_empty());
    s
}

#[test]
fn a_crossbow_arrow_that_kills_a_pillager_completes_whos_the_pillager_now() {
    let s = crossbow_arrow_kills_a_pillager("crossbow");
    assert!(done(&s, "minecraft:adventure/whos_the_pillager_now"));
}

#[test]
fn a_bow_arrow_that_kills_a_pillager_does_not_complete_whos_the_pillager_now() {
    let s = crossbow_arrow_kills_a_pillager("bow");
    assert!(!done(&s, "minecraft:adventure/whos_the_pillager_now"));
}

const SHERDS: [&str; 4] = [
    "angler_pottery_sherd",
    "archer_pottery_sherd",
    "arms_up_pottery_sherd",
    "blade_pottery_sherd",
];
const POT_ADVANCEMENT: &str = "minecraft:adventure/craft_decorated_pot_using_only_sherds";
const TRIMS: &str = "minecraft:adventure/trim_with_all_exclusive_armor_patterns";
const SILENCE: &str = "armor_trimmed_minecraft:silence_armor_trim_smithing_template_smithing_trim";

fn take_result(s: &mut Scene, alice: &Player, view: &mut View, result_slot: i16, shift: bool) {
    let input = if shift {
        ClickInput::QuickMove
    } else {
        ClickInput::Pickup
    };
    let click = predict(
        view,
        press(result_slot, 0, input),
        &[(result_slot, None)],
        None,
    );
    send_click(s, alice, view, click);
    s.tick(2);
}

fn craft_pot(sherds: &[&str], shift: bool) -> Scene {
    let mut s = scene_with(&["Alice"], Vec::new());
    let alice = s.players[0].clone();
    set_block(&mut s, STAND, "minecraft:crafting_table");
    for (index, key) in sherds.iter().enumerate() {
        give(&mut s, &alice, index as i32, key, 1);
    }
    alice.link.take();
    let mut view = open_block(&mut s, &alice, STAND);
    refresh(&alice, &mut view);
    for (index, (key, grid)) in sherds.iter().zip([2, 4, 6, 8]).enumerate() {
        let item = stack(key, 1).expect("a sherd");
        move_stack(&mut s, &alice, &mut view, 37 + index as i16, grid, item);
    }
    take_result(&mut s, &alice, &mut view, 0, shift);
    s
}

#[test]
fn crafting_a_decorated_pot_from_four_sherds_grants_the_pot_criterion() {
    let s = craft_pot(&SHERDS, false);
    assert_eq!(
        s.server.obtained_criteria(s.players[0].id, POT_ADVANCEMENT),
        vec!["pot_crafted_using_only_sherds"]
    );
}

#[test]
fn a_shift_clicked_pot_craft_grants_the_criterion() {
    let s = craft_pot(&SHERDS, true);
    assert!(done(&s, POT_ADVANCEMENT));
}

#[test]
fn a_pot_made_of_bricks_does_not_grant_the_sherd_criterion() {
    let s = craft_pot(&["brick", "brick", "brick", "brick"], false);
    assert!(
        s.server
            .obtained_criteria(s.players[0].id, POT_ADVANCEMENT)
            .is_empty()
    );
}

fn smith(template: &str, base: &str, addition: &str) -> Scene {
    let mut s = scene_with(&["Alice"], Vec::new());
    let alice = s.players[0].clone();
    set_block(&mut s, STAND, "minecraft:smithing_table");
    give(&mut s, &alice, 0, template, 1);
    give(&mut s, &alice, 1, base, 1);
    give(&mut s, &alice, 2, addition, 1);
    alice.link.take();
    let mut view = open_block(&mut s, &alice, STAND);
    refresh(&alice, &mut view);
    for (index, key) in [template, base, addition].iter().enumerate() {
        let item = stack(key, 1).expect("an input");
        move_stack(
            &mut s,
            &alice,
            &mut view,
            31 + index as i16,
            index as i16,
            item,
        );
    }
    take_result(&mut s, &alice, &mut view, 3, false);
    s
}

#[test]
fn trimming_armor_with_the_silence_template_grants_its_criterion() {
    let s = smith(
        "silence_armor_trim_smithing_template",
        "iron_helmet",
        "gold_ingot",
    );
    assert_eq!(
        s.server.obtained_criteria(s.players[0].id, TRIMS),
        vec![SILENCE]
    );
}

#[test]
fn trimming_armor_with_another_template_grants_only_its_own_criterion() {
    let s = smith(
        "wild_armor_trim_smithing_template",
        "iron_helmet",
        "gold_ingot",
    );
    assert!(
        s.server
            .obtained_criteria(s.players[0].id, TRIMS)
            .is_empty()
    );
    assert_eq!(
        s.server.obtained_criteria(
            s.players[0].id,
            "minecraft:adventure/trim_with_any_armor_pattern"
        ),
        vec!["armor_trimmed_minecraft:wild_armor_trim_smithing_template_smithing_trim"]
    );
}

#[test]
fn a_netherite_upgrade_grants_no_trim_criterion() {
    let s = smith(
        "netherite_upgrade_smithing_template",
        "diamond_sword",
        "netherite_ingot",
    );
    assert!(
        s.server
            .obtained_criteria(s.players[0].id, TRIMS)
            .is_empty()
    );
}

fn announcements_after_granting(rule: &str) -> usize {
    let mut s = meadow(Vec::new());
    console(
        &mut s,
        &format!("gamerule show_advancement_messages {rule}"),
    );
    s.players[0].link.take();
    console(
        &mut s,
        "advancement grant Alice only minecraft:story/mine_stone",
    );
    s.players[0]
        .link
        .take()
        .iter()
        .filter(|event| {
            matches!(event, rspigot_core::events::ClientEvent::SystemChat { message, .. }
                if message.plain_text().contains("chat.type.advancement"))
        })
        .count()
}

#[test]
fn the_show_advancement_messages_rule_silences_the_chat_announcement() {
    assert_eq!(announcements_after_granting("true"), 1);
    assert_eq!(announcements_after_granting("false"), 0);
}
