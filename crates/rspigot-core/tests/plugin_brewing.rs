mod common;
#[path = "common/screen.rs"]
mod screen;

use common::*;
use rspigot_core::events::ClickInput;
use rspigot_core::plugins::events::{
    Brew, BrewingStandFuel, BrewingStandFuelPatch, BrewingStart, BrewingStartPatch,
};
use rspigot_core::plugins::{EventKind, EventPatch, EventPayload};
use rspigot_domain::BlockPos;
use rspigot_domain::item::ItemStack;
use rspigot_domain::potion::{self, PotionContents};
use rspigot_items::ItemKind;
use screen::*;

const STAND: BlockPos = BlockPos::new(0, 64, 2);
const WART: &str = "minecraft:nether_wart";
const BLAZE: &str = "minecraft:blaze_powder";
const HOTBAR_ZERO: i16 = 32;

fn water() -> ItemStack {
    potion::stack(ItemKind::Potion, "minecraft:water")
}

fn move_stack(s: &mut Scene, alice: &Player, view: &mut View, from: i16, to: i16, item: ItemStack) {
    let slot = Some(item);
    let take = predict(
        view,
        press(from, 0, ClickInput::Pickup),
        &[(from, None)],
        slot.clone(),
    );
    send_click(s, alice, view, take);
    let put = predict(view, press(to, 0, ClickInput::Pickup), &[(to, slot)], None);
    send_click(s, alice, view, put);
}

fn loaded(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> (Scene, Player, View) {
    let mut s = scene_with(&["Alice"], handlers);
    let alice = s.players[0].clone();
    set_block(&mut s, STAND, "minecraft:brewing_stand");
    give(&mut s, &alice, 1, WART, 1);
    give(&mut s, &alice, 2, BLAZE, 1);
    alice.link.take();
    let mut view = open_block(&mut s, &alice, STAND);
    let item = |key: &str| stack(key, 1).expect("an item");
    move_stack(&mut s, &alice, &mut view, HOTBAR_ZERO + 1, 3, item(WART));
    move_stack(&mut s, &alice, &mut view, HOTBAR_ZERO + 2, 4, item(BLAZE));
    (s, alice, view)
}

fn put_bottle(s: &mut Scene, alice: &Player, view: &mut View) {
    s.act(rspigot_core::plugins::Order::Inventory {
        player: alice.id,
        order: rspigot_core::plugins::InventoryOrder::Set {
            slot: 0,
            item: Some({
                let mut spec = spec("minecraft:potion", 1);
                let contents = rspigot_nbt::Compound::new().with(
                    "minecraft:potion_contents",
                    rspigot_nbt::Compound::new().with("potion", "minecraft:water"),
                );
                rspigot_nbt::write_network(
                    &mut spec.components,
                    &rspigot_nbt::Tag::Compound(contents),
                );
                spec
            }),
        },
    });
    alice.link.take();
    refresh(alice, view);
    move_stack(s, alice, view, HOTBAR_ZERO, 0, water());
}

fn datum(view: &View, id: i16) -> i16 {
    view.data
        .iter()
        .rev()
        .find(|(known, _)| *known == id)
        .map_or(0, |(_, value)| *value)
}

fn potion_in(view: &View, slot: usize) -> Option<String> {
    view.slots[slot]
        .as_ref()
        .and_then(|stack| PotionContents::of(stack).potion.map(str::to_owned))
}

#[test]
fn a_stand_with_fuel_brews_water_into_awkward_in_four_hundred_ticks() {
    let (mut s, alice, mut view) = loaded(Vec::new());
    put_bottle(&mut s, &alice, &mut view);
    s.tick(2);
    refresh(&alice, &mut view);
    assert_eq!(datum(&view, 0), 399);
    assert_eq!(datum(&view, 1), 19);
    assert_eq!(potion_in(&view, 0).as_deref(), Some("minecraft:water"));
    s.tick(400);
    refresh(&alice, &mut view);
    assert_eq!(potion_in(&view, 0).as_deref(), Some("minecraft:awkward"));
    assert_eq!(view.at(3), None);
    assert_eq!(datum(&view, 0), 0);
}

#[test]
fn a_stand_without_a_bottle_does_nothing() {
    let (mut s, alice, mut view) = loaded(Vec::new());
    s.tick(5);
    refresh(&alice, &mut view);
    assert_eq!(datum(&view, 0), 0);
    assert_eq!(datum(&view, 1), 20);
}

#[test]
fn brewing_stand_fuel_asks_with_the_powder_and_a_patch_sets_the_power() {
    let (mut s, alice, mut view) = loaded(vec![deciding(1, EventKind::BrewingStandFuel)]);
    put_bottle(&mut s, &alice, &mut view);
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::BrewingStandFuel);
    let EventPayload::BrewingStandFuel(BrewingStandFuel {
        block,
        consuming,
        fuel,
        fuel_power,
    }) = &asked.payload
    else {
        panic!("a fuel ask");
    };
    assert_eq!(*block, STAND);
    assert!(*consuming);
    assert_eq!(fuel.key, BLAZE);
    assert_eq!(*fuel_power, 20);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::BrewingStandFuel(BrewingStandFuelPatch {
            consuming: Some(false),
            fuel_power: Some(3),
        })),
    );
    s.tick(1);
    refresh(&alice, &mut view);
    assert_eq!(view.at(4), Some((BLAZE, 1)));
    assert_eq!(datum(&view, 1), 2);
}

#[test]
fn a_cancelled_fuel_ask_keeps_the_stand_cold() {
    let (mut s, alice, mut view) = loaded(vec![deciding(1, EventKind::BrewingStandFuel)]);
    put_bottle(&mut s, &alice, &mut view);
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::BrewingStandFuel);
    rule_on(&mut s.server, 1, asked.id, true, None);
    refresh(&alice, &mut view);
    assert_eq!(view.at(4), Some((BLAZE, 1)));
    assert_eq!(datum(&view, 0), 0);
}

#[test]
fn brewing_start_carries_the_ingredient_and_a_patch_sets_the_time() {
    let (mut s, alice, mut view) = loaded(vec![deciding(1, EventKind::BrewingStart)]);
    put_bottle(&mut s, &alice, &mut view);
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::BrewingStart);
    let EventPayload::BrewingStart(BrewingStart {
        block,
        source,
        total_brew_time,
    }) = &asked.payload
    else {
        panic!("a start ask");
    };
    assert_eq!(*block, STAND);
    assert_eq!(source.key, WART);
    assert_eq!(*total_brew_time, 400);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::BrewingStart(BrewingStartPatch {
            total_brew_time: Some(5),
        })),
    );
    s.tick(1);
    refresh(&alice, &mut view);
    assert_eq!(datum(&view, 0), 4);
    assert_eq!(datum(&view, 2), 5);
}

#[test]
fn brew_asks_with_the_results_and_a_cancel_keeps_the_inputs() {
    let (mut s, alice, mut view) = loaded(vec![deciding(1, EventKind::Brew)]);
    put_bottle(&mut s, &alice, &mut view);
    s.tick(401);
    let asked = last_asked(&s.plugin, EventKind::Brew);
    let EventPayload::Brew(Brew {
        block,
        fuel_level,
        results,
        contents,
    }) = &asked.payload
    else {
        panic!("a brew ask");
    };
    assert_eq!(*block, STAND);
    assert_eq!(*fuel_level, 19);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].key, "minecraft:potion");
    assert_eq!(contents.kind, "BREWING");
    rule_on(&mut s.server, 1, asked.id, true, None);
    refresh(&alice, &mut view);
    assert_eq!(potion_in(&view, 0).as_deref(), Some("minecraft:water"));
    assert_eq!(view.at(3), Some((WART, 1)));
}

#[test]
fn brew_allowed_makes_the_potion() {
    let (mut s, alice, mut view) = loaded(vec![deciding(1, EventKind::Brew)]);
    put_bottle(&mut s, &alice, &mut view);
    s.tick(401);
    let asked = last_asked(&s.plugin, EventKind::Brew);
    rule_on(&mut s.server, 1, asked.id, false, None);
    refresh(&alice, &mut view);
    assert_eq!(potion_in(&view, 0).as_deref(), Some("minecraft:awkward"));
    assert_eq!(view.at(3), None);
}

#[test]
fn the_block_shows_a_bottle_in_each_filled_slot() {
    let (mut s, alice, mut view) = loaded(Vec::new());
    let before = s.server.block_state_at(STAND).expect("a stand");
    assert_eq!(before.property("has_bottle_0"), Some("false"));
    put_bottle(&mut s, &alice, &mut view);
    s.tick(2);
    let after = s.server.block_state_at(STAND).expect("a stand");
    assert_eq!(after.property("has_bottle_0"), Some("true"));
    assert_eq!(after.property("has_bottle_1"), Some("false"));
}
