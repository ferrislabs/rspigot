use rspigot_nbt::Tag;
use rspigot_protocol::types::{JsonText, Text};
use serde_json::json;

use crate::ProxySettings;

pub fn response_json(settings: &ProxySettings, online: u32) -> String {
    let mut response = json!({
        "version": {
            "name": rspigot_data::VERSION_NAME,
            "protocol": rspigot_data::PROTOCOL_VERSION,
        },
        "players": {
            "max": settings.max_players,
            "online": online,
            "sample": [],
        },
        "description": { "text": settings.motd },
        "enforcesSecureChat": false,
    });
    if let Some(favicon) = &settings.favicon {
        response["favicon"] = json!(favicon);
    }
    response.to_string()
}

pub fn text(message: &str) -> JsonText {
    JsonText(json!({ "text": message }).to_string())
}

pub fn nbt_text(message: &str) -> Text {
    Text(Tag::String(message.to_owned()))
}

pub fn translatable(key: &str) -> JsonText {
    JsonText(json!({ "translate": key }).to_string())
}

pub fn incompatible() -> JsonText {
    JsonText(
        json!({
            "translate": "multiplayer.disconnect.incompatible",
            "with": [rspigot_data::VERSION_NAME],
        })
        .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::settings;

    #[test]
    fn the_status_shows_the_motd_version_and_player_count() {
        let value: serde_json::Value =
            serde_json::from_str(&response_json(&settings(), 3)).unwrap();
        assert_eq!(value["version"]["protocol"], rspigot_data::PROTOCOL_VERSION);
        assert_eq!(value["version"]["name"], rspigot_data::VERSION_NAME);
        assert_eq!(value["players"]["online"], 3);
        assert_eq!(value["players"]["max"], 100);
        assert_eq!(value["description"]["text"], "A proxy");
        assert!(value.get("favicon").is_none());
    }
}
