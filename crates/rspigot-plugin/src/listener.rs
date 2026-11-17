use std::future::Future;

use rspigot_plugin_protocol::v1;

use crate::commands::{CommandSpec, Invocation};
use crate::events::{Call, EventKind};
use crate::server::Server;

pub trait Plugin: Listeners + Send + Sync + 'static {
    const NAME: &'static str;
    const VERSION: &'static str;
}

pub trait Listeners {
    fn handlers() -> Vec<HandlerSpec>;

    fn dispatch(&self, call: Call) -> impl Future<Output = Outcome> + Send;

    fn enable(&self, server: &Server) -> impl Future<Output = ()> + Send;

    fn disable(&self) -> impl Future<Output = ()> + Send;

    fn commands() -> Vec<CommandSpec> {
        Vec::new()
    }

    fn invoke(&self, invocation: Invocation) -> impl Future<Output = ()> + Send {
        let _ = invocation;
        async {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Access {
    Observe,
    Decide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum Priority {
    Lowest,
    Low,
    #[default]
    Normal,
    High,
    Highest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HandlerSpec {
    pub id: u32,
    pub event: EventKind,
    pub access: Access,
    pub priority: Priority,
    pub ignore_cancelled: bool,
}

impl HandlerSpec {
    pub(crate) fn to_wire(self) -> v1::Handler {
        let event = self.event.to_wire();
        let access = match self.access {
            Access::Observe => v1::Access::Observe,
            Access::Decide => v1::Access::Decide,
        };
        let priority = match self.priority {
            Priority::Lowest => v1::Priority::Lowest,
            Priority::Low => v1::Priority::Low,
            Priority::Normal => v1::Priority::Normal,
            Priority::High => v1::Priority::High,
            Priority::Highest => v1::Priority::Highest,
        };
        v1::Handler {
            id: self.id,
            event: event as i32,
            access: access as i32,
            priority: priority as i32,
            ignore_cancelled: self.ignore_cancelled,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Observed,
    Decided(Verdict),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Verdict {
    pub(crate) cancel: bool,
    pub(crate) patch: Option<Box<v1::Patch>>,
}

impl Verdict {
    pub(crate) fn into_wire(self, handler: u32) -> v1::Verdict {
        v1::Verdict {
            handler,
            cancel: self.cancel,
            patch: self.patch.map(|patch| *patch),
        }
    }
}
