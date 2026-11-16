pub(crate) fn short(name: &str) -> &str {
    name.strip_prefix("minecraft:").unwrap_or(name)
}
