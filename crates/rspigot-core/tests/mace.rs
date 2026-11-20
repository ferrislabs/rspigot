mod common;

use common::*;
use rspigot_core::admin::{AdminLists, ListEntry};
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, PlayerInput};
use rspigot_core::plugins::events::EntityKnockbackCause;
use rspigot_core::plugins::{
    Answer, EventKind, EventPayload, HandlerSpec, InventoryOrder, ItemSpec, Order, Question,
};
use rspigot_domain::{EntityId, Vec3};

const MACE: &str = "minecraft:mace";
const FULL_HEALTH: f32 = 20.0;
const GOLEM_HEALTH: f32 = 100.0;
const CHARGE_TICKS: u32 = 45;

fn mace(enchantments: &[(&str, i32)]) -> ItemSpec {
    let mut spec = ItemSpec::of(MACE, 1);
    spec.enchantments = enchantments
        .iter()
        .map(|(name, level)| ((*name).to_owned(), *level))
        .collect();
    spec
}

fn equip(s: &mut Scene, who: usize, slot: i32, item: ItemSpec) {
    let player = s.players[who].id;
    s.act(Order::Inventory {
        player,
        order: InventoryOrder::Set {
            slot,
            item: Some(item),
        },
    });
}

fn spot(s: &mut Scene, who: usize) -> Vec3 {
    match s.ask(Question::Player(s.players[who].id)) {
        Answer::Player(state) => state.player.position,
        _ => panic!("expected the player"),
    }
}

fn spawn_at(s: &mut Scene, kind: &str, position: Vec3) -> EntityId {
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: kind.into(),
        position,
    }) else {
        panic!("expected the spawned {kind}");
    };
    view.id
}

fn entity_of(s: &mut Scene, who: usize) -> EntityId {
    match s.ask(Question::Player(s.players[who].id)) {
        Answer::Player(state) => EntityId(state.player.entity_id),
        _ => panic!("expected the player"),
    }
}

fn step(s: &mut Scene, who: usize, position: Vec3, on_ground: bool) {
    let player = s.players[who].clone();
    send_input(
        &mut s.server,
        &player,
        PlayerInput::Move {
            position: Some(position),
            rotation: None,
            on_ground,
        },
    );
}

fn fall_onto(s: &mut Scene, who: usize, end: Vec3, fall: f64) {
    let mut y = end.y + fall;
    step(s, who, Vec3::new(end.x, y, end.z), false);
    while y - 1.0 > end.y {
        y -= 1.0;
        step(s, who, Vec3::new(end.x, y, end.z), false);
    }
    step(s, who, end, false);
}

fn health_of(s: &mut Scene, id: EntityId) -> f32 {
    match s.ask(Question::Entity(id)) {
        Answer::Entity(details) => details.health.unwrap_or(0.0),
        _ => panic!("expected the entity {id:?}"),
    }
}

fn velocity_of(s: &mut Scene, id: EntityId) -> Vec3 {
    match s.ask(Question::Entity(id)) {
        Answer::Entity(details) => details.velocity,
        _ => panic!("expected the entity {id:?}"),
    }
}

fn player_health(s: &mut Scene, who: usize) -> f32 {
    match s.ask(Question::Player(s.players[who].id)) {
        Answer::Player(state) => state.health,
        _ => panic!("expected the player"),
    }
}

struct Smash {
    scene: Scene,
    ground: Vec3,
    perch: Vec3,
    target: EntityId,
}

fn setup(kind: &str, enchantments: &[(&str, i32)], handlers: Vec<HandlerSpec>) -> Smash {
    let mut scene = scene_with(&["Alice"], handlers);
    equip(&mut scene, 0, 0, mace(enchantments));
    let ground = spot(&mut scene, 0);
    let target = spawn_at(
        &mut scene,
        kind,
        Vec3::new(ground.x + 1.0, ground.y, ground.z),
    );
    scene.tick(CHARGE_TICKS);
    let perch = Vec3::new(ground.x, ground.y + 1.0, ground.z);
    Smash {
        scene,
        ground,
        perch,
        target,
    }
}

fn smash_damage(fall: f64, enchantments: &[(&str, i32)]) -> f32 {
    let mut smash = setup("minecraft:iron_golem", enchantments, Vec::new());
    fall_onto(&mut smash.scene, 0, smash.perch, fall);
    let alice = smash.scene.players[0].clone();
    send_input(
        &mut smash.scene.server,
        &alice,
        PlayerInput::Attack(smash.target),
    );
    GOLEM_HEALTH - health_of(&mut smash.scene, smash.target)
}

#[test]
fn a_ten_block_fall_adds_the_vanilla_bonus_to_the_mace_hit() {
    let damage = smash_damage(10.0, &[]);
    assert!((damage - (6.0 + 24.0) * 1.5).abs() < 0.01, "{damage}");
}

#[test]
fn a_fall_of_one_block_gets_no_smash_bonus() {
    let damage = smash_damage(1.0, &[]);
    assert!((damage - 6.0 * 1.5).abs() < 0.01, "{damage}");
}

#[test]
fn density_adds_damage_per_fallen_block() {
    let damage = smash_damage(10.0, &[("minecraft:density", 3)]);
    assert!(
        (damage - (6.0 + 24.0 + 15.0) * 1.5).abs() < 0.01,
        "{damage}"
    );
}

#[test]
fn a_smash_resets_the_fall_distance_so_landing_hurts_nothing() {
    let mut smash = setup("minecraft:iron_golem", &[], Vec::new());
    fall_onto(&mut smash.scene, 0, smash.perch, 10.0);
    let alice = smash.scene.players[0].clone();
    send_input(
        &mut smash.scene.server,
        &alice,
        PlayerInput::Attack(smash.target),
    );
    step(&mut smash.scene, 0, smash.perch, true);
    assert_eq!(player_health(&mut smash.scene, 0), FULL_HEALTH);
}

#[test]
fn without_a_smash_the_landing_still_hurts() {
    let mut smash = setup("minecraft:iron_golem", &[], Vec::new());
    fall_onto(&mut smash.scene, 0, smash.perch, 10.0);
    step(&mut smash.scene, 0, smash.perch, true);
    assert!(player_health(&mut smash.scene, 0) < FULL_HEALTH);
}

#[test]
fn nearby_entities_are_knocked_back_and_the_target_is_not() {
    let mut smash = setup("minecraft:zombie", &[], Vec::new());
    let near = spawn_at(
        &mut smash.scene,
        "minecraft:cow",
        Vec3::new(smash.ground.x + 2.5, smash.ground.y, smash.ground.z),
    );
    let far = spawn_at(
        &mut smash.scene,
        "minecraft:cow",
        Vec3::new(smash.ground.x + 8.0, smash.ground.y, smash.ground.z),
    );
    fall_onto(&mut smash.scene, 0, smash.perch, 1.6);
    let alice = smash.scene.players[0].clone();
    send_input(
        &mut smash.scene.server,
        &alice,
        PlayerInput::Attack(smash.target),
    );
    let pushed = velocity_of(&mut smash.scene, near);
    assert!(pushed.y > 0.6, "{pushed:?}");
    assert!(pushed.x > 0.0, "{pushed:?}");
    let calm = velocity_of(&mut smash.scene, far);
    assert!(calm.x == 0.0 && calm.y < 0.1, "{calm:?}");
    let target = velocity_of(&mut smash.scene, smash.target);
    assert!(target.y < 0.5, "{target:?}");
}

#[test]
fn a_smash_plays_the_mace_sound() {
    let mut smash = setup("minecraft:zombie", &[], Vec::new());
    smash.scene.players[0].link.take();
    fall_onto(&mut smash.scene, 0, smash.perch, 1.6);
    let alice = smash.scene.players[0].clone();
    send_input(
        &mut smash.scene.server,
        &alice,
        PlayerInput::Attack(smash.target),
    );
    let sounds: Vec<&str> = smash.scene.players[0]
        .link
        .take()
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Sound { sound, .. } if sound.contains("item.mace") => Some(*sound),
            _ => None,
        })
        .collect();
    assert_eq!(sounds.len(), 1, "{sounds:?}");
}

fn armored_damage(enchantments: &[(&str, i32)]) -> f32 {
    let mut s = scene_with(&["Alice", "Bob"], Vec::new());
    equip(&mut s, 0, 0, mace(enchantments));
    for (slot, key) in [
        (39, "minecraft:diamond_helmet"),
        (38, "minecraft:diamond_chestplate"),
        (37, "minecraft:diamond_leggings"),
        (36, "minecraft:diamond_boots"),
    ] {
        equip(&mut s, 1, slot, ItemSpec::of(key, 1));
    }
    let ground = spot(&mut s, 0);
    let bob = entity_of(&mut s, 1);
    s.tick(CHARGE_TICKS);
    let perch = Vec3::new(ground.x, ground.y + 1.0, ground.z);
    fall_onto(&mut s, 0, perch, 2.0);
    let alice = s.players[0].clone();
    send_input(&mut s.server, &alice, PlayerInput::Attack(bob));
    FULL_HEALTH - player_health(&mut s, 1)
}

#[test]
fn breach_lets_the_hit_through_the_armor() {
    let plain = armored_damage(&[]);
    let breached = armored_damage(&[("minecraft:breach", 4)]);
    assert!(plain > 0.0, "{plain}");
    assert!(breached > plain + 3.0, "{breached} vs {plain}");
}

#[test]
fn wind_burst_launches_the_attacker_upward() {
    let mut smash = setup(
        "minecraft:zombie",
        &[("minecraft:wind_burst", 3)],
        Vec::new(),
    );
    smash.scene.players[0].link.take();
    fall_onto(&mut smash.scene, 0, smash.perch, 1.6);
    let alice = smash.scene.players[0].clone();
    send_input(
        &mut smash.scene.server,
        &alice,
        PlayerInput::Attack(smash.target),
    );
    let launch: Vec<Vec3> = smash.scene.players[0]
        .link
        .take()
        .iter()
        .filter_map(|event| match event {
            ClientEvent::Explode {
                knockback: Some(push),
                ..
            } => Some(*push),
            _ => None,
        })
        .collect();
    assert_eq!(launch.len(), 1, "{launch:?}");
    assert!(launch[0].y > 1.5, "{:?}", launch[0]);
}

#[test]
fn no_wind_burst_without_the_enchantment() {
    let mut smash = setup("minecraft:zombie", &[], Vec::new());
    smash.scene.players[0].link.take();
    fall_onto(&mut smash.scene, 0, smash.perch, 1.6);
    let alice = smash.scene.players[0].clone();
    send_input(
        &mut smash.scene.server,
        &alice,
        PlayerInput::Attack(smash.target),
    );
    let bursts = smash.scene.players[0]
        .link
        .take()
        .iter()
        .filter(|event| matches!(event, ClientEvent::Explode { .. }))
        .count();
    assert_eq!(bursts, 0);
}

#[test]
fn the_plugin_event_carries_the_smash_bonus() {
    let mut smash = setup(
        "minecraft:iron_golem",
        &[],
        vec![observe(1, EventKind::EntityDamageByEntity)],
    );
    fall_onto(&mut smash.scene, 0, smash.perch, 10.0);
    let alice = smash.scene.players[0].clone();
    send_input(
        &mut smash.scene.server,
        &alice,
        PlayerInput::Attack(smash.target),
    );
    let EventPayload::EntityDamageByEntity(hit) =
        &settled_for(&smash.scene.plugin, EventKind::EntityDamageByEntity)[0].payload
    else {
        panic!("a damage by entity");
    };
    assert!(
        (hit.damage - ((6.0 + 24.0) * 1.5)).abs() < 0.01,
        "{}",
        hit.damage
    );
}

#[test]
fn after_a_wind_burst_the_landing_at_the_launch_height_hurts_nothing() {
    let mut smash = setup(
        "minecraft:zombie",
        &[("minecraft:wind_burst", 3)],
        Vec::new(),
    );
    fall_onto(&mut smash.scene, 0, smash.perch, 1.6);
    let alice = smash.scene.players[0].clone();
    send_input(
        &mut smash.scene.server,
        &alice,
        PlayerInput::Attack(smash.target),
    );
    let after_hit = player_health(&mut smash.scene, 0);
    let mut y = smash.perch.y;
    for _ in 0..8 {
        y += 1.0;
        step(
            &mut smash.scene,
            0,
            Vec3::new(smash.perch.x, y, smash.perch.z),
            false,
        );
    }
    while y - 1.0 > smash.perch.y {
        y -= 1.0;
        step(
            &mut smash.scene,
            0,
            Vec3::new(smash.perch.x, y, smash.perch.z),
            false,
        );
    }
    step(&mut smash.scene, 0, smash.perch, true);
    assert_eq!(player_health(&mut smash.scene, 0), after_hit);
}

#[test]
fn falling_below_the_launch_height_still_counts() {
    let mut smash = setup(
        "minecraft:zombie",
        &[("minecraft:wind_burst", 3)],
        Vec::new(),
    );
    fall_onto(&mut smash.scene, 0, smash.perch, 1.6);
    let alice = smash.scene.players[0].clone();
    send_input(
        &mut smash.scene.server,
        &alice,
        PlayerInput::Attack(smash.target),
    );
    let after_hit = player_health(&mut smash.scene, 0);
    let mut y = smash.perch.y;
    for _ in 0..8 {
        y += 1.0;
        step(
            &mut smash.scene,
            0,
            Vec3::new(smash.perch.x, y, smash.perch.z),
            false,
        );
    }
    while y - 1.0 > smash.perch.y - 12.0 {
        y -= 1.0;
        step(
            &mut smash.scene,
            0,
            Vec3::new(smash.perch.x, y, smash.perch.z),
            false,
        );
    }
    step(
        &mut smash.scene,
        0,
        Vec3::new(smash.perch.x, smash.perch.y - 12.0, smash.perch.z),
        true,
    );
    assert!(player_health(&mut smash.scene, 0) < after_hit);
}

fn geared_zombie_loss(enchantments: &[(&str, i32)]) -> f32 {
    let server = configured(|config| {
        config.admin = AdminLists {
            operators: vec![ListEntry::named("Alice")],
            ..AdminLists::default()
        };
    });
    let mut s = scene_on(server, &["Alice"], Vec::new());
    equip(&mut s, 0, 0, mace(enchantments));
    let ground = spot(&mut s, 0);
    let gear = [
        "head:{id:\"minecraft:diamond_helmet\",count:1}",
        "chest:{id:\"minecraft:diamond_chestplate\",count:1}",
        "legs:{id:\"minecraft:diamond_leggings\",count:1}",
        "feet:{id:\"minecraft:diamond_boots\",count:1}",
    ]
    .join(",");
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Command(format!(
            "/summon zombie {} {} {} {{NoAI:1b,equipment:{{{gear}}}}}",
            ground.x + 1.0,
            ground.y,
            ground.z
        )),
    );
    s.tick(2);
    let target = s.server.entities_of(rspigot_entities::EntityKind::Zombie)[0].id;
    s.tick(CHARGE_TICKS);
    let before = health_of(&mut s, target);
    send_input(&mut s.server, &alice, PlayerInput::Attack(target));
    before - health_of(&mut s, target)
}

#[test]
fn breach_lets_the_hit_through_a_mob_armor() {
    let plain = geared_zombie_loss(&[]);
    let breached = geared_zombie_loss(&[("minecraft:breach", 4)]);
    assert!(plain > 0.0, "{plain}");
    assert!(breached > plain, "{breached} vs {plain}");
}

fn source_kind_of_hit(fall: f64) -> String {
    let mut smash = setup(
        "minecraft:iron_golem",
        &[],
        vec![observe(1, EventKind::EntityDamageByEntity)],
    );
    fall_onto(&mut smash.scene, 0, smash.perch, fall);
    let alice = smash.scene.players[0].clone();
    send_input(
        &mut smash.scene.server,
        &alice,
        PlayerInput::Attack(smash.target),
    );
    let EventPayload::EntityDamageByEntity(hit) =
        &settled_for(&smash.scene.plugin, EventKind::EntityDamageByEntity)[0].payload
    else {
        panic!("a damage by entity");
    };
    hit.damage_source.kind.clone()
}

#[test]
fn a_smash_is_a_mace_smash_and_a_plain_mace_hit_is_a_player_attack() {
    assert_eq!(source_kind_of_hit(10.0), "minecraft:mace_smash");
    assert_eq!(source_kind_of_hit(1.0), "minecraft:player_attack");
}

#[test]
fn the_nearby_push_asks_for_an_attack_knockback_and_a_cancel_keeps_it_still() {
    let mut server = server(PluginSettings {
        decision_ticks: 1_000,
        ..PluginSettings::default()
    });
    let plugin = connect_with(
        &mut server,
        1,
        "tester",
        vec![deciding(1, EventKind::EntityKnockback)],
    );
    let alice = join(&mut server, "Alice");
    server.deliver();
    alice.link.take();
    let mut scene = Scene {
        server,
        plugin,
        players: vec![alice],
    };
    equip(&mut scene, 0, 0, mace(&[]));
    let ground = spot(&mut scene, 0);
    let target = spawn_at(
        &mut scene,
        "minecraft:zombie",
        Vec3::new(ground.x + 1.0, ground.y, ground.z),
    );
    let near = spawn_at(
        &mut scene,
        "minecraft:cow",
        Vec3::new(ground.x + 2.5, ground.y, ground.z),
    );
    scene.tick(CHARGE_TICKS);
    let perch = Vec3::new(ground.x, ground.y + 1.0, ground.z);
    fall_onto(&mut scene, 0, perch, 1.6);
    let alice = scene.players[0].clone();
    send_input(&mut scene.server, &alice, PlayerInput::Attack(target));
    let asked: Vec<_> = asked_for(&scene.plugin, EventKind::EntityKnockbackByEntity)
        .into_iter()
        .filter(|event| {
            matches!(&event.payload, EventPayload::EntityKnockbackByEntity(knock)
                if knock.entity.id == near && knock.cause == EntityKnockbackCause::EntityAttack)
        })
        .collect();
    assert_eq!(asked.len(), 1);
    rule_on(&mut scene.server, 1, asked[0].id, true, None);
    let still = velocity_of(&mut scene, near);
    assert!(still.y < 0.1 && still.x.abs() < 0.2, "{still:?}");
}
