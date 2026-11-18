use std::fmt;
use std::time::Duration;

const DEFAULT_POLL_SECONDS: u64 = 2;
const MIN_POLL_SECONDS: u64 = 1;
const MAX_POLL_SECONDS: u64 = 60;

#[derive(Clone)]
pub struct Config {
    pub base_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub username: String,
    pub password: String,
    pub resource: Option<String>,
    pub poll: Duration,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("missing environment variables: {}", .0.join(", "))]
    Missing(Vec<&'static str>),
    #[error(
        "CAPITAINERIE_POLL_SECONDS must be a number from {MIN_POLL_SECONDS} to {MAX_POLL_SECONDS}, got `{0}`"
    )]
    Poll(String),
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self, ConfigError> {
        let get = |key: &str| lookup(key).filter(|value| !value.is_empty());
        let mut missing = Vec::new();
        let mut require = |key: &'static str| {
            let value = get(key);
            if value.is_none() {
                missing.push(key);
            }
            value
        };
        let base_url = require("CAPITAINERIE_URL");
        let token_url = require("CAPITAINERIE_TOKEN_URL");
        let client_id = require("CAPITAINERIE_CLIENT_ID");
        let username = require("CAPITAINERIE_USERNAME");
        let password = require("CAPITAINERIE_PASSWORD");
        let poll = match get("CAPITAINERIE_POLL_SECONDS") {
            None => DEFAULT_POLL_SECONDS,
            Some(raw) => raw
                .parse::<u64>()
                .ok()
                .filter(|seconds| (MIN_POLL_SECONDS..=MAX_POLL_SECONDS).contains(seconds))
                .ok_or(ConfigError::Poll(raw))?,
        };
        match (base_url, token_url, client_id, username, password) {
            (Some(base_url), Some(token_url), Some(client_id), Some(username), Some(password)) => {
                Ok(Self {
                    base_url: base_url.trim_end_matches('/').to_owned(),
                    token_url,
                    client_id,
                    client_secret: get("CAPITAINERIE_CLIENT_SECRET"),
                    username,
                    password,
                    resource: get("CAPITAINERIE_RESOURCE"),
                    poll: Duration::from_secs(poll),
                })
            }
            _ => Err(ConfigError::Missing(missing)),
        }
    }
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("base_url", &self.base_url)
            .field("token_url", &self.token_url)
            .field("client_id", &self.client_id)
            .field("username", &self.username)
            .field("resource", &self.resource)
            .field("poll", &self.poll)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    const FULL: [(&str, &str); 5] = [
        ("CAPITAINERIE_URL", "http://localhost:4100/"),
        ("CAPITAINERIE_TOKEN_URL", "http://ferriskey/token"),
        ("CAPITAINERIE_CLIENT_ID", "capitainerie-minecraft"),
        ("CAPITAINERIE_USERNAME", "bridge"),
        ("CAPITAINERIE_PASSWORD", "s3cret"),
    ];

    #[test]
    fn a_full_environment_gives_a_config_with_defaults() {
        let config = Config::from_lookup(lookup(&FULL)).unwrap();
        assert_eq!(config.base_url, "http://localhost:4100");
        assert_eq!(config.poll, Duration::from_secs(2));
        assert_eq!(config.client_secret, None);
        assert_eq!(config.resource, None);
    }

    #[test]
    fn every_missing_variable_is_named_at_once() {
        let only_url = [("CAPITAINERIE_URL", "http://x")];
        assert_eq!(
            Config::from_lookup(lookup(&only_url)).unwrap_err(),
            ConfigError::Missing(vec![
                "CAPITAINERIE_TOKEN_URL",
                "CAPITAINERIE_CLIENT_ID",
                "CAPITAINERIE_USERNAME",
                "CAPITAINERIE_PASSWORD"
            ])
        );
        let blank = [("CAPITAINERIE_URL", "")];
        assert!(matches!(
            Config::from_lookup(lookup(&blank)),
            Err(ConfigError::Missing(missing)) if missing.len() == 5
        ));
    }

    #[test]
    fn the_poll_interval_is_bounded() {
        for (raw, ok) in [
            ("1", true),
            ("60", true),
            ("0", false),
            ("61", false),
            ("x", false),
        ] {
            let mut pairs = FULL.to_vec();
            pairs.push(("CAPITAINERIE_POLL_SECONDS", raw));
            assert_eq!(Config::from_lookup(lookup(&pairs)).is_ok(), ok, "{raw}");
        }
    }

    #[test]
    fn debug_never_prints_the_password_or_secret() {
        let mut pairs = FULL.to_vec();
        pairs.push(("CAPITAINERIE_CLIENT_SECRET", "topsecret"));
        let shown = format!("{:?}", Config::from_lookup(lookup(&pairs)).unwrap());
        assert!(!shown.contains("s3cret"));
        assert!(!shown.contains("topsecret"));
    }
}
