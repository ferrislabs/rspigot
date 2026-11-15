mod common;
#[path = "common/screen.rs"]
mod screen;
#[path = "common/seeded.rs"]
mod seeded;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::plugins::events::{
    HopperInventorySearchContainerType, HopperInventorySearchPatch, InventoryMoveItemPatch,
};
use rspigot_core::plugins::{
    Answer, BlockOrder, EventKind, EventPatch, EventPayload, InventoryOrder, InventoryOwner,
    ItemSpec, Order, Question,
};
use rspigot_domain::BlockPos;
use seeded::{Seed, contents, world};

const CHEST: BlockPos = BlockPos::new(3, 64, 3);
const HOPPER: BlockPos = BlockPos::new(3, 65, 3);
const SPARE: BlockPos = BlockPos::new(5, 64, 3);

fn pushing(items: &[(&'static str, u8)]) -> Vec<Seed> {
    let mut hopper = Seed::block(HOPPER, BlockKind::Hopper).with("facing", "down");
    for (key, count) in items {
        hopper = hopper.holding(key, *count);
    }
    vec![Seed::block(CHEST, BlockKind::Chest), hopper]
}

fn put(s: &mut Scene, pos: BlockPos, kind: &str) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set(kind.to_owned()),
    });
}

fn prop(s: &mut Scene, pos: BlockPos, name: &str) -> String {
    match s.ask(Question::Block(pos)) {
        Answer::Block(state) => state
            .properties
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
            .unwrap_or_default(),
        other => panic!("expected a block, got {other:?}"),
    }
}

#[test]
fn a_hopper_moves_one_item_into_the_container_it_faces_then_waits_eight_ticks() {
    let mut s = world(pushing(&[("minecraft:stone", 3)]), Vec::new());
    let alice = s.players[0].clone();
    s.tick(1);
    assert_eq!(
        contents(&mut s, &alice, CHEST, 1),
        [Some(("minecraft:stone", 1))]
    );
    s.tick(7);
    assert_eq!(
        contents(&mut s, &alice, CHEST, 1),
        [Some(("minecraft:stone", 1))]
    );
    s.tick(1);
    assert_eq!(
        contents(&mut s, &alice, CHEST, 1),
        [Some(("minecraft:stone", 2))]
    );
    assert_eq!(
        contents(&mut s, &alice, HOPPER, 1),
        [Some(("minecraft:stone", 1))]
    );
}

#[test]
fn a_hopper_pulls_one_item_from_the_container_above_it() {
    let seeds = vec![
        Seed::block(HOPPER, BlockKind::Hopper).with("facing", "east"),
        Seed::block(HOPPER.offset(0, 1, 0), BlockKind::Chest).holding("minecraft:dirt", 2),
    ];
    let mut s = world(seeds, Vec::new());
    let alice = s.players[0].clone();
    s.tick(1);
    assert_eq!(
        contents(&mut s, &alice, HOPPER, 2),
        [Some(("minecraft:dirt", 1)), None]
    );
}

#[test]
fn a_powered_hopper_is_locked() {
    let mut s = world(pushing(&[("minecraft:stone", 3)]), Vec::new());
    let alice = s.players[0].clone();
    put(&mut s, HOPPER.offset(1, 0, 0), "minecraft:redstone_block");
    assert_eq!(prop(&mut s, HOPPER, "enabled"), "false");
    s.tick(10);
    assert_eq!(contents(&mut s, &alice, CHEST, 1), [None]);
    put(&mut s, HOPPER.offset(1, 0, 0), "minecraft:air");
    assert_eq!(prop(&mut s, HOPPER, "enabled"), "true");
    s.tick(1);
    assert_eq!(
        contents(&mut s, &alice, CHEST, 1),
        [Some(("minecraft:stone", 1))]
    );
}

fn drop_at_feet(s: &mut Scene, key: &str, count: i32) {
    let alice = s.players[0].clone();
    for slot in (0..=35).chain([40]) {
        s.act(Order::Inventory {
            player: alice.id,
            order: InventoryOrder::Set {
                slot,
                item: Some(ItemSpec::of("minecraft:cobblestone", 64)),
            },
        });
    }
    s.act(Order::Inventory {
        player: alice.id,
        order: InventoryOrder::Add(ItemSpec::of(key, count)),
    });
}

const FLOOR_HOPPER: BlockPos = BlockPos::new(0, 63, 0);

fn suction() -> Vec<Seed> {
    vec![Seed::block(FLOOR_HOPPER, BlockKind::Hopper).with("facing", "down")]
}

#[test]
fn a_hopper_sucks_in_the_items_lying_above_it() {
    let mut s = world(suction(), Vec::new());
    let alice = s.players[0].clone();
    drop_at_feet(&mut s, "minecraft:diamond", 5);
    s.tick(2);
    assert_eq!(
        contents(&mut s, &alice, FLOOR_HOPPER, 1),
        [Some(("minecraft:diamond", 5))]
    );
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn an_inventory_pickup_event_describes_the_hopper_and_the_item() {
    let mut s = world(suction(), vec![observe(1, EventKind::InventoryPickupItem)]);
    drop_at_feet(&mut s, "minecraft:diamond", 5);
    s.tick(2);
    let heard = settled_for(&s.plugin, EventKind::InventoryPickupItem);
    let EventPayload::InventoryPickupItem(event) = &heard.last().expect("a pickup").payload else {
        panic!("a pickup event");
    };
    assert_eq!(event.inventory.owner, InventoryOwner::Block(FLOOR_HOPPER));
    assert_eq!(event.inventory.kind, "HOPPER");
    assert_eq!(event.item.kind, "minecraft:item");
}

#[test]
fn a_cancelled_inventory_pickup_leaves_the_item_on_the_ground() {
    let mut s = world(suction(), vec![deciding(1, EventKind::InventoryPickupItem)]);
    let alice = s.players[0].clone();
    drop_at_feet(&mut s, "minecraft:diamond", 5);
    s.tick(2);
    let asked = last_asked(&s.plugin, EventKind::InventoryPickupItem);
    rule_on(&mut s.server, 1, asked.id, true, None);
    s.tick(2);
    assert_eq!(contents(&mut s, &alice, FLOOR_HOPPER, 1), [None]);
    assert_eq!(s.server.ground_items().len(), 1);
}

#[test]
fn an_inventory_move_event_names_the_hopper_and_both_inventories() {
    let mut s = world(
        pushing(&[("minecraft:stone", 3)]),
        vec![observe(1, EventKind::InventoryMoveItem)],
    );
    s.tick(1);
    let heard = settled_for(&s.plugin, EventKind::InventoryMoveItem);
    let EventPayload::InventoryMoveItem(event) = &heard.first().expect("a move").payload else {
        panic!("a move event");
    };
    assert_eq!(event.source.owner, InventoryOwner::Block(HOPPER));
    assert_eq!(event.destination.owner, InventoryOwner::Block(CHEST));
    assert_eq!(event.initiator.owner, InventoryOwner::Block(HOPPER));
    assert_eq!(event.destination.kind, "CHEST");
    assert_eq!(
        (event.item.key.as_str(), event.item.count),
        ("minecraft:stone", 1)
    );
}

#[test]
fn a_cancelled_inventory_move_keeps_the_item_and_restarts_the_cooldown() {
    let mut s = world(
        pushing(&[("minecraft:stone", 3)]),
        vec![deciding(1, EventKind::InventoryMoveItem)],
    );
    let alice = s.players[0].clone();
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::InventoryMoveItem);
    rule_on(&mut s.server, 1, asked.id, true, None);
    assert_eq!(contents(&mut s, &alice, CHEST, 1), [None]);
    assert_eq!(
        contents(&mut s, &alice, HOPPER, 1),
        [Some(("minecraft:stone", 3))]
    );
    s.tick(4);
    assert_eq!(asked_for(&s.plugin, EventKind::InventoryMoveItem).len(), 1);
}

#[test]
fn a_patched_item_is_what_reaches_the_destination() {
    let mut s = world(
        pushing(&[("minecraft:stone", 3)]),
        vec![deciding(1, EventKind::InventoryMoveItem)],
    );
    let alice = s.players[0].clone();
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::InventoryMoveItem);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::InventoryMoveItem(InventoryMoveItemPatch {
            item: Some(ItemSpec::of("minecraft:diamond", 1)),
        })),
    );
    assert_eq!(
        contents(&mut s, &alice, CHEST, 1),
        [Some(("minecraft:diamond", 1))]
    );
    assert_eq!(
        contents(&mut s, &alice, HOPPER, 1),
        [Some(("minecraft:stone", 2))]
    );
}

#[test]
fn a_hopper_inventory_search_reports_the_block_it_looks_at() {
    let mut s = world(
        pushing(&[("minecraft:stone", 3)]),
        vec![observe(1, EventKind::HopperInventorySearch)],
    );
    s.tick(1);
    let heard = settled_for(&s.plugin, EventKind::HopperInventorySearch);
    let first = heard
        .iter()
        .filter_map(|fired| match &fired.payload {
            EventPayload::HopperInventorySearch(event) => Some(event.clone()),
            _ => None,
        })
        .next()
        .expect("a search");
    assert_eq!(first.block, HOPPER);
    assert_eq!(first.search_block, CHEST);
    assert_eq!(
        first.container_type,
        HopperInventorySearchContainerType::Destination
    );
    assert_eq!(
        first.inventory.expect("a chest").owner,
        InventoryOwner::Block(CHEST)
    );
}

#[test]
fn a_hopper_inventory_search_can_redirect_the_hopper() {
    let mut seeds = pushing(&[("minecraft:stone", 3)]);
    seeds.push(Seed::block(SPARE, BlockKind::Chest));
    let mut s = world(seeds, vec![deciding(1, EventKind::HopperInventorySearch)]);
    let alice = s.players[0].clone();
    s.tick(1);
    let asked = last_asked(&s.plugin, EventKind::HopperInventorySearch);
    let EventPayload::HopperInventorySearch(event) = &asked.payload else {
        panic!("a search event");
    };
    let mut redirected = event.inventory.clone().expect("a chest");
    redirected.owner = InventoryOwner::Block(SPARE);
    rule_on(
        &mut s.server,
        1,
        asked.id,
        false,
        Some(EventPatch::HopperInventorySearch(
            HopperInventorySearchPatch {
                inventory: Some(Some(redirected)),
            },
        )),
    );
    assert_eq!(contents(&mut s, &alice, CHEST, 1), [None]);
    assert_eq!(
        contents(&mut s, &alice, SPARE, 1),
        [Some(("minecraft:stone", 1))]
    );
}

const STAND: BlockPos = BlockPos::new(3, 64, 3);

fn stand_with_hopper(
    hopper_at: BlockPos,
    facing: &'static str,
    item: (&'static str, u8),
) -> Vec<Seed> {
    vec![
        Seed::block(STAND, BlockKind::BrewingStand),
        Seed::block(hopper_at, BlockKind::Hopper)
            .with("facing", facing)
            .holding(item.0, item.1),
    ]
}

#[test]
fn a_hopper_above_a_brewing_stand_feeds_the_ingredient_slot() {
    let mut s = world(
        stand_with_hopper(STAND.offset(0, 1, 0), "down", ("minecraft:nether_wart", 2)),
        Vec::new(),
    );
    let alice = s.players[0].clone();
    s.tick(1);
    assert_eq!(
        contents(&mut s, &alice, STAND, 5),
        [None, None, None, Some(("minecraft:nether_wart", 1)), None]
    );
}

#[test]
fn a_hopper_beside_a_brewing_stand_feeds_the_fuel_slot_and_only_with_blaze_powder() {
    let mut s = world(
        stand_with_hopper(STAND.offset(1, 0, 0), "west", ("minecraft:blaze_powder", 2)),
        Vec::new(),
    );
    let alice = s.players[0].clone();
    s.tick(1);
    assert_eq!(
        contents(&mut s, &alice, STAND, 5),
        [None, None, None, None, Some(("minecraft:blaze_powder", 1))]
    );
    let mut s = world(
        stand_with_hopper(STAND.offset(1, 0, 0), "west", ("minecraft:nether_wart", 2)),
        Vec::new(),
    );
    let alice = s.players[0].clone();
    s.tick(1);
    assert_eq!(
        contents(&mut s, &alice, STAND, 5),
        [None, None, None, None, None]
    );
}

#[test]
fn a_hopper_beside_a_brewing_stand_fills_an_empty_bottle_slot_only_with_a_bottle() {
    let mut s = world(
        stand_with_hopper(STAND.offset(1, 0, 0), "west", ("minecraft:glass_bottle", 2)),
        Vec::new(),
    );
    let alice = s.players[0].clone();
    s.tick(1);
    assert_eq!(
        contents(&mut s, &alice, STAND, 5),
        [Some(("minecraft:glass_bottle", 1)), None, None, None, None]
    );
}

#[test]
fn a_hopper_under_a_brewing_stand_takes_the_bottles_and_leaves_the_ingredient() {
    let seeds = vec![
        Seed::block(STAND.offset(0, 1, 0), BlockKind::BrewingStand)
            .holding("minecraft:glass_bottle", 1)
            .gap()
            .gap()
            .holding("minecraft:nether_wart", 3),
        Seed::block(STAND, BlockKind::Hopper).with("facing", "east"),
    ];
    let mut s = world(seeds, Vec::new());
    let alice = s.players[0].clone();
    s.tick(1);
    assert_eq!(
        contents(&mut s, &alice, STAND, 1),
        [Some(("minecraft:glass_bottle", 1))]
    );
    s.tick(8);
    assert_eq!(
        contents(&mut s, &alice, STAND.offset(0, 1, 0), 5),
        [None, None, None, Some(("minecraft:nether_wart", 3)), None]
    );
}

fn console(s: &mut Scene, line: &str) {
    s.server
        .handle(rspigot_core::events::Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn chest_cart(s: &mut Scene, at: BlockPos, items: &str) -> rspigot_domain::EntityId {
    console(
        s,
        &format!(
            "summon chest_minecart {} {} {} {{Items:[{items}]}}",
            f64::from(at.x) + 0.5,
            f64::from(at.y) + 0.0625,
            f64::from(at.z) + 0.5
        ),
    );
    s.server
        .entities_of(rspigot_entities::EntityKind::ChestMinecart)
        .pop()
        .unwrap()
        .id
}

fn counts(items: Vec<rspigot_domain::item::Slot>) -> Vec<u8> {
    items
        .into_iter()
        .flatten()
        .map(|stack| stack.count.get())
        .collect()
}

#[test]
fn a_hopper_pushes_into_a_chest_minecart_in_front_of_it() {
    let mut s = world(
        vec![
            Seed::block(HOPPER, BlockKind::Hopper)
                .with("facing", "down")
                .holding("minecraft:stone", 2),
        ],
        Vec::new(),
    );
    let id = chest_cart(&mut s, HOPPER.offset(0, -1, 0), "");
    s.tick(1);
    assert_eq!(counts(s.server.cargo_of(id)), [1]);
}

#[test]
fn a_hopper_pulls_from_a_chest_minecart_above_it() {
    let mut s = world(
        vec![Seed::block(HOPPER, BlockKind::Hopper).with("facing", "east")],
        Vec::new(),
    );
    let id = chest_cart(
        &mut s,
        HOPPER.offset(0, 1, 0),
        "{Slot:0b,id:\"minecraft:dirt\",count:4}",
    );
    s.tick(1);
    assert_eq!(counts(s.server.cargo_of(id)), [3]);
}

#[test]
fn a_dropper_pushes_into_a_chest_minecart_it_faces() {
    let mut s = world(
        vec![
            Seed::block(HOPPER, BlockKind::Dropper)
                .with("facing", "east")
                .holding("minecraft:stone", 2),
        ],
        Vec::new(),
    );
    let id = chest_cart(&mut s, HOPPER.offset(1, 0, 0), "");
    put(&mut s, HOPPER.offset(0, 1, 0), "minecraft:redstone_block");
    s.tick(6);
    assert_eq!(counts(s.server.cargo_of(id)), [1]);
}

fn hopper_cart(s: &mut Scene, at: BlockPos) -> rspigot_domain::EntityId {
    console(
        s,
        &format!(
            "summon hopper_minecart {} {} {}",
            f64::from(at.x) + 0.5,
            f64::from(at.y) + 0.0625,
            f64::from(at.z) + 0.5
        ),
    );
    s.server
        .entities_of(rspigot_entities::EntityKind::HopperMinecart)
        .pop()
        .unwrap()
        .id
}

#[test]
fn a_hopper_minecart_pulls_an_item_each_tick_from_the_container_above_it() {
    let mut s = world(
        vec![Seed::block(STAND.offset(0, 1, 0), BlockKind::Chest).holding("minecraft:dirt", 5)],
        Vec::new(),
    );
    let id = hopper_cart(&mut s, STAND);
    s.tick(1);
    assert_eq!(counts(s.server.cargo_of(id)), [1]);
    s.tick(2);
    assert_eq!(counts(s.server.cargo_of(id)), [3]);
}

#[test]
fn a_hopper_minecart_sucks_in_an_item_lying_in_its_cart() {
    let mut s = world(Vec::new(), Vec::new());
    let id = hopper_cart(&mut s, STAND);
    console(
        &mut s,
        "summon item 3.5 64.9 3.5 {Item:{id:\"minecraft:diamond\",count:3}}",
    );
    s.tick(2);
    assert_eq!(counts(s.server.cargo_of(id)), [3]);
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn a_hopper_minecart_on_a_powered_activator_rail_stops_pulling() {
    let mut s = world(
        vec![
            Seed::block(STAND.offset(0, 1, 0), BlockKind::Chest).holding("minecraft:dirt", 5),
            Seed::block(STAND, BlockKind::ActivatorRail).with("powered", "true"),
        ],
        Vec::new(),
    );
    let id = hopper_cart(&mut s, STAND);
    s.tick(3);
    assert!(counts(s.server.cargo_of(id)).is_empty());
}

fn crafter_fed(disabled: Option<Vec<i32>>) -> Vec<Seed> {
    let mut crafter = Seed::block(STAND, BlockKind::Crafter);
    if let Some(slots) = disabled {
        crafter = crafter.carrying("disabled_slots", rspigot_nbt::Tag::IntArray(slots));
    }
    vec![
        crafter,
        Seed::block(STAND.offset(0, 1, 0), BlockKind::Hopper)
            .with("facing", "down")
            .holding("minecraft:stone", 3),
    ]
}

#[test]
fn a_hopper_fills_the_slots_of_a_crafter_one_stack_after_the_other() {
    let mut s = world(crafter_fed(None), Vec::new());
    s.tick(1);
    s.tick(8);
    s.tick(8);
    assert_eq!(counts(s.server.block_items(STAND, 9)), [1, 1, 1]);
}

#[test]
fn a_hopper_skips_the_disabled_slots_of_a_crafter() {
    let mut s = world(crafter_fed(Some(vec![0, 1])), Vec::new());
    s.tick(1);
    let items = s.server.block_items(STAND, 9);
    assert!(items[0].is_none() && items[1].is_none());
    assert_eq!(items[2].as_ref().map(|stack| stack.count.get()), Some(1));
}
