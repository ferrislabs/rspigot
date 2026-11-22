use std::time::{Duration, Instant};

pub const MAX_HANDLERS: usize = 512;
pub const MAX_DEPENDENCIES: usize = 64;
pub const MAX_NAME_LENGTH: usize = 64;
pub const OUTBOX_CAPACITY: usize = 16_384;
pub const MESSAGES_PER_SECOND: u32 = 4_000;
pub const MESSAGE_BURST: u32 = 8_000;

pub(crate) struct Limiter {
    rate: f64,
    burst: f64,
    tokens: f64,
    last: Instant,
}

impl Limiter {
    pub(crate) fn new(rate: u32, burst: u32, now: Instant) -> Self {
        Self {
            rate: f64::from(rate),
            burst: f64::from(burst),
            tokens: f64::from(burst),
            last: now,
        }
    }

    pub(crate) fn wait(&mut self, now: Instant) -> Duration {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.last = now;
        self.tokens = (self.tokens + elapsed * self.rate).min(self.burst);
        if self.tokens >= 1.0 {
            Duration::ZERO
        } else {
            Duration::from_secs_f64((1.0 - self.tokens) / self.rate)
        }
    }

    pub(crate) fn take(&mut self) {
        self.tokens = (self.tokens - 1.0).max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_passes_then_the_rate_throttles() {
        let start = Instant::now();
        let mut limiter = Limiter::new(100, 10, start);
        for _ in 0..10 {
            assert_eq!(limiter.wait(start), Duration::ZERO);
            limiter.take();
        }
        let wait = limiter.wait(start);
        assert!(wait > Duration::ZERO && wait <= Duration::from_millis(10));
        assert_eq!(limiter.wait(start + wait), Duration::ZERO);
    }

    #[test]
    fn tokens_refill_up_to_the_burst_only() {
        let start = Instant::now();
        let mut limiter = Limiter::new(100, 10, start);
        for _ in 0..10 {
            limiter.take();
        }
        let later = start + Duration::from_secs(60);
        assert_eq!(limiter.wait(later), Duration::ZERO);
        for _ in 0..10 {
            limiter.take();
        }
        assert!(limiter.wait(later) > Duration::ZERO);
    }
}
