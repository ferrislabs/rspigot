use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rspigot_core::events::ClientEvent;
use rspigot_core::ports::ClientLink;
use rspigot_domain::text::Component;
use tokio::sync::mpsc::Sender;
use tokio::sync::mpsc::error::TrySendError;

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
    sender: Sender<Outgoing>,
    connected: Arc<AtomicBool>,
    suspended: Arc<AtomicBool>,
}

impl NetLink {
    pub fn new(sender: Sender<Outgoing>, connected: Arc<AtomicBool>) -> Self {
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
        self.push(Outgoing::Packet { id, body });
    }

    pub fn close(&self) {
        self.push(Outgoing::Close);
    }

    pub fn pong(&self, payload: i64) {
        self.push(Outgoing::Pong(payload));
    }

    fn push(&self, message: Outgoing) -> bool {
        match self.sender.try_send(message) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) => {
                if self.connected.swap(false, Ordering::AcqRel) {
                    tracing::warn!(
                        capacity = self.sender.max_capacity(),
                        "outbound queue full, dropping the client"
                    );
                }
                false
            }
            Err(TrySendError::Closed(_)) => {
                self.connected.store(false, Ordering::Release);
                false
            }
        }
    }
}

impl ClientLink for NetLink {
    fn send(&self, event: ClientEvent) {
        if self.suspended.load(Ordering::Acquire) {
            return;
        }
        let reconfigure = matches!(event, ClientEvent::Reconfigure);
        if self.connected.load(Ordering::Acquire)
            && self.push(Outgoing::Event(Box::new(event)))
            && reconfigure
        {
            self.suspended.store(true, Ordering::Release);
        }
    }

    fn disconnect(&self, reason: Component) {
        self.push(Outgoing::Disconnect(reason));
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc::channel;

    use super::*;

    fn link() -> (NetLink, tokio::sync::mpsc::Receiver<Outgoing>) {
        link_with_capacity(64)
    }

    fn link_with_capacity(capacity: usize) -> (NetLink, tokio::sync::mpsc::Receiver<Outgoing>) {
        let (sender, receiver) = channel(capacity);
        (
            NetLink::new(sender, Arc::new(AtomicBool::new(true))),
            receiver,
        )
    }

    #[test]
    fn a_full_outbound_queue_marks_the_link_disconnected() {
        let (link, mut receiver) = link_with_capacity(2);
        link.send(ClientEvent::TickingStep(1));
        link.send(ClientEvent::TickingStep(2));
        assert!(link.is_connected());
        link.send(ClientEvent::TickingStep(3));
        assert!(!link.is_connected());
        link.send(ClientEvent::TickingStep(4));
        assert!(
            matches!(receiver.try_recv(), Ok(Outgoing::Event(event)) if *event == ClientEvent::TickingStep(1))
        );
        assert!(
            matches!(receiver.try_recv(), Ok(Outgoing::Event(event)) if *event == ClientEvent::TickingStep(2))
        );
        assert!(receiver.try_recv().is_err());
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
