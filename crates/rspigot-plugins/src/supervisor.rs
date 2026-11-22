use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::time::Duration;

use tokio::process::Command;
use tokio::sync::watch;
use tokio::time::Instant;

use crate::roster::Roster;

pub const FIRST_DELAY: Duration = Duration::from_secs(1);
pub const LAST_DELAY: Duration = Duration::from_secs(60);
pub const STABLE_AFTER: Duration = Duration::from_secs(30);
pub const START_WINDOW: Duration = Duration::from_secs(300);
pub const MAX_STARTS: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<String>,
}

impl Launch {
    pub fn parse(command: &str) -> Option<Self> {
        let mut words = command.split_whitespace().map(str::to_owned);
        let program = words.next()?;
        Some(Self {
            program,
            args: words.collect(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedPlugin {
    pub name: String,
    pub launch: Option<Launch>,
}

pub trait Launcher: Send + Sync + 'static {
    fn run(&self, name: &str, launch: &Launch) -> impl Future<Output = io::Result<()>> + Send;
}

pub struct ProcessLauncher {
    pub url: String,
    pub token: Option<String>,
}

impl Launcher for ProcessLauncher {
    async fn run(&self, name: &str, launch: &Launch) -> io::Result<()> {
        let mut command = Command::new(&launch.program);
        command
            .args(&launch.args)
            .env("RSPIGOT_URL", &self.url)
            .env("RSPIGOT_PLUGIN", name)
            .kill_on_drop(true);
        if let Some(token) = &self.token {
            command.env("RSPIGOT_TOKEN", token);
        }
        let status = command.spawn()?.wait().await?;
        tracing::warn!("Plugin process {name} exited with {status}");
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Next {
    After(Duration),
    GiveUp,
}

#[derive(Default)]
pub(crate) struct Restarts {
    starts: VecDeque<Instant>,
    delay: Option<Duration>,
}

impl Restarts {
    pub(crate) fn started(&mut self, now: Instant) {
        self.starts.push_back(now);
    }

    pub(crate) fn exited(&mut self, now: Instant, lived: Duration) -> Next {
        while self
            .starts
            .front()
            .is_some_and(|first| now.saturating_duration_since(*first) >= START_WINDOW)
        {
            self.starts.pop_front();
        }
        if self.starts.len() >= MAX_STARTS {
            return Next::GiveUp;
        }
        if lived >= STABLE_AFTER {
            self.delay = None;
        }
        let delay = self
            .delay
            .map_or(FIRST_DELAY, |last| (last * 2).min(LAST_DELAY));
        self.delay = Some(delay);
        Next::After(delay)
    }
}

pub(crate) async fn supervise<L: Launcher>(
    plugin: ExpectedPlugin,
    launcher: &L,
    roster: Roster,
    mut stop: watch::Receiver<bool>,
) {
    let Some(launch) = plugin.launch else {
        return;
    };
    let name = plugin.name;
    let mut restarts = Restarts::default();
    let mut changes = roster.subscribe();
    loop {
        while roster.is_connected(&name) {
            tokio::select! {
                changed = changes.changed() => if changed.is_err() { return },
                () = crate::service::stopped(&mut stop) => return,
            }
        }
        let started = Instant::now();
        restarts.started(started);
        tracing::info!("Starting plugin process {name}: {}", launch.program);
        let result = tokio::select! {
            result = launcher.run(&name, &launch) => result,
            () = crate::service::stopped(&mut stop) => return,
        };
        if let Err(error) = result {
            tracing::warn!("Could not run plugin process {name}: {error}");
        }
        let now = Instant::now();
        match restarts.exited(now, now.saturating_duration_since(started)) {
            Next::GiveUp => {
                tracing::error!(
                    "Plugin process {name} crashed {MAX_STARTS} times within {} s, not restarting it",
                    START_WINDOW.as_secs()
                );
                roster.failed(&name);
                return;
            }
            Next::After(delay) => {
                tracing::warn!("Restarting plugin process {name} in {} s", delay.as_secs());
                tokio::select! {
                    () = tokio::time::sleep(delay) => {}
                    () = crate::service::stopped(&mut stop) => return,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn a_command_splits_into_a_program_and_its_arguments() {
        assert_eq!(
            Launch::parse("  java -jar guard.jar "),
            Some(Launch {
                program: "java".to_owned(),
                args: vec!["-jar".to_owned(), "guard.jar".to_owned()],
            })
        );
        assert_eq!(Launch::parse("   "), None);
    }

    #[test]
    fn the_delay_doubles_up_to_a_cap_and_resets_after_a_stable_run() {
        let mut restarts = Restarts::default();
        let start = Instant::now();
        let mut delays = Vec::new();
        for step in 0..4 {
            restarts.started(start + Duration::from_secs(step * 400));
            if let Next::After(delay) =
                restarts.exited(start + Duration::from_secs(step * 400), Duration::ZERO)
            {
                delays.push(delay.as_secs());
            }
        }
        assert_eq!(delays, [1, 2, 4, 8]);
        restarts.started(start + Duration::from_secs(2000));
        assert_eq!(
            restarts.exited(start + Duration::from_secs(2100), STABLE_AFTER),
            Next::After(FIRST_DELAY)
        );
    }

    #[test]
    fn a_crash_loop_inside_the_window_gives_up() {
        let mut restarts = Restarts::default();
        let start = Instant::now();
        for _ in 0..MAX_STARTS - 1 {
            restarts.started(start);
            assert!(matches!(
                restarts.exited(start, Duration::ZERO),
                Next::After(_)
            ));
        }
        restarts.started(start);
        assert_eq!(restarts.exited(start, Duration::ZERO), Next::GiveUp);
    }

    struct Crashing(Arc<AtomicUsize>);

    impl Launcher for Crashing {
        async fn run(&self, _name: &str, _launch: &Launch) -> io::Result<()> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn expected(command: Option<&str>) -> ExpectedPlugin {
        ExpectedPlugin {
            name: "guard".to_owned(),
            launch: command.and_then(Launch::parse),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_crashing_process_is_restarted_then_given_up_and_reported_failed() {
        let launches = Arc::new(AtomicUsize::new(0));
        let roster = Roster::new(vec!["guard".to_owned()]);
        let (_stop, stopped) = watch::channel(false);
        supervise(
            expected(Some("guard-bin --flag")),
            &Crashing(Arc::clone(&launches)),
            roster.clone(),
            stopped,
        )
        .await;
        assert_eq!(launches.load(Ordering::SeqCst), MAX_STARTS);
        assert_eq!(
            roster.state("guard"),
            Some(crate::roster::PluginState::Failed)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_plugin_without_a_command_is_never_launched() {
        let launches = Arc::new(AtomicUsize::new(0));
        let (_stop, stopped) = watch::channel(false);
        supervise(
            expected(None),
            &Crashing(Arc::clone(&launches)),
            Roster::new(Vec::new()),
            stopped,
        )
        .await;
        assert_eq!(launches.load(Ordering::SeqCst), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn a_plugin_already_connected_is_not_launched_until_it_leaves() {
        let launches = Arc::new(AtomicUsize::new(0));
        let roster = Roster::new(vec!["guard".to_owned()]);
        roster.connected("guard", 1);
        let (stop, stopped) = watch::channel(false);
        let task = tokio::spawn({
            let roster = roster.clone();
            let launches = Arc::clone(&launches);
            async move {
                supervise(
                    expected(Some("guard-bin")),
                    &Crashing(launches),
                    roster,
                    stopped,
                )
                .await;
            }
        });
        tokio::time::sleep(Duration::from_secs(10)).await;
        assert_eq!(launches.load(Ordering::SeqCst), 0);
        roster.left("guard", 1);
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(launches.load(Ordering::SeqCst) >= 1);
        stop.send(true).unwrap();
        task.await.unwrap();
    }
}
