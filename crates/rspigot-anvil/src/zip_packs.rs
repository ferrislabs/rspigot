use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use rspigot_core::pack_data::SUPPORTED_REGISTRIES;
use rspigot_core::pack_data::registry_overlay::{RUNTIME_DIRECTORIES, SYNCED_DIRECTORIES};
use rspigot_core::ports::{
    PackContents, PackExtras, PackFile, PackMeta, PackRegistryFile, PackTag,
};
use rspigot_domain::Identifier;

pub const MAX_ENTRIES: usize = 100_000;
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

const MCMETA: &str = "pack.mcmeta";

pub struct VirtualPack {
    pub name: String,
    files: BTreeMap<String, String>,
}

fn safe_path(name: &str) -> Option<&str> {
    if name.is_empty() || name.starts_with('/') || name.contains('\\') || name.contains(':') {
        return None;
    }
    if name
        .split('/')
        .any(|part| part.is_empty() || part == ".." || part == ".")
    {
        return None;
    }
    Some(name)
}

fn wanted(path: &str) -> bool {
    path == MCMETA
        || (path.starts_with("data/") && (path.ends_with(".json") || path.ends_with(".mcfunction")))
}

pub fn read_zip(name: String, path: &Path) -> Option<VirtualPack> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) => {
            tracing::warn!("Could not open {}: {error}", path.display());
            return None;
        }
    };
    let mut archive = match zip::ZipArchive::new(file) {
        Ok(archive) => archive,
        Err(error) => {
            tracing::warn!("Could not read the zip {}: {error}", path.display());
            return None;
        }
    };
    if archive.len() > MAX_ENTRIES {
        tracing::warn!(
            "Skipping {}: more than {MAX_ENTRIES} entries",
            path.display()
        );
        return None;
    }
    let mut files = BTreeMap::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(index) else {
            continue;
        };
        if entry.is_dir() {
            continue;
        }
        let entry_name = entry.name().to_owned();
        let Some(clean) = safe_path(&entry_name) else {
            tracing::warn!(
                "Skipping the unsafe entry {entry_name} in {}",
                path.display()
            );
            continue;
        };
        if !wanted(clean) {
            continue;
        }
        if entry.size() > MAX_FILE_BYTES {
            tracing::warn!("Skipping {entry_name} in {}: too large", path.display());
            continue;
        }
        let mut bytes = Vec::new();
        if entry
            .by_ref()
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
            || bytes.len() as u64 > MAX_FILE_BYTES
        {
            tracing::warn!(
                "Skipping {entry_name} in {}: unreadable or too large",
                path.display()
            );
            continue;
        }
        total += bytes.len() as u64;
        if total > MAX_TOTAL_BYTES {
            tracing::warn!(
                "Skipping {}: more than {MAX_TOTAL_BYTES} bytes of data",
                path.display()
            );
            return None;
        }
        match String::from_utf8(bytes) {
            Ok(text) => {
                files.insert(clean.to_owned(), text);
            }
            Err(_) => tracing::warn!("Skipping {entry_name} in {}: not UTF-8", path.display()),
        }
    }
    files
        .contains_key(MCMETA)
        .then_some(VirtualPack { name, files })
}

impl VirtualPack {
    fn under<'a>(
        &'a self,
        directory: &'a str,
        extension: &'a str,
    ) -> impl Iterator<Item = PackFile> + 'a {
        self.files.iter().filter_map(move |(path, text)| {
            let rest = path.strip_prefix("data/")?;
            let (namespace, inner) = rest.split_once('/')?;
            let inner = inner.strip_prefix(directory)?.strip_prefix('/')?;
            let stem = inner.strip_suffix(extension)?;
            let id = Identifier::parse(&format!("{namespace}:{stem}")).ok()?;
            Some(PackFile {
                id,
                text: text.clone(),
            })
        })
    }

    pub fn contents(
        &self,
        parse_meta: impl Fn(&str) -> Result<PackMeta, String>,
    ) -> Option<PackContents> {
        let meta = match parse_meta(self.files.get(MCMETA)?) {
            Ok(meta) => meta,
            Err(error) => {
                tracing::warn!("Could not read {MCMETA} of {}: {error}", self.name);
                return None;
            }
        };
        let mut tags = Vec::new();
        for registry in SUPPORTED_REGISTRIES {
            tags.extend(
                self.under(&format!("tags/{registry}"), ".json")
                    .map(|file| PackTag {
                        registry: (*registry).to_owned(),
                        file,
                    }),
            );
        }
        Some(PackContents {
            name: self.name.clone(),
            meta,
            functions: self.under("function", ".mcfunction").collect(),
            function_tags: self.under("tags/function", ".json").collect(),
            predicates: self.under("predicate", ".json").collect(),
            loot_tables: self.under("loot_table", ".json").collect(),
            item_modifiers: self.under("item_modifier", ".json").collect(),
            tags,
        })
    }

    pub fn extras(&self) -> PackExtras {
        let mut registries = Vec::new();
        for registry in SYNCED_DIRECTORIES.iter().chain(RUNTIME_DIRECTORIES) {
            registries.extend(self.under(registry, ".json").map(|file| PackRegistryFile {
                registry: (*registry).to_owned(),
                file,
            }));
        }
        PackExtras {
            name: self.name.clone(),
            registries,
            advancements: self.under("advancement", ".json").collect(),
            recipes: self.under("recipe", ".json").collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use rspigot_core::ports::DatapackSource;

    use super::*;
    use crate::DirectoryDatapacks;
    use crate::testutil::TempDir;

    fn zip_of(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        for (name, bytes) in entries {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    const META: &[u8] = br#"{"pack":{"description":"Z","pack_format":94}}"#;

    #[test]
    fn a_zipped_pack_is_read_like_a_folder_for_contents_and_extras() {
        let dir = TempDir::new("zip");
        zip_of(
            &dir.path().join("zipped.zip"),
            &[
                ("pack.mcmeta", META),
                ("data/t/function/hi.mcfunction", b"say hi"),
                ("data/t/advancement/a/b.json", b"{}"),
                ("data/t/recipe/r.json", b"{}"),
                ("data/t/dialog/d.json", b"{}"),
                ("data/t/tags/function/load.json", b"{\"values\":[]}"),
                ("readme.txt", b"ignored"),
            ],
        );
        std::fs::create_dir_all(dir.path().join("a_folder/data")).unwrap();
        std::fs::write(dir.path().join("a_folder/pack.mcmeta"), META).unwrap();
        let source = DirectoryDatapacks::new(dir.path());
        let packs = source.read_packs().unwrap();
        let names: Vec<&str> = packs.iter().map(|pack| pack.name.as_str()).collect();
        assert_eq!(names, ["a_folder", "zipped.zip"]);
        let zipped = &packs[1];
        assert_eq!(zipped.meta.description, "Z");
        assert_eq!(zipped.functions[0].id.as_str(), "t:hi");
        assert_eq!(zipped.function_tags[0].id.as_str(), "t:load");
        let extras = source.read_extras().unwrap();
        let zipped = extras
            .iter()
            .find(|pack| pack.name == "zipped.zip")
            .unwrap();
        assert_eq!(zipped.advancements[0].id.as_str(), "t:a/b");
        assert_eq!(zipped.recipes[0].id.as_str(), "t:r");
        assert_eq!(zipped.registries[0].registry, "dialog");
    }

    #[test]
    fn unsafe_names_oversized_entries_and_pack_less_zips_are_skipped() {
        let dir = TempDir::new("zipbad");
        let huge = vec![b' '; usize::try_from(MAX_FILE_BYTES).unwrap() + 1];
        zip_of(
            &dir.path().join("bad.zip"),
            &[
                ("pack.mcmeta", META),
                ("data/t/function/ok.mcfunction", b"say ok"),
                ("data/../../evil/function/x.mcfunction", b"say evil"),
                ("/data/t/function/abs.mcfunction", b"say abs"),
                ("data/t/function/back\\slash.mcfunction", b"say bs"),
                ("data/t/function/huge.mcfunction", &huge),
            ],
        );
        zip_of(
            &dir.path().join("nometa.zip"),
            &[("data/t/function/a.mcfunction", b"x")],
        );
        std::fs::write(dir.path().join("broken.zip"), b"not a zip").unwrap();
        let packs = DirectoryDatapacks::new(dir.path()).read_packs().unwrap();
        assert_eq!(packs.len(), 1);
        let ids: Vec<&str> = packs[0].functions.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["t:ok"]);
    }
}
