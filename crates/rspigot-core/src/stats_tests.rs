use rspigot_blocks::BlockKind;
use rspigot_data::registries::static_entries;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::EntityKind as Kind;

use super::*;
use crate::end_tests::{Tap, arrive};
use crate::mob_actions::new_mob;

const EVERY_CONSTANT: [&str; 78] = [
    custom::LEAVE_GAME,
    custom::PLAY_TIME,
    custom::TOTAL_WORLD_TIME,
    custom::TIME_SINCE_DEATH,
    custom::TIME_SINCE_REST,
    custom::SNEAK_TIME,
    custom::WALK_ONE_CM,
    custom::CROUCH_ONE_CM,
    custom::SPRINT_ONE_CM,
    custom::WALK_ON_WATER_ONE_CM,
    custom::FALL_ONE_CM,
    custom::CLIMB_ONE_CM,
    custom::FLY_ONE_CM,
    custom::WALK_UNDER_WATER_ONE_CM,
    custom::MINECART_ONE_CM,
    custom::BOAT_ONE_CM,
    custom::PIG_ONE_CM,
    custom::HAPPY_GHAST_ONE_CM,
    custom::HORSE_ONE_CM,
    custom::AVIATE_ONE_CM,
    custom::SWIM_ONE_CM,
    custom::STRIDER_ONE_CM,
    custom::NAUTILUS_ONE_CM,
    custom::JUMP,
    custom::DROP,
    custom::DAMAGE_DEALT,
    custom::DAMAGE_DEALT_ABSORBED,
    custom::DAMAGE_DEALT_RESISTED,
    custom::DAMAGE_TAKEN,
    custom::DAMAGE_BLOCKED_BY_SHIELD,
    custom::DAMAGE_ABSORBED,
    custom::DAMAGE_RESISTED,
    custom::DEATHS,
    custom::MOB_KILLS,
    custom::ANIMALS_BRED,
    custom::PLAYER_KILLS,
    custom::FISH_CAUGHT,
    custom::TALKED_TO_VILLAGER,
    custom::TRADED_WITH_VILLAGER,
    custom::EAT_CAKE_SLICE,
    custom::FILL_CAULDRON,
    custom::USE_CAULDRON,
    custom::CLEAN_ARMOR,
    custom::CLEAN_BANNER,
    custom::CLEAN_SHULKER_BOX,
    custom::INTERACT_WITH_BREWINGSTAND,
    custom::INTERACT_WITH_BEACON,
    custom::INSPECT_DROPPER,
    custom::INSPECT_HOPPER,
    custom::INSPECT_DISPENSER,
    custom::PLAY_NOTEBLOCK,
    custom::TUNE_NOTEBLOCK,
    custom::POT_FLOWER,
    custom::TRIGGER_TRAPPED_CHEST,
    custom::OPEN_ENDERCHEST,
    custom::ENCHANT_ITEM,
    custom::PLAY_RECORD,
    custom::INTERACT_WITH_FURNACE,
    custom::INTERACT_WITH_CRAFTING_TABLE,
    custom::OPEN_CHEST,
    custom::SLEEP_IN_BED,
    custom::SLEEP_IN_STRAW_BED,
    custom::OPEN_SHULKER_BOX,
    custom::OPEN_BARREL,
    custom::INTERACT_WITH_BLAST_FURNACE,
    custom::INTERACT_WITH_SMOKER,
    custom::INTERACT_WITH_LECTERN,
    custom::INTERACT_WITH_CAMPFIRE,
    custom::INTERACT_WITH_CARTOGRAPHY_TABLE,
    custom::INTERACT_WITH_LOOM,
    custom::INTERACT_WITH_STONECUTTER,
    custom::BELL_RING,
    custom::RAID_TRIGGER,
    custom::RAID_WIN,
    custom::INTERACT_WITH_ANVIL,
    custom::INTERACT_WITH_GRINDSTONE,
    custom::TARGET_HIT,
    custom::INTERACT_WITH_SMITHING_TABLE,
];

fn stat(server: &Server<Tap>, id: PlayerId, key: &str) -> i32 {
    server
        .players
        .get(&id)
        .map_or(0, |player| player.stats.get(StatKind::Custom, key))
}

fn put(server: &mut Server<Tap>, id: PlayerId, position: Vec3) {
    if let Some(player) = server.players.get_mut(&id) {
        player.position = position;
    }
}

fn water(server: &mut Server<Tap>, at: BlockPos) {
    server.set_block(at, BlockKind::Water.default_state());
}

#[test]
fn every_custom_stat_of_the_registry_has_a_constant_in_the_vanilla_order() {
    let registry = static_entries("minecraft:custom_stat").expect("the registry");
    assert_eq!(registry, EVERY_CONSTANT);
}

#[test]
fn every_custom_stat_has_a_plugin_statistic() {
    for name in EVERY_CONSTANT {
        assert!(bukkit_statistic(StatKind::Custom, name).is_some(), "{name}");
    }
}

#[test]
fn the_saved_document_and_the_packet_carry_every_new_key() {
    let mut stats = PlayerStats::default();
    for name in EVERY_CONSTANT {
        stats.add(StatKind::Custom, name, 1);
    }
    let text = stats.to_json();
    for name in EVERY_CONSTANT {
        assert!(text.contains(&format!("\"{name}\":1")), "{name}");
    }
    assert_eq!(PlayerStats::from_json(&text), stats);
    let ids: Vec<i32> = stats.entries().iter().map(|entry| entry.1).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, (0..78).collect::<Vec<i32>>());
}

#[test]
fn sprinting_in_water_counts_swim_one_cm_in_three_dimensions() {
    let (mut server, id, _) = arrive();
    let from = Vec3::new(0.5, 70.0, 0.5);
    for y in 69..=72 {
        water(&mut server, BlockPos::new(0, y, 0));
        water(&mut server, BlockPos::new(1, y, 0));
    }
    if let Some(player) = server.players.get_mut(&id) {
        player.sprinting = true;
        player.motion.swimming = true;
        player.on_ground = false;
    }
    let to = Vec3::new(1.5, 70.0, 0.5);
    put(&mut server, id, to);
    server.count_movement(id, from, to);
    assert_eq!(stat(&server, id, custom::SWIM_ONE_CM), 100);
    assert_eq!(stat(&server, id, custom::WALK_ON_WATER_ONE_CM), 0);
}

#[test]
fn gliding_counts_aviate_one_cm_instead_of_fly_one_cm() {
    let (mut server, id, _) = arrive();
    let from = Vec3::new(0.5, 100.0, 0.5);
    let to = Vec3::new(3.5, 99.0, 4.5);
    if let Some(player) = server.players.get_mut(&id) {
        player.fall_flying = true;
        player.on_ground = false;
    }
    put(&mut server, id, to);
    server.count_movement(id, from, to);
    assert_eq!(stat(&server, id, custom::AVIATE_ONE_CM), 510);
    assert_eq!(stat(&server, id, custom::FLY_ONE_CM), 0);
}

#[test]
fn riding_a_happy_ghast_or_a_nautilus_counts_their_own_distance() {
    let (mut server, id, _) = arrive();
    for (kind, key) in [
        (Kind::HappyGhast, custom::HAPPY_GHAST_ONE_CM),
        (Kind::Nautilus, custom::NAUTILUS_ONE_CM),
        (Kind::ZombieNautilus, custom::NAUTILUS_ONE_CM),
    ] {
        let mut base = server.new_entity_base(kind);
        base.position = Vec3::new(0.5, 80.0, 0.5);
        let vehicle = server.place_entity(new_mob(base)).expect("placed");
        let before = stat(&server, id, key);
        server.count_vehicle_movement(id, vehicle, Vec3::new(2.0, 0.0, 0.0));
        assert_eq!(stat(&server, id, key), before + 200, "{kind:?}");
    }
}
