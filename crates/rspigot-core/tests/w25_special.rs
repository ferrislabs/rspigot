#[path = "blocks_common/mod.rs"]
mod blocks_common;
mod common;

use std::sync::Arc;
use std::time::Duration;

use blocks_common::*;
use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Direction, Hand, PlayerInput};
use rspigot_core::plugins::{Answer, Question};
use rspigot_core::ports::{ChunkGenerator, PlaceFailure, PlaceRequest, PlacedBlock};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, Vec3};

const SPOT: BlockPos = BlockPos::new(8, 64, 10);

fn small_scene(cells: Cells, seed: u64, speed: i32) -> Scene {
    small_scene_with(cells, seed, speed)
}

fn small_scene_with<G: ChunkGenerator + 'static>(generator: G, seed: u64, speed: i32) -> Scene {
    let mut config = GameConfig {
        view_distance: 2,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    config.rules.random_tick_speed = speed;
    let data = LevelData {
        spawn_mobs: false,
        day_time: 18_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server: Server<Link> = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(generator),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    scene_on(server, &["Alice"], Vec::new())
}

fn rig(cells: Cells, speed: i32) -> Scene {
    let mut s = small_scene(cells, 21, speed);
    stream_chunk_of(&mut s, SPOT);
    s.tick(2);
    s
}

fn sounds(player: &Player) -> Vec<&'static str> {
    player
        .link
        .take()
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } => Some(*sound),
            _ => None,
        })
        .collect()
}

fn survival(s: &mut Scene) -> Player {
    let alice = s.players[0].clone();
    console(s, "gamemode survival Alice");
    alice.link.take();
    alice
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

#[test]
fn a_closed_eyeblossom_opens_at_night_with_the_long_open_sound() {
    let mut s = rig(Cells::default().block(SPOT, BlockKind::ClosedEyeblossom), 0);
    let alice = s.players[0].clone();
    alice.link.take();
    console(&mut s, "gamerule random_tick_speed 4096");
    s.tick(4);
    assert_eq!(kind_at(&s, SPOT), Some(BlockKind::OpenEyeblossom));
    assert!(sounds(&alice).contains(&"minecraft:block.eyeblossom.open_long"));
}

#[test]
fn an_open_eyeblossom_closes_in_daylight() {
    let mut s = rig(
        Cells::default().block(SPOT, BlockKind::OpenEyeblossom),
        4096,
    );
    console(&mut s, "time set day");
    let alice = s.players[0].clone();
    alice.link.take();
    s.tick(4);
    assert_eq!(kind_at(&s, SPOT), Some(BlockKind::ClosedEyeblossom));
    assert!(sounds(&alice).contains(&"minecraft:block.eyeblossom.close_long"));
}

#[test]
fn an_open_eyeblossom_stays_open_at_night() {
    let mut s = rig(
        Cells::default().block(SPOT, BlockKind::OpenEyeblossom),
        4096,
    );
    s.tick(6);
    assert_eq!(kind_at(&s, SPOT), Some(BlockKind::OpenEyeblossom));
}

#[test]
fn a_neighbouring_eyeblossom_follows_a_little_later() {
    let next = SPOT.offset(2, 0, 0);
    let cells = Cells::default()
        .block(SPOT, BlockKind::ClosedEyeblossom)
        .block(next, BlockKind::ClosedEyeblossom);
    let mut s = rig(cells, 0);
    console(&mut s, "gamerule random_tick_speed 4096");
    s.tick(60);
    assert_eq!(kind_at(&s, SPOT), Some(BlockKind::OpenEyeblossom));
    assert_eq!(kind_at(&s, next), Some(BlockKind::OpenEyeblossom));
}

#[test]
fn a_potted_closed_eyeblossom_opens_at_night() {
    let mut s = rig(
        Cells::default().block(SPOT, BlockKind::PottedClosedEyeblossom),
        4096,
    );
    s.tick(4);
    assert_eq!(kind_at(&s, SPOT), Some(BlockKind::PottedOpenEyeblossom));
}

fn cactus_field() -> Cells {
    let mut cells = Cells::default();
    for z in [0, 2, 4, 12, 14] {
        for x in (0..=14).step_by(2) {
            cells = cells.block(BlockPos::new(x, 63, z), BlockKind::Sand).state(
                BlockPos::new(x, 64, z),
                BlockKind::Cactus,
                &[("age", "8")],
            );
        }
    }
    cells
}

#[test]
fn a_cactus_at_age_eight_sometimes_flowers() {
    let mut s = rig(cactus_field(), 4096);
    s.tick(30);
    let mut flowers = 0;
    for z in [0, 2, 4, 12, 14] {
        for x in (0..=14).step_by(2) {
            let above = BlockPos::new(x, 65, z);
            if kind_at(&s, above) == Some(BlockKind::CactusFlower) {
                flowers += 1;
                assert_eq!(
                    kind_at(&s, BlockPos::new(x, 64, z)),
                    Some(BlockKind::Cactus)
                );
            }
        }
    }
    assert!(flowers > 0, "no cactus ever flowered");
}

#[test]
fn a_cactus_with_a_solid_block_beside_it_breaks() {
    let cells = Cells::default()
        .block(SPOT.offset(0, -1, 0), BlockKind::Sand)
        .block(SPOT, BlockKind::Cactus);
    let mut s = rig(cells, 0);
    console(&mut s, "setblock 9 64 10 minecraft:stone");
    s.tick(4);
    assert_eq!(kind_at(&s, SPOT), Some(BlockKind::Air));
}

fn portal_room() -> Cells {
    let mut cells = Cells::default();
    for x in 4..=11 {
        for z in 4..=7 {
            for y in 64..=70 {
                cells = cells.state(
                    BlockPos::new(x, y, z),
                    BlockKind::NetherPortal,
                    &[("axis", "x")],
                );
            }
        }
    }
    cells
}

#[test]
fn a_nether_portal_block_sometimes_spawns_a_zombified_piglin_on_hard() {
    let mut s = rig(portal_room(), 0);
    console(&mut s, "difficulty hard");
    console(&mut s, "gamerule random_tick_speed 4096");
    s.tick(25);
    assert!(count_of(&mut s, "minecraft:zombified_piglin") > 0);
}

#[test]
fn a_nether_portal_block_spawns_nothing_on_peaceful() {
    let mut s = rig(portal_room(), 0);
    console(&mut s, "difficulty peaceful");
    console(&mut s, "gamerule random_tick_speed 4096");
    s.tick(25);
    assert_eq!(count_of(&mut s, "minecraft:zombified_piglin"), 0);
}

fn charge_beside(s: &mut Scene, target: BlockPos) {
    console(
        s,
        &format!(
            "summon minecraft:wind_charge {}.5 67.0 {}.5 {{Motion:[0.0d,-1.0d,0.0d]}}",
            target.x + 1,
            target.z
        ),
    );
    s.tick(6);
}

fn triggered(kind: BlockKind, props: &[(&str, &str)], watch: &str) -> (Scene, Option<String>) {
    let cells = Cells::default().state(SPOT, kind, props);
    let mut s = rig(cells, 0);
    charge_beside(&mut s, SPOT);
    let seen = property_at(&s, SPOT, watch);
    (s, seen)
}

#[test]
fn a_wind_charge_pulls_a_lever() {
    let (_, seen) = triggered(
        BlockKind::Lever,
        &[("face", "floor"), ("facing", "north"), ("powered", "false")],
        "powered",
    );
    assert_eq!(seen.as_deref(), Some("true"));
}

#[test]
fn a_wind_charge_presses_a_button() {
    let (_, seen) = triggered(
        BlockKind::StoneButton,
        &[("face", "floor"), ("facing", "north"), ("powered", "false")],
        "powered",
    );
    assert_eq!(seen.as_deref(), Some("true"));
}

#[test]
fn a_wind_charge_opens_a_wooden_trapdoor_and_a_fence_gate() {
    let (_, seen) = triggered(
        BlockKind::OakTrapdoor,
        &[("half", "bottom"), ("facing", "north"), ("open", "false")],
        "open",
    );
    assert_eq!(seen.as_deref(), Some("true"));
    let (_, seen) = triggered(
        BlockKind::OakFenceGate,
        &[("facing", "north"), ("open", "false")],
        "open",
    );
    assert_eq!(seen.as_deref(), Some("true"));
}

#[test]
fn a_wind_charge_leaves_an_iron_trapdoor_shut() {
    let (_, seen) = triggered(
        BlockKind::IronTrapdoor,
        &[("half", "bottom"), ("facing", "north"), ("open", "false")],
        "open",
    );
    assert_eq!(seen.as_deref(), Some("false"));
}

#[test]
fn a_wind_charge_opens_a_wooden_door_by_its_lower_half() {
    let cells = Cells::default()
        .state(
            SPOT,
            BlockKind::OakDoor,
            &[("half", "lower"), ("facing", "north"), ("open", "false")],
        )
        .state(
            SPOT.offset(0, 1, 0),
            BlockKind::OakDoor,
            &[("half", "upper"), ("facing", "north"), ("open", "false")],
        );
    let mut s = rig(cells, 0);
    charge_beside(&mut s, SPOT);
    assert_eq!(property_at(&s, SPOT, "open").as_deref(), Some("true"));
    assert_eq!(
        property_at(&s, SPOT.offset(0, 1, 0), "open").as_deref(),
        Some("true")
    );
}

#[test]
fn a_wind_charge_blows_out_a_lit_candle() {
    let (_, seen) = triggered(
        BlockKind::Candle,
        &[("candles", "1"), ("lit", "true")],
        "lit",
    );
    assert_eq!(seen.as_deref(), Some("false"));
}

#[test]
fn a_wind_charge_rings_a_bell() {
    let cells = Cells::default().state(
        SPOT,
        BlockKind::Bell,
        &[("attachment", "floor"), ("facing", "north")],
    );
    let mut s = rig(cells, 0);
    let alice = s.players[0].clone();
    alice.link.take();
    charge_beside(&mut s, SPOT);
    assert!(sounds(&alice).contains(&"minecraft:block.bell.use"));
}

fn arrow_at(s: &mut Scene, from: Vec3, motion: &str) {
    console(
        s,
        &format!(
            "summon minecraft:arrow {} {} {} {{Motion:{motion}}}",
            from.x, from.y, from.z
        ),
    );
    s.tick(12);
}

#[test]
fn an_arrow_hitting_a_bell_on_its_axis_rings_it() {
    let cells = Cells::default().state(
        SPOT,
        BlockKind::Bell,
        &[("attachment", "floor"), ("facing", "east")],
    );
    let mut s = rig(cells, 0);
    let alice = s.players[0].clone();
    alice.link.take();
    arrow_at(&mut s, Vec3::new(4.5, 64.5, 10.5), "[1.0d,0.0d,0.0d]");
    assert!(sounds(&alice).contains(&"minecraft:block.bell.use"));
}

#[test]
fn an_arrow_hitting_a_big_dripleaf_tips_it_fully() {
    let cells = Cells::default().state(
        SPOT,
        BlockKind::BigDripleaf,
        &[("facing", "north"), ("tilt", "none")],
    );
    let mut s = rig(cells, 0);
    arrow_at(&mut s, Vec3::new(8.5, 67.0, 10.5), "[0.0d,-1.0d,0.0d]");
    assert_eq!(property_at(&s, SPOT, "tilt").as_deref(), Some("full"));
}

#[test]
fn an_arrow_hitting_amethyst_chimes() {
    let cells = Cells::default().block(SPOT, BlockKind::AmethystBlock);
    let mut s = rig(cells, 0);
    let alice = s.players[0].clone();
    alice.link.take();
    arrow_at(&mut s, Vec3::new(8.5, 67.0, 10.5), "[0.0d,-1.0d,0.0d]");
    let heard = sounds(&alice);
    assert!(heard.contains(&"minecraft:block.amethyst_block.chime"));
    assert!(heard.contains(&"minecraft:block.amethyst_block.hit"));
}

#[test]
fn a_fast_trident_breaks_a_pointed_dripstone_but_an_arrow_does_not() {
    let stalagmite = [
        ("vertical_direction", "up"),
        ("thickness", "tip"),
        ("waterlogged", "false"),
    ];
    let cells = Cells::default().state(SPOT, BlockKind::PointedDripstone, &stalagmite);
    let mut s = rig(cells, 0);
    arrow_at(&mut s, Vec3::new(8.5, 67.0, 10.5), "[0.0d,-1.5d,0.0d]");
    assert_eq!(kind_at(&s, SPOT), Some(BlockKind::PointedDripstone));
    console(
        &mut s,
        "summon minecraft:trident 8.5 67.0 10.5 {Motion:[0.0d,-1.5d,0.0d]}",
    );
    s.tick(12);
    assert_eq!(kind_at(&s, SPOT), Some(BlockKind::Air));
}

#[test]
fn a_snow_layer_over_grass_makes_it_snowy_and_removing_it_clears_it() {
    let cells = Cells::default().block(SPOT, BlockKind::GrassBlock);
    let mut s = rig(cells, 0);
    assert_eq!(property_at(&s, SPOT, "snowy").as_deref(), Some("false"));
    console(&mut s, "setblock 8 65 10 minecraft:snow");
    s.tick(2);
    assert_eq!(property_at(&s, SPOT, "snowy").as_deref(), Some("true"));
    console(&mut s, "setblock 8 65 10 minecraft:air");
    s.tick(2);
    assert_eq!(property_at(&s, SPOT, "snowy").as_deref(), Some("false"));
}

fn sides(s: &Scene) -> [String; 4] {
    ["north", "east", "south", "west"].map(|side| property_at(s, SPOT, side).unwrap_or_default())
}

fn use_wire(s: &mut Scene, player: &Player, sequence: i32) {
    send_input(
        &mut s.server,
        player,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: SPOT,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 0.1, 0.5),
            inside_block: false,
            sequence,
        },
    );
    s.tick(2);
}

#[test]
fn right_clicking_a_lone_redstone_wire_toggles_cross_and_dot() {
    let mut s = rig(Cells::default(), 0);
    let alice = survival(&mut s);
    console(&mut s, "setblock 8 64 10 minecraft:redstone_wire");
    s.tick(1);
    assert_eq!(
        sides(&s),
        ["none", "none", "none", "none"].map(String::from)
    );
    use_wire(&mut s, &alice, 1);
    assert_eq!(
        sides(&s),
        ["side", "side", "side", "side"].map(String::from)
    );
    use_wire(&mut s, &alice, 2);
    assert_eq!(
        sides(&s),
        ["none", "none", "none", "none"].map(String::from)
    );
}

const STRUCTURE: BlockPos = BlockPos::new(8, 64, 6);

fn structure_data(mode: &str, name: &str) -> rspigot_nbt::Compound {
    rspigot_nbt::Compound::new()
        .with("name", name)
        .with("mode", mode)
        .with("posX", 0)
        .with("posY", 1)
        .with("posZ", 0)
        .with("sizeX", 0)
        .with("sizeY", 0)
        .with("sizeZ", 0)
        .with("rotation", "NONE")
        .with("mirror", "NONE")
        .with("integrity", 1.0_f32)
        .with("seed", 0_i64)
        .with("metadata", "")
}

struct Hut(Cells);

impl ChunkGenerator for Hut {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        self.0.generate(dimension, pos)
    }

    fn spawn_point(&self, dimension: &Dimension) -> Vec3 {
        self.0.spawn_point(dimension)
    }

    fn place(
        &self,
        _: &Dimension,
        request: &PlaceRequest,
    ) -> Result<Vec<PlacedBlock>, PlaceFailure> {
        match request {
            PlaceRequest::Template { id, pos, .. } if id == "t:hut" => Ok(vec![
                PlacedBlock {
                    pos: *pos,
                    state: BlockStateId(1),
                },
                PlacedBlock {
                    pos: BlockPos::new(pos.x + 1, pos.y, pos.z),
                    state: BlockStateId(1),
                },
            ]),
            _ => Err(PlaceFailure::Unknown),
        }
    }
}

fn structure_rig() -> Scene {
    let cells = Cells::default()
        .state(STRUCTURE, BlockKind::StructureBlock, &[("mode", "load")])
        .entity(
            STRUCTURE,
            "structure_block",
            structure_data("LOAD", "t:hut"),
        );
    let mut s = small_scene_with(Hut(cells), 21, 0);
    stream_chunk_of(&mut s, SPOT);
    s.tick(2);
    s
}

fn tick_until_placed(s: &mut Scene, pos: BlockPos) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while kind_at(s, pos) == Some(BlockKind::Air) && std::time::Instant::now() < deadline {
        s.tick(1);
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_powered_structure_block_in_load_mode_places_its_template_at_its_offset() {
    let mut s = structure_rig();
    assert_eq!(kind_at(&s, STRUCTURE.offset(0, 1, 0)), Some(BlockKind::Air));
    console(&mut s, "setblock 8 64 5 minecraft:redstone_block");
    tick_until_placed(&mut s, STRUCTURE.offset(0, 1, 0));
    assert_eq!(
        kind_at(&s, STRUCTURE.offset(0, 1, 0)),
        Some(BlockKind::Stone)
    );
    assert_eq!(
        kind_at(&s, STRUCTURE.offset(1, 1, 0)),
        Some(BlockKind::Stone)
    );
}

#[test]
fn an_unpowered_structure_block_in_load_mode_places_nothing() {
    let mut s = structure_rig();
    s.tick(10);
    assert_eq!(kind_at(&s, STRUCTURE.offset(0, 1, 0)), Some(BlockKind::Air));
}

#[test]
fn a_structure_block_in_save_mode_does_not_load_when_powered() {
    let cells = Cells::default()
        .state(STRUCTURE, BlockKind::StructureBlock, &[("mode", "save")])
        .entity(
            STRUCTURE,
            "structure_block",
            structure_data("SAVE", "t:hut"),
        );
    let mut s = small_scene_with(Hut(cells), 21, 0);
    stream_chunk_of(&mut s, SPOT);
    s.tick(2);
    console(&mut s, "setblock 8 64 5 minecraft:redstone_block");
    s.tick(10);
    assert_eq!(kind_at(&s, STRUCTURE.offset(0, 1, 0)), Some(BlockKind::Air));
}

const HOOK_A: BlockPos = BlockPos::new(5, 64, 10);
const WIRE: BlockPos = BlockPos::new(7, 64, 10);
const HOOK_B: BlockPos = BlockPos::new(9, 64, 10);

fn tripwire_line() -> Scene {
    let mut cells = Cells::default()
        .state(
            HOOK_A,
            BlockKind::TripwireHook,
            &[
                ("facing", "east"),
                ("attached", "true"),
                ("powered", "false"),
            ],
        )
        .state(
            HOOK_B,
            BlockKind::TripwireHook,
            &[
                ("facing", "west"),
                ("attached", "true"),
                ("powered", "false"),
            ],
        );
    for x in 6..=8 {
        cells = cells.state(
            BlockPos::new(x, 64, 10),
            BlockKind::Tripwire,
            &[("attached", "true"), ("east", "true"), ("west", "true")],
        );
    }
    rig(cells, 0)
}

fn break_wire_ever_powers_a_hook(held: Option<&str>) -> bool {
    let mut s = tripwire_line();
    let alice = survival(&mut s);
    if let Some(item) = held {
        console(&mut s, &format!("give Alice {item} 1"));
    }
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Dig {
            action: rspigot_core::events::DigAction::Start,
            pos: WIRE,
            face: Direction::Up,
            sequence: 1,
        },
    );
    let mut powered = false;
    for _ in 0..6 {
        s.tick(1);
        powered |= property_at(&s, HOOK_A, "powered").as_deref() == Some("true")
            || property_at(&s, HOOK_B, "powered").as_deref() == Some("true");
    }
    assert_eq!(kind_at(&s, WIRE), Some(BlockKind::Air));
    powered
}

#[test]
fn breaking_a_tripwire_by_hand_powers_its_hooks_for_a_moment() {
    assert!(break_wire_ever_powers_a_hook(None));
}

#[test]
fn breaking_a_tripwire_with_shears_disarms_it_so_the_hooks_stay_quiet() {
    assert!(!break_wire_ever_powers_a_hook(Some("minecraft:shears")));
}

fn hive_with_bees(count: usize) -> rspigot_nbt::Compound {
    let bees = (0..count)
        .map(|_| {
            rspigot_nbt::Tag::Compound(
                rspigot_nbt::Compound::new()
                    .with(
                        "entity_data",
                        rspigot_nbt::Compound::new().with("id", "minecraft:bee"),
                    )
                    .with("ticks_in_hive", 0)
                    .with("min_ticks_in_hive", 2400),
            )
        })
        .collect();
    rspigot_nbt::Compound::new().with("bees", rspigot_nbt::Tag::List(bees))
}

#[test]
fn a_wind_charge_shakes_the_bees_out_of_a_nearby_hive() {
    let cells = Cells::default()
        .state(
            SPOT,
            BlockKind::BeeNest,
            &[("facing", "north"), ("honey_level", "0")],
        )
        .entity(SPOT, "beehive", hive_with_bees(2));
    let mut s = rig(cells, 0);
    assert_eq!(count_of(&mut s, "minecraft:bee"), 0);
    console(
        &mut s,
        "summon minecraft:wind_charge 9.5 67.0 10.5 {Motion:[0.0d,-1.0d,0.0d]}",
    );
    let mut most = 0;
    for _ in 0..8 {
        s.tick(1);
        most = most.max(count_of(&mut s, "minecraft:bee"));
    }
    assert!(most > 0);
}
