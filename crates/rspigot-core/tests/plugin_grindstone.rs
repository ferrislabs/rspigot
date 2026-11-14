mod common;
#[path = "common/screen.rs"]
mod screen;

use common::*;
use rspigot_core::events::ClickInput;
use rspigot_core::plugins::events::{PrepareGrindstone, PrepareGrindstonePatch};
use rspigot_core::plugins::{EventKind, EventPatch, EventPayload, ItemSpec};
use rspigot_domain::BlockPos;
use screen::*;

const GRINDSTONE: BlockPos = BlockPos::new(0, 64, 2);
const SWORD: &str = "minecraft:diamond_sword";
const HOTBAR_ZERO: i16 = 30;

fn enchanted_sword() -> ItemSpec {
    let mut sword = spec(SWORD, 1);
    sword.enchantments = vec![
        ("minecraft:sharpness".to_owned(), 3),
        ("minecraft:vanishing_curse".to_owned(), 1),
    ];
    sword
}

fn grindstone_scene(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> (Scene, Player, View) {
    let mut s = scene_with(&["Alice"], handlers);
    let alice = s.players[0].clone();
    set_block(&mut s, GRINDSTONE, "minecraft:grindstone");
    s.act(rspigot_core::plugins::Order::Inventory {
        player: alice.id,
        order: rspigot_core::plugins::InventoryOrder::Set {
            slot: 0,
            item: Some(enchanted_sword()),
        },
    });
    alice.link.take();
    let view = open_block(&mut s, &alice, GRINDSTONE);
    (s, alice, view)
}

fn place_sword(s: &mut Scene, alice: &Player, view: &mut View) {
    let take = predict(
        view,
        press(HOTBAR_ZERO, 0, ClickInput::Pickup),
        &[(HOTBAR_ZERO, None)],
        stack(SWORD, 1),
    );
    send_click(s, alice, view, take);
    let put = predict(
        view,
        press(0, 0, ClickInput::Pickup),
        &[(0, stack(SWORD, 1))],
        None,
    );
    send_click(s, alice, view, put);
}

#[test]
fn the_grindstone_strips_enchantments_and_taking_the_result_spawns_experience() {
    let (mut s, alice, mut view) = grindstone_scene(Vec::new());
    place_sword(&mut s, &alice, &mut view);
    assert_eq!(view.at(2), Some((SWORD, 1)));
    assert!(s.server.experience_orbs().is_empty());
    let take = predict(
        &mut view,
        press(2, 0, ClickInput::Pickup),
        &[(2, None)],
        stack(SWORD, 1),
    );
    send_click(&mut s, &alice, &mut view, take);
    assert_eq!(view.held(), Some((SWORD, 1)));
    assert_eq!(view.at(0), None);
    let total: i32 = s
        .server
        .experience_orbs()
        .iter()
        .map(|orb| orb.value * orb.count)
        .sum();
    assert!((12..=23).contains(&total), "{total}");
}

#[test]
fn prepare_grindstone_asks_with_the_stripped_result_and_a_patch_replaces_it() {
    let (mut s, alice, mut view) =
        grindstone_scene(vec![deciding(1, EventKind::PrepareGrindstone)]);
    place_sword(&mut s, &alice, &mut view);
    let asked = last_asked(&s.plugin, EventKind::PrepareGrindstone);
    let EventPayload::PrepareGrindstone(PrepareGrindstone {
        result, inventory, ..
    }) = &asked.payload
    else {
        panic!("a grindstone prepare");
    };
    assert_eq!(inventory.kind, "GRINDSTONE");
    assert_eq!(
        result.as_ref().map(|item| item.enchantments.clone()),
        Some(vec![("minecraft:vanishing_curse".to_owned(), 1)])
    );
    refresh(&alice, &mut view);
    assert_eq!(view.at(2), None);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::PrepareGrindstone(PrepareGrindstonePatch {
            result: Some(Some(spec("minecraft:stick", 1))),
        })),
    );
    refresh(&alice, &mut view);
    assert_eq!(view.at(2), Some(("minecraft:stick", 1)));
}
