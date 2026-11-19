use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::settings::MapSettings;

const DEFAULT_LEVEL_NAME: &str = "world";
const LEVEL_NAME_KEY: &str = "level-name";
const PROPERTIES_FILE: &str = "server.properties";
const REGION_DIR: &str = "region";
const REGION_EXTENSION: &str = "mca";
const CREATED_WORLD_INFIX: &str = "_rspigot_";
const DIMENSIONS_DIR: &str = "dimensions";
const WORLD_NAMESPACE: &str = "rspigot";

#[derive(Debug, Error)]
pub enum InstallError {
    #[error("the map folder {0} does not exist")]
    MissingSource(PathBuf),
    #[error("could not install the map ({action} {path}): {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    pub region_dir: PathBuf,
    pub files: Vec<(PathBuf, PathBuf)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Installed {
    Copied(usize),
    AlreadyThere,
}

fn io_error(action: &'static str, path: &Path) -> impl FnOnce(io::Error) -> InstallError {
    let path = path.to_path_buf();
    move |source| InstallError::Io {
        action,
        path,
        source,
    }
}

fn is_region_file(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == REGION_EXTENSION)
}

pub fn parse_level_name(properties: &str) -> String {
    properties
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#') && !line.starts_with('!'))
        .filter_map(|line| line.split_once('='))
        .find(|(key, _)| key.trim() == LEVEL_NAME_KEY)
        .map(|(_, value)| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_LEVEL_NAME)
        .to_owned()
}

pub fn read_level_name(server_dir: &Path) -> Result<String, InstallError> {
    let path = server_dir.join(PROPERTIES_FILE);
    match fs::read_to_string(&path) {
        Ok(text) => Ok(parse_level_name(&text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(DEFAULT_LEVEL_NAME.to_owned()),
        Err(error) => Err(io_error("reading", &path)(error)),
    }
}

pub fn region_dir(map: &MapSettings, level_name: &str, world_name: &str) -> PathBuf {
    map.server_dir
        .join(format!("{level_name}{CREATED_WORLD_INFIX}{world_name}"))
        .join(DIMENSIONS_DIR)
        .join(WORLD_NAMESPACE)
        .join(world_name)
        .join(REGION_DIR)
}

pub fn has_regions(dir: &Path) -> bool {
    fs::read_dir(dir)
        .is_ok_and(|entries| entries.flatten().any(|entry| is_region_file(&entry.path())))
}

pub fn plan_install(
    map: &MapSettings,
    level_name: &str,
    world_name: &str,
) -> Result<InstallPlan, InstallError> {
    let source = map.source_dir.join(REGION_DIR);
    let entries = match fs::read_dir(&source) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(InstallError::MissingSource(map.source_dir.clone()));
        }
        Err(error) => return Err(io_error("listing", &source)(error)),
    };
    let target = region_dir(map, level_name, world_name);
    let mut names = Vec::new();
    for entry in entries {
        let path = entry.map_err(io_error("listing", &source))?.path();
        if is_region_file(&path)
            && let Some(name) = path.file_name()
        {
            names.push(name.to_owned());
        }
    }
    names.sort();
    let files = names
        .into_iter()
        .map(|name| (source.join(&name), target.join(&name)))
        .collect();
    Ok(InstallPlan {
        region_dir: target,
        files,
    })
}

pub fn install(plan: &InstallPlan) -> Result<Installed, InstallError> {
    if has_regions(&plan.region_dir) {
        return Ok(Installed::AlreadyThere);
    }
    fs::create_dir_all(&plan.region_dir).map_err(io_error("creating", &plan.region_dir))?;
    for (from, to) in &plan.files {
        fs::copy(from, to).map_err(io_error("copying", from))?;
    }
    Ok(Installed::Copied(plan.files.len()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::arena::MapSettings;

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("sheepwars-test-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("create scratch");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn fake_map(scratch: &Scratch, files: &[&str]) -> MapSettings {
        let source = scratch.path().join("maps").join("fake");
        fs::create_dir_all(source.join("region")).expect("region dir");
        fs::create_dir_all(source.join("entities")).expect("entities dir");
        fs::write(source.join("level.dat"), b"level").expect("level.dat");
        fs::write(source.join("entities").join("r.0.0.mca"), b"entities").expect("entities");
        for file in files {
            fs::write(source.join("region").join(file), file.as_bytes()).expect("region file");
        }
        MapSettings {
            name: "fake".to_owned(),
            source_dir: source,
            server_dir: scratch.path().join("server"),
        }
    }

    #[test]
    fn plan_lists_exactly_the_region_files_with_the_destination_pattern() {
        let scratch = Scratch::new();
        let map = fake_map(&scratch, &["r.0.0.mca", "r.-1.0.mca", "notes.txt"]);
        let plan = plan_install(&map, "world", "arena").expect("plan");
        let region = scratch
            .path()
            .join("server")
            .join("world_rspigot_arena")
            .join("dimensions")
            .join("rspigot")
            .join("arena")
            .join("region");
        assert_eq!(plan.region_dir, region);
        assert_eq!(
            plan.files,
            vec![
                (
                    map.source_dir.join("region").join("r.-1.0.mca"),
                    region.join("r.-1.0.mca")
                ),
                (
                    map.source_dir.join("region").join("r.0.0.mca"),
                    region.join("r.0.0.mca")
                ),
            ]
        );
    }

    #[test]
    fn plan_on_a_missing_source_is_a_typed_error() {
        let scratch = Scratch::new();
        let mut map = fake_map(&scratch, &[]);
        map.source_dir = scratch.path().join("absent");
        assert!(matches!(
            plan_install(&map, "world", "arena"),
            Err(InstallError::MissingSource(_))
        ));
    }

    #[test]
    fn install_copies_into_a_missing_destination() {
        let scratch = Scratch::new();
        let map = fake_map(&scratch, &["r.0.0.mca", "r.0.-1.mca"]);
        let plan = plan_install(&map, "world", "arena").expect("plan");
        assert_eq!(install(&plan).expect("install"), Installed::Copied(2));
        for (from, to) in &plan.files {
            assert_eq!(fs::read(from).expect("src"), fs::read(to).expect("dst"));
        }
    }

    #[test]
    fn install_copies_into_an_empty_destination() {
        let scratch = Scratch::new();
        let map = fake_map(&scratch, &["r.0.0.mca"]);
        let plan = plan_install(&map, "world", "arena").expect("plan");
        fs::create_dir_all(&plan.region_dir).expect("empty region");
        fs::write(plan.region_dir.join("readme.txt"), b"x").expect("non mca");
        assert_eq!(install(&plan).expect("install"), Installed::Copied(1));
    }

    #[test]
    fn install_leaves_a_populated_destination_untouched() {
        let scratch = Scratch::new();
        let map = fake_map(&scratch, &["r.0.0.mca", "r.1.0.mca"]);
        let plan = plan_install(&map, "world", "arena").expect("plan");
        fs::create_dir_all(&plan.region_dir).expect("region");
        let existing = plan.region_dir.join("r.0.0.mca");
        fs::write(&existing, b"mine").expect("existing");
        assert_eq!(install(&plan).expect("install"), Installed::AlreadyThere);
        assert_eq!(fs::read(&existing).expect("read"), b"mine");
        assert!(!plan.region_dir.join("r.1.0.mca").exists());
    }

    #[test]
    fn install_with_a_vanished_source_is_a_typed_io_error() {
        let scratch = Scratch::new();
        let map = fake_map(&scratch, &["r.0.0.mca"]);
        let plan = plan_install(&map, "world", "arena").expect("plan");
        fs::remove_file(&plan.files[0].0).expect("remove");
        assert!(matches!(install(&plan), Err(InstallError::Io { .. })));
    }

    #[test]
    fn has_regions_needs_an_mca_file() {
        let scratch = Scratch::new();
        assert!(!has_regions(&scratch.path().join("absent")));
        assert!(!has_regions(scratch.path()));
        fs::write(scratch.path().join("r.0.0.mca"), b"x").expect("write");
        assert!(has_regions(scratch.path()));
    }

    #[test]
    fn level_name_is_read_from_the_properties() {
        assert_eq!(parse_level_name("motd=hi\nlevel-name=arena\n"), "arena");
    }

    #[test]
    fn level_name_defaults_to_world_when_absent() {
        assert_eq!(parse_level_name("motd=hi\n"), "world");
        assert_eq!(parse_level_name(""), "world");
        assert_eq!(parse_level_name("level-name=\n"), "world");
    }

    #[test]
    fn level_name_tolerates_spaces_and_skips_comments() {
        let text = "# level-name=ignored\n  level-name = spaced name  \n";
        assert_eq!(parse_level_name(text), "spaced name");
        assert_eq!(parse_level_name("#level-name=a\n! level-name=b\n"), "world");
        assert_eq!(parse_level_name("level-name-other=x\n"), "world");
    }

    #[test]
    fn a_missing_properties_file_gives_the_default_level_name() {
        let scratch = Scratch::new();
        assert_eq!(read_level_name(scratch.path()).expect("read"), "world");
        fs::write(scratch.path().join("server.properties"), "level-name=big\n").expect("write");
        assert_eq!(read_level_name(scratch.path()).expect("read"), "big");
    }
}
