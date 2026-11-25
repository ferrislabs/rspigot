mod command;
mod proxy;
mod runtime;
mod text;

use std::future::Future;

pub use rspigot_plugin_protocol::proxy::v1 as wire;
pub use wire::{ConnectOutcome, Hook, Invocation, Player, Server};

pub use crate::command::{CommandBuilder, command};
pub use crate::proxy::{ConnectResult, Proxy};
pub use crate::runtime::{DEFAULT_URL, run, run_with};
pub use crate::text::{Rich, rich};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("`{0}` is not an http or https URL")]
    Url(String),
    #[error("the proxy refused the plugin: {}", .0.message())]
    Refused(tonic::Status),
    #[error("not connected to the proxy")]
    Disconnected,
    #[error("the proxy did not answer in time")]
    Timeout,
    #[error("the proxy answered: {0}")]
    Answer(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny(String),
    Redirect {
        server: String,
        message: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    Join,
    Plugin,
    Fallback,
}

pub trait Plugin: Send + Sync + 'static {
    const NAME: &'static str;
    const VERSION: &'static str;

    fn subscriptions(&self) -> Vec<(Hook, bool)> {
        Vec::new()
    }

    fn commands(&self) -> Vec<wire::CommandSpec> {
        Vec::new()
    }

    fn accepted(
        &self,
        _proxy: &Proxy,
        _servers: &[Server],
        _players: &[Player],
    ) -> impl Future<Output = ()> + Send {
        async {}
    }

    fn login(&self, _proxy: &Proxy, _player: &Player) -> impl Future<Output = ()> + Send {
        async {}
    }

    fn disconnect(&self, _proxy: &Proxy, _player: &Player) -> impl Future<Output = ()> + Send {
        async {}
    }

    fn connected(
        &self,
        _proxy: &Proxy,
        _player: &Player,
        _server: &str,
        _previous: Option<&str>,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }

    fn initial_server(
        &self,
        _proxy: &Proxy,
        _player: &Player,
        _candidates: &[String],
    ) -> impl Future<Output = Verdict> + Send {
        async { Verdict::Allow }
    }

    fn pre_connect(
        &self,
        _proxy: &Proxy,
        _player: &Player,
        _target: &str,
        _cause: Cause,
    ) -> impl Future<Output = Verdict> + Send {
        async { Verdict::Allow }
    }

    fn kicked(
        &self,
        _proxy: &Proxy,
        _player: &Player,
        _server: &str,
        _reason: &str,
        _fallbacks: &[String],
    ) -> impl Future<Output = Verdict> + Send {
        async { Verdict::Allow }
    }

    fn command(&self, _proxy: &Proxy, _invocation: &Invocation) -> impl Future<Output = ()> + Send {
        async {}
    }
}
