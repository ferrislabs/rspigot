mod common;

use common::*;
use rspigot_core::events::{ClientEvent, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::EventKind;
use rspigot_domain::{BlockPos, EntityId};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

const HARNESS: &str = "equipment:{body:{id:\"minecraft:blue_harness\",count:1}}";

fn world() -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.tick(60);
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon_at(s: &mut Scene, kind: Kind, at: &str, nbt: &str) -> EntityId {
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

fn saved(s: &Scene, id: EntityId) -> Compound {
    s.server.saved_entity(id).expect("a saved mob")
}

fn interact_with(s: &mut Scene, target: EntityId, sneaking: bool) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target,
            hand: Hand::Main,
            sneaking,
            location: rspigot_domain::Vec3::new(0.0, 0.5, 0.0),
        },
    );
    s.tick(2);
}

fn body_id(nbt: &Compound) -> Option<String> {
    nbt.get_compound("equipment")?
        .get_compound("body")?
        .get_str("id")
        .map(str::to_owned)
}

fn passengers(nbt: &Compound) -> usize {
    nbt.get_list("Passengers").map_or(0, <[_]>::len)
}

fn sounds(events: &[ClientEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } => Some(*sound),
            _ => None,
        })
        .collect()
}

#[test]
fn a_happy_ghast_keeps_its_harness_and_still_timeout_in_the_vanilla_layout() {
    let mut s = world();
    let ghast = summon_at(
        &mut s,
        Kind::HappyGhast,
        "3.5 70 0.5",
        &format!("{{NoAI:1b,still_timeout:0,{HARNESS}}}"),
    );
    let nbt = saved(&s, ghast);
    assert_eq!(
        body_id(&nbt).as_deref(),
        Some("minecraft:blue_harness"),
        "{nbt:?}"
    );
    assert_eq!(nbt.get_i32("still_timeout"), Some(0));
}

#[test]
fn a_harness_fits_an_adult_with_the_equip_sound_and_never_a_ghastling() {
    let mut s = world();
    let ghast = summon_at(&mut s, Kind::HappyGhast, "3.5 66 0.5", "{NoAI:1b}");
    console(&mut s, "give Alice minecraft:blue_harness 2");
    s.players[0].link.take();
    interact_with(&mut s, ghast, true);
    let events = s.players[0].link.take();
    assert_eq!(
        body_id(&saved(&s, ghast)).as_deref(),
        Some("minecraft:blue_harness")
    );
    assert!(
        sounds(&events).contains(&"minecraft:entity.happy_ghast.equip"),
        "{events:?}"
    );
    let baby = summon_at(
        &mut s,
        Kind::HappyGhast,
        "3.5 66 6.5",
        "{NoAI:1b,Age:-24000}",
    );
    interact_with(&mut s, baby, true);
    assert_eq!(body_id(&saved(&s, baby)), None);
}

#[test]
fn a_harnessed_ghast_carries_the_player_who_uses_it_unless_sneaking() {
    let mut s = world();
    let ghast = summon_at(
        &mut s,
        Kind::HappyGhast,
        "3.5 66 0.5",
        &format!("{{NoAI:1b,{HARNESS}}}"),
    );
    s.players[0].link.take();
    interact_with(&mut s, ghast, true);
    assert_eq!(passengers(&saved(&s, ghast)), 0);
    interact_with(&mut s, ghast, false);
    let events = s.players[0].link.take();
    assert!(
        events.iter().any(|event| matches!(event, ClientEvent::Passengers { vehicle, passengers } if *vehicle == ghast && passengers.len() == 1)),
        "{events:?}"
    );
    assert!(sounds(&events).contains(&"minecraft:entity.happy_ghast.harness_goggles_down"));
}

#[test]
fn a_bare_adult_cannot_be_ridden() {
    let mut s = world();
    let ghast = summon_at(&mut s, Kind::HappyGhast, "3.5 66 0.5", "{NoAI:1b}");
    s.players[0].link.take();
    interact_with(&mut s, ghast, false);
    let events = s.players[0].link.take();
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, ClientEvent::Passengers { .. })),
        "{events:?}"
    );
}

#[test]
fn shears_take_the_harness_off_with_the_unequip_sound() {
    let mut s = world();
    let ghast = summon_at(
        &mut s,
        Kind::HappyGhast,
        "3.5 66 0.5",
        &format!("{{NoAI:1b,{HARNESS}}}"),
    );
    console(&mut s, "give Alice minecraft:shears 1");
    s.players[0].link.take();
    interact_with(&mut s, ghast, false);
    let events = s.players[0].link.take();
    assert_eq!(body_id(&saved(&s, ghast)), None);
    assert!(
        sounds(&events).contains(&"minecraft:entity.happy_ghast.unequip"),
        "{events:?}"
    );
}

#[test]
fn a_player_standing_on_the_back_makes_the_ghast_stay_still() {
    let mut s = world();
    let ghast = summon_at(&mut s, Kind::HappyGhast, "3.5 66 0.5", "{NoAI:1b}");
    s.tick(5);
    assert_eq!(saved(&s, ghast).get_i32("still_timeout"), Some(0));
    console(&mut s, "tp Alice 3.5 70.2 0.5");
    s.tick(3);
    let timeout = saved(&s, ghast)
        .get_i32("still_timeout")
        .expect("a timeout");
    assert!(timeout > 0, "{timeout}");
}

#[test]
fn a_ghast_picks_a_home_radius_by_harness_and_age() {
    let mut s = world();
    let bare = summon_at(&mut s, Kind::HappyGhast, "3.5 66 0.5", "{}");
    let harnessed = summon_at(
        &mut s,
        Kind::HappyGhast,
        "10.5 66 0.5",
        &format!("{{{HARNESS}}}"),
    );
    let baby = summon_at(&mut s, Kind::HappyGhast, "17.5 66 0.5", "{Age:-24000}");
    s.tick(5);
    assert_eq!(saved(&s, bare).get_i32("home_radius"), Some(64));
    assert_eq!(saved(&s, harnessed).get_i32("home_radius"), Some(32));
    assert_eq!(saved(&s, baby).get_i32("home_radius"), Some(32));
}

#[test]
fn a_hurt_ghast_slowly_heals() {
    let mut s = world();
    let ghast = summon_at(
        &mut s,
        Kind::HappyGhast,
        "3.5 66 0.5",
        "{NoAI:1b,Health:10f}",
    );
    s.tick(700);
    let health = saved(&s, ghast).get_f64("Health").expect("a health");
    assert!(health > 10.0, "{health}");
}

fn distance_to_alice(s: &Scene, ghast: EntityId) -> f64 {
    let at = s
        .server
        .entities_of(Kind::HappyGhast)
        .into_iter()
        .find(|entity| entity.id == ghast)
        .expect("the ghast")
        .position;
    let (dx, dy, dz) = (at.x - 2.5, at.y - 64.0, at.z - 0.5);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

#[test]
fn a_ghastling_follows_a_player_holding_a_snowball() {
    let mut s = world();
    console(&mut s, "tp Alice 2.5 64 0.5");
    let ghast = summon_at(&mut s, Kind::HappyGhast, "12.5 66 0.5", "{Age:-24000}");
    let before = distance_to_alice(&s, ghast);
    console(&mut s, "give Alice minecraft:snowball 1");
    s.tick(200);
    let after = distance_to_alice(&s, ghast);
    assert!(after < before - 3.0, "{before} -> {after}");
}

#[test]
fn an_adult_without_a_harness_turns_to_face_a_player_holding_a_harness() {
    let mut s = world();
    console(&mut s, "tp Alice 2.5 64 0.5");
    let ghast = summon_at(&mut s, Kind::HappyGhast, "12.5 66 0.5", "{}");
    console(&mut s, "give Alice minecraft:red_harness 1");
    s.tick(60);
    let yaw = s
        .server
        .entities_of(Kind::HappyGhast)
        .into_iter()
        .find(|entity| entity.id == ghast)
        .expect("the ghast")
        .yaw;
    assert!((yaw - 90.0).abs() < 10.0, "{yaw}");
}

fn position_of(s: &Scene, ghast: EntityId) -> rspigot_domain::Vec3 {
    s.server
        .entities_of(Kind::HappyGhast)
        .into_iter()
        .find(|entity| entity.id == ghast)
        .expect("the ghast")
        .position
}

fn drive(s: &mut Scene, to: rspigot_domain::Vec3) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::MoveVehicle {
            position: to,
            rotation: rspigot_domain::Rotation {
                yaw: 30.0,
                pitch: 0.0,
            },
            on_ground: false,
        },
    );
    s.tick(2);
}

#[test]
fn the_rider_of_a_harnessed_ghast_steers_it_from_the_client() {
    let mut s = world();
    let ghast = summon_at(
        &mut s,
        Kind::HappyGhast,
        "3.5 66 0.5",
        &format!("{{NoAI:1b,{HARNESS}}}"),
    );
    interact_with(&mut s, ghast, false);
    let target = rspigot_domain::Vec3::new(5.5, 67.0, 4.5);
    drive(&mut s, target);
    assert_eq!(position_of(&s, ghast), target);
}

#[test]
fn a_ghast_that_stays_still_ignores_the_steering_of_its_rider() {
    let mut s = scene(&["Alice", "Bob"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.tick(60);
    let ghast = summon_at(
        &mut s,
        Kind::HappyGhast,
        "3.5 66 0.5",
        &format!("{{NoAI:1b,{HARNESS}}}"),
    );
    interact_with(&mut s, ghast, false);
    console(&mut s, "tp Bob 3.5 70.2 0.5");
    s.tick(3);
    assert!(
        saved(&s, ghast)
            .get_i32("still_timeout")
            .is_some_and(|t| t > 0)
    );
    let before = position_of(&s, ghast);
    drive(&mut s, rspigot_domain::Vec3::new(5.5, 67.0, 4.5));
    assert_eq!(position_of(&s, ghast), before);
}

#[test]
fn steering_a_ghast_announces_the_vehicle_move_to_plugins() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::VehicleMove)]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.tick(60);
    let ghast = summon_at(
        &mut s,
        Kind::HappyGhast,
        "3.5 66 0.5",
        &format!("{{NoAI:1b,{HARNESS}}}"),
    );
    interact_with(&mut s, ghast, false);
    assert!(settled_for(&s.plugin, EventKind::VehicleMove).is_empty());
    drive(&mut s, rspigot_domain::Vec3::new(5.5, 67.0, 4.5));
    assert_eq!(settled_for(&s.plugin, EventKind::VehicleMove).len(), 1);
}

fn age_of(s: &Scene, id: EntityId) -> i32 {
    saved(s, id).get_i32("Age").expect("an age")
}

#[test]
fn a_snowball_speeds_up_a_ghastling_and_a_spawn_egg_on_an_adult_hatches_one() {
    let mut s = world();
    let baby = summon_at(
        &mut s,
        Kind::HappyGhast,
        "3.5 66 0.5",
        "{NoAI:1b,Age:-24000}",
    );
    console(&mut s, "give Alice minecraft:snowball 2");
    let before = age_of(&s, baby);
    interact_with(&mut s, baby, false);
    assert!(age_of(&s, baby) > before + 1000, "{}", age_of(&s, baby));
    let adult = summon_at(&mut s, Kind::HappyGhast, "3.5 66 6.5", "{NoAI:1b}");
    console(&mut s, "clear Alice");
    console(&mut s, "give Alice minecraft:happy_ghast_spawn_egg 1");
    let known = s.server.entities_of(Kind::HappyGhast).len();
    interact_with(&mut s, adult, false);
    let ghasts = s.server.entities_of(Kind::HappyGhast);
    assert_eq!(ghasts.len(), known + 1);
    let hatched = ghasts
        .into_iter()
        .map(|entity| entity.id)
        .find(|id| age_of(&s, *id) < 0 && *id != baby)
        .expect("a ghastling");
    assert!(age_of(&s, hatched) <= -23000);
}

fn place_on_floor(s: &mut Scene, item: &str, x: i32) {
    console(s, &format!("give Alice minecraft:{item} 1"));
    s.tick(1);
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: BlockPos::new(x, 63, 1),
            face: rspigot_core::events::Direction::Up,
            cursor: rspigot_domain::Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 2,
        },
    );
    s.tick(2);
}

fn block_at(s: &Scene, x: i32) -> Option<rspigot_domain::BlockStateId> {
    s.server.block_state_at(BlockPos::new(x, 64, 1))
}

struct Pool;

impl rspigot_core::ports::ChunkGenerator for Pool {
    fn generate(
        &self,
        dimension: &rspigot_domain::world::Dimension,
        pos: rspigot_domain::ChunkPos,
    ) -> rspigot_domain::chunk::ChunkColumn {
        let mut chunk = rspigot_domain::chunk::ChunkColumn::empty(
            pos,
            dimension.height,
            rspigot_domain::BiomeId(0),
        );
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        rspigot_domain::BlockStateId(1),
                    );
                }
            }
        }
        if pos.x == 0 && pos.z == 0 {
            chunk.set_block(
                BlockPos::new(2, 64, 1),
                rspigot_blocks::BlockKind::Water.default_state(),
            );
        }
        chunk
    }

    fn spawn_point(&self, _: &rspigot_domain::world::Dimension) -> rspigot_domain::Vec3 {
        rspigot_domain::Vec3::new(0.5, 64.0, 0.5)
    }
}

fn pool_world() -> Scene {
    let config = rspigot_core::config::GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..rspigot_core::config::GameConfig::default()
    };
    let data = rspigot_core::config::LevelData {
        spawn_mobs: false,
        ..rspigot_core::config::LevelData::default()
    };
    let mut server = rspigot_core::server::Server::new(
        config,
        data,
        std::sync::Arc::new(NoStore),
        std::sync::Arc::new(Pool),
        rspigot_core::status::StatusBoard::new(20),
    );
    server.wait_for_spawn(std::time::Duration::from_secs(10));
    let mut s = scene_on(server, &["Alice"], Vec::new());
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s.tick(60);
    s
}

#[test]
fn a_dried_ghast_placed_in_water_is_waterlogged_and_in_time_hatches_a_ghastling() {
    let mut s = pool_world();
    console(&mut s, "gamemode creative Alice");
    console(&mut s, "tp Alice 2.5 64 4.5");
    place_on_floor(&mut s, "dried_ghast", 2);
    let state = block_at(&s, 2).expect("a block");
    assert_eq!(state.kind(), Some(rspigot_blocks::BlockKind::DriedGhast));
    assert_eq!(state.property("waterlogged"), Some("true"), "{state:?}");
    assert_eq!(state.property("hydration"), Some("0"));
    s.tick(40_000);
    let ghastlings = s.server.entities_of(Kind::HappyGhast);
    assert_eq!(ghastlings.len(), 1);
    assert!(age_of(&s, ghastlings[0].id) < 0);
    assert_ne!(
        block_at(&s, 2).and_then(rspigot_domain::BlockStateId::kind),
        Some(rspigot_blocks::BlockKind::DriedGhast)
    );
}

#[test]
fn a_dried_ghast_on_dry_land_never_hatches() {
    let mut s = world();
    console(&mut s, "gamemode creative Alice");
    console(&mut s, "tp Alice 2.5 64 4.5");
    place_on_floor(&mut s, "dried_ghast", 3);
    let state = block_at(&s, 3).expect("a block");
    assert_eq!(state.kind(), Some(rspigot_blocks::BlockKind::DriedGhast));
    assert_eq!(state.property("waterlogged"), Some("false"));
    s.tick(10_000);
    assert!(s.server.entities_of(Kind::HappyGhast).is_empty());
}
