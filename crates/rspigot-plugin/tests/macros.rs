use rspigot_plugin::prelude::*;
use rspigot_plugin::{Access, EventKind, HandlerSpec};

#[derive(Plugin)]
struct Welcome;

#[listeners]
impl Welcome {
    async fn on_player_join(&self, event: &PlayerJoin) {
        event.player().message().send("Welcome!");
    }

    #[priority(High)]
    async fn on_player_move(&self, event: &mut PlayerMove) {
        event.cancel();
    }

    #[ignore_cancelled]
    #[priority(Lowest)]
    async fn log_real_moves(&self, event: &rspigot_plugin::PlayerMove) {
        let _ = event.to();
    }

    #[priority(Highest)]
    async fn hold_the_spawn(&self, event: &mut ::rspigot_plugin::PlayerMove) {
        if event.to().is_some_and(|to| to.position.y > 100.0) {
            event.cancel();
        } else {
            event.set_to(event.from().clone());
        }
    }

    #[priority(Low)]
    async fn on_player_leave(&self, event: &PlayerLeave) {
        let _ = event.id();
    }

    #[priority(Normal)]
    async fn say_goodbye(&self, event: &PlayerLeave) {
        let _ = event.player();
    }
}

#[derive(Plugin)]
#[plugin(name = "greeter", version = "2.0.0")]
struct Greeter;

#[listeners]
impl Greeter {}

#[derive(Plugin)]
#[plugin(version = "0.0.1-beta")]
struct Beta;

#[listeners]
impl Beta {}

struct Conditional;

#[listeners]
impl Conditional {
    #[cfg(any())]
    async fn never_built(&self, event: &PlayerJoin) {
        let _ = event;
    }

    #[cfg(test)]
    #[priority(High)]
    async fn always_built(&self, event: &mut PlayerMove) {
        event.cancel();
    }
}

struct Lifecycle;

#[listeners]
impl Lifecycle {
    async fn on_enable(&self, server: &Server) {
        server.message().broadcast("Enabled");
    }

    async fn on_disable(&self) {}

    async fn on_player_join(&self, event: &PlayerJoin) {
        let _ = event.id();
    }
}

fn spec(
    id: u32,
    event: EventKind,
    access: Access,
    priority: Priority,
    ignore_cancelled: bool,
) -> HandlerSpec {
    HandlerSpec {
        id,
        event,
        access,
        priority,
        ignore_cancelled,
    }
}

#[test]
fn listeners_registers_every_method_with_its_access_and_priority() {
    assert_eq!(
        Welcome::handlers(),
        vec![
            spec(
                1,
                EventKind::PlayerJoin,
                Access::Observe,
                Priority::Normal,
                false
            ),
            spec(
                2,
                EventKind::PlayerMove,
                Access::Decide,
                Priority::High,
                false
            ),
            spec(
                3,
                EventKind::PlayerMove,
                Access::Observe,
                Priority::Lowest,
                true
            ),
            spec(
                4,
                EventKind::PlayerMove,
                Access::Decide,
                Priority::Highest,
                false
            ),
            spec(
                5,
                EventKind::PlayerLeave,
                Access::Observe,
                Priority::Low,
                false
            ),
            spec(
                6,
                EventKind::PlayerLeave,
                Access::Observe,
                Priority::Normal,
                false
            ),
        ]
    );
    assert_eq!(Greeter::handlers(), Vec::new());
}

#[test]
fn a_handler_left_out_by_cfg_is_not_registered() {
    assert_eq!(
        Conditional::handlers(),
        vec![spec(
            2,
            EventKind::PlayerMove,
            Access::Decide,
            Priority::High,
            false
        )]
    );
}

#[test]
fn lifecycle_hooks_are_not_handlers() {
    assert_eq!(
        Lifecycle::handlers(),
        vec![spec(
            3,
            EventKind::PlayerJoin,
            Access::Observe,
            Priority::Normal,
            false
        )]
    );
}

#[test]
fn derive_plugin_takes_the_crate_name_and_version_unless_overridden() {
    assert_eq!(
        (Welcome::NAME, Welcome::VERSION),
        ("rspigot-plugin", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!((Greeter::NAME, Greeter::VERSION), ("greeter", "2.0.0"));
    assert_eq!(
        (Beta::NAME, Beta::VERSION),
        ("rspigot-plugin", "0.0.1-beta")
    );
}

#[test]
fn handlers_that_break_the_rules_do_not_compile() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
