mod common;

use common::*;
use rspigot_core::plugins::{
    Answer, BlockInfo, BlockOrder, FurnaceEdit, ItemSpec, Order, Question, SpawnerEdit, TileData,
    TileEdit, TileView,
};
use rspigot_domain::BlockPos;

const POS: BlockPos = BlockPos::new(4, 64, 4);

fn arena() -> Scene {
    let mut s = scene(&["Alice"]);
    stream_chunk_of(&mut s, POS);
    s
}

fn place(s: &mut Scene, key: &str) {
    place_at(s, POS, key);
}

fn place_at(s: &mut Scene, pos: BlockPos, key: &str) {
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set(key.to_owned()),
    });
}

fn tile(s: &mut Scene, edit: TileEdit) {
    s.act(Order::Block {
        pos: POS,
        order: BlockOrder::Tile(edit),
    });
}

fn data(s: &mut Scene, properties: &[(&str, &str)]) {
    s.act(Order::Block {
        pos: POS,
        order: BlockOrder::SetData(
            properties
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect(),
        ),
    });
}

fn read(s: &mut Scene) -> TileView {
    match s.ask(Question::Tile(POS)) {
        Answer::Tile(view) => view,
        other => panic!("expected a tile, got {other:?}"),
    }
}

fn properties(s: &mut Scene) -> Vec<(String, String)> {
    match s.ask(Question::Block(POS)) {
        Answer::Block(state) => state.properties,
        other => panic!("expected a block, got {other:?}"),
    }
}

fn warned(s: &Scene, order: &str) -> bool {
    s.warnings().iter().any(|warning| warning.contains(order))
}

fn item(key: &str, count: i32) -> ItemSpec {
    ItemSpec::of(key, count)
}

fn keys(slots: &[(usize, ItemSpec)]) -> Vec<(usize, String, i32)> {
    slots
        .iter()
        .map(|(slot, spec)| (*slot, spec.key.clone(), spec.count))
        .collect()
}

fn contents(view: TileView) -> (usize, Vec<(usize, String, i32)>) {
    match view.data {
        TileData::Inventory(inventory) => (inventory.size, keys(&inventory.slots)),
        other => panic!("expected an inventory, got {other:?}"),
    }
}

fn has(properties: &[(String, String)], name: &str, value: &str) -> bool {
    properties
        .iter()
        .any(|(known, held)| known == name && held == value)
}

#[test]
fn block_data_is_updated_partially_and_an_invalid_property_is_refused() {
    let mut s = arena();
    place(&mut s, "minecraft:oak_stairs");

    data(&mut s, &[("facing", "west"), ("half", "top")]);
    let state = properties(&mut s);
    assert!(has(&state, "facing", "west") && has(&state, "half", "top"));
    assert!(has(&state, "shape", "straight"));

    data(&mut s, &[("facing", "up")]);
    assert!(warned(&s, "block.set_data"));
    data(&mut s, &[("half", "bottom"), ("age", "3")]);
    let state = properties(&mut s);
    assert!(has(&state, "half", "top") && has(&state, "facing", "west"));
}

#[test]
fn a_sign_takes_lines_style_and_wax() {
    let mut s = arena();
    place(&mut s, "minecraft:oak_sign");

    tile(
        &mut s,
        TileEdit::SignLines {
            front: true,
            lines: vec!["Hello".into(), "§cWorld".into()],
        },
    );
    tile(
        &mut s,
        TileEdit::SignLines {
            front: false,
            lines: vec!["Back".into()],
        },
    );
    tile(
        &mut s,
        TileEdit::SignStyle {
            front: true,
            color: Some("red".into()),
            glowing: Some(true),
        },
    );
    tile(&mut s, TileEdit::SignWaxed(true));

    let TileData::Sign(sign) = read(&mut s).data else {
        panic!("expected a sign");
    };
    assert_eq!(sign.front, vec!["Hello", "§cWorld", "", ""]);
    assert_eq!(sign.back, vec!["Back", "", "", ""]);
    assert_eq!(sign.front_color, "red");
    assert!(sign.front_glowing && !sign.back_glowing && sign.waxed);
    assert_eq!(sign.back_color, "black");

    tile(
        &mut s,
        TileEdit::SignLines {
            front: true,
            lines: vec![String::new(); 5],
        },
    );
    tile(
        &mut s,
        TileEdit::SignStyle {
            front: true,
            color: Some("ultraviolet".into()),
            glowing: None,
        },
    );
    assert!(warned(&s, "block.sign.set_lines"));
    assert!(warned(&s, "block.sign.set_style"));
}

#[test]
fn a_hanging_sign_and_a_wrong_block_are_told_apart() {
    let mut s = arena();
    place(&mut s, "minecraft:oak_hanging_sign");
    tile(
        &mut s,
        TileEdit::SignLines {
            front: true,
            lines: vec!["Hung".into()],
        },
    );
    assert_eq!(read(&mut s).kind, "minecraft:hanging_sign");

    place(&mut s, "minecraft:stone");
    tile(&mut s, TileEdit::SignWaxed(true));
    assert!(warned(&s, "block.sign.set_waxed"));
    assert!(matches!(s.ask(Question::Tile(POS)), Answer::Refused(_)));
}

#[test]
fn a_chest_inventory_is_set_added_read_and_cleared() {
    let mut s = arena();
    place(&mut s, "minecraft:chest");

    tile(
        &mut s,
        TileEdit::InventorySet {
            slot: 0,
            item: Some(item("minecraft:stone", 3)),
        },
    );
    tile(&mut s, TileEdit::InventoryAdd(item("minecraft:stone", 5)));
    tile(&mut s, TileEdit::InventoryAdd(item("minecraft:diamond", 2)));

    let (size, slots) = contents(read(&mut s));
    assert_eq!(size, 27);
    assert_eq!(
        slots,
        vec![
            (0, "minecraft:stone".to_owned(), 8),
            (1, "minecraft:diamond".to_owned(), 2)
        ]
    );

    tile(
        &mut s,
        TileEdit::InventorySet {
            slot: 0,
            item: None,
        },
    );
    assert_eq!(contents(read(&mut s)).1.len(), 1);
    tile(
        &mut s,
        TileEdit::InventorySet {
            slot: 27,
            item: Some(item("minecraft:stone", 1)),
        },
    );
    assert!(warned(&s, "block.inventory.set"));
    tile(&mut s, TileEdit::InventoryClear);
    assert!(contents(read(&mut s)).1.is_empty());
}

#[test]
fn a_full_inventory_reports_the_items_that_did_not_fit() {
    let mut s = arena();
    place(&mut s, "minecraft:hopper");
    for _ in 0..5 {
        tile(&mut s, TileEdit::InventoryAdd(item("minecraft:stone", 64)));
    }
    assert!(!warned(&s, "block.inventory.add"));

    tile(&mut s, TileEdit::InventoryAdd(item("minecraft:stone", 10)));

    assert!(warned(&s, "block.inventory.add"));
    let (size, slots) = contents(read(&mut s));
    assert_eq!((size, slots.len()), (5, 5));
}

#[test]
fn every_container_kind_round_trips_its_items_with_its_size() {
    let kinds = [
        ("minecraft:barrel", 27),
        ("minecraft:trapped_chest", 27),
        ("minecraft:dropper", 9),
        ("minecraft:dispenser", 9),
        ("minecraft:shulker_box", 27),
        ("minecraft:red_shulker_box", 27),
        ("minecraft:brewing_stand", 5),
        ("minecraft:chiseled_bookshelf", 6),
        ("minecraft:crafter", 9),
        ("minecraft:furnace", 3),
        ("minecraft:blast_furnace", 3),
        ("minecraft:smoker", 3),
        ("minecraft:copper_chest", 27),
    ];
    for (kind, size) in kinds {
        let mut s = arena();
        place(&mut s, kind);
        tile(
            &mut s,
            TileEdit::InventorySet {
                slot: 1,
                item: Some(item("minecraft:stick", 2)),
            },
        );
        let view = read(&mut s);
        let items = match view.data {
            TileData::Furnace(furnace) => (furnace.contents.size, keys(&furnace.contents.slots)),
            TileData::Inventory(inventory) => (inventory.size, keys(&inventory.slots)),
            other => panic!("{kind}: expected contents, got {other:?}"),
        };
        assert_eq!(items.0, size, "{kind}");
        assert_eq!(
            items.1,
            vec![(1, "minecraft:stick".to_owned(), 2)],
            "{kind}"
        );
    }
}

#[test]
fn a_chiseled_bookshelf_marks_the_slots_it_holds() {
    let mut s = arena();
    place(&mut s, "minecraft:chiseled_bookshelf");

    tile(
        &mut s,
        TileEdit::InventorySet {
            slot: 2,
            item: Some(item("minecraft:book", 1)),
        },
    );

    assert!(has(&properties(&mut s), "slot_2_occupied", "true"));
    tile(&mut s, TileEdit::InventoryClear);
    assert!(has(&properties(&mut s), "slot_2_occupied", "false"));
}

#[test]
fn a_furnace_has_its_progress_fields_set() {
    let mut s = arena();
    place(&mut s, "minecraft:furnace");

    tile(
        &mut s,
        TileEdit::Furnace(FurnaceEdit {
            burn_time: Some(100),
            cook_time: Some(5),
            cook_time_total: Some(200),
        }),
    );

    let TileData::Furnace(furnace) = read(&mut s).data else {
        panic!("expected a furnace");
    };
    assert_eq!(
        (
            furnace.burn_time,
            furnace.cook_time,
            furnace.cook_time_total
        ),
        (100, 5, 200)
    );
    assert!(!furnace.lit);
    tile(
        &mut s,
        TileEdit::Furnace(FurnaceEdit {
            burn_time: Some(-1),
            ..Default::default()
        }),
    );
    assert!(warned(&s, "block.furnace.set"));
    tile(&mut s, TileEdit::Furnace(FurnaceEdit::default()));
    place(&mut s, "minecraft:stone");
    tile(&mut s, TileEdit::Furnace(FurnaceEdit::default()));
    assert!(warned(&s, "block.furnace.set"));
}

#[test]
fn a_spawner_has_its_fields_set_and_refuses_inconsistent_ones() {
    let mut s = arena();
    place(&mut s, "minecraft:spawner");

    tile(
        &mut s,
        TileEdit::Spawner(SpawnerEdit {
            entity: Some("minecraft:zombie".into()),
            delay: Some(10),
            min_delay: Some(50),
            max_delay: Some(150),
            spawn_count: Some(2),
            max_nearby: Some(3),
            required_player_range: Some(24),
            spawn_range: Some(5),
        }),
    );

    let TileData::Spawner(spawner) = read(&mut s).data else {
        panic!("expected a spawner");
    };
    assert_eq!(spawner.entity, "minecraft:zombie");
    assert_eq!(
        (
            spawner.delay,
            spawner.min_delay,
            spawner.max_delay,
            spawner.spawn_count,
            spawner.max_nearby,
            spawner.required_player_range,
            spawner.spawn_range
        ),
        (10, 50, 150, 2, 3, 24, 5)
    );

    tile(
        &mut s,
        TileEdit::Spawner(SpawnerEdit {
            min_delay: Some(500),
            ..Default::default()
        }),
    );
    tile(
        &mut s,
        TileEdit::Spawner(SpawnerEdit {
            entity: Some("minecraft:nothing".into()),
            ..Default::default()
        }),
    );
    assert_eq!(
        s.warnings()
            .iter()
            .filter(|warning| warning.contains("block.spawner.set"))
            .count(),
        2
    );
    let TileData::Spawner(spawner) = read(&mut s).data else {
        panic!("expected a spawner");
    };
    assert_eq!(
        (spawner.min_delay, spawner.entity.as_str()),
        (50, "minecraft:zombie")
    );
}

#[test]
fn a_player_head_takes_an_owner_and_another_skull_refuses() {
    let mut s = arena();
    place(&mut s, "minecraft:player_head");
    let uuid = uuid::Uuid::from_u128(0xabcdef);

    tile(
        &mut s,
        TileEdit::SkullOwner {
            name: Some("Notch".into()),
            uuid: Some(uuid),
        },
    );

    assert_eq!(
        read(&mut s).data,
        TileData::Skull {
            name: Some("Notch".into()),
            uuid: Some(uuid)
        }
    );
    tile(
        &mut s,
        TileEdit::SkullOwner {
            name: None,
            uuid: None,
        },
    );
    assert_eq!(
        read(&mut s).data,
        TileData::Skull {
            name: None,
            uuid: None
        }
    );

    place(&mut s, "minecraft:zombie_head");
    tile(
        &mut s,
        TileEdit::SkullOwner {
            name: Some("Notch".into()),
            uuid: None,
        },
    );
    assert!(warned(&s, "block.skull.set_owner"));
}

#[test]
fn a_banner_takes_patterns_with_valid_colours_only() {
    let mut s = arena();
    place(&mut s, "minecraft:white_banner");

    tile(
        &mut s,
        TileEdit::BannerPatterns(vec![
            ("minecraft:stripe_top".into(), "red".into()),
            ("minecraft:border".into(), "blue".into()),
        ]),
    );

    assert_eq!(
        read(&mut s).data,
        TileData::Banner(vec![
            ("minecraft:stripe_top".into(), "red".into()),
            ("minecraft:border".into(), "blue".into())
        ])
    );
    tile(
        &mut s,
        TileEdit::BannerPatterns(vec![("minecraft:border".into(), "plaid".into())]),
    );
    tile(
        &mut s,
        TileEdit::BannerPatterns(vec![("minecraft:nothing".into(), "red".into())]),
    );
    assert_eq!(
        s.warnings()
            .iter()
            .filter(|warning| warning.contains("block.banner.set_patterns"))
            .count(),
        2
    );
}

#[test]
fn a_command_block_takes_a_command_and_a_name() {
    let mut s = arena();
    place(&mut s, "minecraft:command_block");

    tile(
        &mut s,
        TileEdit::CommandBlock {
            command: Some("say hi".into()),
            name: Some("§aBob".into()),
        },
    );

    assert_eq!(
        read(&mut s).data,
        TileData::CommandBlock {
            command: "say hi".into(),
            name: "§aBob".into()
        }
    );
    let nbt = s.server.block_entity_nbt(POS).expect("a block entity");
    assert_eq!(nbt.get_str("Command"), Some("say hi"));
    tile(
        &mut s,
        TileEdit::CommandBlock {
            command: None,
            name: Some(String::new()),
        },
    );
    assert_eq!(
        read(&mut s).data,
        TileData::CommandBlock {
            command: "say hi".into(),
            name: String::new()
        }
    );
}

#[test]
fn a_beacon_takes_effects_and_refuses_unknown_ones() {
    let mut s = arena();
    place(&mut s, "minecraft:beacon");

    tile(
        &mut s,
        TileEdit::Beacon {
            primary: Some("minecraft:speed".into()),
            secondary: Some("minecraft:regeneration".into()),
        },
    );

    assert_eq!(
        read(&mut s).data,
        TileData::Beacon {
            levels: 0,
            primary: Some("minecraft:speed".into()),
            secondary: Some("minecraft:regeneration".into())
        }
    );
    tile(
        &mut s,
        TileEdit::Beacon {
            primary: Some("minecraft:nothing".into()),
            secondary: None,
        },
    );
    assert!(warned(&s, "block.beacon.set_effects"));
}

#[test]
fn a_jukebox_and_a_lectern_hold_one_item() {
    let mut s = arena();
    place(&mut s, "minecraft:jukebox");
    tile(
        &mut s,
        TileEdit::JukeboxDisc(item("minecraft:music_disc_cat", 1)),
    );
    let TileData::Jukebox(Some(disc)) = read(&mut s).data else {
        panic!("expected a disc");
    };
    assert_eq!(disc.key, "minecraft:music_disc_cat");
    assert!(has(&properties(&mut s), "has_record", "true"));
    tile(&mut s, TileEdit::JukeboxClear);
    assert_eq!(read(&mut s).data, TileData::Jukebox(None));
    assert!(has(&properties(&mut s), "has_record", "false"));

    place(&mut s, "minecraft:lectern");
    tile(
        &mut s,
        TileEdit::LecternBook(item("minecraft:writable_book", 1)),
    );
    let TileData::Lectern(Some(book)) = read(&mut s).data else {
        panic!("expected a book");
    };
    assert_eq!(book.key, "minecraft:writable_book");
    assert!(has(&properties(&mut s), "has_book", "true"));
    tile(&mut s, TileEdit::LecternClear);
    assert_eq!(read(&mut s).data, TileData::Lectern(None));
    assert!(has(&properties(&mut s), "has_book", "false"));
}

#[test]
fn an_end_gateway_and_a_decorated_pot_are_set() {
    let mut s = arena();
    place(&mut s, "minecraft:end_gateway");
    tile(
        &mut s,
        TileEdit::EndGateway {
            exit: BlockPos::new(10, 70, -4),
            exact: true,
        },
    );
    assert_eq!(
        read(&mut s).data,
        TileData::EndGateway {
            exit: Some(BlockPos::new(10, 70, -4)),
            exact: true
        }
    );

    place(&mut s, "minecraft:decorated_pot");
    let sherds = vec![
        "minecraft:brick".to_owned(),
        "minecraft:brick".to_owned(),
        "minecraft:angler_pottery_sherd".to_owned(),
        "minecraft:brick".to_owned(),
    ];
    tile(&mut s, TileEdit::DecoratedPot(sherds.clone()));
    assert_eq!(read(&mut s).data, TileData::DecoratedPot(sherds));
    tile(
        &mut s,
        TileEdit::DecoratedPot(vec!["minecraft:brick".into()]),
    );
    assert!(warned(&s, "block.decorated_pot.set_sherds"));
}

#[test]
fn light_power_and_biome_are_read() {
    let mut s = arena();
    place(&mut s, "minecraft:stone");
    place_at(&mut s, BlockPos::new(5, 64, 4), "minecraft:redstone_block");

    let Answer::BlockInfo(BlockInfo {
        power,
        biome,
        light,
        sky_light,
        block_light,
    }) = s.ask(Question::BlockInfo(POS))
    else {
        panic!("expected block info");
    };

    assert_eq!(power, 15);
    assert!(biome.starts_with("minecraft:"));
    assert_eq!(light, sky_light.max(block_light));
    assert!((0..=15).contains(&light));
    assert_eq!(
        s.ask(Question::BlockInfo(BlockPos::new(100_000, 64, 0))),
        Answer::NotFound
    );
}
