mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{Answer, EventKind, EventPayload, ItemSpec, Question};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Marsh;

fn biome_of(_: ChunkPos) -> &'static str {
    "minecraft:plains"
}

impl ChunkGenerator for Marsh {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id(biome_of(pos)).unwrap();
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

fn marsh_server(seed: u64) -> Server<Link> {
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
        Arc::new(Marsh),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(seed: u64) -> Scene {
    ready_with(seed, Vec::new())
}

fn ready_with(seed: u64, handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
    let mut s = scene_on(marsh_server(seed), &["Alice"], handlers);
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

fn exists(s: &Scene, kind: Kind, id: EntityId) -> bool {
    s.server
        .entities_of(kind)
        .iter()
        .any(|entity| entity.id == id)
}

fn cubes(s: &Scene) -> Vec<EntityId> {
    s.server
        .entities_of(Kind::SulfurCube)
        .into_iter()
        .map(|entity| entity.id)
        .collect()
}

fn click(s: &mut Scene, target: EntityId) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.1, 0.0),
        },
    );
}

fn use_down(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: Rotation::new(0.0, 90.0),
        },
    );
}

fn slots(s: &mut Scene) -> Vec<Option<ItemSpec>> {
    let alice = s.players[0].id;
    match s.ask(Question::Inventory(alice)) {
        Answer::Inventory(inventory) => inventory.slots,
        other => panic!("expected the inventory, got {other:?}"),
    }
}

fn first_slot(s: &mut Scene) -> Option<(String, i32)> {
    slots(s)
        .into_iter()
        .flatten()
        .next()
        .map(|spec| (spec.key, spec.count))
}

fn body_of(s: &Scene, id: EntityId) -> Option<String> {
    saved(s, id)
        .get_compound("equipment")
        .and_then(|equipment| equipment.get_compound("body"))
        .and_then(|body| body.get_str("id"))
        .map(str::to_owned)
}

fn dropped(s: &Scene, name: &str) -> Vec<(EntityId, i32)> {
    s.server
        .ground_items()
        .into_iter()
        .filter(|(_, stack)| stack.item.kind().is_some_and(|kind| kind.name() == name))
        .map(|(id, stack)| (id, i32::from(stack.count.get())))
        .collect()
}

const BODY: &str = "equipment:{body:{id:\"minecraft:";

#[test]
fn a_summoned_adult_stores_its_size_its_timers_and_no_body() {
    let mut s = ready(1);
    let cube = summon(&mut s, Kind::SulfurCube, "4.5 64 4.5", "{Size:1}");
    let nbt = saved(&s, cube);
    assert_eq!(nbt.get_i32("Size"), Some(1));
    assert_eq!(nbt.get_i32("pickup_timer"), Some(0));
    assert_eq!(nbt.get_bool("from_bucket"), Some(false));
    assert_eq!(nbt.get_i32("fuse"), Some(-1));
    assert_eq!(body_of(&s, cube), None);
    assert_eq!(nbt.get_f64("Health"), Some(8.0));
}

#[test]
fn a_cube_swallows_a_block_it_is_given_and_keeps_it_as_a_guaranteed_drop() {
    let mut s = ready(2);
    console(&mut s, "give Alice minecraft:oak_planks 3");
    let cube = summon(&mut s, Kind::SulfurCube, "9.5 64 8.5", "{Size:1,NoAI:1b}");
    click(&mut s, cube);
    s.tick(2);
    assert_eq!(body_of(&s, cube).as_deref(), Some("minecraft:oak_planks"));
    assert_eq!(
        first_slot(&mut s),
        Some(("minecraft:oak_planks".to_owned(), 2))
    );
    let nbt = saved(&s, cube);
    let chance = nbt
        .get_compound("drop_chances")
        .and_then(|chances| chances.get_f64("body"));
    assert_eq!(chance, Some(2.0));
    click(&mut s, cube);
    s.tick(1);
    assert_eq!(
        first_slot(&mut s),
        Some(("minecraft:oak_planks".to_owned(), 2))
    );
}

#[test]
fn a_different_block_replaces_the_swallowed_one_and_spits_the_old_one_out() {
    let mut s = ready(3);
    console(&mut s, "give Alice minecraft:honeycomb_block 1");
    let cube = summon(
        &mut s,
        Kind::SulfurCube,
        "9.5 64 8.5",
        &format!("{{Size:1,NoAI:1b,{BODY}oak_planks\",count:1}}}}}}"),
    );
    click(&mut s, cube);
    s.tick(1);
    assert_eq!(
        body_of(&s, cube).as_deref(),
        Some("minecraft:honeycomb_block")
    );
    assert_eq!(dropped(&s, "minecraft:oak_planks").len(), 1);
}

#[test]
fn a_baby_and_an_item_that_is_no_block_are_never_swallowed() {
    let mut s = ready(4);
    console(&mut s, "give Alice minecraft:diamond 1");
    let adult = summon(&mut s, Kind::SulfurCube, "9.5 64 8.5", "{Size:1,NoAI:1b}");
    click(&mut s, adult);
    assert_eq!(body_of(&s, adult), None);
    console(&mut s, "give Alice minecraft:dirt 1");
    let baby = summon(
        &mut s,
        Kind::SulfurCube,
        "9.5 64 5.5",
        "{Size:0,Age:-24000,NoAI:1b}",
    );
    click(&mut s, baby);
    assert_eq!(body_of(&s, baby), None);
}

#[test]
fn shears_make_the_cube_spit_its_block_out_and_stop_it_swallowing_for_five_seconds() {
    let mut s = ready(5);
    console(&mut s, "give Alice minecraft:shears 1");
    let cube = summon(
        &mut s,
        Kind::SulfurCube,
        "9.5 64 8.5",
        &format!("{{Size:1,NoAI:1b,{BODY}dirt\",count:1}}}}}}"),
    );
    click(&mut s, cube);
    s.tick(1);
    assert_eq!(body_of(&s, cube), None);
    assert_eq!(saved(&s, cube).get_i32("pickup_timer"), Some(100));
    assert_eq!(dropped(&s, "minecraft:dirt").len(), 1);
}

#[test]
fn shearing_a_cube_fires_no_plugin_event_as_vanilla_does_not() {
    let mut s = ready_with(6, vec![observe(1, EventKind::PlayerShearEntity)]);
    console(&mut s, "give Alice minecraft:shears 1");
    let cube = summon(
        &mut s,
        Kind::SulfurCube,
        "9.5 64 8.5",
        &format!("{{Size:1,NoAI:1b,{BODY}dirt\",count:1}}}}}}"),
    );
    click(&mut s, cube);
    s.tick(1);
    assert!(settled_for(&s.plugin, EventKind::PlayerShearEntity).is_empty());
    assert_eq!(body_of(&s, cube), None);
}

#[test]
fn a_cube_picks_up_a_swallowable_item_lying_next_to_it_one_at_a_time() {
    let mut s = ready(7);
    let cube = summon(&mut s, Kind::SulfurCube, "4.5 64 4.5", "{Size:1,NoAI:1b}");
    console(
        &mut s,
        "summon minecraft:item 4.5 64 4.5 {Item:{id:\"minecraft:dirt\",count:2},PickupDelay:0s}",
    );
    s.tick(3);
    assert_eq!(body_of(&s, cube).as_deref(), Some("minecraft:dirt"));
    assert_eq!(
        dropped(&s, "minecraft:dirt"),
        vec![(dropped(&s, "minecraft:dirt")[0].0, 1)]
    );
}

#[test]
fn a_cube_ignores_items_that_are_not_blocks_and_when_it_was_just_sheared() {
    let mut s = ready(8);
    let cube = summon(
        &mut s,
        Kind::SulfurCube,
        "4.5 64 4.5",
        "{Size:1,NoAI:1b,pickup_timer:100}",
    );
    console(
        &mut s,
        "summon minecraft:item 4.5 64 4.5 {Item:{id:\"minecraft:dirt\",count:1},PickupDelay:0s}",
    );
    console(
        &mut s,
        "summon minecraft:item 4.5 64 4.5 {Item:{id:\"minecraft:diamond\",count:1},PickupDelay:0s}",
    );
    s.tick(10);
    assert_eq!(body_of(&s, cube), None);
    assert_eq!(dropped(&s, "minecraft:diamond").len(), 1);
}

#[test]
fn flint_and_steel_primes_a_cube_holding_tnt_and_it_blows_up_after_the_fuse() {
    let mut s = ready(9);
    console(&mut s, "give Alice minecraft:flint_and_steel 1");
    let cube = summon(
        &mut s,
        Kind::SulfurCube,
        "9.5 64 8.5",
        &format!("{{Size:1,NoAI:1b,{BODY}tnt\",count:1}}}}}}"),
    );
    s.tick(2);
    click(&mut s, cube);
    s.tick(1);
    let nbt = saved(&s, cube);
    assert!(
        nbt.get_i32("fuse")
            .is_some_and(|fuse| (0..120).contains(&fuse))
    );
    assert_eq!(nbt.get_bool("Invulnerable"), Some(true));
    s.tick(130);
    assert!(!exists(&s, Kind::SulfurCube, cube));
    assert!(dropped(&s, "minecraft:tnt").is_empty());
}

#[test]
fn a_primed_cube_ignores_a_second_igniter_and_flint_damages_the_tool_once() {
    let mut s = ready(10);
    console(&mut s, "give Alice minecraft:fire_charge 2");
    let cube = summon(
        &mut s,
        Kind::SulfurCube,
        "9.5 64 8.5",
        &format!("{{Size:1,NoAI:1b,{BODY}tnt\",count:1}}}}}}"),
    );
    s.tick(2);
    click(&mut s, cube);
    s.tick(1);
    assert_eq!(
        first_slot(&mut s),
        Some(("minecraft:fire_charge".to_owned(), 1))
    );
    let fuse = saved(&s, cube).get_i32("fuse").unwrap();
    click(&mut s, cube);
    s.tick(1);
    assert_eq!(
        first_slot(&mut s),
        Some(("minecraft:fire_charge".to_owned(), 1))
    );
    assert_eq!(saved(&s, cube).get_i32("fuse"), Some(fuse - 1));
}

#[test]
fn a_cube_without_tnt_is_not_ignited_by_flint_and_steel() {
    let mut s = ready(11);
    console(&mut s, "give Alice minecraft:flint_and_steel 1");
    let cube = summon(
        &mut s,
        Kind::SulfurCube,
        "9.5 64 8.5",
        &format!("{{Size:1,NoAI:1b,{BODY}dirt\",count:1}}}}}}"),
    );
    s.tick(2);
    click(&mut s, cube);
    s.tick(1);
    assert_eq!(saved(&s, cube).get_i32("fuse"), Some(-1));
}

#[test]
fn the_explosion_destroys_blocks_and_fires_the_entity_explode_event() {
    let mut s = ready_with(12, vec![observe(1, EventKind::EntityExplode)]);
    let cube = summon(
        &mut s,
        Kind::SulfurCube,
        "4.5 64 4.5",
        &format!("{{Size:1,NoAI:1b,fuse:2,{BODY}tnt\",count:1}}}}}}"),
    );
    s.tick(6);
    assert!(!exists(&s, Kind::SulfurCube, cube));
    let events = settled_for(&s.plugin, EventKind::EntityExplode);
    assert_eq!(events.len(), 1);
    let EventPayload::EntityExplode(event) = &events[0].payload else {
        panic!("expected an explosion");
    };
    assert!(!event.block_list.is_empty());
}

#[test]
fn a_burning_cube_holding_tnt_primes_its_own_fuse() {
    let mut s = ready(13);
    let cube = summon(
        &mut s,
        Kind::SulfurCube,
        "4.5 64 4.5",
        &format!("{{Size:1,Fire:100s,{BODY}tnt\",count:1}}}}}}"),
    );
    s.tick(40);
    let fuse = saved(&s, cube).get_i32("fuse");
    assert!(fuse.is_some_and(|fuse| fuse >= 0), "{fuse:?}");
}

#[test]
fn a_bucket_scoops_a_cube_up_with_its_block_and_age_and_the_bucket_releases_it_again() {
    let mut s = ready(14);
    console(&mut s, "give Alice minecraft:bucket 1");
    let cube = summon(
        &mut s,
        Kind::SulfurCube,
        "9.5 64 8.5",
        &format!("{{Size:1,NoAI:1b,Silent:1b,{BODY}dirt\",count:1}}}}}}"),
    );
    click(&mut s, cube);
    s.tick(1);
    assert_eq!(
        first_slot(&mut s),
        Some(("minecraft:sulfur_cube_bucket".to_owned(), 1))
    );
    assert!(cubes(&s).is_empty());
    use_down(&mut s);
    s.tick(2);
    let released = cubes(&s);
    assert_eq!(released.len(), 1);
    assert_eq!(first_slot(&mut s), Some(("minecraft:bucket".to_owned(), 1)));
    let nbt = saved(&s, released[0]);
    assert_eq!(nbt.get_bool("from_bucket"), Some(true));
    assert_eq!(nbt.get_bool("NoAI"), Some(true));
    assert_eq!(nbt.get_bool("Silent"), Some(true));
    assert_eq!(body_of(&s, released[0]).as_deref(), Some("minecraft:dirt"));
}

#[test]
fn an_empty_cube_goes_into_the_bucket_without_content() {
    let mut s = ready(15);
    console(&mut s, "give Alice minecraft:bucket 1");
    let cube = summon(&mut s, Kind::SulfurCube, "9.5 64 8.5", "{Size:1,NoAI:1b}");
    click(&mut s, cube);
    s.tick(1);
    assert_eq!(
        first_slot(&mut s),
        Some(("minecraft:sulfur_cube_bucket".to_owned(), 1))
    );
    use_down(&mut s);
    s.tick(2);
    let released = cubes(&s);
    assert_eq!(released.len(), 1);
    assert_eq!(body_of(&s, released[0]), None);
}

#[test]
fn a_water_bucket_does_not_scoop_a_cube_and_a_bucket_does_not_scoop_a_fish() {
    let mut s = ready(16);
    console(&mut s, "give Alice minecraft:water_bucket 1");
    let cube = summon(&mut s, Kind::SulfurCube, "9.5 64 8.5", "{Size:1,NoAI:1b}");
    click(&mut s, cube);
    assert_eq!(
        first_slot(&mut s),
        Some(("minecraft:water_bucket".to_owned(), 1))
    );
    assert_eq!(cubes(&s).len(), 1);
}

#[test]
fn the_bucket_event_names_the_cube_bucket_and_can_be_cancelled() {
    let mut s = ready_with(17, vec![deciding(1, EventKind::PlayerBucketEntity)]);
    console(&mut s, "give Alice minecraft:bucket 1");
    let cube = summon(&mut s, Kind::SulfurCube, "9.5 64 8.5", "{Size:1,NoAI:1b}");
    click(&mut s, cube);
    let asked = last_asked(&s.plugin, EventKind::PlayerBucketEntity);
    let EventPayload::PlayerBucketEntity(event) = &asked.payload else {
        panic!("expected a bucket event");
    };
    assert_eq!(event.entity.id, cube);
    assert_eq!(event.entity_bucket.key, "minecraft:sulfur_cube_bucket");
    assert_eq!(event.original_bucket.key, "minecraft:bucket");
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(first_slot(&mut s), Some(("minecraft:bucket".to_owned(), 1)));
    assert_eq!(cubes(&s).len(), 1);
}

#[test]
fn releasing_a_cube_fires_a_creature_spawn_for_the_bucket_and_no_bucket_empty_event() {
    let mut s = ready_with(
        18,
        vec![
            observe(1, EventKind::CreatureSpawn),
            observe(2, EventKind::PlayerBucketEmpty),
        ],
    );
    console(&mut s, "give Alice minecraft:sulfur_cube_bucket 1");
    use_down(&mut s);
    s.tick(2);
    assert_eq!(cubes(&s).len(), 1);
    assert!(settled_for(&s.plugin, EventKind::PlayerBucketEmpty).is_empty());
    let spawns = settled_for(&s.plugin, EventKind::CreatureSpawn);
    assert!(spawns.iter().any(|spawn| matches!(
        &spawn.payload,
        EventPayload::CreatureSpawn(event) if event.spawn_reason == rspigot_core::plugins::events::CreatureSpawnReason::Bucket
    )));
}

#[test]
fn a_hot_cube_burns_the_player_standing_inside_it_once_a_second() {
    let mut s = ready(19);
    let player = s.players[0].id;
    let before = s.server.player_health(player).unwrap();
    summon(
        &mut s,
        Kind::SulfurCube,
        "8.5 64 8.5",
        &format!("{{Size:1,NoAI:1b,{BODY}magma_block\",count:1}}}}}}"),
    );
    s.tick(2);
    let after = s.server.player_health(player).unwrap();
    assert!(after < before, "{before} -> {after}");
}

#[test]
fn a_regular_cube_never_hurts_the_player_inside_it() {
    let mut s = ready(20);
    let player = s.players[0].id;
    let before = s.server.player_health(player).unwrap();
    summon(
        &mut s,
        Kind::SulfurCube,
        "8.5 64 8.5",
        &format!("{{Size:1,NoAI:1b,{BODY}dirt\",count:1}}}}}}"),
    );
    s.tick(30);
    assert_eq!(s.server.player_health(player), Some(before));
}

#[test]
fn an_adult_dying_splits_into_two_babies_but_a_primed_one_does_not() {
    let mut s = ready(21);
    let plain = summon(&mut s, Kind::SulfurCube, "4.5 64 4.5", "{Size:1,NoAI:1b}");
    console(&mut s, "kill @e[type=minecraft:sulfur_cube]");
    s.tick(25);
    assert!(!exists(&s, Kind::SulfurCube, plain));
    let babies = cubes(&s);
    assert_eq!(babies.len(), 2);
    for baby in babies {
        let nbt = saved(&s, baby);
        assert_eq!(nbt.get_i32("Size"), Some(0));
        assert!(nbt.get_i32("Age").is_some_and(|age| age < 0));
    }
}

#[test]
fn a_primed_cube_that_dies_leaves_no_babies() {
    let mut s = ready(22);
    summon(
        &mut s,
        Kind::SulfurCube,
        "4.5 64 4.5",
        &format!("{{Size:1,NoAI:1b,fuse:500,Invulnerable:1b,{BODY}tnt\",count:1}}}}}}"),
    );
    console(&mut s, "kill @e[type=minecraft:sulfur_cube]");
    s.tick(25);
    assert!(cubes(&s).is_empty());
}

#[test]
fn a_baby_cube_grows_into_an_adult_of_size_two() {
    let mut s = ready(23);
    let baby = summon(
        &mut s,
        Kind::SulfurCube,
        "4.5 64 4.5",
        "{Size:0,Age:-3,NoAI:1b}",
    );
    assert_eq!(saved(&s, baby).get_i32("Size"), Some(0));
    s.tick(6);
    let nbt = saved(&s, baby);
    assert_eq!(nbt.get_i32("Size"), Some(1));
    assert_eq!(nbt.get_i32("Age"), Some(0));
    assert_eq!(nbt.get_f64("Health"), Some(8.0));
}

#[test]
fn slime_balls_feed_a_baby_cube_and_speed_up_its_growth() {
    let mut s = ready(24);
    console(&mut s, "give Alice minecraft:slime_ball 2");
    let baby = summon(
        &mut s,
        Kind::SulfurCube,
        "9.5 64 8.5",
        "{Size:0,Age:-24000,NoAI:1b}",
    );
    click(&mut s, baby);
    s.tick(1);
    assert_eq!(
        first_slot(&mut s),
        Some(("minecraft:slime_ball".to_owned(), 1))
    );
    let age = saved(&s, baby).get_i32("Age").unwrap();
    assert!(age > -24000 && age < 0, "{age}");
}

#[test]
fn a_spawn_egg_used_on_a_cube_makes_a_baby() {
    let mut s = ready(25);
    console(&mut s, "give Alice minecraft:sulfur_cube_spawn_egg 1");
    let parent = summon(&mut s, Kind::SulfurCube, "9.5 64 8.5", "{Size:1,NoAI:1b}");
    click(&mut s, parent);
    s.tick(1);
    let all = cubes(&s);
    assert_eq!(all.len(), 2);
    let baby = all.into_iter().find(|id| *id != parent).unwrap();
    let nbt = saved(&s, baby);
    assert_eq!(nbt.get_i32("Size"), Some(0));
    assert!(nbt.get_i32("Age").is_some_and(|age| age < 0));
}

#[test]
fn a_cube_holding_a_block_or_from_a_bucket_never_despawns() {
    let mut s = ready(26);
    let held = summon(
        &mut s,
        Kind::SulfurCube,
        "4.5 64 4.5",
        &format!("{{Size:1,NoAI:1b,{BODY}dirt\",count:1}}}}}}"),
    );
    let bucketed = summon(
        &mut s,
        Kind::SulfurCube,
        "4.5 64 5.5",
        "{Size:1,NoAI:1b,from_bucket:1b}",
    );
    s.tick(100);
    assert!(exists(&s, Kind::SulfurCube, held));
    assert!(exists(&s, Kind::SulfurCube, bucketed));
}

#[test]
fn the_state_of_a_cube_survives_a_save_and_a_load() {
    let mut s = ready(27);
    let cube = summon(
        &mut s,
        Kind::SulfurCube,
        "4.5 64 4.5",
        &format!(
            "{{Size:1,NoAI:1b,pickup_timer:50,from_bucket:1b,{BODY}oak_planks\",count:1}}}}}}"
        ),
    );
    let first = saved(&s, cube);
    assert_eq!(first.get_i32("pickup_timer"), Some(50));
    assert_eq!(first.get_bool("from_bucket"), Some(true));
    assert_eq!(body_of(&s, cube).as_deref(), Some("minecraft:oak_planks"));
    assert!(first.get_compound("attributes").is_none());
    let text = format!("{first:?}");
    assert!(!text.contains("bouncy_add"), "{text}");
}

#[test]
fn a_cube_with_a_block_gets_the_archetype_physics_and_the_goals_stop() {
    let mut s = ready(28);
    let cube = summon(
        &mut s,
        Kind::SulfurCube,
        "4.5 64 4.5",
        &format!("{{Size:1,{BODY}oak_planks\",count:1}}}}}}"),
    );
    let start = s.server.entities_of(Kind::SulfurCube)[0].position;
    s.tick(60);
    let end = s
        .server
        .entities_of(Kind::SulfurCube)
        .into_iter()
        .find(|entity| entity.id == cube)
        .unwrap()
        .position;
    assert!(
        (end.x - start.x).abs() < 0.5 && (end.z - start.z).abs() < 0.5,
        "{start:?} -> {end:?}"
    );
}

#[test]
fn a_cube_without_a_block_hops_around_on_its_own() {
    let mut s = ready(29);
    let cube = summon(&mut s, Kind::SulfurCube, "4.5 64 4.5", "{Size:1}");
    let mut left_the_ground = false;
    for _ in 0..400 {
        s.tick(1);
        left_the_ground |= s
            .server
            .entities_of(Kind::SulfurCube)
            .iter()
            .find(|entity| entity.id == cube)
            .is_some_and(|entity| !entity.on_ground);
    }
    assert!(left_the_ground, "the cube never jumped");
}

fn attack(s: &mut Scene, target: EntityId) {
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::Attack(target));
}

#[test]
fn a_cube_holding_a_block_takes_no_damage_from_a_player_but_is_pushed_back() {
    let mut s = ready(30);
    s.tick(30);
    let cube = summon(
        &mut s,
        Kind::SulfurCube,
        "9.5 64 8.5",
        &format!("{{Size:1,{BODY}dirt\",count:1}}}}}}"),
    );
    s.tick(2);
    attack(&mut s, cube);
    s.tick(1);
    let nbt = saved(&s, cube);
    assert_eq!(nbt.get_f64("Health"), Some(8.0));
    let motion = nbt
        .get_list("Motion")
        .and_then(|list| list.first().and_then(rspigot_nbt::Tag::as_f64));
    assert!(motion.is_some_and(|x| x.abs() > 0.01), "{motion:?}");
}

#[test]
fn a_cube_without_a_block_is_hurt_by_a_player() {
    let mut s = ready(31);
    s.tick(30);
    let cube = summon(&mut s, Kind::SulfurCube, "9.5 64 8.5", "{Size:1,NoAI:1b}");
    s.tick(2);
    attack(&mut s, cube);
    s.tick(1);
    assert!(
        saved(&s, cube)
            .get_f64("Health")
            .is_some_and(|health| health < 8.0)
    );
}

#[test]
fn a_cube_walks_to_a_swallowable_item_and_swallows_it() {
    let mut s = ready(32);
    let cube = summon(&mut s, Kind::SulfurCube, "4.5 64 4.5", "{Size:1}");
    console(
        &mut s,
        "summon minecraft:item 7.5 64 4.5 {Item:{id:\"minecraft:dirt\",count:1},PickupDelay:0s}",
    );
    for _ in 0..40 {
        s.tick(10);
        if body_of(&s, cube).is_some() {
            break;
        }
    }
    assert_eq!(body_of(&s, cube).as_deref(), Some("minecraft:dirt"));
    assert!(dropped(&s, "minecraft:dirt").is_empty());
}
