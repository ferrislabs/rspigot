use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rspigot_core::plugins::ToPlugin;
use rspigot_core::ports::PluginOutbox;
use tokio::sync::mpsc::Sender;
use tokio::sync::mpsc::error::TrySendError;

#[derive(Clone, Default)]
pub(crate) struct Overflow(Arc<AtomicBool>);

impl Overflow {
    pub(crate) fn happened(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

pub(crate) struct Outbox {
    sender: Sender<ToPlugin>,
    overflow: Overflow,
}

impl Outbox {
    pub(crate) fn new(sender: Sender<ToPlugin>, overflow: Overflow) -> Self {
        Self { sender, overflow }
    }
}

impl PluginOutbox for Outbox {
    fn send(&self, message: ToPlugin) {
        if let Err(TrySendError::Full(_)) = self.sender.try_send(message) {
            self.overflow.0.store(true, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::*;

    #[test]
    fn a_full_queue_drops_the_message_and_reports_the_overflow() {
        let (sender, mut receiver) = mpsc::channel(2);
        let overflow = Overflow::default();
        let outbox = Outbox::new(sender, overflow.clone());
        outbox.send(ToPlugin::Accepted);
        outbox.send(ToPlugin::Accepted);
        assert!(!overflow.happened());
        outbox.send(ToPlugin::Accepted);
        assert!(overflow.happened());
        assert!(receiver.try_recv().is_ok());
        assert!(receiver.try_recv().is_ok());
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn a_closed_queue_is_not_an_overflow() {
        let (sender, receiver) = mpsc::channel(1);
        drop(receiver);
        let overflow = Overflow::default();
        Outbox::new(sender, overflow.clone()).send(ToPlugin::Accepted);
        assert!(!overflow.happened());
    }
}
