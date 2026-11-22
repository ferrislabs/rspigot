use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use rspigot_core::worlds::{register_dimension_spawns, register_dimension_template};
use rspigot_data::spawns::{DimensionSpawns, SpawnCount};
use rspigot_domain::Identifier;
use rspigot_domain::world::{Dimension, WorldHeight};
use serde_json::{Map, Value, json};

use crate::carver::Carver;
use crate::datapack;
use crate::overworld::features::bodies::Compiler;
use crate::overworld::features::place::Modifier;
use crate::overworld::surface_rules::unsupported_in_rule;
use crate::structures::{LoadIssue, install_roots};
use crate::terrain::{check_function, check_settings};

const SETTINGS: [(&str, &str); 3] = [
    ("minecraft:overworld", "normal"),
    ("minecraft:large_biomes", "large_biomes"),
    ("minecraft:amplified", "amplified"),
];

const VANILLA_TYPES: [&str; 4] = [
    "minecraft:overworld",
    "minecraft:overworld_caves",
    "minecraft:the_nether",
    "minecraft:the_end",
];

const UNSUPPORTED_FOLDERS: [(&str, &str); 5] = [
    (
        "multi_noise_biome_source_parameter_list",
        "biome climate parameters are compiled into the server",
    ),
    (
        "material_rule",
        "surface and ore vein rules are compiled into the server",
    ),
    (
        "material_condition",
        "surface rules are compiled into the server",
    ),
    (
        "block_state_provider",
        "block state providers are compiled into the server",
    ),
    ("world_preset", "world presets are not read"),
];

type Source = (Value, PathBuf);
type Table = BTreeMap<String, Source>;

#[derive(Debug, Default)]
pub(crate) struct Overlay {
    noises: Table,
    functions: Table,
    settings: Table,
    biomes: Table,
    placed: Table,
    configured: Table,
    carvers: Table,
    dimensions: Table,
    dimension_types: Table,
}

static INSTALLED: OnceLock<Installed> = OnceLock::new();

#[derive(Debug, Default)]
struct Installed {
    overlay: BTreeMap<&'static str, Vec<(String, Value)>>,
    aliases: BTreeMap<String, String>,
    settings: BTreeSet<String>,
}

pub(crate) fn apply(base: &mut Value) {
    let Some(installed) = INSTALLED.get() else {
        return;
    };
    for (section, entries) in &installed.overlay {
        merge_section(base, section, entries);
    }
}

pub(crate) fn alias(id: &str) -> Option<String> {
    let installed = INSTALLED.get()?;
    let target = match installed.aliases.get(id) {
        Some(target) => target.as_str(),
        None if installed.settings.contains(id) => id,
        None => return None,
    };
    Some(terrain_key(target).to_owned())
}

fn terrain_key(settings: &str) -> &str {
    SETTINGS
        .iter()
        .find(|(name, _)| *name == settings)
        .map_or(settings, |(_, key)| key)
}

fn merge_section(base: &mut Value, section: &str, entries: &[(String, Value)]) {
    for (id, value) in entries {
        match section {
            "functions" => put(base, &["terrain", "functions"], id, value.clone()),
            "variants" => {
                let existing = base
                    .get_mut("terrain")
                    .and_then(|terrain| terrain.get_mut("variants"))
                    .and_then(|variants| variants.get_mut(id.as_str()))
                    .and_then(Value::as_object_mut);
                match (existing, value.as_object()) {
                    (Some(variant), Some(fields)) => {
                        for (key, field) in fields {
                            variant.insert(key.clone(), field.clone());
                        }
                    }
                    (None, _) => put(base, &["terrain", "variants"], id, value.clone()),
                    _ => {}
                }
            }
            "biomes" => {
                if let Some(entry) = base
                    .get_mut("biomes")
                    .and_then(|biomes| biomes.get_mut(id.as_str()))
                    .and_then(Value::as_object_mut)
                    && let Some(fields) = value.as_object()
                {
                    for (key, field) in fields {
                        entry.insert(key.clone(), field.clone());
                    }
                }
            }
            other => put(base, &[other], id, value.clone()),
        }
    }
}

fn put(base: &mut Value, path: &[&str], id: &str, value: Value) {
    let mut node = base;
    for key in path {
        if node.get(*key).is_none_or(|child| !child.is_object()) {
            node[*key] = Value::Object(Map::new());
        }
        node = &mut node[*key];
    }
    node[id] = value;
}

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            out.push(path);
        }
    }
}

fn read_folder(
    base: &Path,
    namespace: &str,
    folder: &str,
    table: &mut Table,
    issues: &mut Vec<LoadIssue>,
) {
    let root = base.join(folder);
    let mut found = Vec::new();
    files(&root, &mut found);
    for path in found {
        let relative = path.strip_prefix(&root).unwrap_or(&path).with_extension("");
        let parts: Vec<String> = relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect();
        let id = format!("{namespace}:{}", parts.join("/"));
        match fs::read(&path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).map_err(|e| e.to_string()))
        {
            Ok(value) => {
                table.insert(id, (value, path));
            }
            Err(error) => issues.push(LoadIssue::new(&path, error)),
        }
    }
}

impl Overlay {
    fn read(roots: &[PathBuf], issues: &mut Vec<LoadIssue>) -> Self {
        let mut overlay = Self::default();
        for root in roots {
            let Ok(namespaces) = fs::read_dir(root.join("data")) else {
                continue;
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
                let read =
                    |dir: &Path, folder: &str, table: &mut Table, issues: &mut Vec<LoadIssue>| {
                        read_folder(dir, &namespace, folder, table, issues);
                    };
                read(&worldgen, "noise", &mut overlay.noises, issues);
                read(
                    &worldgen,
                    "density_function",
                    &mut overlay.functions,
                    issues,
                );
                read(&worldgen, "noise_settings", &mut overlay.settings, issues);
                read(&worldgen, "biome", &mut overlay.biomes, issues);
                read(&worldgen, "placed_feature", &mut overlay.placed, issues);
                read(&worldgen, "feature", &mut overlay.configured, issues);
                read(&worldgen, "carver", &mut overlay.carvers, issues);
                read(&worldgen, "configured_carver", &mut overlay.carvers, issues);
                read(&base, "dimension", &mut overlay.dimensions, issues);
                for (folder, reason) in UNSUPPORTED_FOLDERS {
                    let mut ignored = Table::new();
                    read(&worldgen, folder, &mut ignored, issues);
                    for (id, (_, path)) in ignored {
                        issues.push(LoadIssue::new(
                            &path,
                            format!("{id} cannot override worldgen: {reason}"),
                        ));
                    }
                }
                read(
                    &base,
                    "dimension_type",
                    &mut overlay.dimension_types,
                    issues,
                );
            }
        }
        overlay
    }

    fn drop_entry(table: &mut Table, id: &str, issues: &mut Vec<LoadIssue>, reason: String) {
        if let Some((_, path)) = table.remove(id) {
            issues.push(LoadIssue::new(&path, reason));
        }
    }

    fn validate_noises(&mut self, issues: &mut Vec<LoadIssue>) {
        let bad: Vec<(String, String)> = self
            .noises
            .iter()
            .filter_map(|(id, (value, _))| noise_entry(id, value).err().map(|e| (id.clone(), e)))
            .collect();
        for (id, reason) in bad {
            Self::drop_entry(&mut self.noises, &id, issues, reason);
        }
    }

    fn validate_carvers(&mut self, issues: &mut Vec<LoadIssue>) {
        let bad: Vec<(String, String)> = self
            .carvers
            .iter()
            .filter_map(|(id, (value, _))| Carver::parse(value).err().map(|e| (id.clone(), e)))
            .collect();
        for (id, reason) in bad {
            Self::drop_entry(
                &mut self.carvers,
                &id,
                issues,
                format!("carver {id}: {reason}"),
            );
        }
    }

    fn validate_biomes(&mut self, issues: &mut Vec<LoadIssue>) {
        let bad: Vec<(String, String)> = self
            .biomes
            .iter()
            .filter_map(|(id, (value, _))| biome_entry(value).err().map(|e| (id.clone(), e)))
            .collect();
        for (id, reason) in bad {
            Self::drop_entry(
                &mut self.biomes,
                &id,
                issues,
                format!("biome {id}: {reason}"),
            );
        }
    }

    fn merged(&self) -> Value {
        let mut value = datapack::embedded();
        let mut installed = Installed::default();
        self.fill(&mut installed);
        for (section, entries) in &installed.overlay {
            merge_section(&mut value, section, entries);
        }
        value
    }

    fn fill(&self, installed: &mut Installed) {
        let table =
            |table: &Table, convert: &dyn Fn(&str, &Value) -> Value| -> Vec<(String, Value)> {
                table
                    .iter()
                    .map(|(id, (value, _))| (id.clone(), convert(id, value)))
                    .collect()
            };
        let same = |_: &str, value: &Value| value.clone();
        let overlay = &mut installed.overlay;
        overlay.insert(
            "noises",
            table(&self.noises, &|id, value| {
                noise_entry(id, value).unwrap_or(Value::Null)
            }),
        );
        overlay.insert("functions", table(&self.functions, &same));
        overlay.insert("variants", self.variant_entries());
        overlay.insert(
            "biomes",
            table(&self.biomes, &|_, value| {
                biome_entry(value).unwrap_or(Value::Null)
            }),
        );
        overlay.insert("placed", table(&self.placed, &same));
        overlay.insert("configured", table(&self.configured, &same));
        overlay.insert("carvers", table(&self.carvers, &same));
    }

    fn variant_entries(&self) -> Vec<(String, Value)> {
        self.settings
            .iter()
            .filter_map(|(name, (value, _))| {
                Some((terrain_key(name).to_owned(), settings_entry(value).ok()?))
            })
            .collect()
    }

    fn lift_inline_settings(&mut self) {
        let inline: Vec<(String, Source)> = self
            .dimensions
            .iter()
            .filter_map(|(id, (value, path))| match dimension_settings(value) {
                Ok(Setting::Inline(settings)) => {
                    Some((inline_id(id), (settings.clone(), path.clone())))
                }
                _ => None,
            })
            .collect();
        self.settings.extend(inline);
    }

    fn validate_dimension_types(&mut self, issues: &mut Vec<LoadIssue>) {
        let bad: Vec<(String, String)> = self
            .dimension_types
            .iter()
            .filter_map(|(id, (value, _))| {
                let reason = if VANILLA_TYPES.contains(&id.as_str()) {
                    "dimension types of the game are compiled into the server".to_owned()
                } else {
                    dimension_template(value, 63).err()?
                };
                Some((id.clone(), reason))
            })
            .collect();
        for (id, reason) in bad {
            Self::drop_entry(
                &mut self.dimension_types,
                &id,
                issues,
                format!("dimension type {id} cannot be used: {reason}"),
            );
        }
    }

    fn validate_functions(&mut self, issues: &mut Vec<LoadIssue>) {
        loop {
            let merged = self.merged();
            let terrain = &merged["terrain"]["functions"];
            let failed: Vec<(String, String)> = self
                .functions
                .keys()
                .filter_map(|id| {
                    check_function(terrain, &merged["noises"], id)
                        .err()
                        .map(|reason| (id.clone(), reason))
                })
                .collect();
            if failed.is_empty() {
                return;
            }
            for (id, reason) in failed {
                Self::drop_entry(&mut self.functions, &id, issues, reason);
            }
        }
    }

    fn validate_settings(&mut self, issues: &mut Vec<LoadIssue>) {
        let names: Vec<String> = self.settings.keys().cloned().collect();
        for name in names {
            if name.starts_with("minecraft:") && !SETTINGS.iter().any(|(known, _)| *known == name) {
                Self::drop_entry(
                    &mut self.settings,
                    &name,
                    issues,
                    format!(
                        "noise settings {name} is not one of minecraft:overworld, minecraft:large_biomes or minecraft:amplified, which are the only vanilla ones the server can override"
                    ),
                );
                continue;
            }
            let Some((value, _)) = self.settings.get(&name) else {
                continue;
            };
            let checked = settings_entry(value).and_then(|entry| {
                let merged = self.merged();
                check_settings(&merged["terrain"]["functions"], &merged["noises"], &entry)
            });
            if let Err(reason) = checked {
                Self::drop_entry(
                    &mut self.settings,
                    &name,
                    issues,
                    format!("noise settings {name}: {reason}"),
                );
                continue;
            }
            self.check_surface_rule(&name, issues);
        }
    }

    fn report_unsupported_features(&self, issues: &mut Vec<LoadIssue>) {
        let merged = self.merged();
        let object = |key: &str| {
            merged
                .get(key)
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default()
        };
        let (placed, configured) = (object("placed"), object("configured"));
        let mut compiler = Compiler::new(&placed, &configured);
        for (id, (value, path)) in &self.configured {
            if !compiler.is_supported(value) {
                let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
                issues.push(LoadIssue::new(
                    path,
                    format!(
                        "configured feature {id} has the unsupported type {kind:?}, it will place nothing"
                    ),
                ));
            }
        }
        for (id, (value, path)) in &self.placed {
            let modifiers = value
                .get("placement")
                .and_then(Value::as_array)
                .into_iter()
                .flatten();
            for modifier in modifiers {
                if !Modifier::compile(modifier).is_supported() {
                    let kind = modifier.get("type").and_then(Value::as_str).unwrap_or("");
                    issues.push(LoadIssue::new(
                        path,
                        format!(
                            "placed feature {id} has the unsupported placement modifier {kind:?}, it is ignored"
                        ),
                    ));
                }
            }
        }
    }

    fn check_surface_rule(&mut self, name: &str, issues: &mut Vec<LoadIssue>) {
        let Some((value, path)) = self.settings.get(name) else {
            return;
        };
        let Some(rule) = value.get("surface_rule") else {
            return;
        };
        let merged = self.merged();
        let found = unsupported_in_rule(rule, &merged["noises"]);
        if found.is_empty() {
            return;
        }
        let path = path.clone();
        if let Some((value, _)) = self.settings.get_mut(name)
            && let Some(map) = value.as_object_mut()
        {
            map.remove("surface_rule");
        }
        issues.push(LoadIssue::new(
            &path,
            format!(
                "noise settings {name}: the surface rule uses {}, the rule is not applied and the default one stays",
                found.join(", ")
            ),
        ));
    }

    fn validate_carver_lists(&mut self, issues: &mut Vec<LoadIssue>) {
        loop {
            let merged = self.merged();
            let bad: Vec<(String, String)> = ["minecraft:plains", "minecraft:nether_wastes"]
                .iter()
                .filter_map(|biome| {
                    crate::carver::Carvers::for_biome(&merged, biome)
                        .err()
                        .map(|reason| ((*biome).to_owned(), reason))
                })
                .collect();
            if bad.is_empty() {
                return;
            }
            for (biome, reason) in bad {
                if self.biomes.contains_key(&biome) {
                    Self::drop_entry(
                        &mut self.biomes,
                        &biome,
                        issues,
                        format!("biome {biome}: {reason}"),
                    );
                } else {
                    let carvers: Vec<String> = self.carvers.keys().cloned().collect();
                    for id in carvers {
                        Self::drop_entry(&mut self.carvers, &id, issues, reason.clone());
                    }
                }
            }
        }
    }

    fn aliases(&self, issues: &mut Vec<LoadIssue>) -> BTreeMap<String, String> {
        let mut aliases = BTreeMap::new();
        for (id, (value, path)) in &self.dimensions {
            match dimension_settings(value) {
                Ok(Setting::Named(settings))
                    if self.settings.contains_key(settings)
                        || SETTINGS.iter().any(|(name, _)| *name == settings) =>
                {
                    aliases.insert(id.clone(), settings.to_owned());
                }
                Ok(Setting::Inline(_)) if self.settings.contains_key(&inline_id(id)) => {
                    aliases.insert(id.clone(), inline_id(id));
                }
                Ok(Setting::Inline(_)) => {}
                Ok(Setting::Named(settings)) => issues.push(LoadIssue::new(
                    path,
                    format!("dimension {id} uses noise settings {settings}, which the server cannot generate"),
                )),
                Err(reason) => issues.push(LoadIssue::new(path, format!("dimension {id}: {reason}"))),
            }
        }
        aliases
    }

    fn templates(
        &self,
        aliases: &BTreeMap<String, String>,
        issues: &mut Vec<LoadIssue>,
    ) -> Vec<(String, Dimension, DimensionSpawns)> {
        let mut templates = Vec::new();
        for (id, settings) in aliases {
            let Some((value, path)) = self.dimensions.get(id) else {
                continue;
            };
            let sea_level = self
                .settings
                .get(settings)
                .and_then(|(value, _)| settings_entry(value).ok())
                .and_then(|entry| entry["sea_level"].as_i64())
                .unwrap_or(63) as i32;
            let kind = value.get("type").unwrap_or(&Value::Null);
            let made = match kind {
                Value::Object(_) => dimension_template(kind, sea_level).map(Some),
                Value::String(name) if name == "minecraft:overworld_caves" => {
                    let mut caves = Dimension::overworld();
                    caves.dimension_type = Identifier::minecraft("overworld_caves");
                    caves.has_ceiling = true;
                    Ok(Some(caves))
                }
                Value::String(name) if VANILLA_TYPES.contains(&name.as_str()) => Ok(None),
                Value::String(name) => match self.dimension_types.get(name) {
                    Some((found, _)) => dimension_template(found, sea_level).map(|mut found| {
                        if let Ok(parsed) = Identifier::parse(name) {
                            found.dimension_type = parsed;
                        }
                        Some(found)
                    }),
                    None => Err(format!("unknown dimension type {name}")),
                },
                _ => Err("missing dimension type".to_owned()),
            };
            match made {
                Ok(Some(mut template)) => {
                    if kind.is_object()
                        && let Ok(parsed) = Identifier::parse(id)
                    {
                        template.dimension_type = parsed;
                    }
                    let source = match kind {
                        Value::Object(_) => Some(kind),
                        Value::String(name) => {
                            self.dimension_types.get(name).map(|(found, _)| found)
                        }
                        _ => None,
                    };
                    let spawns = dimension_spawns(source, template.has_ceiling);
                    templates.push((id.clone(), template, spawns));
                }
                Ok(None) => {}
                Err(reason) => {
                    issues.push(LoadIssue::new(path, format!("dimension {id}: {reason}")))
                }
            }
        }
        templates
    }

    fn into_installed(self, aliases: BTreeMap<String, String>) -> Installed {
        let mut installed = Installed {
            aliases,
            settings: self.settings.keys().cloned().collect(),
            ..Installed::default()
        };
        self.fill(&mut installed);
        installed
    }
}

fn number(value: &Value, key: &str) -> Result<f64, String> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| format!("missing number {key}"))
}

pub(crate) fn noise_entry(id: &str, value: &Value) -> Result<Value, String> {
    if value
        .get("normalize")
        .and_then(Value::as_str)
        .unwrap_or("enabled")
        != "enabled"
    {
        return Err(format!("noise {id} uses an unsupported normalization"));
    }
    let modifiers = value
        .get("amplitude_modifiers")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_f64).collect::<Vec<_>>())
        .unwrap_or_default();
    Ok(json!({
        "id": id,
        "base_octave": number(value, "base_octave").map_err(|e| format!("noise {id}: {e}"))? as i64,
        "octave_count": value.get("octave_count").and_then(Value::as_i64).unwrap_or(1),
        "base_amplitude": value.get("base_amplitude").and_then(Value::as_f64).unwrap_or(1.0),
        "amplitude_modifiers": modifiers,
    }))
}

fn biome_entry(value: &Value) -> Result<Value, String> {
    let temperature = number(value, "temperature")?;
    let features = value
        .get("features")
        .filter(|features| features.is_array())
        .ok_or("missing features")?;
    Ok(json!({
        "temperature": temperature,
        "temperature_modifier": value.get("temperature_modifier").and_then(Value::as_str).unwrap_or("none"),
        "has_precipitation": value.get("has_precipitation").and_then(Value::as_bool).unwrap_or(true),
        "features": features,
        "carvers": value.get("carvers").cloned().unwrap_or_else(|| json!([])),
    }))
}

fn settings_entry(value: &Value) -> Result<Value, String> {
    let noise = value.get("noise").ok_or("missing noise")?;
    for key in ["noise_router", "aquifers"] {
        if value.get(key).is_none() {
            return Err(format!("missing {key}"));
        }
    }
    let mut entry = json!({
        "router": value["noise_router"],
        "aquifers": value["aquifers"],
        "sea_level": number(value, "sea_level")? as i64,
        "min_y": number(noise, "min_y")? as i64,
        "height": number(noise, "height")? as i64,
    });
    if let Some(rule) = value.get("surface_rule") {
        entry["surface_rule"] = rule.clone();
    }
    Ok(entry)
}

enum Setting<'a> {
    Named(&'a str),
    Inline(&'a Value),
}

fn inline_id(dimension: &str) -> String {
    format!("inline:{dimension}")
}

fn dimension_settings(value: &Value) -> Result<Setting<'_>, String> {
    let generator = value.get("generator").ok_or("missing generator")?;
    let kind = generator.get("type").and_then(Value::as_str).unwrap_or("");
    if kind != "minecraft:noise" {
        return Err(format!(
            "generator type {kind:?} is not supported, only minecraft:noise"
        ));
    }
    let source = generator
        .get("biome_source")
        .ok_or("missing biome_source")?;
    let source_kind = source.get("type").and_then(Value::as_str).unwrap_or("");
    let preset = source.get("preset").and_then(Value::as_str).unwrap_or("");
    if source_kind != "minecraft:multi_noise" || preset != "minecraft:overworld" {
        return Err(
            "only the multi_noise biome source with the minecraft:overworld preset is supported"
                .to_owned(),
        );
    }
    match generator.get("settings") {
        Some(Value::String(name)) => Ok(Setting::Named(name)),
        Some(inline @ Value::Object(_)) => Ok(Setting::Inline(inline)),
        _ => Err("missing noise settings".to_owned()),
    }
}

fn dimension_spawns(source: Option<&Value>, has_ceiling: bool) -> DimensionSpawns {
    let light = source.and_then(|value| value.get("monster_spawn_light_level"));
    let count = |value: &Value, key: &str| value.get(key).and_then(Value::as_i64).map(|n| n as i32);
    let monster_spawn_light = match light {
        Some(Value::Number(number)) => SpawnCount::Constant(number.as_i64().unwrap_or(0) as i32),
        Some(object @ Value::Object(_)) => match (
            count(object, "min_inclusive"),
            count(object, "max_inclusive"),
            count(object, "value"),
        ) {
            (Some(min), Some(max), _) => SpawnCount::Uniform { min, max },
            (_, _, Some(value)) => SpawnCount::Constant(value),
            _ => SpawnCount::Uniform { min: 0, max: 7 },
        },
        _ => SpawnCount::Uniform { min: 0, max: 7 },
    };
    DimensionSpawns {
        monster_spawn_light,
        monster_spawn_block_light_limit: source
            .and_then(|value| count(value, "monster_spawn_block_light_limit"))
            .unwrap_or(0),
        ambient_light: source
            .and_then(|value| value.get("ambient_light"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0) as f32,
        has_ceiling,
    }
}

fn flag(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn dimension_template(value: &Value, sea_level: i32) -> Result<Dimension, String> {
    let int = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_i64)
            .and_then(|number| i32::try_from(number).ok())
            .ok_or_else(|| format!("missing integer {key}"))
    };
    let (min_y, height) = (int("min_y")?, int("height")?);
    let height = u32::try_from(height).map_err(|_| format!("height {height} is negative"))?;
    let world_height = WorldHeight::new(min_y, height).map_err(|error| error.to_string())?;
    let piglins_zombify = value
        .get("attributes")
        .is_none_or(|attributes| flag(attributes, "minecraft:gameplay/piglins_zombify", true));
    let mut dimension = Dimension::overworld();
    dimension.height = world_height;
    dimension.has_skylight = flag(value, "has_skylight", false);
    dimension.has_ceiling = flag(value, "has_ceiling", false);
    dimension.has_fixed_time = flag(value, "has_fixed_time", false);
    dimension.piglins_zombify = piglins_zombify;
    dimension.sea_level = sea_level;
    Ok(dimension)
}

impl Overlay {
    pub(crate) fn load(roots: &[PathBuf], issues: &mut Vec<LoadIssue>) -> Self {
        let mut overlay = Self::read(roots, issues);
        overlay.lift_inline_settings();
        overlay.validate_dimension_types(issues);
        overlay.validate_noises(issues);
        overlay.validate_carvers(issues);
        overlay.validate_biomes(issues);
        overlay.validate_functions(issues);
        overlay.validate_settings(issues);
        overlay.validate_carver_lists(issues);
        overlay.report_unsupported_features(issues);
        overlay
    }
}

pub(crate) fn install(roots: &[PathBuf]) -> Vec<LoadIssue> {
    let mut issues = Vec::new();
    if datapack::loaded() {
        issues.push(LoadIssue::new(
            Path::new("datapacks"),
            "worldgen data was already in use, datapack worldgen overrides were not applied",
        ));
        return issues;
    }
    let overlay = Overlay::load(roots, &mut issues);
    let aliases = overlay.aliases(&mut issues);
    for (id, template, spawns) in overlay.templates(&aliases, &mut issues) {
        if let Ok(parsed) = Identifier::parse(&id) {
            register_dimension_spawns(&template.dimension_type, spawns);
            register_dimension_template(&parsed, template);
        }
    }
    if INSTALLED.set(overlay.into_installed(aliases)).is_err() {
        issues.push(LoadIssue::new(
            Path::new("datapacks"),
            "worldgen datapack data is already installed",
        ));
    }
    issues
}

pub(crate) fn install_from(dir: &Path) -> Vec<LoadIssue> {
    let roots = install_roots(dir);
    if roots.is_empty() {
        return Vec::new();
    }
    install(&roots)
}

#[cfg(test)]
mod tests;
