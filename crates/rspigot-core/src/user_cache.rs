use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use crossbeam_channel::Sender;
use rspigot_domain::player::GameProfile;
use uuid::Uuid;

use crate::admin::ListEntry;
use crate::ports::ClientLink;
use crate::server::Server;

pub const DEFAULT_SIZE: usize = 1000;
pub const EXPIRY: Duration = Duration::from_secs(30 * 24 * 3600);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedProfile {
    pub name: String,
    pub uuid: Uuid,
    pub expires: SystemTime,
}

#[derive(Debug, Clone)]
pub struct UserCache {
    entries: Arc<Mutex<Vec<CachedProfile>>>,
    limit: usize,
    sink: Option<Sender<Vec<CachedProfile>>>,
}

impl Default for UserCache {
    fn default() -> Self {
        Self::new(Vec::new(), DEFAULT_SIZE)
    }
}

impl PartialEq for UserCache {
    fn eq(&self, other: &Self) -> bool {
        self.limit == other.limit && self.snapshot() == other.snapshot()
    }
}

impl UserCache {
    pub fn new(entries: Vec<CachedProfile>, limit: usize) -> Self {
        Self {
            entries: Arc::new(Mutex::new(entries)),
            limit,
            sink: None,
        }
    }

    pub fn with_sink(mut self, sink: Sender<Vec<CachedProfile>>) -> Self {
        self.sink = Some(sink);
        self
    }

    pub fn snapshot(&self) -> Vec<CachedProfile> {
        self.lock().clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<CachedProfile>> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn by_name(&self, name: &str, now: SystemTime) -> Option<CachedProfile> {
        self.lock()
            .iter()
            .find(|entry| entry.expires > now && entry.name.eq_ignore_ascii_case(name))
            .cloned()
    }

    pub fn by_uuid(&self, uuid: Uuid, now: SystemTime) -> Option<CachedProfile> {
        self.lock()
            .iter()
            .find(|entry| entry.expires > now && entry.uuid == uuid)
            .cloned()
    }

    pub fn remember(&self, profile: &GameProfile, now: SystemTime) {
        if self.limit == 0 {
            return;
        }
        let snapshot = {
            let mut entries = self.lock();
            entries.retain(|entry| entry.uuid != profile.id.0);
            entries.insert(
                0,
                CachedProfile {
                    name: profile.name.clone(),
                    uuid: profile.id.0,
                    expires: now + EXPIRY,
                },
            );
            entries.truncate(self.limit);
            entries.clone()
        };
        if let Some(sink) = &self.sink {
            let _ = sink.send(snapshot);
        }
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn remember_login(&self, profile: &GameProfile) {
        self.config.user_cache.remember(profile, self.now());
    }

    pub(crate) fn offline_entry(&self, name: &str) -> ListEntry {
        match self.config.user_cache.by_name(name, self.now()) {
            Some(cached) => {
                let mut entry = ListEntry::named(&cached.name);
                entry.uuid = Some(cached.uuid);
                entry
            }
            None => ListEntry::named(name),
        }
    }

    pub(crate) fn offline_entry_of(&self, uuid: Uuid) -> Option<ListEntry> {
        let cached = self.config.user_cache.by_uuid(uuid, self.now())?;
        let mut entry = ListEntry::named(&cached.name);
        entry.uuid = Some(cached.uuid);
        Some(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rspigot_domain::player::PlayerId;

    fn profile(name: &str) -> GameProfile {
        GameProfile {
            id: PlayerId::offline(name),
            name: name.to_owned(),
            properties: Vec::new(),
        }
    }

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn a_remembered_login_is_found_by_name_ignoring_case_and_by_uuid() {
        let cache = UserCache::default();
        cache.remember(&profile("Alice"), at(100));
        let found = cache.by_name("aLiCe", at(200)).unwrap();
        assert_eq!(found.name, "Alice");
        assert_eq!(cache.by_uuid(found.uuid, at(200)).unwrap().name, "Alice");
        assert_eq!(found.expires, at(100) + EXPIRY);
    }

    #[test]
    fn expired_entries_are_not_returned() {
        let cache = UserCache::default();
        cache.remember(&profile("Alice"), at(100));
        assert!(cache.by_name("Alice", at(100) + EXPIRY).is_none());
    }

    #[test]
    fn a_second_login_moves_the_entry_first_and_the_limit_drops_the_oldest() {
        let cache = UserCache::new(Vec::new(), 2);
        cache.remember(&profile("A"), at(1));
        cache.remember(&profile("B"), at(2));
        cache.remember(&profile("A"), at(3));
        cache.remember(&profile("C"), at(4));
        let names: Vec<String> = cache.snapshot().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["C", "A"]);
    }

    #[test]
    fn every_change_is_published_to_the_sink() {
        let (sink, published) = crossbeam_channel::unbounded();
        let cache = UserCache::default().with_sink(sink);
        cache.remember(&profile("Alice"), at(1));
        assert_eq!(published.try_recv().unwrap().len(), 1);
    }
}
