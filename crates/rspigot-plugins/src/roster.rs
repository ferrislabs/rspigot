use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use tokio::sync::watch;

pub const CONNECT_LIMIT: usize = 8;
pub const CONNECT_WINDOW: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Dependencies {
    pub depends: Vec<String>,
    pub soft_depends: Vec<String>,
    pub load_before: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginState {
    Expected,
    Waiting(Vec<String>),
    Connected,
    Down,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RosterError {
    #[error("dependency cycle: {0}")]
    Cycle(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Begun {
    pub(crate) ticket: u64,
    pub(crate) already_running: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Blockers {
    pub(crate) hard: Vec<String>,
    pub(crate) soft: Vec<String>,
}

#[derive(Default)]
struct Inner {
    expected: BTreeSet<String>,
    states: BTreeMap<String, PluginState>,
    connections: HashMap<String, u64>,
    attempts: HashMap<String, VecDeque<Instant>>,
    before: HashMap<String, Vec<String>>,
    tickets: HashMap<String, u64>,
    waiting_on: HashMap<String, Vec<String>>,
    next_ticket: u64,
}

struct Shared {
    inner: Mutex<Inner>,
    changes: watch::Sender<u64>,
}

#[derive(Clone)]
pub struct Roster(Arc<Shared>);

impl Roster {
    pub fn new(expected: impl IntoIterator<Item = String>) -> Self {
        let expected: BTreeSet<String> = expected.into_iter().collect();
        let states = expected
            .iter()
            .map(|name| (name.clone(), PluginState::Expected))
            .collect();
        let (changes, _) = watch::channel(0);
        Self(Arc::new(Shared {
            inner: Mutex::new(Inner {
                expected,
                states,
                ..Inner::default()
            }),
            changes,
        }))
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.0
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn notify(&self) {
        self.0.changes.send_modify(|version| *version += 1);
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<u64> {
        self.0.changes.subscribe()
    }

    pub fn expected(&self) -> Vec<String> {
        self.lock().expected.iter().cloned().collect()
    }

    pub fn is_expected(&self, name: &str) -> bool {
        self.lock().expected.contains(name)
    }

    pub fn is_connected(&self, name: &str) -> bool {
        self.lock().connections.contains_key(name)
    }

    pub fn states(&self) -> Vec<(String, PluginState)> {
        self.lock()
            .states
            .iter()
            .map(|(name, state)| (name.clone(), state.clone()))
            .collect()
    }

    pub fn state(&self, name: &str) -> Option<PluginState> {
        self.lock().states.get(name).cloned()
    }

    pub(crate) fn attempt(&self, name: &str, now: Instant) -> Result<(), Duration> {
        let mut inner = self.lock();
        let attempts = inner.attempts.entry(name.to_owned()).or_default();
        while attempts
            .front()
            .is_some_and(|first| now.saturating_duration_since(*first) >= CONNECT_WINDOW)
        {
            attempts.pop_front();
        }
        if attempts.len() >= CONNECT_LIMIT {
            let oldest = attempts.front().copied().unwrap_or(now);
            return Err((oldest + CONNECT_WINDOW).saturating_duration_since(now));
        }
        attempts.push_back(now);
        Ok(())
    }

    pub(crate) fn begin(
        &self,
        name: &str,
        dependencies: &Dependencies,
    ) -> Result<Begun, RosterError> {
        let mut inner = self.lock();
        for dependency in &dependencies.depends {
            if reaches(&inner.waiting_on, dependency, name) {
                return Err(RosterError::Cycle(format!(
                    "{name} -> {dependency} -> {name}"
                )));
            }
        }
        inner.next_ticket += 1;
        let ticket = inner.next_ticket;
        inner.tickets.insert(name.to_owned(), ticket);
        inner
            .before
            .insert(name.to_owned(), dependencies.load_before.clone());
        let already_running = dependencies
            .load_before
            .iter()
            .filter(|other| inner.connections.contains_key(*other))
            .cloned()
            .collect();
        drop(inner);
        self.notify();
        Ok(Begun {
            ticket,
            already_running,
        })
    }

    pub(crate) fn is_current(&self, name: &str, ticket: u64) -> bool {
        self.lock().tickets.get(name) == Some(&ticket)
    }

    pub(crate) fn blockers(&self, name: &str, dependencies: &Dependencies) -> Blockers {
        let mut inner = self.lock();
        let hard: Vec<String> = dependencies
            .depends
            .iter()
            .filter(|dependency| !inner.connections.contains_key(*dependency))
            .cloned()
            .collect();
        let predecessors = inner
            .before
            .iter()
            .filter(|(_, list)| list.iter().any(|other| other == name))
            .map(|(plugin, _)| plugin.clone());
        let mut soft: Vec<String> = dependencies
            .soft_depends
            .iter()
            .cloned()
            .chain(predecessors)
            .filter(|other| {
                inner.expected.contains(other) && !inner.connections.contains_key(other)
            })
            .filter(|other| !hard.contains(other))
            .collect();
        soft.sort();
        soft.dedup();
        if hard.is_empty() {
            inner.waiting_on.remove(name);
        } else {
            inner.waiting_on.insert(name.to_owned(), hard.clone());
        }
        let waiting: Vec<String> = hard.iter().chain(&soft).cloned().collect();
        if !waiting.is_empty() {
            inner
                .states
                .insert(name.to_owned(), PluginState::Waiting(waiting));
        }
        Blockers { hard, soft }
    }

    pub(crate) fn connected(&self, name: &str, connection: u64) {
        let mut inner = self.lock();
        inner.connections.insert(name.to_owned(), connection);
        inner.waiting_on.remove(name);
        inner.states.insert(name.to_owned(), PluginState::Connected);
        drop(inner);
        self.notify();
    }

    pub(crate) fn left(&self, name: &str, connection: u64) {
        let mut inner = self.lock();
        if inner.connections.get(name) == Some(&connection) {
            inner.connections.remove(name);
            inner.states.insert(name.to_owned(), PluginState::Down);
            drop(inner);
            self.notify();
        }
    }

    pub(crate) fn failed(&self, name: &str) {
        let mut inner = self.lock();
        if !inner.connections.contains_key(name) {
            inner.states.insert(name.to_owned(), PluginState::Failed);
        }
        drop(inner);
        self.notify();
    }

    pub(crate) fn withdraw(&self, name: &str, ticket: u64) {
        let mut inner = self.lock();
        if inner.tickets.get(name) != Some(&ticket) {
            return;
        }
        inner.tickets.remove(name);
        inner.waiting_on.remove(name);
        if matches!(inner.states.get(name), Some(PluginState::Waiting(_))) {
            let state = if inner.connections.contains_key(name) {
                PluginState::Connected
            } else {
                PluginState::Down
            };
            inner.states.insert(name.to_owned(), state);
        }
        drop(inner);
        self.notify();
    }
}

fn reaches(waiting_on: &HashMap<String, Vec<String>>, from: &str, target: &str) -> bool {
    let mut seen = BTreeSet::new();
    let mut stack = vec![from];
    while let Some(next) = stack.pop() {
        if next == target {
            return true;
        }
        if !seen.insert(next) {
            continue;
        }
        if let Some(blocked) = waiting_on.get(next) {
            stack.extend(blocked.iter().map(String::as_str));
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    fn needs(depends: &[&str], soft: &[&str], before: &[&str]) -> Dependencies {
        Dependencies {
            depends: names(depends),
            soft_depends: names(soft),
            load_before: names(before),
        }
    }

    #[test]
    fn expected_plugins_start_expected_and_follow_their_connection() {
        let roster = Roster::new(names(&["guard"]));
        assert_eq!(roster.state("guard"), Some(PluginState::Expected));
        roster.connected("guard", 1);
        assert_eq!(roster.state("guard"), Some(PluginState::Connected));
        roster.left("guard", 1);
        assert_eq!(roster.state("guard"), Some(PluginState::Down));
        assert!(!roster.is_connected("guard"));
    }

    #[test]
    fn a_stale_connection_leaving_does_not_mark_its_replacement_down() {
        let roster = Roster::new(names(&["guard"]));
        roster.connected("guard", 1);
        roster.connected("guard", 2);
        roster.left("guard", 1);
        assert_eq!(roster.state("guard"), Some(PluginState::Connected));
        assert!(roster.is_connected("guard"));
    }

    #[test]
    fn a_crash_loop_is_refused_until_the_window_passes() {
        let roster = Roster::new(Vec::new());
        let start = Instant::now();
        for _ in 0..CONNECT_LIMIT {
            assert!(roster.attempt("loop", start).is_ok());
        }
        let wait = roster.attempt("loop", start).unwrap_err();
        assert_eq!(wait, CONNECT_WINDOW);
        assert!(roster.attempt("other", start).is_ok());
        assert!(roster.attempt("loop", start + CONNECT_WINDOW).is_ok());
    }

    #[test]
    fn a_hard_dependency_blocks_until_it_is_connected() {
        let roster = Roster::new(Vec::new());
        let needs_base = needs(&["base"], &[], &[]);
        roster.begin("shop", &needs_base).unwrap();
        let blockers = roster.blockers("shop", &needs_base);
        assert_eq!(blockers.hard, names(&["base"]));
        assert_eq!(
            roster.state("shop"),
            Some(PluginState::Waiting(names(&["base"])))
        );
        roster.connected("base", 1);
        assert!(roster.blockers("shop", &needs_base).hard.is_empty());
    }

    #[test]
    fn a_soft_dependency_only_blocks_when_it_is_expected() {
        let roster = Roster::new(names(&["economy"]));
        let wants = needs(&[], &["economy", "unknown"], &[]);
        roster.begin("shop", &wants).unwrap();
        assert_eq!(roster.blockers("shop", &wants).soft, names(&["economy"]));
        roster.connected("economy", 1);
        assert!(roster.blockers("shop", &wants).soft.is_empty());
    }

    #[test]
    fn a_plugin_that_loads_before_another_holds_the_other_back_while_it_is_expected() {
        let roster = Roster::new(names(&["early", "late"]));
        roster.begin("early", &needs(&[], &[], &["late"])).unwrap();
        let plain = Dependencies::default();
        roster.begin("late", &plain).unwrap();
        assert_eq!(roster.blockers("late", &plain).soft, names(&["early"]));
        roster.connected("early", 1);
        assert!(roster.blockers("late", &plain).soft.is_empty());
    }

    #[test]
    fn loading_before_a_running_plugin_is_reported_as_too_late() {
        let roster = Roster::new(Vec::new());
        roster.connected("late", 1);
        let begun = roster.begin("early", &needs(&[], &[], &["late"])).unwrap();
        assert_eq!(begun.already_running, names(&["late"]));
    }

    #[test]
    fn a_dependency_cycle_between_waiting_plugins_is_refused() {
        let roster = Roster::new(Vec::new());
        let first = needs(&["b"], &[], &[]);
        roster.begin("a", &first).unwrap();
        roster.blockers("a", &first);
        let second = needs(&["a"], &[], &[]);
        let refused = roster.begin("b", &second).unwrap_err();
        assert!(matches!(refused, RosterError::Cycle(_)));
    }

    #[test]
    fn a_newer_session_of_the_same_name_supersedes_the_older_one() {
        let roster = Roster::new(Vec::new());
        let plain = Dependencies::default();
        let old = roster.begin("shop", &plain).unwrap().ticket;
        let new = roster.begin("shop", &plain).unwrap().ticket;
        assert!(!roster.is_current("shop", old));
        assert!(roster.is_current("shop", new));
        roster.withdraw("shop", old);
        assert!(roster.is_current("shop", new));
    }
}
