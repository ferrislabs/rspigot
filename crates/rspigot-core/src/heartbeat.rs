use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickPhase {
    Idle,
    Inbox,
    Tick,
    Deliver,
}

impl TickPhase {
    const ALL: [Self; 4] = [Self::Idle, Self::Inbox, Self::Tick, Self::Deliver];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Idle => "waiting for the next tick",
            Self::Inbox => "handling queued messages",
            Self::Tick => "ticking the world",
            Self::Deliver => "delivering events",
        }
    }
}

#[derive(Debug)]
struct State {
    started: Instant,
    beat_ms: AtomicU64,
    armed: AtomicBool,
    stopped: AtomicBool,
    ticks: AtomicU64,
    phase: AtomicU8,
}

#[derive(Debug, Clone)]
pub struct Heartbeat(Arc<State>);

impl Default for Heartbeat {
    fn default() -> Self {
        Self::new()
    }
}

impl Heartbeat {
    pub fn new() -> Self {
        Self(Arc::new(State {
            started: Instant::now(),
            beat_ms: AtomicU64::new(0),
            armed: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            ticks: AtomicU64::new(0),
            phase: AtomicU8::new(0),
        }))
    }

    pub fn beat(&self, tick: u64) {
        let elapsed = self.0.started.elapsed().as_millis() as u64;
        self.0.beat_ms.store(elapsed, Ordering::Release);
        self.0.ticks.store(tick, Ordering::Release);
        self.0.armed.store(true, Ordering::Release);
    }

    pub fn mark(&self, phase: TickPhase) {
        self.0.phase.store(phase as u8, Ordering::Release);
    }

    pub fn stop(&self) {
        self.0.stopped.store(true, Ordering::Release);
    }

    pub fn stopped(&self) -> bool {
        self.0.stopped.load(Ordering::Acquire)
    }

    pub fn ticks(&self) -> u64 {
        self.0.ticks.load(Ordering::Acquire)
    }

    pub fn phase(&self) -> TickPhase {
        let index = usize::from(self.0.phase.load(Ordering::Acquire));
        TickPhase::ALL
            .get(index)
            .copied()
            .unwrap_or(TickPhase::Idle)
    }

    pub fn since_beat(&self) -> Option<Duration> {
        if !self.0.armed.load(Ordering::Acquire) {
            return None;
        }
        let beat = self.0.beat_ms.load(Ordering::Acquire);
        let now = self.0.started.elapsed().as_millis() as u64;
        Some(Duration::from_millis(now.saturating_sub(beat)))
    }
}

use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub fn with_heartbeat(mut self, heartbeat: Heartbeat) -> Self {
        self.heartbeat = Some(heartbeat);
        self
    }

    pub(crate) fn mark_phase(&self, phase: TickPhase) {
        if let Some(heartbeat) = &self.heartbeat {
            heartbeat.mark(phase);
        }
    }

    pub(crate) fn beat_heart(&self) {
        if let Some(heartbeat) = &self.heartbeat {
            heartbeat.beat(self.tick);
        }
    }

    pub(crate) fn stop_heart(&self) {
        if let Some(heartbeat) = &self.heartbeat {
            heartbeat.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_heartbeat_is_unarmed_until_the_first_beat() {
        let heartbeat = Heartbeat::new();
        assert_eq!(heartbeat.since_beat(), None);
        heartbeat.beat(7);
        assert!(heartbeat.since_beat().is_some());
        assert_eq!(heartbeat.ticks(), 7);
    }

    #[test]
    fn the_phase_and_stop_flag_are_shared_between_clones() {
        let heartbeat = Heartbeat::new();
        let seen = heartbeat.clone();
        assert_eq!(seen.phase(), TickPhase::Idle);
        heartbeat.mark(TickPhase::Tick);
        heartbeat.stop();
        assert_eq!(seen.phase(), TickPhase::Tick);
        assert!(seen.stopped());
    }
}
