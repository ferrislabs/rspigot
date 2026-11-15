mod common;
#[path = "common/screen.rs"]
mod screen;

use common::*;
use rspigot_core::events::Inbound;
use rspigot_core::plugins::events::{PlayerItemMend, PlayerItemMendPatch};
use rspigot_core::plugins::{EventKind, EventPatch, EventPayload, InventoryOrder, ItemSpec, Order};
use rspigot_nbt::{Compound, Tag};
use screen::*;

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

fn changed(player: &Player) -> Option<i32> {
    let mut view = View::inventory();
    view.apply(&player.link.take());
    view.slots[36]
        .as_ref()
        .map(|stack| stack.components.get_i32("minecraft:damage").unwrap_or(0))
}

fn damage(player: &Player) -> i32 {
    changed(player).unwrap_or(0)
}

fn total(s: &Scene, player: &Player) -> i32 {
    s.server.player_experience(player.id).expect("a player").2
}

#[test]
fn an_orb_repairs_a_mending_item_two_durability_per_point_and_spends_its_points() {
    let mut s = scene_with(&["Alice"], Vec::new());
    let alice = s.players[0].clone();
    holding(&mut s, &alice, mended_pickaxe(100, "minecraft:mending"));
    orb(&mut s, 10);
    s.tick(3);
    assert_eq!(damage(&alice), 80);
    assert_eq!(total(&s, &alice), 0);
    assert!(s.server.experience_orbs().is_empty());
}

#[test]
fn what_the_item_cannot_take_goes_to_the_player() {
    let mut s = scene_with(&["Alice"], Vec::new());
    let alice = s.players[0].clone();
    holding(&mut s, &alice, mended_pickaxe(5, "minecraft:mending"));
    orb(&mut s, 10);
    s.tick(3);
    assert_eq!(damage(&alice), 0);
    assert_eq!(total(&s, &alice), 8);
}

#[test]
fn an_item_without_mending_leaves_every_point_to_the_player() {
    let mut s = scene_with(&["Alice"], Vec::new());
    let alice = s.players[0].clone();
    holding(&mut s, &alice, mended_pickaxe(100, "minecraft:unbreaking"));
    orb(&mut s, 10);
    s.tick(3);
    assert_eq!(total(&s, &alice), 10);
}

#[test]
fn player_item_mend_carries_the_item_and_a_patched_amount_is_what_repairs() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerItemMend)]);
    let alice = s.players[0].clone();
    holding(&mut s, &alice, mended_pickaxe(100, "minecraft:mending"));
    orb(&mut s, 10);
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::PlayerItemMend);
    let EventPayload::PlayerItemMend(PlayerItemMend {
        item,
        repair_amount,
        slot,
        ..
    }) = &asked.payload
    else {
        panic!("a mend");
    };
    assert_eq!(item.key, "minecraft:diamond_pickaxe");
    assert_eq!(*repair_amount, 20);
    assert_eq!(format!("{slot:?}"), "Hand");
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PlayerItemMend(PlayerItemMendPatch {
            repair_amount: Some(4),
        })),
    );
    let again = last_asked(&s.plugin, EventKind::PlayerItemMend);
    assert_ne!(again.id, asked.id);
    let EventPayload::PlayerItemMend(PlayerItemMend { repair_amount, .. }) = &again.payload else {
        panic!("a second mend");
    };
    assert_eq!(*repair_amount, 16);
    rule_on(&mut s.server, 1, again.id, true, None);
    s.tick(3);
    assert_eq!(damage(&alice), 96);
    assert_eq!(total(&s, &alice), 8);
}

#[test]
fn a_cancelled_mend_repairs_nothing_and_gives_the_points_to_the_player() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerItemMend)]);
    let alice = s.players[0].clone();
    holding(&mut s, &alice, mended_pickaxe(100, "minecraft:mending"));
    orb(&mut s, 10);
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::PlayerItemMend);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(3);
    assert_eq!(changed(&alice), None);
    assert_eq!(total(&s, &alice), 10);
}
