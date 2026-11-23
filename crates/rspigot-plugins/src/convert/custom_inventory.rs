use rspigot_core::plugins::{
    CustomInventoryOrder, CustomInventoryState, InventorySpec, InventoryType, MerchantProgress,
    Order, ServerOrder,
};
use rspigot_plugin_protocol::v1::{self, custom_inventory_order::Order as Wire};

use super::player::wire_player;
use super::shared::{self, required};
use super::{Malformed, contract};

pub(super) fn inventory_type(value: i32) -> Result<InventoryType, Malformed> {
    use v1::InventoryKind as Kind;
    match Kind::try_from(value) {
        Ok(Kind::Unspecified | Kind::Chest) => Ok(InventoryType::Chest),
        Ok(Kind::Dispenser) => Ok(InventoryType::Dispenser),
        Ok(Kind::Dropper) => Ok(InventoryType::Dropper),
        Ok(Kind::Furnace) => Ok(InventoryType::Furnace),
        Ok(Kind::BlastFurnace) => Ok(InventoryType::BlastFurnace),
        Ok(Kind::Smoker) => Ok(InventoryType::Smoker),
        Ok(Kind::Workbench) => Ok(InventoryType::Workbench),
        Ok(Kind::Enchanting) => Ok(InventoryType::Enchanting),
        Ok(Kind::Brewing) => Ok(InventoryType::Brewing),
        Ok(Kind::Anvil) => Ok(InventoryType::Anvil),
        Ok(Kind::Beacon) => Ok(InventoryType::Beacon),
        Ok(Kind::Hopper) => Ok(InventoryType::Hopper),
        Ok(Kind::ShulkerBox) => Ok(InventoryType::ShulkerBox),
        Ok(Kind::Barrel) => Ok(InventoryType::Barrel),
        Ok(Kind::EnderChest) => Ok(InventoryType::EnderChest),
        Ok(Kind::Grindstone) => Ok(InventoryType::Grindstone),
        Ok(Kind::Loom) => Ok(InventoryType::Loom),
        Ok(Kind::Cartography) => Ok(InventoryType::Cartography),
        Ok(Kind::Smithing) => Ok(InventoryType::Smithing),
        Ok(Kind::Stonecutter) => Ok(InventoryType::Stonecutter),
        Ok(Kind::Crafter) => Ok(InventoryType::Crafter),
        Ok(Kind::Merchant) => Ok(InventoryType::Merchant),
        Err(_) => Err(Malformed::Enum {
            field: "inventory kind",
            value,
        }),
    }
}

const fn wire_inventory_type(kind: InventoryType) -> v1::InventoryKind {
    use v1::InventoryKind as Kind;
    match kind {
        InventoryType::Chest => Kind::Chest,
        InventoryType::Dispenser => Kind::Dispenser,
        InventoryType::Dropper => Kind::Dropper,
        InventoryType::Furnace => Kind::Furnace,
        InventoryType::BlastFurnace => Kind::BlastFurnace,
        InventoryType::Smoker => Kind::Smoker,
        InventoryType::Workbench => Kind::Workbench,
        InventoryType::Enchanting => Kind::Enchanting,
        InventoryType::Brewing => Kind::Brewing,
        InventoryType::Anvil => Kind::Anvil,
        InventoryType::Beacon => Kind::Beacon,
        InventoryType::Hopper => Kind::Hopper,
        InventoryType::ShulkerBox => Kind::ShulkerBox,
        InventoryType::Barrel => Kind::Barrel,
        InventoryType::EnderChest => Kind::EnderChest,
        InventoryType::Grindstone => Kind::Grindstone,
        InventoryType::Loom => Kind::Loom,
        InventoryType::Cartography => Kind::Cartography,
        InventoryType::Smithing => Kind::Smithing,
        InventoryType::Stonecutter => Kind::Stonecutter,
        InventoryType::Crafter => Kind::Crafter,
        InventoryType::Merchant => Kind::Merchant,
    }
}

const fn progress(value: v1::MerchantProgress) -> MerchantProgress {
    MerchantProgress {
        level: value.level,
        xp: value.xp,
        show_progress: value.show_progress,
        can_restock: value.can_restock,
    }
}

const fn wire_progress(value: MerchantProgress) -> v1::MerchantProgress {
    v1::MerchantProgress {
        level: value.level,
        xp: value.xp,
        show_progress: value.show_progress,
        can_restock: value.can_restock,
    }
}

pub(super) fn create(create: v1::CreateInventory) -> Result<ServerOrder, Malformed> {
    let ty = inventory_type(create.kind)?;
    if ty == InventoryType::Chest && create.slots == 0 && create.trades.is_empty() {
        return Ok(ServerOrder::CreateInventory {
            id: create.id,
            rows: create.rows,
            title: create.title,
            locked: create.locked,
        });
    }
    let slots = if ty == InventoryType::Chest && create.slots == 0 {
        create.rows.saturating_mul(9)
    } else {
        create.slots
    };
    Ok(ServerOrder::CreateTypedInventory {
        id: create.id,
        spec: InventorySpec {
            progress: create.progress.map(progress),
            ty,
            slots,
            title: create.title,
            locked: create.locked,
            trades: create
                .trades
                .into_iter()
                .map(contract::trade)
                .collect::<Result<_, _>>()?,
        },
    })
}

pub(super) fn order(order: v1::CustomInventoryOrder) -> Result<Order, Malformed> {
    let inventory = order.inventory;
    let order = match required(order.order)? {
        Wire::Set(v1::SetSlot { slot, item }) => CustomInventoryOrder::Set {
            slot,
            item: item.map(shared::item),
        },
        Wire::Clear(_) => CustomInventoryOrder::Clear,
        Wire::Close(_) => CustomInventoryOrder::Close,
        Wire::Discard(_) => CustomInventoryOrder::Discard,
        Wire::SetProgress(value) => CustomInventoryOrder::SetProgress(progress(value)),
        Wire::SetTrades(list) => CustomInventoryOrder::SetTrades(
            list.trades
                .into_iter()
                .map(contract::trade)
                .collect::<Result<_, _>>()?,
        ),
    };
    Ok(Order::CustomInventory { inventory, order })
}

pub(super) fn wire_state(state: CustomInventoryState) -> v1::CustomInventoryState {
    v1::CustomInventoryState {
        slots: (0..)
            .zip(state.items)
            .map(|(slot, item)| v1::Slot {
                slot,
                item: item.map(shared::wire_item),
            })
            .collect(),
        viewers: state.viewers.into_iter().map(wire_player).collect(),
        kind: wire_inventory_type(state.inventory_type) as i32,
        trades: state.trades.into_iter().map(contract::wire_trade).collect(),
        progress: state.progress.map(wire_progress),
    }
}

#[cfg(test)]
mod tests {
    use rspigot_core::plugins::events::{
        ClickKind, EventResult, InventoryAction, InventoryClick, InventoryClose,
        InventoryTypeSlotType,
    };
    use rspigot_core::plugins::{
        Answer, CustomInventoryOrder, CustomInventoryState, EventKind, EventPayload, FromPlugin,
        InventoryOwner, InventoryRef, InventoryView, ItemSpec, Order, Phase, PlayerOrder,
        PlayerSnapshot, Question, ServerOrder, ToPlugin,
    };
    use rspigot_domain::player::{GameMode, PlayerId};
    use rspigot_domain::{Rotation, Vec3};
    use rspigot_plugin_protocol::v1::{
        self, answer, custom_inventory_order, event, inventory_ref, player_order, server_order,
    };
    use uuid::Uuid;

    use crate::convert::player::wire_player;
    use crate::convert::{from_plugin, intro, question, to_plugin};

    const STEVE: &str = "00000000-0000-0000-0000-000000000001";

    fn steve() -> PlayerSnapshot {
        PlayerSnapshot {
            meta: Default::default(),
            permissions: Vec::new(),
            id: PlayerId(Uuid::from_u128(1)),
            name: "Steve".to_owned(),
            entity_id: 12,
            world: "minecraft:overworld".to_owned(),
            position: Vec3::new(1.0, 64.0, -3.5),
            rotation: Rotation::new(90.0, -30.0),
            game_mode: GameMode::Survival,
        }
    }

    fn convert(body: from_plugin::Body) -> FromPlugin {
        from_plugin(
            v1::FromPlugin {
                body: Some(body),
                origin: None,
                request: None,
                in_world: None,
            },
            &mut Vec::new(),
        )
        .expect("the message converts")
    }

    fn payload(payload: EventPayload) -> event::Payload {
        let Some(to_plugin::Body::Event(v1::Event {
            payload: Some(payload),
            ..
        })) = to_plugin(ToPlugin::Event {
            blocks: Vec::new(),
            id: 4,
            handlers: vec![1],
            phase: Phase::Settled,
            cancelled: false,
            payload,
        })
        .body
        else {
            panic!("expected an event");
        };
        payload
    }

    fn wire_diamond() -> v1::Item {
        v1::Item {
            key: "minecraft:diamond".into(),
            count: 1,
            ..v1::Item::default()
        }
    }

    #[test]
    fn inventory_events_convert_to_the_wire() {
        let menu = InventoryRef {
            owner: InventoryOwner::Custom {
                plugin: "shop".to_owned(),
                id: 7,
            },
            size: 9,
            kind: "CHEST".to_owned(),
            id: 7,
        };
        let own = InventoryRef {
            owner: InventoryOwner::Player(steve().id),
            size: 41,
            kind: "PLAYER".to_owned(),
            id: 3,
        };
        let view = InventoryView {
            top: menu.clone(),
            bottom: own.clone(),
            title: "Menu".to_owned(),
            player: steve(),
        };
        let clicked = payload(EventPayload::InventoryClick(InventoryClick {
            action: InventoryAction::Unknown,
            click: ClickKind::NumberKey,
            clicked_inventory: Some(own),
            current_item: Some(ItemSpec::of("minecraft:diamond", 1)),
            cursor: None,
            hotbar_button: 3,
            inventory: menu,
            left_click: false,
            raw_slot: 39,
            result: EventResult::Default,
            right_click: false,
            shift_click: false,
            slot: 30,
            slot_type: InventoryTypeSlotType::Container,
            view: view.clone(),
            viewers: vec![steve()],
            who_clicked: steve(),
        }));
        let wire_menu = v1::InventoryRef {
            owner: Some(inventory_ref::Owner::Custom(7)),
            size: 9,
            kind: "CHEST".into(),
            plugin: "shop".into(),
            id: 7,
        };
        let wire_own = v1::InventoryRef {
            owner: Some(inventory_ref::Owner::Player(STEVE.into())),
            size: 41,
            kind: "PLAYER".into(),
            plugin: String::new(),
            id: 3,
        };
        let wire_view = v1::InventoryView {
            top: Some(wire_menu.clone()),
            bottom: Some(wire_own.clone()),
            title: "Menu".into(),
            player: Some(wire_player(steve())),
        };
        assert_eq!(
            clicked,
            event::Payload::InventoryClick(v1::InventoryClick {
                action: v1::InventoryAction::Unknown as i32,
                click: v1::ClickKind::NumberKey as i32,
                clicked_inventory: Some(wire_own),
                current_item: Some(wire_diamond()),
                cursor: None,
                hotbar_button: 3,
                inventory: Some(wire_menu.clone()),
                left_click: false,
                raw_slot: 39,
                result: v1::EventResult::Default as i32,
                right_click: false,
                shift_click: false,
                slot: 30,
                slot_type: v1::InventoryTypeSlotType::Container as i32,
                view: Some(wire_view.clone()),
                viewers: vec![wire_player(steve())],
                who_clicked: Some(wire_player(steve())),
            })
        );

        assert_eq!(
            payload(EventPayload::InventoryClose(InventoryClose {
                by_player: true,
                inventory: view.top.clone(),
                player: steve(),
                view,
                viewers: Vec::new(),
            })),
            event::Payload::InventoryClose(v1::InventoryClose {
                by_player: true,
                inventory: Some(wire_menu),
                player: Some(wire_player(steve())),
                view: Some(wire_view),
                viewers: Vec::new(),
            })
        );
    }

    #[test]
    fn typed_inventories_and_trades_convert() {
        use rspigot_core::plugins::{InventorySpec, InventoryType, SchedulerOrder, Trade};

        let create = |create: v1::CreateInventory| {
            convert(from_plugin::Body::Server(v1::ServerOrder {
                order: Some(server_order::Order::CreateInventory(create)),
            }))
        };
        assert_eq!(
            create(v1::CreateInventory {
                id: 1,
                kind: v1::InventoryKind::Anvil as i32,
                title: "Forge".into(),
                ..Default::default()
            }),
            FromPlugin::Order(Order::Server(ServerOrder::CreateTypedInventory {
                id: 1,
                spec: InventorySpec {
                    progress: None,
                    ty: InventoryType::Anvil,
                    slots: 0,
                    title: "Forge".into(),
                    locked: false,
                    trades: Vec::new(),
                },
            }))
        );
        assert_eq!(
            create(v1::CreateInventory {
                id: 2,
                rows: 3,
                kind: v1::InventoryKind::Chest as i32,
                trades: vec![v1::Trade {
                    result: Some(wire_diamond()),
                    ingredients: vec![wire_diamond()],
                    max_uses: 2,
                    ..Default::default()
                }],
                ..Default::default()
            }),
            FromPlugin::Order(Order::Server(ServerOrder::CreateTypedInventory {
                id: 2,
                spec: InventorySpec {
                    progress: None,
                    ty: InventoryType::Chest,
                    slots: 27,
                    title: String::new(),
                    locked: false,
                    trades: vec![Trade {
                        result: ItemSpec::of("minecraft:diamond", 1),
                        ingredients: vec![ItemSpec::of("minecraft:diamond", 1)],
                        uses: 0,
                        max_uses: 2,
                        experience_reward: false,
                        villager_experience: 0,
                        price_multiplier: 0.0,
                        demand: 0,
                        special_price: 0,
                    }],
                },
            }))
        );
        assert_eq!(
            create(v1::CreateInventory {
                id: 3,
                rows: 2,
                ..Default::default()
            }),
            FromPlugin::Order(Order::Server(ServerOrder::CreateInventory {
                id: 3,
                rows: 2,
                title: String::new(),
                locked: false,
            }))
        );
        assert!(
            from_plugin(
                v1::FromPlugin {
                    body: Some(from_plugin::Body::Server(v1::ServerOrder {
                        order: Some(server_order::Order::CreateInventory(v1::CreateInventory {
                            kind: 99,
                            ..Default::default()
                        })),
                    })),
                    origin: None,
                    request: None,
                    in_world: None,
                },
                &mut Vec::new()
            )
            .is_err()
        );
        assert_eq!(
            convert(from_plugin::Body::Server(v1::ServerOrder {
                order: Some(server_order::Order::ScheduleTask(v1::ScheduleTask {
                    id: 5,
                    delay: 1,
                    period: 2
                })),
            })),
            FromPlugin::Order(Order::Server(ServerOrder::Schedule(
                SchedulerOrder::Schedule {
                    id: 5,
                    delay: 1,
                    period: 2
                }
            )))
        );
    }

    #[test]
    fn custom_inventory_calls_convert() {
        assert_eq!(
            convert(from_plugin::Body::Server(v1::ServerOrder {
                order: Some(server_order::Order::CreateInventory(v1::CreateInventory {
                    id: 3,
                    rows: 2,
                    title: "§6Menu".into(),
                    locked: true,
                    ..Default::default()
                })),
            })),
            FromPlugin::Order(Order::Server(ServerOrder::CreateInventory {
                id: 3,
                rows: 2,
                title: "§6Menu".into(),
                locked: true,
            }))
        );
        assert_eq!(
            convert(from_plugin::Body::Player(v1::PlayerOrder {
                player: STEVE.into(),
                order: Some(player_order::Order::OpenInventory(3)),
            })),
            FromPlugin::Order(Order::Player {
                player: PlayerId(Uuid::from_u128(1)),
                order: PlayerOrder::OpenInventory(3),
            })
        );
        let custom = |order| {
            convert(from_plugin::Body::CustomInventory(
                v1::CustomInventoryOrder {
                    inventory: 3,
                    order: Some(order),
                },
            ))
        };
        let on_three = |order| {
            FromPlugin::Order(Order::CustomInventory {
                inventory: 3,
                order,
            })
        };
        assert_eq!(
            custom(custom_inventory_order::Order::Set(v1::SetSlot {
                slot: 4,
                item: Some(wire_diamond()),
            })),
            on_three(CustomInventoryOrder::Set {
                slot: 4,
                item: Some(ItemSpec::of("minecraft:diamond", 1)),
            })
        );
        assert_eq!(
            custom(custom_inventory_order::Order::Set(v1::SetSlot {
                slot: 4,
                item: None,
            })),
            on_three(CustomInventoryOrder::Set {
                slot: 4,
                item: None
            })
        );
        assert_eq!(
            custom(custom_inventory_order::Order::Clear(v1::Empty {})),
            on_three(CustomInventoryOrder::Clear)
        );
        assert_eq!(
            custom(custom_inventory_order::Order::Close(v1::Empty {})),
            on_three(CustomInventoryOrder::Close)
        );
        assert_eq!(
            custom(custom_inventory_order::Order::Discard(v1::Empty {})),
            on_three(CustomInventoryOrder::Discard)
        );
        assert!(
            from_plugin(
                v1::FromPlugin {
                    body: Some(from_plugin::Body::CustomInventory(
                        v1::CustomInventoryOrder {
                            inventory: 3,
                            order: None,
                        }
                    )),
                    origin: None,
                    request: None,
                    in_world: None,
                },
                &mut Vec::new()
            )
            .is_err()
        );

        assert_eq!(
            convert(from_plugin::Body::Question(v1::Question {
                request: 2,
                question: Some(question::Question::CustomInventory(3)),
            })),
            FromPlugin::Question {
                request: 2,
                question: Question::CustomInventory(3),
            }
        );
        let Some(to_plugin::Body::Answer(v1::Answer {
            request: 2,
            body: Some(body),
        })) = to_plugin(ToPlugin::Answer {
            request: 2,
            answer: Answer::CustomInventory(CustomInventoryState {
                items: vec![None, Some(ItemSpec::of("minecraft:diamond", 1))],
                viewers: vec![steve()],
                inventory_type: rspigot_core::plugins::InventoryType::Merchant,
                trades: Vec::new(),
                progress: None,
            }),
        })
        .body
        else {
            panic!("expected an answer");
        };
        assert_eq!(
            body,
            answer::Body::CustomInventory(v1::CustomInventoryState {
                slots: vec![
                    v1::Slot {
                        slot: 0,
                        item: None
                    },
                    v1::Slot {
                        slot: 1,
                        item: Some(wire_diamond())
                    },
                ],
                viewers: vec![wire_player(steve())],
                kind: v1::InventoryKind::Merchant as i32,
                trades: Vec::new(),
                progress: None,
            })
        );
    }

    #[test]
    fn inventory_handlers_are_accepted() {
        let handler = |id, event: v1::EventKind| v1::Handler {
            id,
            event: event as i32,
            access: v1::Access::Observe as i32,
            priority: v1::Priority::Normal as i32,
            ignore_cancelled: false,
        };
        let introduced = intro(v1::Hello {
            protocol_version: 2,
            name: "menus".into(),
            version: "1.0.0".into(),
            token: String::new(),
            commands: Vec::new(),
            depends: Vec::new(),
            soft_depends: Vec::new(),
            load_before: Vec::new(),
            handlers: vec![
                handler(1, v1::EventKind::InventoryClick),
                handler(2, v1::EventKind::InventoryClose),
            ],
        })
        .expect("the introduction converts");
        let events: Vec<EventKind> = introduced
            .handlers
            .iter()
            .map(|handler| handler.event)
            .collect();
        assert_eq!(
            events,
            [EventKind::InventoryClick, EventKind::InventoryClose]
        );
    }
}
