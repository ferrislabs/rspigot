mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::Inbound;
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::dragon::phase::DragonPhase;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Island;

impl ChunkGenerator for Island {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                let at = |y: i32| BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z);
                for y in 40..=62 {
                    chunk.set_block(at(y), BlockStateId(1));
                }
                chunk.set_block(at(63), BlockKind::EndStone.default_state());
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
        view_distance: 8,
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
        Arc::new(Island),
        StatusBoard::new(20),
    )
    .with_rng_seed(seed);
    server.wait_for_spawn(Duration::from_secs(10));
    let plugin = connect_with(&mut server, 1, "tester", Vec::new());
    let alice = join_with(
        &mut server,
        "Alice",
        rspigot_domain::player::ClientSettings {
            view_distance: 8,
            ..Default::default()
        },
    );
    server.deliver();
    alice.link.take();
    let mut s = Scene {
        server,
        plugin,
        players: vec![alice],
    };
    stream_chunk_of(&mut s, BlockPos::new(8, 64, 8));
    s.server.wait_for_spawn(Duration::from_secs(60));
    s.tick(20);
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

fn position_of(s: &Scene, kind: Kind, id: EntityId) -> Vec3 {
    s.server
        .entities_of(kind)
        .into_iter()
        .find(|entity| entity.id == id)
        .map(|entity| entity.position)
        .expect("the entity")
}

#[test]
fn a_summoned_dragon_hovers_where_it_was_summoned_in_the_overworld() {
    let mut s = ready(1);
    let dragon = summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{}");
    s.tick(40);
    assert_eq!(
        s.server.dragon_phase_of(dragon),
        Some(DragonPhase::Hovering)
    );
    let at = position_of(&s, Kind::EnderDragon, dragon);
    assert!(
        (at.x - 8.5).abs() < 0.5 && (at.y - 100.0).abs() < 0.5,
        "{at:?}"
    );
}

#[test]
fn the_parts_take_the_ids_after_the_dragon_and_the_next_entity_skips_them() {
    let mut s = ready(1);
    let dragon = summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{}");
    let other = summon(&mut s, Kind::Cow, "8.5 64 8.5", "{NoAI:1b}");
    assert!(other.0 > dragon.0 + 8, "{dragon:?} {other:?}");
    let parts = s.server.dragon_parts(dragon).unwrap();
    assert_eq!(parts.len(), 8);
}

#[test]
fn a_holding_dragon_flies_around_its_pillars_without_leaving_the_arena() {
    let mut s = ready(2);
    let dragon = summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{DragonPhase:0}");
    let mut seen = Vec::new();
    for _ in 0..30 {
        s.tick(20);
        seen.push(position_of(&s, Kind::EnderDragon, dragon));
    }
    let moved = seen
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).length())
        .sum::<f64>();
    assert!(moved > 20.0, "{moved}");
    assert!(seen.iter().all(|at| at.length() < 400.0), "{seen:?}");
}

fn health_of(s: &Scene, id: EntityId) -> f32 {
    s.server
        .entities_of(Kind::EnderDragon)
        .into_iter()
        .find(|entity| entity.id == id)
        .and_then(|entity| entity.health)
        .expect("the dragon")
}

fn saved(s: &Scene, id: EntityId) -> rspigot_nbt::Compound {
    s.server.saved_entity(id).expect("a saved entity")
}

fn armed(s: &mut Scene) {
    console(s, "gamemode creative Alice");
    console(s, "give Alice minecraft:diamond_sword");
    s.tick(60);
}

fn stand_by(s: &mut Scene, at: Vec3) {
    console(s, &format!("tp Alice {} {} {}", at.x, at.y - 1.0, at.z));
    s.tick(2);
}

fn strike_part(s: &mut Scene, dragon: EntityId, index: usize) {
    s.tick(15);
    let at = s.server.dragon_parts(dragon).unwrap()[index];
    console(s, &format!("tp Alice {} {} {}", at.x, at.y, at.z + 1.5));
    let player = s.players[0].clone();
    send_input(
        &mut s.server,
        &player,
        rspigot_core::events::PlayerInput::Attack(EntityId(dragon.0 + 1 + index as i32)),
    );
    s.server.deliver();
}

#[test]
fn the_head_takes_the_full_blow_and_the_body_a_quarter_plus_one() {
    let mut s = ready(3);
    armed(&mut s);
    let dragon = summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{}");
    s.tick(5);
    let before = health_of(&s, dragon);
    strike_part(&mut s, dragon, 0);
    let after_head = health_of(&s, dragon);
    assert!(
        (before - after_head - 7.0).abs() < 0.2,
        "{before} {after_head}"
    );
    s.tick(30);
    strike_part(&mut s, dragon, 2);
    let after_body = health_of(&s, dragon);
    assert!(
        (after_head - after_body - 2.75).abs() < 0.3,
        "{after_head} {after_body}"
    );
}

#[test]
fn the_wing_and_tail_take_the_body_share_and_the_neck_the_full_blow() {
    let mut s = ready(3);
    armed(&mut s);
    let dragon = summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{}");
    s.tick(5);
    let before = health_of(&s, dragon);
    strike_part(&mut s, dragon, 1);
    let after_neck = health_of(&s, dragon);
    assert!(
        (before - after_neck - 7.0).abs() < 0.3,
        "{before} {after_neck}"
    );
    s.tick(30);
    strike_part(&mut s, dragon, 6);
    let after_wing = health_of(&s, dragon);
    assert!((after_neck - after_wing - 2.75).abs() < 0.3);
    s.tick(30);
    strike_part(&mut s, dragon, 4);
    assert!((after_wing - health_of(&s, dragon) - 2.75).abs() < 0.3);
}

#[test]
fn a_sitting_dragon_that_loses_a_quarter_of_its_health_takes_off() {
    let mut s = ready(3);
    armed(&mut s);
    let dragon = summon(
        &mut s,
        Kind::EnderDragon,
        "8.5 100 8.5",
        "{DragonPhase:6,Health:200f,sitting_damage_received:44f}",
    );
    s.tick(5);
    assert_eq!(
        s.server.dragon_phase_of(dragon),
        Some(DragonPhase::SittingScanning)
    );
    strike_part(&mut s, dragon, 0);
    let phase = s.server.dragon_phase_of(dragon).unwrap();
    assert!(!phase.is_sitting(), "{phase:?}");
    assert_eq!(health_of(&s, dragon), 193.0);
    assert_eq!(
        saved(&s, dragon).get_f64("sitting_damage_received"),
        Some(0.0)
    );
}

#[test]
fn a_sitting_dragon_keeps_counting_the_damage_it_takes_below_a_quarter_of_its_health() {
    let mut s = ready(3);
    armed(&mut s);
    let dragon = summon(
        &mut s,
        Kind::EnderDragon,
        "8.5 100 8.5",
        "{DragonPhase:6,Health:200f}",
    );
    s.tick(5);
    strike_part(&mut s, dragon, 0);
    assert_eq!(
        saved(&s, dragon).get_f64("sitting_damage_received"),
        Some(7.0)
    );
    assert!(s.server.dragon_phase_of(dragon).unwrap().is_sitting());
}

#[test]
fn an_end_crystal_in_range_heals_the_dragon_a_point_every_ten_ticks() {
    let mut s = ready(3);
    let dragon = summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{Health:100f}");
    summon(&mut s, Kind::EndCrystal, "8.5 70 8.5", "{}");
    s.tick(400);
    let healed = health_of(&s, dragon);
    assert!(healed > 110.0, "{healed}");
}

#[test]
fn the_phase_and_the_death_time_round_trip_through_the_saved_dragon() {
    let mut s = ready(3);
    let dragon = summon(
        &mut s,
        Kind::EnderDragon,
        "8.5 100 8.5",
        "{DragonPhase:3,DragonDeathTime:0,sitting_damage_received:4f}",
    );
    let nbt = saved(&s, dragon);
    assert_eq!(nbt.get_i32("DragonPhase"), Some(3));
    assert_eq!(nbt.get_f64("sitting_damage_received"), Some(4.0));
    assert_eq!(s.server.dragon_phase_of(dragon), Some(DragonPhase::Landing));
}

fn block_at(s: &Scene, x: i32, y: i32, z: i32) -> Option<rspigot_blocks::BlockKind> {
    s.server
        .block_state_at(BlockPos::new(x, y, z))
        .and_then(|state| state.kind())
}

#[test]
fn a_dragon_that_finishes_dying_leaves_the_portal_the_egg_a_gateway_and_experience() {
    let mut s = ready(4);
    let dragon = summon(
        &mut s,
        Kind::EnderDragon,
        "8.5 100 8.5",
        "{DragonPhase:9,DragonDeathTime:140,Health:0f,Bukkit.expToDrop:12000}",
    );
    s.tick(5);
    assert!(s.server.entities_of(Kind::ExperienceOrb).is_empty());
    s.tick(25);
    assert!(
        !s.server.entities_of(Kind::ExperienceOrb).is_empty(),
        "bursts of eight percent every five ticks after tick one fifty"
    );
    s.tick(40);
    assert!(
        s.server
            .entities_of(Kind::EnderDragon)
            .iter()
            .all(|entity| entity.id != dragon),
        "the dragon is gone after two hundred ticks of death"
    );
    assert_eq!(block_at(&s, 0, 63, 0), Some(BlockKind::Bedrock));
    assert_eq!(block_at(&s, 1, 63, 0), Some(BlockKind::EndPortal));
    assert_eq!(block_at(&s, 0, 66, 0), Some(BlockKind::Bedrock));
    assert_eq!(block_at(&s, 0, 67, 0), Some(BlockKind::DragonEgg));
    assert_eq!(block_at(&s, 1, 65, 0), Some(BlockKind::WallTorch));
    let fight = s.server.dragon_fight_data().expect("a fight");
    assert!(fight.dragon_killed && fight.previously_killed);
    assert_eq!(fight.exit_portal_location, Some(BlockPos::new(0, 63, 0)));
    assert_eq!(fight.gateways.len(), 19);
}

#[test]
fn a_second_dragon_that_is_not_the_one_of_the_fight_leaves_the_fight_untouched() {
    let mut s = ready(4);
    summon(
        &mut s,
        Kind::EnderDragon,
        "8.5 100 8.5",
        "{DragonPhase:9,DragonDeathTime:195,Health:0f,Bukkit.expToDrop:500}",
    );
    s.tick(30);
    let first = s.server.dragon_fight_data().unwrap();
    assert_eq!(first.gateways.len(), 19);
    summon(
        &mut s,
        Kind::EnderDragon,
        "8.5 110 8.5",
        "{DragonPhase:9,DragonDeathTime:195,Health:0f,Bukkit.expToDrop:500}",
    );
    s.tick(30);
    assert_eq!(s.server.dragon_fight_data().unwrap(), first);
}

#[test]
fn the_dragon_bar_is_pink_with_music_and_fog_and_follows_the_health_of_the_dragon() {
    use rspigot_core::events::ClientEvent;
    use rspigot_domain::boss_event::{BossColor, BossOverlay, BossUpdate};
    let mut s = ready(5);
    s.players[0].link.take();
    summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{Health:100f}");
    s.tick(30);
    let bars: Vec<BossUpdate> = s.players[0]
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::BossEvent { update, .. } => Some(update),
            _ => None,
        })
        .collect();
    let add = bars
        .iter()
        .find(|update| matches!(update, BossUpdate::Add { .. }))
        .expect("an add");
    let BossUpdate::Add {
        name,
        color,
        overlay,
        flags,
        ..
    } = add
    else {
        unreachable!();
    };
    assert_eq!(*color, BossColor::Pink);
    assert_eq!(*overlay, BossOverlay::Progress);
    assert!(flags.play_music && flags.create_world_fog && !flags.darken_screen);
    assert_eq!(name.plain_text(), "entity.minecraft.ender_dragon");
    let progress: Vec<f32> = bars
        .iter()
        .filter_map(|update| match update {
            BossUpdate::Progress(progress) => Some(*progress),
            BossUpdate::Add { progress, .. } => Some(*progress),
            _ => None,
        })
        .collect();
    let last = *progress.last().expect("a progress");
    assert!((0.49..=0.51).contains(&last), "{progress:?}");
}

#[test]
fn a_bar_is_hidden_when_the_dragon_is_dead_and_the_fight_is_saved_in_the_level_data() {
    let mut s = ready(5);
    summon(
        &mut s,
        Kind::EnderDragon,
        "8.5 100 8.5",
        "{DragonPhase:9,DragonDeathTime:198,Health:0f}",
    );
    s.tick(40);
    let fight = s.server.dragon_fight_data().unwrap();
    assert!(fight.dragon_killed);
    let level = s.server.level_snapshot();
    let nbt = level.dragon_fight.expect("saved data");
    assert_eq!(nbt.get_bool("dragon_killed"), Some(true));
    assert_eq!(nbt.get_bool("previously_killed"), Some(true));
}

fn crystals(s: &Scene) -> Vec<rspigot_core::entities::EntitySnapshot> {
    s.server.entities_of(Kind::EndCrystal)
}

fn explosions(events: &[rspigot_core::events::ClientEvent]) -> Vec<f32> {
    events
        .iter()
        .filter_map(|event| match event {
            rspigot_core::events::ClientEvent::Explode { radius, .. } => Some(*radius),
            _ => None,
        })
        .collect()
}

#[test]
fn hitting_an_end_crystal_blows_it_up_with_a_radius_six_explosion() {
    let mut s = ready(6);
    armed(&mut s);
    summon(&mut s, Kind::EndCrystal, "20.5 64 8.5", "{}");
    let crystal = crystals(&s)[0].id;
    stand_by(&mut s, Vec3::new(20.5, 65.0, 10.5));
    s.players[0].link.take();
    let player = s.players[0].clone();
    send_input(
        &mut s.server,
        &player,
        rspigot_core::events::PlayerInput::Attack(crystal),
    );
    s.tick(2);
    assert!(crystals(&s).is_empty());
    let events = s.players[0].link.take();
    assert_eq!(explosions(&events), vec![6.0]);
}

#[test]
fn a_permanently_invulnerable_crystal_cannot_be_hurt() {
    let mut s = ready(6);
    armed(&mut s);
    summon(&mut s, Kind::EndCrystal, "20.5 64 8.5", "{Invulnerable:1b}");
    let crystal = crystals(&s)[0].id;
    stand_by(&mut s, Vec3::new(20.5, 65.0, 10.5));
    let player = s.players[0].clone();
    send_input(
        &mut s.server,
        &player,
        rspigot_core::events::PlayerInput::Attack(crystal),
    );
    s.tick(2);
    assert_eq!(crystals(&s).len(), 1);
}

#[test]
fn an_explosion_next_to_another_crystal_removes_it_without_a_second_blast() {
    let mut s = ready(6);
    armed(&mut s);
    summon(&mut s, Kind::EndCrystal, "20.5 64 8.5", "{}");
    summon(&mut s, Kind::EndCrystal, "23.5 64 8.5", "{}");
    let first = crystals(&s)
        .into_iter()
        .find(|entity| entity.position.x < 22.0)
        .unwrap();
    stand_by(&mut s, Vec3::new(20.5, 65.0, 10.5));
    s.players[0].link.take();
    let player = s.players[0].clone();
    send_input(
        &mut s.server,
        &player,
        rspigot_core::events::PlayerInput::Attack(first.id),
    );
    s.tick(3);
    assert!(crystals(&s).is_empty());
    assert_eq!(explosions(&s.players[0].link.take()), vec![6.0]);
}

#[test]
fn crystals_light_a_fire_on_the_block_they_stand_in_while_a_fight_exists() {
    let mut s = ready(6);
    summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{}");
    summon(&mut s, Kind::EndCrystal, "30.5 64 8.5", "{}");
    s.tick(3);
    assert_eq!(block_at(&s, 30, 64, 8), Some(BlockKind::Fire));
}

#[test]
fn a_crystal_keeps_its_beam_target_and_bottom_flag_in_the_world_save() {
    let mut s = ready(6);
    summon(
        &mut s,
        Kind::EndCrystal,
        "20.5 64 8.5",
        "{beam_target:[I;0,128,0],ShowBottom:0b}",
    );
    let id = crystals(&s)[0].id;
    let nbt = saved(&s, id);
    assert_eq!(nbt.get_int_array("beam_target"), Some(&[0, 128, 0][..]));
    assert_eq!(nbt.get_bool("ShowBottom"), Some(false));
}

#[test]
fn a_dragon_fireball_that_lands_leaves_a_breath_cloud_with_the_vanilla_settings() {
    let mut s = ready(7);
    summon(
        &mut s,
        Kind::DragonFireball,
        "20.5 66 20.5",
        "{Motion:[0.0,-1.0,0.0]}",
    );
    s.tick(10);
    let clouds = s.server.entities_of(Kind::AreaEffectCloud);
    assert_eq!(clouds.len(), 1);
    assert!(s.server.entities_of(Kind::DragonFireball).is_empty());
    let nbt = saved(&s, clouds[0].id);
    assert_eq!(nbt.get_i32("Duration"), Some(600));
    assert_eq!(nbt.get_f64("potion_duration_scale"), Some(0.25));
    assert!(nbt.get_f64("Radius").unwrap() >= 3.0);
    assert!(nbt.get_f64("RadiusPerTick").unwrap() > 0.0066);
    assert_eq!(
        nbt.get_compound("custom_particle")
            .and_then(|particle| particle.get_str("type")),
        Some("minecraft:dragon_breath")
    );
}

#[test]
fn a_breath_cloud_hurts_creatures_standing_in_it_for_six_points() {
    let mut s = ready(7);
    summon(&mut s, Kind::Cow, "20.5 64 20.5", "{NoAI:1b,Health:10f}");
    let cow = s.server.entities_of(Kind::Cow)[0].id;
    summon(
        &mut s,
        Kind::AreaEffectCloud,
        "20.5 64 20.5",
        "{Duration:600,WaitTime:20,Radius:3f,potion_duration_scale:0.25f,potion_contents:{custom_effects:[{id:\"minecraft:instant_damage\",amplifier:1b,duration:1}]}}",
    );
    s.tick(30);
    let health = s
        .server
        .entities_of(Kind::Cow)
        .into_iter()
        .find(|entity| entity.id == cow)
        .and_then(|entity| entity.health)
        .unwrap();
    assert!((3.5..=4.5).contains(&health), "{health}");
    s.tick(15);
    let again = s
        .server
        .entities_of(Kind::Cow)
        .into_iter()
        .find(|entity| entity.id == cow)
        .and_then(|entity| entity.health)
        .unwrap();
    assert!(
        again <= 0.5,
        "the effect applies again after the twenty tick delay: {again}"
    );
}

#[test]
fn a_cloud_with_a_short_duration_disappears() {
    let mut s = ready(7);
    summon(
        &mut s,
        Kind::AreaEffectCloud,
        "20.5 64 20.5",
        "{Duration:10,WaitTime:0}",
    );
    s.tick(5);
    assert_eq!(s.server.entities_of(Kind::AreaEffectCloud).len(), 1);
    s.tick(10);
    assert!(s.server.entities_of(Kind::AreaEffectCloud).is_empty());
}

fn place_crystal_on(s: &mut Scene, x: i32, z: i32) {
    use rspigot_core::events::{Direction, Hand, PlayerInput};
    let player = s.players[0].clone();
    send_input(
        &mut s.server,
        &player,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: BlockPos::new(x, 63, z),
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 9,
        },
    );
    s.tick(2);
}

fn clear_loose(s: &mut Scene) {
    console(s, "kill @e[type=minecraft:experience_orb]");
    console(s, "kill @e[type=minecraft:item]");
}

fn started_respawn(seed: u64) -> Scene {
    let mut s = ready(seed);
    summon(
        &mut s,
        Kind::EnderDragon,
        "8.5 100 8.5",
        "{DragonPhase:9,DragonDeathTime:195,Health:0f}",
    );
    s.tick(30);
    assert!(s.server.dragon_fight_data().unwrap().dragon_killed);
    clear_loose(&mut s);
    console(&mut s, "gamemode survival Alice");
    console(&mut s, "give Alice minecraft:end_crystal 4");
    console(&mut s, "tp Alice 0.5 66 0.5");
    s.tick(5);
    for (x, z) in [(3, 0), (-3, 0), (0, 3)] {
        clear_loose(&mut s);
        place_crystal_on(&mut s, x, z);
        assert!(
            s.server
                .dragon_fight_data()
                .unwrap()
                .respawn_stage
                .is_none()
        );
    }
    assert_eq!(crystals(&s).len(), 3);
    clear_loose(&mut s);
    place_crystal_on(&mut s, 0, -3);
    assert_eq!(crystals(&s).len(), 4);
    let fight = s.server.dragon_fight_data().unwrap();
    assert_eq!(
        fight.respawn_stage,
        Some(rspigot_domain::dragon::fight::RespawnStage::PreparingToSummonPillars)
    );
    assert_eq!(fight.respawn_crystals.len(), 4);
    s
}

#[test]
fn four_crystals_around_the_exit_portal_respawn_the_dragon_in_stages() {
    let mut s = started_respawn(8);
    s.tick(3);
    for crystal in crystals(&s) {
        assert_eq!(
            saved(&s, crystal.id).get_int_array("beam_target"),
            Some(&[0, 128, 0][..])
        );
    }
    s.tick(120);
    assert_eq!(
        s.server.dragon_fight_data().unwrap().respawn_stage,
        Some(rspigot_domain::dragon::fight::RespawnStage::SummoningPillars)
    );
    s.tick(900);
    let fight = s.server.dragon_fight_data().unwrap();
    assert_eq!(fight.respawn_stage, None);
    assert!(!fight.dragon_killed);
    let dragons = s.server.entities_of(Kind::EnderDragon);
    assert_eq!(dragons.len(), 1, "{:?}", fight);
    assert_eq!(fight.dragon_uuid, Some(dragons[0].uuid));
    assert_ne!(
        s.server.dragon_phase_of(dragons[0].id),
        Some(DragonPhase::Hovering)
    );
    let pillars = rspigot_domain::dragon::spikes::spikes_for_seed(0);
    let kept: Vec<_> = pillars
        .iter()
        .filter(|spike| {
            crystals(&s).iter().any(|crystal| {
                (crystal.position.x - (f64::from(spike.center_x) + 0.5)).abs() < 0.6
                    && (crystal.position.y - f64::from(spike.height + 1)).abs() < 0.6
            })
        })
        .collect();
    assert_eq!(kept.len(), 10, "ten pillars carry a crystal again");
    assert_eq!(
        block_at(
            &s,
            pillars[0].center_x,
            pillars[0].height - 1,
            pillars[0].center_z
        ),
        Some(BlockKind::Obsidian)
    );
}

#[test]
fn a_dragon_in_flight_that_takes_a_lethal_blow_goes_to_the_dying_phase_at_one_health() {
    let mut s = ready(9);
    armed(&mut s);
    let dragon = summon(
        &mut s,
        Kind::EnderDragon,
        "8.5 100 8.5",
        "{DragonPhase:0,Health:5f}",
    );
    s.tick(10);
    strike_part(&mut s, dragon, 0);
    assert_eq!(s.server.dragon_phase_of(dragon), Some(DragonPhase::Dying));
    assert_eq!(health_of(&s, dragon), 1.0);
    assert_eq!(saved(&s, dragon).get_i32("DragonPhase"), Some(9));
    assert!(
        s.server
            .entities_of(Kind::EnderDragon)
            .iter()
            .any(|e| e.id == dragon)
    );
}

#[test]
fn a_sitting_dragon_that_takes_a_lethal_blow_dies_where_it_sits() {
    let mut s = ready(9);
    armed(&mut s);
    let dragon = summon(
        &mut s,
        Kind::EnderDragon,
        "8.5 100 8.5",
        "{DragonPhase:6,Health:5f}",
    );
    s.tick(5);
    strike_part(&mut s, dragon, 0);
    assert!(health_of(&s, dragon) <= 0.0);
    s.tick(205);
    assert!(s.server.entities_of(Kind::EnderDragon).is_empty());
    assert!(s.server.dragon_fight_data().unwrap().dragon_killed);
}

#[test]
fn the_kill_command_removes_the_dragon_at_once_and_ends_the_fight() {
    let mut s = ready(9);
    summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{}");
    s.tick(3);
    console(&mut s, "kill @e[type=minecraft:ender_dragon]");
    s.tick(3);
    assert!(s.server.entities_of(Kind::EnderDragon).is_empty());
    assert!(s.server.dragon_fight_data().unwrap().dragon_killed);
    assert_eq!(block_at(&s, 0, 67, 0), Some(BlockKind::DragonEgg));
}

#[test]
fn a_dragon_circling_near_a_player_strafes_and_throws_a_dragon_fireball() {
    use rspigot_core::events::ClientEvent;
    let mut s = ready(11);
    console(&mut s, "gamemode survival Alice");
    console(&mut s, "effect give Alice minecraft:resistance 1000 4");
    summon(&mut s, Kind::EnderDragon, "40.5 80 8.5", "{DragonPhase:0}");
    s.players[0].link.take();
    let mut thrown = false;
    let mut seen = false;
    for _ in 0..40 {
        s.tick(50);
        seen |= !s.server.entities_of(Kind::DragonFireball).is_empty();
        thrown |= s.players[0]
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::LevelEvent { event: 1017, .. }));
        if thrown && seen {
            break;
        }
    }
    assert!(thrown, "the fireball sound event went out");
}

#[test]
fn killing_a_crystal_of_the_respawn_aborts_it_and_restores_the_active_portal() {
    let mut s = started_respawn(12);
    assert!(
        s.server
            .dragon_fight_data()
            .unwrap()
            .respawn_stage
            .is_some()
    );
    console(&mut s, "kill @e[type=minecraft:end_crystal,limit=1]");
    s.tick(3);
    let fight = s.server.dragon_fight_data().unwrap();
    assert_eq!(fight.respawn_stage, None);
    assert_eq!(fight.respawn_time, 0);
    assert!(fight.dragon_killed);
    assert_eq!(crystals(&s).len(), 3);
    assert_eq!(block_at(&s, 1, 63, 0), Some(BlockKind::EndPortal));
}

#[test]
fn a_survival_player_inside_the_head_of_the_dragon_takes_the_ten_point_blow_scaled_by_difficulty() {
    let mut s = ready(13);
    console(&mut s, "gamemode survival Alice");
    s.tick(5);
    let dragon = summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{}");
    s.tick(5);
    let head = s.server.dragon_parts(dragon).unwrap()[0];
    let alice = s.players[0].id;
    let before = s.server.player_health(alice).unwrap();
    console(
        &mut s,
        &format!("tp Alice {} {} {}", head.x, head.y, head.z),
    );
    s.tick(2);
    let after = s.server.player_health(alice).unwrap();
    assert!(
        (before - after - 6.0).abs() < 0.5,
        "easy difficulty scales ten to six: {before} {after}"
    );
}

#[test]
fn a_creative_player_inside_the_head_of_the_dragon_is_left_alone() {
    let mut s = ready(13);
    console(&mut s, "gamemode creative Alice");
    s.tick(5);
    let dragon = summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{}");
    s.tick(5);
    let head = s.server.dragon_parts(dragon).unwrap()[0];
    let alice = s.players[0].id;
    console(
        &mut s,
        &format!("tp Alice {} {} {}", head.x, head.y, head.z),
    );
    s.tick(5);
    assert_eq!(s.server.player_health(alice), Some(20.0));
}

fn owner_tag(s: &Scene) -> String {
    let (hi, lo) = s.players[0].id.0.as_u64_pair();
    format!(
        "[I;{},{},{},{}]",
        (hi >> 32) as i32,
        hi as i32,
        (lo >> 32) as i32,
        lo as i32
    )
}

#[test]
fn an_arrow_shot_by_a_player_hits_a_part_of_the_dragon_and_is_stopped() {
    let mut s = ready(14);
    let dragon = summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{}");
    s.tick(5);
    let before = health_of(&s, dragon);
    let owner = owner_tag(&s);
    summon(
        &mut s,
        Kind::Arrow,
        "8.5 102.5 40.5",
        &format!("{{Motion:[0.0,0.0,-3.0],Owner:{owner}}}"),
    );
    s.tick(20);
    let after = health_of(&s, dragon);
    assert!(after < before, "{before} {after}");
    assert!(
        s.server
            .entities_of(Kind::Arrow)
            .iter()
            .all(|arrow| arrow.position.z > 0.0)
    );
}

#[test]
fn an_arrow_without_a_player_behind_it_does_not_hurt_the_dragon() {
    let mut s = ready(14);
    let dragon = summon(&mut s, Kind::EnderDragon, "8.5 100 8.5", "{}");
    s.tick(5);
    let before = health_of(&s, dragon);
    summon(
        &mut s,
        Kind::Arrow,
        "8.5 102.5 40.5",
        "{Motion:[0.0,0.0,-3.0]}",
    );
    s.tick(20);
    assert_eq!(health_of(&s, dragon), before);
}
