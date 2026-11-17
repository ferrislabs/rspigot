mod fake_host;

use std::sync::Mutex;
use std::time::Duration;

use fake_host::*;
use rspigot_plugin::{
    ActionErrorKind, ClickKind, CustomInventory, Error, InventoryClick, InventoryClose,
    InventoryOwner, InventoryTypeSlotType, ItemStack, Material, Plugin, Uuid, listeners, run_with,
};
use rspigot_plugin_protocol::v1::{
    self, answer, custom_inventory_order, event, from_plugin, inventory_ref, patch, player_order,
    question, server_order, to_plugin,
};
use tonic::Status;

static OLD: Mutex<Option<CustomInventory>> = Mutex::new(None);

#[derive(Plugin)]
#[plugin(name = "clearer", version = "1.0.0")]
struct Clearer;

#[listeners]
impl Clearer {
    async fn refuse_the_click(&self, event: &mut InventoryClick) {
        event.cancel();
    }

    async fn clear_the_slot(&self, event: &mut InventoryClick) {
        event.set_current_item(None);
    }
}

fn creation(body: from_plugin::Body) -> v1::CreateInventory {
    match body {
        from_plugin::Body::Server(v1::ServerOrder {
            order: Some(server_order::Order::CreateInventory(created)),
        }) => created,
        body => panic!("expected an inventory to be created, heard {body:?}"),
    }
}

fn stocked(inventory: u64, order: custom_inventory_order::Order) -> from_plugin::Body {
    from_plugin::Body::CustomInventory(v1::CustomInventoryOrder {
        inventory,
        order: Some(order),
    })
}

fn ordered(order: player_order::Order) -> from_plugin::Body {
    from_plugin::Body::Player(v1::PlayerOrder {
        player: STEVE.to_owned(),
        order: Some(order),
    })
}

fn item(key: &str, count: i32) -> v1::Item {
    v1::Item {
        key: key.to_owned(),
        count,
        ..v1::Item::default()
    }
}

fn custom(id: u64) -> v1::InventoryRef {
    custom_of("caller", id)
}

fn custom_of(plugin: &str, id: u64) -> v1::InventoryRef {
    v1::InventoryRef {
        owner: Some(inventory_ref::Owner::Custom(id)),
        size: 54,
        kind: "CHEST".to_owned(),
        plugin: plugin.to_owned(),
        id: 0,
    }
}

fn own() -> v1::InventoryRef {
    v1::InventoryRef {
        owner: Some(inventory_ref::Owner::Player(STEVE.to_owned())),
        size: 41,
        kind: "PLAYER".to_owned(),
        plugin: String::new(),
        id: 0,
    }
}

fn view(id: u64) -> v1::InventoryView {
    v1::InventoryView {
        top: Some(custom(id)),
        bottom: Some(own()),
        title: "Menu".to_owned(),
        player: Some(steve()),
    }
}

fn click(
    clicked: Option<v1::InventoryRef>,
    slot: i32,
    kind: v1::ClickKind,
    hotbar_button: i32,
    item: Option<v1::Item>,
) -> event::Payload {
    event::Payload::InventoryClick(v1::InventoryClick {
        action: v1::InventoryAction::Unknown as i32,
        click: kind as i32,
        clicked_inventory: clicked,
        current_item: item,
        cursor: None,
        hotbar_button,
        inventory: Some(custom(7)),
        left_click: kind == v1::ClickKind::Left,
        raw_slot: slot,
        result: v1::EventResult::Default as i32,
        right_click: false,
        shift_click: false,
        slot,
        slot_type: v1::InventoryTypeSlotType::Container as i32,
        view: Some(view(7)),
        viewers: vec![steve()],
        who_clicked: Some(steve()),
    })
}

fn close(inventory: u64, by_player: bool) -> event::Payload {
    event::Payload::InventoryClose(v1::InventoryClose {
        by_player,
        inventory: Some(custom(inventory)),
        player: Some(steve()),
        view: Some(view(inventory)),
        viewers: Vec::new(),
    })
}

fn custom_id(click: &Clicked) -> Option<u64> {
    click.inventory.custom().map(CustomInventory::id)
}

fn settled(id: u64, handler: u32, payload: event::Payload) -> to_plugin::Body {
    event(id, &[handler], v1::Phase::Settled, false, payload)
}

async fn clicked(marks: &mut Marks) -> Clicked {
    match marks.next().await {
        Mark::Clicked(click) => click,
        mark => panic!("expected a click, got {mark:?}"),
    }
}

async fn closed(marks: &mut Marks) -> InventoryClose {
    match marks.next().await {
        Mark::Closed(close) => close,
        mark => panic!("expected a close, got {mark:?}"),
    }
}

fn wire(id: u32, event: v1::EventKind) -> v1::Handler {
    v1::Handler {
        id,
        event: event as i32,
        access: v1::Access::Observe as i32,
        priority: v1::Priority::Normal as i32,
        ignore_cancelled: false,
    }
}

#[tokio::test]
async fn creating_and_opening_an_inventory_wait_for_the_result() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let server = player.server();
            let menu = server.create_inventory(1, "§6Menu").await.unwrap();
            let storage = server.create_storage(2, "Box").await.unwrap();
            assert_ne!(menu.id(), storage.id());
            menu.set(4, Some(ItemStack::new(Material::Compass, 1)));
            player.inventory.open(&menu).await.unwrap();
            let full = server.create_inventory(7, "Big").await;
            let Err(Error::Action(error)) = full else {
                panic!("expected a refused creation, got {full:?}");
            };
            assert_eq!(error.kind, ActionErrorKind::InvalidArgument);
        })
    })
    .await;

    let session = &mut called.session;
    let (request, heard) = session.acted().await;
    let menu = creation(heard);
    session.say(answer(request, answer::Body::Done(v1::Empty {})));
    assert_ne!(menu.id, 0);
    assert_eq!(
        (menu.rows, menu.title.as_str(), menu.locked),
        (1, "§6Menu", true)
    );
    let (request, heard) = session.acted().await;
    let storage = creation(heard);
    session.say(answer(request, answer::Body::Done(v1::Empty {})));
    assert_eq!(
        (storage.rows, storage.title.as_str(), storage.locked),
        (2, "Box", false)
    );
    assert_ne!(storage.id, menu.id);
    assert_eq!(
        session.hear().await,
        stocked(
            menu.id,
            custom_inventory_order::Order::Set(v1::SetSlot {
                slot: 4,
                item: Some(item("minecraft:compass", 1)),
            })
        )
    );
    session
        .performed(ordered(player_order::Order::OpenInventory(menu.id)))
        .await;
    let (request, heard) = session.acted().await;
    assert_eq!(creation(heard).rows, 7);
    session.say(answer(
        request,
        answer::Body::Failed(v1::ActionError {
            kind: v1::ActionErrorKind::InvalidArgument as i32,
            reason: "rows 7 is outside 1..=6".to_owned(),
        }),
    ));
    called.finished().await;
}

#[tokio::test]
async fn inventory_orders_and_resolve_use_menu_slots() {
    let mut called = calling(|player| {
        Box::pin(async move {
            let menu = player.server().create_inventory(6, "Shop").await.unwrap();
            menu.set(53, None);
            menu.clear();
            let state = menu.get_state().await.unwrap();
            assert_eq!(
                state.items,
                [None, Some(ItemStack::new(Material::Diamond, 3))]
            );
            let viewers: Vec<&str> = state
                .viewers
                .iter()
                .map(|viewer| viewer.name.as_str())
                .collect();
            assert_eq!(viewers, ["Steve", "Alex"]);
            let shuffled = menu.get_state().await;
            assert!(matches!(shuffled, Err(Error::Malformed(_))), "{shuffled:?}");
            let gone = menu.get_state().await;
            assert!(matches!(gone, Err(Error::NotFound)), "{gone:?}");
            menu.close();
            menu.discard();
        })
    })
    .await;

    let session = &mut called.session;
    let (request, heard) = session.acted().await;
    let menu = creation(heard).id;
    session.say(answer(request, answer::Body::Done(v1::Empty {})));
    for order in [
        custom_inventory_order::Order::Set(v1::SetSlot {
            slot: 53,
            item: None,
        }),
        custom_inventory_order::Order::Clear(v1::Empty {}),
    ] {
        assert_eq!(session.hear().await, stocked(menu, order));
    }
    let slot = |slot, item| v1::Slot { slot, item };
    for body in [
        answer::Body::CustomInventory(v1::CustomInventoryState {
            slots: vec![slot(0, None), slot(1, Some(item("minecraft:diamond", 3)))],
            viewers: vec![steve(), alex()],
        }),
        answer::Body::CustomInventory(v1::CustomInventoryState {
            slots: vec![slot(1, None), slot(0, None)],
            viewers: Vec::new(),
        }),
        answer::Body::NotFound(v1::NotFound {}),
    ] {
        let (request, asked) = session.asked().await;
        assert_eq!(asked, question::Question::CustomInventory(menu));
        session.say(answer(request, body));
    }
    for order in [
        custom_inventory_order::Order::Close(v1::Empty {}),
        custom_inventory_order::Order::Discard(v1::Empty {}),
    ] {
        assert_eq!(session.hear().await, stocked(menu, order));
    }
    called.finished().await;
}

#[tokio::test]
async fn inventory_events_reach_their_handlers() {
    let mut called = calling(|_| Box::pin(async {})).await;
    called.finished().await;

    called.session.say(settled(
        2,
        2,
        click(
            Some(own()),
            39,
            v1::ClickKind::NumberKey,
            3,
            Some(item("minecraft:diamond_helmet", 1)),
        ),
    ));
    let helmet = clicked(&mut called.marks).await;
    assert_eq!(helmet.id, 2);
    assert_eq!(helmet.who, "Steve");
    assert_eq!(custom_id(&helmet), Some(7));
    assert!(matches!(
        helmet.clicked.as_ref().map(|inventory| &inventory.owner),
        Some(InventoryOwner::Player(uuid)) if *uuid == Uuid::from_u128(1)
    ));
    assert_eq!((helmet.slot, helmet.raw_slot), (39, 39));
    assert_eq!(helmet.slot_type, InventoryTypeSlotType::Container);
    assert_eq!(helmet.click, ClickKind::NumberKey);
    assert_eq!(helmet.hotbar_button, 3);
    assert_eq!(
        helmet.item.map(|item| item.material),
        Some(Material::DiamondHelmet)
    );
    assert_eq!(helmet.title, "Menu");
    assert_eq!(helmet.viewers, 1);
    if let Some(menu) = helmet.inventory.custom() {
        menu.close();
    }
    assert_eq!(
        called.session.hear().await,
        stocked(7, custom_inventory_order::Order::Close(v1::Empty {}))
    );

    for (kind, hotbar_button, expected) in [
        (v1::ClickKind::Left, -1, ClickKind::Left),
        (v1::ClickKind::Right, -1, ClickKind::Right),
        (v1::ClickKind::ShiftLeft, -1, ClickKind::ShiftLeft),
        (v1::ClickKind::ShiftRight, -1, ClickKind::ShiftRight),
        (v1::ClickKind::NumberKey, 8, ClickKind::NumberKey),
        (v1::ClickKind::SwapOffhand, -1, ClickKind::SwapOffhand),
        (v1::ClickKind::Drop, -1, ClickKind::Drop),
        (v1::ClickKind::ControlDrop, -1, ClickKind::ControlDrop),
        (v1::ClickKind::DoubleClick, -1, ClickKind::DoubleClick),
        (v1::ClickKind::Middle, -1, ClickKind::Middle),
        (v1::ClickKind::Creative, -1, ClickKind::Creative),
    ] {
        called.session.say(settled(
            4,
            2,
            click(Some(custom(7)), 4, kind, hotbar_button, None),
        ));
        let click = clicked(&mut called.marks).await;
        assert_eq!(
            (click.slot, click.click, click.hotbar_button),
            (4, expected, hotbar_button)
        );
        assert!(click.item.is_none());
    }
    called.session.say(settled(
        5,
        2,
        click(None, -999, v1::ClickKind::Left, -1, None),
    ));
    assert!(clicked(&mut called.marks).await.clicked.is_none());

    let mut without_inventory = click(Some(custom(7)), 4, v1::ClickKind::Left, -1, None);
    if let event::Payload::InventoryClick(click) = &mut without_inventory {
        click.inventory = None;
    }
    for unreadable in [
        click(Some(custom(7)), 4, v1::ClickKind::Unspecified, -1, None),
        without_inventory,
        click(
            None,
            -999,
            v1::ClickKind::Left,
            -1,
            Some(item("minecraft:nothing", 1)),
        ),
    ] {
        called.session.say(settled(20, 2, unreadable));
    }

    called.session.say(settled(30, 3, close(7, true)));
    let by_player = closed(&mut called.marks).await;
    assert_eq!(by_player.id(), 30);
    assert_eq!(by_player.player().name, "Steve");
    assert_eq!(
        by_player.inventory().custom().map(CustomInventory::id),
        Some(7)
    );
    assert!(by_player.by_player());
    called.session.say(settled(31, 3, close(8, false)));
    let by_server = closed(&mut called.marks).await;
    assert_eq!(
        by_server.inventory().custom().map(CustomInventory::id),
        Some(8)
    );
    assert!(!by_server.by_player());
    assert!(called.marks.left().is_empty());
}

#[tokio::test]
async fn a_click_is_cancelled_by_its_result_only() {
    let mut called = calling(|_| Box::pin(async {})).await;
    called.finished().await;

    for (result, flagged, cancelled) in [
        (v1::EventResult::Deny, false, true),
        (v1::EventResult::Default, true, false),
        (v1::EventResult::Allow, false, false),
    ] {
        let mut payload = click(Some(custom(7)), 4, v1::ClickKind::Left, -1, None);
        if let event::Payload::InventoryClick(click) = &mut payload {
            click.result = result as i32;
        }
        called
            .session
            .say(event(6, &[2], v1::Phase::Settled, flagged, payload));
        assert_eq!(
            clicked(&mut called.marks).await.cancelled,
            cancelled,
            "{result:?}"
        );
    }
}

#[tokio::test]
async fn on_enable_runs_before_events_on_every_connection_and_on_disable_when_it_ends() {
    let mut host = Host::start().await;
    let (mut marks, ended) = start_caller(
        &host,
        |_| Box::pin(async {}),
        |_| Box::pin(tokio::time::sleep(Duration::from_millis(50))),
    );
    let knock = host.knock().await;
    assert_eq!(
        knock.hello.handlers,
        [
            wire(1, v1::EventKind::PlayerJoin),
            wire(2, v1::EventKind::InventoryClick),
            wire(3, v1::EventKind::InventoryClose),
        ]
    );
    let first = knock.accept();
    first.say(join(1));
    marks.expect(|mark| matches!(mark, Mark::Enabled)).await;
    marks.expect(|mark| matches!(mark, Mark::Finished)).await;
    first.end(Status::unavailable("the server is stopping"));
    marks.expect(|mark| matches!(mark, Mark::Disabled)).await;

    let second = host.knock().await.accept();
    second.say(join(2));
    marks.expect(|mark| matches!(mark, Mark::Enabled)).await;
    marks.expect(|mark| matches!(mark, Mark::Finished)).await;
    second.say(to_plugin::Body::Replaced(v1::Replaced {}));

    let ended = within(ended).await.unwrap();
    assert!(matches!(ended, Err(Error::Replaced)), "got {ended:?}");
    let left = marks.left();
    assert!(
        matches!(left.as_slice(), [Mark::Disabled]),
        "expected the last on_disable before the plugin stopped, got {left:?}"
    );
}

#[tokio::test]
async fn an_order_on_an_inventory_from_an_earlier_connection_is_dropped() {
    let mut host = Host::start().await;
    let (mut marks, _) = start_caller(
        &host,
        |player| {
            Box::pin(async move {
                let old = OLD.lock().unwrap().clone().expect("a stored inventory");
                old.set(0, None);
                let opened = player.inventory.open(&old).await;
                assert!(matches!(opened, Err(Error::Disconnected)), "{opened:?}");
                let resolved = old.get_state().await;
                assert!(matches!(resolved, Err(Error::NotFound)), "{resolved:?}");
                player.message.send("marker");
            })
        },
        |server| {
            Box::pin(async move {
                let stored = OLD.lock().unwrap().is_some();
                if !stored {
                    let made = server.create_inventory(1, "M").await.unwrap();
                    *OLD.lock().unwrap() = Some(made);
                }
            })
        },
    );
    let mut first = host.knock().await.accept();
    let (request, heard) = first.acted().await;
    let created = creation(heard);
    first.say(answer(request, answer::Body::Done(v1::Empty {})));
    assert_eq!(
        (created.rows, created.title.as_str(), created.locked),
        (1, "M", true)
    );
    marks.expect(|mark| matches!(mark, Mark::Enabled)).await;
    first.end(Status::unavailable("the server is stopping"));
    marks.expect(|mark| matches!(mark, Mark::Disabled)).await;

    let mut second = host.knock().await.accept();
    marks.expect(|mark| matches!(mark, Mark::Enabled)).await;
    second.say(join(2));
    marks.expect(|mark| matches!(mark, Mark::Finished)).await;
    assert_eq!(
        second.hear().await,
        ordered(player_order::Order::SendMessage("marker".to_owned()))
    );
}

#[tokio::test]
async fn every_deciding_handler_of_a_click_runs_after_a_cancel() {
    let mut host = Host::start().await;
    let _plugin = tokio::spawn(run_with(Clearer, host.url.clone(), None));
    let mut session = host.knock().await.accept();

    let diamond = Some(item("minecraft:diamond", 1));
    session.say(event(
        3,
        &[1, 2],
        v1::Phase::Decide,
        false,
        click(Some(custom(7)), 4, v1::ClickKind::Left, -1, diamond),
    ));
    let denied = v1::InventoryClickPatch {
        result: Some(v1::EventResult::Deny as i32),
        ..v1::InventoryClickPatch::default()
    };
    let cleared = v1::InventoryClickPatch {
        current_item: Some(v1::NullableItem { value: None }),
        ..v1::InventoryClickPatch::default()
    };
    assert_eq!(
        session.hear().await,
        from_plugin::Body::Decision(v1::Decision {
            event: 3,
            verdicts: vec![
                v1::Verdict {
                    handler: 1,
                    cancel: false,
                    patch: Some(v1::Patch {
                        change: Some(patch::Change::InventoryClick(denied)),
                    }),
                },
                v1::Verdict {
                    handler: 2,
                    cancel: false,
                    patch: Some(v1::Patch {
                        change: Some(patch::Change::InventoryClick(cleared)),
                    }),
                },
            ],
        })
    );
}

#[tokio::test]
async fn an_inventory_of_another_plugin_never_equals_mine_and_takes_no_order() {
    let mut called = calling(|_| Box::pin(async {})).await;
    called.finished().await;

    called.session.say(settled(
        2,
        2,
        click(Some(custom(7)), 4, v1::ClickKind::Left, -1, None),
    ));
    let mine = clicked(&mut called.marks).await;
    let mut foreign = click(Some(custom_of("shop", 7)), 4, v1::ClickKind::Left, -1, None);
    if let event::Payload::InventoryClick(click) = &mut foreign {
        click.inventory = Some(custom_of("shop", 7));
    }
    called.session.say(settled(3, 2, foreign));
    let theirs = clicked(&mut called.marks).await;
    let (Some(mine), Some(theirs)) = (mine.inventory.custom(), theirs.inventory.custom()) else {
        panic!("both clicks are in a plugin inventory");
    };
    assert_eq!(mine.id(), theirs.id());
    assert_eq!(theirs.plugin(), "shop");
    assert!(mine.is_mine());
    assert!(!theirs.is_mine());
    assert_ne!(mine, theirs);
    assert_eq!(mine, &mine.clone());
    theirs.close();
    mine.close();
    assert_eq!(
        called.session.hear().await,
        stocked(7, custom_inventory_order::Order::Close(v1::Empty {}))
    );
    assert!(matches!(theirs.get_state().await, Err(Error::NotFound)));
}
