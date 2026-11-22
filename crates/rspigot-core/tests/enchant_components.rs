mod common;

use std::num::NonZeroU8;

use common::*;
use rspigot_core::health::DamageSource;
use rspigot_core::plugins::{Answer, InventoryOrder, ItemSpec, Order, Question};
use rspigot_domain::damage::DamageType;
use rspigot_domain::item::ItemStack;
use rspigot_domain::{EntityId, ItemId};

fn stack(name: &str, enchantments: &[(&str, u8)]) -> ItemStack {
    let id = rspigot_data::registries::static_index("minecraft:item", name).unwrap();
    let mut stack = ItemStack::new(ItemId(id as u16), NonZeroU8::MIN);
    let levels: Vec<(String, u8)> = enchantments
        .iter()
        .map(|(name, level)| ((*name).to_owned(), *level))
        .collect();
    stack.set_enchantments(&levels);
    stack
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

fn entity_of(s: &mut Scene) -> EntityId {
    match s.ask(Question::Player(s.players[0].id)) {
        Answer::Player(state) => EntityId(state.player.entity_id),
        _ => panic!("expected the player"),
    }
}

#[test]
fn silk_touch_sets_the_experience_of_a_broken_block_to_zero() {
    let mut s = scene(&["Alice"]);
    let silk = stack("minecraft:diamond_pickaxe", &[("minecraft:silk_touch", 1)]);
    let plain = stack("minecraft:diamond_pickaxe", &[]);
    assert_eq!(s.server.enchanted_block_experience(Some(&silk), 6), 0);
    assert_eq!(s.server.enchanted_block_experience(Some(&plain), 6), 6);
    assert_eq!(s.server.enchanted_block_experience(None, 6), 6);
}

#[test]
fn unbreaking_spares_durability_through_the_item_damage_component() {
    let mut s = scene(&["Alice"]);
    let sword = stack("minecraft:diamond_sword", &[("minecraft:unbreaking", 3)]);
    let plain = stack("minecraft:diamond_sword", &[]);
    let worn: i32 = (0..2_000)
        .map(|_| s.server.enchanted_item_damage(&sword, 1))
        .sum();
    assert!((350..650).contains(&worn), "{worn}");
    assert_eq!(s.server.enchanted_item_damage(&plain, 3), 3);
}

#[test]
fn a_killer_without_a_mob_experience_enchantment_keeps_the_base_reward() {
    let mut s = scene(&["Alice"]);
    give(
        &mut s,
        0,
        "minecraft:diamond_sword",
        &[("minecraft:looting", 3)],
    );
    let alice = entity_of(&mut s);
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: "minecraft:zombie".into(),
        position: rspigot_domain::Vec3::new(4.5, 64.0, 4.5),
    }) else {
        panic!("expected a zombie");
    };
    assert_eq!(s.server.enchanted_mob_experience(alice, view.id, 5), 5);
}

#[test]
fn frost_walker_boots_make_the_wearer_immune_to_the_hot_floor_but_not_to_fire() {
    let mut s = scene(&["Alice"]);
    let alice = entity_of(&mut s);
    let hot = DamageSource::new(DamageType::HotFloor);
    let fire = DamageSource::new(DamageType::OnFire);
    assert!(!s.server.is_enchant_immune(alice, &hot));
    give(
        &mut s,
        36,
        "minecraft:diamond_boots",
        &[("minecraft:frost_walker", 1)],
    );
    assert!(s.server.is_enchant_immune(alice, &hot));
    assert!(!s.server.is_enchant_immune(alice, &fire));
}
