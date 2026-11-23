pub(super) fn allows_logging(label: &str, args: &[&str]) -> bool {
    let sub = args.first().copied().unwrap_or_default();
    match label {
        "help" | "?" | "list" | "seed" | "version" | "ver" | "about" | "plugins" | "pl" | "tps"
        | "msg" | "tell" | "w" | "teammsg" | "tm" | "trigger" | "locate" | "locatebiome"
        | "banlist" | "perf" | "debug" | "execute" | "tellraw" | "say" | "me" | "stop"
        | "reload" | "restart" | "spigot" | "timings" | "random" | "dialog" | "stopwatch" => false,
        "time" => sub != "query",
        "data" => sub != "get",
        "experience" | "xp" => sub != "query",
        "worldborder" => sub != "get",
        "bossbar" => !matches!(sub, "get" | "list"),
        "whitelist" => sub != "list",
        "scoreboard" => {
            !(matches!(sub, "players") && matches!(args.get(1).copied(), Some("get" | "list")))
                && !(matches!(sub, "objectives" | "teams") && args.get(1).copied() == Some("list"))
        }
        "gamerule" => args.len() >= 2,
        "difficulty" | "defaultgamemode" => !args.is_empty(),
        "advancement" => sub != "test",
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::allows_logging;

    #[test]
    fn queries_do_not_log_and_changes_do() {
        assert!(!allows_logging("seed", &[]));
        assert!(!allows_logging("time", &["query", "daytime"]));
        assert!(allows_logging("time", &["set", "day"]));
        assert!(!allows_logging("data", &["get", "entity", "@s"]));
        assert!(allows_logging("data", &["merge", "entity", "@s", "{}"]));
        assert!(!allows_logging("scoreboard", &["players", "get", "a", "b"]));
        assert!(allows_logging(
            "scoreboard",
            &["players", "set", "a", "b", "1"]
        ));
        assert!(!allows_logging("gamerule", &["doDaylightCycle"]));
        assert!(allows_logging("gamerule", &["doDaylightCycle", "false"]));
        assert!(allows_logging("give", &["@s", "stone"]));
    }
}
