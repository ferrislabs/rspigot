mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::ClientEvent;
use rspigot_core::plugins::{InventoryOrder, ItemSpec, Order};

fn scene_of() -> Scene {
    scene_on(
        server(PluginSettings::default()).with_rng_seed(2),
        &["Alice"],
        Vec::new(),
    )
}

fn wear(s: &mut Scene, slot: i32, key: &str, enchantment: &str) {
    let alice = s.players[0].clone();
    let mut spec = ItemSpec::of(key, 1);
    spec.enchantments = vec![(enchantment.to_owned(), 3)];
    s.act(Order::Inventory {
        player: alice.id,
        order: InventoryOrder::Set {
            slot,
            item: Some(spec),
        },
    });
}

fn all_synced(s: &Scene) -> Vec<rspigot_domain::effect::AttributeState> {
    s.players[0]
        .link
        .take_raw()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::Attributes { attributes, .. } => Some(attributes),
            _ => None,
        })
        .flatten()
        .collect()
}

fn synced(states: &[rspigot_domain::effect::AttributeState], attribute: &str) -> Option<f64> {
    states
        .iter()
        .filter(|state| state.attribute == attribute)
        .map(rspigot_domain::attribute::value)
        .next_back()
}

#[test]
fn depth_strider_boots_send_water_movement_efficiency_to_the_client() {
    let mut s = scene_of();
    s.tick(2);
    let _ = s.players[0].link.take_raw();
    wear(
        &mut s,
        36,
        "minecraft:iron_boots",
        "minecraft:depth_strider",
    );
    s.tick(2);
    let states = all_synced(&s);
    let value = synced(&states, "minecraft:water_movement_efficiency").expect("synced");
    assert!((value - 1.0).abs() < 1e-5, "{value}");
}

#[test]
fn respiration_and_swift_sneak_reach_the_client() {
    let mut s = scene_of();
    s.tick(2);
    let _ = s.players[0].link.take_raw();
    wear(&mut s, 39, "minecraft:iron_helmet", "minecraft:respiration");
    wear(
        &mut s,
        37,
        "minecraft:iron_leggings",
        "minecraft:swift_sneak",
    );
    s.tick(2);
    let states = all_synced(&s);
    assert_eq!(synced(&states, "minecraft:oxygen_bonus"), Some(3.0));
    let sneak = synced(&states, "minecraft:sneaking_speed").expect("synced");
    assert!((sneak - 0.75).abs() < 1e-5, "{sneak}");
}
