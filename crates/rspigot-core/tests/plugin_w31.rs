mod common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use common::*;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{
    ClickInput, ClientEvent, ContainerClick, HashedSlot, Inbound, PlayerInput,
};
use rspigot_core::plugins::events::ResourcePackStatus;
use rspigot_core::plugins::{
    Answer, BlockOrder, BlockState, BorderOrder, ClientChange, CustomInventoryOrder, EntityOrder,
    EventKind, EventPayload, FluidMode, InventorySpec, InventoryType, ItemSpec, LookupAnswer,
    LookupQuestion, LootRequest, LootTarget, MerchantProgress, Order, PlayerAction, PlayerOrder,
    Question, RayTraceQuery, SchedulerOrder, ServerOrder, TargetHit, ToPlugin, Trade,
};
use rspigot_core::plugins::{ExtraAnswer, ExtraQuestion, FromPlugin, ProfileLookup};
use rspigot_core::plugins::{IngredientSpec, RecipeKind, RecipeShape, RecipeSpec};
use rspigot_core::plugins::{
    ObjectAnswer, ObjectQuestion, PdcChange, PdcHolder, PdcValue, WorldObjectOrder, WorldOrder,
};
use rspigot_core::ports::{
    ChunkGenerator, PlaceFailure, PlaceRequest, PlacedBlock, TemplateInfo, TemplateMirror,
    TemplateRotation,
};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, Vec3};
use uuid::Uuid;

fn server_order(order: ServerOrder) -> Order {
    Order::Server(order)
}

fn client(s: &mut Scene, change: ClientChange) {
    let alice = s.players[0].clone();
    s.act(to(
        &alice,
        PlayerOrder::Action(PlayerAction::Client(change)),
    ));
}

fn player_action(s: &mut Scene, action: PlayerAction) {
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::Action(action)));
}

fn names(answer: Answer) -> Vec<String> {
    match answer {
        Answer::Lookup(LookupAnswer::Names(names)) => names,
        other => panic!("expected names, got {other:?}"),
    }
}

fn due(s: &Scene) -> Vec<u64> {
    s.plugin
        .received()
        .into_iter()
        .filter_map(|message| match message {
            ToPlugin::TaskDue(id) => Some(id),
            _ => None,
        })
        .collect()
}

#[test]
fn a_block_change_reaches_only_the_player_and_leaves_the_world_alone() {
    let mut s = scene(&["Alice", "Bob"]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    let pos = BlockPos::new(2, 70, 2);
    let gold = BlockState {
        kind: "minecraft:gold_block".to_owned(),
        properties: Vec::new(),
    };

    client(&mut s, ClientChange::Blocks(vec![(pos, gold)]));

    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
    let sent = alice.link.take();
    assert!(
        sent.iter().any(|event| match event {
            ClientEvent::BlockUpdate { pos: at, .. } => *at == pos,
            ClientEvent::SectionBlocks { changes, .. } => {
                changes.iter().any(|(at, _)| *at == pos)
            }
            _ => false,
        }),
        "{sent:?}"
    );
    assert!(bob.link.take().is_empty());
    let Answer::Block(state) = s.ask(Question::Block(pos)) else {
        panic!("a block");
    };
    assert_eq!(state.kind, "minecraft:air");
}

#[test]
fn an_unknown_block_state_is_refused() {
    let mut s = scene(&["Alice"]);
    let bad = BlockState {
        kind: "minecraft:nonsense".to_owned(),
        properties: Vec::new(),
    };
    client(
        &mut s,
        ClientChange::Blocks(vec![(BlockPos::new(0, 70, 0), bad)]),
    );
    assert_eq!(s.warnings().len(), 1);
}

#[test]
fn a_sign_change_sends_a_block_entity_with_the_lines() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();

    client(
        &mut s,
        ClientChange::Sign {
            pos: BlockPos::new(3, 70, 3),
            lines: vec!["one".to_owned(), "two".to_owned()],
            color: Some("red".to_owned()),
            glowing: true,
        },
    );

    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
    let update = alice
        .link
        .take()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::BlockEntityUpdate { pos, kind, data } => Some((pos, kind, data)),
            _ => None,
        })
        .expect("a block entity update");
    assert_eq!(update.0, BlockPos::new(3, 70, 3));
    assert_eq!(update.1, "sign");
    let front = update.2.get_compound("front_text").expect("front text");
    assert_eq!(front.get_str("color"), Some("red"));
    assert_eq!(front.get_list("messages").map(<[_]>::len), Some(4));
}

#[test]
fn a_sign_change_with_a_bad_colour_or_five_lines_is_refused() {
    let mut s = scene(&["Alice"]);
    client(
        &mut s,
        ClientChange::Sign {
            pos: BlockPos::new(3, 70, 3),
            lines: Vec::new(),
            color: Some("octarine".to_owned()),
            glowing: false,
        },
    );
    client(
        &mut s,
        ClientChange::Sign {
            pos: BlockPos::new(3, 70, 3),
            lines: vec![String::new(); 5],
            color: None,
            glowing: false,
        },
    );
    assert_eq!(s.warnings().len(), 2);
}

#[test]
fn an_equipment_change_and_a_hurt_animation_are_sent_to_the_player() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let entity = rspigot_domain::EntityId(77);

    client(
        &mut s,
        ClientChange::Equipment {
            entity,
            slots: vec![(
                "head".to_owned(),
                Some(ItemSpec::of("minecraft:diamond_helmet", 1)),
            )],
        },
    );
    client(&mut s, ClientChange::Hurt { entity, yaw: 90.0 });

    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
    let sent = alice.link.take();
    assert!(sent.iter().any(|event| matches!(
        event,
        ClientEvent::EntityEquipment { entity: at, slots } if *at == entity && slots.len() == 1
    )));
    assert!(sent.iter().any(|event| matches!(
        event,
        ClientEvent::HurtAnimation { entity: at, yaw } if *at == entity && *yaw == 90.0
    )));
}

#[test]
fn a_resource_pack_is_pushed_removed_and_its_status_is_heard() {
    let mut s = scene_with(
        &["Alice"],
        vec![observe(1, EventKind::PlayerResourcePackStatus)],
    );
    let alice = s.players[0].clone();
    let pack = Uuid::from_u128(9);

    client(
        &mut s,
        ClientChange::ResourcePack {
            id: pack,
            url: "https://example.org/pack.zip".to_owned(),
            hash: "0123456789abcdef0123456789abcdef01234567".to_owned(),
            required: false,
            prompt: Some("Please".to_owned()),
        },
    );
    let sent = alice.link.take();
    assert!(sent.iter().any(|event| matches!(
        event,
        ClientEvent::ResourcePackPush(offer) if offer.id == pack && offer.url.ends_with("pack.zip")
    )));

    s.server.handle(Inbound::Input {
        player: alice.id,
        input: PlayerInput::ResourcePack {
            id: pack,
            status: ResourcePackStatus::SuccessfullyLoaded,
        },
    });
    s.server.deliver();
    let heard = settled_for(&s.plugin, EventKind::PlayerResourcePackStatus);
    assert_eq!(heard.len(), 1);
    let EventPayload::PlayerResourcePackStatus(status) = &heard[0].payload else {
        panic!("a status");
    };
    assert_eq!(status.pack_id, pack);
    assert_eq!(status.status, ResourcePackStatus::SuccessfullyLoaded);

    client(&mut s, ClientChange::RemoveResourcePack(Some(pack)));
    assert!(
        alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::ResourcePackPop(Some(id)) if *id == pack))
    );
}

#[test]
fn a_delayed_task_fires_once_after_its_delay_in_server_ticks() {
    let mut s = scene(&["Alice"]);
    s.act(server_order(ServerOrder::Schedule(
        SchedulerOrder::Schedule {
            id: 1,
            delay: 3,
            period: 0,
        },
    )));

    s.tick(2);
    assert!(due(&s).is_empty());
    s.tick(1);
    assert_eq!(due(&s), vec![1]);
    s.tick(10);
    assert_eq!(due(&s), vec![1]);
    assert_eq!(s.server.plugin_task_count(), 0);
}

#[test]
fn a_repeating_task_fires_every_period_until_it_is_cancelled() {
    let mut s = scene(&["Alice"]);
    s.act(server_order(ServerOrder::Schedule(
        SchedulerOrder::Schedule {
            id: 4,
            delay: 2,
            period: 5,
        },
    )));

    s.tick(2);
    assert_eq!(due(&s), vec![4]);
    s.tick(4);
    assert_eq!(due(&s), vec![4]);
    s.tick(1);
    assert_eq!(due(&s), vec![4, 4]);
    s.tick(5);
    assert_eq!(due(&s), vec![4, 4, 4]);

    s.act(server_order(ServerOrder::Schedule(SchedulerOrder::Cancel(
        4,
    ))));
    s.tick(20);
    assert_eq!(due(&s).len(), 3);
    assert_eq!(s.server.plugin_task_count(), 0);
}

#[test]
fn a_zero_delay_runs_on_the_next_tick() {
    let mut s = scene(&["Alice"]);
    s.act(server_order(ServerOrder::Schedule(
        SchedulerOrder::Schedule {
            id: 2,
            delay: 0,
            period: 0,
        },
    )));
    s.tick(1);
    assert_eq!(due(&s), vec![2]);
}

#[test]
fn tasks_are_dropped_when_the_plugin_unloads_and_do_not_leak_to_the_next_one() {
    let mut s = scene(&["Alice"]);
    s.act(server_order(ServerOrder::Schedule(
        SchedulerOrder::Schedule {
            id: 1,
            delay: 2,
            period: 2,
        },
    )));
    assert_eq!(s.server.plugin_task_count(), 1);

    s.server
        .handle(Inbound::PluginDisconnected { connection: 1 });

    assert_eq!(s.server.plugin_task_count(), 0);
    let fresh = connect(&mut s.server, 2, "second");
    s.tick(10);
    assert!(
        fresh
            .received()
            .iter()
            .all(|message| !matches!(message, ToPlugin::TaskDue(_)))
    );
}

#[test]
fn a_replaced_plugin_loses_the_tasks_of_the_old_connection() {
    let mut s = scene(&["Alice"]);
    s.act(server_order(ServerOrder::Schedule(
        SchedulerOrder::Schedule {
            id: 1,
            delay: 2,
            period: 2,
        },
    )));

    let _again = connect(&mut s.server, 5, "tester");

    assert_eq!(s.server.plugin_task_count(), 0);
}

fn typed(id: u64, ty: InventoryType, slots: i32, trades: Vec<Trade>) -> Order {
    typed_locked(id, ty, slots, trades, true)
}

fn typed_locked(id: u64, ty: InventoryType, slots: i32, trades: Vec<Trade>, locked: bool) -> Order {
    server_order(ServerOrder::CreateTypedInventory {
        id,
        spec: InventorySpec {
            ty,
            slots,
            title: "Typed".to_owned(),
            locked,
            trades,
            progress: None,
        },
    })
}

fn custom_state(s: &mut Scene, id: u64) -> rspigot_core::plugins::CustomInventoryState {
    let Answer::CustomInventory(state) = s.ask(Question::CustomInventory(id)) else {
        panic!("a custom inventory");
    };
    state
}

fn screen_of(events: &[ClientEvent]) -> Option<(i32, i32)> {
    events.iter().find_map(|event| match event {
        ClientEvent::OpenScreen { window, menu, .. } => Some((*window, *menu)),
        _ => None,
    })
}

#[test]
fn every_vanilla_inventory_type_opens_with_its_screen_and_size() {
    let table: [(InventoryType, i32, i32, usize); 22] = [
        (InventoryType::Chest, 27, 2, 27),
        (InventoryType::Dispenser, 0, 6, 9),
        (InventoryType::Dropper, 0, 6, 9),
        (InventoryType::Furnace, 0, 14, 3),
        (InventoryType::BlastFurnace, 0, 10, 3),
        (InventoryType::Smoker, 0, 22, 3),
        (InventoryType::Workbench, 0, 12, 10),
        (InventoryType::Enchanting, 0, 13, 2),
        (InventoryType::Brewing, 0, 11, 5),
        (InventoryType::Anvil, 0, 8, 3),
        (InventoryType::Beacon, 0, 9, 1),
        (InventoryType::Hopper, 0, 16, 5),
        (InventoryType::ShulkerBox, 0, 20, 27),
        (InventoryType::Barrel, 0, 2, 27),
        (InventoryType::EnderChest, 0, 2, 27),
        (InventoryType::Grindstone, 0, 15, 3),
        (InventoryType::Loom, 0, 18, 4),
        (InventoryType::Cartography, 0, 23, 3),
        (InventoryType::Smithing, 0, 21, 4),
        (InventoryType::Stonecutter, 0, 24, 2),
        (InventoryType::Crafter, 0, 7, 10),
        (InventoryType::Merchant, 0, 19, 3),
    ];
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    for (index, (ty, slots, menu, size)) in table.into_iter().enumerate() {
        let id = index as u64 + 1;
        s.act(typed(id, ty, slots, Vec::new()));
        assert!(s.warnings().is_empty(), "{ty:?}: {:?}", s.warnings());
        let state = custom_state(&mut s, id);
        assert_eq!(state.inventory_type, ty);
        assert_eq!(state.items.len(), size, "{ty:?}");

        s.act(to(&alice, PlayerOrder::OpenInventory(id)));
        let events = alice.link.take();
        let (_, shown) = screen_of(&events).unwrap_or_else(|| panic!("{ty:?} does not open"));
        assert_eq!(shown, menu, "{ty:?}");
        s.act(to(&alice, PlayerOrder::CloseInventory));
        alice.link.take();
    }
}

#[test]
fn chest_sizes_are_multiples_of_nine_up_to_fifty_four() {
    let mut s = scene(&["Alice"]);
    for (id, slots) in [(1, 9), (2, 18), (3, 45), (4, 54)] {
        s.act(typed(id, InventoryType::Chest, slots, Vec::new()));
        assert_eq!(
            custom_state(&mut s, id).items.len(),
            usize::try_from(slots).unwrap()
        );
    }
    for slots in [0, 10, 63, -9] {
        s.act(typed(9, InventoryType::Chest, slots, Vec::new()));
    }
    assert_eq!(s.warnings().len(), 4);
    assert!(matches!(
        s.ask(Question::CustomInventory(9)),
        Answer::NotFound
    ));
}

#[test]
fn only_a_merchant_takes_trades() {
    let mut s = scene(&["Alice"]);
    s.act(typed(1, InventoryType::Hopper, 0, vec![trade(1)]));
    assert!(matches!(
        s.ask(Question::CustomInventory(1)),
        Answer::NotFound
    ));
}

fn trade(price: i32) -> Trade {
    Trade {
        result: ItemSpec::of("minecraft:diamond", 1),
        ingredients: vec![ItemSpec::of("minecraft:emerald", price)],
        uses: 0,
        max_uses: 5,
        experience_reward: false,
        villager_experience: 0,
        price_multiplier: 0.0,
        demand: 0,
        special_price: 0,
    }
}

fn offers_sent(events: &[ClientEvent]) -> Vec<usize> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::MerchantOffers { offers, .. } => Some(offers.len()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_custom_merchant_opens_with_its_recipes_and_takes_new_ones() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(typed(
        1,
        InventoryType::Merchant,
        0,
        vec![trade(1), trade(2)],
    ));
    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
    assert_eq!(custom_state(&mut s, 1).trades.len(), 2);

    s.act(to(&alice, PlayerOrder::OpenInventory(1)));
    assert_eq!(offers_sent(&alice.link.take()), vec![2]);

    s.act(Order::CustomInventory {
        inventory: 1,
        order: CustomInventoryOrder::SetTrades(vec![trade(3)]),
    });
    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
    assert_eq!(offers_sent(&alice.link.take()), vec![1]);
    let trades = custom_state(&mut s, 1).trades;
    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].ingredients[0].count, 3);
}

#[test]
fn trading_with_a_custom_merchant_gives_the_result_and_counts_a_use() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(Order::Inventory {
        player: alice.id,
        order: rspigot_core::plugins::InventoryOrder::Set {
            slot: 0,
            item: Some(ItemSpec::of("minecraft:emerald", 4)),
        },
    });
    s.act(typed_locked(
        1,
        InventoryType::Merchant,
        0,
        vec![trade(2)],
        false,
    ));
    s.act(to(&alice, PlayerOrder::OpenInventory(1)));
    let events = alice.link.take();
    let (window, _) = screen_of(&events).expect("the merchant opens");

    s.server.handle(Inbound::Input {
        player: alice.id,
        input: PlayerInput::SelectTrade(0),
    });
    s.server.deliver();
    let after_select = alice.link.take();
    let state_id = after_select
        .iter()
        .rev()
        .filter_map(|event| match event {
            ClientEvent::ContainerContent { state_id, .. }
            | ClientEvent::ContainerSlot { state_id, .. } => Some(*state_id),
            _ => None,
        })
        .next()
        .unwrap_or(0);
    s.server.handle(Inbound::Input {
        player: alice.id,
        input: PlayerInput::ContainerClick(ContainerClick {
            container: window,
            state_id,
            slot: 2,
            button: 0,
            input: ClickInput::Pickup,
            changed: Vec::new(),
            carried: HashedSlot::EMPTY,
        }),
    });
    s.server.deliver();

    assert_eq!(custom_state(&mut s, 1).trades[0].uses, 1);
    let carried = alice.link.take().into_iter().find_map(|event| match event {
        ClientEvent::ContainerContent { carried, .. } => carried,
        _ => None,
    });
    let carried = carried.expect("the result is on the cursor");
    assert_eq!(carried.count.get(), 1);
}

struct Templates;

impl ChunkGenerator for Templates {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        Floor.generate(dimension, pos)
    }

    fn spawn_point(&self, dimension: &Dimension) -> Vec3 {
        Floor.spawn_point(dimension)
    }

    fn template_names(&self) -> Vec<String> {
        vec!["t:hut".to_owned()]
    }

    fn template_info(&self, id: &str) -> Option<TemplateInfo> {
        (id == "t:hut").then_some(TemplateInfo {
            size: [3, 2, 4],
            blocks: 2,
        })
    }

    fn nearest_structure(
        &self,
        _: &Dimension,
        _: Vec3,
        structure_id: &str,
    ) -> Option<(String, Vec3)> {
        (structure_id == "minecraft:village_plains")
            .then(|| (structure_id.to_owned(), Vec3::new(100.5, 64.0, -40.5)))
    }

    fn place(
        &self,
        _: &Dimension,
        request: &PlaceRequest,
    ) -> Result<Vec<PlacedBlock>, PlaceFailure> {
        match request {
            PlaceRequest::Template { id, pos, .. } if id == "t:hut" => Ok(vec![
                PlacedBlock {
                    pos: *pos,
                    state: BlockStateId(1),
                },
                PlacedBlock {
                    pos: BlockPos::new(pos.x + 2, pos.y + 1, pos.z + 3),
                    state: BlockStateId(1),
                },
            ]),
            PlaceRequest::Template { .. } => Err(PlaceFailure::Unknown),
            _ => Err(PlaceFailure::Failed),
        }
    }
}

fn templated_scene() -> Scene {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(Templates),
        StatusBoard::new(20),
    );
    server.wait_for_spawn(std::time::Duration::from_secs(10));
    scene_on(server, &["Alice"], Vec::new())
}

fn lookup(s: &mut Scene, question: LookupQuestion) -> Answer {
    s.ask(Question::Lookup(question))
}

#[test]
fn a_structure_template_is_placed_in_the_world_and_counted() {
    let mut s = templated_scene();
    let at = BlockPos::new(4, 70, 4);

    let answer = lookup(
        &mut s,
        LookupQuestion::PlaceStructure {
            template: "t:hut".to_owned(),
            pos: at,
            rotation: TemplateRotation::None,
            mirror: TemplateMirror::None,
            integrity: 1.0,
            seed: 0,
        },
    );

    assert_eq!(answer, Answer::Lookup(LookupAnswer::Placed(2)));
    for pos in [at, BlockPos::new(6, 71, 7)] {
        let Answer::Block(state) = s.ask(Question::Block(pos)) else {
            panic!("a block");
        };
        assert_eq!(state.kind, "minecraft:stone");
    }
}

#[test]
fn an_unknown_template_is_not_found_and_changes_nothing() {
    let mut s = templated_scene();
    let answer = lookup(
        &mut s,
        LookupQuestion::PlaceStructure {
            template: "t:nothing".to_owned(),
            pos: BlockPos::new(4, 70, 4),
            rotation: TemplateRotation::None,
            mirror: TemplateMirror::None,
            integrity: 1.0,
            seed: 0,
        },
    );
    assert_eq!(answer, Answer::NotFound);
}

#[test]
fn reading_a_structure_gives_its_bounding_box_without_placing_it() {
    let mut s = templated_scene();

    let answer = lookup(&mut s, LookupQuestion::Structure("t:hut".to_owned()));

    let Answer::Lookup(LookupAnswer::Structure(size)) = answer else {
        panic!("a structure size, got {answer:?}");
    };
    assert_eq!((size.x, size.y, size.z, size.blocks), (3, 2, 4, 2));
    assert_eq!(
        lookup(&mut s, LookupQuestion::Structure("t:none".to_owned())),
        Answer::NotFound
    );
}

#[test]
fn the_known_structures_are_listed_and_the_nearest_one_is_located() {
    let mut s = templated_scene();

    let listed = names(lookup(&mut s, LookupQuestion::Structures));
    assert_eq!(listed, ["t:hut"]);

    let found = lookup(
        &mut s,
        LookupQuestion::LocateStructure {
            structure: "village_plains".to_owned(),
            from: Vec3::new(0.0, 64.0, 0.0),
        },
    );
    assert_eq!(
        found,
        Answer::Lookup(LookupAnswer::Located(Some((
            "minecraft:village_plains".to_owned(),
            Vec3::new(100.5, 64.0, -40.5)
        ))))
    );
    let none = lookup(
        &mut s,
        LookupQuestion::LocateStructure {
            structure: "minecraft:igloo".to_owned(),
            from: Vec3::new(0.0, 64.0, 0.0),
        },
    );
    assert_eq!(none, Answer::Lookup(LookupAnswer::Located(None)));
}

#[test]
fn a_loot_table_generates_items_from_a_context() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let request = |seed| LootRequest {
        table: "minecraft:chests/spawn_bonus_chest".to_owned(),
        position: Some(Vec3::new(0.5, 64.0, 0.5)),
        looter: Some(alice.id),
        luck: 1.0,
        seed: Some(seed),
    };

    let first = lookup(&mut s, LookupQuestion::GenerateLoot(request(7)));
    let again = lookup(&mut s, LookupQuestion::GenerateLoot(request(7)));

    let Answer::Lookup(LookupAnswer::Loot(items)) = first.clone() else {
        panic!("loot, got {first:?}");
    };
    assert!(!items.is_empty());
    assert_eq!(first, again);
}

#[test]
fn an_unknown_loot_table_or_looter_is_not_found() {
    let mut s = scene(&["Alice"]);
    let unknown = LootRequest {
        table: "minecraft:chests/nope".to_owned(),
        position: None,
        looter: None,
        luck: 0.0,
        seed: Some(1),
    };
    assert_eq!(
        lookup(&mut s, LookupQuestion::GenerateLoot(unknown)),
        Answer::NotFound
    );
    let ghost = LootRequest {
        table: "minecraft:chests/spawn_bonus_chest".to_owned(),
        position: None,
        looter: Some(rspigot_domain::player::PlayerId(Uuid::from_u128(404))),
        luck: 0.0,
        seed: Some(1),
    };
    assert_eq!(
        lookup(&mut s, LookupQuestion::GenerateLoot(ghost)),
        Answer::NotFound
    );
}

#[test]
fn tags_and_registries_are_readable() {
    let mut s = scene(&["Alice"]);

    let members = names(lookup(
        &mut s,
        LookupQuestion::TagMembers {
            registry: "block".to_owned(),
            tag: "#mineable/pickaxe".to_owned(),
        },
    ));
    assert!(members.contains(&"minecraft:stone".to_owned()));
    assert!(!members.contains(&"minecraft:dirt".to_owned()));

    let tags = names(lookup(
        &mut s,
        LookupQuestion::Tags("minecraft:block".to_owned()),
    ));
    assert!(tags.contains(&"minecraft:mineable/pickaxe".to_owned()));

    let entries = names(lookup(
        &mut s,
        LookupQuestion::RegistryEntries("minecraft:item".to_owned()),
    ));
    assert!(entries.contains(&"minecraft:diamond".to_owned()));

    let registries = names(lookup(&mut s, LookupQuestion::Registries));
    assert!(registries.contains(&"minecraft:block".to_owned()));
    assert!(registries.contains(&"minecraft:activity".to_owned()));
    assert!(registries.windows(2).all(|pair| pair[0] < pair[1]));

    assert_eq!(
        lookup(
            &mut s,
            LookupQuestion::TagMembers {
                registry: "block".to_owned(),
                tag: "nope".to_owned(),
            }
        ),
        Answer::NotFound
    );
    assert_eq!(
        lookup(&mut s, LookupQuestion::RegistryEntries("nope".to_owned())),
        Answer::NotFound
    );
}

#[test]
fn the_loot_tables_are_listed() {
    let mut s = scene(&["Alice"]);
    let tables = names(lookup(&mut s, LookupQuestion::LootTables));
    assert!(tables.contains(&"minecraft:chests/spawn_bonus_chest".to_owned()));
    assert!(tables.windows(2).all(|pair| pair[0] < pair[1]));
}

fn progress_sent(events: &[ClientEvent]) -> Vec<(i32, i32, bool, bool)> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::MerchantOffers {
                level,
                xp,
                show_progress,
                can_restock,
                ..
            } => Some((*level, *xp, *show_progress, *can_restock)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_custom_merchant_shows_its_level_xp_and_restock_flags_and_updates_them() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(server_order(ServerOrder::CreateTypedInventory {
        id: 1,
        spec: InventorySpec {
            ty: InventoryType::Merchant,
            slots: 0,
            title: "Shop".to_owned(),
            locked: false,
            trades: vec![trade(1)],
            progress: Some(MerchantProgress {
                level: 3,
                xp: 40,
                show_progress: true,
                can_restock: true,
            }),
        },
    }));
    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
    assert_eq!(
        custom_state(&mut s, 1).progress,
        Some(MerchantProgress {
            level: 3,
            xp: 40,
            show_progress: true,
            can_restock: true
        })
    );

    s.act(to(&alice, PlayerOrder::OpenInventory(1)));
    assert_eq!(progress_sent(&alice.link.take()), vec![(3, 40, true, true)]);

    s.act(Order::CustomInventory {
        inventory: 1,
        order: CustomInventoryOrder::SetProgress(MerchantProgress {
            level: 5,
            xp: 0,
            show_progress: false,
            can_restock: false,
        }),
    });
    assert_eq!(
        progress_sent(&alice.link.take()),
        vec![(5, 0, false, false)]
    );
}

#[test]
fn a_merchant_progress_outside_its_bounds_is_refused() {
    let mut s = scene(&["Alice"]);
    s.act(typed_locked(
        1,
        InventoryType::Merchant,
        0,
        Vec::new(),
        false,
    ));
    s.act(Order::CustomInventory {
        inventory: 1,
        order: CustomInventoryOrder::SetProgress(MerchantProgress {
            level: 9,
            xp: 0,
            show_progress: true,
            can_restock: true,
        }),
    });
    s.act(Order::CustomInventory {
        inventory: 1,
        order: CustomInventoryOrder::SetProgress(MerchantProgress {
            level: 1,
            xp: -4,
            show_progress: true,
            can_restock: true,
        }),
    });
    assert_eq!(s.warnings().len(), 2);
}

fn lootable(s: &mut Scene, target: LootTarget) -> Answer {
    lookup(s, LookupQuestion::Lootable(target))
}

#[test]
fn a_container_takes_a_loot_table_that_can_be_read_and_cleared() {
    let mut s = scene(&["Alice"]);
    let pos = BlockPos::new(3, 64, 3);
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set("minecraft:chest".to_owned()),
    });

    s.act(Order::Block {
        pos,
        order: BlockOrder::SetLootTable {
            table: Some("minecraft:chests/spawn_bonus_chest".to_owned()),
            seed: 5,
        },
    });
    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
    assert_eq!(
        lootable(&mut s, LootTarget::Block(pos)),
        Answer::Lookup(LookupAnswer::Lootable {
            table: Some("minecraft:chests/spawn_bonus_chest".to_owned()),
            seed: 5
        })
    );

    s.act(Order::Block {
        pos,
        order: BlockOrder::SetLootTable {
            table: None,
            seed: 0,
        },
    });
    assert_eq!(
        lootable(&mut s, LootTarget::Block(pos)),
        Answer::Lookup(LookupAnswer::Lootable {
            table: None,
            seed: 0
        })
    );
}

#[test]
fn a_loot_table_is_refused_on_a_plain_block_or_with_an_unknown_name() {
    let mut s = scene(&["Alice"]);
    let pos = BlockPos::new(3, 64, 3);
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set("minecraft:stone".to_owned()),
    });
    s.act(Order::Block {
        pos,
        order: BlockOrder::SetLootTable {
            table: Some("minecraft:chests/spawn_bonus_chest".to_owned()),
            seed: 0,
        },
    });
    let chest = BlockPos::new(4, 64, 3);
    s.act(Order::Block {
        pos: chest,
        order: BlockOrder::Set("minecraft:chest".to_owned()),
    });
    s.act(Order::Block {
        pos: chest,
        order: BlockOrder::SetLootTable {
            table: Some("minecraft:chests/nope".to_owned()),
            seed: 0,
        },
    });
    assert_eq!(s.warnings().len(), 2);
    assert_eq!(lootable(&mut s, LootTarget::Block(pos)), Answer::NotFound);
}

#[test]
fn a_mob_takes_a_death_loot_table_that_can_be_read() {
    let mut s = scene(&["Alice"]);
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: "minecraft:zombie".to_owned(),
        position: Vec3::new(8.5, 64.0, 8.5),
    }) else {
        panic!("a zombie");
    };
    s.act(Order::Entity {
        entity: view.id,
        order: EntityOrder::SetLootTable {
            table: Some("minecraft:chests/spawn_bonus_chest".to_owned()),
            seed: 11,
        },
    });
    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
    assert_eq!(
        lootable(&mut s, LootTarget::Entity(view.id)),
        Answer::Lookup(LookupAnswer::Lootable {
            table: Some("minecraft:chests/spawn_bonus_chest".to_owned()),
            seed: 11
        })
    );
}

struct Anywhere;

impl ChunkGenerator for Anywhere {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        Floor.generate(dimension, pos)
    }

    fn spawn_point(&self, dimension: &Dimension) -> Vec3 {
        Floor.spawn_point(dimension)
    }

    fn nearest_structure(
        &self,
        _: &Dimension,
        _: Vec3,
        structure_id: &str,
    ) -> Option<(String, Vec3)> {
        Some((structure_id.to_owned(), Vec3::new(64.5, 64.0, 64.5)))
    }
}

fn scene_with_generator<G: ChunkGenerator>(generator: G) -> Scene {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(generator),
        StatusBoard::new(20),
    );
    server.wait_for_spawn(std::time::Duration::from_secs(10));
    scene_on(server, &["Alice"], Vec::new())
}

fn map_loot(s: &mut Scene, position: Option<Vec3>) -> usize {
    let request = LootRequest {
        table: "minecraft:chests/shipwreck_map".to_owned(),
        position,
        looter: None,
        luck: 0.0,
        seed: Some(3),
    };
    match lookup(s, LookupQuestion::GenerateLoot(request)) {
        Answer::Lookup(LookupAnswer::Loot(items)) => items.len(),
        other => panic!("loot expected, got {other:?}"),
    }
}

#[test]
fn the_loot_position_resolves_exploration_maps_and_drops_those_it_cannot_find() {
    let at = Some(Vec3::new(0.5, 64.0, 0.5));
    let mut finding = scene_with_generator(Anywhere);
    let found = map_loot(&mut finding, at);
    let mut blind = scene_with_generator(Floor_);
    let lost = map_loot(&mut blind, at);
    assert_eq!(found, lost + 1);
}

struct Floor_;

impl ChunkGenerator for Floor_ {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        Floor.generate(dimension, pos)
    }

    fn spawn_point(&self, dimension: &Dimension) -> Vec3 {
        Floor.spawn_point(dimension)
    }
}

#[test]
fn a_biome_change_rewrites_the_cell_and_resends_the_chunk() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let pos = BlockPos::new(5, 70, 5);
    alice.link.take();

    s.act(Order::Block {
        pos,
        order: BlockOrder::SetBiome("minecraft:desert".to_owned()),
    });

    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
    let Answer::BlockInfo(info) = s.ask(Question::BlockInfo(pos)) else {
        panic!("a block info");
    };
    assert_eq!(info.biome, "minecraft:desert");
    s.tick(3);
    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::Chunk { chunk, .. } if chunk.pos() == pos.chunk()
    )));
}

#[test]
fn an_unknown_biome_or_a_height_outside_the_world_is_refused() {
    let mut s = scene(&["Alice"]);
    s.act(Order::Block {
        pos: BlockPos::new(5, 70, 5),
        order: BlockOrder::SetBiome("minecraft:nowhere".to_owned()),
    });
    s.act(Order::Block {
        pos: BlockPos::new(5, 9000, 5),
        order: BlockOrder::SetBiome("minecraft:desert".to_owned()),
    });
    assert_eq!(s.warnings().len(), 2);
}

const MANY: &str = "minecraft:adventure/kill_all_mobs";

fn progress(s: &mut Scene, player: &Player) -> (bool, Vec<String>, Vec<String>) {
    match s.ask(Question::AdvancementProgress {
        player: player.id,
        advancement: MANY.to_owned(),
    }) {
        Answer::AdvancementProgress(view) => {
            (view.done, view.criteria_done, view.criteria_remaining)
        }
        other => panic!("expected a progress, got {other:?}"),
    }
}

#[test]
fn one_criterion_is_awarded_and_revoked_without_finishing_the_advancement() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let (_, done, remaining) = progress(&mut s, &alice);
    assert!(done.is_empty());
    assert!(remaining.len() > 1);
    let criterion = remaining[0].clone();

    player_action(
        &mut s,
        PlayerAction::AwardCriterion {
            advancement: MANY.to_owned(),
            criterion: criterion.clone(),
        },
    );
    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
    let (finished, done, left) = progress(&mut s, &alice);
    assert!(!finished);
    assert_eq!(done, vec![criterion.clone()]);
    assert_eq!(left.len(), remaining.len() - 1);

    player_action(
        &mut s,
        PlayerAction::RevokeCriterion {
            advancement: MANY.to_owned(),
            criterion,
        },
    );
    let (_, done, left) = progress(&mut s, &alice);
    assert!(done.is_empty());
    assert_eq!(left.len(), remaining.len());
}

#[test]
fn an_unknown_criterion_or_advancement_is_refused() {
    let mut s = scene(&["Alice"]);
    for (advancement, criterion) in [(MANY, "minecraft:nothing"), ("minecraft:nope/x", "a")] {
        player_action(
            &mut s,
            PlayerAction::AwardCriterion {
                advancement: advancement.to_owned(),
                criterion: criterion.to_owned(),
            },
        );
    }
    assert_eq!(s.warnings().len(), 2);
}

fn spawn_at(s: &mut Scene, kind: &str, position: Vec3) -> rspigot_domain::EntityId {
    let Answer::Spawned(view) = s.ask(Question::Spawn {
        kind: kind.to_owned(),
        position,
    }) else {
        panic!("a {kind}");
    };
    view.id
}

fn removed(events: &[ClientEvent], id: rspigot_domain::EntityId) -> bool {
    events
        .iter()
        .any(|event| matches!(event, ClientEvent::RemoveEntities(ids) if ids.contains(&id)))
}

fn spawned(events: &[ClientEvent], id: rspigot_domain::EntityId) -> bool {
    events
        .iter()
        .any(|event| matches!(event, ClientEvent::SpawnEntity { entity, .. } if *entity == id))
}

#[test]
fn an_entity_is_hidden_from_one_viewer_and_shown_again() {
    let mut s = scene(&["Alice", "Bob"]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    stream_chunk_of(&mut s, BlockPos::new(2, 64, 2));
    let zombie = spawn_at(&mut s, "minecraft:zombie", Vec3::new(2.5, 64.0, 2.5));
    s.tick(3);
    assert!(spawned(&alice.link.take(), zombie));
    assert!(spawned(&bob.link.take(), zombie));

    player_action(&mut s, PlayerAction::HideEntity(zombie));
    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
    assert!(removed(&alice.link.take(), zombie));
    s.tick(5);
    assert!(!spawned(&alice.link.take(), zombie));
    assert!(!removed(&bob.link.take(), zombie));

    player_action(&mut s, PlayerAction::ShowEntity(zombie));
    s.tick(3);
    assert!(spawned(&alice.link.take(), zombie));
}

#[test]
fn hiding_a_players_entity_hides_that_player_and_unloading_shows_everything_again() {
    let mut s = scene(&["Alice", "Bob"]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    stream_chunk_of(&mut s, BlockPos::new(2, 64, 2));
    let zombie = spawn_at(&mut s, "minecraft:zombie", Vec3::new(2.5, 64.0, 2.5));
    s.tick(3);
    let Answer::Player(state) = s.ask(Question::Player(bob.id)) else {
        panic!("a player");
    };
    let bob_entity = rspigot_domain::EntityId(state.player.entity_id);
    alice.link.take();

    player_action(&mut s, PlayerAction::HideEntity(bob_entity));
    player_action(&mut s, PlayerAction::HideEntity(zombie));
    let sent = alice.link.take();
    assert!(removed(&sent, bob_entity));
    assert!(removed(&sent, zombie));

    s.server
        .handle(Inbound::PluginDisconnected { connection: 1 });
    s.tick(3);
    let sent = alice.link.take();
    assert!(spawned(&sent, zombie));
}

#[test]
fn a_player_border_is_sent_to_that_player_only_and_can_be_reset() {
    let mut s = scene(&["Alice", "Bob"]);
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());

    player_action(
        &mut s,
        PlayerAction::SetBorder(BorderOrder::Resize {
            size: 50.0,
            ticks: 0,
        }),
    );
    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
    let sent = alice.link.take();
    assert!(sent.iter().any(|event| matches!(
        event,
        ClientEvent::Border(view) if view.new_size == 50.0
    )));
    assert!(bob.link.take().is_empty());

    player_action(
        &mut s,
        PlayerAction::SetBorder(BorderOrder::SetCenter { x: 10.0, z: 20.0 }),
    );
    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::Border(view) if view.new_size == 50.0 && view.center_x == 10.0 && view.center_z == 20.0
    )));

    player_action(&mut s, PlayerAction::ResetBorder);
    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::Border(view) if view.new_size > 1_000_000.0
    )));
}

#[test]
fn a_player_border_outside_its_limits_is_refused() {
    let mut s = scene(&["Alice"]);
    player_action(
        &mut s,
        PlayerAction::SetBorder(BorderOrder::Resize {
            size: -3.0,
            ticks: 0,
        }),
    );
    assert_eq!(s.warnings().len(), 1);
}

fn ray(origin: Vec3, direction: Vec3) -> RayTraceQuery {
    RayTraceQuery {
        origin,
        direction,
        max_distance: 30.0,
        blocks: true,
        entities: false,
        fluid: FluidMode::Never,
        ignore_passable: false,
        ray_size: 0.0,
        ignore_entity: None,
    }
}

fn traced(s: &mut Scene, query: RayTraceQuery) -> (Option<TargetHit>, Vec3, f64) {
    match lookup(s, LookupQuestion::RayTrace(query)) {
        Answer::Lookup(LookupAnswer::RayHit(hit)) => (hit.hit, hit.position, hit.distance),
        other => panic!("a ray hit expected, got {other:?}"),
    }
}

fn down() -> Vec3 {
    Vec3::new(0.0, -1.0, 0.0)
}

fn block_hit(hit: Option<TargetHit>) -> Option<(BlockPos, String)> {
    match hit {
        Some(TargetHit::Block { pos, kind, .. }) => Some((pos, kind)),
        _ => None,
    }
}

#[test]
fn a_ray_down_hits_the_ground_with_its_face_position_and_distance() {
    let mut s = scene(&["Alice"]);
    let (hit, position, distance) = traced(&mut s, ray(Vec3::new(5.5, 70.0, 5.5), down()));
    assert!(matches!(
        hit,
        Some(TargetHit::Block {
            pos,
            face: rspigot_core::events::Direction::Up,
            ..
        }) if pos == BlockPos::new(5, 63, 5)
    ));
    assert!((distance - 6.0).abs() < 1.0e-6, "{distance}");
    assert!((position.y - 64.0).abs() < 1.0e-6);
}

#[test]
fn a_ray_that_hits_nothing_ends_at_its_maximum_distance() {
    let mut s = scene(&["Alice"]);
    let mut query = ray(Vec3::new(5.5, 70.0, 5.5), Vec3::new(1.0, 0.0, 0.0));
    query.max_distance = 12.0;
    let (hit, position, distance) = traced(&mut s, query);
    assert!(hit.is_none());
    assert_eq!(distance, 12.0);
    assert!((position.x - 17.5).abs() < 1.0e-6);
}

#[test]
fn a_ray_finds_the_nearest_entity_honouring_the_size_and_the_ignored_one() {
    let mut s = scene(&["Alice"]);
    let zombie = spawn_at(&mut s, "minecraft:zombie", Vec3::new(5.5, 64.0, 9.5));
    let along = |size: f64, offset: f64| {
        let mut query = ray(Vec3::new(5.5 + offset, 65.0, 5.5), Vec3::new(0.0, 0.0, 1.0));
        query.blocks = false;
        query.entities = true;
        query.ray_size = size;
        query
    };

    let (hit, _, distance) = traced(&mut s, along(0.0, 0.0));
    assert!(matches!(hit, Some(TargetHit::Entity(view)) if view.id == zombie));
    assert!(distance > 3.0 && distance < 4.0, "{distance}");

    assert!(traced(&mut s, along(0.0, 0.8)).0.is_none());
    assert!(traced(&mut s, along(1.0, 0.8)).0.is_some());

    let mut ignoring = along(0.0, 0.0);
    ignoring.ignore_entity = Some(zombie);
    assert!(traced(&mut s, ignoring).0.is_none());

    let mut blocks_only = along(0.0, 0.0);
    blocks_only.entities = false;
    assert!(traced(&mut s, blocks_only).0.is_none());
}

#[test]
fn fluid_modes_decide_which_water_a_ray_stops_at() {
    let mut s = scene(&["Alice"]);
    let pos = BlockPos::new(5, 64, 5);
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set("minecraft:water".to_owned()),
    });
    let with = |fluid| {
        let mut query = ray(Vec3::new(5.5, 70.0, 5.5), down());
        query.fluid = fluid;
        query
    };

    let (never, _, _) = traced(&mut s, with(FluidMode::Never));
    assert_eq!(
        block_hit(never).map(|(at, _)| at),
        Some(BlockPos::new(5, 63, 5))
    );
    let (source, _, _) = traced(&mut s, with(FluidMode::SourceOnly));
    assert_eq!(block_hit(source).map(|(at, _)| at), Some(pos));
    let (always, _, _) = traced(&mut s, with(FluidMode::Always));
    assert_eq!(block_hit(always).map(|(at, _)| at), Some(pos));
    let mut passing = with(FluidMode::Always);
    passing.ignore_passable = true;
    assert_eq!(
        block_hit(traced(&mut s, passing).0).map(|(at, _)| at),
        Some(pos)
    );
    let mut dry = with(FluidMode::Never);
    dry.ignore_passable = true;
    assert_eq!(
        block_hit(traced(&mut s, dry).0).map(|(at, _)| at),
        Some(BlockPos::new(5, 63, 5))
    );
}

#[test]
fn passable_blocks_stop_a_ray_unless_they_are_ignored() {
    let mut s = scene(&["Alice"]);
    let pos = BlockPos::new(5, 64, 5);
    s.act(Order::Block {
        pos,
        order: BlockOrder::Set("minecraft:short_grass".to_owned()),
    });
    let with = |ignore| {
        let mut query = ray(Vec3::new(5.5, 70.0, 5.5), down());
        query.ignore_passable = ignore;
        query
    };

    let (kept, _, _) = traced(&mut s, with(false));
    assert_eq!(block_hit(kept).map(|(at, _)| at), Some(pos));
    let (skipped, _, _) = traced(&mut s, with(true));
    assert_eq!(
        block_hit(skipped).map(|(at, _)| at),
        Some(BlockPos::new(5, 63, 5))
    );
}

#[test]
fn a_ray_without_a_direction_or_with_a_bad_number_is_refused() {
    let mut s = scene(&["Alice"]);
    for query in [
        ray(Vec3::new(0.0, 70.0, 0.0), Vec3::ZERO),
        ray(Vec3::new(f64::NAN, 70.0, 0.0), down()),
        {
            let mut query = ray(Vec3::new(0.0, 70.0, 0.0), down());
            query.max_distance = -1.0;
            query
        },
    ] {
        assert!(matches!(
            lookup(&mut s, LookupQuestion::RayTrace(query)),
            Answer::Refused(_)
        ));
    }
}

struct Growing;

impl ChunkGenerator for Growing {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        Floor.generate(dimension, pos)
    }

    fn spawn_point(&self, dimension: &Dimension) -> Vec3 {
        Floor.spawn_point(dimension)
    }

    fn place(
        &self,
        dimension: &Dimension,
        request: &PlaceRequest,
    ) -> Result<Vec<PlacedBlock>, PlaceFailure> {
        rspigot_worldgen::WorldGenerator::from_level_type("void", 1, "").place(dimension, request)
    }
}

#[test]
fn a_tree_is_generated_at_a_position_with_the_vanilla_feature() {
    let mut s = scene_with_generator(Growing);
    let at = BlockPos::new(4, 64, 4);

    let answer = lookup(
        &mut s,
        LookupQuestion::GenerateTree {
            pos: at,
            feature: "oak".to_owned(),
        },
    );

    let Answer::Lookup(LookupAnswer::Placed(blocks)) = answer else {
        panic!("a placement, got {answer:?}");
    };
    assert!(blocks > 4, "{blocks}");
    let Answer::Block(state) = s.ask(Question::Block(at)) else {
        panic!("a block");
    };
    assert_eq!(state.kind, "minecraft:oak_log");
    assert!(s.warnings().is_empty());
}

#[test]
fn an_unknown_tree_feature_is_not_found_and_leaves_the_world_alone() {
    let mut s = scene_with_generator(Growing);
    let at = BlockPos::new(4, 64, 4);
    assert_eq!(
        lookup(
            &mut s,
            LookupQuestion::GenerateTree {
                pos: at,
                feature: "no_such_tree".to_owned(),
            }
        ),
        Answer::NotFound
    );
    let Answer::Block(state) = s.ask(Question::Block(at)) else {
        panic!("a block");
    };
    assert_eq!(state.kind, "minecraft:air");
}

const PLUGIN_DATA: &str = "rspigot_plugin_data";

#[derive(Default)]
struct Recorder {
    chunks: Mutex<HashMap<ChunkPos, ChunkColumn>>,
    data: Mutex<HashMap<String, rspigot_nbt::Compound>>,
}

impl rspigot_core::ports::ChunkStore for Recorder {
    fn load(
        &self,
        _: &rspigot_domain::Identifier,
        pos: ChunkPos,
    ) -> Result<Option<ChunkColumn>, rspigot_core::ports::StoreError> {
        Ok(self.chunks.lock().unwrap().get(&pos).cloned())
    }

    fn save(
        &self,
        _: &rspigot_domain::Identifier,
        chunk: &ChunkColumn,
    ) -> Result<(), rspigot_core::ports::StoreError> {
        self.chunks
            .lock()
            .unwrap()
            .insert(chunk.pos(), chunk.clone());
        Ok(())
    }

    fn flush(&self) -> Result<(), rspigot_core::ports::StoreError> {
        Ok(())
    }
}

impl rspigot_core::ports::EntityStore for Recorder {
    fn load_entities(
        &self,
        _: &rspigot_domain::Identifier,
        _: ChunkPos,
    ) -> Result<Option<Vec<rspigot_nbt::Compound>>, rspigot_core::ports::StoreError> {
        Ok(None)
    }

    fn save_entities(
        &self,
        _: &rspigot_domain::Identifier,
        _: ChunkPos,
        _: &[rspigot_nbt::Compound],
    ) -> Result<(), rspigot_core::ports::StoreError> {
        Ok(())
    }

    fn load_world_data(
        &self,
        _: &rspigot_domain::Identifier,
        name: &str,
    ) -> Result<Option<rspigot_nbt::Compound>, rspigot_core::ports::StoreError> {
        Ok(self.data.lock().unwrap().get(name).cloned())
    }

    fn save_world_data(
        &self,
        _: &rspigot_domain::Identifier,
        name: &str,
        data: &rspigot_nbt::Compound,
    ) -> Result<(), rspigot_core::ports::StoreError> {
        self.data
            .lock()
            .unwrap()
            .insert(name.to_owned(), data.clone());
        Ok(())
    }
}

fn recorded_scene(store: &Arc<Recorder>, level: LevelData) -> Scene {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let level = LevelData {
        spawn_mobs: false,
        ..level
    };
    let mut server = Server::new(
        config,
        level,
        Arc::clone(store),
        Arc::new(Floor),
        StatusBoard::new(20),
    );
    server.wait_for_spawn(std::time::Duration::from_secs(10));
    scene_on(server, &["Alice"], Vec::new())
}

fn pdc_of(
    s: &mut Scene,
    holder: PdcHolder,
) -> Option<std::collections::BTreeMap<String, PdcValue>> {
    match s.ask(Question::Object(ObjectQuestion::Pdc(holder))) {
        Answer::Object(ObjectAnswer::Pdc(entries)) => Some(entries),
        _ => None,
    }
}

fn set_pdc(key: &str, value: PdcValue) -> PdcChange {
    PdcChange::Set {
        key: key.to_owned(),
        value,
    }
}

fn wait_for(check: impl Fn() -> bool) {
    for _ in 0..100 {
        if check() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    panic!("the store did not receive the data");
}

#[test]
fn a_chunk_holds_plugin_data_that_is_saved_with_it_and_read_after_a_restart() {
    let store = Arc::new(Recorder::default());
    let mut s = recorded_scene(&store, LevelData::default());
    let chunk = ChunkPos::new(0, 0);

    s.act(Order::World(WorldOrder::Object(
        WorldObjectOrder::ChunkPdc {
            chunk,
            change: set_pdc("plugin:level", PdcValue::Int(7)),
        },
    )));
    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
    let held = pdc_of(&mut s, PdcHolder::Chunk(chunk)).expect("the chunk data");
    assert_eq!(held.get("plugin:level"), Some(&PdcValue::Int(7)));

    s.act(Order::World(WorldOrder::Save));
    wait_for(|| {
        store
            .chunks
            .lock()
            .unwrap()
            .get(&chunk)
            .is_some_and(|saved| {
                saved
                    .extra
                    .get_compound("ChunkBukkitValues")
                    .is_some_and(|values| values.contains_key("plugin:level"))
            })
    });

    let mut again = recorded_scene(&store, LevelData::default());
    let held = pdc_of(&mut again, PdcHolder::Chunk(chunk)).expect("the chunk data");
    assert_eq!(held.get("plugin:level"), Some(&PdcValue::Int(7)));

    again.act(Order::World(WorldOrder::Object(
        WorldObjectOrder::ChunkPdc {
            chunk,
            change: PdcChange::Remove("plugin:level".to_owned()),
        },
    )));
    assert!(
        pdc_of(&mut again, PdcHolder::Chunk(chunk))
            .expect("the chunk data")
            .is_empty()
    );
}

#[test]
fn chunk_data_on_an_unloaded_chunk_is_refused() {
    let store = Arc::new(Recorder::default());
    let mut s = recorded_scene(&store, LevelData::default());
    s.act(Order::World(WorldOrder::Object(
        WorldObjectOrder::ChunkPdc {
            chunk: ChunkPos::new(900, 900),
            change: set_pdc("plugin:level", PdcValue::Int(7)),
        },
    )));
    assert_eq!(s.warnings().len(), 1);
    assert!(pdc_of(&mut s, PdcHolder::Chunk(ChunkPos::new(900, 900))).is_none());
}

#[test]
fn a_world_holds_plugin_data_saved_with_the_level_and_restored_from_it() {
    let store = Arc::new(Recorder::default());
    let mut s = recorded_scene(&store, LevelData::default());

    s.act(Order::World(WorldOrder::Object(
        WorldObjectOrder::WorldPdc(set_pdc("plugin:mode", PdcValue::Text("arena".to_owned()))),
    )));
    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
    s.act(Order::World(WorldOrder::Save));
    wait_for(|| store.data.lock().unwrap().contains_key(PLUGIN_DATA));

    let saved = store.data.lock().unwrap().get(PLUGIN_DATA).cloned();
    let level = LevelData {
        plugin_data: saved,
        ..LevelData::default()
    };
    let mut again = recorded_scene(&store, level);
    let held = pdc_of(&mut again, PdcHolder::World).expect("the world data");
    assert_eq!(
        held.get("plugin:mode"),
        Some(&PdcValue::Text("arena".to_owned()))
    );
}

#[test]
fn a_cookie_is_stored_on_the_client_and_a_bad_key_or_size_is_refused() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();

    player_action(
        &mut s,
        PlayerAction::StoreCookie {
            key: "plugin:session".to_owned(),
            payload: vec![1, 2, 3],
        },
    );
    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
    assert!(alice.link.take().iter().any(|event| matches!(
        event,
        ClientEvent::StoreCookie { key, payload } if key == "plugin:session" && payload == &[1, 2, 3]
    )));

    player_action(
        &mut s,
        PlayerAction::StoreCookie {
            key: "NOT A KEY".to_owned(),
            payload: Vec::new(),
        },
    );
    player_action(
        &mut s,
        PlayerAction::StoreCookie {
            key: "plugin:big".to_owned(),
            payload: vec![0; 6000],
        },
    );
    assert_eq!(s.warnings().len(), 2);
}

fn ask_later(s: &mut Scene, request: u64, question: Question) {
    s.server.handle(Inbound::PluginMessage {
        connection: 1,
        message: Box::new(FromPlugin::Question { request, question }),
    });
    s.server.deliver();
}

fn answer_to(s: &Scene, request: u64) -> Option<Answer> {
    s.plugin
        .received()
        .into_iter()
        .find_map(|message| match message {
            ToPlugin::Answer {
                request: answered,
                answer,
            } if answered == request => Some(answer),
            _ => None,
        })
}

#[test]
fn a_cookie_is_retrieved_once_the_client_answers_and_nothing_is_said_before() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();

    ask_later(
        &mut s,
        500,
        Question::Extra(ExtraQuestion::RetrieveCookie {
            player: alice.id,
            key: "plugin:session".to_owned(),
        }),
    );
    assert!(answer_to(&s, 500).is_none());
    assert!(
        alice.link.take().iter().any(
            |event| matches!(event, ClientEvent::CookieRequest(key) if key == "plugin:session")
        )
    );

    s.server.handle(Inbound::Input {
        player: alice.id,
        input: PlayerInput::CookieResponse {
            key: "plugin:session".to_owned(),
            payload: Some(vec![9, 8]),
        },
    });
    s.server.deliver();

    assert_eq!(
        answer_to(&s, 500),
        Some(Answer::Extra(ExtraAnswer::Cookie(Some(vec![9, 8]))))
    );
}

#[test]
fn a_missing_cookie_answers_empty_and_a_silent_client_gets_a_refusal_after_thirty_seconds() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let retrieve = |key: &str| {
        Question::Extra(ExtraQuestion::RetrieveCookie {
            player: alice.id,
            key: key.to_owned(),
        })
    };

    ask_later(&mut s, 1, retrieve("plugin:none"));
    s.server.handle(Inbound::Input {
        player: alice.id,
        input: PlayerInput::CookieResponse {
            key: "plugin:none".to_owned(),
            payload: None,
        },
    });
    s.server.deliver();
    assert_eq!(
        answer_to(&s, 1),
        Some(Answer::Extra(ExtraAnswer::Cookie(None)))
    );

    ask_later(&mut s, 2, retrieve("plugin:silent"));
    s.tick(599);
    assert!(answer_to(&s, 2).is_none());
    s.tick(2);
    assert!(matches!(answer_to(&s, 2), Some(Answer::Refused(_))));

    ask_later(&mut s, 3, retrieve("BAD KEY"));
    assert!(matches!(answer_to(&s, 3), Some(Answer::Refused(_))));
}

#[test]
fn server_links_are_sent_to_the_player() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();

    player_action(
        &mut s,
        PlayerAction::SendLinks(vec![
            (
                "bug_report".to_owned(),
                "https://example.org/bugs".to_owned(),
            ),
            ("Our shop".to_owned(), "https://example.org/shop".to_owned()),
        ]),
    );

    assert!(s.warnings().is_empty(), "{:?}", s.warnings());
    let links = alice
        .link
        .take()
        .into_iter()
        .find_map(|event| match event {
            ClientEvent::ServerLinks(links) => Some(links),
            _ => None,
        })
        .expect("server links");
    assert_eq!(links.len(), 2);
    assert_eq!(links[1].url, "https://example.org/shop");
}

fn profile_of(s: &mut Scene, lookup_by: ProfileLookup) -> Answer {
    lookup(s, LookupQuestion::Profile(lookup_by))
}

#[test]
fn a_profile_comes_from_the_online_player_with_its_textures_or_from_the_cache() {
    let cached = rspigot_domain::player::PlayerId::offline("Gone");
    let server = configured(|config| {
        config.user_cache = rspigot_core::user_cache::UserCache::new(
            vec![rspigot_core::user_cache::CachedProfile {
                name: "Gone".to_owned(),
                uuid: cached.0,
                expires: std::time::SystemTime::now() + std::time::Duration::from_secs(86_400),
            }],
            10,
        );
    });
    let mut s = scene_on(server, &[], Vec::new());
    let textured = rspigot_domain::player::PlayerId::offline("Skinny");
    s.server.handle(Inbound::Join {
        profile: rspigot_domain::player::GameProfile {
            id: textured,
            name: "Skinny".to_owned(),
            properties: vec![rspigot_domain::player::ProfileProperty {
                name: "textures".to_owned(),
                value: "VALUE".to_owned(),
                signature: Some("SIG".to_owned()),
            }],
        },
        settings: rspigot_domain::player::ClientSettings::default(),
        link: Link::default(),
        address: std::net::IpAddr::from([127, 0, 0, 1]),
        session: 9_000,
    });
    s.server.deliver();

    let Answer::Lookup(LookupAnswer::Profile(info)) =
        profile_of(&mut s, ProfileLookup::Name("skinny".to_owned()))
    else {
        panic!("a profile");
    };
    assert_eq!(info.name, "Skinny");
    assert_eq!(info.uuid, textured.0);
    assert!(info.online);
    assert_eq!(
        info.textures,
        Some(("VALUE".to_owned(), Some("SIG".to_owned())))
    );

    let Answer::Lookup(LookupAnswer::Profile(info)) =
        profile_of(&mut s, ProfileLookup::Id(cached.0))
    else {
        panic!("a cached profile");
    };
    assert_eq!(
        (info.name.as_str(), info.online, info.textures),
        ("Gone", false, None)
    );

    assert_eq!(
        profile_of(&mut s, ProfileLookup::Name("Nobody".to_owned())),
        Answer::NotFound
    );
}

fn add_recipe(s: &mut Scene, id: &str, shape: RecipeShape) -> Answer {
    let request = 7_000 + RECIPES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    common::order(
        &mut s.server,
        1,
        FromPlugin::Action {
            request,
            order: recipe(id, shape),
        },
    );
    answer_to(s, request).expect("an answer")
}

static RECIPES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn recipe(id: &str, shape: RecipeShape) -> Order {
    server_order(ServerOrder::AddRecipe(Box::new(RecipeSpec {
        id: id.to_owned(),
        shape,
        result: ItemSpec::of("minecraft:emerald", 1),
        group: String::new(),
        category: rspigot_domain::plugin_recipes::Category::Misc,
        override_vanilla: false,
        auto_discover: false,
    })))
}

fn kind_of_recipe(s: &mut Scene, id: &str) -> Option<RecipeKind> {
    match s.ask(Question::Recipes) {
        Answer::Recipes(views) => views
            .into_iter()
            .find(|view| view.id == id)
            .map(|view| view.kind),
        other => panic!("recipes expected, got {other:?}"),
    }
}

fn items(keys: &[&str]) -> IngredientSpec {
    IngredientSpec::Items(keys.iter().map(|key| (*key).to_owned()).collect())
}

fn exact(key: &str, name: &str) -> IngredientSpec {
    let mut item = ItemSpec::of(key, 1);
    item.name = Some(name.to_owned());
    IngredientSpec::Exact(item)
}

#[test]
fn exact_trim_and_transmute_recipes_are_registered_and_listed_by_kind() {
    let mut s = scene(&["Alice"]);
    let accepted = [
        add_recipe(
            &mut s,
            "plugin:exact",
            RecipeShape::Shapeless(vec![
                exact("minecraft:stick", "Magic"),
                items(&["minecraft:diamond"]),
            ]),
        ),
        add_recipe(
            &mut s,
            "plugin:trim",
            RecipeShape::SmithingTrim {
                template: items(&["minecraft:netherite_upgrade_smithing_template"]),
                base: items(&["minecraft:diamond_chestplate"]),
                addition: items(&["minecraft:emerald"]),
                pattern: "minecraft:coast".to_owned(),
            },
        ),
        add_recipe(
            &mut s,
            "plugin:transmute",
            RecipeShape::Transmute {
                input: items(&["minecraft:diamond_sword"]),
                material: items(&["minecraft:emerald"]),
            },
        ),
    ];
    assert!(
        accepted.iter().all(|answer| *answer == Answer::Done),
        "{accepted:?}"
    );
    assert_eq!(
        kind_of_recipe(&mut s, "plugin:exact"),
        Some(RecipeKind::Shapeless)
    );
    assert_eq!(
        kind_of_recipe(&mut s, "plugin:trim"),
        Some(RecipeKind::Smithing)
    );
    assert_eq!(
        kind_of_recipe(&mut s, "plugin:transmute"),
        Some(RecipeKind::Special)
    );
}

#[test]
fn a_bad_pattern_an_exact_stonecutting_input_or_a_missing_item_is_refused() {
    let mut s = scene(&["Alice"]);
    let refused = [
        add_recipe(
            &mut s,
            "plugin:bad_trim",
            RecipeShape::SmithingTrim {
                template: items(&["minecraft:stick"]),
                base: items(&["minecraft:stick"]),
                addition: items(&["minecraft:stick"]),
                pattern: "Not A Pattern".to_owned(),
            },
        ),
        add_recipe(
            &mut s,
            "plugin:cut",
            RecipeShape::Stonecutting(exact("minecraft:stone", "x")),
        ),
        add_recipe(
            &mut s,
            "plugin:ghost",
            RecipeShape::Shapeless(vec![exact("minecraft:nothing_here", "x")]),
        ),
    ];
    assert!(
        refused
            .iter()
            .all(|answer| matches!(answer, Answer::Failed(_))),
        "{refused:?}"
    );
}
