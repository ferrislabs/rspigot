mod common;

use std::sync::Arc;
use std::time::Duration;

use common::*;
use rspigot_core::config::{GameConfig, LevelData, PluginSettings};
use rspigot_core::events::{
    ClientEvent, EntityKind as WireKind, EntityStatus, Hand, Inbound, PlayerInput,
};
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPayload, HandlerSpec, Order, Question,
};
use rspigot_core::ports::ChunkGenerator;
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::damage::DamageType;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, BlockStateId, ChunkPos, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;

struct Plains;

impl ChunkGenerator for Plains {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
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
}

fn plains_server(adjust: impl FnOnce(&mut GameConfig)) -> Server<Link> {
    let mut config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        plugins: PluginSettings {
            decision_ticks: 1_000,
            ..PluginSettings::default()
        },
        ..GameConfig::default()
    };
    adjust(&mut config);
    let data = LevelData {
        spawn_mobs: false,
        day_time: 18_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Plains),
        StatusBoard::new(20),
    );
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn ready(handlers: Vec<HandlerSpec>) -> Scene {
    ready_with(handlers, |_| {})
}

fn ready_with(handlers: Vec<HandlerSpec>, adjust: impl FnOnce(&mut GameConfig)) -> Scene {
    let mut s = scene_on(plains_server(adjust), &["Alice"], handlers);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    s
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn summon(s: &mut Scene, kind: Kind, nbt: &str, at: Vec3) -> EntityId {
    let before = s.server.entities_of(kind).len();
    let name = kind.name();
    console(
        s,
        &format!("summon {name} {} {} {} {nbt}", at.x, at.y, at.z),
    );
    let found = s.server.entities_of(kind);
    assert_eq!(found.len(), before + 1, "{name} was not summoned");
    found.last().unwrap().id
}

fn health(s: &Scene, kind: Kind, id: EntityId) -> f32 {
    s.server
        .entities_of(kind)
        .into_iter()
        .find(|entity| entity.id == id)
        .and_then(|entity| entity.health)
        .expect("a living entity")
}

fn exists(s: &Scene, kind: Kind, id: EntityId) -> bool {
    s.server
        .entities_of(kind)
        .into_iter()
        .any(|entity| entity.id == id)
}

fn status_events(events: &[ClientEvent], id: EntityId) -> Vec<i8> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::EntityStatus {
                entity,
                status: EntityStatus::Event(event),
            } if *entity == id => Some(*event),
            _ => None,
        })
        .collect()
}

fn spawned(events: &[ClientEvent], kind: Kind) -> Vec<(EntityId, i32)> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::SpawnEntity {
                entity,
                kind: WireKind::Other(id),
                data,
                ..
            } if *id == kind.id().0 => Some((*entity, *data)),
            _ => None,
        })
        .collect()
}

fn thrown_damage(events: &[ClientEvent], id: EntityId) -> usize {
    events
        .iter()
        .filter(|event| {
            matches!(event, ClientEvent::DamageEvent { entity, kind: DamageType::Thrown, .. } if *entity == id)
        })
        .count()
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

fn snow_at(events: &[ClientEvent], pos: BlockPos) -> bool {
    events.iter().any(|event| {
        matches!(event, ClientEvent::BlockUpdate { pos: at, state } if *at == pos
            && state.kind().is_some_and(|kind| kind.name() == "minecraft:snow"))
    })
}

const FAR: Vec3 = Vec3::new(20.5, 120.0, 20.5);

#[test]
fn a_snowball_flies_with_the_vanilla_gravity_and_drag() {
    let mut s = ready(Vec::new());
    let id = summon(&mut s, Kind::Snowball, "{Motion:[0.0d,0.2d,0.1d]}", FAR);
    let mut velocity = Vec3::new(0.0, 0.2, 0.1);
    let mut position = FAR;
    let drag = f64::from(0.99f32);
    for tick in 1..=30 {
        s.tick(1);
        velocity = (velocity + Vec3::new(0.0, -0.03, 0.0)).scale(drag);
        position = position + velocity;
        let ball = s
            .server
            .entities_of(Kind::Snowball)
            .into_iter()
            .find(|entity| entity.id == id)
            .expect("the snowball keeps flying");
        assert!(
            (ball.velocity.y - velocity.y).abs() < 1.0e-9
                && (ball.velocity.z - velocity.z).abs() < 1.0e-9,
            "tick {tick}: {:?} is not {velocity:?}",
            ball.velocity
        );
        assert!(
            (ball.position.y - position.y).abs() < 1.0e-9
                && (ball.position.z - position.z).abs() < 1.0e-9,
            "tick {tick}: {:?} is not {position:?}",
            ball.position
        );
    }
}

#[test]
fn a_snowball_hitting_a_block_tells_the_trackers_and_is_discarded() {
    let mut s = ready(Vec::new());
    let id = summon(
        &mut s,
        Kind::Snowball,
        "{Motion:[0.0d,-1.0d,0.0d]}",
        Vec3::new(3.5, 66.0, 3.5),
    );
    s.players[0].link.take();
    s.tick(3);
    assert!(!exists(&s, Kind::Snowball, id));
    let events = s.players[0].link.take();
    assert_eq!(status_events(&events, id), vec![3]);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ClientEvent::RemoveEntities(ids) if ids.contains(&id)))
    );
}

#[test]
fn a_snowball_pushes_a_zombie_without_hurting_it() {
    let mut s = ready(Vec::new());
    let zombie = summon(
        &mut s,
        Kind::Zombie,
        "{NoAI:1b}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    let id = summon(
        &mut s,
        Kind::Snowball,
        "{Motion:[0.0d,0.0d,1.0d]}",
        Vec3::new(30.5, 65.0, 26.5),
    );
    s.players[0].link.take();
    s.tick(6);
    assert!(!exists(&s, Kind::Snowball, id));
    assert_eq!(health(&s, Kind::Zombie, zombie), 20.0);
    let events = s.players[0].link.take();
    assert_eq!(thrown_damage(&events, zombie), 1);
    assert_eq!(status_events(&events, id), vec![3]);
    let pushed = s
        .server
        .entities_of(Kind::Zombie)
        .into_iter()
        .find(|entity| entity.id == zombie)
        .unwrap();
    assert!(pushed.velocity.z > 0.0, "{:?}", pushed.velocity);
}

#[test]
fn a_snowball_breaks_on_a_player_without_hurting_them() {
    let mut s = ready(Vec::new());
    let alice = s.server.player_entity_id(s.players[0].id).unwrap();
    let ball = summon(
        &mut s,
        Kind::Snowball,
        "{Motion:[0.0d,0.0d,1.0d]}",
        Vec3::new(0.5, 65.0, -4.5),
    );
    s.players[0].link.take();
    s.tick(6);
    let events = s.players[0].link.take();
    assert_eq!(status_events(&events, ball), vec![3]);
    assert_eq!(thrown_damage(&events, alice), 0);
    assert_eq!(s.server.player_health(s.players[0].id), Some(20.0));
    assert!(!exists(&s, Kind::Snowball, ball));
}

#[test]
fn a_snowball_hurts_a_blaze_by_three() {
    let mut s = ready(Vec::new());
    let blaze = summon(
        &mut s,
        Kind::Blaze,
        "{NoAI:1b}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    summon(
        &mut s,
        Kind::Snowball,
        "{Motion:[0.0d,0.0d,1.0d]}",
        Vec3::new(30.5, 65.0, 26.5),
    );
    s.tick(6);
    assert_eq!(health(&s, Kind::Blaze, blaze), 17.0);
}

#[test]
fn a_snowball_does_not_hit_its_owner_until_it_has_left_it() {
    let mut s = ready(Vec::new());
    let owner = summon(
        &mut s,
        Kind::Zombie,
        "{NoAI:1b}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    let uuid = s
        .server
        .saved_entity(owner)
        .and_then(|nbt| nbt.get("UUID").cloned())
        .expect("a uuid");
    let nbt = format!(
        "{{Motion:[0.0d,0.0d,0.4d],Owner:{}}}",
        match uuid {
            rspigot_nbt::Tag::IntArray(parts) =>
                format!("[I;{},{},{},{}]", parts[0], parts[1], parts[2], parts[3]),
            _ => panic!("an int array uuid"),
        }
    );
    let ball = summon(&mut s, Kind::Snowball, &nbt, Vec3::new(30.5, 65.0, 30.5));
    s.players[0].link.take();
    s.tick(2);
    assert!(exists(&s, Kind::Snowball, ball));
    assert_eq!(thrown_damage(&s.players[0].link.take(), owner), 0);
}

fn golem_and_zombie(s: &mut Scene) -> (EntityId, EntityId) {
    let golem = summon(s, Kind::SnowGolem, "{}", Vec3::new(6.5, 64.0, 2.5));
    let zombie = summon(s, Kind::Zombie, "{NoAI:1b}", Vec3::new(6.5, 64.0, 9.5));
    (golem, zombie)
}

#[test]
fn a_snow_golem_throws_snowballs_at_a_hostile_mob_and_makes_a_noise() {
    let mut s = ready(Vec::new());
    let (golem, zombie) = golem_and_zombie(&mut s);
    let mut owner = None;
    let mut heard = false;
    let mut hit = false;
    for _ in 0..200 {
        s.tick(1);
        let events = s.players[0].link.take();
        owner = owner.or_else(|| {
            spawned(&events, Kind::Snowball)
                .first()
                .map(|(_, data)| *data)
        });
        heard |= sounds(&events).contains(&"minecraft:entity.snow_golem.shoot");
        hit |= thrown_damage(&events, zombie) > 0;
        if hit {
            break;
        }
    }
    assert_eq!(owner, Some(golem.0), "the snowball belongs to the golem");
    assert!(heard);
    assert!(hit, "no snowball reached the zombie");
    assert_eq!(health(&s, Kind::Zombie, zombie), 20.0);
}

#[test]
fn a_snow_golem_ignores_a_cow() {
    let mut s = ready(Vec::new());
    summon(&mut s, Kind::SnowGolem, "{}", Vec3::new(6.5, 64.0, 2.5));
    summon(&mut s, Kind::Cow, "{NoAI:1b}", Vec3::new(6.5, 64.0, 8.5));
    let mut thrown = false;
    for _ in 0..100 {
        s.tick(1);
        thrown |= !s.server.entities_of(Kind::Snowball).is_empty();
    }
    assert!(!thrown);
}

#[test]
fn a_launch_is_a_cancellable_projectile_launch_of_a_snowball() {
    let mut s = ready(vec![deciding(1, EventKind::ProjectileLaunch)]);
    golem_and_zombie(&mut s);
    for _ in 0..200 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::ProjectileLaunch).is_empty() {
            break;
        }
    }
    let asked = last_asked(&s.plugin, EventKind::ProjectileLaunch);
    let EventPayload::ProjectileLaunch(launch) = &asked.payload else {
        panic!("a projectile launch");
    };
    assert_eq!(launch.entity_type, "minecraft:snowball");
    assert!(s.server.entities_of(Kind::Snowball).is_empty());
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(s.server.entities_of(Kind::Snowball).is_empty());
}

#[test]
fn an_allowed_launch_puts_the_snowball_in_the_world() {
    let mut s = ready(vec![deciding(1, EventKind::ProjectileLaunch)]);
    golem_and_zombie(&mut s);
    for _ in 0..200 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::ProjectileLaunch).is_empty() {
            break;
        }
    }
    let asked = last_asked(&s.plugin, EventKind::ProjectileLaunch);
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert_eq!(s.server.entities_of(Kind::Snowball).len(), 1);
}

fn hit_scene() -> (Scene, EntityId, EntityId) {
    let mut s = ready(vec![deciding(1, EventKind::ProjectileHit)]);
    let zombie = summon(
        &mut s,
        Kind::Zombie,
        "{NoAI:1b}",
        Vec3::new(30.5, 64.0, 30.5),
    );
    let ball = summon(
        &mut s,
        Kind::Snowball,
        "{Motion:[0.0d,0.0d,1.0d]}",
        Vec3::new(30.5, 65.0, 26.5),
    );
    for _ in 0..10 {
        s.tick(1);
        if !asked_for(&s.plugin, EventKind::ProjectileHit).is_empty() {
            break;
        }
    }
    (s, zombie, ball)
}

#[test]
fn an_entity_hit_asks_the_plugins_and_waits_for_the_answer() {
    let (mut s, zombie, ball) = hit_scene();
    let asked = last_asked(&s.plugin, EventKind::ProjectileHit);
    let EventPayload::ProjectileHit(hit) = &asked.payload else {
        panic!("a projectile hit");
    };
    assert_eq!(hit.entity.id, ball);
    assert_eq!(hit.entity_type, "minecraft:snowball");
    assert_eq!(hit.hit_entity.as_ref().map(|view| view.id), Some(zombie));
    assert!(hit.hit_block.is_none());
    s.tick(3);
    assert!(exists(&s, Kind::Snowball, ball));
    rule_on(&mut s.server, 1, asked.id, false, None);
    assert!(!exists(&s, Kind::Snowball, ball));
}

#[test]
fn a_cancelled_entity_hit_lets_the_snowball_fly_on() {
    let (mut s, zombie, ball) = hit_scene();
    let asked = last_asked(&s.plugin, EventKind::ProjectileHit);
    s.players[0].link.take();
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(exists(&s, Kind::Snowball, ball));
    assert_eq!(thrown_damage(&s.players[0].link.take(), zombie), 0);
}

#[test]
fn a_block_hit_is_announced_with_the_block_and_the_face() {
    let mut s = ready(vec![observe(2, EventKind::ProjectileHit)]);
    let ball = summon(
        &mut s,
        Kind::Snowball,
        "{Motion:[0.0d,-1.0d,0.0d]}",
        Vec3::new(3.5, 66.0, 3.5),
    );
    s.tick(3);
    assert!(!exists(&s, Kind::Snowball, ball));
    let settled = settled_for(&s.plugin, EventKind::ProjectileHit);
    let EventPayload::ProjectileHit(hit) = &settled.last().expect("a hit").payload else {
        panic!("a projectile hit");
    };
    assert_eq!(hit.hit_block, Some(BlockPos::new(3, 63, 3)));
    assert!(hit.hit_entity.is_none());
}

#[test]
fn a_walking_golem_leaves_a_trail_of_snow_under_its_feet() {
    let mut s = ready(Vec::new());
    summon(
        &mut s,
        Kind::SnowGolem,
        "{NoAI:1b}",
        Vec3::new(3.5, 64.0, 3.5),
    );
    s.players[0].link.take();
    s.tick(3);
    let events = s.players[0].link.take();
    assert!(snow_at(&events, BlockPos::new(3, 64, 3)));
}

#[test]
fn a_golem_straddling_blocks_trails_all_four_spots() {
    let mut s = ready(Vec::new());
    summon(
        &mut s,
        Kind::SnowGolem,
        "{NoAI:1b}",
        Vec3::new(4.01, 64.0, 4.01),
    );
    s.players[0].link.take();
    s.tick(3);
    let events = s.players[0].link.take();
    for (x, z) in [(3, 3), (4, 3), (3, 4), (4, 4)] {
        assert!(snow_at(&events, BlockPos::new(x, 64, z)), "{x} {z}");
    }
}

#[test]
fn no_trail_forms_when_mob_griefing_is_off() {
    let mut s = ready_with(Vec::new(), |config| config.rules.mob_griefing = false);
    summon(
        &mut s,
        Kind::SnowGolem,
        "{NoAI:1b}",
        Vec3::new(3.5, 64.0, 3.5),
    );
    s.players[0].link.take();
    s.tick(5);
    assert!(!snow_at(&s.players[0].link.take(), BlockPos::new(3, 64, 3)));
}

#[test]
fn a_cancelled_block_form_keeps_the_ground_bare() {
    let mut s = ready(vec![deciding(1, EventKind::EntityBlockForm)]);
    let golem = summon(
        &mut s,
        Kind::SnowGolem,
        "{NoAI:1b}",
        Vec3::new(3.5, 64.0, 3.5),
    );
    s.tick(2);
    let asked = last_asked(&s.plugin, EventKind::EntityBlockForm);
    let EventPayload::EntityBlockForm(form) = &asked.payload else {
        panic!("a block form");
    };
    assert_eq!(form.entity.id, golem);
    assert_eq!(form.block, BlockPos::new(3, 64, 3));
    s.players[0].link.take();
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert!(!snow_at(&s.players[0].link.take(), BlockPos::new(3, 64, 3)));
    s.tick(1);
    let again = last_asked(&s.plugin, EventKind::EntityBlockForm);
    assert_ne!(again.id, asked.id);
    rule_on(&mut s.server, 1, again.id, false, None);
    assert!(snow_at(&s.players[0].link.take(), BlockPos::new(3, 64, 3)));
}

#[test]
fn the_golem_does_not_form_snow_where_the_ground_cannot_hold_it() {
    let mut s = ready(Vec::new());
    s.act(Order::Block {
        pos: BlockPos::new(3, 63, 3),
        order: BlockOrder::Set("minecraft:ice".to_owned()),
    });
    summon(
        &mut s,
        Kind::SnowGolem,
        "{NoAI:1b}",
        Vec3::new(3.5, 64.0, 3.5),
    );
    s.players[0].link.take();
    s.tick(5);
    assert!(!snow_at(&s.players[0].link.take(), BlockPos::new(3, 64, 3)));
}

#[test]
fn a_golem_does_not_melt_on_plains() {
    let mut s = ready(Vec::new());
    let id = summon(
        &mut s,
        Kind::SnowGolem,
        "{NoAI:1b}",
        Vec3::new(3.5, 64.0, 3.5),
    );
    s.tick(5);
    assert_eq!(health(&s, Kind::SnowGolem, id), 4.0);
}

#[test]
fn a_golem_melts_to_death_in_a_hot_biome() {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, BlockPos::new(1, 64, 1));
    let id = summon(
        &mut s,
        Kind::SnowGolem,
        "{NoAI:1b}",
        Vec3::new(3.5, 64.0, 3.5),
    );
    s.tick(1);
    let first = health(&s, Kind::SnowGolem, id);
    assert!(first < 4.0, "health was {first}");
    s.tick(120);
    assert!(!exists(&s, Kind::SnowGolem, id));
}

#[test]
fn a_golem_standing_in_water_takes_drowning_damage() {
    let mut s = ready(Vec::new());
    s.act(Order::Block {
        pos: BlockPos::new(3, 64, 3),
        order: BlockOrder::Set("minecraft:water".to_owned()),
    });
    let id = summon(
        &mut s,
        Kind::SnowGolem,
        "{NoAI:1b}",
        Vec3::new(3.5, 64.0, 3.5),
    );
    s.tick(2);
    assert_eq!(health(&s, Kind::SnowGolem, id), 3.0);
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
}

fn carved_pumpkins(s: &Scene) -> u32 {
    s.server
        .ground_items()
        .iter()
        .filter(|(_, stack)| stack.item == rspigot_items::ItemKind::CarvedPumpkin.id())
        .map(|(_, stack)| u32::from(stack.count.get()))
        .sum()
}

#[test]
fn shears_take_the_pumpkin_and_drop_a_carved_pumpkin() {
    let mut s = ready(Vec::new());
    console(&mut s, "give Alice minecraft:shears");
    let id = summon(
        &mut s,
        Kind::SnowGolem,
        "{NoAI:1b}",
        Vec3::new(3.5, 64.0, 3.5),
    );
    assert_eq!(
        s.server.saved_entity(id).unwrap().get_bool("Pumpkin"),
        Some(true)
    );
    s.players[0].link.take();
    interact(&mut s, id);
    s.server.deliver();
    s.tick(1);
    assert_eq!(
        s.server.saved_entity(id).unwrap().get_bool("Pumpkin"),
        Some(false)
    );
    assert_eq!(carved_pumpkins(&s), 1);
    assert!(sounds(&s.players[0].link.take()).contains(&"minecraft:entity.snow_golem.shear"));
    interact(&mut s, id);
    s.server.deliver();
    s.tick(1);
    assert_eq!(carved_pumpkins(&s), 1);
}

#[test]
fn a_cancelled_shear_leaves_the_pumpkin_on() {
    let mut s = ready(vec![deciding(1, EventKind::PlayerShearEntity)]);
    console(&mut s, "give Alice minecraft:shears");
    let id = summon(
        &mut s,
        Kind::SnowGolem,
        "{NoAI:1b}",
        Vec3::new(3.5, 64.0, 3.5),
    );
    interact(&mut s, id);
    s.server.deliver();
    let asked = last_asked(&s.plugin, EventKind::PlayerShearEntity);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(
        s.server.saved_entity(id).unwrap().get_bool("Pumpkin"),
        Some(true)
    );
    assert_eq!(carved_pumpkins(&s), 0);
}

#[test]
fn a_bare_golem_is_not_sheared() {
    let mut s = ready(Vec::new());
    console(&mut s, "give Alice minecraft:shears");
    let id = summon(
        &mut s,
        Kind::SnowGolem,
        "{NoAI:1b,Pumpkin:0b}",
        Vec3::new(3.5, 64.0, 3.5),
    );
    interact(&mut s, id);
    s.server.deliver();
    s.tick(1);
    assert_eq!(carved_pumpkins(&s), 0);
    let Answer::Inventory(state) = s.ask(Question::Inventory(s.players[0].id)) else {
        panic!("an inventory");
    };
    assert!(
        state
            .slots
            .iter()
            .flatten()
            .any(|item| item.key == "minecraft:shears")
    );
}
