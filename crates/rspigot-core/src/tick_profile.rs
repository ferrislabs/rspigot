use std::time::Duration;

use rustc_hash::FxHashMap;

const WINDOW_TICKS: u32 = 100;
const SLOW_TICK: Duration = Duration::from_millis(50);
const LOGGED_PASSES: usize = 8;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PassStat {
    pub total: Duration,
    pub max: Duration,
    pub count: u64,
}

impl PassStat {
    fn add(&mut self, elapsed: Duration) {
        self.total += elapsed;
        self.max = self.max.max(elapsed);
        self.count += 1;
    }

    pub fn mean(&self) -> Duration {
        if self.count == 0 {
            Duration::ZERO
        } else {
            self.total / u32::try_from(self.count).unwrap_or(u32::MAX)
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Stat {
    window: PassStat,
    all: PassStat,
}

#[derive(Debug, Default)]
pub struct TickProfile {
    passes: FxHashMap<&'static str, Stat>,
    current: Vec<(&'static str, Duration)>,
    window_ticks: u32,
    window_total: Duration,
    ticks: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    ticks: u64,
    passes: Vec<(&'static str, PassStat)>,
}

impl Snapshot {
    pub fn since(&self, earlier: &Self) -> Self {
        let before: FxHashMap<&'static str, PassStat> = earlier.passes.iter().copied().collect();
        let mut passes: Vec<(&'static str, PassStat)> = self
            .passes
            .iter()
            .map(|(name, stat)| {
                let old = before.get(name).copied().unwrap_or_default();
                (
                    *name,
                    PassStat {
                        total: stat.total.saturating_sub(old.total),
                        max: stat.max,
                        count: stat.count.saturating_sub(old.count),
                    },
                )
            })
            .filter(|(_, stat)| stat.count > 0)
            .collect();
        passes.sort_by_key(|(_, stat)| std::cmp::Reverse(stat.total));
        Self {
            ticks: self.ticks.saturating_sub(earlier.ticks),
            passes,
        }
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    pub fn passes(&self) -> &[(&'static str, PassStat)] {
        &self.passes
    }

    pub fn lines(&self) -> Vec<String> {
        let ticks = self.ticks.max(1) as f64;
        self.passes
            .iter()
            .map(|(name, stat)| {
                format!(
                    "{name}: mean_us {:.1} max_us {:.1} per_tick_us {:.1}",
                    stat.mean().as_secs_f64() * 1e6,
                    stat.max.as_secs_f64() * 1e6,
                    stat.total.as_secs_f64() * 1e6 / ticks
                )
            })
            .collect()
    }
}

impl TickProfile {
    pub fn record(&mut self, name: &'static str, elapsed: Duration) {
        let stat = self.passes.entry(name).or_default();
        stat.window.add(elapsed);
        stat.all.add(elapsed);
        self.current.push((name, elapsed));
    }

    pub fn end_tick(&mut self, took: Duration) {
        self.ticks += 1;
        self.window_ticks += 1;
        self.window_total += took;
        if took >= SLOW_TICK {
            self.log_slow_tick(took);
        }
        self.current.clear();
        if self.window_ticks >= WINDOW_TICKS {
            self.log_window();
            for stat in self.passes.values_mut() {
                stat.window = PassStat::default();
            }
            self.window_ticks = 0;
            self.window_total = Duration::ZERO;
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut passes: Vec<(&'static str, PassStat)> = self
            .passes
            .iter()
            .map(|(name, stat)| (*name, stat.all))
            .collect();
        passes.sort_by_key(|(_, stat)| std::cmp::Reverse(stat.total));
        Snapshot {
            ticks: self.ticks,
            passes,
        }
    }

    fn log_slow_tick(&mut self, took: Duration) {
        if !tracing::enabled!(tracing::Level::DEBUG) {
            return;
        }
        self.current
            .sort_by_key(|(_, elapsed)| std::cmp::Reverse(*elapsed));
        let heaviest: Vec<String> = self
            .current
            .iter()
            .take(LOGGED_PASSES)
            .map(|(name, elapsed)| format!("{name} {:.2}ms", elapsed.as_secs_f64() * 1e3))
            .collect();
        tracing::debug!(
            tick = self.ticks,
            took_ms = format_args!("{:.2}", took.as_secs_f64() * 1e3),
            heaviest = heaviest.join(", "),
            "slow tick"
        );
    }

    fn log_window(&self) {
        if !tracing::enabled!(tracing::Level::DEBUG) {
            return;
        }
        let mut passes: Vec<(&'static str, PassStat)> = self
            .passes
            .iter()
            .filter(|(_, stat)| stat.window.count > 0)
            .map(|(name, stat)| (*name, stat.window))
            .collect();
        passes.sort_by_key(|(_, stat)| std::cmp::Reverse(stat.total));
        let ticks = f64::from(self.window_ticks.max(1));
        let heaviest: Vec<String> = passes
            .iter()
            .take(LOGGED_PASSES)
            .map(|(name, stat)| {
                format!(
                    "{name} {:.0}us (max {:.0}us)",
                    stat.total.as_secs_f64() * 1e6 / ticks,
                    stat.max.as_secs_f64() * 1e6
                )
            })
            .collect();
        tracing::debug!(
            ticks = self.window_ticks,
            mean_tick_ms = format_args!("{:.3}", self.window_total.as_secs_f64() * 1e3 / ticks),
            heaviest = heaviest.join(", "),
            "tick profile"
        );
        for (name, stat) in &passes {
            tracing::trace!(
                pass = name,
                per_tick_us = format_args!("{:.1}", stat.total.as_secs_f64() * 1e6 / ticks),
                max_us = format_args!("{:.1}", stat.max.as_secs_f64() * 1e6),
                count = stat.count,
                "tick pass"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn a_snapshot_sorts_passes_by_total_time() {
        let mut profile = TickProfile::default();
        profile.record("cheap", ms(1));
        profile.record("costly", ms(5));
        profile.record("cheap", ms(1));
        profile.end_tick(ms(7));
        let snapshot = profile.snapshot();
        assert_eq!(snapshot.ticks(), 1);
        assert_eq!(snapshot.passes()[0].0, "costly");
        assert_eq!(snapshot.passes()[1].1.count, 2);
        assert_eq!(snapshot.passes()[1].1.total, ms(2));
    }

    #[test]
    fn the_difference_between_two_snapshots_covers_only_the_run() {
        let mut profile = TickProfile::default();
        profile.record("entities", ms(3));
        profile.end_tick(ms(3));
        let before = profile.snapshot();
        profile.record("entities", ms(2));
        profile.record("hoppers", ms(1));
        profile.end_tick(ms(3));
        let run = profile.snapshot().since(&before);
        assert_eq!(run.ticks(), 1);
        let entities = run
            .passes()
            .iter()
            .find(|(n, _)| *n == "entities")
            .expect("entities");
        assert_eq!(entities.1.total, ms(2));
        assert_eq!(entities.1.count, 1);
        assert_eq!(run.lines().len(), 2);
    }

    #[test]
    fn the_window_resets_every_hundred_ticks_but_the_totals_stay() {
        let mut profile = TickProfile::default();
        for _ in 0..WINDOW_TICKS {
            profile.record("tick_entities", ms(1));
            profile.end_tick(ms(1));
        }
        let stat = profile.passes["tick_entities"];
        assert_eq!(stat.window.count, 0);
        assert_eq!(stat.all.count, u64::from(WINDOW_TICKS));
        assert_eq!(profile.window_ticks, 0);
    }

    #[test]
    fn a_pass_mean_handles_an_empty_stat() {
        assert_eq!(PassStat::default().mean(), Duration::ZERO);
    }
}
