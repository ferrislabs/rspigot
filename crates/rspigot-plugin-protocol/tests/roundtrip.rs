use prost::Message;
use rspigot_plugin_protocol::PROTOCOL_VERSION;
use rspigot_plugin_protocol::v1::{
    Access, AsyncPlayerChatPatch, BlockPos, BlockPosListDiff, BlockPosReplacement, BlockSnapshot,
    BlockState, BookMeta, BoundingBox, ChunkRef, ClickKind, CommandSender, CommandSenderListDiff,
    CommandSenderReplacement, DamageSource, Decision, DroppedItem, DroppedItemListDiff,
    DroppedItemReplacement, Effect, Empty, Enchant, EnchantOffer, EntityDamageByEntity,
    EntityDamageCause, EntitySnapshot, EntityTargetPatch, Event, EventKind, EventResult,
    ExplosionResult, FromPlugin, GameMode, Handler, Hello, HolderRef, Input, InventoryAction,
    InventoryClick, InventoryRef, InventoryTypeSlotType, InventoryView, Item, ItemListDiff,
    ItemReplacement, LecternData, Location, LootContext, MerchantRef, NullableBlockPos,
    NullableEntitySnapshot, NullableInventoryRef, NullableItem, NullableLocation, NullableString,
    OwnerRef, Patch, Phase, PlayerDeath, PlayerDeathPatch, PlayerMove, PlayerMovePatch,
    PlayerSnapshot, PlayerSnapshotListDiff, PlayerSnapshotReplacement, PluginInfo, Priority,
    Property, Rotation, ServerListPingPatch, SignData, SpawnerData, StringListDiff,
    StringReplacement, ToPlugin, Trade, TrialSpawnerData, Vec3, Verdict, command_sender, event,
    from_plugin, holder_ref, inventory_ref, merchant_ref, owner_ref, patch, to_plugin,
};

fn steve() -> PlayerSnapshot {
    PlayerSnapshot {
        permissions: Vec::new(),
        uuid: "00000000-0000-0000-0000-000000000001".to_owned(),
        name: "Steve".to_owned(),
        entity_id: 12,
        world: "world".to_owned(),
        position: Some(Vec3 {
            x: 1.0,
            y: 64.0,
            z: -3.5,
        }),
        game_mode: GameMode::Survival as i32,
        rotation: Some(Rotation {
            yaw: 90.0,
            pitch: -30.0,
        }),
        ..Default::default()
    }
}

fn zombie() -> EntitySnapshot {
    EntitySnapshot {
        entity_id: 40,
        uuid: "00000000-0000-0000-0000-000000000028".to_owned(),
        kind: "minecraft:zombie".to_owned(),
        position: Some(Vec3 {
            x: 2.0,
            y: 64.0,
            z: -3.5,
        }),
        player: None,
        ..Default::default()
    }
}

fn at(x: f64) -> Location {
    Location {
        world: "minecraft:overworld".to_owned(),
        position: Some(Vec3 {
            x,
            y: 64.0,
            z: -3.5,
        }),
        rotation: Some(Rotation {
            yaw: 0.0,
            pitch: 0.0,
        }),
    }
}

fn diamond(count: i32) -> Item {
    Item {
        key: "minecraft:diamond".to_owned(),
        count,
        name: Some("Shiny".to_owned()),
        ..Item::default()
    }
}

fn custom(id: u64) -> InventoryRef {
    InventoryRef {
        owner: Some(inventory_ref::Owner::Custom(id)),
        size: 27,
        kind: "CHEST".to_owned(),
        plugin: "shop".to_owned(),
        id: 0,
    }
}

fn event(payload: event::Payload) -> ToPlugin {
    ToPlugin {
        body: Some(to_plugin::Body::Event(Event {
            id: 7,
            handlers: vec![1, 2],
            phase: Phase::Decide as i32,
            cancelled: false,
            payload: Some(payload),
            ..Default::default()
        })),
    }
}

fn survives(message: &ToPlugin) {
    let decoded = ToPlugin::decode(message.encode_to_vec().as_slice()).unwrap();
    assert_eq!(&decoded, message);
}

fn round_trips<M: Message + Default + PartialEq + std::fmt::Debug>(message: &M) {
    let encoded = message.encode_to_vec();
    assert!(!encoded.is_empty(), "{message:?} encodes to nothing");
    assert_eq!(&M::decode(encoded.as_slice()).unwrap(), message);
}

#[test]
fn a_hello_and_an_event_survive_encoding() {
    let hello = FromPlugin {
        body: Some(from_plugin::Body::Hello(Hello {
            protocol_version: PROTOCOL_VERSION,
            name: "welcome".to_owned(),
            version: "0.1.0".to_owned(),
            token: String::new(),
            commands: Vec::new(),
            handlers: vec![Handler {
                id: 1,
                event: EventKind::EntityDamageByEntity as i32,
                access: Access::Decide as i32,
                priority: Priority::Normal as i32,
                ignore_cancelled: false,
            }],
        })),
        origin: None,
        request: None,
        in_world: None,
    };
    let decoded = FromPlugin::decode(hello.encode_to_vec().as_slice()).unwrap();
    assert_eq!(decoded, hello);

    survives(&event(event::Payload::PlayerMove(PlayerMove {
        player: Some(steve()),
        from: Some(at(1.0)),
        to: Some(at(2.0)),
    })));
}

#[test]
fn every_kind_of_field_survives_encoding() {
    let block = BlockPos { x: 3, y: 64, z: -2 };
    let samples = [
        event::Payload::InventoryClick(InventoryClick {
            action: InventoryAction::PickupAll as i32,
            click: ClickKind::Creative as i32,
            clicked_inventory: Some(InventoryRef {
                owner: Some(inventory_ref::Owner::Player(steve().uuid)),
                size: 41,
                kind: "PLAYER".to_owned(),
                plugin: String::new(),
                id: 0,
            }),
            current_item: Some(diamond(3)),
            cursor: None,
            hotbar_button: -1,
            inventory: Some(custom(5)),
            left_click: true,
            raw_slot: 30,
            result: EventResult::Deny as i32,
            right_click: false,
            shift_click: false,
            slot: 3,
            slot_type: InventoryTypeSlotType::Quickbar as i32,
            view: Some(InventoryView {
                top: Some(custom(5)),
                bottom: Some(InventoryRef {
                    owner: Some(inventory_ref::Owner::Block(block)),
                    size: 9,
                    kind: "DISPENSER".to_owned(),
                    plugin: String::new(),
                    id: 0,
                }),
                title: "§6Menu".to_owned(),
                player: Some(steve()),
            }),
            viewers: vec![steve(), steve()],
            who_clicked: Some(steve()),
        }),
        event::Payload::EntityDamageByEntity(EntityDamageByEntity {
            cause: EntityDamageCause::EntityAttack as i32,
            damage: 4.5,
            damage_source: Some(DamageSource {
                kind: "minecraft:player_attack".to_owned(),
                causing: Some(zombie()),
                direct: Some(zombie()),
                location: None,
            }),
            damager: Some(zombie()),
            entity: Some(zombie()),
            entity_type: "minecraft:zombie".to_owned(),
            final_damage: 3.0,
        }),
        event::Payload::BlockExplode(rspigot_plugin_protocol::v1::BlockExplode {
            block: Some(block),
            block_list: vec![block, BlockPos { x: 4, y: 64, z: -2 }],
            exploded_block_state: Some(BlockSnapshot {
                pos: Some(block),
                state: Some(BlockState {
                    block: "minecraft:tnt".to_owned(),
                    properties: vec![Property {
                        name: "unstable".to_owned(),
                        value: "false".to_owned(),
                    }],
                }),
            }),
            explosion_result: ExplosionResult::DestroyWithDecay as i32,
            explosion_yield: 0.25,
        }),
        event::Payload::PlayerDeath(PlayerDeath {
            damage_source: None,
            death_message: Some("Steve fell".to_owned()),
            dropped_exp: 7,
            drops: vec![diamond(1), diamond(64)],
            entity: Some(steve()),
            entity_type: "minecraft:player".to_owned(),
            keep_inventory: true,
            keep_level: false,
            new_exp: 0,
            new_level: 3,
            new_total_exp: 30,
        }),
    ];
    for payload in samples {
        survives(&event(payload));
    }
}

#[test]
fn the_protocol_is_version_five() {
    assert_eq!(PROTOCOL_VERSION, 5);
}

#[test]
fn a_decision_carries_one_verdict_per_handler_with_its_patch() {
    let decision = FromPlugin {
        body: Some(from_plugin::Body::Decision(Decision {
            event: 7,
            verdicts: vec![
                Verdict {
                    handler: 2,
                    cancel: false,
                    patch: Some(Patch {
                        change: Some(patch::Change::PlayerMove(PlayerMovePatch {
                            to: Some(at(4.5)),
                        })),
                    }),
                },
                Verdict {
                    handler: 4,
                    cancel: true,
                    patch: None,
                },
                Verdict {
                    handler: 5,
                    cancel: false,
                    patch: Some(Patch {
                        change: Some(patch::Change::PlayerDeath(PlayerDeathPatch {
                            death_message: Some(NullableString { value: None }),
                            drops: Some(ItemListDiff {
                                removed: vec![0, 2],
                                replaced: Vec::new(),
                                added: vec![diamond(3)],
                            }),
                            keep_inventory: Some(true),
                            ..PlayerDeathPatch::default()
                        })),
                    }),
                },
                Verdict {
                    handler: 6,
                    cancel: false,
                    patch: Some(Patch {
                        change: Some(patch::Change::EntityTarget(EntityTargetPatch {
                            target: Some(NullableEntitySnapshot {
                                value: Some(zombie()),
                            }),
                        })),
                    }),
                },
            ],
        })),
        origin: None,
        request: None,
        in_world: None,
    };
    let decoded = FromPlugin::decode(decision.encode_to_vec().as_slice()).unwrap();
    assert_eq!(decoded, decision);
}

#[test]
fn a_patch_shares_its_field_number_with_the_event_payload() {
    let moved = Event {
        payload: Some(event::Payload::PlayerMove(PlayerMove::default())),
        ..Event::default()
    };
    let patched = Patch {
        change: Some(patch::Change::PlayerMove(PlayerMovePatch::default())),
    };
    assert_eq!(moved.encode_to_vec()[0] >> 3, 12);
    assert_eq!(patched.encode_to_vec()[0] >> 3, 12);
    assert_eq!(EventKind::PlayerJoin as i32, 1);
    assert_eq!(EventKind::InventoryClose as i32, 5);
}

#[test]
fn a_list_diff_never_reads_the_number_of_the_list_it_replaced() {
    let diff = AsyncPlayerChatPatch {
        recipients: Some(PlayerSnapshotListDiff {
            removed: vec![1],
            replaced: Vec::new(),
            added: vec![steve()],
        }),
        ..AsyncPlayerChatPatch::default()
    };
    assert_eq!(diff.encode_to_vec()[..2], [0xe2, 0x3e]);
    let mut old_list = Vec::new();
    prost::encoding::message::encode(4, &steve(), &mut old_list);
    let read = AsyncPlayerChatPatch::decode(old_list.as_slice()).unwrap();
    assert_eq!(read, AsyncPlayerChatPatch::default());
}

#[test]
fn every_contract_type_survives_encoding() {
    let block = BlockPos { x: 3, y: 64, z: -2 };
    let state = BlockState {
        block: "minecraft:oak_sign".to_owned(),
        properties: vec![Property {
            name: "rotation".to_owned(),
            value: "4".to_owned(),
        }],
    };
    let mut book = diamond(1);
    book.lore = vec!["Old".to_owned(), "Dusty".to_owned()];
    book.enchantments = vec![Enchant {
        key: "minecraft:unbreaking".to_owned(),
        level: 3,
    }];
    book.components = vec![0x0a, 0x00, 0xff, 0x7f];
    round_trips(&book);
    round_trips(&at(1.5));
    round_trips(&BlockSnapshot {
        pos: Some(block),
        state: Some(state.clone()),
    });
    for owner in [
        inventory_ref::Owner::Custom(5),
        inventory_ref::Owner::Player(steve().uuid),
        inventory_ref::Owner::Block(block),
        inventory_ref::Owner::Entity(40),
        inventory_ref::Owner::Detached(Empty {}),
    ] {
        round_trips(&InventoryRef {
            owner: Some(owner),
            size: 27,
            kind: "CHEST".to_owned(),
            plugin: "shop".to_owned(),
            id: 0,
        });
    }
    round_trips(&InventoryView {
        top: Some(custom(5)),
        bottom: Some(custom(6)),
        title: "§6Menu".to_owned(),
        player: Some(steve()),
    });
    round_trips(&ChunkRef {
        world: "minecraft:overworld".to_owned(),
        x: -3,
        z: 7,
    });
    round_trips(&DamageSource {
        kind: "minecraft:arrow".to_owned(),
        causing: Some(zombie()),
        direct: Some(zombie()),
        location: Some(Vec3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        }),
    });
    for sender in [
        command_sender::Sender::Player(steve()),
        command_sender::Sender::Console(Empty {}),
    ] {
        round_trips(&CommandSender {
            sender: Some(sender),
        });
    }
    round_trips(&Trade {
        result: Some(diamond(2)),
        ingredients: vec![diamond(9), diamond(1)],
        uses: 1,
        max_uses: 12,
        experience_reward: true,
        villager_experience: 5,
        price_multiplier: 0.05,
        demand: -2,
        special_price: 3,
    });
    round_trips(&PluginInfo {
        name: "guard".to_owned(),
        version: "1.2.3".to_owned(),
    });
    round_trips(&BoundingBox {
        min: Some(Vec3 {
            x: -0.5,
            y: 0.0,
            z: -0.5,
        }),
        max: Some(Vec3 {
            x: 0.5,
            y: 1.8,
            z: 0.5,
        }),
    });
    round_trips(&Input {
        forward: true,
        backward: true,
        left: true,
        right: true,
        jump: true,
        sneak: true,
        sprint: true,
    });
    round_trips(&SpawnerData {
        pos: Some(block),
        spawned: Some("minecraft:zombie".to_owned()),
        delay: 200,
    });
    round_trips(&TrialSpawnerData {
        pos: Some(block),
        ominous: true,
    });
    round_trips(&LecternData {
        pos: Some(block),
        book: Some(book),
        page: 2,
    });
    round_trips(&SignData {
        pos: Some(block),
        front: vec!["Hello".to_owned(), String::new()],
        back: vec!["Bye".to_owned()],
        waxed: true,
    });
    round_trips(&EnchantOffer {
        enchantment: "minecraft:sharpness".to_owned(),
        level: 2,
        cost: 14,
    });
    for owner in [
        owner_ref::Owner::Player(steve()),
        owner_ref::Owner::Entity(zombie()),
        owner_ref::Owner::Offline(steve().uuid),
    ] {
        round_trips(&OwnerRef { owner: Some(owner) });
    }
    for holder in [
        holder_ref::Holder::Player(steve()),
        holder_ref::Holder::Entity(zombie()),
        holder_ref::Holder::Block(block),
    ] {
        round_trips(&HolderRef {
            holder: Some(holder),
        });
    }
    for merchant in [
        merchant_ref::Merchant::Entity(zombie()),
        merchant_ref::Merchant::Custom(Empty {}),
    ] {
        round_trips(&MerchantRef {
            merchant: Some(merchant),
        });
    }
    round_trips(&BookMeta {
        title: Some("Notes".to_owned()),
        author: Some("Steve".to_owned()),
        pages: vec!["one".to_owned(), "two".to_owned()],
    });
    round_trips(&LootContext {
        location: Some(at(4.0)),
        luck: 1.5,
        killer: Some(zombie()),
        looted: Some(zombie()),
    });
    round_trips(&Effect {
        key: "minecraft:speed".to_owned(),
        duration: 600,
        amplifier: 1,
        ambient: true,
        particles: true,
        icon: true,
    });
}

#[test]
fn every_wrapper_of_a_patch_survives_encoding() {
    let block = BlockPos { x: 3, y: 64, z: -2 };
    round_trips(&NullableBlockPos { value: Some(block) });
    round_trips(&NullableEntitySnapshot {
        value: Some(zombie()),
    });
    round_trips(&NullableInventoryRef {
        value: Some(custom(5)),
    });
    round_trips(&NullableItem {
        value: Some(diamond(1)),
    });
    round_trips(&NullableLocation {
        value: Some(at(1.0)),
    });
    round_trips(&NullableString {
        value: Some(String::new()),
    });
    round_trips(&BlockPosListDiff {
        removed: vec![0, 7],
        replaced: vec![BlockPosReplacement {
            index: 2,
            value: Some(block),
        }],
        added: vec![block, block],
    });
    round_trips(&CommandSenderListDiff {
        removed: vec![1],
        replaced: vec![CommandSenderReplacement {
            index: 0,
            value: Some(CommandSender {
                sender: Some(command_sender::Sender::Console(Empty {})),
            }),
        }],
        added: vec![CommandSender {
            sender: Some(command_sender::Sender::Console(Empty {})),
        }],
    });
    round_trips(&DroppedItemListDiff {
        removed: vec![0],
        replaced: vec![DroppedItemReplacement {
            index: 1,
            value: Some(DroppedItem {
                entity: Some(zombie()),
                item: Some(diamond(2)),
            }),
        }],
        added: vec![DroppedItem {
            entity: Some(zombie()),
            item: Some(Item {
                key: "minecraft:dirt".to_owned(),
                count: 1,
                ..Item::default()
            }),
        }],
    });
    round_trips(&ItemListDiff {
        removed: vec![u32::MAX],
        replaced: vec![ItemReplacement {
            index: 3,
            value: Some(diamond(9)),
        }],
        added: vec![diamond(1), diamond(64)],
    });
    round_trips(&PlayerSnapshotListDiff {
        removed: vec![2, 0],
        replaced: vec![PlayerSnapshotReplacement {
            index: 1,
            value: Some(steve()),
        }],
        added: vec![steve()],
    });
    round_trips(&StringListDiff {
        removed: Vec::new(),
        replaced: Vec::new(),
        added: vec!["a".to_owned(), String::new()],
    });
    round_trips(&StringListDiff {
        removed: vec![3],
        replaced: vec![StringReplacement {
            index: 0,
            value: String::new(),
        }],
        added: Vec::new(),
    });
    let pinged = Patch {
        change: Some(patch::Change::ServerListPing(ServerListPingPatch {
            max_players: Some(0),
            motd: Some(String::new()),
            server_icon: Some(vec![0x89, 0x50, 0x4e, 0x47]),
            hidden_players: Some(StringListDiff {
                removed: vec![1],
                replaced: Vec::new(),
                added: vec![steve().uuid],
            }),
        })),
    };
    round_trips(&pinged);
    let NullableString { value } = NullableString::decode(
        NullableString {
            value: Some(String::new()),
        }
        .encode_to_vec()
        .as_slice(),
    )
    .unwrap();
    assert_eq!(value, Some(String::new()));
    assert_eq!(
        NullableString::decode(NullableString { value: None }.encode_to_vec().as_slice())
            .unwrap()
            .value,
        None
    );
}

#[test]
fn the_npc_messages_survive_encoding() {
    use rspigot_plugin_protocol::v1::{
        NpcEquipment, NpcGaze, NpcList, NpcOrder, NpcSkin, NpcSpec, NpcState, Question, answer,
        npc_order, question,
    };

    let spec = NpcSpec {
        key: "agent-7".to_owned(),
        name: "Scout".to_owned(),
        position: Some(Vec3 {
            x: 1.0,
            y: 64.0,
            z: -3.5,
        }),
        rotation: Some(Rotation {
            yaw: 90.0,
            pitch: 0.0,
        }),
        skin: Some(NpcSkin {
            value: "dGV4dHVyZXM=".to_owned(),
            signature: "c2ln".to_owned(),
        }),
        skin_layers: Some(0x7f),
    };
    let asked = FromPlugin {
        body: Some(from_plugin::Body::Question(Question {
            request: 9,
            question: Some(question::Question::CreateNpc(spec)),
        })),
        origin: None,
        request: None,
        in_world: None,
    };
    round_trips(&asked);

    for order in [
        npc_order::Order::Remove(Empty {}),
        npc_order::Order::SetPose("crouching".to_owned()),
        npc_order::Order::SetLabel("§aWorking".to_owned()),
        npc_order::Order::WalkTo(rspigot_plugin_protocol::v1::NpcWalk {
            target: Some(Vec3 {
                x: 1.0,
                y: 64.0,
                z: 2.0,
            }),
            speed: 4.0,
        }),
        npc_order::Order::StopWalking(Empty {}),
        npc_order::Order::HideFrom("00000000-0000-0000-0000-000000000001".to_owned()),
        npc_order::Order::ShowTo("00000000-0000-0000-0000-000000000001".to_owned()),
        npc_order::Order::SetSkinLayers(0x7f),
        npc_order::Order::SwingMainHand(Empty {}),
        npc_order::Order::LookAtNearest(NpcGaze { range: 6.0 }),
        npc_order::Order::SetEquipment(NpcEquipment {
            slot: "head".to_owned(),
            item: None,
        }),
    ] {
        round_trips(&FromPlugin {
            body: Some(from_plugin::Body::Npc(NpcOrder {
                npc: 3,
                order: Some(order),
            })),
            origin: None,
            request: None,
            in_world: None,
        });
    }

    let state = NpcState {
        entity_id: 3,
        uuid: "00000000-0000-0000-0000-000000000003".to_owned(),
        key: "agent-7".to_owned(),
        name: "Scout".to_owned(),
        world: "minecraft:overworld".to_owned(),
        position: Some(Vec3::default()),
        rotation: Some(Rotation::default()),
        skin: None,
        skin_layers: 0x7f,
        pose: "standing".to_owned(),
        gaze_range: 0.0,
        label: Some("Working".to_owned()),
        walking: true,
    };
    let list = NpcList {
        npcs: vec![state.clone()],
    };
    for body in [answer::Body::Npc(state), answer::Body::Npcs(list)] {
        let message = ToPlugin {
            body: Some(to_plugin::Body::Answer(
                rspigot_plugin_protocol::v1::Answer {
                    request: 9,
                    body: Some(body),
                },
            )),
        };
        let decoded = ToPlugin::decode(message.encode_to_vec().as_slice()).unwrap();
        assert_eq!(decoded, message);
    }
}

#[test]
fn a_dialog_order_survives_encoding() {
    use rspigot_plugin_protocol::v1::{
        Dialog, DialogButton, DialogInput, PlayerOrder, player_order,
    };

    let dialog = Dialog {
        title: "Task".to_owned(),
        body: vec!["Describe it".to_owned()],
        inputs: vec![DialogInput {
            key: "task".to_owned(),
            label: "What".to_owned(),
            initial: "x".to_owned(),
            max_length: 80,
            multiline: true,
        }],
        buttons: vec![DialogButton {
            label: "Run".to_owned(),
            id: "agent:run".to_owned(),
        }],
        can_close_with_escape: false,
    };
    for order in [
        player_order::Order::ShowDialog(dialog),
        player_order::Order::ClearDialog(Empty {}),
    ] {
        round_trips(&FromPlugin {
            body: Some(from_plugin::Body::Player(PlayerOrder {
                player: steve().uuid,
                order: Some(order),
            })),
            origin: None,
            request: None,
            in_world: None,
        });
    }
}

#[test]
fn a_rich_message_order_survives_encoding() {
    use rspigot_plugin_protocol::v1::{
        PlayerOrder, RichClick, RichMessage, RichPart, player_order, rich_click,
    };

    let parts = [
        rich_click::Action::RunCommand("/go".to_owned()),
        rich_click::Action::SuggestCommand("/go".to_owned()),
        rich_click::Action::OpenUrl("https://a.b".to_owned()),
        rich_click::Action::CopyToClipboard("x".to_owned()),
        rich_click::Action::Custom("agent:cancel".to_owned()),
    ]
    .into_iter()
    .map(|action| RichPart {
        text: "t".to_owned(),
        hover: Some("h".to_owned()),
        click: Some(RichClick {
            action: Some(action),
        }),
    })
    .collect();
    round_trips(&FromPlugin {
        body: Some(from_plugin::Body::Player(PlayerOrder {
            player: steve().uuid,
            order: Some(player_order::Order::SendRichMessage(RichMessage { parts })),
        })),
        origin: None,
        request: None,
        in_world: None,
    });
}

fn plain(body: from_plugin::Body) -> FromPlugin {
    FromPlugin {
        body: Some(body),
        origin: None,
        request: None,
        in_world: None,
    }
}

#[test]
fn every_entity_edit_survives_encoding() {
    use rspigot_plugin_protocol::v1::{
        DropChanceEdit, EntityEdit, EntityOrder, EquipmentEdit, entity_edit, entity_order,
    };

    let helmet = Item {
        key: "minecraft:iron_helmet".to_owned(),
        count: 1,
        ..Default::default()
    };
    let edits = vec![
        entity_edit::Edit::SetCustomName("Bob".to_owned()),
        entity_edit::Edit::ClearCustomName(Empty {}),
        entity_edit::Edit::SetCustomNameVisible(true),
        entity_edit::Edit::SetGlowing(true),
        entity_edit::Edit::SetInvisible(true),
        entity_edit::Edit::SetSilent(true),
        entity_edit::Edit::SetGravity(false),
        entity_edit::Edit::SetInvulnerable(true),
        entity_edit::Edit::SetPersistent(true),
        entity_edit::Edit::SetFireTicks(40),
        entity_edit::Edit::SetFreezeTicks(10),
        entity_edit::Edit::SetAir(100),
        entity_edit::Edit::SetFallDistance(2.5),
        entity_edit::Edit::SetRotation(Rotation {
            yaw: 90.0,
            pitch: 10.0,
        }),
        entity_edit::Edit::LookAt(Vec3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        }),
        entity_edit::Edit::SetTicksLived(5),
        entity_edit::Edit::AddTag("a".to_owned()),
        entity_edit::Edit::RemoveTag("a".to_owned()),
        entity_edit::Edit::Mount(7),
        entity_edit::Edit::Dismount(Empty {}),
        entity_edit::Edit::EjectPassengers(Empty {}),
        entity_edit::Edit::SetHealth(4.0),
        entity_edit::Edit::SetAbsorption(2.0),
        entity_edit::Edit::SetAi(false),
        entity_edit::Edit::SetCanPickUpLoot(true),
        entity_edit::Edit::SetLeftHanded(true),
        entity_edit::Edit::SetEquipment(EquipmentEdit {
            slot: "head".to_owned(),
            item: Some(helmet),
        }),
        entity_edit::Edit::SetDropChance(DropChanceEdit {
            slot: "head".to_owned(),
            chance: 1.0,
        }),
        entity_edit::Edit::SetLeashHolder(8),
        entity_edit::Edit::ClearLeash(Empty {}),
        entity_edit::Edit::SetOwner("Steve".to_owned()),
        entity_edit::Edit::ClearOwner(Empty {}),
        entity_edit::Edit::SetTamed(true),
        entity_edit::Edit::SetSitting(true),
        entity_edit::Edit::SetTarget(9),
        entity_edit::Edit::ClearTarget(Empty {}),
        entity_edit::Edit::SetAge(-100),
        entity_edit::Edit::SetAgeLocked(true),
        entity_edit::Edit::SetBreeding(true),
    ];
    for edit in edits {
        round_trips(&plain(from_plugin::Body::Entity(EntityOrder {
            entity: 40,
            order: Some(entity_order::Order::Edit(EntityEdit { edit: Some(edit) })),
        })));
    }
}

#[test]
fn every_tile_edit_and_data_edit_survives_encoding() {
    use rspigot_plugin_protocol::v1::{
        BannerPattern, BannerPatterns, BeaconEffects, BlockDataEdit, BlockOrder, CommandEdit,
        FurnaceEdit, GatewayExit, PotSherds, SetSlot, SignLines, SignStyle, SkullOwner,
        SpawnerEdit, TileEdit, block_order, tile_edit,
    };

    let pos = BlockPos { x: 1, y: 2, z: 3 };
    let stone = Item {
        key: "minecraft:stone".to_owned(),
        count: 3,
        ..Default::default()
    };
    let patterns = BannerPatterns {
        patterns: vec![BannerPattern {
            pattern: "minecraft:stripe_top".to_owned(),
            color: "red".to_owned(),
        }],
    };
    let edits = vec![
        tile_edit::Edit::SignLines(SignLines {
            front: true,
            lines: vec!["a".to_owned(), "b".to_owned()],
        }),
        tile_edit::Edit::SignStyle(SignStyle {
            front: false,
            color: Some("red".to_owned()),
            glowing: Some(true),
        }),
        tile_edit::Edit::SignWaxed(true),
        tile_edit::Edit::InventorySet(SetSlot {
            slot: 2,
            item: Some(stone.clone()),
        }),
        tile_edit::Edit::InventoryAdd(stone.clone()),
        tile_edit::Edit::InventoryClear(Empty {}),
        tile_edit::Edit::Furnace(FurnaceEdit {
            burn_time: Some(100),
            cook_time: Some(5),
            cook_time_total: Some(200),
        }),
        tile_edit::Edit::Spawner(SpawnerEdit {
            entity: Some("minecraft:zombie".to_owned()),
            delay: Some(1),
            min_delay: Some(2),
            max_delay: Some(3),
            spawn_count: Some(4),
            max_nearby: Some(5),
            required_player_range: Some(6),
            spawn_range: Some(7),
        }),
        tile_edit::Edit::SkullOwner(SkullOwner {
            name: Some("Steve".to_owned()),
            uuid: None,
        }),
        tile_edit::Edit::BannerPatterns(patterns.clone()),
        tile_edit::Edit::CommandBlock(CommandEdit {
            command: Some("say hi".to_owned()),
            name: Some("n".to_owned()),
        }),
        tile_edit::Edit::Beacon(BeaconEffects {
            primary: Some("minecraft:speed".to_owned()),
            secondary: None,
        }),
        tile_edit::Edit::JukeboxDisc(stone.clone()),
        tile_edit::Edit::JukeboxClear(Empty {}),
        tile_edit::Edit::EndGateway(GatewayExit {
            exit: Some(pos),
            exact: true,
        }),
        tile_edit::Edit::LecternBook(stone),
        tile_edit::Edit::LecternClear(Empty {}),
        tile_edit::Edit::DecoratedPot(PotSherds {
            sherds: vec!["minecraft:brick".to_owned()],
        }),
    ];
    for edit in edits {
        round_trips(&plain(from_plugin::Body::Block(BlockOrder {
            pos: Some(pos),
            order: Some(block_order::Order::Tile(TileEdit { edit: Some(edit) })),
        })));
    }
    round_trips(&plain(from_plugin::Body::Block(BlockOrder {
        pos: Some(pos),
        order: Some(block_order::Order::SetData(BlockDataEdit {
            properties: vec![Property {
                name: "facing".to_owned(),
                value: "north".to_owned(),
            }],
        })),
    })));
}

#[test]
fn the_new_questions_and_answers_survive_encoding() {
    use rspigot_plugin_protocol::v1::{
        Answer, BlockInfo, DiscState, EntityState, EquipmentState, InventoryContents, LivingExtra,
        Question, SignState, Slot, TileState, answer, question, tile_state,
    };

    let pos = BlockPos { x: 1, y: 2, z: 3 };
    for asked in [
        question::Question::Tile(pos),
        question::Question::BlockInfo(pos),
    ] {
        round_trips(&plain(from_plugin::Body::Question(Question {
            request: 4,
            question: Some(asked),
        })));
    }
    let item = Item {
        key: "minecraft:stone".to_owned(),
        count: 1,
        ..Default::default()
    };
    let state = EntityState {
        entity: Some(zombie()),
        passengers: vec![1, 2],
        vehicle: Some(3),
        tags: vec!["x".to_owned()],
        rotation: Some(Rotation {
            yaw: 1.0,
            pitch: 2.0,
        }),
        living: Some(LivingExtra {
            equipment: vec![EquipmentState {
                slot: "head".to_owned(),
                item: Some(item.clone()),
                drop_chance: 0.5,
            }],
            owner: Some("id".to_owned()),
            target: Some(5),
            leash_holder: Some(6),
            ..Default::default()
        }),
        ..Default::default()
    };
    round_trips(&Answer {
        request: 4,
        body: Some(answer::Body::Entity(state)),
    });
    round_trips(&Answer {
        request: 4,
        body: Some(answer::Body::BlockInfo(BlockInfo {
            light: 15,
            sky_light: 15,
            block_light: 0,
            power: 3,
            biome: "minecraft:plains".to_owned(),
        })),
    });
    let tiles = [
        tile_state::State::Sign(SignState {
            front: vec!["a".to_owned()],
            waxed: true,
            ..Default::default()
        }),
        tile_state::State::Inventory(InventoryContents {
            size: 27,
            slots: vec![Slot {
                slot: 0,
                item: Some(item.clone()),
            }],
        }),
        tile_state::State::Jukebox(DiscState { item: Some(item) }),
    ];
    for state in tiles {
        round_trips(&Answer {
            request: 4,
            body: Some(answer::Body::Tile(TileState {
                kind: "minecraft:chest".to_owned(),
                state: Some(state),
            })),
        });
    }
}

#[test]
fn an_entity_snapshot_carries_its_name_and_tags() {
    round_trips(&EntitySnapshot {
        custom_name: Some("Bob".to_owned()),
        tags: vec!["boss".to_owned()],
        ..zombie()
    });
}
