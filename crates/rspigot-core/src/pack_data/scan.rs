use std::path::{Path, PathBuf};

use rspigot_domain::Identifier;

use super::registry_overlay::{RUNTIME_DIRECTORIES, SYNCED_DIRECTORIES};
use crate::ports::{DatapackError, PackExtras, PackFile, PackRegistryFile};

const ADVANCEMENT_DIR: &str = "advancement";
const RECIPE_DIR: &str = "recipe";

pub fn scan_root(root: &Path) -> Result<Vec<PackExtras>, DatapackError> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut folders: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.path().is_dir() && !entry.file_name().to_string_lossy().starts_with('.')
        })
        .filter(|entry| entry.path().join("pack.mcmeta").is_file())
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                entry.path(),
            )
        })
        .collect();
    folders.sort();
    Ok(folders
        .into_iter()
        .map(|(name, folder)| scan_pack(name, &folder))
        .collect())
}

fn sorted(folder: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(folder)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok().map(|e| e.path()))
                .collect()
        })
        .unwrap_or_default();
    paths.sort();
    paths
}

fn collect(base: &Path, folder: &Path, namespace: &str, out: &mut Vec<PackFile>) {
    for path in sorted(folder) {
        if path.is_dir() {
            collect(base, &path, namespace, out);
            continue;
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let Ok(relative) = path.strip_prefix(base) else {
            continue;
        };
        let name = relative
            .with_extension("")
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
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

fn scan_pack(name: String, folder: &Path) -> PackExtras {
    let mut extras = PackExtras {
        name,
        ..PackExtras::default()
    };
    for namespace in sorted(&folder.join("data")) {
        let Some(ns) = namespace
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|_| namespace.is_dir())
        else {
            continue;
        };
        let dir = |name: &str| namespace.join(name);
        collect(
            &dir(ADVANCEMENT_DIR),
            &dir(ADVANCEMENT_DIR),
            ns,
            &mut extras.advancements,
        );
        collect(&dir(RECIPE_DIR), &dir(RECIPE_DIR), ns, &mut extras.recipes);
        for registry in SYNCED_DIRECTORIES.iter().chain(RUNTIME_DIRECTORIES) {
            let mut files = Vec::new();
            collect(&dir(registry), &dir(registry), ns, &mut files);
            extras
                .registries
                .extend(files.into_iter().map(|file| PackRegistryFile {
                    registry: (*registry).to_owned(),
                    file,
                }));
        }
    }
    extras
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pack_folder_yields_its_advancements_recipes_and_registries() {
        let root = std::env::temp_dir().join(format!("rspigot-scan-{}", std::process::id()));
        let write = |relative: &str, text: &str| {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write("p/pack.mcmeta", "{}");
        write("p/data/t/advancement/a/b.json", "{}");
        write("p/data/t/recipe/r.json", "{}");
        write("p/data/t/dialog/d.json", "{}");
        write("p/data/t/trial_spawner/x/s.json", "{}");
        write("q/data/t/recipe/ignored.json", "{}");
        let found = scan_root(&root).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].advancements[0].id.as_str(), "t:a/b");
        assert_eq!(found[0].recipes[0].id.as_str(), "t:r");
        let registries: Vec<(&str, &str)> = found[0]
            .registries
            .iter()
            .map(|entry| (entry.registry.as_str(), entry.file.id.as_str()))
            .collect();
        assert_eq!(registries, [("dialog", "t:d"), ("trial_spawner", "t:x/s")]);
    }
}
