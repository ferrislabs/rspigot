mod common;

use common::*;
use rspigot_core::events::{ClientEvent, EquipmentSlot};
use rspigot_core::plugins::{
    Answer, InventoryOrder, InventoryState, ItemSpec, Order, PlayerState, Question,
};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};

fn inv(player: &Player, order: InventoryOrder) -> Order {
    Order::Inventory {
        player: player.id,
        order,
    }
}

fn set(slot: i32, key: &str, count: i32) -> InventoryOrder {
    InventoryOrder::Set {
        slot,
        item: Some(ItemSpec::of(key, count)),
    }
}

fn inventory(s: &mut Scene, player: &Player) -> InventoryState {
    let Answer::Inventory(state) = s.ask(Question::Inventory(player.id)) else {
        panic!("expected the inventory of the player");
    };
    state
}

fn state(s: &mut Scene, player: &Player) -> PlayerState {
    let Answer::Player(state) = s.ask(Question::Player(player.id)) else {
        panic!("expected the state of the player");
    };
    state
}

fn item(state: &InventoryState, slot: usize) -> ItemSpec {
    state.slots[slot].clone().expect("an item in the slot")
}

fn network_nbt(components: Compound) -> Vec<u8> {
    let mut bytes = Vec::new();
    rspigot_nbt::write_network(&mut bytes, &Tag::Compound(components));
    bytes
}

#[test]
fn add_fills_the_inventory_and_drops_the_rest_at_the_feet() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    for slot in (0..=35).chain([40]) {
        s.act(inv(&alice, set(slot, "minecraft:stone", 64)));
    }
    let before = inventory(&mut s, &alice);
    s.act(inv(
        &alice,
        InventoryOrder::Add(ItemSpec::of("minecraft:diamond", 5)),
    ));
    let dropped = s.server.ground_items();
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].1.item, ItemKind::Diamond.id());
    assert_eq!(dropped[0].1.count.get(), 5);
    let feet = state(&mut s, &alice).player.position;
    let entities = s.server.entities_of(Kind::Item);
    assert_eq!(entities.len(), 1);
    assert!(entities[0].position.distance_squared(feet) <= 1.0);
    assert_eq!(inventory(&mut s, &alice), before);
    assert!(s.warnings().is_empty());
}

#[test]
fn resolve_reports_every_slot_in_spigot_numbering() {
    let mut s = scene(&["Alice", "Bob"]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    let bob_sees_alice = (0..40).any(|_| {
        s.tick(1);
        bob.link.take().iter().any(
            |event| matches!(event, ClientEvent::SpawnEntity { uuid, .. } if *uuid == alice.id.0),
        )
    });
    assert!(bob_sees_alice);
    alice.link.take();
    let placed = [
        (0, "minecraft:diamond"),
        (9, "minecraft:stone"),
        (36, "minecraft:iron_boots"),
        (39, "minecraft:iron_helmet"),
        (40, "minecraft:shield"),
    ];
    for (slot, key) in placed {
        s.act(inv(&alice, set(slot, key, 1)));
    }
    let state = inventory(&mut s, &alice);
    assert_eq!(state.slots.len(), 41);
    for (slot, item) in state.slots.iter().enumerate() {
        let expected = placed
            .iter()
            .find(|(placed, _)| usize::try_from(*placed) == Ok(slot))
            .map(|(_, key)| *key);
        assert_eq!(item.as_ref().map(|item| item.key.as_str()), expected);
    }
    assert_eq!(state.held_slot, 0);
    let updated: Vec<i16> = alice
        .link
        .take()
        .into_iter()
        .filter_map(|event| match event {
            ClientEvent::InventorySlot { slot, .. } => Some(slot),
            _ => None,
        })
        .collect();
    assert_eq!(updated, [36, 9, 8, 5, 45]);
    let boots = bob.link.take().into_iter().any(|event| match event {
        ClientEvent::EntityEquipment { slots, .. } => slots.iter().any(|(slot, stack)| {
            *slot == EquipmentSlot::Feet
                && stack.as_ref().map(|stack| stack.item) == Some(ItemKind::IronBoots.id())
        }),
        _ => false,
    });
    assert!(boots, "Bob sees the iron boots on Alice's feet");
}

#[test]
fn named_lore_enchanted_items_reach_the_client() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let sword = ItemSpec {
        name: Some("§6Excalibur".into()),
        lore: vec!["§7Old".into(), "Sharp".into()],
        enchantments: vec![("minecraft:sharpness".into(), 5)],
        ..ItemSpec::of("minecraft:diamond_sword", 1)
    };
    s.act(inv(
        &alice,
        InventoryOrder::Set {
            slot: 0,
            item: Some(sword),
        },
    ));
    let sent = alice
        .link
        .take()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::InventorySlot {
                slot: 36,
                stack: Some(stack),
                ..
            } => Some(stack),
            _ => None,
        })
        .expect("Alice receives the sword");
    for key in [
        "minecraft:custom_name",
        "minecraft:lore",
        "minecraft:enchantments",
    ] {
        assert!(sent.components.contains_key(key), "{key} is missing");
    }
    let read = item(&inventory(&mut s, &alice), 0);
    assert_eq!(read.name.as_deref(), Some("§6Excalibur"));
    assert_eq!(read.lore, ["§7Old", "Sharp"]);
    assert_eq!(read.enchantments, [("minecraft:sharpness".to_owned(), 5)]);
    assert!(read.components.is_empty());
}

#[test]
fn custom_model_data_reaches_the_client_and_reads_back_typed() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let data = rspigot_domain::item::CustomModelData {
        floats: vec![1.5],
        flags: vec![true],
        strings: vec!["skin".into()],
        colors: vec![0xFF0000],
    };
    let stick = ItemSpec {
        custom_model_data: Some(data.clone()),
        ..ItemSpec::of("minecraft:stick", 1)
    };
    s.act(inv(
        &alice,
        InventoryOrder::Set {
            slot: 0,
            item: Some(stick),
        },
    ));
    let sent = alice
        .link
        .take()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::InventorySlot {
                slot: 36,
                stack: Some(stack),
                ..
            } => Some(stack),
            _ => None,
        })
        .expect("Alice receives the stick");
    assert!(sent.components.contains_key("minecraft:custom_model_data"));
    let read = item(&inventory(&mut s, &alice), 0);
    assert_eq!(read.custom_model_data, Some(data));
    assert!(read.components.is_empty());
}

#[test]
fn a_damaged_sword_keeps_its_damage_when_moved() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let damaged = Compound::new().with("minecraft:damage", 12);
    let sword = ItemSpec {
        components: network_nbt(damaged.clone()),
        ..ItemSpec::of("minecraft:diamond_sword", 1)
    };
    s.act(inv(
        &alice,
        InventoryOrder::Set {
            slot: 0,
            item: Some(sword),
        },
    ));
    let read = item(&inventory(&mut s, &alice), 0);
    s.act(inv(
        &alice,
        InventoryOrder::Set {
            slot: 5,
            item: Some(read),
        },
    ));
    let moved = item(&inventory(&mut s, &alice), 5);
    let decoded = rspigot_nbt::read_network_compound(&mut moved.components.as_slice()).unwrap();
    assert_eq!(decoded, Some(damaged));
}

#[test]
fn invalid_items_give_one_warning() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(inv(&alice, set(0, "minecraft:diamond", 3)));
    let before = inventory(&mut s, &alice);
    let sword = |enchantment: &str, level: i32| ItemSpec {
        enchantments: vec![(enchantment.into(), level)],
        ..ItemSpec::of("minecraft:diamond_sword", 1)
    };
    let set_item = |slot: i32, item: ItemSpec| InventoryOrder::Set {
        slot,
        item: Some(item),
    };
    let too_long = ItemSpec {
        lore: vec!["line".into(); 257],
        ..ItemSpec::of("minecraft:stone", 1)
    };
    let refused = [
        (
            "inventory.add",
            InventoryOrder::Add(ItemSpec::of("minecraft:nope", 1)),
        ),
        ("inventory.set", set(1, "minecraft:stone", 0)),
        ("inventory.set", set(1, "minecraft:stone", 100)),
        (
            "inventory.add",
            InventoryOrder::Add(sword("minecraft:nope", 1)),
        ),
        (
            "inventory.set",
            set_item(1, sword("minecraft:sharpness", 0)),
        ),
        (
            "inventory.set",
            set_item(1, sword("minecraft:sharpness", 256)),
        ),
        (
            "inventory.add",
            InventoryOrder::Add(ItemSpec {
                components: vec![0xFF],
                ..ItemSpec::of("minecraft:stone", 1)
            }),
        ),
        ("inventory.set", set_item(1, too_long)),
        ("inventory.set", set(41, "minecraft:stone", 1)),
        ("inventory.set", set(-1, "minecraft:stone", 1)),
        (
            "inventory.remove",
            InventoryOrder::Remove {
                key: "minecraft:diamond".into(),
                count: 0,
            },
        ),
        (
            "inventory.remove",
            InventoryOrder::Remove {
                key: "minecraft:nope".into(),
                count: 1,
            },
        ),
        ("inventory.set_held_slot", InventoryOrder::SetHeldSlot(9)),
        ("inventory.set_held_slot", InventoryOrder::SetHeldSlot(-1)),
    ];
    let total = refused.len();
    for (index, (name, order)) in refused.into_iter().enumerate() {
        s.act(inv(&alice, order));
        let warnings = s.warnings();
        assert_eq!(warnings.len(), index + 1, "{name} gives one warning");
        assert!(
            warnings[index].starts_with(&format!("{name} was not applied: ")),
            "{}",
            warnings[index]
        );
    }
    assert_eq!(s.warnings().len(), total);
    assert_eq!(inventory(&mut s, &alice), before);
    assert!(s.server.ground_items().is_empty());
}

#[test]
fn air_is_an_empty_slot() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(inv(&alice, set(0, "minecraft:diamond", 1)));
    s.act(inv(&alice, set(0, "minecraft:air", 1)));
    assert_eq!(inventory(&mut s, &alice).slots[0], None);
    s.act(inv(
        &alice,
        InventoryOrder::Add(ItemSpec::of("minecraft:air", 1)),
    ));
    assert!(s.server.ground_items().is_empty());
    assert!(inventory(&mut s, &alice).slots.iter().all(Option::is_none));
    assert!(s.warnings().is_empty());
}

#[test]
fn remove_clear_and_held_slot() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(inv(&alice, set(0, "minecraft:stone", 10)));
    s.act(inv(&alice, set(1, "minecraft:stone", 10)));
    s.act(inv(&alice, set(2, "minecraft:diamond", 4)));
    s.act(inv(&alice, set(39, "minecraft:iron_helmet", 1)));
    s.act(inv(
        &alice,
        InventoryOrder::Remove {
            key: "minecraft:stone".into(),
            count: 15,
        },
    ));
    let state = inventory(&mut s, &alice);
    assert_eq!(state.slots[0], None);
    assert_eq!(item(&state, 1).count, 5);
    assert_eq!(item(&state, 2).count, 4);
    s.act(inv(&alice, InventoryOrder::Clear));
    assert!(inventory(&mut s, &alice).slots.iter().all(Option::is_none));
    alice.link.take();
    s.act(inv(&alice, InventoryOrder::SetHeldSlot(3)));
    assert!(alice.link.take().contains(&ClientEvent::HeldSlot(3)));
    assert_eq!(inventory(&mut s, &alice).held_slot, 3);
    assert!(s.warnings().is_empty());
}

#[test]
fn a_reset_name_is_not_italic_and_survives_a_round_trip() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let plain = ItemSpec {
        name: Some("§rPlain".into()),
        ..ItemSpec::of("minecraft:stone", 1)
    };
    s.act(inv(
        &alice,
        InventoryOrder::Set {
            slot: 0,
            item: Some(plain),
        },
    ));
    let sent = alice
        .link
        .take()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::InventorySlot {
                slot: 36,
                stack: Some(stack),
                ..
            } => Some(stack),
            _ => None,
        })
        .expect("Alice receives the stone");
    let Some(Tag::Compound(name)) = sent.components.get("minecraft:custom_name") else {
        panic!("the name is a styled compound");
    };
    assert_eq!(name.get_bool("italic"), Some(false));
    let read = item(&inventory(&mut s, &alice), 0);
    assert_eq!(read.name.as_deref(), Some("§rPlain"));
    s.act(inv(
        &alice,
        InventoryOrder::Set {
            slot: 1,
            item: Some(read),
        },
    ));
    assert_eq!(
        item(&inventory(&mut s, &alice), 1).name.as_deref(),
        Some("§rPlain")
    );
}

#[test]
fn a_name_with_an_ampersand_survives_a_round_trip() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let rock = ItemSpec {
        name: Some("Rock&Roll".into()),
        ..ItemSpec::of("minecraft:stone", 1)
    };
    s.act(inv(
        &alice,
        InventoryOrder::Set {
            slot: 0,
            item: Some(rock),
        },
    ));
    let read = item(&inventory(&mut s, &alice), 0);
    s.act(inv(
        &alice,
        InventoryOrder::Set {
            slot: 1,
            item: Some(read),
        },
    ));
    assert_eq!(
        item(&inventory(&mut s, &alice), 1).name.as_deref(),
        Some("Rock&Roll")
    );
}

#[test]
fn inventory_of_an_offline_player_is_not_found_and_orders_are_dropped() {
    let mut s = scene(&["Alice"]);
    let ghost = Player {
        id: rspigot_domain::player::PlayerId::offline("Ghost"),
        link: Link::default(),
        session: 0,
    };
    s.act(inv(&ghost, set(0, "minecraft:nope", 1)));
    assert_eq!(s.ask(Question::Inventory(ghost.id)), Answer::NotFound);
    assert!(s.warnings().is_empty());
}
