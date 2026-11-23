use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{
    DataError, advancement_files, functions, json, modifier, predicate_file, slots, table,
};
use crate::ports::PackFile;

fn root() -> Option<PathBuf> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../re/server/data/minecraft");
    path.is_dir().then_some(path)
}

fn json_files(dir: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            json_files(&path, into);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            into.push(path);
        }
    }
}

fn walk(
    root: &Path,
    folder: &str,
    check: impl Fn(&str, &str) -> Result<(), String>,
) -> Vec<String> {
    let base = root.join(folder);
    let mut files = Vec::new();
    json_files(&base, &mut files);
    assert!(
        !files.is_empty() || folder == "item_modifier",
        "no {folder} files found"
    );
    let mut failures = Vec::new();
    for path in files {
        let relative = path.strip_prefix(&base).unwrap().with_extension("");
        let id = format!("minecraft:{}", relative.to_string_lossy());
        let text = fs::read_to_string(&path).unwrap();
        if let Err(reason) = check(&id, &text) {
            failures.push(format!("{folder}/{id}: {reason}"));
        }
    }
    failures
}

fn report(failures: &[String]) {
    assert!(
        failures.is_empty(),
        "{} files do not parse:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_vanilla_loot_table_parses_without_losing_a_part() {
    let Some(root) = root() else { return };
    let failures = walk(&root, "loot_table", |_, text| {
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let outcome = table::parse_named(&value, "minecraft:test").map_err(|e| e.to_string())?;
        if outcome.skipped.is_empty() {
            Ok(())
        } else {
            Err(outcome.skipped.join("; "))
        }
    });
    report(&failures);
}

#[test]
fn every_vanilla_predicate_parses() {
    let Some(root) = root() else { return };
    let failures = walk(&root, "predicate", |_, text| {
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        predicate_file(&value)
            .map(|_| ())
            .map_err(|e| e.to_string())
    });
    report(&failures);
}

#[test]
fn every_vanilla_item_modifier_parses() {
    let Some(root) = root() else { return };
    let failures = walk(&root, "item_modifier", |_, text| {
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        modifier::parse_functions(&value)
            .map(|_| ())
            .map_err(|e| e.to_string())
    });
    report(&failures);
}

#[test]
fn every_vanilla_advancement_parses() {
    let Some(root) = root() else { return };
    let failures = walk(&root, "advancement", |id, text| {
        let file = PackFile {
            id: rspigot_domain::Identifier::parse(id).unwrap(),
            text: text.to_owned(),
        };
        advancement_files::parse(&file, 0.0, 0.0)
            .map(|_| ())
            .map_err(|e| e.to_string())
    });
    report(&failures);
}

fn read(root: &Path, folder: &str, id: &str) -> Option<Value> {
    let path = root
        .join(folder)
        .join(format!("{}.json", id.strip_prefix("minecraft:")?));
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

#[test]
fn the_parsed_vanilla_tables_equal_the_generated_ones() {
    let Some(root) = root() else { return };
    let mut checked = 0;
    let mut differing = Vec::new();
    for (name, generated) in rspigot_data::loot::tables() {
        let Some(value) = read(&root, "loot_table", name) else {
            continue;
        };
        checked += 1;
        match table::parse_named(&value, name) {
            Ok(outcome) if outcome.table == *generated => {}
            Ok(_) => differing.push((*name).to_owned()),
            Err(error) => differing.push(format!("{name}: {error}")),
        }
    }
    assert!(checked > 500, "only {checked} tables compared");
    assert!(
        differing.is_empty(),
        "{} tables differ from the generated data:\n{}",
        differing.len(),
        differing.join("\n")
    );
}

#[test]
fn the_parsed_vanilla_predicates_equal_the_generated_ones() {
    let Some(root) = root() else { return };
    let mut checked = 0;
    let mut differing = Vec::new();
    for (name, generated) in rspigot_data::loot::predicates() {
        let Some(value) = read(&root, "predicate", name) else {
            continue;
        };
        checked += 1;
        match predicate_file(&value) {
            Ok(parsed) if parsed == *generated => {}
            Ok(_) => differing.push((*name).to_owned()),
            Err(error) => differing.push(format!("{name}: {error}")),
        }
    }
    assert!(checked >= 3, "only {checked} predicates compared");
    assert!(differing.is_empty(), "differ: {}", differing.join("\n"));
}

fn unrecognised(registry: &str, probe: impl Fn(&str) -> Option<String>) -> Vec<String> {
    let names = rspigot_data::registries::static_entries(registry)
        .unwrap_or_else(|| panic!("no static registry {registry}"));
    assert!(!names.is_empty(), "{registry} is empty");
    names
        .iter()
        .filter_map(|name| probe(name).map(|reason| format!("{registry} {name}: {reason}")))
        .collect()
}

fn flagged<T>(result: Result<T, DataError>) -> Option<String> {
    match result {
        Err(DataError::Unsupported(reason)) => Some(reason),
        _ => None,
    }
}

#[test]
fn every_registered_loot_type_is_recognised_by_the_parsers() {
    let mut missing = Vec::new();
    missing.extend(unrecognised("minecraft:loot_function_type", |name| {
        flagged(functions::function_list(
            &serde_json::json!({ "type": name }),
        ))
    }));
    missing.extend(unrecognised("minecraft:loot_condition_type", |name| {
        flagged(json::condition(&serde_json::json!({ "condition": name })))
    }));
    missing.extend(unrecognised("minecraft:loot_pool_entry_type", |name| {
        let table = serde_json::json!({"pools": [{"rolls": 1, "entries": [{"type": name}]}]});
        table::parse_named(&table, "minecraft:probe")
            .ok()
            .and_then(|outcome| {
                outcome
                    .skipped
                    .into_iter()
                    .find(|reason| reason.contains("unsupported"))
            })
    }));
    missing.extend(unrecognised(
        "minecraft:context_int_provider_type",
        |name| flagged(json::int_expr(&serde_json::json!({ "type": name }))),
    ));
    missing.extend(unrecognised(
        "minecraft:context_float_provider_type",
        |name| flagged(json::float_expr(&serde_json::json!({ "type": name }))),
    ));
    missing.extend(unrecognised("minecraft:slot_source_type", |name| {
        flagged(slots::slot_source(&serde_json::json!({ "type": name })))
    }));
    missing.extend(unrecognised("minecraft:loot_score_provider_type", |name| {
        let provider = serde_json::json!({
            "type": "minecraft:score", "target": {"type": name}, "score": "n"
        });
        flagged(json::int_expr(&provider))
    }));
    missing.extend(unrecognised("minecraft:loot_nbt_provider_type", |name| {
        let function = serde_json::json!({
            "type": "minecraft:copy_custom_data", "source": {"type": name}, "ops": []
        });
        flagged(functions::function_list(&function))
    }));
    assert!(
        missing.is_empty(),
        "{} registered types fall into an unsupported path:\n{}",
        missing.len(),
        missing.join("\n")
    );
}

#[test]
fn unknown_loot_types_are_still_reported_as_unsupported() {
    let probe = serde_json::json!({ "type": "minecraft:nope" });
    assert!(flagged(functions::function_list(&probe)).is_some());
    assert!(
        flagged(json::condition(
            &serde_json::json!({"condition": "minecraft:nope"})
        ))
        .is_some()
    );
    assert!(flagged(json::int_expr(&probe)).is_some());
    assert!(flagged(json::float_expr(&probe)).is_some());
    assert!(flagged(slots::slot_source(&probe)).is_some());
}

#[test]
fn every_vanilla_context_provider_parses() {
    let Some(root) = root() else { return };
    let mut failures = walk(&root, "context_int_provider", |_, text| {
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        json::int_expr(&value)
            .map(|_| ())
            .map_err(|e| e.to_string())
    });
    failures.extend(walk(&root, "context_float_provider", |_, text| {
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        json::float_expr(&value)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }));
    report(&failures);
}

#[test]
fn no_vanilla_loot_data_names_a_type_outside_the_registries() {
    let Some(root) = root() else { return };
    let known: std::collections::BTreeSet<&str> = [
        "minecraft:loot_function_type",
        "minecraft:loot_condition_type",
        "minecraft:loot_pool_entry_type",
        "minecraft:context_int_provider_type",
        "minecraft:context_float_provider_type",
        "minecraft:slot_source_type",
        "minecraft:loot_score_provider_type",
        "minecraft:loot_nbt_provider_type",
    ]
    .iter()
    .flat_map(|registry| rspigot_data::registries::static_entries(registry).unwrap_or(&[]))
    .copied()
    .collect();
    let mut stray = Vec::new();
    for folder in ["loot_table", "predicate", "item_modifier"] {
        let mut files = Vec::new();
        json_files(&root.join(folder), &mut files);
        for path in files {
            let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
            typed_names(&value, &known, &mut stray);
        }
    }
    stray.sort();
    stray.dedup();
    assert!(
        stray.is_empty(),
        "not in a loot registry: {}",
        stray.join(", ")
    );
}

fn typed_names(value: &Value, known: &std::collections::BTreeSet<&str>, stray: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for key in ["function", "condition"] {
                if let Some(Value::String(name)) = map.get(key)
                    && name.starts_with("minecraft:")
                    && !name.contains('/')
                    && !known.contains(name.as_str())
                    && name != "minecraft:reference"
                    && name != "minecraft:block_state_property"
                {
                    stray.push(name.clone());
                }
            }
            map.values()
                .for_each(|inner| typed_names(inner, known, stray));
        }
        Value::Array(items) => items
            .iter()
            .for_each(|inner| typed_names(inner, known, stray)),
        _ => {}
    }
}
