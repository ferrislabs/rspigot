use std::path::PathBuf;

use crate::config::{Config, ConfigError, ConfigValue};

pub trait FromConfig: Sized {
    fn from_config(config: &Config) -> Result<Self, ConfigError>;
}

pub trait FromValue: Sized {
    const OPTIONAL: bool = false;

    fn from_value(value: &ConfigValue) -> Option<Self>;

    fn absent() -> Option<Self> {
        None
    }
}

impl FromValue for String {
    fn from_value(value: &ConfigValue) -> Option<Self> {
        value.as_str().map(str::to_owned)
    }
}

impl FromValue for bool {
    fn from_value(value: &ConfigValue) -> Option<Self> {
        value.as_bool()
    }
}

impl FromValue for f64 {
    fn from_value(value: &ConfigValue) -> Option<Self> {
        value.as_f64()
    }
}

impl FromValue for f32 {
    fn from_value(value: &ConfigValue) -> Option<Self> {
        value.as_f64().map(|number| number as f32)
    }
}

macro_rules! integers {
    ($($ty:ty),*) => {$(
        impl FromValue for $ty {
            fn from_value(value: &ConfigValue) -> Option<Self> {
                value.as_i64().and_then(|number| Self::try_from(number).ok())
            }
        }
    )*};
}

integers!(i8, i16, i32, i64, u8, u16, u32, u64, usize, isize);

impl<T: FromValue> FromValue for Vec<T> {
    fn from_value(value: &ConfigValue) -> Option<Self> {
        match value {
            ConfigValue::List(items) => items.iter().map(T::from_value).collect(),
            _ => None,
        }
    }
}

impl<T: FromValue> FromValue for Option<T> {
    const OPTIONAL: bool = true;

    fn from_value(value: &ConfigValue) -> Option<Self> {
        T::from_value(value).map(Some)
    }

    fn absent() -> Option<Self> {
        Some(None)
    }
}

pub fn read_field<T: FromValue>(config: &Config, key: &str) -> Result<T, ConfigError> {
    let invalid = |message: String| ConfigError::Parse {
        path: config.path.clone(),
        message,
    };
    match config.get(key) {
        Some(value) => T::from_value(&value)
            .ok_or_else(|| invalid(format!("`{key}` has the wrong type or is out of range"))),
        None => T::absent().ok_or_else(|| invalid(format!("`{key}` is missing"))),
    }
}

pub fn load_typed<T: FromConfig>(folder: impl Into<PathBuf>) -> Result<T, ConfigError> {
    T::from_config(&Config::open(folder.into())?)
}

pub fn load_typed_for<T: FromConfig>(plugin: &str) -> Result<T, ConfigError> {
    T::from_config(&Config::load(plugin)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded(text: &str) -> (tempfile::TempDir, Config) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("config.toml"), text).unwrap();
        let config = Config::open(dir.path()).unwrap();
        (dir, config)
    }

    #[test]
    fn fields_read_by_type_and_dotted_path() {
        let (_dir, config) = loaded(
            "name = \"x\"\nlimit = 3\nrate = 2\nnested = { on = true }\ntags = [\"a\", \"b\"]\n",
        );
        assert_eq!(read_field::<String>(&config, "name").unwrap(), "x");
        assert_eq!(read_field::<u16>(&config, "limit").unwrap(), 3);
        assert_eq!(read_field::<f64>(&config, "rate").unwrap(), 2.0);
        assert!(read_field::<bool>(&config, "nested.on").unwrap());
        assert_eq!(
            read_field::<Vec<String>>(&config, "tags").unwrap(),
            ["a", "b"]
        );
    }

    #[test]
    fn a_missing_key_is_an_error_unless_the_field_is_optional() {
        let (_dir, config) = loaded("limit = 3\n");
        let error = read_field::<String>(&config, "name").unwrap_err();
        assert!(error.to_string().contains("`name` is missing"));
        assert_eq!(read_field::<Option<String>>(&config, "name").unwrap(), None);
        assert_eq!(
            read_field::<Option<i64>>(&config, "limit").unwrap(),
            Some(3)
        );
    }

    #[test]
    fn a_value_of_the_wrong_type_or_out_of_range_is_refused() {
        let (_dir, config) = loaded("limit = 70000\nname = 1\n");
        assert!(read_field::<u16>(&config, "limit").is_err());
        assert!(read_field::<String>(&config, "name").is_err());
        assert!(read_field::<Option<String>>(&config, "name").is_err());
    }
}
