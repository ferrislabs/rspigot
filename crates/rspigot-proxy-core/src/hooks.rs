use std::future::Future;
use std::net::IpAddr;

use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerView {
    pub id: Uuid,
    pub name: String,
    pub address: IpAddr,
    pub server: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectCause {
    Join,
    Plugin,
    Fallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectOutcome {
    Connected,
    AlreadyThere,
    Denied(String),
    Unreachable,
    UnknownServer,
    Offline,
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

pub trait ProxyHooks: Send + Sync + 'static {
    fn login(&self, _player: &PlayerView) {}

    fn disconnect(&self, _player: &PlayerView) {}

    fn connected(&self, _player: &PlayerView, _server: &str, _previous: Option<&str>) {}

    fn command(&self, _player: &PlayerView, _invocation: crate::Invocation) {}

    fn initial_server(
        &self,
        _player: PlayerView,
        _candidates: Vec<String>,
    ) -> impl Future<Output = Verdict> + Send {
        async { Verdict::Allow }
    }

    fn pre_connect(
        &self,
        _player: PlayerView,
        _target: String,
        _cause: ConnectCause,
    ) -> impl Future<Output = Verdict> + Send {
        async { Verdict::Allow }
    }

    fn kicked(
        &self,
        _player: PlayerView,
        _server: String,
        _reason: String,
        _fallbacks: Vec<String>,
    ) -> impl Future<Output = Verdict> + Send {
        async { Verdict::Allow }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoHooks;

impl ProxyHooks for NoHooks {}
