use std::path::{Path, PathBuf};
use std::sync::Arc;

use crossbeam_channel::Receiver;
use rspigot_anvil::AnvilStore;
use rspigot_core::worlds::{GeneratorSpec, LayerSpec, WorldKind, WorldSpec, WorldsChange};
use rspigot_domain::Identifier;
use serde_json::{Value, json};

const FILE: &str = "created-worlds.json";

pub fn path(root: &Path) -> PathBuf {
    root.join(FILE)
}

fn kind_name(kind: WorldKind) -> &'static str {
    match kind {
        WorldKind::Overworld => "overworld",
        WorldKind::Nether => "nether",
        WorldKind::End => "end",
    }
}

fn generator_json(generator: &GeneratorSpec) -> Value {
    match generator {
        GeneratorSpec::Vanilla => json!({ "type": "vanilla" }),
        GeneratorSpec::Void => json!({ "type": "void" }),
        GeneratorSpec::Flat(layers) => json!({
            "type": "flat",
            "layers": layers
                .iter()
                .map(|layer| json!({ "block": layer.block, "height": layer.height }))
                .collect::<Vec<_>>(),
        }),
        GeneratorSpec::Datapack(id) => json!({ "type": "datapack", "id": id.as_str() }),
    }
}

pub fn encode(worlds: &[WorldSpec]) -> String {
    let entries: Vec<Value> = worlds
        .iter()
        .map(|spec| {
            json!({
                "name": spec.name,
                "kind": kind_name(spec.kind),
                "seed": spec.seed,
                "generator": generator_json(&spec.generator),
                "structures": spec.structures,
            })
        })
        .collect();
    serde_json::to_string_pretty(&entries).unwrap_or_else(|_| "[]".to_owned())
}

fn generator_of(value: &Value) -> Option<GeneratorSpec> {
    Some(match value.get("type")?.as_str()? {
        "vanilla" => GeneratorSpec::Vanilla,
        "void" => GeneratorSpec::Void,
        "flat" => GeneratorSpec::Flat(
            value
                .get("layers")?
                .as_array()?
                .iter()
                .map(|layer| {
                    Some(LayerSpec {
                        block: layer.get("block")?.as_str()?.to_owned(),
                        height: u32::try_from(layer.get("height")?.as_u64()?).ok()?,
                    })
                })
                .collect::<Option<Vec<_>>>()?,
        ),
        "datapack" => GeneratorSpec::Datapack(Identifier::parse(value.get("id")?.as_str()?).ok()?),
        _ => return None,
    })
}

fn spec_of(value: &Value) -> Option<WorldSpec> {
    let kind = match value.get("kind")?.as_str()? {
        "overworld" => WorldKind::Overworld,
        "nether" => WorldKind::Nether,
        "end" => WorldKind::End,
        _ => return None,
    };
    Some(WorldSpec {
        name: value.get("name")?.as_str()?.to_owned(),
        kind,
        seed: value.get("seed")?.as_i64()?,
        generator: generator_of(value.get("generator")?)?,
        structures: value.get("structures")?.as_bool()?,
    })
}

pub fn decode(text: &str) -> Result<Vec<WorldSpec>, String> {
    let value: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let entries = value.as_array().ok_or("expected a list of worlds")?;
    entries
        .iter()
        .map(|entry| spec_of(entry).ok_or_else(|| format!("unreadable world entry {entry}")))
        .collect()
}

pub fn load(path: &Path) -> Vec<WorldSpec> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            tracing::warn!(%error, "Could not read {}", path.display());
            return Vec::new();
        }
    };
    decode(&text).unwrap_or_else(|reason| {
        tracing::warn!(%reason, "Ignoring {}", path.display());
        Vec::new()
    })
}

fn write(path: &Path, worlds: &[WorldSpec]) -> std::io::Result<()> {
    let staging = path.with_extension("json.tmp");
    std::fs::write(&staging, encode(worlds))?;
    std::fs::rename(&staging, path)
}

pub fn spawn_writer(path: PathBuf, store: Arc<AnvilStore>, changes: Receiver<WorldsChange>) {
    std::thread::spawn(move || {
        for change in changes {
            if let Some(world) = &change.erased
                && let Err(error) = store.erase_dimension(world)
            {
                tracing::warn!(%error, %world, "Could not delete the files of a world");
            }
            if let Err(error) = write(&path, &change.worlds) {
                tracing::warn!(%error, "Could not save {}", path.display());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<WorldSpec> {
        vec![
            WorldSpec {
                name: "arena".to_owned(),
                kind: WorldKind::Overworld,
                seed: -5,
                generator: GeneratorSpec::Flat(vec![LayerSpec {
                    block: "minecraft:stone".to_owned(),
                    height: 3,
                }]),
                structures: false,
            },
            WorldSpec {
                name: "hell".to_owned(),
                kind: WorldKind::Nether,
                seed: 9,
                generator: GeneratorSpec::Datapack(
                    Identifier::parse("minecraft:amplified").unwrap(),
                ),
                structures: true,
            },
            WorldSpec {
                name: "empty".to_owned(),
                kind: WorldKind::End,
                seed: 0,
                generator: GeneratorSpec::Void,
                structures: false,
            },
            WorldSpec {
                name: "plain".to_owned(),
                kind: WorldKind::Overworld,
                seed: 1,
                generator: GeneratorSpec::Vanilla,
                structures: true,
            },
        ]
    }

    #[test]
    fn the_list_survives_a_round_trip() {
        assert_eq!(decode(&encode(&sample())).unwrap(), sample());
    }

    #[test]
    fn a_broken_file_is_refused() {
        assert!(decode("{}").is_err());
        assert!(decode(r#"[{"name":"x"}]"#).is_err());
    }

    #[test]
    fn a_missing_file_means_no_worlds() {
        assert!(load(Path::new("/definitely/not/here.json")).is_empty());
    }
}
