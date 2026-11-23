mod fake_host;

use fake_host::*;
use rspigot_plugin::{
    BlockPos, BlockType, EquipmentSlot, InventoryType, ItemStack, Material, PlaceOptions, Position,
    ResourcePack, StructureRotation, Trade, Uuid,
};
use rspigot_plugin_protocol::v1::{
    self, answer, custom_inventory_order, from_plugin, lookup_answer, lookup_question,
    player_extra, question, server_order, to_plugin,
};

const fn vec3(x: f64, y: f64, z: f64) -> v1::Vec3 {
    v1::Vec3 { x, y, z }
}

fn extra(order: player_extra::Order) -> from_plugin::Body {
    from_plugin::Body::Player(v1::PlayerOrder {
        player: STEVE.to_owned(),
        order: Some(v1::player_order::Order::Extra(v1::PlayerExtra {
            order: Some(order),
        })),
    })
}

fn server(order: server_order::Order) -> from_plugin::Body {
    from_plugin::Body::Server(v1::ServerOrder { order: Some(order) })
}

fn lookup_answer(answer_body: lookup_answer::Answer) -> answer::Body {
    answer::Body::Lookup(v1::LookupAnswer {
        answer: Some(answer_body),
    })
}

async fn looked_up(session: &mut Session) -> (u64, lookup_question::Query) {
    match session.asked().await {
        (request, question::Question::Lookup(v1::LookupQuestion { query: Some(query) })) => {
            (request, query)
        }
        (_, other) => panic!("expected a lookup, heard {other:?}"),
    }
}

fn created(body: from_plugin::Body) -> (u64, v1::CreateInventory) {
    match body {
        from_plugin::Body::Server(v1::ServerOrder {
            order: Some(server_order::Order::CreateInventory(created)),
        }) => (created.id, created),
        body => panic!("expected an inventory to be created, heard {body:?}"),
    }
}

fn wire_trade(price: i32) -> v1::Trade {
    v1::Trade {
        result: Some(v1::Item {
            key: "minecraft:diamond".to_owned(),
            count: 1,
            ..Default::default()
        }),
        ingredients: vec![v1::Item {
            key: "minecraft:emerald".to_owned(),
            count: price,
            ..Default::default()
        }],
        max_uses: 4,
        price_multiplier: 0.0,
        ..Default::default()
    }
}

#[tokio::test]
async fn client_side_changes_and_resource_packs_go_on_the_wire() {
    let pack = Uuid::from_u128(5);
    let mut called = calling(|player| {
        Box::pin(async move {
            let client = &player.client;
            let stone = rspigot_plugin::BlockState {
                kind: BlockType::Stone,
                properties: Vec::new(),
            };
            client.send_block_change(BlockPos { x: 1, y: 70, z: 2 }, &stone);
            client.send_block_changes(&[
                (BlockPos { x: 1, y: 70, z: 3 }, stone.clone()),
                (BlockPos { x: 1, y: 70, z: 4 }, stone),
            ]);
            client.send_sign_change(
                BlockPos { x: 5, y: 64, z: 5 },
                &["a", "b"],
                Some("red"),
                true,
            );
            client.send_equipment_change(
                30,
                &[
                    (
                        EquipmentSlot::Head,
                        Some(ItemStack::new(Material::Diamond, 1)),
                    ),
                    (EquipmentSlot::Hand, None),
                ],
            );
            client.send_hurt_animation(45.0);
            client.send_resource_pack(
                &ResourcePack::new(Uuid::from_u128(5), "https://x.org/p.zip", "abc")
                    .required(true)
                    .prompt("hi"),
            );
            client.remove_resource_pack(Uuid::from_u128(5));
            client.remove_resource_packs();
        })
    })
    .await;
    let session = &mut called.session;

    let block = |x, y, z| v1::ClientBlock {
        pos: Some(v1::BlockPos { x, y, z }),
        state: Some(v1::BlockState {
            block: "minecraft:stone".to_owned(),
            properties: Vec::new(),
        }),
    };
    assert_eq!(
        session.hear().await,
        extra(player_extra::Order::SendBlockChanges(v1::ClientBlocks {
            changes: vec![block(1, 70, 2)]
        }))
    );
    assert_eq!(
        session.hear().await,
        extra(player_extra::Order::SendBlockChanges(v1::ClientBlocks {
            changes: vec![block(1, 70, 3), block(1, 70, 4)]
        }))
    );
    assert_eq!(
        session.hear().await,
        extra(player_extra::Order::SendSignChange(v1::ClientSign {
            pos: Some(v1::BlockPos { x: 5, y: 64, z: 5 }),
            lines: vec!["a".to_owned(), "b".to_owned()],
            color: Some("red".to_owned()),
            glowing: true,
        }))
    );
    let heard = session.hear().await;
    let from_plugin::Body::Player(v1::PlayerOrder {
        order:
            Some(v1::player_order::Order::Extra(v1::PlayerExtra {
                order: Some(player_extra::Order::SendEquipmentChange(equipment)),
            })),
        ..
    }) = heard.clone()
    else {
        panic!("expected an equipment change, heard {heard:?}");
    };
    assert_eq!(equipment.entity, 30);
    assert_eq!(equipment.slots[0].slot, "head");
    assert_eq!(
        equipment.slots[0].item.as_ref().unwrap().key,
        "minecraft:diamond"
    );
    assert_eq!(equipment.slots[1].slot, "mainhand");
    assert!(equipment.slots[1].item.is_none());
    assert_eq!(
        session.hear().await,
        extra(player_extra::Order::SendHurtAnimation(v1::ClientHurt {
            entity: 12,
            yaw: 45.0
        }))
    );
    assert_eq!(
        session.hear().await,
        extra(player_extra::Order::SendResourcePack(v1::PackOffer {
            id: pack.to_string(),
            url: "https://x.org/p.zip".to_owned(),
            hash: "abc".to_owned(),
            required: true,
            prompt: Some("hi".to_owned()),
        }))
    );
    assert_eq!(
        session.hear().await,
        extra(player_extra::Order::RemoveResourcePack(v1::PackRemoval {
            id: Some(pack.to_string())
        }))
    );
    assert_eq!(
        session.hear().await,
        extra(player_extra::Order::RemoveResourcePack(v1::PackRemoval {
            id: None
        }))
    );
    called.finished().await;
}

#[tokio::test]
async fn typed_inventories_chests_and_merchants_go_on_the_wire_and_read_back() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let server = player.server();
            let hopper = server
                .create_inventory_of(InventoryType::Hopper, "Hop")
                .await
                .unwrap();
            let chest = server.create_chest(18, "Chest").await.unwrap();
            let trade = Trade {
                result: ItemStack::new(Material::Diamond, 1),
                ingredients: vec![ItemStack::new(Material::Emerald, 2)],
                uses: 0,
                max_uses: 4,
                experience_reward: false,
                villager_experience: 0,
                price_multiplier: 0.0,
                demand: 0,
                special_price: 0,
            };
            let merchant = server
                .create_merchant("Shop", std::slice::from_ref(&trade))
                .await
                .unwrap();
            merchant.set_trades(&[trade.clone(), trade]);
            let state = merchant.get_state().await.unwrap();
            assert_eq!(state.kind, InventoryType::Merchant);
            assert_eq!(state.trades.len(), 2);
            assert_eq!(state.trades[0].ingredients[0].count, 2);
            let _ = (hopper, chest);
        })
    })
    .await;
    let session = &mut called.session;

    let (request, body) = session.acted().await;
    let (_, hopper) = created(body);
    assert_eq!(hopper.kind, v1::InventoryKind::Hopper as i32);
    assert_eq!(
        (hopper.slots, hopper.title.as_str(), hopper.locked),
        (0, "Hop", true)
    );
    session.say(answer(request, answer::Body::Done(v1::Empty {})));

    let (request, body) = session.acted().await;
    let (_, chest) = created(body);
    assert_eq!(chest.kind, v1::InventoryKind::Chest as i32);
    assert_eq!(chest.slots, 18);
    session.say(answer(request, answer::Body::Done(v1::Empty {})));

    let (request, body) = session.acted().await;
    let (menu, merchant) = created(body);
    assert_eq!(merchant.kind, v1::InventoryKind::Merchant as i32);
    assert!(!merchant.locked);
    assert_eq!(merchant.trades, vec![wire_trade(2)]);
    session.say(answer(request, answer::Body::Done(v1::Empty {})));

    assert_eq!(
        session.hear().await,
        from_plugin::Body::CustomInventory(v1::CustomInventoryOrder {
            inventory: menu,
            order: Some(custom_inventory_order::Order::SetTrades(v1::TradeList {
                trades: vec![wire_trade(2), wire_trade(2)],
            })),
        })
    );
    let (request, asked) = session.asked().await;
    assert_eq!(asked, question::Question::CustomInventory(menu));
    session.say(answer(
        request,
        answer::Body::CustomInventory(v1::CustomInventoryState {
            slots: vec![
                v1::Slot {
                    slot: 0,
                    item: None,
                },
                v1::Slot {
                    slot: 1,
                    item: None,
                },
                v1::Slot {
                    slot: 2,
                    item: None,
                },
            ],
            viewers: Vec::new(),
            kind: v1::InventoryKind::Merchant as i32,
            trades: vec![wire_trade(2), wire_trade(2)],
            progress: None,
        }),
    ));
    called.finished().await;
}

#[tokio::test]
async fn structures_loot_tags_and_registries_are_asked_and_read() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let server = player.server();
            let world = &player.world;
            let listed = world.structures.list().await.unwrap();
            assert_eq!(listed, ["minecraft:igloo"]);
            let size = world.structures.read("minecraft:igloo/top").await.unwrap();
            assert_eq!((size.x, size.y, size.z, size.blocks), (7, 5, 8, 90));
            let placed = world
                .structures
                .place(
                    "minecraft:igloo/top",
                    BlockPos { x: 1, y: 64, z: 2 },
                    PlaceOptions {
                        rotation: StructureRotation::Clockwise90,
                        ..Default::default()
                    },
                )
                .await
                .unwrap();
            assert_eq!(placed, 90);
            let found = world
                .structures
                .locate(
                    "minecraft:igloo",
                    Position {
                        x: 0.0,
                        y: 64.0,
                        z: 0.0,
                    },
                )
                .await
                .unwrap();
            assert_eq!(found.unwrap().0, "minecraft:igloo");
            let none = world
                .structures
                .locate("minecraft:igloo", Position::default())
                .await
                .unwrap();
            assert!(none.is_none());

            let loot = server
                .loot
                .generate(
                    &rspigot_plugin::LootRequest::new("minecraft:chests/x")
                        .looted_by(&player)
                        .luck(1.0)
                        .seed(9),
                )
                .await
                .unwrap();
            assert_eq!(loot.len(), 1);

            assert_eq!(
                server.registries.registries().await.unwrap(),
                ["minecraft:block"]
            );
            assert_eq!(
                server.registries.entries("block").await.unwrap(),
                ["minecraft:stone"]
            );
            assert_eq!(
                server.registries.tags("block").await.unwrap(),
                ["minecraft:mineable/pickaxe"]
            );
            assert!(
                server
                    .registries
                    .is_tagged("block", "mineable/pickaxe", "stone")
                    .await
                    .unwrap()
            );
            assert!(
                !server
                    .registries
                    .is_tagged("block", "mineable/pickaxe", "dirt")
                    .await
                    .unwrap()
            );
        })
    })
    .await;
    let session = &mut called.session;
    let names = |names: &[&str]| {
        lookup_answer(lookup_answer::Answer::Names(v1::NameList {
            names: names.iter().map(|name| (*name).to_owned()).collect(),
        }))
    };

    let (request, query) = looked_up(session).await;
    assert_eq!(query, lookup_question::Query::Structures(v1::Empty {}));
    session.say(answer(request, names(&["minecraft:igloo"])));

    let (request, query) = looked_up(session).await;
    assert_eq!(
        query,
        lookup_question::Query::Structure("minecraft:igloo/top".to_owned())
    );
    session.say(answer(
        request,
        lookup_answer(lookup_answer::Answer::Structure(v1::StructureSize {
            x: 7,
            y: 5,
            z: 8,
            blocks: 90,
        })),
    ));

    let (request, query) = looked_up(session).await;
    assert_eq!(
        query,
        lookup_question::Query::PlaceStructure(v1::PlaceStructure {
            template: "minecraft:igloo/top".to_owned(),
            pos: Some(v1::BlockPos { x: 1, y: 64, z: 2 }),
            rotation: "clockwise_90".to_owned(),
            mirror: "none".to_owned(),
            integrity: 1.0,
            seed: 0,
        })
    );
    session.say(answer(
        request,
        lookup_answer(lookup_answer::Answer::Placed(v1::StructurePlaced {
            blocks: 90,
        })),
    ));

    let (request, query) = looked_up(session).await;
    assert!(matches!(query, lookup_question::Query::LocateStructure(_)));
    session.say(answer(
        request,
        lookup_answer(lookup_answer::Answer::Located(v1::StructureLocation {
            structure: Some("minecraft:igloo".to_owned()),
            position: Some(v1::Vec3 {
                x: 5.0,
                y: 64.0,
                z: 5.0,
            }),
        })),
    ));
    let (request, _) = looked_up(session).await;
    session.say(answer(
        request,
        lookup_answer(lookup_answer::Answer::Located(
            v1::StructureLocation::default(),
        )),
    ));

    let (request, query) = looked_up(session).await;
    let lookup_question::Query::GenerateLoot(loot) = query else {
        panic!("expected a loot query");
    };
    assert_eq!(
        (
            loot.table.as_str(),
            loot.looter.as_deref(),
            loot.luck,
            loot.seed
        ),
        ("minecraft:chests/x", Some(STEVE), 1.0, Some(9))
    );
    session.say(answer(
        request,
        lookup_answer(lookup_answer::Answer::Loot(v1::LootItems {
            items: vec![v1::Item {
                key: "minecraft:diamond".to_owned(),
                count: 2,
                ..Default::default()
            }],
        })),
    ));

    let (request, query) = looked_up(session).await;
    assert_eq!(query, lookup_question::Query::Registries(v1::Empty {}));
    session.say(answer(request, names(&["minecraft:block"])));
    let (request, query) = looked_up(session).await;
    assert_eq!(
        query,
        lookup_question::Query::RegistryEntries("block".to_owned())
    );
    session.say(answer(request, names(&["minecraft:stone"])));
    let (request, query) = looked_up(session).await;
    assert_eq!(query, lookup_question::Query::Tags("block".to_owned()));
    session.say(answer(request, names(&["minecraft:mineable/pickaxe"])));
    for _ in 0..2 {
        let (request, query) = looked_up(session).await;
        assert_eq!(
            query,
            lookup_question::Query::TagMembers(v1::TagQuery {
                registry: "block".to_owned(),
                tag: "mineable/pickaxe".to_owned(),
            })
        );
        session.say(answer(request, names(&["minecraft:stone"])));
    }
    called.finished().await;
}

fn greeting(text: &str) -> from_plugin::Body {
    from_plugin::Body::Player(v1::PlayerOrder {
        player: STEVE.to_owned(),
        order: Some(v1::player_order::Order::SendMessage(text.to_owned())),
    })
}

fn scheduled(body: from_plugin::Body) -> v1::ScheduleTask {
    match body {
        from_plugin::Body::Server(v1::ServerOrder {
            order: Some(server_order::Order::ScheduleTask(task)),
        }) => task,
        body => panic!("expected a scheduled task, heard {body:?}"),
    }
}

fn due(id: u64) -> to_plugin::Body {
    to_plugin::Body::TaskDue(v1::TaskDue { id })
}

#[tokio::test]
async fn a_delayed_task_runs_once_when_the_host_says_it_is_due() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let speaker = player.clone();
            player.server().scheduler.run_later(3, move || async move {
                speaker.message.send("later");
            });
        })
    })
    .await;
    let session = &mut called.session;

    let task = scheduled(session.hear().await);
    assert_eq!((task.delay, task.period), (3, 0));
    session.say(due(task.id));
    assert_eq!(session.hear().await, greeting("later"));
    session.say(due(task.id));
    called.finished().await;
}

#[tokio::test]
async fn a_repeating_task_runs_on_each_tick_until_it_is_cancelled() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let speaker = player.clone();
            let task = player.server().scheduler.run_timer(2, 5, move || {
                let speaker = speaker.clone();
                async move { speaker.message.send("again") }
            });
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            task.cancel();
        })
    })
    .await;
    let session = &mut called.session;

    let task = scheduled(session.hear().await);
    assert_eq!((task.delay, task.period), (2, 5));
    session.say(due(task.id));
    assert_eq!(session.hear().await, greeting("again"));
    session.say(due(task.id));
    assert_eq!(session.hear().await, greeting("again"));
    assert_eq!(
        session.hear().await,
        server(server_order::Order::CancelTask(task.id))
    );
    session.say(due(task.id));
    called.finished().await;
}

#[tokio::test]
async fn criteria_viewer_border_cookie_and_link_orders_go_on_the_wire() {
    let mut called = calling(|player| {
        Box::pin(async move {
            player
                .criteria
                .award("minecraft:adventure/kill_all_mobs", "minecraft:zombie");
            player
                .criteria
                .revoke("minecraft:adventure/kill_all_mobs", "minecraft:zombie");
            player.view.hide_entity(41);
            player.view.show_entity(41);
            player.view.set_border_size(50.0);
            player
                .view
                .resize_border(30.0, std::time::Duration::from_secs(10));
            player.view.set_border_center(1.5, -2.5);
            player.view.reset_border();
            player.cookies.store("plugin:session", vec![1, 2]);
            player.links.send(&[("bug_report", "https://x.org/bugs")]);
            let cookie = player.cookies.retrieve("plugin:session").await.unwrap();
            assert_eq!(cookie, Some(vec![7, 7]));
        })
    })
    .await;
    let session = &mut called.session;
    let criterion = || v1::CriterionOrder {
        advancement: "minecraft:adventure/kill_all_mobs".to_owned(),
        criterion: "minecraft:zombie".to_owned(),
    };
    for expected in [
        extra(player_extra::Order::AwardCriterion(criterion())),
        extra(player_extra::Order::RevokeCriterion(criterion())),
        extra(player_extra::Order::HideEntity(41)),
        extra(player_extra::Order::ShowEntity(41)),
        extra(player_extra::Order::SetPlayerBorder(v1::WorldBorderOrder {
            change: Some(v1::world_border_order::Change::Resize(v1::BorderResize {
                size: 50.0,
                ticks: 0,
            })),
        })),
        extra(player_extra::Order::SetPlayerBorder(v1::WorldBorderOrder {
            change: Some(v1::world_border_order::Change::Resize(v1::BorderResize {
                size: 30.0,
                ticks: 200,
            })),
        })),
        extra(player_extra::Order::SetPlayerBorder(v1::WorldBorderOrder {
            change: Some(v1::world_border_order::Change::SetCenter(
                v1::BorderCenter { x: 1.5, z: -2.5 },
            )),
        })),
        extra(player_extra::Order::ResetPlayerBorder(v1::Empty {})),
        extra(player_extra::Order::StoreCookie(v1::CookieStore {
            key: "plugin:session".to_owned(),
            payload: vec![1, 2],
        })),
        extra(player_extra::Order::SendLinks(v1::LinkList {
            links: vec![v1::LinkEntry {
                label: "bug_report".to_owned(),
                url: "https://x.org/bugs".to_owned(),
            }],
        })),
    ] {
        assert_eq!(session.hear().await, expected);
    }
    let (request, asked) = session.asked().await;
    assert_eq!(
        asked,
        question::Question::Extra(v1::ExtraQuestion {
            query: Some(v1::extra_question::Query::RetrieveCookie(v1::CookieQuery {
                player: STEVE.to_owned(),
                key: "plugin:session".to_owned(),
            })),
        })
    );
    session.say(answer(
        request,
        answer::Body::Extra(v1::ExtraAnswer {
            answer: Some(v1::extra_answer::Answer::Cookie(v1::CookieValue {
                payload: Some(vec![7, 7]),
            })),
        }),
    ));
    called.finished().await;
}

#[tokio::test]
async fn biome_loot_tables_trees_rays_profiles_and_pdc_are_asked_and_read() {
    use rspigot_plugin::{
        BlockPos, FluidCollisionMode, NamespacedKey, PdcValue, RayTraceOptions, Target, TreeKind,
        Vector,
    };
    let mut called = calling(|player| {
        Box::pin(async move {
            let server = player.server();
            let world = &player.world;
            let block = world.block(BlockPos { x: 3, y: 64, z: 3 });
            block.set_biome("minecraft:desert");
            block.set_loot_table(Some("minecraft:chests/spawn_bonus_chest"), 5);
            block.set_loot_table(None, 0);
            assert_eq!(
                block.get_loot_table().await.unwrap(),
                Some(("minecraft:chests/spawn_bonus_chest".to_owned(), 5))
            );
            assert_eq!(server.loot.tables().await.unwrap(), ["minecraft:chests/x"]);
            let placed = world
                .structures
                .generate_tree(BlockPos { x: 4, y: 64, z: 4 }, TreeKind::Oak)
                .await
                .unwrap();
            assert_eq!(placed, 31);

            let options = RayTraceOptions {
                fluid: FluidCollisionMode::SourceOnly,
                ..Default::default()
            };
            let hit = world
                .ray_trace(
                    rspigot_plugin::Position {
                        x: 5.5,
                        y: 70.0,
                        z: 5.5,
                    },
                    Vector {
                        x: 0.0,
                        y: -1.0,
                        z: 0.0,
                    },
                    &options,
                )
                .await
                .unwrap();
            assert_eq!(hit.block.unwrap().block.pos, BlockPos { x: 5, y: 63, z: 5 });
            assert_eq!(hit.distance, 6.0);
            let hit = world
                .ray_trace(
                    rspigot_plugin::Position {
                        x: 5.5,
                        y: 65.0,
                        z: 5.5,
                    },
                    Vector {
                        x: 0.0,
                        y: 0.0,
                        z: 1.0,
                    },
                    &options,
                )
                .await
                .unwrap();
            let Some(Target::Entity(zombie)) = hit.entity else {
                panic!("an entity hit");
            };
            zombie.set_loot_table(Some("minecraft:chests/spawn_bonus_chest"), 2);

            let profile = server.profiles.by_name("Skinny").await.unwrap();
            assert_eq!(profile.textures, Some(("VALUE".to_owned(), None)));
            assert!(profile.online);

            let key = NamespacedKey::new("plugin", "level").unwrap();
            let chunk = world.chunks.at(1, 2);
            chunk.pdc().set(&key, 7_i32);
            world.pdc().set(&key, "arena");
            assert_eq!(chunk.pdc().get(&key).await.unwrap(), Some(PdcValue::Int(7)));
            assert_eq!(
                world.pdc().get(&key).await.unwrap(),
                Some(PdcValue::Text("arena".to_owned()))
            );
        })
    })
    .await;
    let session = &mut called.session;

    let block = |order| {
        from_plugin::Body::Block(v1::BlockOrder {
            pos: Some(v1::BlockPos { x: 3, y: 64, z: 3 }),
            order: Some(order),
        })
    };
    assert_eq!(
        session.hear().await,
        block(v1::block_order::Order::SetBiome(
            "minecraft:desert".to_owned()
        ))
    );
    assert_eq!(
        session.hear().await,
        block(v1::block_order::Order::SetLootTable(v1::LootTableSet {
            table: Some("minecraft:chests/spawn_bonus_chest".to_owned()),
            seed: 5
        }))
    );
    assert_eq!(
        session.hear().await,
        block(v1::block_order::Order::SetLootTable(v1::LootTableSet {
            table: None,
            seed: 0
        }))
    );

    let (request, query) = looked_up(session).await;
    assert_eq!(
        query,
        lookup_question::Query::Lootable(v1::LootTarget {
            target: Some(v1::loot_target::Target::Block(v1::BlockPos {
                x: 3,
                y: 64,
                z: 3
            })),
        })
    );
    session.say(answer(
        request,
        lookup_answer(lookup_answer::Answer::Lootable(v1::LootTableSet {
            table: Some("minecraft:chests/spawn_bonus_chest".to_owned()),
            seed: 5,
        })),
    ));
    let (request, query) = looked_up(session).await;
    assert_eq!(query, lookup_question::Query::LootTables(v1::Empty {}));
    session.say(answer(
        request,
        lookup_answer(lookup_answer::Answer::Names(v1::NameList {
            names: vec!["minecraft:chests/x".to_owned()],
        })),
    ));
    let (request, query) = looked_up(session).await;
    assert_eq!(
        query,
        lookup_question::Query::GenerateTree(v1::GenerateTree {
            pos: Some(v1::BlockPos { x: 4, y: 64, z: 4 }),
            feature: "oak".to_owned(),
        })
    );
    session.say(answer(
        request,
        lookup_answer(lookup_answer::Answer::Placed(v1::StructurePlaced {
            blocks: 31,
        })),
    ));

    let (request, query) = looked_up(session).await;
    let lookup_question::Query::RayTrace(ray) = query else {
        panic!("a ray trace");
    };
    assert_eq!(ray.fluid, v1::FluidMode::SourceOnly as i32);
    assert_eq!(ray.direction, Some(vec3(0.0, -1.0, 0.0)));
    session.say(answer(
        request,
        lookup_answer(lookup_answer::Answer::RayHit(v1::RayHit {
            hit: Some(v1::Target {
                hit: Some(v1::target::Hit::Block(v1::BlockHit {
                    pos: Some(v1::BlockPos { x: 5, y: 63, z: 5 }),
                    face: v1::Face::Up as i32,
                    block: "minecraft:stone".to_owned(),
                })),
            }),
            position: Some(vec3(5.5, 64.0, 5.5)),
            distance: 6.0,
        })),
    ));
    let (request, _) = looked_up(session).await;
    session.say(answer(
        request,
        lookup_answer(lookup_answer::Answer::RayHit(v1::RayHit {
            hit: Some(v1::Target {
                hit: Some(v1::target::Hit::Entity(v1::EntitySnapshot {
                    entity_id: 90,
                    uuid: "00000000-0000-0000-0000-0000000000aa".to_owned(),
                    kind: "minecraft:zombie".to_owned(),
                    position: Some(vec3(5.5, 64.0, 9.5)),
                    ..Default::default()
                })),
            }),
            position: Some(vec3(5.5, 65.0, 8.5)),
            distance: 3.0,
        })),
    ));
    assert_eq!(
        session.hear().await,
        from_plugin::Body::Entity(v1::EntityOrder {
            entity: 90,
            order: Some(v1::entity_order::Order::SetLootTable(v1::LootTableSet {
                table: Some("minecraft:chests/spawn_bonus_chest".to_owned()),
                seed: 2,
            })),
        })
    );

    let (request, query) = looked_up(session).await;
    assert_eq!(
        query,
        lookup_question::Query::Profile(v1::ProfileQuery {
            by: Some(v1::profile_query::By::Name("Skinny".to_owned())),
        })
    );
    session.say(answer(
        request,
        lookup_answer(lookup_answer::Answer::Profile(v1::ProfileInfo {
            name: "Skinny".to_owned(),
            uuid: STEVE.to_owned(),
            textures: Some(v1::ProfileTextures {
                value: "VALUE".to_owned(),
                signature: None,
            }),
            online: true,
        })),
    ));

    let pdc = |order| {
        from_plugin::Body::World(v1::WorldOrder {
            order: Some(v1::world_order::Order::Object(v1::ObjectWorldOrder {
                order: Some(order),
            })),
        })
    };
    let set = |value: v1::PdcValue| v1::PdcChange {
        change: Some(v1::pdc_change::Change::Set(v1::PdcSet {
            key: "plugin:level".to_owned(),
            value: Some(value),
        })),
    };
    assert_eq!(
        session.hear().await,
        pdc(v1::object_world_order::Order::ChunkPdc(
            v1::ChunkPdcChange {
                x: 1,
                z: 2,
                change: Some(set(v1::PdcValue {
                    value: Some(v1::pdc_value::Value::Int(7)),
                })),
            }
        ))
    );
    assert_eq!(
        session.hear().await,
        pdc(v1::object_world_order::Order::WorldPdc(set(v1::PdcValue {
            value: Some(v1::pdc_value::Value::Text("arena".to_owned())),
        })))
    );
    let held = |value: v1::pdc_value::Value| {
        answer::Body::Object(v1::ObjectAnswer {
            answer: Some(v1::object_answer::Answer::Pdc(v1::PdcContainer {
                entries: [(
                    "plugin:level".to_owned(),
                    v1::PdcValue { value: Some(value) },
                )]
                .into(),
            })),
        })
    };
    let (request, asked) = session.asked().await;
    assert!(matches!(
        asked,
        question::Question::Object(v1::ObjectQuestion {
            query: Some(v1::object_question::Query::Pdc(v1::PdcQuery {
                holder: Some(v1::pdc_query::Holder::Chunk(v1::ChunkCoords { x: 1, z: 2 })),
            })),
        })
    ));
    session.say(answer(request, held(v1::pdc_value::Value::Int(7))));
    let (request, asked) = session.asked().await;
    assert!(matches!(
        asked,
        question::Question::Object(v1::ObjectQuestion {
            query: Some(v1::object_question::Query::Pdc(v1::PdcQuery {
                holder: Some(v1::pdc_query::Holder::World(_)),
            })),
        })
    ));
    session.say(answer(
        request,
        held(v1::pdc_value::Value::Text("arena".to_owned())),
    ));
    called.finished().await;
}

#[tokio::test]
async fn merchant_progress_and_the_new_recipe_shapes_go_on_the_wire() {
    use rspigot_plugin::{Ingredient, MerchantProgress, Recipe};
    let mut called = calling(|player| {
        Box::pin(async move {
            let server = player.server();
            let progress = MerchantProgress {
                level: 3,
                xp: 40,
                show_progress: true,
                can_restock: true,
            };
            let merchant = server
                .create_merchant_with_progress("Shop", &[], progress)
                .await
                .unwrap();
            merchant.set_progress(MerchantProgress {
                level: 5,
                ..progress
            });
            let state = merchant.get_state().await.unwrap();
            assert_eq!(state.progress, Some(progress));

            let sword = Ingredient::item(Material::DiamondSword);
            let emerald = Ingredient::item(Material::Emerald);
            let magic = Ingredient::exact(ItemStack::new(Material::Stick, 1).name("Magic"));
            server
                .recipes
                .add(Recipe::transmute(
                    "plugin:t",
                    sword.clone(),
                    emerald.clone(),
                    ItemStack::new(Material::NetheriteSword, 1),
                ))
                .await
                .unwrap();
            server
                .recipes
                .add(Recipe::smithing_trim(
                    "plugin:trim",
                    magic.clone(),
                    sword,
                    emerald,
                    "minecraft:coast",
                    ItemStack::new(Material::DiamondSword, 1),
                ))
                .await
                .unwrap();
        })
    })
    .await;
    let session = &mut called.session;

    let (request, body) = session.acted().await;
    let (menu, created) = created(body);
    assert_eq!(
        created.progress,
        Some(v1::MerchantProgress {
            level: 3,
            xp: 40,
            show_progress: true,
            can_restock: true
        })
    );
    session.say(answer(request, answer::Body::Done(v1::Empty {})));
    assert_eq!(
        session.hear().await,
        from_plugin::Body::CustomInventory(v1::CustomInventoryOrder {
            inventory: menu,
            order: Some(custom_inventory_order::Order::SetProgress(
                v1::MerchantProgress {
                    level: 5,
                    xp: 40,
                    show_progress: true,
                    can_restock: true
                }
            )),
        })
    );
    let (request, _) = session.asked().await;
    session.say(answer(
        request,
        answer::Body::CustomInventory(v1::CustomInventoryState {
            kind: v1::InventoryKind::Merchant as i32,
            progress: Some(v1::MerchantProgress {
                level: 3,
                xp: 40,
                show_progress: true,
                can_restock: true,
            }),
            ..Default::default()
        }),
    ));

    let (request, body) = session.acted().await;
    let from_plugin::Body::Server(v1::ServerOrder {
        order: Some(server_order::Order::AddRecipe(def)),
    }) = body
    else {
        panic!("a recipe");
    };
    assert!(matches!(def.body, Some(v1::recipe_def::Body::Transmute(_))));
    session.say(answer(request, answer::Body::Done(v1::Empty {})));
    let (request, body) = session.acted().await;
    let from_plugin::Body::Server(v1::ServerOrder {
        order: Some(server_order::Order::AddRecipe(def)),
    }) = body
    else {
        panic!("a recipe");
    };
    let Some(v1::recipe_def::Body::SmithingTrim(trim)) = def.body else {
        panic!("a trim recipe");
    };
    assert_eq!(trim.pattern, "minecraft:coast");
    assert!(matches!(
        trim.template.and_then(|ingredient| ingredient.choice),
        Some(v1::recipe_ingredient::Choice::Exact(_))
    ));
    session.say(answer(request, answer::Body::Done(v1::Empty {})));
    called.finished().await;
}
