use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

pub const DEFAULTS: [(&str, &str); 70] = [
    ("accepts-transfers", "false"),
    ("allow-flight", "false"),
    ("broadcast-console-to-ops", "true"),
    ("broadcast-rcon-to-ops", "true"),
    ("bug-report-link", ""),
    ("chat-spam-threshold-seconds", "10"),
    ("command-spam-threshold-seconds", "10"),
    ("debug", "false"),
    ("difficulty", "easy"),
    ("enable-code-of-conduct", "false"),
    ("enable-jmx-monitoring", "false"),
    ("enable-query", "false"),
    ("enable-rcon", "false"),
    ("enable-status", "true"),
    ("enforce-secure-profile", "true"),
    ("enforce-whitelist", "false"),
    ("entity-broadcast-range-percentage", "100"),
    ("force-gamemode", "false"),
    ("function-permission-level", "2"),
    ("gamemode", "survival"),
    ("generate-structures", "true"),
    ("generator-settings", "{}"),
    ("hardcore", "false"),
    ("hide-online-players", "false"),
    ("initial-disabled-packs", ""),
    ("initial-enabled-packs", "vanilla"),
    ("level-name", "world"),
    ("level-seed", ""),
    ("level-type", "minecraft\\:normal"),
    ("log-ips", "true"),
    ("management-server-allowed-origins", ""),
    ("management-server-enabled", "false"),
    ("management-server-host", "localhost"),
    ("management-server-port", "0"),
    ("management-server-secret", ""),
    ("management-server-tls-enabled", "true"),
    ("management-server-tls-keystore", ""),
    ("management-server-tls-keystore-password", ""),
    ("max-chained-neighbor-updates", "1000000"),
    ("max-players", "20"),
    ("max-tick-time", "60000"),
    ("max-world-size", "29999984"),
    ("motd", "A Minecraft Server"),
    ("network-compression-threshold", "256"),
    ("online-mode", "true"),
    ("op-permission-level", "4"),
    ("pause-when-empty-seconds", "60"),
    ("player-idle-timeout", "0"),
    ("prevent-proxy-connections", "false"),
    ("query.port", "25565"),
    ("rate-limit", "0"),
    ("rcon.password", ""),
    ("rcon.port", "25575"),
    ("region-file-compression", "deflate"),
    ("require-resource-pack", "false"),
    ("resource-pack", ""),
    ("resource-pack-id", ""),
    ("resource-pack-prompt", ""),
    ("resource-pack-sha1", ""),
    ("server-ip", ""),
    ("server-port", "25565"),
    ("simulation-distance", "10"),
    ("spawn-protection", "16"),
    ("status-heartbeat-interval", "0"),
    ("sync-chunk-writes", "true"),
    ("text-filtering-config", ""),
    ("text-filtering-version", "0"),
    ("use-native-transport", "true"),
    ("view-distance", "10"),
    ("white-list", "true"),
];

#[derive(Debug, Clone, Default)]
pub struct Properties {
    values: BTreeMap<String, String>,
}

fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if let Some(decoded) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(decoded);
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

fn escape(value: &str, key: bool) -> String {
    let mut out = String::with_capacity(value.len());
    for (i, c) in value.chars().enumerate() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            ':' | '=' | '#' | '!' => {
                out.push('\\');
                out.push(c);
            }
            ' ' if key || i == 0 => out.push_str("\\ "),
            c if !c.is_ascii() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

impl Properties {
    pub fn parse(text: &str) -> Self {
        let mut values = BTreeMap::new();
        for line in text.lines() {
            let line = line.trim_start();
            if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
                continue;
            }
            let mut split = None;
            let mut escaped = false;
            for (i, c) in line.char_indices() {
                if escaped {
                    escaped = false;
                    continue;
                }
                match c {
                    '\\' => escaped = true,
                    '=' | ':' => {
                        split = Some((i, 1));
                        break;
                    }
                    _ => {}
                }
            }
            let (key, value) = match split {
                Some((i, len)) => (&line[..i], &line[i + len..]),
                None => (line, ""),
            };
            values.insert(unescape(key.trim_end()), unescape(value.trim_start()));
        }
        Self { values }
    }

    pub fn load_or_create(path: &Path) -> io::Result<Self> {
        let mut properties = match fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(error) => return Err(error),
        };
        for (key, value) in DEFAULTS {
            properties
                .values
                .entry(key.to_owned())
                .or_insert_with(|| unescape(value));
        }
        properties.save(path)?;
        Ok(properties)
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let mut text = String::from("#Minecraft server properties\n");
        for (key, value) in &self.values {
            text.push_str(&escape(key, true));
            text.push('=');
            text.push_str(&escape(value, false));
            text.push('\n');
        }
        fs::write(path, text)
    }

    pub fn string(&self, key: &str) -> &str {
        self.values.get(key).map_or("", String::as_str)
    }

    pub fn bool(&self, key: &str) -> bool {
        self.string(key).eq_ignore_ascii_case("true")
    }

    pub fn int(&self, key: &str, fallback: i64) -> i64 {
        self.string(key).trim().parse().unwrap_or(fallback)
    }

    pub fn set(&mut self, key: &str, value: &str) {
        self.values.insert(key.to_owned(), value.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_escaped_values_and_comments() {
        let p = Properties::parse(
            "#comment\nlevel-type=minecraft\\:flat\nmotd=Hello \\u00e9 World\nempty=\n",
        );
        assert_eq!(p.string("level-type"), "minecraft:flat");
        assert_eq!(p.string("motd"), "Hello é World");
        assert_eq!(p.string("empty"), "");
    }

    #[test]
    fn escape_roundtrip() {
        let mut p = Properties::default();
        p.set("motd", "A: B = C é");
        let mut text = String::new();
        for (k, v) in &p.values {
            text.push_str(&format!("{}={}\n", escape(k, true), escape(v, false)));
        }
        assert_eq!(Properties::parse(&text).string("motd"), "A: B = C é");
    }

    #[test]
    fn defaults_parse_to_typed_values() {
        let p = Properties {
            values: DEFAULTS
                .iter()
                .map(|(k, v)| ((*k).to_owned(), unescape(v)))
                .collect(),
        };
        assert_eq!(p.int("server-port", 0), 25565);
        assert_eq!(p.string("level-type"), "minecraft:normal");
        assert!(p.bool("online-mode"));
    }
}
