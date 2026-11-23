mod common;

use common::*;
use rspigot_core::config::PluginSettings;
use rspigot_core::events::{
    ClickInput, ClientEvent, ContainerClick, Hand, HashedSlot, PlayerInput,
};
use rspigot_core::plugins::{Answer, InventoryOrder, ItemSpec, Order, Question};
use rspigot_domain::Rotation;
use rspigot_nbt::{Compound, Tag};

const BUNDLE: i32 = 0;
const STONES: i32 = 1;
const DIRT: i32 = 2;

fn scene_of() -> Scene {
    scene_on(
        server(PluginSettings::default()).with_rng_seed(3),
        &["Alice"],
        Vec::new(),
    )
}

fn put(s: &mut Scene, slot: i32, item: ItemSpec) {
    let alice = s.players[0].clone();
    s.act(Order::Inventory {
        player: alice.id,
        order: InventoryOrder::Set {
            slot,
            item: Some(item),
        },
    });
}

fn slot(s: &mut Scene, slot: usize) -> Option<ItemSpec> {
    let Answer::Inventory(inventory) = s.ask(Question::Inventory(s.players[0].id)) else {
        panic!("expected Alice's inventory");
    };
    inventory.slots[slot].clone()
}

fn menu_slot(hotbar: i32) -> i16 {
    36 + hotbar as i16
}

fn click(s: &mut Scene, hotbar: i32, button: i8) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::ContainerClick(ContainerClick {
            container: 0,
            state_id: 0,
            slot: menu_slot(hotbar),
            button,
            input: ClickInput::Pickup,
            changed: Vec::new(),
            carried: HashedSlot::default(),
        }),
    );
}

fn cursor(s: &Scene) -> Option<rspigot_domain::item::ItemStack> {
    s.players[0]
        .link
        .take_raw()
        .into_iter()
        .rev()
        .find_map(|event| match event {
            ClientEvent::CursorItem(stack) => Some(stack),
            ClientEvent::InventoryContent { carried, .. } => Some(carried),
            _ => None,
        })
        .flatten()
}

fn contents(spec: &ItemSpec) -> Vec<(String, i64)> {
    let mut bytes = spec.components.as_slice();
    let Ok(Some(Tag::Compound(components))) = rspigot_nbt::read_network(&mut bytes) else {
        return Vec::new();
    };
    components
        .get_list("minecraft:bundle_contents")
        .unwrap_or_default()
        .iter()
        .filter_map(Tag::as_compound)
        .map(|entry| {
            (
                entry.get_str("id").unwrap_or_default().to_owned(),
                entry.get_i64("count").unwrap_or(0),
            )
        })
        .collect()
}

fn with_bundle_contents(mut spec: ItemSpec, items: &[(&str, i32)]) -> ItemSpec {
    let list = items
        .iter()
        .map(|(id, count)| Tag::Compound(Compound::new().with("id", *id).with("count", *count)))
        .collect();
    let components = Compound::new().with("minecraft:bundle_contents", Tag::List(list));
    rspigot_nbt::write_network(&mut spec.components, &Tag::Compound(components));
    spec
}

#[test]
fn carrying_stone_and_left_clicking_a_bundle_inserts_it() {
    let mut s = scene_of();
    put(&mut s, BUNDLE, ItemSpec::of("minecraft:bundle", 1));
    put(&mut s, STONES, ItemSpec::of("minecraft:stone", 10));
    click(&mut s, STONES, 0);
    click(&mut s, BUNDLE, 0);
    s.tick(1);
    let bundle = slot(&mut s, 0).unwrap();
    assert_eq!(contents(&bundle), vec![("minecraft:stone".to_owned(), 10)]);
    assert!(cursor(&s).is_none());
}

#[test]
fn carrying_a_bundle_and_left_clicking_a_stack_takes_it_in() {
    let mut s = scene_of();
    put(&mut s, BUNDLE, ItemSpec::of("minecraft:bundle", 1));
    put(&mut s, DIRT, ItemSpec::of("minecraft:dirt", 20));
    click(&mut s, BUNDLE, 0);
    click(&mut s, DIRT, 0);
    s.tick(1);
    assert!(slot(&mut s, 2).is_none());
    click(&mut s, BUNDLE, 0);
    s.tick(1);
    let bundle = slot(&mut s, 0).unwrap();
    assert_eq!(contents(&bundle), vec![("minecraft:dirt".to_owned(), 20)]);
}

#[test]
fn right_clicking_a_bundle_with_an_empty_cursor_takes_a_stack_out() {
    let mut s = scene_of();
    put(
        &mut s,
        BUNDLE,
        with_bundle_contents(
            ItemSpec::of("minecraft:bundle", 1),
            &[("minecraft:stone", 5), ("minecraft:dirt", 3)],
        ),
    );
    click(&mut s, BUNDLE, 1);
    s.tick(1);
    let carried = cursor(&s).unwrap();
    assert_eq!(carried.item, rspigot_items::ItemKind::Stone.id());
    assert_eq!(carried.count.get(), 5);
    let bundle = slot(&mut s, 0).unwrap();
    assert_eq!(contents(&bundle), vec![("minecraft:dirt".to_owned(), 3)]);
}

#[test]
fn a_bundle_refuses_more_than_its_capacity() {
    let mut s = scene_of();
    put(
        &mut s,
        BUNDLE,
        with_bundle_contents(
            ItemSpec::of("minecraft:bundle", 1),
            &[("minecraft:dirt", 60)],
        ),
    );
    put(&mut s, STONES, ItemSpec::of("minecraft:stone", 10));
    click(&mut s, STONES, 0);
    click(&mut s, BUNDLE, 0);
    s.tick(1);
    let bundle = slot(&mut s, 0).unwrap();
    let held = contents(&bundle);
    assert_eq!(held[0], ("minecraft:stone".to_owned(), 4));
    let carried = cursor(&s).unwrap();
    assert_eq!(carried.count.get(), 6);
}

#[test]
fn using_a_bundle_drops_its_contents_and_empties_it() {
    let mut s = scene_of();
    put(
        &mut s,
        BUNDLE,
        with_bundle_contents(
            ItemSpec::of("minecraft:bundle", 1),
            &[("minecraft:stone", 5), ("minecraft:dirt", 3)],
        ),
    );
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
    s.tick(2);
    let bundle = slot(&mut s, 0).unwrap();
    assert!(contents(&bundle).is_empty());
    let Answer::Entities(entities) = s.ask(Question::Entities {
        near: None,
        kind: Some("minecraft:item".to_owned()),
    }) else {
        panic!("expected entities");
    };
    assert_eq!(entities.len(), 2);
}

#[test]
fn a_selected_item_is_the_one_taken_out_by_right_click() {
    let mut s = scene_of();
    put(
        &mut s,
        BUNDLE,
        with_bundle_contents(
            ItemSpec::of("minecraft:bundle", 1),
            &[("minecraft:stone", 5), ("minecraft:dirt", 3)],
        ),
    );
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::BundleItemSelected {
            slot: i32::from(menu_slot(BUNDLE)),
            index: 1,
        },
    );
    click(&mut s, BUNDLE, 1);
    s.tick(1);
    let carried = cursor(&s).unwrap();
    assert_eq!(carried.item, rspigot_items::ItemKind::Dirt.id());
}

mod chest {
    use super::*;
    use rspigot_core::events::{Direction, Inbound};
    use rspigot_core::plugins::{BlockOrder, TileEdit};
    use rspigot_domain::{BlockPos, Vec3};

    const CHEST: BlockPos = BlockPos::new(2, 64, 2);

    fn opened_scene() -> (Scene, i32) {
        let mut s = scene_of();
        stream_chunk_of(&mut s, CHEST);
        s.server.handle(Inbound::Console(
            "setblock 2 64 2 minecraft:chest".to_owned(),
        ));
        s.server.deliver();
        s.act(Order::Block {
            pos: CHEST,
            order: BlockOrder::Tile(TileEdit::InventorySet {
                slot: 0,
                item: Some(with_bundle_contents(
                    ItemSpec::of("minecraft:bundle", 1),
                    &[("minecraft:stone", 5), ("minecraft:dirt", 3)],
                )),
            }),
        });
        put(&mut s, STONES, ItemSpec::of("minecraft:cobblestone", 7));
        let alice = s.players[0].clone();
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::UseItemOn {
                hand: Hand::Main,
                pos: CHEST,
                face: Direction::Up,
                cursor: Vec3::new(0.5, 1.0, 0.5),
                inside_block: false,
                sequence: 3,
            },
        );
        s.tick(2);
        let window = alice
            .link
            .take_raw()
            .into_iter()
            .rev()
            .find_map(|event| match event {
                ClientEvent::OpenScreen { window, .. } => Some(window),
                _ => None,
            })
            .expect("the chest opens");
        (s, window)
    }

    fn click_slot(s: &mut Scene, window: i32, slot: i16, button: i8) {
        let alice = s.players[0].clone();
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::ContainerClick(ContainerClick {
                container: window,
                state_id: 0,
                slot,
                button,
                input: ClickInput::Pickup,
                changed: Vec::new(),
                carried: HashedSlot::default(),
            }),
        );
        s.tick(1);
    }

    fn chest_bundle(s: &Scene) -> Vec<(String, i64)> {
        let data = s.server.block_entity_nbt(CHEST).unwrap_or_default();
        data.get_list("Items")
            .and_then(|items| items.first())
            .and_then(Tag::as_compound)
            .and_then(|item| item.get_compound("components"))
            .and_then(|components| components.get_list("minecraft:bundle_contents"))
            .unwrap_or_default()
            .iter()
            .filter_map(Tag::as_compound)
            .map(|entry| {
                (
                    entry.get_str("id").unwrap_or_default().to_owned(),
                    entry.get_i64("count").unwrap_or(0),
                )
            })
            .collect()
    }

    #[test]
    fn a_carried_stack_left_clicked_on_a_bundle_in_a_chest_goes_inside() {
        let (mut s, window) = opened_scene();
        click_slot(&mut s, window, 54 + STONES as i16, 0);
        click_slot(&mut s, window, 0, 0);
        let held = chest_bundle(&s);
        assert_eq!(held[0], ("minecraft:cobblestone".to_owned(), 7));
    }

    #[test]
    fn a_selected_stack_in_a_chest_bundle_is_the_one_taken_out() {
        let (mut s, window) = opened_scene();
        let alice = s.players[0].clone();
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::BundleItemSelected { slot: 0, index: 1 },
        );
        click_slot(&mut s, window, 0, 1);
        let carried = alice
            .link
            .take_raw()
            .into_iter()
            .rev()
            .find_map(|event| match event {
                ClientEvent::ContainerContent { carried, .. } => Some(carried),
                ClientEvent::CursorItem(carried) => Some(carried),
                _ => None,
            })
            .flatten()
            .expect("a carried stack");
        assert_eq!(carried.item, rspigot_items::ItemKind::Dirt.id());
        assert_eq!(chest_bundle(&s), vec![("minecraft:stone".to_owned(), 5)]);
    }
}
