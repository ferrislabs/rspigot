use std::sync::OnceLock;

use serde_json::Value;

const SOURCE: &str = include_str!("../../data/structures.json");

pub(crate) fn structure_data() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| serde_json::from_str(SOURCE).unwrap_or(Value::Null))
}

pub(crate) fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}
