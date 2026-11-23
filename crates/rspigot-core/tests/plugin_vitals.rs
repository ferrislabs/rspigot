mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{ClientEvent, PlayerInput};
use rspigot_core::plugins::events::{
    EntityExhaustionPatch, EntityExhaustionReason, EntityRegainHealthPatch,
    EntityRegainHealthRegainReason, FoodLevelChangePatch,
};
use rspigot_core::plugins::{
    Answer, EffectSpec, EventKind, EventPatch, EventPayload, HandlerSpec, Order, PlayerOrder,
    PlayerState, Question, WorldOrder,
};
use rspigot_domain::world::Difficulty;
use rspigot_domain::{EntityId, Vec3};

fn waiting_scene(handlers: Vec<HandlerSpec>) -> Scene {
    let mut server = server(PluginSettings {
        decision_ticks: 1_000,
        ..PluginSettings::default()
    });
    let plugin = connect_with(&mut server, 1, "tester", handlers);
    let alice = join(&mut server, "Alice");
    server.deliver();
    alice.link.take();
    Scene {
        server,
        plugin,
        players: vec![alice],
    }
}

fn state(s: &mut Scene) -> PlayerState {
    let Answer::Player(state) = s.ask(Question::Player(s.players[0].id)) else {
        panic!("expected Alice");
    };
    state
}

fn order(s: &mut Scene, order: PlayerOrder) {
    let alice = s.players[0].clone();
    s.act(to(&alice, order));
}

fn effect(key: &str, duration: i32, amplifier: i32) -> EffectSpec {
    EffectSpec {
        key: key.into(),
        duration,
        amplifier,
        ambient: false,
        particles: true,
        icon: true,
    }
}

fn regains(s: &Scene) -> Vec<(EntityRegainHealthRegainReason, f64)> {
    fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityRegainHealth(regain) => Some((regain.regain_reason, regain.amount)),
            _ => None,
        })
        .collect()
}

fn exhaustions(s: &Scene) -> Vec<(EntityExhaustionReason, f32)> {
    fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::EntityExhaustion(exhaustion) => {
                Some((exhaustion.exhaustion_reason, exhaustion.exhaustion))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn peaceful_regeneration_and_healing_effects_name_their_reason() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityRegainHealth)]);
    order(&mut s, PlayerOrder::SetHealth(10.0));
    s.act(Order::World(WorldOrder::SetDifficulty(
        Difficulty::Peaceful,
    )));
    s.tick(20);
    assert!(regains(&s).contains(&(EntityRegainHealthRegainReason::Regen, 1.0)));
    assert!(
        regains(&s)
            .iter()
            .any(|(reason, _)| *reason == EntityRegainHealthRegainReason::Satiated)
    );
    let entity = state(&mut s).player.entity_id;
    let EventPayload::EntityRegainHealth(regain) =
        &settled_for(&s.plugin, EventKind::EntityRegainHealth)[0].payload
    else {
        panic!("a regain");
    };
    assert_eq!(regain.entity.id, EntityId(entity));
    assert_eq!(regain.entity_type, "minecraft:player");
    s.act(Order::World(WorldOrder::SetDifficulty(Difficulty::Normal)));
    order(
        &mut s,
        PlayerOrder::AddEffect(effect("minecraft:instant_health", 1, 0)),
    );
    s.tick(1);
    assert!(regains(&s).contains(&(EntityRegainHealthRegainReason::Magic, 4.0)));
    order(
        &mut s,
        PlayerOrder::AddEffect(effect("minecraft:regeneration", 200, 1)),
    );
    s.tick(30);
    assert!(
        regains(&s)
            .iter()
            .any(|(reason, _)| *reason == EntityRegainHealthRegainReason::MagicRegen)
    );
}

#[test]
fn a_cancelled_regain_keeps_the_health_and_a_patched_one_heals_its_amount() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::EntityRegainHealth)]);
    order(&mut s, PlayerOrder::SetHealth(10.0));
    order(
        &mut s,
        PlayerOrder::AddEffect(effect("minecraft:instant_health", 1, 0)),
    );
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::EntityRegainHealth);
    assert_eq!(state(&mut s).health, 10.0);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(state(&mut s).health, 10.0);
    order(
        &mut s,
        PlayerOrder::AddEffect(effect("minecraft:instant_health", 1, 0)),
    );
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::EntityRegainHealth);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::EntityRegainHealth(EntityRegainHealthPatch {
            amount: Some(7.0),
        })),
    );
    assert_eq!(state(&mut s).health, 17.0);
}

#[test]
fn exhaustion_comes_from_attacks_hurts_and_hunger_with_its_reason() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::EntityExhaustion)]);
    let alice = s.players[0].clone();
    let Answer::Spawned(cow) = s.ask(Question::Spawn {
        kind: "minecraft:cow".into(),
        position: Vec3::new(1.5, 64.0, 0.5),
    }) else {
        panic!("a cow");
    };
    send_input(&mut s.server, &alice, PlayerInput::Attack(cow.id));
    order(&mut s, PlayerOrder::Damage(2.0));
    order(
        &mut s,
        PlayerOrder::AddEffect(effect("minecraft:hunger", 40, 0)),
    );
    s.tick(1);
    let reasons: Vec<EntityExhaustionReason> = exhaustions(&s)
        .into_iter()
        .map(|(reason, _)| reason)
        .collect();
    assert!(reasons.contains(&EntityExhaustionReason::Attack));
    assert!(reasons.contains(&EntityExhaustionReason::Damaged));
    assert!(reasons.contains(&EntityExhaustionReason::HungerEffect));
    assert!(
        exhaustions(&s).contains(&(EntityExhaustionReason::Attack, 0.1)),
        "{:?}",
        exhaustions(&s)
    );
}

#[test]
fn a_patched_exhaustion_is_what_the_food_data_gets() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::EntityExhaustion)]);
    order(
        &mut s,
        PlayerOrder::AddEffect(effect("minecraft:hunger", 40, 0)),
    );
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::EntityExhaustion);
    assert_eq!(state(&mut s).exhaustion, 0.0);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::EntityExhaustion(EntityExhaustionPatch {
            exhaustion: Some(2.5),
        })),
    );
    assert_eq!(state(&mut s).exhaustion, 2.5);
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::EntityExhaustion);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(state(&mut s).exhaustion, 2.5);
}

#[test]
fn hunger_asks_the_lower_food_level_and_a_cancel_resends_the_health() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::FoodLevelChange)]);
    let alice = s.players[0].clone();
    order(&mut s, PlayerOrder::SetSaturation(0.0));
    order(&mut s, PlayerOrder::SetExhaustion(4.5));
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::FoodLevelChange);
    let EventPayload::FoodLevelChange(change) = &asked.payload else {
        panic!("a food level change");
    };
    assert_eq!(change.food_level, 19);
    assert_eq!(change.item, None);
    assert_eq!(state(&mut s).food, 20);
    alice.link.take();
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(state(&mut s).food, 20);
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::Health { food: 20, .. }))
    );
    order(&mut s, PlayerOrder::SetExhaustion(4.5));
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::FoodLevelChange);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::FoodLevelChange(FoodLevelChangePatch {
            food_level: Some(12),
        })),
    );
    assert_eq!(state(&mut s).food, 12);
}

#[test]
fn the_saturation_effect_asks_the_raised_food_level() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::FoodLevelChange)]);
    order(&mut s, PlayerOrder::SetFoodLevel(10));
    order(
        &mut s,
        PlayerOrder::AddEffect(effect("minecraft:saturation", 1, 1)),
    );
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::FoodLevelChange);
    let EventPayload::FoodLevelChange(change) = &asked.payload else {
        panic!("a food level change");
    };
    assert_eq!(change.food_level, 12);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::FoodLevelChange(FoodLevelChangePatch {
            food_level: Some(15),
        })),
    );
    assert_eq!(state(&mut s).food, 15);
}

#[test]
fn every_exhaustion_is_asked_in_turn_while_the_first_waits() {
    let mut s = waiting_scene(vec![deciding(1, EventKind::EntityExhaustion)]);
    order(
        &mut s,
        PlayerOrder::AddEffect(effect("minecraft:hunger", 40, 0)),
    );
    s.tick(3);
    let asked = asked_for(&s.plugin, EventKind::EntityExhaustion);
    assert_eq!(asked.len(), 3);
    for event in &asked {
        rule_on(&mut s.server, 1, event.id, false, None);
    }
    assert!((state(&mut s).exhaustion - 0.015).abs() < 1e-6);
}
