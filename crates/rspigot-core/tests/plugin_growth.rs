mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::plugins::events::BlockIgniteCause;
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPayload, Order, Question, WorldOrder,
};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, Vec3};

const FIRE: BlockPos = BlockPos::new(3, 64, 3);
const PLANKS: BlockPos = BlockPos::new(4, 64, 3);

struct Ground(&'static str);

impl ChunkGenerator for Ground {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let plains = rspigot_data::registries::biome_id(self.0).unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(plains));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockStateId(1),
                    );
                }
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(0.5, 64.0, 0.5)
    }

    fn place(
        &self,
        dimension: &Dimension,
        request: &rspigot_core::ports::PlaceRequest,
    ) -> Result<Vec<rspigot_core::ports::PlacedBlock>, rspigot_core::ports::PlaceFailure> {
        rspigot_worldgen::WorldGenerator::from_level_type("void", 1, "").place(dimension, request)
    }
}

fn growing_server(
    seed: u64,
    biome: &'static str,
    adjust: impl FnOnce(&mut GameConfig),
) -> Server<Link> {
    let mut config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    adjust(&mut config);
    let data = LevelData {
        spawn_mobs: false,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Ground(biome)),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn growing(seed: u64, handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
    scene_on(
        growing_server(seed, "minecraft:plains", |_| {}),
        &["Alice"],
        handlers,
    )
}

fn set(s: &mut Scene, pos: BlockPos, key: &str) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set(key.to_owned()),
    });
}

fn kind_at(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        other => panic!("expected a block, got {other:?}"),
    }
}

fn rain(s: &mut Scene) {
    s.act(Order::World(WorldOrder::SetWeather(
        rspigot_core::commands::Weather::Rain,
    )));
}

fn wait_for(s: &mut Scene, kind: EventKind, limit: u32) -> Vec<Fired> {
    for _ in 0..limit {
        let heard = settled_for(&s.plugin, kind);
        if !heard.is_empty() {
            return heard;
        }
        s.tick(1);
    }
    Vec::new()
}

fn decide_until(
    s: &mut Scene,
    kind: EventKind,
    limit: u32,
    cancel: bool,
    done: impl Fn(&Fired) -> bool,
) -> Option<Fired> {
    let mut answered: Vec<u64> = Vec::new();
    for _ in 0..limit {
        for asked in asked_for(&s.plugin, kind) {
            if answered.contains(&asked.id) {
                continue;
            }
            answered.push(asked.id);
            let wanted = done(&asked);
            rule_on(&mut s.server, 1, asked.id, cancel && wanted, None);
            if wanted {
                return Some(asked);
            }
        }
        s.tick(1);
    }
    None
}

fn burning_planks(s: &mut Scene) {
    set(s, PLANKS, "minecraft:oak_planks");
    set(s, FIRE, "minecraft:fire");
}

#[test]
fn a_fire_beside_planks_asks_block_burn_with_the_fire_as_igniter() {
    let mut s = growing(1, vec![observe(1, EventKind::BlockBurn)]);
    burning_planks(&mut s);
    let heard = wait_for(&mut s, EventKind::BlockBurn, 4_000);
    let burn = heard
        .iter()
        .find_map(|fired| match &fired.payload {
            EventPayload::BlockBurn(event) if event.block == PLANKS => Some(event.clone()),
            _ => None,
        })
        .expect("the planks burn");
    assert_eq!(burn.igniting_block, Some(FIRE));
}

#[test]
fn a_cancelled_block_burn_keeps_the_planks() {
    let mut s = growing(1, vec![deciding(1, EventKind::BlockBurn)]);
    burning_planks(&mut s);
    let asked = decide_until(
        &mut s,
        EventKind::BlockBurn,
        4_000,
        true,
        |fired| matches!(&fired.payload, EventPayload::BlockBurn(event) if event.block == PLANKS),
    );
    assert!(asked.is_some());
    s.tick(2);
    assert_eq!(kind_at(&mut s, PLANKS), "minecraft:oak_planks");
}

#[test]
fn burning_planks_without_a_handler_turn_to_fire_or_air() {
    let mut s = growing(1, Vec::new());
    burning_planks(&mut s);
    for _ in 0..4_000 {
        s.tick(1);
        if kind_at(&mut s, PLANKS) != "minecraft:oak_planks" {
            break;
        }
    }
    let after = kind_at(&mut s, PLANKS);
    assert!(
        after == "minecraft:fire" || after == "minecraft:air",
        "{after}"
    );
}

#[test]
fn a_fire_hanging_in_the_air_fades_and_a_cancel_keeps_it() {
    let hanging = BlockPos::new(8, 70, 8);
    let mut s = growing(2, vec![deciding(1, EventKind::BlockFade)]);
    set(&mut s, hanging, "minecraft:fire");
    let asked = decide_until(&mut s, EventKind::BlockFade, 200, true, |_| true)
        .expect("the fire is asked about");
    match asked.payload {
        EventPayload::BlockFade(event) => {
            assert_eq!(event.block, hanging);
            assert_eq!(event.new_state.state.kind, "minecraft:air");
        }
        other => panic!("expected a fade, got {other:?}"),
    }
    s.tick(2);
    assert_eq!(kind_at(&mut s, hanging), "minecraft:fire");
}

#[test]
fn a_fire_hanging_in_the_air_dies_without_a_handler() {
    let hanging = BlockPos::new(8, 70, 8);
    let mut s = growing(2, Vec::new());
    set(&mut s, hanging, "minecraft:fire");
    s.tick(60);
    assert_eq!(kind_at(&mut s, hanging), "minecraft:air");
}

#[test]
fn a_spreading_fire_asks_ignite_then_spread_from_the_fire() {
    let mut s = growing(
        3,
        vec![
            observe(1, EventKind::BlockIgnite),
            observe(1, EventKind::BlockSpread),
        ],
    );
    burning_planks(&mut s);
    let spread = wait_for(&mut s, EventKind::BlockSpread, 4_000);
    assert!(!spread.is_empty());
    let ignite = settled_for(&s.plugin, EventKind::BlockIgnite);
    let first = ignite
        .iter()
        .find_map(|fired| match &fired.payload {
            EventPayload::BlockIgnite(event) => Some(event.clone()),
            _ => None,
        })
        .expect("an ignite was asked");
    assert_eq!(first.cause, BlockIgniteCause::Spread);
    assert_eq!(first.igniting_block, Some(FIRE));
    match &spread[0].payload {
        EventPayload::BlockSpread(event) => {
            assert_eq!(event.source, FIRE);
            assert_eq!(event.new_state.state.kind, "minecraft:fire");
        }
        other => panic!("expected a spread, got {other:?}"),
    }
}

#[test]
fn a_fire_spread_radius_of_zero_keeps_fire_from_spreading() {
    let mut s = scene_on(
        growing_server(3, "minecraft:plains", |config| {
            config.rules.fire_spread_radius = 0
        }),
        &["Alice"],
        vec![
            observe(1, EventKind::BlockBurn),
            observe(1, EventKind::BlockSpread),
        ],
    );
    burning_planks(&mut s);
    s.tick(1_500);
    assert!(settled_for(&s.plugin, EventKind::BlockBurn).is_empty());
    assert!(settled_for(&s.plugin, EventKind::BlockSpread).is_empty());
    assert_eq!(kind_at(&mut s, PLANKS), "minecraft:oak_planks");
}

fn fades_within(s: &mut Scene, ticks: u32) -> bool {
    set(s, PLANKS, "minecraft:oak_planks");
    set(s, FIRE, "minecraft:fire");
    s.tick(ticks);
    !settled_for(&s.plugin, EventKind::BlockFade).is_empty()
}

#[test]
fn rain_puts_a_fire_out_with_a_fade() {
    let mut dry = growing(4, vec![observe(1, EventKind::BlockFade)]);
    assert!(!fades_within(&mut dry, 250));
    let mut wet = growing(4, vec![observe(1, EventKind::BlockFade)]);
    rain(&mut wet);
    assert!(fades_within(&mut wet, 250));
}

fn ticking(
    biome: &'static str,
    speed: i32,
    handlers: Vec<rspigot_core::plugins::HandlerSpec>,
) -> Scene {
    scene_on(
        growing_server(5, biome, |config| config.rules.random_tick_speed = speed),
        &["Alice"],
        handlers,
    )
}

fn set_with(s: &mut Scene, pos: BlockPos, key: &str, properties: &[(&str, &str)]) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::SetWith {
            kind: key.to_owned(),
            properties: properties
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        },
    });
}

fn property_at(s: &mut Scene, pos: BlockPos, name: &str) -> Option<String> {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state
            .properties
            .into_iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value),
        other => panic!("expected a block, got {other:?}"),
    }
}

const SOIL: BlockPos = BlockPos::new(2, 63, 2);
const SOIL_TOP: BlockPos = BlockPos::new(2, 64, 2);
const NEXT_SOIL: BlockPos = BlockPos::new(3, 63, 2);

fn fade_of(fired: &Fired) -> Option<(BlockPos, String)> {
    match &fired.payload {
        EventPayload::BlockFade(event) => Some((event.block, event.new_state.state.kind.clone())),
        _ => None,
    }
}

#[test]
fn grass_under_a_solid_block_fades_to_dirt_and_a_cancel_keeps_it() {
    let mut s = ticking(
        "minecraft:plains",
        300,
        vec![deciding(1, EventKind::BlockFade)],
    );
    set(&mut s, SOIL, "minecraft:grass_block");
    set(&mut s, SOIL_TOP, "minecraft:stone");
    let asked = decide_until(&mut s, EventKind::BlockFade, 400, true, |fired| {
        fade_of(fired).is_some_and(|(block, _)| block == SOIL)
    })
    .expect("the grass fades");
    assert_eq!(fade_of(&asked), Some((SOIL, "minecraft:dirt".to_owned())));
    s.tick(2);
    assert_eq!(kind_at(&mut s, SOIL), "minecraft:grass_block");
}

#[test]
fn grass_under_a_solid_block_turns_to_dirt_without_a_handler() {
    let mut s = ticking("minecraft:plains", 300, Vec::new());
    set(&mut s, SOIL, "minecraft:grass_block");
    set(&mut s, SOIL_TOP, "minecraft:stone");
    for _ in 0..400 {
        s.tick(1);
        if kind_at(&mut s, SOIL) == "minecraft:dirt" {
            return;
        }
    }
    panic!("the grass never died");
}

#[test]
fn grass_in_the_light_spreads_to_bare_dirt() {
    let mut s = ticking("minecraft:plains", 300, Vec::new());
    set(&mut s, SOIL, "minecraft:grass_block");
    set(&mut s, NEXT_SOIL, "minecraft:dirt");
    for _ in 0..600 {
        s.tick(1);
        if kind_at(&mut s, NEXT_SOIL) == "minecraft:grass_block" {
            return;
        }
    }
    panic!("the grass never spread");
}

#[test]
fn a_snow_layer_beside_a_torch_melts_with_a_fade() {
    let mut s = ticking(
        "minecraft:plains",
        300,
        vec![deciding(1, EventKind::BlockFade)],
    );
    set(&mut s, SOIL_TOP, "minecraft:snow");
    set(&mut s, BlockPos::new(3, 64, 2), "minecraft:torch");
    let asked = decide_until(&mut s, EventKind::BlockFade, 400, true, |fired| {
        fade_of(fired).is_some_and(|(block, _)| block == SOIL_TOP)
    })
    .expect("the snow fades");
    assert_eq!(
        fade_of(&asked),
        Some((SOIL_TOP, "minecraft:air".to_owned()))
    );
    s.tick(2);
    assert_eq!(kind_at(&mut s, SOIL_TOP), "minecraft:snow");
}

#[test]
fn ice_beside_a_torch_melts_into_water_with_a_fade() {
    let mut s = ticking(
        "minecraft:plains",
        300,
        vec![deciding(1, EventKind::BlockFade)],
    );
    set(&mut s, SOIL_TOP, "minecraft:ice");
    set(&mut s, BlockPos::new(3, 64, 2), "minecraft:torch");
    let asked = decide_until(&mut s, EventKind::BlockFade, 400, false, |fired| {
        fade_of(fired).is_some_and(|(block, _)| block == SOIL_TOP)
    })
    .expect("the ice fades");
    assert_eq!(
        fade_of(&asked),
        Some((SOIL_TOP, "minecraft:water".to_owned()))
    );
    s.tick(2);
    assert_eq!(kind_at(&mut s, SOIL_TOP), "minecraft:water");
}

#[test]
fn dry_farmland_asks_a_moisture_change_and_a_cancel_keeps_the_moisture() {
    let mut s = ticking(
        "minecraft:plains",
        300,
        vec![deciding(1, EventKind::MoistureChange)],
    );
    set_with(&mut s, SOIL, "minecraft:farmland", &[("moisture", "7")]);
    let mut answered: Vec<u64> = Vec::new();
    let mut seen = None;
    for _ in 0..400 {
        for asked in asked_for(&s.plugin, EventKind::MoistureChange) {
            if answered.contains(&asked.id) {
                continue;
            }
            answered.push(asked.id);
            if let EventPayload::MoistureChange(event) = &asked.payload
                && event.block == SOIL
            {
                seen = Some(event.new_state.state.properties.clone());
                rule_on(&mut s.server, 1, asked.id, true, None);
            } else {
                rule_on(&mut s.server, 1, asked.id, false, None);
            }
        }
        if seen.is_some() {
            break;
        }
        s.tick(1);
    }
    let properties = seen.expect("the farmland dries");
    assert!(properties.contains(&("moisture".to_owned(), "6".to_owned())));
    s.tick(2);
    assert_eq!(property_at(&mut s, SOIL, "moisture").as_deref(), Some("7"));
}

#[test]
fn farmland_without_water_dries_out_and_turns_to_dirt() {
    let mut s = ticking("minecraft:plains", 300, Vec::new());
    set_with(&mut s, SOIL, "minecraft:farmland", &[("moisture", "1")]);
    for _ in 0..1_500 {
        s.tick(1);
        if kind_at(&mut s, SOIL) == "minecraft:dirt" {
            return;
        }
    }
    panic!("the farmland never dried out");
}

#[test]
fn leaves_far_from_a_log_decay_and_a_cancel_keeps_them() {
    let mut s = ticking(
        "minecraft:plains",
        300,
        vec![deciding(1, EventKind::LeavesDecay)],
    );
    set_with(
        &mut s,
        SOIL_TOP,
        "minecraft:oak_leaves",
        &[("distance", "7"), ("persistent", "false")],
    );
    let asked = decide_until(
        &mut s,
        EventKind::LeavesDecay,
        400,
        true,
        |fired| matches!(&fired.payload, EventPayload::LeavesDecay(event) if event.block == SOIL_TOP),
    );
    assert!(asked.is_some());
    s.tick(2);
    assert_eq!(kind_at(&mut s, SOIL_TOP), "minecraft:oak_leaves");
}

#[test]
fn persistent_leaves_never_decay() {
    let mut s = ticking(
        "minecraft:plains",
        300,
        vec![observe(1, EventKind::LeavesDecay)],
    );
    set_with(
        &mut s,
        SOIL_TOP,
        "minecraft:oak_leaves",
        &[("distance", "7"), ("persistent", "true")],
    );
    s.tick(100);
    assert!(settled_for(&s.plugin, EventKind::LeavesDecay).is_empty());
}

#[test]
fn leaves_next_to_a_log_settle_their_distance_and_do_not_decay() {
    let mut s = ticking(
        "minecraft:plains",
        300,
        vec![observe(1, EventKind::LeavesDecay)],
    );
    set_with(
        &mut s,
        SOIL_TOP,
        "minecraft:oak_leaves",
        &[("distance", "7"), ("persistent", "false")],
    );
    set(&mut s, BlockPos::new(3, 64, 2), "minecraft:oak_log");
    s.tick(100);
    assert_eq!(
        property_at(&mut s, SOIL_TOP, "distance").as_deref(),
        Some("1")
    );
    assert!(settled_for(&s.plugin, EventKind::LeavesDecay).is_empty());
}

fn form_of(fired: &Fired) -> Option<(BlockPos, String)> {
    match &fired.payload {
        EventPayload::BlockForm(event) => Some((event.block, event.new_state.state.kind.clone())),
        _ => None,
    }
}

#[test]
fn rain_on_a_cold_biome_forms_snow_and_a_cancel_keeps_the_air() {
    let mut s = ticking(
        "minecraft:snowy_plains",
        100,
        vec![deciding(1, EventKind::BlockForm)],
    );
    rain(&mut s);
    let asked = decide_until(&mut s, EventKind::BlockForm, 600, true, |_| true)
        .expect("snow is about to form");
    let (block, kind) = form_of(&asked).unwrap();
    assert_eq!(kind, "minecraft:snow");
    s.tick(2);
    assert_eq!(kind_at(&mut s, block), "minecraft:air");
}

#[test]
fn rain_on_a_cold_biome_lays_snow_without_a_handler() {
    let mut s = ticking("minecraft:snowy_plains", 100, Vec::new());
    rain(&mut s);
    for _ in 0..600 {
        s.tick(1);
        if kind_at(&mut s, BlockPos::new(2, 64, 2)) == "minecraft:snow" {
            return;
        }
        for x in 0..16 {
            for z in 0..16 {
                if kind_at(&mut s, BlockPos::new(x, 64, z)) == "minecraft:snow" {
                    return;
                }
            }
        }
    }
    panic!("no snow fell");
}

#[test]
fn a_water_source_in_the_cold_freezes_into_ice_with_a_form() {
    let mut s = ticking(
        "minecraft:snowy_plains",
        100,
        vec![observe(1, EventKind::BlockForm)],
    );
    for x in 0..16 {
        for z in 0..16 {
            set(&mut s, BlockPos::new(x, 63, z), "minecraft:water");
        }
    }
    for _ in 0..600 {
        s.tick(1);
        let heard = settled_for(&s.plugin, EventKind::BlockForm);
        if heard
            .iter()
            .any(|fired| form_of(fired).is_some_and(|(_, kind)| kind == "minecraft:ice"))
        {
            return;
        }
    }
    panic!("the water never froze");
}

#[test]
fn concrete_powder_beside_water_hardens_with_a_form() {
    let mut s = growing(6, vec![deciding(1, EventKind::BlockForm)]);
    set(
        &mut s,
        BlockPos::new(2, 64, 2),
        "minecraft:white_concrete_powder",
    );
    set(&mut s, BlockPos::new(3, 64, 2), "minecraft:water");
    let asked = last_asked(&s.plugin, EventKind::BlockForm);
    assert_eq!(
        form_of(&asked),
        Some((
            BlockPos::new(2, 64, 2),
            "minecraft:white_concrete".to_owned()
        ))
    );
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(5);
    assert_eq!(
        kind_at(&mut s, BlockPos::new(2, 64, 2)),
        "minecraft:white_concrete_powder"
    );
}

fn give_bone_meal(s: &mut Scene) {
    s.server.handle(rspigot_core::events::Inbound::Console(
        "give Alice minecraft:bone_meal 16".to_owned(),
    ));
    s.server.deliver();
}

fn bone_meal(s: &mut Scene, pos: BlockPos) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        rspigot_core::events::PlayerInput::UseItemOn {
            hand: rspigot_core::events::Hand::Main,
            pos,
            face: rspigot_core::events::Direction::Up,
            cursor: rspigot_domain::Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 3,
        },
    );
    s.tick(1);
}

fn structure_of(fired: &Fired) -> Option<rspigot_core::plugins::events::StructureGrow> {
    match &fired.payload {
        EventPayload::StructureGrow(event) => Some(event.clone()),
        _ => None,
    }
}

#[test]
fn a_ripe_sapling_grows_a_tree_with_structure_grow_and_a_cancel_keeps_it() {
    let mut s = ticking(
        "minecraft:plains",
        1_500,
        vec![deciding(1, EventKind::StructureGrow)],
    );
    set_with(&mut s, SOIL_TOP, "minecraft:oak_sapling", &[("stage", "1")]);
    let asked = decide_until(&mut s, EventKind::StructureGrow, 3_000, true, |_| true)
        .expect("the sapling grows");
    let event = structure_of(&asked).unwrap();
    assert!(!event.from_bonemeal);
    assert_eq!(event.player, None);
    assert_eq!(event.species, rspigot_core::plugins::events::TreeType::Tree);
    assert!(
        event
            .blocks
            .iter()
            .any(|block| block.pos == SOIL_TOP && block.state.kind == "minecraft:oak_log")
    );
    assert!(
        event
            .blocks
            .iter()
            .any(|block| block.state.kind == "minecraft:oak_leaves")
    );
    s.tick(2);
    assert_eq!(kind_at(&mut s, SOIL_TOP), "minecraft:oak_sapling");
}

#[test]
fn a_ripe_sapling_grows_a_tree_without_a_handler() {
    let mut s = ticking("minecraft:plains", 1_500, Vec::new());
    set_with(&mut s, SOIL_TOP, "minecraft:oak_sapling", &[("stage", "1")]);
    for _ in 0..3_000 {
        s.tick(1);
        if kind_at(&mut s, SOIL_TOP) == "minecraft:oak_log" {
            assert_eq!(
                kind_at(&mut s, BlockPos::new(2, 67, 2)),
                "minecraft:oak_log"
            );
            return;
        }
    }
    panic!("the sapling never grew");
}

fn fertilize_of(fired: &Fired) -> Option<rspigot_core::plugins::events::BlockFertilize> {
    match &fired.payload {
        EventPayload::BlockFertilize(event) => Some(event.clone()),
        _ => None,
    }
}

#[test]
fn bone_meal_on_a_young_sapling_asks_a_fertilize_with_the_next_stage() {
    let mut s = growing(7, vec![deciding(1, EventKind::BlockFertilize)]);
    set_with(&mut s, SOIL_TOP, "minecraft:oak_sapling", &[("stage", "0")]);
    give_bone_meal(&mut s);
    let mut asked = None;
    for _ in 0..30 {
        bone_meal(&mut s, SOIL_TOP);
        if let Some(fired) = asked_for(&s.plugin, EventKind::BlockFertilize).pop() {
            asked = Some(fired);
            break;
        }
    }
    let asked = asked.expect("a fertilize was asked");
    let event = fertilize_of(&asked).unwrap();
    assert_eq!(event.block, SOIL_TOP);
    assert!(event.player.is_some());
    assert_eq!(event.blocks.len(), 1);
    assert_eq!(event.blocks[0].state.kind, "minecraft:oak_sapling");
    assert!(
        event.blocks[0]
            .state
            .properties
            .contains(&("stage".to_owned(), "1".to_owned()))
    );
    assert_eq!(property_at(&mut s, SOIL_TOP, "stage").as_deref(), Some("0"));
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(property_at(&mut s, SOIL_TOP, "stage").as_deref(), Some("0"));
}

#[test]
fn bone_meal_on_a_young_sapling_advances_its_stage_without_a_handler() {
    let mut s = growing(7, Vec::new());
    set_with(&mut s, SOIL_TOP, "minecraft:oak_sapling", &[("stage", "0")]);
    give_bone_meal(&mut s);
    for _ in 0..30 {
        bone_meal(&mut s, SOIL_TOP);
        if property_at(&mut s, SOIL_TOP, "stage").as_deref() == Some("1") {
            return;
        }
    }
    panic!("the sapling never advanced");
}

#[test]
fn bone_meal_on_a_sapling_already_growing_starts_no_second_tree() {
    let mut s = growing(8, vec![deciding(1, EventKind::StructureGrow)]);
    set_with(&mut s, SOIL_TOP, "minecraft:oak_sapling", &[("stage", "1")]);
    give_bone_meal(&mut s);
    for _ in 0..20 {
        bone_meal(&mut s, SOIL_TOP);
    }
    let mut asked = None;
    for _ in 0..100 {
        s.tick(1);
        if let Some(fired) = asked_for(&s.plugin, EventKind::StructureGrow).pop() {
            asked = Some(fired);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let asked = asked.expect("a structure grow was asked");
    rule_on(&mut s.server, 1, asked.id, true, None);
    for _ in 0..40 {
        s.tick(1);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(asked_for(&s.plugin, EventKind::StructureGrow).len(), 1);
    assert_eq!(kind_at(&mut s, SOIL_TOP), "minecraft:oak_sapling");
}

#[test]
fn a_cancelled_structure_grow_of_a_bone_meal_still_reports_a_refused_fertilize() {
    let mut s = growing(
        8,
        vec![
            deciding(1, EventKind::StructureGrow),
            observe(1, EventKind::BlockFertilize),
        ],
    );
    set_with(&mut s, SOIL_TOP, "minecraft:oak_sapling", &[("stage", "1")]);
    give_bone_meal(&mut s);
    let mut asked = None;
    for _ in 0..60 {
        bone_meal(&mut s, SOIL_TOP);
        std::thread::sleep(std::time::Duration::from_millis(10));
        s.tick(1);
        if let Some(fired) = asked_for(&s.plugin, EventKind::StructureGrow).pop() {
            asked = Some(fired);
            break;
        }
    }
    let asked = asked.expect("a structure grow was asked");
    let event = structure_of(&asked).unwrap();
    assert!(event.from_bonemeal);
    assert!(event.player.is_some());
    rule_on(&mut s.server, 1, asked.id, true, None);
    let mut heard = Vec::new();
    for _ in 0..60 {
        s.tick(1);
        heard = settled_for(&s.plugin, EventKind::BlockFertilize);
        if !heard.is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(heard.len(), 1);
    assert!(heard[0].cancelled);
    assert_eq!(kind_at(&mut s, SOIL_TOP), "minecraft:oak_sapling");
}

#[test]
fn bone_meal_on_a_sapling_grows_a_tree_through_structure_grow_then_fertilize() {
    let mut s = growing(
        9,
        vec![
            observe(1, EventKind::StructureGrow),
            observe(1, EventKind::BlockFertilize),
        ],
    );
    set_with(&mut s, SOIL_TOP, "minecraft:oak_sapling", &[("stage", "1")]);
    give_bone_meal(&mut s);
    for _ in 0..60 {
        bone_meal(&mut s, SOIL_TOP);
        std::thread::sleep(std::time::Duration::from_millis(10));
        s.tick(1);
        if kind_at(&mut s, SOIL_TOP) == "minecraft:oak_log" {
            break;
        }
    }
    assert_eq!(kind_at(&mut s, SOIL_TOP), "minecraft:oak_log");
    let structures = settled_for(&s.plugin, EventKind::StructureGrow);
    let fertilized = settled_for(&s.plugin, EventKind::BlockFertilize);
    assert_eq!(structures.len(), 1);
    assert_eq!(fertilized.len(), 1);
    assert!(!fertilized[0].cancelled);
    assert!(structure_of(&structures[0]).unwrap().from_bonemeal);
}

#[test]
fn bone_meal_on_grass_asks_a_fertilize_listing_the_new_plants() {
    let mut s = growing(10, vec![deciding(1, EventKind::BlockFertilize)]);
    set(&mut s, SOIL, "minecraft:grass_block");
    give_bone_meal(&mut s);
    bone_meal(&mut s, SOIL);
    let asked = last_asked(&s.plugin, EventKind::BlockFertilize);
    let event = fertilize_of(&asked).unwrap();
    assert_eq!(event.block, SOIL);
    assert!(!event.blocks.is_empty());
    assert!(
        event
            .blocks
            .iter()
            .all(|block| block.state.kind == "minecraft:short_grass"
                || block.state.kind == "minecraft:tall_grass")
    );
    let spot = event.blocks[0].pos;
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(kind_at(&mut s, spot), "minecraft:air");
}

#[test]
fn bone_meal_on_grass_grows_short_grass_without_a_handler() {
    let mut s = growing(10, Vec::new());
    set(&mut s, SOIL, "minecraft:grass_block");
    give_bone_meal(&mut s);
    bone_meal(&mut s, SOIL);
    assert_eq!(kind_at(&mut s, SOIL_TOP), "minecraft:short_grass");
}
