const DEFAULT_RATE: f32 = 20.0;
const NANOS_PER_SECOND: f64 = 1.0e9;
const SAMPLES: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sprint {
    remaining: i64,
    total: i64,
    elapsed: u64,
    was_frozen: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SprintReport {
    pub ticks: i64,
    pub elapsed_nanos: u64,
}

impl SprintReport {
    pub fn ticks_per_second(&self) -> f64 {
        if self.elapsed_nanos == 0 {
            return f64::INFINITY;
        }
        self.ticks as f64 * NANOS_PER_SECOND / self.elapsed_nanos as f64
    }

    pub fn millis_per_tick(&self) -> f64 {
        if self.ticks == 0 {
            return 0.0;
        }
        self.elapsed_nanos as f64 / self.ticks as f64 / 1.0e6
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TickRate {
    rate: f32,
    frozen: bool,
    steps: i32,
    sprint: Option<Sprint>,
    samples: Vec<u64>,
    next_sample: usize,
}

impl Default for TickRate {
    fn default() -> Self {
        Self {
            rate: DEFAULT_RATE,
            frozen: false,
            steps: 0,
            sprint: None,
            samples: Vec::new(),
            next_sample: 0,
        }
    }
}

impl TickRate {
    pub const MIN_RATE: f32 = 1.0;
    pub const MAX_RATE: f32 = 10_000.0;

    pub fn rate(&self) -> f32 {
        self.rate
    }

    pub fn set_rate(&mut self, rate: f32) {
        self.rate = rate.clamp(Self::MIN_RATE, Self::MAX_RATE);
    }

    pub fn nanos_per_tick(&self) -> u64 {
        (NANOS_PER_SECOND / f64::from(self.rate)) as u64
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    pub fn set_frozen(&mut self, frozen: bool) {
        self.frozen = frozen;
    }

    pub fn is_sprinting(&self) -> bool {
        self.sprint.is_some()
    }

    pub fn is_stepping(&self) -> bool {
        self.steps > 0
    }

    pub fn pending_steps(&self) -> i32 {
        self.steps
    }

    pub fn step_if_frozen(&mut self, ticks: i32) -> bool {
        if !self.frozen {
            return false;
        }
        self.steps = ticks;
        true
    }

    pub fn stop_step(&mut self) -> bool {
        let was = self.steps > 0;
        self.steps = 0;
        was
    }

    pub fn request_sprint(&mut self, ticks: i64) -> bool {
        let was_sprinting = self.sprint.take();
        let was_frozen = was_sprinting.map_or(self.frozen, |sprint| sprint.was_frozen);
        self.sprint = Some(Sprint {
            remaining: ticks,
            total: ticks,
            elapsed: 0,
            was_frozen,
        });
        self.frozen = false;
        was_sprinting.is_some()
    }

    pub fn stop_sprint(&mut self) -> Option<SprintReport> {
        let sprint = self.sprint.take()?;
        self.frozen = sprint.was_frozen;
        Some(SprintReport {
            ticks: sprint.total - sprint.remaining,
            elapsed_nanos: sprint.elapsed,
        })
    }

    pub fn runs_simulation(&mut self) -> bool {
        if !self.frozen {
            return true;
        }
        if self.steps > 0 {
            self.steps -= 1;
            return true;
        }
        false
    }

    pub fn finish_tick(&mut self, nanos: u64) -> Option<SprintReport> {
        self.record(nanos);
        let sprint = self.sprint.as_mut()?;
        sprint.elapsed += nanos;
        sprint.remaining -= 1;
        if sprint.remaining > 0 {
            return None;
        }
        self.stop_sprint()
    }

    fn record(&mut self, nanos: u64) {
        if self.samples.len() < SAMPLES {
            self.samples.push(nanos);
        } else {
            self.samples[self.next_sample] = nanos;
        }
        self.next_sample = (self.next_sample + 1) % SAMPLES;
    }

    pub fn average_nanos(&self) -> u64 {
        if self.samples.is_empty() {
            return 0;
        }
        self.samples.iter().sum::<u64>() / self.samples.len() as u64
    }

    pub fn sorted_samples(&self) -> Vec<u64> {
        let mut sorted = self.samples.clone();
        sorted.sort_unstable();
        sorted
    }

    pub fn is_lagging(&self) -> bool {
        !self.frozen && self.nanos_per_tick() < self.average_nanos()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_twenty_ticks_per_second_running() {
        let state = TickRate::default();
        assert_eq!(state.rate(), 20.0);
        assert_eq!(state.nanos_per_tick(), 50_000_000);
        assert!(!state.is_frozen());
    }

    #[test]
    fn the_rate_stays_between_one_and_ten_thousand() {
        let mut state = TickRate::default();
        state.set_rate(0.1);
        assert_eq!(state.rate(), 1.0);
        state.set_rate(99_999.0);
        assert_eq!(state.rate(), 10_000.0);
    }

    #[test]
    fn a_frozen_game_only_runs_the_ticks_it_was_stepped_through() {
        let mut state = TickRate::default();
        assert!(!state.step_if_frozen(2));
        state.set_frozen(true);
        assert!(!state.runs_simulation());
        assert!(state.step_if_frozen(2));
        assert!(state.runs_simulation());
        assert!(state.runs_simulation());
        assert!(!state.runs_simulation());
        assert!(!state.is_stepping());
    }

    #[test]
    fn stopping_a_step_reports_whether_one_was_pending() {
        let mut state = TickRate::default();
        state.set_frozen(true);
        assert!(!state.stop_step());
        state.step_if_frozen(5);
        assert!(state.stop_step());
        assert!(!state.runs_simulation());
    }

    #[test]
    fn a_sprint_thaws_the_game_and_restores_the_frozen_flag_when_it_ends() {
        let mut state = TickRate::default();
        state.set_frozen(true);
        assert!(!state.request_sprint(2));
        assert!(state.is_sprinting() && !state.is_frozen());
        assert_eq!(state.finish_tick(1_000_000), None);
        let report = state.finish_tick(3_000_000).expect("the sprint is done");
        assert_eq!(report.ticks, 2);
        assert_eq!(report.elapsed_nanos, 4_000_000);
        assert!((report.millis_per_tick() - 2.0).abs() < 1e-9);
        assert!(!state.is_sprinting() && state.is_frozen());
    }

    #[test]
    fn a_second_sprint_replaces_the_first_and_keeps_the_original_frozen_flag() {
        let mut state = TickRate::default();
        state.set_frozen(true);
        state.request_sprint(10);
        assert!(state.request_sprint(3));
        state.stop_sprint();
        assert!(state.is_frozen());
        assert_eq!(state.stop_sprint(), None);
    }

    #[test]
    fn the_samples_keep_the_last_hundred_and_sort_for_percentiles() {
        let mut state = TickRate::default();
        for nanos in 0..150u64 {
            state.finish_tick(nanos);
        }
        let sorted = state.sorted_samples();
        assert_eq!(sorted.len(), 100);
        assert_eq!(sorted[0], 50);
        assert_eq!(sorted[99], 149);
        assert_eq!(state.average_nanos(), 99);
    }

    #[test]
    fn lagging_means_the_average_tick_is_longer_than_the_target() {
        let mut state = TickRate::default();
        state.finish_tick(60_000_000);
        assert!(state.is_lagging());
        state.set_frozen(true);
        assert!(!state.is_lagging());
    }
}
