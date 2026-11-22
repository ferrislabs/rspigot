use std::time::{Duration, Instant};

const WINDOW: Duration = Duration::from_secs(1);
const KEEP: f32 = 0.75;
const NEW: f32 = 0.25;

pub(crate) const DISCONNECT_KEY: &str = "disconnect.exceeded_packet_rate";

#[derive(Debug)]
pub(crate) struct RateLimiter {
    limit: u32,
    average: f32,
    count: u32,
    window_start: Instant,
}

impl RateLimiter {
    pub(crate) fn new(limit: u32, now: Instant) -> Self {
        Self {
            limit,
            average: 0.0,
            count: 0,
            window_start: now,
        }
    }

    pub(crate) fn record(&mut self, now: Instant) -> bool {
        if self.limit == 0 {
            return false;
        }
        while now.saturating_duration_since(self.window_start) >= WINDOW {
            self.average = self.average * KEEP + self.count as f32 * NEW;
            self.count = 0;
            self.window_start += WINDOW;
        }
        self.count += 1;
        self.average > self.limit as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flood(limiter: &mut RateLimiter, start: Instant, seconds: u64, per_second: u32) -> bool {
        let mut exceeded = false;
        for second in 0..seconds {
            for _ in 0..per_second {
                exceeded |= limiter.record(start + Duration::from_secs(second));
            }
        }
        exceeded |= limiter.record(start + Duration::from_secs(seconds));
        exceeded
    }

    #[test]
    fn a_limit_of_zero_never_trips() {
        let start = Instant::now();
        let mut limiter = RateLimiter::new(0, start);
        assert!(!flood(&mut limiter, start, 10, 10_000));
    }

    #[test]
    fn a_sustained_flood_above_the_limit_trips_the_average() {
        let start = Instant::now();
        let mut limiter = RateLimiter::new(100, start);
        assert!(flood(&mut limiter, start, 10, 1_000));
    }

    #[test]
    fn traffic_under_the_limit_never_trips() {
        let start = Instant::now();
        let mut limiter = RateLimiter::new(100, start);
        assert!(!flood(&mut limiter, start, 20, 90));
    }

    #[test]
    fn one_burst_is_smoothed_by_the_average() {
        let start = Instant::now();
        let mut limiter = RateLimiter::new(100, start);
        assert!(!flood(&mut limiter, start, 1, 300));
    }
}
