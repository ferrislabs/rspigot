use rspigot_domain::player::PlayerId;

use crate::plugins::actions::Reply;
use crate::plugins::{Answer, ExtraAnswer, ToPlugin};
use crate::ports::ClientLink;
use crate::server::Server;

const WAIT_TICKS: u64 = 600;

#[derive(Debug)]
struct Wait {
    player: PlayerId,
    key: String,
    reply: Reply,
    deadline: u64,
}

#[derive(Debug, Default)]
pub(crate) struct CookieWaits {
    waiting: Vec<Wait>,
}

impl<L: ClientLink> Server<L> {
    pub(super) fn begin_cookie_wait(
        &mut self,
        connection: u64,
        request: u64,
        player: PlayerId,
        key: &str,
    ) -> Option<Answer> {
        if !self.players.contains_key(&player) {
            return Some(Answer::NotFound);
        }
        if let Err(error) = self.request_cookie(player, key) {
            return Some(Answer::Refused(error.to_string()));
        }
        let deadline = self.current_tick() + WAIT_TICKS;
        self.plugins.cookie_waits.waiting.push(Wait {
            player,
            key: key.to_owned(),
            reply: Reply {
                connection,
                request,
            },
            deadline,
        });
        None
    }

    pub(crate) fn answer_cookie_waits(
        &mut self,
        player: PlayerId,
        key: &str,
        payload: Option<&Vec<u8>>,
    ) {
        let waits = &mut self.plugins.cookie_waits.waiting;
        let mut answered = Vec::new();
        waits.retain(|wait| {
            let matches = wait.player == player && wait.key == key;
            if matches {
                answered.push(wait.reply);
            }
            !matches
        });
        for reply in answered {
            self.plugins.send(
                reply.connection,
                ToPlugin::Answer {
                    request: reply.request,
                    answer: Answer::Extra(ExtraAnswer::Cookie(payload.cloned())),
                },
            );
        }
    }

    pub(crate) fn expire_cookie_waits(&mut self) {
        if self.plugins.cookie_waits.waiting.is_empty() {
            return;
        }
        let now = self.current_tick();
        let mut expired = Vec::new();
        self.plugins.cookie_waits.waiting.retain(|wait| {
            let live = wait.deadline > now;
            if !live {
                expired.push(wait.reply);
            }
            live
        });
        for reply in expired {
            self.plugins.send(
                reply.connection,
                ToPlugin::Answer {
                    request: reply.request,
                    answer: Answer::Refused("the client did not answer".to_owned()),
                },
            );
        }
    }

    pub(crate) fn forget_cookie_waits(&mut self, connection: u64) {
        self.plugins
            .cookie_waits
            .waiting
            .retain(|wait| wait.reply.connection != connection);
    }
}
