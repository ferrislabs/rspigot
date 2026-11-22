mod common;

use common::*;
use rspigot_core::events::Inbound;
use rspigot_core::plugins::events::PlayerExpChange;
use rspigot_core::plugins::{EventKind, EventPayload, InventoryOrder, ItemSpec, Order};
use rspigot_nbt::{Compound, Tag};

fn mended_pickaxe(damage: i32, enchantment: &str) -> ItemSpec {
    let mut spec = ItemSpec::of("minecraft:diamond_pickaxe", 1);
    spec.enchantments = vec![(enchantment.to_owned(), 1)];
    let components = Compound::new().with("minecraft:damage", damage);
    rspigot_nbt::write_network(&mut spec.components, &Tag::Compound(components));
    spec
}

fn holding(s: &mut Scene, player: &Player, item: ItemSpec) {
    s.act(Order::Inventory {
        player: player.id,
        order: InventoryOrder::Set {
            slot: 0,
            item: Some(item),
        },
    });
    player.link.take();
}

fn orb(s: &mut Scene, value: i32) {
    s.server.handle(Inbound::Console(format!(
        "summon experience_orb 0.5 64 0.5 {{Value:{value}s}}"
    )));
    s.server.deliver();
}

fn total(s: &Scene, player: &Player) -> i32 {
    s.server.player_experience(player.id).expect("a player").2
}

fn exp_changes(s: &Scene) -> Vec<i32> {
    settled_for(&s.plugin, EventKind::PlayerExpChange)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::PlayerExpChange(PlayerExpChange { amount, .. }) => Some(amount),
            _ => None,
        })
        .collect()
}

#[test]
fn player_exp_change_carries_only_what_mending_left() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerExpChange)]);
    let alice = s.players[0].clone();
    holding(&mut s, &alice, mended_pickaxe(5, "minecraft:mending"));
    orb(&mut s, 10);
    s.tick(3);
    assert_eq!(exp_changes(&s), [8]);
    assert_eq!(total(&s, &alice), 8);
}

#[test]
fn player_exp_change_is_not_asked_when_mending_takes_every_point() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerExpChange)]);
    let alice = s.players[0].clone();
    holding(&mut s, &alice, mended_pickaxe(100, "minecraft:mending"));
    orb(&mut s, 10);
    s.tick(3);
    assert!(exp_changes(&s).is_empty());
    assert_eq!(total(&s, &alice), 0);
}

#[test]
fn player_exp_change_carries_the_whole_orb_without_mending() {
    let mut s = scene_with(&["Alice"], vec![observe(1, EventKind::PlayerExpChange)]);
    let alice = s.players[0].clone();
    holding(&mut s, &alice, mended_pickaxe(100, "minecraft:unbreaking"));
    orb(&mut s, 10);
    s.tick(3);
    assert_eq!(exp_changes(&s), [10]);
}
