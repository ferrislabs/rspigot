use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, bail};
use base64::Engine;
use rspigot_proxy_core::ServerRegistry;
use rspigot_proxy_network::ProxySettings;
use serde::Deserialize;

pub const FILE: &str = "proxy.toml";

pub const DEFAULT: &str = r#"bind = "0.0.0.0:25565"
online_mode = true
motd = "An rspigot proxy"
max_players = 100
favicon = "server-icon.png"
forwarding_secret_file = "forwarding.secret"
compression_threshold = 256
compression_level = 6
login_timeout_secs = 30
connect_timeout_secs = 5
shutdown_message = "The proxy is restarting, come back in a moment."
shutdown_timeout_secs = 10
try = ["lobby"]

[servers]
lobby = "127.0.0.1:25566"

[plugins]
bind = "127.0.0.1:25579"
token_file = ""
decide_timeout_ms = 1000

[metrics]
bind = "127.0.0.1:9100"
"#;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProxyFile {
    pub bind: String,
    pub online_mode: bool,
    pub motd: String,
    pub max_players: u32,
    #[serde(default)]
    pub favicon: Option<String>,
    pub forwarding_secret_file: String,
    pub compression_threshold: i64,
    pub compression_level: u32,
    pub login_timeout_secs: u64,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,
    #[serde(default = "default_shutdown_message")]
    pub shutdown_message: String,
    #[serde(default = "default_shutdown_timeout")]
    pub shutdown_timeout_secs: u64,
    #[serde(rename = "try")]
    pub try_order: Vec<String>,
    pub servers: BTreeMap<String, String>,
    #[serde(default)]
    pub plugins: Option<PluginsFile>,
    #[serde(default)]
    pub metrics: Option<MetricsFile>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetricsFile {
    pub bind: String,
}

fn default_shutdown_message() -> String {
    "The proxy is restarting, come back in a moment.".to_owned()
}

const fn default_shutdown_timeout() -> u64 {
    10
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginsFile {
    pub bind: String,
    #[serde(default)]
    pub token_file: String,
    #[serde(default = "default_decide_timeout")]
    pub decide_timeout_ms: u64,
}

impl PluginsFile {
    pub fn token(&self, root: &Path) -> anyhow::Result<Option<String>> {
        if self.token_file.is_empty() {
            return Ok(None);
        }
        let path = root.join(&self.token_file);
        let token = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read the plugin token {}", path.display()))?;
        let token = token.trim();
        if token.is_empty() {
            bail!("the plugin token {} is empty", path.display());
        }
        Ok(Some(token.to_owned()))
    }

    pub fn decide_timeout(&self) -> Duration {
        Duration::from_millis(self.decide_timeout_ms)
    }
}

const fn default_decide_timeout() -> u64 {
    1000
}

const fn default_connect_timeout() -> u64 {
    5
}

impl ProxyFile {
    pub fn parse(text: &str) -> anyhow::Result<Self> {
        let file: Self = toml::from_str(text).context("proxy.toml is invalid")?;
        file.registry()?;
        if file.compression_level > 9 {
            bail!("compression_level must be between 0 and 9");
        }
        Ok(file)
    }

    pub fn settings(&self, root: &Path) -> anyhow::Result<ProxySettings> {
        let secret_path = root.join(&self.forwarding_secret_file);
        let secret = std::fs::read_to_string(&secret_path).with_context(|| {
            format!(
                "cannot read the forwarding secret {}",
                secret_path.display()
            )
        })?;
        let secret = secret.trim();
        if secret.is_empty() {
            bail!("the forwarding secret {} is empty", secret_path.display());
        }
        Ok(ProxySettings {
            online_mode: self.online_mode,
            motd: self.motd.clone(),
            max_players: self.max_players,
            favicon: self.favicon(root)?,
            compression_threshold: usize::try_from(self.compression_threshold).ok(),
            compression_level: self.compression_level,
            forwarding_secret: secret.as_bytes().to_vec(),
            login_timeout: Duration::from_secs(self.login_timeout_secs),
            connect_timeout: Duration::from_secs(self.connect_timeout_secs),
        })
    }

    pub fn registry(&self) -> anyhow::Result<ServerRegistry> {
        Ok(ServerRegistry::new(
            self.servers.clone(),
            self.try_order.clone(),
        )?)
    }

    fn favicon(&self, root: &Path) -> anyhow::Result<Option<String>> {
        let Some(name) = &self.favicon else {
            return Ok(None);
        };
        let path = root.join(name);
        match std::fs::read(&path) {
            Ok(png) => Ok(Some(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            ))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).with_context(|| format!("cannot read {}", path.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_file_is_valid_and_points_at_the_lobby() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("forwarding.secret"), "s3cret\n").unwrap();
        let file = ProxyFile::parse(DEFAULT).unwrap();
        let settings = file.settings(dir.path()).unwrap();
        let candidates = file.registry().unwrap().join_candidates();
        assert_eq!(candidates[0].address, "127.0.0.1:25566");
        assert_eq!(settings.connect_timeout, Duration::from_secs(5));
        assert_eq!(settings.forwarding_secret, b"s3cret");
        assert_eq!(settings.compression_threshold, Some(256));
        assert!(settings.online_mode);
        assert_eq!(settings.favicon, None);
    }

    #[test]
    fn the_default_plugin_host_listens_on_loopback_without_a_token() {
        let dir = tempfile::tempdir().unwrap();
        let file = ProxyFile::parse(DEFAULT).unwrap();
        let plugins = file.plugins.unwrap();
        assert_eq!(plugins.bind, "127.0.0.1:25579");
        assert_eq!(plugins.token(dir.path()).unwrap(), None);
        assert_eq!(plugins.decide_timeout(), Duration::from_secs(1));
        let without = DEFAULT.split("[plugins]").next().unwrap();
        assert!(ProxyFile::parse(without).unwrap().plugins.is_none());
    }

    #[test]
    fn a_plugin_token_file_is_read_and_trimmed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("plugins.token"), "abc\n").unwrap();
        let text = DEFAULT.replace("token_file = \"\"", "token_file = \"plugins.token\"");
        let plugins = ProxyFile::parse(&text).unwrap().plugins.unwrap();
        assert_eq!(plugins.token(dir.path()).unwrap().as_deref(), Some("abc"));
    }

    #[test]
    fn the_default_file_serves_metrics_and_drains_for_ten_seconds() {
        let file = ProxyFile::parse(DEFAULT).unwrap();
        assert_eq!(file.metrics.unwrap().bind, "127.0.0.1:9100");
        assert_eq!(file.shutdown_timeout_secs, 10);
        assert!(file.shutdown_message.contains("restarting"));
    }

    #[test]
    fn a_negative_threshold_turns_compression_off() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("forwarding.secret"), "s").unwrap();
        let text = DEFAULT.replace("compression_threshold = 256", "compression_threshold = -1");
        let settings = ProxyFile::parse(&text)
            .unwrap()
            .settings(dir.path())
            .unwrap();
        assert_eq!(settings.compression_threshold, None);
    }

    #[test]
    fn try_must_name_known_servers() {
        let text = DEFAULT.replace("try = [\"lobby\"]", "try = [\"hub\"]");
        assert!(ProxyFile::parse(&text).is_err());
        let text = DEFAULT.replace("try = [\"lobby\"]", "try = []");
        assert!(ProxyFile::parse(&text).is_err());
    }

    #[test]
    fn a_missing_or_empty_secret_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let file = ProxyFile::parse(DEFAULT).unwrap();
        assert!(file.settings(dir.path()).is_err());
        std::fs::write(dir.path().join("forwarding.secret"), "  \n").unwrap();
        assert!(file.settings(dir.path()).is_err());
    }

    #[test]
    fn a_favicon_file_is_sent_as_a_data_url() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("forwarding.secret"), "s").unwrap();
        std::fs::write(dir.path().join("server-icon.png"), [0x89, b'P', b'N', b'G']).unwrap();
        let settings = ProxyFile::parse(DEFAULT)
            .unwrap()
            .settings(dir.path())
            .unwrap();
        assert_eq!(
            settings.favicon.as_deref(),
            Some("data:image/png;base64,iVBORw==")
        );
    }
}
