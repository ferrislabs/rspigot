pub const LEVEL_ALL: u8 = 0;
pub const LEVEL_MODERATORS: u8 = 1;
pub const LEVEL_GAMEMASTERS: u8 = 2;
pub const LEVEL_ADMINS: u8 = 3;
pub const LEVEL_OWNERS: u8 = 4;

const LEVELS: &[(&str, u8)] = &[
    ("help", LEVEL_ALL),
    ("?", LEVEL_ALL),
    ("list", LEVEL_ALL),
    ("msg", LEVEL_ALL),
    ("tell", LEVEL_ALL),
    ("w", LEVEL_ALL),
    ("me", LEVEL_ALL),
    ("teammsg", LEVEL_ALL),
    ("tm", LEVEL_ALL),
    ("trigger", LEVEL_ALL),
    ("version", LEVEL_ALL),
    ("ver", LEVEL_ALL),
    ("about", LEVEL_ALL),
    ("plugins", LEVEL_ALL),
    ("pl", LEVEL_ALL),
    ("tps", LEVEL_ALL),
    ("advancement", LEVEL_GAMEMASTERS),
    ("attribute", LEVEL_GAMEMASTERS),
    ("bossbar", LEVEL_GAMEMASTERS),
    ("clear", LEVEL_GAMEMASTERS),
    ("clone", LEVEL_GAMEMASTERS),
    ("damage", LEVEL_GAMEMASTERS),
    ("data", LEVEL_GAMEMASTERS),
    ("datapack", LEVEL_GAMEMASTERS),
    ("defaultgamemode", LEVEL_GAMEMASTERS),
    ("dialog", LEVEL_GAMEMASTERS),
    ("difficulty", LEVEL_GAMEMASTERS),
    ("effect", LEVEL_GAMEMASTERS),
    ("enchant", LEVEL_GAMEMASTERS),
    ("execute", LEVEL_GAMEMASTERS),
    ("experience", LEVEL_GAMEMASTERS),
    ("xp", LEVEL_GAMEMASTERS),
    ("fetchprofile", LEVEL_GAMEMASTERS),
    ("fill", LEVEL_GAMEMASTERS),
    ("fillbiome", LEVEL_GAMEMASTERS),
    ("forceload", LEVEL_GAMEMASTERS),
    ("function", LEVEL_GAMEMASTERS),
    ("gamemode", LEVEL_GAMEMASTERS),
    ("gamerule", LEVEL_GAMEMASTERS),
    ("give", LEVEL_GAMEMASTERS),
    ("item", LEVEL_GAMEMASTERS),
    ("kill", LEVEL_GAMEMASTERS),
    ("locate", LEVEL_GAMEMASTERS),
    ("loot", LEVEL_GAMEMASTERS),
    ("particle", LEVEL_GAMEMASTERS),
    ("place", LEVEL_GAMEMASTERS),
    ("playsound", LEVEL_GAMEMASTERS),
    ("random", LEVEL_GAMEMASTERS),
    ("recipe", LEVEL_GAMEMASTERS),
    ("reload", LEVEL_GAMEMASTERS),
    ("return", LEVEL_GAMEMASTERS),
    ("ride", LEVEL_GAMEMASTERS),
    ("rotate", LEVEL_GAMEMASTERS),
    ("say", LEVEL_GAMEMASTERS),
    ("schedule", LEVEL_GAMEMASTERS),
    ("scoreboard", LEVEL_GAMEMASTERS),
    ("seed", LEVEL_GAMEMASTERS),
    ("setblock", LEVEL_GAMEMASTERS),
    ("setworldspawn", LEVEL_GAMEMASTERS),
    ("spawnpoint", LEVEL_GAMEMASTERS),
    ("spectate", LEVEL_GAMEMASTERS),
    ("spreadplayers", LEVEL_GAMEMASTERS),
    ("stopsound", LEVEL_GAMEMASTERS),
    ("stopwatch", LEVEL_GAMEMASTERS),
    ("summon", LEVEL_GAMEMASTERS),
    ("swing", LEVEL_GAMEMASTERS),
    ("tag", LEVEL_GAMEMASTERS),
    ("team", LEVEL_GAMEMASTERS),
    ("teleport", LEVEL_GAMEMASTERS),
    ("tp", LEVEL_GAMEMASTERS),
    ("tellraw", LEVEL_GAMEMASTERS),
    ("time", LEVEL_GAMEMASTERS),
    ("title", LEVEL_GAMEMASTERS),
    ("waypoint", LEVEL_GAMEMASTERS),
    ("weather", LEVEL_GAMEMASTERS),
    ("worldborder", LEVEL_GAMEMASTERS),
    ("ban", LEVEL_ADMINS),
    ("ban-ip", LEVEL_ADMINS),
    ("banlist", LEVEL_ADMINS),
    ("deop", LEVEL_ADMINS),
    ("op", LEVEL_ADMINS),
    ("pardon", LEVEL_ADMINS),
    ("pardon-ip", LEVEL_ADMINS),
    ("kick", LEVEL_ADMINS),
    ("raid", LEVEL_ADMINS),
    ("setidletimeout", LEVEL_ADMINS),
    ("tick", LEVEL_ADMINS),
    ("transfer", LEVEL_ADMINS),
    ("whitelist", LEVEL_ADMINS),
    ("jfr", LEVEL_OWNERS),
    ("perf", LEVEL_OWNERS),
    ("publish", LEVEL_OWNERS),
    ("save-all", LEVEL_OWNERS),
    ("save-off", LEVEL_OWNERS),
    ("save-on", LEVEL_OWNERS),
    ("stop", LEVEL_OWNERS),
    ("restart", LEVEL_OWNERS),
    ("spigot", LEVEL_OWNERS),
    ("timings", LEVEL_OWNERS),
    ("debug", LEVEL_ADMINS),
];

pub fn required_level(label: &str) -> Option<u8> {
    LEVELS
        .iter()
        .find_map(|(name, level)| (*name == label).then_some(*level))
}

pub fn may_use(label: &str, level: u8) -> bool {
    required_level(label).is_none_or(|required| level >= required)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_the_vanilla_registrations() {
        assert_eq!(required_level("msg"), Some(LEVEL_ALL));
        assert_eq!(required_level("gamemode"), Some(LEVEL_GAMEMASTERS));
        assert_eq!(required_level("op"), Some(LEVEL_ADMINS));
        assert_eq!(required_level("stop"), Some(LEVEL_OWNERS));
        assert_eq!(required_level("frobnicate"), None);
    }

    #[test]
    fn a_level_unlocks_everything_at_or_below_it() {
        assert!(may_use("say", 2));
        assert!(!may_use("say", 1));
        assert!(!may_use("ban", 2));
        assert!(may_use("ban", 3));
        assert!(!may_use("stop", 3));
        assert!(may_use("stop", 4));
        assert!(may_use("anything-registered-by-a-plugin", 0));
    }
}
