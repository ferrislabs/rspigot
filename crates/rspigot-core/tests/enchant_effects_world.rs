mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, DigAction, Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, InventoryOrder, ItemSpec, Order, PlayerOrder, Question};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, EntityId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

const FEET: i32 = 36;
const MOVEMENT_SPEED: &str = "minecraft:movement_speed";
const WALK_SPEED: f64 = 0.1;

struct Ground;

impl ChunkGenerator for Ground {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                let (bx, bz) = (pos.min_block_x() + x, pos.min_block_z() + z);
                for y in 60..=61 {
                    chunk.set_block(BlockPos::new(bx, y, bz), BlockKind::Stone.default_state());
                }
                let surface = match bx {
                    ..=9 => BlockKind::Water,
                    10..=11 => BlockKind::Stone,
                    _ => BlockKind::SoulSand,
                };
                chunk.set_block(BlockPos::new(bx, 62, bz), surface.default_state());
            }
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 63.0, 8.5)
    }
}

fn ground() -> Scene {
    ground_ticking(3)
}

fn ground_ticking(speed: i32) -> Scene {
    let mut config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        thunder_chance: 0,
        ..GameConfig::default()
    };
    config.rules.random_tick_speed = speed;
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
        Arc::new(Ground),
        StatusBoard::new(20),
    )
    .with_rng_seed(11);
    server.wait_for_spawn(Duration::from_secs(10));
    let mut s = scene_on(server, &["Alice"], Vec::new());
    s.tick(30);
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn give(s: &mut Scene, slot: i32, item: &str, enchantments: &[(&str, i32)]) {
    let player = s.players[0].id;
    let mut spec = ItemSpec::of(item, 1);
    spec.enchantments = enchantments
        .iter()
        .map(|(name, level)| ((*name).to_owned(), *level))
        .collect();
    s.act(Order::Inventory {
        player,
        order: InventoryOrder::Set {
            slot,
            item: Some(spec),
        },
    });
}

fn walk_to(s: &mut Scene, position: Vec3) {
    let alice = s.players[0].clone();
    let start = s.server.player_location(alice.id).unwrap_or(position);
    let legs = ((position - start).length() / 6.0).ceil().max(1.0) as u32;
    for leg in 1..=legs {
        let along = start + (position - start).scale(f64::from(leg) / f64::from(legs));
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::Move {
                position: Some(along),
                rotation: None,
                on_ground: true,
            },
        );
        s.tick(1);
    }
}

fn block_kind(s: &mut Scene, pos: BlockPos) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state.kind,
        other => panic!("expected a block, got {other:?}"),
    }
}

fn entity_of(s: &mut Scene) -> EntityId {
    match s.ask(Question::Player(s.players[0].id)) {
        Answer::Player(state) => EntityId(state.player.entity_id),
        _ => panic!("expected the player"),
    }
}

fn speed(s: &mut Scene) -> f64 {
    let entity = entity_of(s);
    match s.ask(Question::AttributeValue {
        entity,
        attribute: MOVEMENT_SPEED.to_owned(),
    }) {
        Answer::AttributeValue(value) => value,
        other => panic!("expected a value, got {other:?}"),
    }
}

#[test]
fn frost_walker_freezes_the_water_under_a_wearer_who_steps_onto_it() {
    let mut s = ground();
    give(
        &mut s,
        FEET,
        "minecraft:diamond_boots",
        &[("minecraft:frost_walker", 1)],
    );
    s.tick(2);
    assert_eq!(
        block_kind(&mut s, BlockPos::new(9, 62, 8)),
        "minecraft:water"
    );
    walk_to(&mut s, Vec3::new(9.5, 63.0, 8.5));
    assert_eq!(
        block_kind(&mut s, BlockPos::new(9, 62, 8)),
        "minecraft:frosted_ice"
    );
    assert_eq!(
        block_kind(&mut s, BlockPos::new(8, 62, 8)),
        "minecraft:frosted_ice"
    );
    assert_eq!(
        block_kind(&mut s, BlockPos::new(9, 62, 12)),
        "minecraft:water"
    );
}

#[test]
fn without_frost_walker_the_water_stays() {
    let mut s = ground();
    walk_to(&mut s, Vec3::new(9.5, 63.0, 8.5));
    assert_eq!(
        block_kind(&mut s, BlockPos::new(9, 62, 8)),
        "minecraft:water"
    );
}

fn particles_of(link: &Link) -> usize {
    link.take()
        .iter()
        .filter(|event| matches!(event, ClientEvent::Particle(p) if p.kind == "minecraft:soul"))
        .count()
}

#[test]
fn soul_speed_boosts_the_wearer_on_soul_sand_and_leaves_with_them() {
    let mut s = ground();
    give(
        &mut s,
        FEET,
        "minecraft:diamond_boots",
        &[("minecraft:soul_speed", 3)],
    );
    s.tick(2);
    assert!((speed(&mut s) - WALK_SPEED).abs() < 1e-6);
    walk_to(&mut s, Vec3::new(11.5, 63.0, 8.5));
    walk_to(&mut s, Vec3::new(12.5, 63.0, 8.5));
    let boosted = speed(&mut s);
    assert!(
        (boosted - (WALK_SPEED + 0.0405 + 0.0105 * 2.0)).abs() < 1e-6,
        "{boosted}"
    );
    walk_to(&mut s, Vec3::new(11.5, 63.0, 8.5));
    assert!((speed(&mut s) - WALK_SPEED).abs() < 1e-6);
}

#[test]
fn soul_speed_boots_trail_soul_particles_while_walking_on_soul_sand() {
    let mut s = ground();
    give(
        &mut s,
        FEET,
        "minecraft:diamond_boots",
        &[("minecraft:soul_speed", 1)],
    );
    walk_to(&mut s, Vec3::new(12.5, 63.0, 8.5));
    s.players[0].link.take();
    let mut seen = 0;
    for index in 0..20 {
        let x = 12.5 + 0.2 * f64::from(index % 2);
        walk_to(&mut s, Vec3::new(x, 63.0, 8.5));
        seen += particles_of(&s.players[0].link);
    }
    assert!(seen > 0, "no soul particle");
}

#[test]
fn bare_feet_leave_no_soul_particles() {
    let mut s = ground();
    walk_to(&mut s, Vec3::new(12.5, 63.0, 8.5));
    s.players[0].link.take();
    let mut seen = 0;
    for index in 0..20 {
        let x = 12.5 + 0.2 * f64::from(index % 2);
        walk_to(&mut s, Vec3::new(x, 63.0, 8.5));
        seen += particles_of(&s.players[0].link);
    }
    assert_eq!(seen, 0);
}

#[test]
fn lunge_pushes_tires_and_sounds_through_the_post_piercing_effects() {
    let mut s = ground();
    give(&mut s, 0, "minecraft:iron_spear", &[("minecraft:lunge", 2)]);
    s.tick(2);
    s.players[0].link.take();
    let before = match s.ask(Question::Player(s.players[0].id)) {
        Answer::Player(state) => state.saturation * 4.0 - state.exhaustion,
        _ => panic!("expected the player"),
    };
    s.server.run_post_piercing_attack(s.players[0].id);
    s.tick(1);
    let events = s.players[0].link.take();
    let pushed = events.iter().find_map(|event| match event {
        ClientEvent::EntityVelocity { velocity, .. } if velocity.length_sqr() > 0.0 => {
            Some(*velocity)
        }
        _ => None,
    });
    let pushed = pushed.expect("a velocity packet");
    assert!(pushed.z > 0.9 && pushed.z < 0.92, "{pushed:?}");
    assert!(pushed.y.abs() < 1e-9);
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::Sound { sound, .. } if sound.starts_with("minecraft:item.spear.lunge_")
    )));
    let after = match s.ask(Question::Player(s.players[0].id)) {
        Answer::Player(state) => state.saturation * 4.0 - state.exhaustion,
        _ => panic!("expected the player"),
    };
    assert!((before - after - 8.0).abs() < 1e-4, "{before} -> {after}");
}

#[test]
fn a_hungry_survival_wielder_does_not_lunge() {
    let mut s = ground();
    give(&mut s, 0, "minecraft:iron_spear", &[("minecraft:lunge", 2)]);
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::SetFoodLevel(3)));
    s.players[0].link.take();
    s.server.run_post_piercing_attack(s.players[0].id);
    s.tick(1);
    let pushed = s.players[0]
        .link
        .take()
        .iter()
        .any(|event| matches!(event, ClientEvent::EntityVelocity { velocity, .. } if velocity.length_sqr() > 0.0));
    assert!(!pushed);
}

fn throw_trident(s: &mut Scene) -> EntityId {
    give(s, 0, "minecraft:trident", &[("minecraft:channeling", 1)]);
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
    s.tick(12);
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
    s.tick(1);
    s.server
        .entities_of(Kind::Trident)
        .first()
        .map(|view| view.id)
        .expect("a thrown trident")
}

#[test]
fn a_channeling_trident_that_hits_a_lightning_rod_in_a_thunderstorm_calls_lightning() {
    let mut s = ground();
    console(&mut s, "weather thunder");
    s.tick(2);
    let trident = throw_trident(&mut s);
    console(&mut s, "setblock 8 64 30 minecraft:lightning_rod");
    assert!(s.server.entities_of(Kind::LightningBolt).is_empty());
    s.server
        .trident_hit_block(trident, BlockPos::new(8, 64, 30));
    s.tick(2);
    assert!(!s.server.entities_of(Kind::LightningBolt).is_empty());
}

#[test]
fn the_same_hit_on_plain_stone_or_in_clear_weather_calls_nothing() {
    let mut s = ground();
    console(&mut s, "weather thunder");
    s.tick(2);
    let trident = throw_trident(&mut s);
    s.server
        .trident_hit_block(trident, BlockPos::new(8, 63, 30));
    s.tick(2);
    assert!(s.server.entities_of(Kind::LightningBolt).is_empty());

    let mut s = ground();
    console(&mut s, "weather clear");
    s.tick(2);
    let trident = throw_trident(&mut s);
    console(&mut s, "setblock 8 64 30 minecraft:lightning_rod");
    s.server
        .trident_hit_block(trident, BlockPos::new(8, 64, 30));
    s.tick(2);
    assert!(s.server.entities_of(Kind::LightningBolt).is_empty());
}

fn orb_total(s: &Scene) -> i32 {
    s.server
        .experience_orbs()
        .iter()
        .map(|orb| orb.value * orb.count.max(1))
        .sum()
}

fn mine_ore(enchantments: &[(&str, i32)]) -> i32 {
    let mut s = ground();
    give(&mut s, 0, "minecraft:diamond_pickaxe", enchantments);
    console(&mut s, "setblock 11 62 11 minecraft:diamond_ore");
    s.tick(2);
    let alice = s.players[0].clone();
    let pos = BlockPos::new(11, 62, 11);
    for (action, sequence) in [(DigAction::Start, 1), (DigAction::Finish, 2)] {
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::Dig {
                action,
                pos,
                face: Direction::Up,
                sequence,
            },
        );
        s.tick(60);
    }
    assert_ne!(block_kind(&mut s, pos), "minecraft:diamond_ore");
    orb_total(&s)
}

#[test]
fn breaking_an_ore_with_the_right_tool_drops_experience() {
    let xp = mine_ore(&[]);
    assert!((3..=7).contains(&xp), "{xp}");
}

#[test]
fn silk_touch_drops_no_experience_through_the_block_experience_component() {
    assert_eq!(mine_ore(&[("minecraft:silk_touch", 1)]), 0);
}

#[test]
fn ice_placed_by_frost_walker_melts_back_to_water() {
    let mut s = ground();
    give(
        &mut s,
        FEET,
        "minecraft:diamond_boots",
        &[("minecraft:frost_walker", 1)],
    );
    s.tick(2);
    walk_to(&mut s, Vec3::new(9.5, 63.0, 8.5));
    let frozen = BlockPos::new(9, 62, 8);
    assert_eq!(block_kind(&mut s, frozen), "minecraft:frosted_ice");
    walk_to(&mut s, Vec3::new(9.5, 63.0, 20.5));
    s.tick(900);
    assert_eq!(block_kind(&mut s, frozen), "minecraft:water");
}

#[test]
fn vanishing_curse_items_are_destroyed_on_death_and_the_rest_drops() {
    let mut s = ground();
    give(
        &mut s,
        0,
        "minecraft:diamond_sword",
        &[("minecraft:vanishing_curse", 1)],
    );
    give(&mut s, 1, "minecraft:stick", &[]);
    s.tick(2);
    console(&mut s, "kill Alice");
    s.tick(5);
    assert_eq!(s.server.items_on_ground(), 1);
}

fn health_of(s: &mut Scene) -> f32 {
    match s.ask(Question::Player(s.players[0].id)) {
        Answer::Player(state) => state.health,
        _ => panic!("expected the player"),
    }
}

#[test]
fn frost_walker_boots_shrug_off_hot_floor_damage_that_bare_feet_take() {
    let mut s = ground();
    console(&mut s, "damage Alice 4 minecraft:hot_floor");
    s.tick(2);
    assert_eq!(health_of(&mut s), 16.0);
    let mut s = ground();
    give(
        &mut s,
        FEET,
        "minecraft:diamond_boots",
        &[("minecraft:frost_walker", 1)],
    );
    s.tick(2);
    console(&mut s, "damage Alice 4 minecraft:hot_floor");
    s.tick(2);
    assert_eq!(health_of(&mut s), 20.0);
}
