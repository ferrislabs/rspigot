mod fake_host;

use fake_host::*;
use rspigot_plugin::{
    BlockPos, BlockType, DyeColor, EffectType, EntityType, ItemStack, Material, Side, Uuid,
};
use rspigot_plugin_protocol::v1::{
    self, answer, block_order, from_plugin, question, tile_edit::Edit, tile_state,
};

const AT: v1::BlockPos = v1::BlockPos { x: 1, y: 64, z: -2 };

fn tile(edit: Edit) -> from_plugin::Body {
    from_plugin::Body::Block(v1::BlockOrder {
        pos: Some(AT),
        order: Some(block_order::Order::Tile(v1::TileEdit { edit: Some(edit) })),
    })
}

fn stick() -> v1::Item {
    v1::Item {
        key: "minecraft:stick".to_owned(),
        count: 2,
        ..Default::default()
    }
}

async fn serve_tile(called: &mut Called, state: tile_state::State) {
    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, question::Question::Tile(AT));
    called.session.say(answer(
        request,
        answer::Body::Tile(v1::TileState {
            kind: "minecraft:x".to_owned(),
            state: Some(state),
        }),
    ));
}

#[tokio::test]
async fn a_block_handle_sends_data_and_tile_orders() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let block = player.world.block(BlockPos { x: 1, y: 64, z: -2 });
            block.data.set(&[("facing", "north")]);
            block.set_data(&[("half", "top")]);
            block.sign.set_lines(Side::Back, &["a", "b"]);
            block.sign.set_color(Side::Front, DyeColor::Red);
            block.sign.set_glowing(Side::Front, true);
            block.sign.set_waxed(true);
            let stack = ItemStack::new(Material::Stick, 2);
            block.inventory.set(3, &stack);
            block.inventory.clear_slot(4);
            block.inventory.add(&stack);
            block.inventory.clear();
            block.furnace.set_burn_time(100);
            block.furnace.set_cook_time(5);
            block.furnace.set_cook_time_total(200);
            block.spawner.set_entity_type(EntityType::Zombie);
            block.spawner.set_delay(1);
            block.spawner.set_min_delay(2);
            block.spawner.set_max_delay(3);
            block.spawner.set_spawn_count(4);
            block.spawner.set_max_nearby(5);
            block.spawner.set_required_player_range(6);
            block.spawner.set_spawn_range(7);
            block.skull.set_owner("Notch");
            block.skull.set_owner_uuid(Uuid::from_u128(3));
            block.skull.clear_owner();
            block
                .banner
                .set_patterns(&[("minecraft:border", DyeColor::Blue)]);
            block.command.set_command("say hi");
            block.command.set_name("n");
            block.beacon.set_effects(Some(EffectType::Speed), None);
            block.jukebox.set_disc(&stack);
            block.jukebox.clear();
            block.lectern.set_book(&stack);
            block.lectern.clear();
            block
                .end_gateway
                .set_exit(BlockPos { x: 5, y: 6, z: 7 }, true);
            block.decorated_pot.set_sherds([
                Material::Brick,
                Material::Brick,
                Material::Brick,
                Material::Brick,
            ]);
        })
    })
    .await;

    let property = |name: &str, value: &str| v1::Property {
        name: name.to_owned(),
        value: value.to_owned(),
    };
    for (name, value) in [("facing", "north"), ("half", "top")] {
        assert_eq!(
            called.session.hear().await,
            from_plugin::Body::Block(v1::BlockOrder {
                pos: Some(AT),
                order: Some(block_order::Order::SetData(v1::BlockDataEdit {
                    properties: vec![property(name, value)],
                })),
            })
        );
    }
    let expected = [
        Edit::SignLines(v1::SignLines {
            front: false,
            lines: vec!["a".to_owned(), "b".to_owned()],
        }),
        Edit::SignStyle(v1::SignStyle {
            front: true,
            color: Some("red".to_owned()),
            glowing: None,
        }),
        Edit::SignStyle(v1::SignStyle {
            front: true,
            color: None,
            glowing: Some(true),
        }),
        Edit::SignWaxed(true),
        Edit::InventorySet(v1::SetSlot {
            slot: 3,
            item: Some(stick()),
        }),
        Edit::InventorySet(v1::SetSlot {
            slot: 4,
            item: None,
        }),
        Edit::InventoryAdd(stick()),
        Edit::InventoryClear(v1::Empty {}),
        Edit::Furnace(v1::FurnaceEdit {
            burn_time: Some(100),
            ..Default::default()
        }),
        Edit::Furnace(v1::FurnaceEdit {
            cook_time: Some(5),
            ..Default::default()
        }),
        Edit::Furnace(v1::FurnaceEdit {
            cook_time_total: Some(200),
            ..Default::default()
        }),
        Edit::Spawner(v1::SpawnerEdit {
            entity: Some("minecraft:zombie".to_owned()),
            ..Default::default()
        }),
        Edit::Spawner(v1::SpawnerEdit {
            delay: Some(1),
            ..Default::default()
        }),
        Edit::Spawner(v1::SpawnerEdit {
            min_delay: Some(2),
            ..Default::default()
        }),
        Edit::Spawner(v1::SpawnerEdit {
            max_delay: Some(3),
            ..Default::default()
        }),
        Edit::Spawner(v1::SpawnerEdit {
            spawn_count: Some(4),
            ..Default::default()
        }),
        Edit::Spawner(v1::SpawnerEdit {
            max_nearby: Some(5),
            ..Default::default()
        }),
        Edit::Spawner(v1::SpawnerEdit {
            required_player_range: Some(6),
            ..Default::default()
        }),
        Edit::Spawner(v1::SpawnerEdit {
            spawn_range: Some(7),
            ..Default::default()
        }),
        Edit::SkullOwner(v1::SkullOwner {
            name: Some("Notch".to_owned()),
            uuid: None,
        }),
        Edit::SkullOwner(v1::SkullOwner {
            name: None,
            uuid: Some(Uuid::from_u128(3).to_string()),
        }),
        Edit::SkullOwner(v1::SkullOwner::default()),
        Edit::BannerPatterns(v1::BannerPatterns {
            patterns: vec![v1::BannerPattern {
                pattern: "minecraft:border".to_owned(),
                color: "blue".to_owned(),
            }],
        }),
        Edit::CommandBlock(v1::CommandEdit {
            command: Some("say hi".to_owned()),
            name: None,
        }),
        Edit::CommandBlock(v1::CommandEdit {
            command: None,
            name: Some("n".to_owned()),
        }),
        Edit::Beacon(v1::BeaconEffects {
            primary: Some("minecraft:speed".to_owned()),
            secondary: None,
        }),
        Edit::JukeboxDisc(stick()),
        Edit::JukeboxClear(v1::Empty {}),
        Edit::LecternBook(stick()),
        Edit::LecternClear(v1::Empty {}),
        Edit::EndGateway(v1::GatewayExit {
            exit: Some(v1::BlockPos { x: 5, y: 6, z: 7 }),
            exact: true,
        }),
        Edit::DecoratedPot(v1::PotSherds {
            sherds: vec!["minecraft:brick".to_owned(); 4],
        }),
    ];
    for wanted in expected {
        assert_eq!(called.session.hear().await, tile(wanted));
    }
    called.finished().await;
}

#[tokio::test]
async fn the_tile_reads_decode_each_kind_and_refuse_the_wrong_one() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let block = player.world.block(BlockPos { x: 1, y: 64, z: -2 });
            let sign = block.sign.get().await.unwrap();
            assert_eq!(sign.front, ["a"]);
            assert_eq!(sign.front_color, DyeColor::Red);
            assert!(sign.waxed && sign.front_glowing);
            let items = block.inventory.get_items().await.unwrap();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0].0, 2);
            assert_eq!(items[0].1.material, Material::Stick);
            assert_eq!(block.inventory.get_size().await.unwrap(), 27);
            let furnace = block.furnace.get().await.unwrap();
            assert_eq!((furnace.burn_time, furnace.lit), (100, true));
            let spawner = block.spawner.get().await.unwrap();
            assert_eq!(spawner.entity, Some(EntityType::Zombie));
            assert_eq!(spawner.max_nearby, 6);
            let owner = block.skull.get_owner().await.unwrap();
            assert_eq!(owner.name.as_deref(), Some("Notch"));
            assert_eq!(owner.uuid, Some(Uuid::from_u128(3)));
            assert_eq!(
                block.banner.get_patterns().await.unwrap(),
                [("minecraft:border".to_owned(), DyeColor::Blue)]
            );
            let command = block.command.get().await.unwrap();
            assert_eq!(command.command, "say hi");
            let beacon = block.beacon.get().await.unwrap();
            assert_eq!(beacon.primary, Some(EffectType::Speed));
            assert_eq!(beacon.secondary, None);
            assert!(block.jukebox.get_disc().await.unwrap().is_some());
            assert!(block.lectern.get_book().await.unwrap().is_none());
            assert_eq!(
                block.end_gateway.get_exit().await.unwrap(),
                Some((BlockPos { x: 5, y: 6, z: 7 }, true))
            );
            assert_eq!(
                block.decorated_pot.get_sherds().await.unwrap(),
                [Material::Brick; 4]
            );
            assert!(block.sign.get().await.is_err());
        })
    })
    .await;

    let slots = |size| v1::InventoryContents {
        size,
        slots: vec![v1::Slot {
            slot: 2,
            item: Some(stick()),
        }],
    };
    serve_tile(
        &mut called,
        tile_state::State::Sign(v1::SignState {
            front: vec!["a".to_owned()],
            waxed: true,
            front_color: "red".to_owned(),
            back_color: "black".to_owned(),
            front_glowing: true,
            ..Default::default()
        }),
    )
    .await;
    serve_tile(&mut called, tile_state::State::Inventory(slots(27))).await;
    serve_tile(&mut called, tile_state::State::Inventory(slots(27))).await;
    serve_tile(
        &mut called,
        tile_state::State::Furnace(v1::FurnaceState {
            burn_time: 100,
            lit: true,
            contents: Some(slots(3)),
            ..Default::default()
        }),
    )
    .await;
    serve_tile(
        &mut called,
        tile_state::State::Spawner(v1::SpawnerState {
            entity: "minecraft:zombie".to_owned(),
            max_nearby: 6,
            ..Default::default()
        }),
    )
    .await;
    serve_tile(
        &mut called,
        tile_state::State::Skull(v1::SkullOwner {
            name: Some("Notch".to_owned()),
            uuid: Some(Uuid::from_u128(3).to_string()),
        }),
    )
    .await;
    serve_tile(
        &mut called,
        tile_state::State::Banner(v1::BannerPatterns {
            patterns: vec![v1::BannerPattern {
                pattern: "minecraft:border".to_owned(),
                color: "blue".to_owned(),
            }],
        }),
    )
    .await;
    serve_tile(
        &mut called,
        tile_state::State::CommandBlock(v1::CommandState {
            command: "say hi".to_owned(),
            name: String::new(),
        }),
    )
    .await;
    serve_tile(
        &mut called,
        tile_state::State::Beacon(v1::BeaconState {
            levels: 4,
            primary: Some("minecraft:speed".to_owned()),
            secondary: None,
        }),
    )
    .await;
    serve_tile(
        &mut called,
        tile_state::State::Jukebox(v1::DiscState {
            item: Some(stick()),
        }),
    )
    .await;
    serve_tile(
        &mut called,
        tile_state::State::Lectern(v1::DiscState { item: None }),
    )
    .await;
    serve_tile(
        &mut called,
        tile_state::State::EndGateway(v1::GatewayExit {
            exit: Some(v1::BlockPos { x: 5, y: 6, z: 7 }),
            exact: true,
        }),
    )
    .await;
    serve_tile(
        &mut called,
        tile_state::State::DecoratedPot(v1::PotSherds {
            sherds: vec!["minecraft:brick".to_owned(); 4],
        }),
    )
    .await;
    serve_tile(
        &mut called,
        tile_state::State::Jukebox(v1::DiscState::default()),
    )
    .await;
    called.finished().await;
}

#[tokio::test]
async fn block_data_light_power_and_biome_are_awaited_reads() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let block = player.world.block(BlockPos { x: 1, y: 64, z: -2 });
            let data = block.get_data().await.unwrap();
            assert_eq!(data.kind, BlockType::OakStairs);
            assert_eq!(
                data.properties.get("facing").map(String::as_str),
                Some("east")
            );
            assert_eq!(
                block.data.get_property("half").await.unwrap().as_deref(),
                Some("top")
            );
            assert_eq!(block.get_light().await.unwrap(), 15);
            assert_eq!(block.get_sky_light().await.unwrap(), 15);
            assert_eq!(block.get_block_light().await.unwrap(), 3);
            assert_eq!(block.get_power().await.unwrap(), 4);
            assert!(block.is_powered().await.unwrap());
            assert_eq!(block.get_biome().await.unwrap(), "minecraft:plains");
        })
    })
    .await;

    let stairs = || v1::BlockState {
        block: "minecraft:oak_stairs".to_owned(),
        properties: vec![
            v1::Property {
                name: "facing".to_owned(),
                value: "east".to_owned(),
            },
            v1::Property {
                name: "half".to_owned(),
                value: "top".to_owned(),
            },
        ],
    };
    for _ in 0..2 {
        let (request, asked) = called.session.asked().await;
        assert_eq!(asked, question::Question::Block(AT));
        called
            .session
            .say(answer(request, answer::Body::Block(stairs())));
    }
    for _ in 0..5 {
        let (request, asked) = called.session.asked().await;
        assert_eq!(asked, question::Question::BlockInfo(AT));
        called.session.say(answer(
            request,
            answer::Body::BlockInfo(v1::BlockInfo {
                light: 15,
                sky_light: 15,
                block_light: 3,
                power: 4,
                biome: "minecraft:plains".to_owned(),
            }),
        ));
    }
    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, question::Question::BlockInfo(AT));
    called.session.say(answer(
        request,
        answer::Body::BlockInfo(v1::BlockInfo {
            power: 4,
            biome: "minecraft:plains".to_owned(),
            ..Default::default()
        }),
    ));
    called.finished().await;
}
