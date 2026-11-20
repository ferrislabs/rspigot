use std::future::Future;

use crate::commands::{CommandSpec, Invocation};
use crate::events::Call;
use crate::listener::{HandlerSpec, Outcome};
use crate::server::Server;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandModule {
    pub name: String,
    pub first: u32,
    pub last: u32,
}

impl CommandModule {
    pub(crate) fn contains(&self, id: u32) -> bool {
        self.first <= id && id <= self.last
    }

    pub fn within(self, field: &str) -> Self {
        Self {
            name: format!("{field}.{}", self.name),
            ..self
        }
    }
}

pub fn push_own_module<M: ?Sized>(out: &mut Vec<CommandModule>, base: u32, slots: u32) {
    if slots == 0 {
        return;
    }
    let full = std::any::type_name::<M>();
    let name = full.rsplit("::").next().unwrap_or(full);
    out.push(CommandModule {
        name: name.to_owned(),
        first: base + 1,
        last: base.saturating_add(slots),
    });
}

pub trait Children {
    const HANDLER_SLOTS: u32;
    const COMMAND_SLOTS: u32;

    fn handlers(base: u32, out: &mut Vec<HandlerSpec>);

    fn commands(base: u32, out: &mut Vec<CommandSpec>);

    fn command_modules(base: u32, out: &mut Vec<CommandModule>);

    fn dispatch(&self, base: u32, handler: u32, call: Call)
    -> impl Future<Output = Outcome> + Send;

    fn enable(&self, server: &Server) -> impl Future<Output = ()> + Send;

    fn disable(&self) -> impl Future<Output = ()> + Send;

    fn invoke(&self, base: u32, invocation: Invocation) -> impl Future<Output = ()> + Send;
}

pub trait Kids<T> {
    const PRESENT: bool;
    const HANDLER_SLOTS: u32;
    const COMMAND_SLOTS: u32;

    fn handlers(&self, base: u32, out: &mut Vec<HandlerSpec>);

    fn commands(&self, base: u32, out: &mut Vec<CommandSpec>);

    fn command_modules(&self, base: u32, out: &mut Vec<CommandModule>);

    fn dispatch(
        &self,
        owner: &T,
        base: u32,
        handler: u32,
        call: Call,
    ) -> impl Future<Output = Outcome> + Send;

    fn enable(&self, owner: &T, server: &Server) -> impl Future<Output = ()> + Send;

    fn disable(&self, owner: &T) -> impl Future<Output = ()> + Send;

    fn invoke(
        &self,
        owner: &T,
        base: u32,
        invocation: Invocation,
    ) -> impl Future<Output = ()> + Send;
}

#[derive(Debug, Clone, Copy)]
pub struct Present;

#[derive(Debug, Clone, Copy)]
pub struct Absent;

pub trait Detect {
    const __RSPIGOT_KIDS: Absent = Absent;
}

impl<T: ?Sized> Detect for T {}

impl<T: Children> Kids<T> for Present {
    const PRESENT: bool = true;
    const HANDLER_SLOTS: u32 = T::HANDLER_SLOTS;
    const COMMAND_SLOTS: u32 = T::COMMAND_SLOTS;

    fn handlers(&self, base: u32, out: &mut Vec<HandlerSpec>) {
        T::handlers(base, out);
    }

    fn commands(&self, base: u32, out: &mut Vec<CommandSpec>) {
        T::commands(base, out);
    }

    fn command_modules(&self, base: u32, out: &mut Vec<CommandModule>) {
        T::command_modules(base, out);
    }

    fn dispatch(
        &self,
        owner: &T,
        base: u32,
        handler: u32,
        call: Call,
    ) -> impl Future<Output = Outcome> + Send {
        owner.dispatch(base, handler, call)
    }

    fn enable(&self, owner: &T, server: &Server) -> impl Future<Output = ()> + Send {
        owner.enable(server)
    }

    fn disable(&self, owner: &T) -> impl Future<Output = ()> + Send {
        owner.disable()
    }

    fn invoke(
        &self,
        owner: &T,
        base: u32,
        invocation: Invocation,
    ) -> impl Future<Output = ()> + Send {
        owner.invoke(base, invocation)
    }
}

impl<T> Kids<T> for Absent {
    const PRESENT: bool = false;
    const HANDLER_SLOTS: u32 = 0;
    const COMMAND_SLOTS: u32 = 0;

    fn handlers(&self, _base: u32, _out: &mut Vec<HandlerSpec>) {}

    fn commands(&self, _base: u32, _out: &mut Vec<CommandSpec>) {}

    fn command_modules(&self, _base: u32, _out: &mut Vec<CommandModule>) {}

    fn dispatch(
        &self,
        _owner: &T,
        _base: u32,
        _handler: u32,
        _call: Call,
    ) -> impl Future<Output = Outcome> + Send {
        std::future::ready(Outcome::Observed)
    }

    fn enable(&self, _owner: &T, _server: &Server) -> impl Future<Output = ()> + Send {
        std::future::ready(())
    }

    fn disable(&self, _owner: &T) -> impl Future<Output = ()> + Send {
        std::future::ready(())
    }

    fn invoke(
        &self,
        _owner: &T,
        _base: u32,
        _invocation: Invocation,
    ) -> impl Future<Output = ()> + Send {
        std::future::ready(())
    }
}

pub const fn present<T, K: Kids<T>>(_kids: &K) -> bool {
    K::PRESENT
}

pub const fn kids_handler_slots<T, K: Kids<T>>(_kids: &K) -> u32 {
    K::HANDLER_SLOTS
}

pub const fn kids_command_slots<T, K: Kids<T>>(_kids: &K) -> u32 {
    K::COMMAND_SLOTS
}

pub const fn handler_slots(slots: u32) -> u32 {
    assert!(
        slots != u32::MAX,
        "a hand-written `Listeners` impl must set `HANDLER_SLOTS` to be used as a module"
    );
    slots
}

pub const fn command_slots(slots: u32) -> u32 {
    assert!(
        slots != u32::MAX,
        "a hand-written `Listeners` impl must set `COMMAND_SLOTS` to be used as a module"
    );
    slots
}

pub fn push_handlers(out: &mut Vec<HandlerSpec>, base: u32, specs: Vec<HandlerSpec>, slots: u32) {
    out.extend(specs.into_iter().map(|spec| HandlerSpec {
        id: if spec.id == u32::MAX {
            base + slots
        } else {
            base + spec.id
        },
        ..spec
    }));
}

pub fn push_commands(out: &mut Vec<CommandSpec>, base: u32, specs: Vec<CommandSpec>) {
    out.extend(specs.into_iter().map(|spec| CommandSpec {
        id: base + spec.id,
        ..spec
    }));
}

pub fn push_command_modules(out: &mut Vec<CommandModule>, field: &str, inner: Vec<CommandModule>) {
    out.extend(inner.into_iter().map(|module| module.within(field)));
}

pub const fn local_handler(handler: u32, base: u32, slots: u32, tab: bool) -> Option<u32> {
    if handler <= base || handler - base > slots {
        return None;
    }
    let local = handler - base;
    if tab && local == slots {
        Some(u32::MAX)
    } else {
        Some(local)
    }
}

pub const fn owns_command(command: u32, base: u32, slots: u32) -> bool {
    command > base && command - base <= slots
}
