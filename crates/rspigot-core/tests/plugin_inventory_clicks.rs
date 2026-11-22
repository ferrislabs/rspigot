mod common;
#[path = "common/screen.rs"]
mod screen;

use common::*;
use rspigot_core::events::{ClickInput, ClientEvent};
use rspigot_core::plugins::events::{
    ClickKind, CraftItem, CraftItemPatch, EventResult, InventoryAction, InventoryClick,
    InventoryClickPatch, InventoryTypeSlotType,
};
use rspigot_core::plugins::{
    Answer, CustomInventoryOrder, EventKind, EventPatch, EventPayload, HandlerSpec, InventoryOwner,
    InventoryState, Order, PlayerOrder, Question, ServerOrder,
};
use rspigot_domain::BlockPos;
use screen::*;

const CHEST: BlockPos = BlockPos::new(2, 64, 0);
const TABLE: BlockPos = BlockPos::new(-2, 64, 0);
const SMITHY: BlockPos = BlockPos::new(0, 64, 2);
const FURNACE: BlockPos = BlockPos::new(0, 64, -2);
const CHEST_HOTBAR: i16 = 54;
const TABLE_HOTBAR: i16 = 37;
const SMITHY_HOTBAR: i16 = 31;
const FURNACE_HOTBAR: i16 = 30;
const PLAYER_HOTBAR: i16 = 36;
const MENU: u64 = 7;

fn click_of(event: &Fired) -> InventoryClick {
    match &event.payload {
        EventPayload::InventoryClick(click) => click.clone(),
        EventPayload::CraftItem(craft) => InventoryClick {
            action: craft.action,
            click: craft.click,
            clicked_inventory: craft.clicked_inventory.clone(),
            current_item: craft.current_item.clone(),
            cursor: craft.cursor.clone(),
            hotbar_button: craft.hotbar_button,
            inventory: craft.inventory.clone(),
            left_click: craft.left_click,
            raw_slot: craft.raw_slot,
            result: craft.result,
            right_click: craft.right_click,
            shift_click: craft.shift_click,
            slot: craft.slot,
            slot_type: craft.slot_type,
            view: craft.view.clone(),
            viewers: craft.viewers.clone(),
            who_clicked: craft.who_clicked.clone(),
        },
        EventPayload::SmithItem(smith) => InventoryClick {
            action: smith.action,
            click: smith.click,
            clicked_inventory: smith.clicked_inventory.clone(),
            current_item: smith.current_item.clone(),
            cursor: smith.cursor.clone(),
            hotbar_button: smith.hotbar_button,
            inventory: smith.inventory.clone(),
            left_click: smith.left_click,
            raw_slot: smith.raw_slot,
            result: smith.result,
            right_click: smith.right_click,
            shift_click: smith.shift_click,
            slot: smith.slot,
            slot_type: smith.slot_type,
            view: smith.view.clone(),
            viewers: smith.viewers.clone(),
            who_clicked: smith.who_clicked.clone(),
        },
        other => panic!("expected a click, got {other:?}"),
    }
}

fn clicks(s: &Scene) -> Vec<InventoryClick> {
    fired(&s.plugin)
        .iter()
        .filter(|event| event.phase == rspigot_core::plugins::Phase::Settled)
        .filter(|event| {
            matches!(
                event.payload,
                EventPayload::InventoryClick(_)
                    | EventPayload::CraftItem(_)
                    | EventPayload::SmithItem(_)
            )
        })
        .map(click_of)
        .collect()
}

fn last_click(s: &Scene) -> InventoryClick {
    clicks(s).pop().expect("a click was heard")
}

fn inventory(s: &mut Scene, player: &Player) -> InventoryState {
    match s.ask(Question::Inventory(player.id)) {
        Answer::Inventory(state) => state,
        other => panic!("expected an inventory, got {other:?}"),
    }
}

fn held(s: &mut Scene, player: &Player, slot: usize) -> Option<(String, i32)> {
    inventory(s, player).slots[slot]
        .as_ref()
        .map(|item| (item.key.clone(), item.count))
}

fn observing() -> Vec<HandlerSpec> {
    vec![observe(1, EventKind::InventoryClick)]
}

fn deciding_clicks() -> Vec<HandlerSpec> {
    vec![
        deciding(1, EventKind::InventoryClick),
        observe(3, EventKind::InventoryClick),
    ]
}

fn alice_scene(handlers: Vec<HandlerSpec>) -> (Scene, Player) {
    let s = scene_with(&["Alice", "Bob"], handlers);
    let alice = s.players[0].clone();
    (s, alice)
}

fn chest_with_diamond(s: &mut Scene, alice: &Player) -> View {
    set_block(s, CHEST, "minecraft:chest");
    give(s, alice, 0, "minecraft:diamond", 1);
    alice.link.take();
    let mut view = open_block(s, alice, CHEST);
    let take = predict(
        &mut view,
        press(CHEST_HOTBAR, 0, ClickInput::Pickup),
        &[(CHEST_HOTBAR, None)],
        stack("minecraft:diamond", 1),
    );
    send_click(s, alice, &mut view, take);
    let place = predict(
        &mut view,
        press(0, 0, ClickInput::Pickup),
        &[(0, stack("minecraft:diamond", 1))],
        None,
    );
    send_click(s, alice, &mut view, place);
    view
}

fn pickup(view: &mut View, slot: i16, taken: (&str, u8)) -> rspigot_core::events::ContainerClick {
    predict(
        view,
        press(slot, 0, ClickInput::Pickup),
        &[(slot, None)],
        stack(taken.0, taken.1),
    )
}

#[test]
fn a_click_in_each_window_reaches_the_plugins_with_its_view_and_slot() {
    let (mut s, alice) = alice_scene(observing());
    let mut chest = chest_with_diamond(&mut s, &alice);
    let placed = last_click(&s);
    assert_eq!(placed.click, ClickKind::Left);
    assert_eq!(placed.action, InventoryAction::PlaceAll);
    assert_eq!(placed.raw_slot, 0);
    assert_eq!(placed.slot, 0);
    assert_eq!(placed.slot_type, InventoryTypeSlotType::Container);
    assert_eq!(placed.inventory.kind, "CHEST");
    assert_eq!(placed.inventory.size, 27);
    assert_eq!(placed.inventory.owner, InventoryOwner::Block(CHEST));
    assert_eq!(placed.view.title, "Chest");
    assert_eq!(placed.cursor, Some(spec("minecraft:diamond", 1)));
    assert_eq!(placed.current_item, None);
    assert_eq!(placed.who_clicked.name, "Alice");
    assert_eq!(placed.viewers.len(), 1);
    let took = &clicks(&s)[0];
    assert_eq!(took.action, InventoryAction::PickupAll);
    assert_eq!(took.raw_slot, i32::from(CHEST_HOTBAR));
    assert_eq!(took.slot, 0);
    assert_eq!(took.slot_type, InventoryTypeSlotType::Quickbar);
    assert_eq!(
        took.clicked_inventory
            .as_ref()
            .map(|inventory| &inventory.owner),
        Some(&InventoryOwner::Player(alice.id))
    );
    assert_eq!(took.current_item, Some(spec("minecraft:diamond", 1)));
    close_window(&mut s, &alice, &mut chest);

    give(&mut s, &alice, 0, "minecraft:oak_log", 1);
    set_block(&mut s, TABLE, "minecraft:crafting_table");
    alice.link.take();
    let mut table = open_block(&mut s, &alice, TABLE);
    let take = pickup(&mut table, TABLE_HOTBAR, ("minecraft:oak_log", 1));
    send_click(&mut s, &alice, &mut table, take);
    let put = predict(
        &mut table,
        press(1, 0, ClickInput::Pickup),
        &[(1, stack("minecraft:oak_log", 1))],
        None,
    );
    send_click(&mut s, &alice, &mut table, put);
    let grid = last_click(&s);
    assert_eq!(grid.inventory.kind, "WORKBENCH");
    assert_eq!(grid.inventory.size, 10);
    assert_eq!(grid.inventory.owner, InventoryOwner::Block(TABLE));
    assert_eq!(grid.view.title, "Crafting");
    assert_eq!(grid.slot_type, InventoryTypeSlotType::Crafting);
    assert_eq!(grid.slot, 1);
    close_window(&mut s, &alice, &mut table);

    set_block(&mut s, SMITHY, "minecraft:smithing_table");
    give(&mut s, &alice, 0, "minecraft:diamond_sword", 1);
    alice.link.take();
    let mut smithy = open_block(&mut s, &alice, SMITHY);
    let take = pickup(&mut smithy, SMITHY_HOTBAR, ("minecraft:diamond_sword", 1));
    send_click(&mut s, &alice, &mut smithy, take);
    let smithing = last_click(&s);
    assert_eq!(smithing.inventory.kind, "SMITHING");
    assert_eq!(smithing.inventory.size, 4);
    assert_eq!(smithing.view.title, "Upgrade Gear");
    assert_eq!(smithing.slot_type, InventoryTypeSlotType::Quickbar);
    close_window(&mut s, &alice, &mut smithy);

    set_block(&mut s, FURNACE, "minecraft:furnace");
    give(&mut s, &alice, 0, "minecraft:coal", 3);
    alice.link.take();
    let mut furnace = open_block(&mut s, &alice, FURNACE);
    let take = pickup(&mut furnace, FURNACE_HOTBAR, ("minecraft:coal", 3));
    send_click(&mut s, &alice, &mut furnace, take);
    let fuel = predict(
        &mut furnace,
        press(1, 0, ClickInput::Pickup),
        &[(1, stack("minecraft:coal", 3))],
        None,
    );
    send_click(&mut s, &alice, &mut furnace, fuel);
    let fuelled = last_click(&s);
    assert_eq!(fuelled.inventory.kind, "FURNACE");
    assert_eq!(fuelled.inventory.owner, InventoryOwner::Block(FURNACE));
    assert_eq!(fuelled.view.title, "Furnace");
    assert_eq!(fuelled.slot_type, InventoryTypeSlotType::Fuel);
    close_window(&mut s, &alice, &mut furnace);

    give(&mut s, &alice, 0, "minecraft:stone", 2);
    alice.link.take();
    let mut own = View::inventory();
    let take = pickup(&mut own, PLAYER_HOTBAR, ("minecraft:stone", 2));
    send_click(&mut s, &alice, &mut own, take);
    let mine = last_click(&s);
    assert_eq!(mine.inventory.kind, "CRAFTING");
    assert_eq!(mine.inventory.size, 5);
    assert_eq!(mine.inventory.owner, InventoryOwner::Player(alice.id));
    assert_eq!(mine.view.title, "Crafting");
    assert_eq!(mine.view.bottom.kind, "PLAYER");
    assert_eq!(mine.slot, 0);
    assert_eq!(mine.slot_type, InventoryTypeSlotType::Quickbar);
    let armor = predict(
        &mut own,
        press(5, 0, ClickInput::Pickup),
        &[],
        stack("minecraft:stone", 2),
    );
    send_click(&mut s, &alice, &mut own, armor);
    let helmet = last_click(&s);
    assert_eq!(helmet.slot, 39);
    assert_eq!(helmet.slot_type, InventoryTypeSlotType::Armor);
    assert_eq!(helmet.action, InventoryAction::PlaceAll);
    close_window(&mut s, &alice, &mut own);

    s.act(Order::Server(ServerOrder::CreateInventory {
        id: MENU,
        rows: 1,
        title: "Box".to_owned(),
        locked: false,
    }));
    s.act(to(&alice, PlayerOrder::OpenInventory(MENU)));
    let mut menu = opened_view(&alice).expect("the plugin menu opens");
    let take = pickup(&mut menu, 36, ("minecraft:stone", 2));
    send_click(&mut s, &alice, &mut menu, take);
    let put = predict(
        &mut menu,
        press(0, 0, ClickInput::Pickup),
        &[(0, stack("minecraft:stone", 2))],
        None,
    );
    send_click(&mut s, &alice, &mut menu, put);
    let custom = last_click(&s);
    assert_eq!(custom.inventory.kind, "CHEST");
    assert_eq!(
        custom.inventory.owner,
        InventoryOwner::Custom {
            plugin: "tester".to_owned(),
            id: MENU
        }
    );
    assert_eq!(custom.view.title, "Box");
    assert_eq!(custom.action, InventoryAction::PlaceAll);
    assert!(s.warnings().is_empty());
}

#[test]
fn a_cancelled_click_leaves_the_server_and_the_client_unchanged_in_every_window() {
    let (mut s, alice) = alice_scene(observing());
    let mut chest = chest_with_diamond(&mut s, &alice);
    close_window(&mut s, &alice, &mut chest);
    let judge = connect_with(&mut s.server, 2, "judge", deciding_clicks());

    let mut chest = open_block(&mut s, &alice, CHEST);
    let take = pickup(&mut chest, 0, ("minecraft:diamond", 1));
    send_click(&mut s, &alice, &mut chest, take);
    assert_eq!(chest.at(0), None);
    let asked = last_asked(&judge, EventKind::InventoryClick);
    rule_on(&mut s.server, 2, asked.id, true, None);
    refresh(&alice, &mut chest);
    assert_eq!(chest.at(0), Some(("minecraft:diamond", 1)));
    assert_eq!(chest.held(), None);
    close_window(&mut s, &alice, &mut chest);
    let reopened = open_block(&mut s, &alice, CHEST);
    assert_eq!(reopened.at(0), Some(("minecraft:diamond", 1)));
    assert_eq!(reopened.held(), None);

    let mut chest = reopened;
    let take = pickup(&mut chest, 0, ("minecraft:diamond", 1));
    send_click(&mut s, &alice, &mut chest, take);
    let asked = last_asked(&judge, EventKind::InventoryClick);
    rule_on(&mut s.server, 2, asked.id, false, None);
    refresh(&alice, &mut chest);
    assert_eq!(chest.at(0), None);
    assert_eq!(chest.held(), Some(("minecraft:diamond", 1)));
    let back = predict(
        &mut chest,
        press(0, 0, ClickInput::Pickup),
        &[(0, stack("minecraft:diamond", 1))],
        None,
    );
    send_click(&mut s, &alice, &mut chest, back);
    let asked = last_asked(&judge, EventKind::InventoryClick);
    rule_on(&mut s.server, 2, asked.id, false, None);
    close_window(&mut s, &alice, &mut chest);

    give(&mut s, &alice, 9, "minecraft:stone", 5);
    alice.link.take();
    let mut own = View::inventory();
    let take = pickup(&mut own, 9, ("minecraft:stone", 5));
    send_click(&mut s, &alice, &mut own, take);
    let asked = last_asked(&judge, EventKind::InventoryClick);
    rule_on(&mut s.server, 2, asked.id, true, None);
    refresh(&alice, &mut own);
    assert_eq!(own.at(9), Some(("minecraft:stone", 5)));
    assert_eq!(own.held(), None);
    assert_eq!(
        held(&mut s, &alice, 9),
        Some(("minecraft:stone".to_owned(), 5))
    );

    set_block(&mut s, FURNACE, "minecraft:furnace");
    give(&mut s, &alice, 0, "minecraft:coal", 4);
    alice.link.take();
    let mut furnace = open_block(&mut s, &alice, FURNACE);
    let take = pickup(&mut furnace, FURNACE_HOTBAR, ("minecraft:coal", 4));
    send_click(&mut s, &alice, &mut furnace, take);
    let asked = last_asked(&judge, EventKind::InventoryClick);
    rule_on(&mut s.server, 2, asked.id, true, None);
    refresh(&alice, &mut furnace);
    assert_eq!(
        furnace.at(FURNACE_HOTBAR as usize),
        Some(("minecraft:coal", 4))
    );
    assert_eq!(furnace.held(), None);
    assert_eq!(
        held(&mut s, &alice, 0),
        Some(("minecraft:coal".to_owned(), 4))
    );
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn a_cancelled_click_with_a_cleared_slot_empties_the_slot_and_refuses_the_click() {
    let (mut s, alice) = alice_scene(deciding_clicks());
    give(&mut s, &alice, 9, "minecraft:stone", 5);
    alice.link.take();
    let mut own = View::inventory();
    let take = pickup(&mut own, 9, ("minecraft:stone", 5));
    send_click(&mut s, &alice, &mut own, take);
    let asked = last_asked(&s.plugin, EventKind::InventoryClick);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        true,
        Some(EventPatch::InventoryClick(InventoryClickPatch {
            current_item: Some(None),
            ..InventoryClickPatch::default()
        })),
    );
    refresh(&alice, &mut own);
    assert_eq!(held(&mut s, &alice, 9), None);
    assert_eq!(own.at(9), None);
    assert_eq!(own.held(), None);
    assert!(s.server.ground_items().is_empty());

    give(&mut s, &alice, 10, "minecraft:dirt", 2);
    refresh(&alice, &mut own);
    let take = pickup(&mut own, 10, ("minecraft:dirt", 2));
    send_click(&mut s, &alice, &mut own, take);
    let asked = last_asked(&s.plugin, EventKind::InventoryClick);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        true,
        Some(EventPatch::InventoryClick(InventoryClickPatch {
            cursor: Some(Some(spec("minecraft:gold_ingot", 3))),
            ..InventoryClickPatch::default()
        })),
    );
    refresh(&alice, &mut own);
    assert_eq!(own.at(10), Some(("minecraft:dirt", 2)));
    assert_eq!(own.held(), Some(("minecraft:gold_ingot", 3)));
    let settled = settled_for(&s.plugin, EventKind::InventoryClick);
    let last = settled.last().expect("the click settled");
    assert!(last.cancelled);
    let click = click_of(last);
    assert_eq!(click.cursor, Some(spec("minecraft:gold_ingot", 3)));
    assert_eq!(click.result, EventResult::Deny);
}

#[test]
fn a_spectator_click_is_asked_denied_and_a_later_handler_can_let_it_through() {
    let mut s = scene_with(
        &["Alice"],
        vec![
            deciding(1, EventKind::InventoryClick),
            HandlerSpec {
                priority: rspigot_core::plugins::Priority::High,
                ..deciding(2, EventKind::InventoryClick)
            },
            observe(3, EventKind::InventoryClick),
        ],
    );
    let alice = s.players[0].clone();
    give(&mut s, &alice, 9, "minecraft:stone", 5);
    s.act(to(
        &alice,
        PlayerOrder::SetGameMode(rspigot_domain::player::GameMode::Spectator),
    ));
    s.tick(1);
    alice.link.take();
    let mut own = View::inventory();
    let take = pickup(&mut own, 9, ("minecraft:stone", 5));
    send_click(&mut s, &alice, &mut own, take);
    let asked = last_asked(&s.plugin, EventKind::InventoryClick);
    assert_eq!(click_of(&asked).result, EventResult::Deny);
    refresh(&alice, &mut own);
    assert_eq!(own.held(), Some(("minecraft:stone", 5)));
    s.server
        .handle(rspigot_core::events::Inbound::PluginMessage {
            connection: 1,
            message: Box::new(rspigot_core::plugins::FromPlugin::Decision {
                event: asked.id,
                verdicts: vec![
                    rspigot_core::plugins::Verdict {
                        handler: 1,
                        cancel: true,
                        patch: None,
                    },
                    rspigot_core::plugins::Verdict {
                        handler: 2,
                        cancel: false,
                        patch: Some(EventPatch::InventoryClick(InventoryClickPatch {
                            result: Some(EventResult::Allow),
                            ..InventoryClickPatch::default()
                        })),
                    },
                ],
            }),
        });
    s.server.deliver();
    refresh(&alice, &mut own);
    assert_eq!(held(&mut s, &alice, 9), None);
    assert_eq!(own.held(), Some(("minecraft:stone", 5)));
    let settled = settled_for(&s.plugin, EventKind::InventoryClick);
    assert!(!settled.last().expect("settled").cancelled);
}

#[test]
fn a_click_is_refused_and_resent_when_another_viewer_changed_its_slot_and_a_patch_writes() {
    let (mut s, alice) = alice_scene(observing());
    let bob = s.players[1].clone();
    let mut chest = chest_with_diamond(&mut s, &alice);
    let judge = connect_with(&mut s.server, 2, "judge", deciding_clicks());
    let mut seen = open_block(&mut s, &bob, CHEST);

    let take = pickup(&mut chest, 0, ("minecraft:diamond", 1));
    send_click(&mut s, &alice, &mut chest, take);
    let alice_asked = last_asked(&judge, EventKind::InventoryClick);
    let take = pickup(&mut seen, 0, ("minecraft:diamond", 1));
    send_click(&mut s, &bob, &mut seen, take);
    let bob_asked = last_asked(&judge, EventKind::InventoryClick);
    rule_on(&mut s.server, 2, bob_asked.id, false, None);
    refresh(&bob, &mut seen);
    assert_eq!(seen.held(), Some(("minecraft:diamond", 1)));
    rule_on(
        &mut s.server,
        2,
        alice_asked.id,
        false,
        Some(EventPatch::InventoryClick(InventoryClickPatch {
            cursor: Some(Some(spec("minecraft:emerald", 1))),
            ..InventoryClickPatch::default()
        })),
    );
    refresh(&alice, &mut chest);
    assert_eq!(chest.held(), None);
    assert_eq!(chest.at(0), None);
    assert!(
        alice
            .link
            .take()
            .iter()
            .all(|event| !matches!(event, ClientEvent::CursorItem(Some(_))))
    );

    let back = predict(
        &mut seen,
        press(0, 0, ClickInput::Pickup),
        &[(0, stack("minecraft:diamond", 1))],
        None,
    );
    send_click(&mut s, &bob, &mut seen, back);
    rule_on(
        &mut s.server,
        2,
        last_asked(&judge, EventKind::InventoryClick).id,
        false,
        None,
    );
    refresh(&alice, &mut chest);
    assert_eq!(chest.at(0), Some(("minecraft:diamond", 1)));

    let take = pickup(&mut chest, 0, ("minecraft:diamond", 1));
    send_click(&mut s, &alice, &mut chest, take);
    let alice_asked = last_asked(&judge, EventKind::InventoryClick);
    let take = pickup(&mut seen, 0, ("minecraft:diamond", 1));
    send_click(&mut s, &bob, &mut seen, take);
    rule_on(
        &mut s.server,
        2,
        last_asked(&judge, EventKind::InventoryClick).id,
        false,
        None,
    );
    rule_on(&mut s.server, 2, alice_asked.id, false, None);
    refresh(&alice, &mut chest);
    refresh(&bob, &mut seen);
    assert_eq!(seen.held(), Some(("minecraft:diamond", 1)));
    assert_eq!(chest.held(), None);
    assert_eq!(chest.at(0), None);
    close_window(&mut s, &bob, &mut seen);
    close_window(&mut s, &alice, &mut chest);
    let diamonds = |state: &InventoryState| {
        state
            .slots
            .iter()
            .flatten()
            .filter(|item| item.key == "minecraft:diamond")
            .map(|item| item.count)
            .sum::<i32>()
    };
    let total = diamonds(&inventory(&mut s, &alice)) + diamonds(&inventory(&mut s, &bob));
    assert_eq!(total, 1);
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn a_craft_reaches_the_inventory_click_handlers_and_their_patch_changes_what_is_taken() {
    let mut s = scene_with(&["Alice"], deciding_clicks());
    let alice = s.players[0].clone();
    give(&mut s, &alice, 9, "minecraft:oak_log", 2);
    alice.link.take();
    let mut own = View::inventory();
    let take = pickup(&mut own, 9, ("minecraft:oak_log", 2));
    send_click(&mut s, &alice, &mut own, take);
    rule_on(
        &mut s.server,
        1,
        last_asked(&s.plugin, EventKind::InventoryClick).id,
        false,
        None,
    );
    let put = predict(
        &mut own,
        press(1, 0, ClickInput::Pickup),
        &[(1, stack("minecraft:oak_log", 2))],
        None,
    );
    send_click(&mut s, &alice, &mut own, put);
    rule_on(
        &mut s.server,
        1,
        last_asked(&s.plugin, EventKind::InventoryClick).id,
        false,
        None,
    );
    refresh(&alice, &mut own);
    assert_eq!(own.at(0), Some(("minecraft:oak_planks", 4)));

    let craft = predict(
        &mut own,
        press(0, 0, ClickInput::Pickup),
        &[(0, None)],
        stack("minecraft:oak_planks", 4),
    );
    send_click(&mut s, &alice, &mut own, craft);
    let asked = last_asked(&s.plugin, EventKind::CraftItem);
    let EventPayload::CraftItem(CraftItem {
        recipe,
        current_item,
        action,
        slot_type,
        ..
    }) = &asked.payload
    else {
        panic!("a craft is asked as CraftItem");
    };
    assert_eq!(recipe, "minecraft:oak_planks");
    assert_eq!(current_item, &Some(spec("minecraft:oak_planks", 4)));
    assert_eq!(*action, InventoryAction::PickupAll);
    assert_eq!(*slot_type, InventoryTypeSlotType::Result);
    let handlers = s
        .plugin
        .received()
        .into_iter()
        .find_map(|message| match message {
            rspigot_core::plugins::ToPlugin::Event { id, handlers, .. } if id == asked.id => {
                Some(handlers)
            }
            _ => None,
        });
    assert_eq!(handlers, Some(vec![1]));
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::InventoryClick(InventoryClickPatch {
            current_item: Some(Some(spec("minecraft:diamond", 1))),
            ..InventoryClickPatch::default()
        })),
    );
    refresh(&alice, &mut own);
    assert_eq!(own.held(), Some(("minecraft:diamond", 1)));
    assert_eq!(own.at(1), Some(("minecraft:oak_log", 1)));
    let settled = settled_for(&s.plugin, EventKind::CraftItem);
    assert!(matches!(
        &settled.last().expect("settled").payload,
        EventPayload::CraftItem(CraftItem { current_item: Some(item), .. }) if item.key == "minecraft:diamond"
    ));
}

#[test]
fn a_cancelled_craft_uses_no_ingredient_and_resends_the_whole_window() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::CraftItem)]);
    let alice = s.players[0].clone();
    set_block(&mut s, TABLE, "minecraft:crafting_table");
    give(&mut s, &alice, 0, "minecraft:oak_log", 1);
    alice.link.take();
    let mut table = open_block(&mut s, &alice, TABLE);
    let take = pickup(&mut table, TABLE_HOTBAR, ("minecraft:oak_log", 1));
    send_click(&mut s, &alice, &mut table, take);
    let put = predict(
        &mut table,
        press(1, 0, ClickInput::Pickup),
        &[(1, stack("minecraft:oak_log", 1))],
        None,
    );
    send_click(&mut s, &alice, &mut table, put);
    assert_eq!(table.at(0), Some(("minecraft:oak_planks", 4)));
    let craft = predict(
        &mut table,
        press(0, 0, ClickInput::QuickMove),
        &[(0, None), (1, None)],
        None,
    );
    send_click(&mut s, &alice, &mut table, craft);
    let asked = last_asked(&s.plugin, EventKind::CraftItem);
    alice.link.take();
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::CraftItem(CraftItemPatch {
            result: Some(EventResult::Deny),
            ..CraftItemPatch::default()
        })),
    );
    let events = alice.link.take();
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::ContainerContent { window, .. } if *window == table.window
    )));
    table.apply(&events);
    assert_eq!(table.at(0), Some(("minecraft:oak_planks", 4)));
    assert_eq!(table.at(1), Some(("minecraft:oak_log", 1)));
    assert!(
        inventory(&mut s, &alice)
            .slots
            .iter()
            .flatten()
            .all(|item| item.key != "minecraft:oak_planks")
    );
}

#[test]
fn a_click_in_a_locked_plugin_menu_is_born_cancelled_and_never_asked() {
    let (mut s, alice) = alice_scene(deciding_clicks());
    s.act(Order::Server(ServerOrder::CreateInventory {
        id: MENU,
        rows: 1,
        title: "Locked".to_owned(),
        locked: true,
    }));
    s.act(Order::CustomInventory {
        inventory: MENU,
        order: CustomInventoryOrder::Set {
            slot: 0,
            item: Some(spec("minecraft:diamond", 1)),
        },
    });
    s.act(to(&alice, PlayerOrder::OpenInventory(MENU)));
    let mut menu = opened_view(&alice).expect("the menu opens");
    let take = pickup(&mut menu, 0, ("minecraft:diamond", 1));
    send_click(&mut s, &alice, &mut menu, take);
    assert!(asked_for(&s.plugin, EventKind::InventoryClick).is_empty());
    let settled = settled_for(&s.plugin, EventKind::InventoryClick);
    assert_eq!(settled.len(), 1);
    assert!(settled[0].cancelled);
    assert_eq!(click_of(&settled[0]).result, EventResult::Deny);
    assert_eq!(menu.at(0), Some(("minecraft:diamond", 1)));
    assert_eq!(menu.held(), None);
}

struct Row {
    grid: Option<(&'static str, u8)>,
    slots: &'static [(i32, &'static str, i32)],
    cursor: Option<(&'static str, i32)>,
    creative: bool,
    slot: i16,
    button: i8,
    input: ClickInput,
    expected: Option<(ClickKind, InventoryAction)>,
}

const fn row(
    slots: &'static [(i32, &'static str, i32)],
    cursor: Option<(&'static str, i32)>,
    slot: i16,
    button: i8,
    input: ClickInput,
    expected: Option<(ClickKind, InventoryAction)>,
) -> Row {
    Row {
        grid: None,
        slots,
        cursor,
        creative: false,
        slot,
        button,
        input,
        expected,
    }
}

const STONE: &str = "minecraft:stone";
const DIRT: &str = "minecraft:dirt";
const LOG: &str = "minecraft:oak_log";
const PLANKS: &str = "minecraft:oak_planks";
const HELMET: &str = "minecraft:iron_helmet";
const CURSOR_SLOT: i32 = 20;
const GRID_SOURCE: i32 = 21;

fn spigot_slot(menu: i16) -> i32 {
    match menu {
        36..=44 => i32::from(menu) - 36,
        5..=8 => 44 - i32::from(menu),
        45 => 40,
        _ => i32::from(menu),
    }
}

fn play(s: &mut Scene, alice: &Player, row: &Row) -> Vec<InventoryClick> {
    let mut own = View::inventory();
    close_window(s, alice, &mut own);
    s.act(Order::Inventory {
        player: alice.id,
        order: rspigot_core::plugins::InventoryOrder::Clear,
    });
    let mode = if row.creative {
        rspigot_domain::player::GameMode::Creative
    } else {
        rspigot_domain::player::GameMode::Survival
    };
    s.act(to(alice, PlayerOrder::SetGameMode(mode)));
    for (slot, key, count) in row.slots {
        give(s, alice, *slot, key, *count);
    }
    if let Some((key, count)) = row.grid {
        give(s, alice, GRID_SOURCE, key, i32::from(count));
        let mut own = View::inventory();
        refresh(alice, &mut own);
        let take = pickup(&mut own, GRID_SOURCE as i16, (key, count));
        send_click(s, alice, &mut own, take);
        let put = predict(
            &mut own,
            press(1, 0, ClickInput::Pickup),
            &[(1, stack(key, count))],
            None,
        );
        send_click(s, alice, &mut own, put);
    }
    if let Some((key, count)) = row.cursor {
        give(s, alice, CURSOR_SLOT, key, count);
        let mut own = View::inventory();
        refresh(alice, &mut own);
        let take = predict(
            &mut own,
            press(CURSOR_SLOT as i16, 0, ClickInput::Pickup),
            &[(CURSOR_SLOT as i16, None)],
            stack(key, u8::try_from(count).unwrap_or(1)),
        );
        send_click(s, alice, &mut own, take);
    }
    let before = clicks(s).len();
    let mut own = View::inventory();
    refresh(alice, &mut own);
    send_click(s, alice, &mut own, press(row.slot, row.button, row.input));
    clicks(s).split_off(before)
}

#[test]
fn every_row_of_the_click_table_is_classified_as_spigot_does() {
    use ClickInput::{Clone, Pickup, PickupAll, QuickCraft, QuickMove, Swap, Throw};
    use ClickKind as K;
    use InventoryAction as A;
    let stone5: &[(i32, &str, i32)] = &[(9, STONE, 5)];
    let rows = [
        row(
            &[],
            Some((STONE, 5)),
            -999,
            0,
            Pickup,
            Some((K::Left, A::DropAllCursor)),
        ),
        row(
            &[],
            Some((STONE, 5)),
            -999,
            1,
            Pickup,
            Some((K::Right, A::DropOneCursor)),
        ),
        row(&[], None, -999, 0, Pickup, Some((K::Left, A::Nothing))),
        row(&[], None, -1, 0, Pickup, Some((K::Left, A::Nothing))),
        row(
            &[],
            Some((STONE, 5)),
            9,
            0,
            Pickup,
            Some((K::Left, A::PlaceAll)),
        ),
        row(
            &[],
            Some((STONE, 5)),
            9,
            1,
            Pickup,
            Some((K::Right, A::PlaceOne)),
        ),
        row(&[], None, 9, 0, Pickup, Some((K::Left, A::Nothing))),
        row(stone5, None, 9, 0, Pickup, Some((K::Left, A::PickupAll))),
        row(stone5, None, 9, 1, Pickup, Some((K::Right, A::PickupHalf))),
        row(
            stone5,
            Some((STONE, 3)),
            9,
            0,
            Pickup,
            Some((K::Left, A::PlaceAll)),
        ),
        row(
            stone5,
            Some((STONE, 3)),
            9,
            1,
            Pickup,
            Some((K::Right, A::PlaceOne)),
        ),
        row(
            &[(9, STONE, 62)],
            Some((STONE, 5)),
            9,
            0,
            Pickup,
            Some((K::Left, A::PlaceSome)),
        ),
        row(
            &[(9, STONE, 64)],
            Some((STONE, 5)),
            9,
            0,
            Pickup,
            Some((K::Left, A::Nothing)),
        ),
        row(
            &[(9, STONE, 65)],
            Some((STONE, 1)),
            9,
            0,
            Pickup,
            Some((K::Left, A::PickupOne)),
        ),
        row(
            &[(9, STONE, 70)],
            Some((STONE, 1)),
            9,
            0,
            Pickup,
            Some((K::Left, A::PickupSome)),
        ),
        row(
            stone5,
            Some((DIRT, 3)),
            9,
            0,
            Pickup,
            Some((K::Left, A::SwapWithCursor)),
        ),
        row(
            &[(39, "minecraft:diamond_helmet", 1)],
            Some((HELMET, 2)),
            5,
            0,
            Pickup,
            Some((K::Left, A::Nothing)),
        ),
        Row {
            grid: Some((LOG, 1)),
            ..row(
                &[],
                Some((PLANKS, 2)),
                0,
                0,
                Pickup,
                Some((K::Left, A::PickupAll)),
            )
        },
        Row {
            grid: Some((LOG, 1)),
            ..row(
                &[],
                Some((STONE, 2)),
                0,
                0,
                Pickup,
                Some((K::Left, A::Nothing)),
            )
        },
        row(stone5, None, 9, 2, Pickup, Some((K::Unknown, A::Unknown))),
        row(
            stone5,
            None,
            9,
            0,
            QuickMove,
            Some((K::ShiftLeft, A::MoveToOtherInventory)),
        ),
        row(
            stone5,
            None,
            9,
            1,
            QuickMove,
            Some((K::ShiftRight, A::MoveToOtherInventory)),
        ),
        row(&[], None, 9, 0, QuickMove, Some((K::ShiftLeft, A::Nothing))),
        row(
            &[],
            None,
            -999,
            0,
            QuickMove,
            Some((K::ShiftLeft, A::Nothing)),
        ),
        row(
            stone5,
            None,
            9,
            2,
            QuickMove,
            Some((K::Unknown, A::Unknown)),
        ),
        row(
            stone5,
            None,
            9,
            0,
            Swap,
            Some((K::NumberKey, A::HotbarSwap)),
        ),
        row(
            &[(9, STONE, 5), (0, DIRT, 1)],
            None,
            9,
            0,
            Swap,
            Some((K::NumberKey, A::HotbarSwap)),
        ),
        Row {
            grid: Some((STONE, 5)),
            ..row(
                &[(0, DIRT, 1)],
                None,
                1,
                0,
                Swap,
                Some((K::NumberKey, A::HotbarMoveAndReadd)),
            )
        },
        row(
            &[(0, DIRT, 1)],
            None,
            9,
            0,
            Swap,
            Some((K::NumberKey, A::HotbarSwap)),
        ),
        row(
            &[(0, DIRT, 1)],
            None,
            0,
            0,
            Swap,
            Some((K::NumberKey, A::Nothing)),
        ),
        row(
            stone5,
            None,
            9,
            40,
            Swap,
            Some((K::SwapOffhand, A::HotbarSwap)),
        ),
        row(stone5, None, 9, 9, Swap, Some((K::Unknown, A::Unknown))),
        Row {
            creative: true,
            ..row(stone5, None, 9, 2, Clone, Some((K::Middle, A::CloneStack)))
        },
        row(stone5, None, 9, 2, Clone, Some((K::Middle, A::Nothing))),
        row(stone5, None, 9, 0, Clone, Some((K::Unknown, A::Unknown))),
        row(stone5, None, 9, 0, Throw, Some((K::Drop, A::DropOneSlot))),
        row(
            stone5,
            None,
            9,
            1,
            Throw,
            Some((K::ControlDrop, A::DropAllSlot)),
        ),
        row(&[], None, 9, 0, Throw, Some((K::Drop, A::Nothing))),
        row(&[], None, -999, 0, Throw, Some((K::Left, A::Nothing))),
        row(&[], None, -999, 1, Throw, Some((K::Right, A::Nothing))),
        row(stone5, None, 9, 2, Throw, Some((K::Unknown, A::Unknown))),
        row(
            &[(10, STONE, 5)],
            Some((STONE, 2)),
            9,
            0,
            PickupAll,
            Some((K::DoubleClick, A::CollectToCursor)),
        ),
        row(
            &[(10, STONE, 5)],
            None,
            9,
            0,
            PickupAll,
            Some((K::DoubleClick, A::Nothing)),
        ),
        row(
            &[(10, STONE, 5)],
            Some((STONE, 2)),
            -999,
            0,
            PickupAll,
            Some((K::DoubleClick, A::Nothing)),
        ),
        row(stone5, None, 9, 0, QuickCraft, None),
    ];
    let (mut s, alice) = alice_scene(observing());
    for (index, row) in rows.iter().enumerate() {
        let heard = play(&mut s, &alice, row);
        let classified: Vec<(ClickKind, InventoryAction)> = heard
            .iter()
            .map(|click| (click.click, click.action))
            .collect();
        match row.expected {
            Some(expected) => assert_eq!(classified, [expected], "row {index}"),
            None => assert!(classified.is_empty(), "row {index}"),
        }
        if let Some(click) = heard.first() {
            assert_eq!(click.raw_slot, i32::from(row.slot), "row {index}");
            if row.slot >= 0 {
                assert_eq!(click.slot, spigot_slot(row.slot), "row {index}");
            }
            let key = (click.click == ClickKind::NumberKey).then_some(i32::from(row.button));
            assert_eq!(click.hotbar_button, key.unwrap_or(-1), "row {index}");
        }
    }
}

#[test]
fn a_menu_button_waits_behind_a_parked_click_on_the_result() {
    let mut s = scene_with(&["Alice"], deciding_clicks());
    let alice = s.players[0].clone();
    let cutter = BlockPos::new(-2, 64, 2);
    set_block(&mut s, cutter, "minecraft:stonecutter");
    give(&mut s, &alice, 0, "minecraft:stone", 2);
    alice.link.take();
    let mut view = open_block(&mut s, &alice, cutter);
    let take = pickup(&mut view, 29, ("minecraft:stone", 2));
    send_click(&mut s, &alice, &mut view, take);
    rule_on(
        &mut s.server,
        1,
        last_asked(&s.plugin, EventKind::InventoryClick).id,
        false,
        None,
    );
    let put = predict(
        &mut view,
        press(0, 0, ClickInput::Pickup),
        &[(0, stack("minecraft:stone", 2))],
        None,
    );
    send_click(&mut s, &alice, &mut view, put);
    rule_on(
        &mut s.server,
        1,
        last_asked(&s.plugin, EventKind::InventoryClick).id,
        false,
        None,
    );
    let window = view.window;
    let press_button = |s: &mut Scene, button: i32| {
        s.server.handle(rspigot_core::events::Inbound::Input {
            player: alice.id,
            input: rspigot_core::events::PlayerInput::ContainerButtonClick { window, button },
        });
        s.server.deliver();
    };
    press_button(&mut s, 0);
    refresh(&alice, &mut view);
    let first = view.at(1).expect("the first cut shows");
    let take = pickup(&mut view, 1, first);
    send_click(&mut s, &alice, &mut view, take);
    let parked = last_asked(&s.plugin, EventKind::InventoryClick);
    let seen = click_of(&parked)
        .current_item
        .expect("the click sees the cut");
    press_button(&mut s, 1);
    rule_on(&mut s.server, 1, parked.id, false, None);
    refresh(&alice, &mut view);
    assert_eq!(
        view.held()
            .map(|(key, count)| (key.to_owned(), i32::from(count))),
        Some((seen.key, seen.count))
    );
}

#[test]
fn a_craft_whose_recipe_changed_while_it_waited_is_refused_and_resent() {
    let mut s = scene_with(&["Alice"], deciding_clicks());
    let alice = s.players[0].clone();
    give(&mut s, &alice, 9, "minecraft:oak_log", 2);
    alice.link.take();
    let mut own = View::inventory();
    let take = pickup(&mut own, 9, ("minecraft:oak_log", 2));
    send_click(&mut s, &alice, &mut own, take);
    rule_on(
        &mut s.server,
        1,
        last_asked(&s.plugin, EventKind::InventoryClick).id,
        false,
        None,
    );
    let put = predict(
        &mut own,
        press(1, 0, ClickInput::Pickup),
        &[(1, stack("minecraft:oak_log", 2))],
        None,
    );
    send_click(&mut s, &alice, &mut own, put);
    rule_on(
        &mut s.server,
        1,
        last_asked(&s.plugin, EventKind::InventoryClick).id,
        false,
        None,
    );
    let craft = predict(
        &mut own,
        press(0, 0, ClickInput::Pickup),
        &[(0, None)],
        stack("minecraft:oak_planks", 4),
    );
    send_click(&mut s, &alice, &mut own, craft);
    let parked = last_asked(&s.plugin, EventKind::CraftItem);
    s.server.handle(rspigot_core::events::Inbound::Console(
        "clear Alice".to_owned(),
    ));
    s.server.deliver();
    give(&mut s, &alice, 9, "minecraft:birch_log", 1);
    alice.link.take();
    rule_on(&mut s.server, 1, parked.id, false, None);
    let events = alice.link.take();
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ClientEvent::InventoryContent { .. }))
    );
    own.apply(&events);
    assert_eq!(own.held(), None);
    assert_eq!(
        inventory(&mut s, &alice)
            .slots
            .iter()
            .flatten()
            .filter(|item| item.key.ends_with("_planks"))
            .count(),
        0
    );
}
