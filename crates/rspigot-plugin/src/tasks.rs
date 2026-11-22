use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const TASK_RESOLUTION: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub struct TaskClock {
    now: Instant,
    next: HashMap<&'static str, Instant>,
}

impl TaskClock {
    pub fn new(now: Instant) -> Self {
        Self {
            now,
            next: HashMap::new(),
        }
    }

    pub fn advance(&mut self, now: Instant) {
        self.now = now;
    }

    pub fn due(&mut self, key: &'static str, every: Duration) -> bool {
        let now = self.now;
        let next = self.next.entry(key).or_insert_with(|| now + every);
        if now < *next {
            return false;
        }
        *next = now + every;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_task_runs_once_per_period_and_not_before_the_first_one_elapsed() {
        let start = Instant::now();
        let mut clock = TaskClock::new(start);
        let every = Duration::from_secs(10);
        assert!(!clock.due("a", every));
        clock.advance(start + Duration::from_secs(9));
        assert!(!clock.due("a", every));
        clock.advance(start + Duration::from_secs(10));
        assert!(clock.due("a", every));
        assert!(!clock.due("a", every));
        clock.advance(start + Duration::from_secs(19));
        assert!(!clock.due("a", every));
        clock.advance(start + Duration::from_secs(20));
        assert!(clock.due("a", every));
    }

    #[test]
    fn tasks_are_scheduled_independently() {
        let start = Instant::now();
        let mut clock = TaskClock::new(start);
        assert!(!clock.due("slow", Duration::from_secs(60)));
        assert!(!clock.due("fast", Duration::from_secs(1)));
        clock.advance(start + Duration::from_secs(1));
        assert!(!clock.due("slow", Duration::from_secs(60)));
        assert!(clock.due("fast", Duration::from_secs(1)));
    }
}
