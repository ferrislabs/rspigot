use std::path::{Path, PathBuf};

use rspigot_core::pack_data::SUPPORTED_REGISTRIES;
use rspigot_core::ports::{
    DatapackError, DatapackSource, PackContents, PackFile, PackMeta, PackTag,
};
use rspigot_domain::Identifier;

const MCMETA: &str = "pack.mcmeta";
const FUNCTION_DIR: &str = "function";
const TAG_DIR: &str = "tags/function";
const PREDICATE_DIR: &str = "predicate";
const LOOT_TABLE_DIR: &str = "loot_table";

#[derive(Debug, Clone)]
pub struct DirectoryDatapacks {
    root: PathBuf,
}

impl DirectoryDatapacks {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

impl DatapackSource for DirectoryDatapacks {
    fn read_packs(&self) -> Result<Vec<PackContents>, DatapackError> {
        let entries = match std::fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let mut folders: Vec<(String, PathBuf)> = Vec::new();
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    tracing::warn!(
                        "Skipping an unreadable entry in {}: {error}",
                        self.root.display()
                    );
                    continue;
                }
            };
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !path.is_dir() {
                continue;
            }
            folders.push((name, path));
        }
        folders.sort_by(|a, b| a.0.cmp(&b.0));
        let mut packs = Vec::new();
        for (name, path) in folders {
            if let Some(pack) = read_pack(name, &path) {
                packs.push(pack);
            }
        }
        Ok(packs)
    }
}

fn read_pack(name: String, folder: &Path) -> Option<PackContents> {
    let meta_path = folder.join(MCMETA);
    if !meta_path.is_file() {
        tracing::warn!("Datapack folder {name} has no {MCMETA}, skipping it");
        return None;
    }
    let meta = match std::fs::read_to_string(&meta_path)
        .map_err(|error| error.to_string())
        .and_then(|text| parse_meta(&text))
    {
        Ok(meta) => meta,
        Err(error) => {
            tracing::warn!("Could not read {}: {error}", meta_path.display());
            return None;
        }
    };
    let mut functions = Vec::new();
    let mut function_tags = Vec::new();
    let mut predicates = Vec::new();
    let mut loot_tables = Vec::new();
    let mut tags = Vec::new();
    let data = folder.join("data");
    for namespace in sorted_entries(&data) {
        if !namespace.is_dir() {
            continue;
        }
        let Some(ns) = namespace.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        collect(
            &namespace.join(FUNCTION_DIR),
            ns,
            "mcfunction",
            &mut functions,
        );
        collect(&namespace.join(TAG_DIR), ns, "json", &mut function_tags);
        collect(&namespace.join(PREDICATE_DIR), ns, "json", &mut predicates);
        collect(
            &namespace.join(LOOT_TABLE_DIR),
            ns,
            "json",
            &mut loot_tables,
        );
        for registry in SUPPORTED_REGISTRIES {
            let mut files = Vec::new();
            collect(
                &namespace.join("tags").join(registry),
                ns,
                "json",
                &mut files,
            );
            tags.extend(files.into_iter().map(|file| PackTag {
                registry: (*registry).to_owned(),
                file,
            }));
        }
    }
    Some(PackContents {
        name,
        meta,
        functions,
        function_tags,
        predicates,
        loot_tables,
        tags,
    })
}

fn parse_meta(text: &str) -> Result<PackMeta, String> {
    let root: serde_json::Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let pack = root
        .get("pack")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "no pack section".to_owned())?;
    let number = |value: Option<&serde_json::Value>| match value {
        Some(serde_json::Value::Number(number)) => number.as_i64().map(|n| n as i32),
        Some(serde_json::Value::Array(items)) => items
            .first()
            .and_then(serde_json::Value::as_i64)
            .map(|n| n as i32),
        _ => None,
    };
    let description = match pack.get("description") {
        Some(serde_json::Value::String(text)) => text.clone(),
        Some(serde_json::Value::Object(object)) => object
            .get("text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        Some(serde_json::Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| match part {
                serde_json::Value::String(text) => Some(text.as_str()),
                serde_json::Value::Object(object) => {
                    object.get("text").and_then(serde_json::Value::as_str)
                }
                _ => None,
            })
            .collect(),
        _ => String::new(),
    };
    Ok(PackMeta {
        description,
        pack_format: number(pack.get("pack_format")),
        min_format: number(pack.get("min_format")),
    })
}

fn sorted_entries(folder: &Path) -> Vec<PathBuf> {
    let entries = match std::fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => {
            tracing::warn!("Could not list {}: {error}", folder.display());
            return Vec::new();
        }
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    paths.sort();
    paths
}

fn collect(folder: &Path, namespace: &str, extension: &str, out: &mut Vec<PackFile>) {
    walk(folder, folder, namespace, extension, out);
}

fn walk(base: &Path, folder: &Path, namespace: &str, extension: &str, out: &mut Vec<PackFile>) {
    for path in sorted_entries(folder) {
        if path.is_dir() {
            walk(base, &path, namespace, extension, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some(extension) {
            continue;
        }
        let Ok(relative) = path.strip_prefix(base) else {
            continue;
        };
        let stem = relative.with_extension("");
        let name = stem
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let Ok(id) = Identifier::parse(&format!("{namespace}:{name}")) else {
            tracing::warn!("Skipping {}: not a valid resource name", path.display());
            continue;
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => out.push(PackFile { id, text }),
            Err(error) => tracing::warn!("Could not read {}: {error}", path.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use rspigot_core::functions::FunctionLibrary;

    use super::*;
    use crate::testutil::TempDir;

    fn write(root: &Path, relative: &str, contents: &[u8]) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn meta(root: &Path, pack: &str) {
        write(
            root,
            &format!("{pack}/pack.mcmeta"),
            br#"{"pack":{"description":"Pack","min_format":[94,1],"max_format":[95,0]}}"#,
        );
    }

    #[derive(Default, Clone)]
    struct Warnings(Arc<Mutex<Vec<String>>>);

    impl tracing::Subscriber for Warnings {
        fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
            *metadata.level() <= tracing::Level::WARN
        }
        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }
        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
        fn event(&self, event: &tracing::Event<'_>) {
            struct Message(String);
            impl tracing::field::Visit for Message {
                fn record_debug(
                    &mut self,
                    field: &tracing::field::Field,
                    value: &dyn std::fmt::Debug,
                ) {
                    if field.name() == "message" {
                        self.0 = format!("{value:?}");
                    }
                }
            }
            let mut message = Message(String::new());
            event.record(&mut message);
            self.0.lock().unwrap().push(message.0);
        }
        fn enter(&self, _: &tracing::span::Id) {}
        fn exit(&self, _: &tracing::span::Id) {}
    }

    #[test]
    fn a_missing_folder_has_no_packs() {
        let dir = TempDir::new("packs-none");
        let source = DirectoryDatapacks::new(dir.path().join("datapacks"));
        assert!(source.read_packs().unwrap().is_empty());
    }

    #[test]
    fn packs_come_alphabetically_and_a_later_pack_overrides_a_function() {
        let dir = TempDir::new("packs-two");
        let root = dir.path();
        meta(root, "b-second");
        meta(root, "a-first");
        write(
            root,
            "a-first/data/t/function/shared.mcfunction",
            b"say first",
        );
        write(root, "a-first/data/t/function/only_a.mcfunction", b"say a");
        write(
            root,
            "a-first/data/t/function/deep/nested.mcfunction",
            b"say nested",
        );
        write(
            root,
            "a-first/data/t/tags/function/group.json",
            br#"{"values":["t:shared"]}"#,
        );
        write(
            root,
            "b-second/data/t/function/shared.mcfunction",
            b"say second",
        );
        write(
            root,
            "b-second/data/minecraft/tags/function/load.json",
            br#"{"values":["t:shared"]}"#,
        );
        write(root, "notes.txt", b"ignored");
        let packs = DirectoryDatapacks::new(root).read_packs().unwrap();
        let names: Vec<&str> = packs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["a-first", "b-second"]);
        assert_eq!(packs[0].meta.description, "Pack");
        assert_eq!(packs[0].meta.min_format, Some(94));
        let ids: Vec<String> = packs[0]
            .functions
            .iter()
            .map(|f| f.id.to_string())
            .collect();
        assert_eq!(ids, ["t:deep/nested", "t:only_a", "t:shared"]);
        assert_eq!(packs[0].function_tags[0].id.to_string(), "t:group");
        let library = FunctionLibrary::build(&packs);
        let shared = library
            .function(&Identifier::parse("t:shared").unwrap())
            .unwrap();
        assert_eq!(shared.commands(None).unwrap(), ["say second"]);
        assert!(
            library
                .function(&Identifier::parse("t:only_a").unwrap())
                .is_some()
        );
        assert_eq!(
            library
                .tag(&Identifier::parse("minecraft:load").unwrap())
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn an_unreadable_file_logs_a_warning_and_loading_goes_on() {
        let dir = TempDir::new("packs-bad");
        let root = dir.path();
        meta(root, "a");
        write(root, "a/data/t/function/good.mcfunction", b"say ok");
        write(
            root,
            "a/data/t/function/binary.mcfunction",
            &[0xff, 0xfe, 0x00],
        );
        write(root, "a/data/t/function/Upper.mcfunction", b"say bad name");
        write(root, "broken/pack.mcmeta", b"{not json");
        write(root, "bare/data/t/function/x.mcfunction", b"say x");
        let warnings = Warnings::default();
        let packs = tracing::subscriber::with_default(warnings.clone(), || {
            DirectoryDatapacks::new(root).read_packs().unwrap()
        });
        assert_eq!(packs.len(), 1);
        let ids: Vec<String> = packs[0]
            .functions
            .iter()
            .map(|f| f.id.to_string())
            .collect();
        assert_eq!(ids, ["t:good"]);
        let logged = warnings.0.lock().unwrap().join("\n");
        assert!(logged.contains("binary.mcfunction"), "{logged}");
        assert!(logged.contains("Upper.mcfunction"), "{logged}");
        assert!(logged.contains("broken"), "{logged}");
        assert!(logged.contains("bare"), "{logged}");
    }

    #[test]
    fn predicates_loot_tables_and_registry_tags_are_read_with_their_registry() {
        let dir = TempDir::new("packs-data");
        let root = dir.path();
        meta(root, "a");
        write(root, "a/data/t/predicate/deep/p.json", b"{}");
        write(root, "a/data/t/loot_table/blocks/stone.json", b"{}");
        write(root, "a/data/t/tags/block/group.json", b"{}");
        write(root, "a/data/t/tags/worldgen/biome/warm.json", b"{}");
        write(root, "a/data/t/tags/nonsense/other.json", b"{}");
        let packs = DirectoryDatapacks::new(root).read_packs().unwrap();
        let pack = &packs[0];
        assert_eq!(pack.predicates[0].id.to_string(), "t:deep/p");
        assert_eq!(pack.loot_tables[0].id.to_string(), "t:blocks/stone");
        let tags: Vec<(&str, String)> = pack
            .tags
            .iter()
            .map(|tag| (tag.registry.as_str(), tag.file.id.to_string()))
            .collect();
        assert_eq!(
            tags,
            [
                ("block", "t:group".to_owned()),
                ("worldgen/biome", "t:warm".to_owned())
            ]
        );
    }
}
