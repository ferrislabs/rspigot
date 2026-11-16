use std::time::Duration;

use rspigot_domain::player::PlayerId;

use super::{CommandOutcome, CommandSender, execute};
use crate::end_tests::{Tap, arrive};
use crate::events::ClientEvent;
use crate::server::Server;

struct Rig {
    server: Server<Tap>,
    id: PlayerId,
    link: Tap,
}

fn rig() -> Rig {
    let (mut server, id, link) = arrive();
    if let Some(player) = server.players.get_mut(&id) {
        player.operator = true;
        player.op_level = 4;
    }
    server.deliver();
    link.take();
    Rig { server, id, link }
}

impl Rig {
    fn run(&mut self, line: &str) -> CommandOutcome {
        let outcome = execute(&mut self.server, CommandSender::Player(self.id), line);
        self.server.deliver();
        outcome
    }

    fn events(&self) -> Vec<ClientEvent> {
        self.link.take()
    }

    fn texts(&self) -> Vec<String> {
        self.events()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::SystemChat { message, .. } => Some(message.plain_text()),
                _ => None,
            })
            .collect()
    }

    fn game_time(&self) -> i64 {
        self.server.world.level.game_time
    }

    fn pulse(&mut self) {
        self.server.tick();
        self.server.finish_tick_timing(Duration::from_millis(2));
    }
}

#[test]
fn rate_changes_the_target_and_tells_every_client() {
    let mut rig = rig();
    assert_eq!(rig.run("tick rate 40"), CommandOutcome::Success);
    assert_eq!(rig.server.tick_rate.rate(), 40.0);
    let events = rig.events();
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::TickingState { rate, frozen: false } if *rate == 40.0
    )));
    assert_eq!(rig.run("tick rate 0.5"), CommandOutcome::Failure);
    assert_eq!(rig.run("tick rate 20000"), CommandOutcome::Failure);
    assert_eq!(rig.run("tick rate fast"), CommandOutcome::Failure);
}

#[test]
fn rate_reports_the_new_rate_with_one_decimal() {
    let mut rig = rig();
    rig.run("tick rate 25");
    assert_eq!(rig.texts(), ["commands.tick.rate.success 25.0"]);
}

#[test]
fn a_frozen_game_stops_the_clock_until_it_is_stepped() {
    let mut rig = rig();
    assert_eq!(rig.run("tick freeze"), CommandOutcome::Success);
    assert!(
        rig.events()
            .iter()
            .any(|event| matches!(event, ClientEvent::TickingState { frozen: true, .. }))
    );
    let frozen_at = rig.game_time();
    rig.pulse();
    rig.pulse();
    assert_eq!(rig.game_time(), frozen_at);
    assert_eq!(rig.run("tick step 3"), CommandOutcome::Success);
    assert!(
        rig.events()
            .iter()
            .any(|event| matches!(event, ClientEvent::TickingStep(3)))
    );
    for _ in 0..5 {
        rig.pulse();
    }
    assert_eq!(rig.game_time(), frozen_at + 3);
    assert_eq!(rig.run("tick unfreeze"), CommandOutcome::Success);
    rig.pulse();
    assert_eq!(rig.game_time(), frozen_at + 4);
}

#[test]
fn step_needs_a_frozen_game_and_stop_needs_a_step() {
    let mut rig = rig();
    assert_eq!(rig.run("tick step"), CommandOutcome::Failure);
    assert_eq!(rig.texts(), ["commands.tick.step.fail"]);
    assert_eq!(rig.run("tick step stop"), CommandOutcome::Failure);
    rig.run("tick freeze");
    rig.texts();
    assert_eq!(rig.run("tick step 10"), CommandOutcome::Success);
    assert_eq!(rig.texts(), ["commands.tick.step.success 10"]);
    assert_eq!(rig.run("tick step stop"), CommandOutcome::Success);
    assert_eq!(rig.texts(), ["commands.tick.step.stop.success"]);
}

#[test]
fn a_sprint_runs_its_ticks_then_reports_and_restores_the_freeze() {
    let mut rig = rig();
    rig.run("tick freeze");
    rig.texts();
    assert_eq!(rig.run("tick sprint 3"), CommandOutcome::Success);
    assert_eq!(rig.texts(), ["commands.tick.status.sprinting"]);
    assert!(rig.server.tick_rate.is_sprinting());
    let start = rig.game_time();
    for _ in 0..3 {
        rig.pulse();
    }
    assert_eq!(rig.game_time(), start + 3);
    assert!(!rig.server.tick_rate.is_sprinting());
    assert!(rig.server.tick_rate.is_frozen());
    let texts = rig.texts();
    assert!(
        texts
            .iter()
            .any(|text| text.starts_with("commands.tick.sprint.report")),
        "{texts:?}"
    );
    assert_eq!(rig.run("tick sprint stop"), CommandOutcome::Failure);
    assert_eq!(rig.texts(), ["commands.tick.sprint.stop.fail"]);
}

#[test]
fn query_names_the_state_the_rate_and_the_percentiles() {
    let mut rig = rig();
    rig.pulse();
    rig.texts();
    assert_eq!(rig.run("tick query"), CommandOutcome::Success);
    let texts = rig.texts();
    assert_eq!(texts[0], "commands.tick.status.running");
    assert_eq!(texts[1], "commands.tick.query.rate.running 20.0 2.0 50.0");
    assert!(texts[2].starts_with("commands.tick.query.percentiles 2.0 2.0 2.0 1"));
    rig.run("tick freeze");
    rig.texts();
    rig.run("tick query");
    assert_eq!(rig.texts()[0], "commands.tick.status.frozen");
}

#[test]
fn freezing_cancels_a_sprint_and_a_pending_step() {
    let mut rig = rig();
    rig.run("tick sprint 100");
    rig.run("tick freeze");
    assert!(!rig.server.tick_rate.is_sprinting());
    assert!(rig.server.tick_rate.is_frozen());
}
