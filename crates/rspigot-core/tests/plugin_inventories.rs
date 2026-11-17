mod common;

use std::num::NonZeroU8;

use common::*;
use rspigot_core::events::{
    ClickInput, ClientEvent, ContainerClick, Direction, Hand, HashedSlot, Inbound, PlayerInput,
};
use rspigot_core::plugins::events::{self, InventoryClick, InventoryClose};
use rspigot_core::plugins::{
    Answer, BlockOrder, CustomInventoryOrder, CustomInventoryState, EventKind, EventPayload,
    FromPlugin, InventoryOrder, InventoryOwner, InventoryRef, InventoryState, ItemSpec,
    MAX_PLUGIN_INVENTORIES, Order, PlayerOrder, Question, ServerOrder, ToPlugin,
};
use rspigot_domain::item::{ItemStack, Slot};
use rspigot_domain::text::Component;
use rspigot_domain::{BlockPos, Vec3};
use rspigot_items::ItemKind;

const MENU: u64 = 1;
const BOX: u64 = 2;
const MENU_SIZE: usize = 9;

struct Screen {
    window: i32,
    state: i32,
}

impl Screen {
    fn follow(&mut self, events: &[ClientEvent]) {
        for event in events {
            match event {
                ClientEvent::ContainerContent {
                    window, state_id, ..
                }
                | ClientEvent::ContainerSlot {
                    window, state_id, ..
                } if *window == self.window => self.state = *state_id,
                _ => {}
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClickedSlot {
    Menu(u16),
    Player(i32),
    Outside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClickKind {
    Left,
    Right,
    ShiftLeft,
    ShiftRight,
    NumberKey(u8),
    SwapOffhand,
    Drop,
    ControlDrop,
    DoubleClick,
    Middle,
}

fn custom_id(inventory: &InventoryRef) -> u64 {
    match &inventory.owner {
        InventoryOwner::Custom { id, .. } => *id,
        other => panic!("expected a plugin inventory, got {other:?}"),
    }
}

fn clicked(click: &InventoryClick) -> (ClickedSlot, ClickKind) {
    let slot = match click
        .clicked_inventory
        .as_ref()
        .map(|inventory| &inventory.owner)
    {
        Some(InventoryOwner::Custom { .. }) => {
            ClickedSlot::Menu(u16::try_from(click.slot).expect("a menu slot"))
        }
        Some(InventoryOwner::Player(_)) => ClickedSlot::Player(click.slot),
        None => ClickedSlot::Outside,
        Some(other) => panic!("unexpected clicked inventory {other:?}"),
    };
    let kind = match click.click {
        events::ClickKind::Left => ClickKind::Left,
        events::ClickKind::Right => ClickKind::Right,
        events::ClickKind::ShiftLeft => ClickKind::ShiftLeft,
        events::ClickKind::ShiftRight => ClickKind::ShiftRight,
        events::ClickKind::NumberKey => {
            ClickKind::NumberKey(u8::try_from(click.hotbar_button).expect("a hotbar key"))
        }
        events::ClickKind::SwapOffhand => ClickKind::SwapOffhand,
        events::ClickKind::Drop => ClickKind::Drop,
        events::ClickKind::ControlDrop => ClickKind::ControlDrop,
        events::ClickKind::DoubleClick => ClickKind::DoubleClick,
        events::ClickKind::Middle => ClickKind::Middle,
        other => panic!("unexpected click {other:?}"),
    };
    (slot, kind)
}

struct Click {
    handlers: Vec<u32>,
    inventory: u64,
    slot: ClickedSlot,
    kind: ClickKind,
    item: Option<ItemSpec>,
}

struct Close {
    handlers: Vec<u32>,
    inventory: u64,
    by_player: bool,
    viewers: Vec<String>,
}

fn handlers() -> Vec<rspigot_core::plugins::HandlerSpec> {
    vec![
        observe(1, EventKind::InventoryClick),
        observe(2, EventKind::InventoryClose),
    ]
}

fn spec(key: &str, count: i32) -> ItemSpec {
    ItemSpec::of(key, count)
}

fn diamond() -> ItemSpec {
    spec("minecraft:diamond", 1)
}

fn stack(key: &str, count: u8) -> Slot {
    let kind = ItemKind::from_name(key).expect("a known item");
    Some(ItemStack::new(
        kind.id(),
        NonZeroU8::new(count).expect("a count"),
    ))
}

fn shown(slot: &Slot) -> Option<(&'static str, u8)> {
    slot.as_ref().map(|stack| {
        (
            stack.item.kind().map_or("?", ItemKind::name),
            stack.count.get(),
        )
    })
}

fn create(id: u64, rows: i32, title: &str, locked: bool) -> Order {
    Order::Server(ServerOrder::CreateInventory {
        id,
        rows,
        title: title.to_owned(),
        locked,
    })
}

fn custom(inventory: u64, order: CustomInventoryOrder) -> Order {
    Order::CustomInventory { inventory, order }
}

fn put(slot: i32, item: ItemSpec) -> CustomInventoryOrder {
    CustomInventoryOrder::Set {
        slot,
        item: Some(item),
    }
}

fn give(player: &Player, slot: i32, item: ItemSpec) -> Order {
    Order::Inventory {
        player: player.id,
        order: InventoryOrder::Set {
            slot,
            item: Some(item),
        },
    }
}

fn menu_scene() -> (Scene, Player, Player) {
    let mut s = scene_with(&["Alice", "Bob"], handlers());
    let (alice, bob) = (s.players[0].clone(), s.players[1].clone());
    s.act(create(MENU, 1, "§6Menu", true));
    s.act(custom(MENU, put(0, diamond())));
    (s, alice, bob)
}

fn open(s: &mut Scene, player: &Player, inventory: u64) -> (Screen, Vec<ClientEvent>) {
    s.act(to(player, PlayerOrder::OpenInventory(inventory)));
    let events = player.link.take();
    let window = events
        .iter()
        .find_map(|event| match event {
            ClientEvent::OpenScreen { window, .. } => Some(*window),
            _ => None,
        })
        .expect("the menu opens");
    let mut screen = Screen { window, state: 0 };
    screen.follow(&events);
    (screen, events)
}

fn press(slot: i16, button: i8, input: ClickInput) -> ContainerClick {
    ContainerClick {
        container: 0,
        state_id: 0,
        slot,
        button,
        input,
        changed: Vec::new(),
        carried: HashedSlot::EMPTY,
    }
}

fn click(
    s: &mut Scene,
    player: &Player,
    screen: &mut Screen,
    click: ContainerClick,
) -> Vec<ClientEvent> {
    s.server.handle(Inbound::Input {
        player: player.id,
        input: PlayerInput::ContainerClick(ContainerClick {
            container: screen.window,
            state_id: screen.state,
            ..click
        }),
    });
    s.server.deliver();
    let events = player.link.take();
    screen.follow(&events);
    events
}

fn close_window(s: &mut Scene, player: &Player, window: i32) {
    s.server.handle(Inbound::Input {
        player: player.id,
        input: PlayerInput::ContainerClose(window),
    });
    s.server.deliver();
}

fn content(events: &[ClientEvent], window: i32) -> Option<(Vec<Slot>, Slot)> {
    events.iter().rev().find_map(|event| match event {
        ClientEvent::ContainerContent {
            window: at,
            slots,
            carried,
            ..
        } if *at == window => Some((slots.clone(), carried.clone())),
        _ => None,
    })
}

fn closed(events: &[ClientEvent], window: i32) -> bool {
    events.contains(&ClientEvent::ContainerClose(window))
}

fn state_of(s: &mut Scene, inventory: u64) -> CustomInventoryState {
    let Answer::CustomInventory(state) = s.ask(Question::CustomInventory(inventory)) else {
        panic!("expected the custom inventory");
    };
    state
}

fn inventory(s: &mut Scene, player: &Player) -> InventoryState {
    let Answer::Inventory(state) = s.ask(Question::Inventory(player.id)) else {
        panic!("expected the inventory of the player");
    };
    state
}

fn events_of(mailbox: &Mailbox) -> Vec<(Vec<u32>, EventPayload)> {
    mailbox
        .received()
        .into_iter()
        .filter_map(|message| match message {
            ToPlugin::Event {
                handlers, payload, ..
            } => Some((handlers, payload)),
            _ => None,
        })
        .collect()
}

fn clicks(mailbox: &Mailbox, player: &Player) -> Vec<Click> {
    events_of(mailbox)
        .into_iter()
        .filter_map(|(handlers, payload)| match payload {
            EventPayload::InventoryClick(click) if click.who_clicked.id == player.id => {
                let (slot, kind) = clicked(&click);
                Some(Click {
                    handlers,
                    inventory: custom_id(&click.inventory),
                    slot,
                    kind,
                    item: click.current_item,
                })
            }
            _ => None,
        })
        .collect()
}

fn last_click(s: &Scene, player: &Player) -> Click {
    clicks(&s.plugin, player)
        .pop()
        .expect("the plugin heard a click")
}

fn closes(mailbox: &Mailbox, player: &Player) -> Vec<Close> {
    events_of(mailbox)
        .into_iter()
        .filter_map(|(handlers, payload)| match payload {
            EventPayload::InventoryClose(InventoryClose {
                player: who,
                inventory,
                by_player,
                viewers,
                ..
            }) if who.id == player.id => Some(Close {
                handlers,
                inventory: custom_id(&inventory),
                by_player,
                viewers: viewers.into_iter().map(|viewer| viewer.name).collect(),
            }),
            _ => None,
        })
        .collect()
}

fn last_close(s: &Scene, player: &Player) -> Close {
    closes(&s.plugin, player)
        .pop()
        .expect("the plugin heard a close")
}

fn assert_click(
    heard: &Click,
    slot: ClickedSlot,
    kind: ClickKind,
    item: Option<ItemSpec>,
    inventory: u64,
) {
    assert_eq!(heard.handlers, [1]);
    assert_eq!(heard.inventory, inventory);
    assert_eq!(heard.slot, slot);
    assert_eq!(heard.kind, kind);
    assert_eq!(heard.item, item);
}

#[test]
fn a_locked_menu_refuses_every_click_and_resyncs() {
    let (mut s, alice, _bob) = menu_scene();
    s.act(give(&alice, 9, spec("minecraft:stone", 5)));
    let (mut screen, opened) = open(&mut s, &alice, MENU);
    assert!(opened.contains(&ClientEvent::OpenScreen {
        window: screen.window,
        menu: 0,
        title: Component::from_section_codes("§6Menu"),
    }));
    let (slots, carried) = content(&opened, screen.window).expect("the menu content");
    assert_eq!(slots.len(), MENU_SIZE + 36);
    assert_eq!(shown(&slots[0]), Some(("minecraft:diamond", 1)));
    assert_eq!(shown(&slots[MENU_SIZE]), Some(("minecraft:stone", 5)));
    assert!(carried.is_none());
    let items = state_of(&mut s, MENU).items;
    let before = inventory(&mut s, &alice);

    let taken = click(
        &mut s,
        &alice,
        &mut screen,
        ContainerClick {
            changed: vec![(0, HashedSlot::EMPTY)],
            carried: HashedSlot::of(&stack("minecraft:diamond", 1)),
            ..press(0, 0, ClickInput::Pickup)
        },
    );
    let (slots, carried) = content(&taken, screen.window).expect("a resync");
    assert_eq!(shown(&slots[0]), Some(("minecraft:diamond", 1)));
    assert!(carried.is_none());
    assert_eq!(state_of(&mut s, MENU).items[0], Some(diamond()));
    assert_click(
        &last_click(&s, &alice),
        ClickedSlot::Menu(0),
        ClickKind::Left,
        Some(diamond()),
        MENU,
    );

    let tries = [
        (
            press(0, 1, ClickInput::Pickup),
            ClickedSlot::Menu(0),
            ClickKind::Right,
            Some(diamond()),
        ),
        (
            press(0, 0, ClickInput::QuickMove),
            ClickedSlot::Menu(0),
            ClickKind::ShiftLeft,
            Some(diamond()),
        ),
        (
            press(0, 1, ClickInput::QuickMove),
            ClickedSlot::Menu(0),
            ClickKind::ShiftRight,
            Some(diamond()),
        ),
        (
            press(0, 0, ClickInput::PickupAll),
            ClickedSlot::Menu(0),
            ClickKind::DoubleClick,
            Some(diamond()),
        ),
        (
            press(0, 0, ClickInput::Throw),
            ClickedSlot::Menu(0),
            ClickKind::Drop,
            Some(diamond()),
        ),
        (
            press(0, 1, ClickInput::Throw),
            ClickedSlot::Menu(0),
            ClickKind::ControlDrop,
            Some(diamond()),
        ),
        (
            press(0, 2, ClickInput::Clone),
            ClickedSlot::Menu(0),
            ClickKind::Middle,
            Some(diamond()),
        ),
        (
            press(-999, 0, ClickInput::Pickup),
            ClickedSlot::Outside,
            ClickKind::Left,
            None,
        ),
        (
            press(MENU_SIZE as i16, 0, ClickInput::Pickup),
            ClickedSlot::Player(9),
            ClickKind::Left,
            Some(spec("minecraft:stone", 5)),
        ),
        (
            press(MENU_SIZE as i16 + 27, 0, ClickInput::QuickMove),
            ClickedSlot::Player(0),
            ClickKind::ShiftLeft,
            None,
        ),
    ];
    for (pressed, slot, kind, item) in tries {
        let answered = click(&mut s, &alice, &mut screen, pressed);
        let (slots, _) = content(&answered, screen.window).expect("a resync");
        assert_eq!(shown(&slots[0]), Some(("minecraft:diamond", 1)));
        assert_eq!(shown(&slots[MENU_SIZE]), Some(("minecraft:stone", 5)));
        assert_click(&last_click(&s, &alice), slot, kind, item, MENU);
        assert_eq!(state_of(&mut s, MENU).items, items);
        assert_eq!(inventory(&mut s, &alice), before);
    }
    assert!(s.server.ground_items().is_empty());

    let heard = clicks(&s.plugin, &alice).len();
    s.server.handle(Inbound::Input {
        player: alice.id,
        input: PlayerInput::ContainerClick(press(9, 0, ClickInput::Pickup)),
    });
    s.server.deliver();
    assert!(alice.link.take().is_empty());
    assert_eq!(inventory(&mut s, &alice), before);
    assert_eq!(clicks(&s.plugin, &alice).len(), heard);

    let dragged = click(
        &mut s,
        &alice,
        &mut screen,
        press(-999, 0, ClickInput::QuickCraft),
    );
    assert!(content(&dragged, screen.window).is_some());
    assert_eq!(clicks(&s.plugin, &alice).len(), heard);
    assert!(s.warnings().is_empty());
}

#[test]
fn a_locked_menu_resyncs_the_offhand_after_a_swap() {
    let (mut s, alice, _bob) = menu_scene();
    s.act(give(&alice, 40, spec("minecraft:shield", 1)));
    s.act(give(&alice, 3, spec("minecraft:stone", 2)));
    let (mut screen, _) = open(&mut s, &alice, MENU);
    let before = inventory(&mut s, &alice);

    let swapped = click(&mut s, &alice, &mut screen, press(0, 40, ClickInput::Swap));
    let (slots, _) = content(&swapped, screen.window).expect("a resync");
    assert_eq!(shown(&slots[0]), Some(("minecraft:diamond", 1)));
    assert!(swapped.contains(&ClientEvent::PlayerInventorySlot {
        slot: 40,
        stack: stack("minecraft:shield", 1),
    }));
    assert_click(
        &last_click(&s, &alice),
        ClickedSlot::Menu(0),
        ClickKind::SwapOffhand,
        Some(diamond()),
        MENU,
    );

    let keyed = click(&mut s, &alice, &mut screen, press(0, 3, ClickInput::Swap));
    let (slots, _) = content(&keyed, screen.window).expect("a resync");
    assert_eq!(shown(&slots[0]), Some(("minecraft:diamond", 1)));
    assert_eq!(
        shown(&slots[MENU_SIZE + 27 + 3]),
        Some(("minecraft:stone", 2))
    );
    assert!(
        !keyed
            .iter()
            .any(|event| matches!(event, ClientEvent::PlayerInventorySlot { .. }))
    );
    assert_click(
        &last_click(&s, &alice),
        ClickedSlot::Menu(0),
        ClickKind::NumberKey(3),
        Some(diamond()),
        MENU,
    );
    assert_eq!(inventory(&mut s, &alice), before);
    assert_eq!(state_of(&mut s, MENU).items[0], Some(diamond()));
}

#[test]
fn an_open_storage_keeps_moved_items() {
    let (mut s, alice, bob) = menu_scene();
    s.act(create(BOX, 1, "Box", false));
    s.act(give(&alice, 0, diamond()));
    let (mut screen, _) = open(&mut s, &alice, BOX);
    let (bob_screen, _) = open(&mut s, &bob, BOX);

    let moved = click(
        &mut s,
        &alice,
        &mut screen,
        press(MENU_SIZE as i16 + 27, 0, ClickInput::QuickMove),
    );
    assert!(moved.iter().any(|event| matches!(
        event,
        ClientEvent::ContainerSlot { window, slot: 0, stack, .. }
            if *window == screen.window && shown(stack) == Some(("minecraft:diamond", 1))
    )));
    let state = state_of(&mut s, BOX);
    assert_eq!(state.items.len(), MENU_SIZE);
    assert_eq!(state.items[0], Some(diamond()));
    let mut viewers: Vec<String> = state.viewers.into_iter().map(|p| p.name).collect();
    viewers.sort();
    assert_eq!(viewers, ["Alice", "Bob"]);
    assert_eq!(inventory(&mut s, &alice).slots[0], None);
    assert_click(
        &last_click(&s, &alice),
        ClickedSlot::Player(0),
        ClickKind::ShiftLeft,
        Some(diamond()),
        BOX,
    );
    let seen = bob.link.take();
    assert!(seen.iter().any(|event| matches!(
        event,
        ClientEvent::ContainerSlot { window, slot: 0, stack, .. }
            if *window == bob_screen.window && shown(stack) == Some(("minecraft:diamond", 1))
    )));

    let taken = click(&mut s, &alice, &mut screen, press(0, 0, ClickInput::Pickup));
    assert!(content(&taken, screen.window).is_none());
    close_window(&mut s, &alice, screen.window);
    assert_eq!(state_of(&mut s, BOX).items[0], None);
    assert_eq!(inventory(&mut s, &alice).slots[0], Some(diamond()));
    assert!(s.warnings().is_empty());
}

#[test]
fn closing_reports_who_closed() {
    let (mut s, alice, bob) = menu_scene();
    let (screen, _) = open(&mut s, &alice, MENU);
    let (bob_screen, _) = open(&mut s, &bob, MENU);
    close_window(&mut s, &alice, screen.window);
    let heard = last_close(&s, &alice);
    assert_eq!(heard.handlers, [2]);
    assert_eq!(heard.inventory, MENU);
    assert!(heard.by_player);
    let mut viewers = heard.viewers;
    viewers.sort();
    assert_eq!(viewers, ["Alice", "Bob"]);
    close_window(&mut s, &bob, bob_screen.window);
    assert_eq!(last_close(&s, &bob).viewers, ["Bob"]);
    assert!(state_of(&mut s, MENU).viewers.is_empty());

    let (screen, _) = open(&mut s, &alice, MENU);
    s.act(custom(MENU, CustomInventoryOrder::Close));
    assert!(closed(&alice.link.take(), screen.window));
    assert!(!last_close(&s, &alice).by_player);
    assert_eq!(closes(&s.plugin, &alice).len(), 2);

    let (screen, _) = open(&mut s, &alice, MENU);
    s.act(to(&alice, PlayerOrder::CloseInventory));
    assert!(closed(&alice.link.take(), screen.window));
    assert!(!last_close(&s, &alice).by_player);
    assert_eq!(closes(&s.plugin, &alice).len(), 3);

    let (screen, _) = open(&mut s, &alice, MENU);
    s.act(custom(MENU, CustomInventoryOrder::Discard));
    assert!(closed(&alice.link.take(), screen.window));
    assert_eq!(closes(&s.plugin, &alice).len(), 4);
    assert_eq!(s.ask(Question::CustomInventory(MENU)), Answer::NotFound);
    assert!(s.warnings().is_empty());
}

#[test]
fn the_owner_disconnecting_closes_its_menus_everywhere() {
    let (mut s, alice, bob) = menu_scene();
    let (alice_screen, _) = open(&mut s, &alice, MENU);
    let (bob_screen, _) = open(&mut s, &bob, MENU);
    s.server
        .handle(Inbound::PluginDisconnected { connection: 1 });
    s.server.deliver();
    assert!(closed(&alice.link.take(), alice_screen.window));
    assert!(closed(&bob.link.take(), bob_screen.window));

    s.plugin = connect_with(&mut s.server, 2, "tester", handlers());
    assert_eq!(
        s.ask_on(2, Question::CustomInventory(MENU)),
        Answer::NotFound
    );

    order(
        &mut s.server,
        2,
        FromPlugin::Order(create(MENU, 2, "Again", true)),
    );
    order(
        &mut s.server,
        2,
        FromPlugin::Order(to(&alice, PlayerOrder::OpenInventory(MENU))),
    );
    let window = alice
        .link
        .take()
        .iter()
        .find_map(|event| match event {
            ClientEvent::OpenScreen {
                window, menu: 1, ..
            } => Some(*window),
            _ => None,
        })
        .expect("the second plugin opens its menu");
    s.plugin = connect_with(&mut s.server, 3, "tester", handlers());
    assert!(closed(&alice.link.take(), window));
    assert_eq!(
        s.ask_on(3, Question::CustomInventory(MENU)),
        Answer::NotFound
    );
}

#[test]
fn every_listening_plugin_hears_the_events_of_a_plugin_inventory() {
    let (mut s, alice, _bob) = menu_scene();
    let other = connect_with(&mut s.server, 2, "other", handlers());
    let (mut screen, _) = open(&mut s, &alice, MENU);
    click(&mut s, &alice, &mut screen, press(0, 0, ClickInput::Pickup));
    close_window(&mut s, &alice, screen.window);
    assert_eq!(clicks(&s.plugin, &alice).len(), 1);
    assert_eq!(closes(&s.plugin, &alice).len(), 1);
    assert_eq!(clicks(&other, &alice).len(), 1);
    assert_eq!(closes(&other, &alice).len(), 1);
    assert_eq!(clicks(&other, &alice)[0].inventory, MENU);

    order(
        &mut s.server,
        2,
        FromPlugin::Order(create(MENU, 1, "Other", true)),
    );
    order(
        &mut s.server,
        2,
        FromPlugin::Order(to(&alice, PlayerOrder::OpenInventory(MENU))),
    );
    let (slots, _) = content(&alice.link.take(), screen.window + 1).expect("the other menu");
    assert!(slots[0].is_none());
}

#[test]
fn limits_and_unknown_ids() {
    let (mut s, alice, _bob) = menu_scene();
    let refused = |what: &str| format!("server.create_inventory was not applied: {what}");

    s.act(create(2, 0, "None", true));
    s.act(create(3, 7, "Seven", true));
    assert_eq!(
        s.warnings(),
        [
            refused("rows 0 is outside 1..=6"),
            refused("rows 7 is outside 1..=6")
        ]
    );
    s.act(create(MENU, 2, "Twin", false));
    assert_eq!(
        s.warnings().last(),
        Some(&refused("inventory 1 already exists"))
    );
    s.act(custom(MENU, put(9, diamond())));
    s.act(custom(MENU, put(-1, diamond())));
    assert_eq!(
        s.warnings()[3..],
        [
            "custom_inventory.set was not applied: slot 9 is outside 0..=8".to_owned(),
            "custom_inventory.set was not applied: slot -1 is outside 0..=8".to_owned(),
        ]
    );
    s.act(custom(MENU, put(1, spec("minecraft:diamond", 0))));
    assert_eq!(s.warnings().len(), 6);
    assert_eq!(state_of(&mut s, MENU).items[1], None);

    s.act(to(&alice, PlayerOrder::OpenInventory(99)));
    s.act(custom(99, put(0, diamond())));
    s.act(custom(99, CustomInventoryOrder::Close));
    assert!(
        !alice
            .link
            .take()
            .iter()
            .any(|event| matches!(event, ClientEvent::OpenScreen { .. }))
    );
    assert_eq!(s.warnings().len(), 9);
    assert_eq!(s.ask(Question::CustomInventory(99)), Answer::NotFound);

    let table = BlockPos::new(2, 64, 0);
    s.act(Order::Block {
        pos: table,
        order: BlockOrder::Set("minecraft:crafting_table".to_owned()),
    });
    let (screen, _) = open(&mut s, &alice, MENU);
    s.server.handle(Inbound::Input {
        player: alice.id,
        input: PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: table,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 1.0, 0.5),
            inside_block: false,
            sequence: 1,
        },
    });
    s.server.deliver();
    let events = alice.link.take();
    assert!(closed(&events, screen.window));
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::OpenScreen { window, .. } if *window == screen.window + 1
    )));
    let heard = last_close(&s, &alice);
    assert_eq!(heard.inventory, MENU);
    assert!(!heard.by_player);
    assert!(state_of(&mut s, MENU).viewers.is_empty());

    for id in 2..=MAX_PLUGIN_INVENTORIES as u64 {
        s.act(create(id, 1, "Many", true));
    }
    assert_eq!(s.warnings().len(), 9);
    s.act(create(
        MAX_PLUGIN_INVENTORIES as u64 + 1,
        1,
        "One too many",
        true,
    ));
    assert_eq!(
        s.warnings()[9..],
        [refused("the plugin already holds 1024 inventories")]
    );
    assert_eq!(
        s.ask(Question::CustomInventory(MAX_PLUGIN_INVENTORIES as u64 + 1)),
        Answer::NotFound
    );
}

#[test]
fn two_plugins_with_the_same_inventory_id_hand_out_different_inventories() {
    let (mut s, alice, bob) = menu_scene();
    let other = connect_with(&mut s.server, 2, "other", handlers());
    order(
        &mut s.server,
        2,
        FromPlugin::Order(create(MENU, 1, "Theirs", true)),
    );
    let (mut mine, _) = open(&mut s, &alice, MENU);
    order(
        &mut s.server,
        2,
        FromPlugin::Order(to(&bob, PlayerOrder::OpenInventory(MENU))),
    );
    let window = bob
        .link
        .take()
        .iter()
        .find_map(|event| match event {
            ClientEvent::OpenScreen { window, .. } => Some(*window),
            _ => None,
        })
        .expect("the other plugin opens its menu");
    let mut theirs = Screen { window, state: 0 };
    click(&mut s, &alice, &mut mine, press(0, 0, ClickInput::Pickup));
    click(&mut s, &bob, &mut theirs, press(0, 0, ClickInput::Pickup));
    let owners: Vec<InventoryOwner> = events_of(&other)
        .into_iter()
        .filter_map(|(_, payload)| match payload {
            EventPayload::InventoryClick(click) => Some(click.inventory.owner),
            _ => None,
        })
        .collect();
    assert_eq!(
        owners,
        [
            InventoryOwner::Custom {
                plugin: "tester".to_owned(),
                id: MENU
            },
            InventoryOwner::Custom {
                plugin: "other".to_owned(),
                id: MENU
            },
        ]
    );
    assert_ne!(owners[0], owners[1]);
}
