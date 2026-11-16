use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use thiserror::Error;

use super::data::short;
use super::placement::Placement;
use super::template;

#[derive(Debug, Error)]
pub(crate) enum PackError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Nbt(#[from] rspigot_nbt::NbtError),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadIssue {
    pub path: PathBuf,
    pub reason: String,
}

impl LoadIssue {
    pub(crate) fn new(path: &Path, error: impl fmt::Display) -> Self {
        Self {
            path: path.to_owned(),
            reason: error.to_string(),
        }
    }
}

impl fmt::Display for LoadIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.reason)
    }
}

#[derive(Debug, Default)]
pub(crate) struct TagFile {
    pub(crate) replace: bool,
    pub(crate) values: Vec<String>,
}

#[derive(Debug, Default)]
pub(crate) struct Pack {
    pub(crate) structures: Vec<(String, Value)>,
    pub(crate) sets: Vec<(String, Value)>,
    pub(crate) pools: Vec<(String, Value)>,
    pub(crate) lists: Vec<(String, Value)>,
    pub(crate) templates: Vec<(String, Value)>,
    pub(crate) biome_tags: Vec<(String, TagFile)>,
}

fn invalid(text: &str) -> PackError {
    PackError::Invalid(text.to_owned())
}

fn files(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) -> Result<(), std::io::Error> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            files(&path, extension, out)?;
        } else if path.extension().is_some_and(|found| found == extension) {
            out.push(path);
        }
    }
    Ok(())
}

fn ident(namespace: &str, base: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(base).unwrap_or(path).with_extension("");
    let parts: Vec<String> = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    format!("{namespace}:{}", parts.join("/"))
}

pub(crate) fn short_key(ident: &str) -> String {
    short(ident).to_owned()
}

fn read_json(path: &Path) -> Result<Value, PackError> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

fn text(value: &Value, key: &str) -> Result<(), PackError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(drop)
        .ok_or_else(|| PackError::Invalid(format!("missing \"{key}\"")))
}

fn check_structure(value: &Value) -> Result<(), PackError> {
    text(value, "type")?;
    if value.get("biomes").is_none() {
        return Err(invalid("missing \"biomes\""));
    }
    if value.get("type").and_then(Value::as_str).map(short) == Some("jigsaw") {
        text(value, "start_pool")?;
        value
            .get("size")
            .and_then(Value::as_i64)
            .ok_or_else(|| invalid("missing \"size\""))?;
    }
    Ok(())
}

fn check_set(value: &Value) -> Result<(), PackError> {
    let placement = value
        .get("placement")
        .ok_or_else(|| invalid("missing \"placement\""))?;
    Placement::parse(placement).ok_or_else(|| invalid("unsupported or malformed placement"))?;
    let entries = value
        .get("structures")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("missing \"structures\""))?;
    for entry in entries {
        text(entry, "structure")?;
        entry
            .get("weight")
            .and_then(Value::as_i64)
            .ok_or_else(|| invalid("missing \"weight\""))?;
    }
    Ok(())
}

fn check_pool(value: &Value) -> Result<(), PackError> {
    let elements = value
        .get("elements")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("missing \"elements\""))?;
    for entry in elements {
        entry
            .get("weight")
            .and_then(Value::as_i64)
            .ok_or_else(|| invalid("missing \"weight\""))?;
        let element = entry
            .get("element")
            .ok_or_else(|| invalid("missing \"element\""))?;
        text(element, "element_type")?;
    }
    Ok(())
}

fn check_list(value: &Value) -> Result<(), PackError> {
    value
        .get("processors")
        .and_then(Value::as_array)
        .map(drop)
        .ok_or_else(|| invalid("missing \"processors\""))
}

fn tag_file(value: &Value) -> Result<TagFile, PackError> {
    let items = value
        .get("values")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("missing \"values\""))?;
    Ok(TagFile {
        replace: value
            .get("replace")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        values: items
            .iter()
            .filter_map(|item| match item {
                Value::String(name) => Some(name.clone()),
                Value::Object(map) => map.get("id").and_then(Value::as_str).map(str::to_owned),
                _ => None,
            })
            .collect(),
    })
}

fn json_kind(
    folder: &Path,
    namespace: &str,
    check: fn(&Value) -> Result<(), PackError>,
    out: &mut Vec<(String, Value)>,
    issues: &mut Vec<LoadIssue>,
) {
    let mut found = Vec::new();
    if !folder.is_dir() || files(folder, "json", &mut found).is_err() {
        return;
    }
    for path in found {
        let loaded = read_json(&path).and_then(|value| check(&value).map(|()| value));
        match loaded {
            Ok(value) => out.push((ident(namespace, folder, &path), value)),
            Err(error) => issues.push(LoadIssue::new(&path, error)),
        }
    }
}

pub(crate) fn read(root: &Path, issues: &mut Vec<LoadIssue>) -> Pack {
    let mut pack = Pack::default();
    let Ok(namespaces) = fs::read_dir(root.join("data")) else {
        return pack;
    };
    let mut namespaces: Vec<PathBuf> = namespaces
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    namespaces.sort();
    for base in namespaces {
        let namespace = base
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let worldgen = base.join("worldgen");
        json_kind(
            &worldgen.join("structure"),
            &namespace,
            check_structure,
            &mut pack.structures,
            issues,
        );
        json_kind(
            &worldgen.join("structure_set"),
            &namespace,
            check_set,
            &mut pack.sets,
            issues,
        );
        json_kind(
            &worldgen.join("template_pool"),
            &namespace,
            check_pool,
            &mut pack.pools,
            issues,
        );
        json_kind(
            &worldgen.join("processor_list"),
            &namespace,
            check_list,
            &mut pack.lists,
            issues,
        );
        let mut tags = Vec::new();
        let tag_root = base.join("tags/worldgen/biome");
        json_kind(
            &tag_root,
            &namespace,
            |value| tag_file(value).map(drop),
            &mut tags,
            issues,
        );
        for (name, value) in tags {
            if let Ok(file) = tag_file(&value) {
                pack.biome_tags.push((name, file));
            }
        }
        let template_root = base.join("structure");
        let mut found = Vec::new();
        if template_root.is_dir() && files(&template_root, "nbt", &mut found).is_ok() {
            for path in found {
                let decoded = fs::read(&path)
                    .map_err(PackError::from)
                    .and_then(|bytes| template::decode(&bytes));
                match decoded {
                    Ok(value) => pack
                        .templates
                        .push((ident(&namespace, &template_root, &path), value)),
                    Err(error) => issues.push(LoadIssue::new(&path, error)),
                }
            }
        }
    }
    pack
}

pub(crate) fn is_pack(dir: &Path) -> Result<(), PackError> {
    let meta = read_json(&dir.join("pack.mcmeta"))?;
    meta.get("pack")
        .and_then(Value::as_object)
        .map(drop)
        .ok_or_else(|| invalid("pack.mcmeta has no \"pack\" section"))
}
