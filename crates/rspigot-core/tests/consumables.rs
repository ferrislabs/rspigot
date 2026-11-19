mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{Hand, PlayerInput};
use rspigot_core::plugins::{
    Answer, EffectSpec, InventoryOrder, ItemSpec, Order, PlayerOrder, PlayerState, Question,
};
use rspigot_domain::Rotation;
use rspigot_nbt::{Compound, Tag};

const SEED: u64 = 11;

fn scene_of() -> Scene {
    scene_on(
        server(PluginSettings::default()).with_rng_seed(SEED),
        &["Alice"],
        Vec::new(),
    )
}

fn component(spec: &mut ItemSpec, key: &str, value: impl Into<Tag>) {
    let compound = Compound::new().with(key, value);
    rspigot_nbt::write_network(&mut spec.components, &Tag::Compound(compound));
}

fn hold(s: &mut Scene, item: ItemSpec) {
    let alice = s.players[0].clone();
    s.act(Order::Inventory {
        player: alice.id,
        order: InventoryOrder::Set {
            slot: 0,
            item: Some(item),
        },
    });
}

fn player(s: &mut Scene, order: PlayerOrder) {
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

fn consume(s: &mut Scene) {
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
    s.tick(70);
}

fn state(s: &mut Scene) -> PlayerState {
    let Answer::Player(state) = s.ask(Question::Player(s.players[0].id)) else {
        panic!("expected Alice");
    };
    state
}

fn slot_zero(s: &mut Scene) -> Option<ItemSpec> {
    let Answer::Inventory(inventory) = s.ask(Question::Inventory(s.players[0].id)) else {
        panic!("expected Alice's inventory");
    };
    inventory.slots[0].clone()
}

fn active(state: &PlayerState, key: &str) -> Option<EffectSpec> {
    state.effects.iter().find(|e| e.key == key).cloned()
}

#[test]
fn drinking_a_speed_potion_applies_speed_for_the_potion_duration_and_returns_a_bottle() {
    let mut s = scene_of();
    let mut potion = ItemSpec::of("minecraft:potion", 1);
    component(
        &mut potion,
        "minecraft:potion_contents",
        Compound::new().with("potion", "minecraft:swiftness"),
    );
    hold(&mut s, potion);
    consume(&mut s);
    let speed = active(&state(&mut s), "minecraft:speed").expect("speed");
    assert_eq!(speed.amplifier, 0);
    assert!(
        (3500..=3600).contains(&speed.duration),
        "{}",
        speed.duration
    );
    let left = slot_zero(&mut s).expect("a bottle");
    assert_eq!(left.key, "minecraft:glass_bottle");
    assert_eq!(left.count, 1);
}

#[test]
fn drinking_a_water_bottle_applies_nothing_and_returns_a_bottle() {
    let mut s = scene_of();
    let mut potion = ItemSpec::of("minecraft:potion", 1);
    component(
        &mut potion,
        "minecraft:potion_contents",
        Compound::new().with("potion", "minecraft:water"),
    );
    hold(&mut s, potion);
    consume(&mut s);
    assert!(state(&mut s).effects.is_empty());
    assert_eq!(slot_zero(&mut s).unwrap().key, "minecraft:glass_bottle");
}

#[test]
fn a_potion_in_a_stack_leaves_the_rest_and_a_bottle_in_the_inventory() {
    let mut s = scene_of();
    let mut potion = ItemSpec::of("minecraft:potion", 2);
    component(
        &mut potion,
        "minecraft:potion_contents",
        Compound::new().with("potion", "minecraft:swiftness"),
    );
    hold(&mut s, potion);
    consume(&mut s);
    assert_eq!(slot_zero(&mut s).unwrap().count, 1);
    assert_eq!(slot_zero(&mut s).unwrap().key, "minecraft:potion");
}

#[test]
fn milk_clears_every_effect() {
    let mut s = scene_of();
    player(
        &mut s,
        PlayerOrder::AddEffect(effect("minecraft:speed", 600, 0)),
    );
    player(
        &mut s,
        PlayerOrder::AddEffect(effect("minecraft:poison", 600, 0)),
    );
    assert_eq!(state(&mut s).effects.len(), 2);
    hold(&mut s, ItemSpec::of("minecraft:milk_bucket", 1));
    consume(&mut s);
    assert!(state(&mut s).effects.is_empty());
    assert_eq!(slot_zero(&mut s).unwrap().key, "minecraft:bucket");
}

#[test]
fn a_honey_bottle_removes_poison_feeds_and_returns_a_bottle() {
    let mut s = scene_of();
    player(
        &mut s,
        PlayerOrder::AddEffect(effect("minecraft:poison", 600, 0)),
    );
    player(
        &mut s,
        PlayerOrder::AddEffect(effect("minecraft:speed", 600, 0)),
    );
    player(&mut s, PlayerOrder::SetFoodLevel(10));
    hold(&mut s, ItemSpec::of("minecraft:honey_bottle", 1));
    consume(&mut s);
    let after = state(&mut s);
    assert!(active(&after, "minecraft:poison").is_none());
    assert!(active(&after, "minecraft:speed").is_some());
    assert_eq!(after.food, 16);
    assert_eq!(slot_zero(&mut s).unwrap().key, "minecraft:glass_bottle");
}

#[test]
fn an_enchanted_golden_apple_gives_its_four_effects() {
    let mut s = scene_of();
    hold(&mut s, ItemSpec::of("minecraft:enchanted_golden_apple", 1));
    consume(&mut s);
    let after = state(&mut s);
    let regeneration = active(&after, "minecraft:regeneration").expect("regeneration");
    assert_eq!(regeneration.amplifier, 1);
    assert!(regeneration.duration <= 400);
    assert!(active(&after, "minecraft:resistance").is_some());
    assert!(active(&after, "minecraft:fire_resistance").is_some());
    let absorption = active(&after, "minecraft:absorption").expect("absorption");
    assert_eq!(absorption.amplifier, 3);
    assert!(slot_zero(&mut s).is_none());
}

#[test]
fn a_suspicious_stew_applies_its_stored_effect_and_returns_a_bowl() {
    let mut s = scene_of();
    let mut stew = ItemSpec::of("minecraft:suspicious_stew", 1);
    component(
        &mut stew,
        "minecraft:suspicious_stew_effects",
        Tag::List(vec![Tag::Compound(
            Compound::new()
                .with("id", "minecraft:night_vision")
                .with("duration", 160),
        )]),
    );
    hold(&mut s, stew);
    consume(&mut s);
    let night = active(&state(&mut s), "minecraft:night_vision").expect("night vision");
    assert!(
        night.duration <= 160 && night.duration > 60,
        "{}",
        night.duration
    );
    assert_eq!(slot_zero(&mut s).unwrap().key, "minecraft:bowl");
}

#[test]
fn chorus_fruit_teleports_within_sixteen_blocks_on_solid_ground() {
    let mut s = scene_of();
    let before = state(&mut s).player.position;
    player(&mut s, PlayerOrder::SetFoodLevel(20));
    hold(&mut s, ItemSpec::of("minecraft:chorus_fruit", 1));
    consume(&mut s);
    let after = state(&mut s).player.position;
    assert!(after != before);
    assert!((after.x - before.x).abs() <= 8.0 && (after.z - before.z).abs() <= 8.0);
    assert_eq!(after.y, 64.0);
    assert!(slot_zero(&mut s).is_none());
}

fn use_item(s: &mut Scene) {
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
}

#[test]
fn eating_a_chorus_fruit_starts_its_cooldown_and_a_second_use_does_nothing() {
    let mut s = scene_of();
    let alice = s.players[0].id;
    let fruit = rspigot_items::ItemKind::ChorusFruit.id();
    player(&mut s, PlayerOrder::SetFoodLevel(20));
    hold(&mut s, ItemSpec::of("minecraft:chorus_fruit", 2));
    use_item(&mut s);
    s.tick(34);
    assert_eq!(slot_zero(&mut s).unwrap().count, 1);
    assert!(s.server.is_on_cooldown(alice, fruit));
    use_item(&mut s);
    s.tick(40);
    assert_eq!(slot_zero(&mut s).unwrap().count, 1);
    assert!(!s.server.is_on_cooldown(alice, fruit));
}

#[test]
fn an_effect_below_probability_one_follows_the_probability() {
    let mut s = scene_of();
    let mut poisoned = 0;
    for _ in 0..30 {
        player(&mut s, PlayerOrder::SetFoodLevel(0));
        hold(&mut s, ItemSpec::of("minecraft:chicken", 1));
        consume(&mut s);
        if active(&state(&mut s), "minecraft:hunger").is_some() {
            poisoned += 1;
            player(&mut s, PlayerOrder::RemoveEffect("minecraft:hunger".into()));
        }
    }
    assert!(poisoned > 0 && poisoned < 30, "{poisoned}");
}

#[test]
fn a_pufferfish_always_gives_its_three_effects() {
    let mut s = scene_of();
    player(&mut s, PlayerOrder::SetFoodLevel(5));
    hold(&mut s, ItemSpec::of("minecraft:pufferfish", 1));
    consume(&mut s);
    let after = state(&mut s);
    assert_eq!(active(&after, "minecraft:poison").unwrap().amplifier, 1);
    assert_eq!(active(&after, "minecraft:hunger").unwrap().amplifier, 2);
    assert!(active(&after, "minecraft:nausea").is_some());
}
