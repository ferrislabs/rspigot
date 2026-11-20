use std::time::Duration;

use crossbeam_channel::Sender;

use crate::console::console_text;
use crate::events::Inbound;
use crate::plugins::events::{RemoteServerCommand, RemoteServerCommandPatch};
use crate::plugins::{Continuation, EventKind, EventPayload, Lane, SenderView};
use crate::ports::{ClientLink, RconExecutor};
use crate::server::Server;

const ANSWER_TIMEOUT: Duration = Duration::from_secs(30);
pub const SOURCE_NAME: &str = "Rcon";

#[derive(Debug, Clone)]
pub struct RconCall {
    pub command: String,
    pub reply: Sender<String>,
}

pub struct RconGateway<L> {
    inbound: Sender<Inbound<L>>,
}

impl<L> RconGateway<L> {
    pub fn new(inbound: Sender<Inbound<L>>) -> Self {
        Self { inbound }
    }
}

impl<L: Send + 'static> RconExecutor for RconGateway<L> {
    fn execute(&self, command: &str) -> Option<String> {
        let (reply, answer) = crossbeam_channel::bounded(1);
        self.inbound
            .send(Inbound::Rcon(RconCall {
                command: command.to_owned(),
                reply,
            }))
            .ok()?;
        answer.recv_timeout(ANSWER_TIMEOUT).ok()
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn rcon_line(&mut self, call: RconCall) {
        if !self.listened(EventKind::RemoteServerCommand) {
            self.finish_rcon(call, RemoteServerCommandPatch::default());
            return;
        }
        let command = call.command.clone();
        self.fire(
            Lane::Console,
            Continuation::RemoteServerCommand(Box::new(call)),
            move |_| {
                Some(EventPayload::RemoteServerCommand(RemoteServerCommand {
                    command,
                    sender: SenderView::Console,
                }))
            },
        );
    }

    pub(crate) fn finish_rcon(&mut self, call: RconCall, patch: RemoteServerCommandPatch) {
        let output = if self.running {
            let line = patch.command.as_deref().unwrap_or(&call.command);
            self.run_captured_console(SOURCE_NAME, line)
                .iter()
                .map(console_text)
                .collect()
        } else {
            String::new()
        };
        let _ = call.reply.send(output);
    }

    pub(crate) fn cancel_rcon(call: &RconCall) {
        let _ = call.reply.send(String::new());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::end_tests::arrive;

    fn run(server: &mut Server<crate::end_tests::Tap>, line: &str) -> String {
        let (reply, answer) = crossbeam_channel::bounded(1);
        server.handle(Inbound::Rcon(RconCall {
            command: line.to_owned(),
            reply,
        }));
        answer.try_recv().expect("answered")
    }

    #[test]
    fn a_command_answers_with_its_plain_feedback_text() {
        let (mut server, _, _) = arrive();
        let text = run(&mut server, "seed");
        assert!(text.starts_with("Seed: "), "{text}");
    }

    #[test]
    fn a_command_without_feedback_answers_with_nothing() {
        let (mut server, _, _) = arrive();
        assert_eq!(run(&mut server, "say hello"), "");
    }

    #[test]
    fn the_source_has_permission_four_and_the_name_rcon() {
        let (mut server, _, _) = arrive();
        let text = run(&mut server, "op Alice");
        assert!(!text.contains("permission"), "{text}");
    }

    #[test]
    fn nothing_runs_once_the_server_stopped() {
        let (mut server, id, _) = arrive();
        server.running = false;
        assert_eq!(run(&mut server, "kick Alice"), "");
        assert!(server.players.contains_key(&id));
    }

    #[test]
    fn the_capture_does_not_leak_into_the_next_console_line() {
        let (mut server, _, _) = arrive();
        run(&mut server, "seed");
        assert!(server.running_command_name().is_none());
    }
}
