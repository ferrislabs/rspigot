mod common;

use std::time::Duration;

use common::*;
use rspigot_core::events::ClientEvent;
use rspigot_core::player_data::PlayerSave;
use rspigot_core::plugins::{Answer, InventoryOrder, InventoryState, ItemSpec, Order, Question};
use rspigot_domain::Vec3;
use rspigot_domain::meta::MetaValue;
use rspigot_nbt::{Compound, Tag};

fn inv(player: &Player, order: InventoryOrder) -> Order {
    Order::Inventory {
        player: player.id,
        order,
    }
}

fn network_nbt(components: Compound) -> Vec<u8> {
    let mut bytes = Vec::new();
    rspigot_nbt::write_network(&mut bytes, &Tag::Compound(components));
    bytes
}

fn decoded(bytes: &[u8]) -> Compound {
    rspigot_nbt::read_network_compound(&mut &*bytes)
        .expect("network nbt")
        .expect("a compound")
}

fn with(key: &str, components: Compound) -> ItemSpec {
    ItemSpec {
        components: network_nbt(components),
        ..ItemSpec::of(key, 1)
    }
}

fn public(values: Compound) -> Compound {
    Compound::new().with(
        "minecraft:custom_data",
        Compound::new().with("PublicBukkitValues", values),
    )
}

fn tagged(kind: &str) -> Compound {
    public(Compound::new().with("relic:kind", MetaValue::Text(kind.to_owned()).to_nbt()))
}

fn modifier() -> Tag {
    Tag::Compound(
        Compound::new()
            .with("type", "minecraft:attack_damage")
            .with("id", "relic:bonus")
            .with("amount", 4.0)
            .with("operation", "add_value")
            .with("slot", "mainhand"),
    )
}

fn relic_components() -> Compound {
    let mut components = tagged("fire");
    components.insert("minecraft:unbreakable", Compound::new());
    components.insert("minecraft:damage", 3);
    components.insert("minecraft:rarity", "epic");
    components.insert(
        "minecraft:tooltip_display",
        Compound::new().with(
            "hidden_components",
            Tag::List(vec![
                Tag::from("minecraft:unbreakable"),
                Tag::from("minecraft:attribute_modifiers"),
            ]),
        ),
    );
    components.insert("minecraft:attribute_modifiers", Tag::List(vec![modifier()]));
    components.insert("minecraft:item_name", "§6Relic");
    components
}

fn inventory(s: &mut Scene, player: &Player) -> InventoryState {
    let Answer::Inventory(state) = s.ask(Question::Inventory(player.id)) else {
        panic!("expected the inventory of the player");
    };
    state
}

fn sent_in(player: &Player, slot: i16) -> rspigot_domain::item::ItemStack {
    player
        .link
        .take()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::InventorySlot {
                slot: found,
                stack: Some(stack),
                ..
            } if found == slot => Some(stack),
            _ => None,
        })
        .expect("the client receives the item")
}

#[test]
fn a_built_item_reaches_the_client_the_plugin_and_the_save() {
    let (mut s, saves) = saving_scene(&["Alice"], Vec::new());
    let alice = s.players[0].clone();
    s.act(inv(
        &alice,
        InventoryOrder::Set {
            slot: 0,
            item: Some(with("minecraft:diamond_sword", relic_components())),
        },
    ));
    let sent = sent_in(&alice, 36);
    for key in [
        "minecraft:unbreakable",
        "minecraft:damage",
        "minecraft:rarity",
        "minecraft:tooltip_display",
        "minecraft:attribute_modifiers",
        "minecraft:item_name",
        "minecraft:custom_data",
    ] {
        assert!(sent.components.contains_key(key), "{key} is missing");
    }
    assert_ne!(
        sent.components.get("minecraft:item_name"),
        Some(&Tag::from("§6Relic"))
    );
    assert_eq!(
        sent.components.get("minecraft:attribute_modifiers"),
        Some(&Tag::List(vec![modifier()]))
    );

    let read = inventory(&mut s, &alice).slots[0].clone().expect("an item");
    let back = decoded(&read.components);
    assert_eq!(back.get("minecraft:item_name"), Some(&Tag::from("§6Relic")));
    assert_eq!(back.get("minecraft:damage"), Some(&Tag::Int(3)));
    assert_eq!(
        back.get("minecraft:unbreakable"),
        Some(&Tag::Compound(Compound::new()))
    );
    assert_eq!(
        back.get("minecraft:custom_data"),
        relic_components().get("minecraft:custom_data")
    );

    s.server.handle(leave_of(&alice));
    s.server.deliver();
    s.tick(2);
    let mut saved = None;
    for _ in 0..100 {
        saved = rspigot_core::ports::PlayerStore::load_player(&*saves, alice.id).unwrap();
        if saved.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let save = PlayerSave::from_nbt(&saved.expect("saved")).expect("decodes");
    let stored = save.inventory.slots()[36]
        .clone()
        .expect("the sword is saved");
    assert_eq!(
        stored.components.get("minecraft:custom_data"),
        relic_components().get("minecraft:custom_data")
    );
    assert_eq!(
        stored.components.get("minecraft:damage"),
        Some(&Tag::Int(3))
    );
    assert!(
        stored
            .components
            .contains_key("minecraft:attribute_modifiers")
    );
}

#[test]
fn foreign_spigot_values_survive_a_give_and_a_read_untouched() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let values = Compound::new()
        .with("spigot:level", 5)
        .with("spigot:odd", Tag::List(vec![Tag::Int(1)]))
        .with("relic:kind", MetaValue::Text("ice".to_owned()).to_nbt());
    let custom = Compound::new().with(
        "minecraft:custom_data",
        Compound::new()
            .with("PublicBukkitValues", values.clone())
            .with("other", 1),
    );
    s.act(inv(
        &alice,
        InventoryOrder::Set {
            slot: 0,
            item: Some(with("minecraft:stick", custom.clone())),
        },
    ));
    assert!(s.warnings().is_empty());
    let read = inventory(&mut s, &alice).slots[0].clone().expect("an item");
    assert_eq!(decoded(&read.components), custom);
}

#[test]
fn stacks_with_different_custom_data_do_not_merge_in_an_inventory() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    for kind in ["fire", "ice", "fire"] {
        s.act(inv(
            &alice,
            InventoryOrder::Add(with("minecraft:stick", tagged(kind))),
        ));
    }
    s.act(inv(
        &alice,
        InventoryOrder::Add(ItemSpec::of("minecraft:stick", 1)),
    ));
    assert!(s.warnings().is_empty());
    let state = inventory(&mut s, &alice);
    let mut stacks: Vec<(Compound, i32)> = state
        .slots
        .iter()
        .flatten()
        .map(|item| (decoded_or_empty(&item.components), item.count))
        .collect();
    stacks.sort_by_key(|(components, _)| format!("{components:?}"));
    assert_eq!(stacks.len(), 3, "{stacks:?}");
    let fire = stacks
        .iter()
        .find(|(components, _)| *components == tagged("fire"))
        .expect("the fire stack");
    assert_eq!(fire.1, 2);
    assert!(
        stacks
            .iter()
            .any(|(components, count)| components.is_empty() && *count == 1)
    );
    assert!(
        stacks
            .iter()
            .any(|(components, count)| *components == tagged("ice") && *count == 1)
    );
}

fn decoded_or_empty(bytes: &[u8]) -> Compound {
    if bytes.is_empty() {
        Compound::new()
    } else {
        decoded(bytes)
    }
}

#[test]
fn dropped_items_with_different_custom_data_do_not_merge() {
    let mut s = scene(&["Alice"]);
    for (x, kind) in [(9.5, "fire"), (9.6, "ice"), (9.7, "fire")] {
        let Answer::Spawned(_) = s.ask(Question::DropItem {
            position: Vec3::new(x, 64.0, 9.5),
            item: with("minecraft:stick", tagged(kind)),
        }) else {
            panic!("expected the dropped stick");
        };
    }
    let Answer::Spawned(_) = s.ask(Question::DropItem {
        position: Vec3::new(9.5, 64.0, 9.6),
        item: ItemSpec::of("minecraft:stick", 1),
    }) else {
        panic!("expected the plain stick");
    };
    s.tick(200);
    let left: Vec<(Compound, u8)> = s
        .server
        .ground_items()
        .into_iter()
        .map(|(_, stack)| (stack.components.clone(), stack.count.get()))
        .collect();
    let of = |wanted: &Compound| -> Vec<u8> {
        left.iter()
            .filter(|(components, _)| components == wanted)
            .map(|(_, count)| *count)
            .collect()
    };
    assert_eq!(of(&tagged("ice")), [1], "{left:?}");
    assert_eq!(of(&Compound::new()), [1], "{left:?}");
    let fire = of(&tagged("fire"));
    assert_eq!(fire.iter().sum::<u8>(), 2, "{left:?}");
    assert!(left.iter().all(|(_, count)| *count <= 2), "{left:?}");
}

#[test]
fn a_stack_moved_through_the_inventory_keeps_its_custom_data() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(inv(
        &alice,
        InventoryOrder::Set {
            slot: 4,
            item: Some(with("minecraft:stick", tagged("fire"))),
        },
    ));
    let moved = inventory(&mut s, &alice).slots[4].clone().expect("an item");
    s.act(inv(
        &alice,
        InventoryOrder::Set {
            slot: 5,
            item: Some(moved),
        },
    ));
    let again = inventory(&mut s, &alice).slots[5].clone().expect("an item");
    assert_eq!(decoded(&again.components), tagged("fire"));
}

#[test]
fn invalid_component_values_are_refused_with_one_warning_each() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(inv(
        &alice,
        InventoryOrder::Set {
            slot: 0,
            item: Some(ItemSpec::of("minecraft:diamond", 3)),
        },
    ));
    let before = inventory(&mut s, &alice);
    let many: Compound = (0..65)
        .map(|index| {
            (
                format!("relic:k{index}"),
                MetaValue::Int(i64::from(index)).to_nbt(),
            )
        })
        .collect();
    let refused = [
        Compound::new().with("minecraft:damage", -1),
        Compound::new().with("minecraft:max_damage", 0),
        Compound::new().with("minecraft:repair_cost", -5),
        Compound::new().with("minecraft:max_stack_size", 0),
        Compound::new().with("minecraft:max_stack_size", 100),
        Compound::new().with("minecraft:item_model", "Bad Id"),
        Compound::new().with("minecraft:mod_unknown", 1),
        Compound::new().with(
            "minecraft:attribute_modifiers",
            Tag::List(vec![Tag::Compound(
                Compound::new()
                    .with("type", "minecraft:nope")
                    .with("id", "relic:bonus")
                    .with("amount", 1.0)
                    .with("operation", "add_value"),
            )]),
        ),
        Compound::new().with(
            "minecraft:attribute_modifiers",
            Tag::List(vec![Tag::Compound(
                Compound::new()
                    .with("type", "minecraft:attack_damage")
                    .with("id", "relic:bonus")
                    .with("amount", f64::NAN)
                    .with("operation", "add_value"),
            )]),
        ),
        Compound::new().with(
            "minecraft:tooltip_display",
            Compound::new().with(
                "hidden_components",
                Tag::List(vec![Tag::from("minecraft:nope")]),
            ),
        ),
        public(Compound::new().with("nokey", MetaValue::Int(1).to_nbt())),
        public(many),
    ];
    let total = refused.len();
    for (index, components) in refused.into_iter().enumerate() {
        s.act(inv(
            &alice,
            InventoryOrder::Add(with("minecraft:stone", components)),
        ));
        let warnings = s.warnings();
        assert_eq!(warnings.len(), index + 1, "{warnings:?}");
        assert!(
            warnings[index].starts_with("inventory.add was not applied: "),
            "{}",
            warnings[index]
        );
    }
    assert_eq!(s.warnings().len(), total);
    assert_eq!(inventory(&mut s, &alice), before);
    assert!(s.server.ground_items().is_empty());
}

mod attributes {
    use super::*;
    use rspigot_core::events::PlayerInput;
    use rspigot_core::plugins::{EntityView, PlayerOrder};
    use rspigot_domain::EntityId;

    const ARMOR_SLOTS: [(i32, &str); 4] = [
        (39, "minecraft:diamond_helmet"),
        (38, "minecraft:diamond_chestplate"),
        (37, "minecraft:diamond_leggings"),
        (36, "minecraft:diamond_boots"),
    ];

    fn modifier(attribute: &str, amount: f64, operation: &str, slot: &str) -> Tag {
        Tag::Compound(
            Compound::new()
                .with("type", attribute)
                .with("id", "relic:mod")
                .with("amount", amount)
                .with("operation", operation)
                .with("slot", slot),
        )
    }

    fn modified(key: &str, entries: Vec<Tag>) -> ItemSpec {
        with(
            key,
            Compound::new().with("minecraft:attribute_modifiers", Tag::List(entries)),
        )
    }

    fn put(s: &mut Scene, slot: i32, item: ItemSpec) {
        let player = s.players[0].id;
        s.act(Order::Inventory {
            player,
            order: InventoryOrder::Set {
                slot,
                item: Some(item),
            },
        });
    }

    fn cow(s: &mut Scene) -> EntityView {
        let Answer::Spawned(view) = s.ask(Question::Spawn {
            kind: "minecraft:cow".into(),
            position: Vec3::new(1.5, 64.0, 0.5),
        }) else {
            panic!("expected the spawned cow");
        };
        view
    }

    fn health_of(s: &mut Scene, id: EntityId) -> f32 {
        match s.ask(Question::Entity(id)) {
            Answer::Entity(details) => details.health.unwrap_or(0.0),
            _ => panic!("expected the entity"),
        }
    }

    fn hit_with(held: ItemSpec) -> f32 {
        let mut s = scene(&["Alice"]);
        put(&mut s, 0, held);
        let target = cow(&mut s);
        s.tick(25);
        let alice = s.players[0].clone();
        send_input(&mut s.server, &alice, PlayerInput::Attack(target.id));
        health_of(&mut s, target.id)
    }

    fn armor_lost(pieces: Vec<(i32, ItemSpec)>) -> f32 {
        let mut s = scene(&["Alice", "Bob"]);
        for (slot, item) in pieces {
            put(&mut s, slot, item);
        }
        let bob = s.players[1].clone();
        s.act(Order::Inventory {
            player: bob.id,
            order: InventoryOrder::Set {
                slot: 0,
                item: Some(ItemSpec::of("minecraft:diamond_sword", 1)),
            },
        });
        let alice = s.players[0].id;
        s.act(Order::Player {
            player: alice,
            order: PlayerOrder::SetHealth(20.0),
        });
        let Answer::Player(state) = s.ask(Question::Player(alice)) else {
            panic!("expected Alice");
        };
        s.tick(25);
        send_input(
            &mut s.server,
            &bob,
            PlayerInput::Attack(EntityId(state.player.entity_id)),
        );
        match s.ask(Question::Player(alice)) {
            Answer::Player(state) => 20.0 - state.health,
            _ => panic!("expected Alice"),
        }
    }

    const SWORD: &str = "minecraft:diamond_sword";
    const DAMAGE: &str = "minecraft:attack_damage";
    const ARMOR: &str = "minecraft:armor";

    #[test]
    fn a_main_hand_modifier_replaces_the_default_attack_damage_of_the_item() {
        assert_eq!(hit_with(ItemSpec::of(SWORD, 1)), 3.0);
        let relic = modified(SWORD, vec![modifier(DAMAGE, 5.0, "add_value", "mainhand")]);
        assert_eq!(hit_with(relic), 4.0);
        let stick = modified(
            "minecraft:stick",
            vec![modifier(DAMAGE, 5.0, "add_value", "mainhand")],
        );
        assert_eq!(hit_with(stick), 4.0);
    }

    #[test]
    fn modifiers_apply_their_operations_in_vanilla_order() {
        let stick = modified(
            "minecraft:stick",
            vec![
                modifier(DAMAGE, 1.0, "add_value", "hand"),
                modifier(DAMAGE, 0.5, "add_multiplied_base", "any"),
                modifier(DAMAGE, 1.0, "add_multiplied_total", "mainhand"),
            ],
        );
        assert_eq!(hit_with(stick), 4.0);
    }

    #[test]
    fn a_modifier_for_another_slot_group_does_nothing_and_still_replaces_the_defaults() {
        let sword = modified(SWORD, vec![modifier(DAMAGE, 5.0, "add_value", "offhand")]);
        assert_eq!(hit_with(sword), 9.0);
        let empty = modified(SWORD, Vec::new());
        assert_eq!(hit_with(empty), 9.0);
    }

    #[test]
    fn the_modifier_ends_when_the_item_is_swapped_out() {
        let mut s = scene(&["Alice"]);
        let alice = s.players[0].clone();
        put(
            &mut s,
            0,
            modified(
                "minecraft:stick",
                vec![modifier(DAMAGE, 5.0, "add_value", "mainhand")],
            ),
        );
        let first = cow(&mut s);
        s.tick(25);
        send_input(&mut s.server, &alice, PlayerInput::Attack(first.id));
        assert_eq!(health_of(&mut s, first.id), 4.0);
        put(&mut s, 0, ItemSpec::of("minecraft:stick", 1));
        let second = cow(&mut s);
        s.tick(25);
        send_input(&mut s.server, &alice, PlayerInput::Attack(second.id));
        assert_eq!(health_of(&mut s, second.id), 9.0);
    }

    fn all_diamond() -> Vec<(i32, ItemSpec)> {
        ARMOR_SLOTS
            .iter()
            .map(|(slot, key)| (*slot, ItemSpec::of(*key, 1)))
            .collect()
    }

    #[test]
    fn an_armor_modifier_replaces_the_default_armor_and_an_empty_list_removes_it() {
        let chest_only = |piece: ItemSpec| vec![(38, piece)];
        let plain = armor_lost(chest_only(ItemSpec::of("minecraft:diamond_chestplate", 1)));
        assert!((plain - 5.544).abs() < 0.01, "{plain}");
        let weak = modified(
            "minecraft:diamond_chestplate",
            vec![modifier(ARMOR, 2.0, "add_value", "chest")],
        );
        let weak = armor_lost(chest_only(weak));
        assert!((weak - 6.888).abs() < 0.01, "{weak}");
        let bare = armor_lost(chest_only(modified(
            "minecraft:diamond_chestplate",
            Vec::new(),
        )));
        assert!((bare - 7.0).abs() < 0.01, "{bare}");
        let wrong_slot = modified(
            "minecraft:diamond_chestplate",
            vec![modifier(ARMOR, 7.0, "add_value", "feet")],
        );
        let wrong = armor_lost(chest_only(wrong_slot));
        assert!((wrong - 7.0).abs() < 0.01, "{wrong}");
        let full = armor_lost(all_diamond());
        assert!(full < plain, "{full}");
    }

    #[test]
    fn equipping_a_modified_piece_sends_the_new_armor_to_the_client() {
        let mut s = scene(&["Alice"]);
        let alice = s.players[0].clone();
        put(
            &mut s,
            38,
            modified(
                "minecraft:diamond_chestplate",
                vec![modifier(ARMOR, 2.0, "add_value", "chest")],
            ),
        );
        s.tick(2);
        let sent: Vec<f64> = alice
            .link
            .take()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::Attributes { attributes, .. } => attributes
                    .into_iter()
                    .find(|state| state.attribute == ARMOR)
                    .map(|state| rspigot_domain::attribute::value(&state)),
                _ => None,
            })
            .collect();
        assert_eq!(sent.last().copied(), Some(2.0), "{sent:?}");
    }
}
