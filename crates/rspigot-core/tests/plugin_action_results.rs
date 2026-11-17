mod common;

use std::fmt::Write as _;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use common::*;
use rspigot_core::plugins::{
    ActionError, ActionErrorKind, Answer, EventKind, FromPlugin, MAX_PLUGIN_INVENTORIES, Order,
    PlayerOrder, Question, ServerOrder, TitleSpec, ToPlugin,
};
use rspigot_domain::{Rotation, Vec3};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Metadata, Subscriber, span};

static NEXT: AtomicU64 = AtomicU64::new(1_000);

const MENU: u64 = 7;

fn at(x: f64, z: f64) -> Vec3 {
    Vec3::new(x, 64.0, z)
}

fn perform(s: &mut Scene, order: Order) -> (u64, Option<Answer>) {
    let request = NEXT.fetch_add(1, Ordering::Relaxed);
    order_of(s, request, order);
    (request, answer_to(s, request))
}

fn order_of(s: &mut Scene, request: u64, order: Order) {
    common::order(&mut s.server, 1, FromPlugin::Action { request, order });
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

fn refused(answer: Option<Answer>) -> ActionError {
    match answer {
        Some(Answer::Failed(error)) => error,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn create_menu(s: &mut Scene, id: u64, rows: i32) -> Option<Answer> {
    perform(
        s,
        Order::Server(ServerOrder::CreateInventory {
            id,
            rows,
            title: "Shop".to_owned(),
            locked: true,
        }),
    )
    .1
}

struct Logged(Arc<Mutex<Vec<String>>>);

struct Fields(String);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let _ = write!(self.0, "{}={value:?} ", field.name());
    }
}

impl Subscriber for Logged {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(1)
    }

    fn record(&self, _: &span::Id, _: &span::Record<'_>) {}

    fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}

    fn event(&self, event: &Event<'_>) {
        if *event.metadata().level() != Level::WARN {
            return;
        }
        let mut fields = Fields(String::new());
        event.record(&mut fields);
        self.0.lock().unwrap().push(fields.0);
    }

    fn enter(&self, _: &span::Id) {}

    fn exit(&self, _: &span::Id) {}
}

#[test]
fn teleporting_a_disconnected_player_returns_an_error() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.server.handle(leave_of(&alice));
    s.server.deliver();

    let (_, answer) = perform(&mut s, to(&alice, PlayerOrder::Teleport(at(5.5, 0.5))));

    assert_eq!(refused(answer).kind, ActionErrorKind::TargetGone);
}

#[test]
fn teleporting_an_online_player_answers_done_once_it_has_moved() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();

    let (_, answer) = perform(&mut s, to(&alice, PlayerOrder::Teleport(at(5.5, 0.5))));

    assert_eq!(answer, Some(Answer::Done));
    assert_eq!(s.server.player_location(alice.id), Some(at(5.5, 0.5)));
}

#[test]
fn teleporting_to_a_position_that_is_not_finite_is_an_invalid_argument() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();

    let (_, answer) = perform(
        &mut s,
        to(
            &alice,
            PlayerOrder::Teleport(Vec3::new(f64::NAN, 64.0, 0.5)),
        ),
    );

    assert_eq!(refused(answer).kind, ActionErrorKind::InvalidArgument);
}

#[test]
fn teleporting_to_an_unknown_world_is_not_found() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();

    let (_, answer) = perform(
        &mut s,
        to(
            &alice,
            PlayerOrder::TeleportTo {
                world: "minecraft:nowhere".to_owned(),
                position: at(0.5, 0.5),
                rotation: Some(Rotation::default()),
            },
        ),
    );

    assert_eq!(refused(answer).kind, ActionErrorKind::NotFound);
}

#[test]
fn teleporting_a_dead_player_is_an_incompatible_state() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.act(to(&alice, PlayerOrder::SetHealth(0.0)));
    assert_eq!(s.server.player_is_dead(alice.id), Some(true));

    let (_, answer) = perform(&mut s, to(&alice, PlayerOrder::Teleport(at(5.5, 0.5))));

    assert_eq!(refused(answer).kind, ActionErrorKind::IncompatibleState);
}

#[test]
fn a_teleport_cancelled_by_a_plugin_is_cancelled() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerTeleport)]);
    let alice = s.players[0].clone();
    let request = NEXT.fetch_add(1, Ordering::Relaxed);
    order_of(
        &mut s,
        request,
        to(&alice, PlayerOrder::Teleport(at(9.5, 0.5))),
    );
    assert_eq!(answer_to(&s, request), None);

    let asked = last_asked(&s.plugin, EventKind::PlayerTeleport);
    rule_on(&mut s.server, 1, asked.id, true, None);

    assert_eq!(
        refused(answer_to(&s, request)).kind,
        ActionErrorKind::Cancelled
    );
    assert_ne!(s.server.player_location(alice.id), Some(at(9.5, 0.5)));
}

#[test]
fn a_teleport_allowed_by_a_plugin_answers_done_after_the_verdict() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::PlayerTeleport)]);
    let alice = s.players[0].clone();
    let request = NEXT.fetch_add(1, Ordering::Relaxed);
    order_of(
        &mut s,
        request,
        to(&alice, PlayerOrder::Teleport(at(9.5, 0.5))),
    );
    assert_eq!(answer_to(&s, request), None);

    let asked = last_asked(&s.plugin, EventKind::PlayerTeleport);
    rule_on(&mut s.server, 1, asked.id, false, None);

    assert_eq!(answer_to(&s, request), Some(Answer::Done));
}

#[test]
fn opening_an_inventory_reports_each_way_it_can_fail() {
    let mut s = scene(&["Alice", "Bob"]);
    let alice = s.players[0].clone();
    let bob = s.players[1].clone();
    assert_eq!(create_menu(&mut s, MENU, 1), Some(Answer::Done));

    let (_, unknown) = perform(&mut s, to(&alice, PlayerOrder::OpenInventory(MENU + 1)));
    assert_eq!(refused(unknown).kind, ActionErrorKind::NotFound);

    s.server.handle(leave_of(&bob));
    s.server.deliver();
    let (_, gone) = perform(&mut s, to(&bob, PlayerOrder::OpenInventory(MENU)));
    assert_eq!(refused(gone).kind, ActionErrorKind::TargetGone);

    let (_, opened) = perform(&mut s, to(&alice, PlayerOrder::OpenInventory(MENU)));
    assert_eq!(opened, Some(Answer::Done));
}

#[test]
fn opening_an_inventory_for_a_dead_player_is_an_incompatible_state() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    assert_eq!(create_menu(&mut s, MENU, 1), Some(Answer::Done));
    s.act(to(&alice, PlayerOrder::SetHealth(0.0)));

    let (_, answer) = perform(&mut s, to(&alice, PlayerOrder::OpenInventory(MENU)));

    assert_eq!(refused(answer).kind, ActionErrorKind::IncompatibleState);
}

#[test]
fn an_open_cancelled_by_a_plugin_is_cancelled() {
    let mut s = scene_with(&["Alice"], vec![deciding(1, EventKind::InventoryOpen)]);
    let alice = s.players[0].clone();
    assert_eq!(create_menu(&mut s, MENU, 1), Some(Answer::Done));
    let request = NEXT.fetch_add(1, Ordering::Relaxed);
    order_of(
        &mut s,
        request,
        to(&alice, PlayerOrder::OpenInventory(MENU)),
    );
    assert_eq!(answer_to(&s, request), None);

    let asked = last_asked(&s.plugin, EventKind::InventoryOpen);
    rule_on(&mut s.server, 1, asked.id, true, None);

    assert_eq!(
        refused(answer_to(&s, request)).kind,
        ActionErrorKind::Cancelled
    );
}

#[test]
fn creating_an_inventory_reports_a_bad_size_a_duplicate_and_the_limit() {
    let mut s = scene(&["Alice"]);

    assert_eq!(
        refused(create_menu(&mut s, MENU, 7)).kind,
        ActionErrorKind::InvalidArgument
    );
    assert_eq!(create_menu(&mut s, MENU, 2), Some(Answer::Done));
    assert_eq!(
        refused(create_menu(&mut s, MENU, 2)).kind,
        ActionErrorKind::IncompatibleState
    );
    for id in 0..MAX_PLUGIN_INVENTORIES as u64 - 1 {
        assert_eq!(create_menu(&mut s, 100 + id, 1), Some(Answer::Done));
    }
    assert_eq!(
        refused(create_menu(&mut s, 5_000, 1)).kind,
        ActionErrorKind::LimitReached
    );
}

#[test]
fn spawning_an_unknown_entity_type_is_an_invalid_argument() {
    let mut s = scene(&["Alice"]);

    let answer = s.ask(Question::Spawn {
        kind: "minecraft:nope".to_owned(),
        position: at(0.5, 0.5),
    });

    assert_eq!(refused(Some(answer)).kind, ActionErrorKind::InvalidArgument);
}

#[test]
fn an_order_that_does_not_report_a_result_is_refused_when_a_result_is_asked() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();

    let (_, answer) = perform(&mut s, to(&alice, PlayerOrder::Kill));

    assert_eq!(refused(answer).kind, ActionErrorKind::InvalidArgument);
}

#[test]
fn a_failed_title_logs_a_warning_naming_the_plugin() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    let logged = Arc::new(Mutex::new(Vec::new()));

    tracing::subscriber::with_default(Logged(logged.clone()), || {
        s.act(to(
            &alice,
            PlayerOrder::SendTitle(TitleSpec {
                title: "Hi".to_owned(),
                subtitle: None,
                fade_in: -1,
                stay: 20,
                fade_out: 10,
            }),
        ));
    });

    let logged = logged.lock().unwrap();
    assert_eq!(logged.len(), 1, "{logged:?}");
    assert!(logged[0].contains("plugin=\"tester\""), "{logged:?}");
    assert!(logged[0].contains("player.send_title"), "{logged:?}");
    assert!(logged[0].contains("fade in"), "{logged:?}");
}

#[test]
fn a_display_order_for_a_player_who_left_logs_a_warning() {
    let mut s = scene(&["Alice"]);
    let alice = s.players[0].clone();
    s.server.handle(leave_of(&alice));
    s.server.deliver();
    let logged = Arc::new(Mutex::new(Vec::new()));

    tracing::subscriber::with_default(Logged(logged.clone()), || {
        s.act(to(&alice, PlayerOrder::SendMessage("hello".to_owned())));
    });

    let logged = logged.lock().unwrap();
    assert_eq!(logged.len(), 1, "{logged:?}");
    assert!(logged[0].contains("plugin=\"tester\""), "{logged:?}");
    assert!(logged[0].contains("not online"), "{logged:?}");
}
