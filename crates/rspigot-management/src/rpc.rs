use std::time::SystemTime;

use chrono::{DateTime, SecondsFormat, Utc};
use rspigot_core::management::{
    BanRequest, BanView, IpBanRequest, IpBanView, ListAction, ManagementNotice, ManagementReply,
    ManagementRequest, OperatorRequest, OperatorView, PlayerRef, Setting, SettingValue, StatusView,
};
use serde_json::{Map, Value, json};
use uuid::Uuid;

pub const PARSE_ERROR: i64 = -32700;
pub const INVALID_REQUEST: i64 = -32600;
pub const METHOD_NOT_FOUND: i64 = -32601;
pub const INVALID_PARAMS: i64 = -32602;
pub const INTERNAL_ERROR: i64 = -32603;

const NAMESPACE: &str = "minecraft:";

#[derive(Debug, Clone, PartialEq)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<String>,
}

impl RpcError {
    fn new(code: i64, message: &str) -> Self {
        Self {
            code,
            message: message.to_owned(),
            data: None,
        }
    }

    fn params(detail: impl Into<String>) -> Self {
        Self {
            code: INVALID_PARAMS,
            message: "Invalid params".to_owned(),
            data: Some(detail.into()),
        }
    }
}

pub fn error_response(id: &Value, error: &RpcError) -> Value {
    let mut body = Map::new();
    body.insert("code".into(), json!(error.code));
    body.insert("message".into(), json!(error.message));
    if let Some(data) = &error.data {
        body.insert("data".into(), json!(data));
    }
    json!({"jsonrpc": "2.0", "id": id, "error": body})
}

pub fn result_response(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub id: Option<Value>,
    pub method: String,
    pub params: Value,
}

pub fn parse_call(text: &str) -> Result<Call, Value> {
    let value: Value = serde_json::from_str(text)
        .map_err(|_| error_response(&Value::Null, &RpcError::new(PARSE_ERROR, "Parse error")))?;
    let Some(object) = value.as_object() else {
        return Err(error_response(
            &Value::Null,
            &RpcError::new(INVALID_REQUEST, "Invalid Request"),
        ));
    };
    let id = object.get("id").cloned();
    let shown = id.clone().unwrap_or(Value::Null);
    if let Some(id) = &id
        && !(id.is_string() || id.is_number() || id.is_null())
    {
        return Err(error_response(
            &Value::Null,
            &RpcError::new(INVALID_REQUEST, "Invalid Request"),
        ));
    }
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return Err(error_response(
            &shown,
            &RpcError::new(INVALID_REQUEST, "Invalid Request"),
        ));
    };
    Ok(Call {
        id,
        method: method.to_owned(),
        params: object.get("params").cloned().unwrap_or(Value::Null),
    })
}

fn param<'a>(params: &'a Value, name: &str) -> Result<&'a Value, RpcError> {
    match params {
        Value::Array(items) => match items.as_slice() {
            [one] => Ok(one),
            [] => Err(RpcError::params(format!("Missing parameter: {name}"))),
            _ => Err(RpcError::params("Expected exactly one parameter")),
        },
        Value::Object(map) => map
            .get(name)
            .or_else(|| (map.len() == 1).then(|| map.values().next()).flatten())
            .ok_or_else(|| RpcError::params(format!("Missing parameter: {name}"))),
        _ => Err(RpcError::params(format!("Missing parameter: {name}"))),
    }
}

fn list_of<'a>(value: &'a Value, what: &str) -> Result<&'a Vec<Value>, RpcError> {
    value
        .as_array()
        .ok_or_else(|| RpcError::params(format!("{what} must be an array")))
}

pub fn parse_player(value: &Value) -> Result<PlayerRef, RpcError> {
    let object = value
        .as_object()
        .ok_or_else(|| RpcError::params("A player must be an object"))?;
    let id = match object.get("id").filter(|v| !v.is_null()) {
        Some(raw) => Some(
            raw.as_str()
                .and_then(|s| Uuid::parse_str(s).ok())
                .ok_or_else(|| RpcError::params("Invalid player id"))?,
        ),
        None => None,
    };
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_owned);
    if id.is_none() && name.is_none() {
        return Err(RpcError::params("A player needs an id or a name"));
    }
    Ok(PlayerRef { id, name })
}

fn players(value: &Value, what: &str) -> Result<Vec<PlayerRef>, RpcError> {
    list_of(value, what)?.iter().map(parse_player).collect()
}

fn optional_text(object: &Map<String, Value>, key: &str) -> Option<String> {
    object.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn parse_instant(object: &Map<String, Value>) -> Result<Option<SystemTime>, RpcError> {
    match object.get("expires").and_then(Value::as_str) {
        None => Ok(None),
        Some(text) => DateTime::parse_from_rfc3339(text)
            .map(|d| Some(d.with_timezone(&Utc).into()))
            .map_err(|_| RpcError::params("Invalid expiry date")),
    }
}

fn object_of<'a>(value: &'a Value, what: &str) -> Result<&'a Map<String, Value>, RpcError> {
    value
        .as_object()
        .ok_or_else(|| RpcError::params(format!("{what} must be an object")))
}

fn parse_ban(value: &Value) -> Result<BanRequest, RpcError> {
    let object = object_of(value, "A ban")?;
    Ok(BanRequest {
        player: parse_player(
            object
                .get("player")
                .ok_or_else(|| RpcError::params("Missing player"))?,
        )?,
        reason: optional_text(object, "reason"),
        source: optional_text(object, "source"),
        expires: parse_instant(object)?,
    })
}

fn parse_ip_ban(value: &Value) -> Result<IpBanRequest, RpcError> {
    let object = object_of(value, "An ip ban")?;
    Ok(IpBanRequest {
        player: object
            .get("player")
            .filter(|v| !v.is_null())
            .map(parse_player)
            .transpose()?,
        ip: optional_text(object, "ip"),
        reason: optional_text(object, "reason"),
        source: optional_text(object, "source"),
        expires: parse_instant(object)?,
    })
}

fn parse_operator(value: &Value) -> Result<OperatorRequest, RpcError> {
    let object = object_of(value, "An operator")?;
    let level = match object.get("permissionLevel").filter(|v| !v.is_null()) {
        Some(raw) => Some(
            raw.as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .filter(|n| *n <= 4)
                .ok_or_else(|| RpcError::params("permissionLevel must be 0 to 4"))?,
        ),
        None => None,
    };
    Ok(OperatorRequest {
        player: parse_player(
            object
                .get("player")
                .ok_or_else(|| RpcError::params("Missing player"))?,
        )?,
        level,
        bypasses_player_limit: object.get("bypassesPlayerLimit").and_then(Value::as_bool),
    })
}

pub fn message_text(value: &Value) -> Result<String, RpcError> {
    let object = object_of(value, "A message")?;
    object
        .get("literal")
        .or_else(|| object.get("translatable"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| RpcError::params("A message needs a literal or a translatable"))
}

fn parse_kick(value: &Value) -> Result<(PlayerRef, Option<String>), RpcError> {
    let object = object_of(value, "A kick")?;
    let player = parse_player(
        object
            .get("player")
            .ok_or_else(|| RpcError::params("Missing player"))?,
    )?;
    let message = object
        .get("message")
        .filter(|v| !v.is_null())
        .map(message_text)
        .transpose()?;
    Ok((player, message))
}

fn rule_value_text(value: &Value) -> Result<String, RpcError> {
    match value {
        Value::Bool(b) => Ok(b.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        _ => Err(RpcError::params(
            "A game rule value is a boolean or an integer",
        )),
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Bool,
    Int,
    Text,
}

const SETTINGS: [(&str, Setting, Kind); 8] = [
    ("autosave", Setting::Autosave, Kind::Bool),
    ("difficulty", Setting::Difficulty, Kind::Text),
    ("enforce_allowlist", Setting::EnforceAllowlist, Kind::Bool),
    ("max_players", Setting::MaxPlayers, Kind::Int),
    ("game_mode", Setting::GameMode, Kind::Text),
    ("force_game_mode", Setting::ForceGameMode, Kind::Bool),
    ("allow_flight", Setting::AllowFlight, Kind::Bool),
    (
        "operator_user_permission_level",
        Setting::OperatorPermissionLevel,
        Kind::Int,
    ),
];

fn setting_value(kind: Kind, value: &Value) -> Result<SettingValue, RpcError> {
    match kind {
        Kind::Bool => value.as_bool().map(SettingValue::Bool),
        Kind::Int => value.as_i64().map(SettingValue::Int),
        Kind::Text => value.as_str().map(|s| SettingValue::Text(s.to_owned())),
    }
    .ok_or_else(|| RpcError::params("The value has the wrong type"))
}

fn setting_json(value: &SettingValue) -> Value {
    match value {
        SettingValue::Bool(b) => json!(b),
        SettingValue::Int(n) => json!(n),
        SettingValue::Text(t) => json!(t),
    }
}

pub fn method_names() -> Vec<String> {
    let mut names: Vec<String> = [
        "allowlist",
        "allowlist/add",
        "allowlist/remove",
        "allowlist/set",
        "allowlist/clear",
        "bans",
        "bans/add",
        "bans/remove",
        "bans/set",
        "bans/clear",
        "ip_bans",
        "ip_bans/add",
        "ip_bans/remove",
        "ip_bans/set",
        "ip_bans/clear",
        "operators",
        "operators/add",
        "operators/remove",
        "operators/set",
        "operators/clear",
        "players",
        "players/kick",
        "gamerules",
        "gamerules/update",
        "server/status",
        "server/save",
        "server/stop",
        "server/system_message",
    ]
    .iter()
    .map(|n| format!("{NAMESPACE}{n}"))
    .collect();
    for (name, _, _) in SETTINGS {
        names.push(format!("{NAMESPACE}serversettings/{name}"));
        names.push(format!("{NAMESPACE}serversettings/{name}/set"));
    }
    names.push("rpc.discover".to_owned());
    names
}

pub fn to_request(method: &str, params: &Value) -> Result<ManagementRequest, RpcError> {
    let Some(name) = method.strip_prefix(NAMESPACE) else {
        return Err(RpcError::new(METHOD_NOT_FOUND, "Method not found"));
    };
    Ok(match name {
        "allowlist" => ManagementRequest::Allowlist(ListAction::Get),
        "allowlist/add" => {
            ManagementRequest::Allowlist(ListAction::Add(players(param(params, "add")?, "add")?))
        }
        "allowlist/remove" => ManagementRequest::Allowlist(ListAction::Remove(players(
            param(params, "remove")?,
            "remove",
        )?)),
        "allowlist/set" => ManagementRequest::Allowlist(ListAction::Set(players(
            param(params, "players")?,
            "players",
        )?)),
        "allowlist/clear" => ManagementRequest::Allowlist(ListAction::Clear),
        "bans" => ManagementRequest::Bans(ListAction::Get),
        "bans/add" => ManagementRequest::Bans(ListAction::Add(
            list_of(param(params, "add")?, "add")?
                .iter()
                .map(parse_ban)
                .collect::<Result<_, _>>()?,
        )),
        "bans/remove" => ManagementRequest::Bans(ListAction::Remove(players(
            param(params, "remove")?,
            "remove",
        )?)),
        "bans/set" => ManagementRequest::Bans(ListAction::Set(
            list_of(param(params, "bans")?, "bans")?
                .iter()
                .map(parse_ban)
                .collect::<Result<_, _>>()?,
        )),
        "bans/clear" => ManagementRequest::Bans(ListAction::Clear),
        "ip_bans" => ManagementRequest::IpBans(ListAction::Get),
        "ip_bans/add" => ManagementRequest::IpBans(ListAction::Add(
            list_of(param(params, "add")?, "add")?
                .iter()
                .map(parse_ip_ban)
                .collect::<Result<_, _>>()?,
        )),
        "ip_bans/remove" => ManagementRequest::IpBans(ListAction::Remove(
            list_of(param(params, "ip")?, "ip")?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| RpcError::params("An ip is a string"))
                })
                .collect::<Result<_, _>>()?,
        )),
        "ip_bans/set" => ManagementRequest::IpBans(ListAction::Set(
            list_of(param(params, "banlist")?, "banlist")?
                .iter()
                .map(parse_ip_ban)
                .collect::<Result<_, _>>()?,
        )),
        "ip_bans/clear" => ManagementRequest::IpBans(ListAction::Clear),
        "operators" => ManagementRequest::Operators(ListAction::Get),
        "operators/add" => ManagementRequest::Operators(ListAction::Add(
            list_of(param(params, "add")?, "add")?
                .iter()
                .map(parse_operator)
                .collect::<Result<_, _>>()?,
        )),
        "operators/remove" => ManagementRequest::Operators(ListAction::Remove(players(
            param(params, "remove")?,
            "remove",
        )?)),
        "operators/set" => ManagementRequest::Operators(ListAction::Set(
            list_of(param(params, "operators")?, "operators")?
                .iter()
                .map(parse_operator)
                .collect::<Result<_, _>>()?,
        )),
        "operators/clear" => ManagementRequest::Operators(ListAction::Clear),
        "players" => ManagementRequest::Players,
        "players/kick" => ManagementRequest::Kick(
            list_of(param(params, "kick")?, "kick")?
                .iter()
                .map(parse_kick)
                .collect::<Result<_, _>>()?,
        ),
        "gamerules" => ManagementRequest::GameRules,
        "gamerules/update" => {
            let object = object_of(param(params, "gamerule")?, "A game rule update")?;
            let key = optional_text(object, "key")
                .ok_or_else(|| RpcError::params("A game rule update needs a key"))?;
            let value = rule_value_text(
                object
                    .get("value")
                    .ok_or_else(|| RpcError::params("A game rule update needs a value"))?,
            )?;
            ManagementRequest::GameRule { key, value }
        }
        "server/status" => ManagementRequest::Status,
        "server/save" => ManagementRequest::Save {
            flush: match param(params, "flush") {
                Ok(value) => value
                    .as_bool()
                    .ok_or_else(|| RpcError::params("flush must be a boolean"))?,
                Err(_) => false,
            },
        },
        "server/stop" => ManagementRequest::Stop,
        "server/system_message" => {
            let object = object_of(param(params, "message")?, "A system message")?;
            let message = message_text(
                object
                    .get("message")
                    .ok_or_else(|| RpcError::params("A system message needs a message"))?,
            )?;
            let overlay = object
                .get("overlay")
                .and_then(Value::as_bool)
                .ok_or_else(|| RpcError::params("A system message needs overlay"))?;
            let receivers = object
                .get("receivingPlayers")
                .filter(|v| !v.is_null())
                .map(|v| players(v, "receivingPlayers"))
                .transpose()?;
            ManagementRequest::SystemMessage {
                message,
                overlay,
                receivers,
            }
        }
        other => {
            let rest = other
                .strip_prefix("serversettings/")
                .ok_or_else(|| RpcError::new(METHOD_NOT_FOUND, "Method not found"))?;
            let (key, setting) = match rest.strip_suffix("/set") {
                Some(key) => (key, true),
                None => (rest, false),
            };
            let (_, which, kind) = SETTINGS
                .iter()
                .find(|(name, _, _)| *name == key)
                .ok_or_else(|| RpcError::new(METHOD_NOT_FOUND, "Method not found"))?;
            let value = if setting {
                let raw = match params {
                    Value::Array(items) if items.len() == 1 => &items[0],
                    Value::Object(map) if map.len() == 1 => {
                        map.values().next().unwrap_or(&Value::Null)
                    }
                    _ => return Err(RpcError::params("Expected exactly one parameter")),
                };
                Some(setting_value(*kind, raw)?)
            } else {
                None
            };
            ManagementRequest::Setting {
                setting: *which,
                value,
            }
        }
    })
}

fn player_json(player: &PlayerRef) -> Value {
    let mut object = Map::new();
    if let Some(id) = player.id {
        object.insert("id".into(), json!(id.to_string()));
    }
    if let Some(name) = &player.name {
        object.insert("name".into(), json!(name));
    }
    Value::Object(object)
}

fn instant_json(time: SystemTime) -> String {
    DateTime::<Utc>::from(time).to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn ban_json(ban: &BanView) -> Value {
    let mut object = Map::new();
    object.insert("player".into(), player_json(&ban.player));
    if let Some(reason) = &ban.reason {
        object.insert("reason".into(), json!(reason));
    }
    object.insert("source".into(), json!(ban.source));
    if let Some(expires) = ban.expires {
        object.insert("expires".into(), json!(instant_json(expires)));
    }
    Value::Object(object)
}

fn ip_ban_json(ban: &IpBanView) -> Value {
    let mut object = Map::new();
    object.insert("ip".into(), json!(ban.ip));
    if let Some(reason) = &ban.reason {
        object.insert("reason".into(), json!(reason));
    }
    object.insert("source".into(), json!(ban.source));
    if let Some(expires) = ban.expires {
        object.insert("expires".into(), json!(instant_json(expires)));
    }
    Value::Object(object)
}

fn operator_json(op: &OperatorView) -> Value {
    json!({
        "player": player_json(&op.player),
        "permissionLevel": op.level,
        "bypassesPlayerLimit": op.bypasses_player_limit,
    })
}

fn status_json(status: &StatusView) -> Value {
    json!({
        "started": status.started,
        "players": status.players.iter().map(player_json).collect::<Vec<_>>(),
        "version": {"name": status.version_name, "protocol": status.protocol},
    })
}

fn rule_json(key: &str, value: &str) -> Value {
    let (value, kind) = match value {
        "true" => (json!(true), "boolean"),
        "false" => (json!(false), "boolean"),
        other => (
            other
                .parse::<i64>()
                .map_or_else(|_| json!(other), |n| json!(n)),
            "integer",
        ),
    };
    json!({"key": format!("{NAMESPACE}{key}"), "value": value, "type": kind})
}

pub fn reply_json(reply: &ManagementReply) -> Result<Value, RpcError> {
    Ok(match reply {
        ManagementReply::Allowlist(list) | ManagementReply::Players(list) => {
            Value::Array(list.iter().map(player_json).collect())
        }
        ManagementReply::Bans(list) => Value::Array(list.iter().map(ban_json).collect()),
        ManagementReply::IpBans(list) => Value::Array(list.iter().map(ip_ban_json).collect()),
        ManagementReply::Operators(list) => Value::Array(list.iter().map(operator_json).collect()),
        ManagementReply::GameRules(rules) => {
            Value::Array(rules.iter().map(|(k, v)| rule_json(k, v)).collect())
        }
        ManagementReply::GameRule(key, value) => rule_json(key, value),
        ManagementReply::Setting(value) => setting_json(value),
        ManagementReply::Status(status) => status_json(status),
        ManagementReply::Done(done) => json!(done),
        ManagementReply::Fault(rspigot_core::management::ManagementFault::InvalidParams(text)) => {
            return Err(RpcError::params(text.clone()));
        }
        ManagementReply::Fault(rspigot_core::management::ManagementFault::Failed(text)) => {
            return Err(RpcError {
                code: INTERNAL_ERROR,
                message: "Internal error".to_owned(),
                data: Some(text.clone()),
            });
        }
    })
}

pub fn notification(notice: &ManagementNotice) -> Value {
    let (name, params): (&str, Option<Value>) = match notice {
        ManagementNotice::Started => ("server/started", None),
        ManagementNotice::Stopping => ("server/stopping", None),
        ManagementNotice::Saving => ("server/saving", None),
        ManagementNotice::Saved => ("server/saved", None),
        ManagementNotice::PlayerJoined(p) => ("players/joined", Some(player_json(p))),
        ManagementNotice::PlayerLeft(p) => ("players/left", Some(player_json(p))),
        ManagementNotice::OperatorAdded(o) => ("operators/added", Some(operator_json(o))),
        ManagementNotice::OperatorRemoved(o) => ("operators/removed", Some(operator_json(o))),
        ManagementNotice::AllowlistAdded(p) => ("allowlist/added", Some(player_json(p))),
        ManagementNotice::AllowlistRemoved(p) => ("allowlist/removed", Some(player_json(p))),
        ManagementNotice::BanAdded(b) => ("bans/added", Some(ban_json(b))),
        ManagementNotice::BanRemoved(p) => ("bans/removed", Some(player_json(p))),
        ManagementNotice::IpBanAdded(b) => ("ip_bans/added", Some(ip_ban_json(b))),
        ManagementNotice::IpBanRemoved(ip) => ("ip_bans/removed", Some(json!(ip))),
        ManagementNotice::GameRuleUpdated { key, value } => {
            ("gamerules/updated", Some(rule_json(key, value)))
        }
    };
    let mut object = Map::new();
    object.insert("jsonrpc".into(), json!("2.0"));
    object.insert(
        "method".into(),
        json!(format!("{NAMESPACE}notification/{name}")),
    );
    if let Some(params) = params {
        object.insert("params".into(), json!([params]));
    }
    Value::Object(object)
}

pub fn discovery() -> Value {
    json!({
        "openrpc": "1.3.2",
        "info": {"title": "Minecraft Server JSON-RPC", "version": "3.1.0"},
        "methods": method_names()
            .into_iter()
            .map(|name| json!({"name": name}))
            .collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_parse_and_bad_json_is_a_parse_error() {
        let call = parse_call(r#"{"jsonrpc":"2.0","id":1,"method":"minecraft:players"}"#).unwrap();
        assert_eq!(call.method, "minecraft:players");
        assert_eq!(call.id, Some(json!(1)));
        let error = parse_call("{").unwrap_err();
        assert_eq!(error["error"]["code"], PARSE_ERROR);
        assert_eq!(error["id"], Value::Null);
        let error = parse_call(r#"{"id":[1],"method":"x"}"#).unwrap_err();
        assert_eq!(error["error"]["code"], INVALID_REQUEST);
    }

    #[test]
    fn positional_and_named_parameters_are_both_accepted() {
        let player = json!({"name": "Bob", "id": "11111111-2222-3333-4444-555555555555"});
        let positional = to_request("minecraft:allowlist/add", &json!([[player.clone()]])).unwrap();
        let named = to_request("minecraft:allowlist/add", &json!({"add": [player]})).unwrap();
        assert_eq!(positional, named);
        let ManagementRequest::Allowlist(ListAction::Add(list)) = positional else {
            panic!("allowlist");
        };
        assert_eq!(list[0].name.as_deref(), Some("Bob"));
        assert!(list[0].id.is_some());
    }

    #[test]
    fn bad_params_and_unknown_methods_use_the_standard_codes() {
        assert_eq!(
            to_request("minecraft:allowlist/add", &json!([]))
                .unwrap_err()
                .code,
            INVALID_PARAMS
        );
        assert_eq!(
            to_request("minecraft:allowlist/add", &json!([[{}]]))
                .unwrap_err()
                .code,
            INVALID_PARAMS
        );
        assert_eq!(
            to_request("minecraft:nothing", &Value::Null)
                .unwrap_err()
                .code,
            METHOD_NOT_FOUND
        );
        assert_eq!(
            to_request("allowlist", &Value::Null).unwrap_err().code,
            METHOD_NOT_FOUND
        );
    }

    #[test]
    fn bans_carry_reason_source_and_expiry() {
        let params = json!([[{
            "player": {"name": "Eve"},
            "reason": "grief",
            "expires": "2030-01-02T03:04:05Z"
        }]]);
        let ManagementRequest::Bans(ListAction::Add(list)) =
            to_request("minecraft:bans/add", &params).unwrap()
        else {
            panic!("bans");
        };
        assert_eq!(list[0].reason.as_deref(), Some("grief"));
        assert!(list[0].expires.is_some());
        let view = BanView {
            player: PlayerRef::named("Eve"),
            reason: None,
            source: "Server".to_owned(),
            expires: list[0].expires,
        };
        assert_eq!(ban_json(&view)["expires"], "2030-01-02T03:04:05Z");
    }

    #[test]
    fn settings_check_their_value_type() {
        let ok = to_request("minecraft:serversettings/max_players/set", &json!([12])).unwrap();
        assert_eq!(
            ok,
            ManagementRequest::Setting {
                setting: Setting::MaxPlayers,
                value: Some(SettingValue::Int(12))
            }
        );
        assert!(to_request("minecraft:serversettings/max_players/set", &json!(["x"])).is_err());
        assert_eq!(
            to_request("minecraft:serversettings/autosave", &Value::Null).unwrap(),
            ManagementRequest::Setting {
                setting: Setting::Autosave,
                value: None
            }
        );
    }

    #[test]
    fn game_rules_are_typed_in_the_reply() {
        let reply = ManagementReply::GameRules(vec![
            ("keep_inventory".to_owned(), "true".to_owned()),
            ("random_tick_speed".to_owned(), "3".to_owned()),
        ]);
        assert_eq!(
            reply_json(&reply).unwrap(),
            json!([
                {"key": "minecraft:keep_inventory", "value": true, "type": "boolean"},
                {"key": "minecraft:random_tick_speed", "value": 3, "type": "integer"}
            ])
        );
    }

    #[test]
    fn notifications_use_the_notification_namespace() {
        let joined = notification(&ManagementNotice::PlayerJoined(PlayerRef::named("Bob")));
        assert_eq!(joined["method"], "minecraft:notification/players/joined");
        assert_eq!(joined["params"], json!([{"name": "Bob"}]));
        assert!(joined.get("id").is_none());
        let started = notification(&ManagementNotice::Started);
        assert!(started.get("params").is_none());
    }

    #[test]
    fn faults_map_to_errors() {
        let fault = ManagementReply::Fault(
            rspigot_core::management::ManagementFault::InvalidParams("no".to_owned()),
        );
        assert_eq!(reply_json(&fault).unwrap_err().code, INVALID_PARAMS);
    }

    #[test]
    fn every_listed_method_resolves_or_is_discovery() {
        for name in method_names() {
            if name == "rpc.discover" {
                continue;
            }
            let outcome = to_request(&name, &Value::Null);
            if let Err(error) = outcome {
                assert_ne!(error.code, METHOD_NOT_FOUND, "{name}");
            }
        }
    }
}
