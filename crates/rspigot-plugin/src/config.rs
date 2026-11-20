use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const FILE: &str = "config.toml";
const FOLDER: &str = "plugins";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("could not read or write {path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("{path} is not valid TOML: {message}")]
    Parse { path: PathBuf, message: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    String(String),
    Integer(i64),
    Float(f64),
    Bool(bool),
    List(Vec<ConfigValue>),
    Table(Vec<(String, ConfigValue)>),
}

impl ConfigValue {
    fn of(value: &toml::Value) -> Self {
        match value {
            toml::Value::String(text) => Self::String(text.clone()),
            toml::Value::Integer(number) => Self::Integer(*number),
            toml::Value::Float(number) => Self::Float(*number),
            toml::Value::Boolean(flag) => Self::Bool(*flag),
            toml::Value::Datetime(moment) => Self::String(moment.to_string()),
            toml::Value::Array(items) => Self::List(items.iter().map(Self::of).collect()),
            toml::Value::Table(table) => Self::Table(
                table
                    .iter()
                    .map(|(key, value)| (key.clone(), Self::of(value)))
                    .collect(),
            ),
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(text) => Some(text),
            _ => None,
        }
    }

    pub const fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(number) => Some(*number),
            _ => None,
        }
    }

    pub const fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(flag) => Some(*flag),
            _ => None,
        }
    }

    pub const fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(number) => Some(*number),
            Self::Integer(number) => Some(*number as f64),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    path: PathBuf,
    root: toml::Table,
}

impl Config {
    pub fn load(plugin: &str) -> Result<Self, ConfigError> {
        Self::open(Path::new(FOLDER).join(plugin))
    }

    pub fn open(folder: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let mut config = Self {
            path: folder.as_ref().join(FILE),
            root: toml::Table::new(),
        };
        config.reload()?;
        Ok(config)
    }

    pub fn save_default(plugin: &str, text: &str) -> Result<bool, ConfigError> {
        Self::save_default_in(Path::new(FOLDER).join(plugin), text)
    }

    pub fn save_default_in(folder: impl AsRef<Path>, text: &str) -> Result<bool, ConfigError> {
        let folder = folder.as_ref();
        let path = folder.join(FILE);
        if path.exists() {
            return Ok(false);
        }
        let failed = |source| ConfigError::Io {
            path: path.clone(),
            source,
        };
        fs::create_dir_all(folder).map_err(failed)?;
        fs::write(&path, text).map_err(failed)?;
        Ok(true)
    }

    pub fn reload(&mut self) -> Result<(), ConfigError> {
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                self.root = toml::Table::new();
                return Ok(());
            }
            Err(source) => {
                return Err(ConfigError::Io {
                    path: self.path.clone(),
                    source,
                });
            }
        };
        self.root = text
            .parse::<toml::Table>()
            .map_err(|error| ConfigError::Parse {
                path: self.path.clone(),
                message: error.to_string(),
            })?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn find(&self, path: &str) -> Option<&toml::Value> {
        let mut keys = path.split('.');
        let mut value = self.root.get(keys.next()?)?;
        for key in keys {
            value = value.as_table()?.get(key)?;
        }
        Some(value)
    }

    pub fn contains(&self, path: &str) -> bool {
        self.find(path).is_some()
    }

    pub fn get(&self, path: &str) -> Option<ConfigValue> {
        self.find(path).map(ConfigValue::of)
    }

    pub fn get_string(&self, path: &str) -> Option<&str> {
        self.find(path).and_then(toml::Value::as_str)
    }

    pub fn get_i64(&self, path: &str) -> Option<i64> {
        self.find(path).and_then(toml::Value::as_integer)
    }

    pub fn get_bool(&self, path: &str) -> Option<bool> {
        self.find(path).and_then(toml::Value::as_bool)
    }

    pub fn get_f64(&self, path: &str) -> Option<f64> {
        match self.find(path)? {
            toml::Value::Float(number) => Some(*number),
            toml::Value::Integer(number) => Some(*number as f64),
            _ => None,
        }
    }

    pub fn get_list(&self, path: &str) -> Option<Vec<ConfigValue>> {
        self.find(path)
            .and_then(toml::Value::as_array)
            .map(|items| items.iter().map(ConfigValue::of).collect())
    }

    pub fn get_keys(&self, path: &str) -> Option<Vec<&str>> {
        let table = if path.is_empty() {
            &self.root
        } else {
            self.find(path)?.as_table()?
        };
        Some(table.keys().map(String::as_str).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "name = \"Market\"\nopen = true\nlimit = 12\nrate = 0.5\nwhole = 3\ntags = [\"a\", \"b\", 3]\n\n[prices]\ndiamond = 10\n\n[prices.rare]\nnetherite = 99.5\n";

    fn loaded(text: &str) -> (tempfile::TempDir, Config) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE), text).unwrap();
        let config = Config::open(dir.path()).unwrap();
        (dir, config)
    }

    #[test]
    fn typed_getters_read_values_and_dotted_paths() {
        let (_dir, config) = loaded(SAMPLE);
        assert_eq!(config.get_string("name"), Some("Market"));
        assert_eq!(config.get_bool("open"), Some(true));
        assert_eq!(config.get_i64("limit"), Some(12));
        assert_eq!(config.get_f64("rate"), Some(0.5));
        assert_eq!(config.get_i64("prices.diamond"), Some(10));
        assert_eq!(config.get_f64("prices.rare.netherite"), Some(99.5));
        assert!(config.contains("prices.rare"));
        assert_eq!(config.get_keys("prices"), Some(vec!["diamond", "rare"]));
    }

    #[test]
    fn a_getter_of_another_type_or_a_missing_path_is_none() {
        let (_dir, config) = loaded(SAMPLE);
        assert_eq!(config.get_i64("name"), None);
        assert_eq!(config.get_string("limit"), None);
        assert_eq!(config.get_bool("limit"), None);
        assert_eq!(config.get_f64("name"), None);
        assert_eq!(config.get_i64("rate"), None);
        assert_eq!(config.get_string("nope.deeper"), None);
        assert_eq!(config.get_string("name.deeper"), None);
        assert_eq!(config.get_list("name"), None);
    }

    #[test]
    fn an_integer_reads_as_a_float_and_lists_keep_their_items() {
        let (_dir, config) = loaded(SAMPLE);
        assert_eq!(config.get_f64("whole"), Some(3.0));
        assert_eq!(
            config.get_list("tags"),
            Some(vec![
                ConfigValue::String("a".to_owned()),
                ConfigValue::String("b".to_owned()),
                ConfigValue::Integer(3),
            ])
        );
        assert_eq!(config.get("tags").unwrap().as_str(), None);
        assert_eq!(ConfigValue::Integer(2).as_f64(), Some(2.0));
        assert_eq!(ConfigValue::Bool(true).as_bool(), Some(true));
        assert_eq!(ConfigValue::Integer(2).as_i64(), Some(2));
    }

    #[test]
    fn a_missing_file_is_an_empty_config() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::open(dir.path()).unwrap();
        assert_eq!(config.get_string("name"), None);
        assert_eq!(config.get_keys(""), Some(Vec::new()));
    }

    #[test]
    fn a_broken_file_is_reported_with_its_path() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(FILE), "name = ").unwrap();
        let error = Config::open(dir.path()).unwrap_err();
        assert!(matches!(&error, ConfigError::Parse { path, .. } if path.ends_with(FILE)));
        assert!(error.to_string().contains("config.toml"));
    }

    #[test]
    fn save_default_creates_the_folder_and_the_file_once() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("plugins").join("market");
        assert!(Config::save_default_in(&folder, "limit = 1\n").unwrap());
        assert!(!Config::save_default_in(&folder, "limit = 2\n").unwrap());
        let config = Config::open(&folder).unwrap();
        assert_eq!(config.get_i64("limit"), Some(1));
        assert_eq!(config.path(), folder.join(FILE));
    }

    #[test]
    fn reload_picks_up_edits_and_deletions() {
        let (dir, mut config) = loaded("limit = 1\n");
        fs::write(dir.path().join(FILE), "limit = 5\nextra = true\n").unwrap();
        assert_eq!(config.get_i64("limit"), Some(1));
        config.reload().unwrap();
        assert_eq!(config.get_i64("limit"), Some(5));
        assert_eq!(config.get_bool("extra"), Some(true));
        fs::remove_file(dir.path().join(FILE)).unwrap();
        config.reload().unwrap();
        assert!(!config.contains("limit"));
    }

    #[test]
    fn a_failed_reload_keeps_the_last_good_values() {
        let (dir, mut config) = loaded("limit = 1\n");
        fs::write(dir.path().join(FILE), "limit = ").unwrap();
        assert!(config.reload().is_err());
        assert_eq!(config.get_i64("limit"), Some(1));
    }

    #[test]
    fn load_reads_below_the_plugins_folder_of_the_working_directory() {
        let config = Config::load("market-not-there").unwrap();
        assert_eq!(
            config.path(),
            Path::new("plugins/market-not-there/config.toml")
        );
        assert!(!config.contains("anything"));
    }
}
