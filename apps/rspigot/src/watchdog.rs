use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use rspigot_core::heartbeat::{Heartbeat, TickPhase};

pub const POLL: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchdogSettings {
    pub timeout: Duration,
    pub restart: bool,
    pub poll: Duration,
}

impl WatchdogSettings {
    pub fn new(timeout_seconds: i64, restart: bool) -> Option<Self> {
        let seconds = u64::try_from(timeout_seconds).ok().filter(|s| *s > 0)?;
        Some(Self {
            timeout: Duration::from_secs(seconds),
            restart,
            poll: POLL,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HangReport {
    pub silent_for: Duration,
    pub ticks: u64,
    pub phase: TickPhase,
    pub threads: Vec<String>,
}

pub struct Watchdog {
    stopping: Arc<AtomicBool>,
    thread: JoinHandle<Option<HangReport>>,
}

impl Watchdog {
    pub fn spawn(
        settings: WatchdogSettings,
        heartbeat: Heartbeat,
        restart: impl FnOnce() + Send + 'static,
    ) -> std::io::Result<Self> {
        let stopping = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stopping);
        let thread = std::thread::Builder::new()
            .name("Spigot Watchdog Thread".to_owned())
            .spawn(move || watch(settings, &heartbeat, &flag, restart))?;
        Ok(Self { stopping, thread })
    }

    pub fn stop(self) -> JoinHandle<Option<HangReport>> {
        self.stopping.store(true, Ordering::Release);
        self.thread
    }
}

fn watch(
    settings: WatchdogSettings,
    heartbeat: &Heartbeat,
    stopping: &AtomicBool,
    restart: impl FnOnce(),
) -> Option<HangReport> {
    loop {
        if stopping.load(Ordering::Acquire) || heartbeat.stopped() {
            return None;
        }
        if let Some(silent_for) = heartbeat.since_beat()
            && silent_for > settings.timeout
        {
            let report = HangReport {
                silent_for,
                ticks: heartbeat.ticks(),
                phase: heartbeat.phase(),
                threads: thread_dump(),
            };
            log_report(&report);
            if settings.restart && !heartbeat.stopped() {
                restart();
            }
            return Some(report);
        }
        std::thread::sleep(settings.poll);
    }
}

fn log_report(report: &HangReport) {
    let lines = [
        "------------------------------",
        "The server has stopped responding! This is (probably) not a Spigot bug.",
        "If you see a plugin in the Server thread dump below, then please report it to that author",
        "\t *Especially* if it looks like HTTP or MySQL operations are occurring",
        "If you see a world save or edit, then it means you did far more than your server can handle at once",
        "\t If this is the case, consider increasing timeout-time in spigot.yml but note that this will replace the crash with LARGE lag spikes",
        "Be sure to include ALL relevant console errors and Minecraft crash reports",
        "------------------------------",
    ];
    for line in lines {
        tracing::error!("{line}");
    }
    tracing::error!("Rspigot version: {}", env!("CARGO_PKG_VERSION"));
    tracing::error!("------------------------------");
    tracing::error!("Server thread dump:");
    tracing::error!(
        "Server thread: last tick {} ms ago, tick count {}, state: {}",
        report.silent_for.as_millis(),
        report.ticks,
        report.phase.name()
    );
    tracing::error!("------------------------------");
    tracing::error!("Entire Thread Dump:");
    for thread in &report.threads {
        tracing::error!("{thread}");
    }
    tracing::error!("------------------------------");
}

#[cfg(target_os = "linux")]
fn thread_dump() -> Vec<String> {
    let Ok(tasks) = std::fs::read_dir("/proc/self/task") else {
        return Vec::new();
    };
    let mut threads: Vec<String> = tasks
        .flatten()
        .filter_map(|task| {
            let path = task.path();
            let name = std::fs::read_to_string(path.join("comm")).ok()?;
            let stat = std::fs::read_to_string(path.join("stat")).ok()?;
            let state = stat.rsplit(')').next()?.split_whitespace().next()?;
            let wchan = std::fs::read_to_string(path.join("wchan")).unwrap_or_default();
            Some(format!(
                "Thread {} [{}] state {} waiting in {}",
                task.file_name().to_string_lossy(),
                name.trim(),
                state,
                wchan.trim()
            ))
        })
        .collect();
    threads.sort();
    threads
}

#[cfg(not(target_os = "linux"))]
fn thread_dump() -> Vec<String> {
    vec![format!(
        "{} threads: per-thread stacks are not available on this platform",
        std::thread::available_parallelism().map_or(0, usize::from)
    )]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(restart: bool) -> WatchdogSettings {
        WatchdogSettings {
            timeout: Duration::from_millis(60),
            restart,
            poll: Duration::from_millis(10),
        }
    }

    #[test]
    fn a_non_positive_timeout_disables_the_watchdog() {
        assert_eq!(WatchdogSettings::new(0, true), None);
        assert_eq!(WatchdogSettings::new(-5, true), None);
        assert_eq!(
            WatchdogSettings::new(60, false).map(|s| (s.timeout, s.restart)),
            Some((Duration::from_secs(60), false))
        );
    }

    #[test]
    fn a_stalled_tick_dumps_the_state_and_runs_the_restart() {
        let heartbeat = Heartbeat::new();
        heartbeat.beat(42);
        heartbeat.mark(TickPhase::Tick);
        let restarted = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&restarted);
        let watchdog = Watchdog::spawn(settings(true), heartbeat, move || {
            flag.store(true, Ordering::Release);
        })
        .unwrap();
        let report = watchdog.finish_after_report();
        assert!(restarted.load(Ordering::Acquire));
        assert_eq!(report.ticks, 42);
        assert_eq!(report.phase, TickPhase::Tick);
        assert!(report.silent_for > Duration::from_millis(60));
        assert!(!report.threads.is_empty());
    }

    #[test]
    fn without_restart_on_crash_the_hang_is_only_reported() {
        let heartbeat = Heartbeat::new();
        heartbeat.beat(1);
        let restarted = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&restarted);
        let watchdog = Watchdog::spawn(settings(false), heartbeat, move || {
            flag.store(true, Ordering::Release);
        })
        .unwrap();
        let report = watchdog.finish_after_report();
        assert!(!restarted.load(Ordering::Acquire));
        assert_eq!(report.ticks, 1);
    }

    #[test]
    fn a_server_that_keeps_ticking_is_left_alone() {
        let heartbeat = Heartbeat::new();
        let watchdog = Watchdog::spawn(settings(true), heartbeat.clone(), || {
            panic!("restarted a healthy server")
        })
        .unwrap();
        for tick in 0..12 {
            heartbeat.beat(tick);
            std::thread::sleep(Duration::from_millis(15));
        }
        assert_eq!(watchdog.stop().join().unwrap(), None);
    }

    #[test]
    fn a_stopped_server_is_never_restarted() {
        let heartbeat = Heartbeat::new();
        heartbeat.beat(1);
        heartbeat.stop();
        let watchdog = Watchdog::spawn(settings(true), heartbeat, || {
            panic!("restarted a stopped server")
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(watchdog.stop().join().unwrap(), None);
    }

    #[test]
    fn a_server_that_never_ticked_is_not_armed() {
        let heartbeat = Heartbeat::new();
        let watchdog = Watchdog::spawn(settings(true), heartbeat, || {
            panic!("restarted before the first tick")
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(watchdog.stop().join().unwrap(), None);
    }

    impl Watchdog {
        fn finish_after_report(self) -> HangReport {
            self.thread.join().unwrap().expect("a hang report")
        }
    }
}
