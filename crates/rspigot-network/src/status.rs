use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rspigot_core::status::{PlayerSample, StatusReply, StatusSnapshot};
use rspigot_domain::text::Component;
use rspigot_protocol::status::MAX_STATUS_JSON_CHARS;
use serde_json::{Value, json};

use crate::NetworkConfig;
use crate::text::to_json_value;

pub struct Shown<'a> {
    motd: &'a str,
    max_players: i64,
    online: i64,
    sample: &'a [PlayerSample],
    icon: Option<&'a [u8]>,
}

impl<'a> Shown<'a> {
    pub fn board(config: &'a NetworkConfig, snapshot: &'a StatusSnapshot) -> Self {
        Self {
            motd: &config.motd,
            max_players: i64::from(snapshot.max_players),
            online: i64::from(snapshot.online),
            sample: &snapshot.sample,
            icon: None,
        }
    }

    pub fn replied(reply: &'a StatusReply) -> Self {
        Self {
            motd: &reply.motd,
            max_players: i64::from(reply.max_players),
            online: i64::from(reply.online),
            sample: &reply.sample,
            icon: reply.icon.as_deref(),
        }
    }
}

pub fn status_json(config: &NetworkConfig, shown: &Shown) -> String {
    let sample: Vec<Value> = if config.hide_online_players {
        Vec::new()
    } else {
        shown
            .sample
            .iter()
            .map(|p| json!({ "name": p.name, "id": p.id.0.hyphenated().to_string() }))
            .collect()
    };
    let mut status = json!({
        "version": { "name": config.version_name, "protocol": rspigot_data::PROTOCOL_VERSION },
        "players": { "max": shown.max_players, "online": shown.online, "sample": sample },
        "description": to_json_value(&Component::text(shown.motd)),
        "enforcesSecureChat": config.enforce_secure_profile,
    });
    let favicon = match shown.icon {
        Some(png) => Some(format!("data:image/png;base64,{}", STANDARD.encode(png))),
        None => config.favicon.clone(),
    };
    if let (Some(favicon), Some(object)) = (favicon, status.as_object_mut()) {
        object.insert("favicon".to_owned(), Value::String(favicon));
    }
    let rendered = status.to_string();
    if units(&rendered) <= MAX_STATUS_JSON_CHARS {
        return rendered;
    }
    if let Some(object) = status.as_object_mut() {
        object.remove("favicon");
    }
    let mut keep = shown.motd.chars().count();
    loop {
        let rendered = status.to_string();
        let over = units(&rendered).saturating_sub(MAX_STATUS_JSON_CHARS);
        if over == 0 || keep == 0 {
            return rendered;
        }
        keep = keep.saturating_sub(over);
        let cut = shown
            .motd
            .char_indices()
            .nth(keep)
            .map_or(shown.motd, |(at, _)| &shown.motd[..at]);
        status["description"] = to_json_value(&Component::text(cut));
    }
}

fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

pub fn legacy_status(config: &NetworkConfig, shown: &Shown) -> Vec<u8> {
    let motd: String = shown.motd.chars().filter(|c| *c != '\0').collect();
    let text = format!(
        "§1\0{}\0{}\0{}\0{}\0{}",
        rspigot_data::PROTOCOL_VERSION,
        config.version_name,
        motd,
        shown.online,
        shown.max_players
    );
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut out = Vec::with_capacity(3 + units.len() * 2);
    out.push(0xFF);
    out.extend_from_slice(&(units.len() as u16).to_be_bytes());
    for unit in units {
        out.extend_from_slice(&unit.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use rspigot_domain::player::PlayerId;

    use super::*;

    #[test]
    fn status_contains_protocol_and_motd() {
        let config = NetworkConfig::default();
        let snapshot = StatusSnapshot {
            online: 1,
            max_players: 20,
            ..StatusSnapshot::default()
        };
        let json = status_json(&config, &Shown::board(&config, &snapshot));
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["version"]["protocol"], 777);
        assert_eq!(parsed["players"]["online"], 1);
        assert_eq!(parsed["description"]["text"], "A Minecraft Server");
    }

    #[test]
    fn the_board_status_carries_the_configured_favicon_only_when_there_is_one() {
        let snapshot = StatusSnapshot::default();
        let plain = NetworkConfig::default();
        let parsed: Value =
            serde_json::from_str(&status_json(&plain, &Shown::board(&plain, &snapshot))).unwrap();
        assert!(parsed.get("favicon").is_none());
        let iconic = NetworkConfig {
            favicon: Some("data:image/png;base64,AAAA".to_owned()),
            ..NetworkConfig::default()
        };
        let parsed: Value =
            serde_json::from_str(&status_json(&iconic, &Shown::board(&iconic, &snapshot))).unwrap();
        assert_eq!(parsed["favicon"], "data:image/png;base64,AAAA");
    }

    #[test]
    fn legacy_status_is_utf16_with_kick_prefix() {
        let config = NetworkConfig::default();
        let snapshot = StatusSnapshot::default();
        let bytes = legacy_status(&config, &Shown::board(&config, &snapshot));
        assert_eq!(bytes[0], 0xFF);
        assert_eq!(&bytes[3..5], &[0x00, 0xA7]);
    }

    fn reply(icon: Option<Vec<u8>>) -> StatusReply {
        StatusReply {
            motd: "Patched".to_owned(),
            max_players: -1,
            online: 2,
            sample: vec![PlayerSample {
                id: PlayerId::offline("Bob"),
                name: "Bob".to_owned(),
            }],
            icon,
        }
    }

    #[test]
    fn a_settled_ping_shows_its_motd_counts_sample_and_icon() {
        let config = NetworkConfig {
            favicon: Some("data:image/png;base64,c2VydmVy".to_owned()),
            ..NetworkConfig::default()
        };
        let patched = reply(Some(b"icon".to_vec()));
        let parsed: Value =
            serde_json::from_str(&status_json(&config, &Shown::replied(&patched))).unwrap();
        assert_eq!(parsed["description"]["text"], "Patched");
        assert_eq!(parsed["players"]["max"], -1);
        assert_eq!(parsed["players"]["online"], 2);
        assert_eq!(parsed["players"]["sample"][0]["name"], "Bob");
        assert_eq!(parsed["favicon"], "data:image/png;base64,aWNvbg==");
        let kept = reply(None);
        let parsed: Value =
            serde_json::from_str(&status_json(&config, &Shown::replied(&kept))).unwrap();
        assert_eq!(parsed["favicon"], "data:image/png;base64,c2VydmVy");
    }

    #[test]
    fn an_answer_too_long_for_the_protocol_drops_the_icon_then_cuts_the_motd() {
        let config = NetworkConfig::default();
        let heavy = StatusReply {
            icon: Some(vec![7; 30_000]),
            ..reply(None)
        };
        let json = status_json(&config, &Shown::replied(&heavy));
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("favicon").is_none());
        assert_eq!(parsed["description"]["text"], "Patched");
        let wordy = StatusReply {
            motd: "\"é".repeat(20_000),
            icon: Some(vec![7; 3_000]),
            ..reply(None)
        };
        let json = status_json(&config, &Shown::replied(&wordy));
        assert!(units(&json) <= MAX_STATUS_JSON_CHARS);
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("favicon").is_none());
        let motd = parsed["description"]["text"].as_str().unwrap();
        assert!(wordy.motd.starts_with(motd));
        assert!(motd.len() > 10_000);
        let small = reply(Some(vec![7; 3_000]));
        let parsed: Value =
            serde_json::from_str(&status_json(&config, &Shown::replied(&small))).unwrap();
        assert!(parsed.get("favicon").is_some());
    }

    #[test]
    fn a_hidden_sample_stays_hidden_after_a_settled_ping() {
        let config = NetworkConfig {
            hide_online_players: true,
            ..NetworkConfig::default()
        };
        let patched = reply(None);
        let parsed: Value =
            serde_json::from_str(&status_json(&config, &Shown::replied(&patched))).unwrap();
        assert_eq!(parsed["players"]["sample"], json!([]));
        assert_eq!(parsed["players"]["online"], 2);
    }

    #[test]
    fn a_legacy_answer_carries_the_settled_motd_and_counts() {
        let config = NetworkConfig::default();
        let patched = reply(None);
        let bytes = legacy_status(&config, &Shown::replied(&patched));
        let units: Vec<u16> = bytes[3..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect();
        let text = String::from_utf16(&units).unwrap();
        let fields: Vec<&str> = text.split('\0').collect();
        assert_eq!(&fields[3..], ["Patched", "2", "-1"]);
    }
}
