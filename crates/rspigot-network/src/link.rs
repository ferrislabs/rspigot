use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rspigot_core::events::ClientEvent;
use rspigot_core::ports::ClientLink;
use rspigot_domain::text::Component;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug)]
pub enum Outgoing {
    Event(Box<ClientEvent>),
    Disconnect(Component),
    Pong(i64),
    Packet { id: i32, body: Vec<u8> },
    Close,
}

#[derive(Clone)]
pub struct NetLink {
    sender: UnboundedSender<Outgoing>,
    connected: Arc<AtomicBool>,
    suspended: Arc<AtomicBool>,
}

impl NetLink {
    pub fn new(sender: UnboundedSender<Outgoing>, connected: Arc<AtomicBool>) -> Self {
        Self {
            sender,
            connected,
            suspended: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn suspension(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.suspended)
    }

    pub fn is_suspended(&self) -> bool {
        self.suspended.load(Ordering::Acquire)
    }

    pub fn resume(&self) {
        self.suspended.store(false, Ordering::Release);
    }

    pub fn raw(&self, id: i32, body: Vec<u8>) {
        let _ = self.sender.send(Outgoing::Packet { id, body });
    }

    pub fn close(&self) {
        let _ = self.sender.send(Outgoing::Close);
    }

    pub fn pong(&self, payload: i64) {
        let _ = self.sender.send(Outgoing::Pong(payload));
    }
}

impl ClientLink for NetLink {
    fn send(&self, event: ClientEvent) {
        if self.suspended.load(Ordering::Acquire) {
            return;
        }
        let reconfigure = matches!(event, ClientEvent::Reconfigure);
        if self.connected.load(Ordering::Acquire)
            && self.sender.send(Outgoing::Event(Box::new(event))).is_err()
        {
            self.connected.store(false, Ordering::Release);
        } else if reconfigure {
            self.suspended.store(true, Ordering::Release);
        }
    }

    fn disconnect(&self, reason: Component) {
        if self.sender.send(Outgoing::Disconnect(reason)).is_err() {
            self.connected.store(false, Ordering::Release);
        }
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc::unbounded_channel;

    use super::*;

    fn link() -> (NetLink, tokio::sync::mpsc::UnboundedReceiver<Outgoing>) {
        let (sender, receiver) = unbounded_channel();
        (
            NetLink::new(sender, Arc::new(AtomicBool::new(true))),
            receiver,
        )
    }

    #[test]
    fn reconfiguring_suspends_the_play_events_until_the_link_resumes() {
        let (link, mut receiver) = link();
        link.send(ClientEvent::Reconfigure);
        assert!(link.is_suspended());
        link.send(ClientEvent::TickingStep(1));
        link.raw(3, vec![]);
        assert!(
            matches!(receiver.try_recv(), Ok(Outgoing::Event(event)) if *event == ClientEvent::Reconfigure)
        );
        assert!(matches!(
            receiver.try_recv(),
            Ok(Outgoing::Packet { id: 3, .. })
        ));
        assert!(receiver.try_recv().is_err());
        link.resume();
        link.send(ClientEvent::TickingStep(2));
        assert!(
            matches!(receiver.try_recv(), Ok(Outgoing::Event(event)) if *event == ClientEvent::TickingStep(2))
        );
    }
}
