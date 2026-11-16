use rspigot_domain::player::PlayerId;

use super::{CommandOutcome, CommandSender, execute};
use crate::end_tests::{Tap, arrive};
use crate::events::{ClientEvent, Inbound};
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
}

#[test]
fn transfer_sends_the_player_to_another_server() {
    let mut rig = rig();
    assert_eq!(
        rig.run("transfer play.example.org 25570"),
        CommandOutcome::Success
    );
    let events = rig.events();
    assert!(events.iter().any(|event| matches!(
        event,
        ClientEvent::Transfer { host, port } if host == "play.example.org" && *port == 25570
    )));
    assert_eq!(
        rig.run("transfer play.example.org"),
        CommandOutcome::Success
    );
    assert!(rig.events().iter().any(|event| matches!(
        event,
        ClientEvent::Transfer { port, .. } if *port == 25565
    )));
}

#[test]
fn transfer_names_the_target_or_counts_them() {
    let mut rig = rig();
    rig.run("transfer a.b 1 @s");
    assert_eq!(
        rig.texts(),
        ["commands.transfer.success.single Alice a.b 1"]
    );
    rig.run("transfer a.b 2 @a");
    assert_eq!(
        rig.texts(),
        ["commands.transfer.success.single Alice a.b 2"]
    );
}

#[test]
fn transfer_checks_the_port_and_needs_a_player_from_the_console() {
    let mut rig = rig();
    assert_eq!(rig.run("transfer a.b 0"), CommandOutcome::Failure);
    assert_eq!(rig.run("transfer a.b 65536"), CommandOutcome::Failure);
    assert_eq!(rig.run("transfer a.b port"), CommandOutcome::Failure);
    assert_eq!(rig.run("transfer"), CommandOutcome::Failure);
    rig.server
        .handle(Inbound::Console("transfer a.b 25565".to_owned()));
    rig.server.deliver();
    assert!(
        !rig.events()
            .iter()
            .any(|event| matches!(event, ClientEvent::Transfer { .. }))
    );
}

#[test]
fn setidletimeout_stores_the_minutes_and_says_when_it_is_off() {
    let mut rig = rig();
    assert_eq!(rig.run("setidletimeout 5"), CommandOutcome::Success);
    assert_eq!(rig.server.config.player_idle_timeout, 5);
    assert_eq!(rig.texts(), ["commands.setidletimeout.success 5"]);
    assert_eq!(rig.run("setidletimeout 0"), CommandOutcome::Success);
    assert_eq!(rig.server.config.player_idle_timeout, 0);
    assert_eq!(rig.texts(), ["commands.setidletimeout.success.disabled"]);
    assert_eq!(rig.run("setidletimeout -1"), CommandOutcome::Failure);
    assert_eq!(rig.run("setidletimeout soon"), CommandOutcome::Failure);
}

#[test]
fn debug_start_and_stop_report_the_ticks_that_ran() {
    let mut rig = rig();
    assert_eq!(rig.run("debug stop"), CommandOutcome::Failure);
    assert_eq!(rig.texts(), ["commands.debug.notRunning"]);
    assert_eq!(rig.run("debug start"), CommandOutcome::Success);
    assert_eq!(rig.texts(), ["commands.debug.started"]);
    assert_eq!(rig.run("debug start"), CommandOutcome::Failure);
    rig.texts();
    for _ in 0..4 {
        rig.server.tick();
    }
    assert_eq!(rig.run("debug stop"), CommandOutcome::Success);
    let texts = rig.texts();
    assert_eq!(texts.len(), 1);
    let parts: Vec<&str> = texts[0].split(' ').collect();
    assert_eq!(parts[0], "commands.debug.stopped");
    assert_eq!(parts[2], "4");
    assert_eq!(rig.run("debug"), CommandOutcome::Failure);
}
