use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde_json::value::RawValue;
use serde_json::{Map, Value, json};

use super::blueprint::Blueprint;
use super::data::short;
use super::pack::{self, LoadIssue, Pack, PackError, TagFile, short_key};
use super::pool::Pools;
use super::processors::{Step, compile_list};
use super::template;

const STRUCTURES: &str = include_str!("../../data/structures.json");
const TEMPLATES: &str = include_str!("../../data/jigsaw_templates.json");
const BIOME_REGISTRY: &str = "minecraft:worldgen/biome";
const TAG_DEPTH: usize = 16;

#[derive(Debug)]
pub(crate) struct Library {
    sets: Map<String, Value>,
    structures: Map<String, Value>,
    pools: Map<String, Value>,
    lists: Map<String, Value>,
    blueprints: HashMap<String, Lazy>,
    compiled_pools: OnceLock<Pools>,
    compiled_lists: OnceLock<HashMap<String, Vec<Step>>>,
}

#[derive(Debug)]
enum Source {
    Embedded(&'static RawValue),
    File(PathBuf),
}

#[derive(Debug)]
struct Lazy {
    source: Source,
    fallback: Option<&'static RawValue>,
    parsed: OnceLock<Option<Blueprint>>,
}

impl Lazy {
    fn new(source: Source) -> Self {
        Self {
            source,
            fallback: None,
            parsed: OnceLock::new(),
        }
    }

    fn overriding(path: PathBuf, previous: Option<&Self>) -> Self {
        let fallback = previous.and_then(|entry| match entry.source {
            Source::Embedded(raw) => Some(raw),
            Source::File(_) => entry.fallback,
        });
        Self {
            fallback,
            ..Self::new(Source::File(path))
        }
    }

    fn get(&self, name: &str) -> Option<&Blueprint> {
        self.parsed.get_or_init(|| self.load(name)).as_ref()
    }

    fn load(&self, name: &str) -> Option<Blueprint> {
        let loaded = match &self.source {
            Source::Embedded(raw) => serde_json::from_str::<Value>(raw.get())
                .map_err(PackError::from)
                .and_then(|value| Self::parse(&value)),
            Source::File(path) => fs::read(path)
                .map_err(PackError::from)
                .and_then(|bytes| template::decode(&bytes))
                .and_then(|value| Self::parse(&value)),
        };
        match loaded {
            Ok(blueprint) => Some(blueprint),
            Err(error) => {
                match &self.source {
                    Source::Embedded(_) => {
                        tracing::warn!(template = name, %error, "template is malformed");
                    }
                    Source::File(path) => {
                        tracing::warn!(
                            template = name,
                            path = %path.display(),
                            %error,
                            "template is malformed"
                        );
                    }
                }
                self.fallback.and_then(|raw| {
                    serde_json::from_str::<Value>(raw.get())
                        .ok()
                        .and_then(|value| Blueprint::parse(&value))
                })
            }
        }
    }

    fn parse(value: &Value) -> Result<Blueprint, PackError> {
        Blueprint::parse(value).ok_or_else(|| PackError::Invalid("not a valid template".to_owned()))
    }
}

static INSTALLED: OnceLock<&'static Library> = OnceLock::new();
static EMBEDDED: OnceLock<&'static Library> = OnceLock::new();

fn section(data: &mut Value, key: &str) -> Map<String, Value> {
    match data.get_mut(key).map(Value::take) {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

struct BiomeTags {
    custom: HashMap<String, (bool, Vec<String>)>,
}

impl BiomeTags {
    fn new(packs: &[Pack]) -> Self {
        let mut custom: HashMap<String, (bool, Vec<String>)> = HashMap::new();
        for pack in packs {
            for (name, TagFile { replace, values }) in &pack.biome_tags {
                let entry = custom.entry(name.clone()).or_default();
                if *replace {
                    *entry = (true, Vec::new());
                }
                entry.1.extend(values.iter().cloned());
            }
        }
        Self { custom }
    }

    fn builtin(tag: &str) -> Vec<String> {
        let entries = rspigot_data::registries::synced_entries(BIOME_REGISTRY).unwrap_or(&[]);
        rspigot_data::tags::tag(BIOME_REGISTRY, tag)
            .unwrap_or(&[])
            .iter()
            .filter_map(|id| entries.get(usize::try_from(*id).ok()?))
            .map(|name| short_key(name))
            .collect()
    }

    fn expand(&self, tag: &str, depth: usize, out: &mut Vec<String>) {
        if depth > TAG_DEPTH {
            return;
        }
        let custom = self.custom.get(tag);
        if !custom.is_some_and(|(replaced, _)| *replaced) {
            out.extend(Self::builtin(tag));
        }
        for value in custom.map_or(&[][..], |(_, values)| values) {
            match value.strip_prefix('#') {
                Some(inner) => self.expand(inner, depth + 1, out),
                None => out.push(short_key(value)),
            }
        }
    }

    fn resolve(&self, value: &Value) -> Vec<String> {
        let mut out = Vec::new();
        let mut add = |text: &str| match text.strip_prefix('#') {
            Some(tag) => self.expand(tag, 0, &mut out),
            None => out.push(short_key(text)),
        };
        match value {
            Value::String(text) => add(text),
            Value::Array(items) => items.iter().filter_map(Value::as_str).for_each(&mut add),
            _ => {}
        }
        out.sort();
        out.dedup();
        out
    }
}

fn templates_of(element: &Value, out: &mut Vec<String>) {
    match element
        .get("element_type")
        .and_then(Value::as_str)
        .map(short)
    {
        Some("single_pool_element" | "legacy_single_pool_element") => {
            if let Some(location) = element.get("location").and_then(Value::as_str) {
                out.push(location.to_owned());
            }
        }
        Some("list_pool_element") => {
            for inner in element
                .get("elements")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                templates_of(inner, out);
            }
        }
        _ => {}
    }
}

impl Library {
    fn base() -> Self {
        let mut data = serde_json::from_str::<Value>(STRUCTURES).unwrap_or(Value::Null);
        let blueprints = serde_json::from_str::<HashMap<String, &'static RawValue>>(TEMPLATES)
            .unwrap_or_default()
            .into_iter()
            .map(|(name, raw)| (name, Lazy::new(Source::Embedded(raw))))
            .collect();
        Self {
            sets: section(&mut data, "sets"),
            structures: section(&mut data, "structures"),
            pools: section(&mut data, "pools"),
            lists: section(&mut data, "processor_lists"),
            blueprints,
            compiled_pools: OnceLock::new(),
            compiled_lists: OnceLock::new(),
        }
    }

    pub(crate) fn embedded() -> &'static Self {
        EMBEDDED.get_or_init(|| Box::leak(Box::new(Self::base())))
    }

    pub(crate) fn current() -> &'static Self {
        INSTALLED.get().copied().unwrap_or_else(Self::embedded)
    }

    pub(crate) fn install(library: Self) -> Result<(), String> {
        INSTALLED
            .set(Box::leak(Box::new(library)))
            .map_err(|_| "a datapack set is already installed".to_owned())
    }

    pub(crate) fn discover(dir: &Path) -> (Vec<PathBuf>, Vec<LoadIssue>) {
        let mut packs = Vec::new();
        let mut issues = Vec::new();
        let Ok(entries) = fs::read_dir(dir) else {
            return (packs, issues);
        };
        let mut entries: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        entries.sort();
        for path in entries {
            let hidden = path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with('.'));
            if hidden {
                continue;
            }
            if !path.is_dir() {
                if path.extension().is_some_and(|ext| ext == "zip") {
                    issues.push(LoadIssue::new(
                        &path,
                        "zipped datapacks are not supported, extract it",
                    ));
                }
                continue;
            }
            match pack::is_pack(&path) {
                Ok(()) => packs.push(path),
                Err(error) => issues.push(LoadIssue::new(&path, error)),
            }
        }
        (packs, issues)
    }

    pub(crate) fn build(roots: &[PathBuf]) -> (Self, Vec<LoadIssue>) {
        let mut library = Self::base();
        let mut issues = Vec::new();
        let packs: Vec<Pack> = roots
            .iter()
            .map(|root| pack::read(root, &mut issues))
            .collect();
        let tags = BiomeTags::new(&packs);
        for (root, pack) in roots.iter().zip(packs) {
            library.apply(root, pack, &tags, &mut issues);
        }
        (library, issues)
    }

    fn apply(&mut self, root: &Path, pack: Pack, tags: &BiomeTags, issues: &mut Vec<LoadIssue>) {
        for (name, mut value) in pack.structures {
            if let Err(reason) = super::structure::validate(&value) {
                issues.push(LoadIssue::new(
                    root,
                    format!("structure {name} was not installed: {reason}"),
                ));
                continue;
            }
            let biomes = tags.resolve(value.get("biomes").unwrap_or(&Value::Null));
            value["biomes"] = json!(biomes);
            self.structures.insert(short_key(&name), value);
        }
        for (name, value) in pack.sets {
            let mut placement = value.get("placement").cloned().unwrap_or(Value::Null);
            if let Some(preferred) = placement.get("preferred_biomes") {
                placement["preferred_biomes"] = json!(tags.resolve(preferred));
            }
            let structures: Vec<Value> = value
                .get("structures")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|entry| {
                    Some(json!({
                        "structure": short_key(entry.get("structure")?.as_str()?),
                        "weight": entry.get("weight")?,
                    }))
                })
                .collect();
            self.sets.insert(
                short_key(&name),
                json!({"placement": placement, "structures": structures}),
            );
        }
        let mut pool_names = Vec::new();
        for (name, value) in pack.pools {
            pool_names.push(name.clone());
            self.pools.insert(name, value);
        }
        for (name, value) in pack.lists {
            self.lists.insert(name, value);
        }
        for (name, path) in pack.templates {
            let entry = Lazy::overriding(path, self.blueprints.get(&name));
            self.blueprints.insert(name, entry);
        }
        for name in pool_names {
            let mut wanted = Vec::new();
            let elements = self.pools[&name].get("elements").and_then(Value::as_array);
            for entry in elements.into_iter().flatten() {
                if let Some(element) = entry.get("element") {
                    templates_of(element, &mut wanted);
                }
            }
            for template in wanted {
                if !self.blueprints.contains_key(&template) {
                    issues.push(LoadIssue::new(
                        root,
                        format!("pool {name} uses the missing template {template}"),
                    ));
                }
            }
        }
    }

    pub(crate) fn sets(&self) -> &Map<String, Value> {
        &self.sets
    }

    pub(crate) fn structures(&self) -> &Map<String, Value> {
        &self.structures
    }

    pub(crate) fn pool_defs(&self) -> &Map<String, Value> {
        &self.pools
    }

    pub(crate) fn blueprint(&self, name: &str) -> Option<&Blueprint> {
        self.blueprints.get(name)?.get(name)
    }

    #[cfg(test)]
    pub(crate) fn parsed(&self) -> usize {
        self.blueprints
            .values()
            .filter(|entry| entry.parsed.get().is_some())
            .count()
    }

    pub(crate) fn blueprint_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.blueprints.keys().cloned().collect();
        names.sort_unstable();
        names
    }

    pub(crate) fn pools(&'static self) -> &'static Pools {
        self.compiled_pools.get_or_init(|| Pools::load(self))
    }

    pub(crate) fn steps(&self, name: &str) -> Option<&[Step]> {
        self.compiled_lists
            .get_or_init(|| {
                self.lists
                    .iter()
                    .map(|(name, value)| (name.clone(), compile_list(value)))
                    .collect()
            })
            .get(name)
            .map(Vec::as_slice)
    }
}
