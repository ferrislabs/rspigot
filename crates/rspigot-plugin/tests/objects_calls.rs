mod fake_host;

use std::collections::BTreeMap;

use fake_host::*;
use rspigot_plugin::{
    Billboard, BlockPos, DisplayProps, EntityType, FireworkLaunch, HangingFace, InteractionBox,
    ItemStack, MapMarker, Material, NamespacedKey, PdcValue, Position, TextAlignment, TextDisplay,
    Trade, Transformation, Uuid, Vector,
};
use rspigot_plugin_protocol::v1::{
    self, answer, block_order, entity_order, from_plugin, map_decoration_edit, map_edit,
    object_answer, object_entity_order, object_player_order, object_question, object_world_order,
    pdc_change, pdc_query, pdc_value, player_order, question, world_order,
};

const COW: i32 = 31;

const fn vec3(x: f64, y: f64, z: f64) -> v1::Vec3 {
    v1::Vec3 { x, y, z }
}

fn snapshot(id: i32) -> v1::EntitySnapshot {
    v1::EntitySnapshot {
        entity_id: id,
        uuid: Uuid::from_u128(id as u128).to_string(),
        kind: "minecraft:cow".to_owned(),
        position: Some(vec3(0.0, 64.0, 0.0)),
        ..Default::default()
    }
}

async fn serve_list(called: &mut Called) {
    let (request, _) = called.session.asked().await;
    called.session.say(answer(
        request,
        answer::Body::Entities(v1::EntityList {
            entities: vec![snapshot(COW)],
        }),
    ));
}

fn object(order: object_entity_order::Order) -> from_plugin::Body {
    from_plugin::Body::Entity(v1::EntityOrder {
        entity: COW,
        order: Some(entity_order::Order::Object(v1::ObjectEntityOrder {
            order: Some(order),
        })),
    })
}

fn player_object(order: object_player_order::Order) -> from_plugin::Body {
    from_plugin::Body::Player(v1::PlayerOrder {
        player: STEVE.to_owned(),
        order: Some(player_order::Order::Object(v1::ObjectPlayerOrder {
            order: Some(order),
        })),
    })
}

fn map_order(edit: map_edit::Edit) -> from_plugin::Body {
    from_plugin::Body::World(v1::WorldOrder {
        order: Some(world_order::Order::Object(v1::ObjectWorldOrder {
            order: Some(object_world_order::Order::EditMap(v1::MapEdit {
                map: 7,
                edit: Some(edit),
            })),
        })),
    })
}

fn asking(query: object_question::Query) -> question::Question {
    question::Question::Object(v1::ObjectQuestion { query: Some(query) })
}

fn object_body(body: object_answer::Answer) -> answer::Body {
    answer::Body::Object(v1::ObjectAnswer { answer: Some(body) })
}

fn int(value: i32) -> v1::PdcValue {
    v1::PdcValue {
        value: Some(pdc_value::Value::Int(value)),
    }
}

fn set_pdc(key: &str, value: v1::PdcValue) -> v1::PdcChange {
    v1::PdcChange {
        change: Some(pdc_change::Change::Set(v1::PdcSet {
            key: key.to_owned(),
            value: Some(value),
        })),
    }
}

fn wire_item(key: &str, count: i32) -> v1::Item {
    v1::Item {
        key: key.to_owned(),
        count,
        ..Default::default()
    }
}

fn wire_trade() -> v1::Trade {
    v1::Trade {
        result: Some(wire_item("minecraft:diamond", 2)),
        ingredients: vec![wire_item("minecraft:emerald", 5)],
        uses: 0,
        max_uses: 8,
        experience_reward: true,
        villager_experience: 3,
        price_multiplier: 0.05,
        demand: 0,
        special_price: 0,
    }
}

fn trade() -> Trade {
    Trade {
        result: ItemStack::new(Material::Diamond, 2),
        ingredients: vec![ItemStack::new(Material::Emerald, 5)],
        uses: 0,
        max_uses: 8,
        experience_reward: true,
        villager_experience: 3,
        price_multiplier: 0.05,
        demand: 0,
        special_price: 0,
    }
}

#[tokio::test]
async fn pdc_merchant_display_firework_vehicle_frame_and_map_orders_go_on_the_wire() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let cow = player
                .world
                .entities
                .get_entities()
                .await
                .unwrap()
                .remove(0);
            let level = NamespacedKey::new("plugin", "level").unwrap();
            let nested = NamespacedKey::parse("plugin:nested").unwrap();
            cow.pdc.set(&level, 7);
            cow.pdc.set(
                &nested,
                BTreeMap::from([("n".to_owned(), PdcValue::Text("x".to_owned()))]),
            );
            cow.pdc.remove(&level);
            cow.pdc.clear();
            player
                .world
                .objects
                .block_data(BlockPos { x: 1, y: 64, z: 2 })
                .set(&level, 5_i64);
            player.objects.item_data(3).set(&level, true);
            cow.merchant.set_trades(&[trade()]);
            player.objects.open_merchant(&cow);
            cow.display.edit(
                TextDisplay::new("holo")
                    .props(
                        DisplayProps::default()
                            .billboard(Billboard::Center)
                            .brightness(4, 9)
                            .transformation(Transformation::default().scale(2.0, 2.0, 2.0)),
                    )
                    .alignment(TextAlignment::Left),
            );
            cow.display
                .edit(InteractionBox::new(2.0, 3.0).response(true));
            cow.firework.detonate();
            cow.vehicle.set_velocity(Vector {
                x: 0.5,
                y: 0.0,
                z: -0.5,
            });
            cow.frame
                .set_item(Some(&ItemStack::new(Material::Compass, 1)));
            cow.frame.set_item(None);
            cow.frame.set_rotation(3);
            cow.frame.set_visible(false);
            let map = player.world.objects.map(7);
            map.set_center(256, -128);
            map.set_scale(2);
            map.set_locked(true);
            map.set_tracking_position(false);
            map.set_unlimited_tracking(true);
            map.set_pixel(3, 4, 44);
            map.set_canvas(&[1, 2, 3]);
            player.objects.set_map_marker(
                0,
                "home",
                &MapMarker {
                    kind: "minecraft:red_x".to_owned(),
                    x: 1.0,
                    z: 2.0,
                    rotation: 90.0,
                },
            );
            player.objects.remove_map_marker(0, "home");
        })
    })
    .await;
    serve_list(&mut called).await;

    let level = "plugin:level";
    let pdc_order = |change| object(object_entity_order::Order::Pdc(change));
    for expected in [
        pdc_order(set_pdc(level, int(7))),
        pdc_order(set_pdc(
            "plugin:nested",
            v1::PdcValue {
                value: Some(pdc_value::Value::Container(v1::PdcContainer {
                    entries: [(
                        "n".to_owned(),
                        v1::PdcValue {
                            value: Some(pdc_value::Value::Text("x".to_owned())),
                        },
                    )]
                    .into(),
                })),
            },
        )),
        pdc_order(v1::PdcChange {
            change: Some(pdc_change::Change::Remove(level.to_owned())),
        }),
        pdc_order(v1::PdcChange {
            change: Some(pdc_change::Change::Clear(v1::Empty {})),
        }),
        from_plugin::Body::Block(v1::BlockOrder {
            pos: Some(v1::BlockPos { x: 1, y: 64, z: 2 }),
            order: Some(block_order::Order::Pdc(set_pdc(
                level,
                v1::PdcValue {
                    value: Some(pdc_value::Value::Long(5)),
                },
            ))),
        }),
        player_object(object_player_order::Order::ItemPdc(v1::ItemPdc {
            slot: 3,
            change: Some(set_pdc(
                level,
                v1::PdcValue {
                    value: Some(pdc_value::Value::Byte(1)),
                },
            )),
        })),
    ] {
        assert_eq!(called.session.hear().await, expected);
    }

    assert_eq!(
        called.session.hear().await,
        object(object_entity_order::Order::SetTrades(v1::TradeList {
            trades: vec![wire_trade()],
        }))
    );
    assert_eq!(
        called.session.hear().await,
        player_object(object_player_order::Order::OpenMerchant(COW))
    );
    let from_plugin::Body::Entity(v1::EntityOrder {
        order:
            Some(entity_order::Order::Object(v1::ObjectEntityOrder {
                order: Some(object_entity_order::Order::EditDisplay(text)),
            })),
        ..
    }) = called.session.hear().await
    else {
        panic!("expected a display edit");
    };
    let Some(v1::display_spec::Body::Text(data)) = text.body else {
        panic!("expected text data");
    };
    assert_eq!(data.text.as_deref(), Some("holo"));
    assert_eq!(data.alignment, Some(v1::TextAlignment::Left as i32));
    let props = text.props.expect("props");
    assert_eq!(props.billboard, Some(v1::Billboard::Center as i32));
    assert_eq!(
        props.brightness,
        Some(v1::DisplayBrightness { block: 4, sky: 9 })
    );
    let from_plugin::Body::Entity(v1::EntityOrder {
        order:
            Some(entity_order::Order::Object(v1::ObjectEntityOrder {
                order: Some(object_entity_order::Order::EditDisplay(interaction)),
            })),
        ..
    }) = called.session.hear().await
    else {
        panic!("expected an interaction edit");
    };
    assert_eq!(
        interaction.body,
        Some(v1::display_spec::Body::Interaction(v1::InteractionData {
            width: Some(2.0),
            height: Some(3.0),
            response: Some(true),
        }))
    );
    for expected in [
        object(object_entity_order::Order::DetonateFirework(v1::Empty {})),
        object(object_entity_order::Order::SetVehicleVelocity(vec3(
            0.5, 0.0, -0.5,
        ))),
        object(object_entity_order::Order::SetFrameItem(v1::FrameItem {
            item: Some(wire_item("minecraft:compass", 1)),
        })),
        object(object_entity_order::Order::SetFrameItem(v1::FrameItem {
            item: None,
        })),
        object(object_entity_order::Order::SetFrameRotation(3)),
        object(object_entity_order::Order::SetFrameVisible(false)),
        map_order(map_edit::Edit::SetCenter(v1::MapCenter { x: 256, z: -128 })),
        map_order(map_edit::Edit::SetScale(2)),
        map_order(map_edit::Edit::SetLocked(true)),
        map_order(map_edit::Edit::SetTrackingPosition(false)),
        map_order(map_edit::Edit::SetUnlimitedTracking(true)),
        map_order(map_edit::Edit::SetPixel(v1::MapPixel {
            x: 3,
            y: 4,
            color: 44,
        })),
        map_order(map_edit::Edit::SetCanvas(vec![1, 2, 3])),
        player_object(object_player_order::Order::MapDecoration(
            v1::MapDecorationEdit {
                slot: 0,
                key: "home".to_owned(),
                change: Some(map_decoration_edit::Change::Set(v1::MapMarker {
                    kind: "minecraft:red_x".to_owned(),
                    x: 1.0,
                    z: 2.0,
                    rotation: 90.0,
                })),
            },
        )),
        player_object(object_player_order::Order::MapDecoration(
            v1::MapDecorationEdit {
                slot: 0,
                key: "home".to_owned(),
                change: Some(map_decoration_edit::Change::Remove(v1::Empty {})),
            },
        )),
    ] {
        assert_eq!(called.session.hear().await, expected);
    }
    called.finished().await;
}

#[tokio::test]
async fn launches_spawns_and_reads_are_asked_and_answered() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let cow = player
                .world
                .entities
                .get_entities()
                .await
                .unwrap()
                .remove(0);
            let objects = &player.world.objects;
            let at = Position {
                x: 1.0,
                y: 65.0,
                z: 2.0,
            };
            let arrow = objects
                .launch_projectile(
                    EntityType::Arrow,
                    at,
                    Vector {
                        x: 0.0,
                        y: 1.0,
                        z: 0.0,
                    },
                    Some(&cow),
                )
                .await
                .unwrap();
            assert_eq!(arrow.entity_id, 40);
            let rocket = ItemStack::new(Material::FireworkRocket, 1);
            objects
                .launch_firework(
                    at,
                    &rocket,
                    FireworkLaunch::new().shooter(&cow).angled(false),
                )
                .await
                .unwrap();
            objects
                .spawn_vehicle(EntityType::OakBoat, at, None)
                .await
                .unwrap();
            objects
                .place_item_frame(
                    BlockPos { x: 5, y: 64, z: 4 },
                    HangingFace::North,
                    true,
                    None,
                )
                .await
                .unwrap();
            objects
                .place_painting(
                    BlockPos { x: 5, y: 66, z: 4 },
                    HangingFace::North,
                    Some("minecraft:kebab"),
                )
                .await
                .unwrap();
            objects
                .spawn_display(at, InteractionBox::new(1.0, 1.0))
                .await
                .unwrap();

            let trades = cow.merchant.trades().await.unwrap();
            assert_eq!(trades, vec![trade()]);
            let spec = cow.display.get().await.unwrap();
            let rspigot_plugin::DisplaySpec::Text(text) = spec else {
                panic!("expected a text display");
            };
            assert_eq!(text.text.as_deref(), Some("holo"));
            let held = cow.pdc.entries().await.unwrap();
            assert_eq!(held.get("plugin:level"), Some(&PdcValue::Int(7)));
            let level = NamespacedKey::new("plugin", "level").unwrap();
            assert_eq!(cow.pdc.get(&level).await.unwrap(), Some(PdcValue::Int(7)));
            let frame = cow.frame.get_state().await.unwrap();
            assert_eq!(frame.facing, "north");
            assert_eq!(frame.rotation, 2);
            let view = objects.map(7).get_view().await.unwrap();
            assert_eq!((view.center_x, view.scale, view.locked), (10, 2, true));
            assert_eq!(view.decorations[0].kind, 4);
            let by_block = objects
                .block_data(BlockPos { x: 1, y: 64, z: 2 })
                .entries()
                .await
                .unwrap();
            assert!(by_block.is_empty());
            assert!(matches!(
                objects.map(9).get_view().await,
                Err(rspigot_plugin::Error::NotFound)
            ));
        })
    })
    .await;
    serve_list(&mut called).await;

    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        asking(object_question::Query::LaunchProjectile(
            v1::LaunchProjectile {
                kind: "minecraft:arrow".to_owned(),
                position: Some(vec3(1.0, 65.0, 2.0)),
                velocity: Some(vec3(0.0, 1.0, 0.0)),
                shooter: Some(COW),
            }
        ))
    );
    called
        .session
        .say(answer(request, answer::Body::Spawned(snapshot(40))));
    let (request, asked) = called.session.asked().await;
    let question::Question::Object(v1::ObjectQuestion {
        query: Some(object_question::Query::LaunchFirework(launch)),
    }) = asked
    else {
        panic!("expected a firework launch");
    };
    assert_eq!(launch.shooter, Some(COW));
    assert!(!launch.angled);
    called
        .session
        .say(answer(request, answer::Body::Spawned(snapshot(41))));
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        asking(object_question::Query::SpawnVehicle(v1::SpawnVehicle {
            kind: "minecraft:oak_boat".to_owned(),
            position: Some(vec3(1.0, 65.0, 2.0)),
            velocity: None,
        }))
    );
    called
        .session
        .say(answer(request, answer::Body::Spawned(snapshot(42))));
    let (request, asked) = called.session.asked().await;
    assert_eq!(
        asked,
        asking(object_question::Query::PlaceHanging(v1::PlaceHanging {
            kind: "minecraft:glow_item_frame".to_owned(),
            pos: Some(v1::BlockPos { x: 5, y: 64, z: 4 }),
            face: "north".to_owned(),
            variant: None,
            item: None,
        }))
    );
    called
        .session
        .say(answer(request, answer::Body::Spawned(snapshot(43))));
    let (request, asked) = called.session.asked().await;
    let question::Question::Object(v1::ObjectQuestion {
        query: Some(object_question::Query::PlaceHanging(painting)),
    }) = asked
    else {
        panic!("expected a painting");
    };
    assert_eq!(painting.kind, "minecraft:painting");
    assert_eq!(painting.variant.as_deref(), Some("minecraft:kebab"));
    called
        .session
        .say(answer(request, answer::Body::Spawned(snapshot(44))));
    let (request, asked) = called.session.asked().await;
    assert!(matches!(
        asked,
        question::Question::Object(v1::ObjectQuestion {
            query: Some(object_question::Query::SpawnDisplay(_))
        })
    ));
    called
        .session
        .say(answer(request, answer::Body::Spawned(snapshot(45))));

    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, asking(object_question::Query::Trades(COW)));
    called.session.say(answer(
        request,
        object_body(object_answer::Answer::Trades(v1::TradeList {
            trades: vec![wire_trade()],
        })),
    ));
    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, asking(object_question::Query::Display(COW)));
    called.session.say(answer(
        request,
        object_body(object_answer::Answer::Display(v1::DisplaySpec {
            props: Some(v1::DisplayProps::default()),
            body: Some(v1::display_spec::Body::Text(v1::TextDisplayData {
                text: Some("holo".to_owned()),
                ..Default::default()
            })),
        })),
    ));
    let held = v1::PdcContainer {
        entries: [("plugin:level".to_owned(), int(7))].into(),
    };
    for _ in 0..2 {
        let (request, asked) = called.session.asked().await;
        assert_eq!(
            asked,
            asking(object_question::Query::Pdc(v1::PdcQuery {
                holder: Some(pdc_query::Holder::Entity(COW)),
            }))
        );
        called.session.say(answer(
            request,
            object_body(object_answer::Answer::Pdc(held.clone())),
        ));
    }
    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, asking(object_question::Query::Hanging(COW)));
    called.session.say(answer(
        request,
        object_body(object_answer::Answer::Hanging(v1::HangingView {
            kind: "minecraft:glow_item_frame".to_owned(),
            pos: Some(v1::BlockPos { x: 5, y: 64, z: 4 }),
            facing: "north".to_owned(),
            item: None,
            rotation: 2,
            visible: true,
            variant: None,
        })),
    ));
    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, asking(object_question::Query::Map(7)));
    called.session.say(answer(
        request,
        object_body(object_answer::Answer::Map(v1::MapView {
            id: 7,
            dimension: "minecraft:overworld".to_owned(),
            center_x: 10,
            center_z: 20,
            scale: 2,
            locked: true,
            tracking_position: true,
            unlimited_tracking: false,
            colors: vec![0; 4],
            decorations: vec![v1::MapDecorationInfo {
                kind: 4,
                x: 1,
                y: 2,
                rotation: 3,
                name: None,
            }],
        })),
    ));
    let (request, _) = called.session.asked().await;
    called.session.say(answer(
        request,
        object_body(object_answer::Answer::Pdc(v1::PdcContainer::default())),
    ));
    let (request, asked) = called.session.asked().await;
    assert_eq!(asked, asking(object_question::Query::Map(9)));
    called
        .session
        .say(answer(request, answer::Body::NotFound(v1::NotFound {})));
    called.finished().await;
}
