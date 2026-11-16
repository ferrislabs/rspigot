use std::fmt::Write;
use std::net::IpAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, TimeZone, Utc};
use rspigot_domain::player::GameProfile;
use rspigot_domain::text::Component;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListEntry {
    pub name: String,
    pub uuid: Option<Uuid>,
    pub level: Option<u8>,
    pub bypasses_player_limit: bool,
}

impl ListEntry {
    pub fn named(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            uuid: None,
            level: None,
            bypasses_player_limit: false,
        }
    }

    pub fn of(profile: &GameProfile) -> Self {
        Self {
            name: profile.name.clone(),
            uuid: Some(profile.id.0),
            level: None,
            bypasses_player_limit: false,
        }
    }

    pub fn matches_profile(&self, profile: &GameProfile) -> bool {
        self.uuid == Some(profile.id.0) || self.name.eq_ignore_ascii_case(&profile.name)
    }

    pub fn matches_name(&self, name: &str) -> bool {
        self.name.eq_ignore_ascii_case(name)
    }

    pub fn operator(mut self, level: u8) -> Self {
        self.level = Some(level.min(MAX_PERMISSION_LEVEL));
        self
    }
}

pub const MAX_PERMISSION_LEVEL: u8 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ban<T> {
    pub target: T,
    pub created: SystemTime,
    pub source: String,
    pub expires: Option<SystemTime>,
    pub reason: Option<String>,
}

pub type BanEntry = Ban<ListEntry>;
pub type IpBanEntry = Ban<IpAddr>;

pub fn ban_reason(reason: Option<&str>) -> Component {
    reason.map_or_else(
        || Component::translatable("multiplayer.disconnect.banned.reason.default", Vec::new()),
        Component::text,
    )
}

impl<T> Ban<T> {
    pub fn reason_message(&self) -> Component {
        ban_reason(self.reason.as_deref())
    }

    pub fn has_expired(&self, now: SystemTime) -> bool {
        self.expires.is_some_and(|expires| expires < now)
    }
}

impl BanEntry {
    fn bans(&self, name: &str, uuid: Option<Uuid>) -> bool {
        match (self.target.uuid, uuid) {
            (Some(banned), Some(uuid)) => banned == uuid,
            _ => self.target.matches_name(name),
        }
    }
}

pub(crate) fn utc(time: SystemTime) -> Option<DateTime<Utc>> {
    let (seconds, nanos) = match time.duration_since(UNIX_EPOCH) {
        Ok(after) => (i64::try_from(after.as_secs()).ok()?, after.subsec_nanos()),
        Err(before) => {
            let before = before.duration();
            let seconds = i64::try_from(before.as_secs()).ok()?;
            match before.subsec_nanos() {
                0 => (-seconds, 0),
                nanos => (-seconds - 1, 1_000_000_000 - nanos),
            }
        }
    };
    DateTime::from_timestamp(seconds, nanos)
}

pub fn format_ban_date(time: SystemTime, pattern: &str) -> Option<String> {
    let mut text = String::new();
    write!(text, "{}", utc(time)?.format(pattern)).ok()?;
    Some(text)
}

pub fn parse_ban_date(text: &str, pattern: &str) -> Option<SystemTime> {
    let date = DateTime::parse_from_str(text, pattern).ok()?;
    system_time(&date)
}

fn system_time<Tz: TimeZone>(date: &DateTime<Tz>) -> Option<SystemTime> {
    let seconds = Duration::from_secs(date.timestamp().unsigned_abs());
    let whole = if date.timestamp() < 0 {
        UNIX_EPOCH.checked_sub(seconds)
    } else {
        UNIX_EPOCH.checked_add(seconds)
    }?;
    whole.checked_add(Duration::from_nanos(u64::from(
        date.timestamp_subsec_nanos(),
    )))
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AdminLists {
    pub operators: Vec<ListEntry>,
    pub whitelist: Vec<ListEntry>,
    pub whitelist_enabled: bool,
    pub bans: Vec<BanEntry>,
    pub ip_bans: Vec<IpBanEntry>,
}

impl AdminLists {
    pub fn is_operator(&self, profile: &GameProfile) -> bool {
        self.operators.iter().any(|e| e.matches_profile(profile))
    }

    pub fn operator_entry(&self, profile: &GameProfile) -> Option<&ListEntry> {
        self.operators.iter().find(|e| e.matches_profile(profile))
    }

    pub fn permission_level(&self, profile: &GameProfile, default: u8) -> u8 {
        self.operator_entry(profile)
            .map_or(0, |entry| entry.level.unwrap_or(default))
            .min(MAX_PERMISSION_LEVEL)
    }

    pub fn bypasses_player_limit(&self, profile: &GameProfile) -> bool {
        self.operator_entry(profile)
            .is_some_and(|entry| entry.bypasses_player_limit)
    }

    pub fn may_join(&self, profile: &GameProfile) -> bool {
        !self.whitelist_enabled
            || self.is_operator(profile)
            || self.whitelist.iter().any(|e| e.matches_profile(profile))
    }

    pub fn profile_ban(&self, profile: &GameProfile, now: SystemTime) -> Option<&BanEntry> {
        self.ban_matching(&profile.name, Some(profile.id.0), now)
    }

    pub fn ban_of(&self, target: &ListEntry, now: SystemTime) -> Option<&BanEntry> {
        self.ban_matching(&target.name, target.uuid, now)
    }

    fn ban_matching(&self, name: &str, uuid: Option<Uuid>, now: SystemTime) -> Option<&BanEntry> {
        self.bans
            .iter()
            .find(|ban| ban.bans(name, uuid) && !ban.has_expired(now))
    }

    pub fn remove_ban(&mut self, target: &ListEntry) -> bool {
        let before = self.bans.len();
        self.bans.retain(|ban| !ban.bans(&target.name, target.uuid));
        self.bans.len() != before
    }

    pub fn ip_ban(&self, address: IpAddr, now: SystemTime) -> Option<&IpBanEntry> {
        let address = address.to_canonical();
        self.ip_bans
            .iter()
            .find(|ban| ban.target == address && !ban.has_expired(now))
    }

    pub fn learn_uuid(&mut self, profile: &GameProfile) -> bool {
        let mut learned = false;
        for ban in &mut self.bans {
            if ban.target.uuid.is_none() && ban.target.matches_name(&profile.name) {
                ban.target.uuid = Some(profile.id.0);
                learned = true;
            }
        }
        learned
    }

    pub fn prune_expired(&mut self, now: SystemTime) -> bool {
        let before = self.bans.len() + self.ip_bans.len();
        self.bans.retain(|ban| !ban.has_expired(now));
        self.ip_bans.retain(|ban| !ban.has_expired(now));
        self.bans.len() + self.ip_bans.len() != before
    }

    pub fn add(list: &mut Vec<ListEntry>, entry: ListEntry) -> bool {
        if let Some(existing) = list
            .iter_mut()
            .find(|e| e.matches_name(&entry.name) || (entry.uuid.is_some() && e.uuid == entry.uuid))
        {
            if existing.uuid.is_none() && entry.uuid.is_some() {
                *existing = entry;
            }
            return false;
        }
        list.push(entry);
        true
    }

    pub fn remove(list: &mut Vec<ListEntry>, name: &str) -> bool {
        let before = list.len();
        list.retain(|e| !e.matches_name(name));
        list.len() != before
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use rspigot_domain::player::PlayerId;

    use super::*;

    fn noon() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_793_534_400)
    }

    fn ban<T>(target: T, expires: Option<SystemTime>) -> Ban<T> {
        Ban {
            target,
            created: noon(),
            source: "Server".to_owned(),
            expires,
            reason: None,
        }
    }

    fn profile(name: &str) -> GameProfile {
        GameProfile {
            id: PlayerId::offline(name),
            name: name.to_owned(),
            properties: Vec::new(),
        }
    }

    #[test]
    fn whitelist_blocks_unlisted_players_but_not_operators() {
        let mut lists = AdminLists {
            whitelist_enabled: true,
            ..AdminLists::default()
        };
        assert!(!lists.may_join(&profile("Alice")));
        AdminLists::add(&mut lists.whitelist, ListEntry::named("alice"));
        assert!(lists.may_join(&profile("Alice")));
        AdminLists::add(&mut lists.operators, ListEntry::of(&profile("Bob")));
        assert!(lists.may_join(&profile("Bob")));
        lists.whitelist_enabled = false;
        assert!(lists.may_join(&profile("Carol")));
    }

    #[test]
    fn add_is_idempotent_and_remove_reports_change() {
        let mut list = Vec::new();
        assert!(AdminLists::add(&mut list, ListEntry::named("Alice")));
        assert!(!AdminLists::add(
            &mut list,
            ListEntry::of(&profile("alice"))
        ));
        assert_eq!(list[0].uuid, Some(PlayerId::offline("alice").0));
        assert!(AdminLists::remove(&mut list, "ALICE"));
        assert!(!AdminLists::remove(&mut list, "alice"));
    }

    #[test]
    fn a_uuid_entry_matches_the_uuid_and_a_name_only_entry_the_name() {
        let renamed = GameProfile {
            name: "Alex".to_owned(),
            ..profile("Steve")
        };
        let mut lists = AdminLists {
            bans: vec![
                ban(ListEntry::of(&profile("Steve")), None),
                ban(ListEntry::named("CAROL"), None),
            ],
            ..AdminLists::default()
        };
        assert_eq!(
            lists.profile_ban(&renamed, noon()).map(|b| &b.target),
            Some(&ListEntry::of(&profile("Steve")))
        );
        assert!(lists.profile_ban(&profile("steve"), noon()).is_none());
        assert_eq!(
            lists
                .profile_ban(&profile("carol"), noon())
                .map(|b| &b.target),
            Some(&ListEntry::named("CAROL"))
        );
        assert!(lists.profile_ban(&profile("Dave"), noon()).is_none());
        assert!(lists.ban_of(&ListEntry::named("steve"), noon()).is_some());
        assert!(
            lists
                .ban_of(&ListEntry::of(&profile("steve")), noon())
                .is_none()
        );
        assert!(!lists.remove_ban(&ListEntry::of(&profile("steve"))));
        assert_eq!(lists.bans.len(), 2);
        assert!(lists.learn_uuid(&profile("carol")));
        assert_eq!(
            lists.bans[1].target.uuid,
            Some(PlayerId::offline("carol").0)
        );
        assert!(!lists.learn_uuid(&profile("carol")));
    }

    #[test]
    fn expired_entries_are_pruned() {
        let hour = Duration::from_secs(3600);
        let address = IpAddr::from([203, 0, 113, 7]);
        let mut lists = AdminLists {
            bans: vec![
                ban(ListEntry::named("Gone"), Some(noon() - hour)),
                ban(ListEntry::named("Kept"), Some(noon() + hour)),
                ban(ListEntry::named("Forever"), None),
            ],
            ip_bans: vec![ban(address, Some(noon() - hour))],
            ..AdminLists::default()
        };
        assert!(lists.profile_ban(&profile("Gone"), noon()).is_none());
        assert!(lists.ip_ban(address, noon()).is_none());
        assert!(lists.ip_ban(address, noon() - hour - hour).is_some());
        let mapped = "::ffff:203.0.113.7".parse().expect("an address");
        assert!(lists.ip_ban(mapped, noon() - hour - hour).is_some());
        assert!(lists.prune_expired(noon()));
        let names: Vec<&str> = lists.bans.iter().map(|b| b.target.name.as_str()).collect();
        assert_eq!(names, ["Kept", "Forever"]);
        assert!(lists.ip_bans.is_empty());
        assert!(!lists.prune_expired(noon()));
        assert_eq!(
            lists.bans[0].reason_message(),
            Component::translatable("multiplayer.disconnect.banned.reason.default", Vec::new())
        );
    }

    #[test]
    fn ban_dates_outside_the_range_are_refused_not_panicked() {
        let pattern = "%Y-%m-%d %H:%M:%S %z";
        let far = UNIX_EPOCH + Duration::from_millis(i64::MAX.unsigned_abs());
        let far_back = UNIX_EPOCH - Duration::from_millis(i64::MAX.unsigned_abs());
        assert_eq!(format_ban_date(far, pattern), None);
        assert_eq!(format_ban_date(far_back, pattern), None);
        assert_eq!(
            format_ban_date(noon(), pattern).as_deref(),
            Some("2026-11-01 12:00:00 +0000")
        );
        let before_1970 = UNIX_EPOCH - Duration::from_millis(1_500);
        assert_eq!(
            format_ban_date(before_1970, pattern).as_deref(),
            Some("1969-12-31 23:59:58 +0000")
        );
        assert_eq!(
            parse_ban_date("2026-11-01 14:00:00 +0200", pattern),
            Some(noon())
        );
        assert_eq!(
            parse_ban_date("1969-12-31 23:59:59 +0000", pattern),
            Some(UNIX_EPOCH - Duration::from_secs(1))
        );
        assert_eq!(parse_ban_date("forever", pattern), None);
    }
}
