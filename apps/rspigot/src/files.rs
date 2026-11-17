use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crossbeam_channel::Receiver;
use rspigot_core::admin::{
    AdminLists, Ban, BanEntry, IpBanEntry, ListEntry, MAX_PERMISSION_LEVEL, format_ban_date,
    parse_ban_date,
};
use rspigot_core::events::Inbound;
use rspigot_core::reload::{ReloadData, ReloadRequest};
use rspigot_network::NetLink;
use uuid::Uuid;

use crate::config::Properties;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Map, Value};

const DATE_FORMAT: &str = "%Y-%m-%d %H:%M:%S %z";
const FOREVER: &str = "forever";
const UNKNOWN_SOURCE: &str = "(Unknown)";

fn read_entries(path: &Path) -> Vec<Value> {
    let Ok(text) = std::fs::read_to_string(path) else {
        let _ = std::fs::write(path, "[]");
        return Vec::new();
    };
    let Ok(Value::Array(entries)) = serde_json::from_str::<Value>(&text) else {
        tracing::warn!("{} is not a JSON array, ignoring it", path.display());
        return Vec::new();
    };
    entries
}

fn list_entry(entry: &Value) -> Option<ListEntry> {
    let name = entry.get("name").and_then(Value::as_str)?;
    let uuid = entry
        .get("uuid")
        .and_then(Value::as_str)
        .and_then(|u| Uuid::parse_str(u).ok());
    let level = entry
        .get("level")
        .and_then(Value::as_u64)
        .map(|level| level.min(u64::from(MAX_PERMISSION_LEVEL)) as u8);
    let bypasses_player_limit = entry
        .get("bypassesPlayerLimit")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Some(ListEntry {
        name: name.to_owned(),
        uuid,
        level,
        bypasses_player_limit,
    })
}

fn read_list(path: &Path) -> Vec<ListEntry> {
    read_entries(path).iter().filter_map(list_entry).collect()
}

fn parse_date(text: &str) -> Option<SystemTime> {
    parse_ban_date(text, DATE_FORMAT)
}

fn format_date(time: SystemTime) -> Option<String> {
    format_ban_date(time, DATE_FORMAT)
}

fn read_ban<T>(entry: &Value, target: T, now: SystemTime) -> Ban<T> {
    let text = |key: &str| entry.get(key).and_then(Value::as_str);
    Ban {
        target,
        created: text("created").and_then(parse_date).unwrap_or(now),
        source: text("source").unwrap_or(UNKNOWN_SOURCE).to_owned(),
        expires: text("expires").and_then(parse_date),
        reason: text("reason").map(str::to_owned),
    }
}

fn read_bans(path: &Path, now: SystemTime) -> Vec<BanEntry> {
    read_entries(path)
        .iter()
        .filter_map(|entry| list_entry(entry).map(|target| read_ban(entry, target, now)))
        .collect()
}

fn read_ip_bans(path: &Path, now: SystemTime) -> Vec<IpBanEntry> {
    read_entries(path)
        .iter()
        .filter_map(|entry| {
            let address = entry.get("ip")?.as_str()?.parse::<IpAddr>().ok()?;
            Some(read_ban(entry, address.to_canonical(), now))
        })
        .collect()
}

pub fn read_admin_lists(root: &Path, whitelist_enabled: bool, now: SystemTime) -> AdminLists {
    let mut lists = AdminLists {
        operators: read_list(&root.join("ops.json")),
        whitelist: read_list(&root.join("whitelist.json")),
        whitelist_enabled,
        bans: read_bans(&root.join("banned-players.json"), now),
        ip_bans: read_ip_bans(&root.join("banned-ips.json"), now),
    };
    lists.prune_expired(now);
    lists
}

fn write_json(path: &Path, values: Vec<Value>) -> std::io::Result<()> {
    let text =
        serde_json::to_string_pretty(&Value::Array(values)).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

fn list_object(entry: &ListEntry) -> Map<String, Value> {
    let mut object = Map::new();
    if let Some(uuid) = entry.uuid {
        object.insert(
            "uuid".to_owned(),
            Value::String(uuid.hyphenated().to_string()),
        );
    }
    object.insert("name".to_owned(), Value::String(entry.name.clone()));
    object
}

fn write_list(path: &Path, entries: &[ListEntry], operator: bool) -> std::io::Result<()> {
    let values: Vec<Value> = entries
        .iter()
        .map(|e| {
            let mut object = list_object(e);
            if operator {
                object.insert(
                    "level".to_owned(),
                    Value::from(e.level.unwrap_or(MAX_PERMISSION_LEVEL)),
                );
                object.insert(
                    "bypassesPlayerLimit".to_owned(),
                    Value::Bool(e.bypasses_player_limit),
                );
            }
            Value::Object(object)
        })
        .collect();
    write_json(path, values)
}

fn ban_object<T>(mut object: Map<String, Value>, ban: &Ban<T>) -> Value {
    if let Some(created) = format_date(ban.created) {
        object.insert("created".to_owned(), Value::String(created));
    }
    object.insert("source".to_owned(), Value::String(ban.source.clone()));
    let expires = ban
        .expires
        .and_then(format_date)
        .unwrap_or_else(|| FOREVER.to_owned());
    object.insert("expires".to_owned(), Value::String(expires));
    if let Some(reason) = &ban.reason {
        object.insert("reason".to_owned(), Value::String(reason.clone()));
    }
    Value::Object(object)
}

fn write_bans(path: &Path, bans: &[BanEntry]) -> std::io::Result<()> {
    write_json(
        path,
        bans.iter()
            .map(|ban| ban_object(list_object(&ban.target), ban))
            .collect(),
    )
}

fn write_ip_bans(path: &Path, bans: &[IpBanEntry]) -> std::io::Result<()> {
    write_json(
        path,
        bans.iter()
            .map(|ban| {
                let mut object = Map::new();
                object.insert("ip".to_owned(), Value::String(ban.target.to_string()));
                ban_object(object, ban)
            })
            .collect(),
    )
}

fn write_admin_lists(root: &Path, lists: &AdminLists) -> std::io::Result<()> {
    write_list(&root.join("ops.json"), &lists.operators, true)?;
    write_list(&root.join("whitelist.json"), &lists.whitelist, false)?;
    write_bans(&root.join("banned-players.json"), &lists.bans)?;
    write_ip_bans(&root.join("banned-ips.json"), &lists.ip_bans)
}

pub fn spawn_admin_writer(root: PathBuf, updates: Receiver<AdminLists>) {
    let spawned = std::thread::Builder::new()
        .name("admin-lists".to_owned())
        .spawn(move || {
            let mut whitelist_enabled = None;
            for lists in updates {
                if let Err(error) = write_admin_lists(&root, &lists) {
                    tracing::error!(
                        %error,
                        "could not save ops.json, whitelist.json, banned-players.json or banned-ips.json"
                    );
                }
                if whitelist_enabled != Some(lists.whitelist_enabled) {
                    whitelist_enabled = Some(lists.whitelist_enabled);
                    let path = root.join("server.properties");
                    if let Ok(mut properties) = Properties::load_or_create(&path) {
                        properties.set("white-list", &lists.whitelist_enabled.to_string());
                        if let Err(error) = properties.save(&path) {
                            tracing::error!(%error, "could not save server.properties");
                        }
                    }
                }
            }
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "admin list writer failed to start");
    }
}

pub fn read_favicon(root: &Path) -> Option<String> {
    let bytes = std::fs::read(root.join("server-icon.png")).ok()?;
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        tracing::warn!("server-icon.png is not a PNG image");
        return None;
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    if width != 64 || height != 64 {
        tracing::warn!("server-icon.png must be 64x64 pixels, got {width}x{height}");
        return None;
    }
    Some(format!("data:image/png;base64,{}", STANDARD.encode(&bytes)))
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use rspigot_core::admin::{AdminLists, Ban, ListEntry};
    use serde_json::{Value, json};
    use uuid::Uuid;

    use super::*;

    const STEVE: &str = "00000000-0000-0000-0000-000000000001";

    fn noon() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(1_793_534_400)
    }

    fn directory(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("rspigot-bans-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a temporary directory");
        root
    }

    fn parsed(path: &Path) -> Value {
        let text = std::fs::read_to_string(path).expect("the file was written");
        serde_json::from_str(&text).expect("the file is JSON")
    }

    #[test]
    fn ban_files_are_written_in_the_vanilla_format() {
        let root = directory("written");
        let lists = AdminLists {
            bans: vec![Ban {
                target: ListEntry {
                    name: "Steve".into(),
                    uuid: Some(Uuid::parse_str(STEVE).expect("a uuid")),
                    level: None,
                    bypasses_player_limit: false,
                },
                created: noon(),
                source: "Server".into(),
                expires: None,
                reason: Some("griefing".into()),
            }],
            ip_bans: vec![Ban {
                target: IpAddr::from([203, 0, 113, 7]),
                created: noon(),
                source: "Server".into(),
                expires: None,
                reason: None,
            }],
            ..AdminLists::default()
        };
        write_admin_lists(&root, &lists).expect("the lists are written");
        assert_eq!(
            parsed(&root.join("banned-players.json")),
            json!([{
                "uuid": STEVE,
                "name": "Steve",
                "created": "2026-11-01 12:00:00 +0000",
                "source": "Server",
                "expires": "forever",
                "reason": "griefing",
            }])
        );
        assert_eq!(
            parsed(&root.join("banned-ips.json")),
            json!([{
                "ip": "203.0.113.7",
                "created": "2026-11-01 12:00:00 +0000",
                "source": "Server",
                "expires": "forever",
            }])
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ban_files_are_read_back_and_expired_entries_pruned() {
        let root = directory("read");
        let players = json!([
            {
                "uuid": STEVE,
                "name": "Old",
                "created": "2026-10-01 12:00:00 +0000",
                "source": "Server",
                "expires": "2026-11-01 11:00:00 +0000",
                "reason": "spam",
            },
            {
                "name": "Steve",
                "created": "2026-11-01 14:00:00 +0200",
                "source": "Alice",
                "expires": "forever",
                "reason": "griefing",
            },
        ]);
        let ips = json!([
            {
                "ip": "203.0.113.7",
                "created": "yesterday",
                "expires": "2026-11-01 15:00:00 +0200",
            },
        ]);
        std::fs::write(root.join("banned-players.json"), players.to_string()).expect("written");
        std::fs::write(root.join("banned-ips.json"), ips.to_string()).expect("written");
        let lists = read_admin_lists(&root, false, noon());
        assert_eq!(
            lists.bans,
            [Ban {
                target: ListEntry::named("Steve"),
                created: noon(),
                source: "Alice".into(),
                expires: None,
                reason: Some("griefing".into()),
            }]
        );
        assert_eq!(
            lists.ip_bans,
            [Ban {
                target: IpAddr::from([203, 0, 113, 7]),
                created: noon(),
                source: "(Unknown)".into(),
                expires: Some(noon() + Duration::from_secs(3600)),
                reason: None,
            }]
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_ban_without_a_reason_is_written_without_one_and_reads_back_as_none() {
        let root = directory("reasonless");
        let ban = Ban {
            target: ListEntry::named("Steve"),
            created: noon(),
            source: "Server".into(),
            expires: Some(noon() + Duration::from_secs(3600)),
            reason: None,
        };
        let lists = AdminLists {
            bans: vec![ban.clone()],
            ..AdminLists::default()
        };
        write_admin_lists(&root, &lists).expect("the lists are written");
        assert_eq!(
            parsed(&root.join("banned-players.json")),
            json!([{
                "name": "Steve",
                "created": "2026-11-01 12:00:00 +0000",
                "source": "Server",
                "expires": "2026-11-01 13:00:00 +0000",
            }])
        );
        assert_eq!(read_admin_lists(&root, false, noon()).bans, [ban]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn operator_levels_and_the_player_limit_flag_survive_a_round_trip() {
        let root = directory("ops");
        std::fs::write(
            root.join("ops.json"),
            r#"[{"name":"Mod","level":2,"bypassesPlayerLimit":true},{"name":"Owner"}]"#,
        )
        .expect("the file is written");
        let lists = read_admin_lists(&root, false, noon());
        assert_eq!(lists.operators[0].level, Some(2));
        assert!(lists.operators[0].bypasses_player_limit);
        assert_eq!(lists.operators[1].level, None);
        write_admin_lists(&root, &lists).expect("the lists are written");
        assert_eq!(
            parsed(&root.join("ops.json")),
            json!([
                {"name": "Mod", "level": 2, "bypassesPlayerLimit": true},
                {"name": "Owner", "level": 4, "bypassesPlayerLimit": false}
            ])
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dates_outside_the_range_never_panic_the_writer() {
        let root = directory("far");
        let far = UNIX_EPOCH + Duration::from_millis(i64::MAX.unsigned_abs());
        let lists = AdminLists {
            bans: vec![Ban {
                target: ListEntry::named("Far"),
                created: far,
                source: "Server".into(),
                expires: Some(far),
                reason: None,
            }],
            ip_bans: vec![Ban {
                target: IpAddr::from([203, 0, 113, 7]),
                created: UNIX_EPOCH - Duration::from_millis(i64::MAX.unsigned_abs()),
                source: "Server".into(),
                expires: None,
                reason: None,
            }],
            ..AdminLists::default()
        };
        write_admin_lists(&root, &lists).expect("the lists are written");
        assert_eq!(
            parsed(&root.join("banned-players.json")),
            json!([{"name": "Far", "source": "Server", "expires": "forever"}])
        );
        assert_eq!(
            parsed(&root.join("banned-ips.json")),
            json!([{"ip": "203.0.113.7", "source": "Server", "expires": "forever"}])
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}

pub fn reload(root: &Path, request: ReloadRequest) -> anyhow::Result<ReloadData> {
    let files = crate::config::ServerFiles::load(root)?;
    let admin = read_admin_lists(root, files.properties.bool("white-list"), SystemTime::now());
    let config = (!request.whitelist_only).then(|| files.game_config(admin.clone()));
    let permissions = (!request.whitelist_only)
        .then(|| crate::config::permissions::load_or_create(&root.join("permissions.yml")))
        .transpose()?;
    Ok(ReloadData {
        admin,
        config,
        permissions,
    })
}

pub fn spawn_reloader(
    root: PathBuf,
    requests: Receiver<ReloadRequest>,
    inbound: crossbeam_channel::Sender<Inbound<NetLink>>,
) {
    let spawned = std::thread::Builder::new()
        .name("reload".to_owned())
        .spawn(move || {
            for request in requests {
                match reload(&root, request) {
                    Ok(data) => {
                        if inbound.send(Inbound::Reload(Box::new(data))).is_err() {
                            break;
                        }
                    }
                    Err(error) => tracing::error!(%error, "reload failed, keeping the old data"),
                }
            }
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "reload thread failed to start");
    }
}

#[cfg(test)]
mod reload_tests {
    use super::*;

    #[test]
    fn a_reload_rereads_the_properties_and_the_lists() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("server.properties"),
            "max-players=7\nwhite-list=true\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("whitelist.json"),
            r#"[{"uuid":"11111111-2222-3333-4444-555555555555","name":"Zed"}]"#,
        )
        .unwrap();
        let data = reload(
            dir.path(),
            ReloadRequest {
                whitelist_only: false,
            },
        )
        .unwrap();
        assert_eq!(data.config.expect("config").max_players, 7);
        assert_eq!(data.admin.whitelist.len(), 1);
        assert!(data.admin.whitelist_enabled);
        let data = reload(
            dir.path(),
            ReloadRequest {
                whitelist_only: true,
            },
        )
        .unwrap();
        assert!(data.config.is_none());
    }
}
