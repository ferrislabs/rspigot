use rspigot_plugin::{
    ActionError, ActionErrorKind, BlockPos, BlockType, Dimension, Error, Generator, Server, World,
    WorldOptions,
};

use super::install::{
    InstallError, Installed, has_regions, install, plan_install, read_level_name, region_dir,
};
use super::layout::{Layout, PlacedBlock};
use super::settings::{ArenaError, ArenaSettings};

const WORLD_SEED: i64 = 0;
const WORLD_NAMESPACE: &str = "rspigot";
const ALREADY_EXISTS: &str = "already exists";

fn created_world_id(name: &str) -> String {
    format!("{WORLD_NAMESPACE}:{name}")
}

fn is_already_created(error: &ActionError) -> bool {
    error.kind == ActionErrorKind::InvalidArgument && error.reason.contains(ALREADY_EXISTS)
}

fn install_map(settings: ArenaSettings) -> Result<ArenaSettings, ArenaError> {
    let Some(map) = &settings.map else {
        return Ok(settings);
    };
    let level = read_level_name(&map.server_dir)?;
    let outcome = plan_install(map, &level, &settings.world_name).and_then(|plan| install(&plan));
    match outcome {
        Ok(Installed::Copied(count)) => {
            tracing::info!("installed {count} region files of the map {}", map.name);
            Ok(settings)
        }
        Ok(Installed::AlreadyThere) => Ok(settings),
        Err(InstallError::MissingSource(path))
            if has_regions(&region_dir(map, &level, &settings.world_name)) =>
        {
            tracing::debug!(
                "map folder {} is absent but the world is installed",
                path.display()
            );
            Ok(settings)
        }
        Err(InstallError::MissingSource(path)) => {
            tracing::warn!(
                "map folder {} is missing, falling back to the generated platform",
                path.display()
            );
            Ok(ArenaSettings::fallback_platform())
        }
        Err(error) => Err(error.into()),
    }
}

pub struct Arena {
    pub world: World,
    pub layout: Layout,
}

impl Arena {
    pub async fn create(server: &Server, settings: ArenaSettings) -> Result<Self, ArenaError> {
        let settings = install_map(settings)?;
        let options = WorldOptions {
            name: settings.world_name.clone(),
            dimension: Dimension::Overworld,
            seed: WORLD_SEED,
            generator: Generator::Void,
            structures: false,
        };
        let world = match server.world.create(options).await {
            Ok(world) => world,
            Err(Error::Action(error)) if is_already_created(&error) => {
                server.world.by_id(created_world_id(&settings.world_name))
            }
            Err(error) => return Err(error.into()),
        };
        let layout = Layout::new(settings);
        Ok(Self { world, layout })
    }

    pub fn place_chunk(&self, chunk_x: i32, chunk_z: i32) -> Result<usize, ArenaError> {
        let placed = self.layout.blocks_in_chunk(chunk_x, chunk_z);
        let resolved = resolve(&placed)?;
        for (pos, kind) in &resolved {
            self.world.block(*pos).set(*kind);
        }
        Ok(resolved.len())
    }
}

fn resolve(placed: &[PlacedBlock]) -> Result<Vec<(BlockPos, BlockType)>, ArenaError> {
    placed
        .iter()
        .map(|block| {
            let kind = BlockType::from_key(&block.block)
                .ok_or_else(|| ArenaError::UnknownBlock(block.block.clone()))?;
            Ok((
                BlockPos {
                    x: block.x,
                    y: block.y,
                    z: block.z,
                },
                kind,
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placed(block: &str) -> PlacedBlock {
        PlacedBlock {
            x: 1,
            y: 2,
            z: 3,
            block: block.to_owned(),
        }
    }

    #[test]
    fn a_created_world_id_carries_the_server_namespace() {
        assert_eq!(created_world_id("sheepwars"), "rspigot:sheepwars");
    }

    #[test]
    fn only_an_already_exists_refusal_reuses_the_world() {
        let refusal = |kind, reason: &str| ActionError {
            kind,
            reason: reason.to_owned(),
        };
        assert!(is_already_created(&refusal(
            ActionErrorKind::InvalidArgument,
            "the world rspigot:sheepwars already exists"
        )));
        assert!(!is_already_created(&refusal(
            ActionErrorKind::InvalidArgument,
            "`Bad Name` is not a world name"
        )));
        assert!(!is_already_created(&refusal(
            ActionErrorKind::LimitReached,
            "the world already exists"
        )));
    }

    #[test]
    fn resolves_known_blocks() {
        let resolved = resolve(&[placed("minecraft:stone")]).expect("known");
        assert_eq!(resolved[0].0, BlockPos { x: 1, y: 2, z: 3 });
    }

    #[test]
    fn unknown_block_is_a_typed_error() {
        assert!(matches!(
            resolve(&[placed("minecraft:nope")]),
            Err(ArenaError::UnknownBlock(_))
        ));
    }

    #[test]
    fn default_blocks_all_resolve() {
        let layout = Layout::new(ArenaSettings::fallback_platform());
        assert!(resolve(&layout.blocks()).is_ok());
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("sheepwars-build-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    fn map_settings(dir: &std::path::Path) -> ArenaSettings {
        let mut settings = ArenaSettings {
            world_name: "arena".to_owned(),
            ..ArenaSettings::default()
        };
        let map = settings.map.as_mut().expect("map");
        map.source_dir = dir.join("source");
        map.server_dir = dir.join("server");
        settings
    }

    #[test]
    fn a_missing_map_falls_back_to_the_generated_platform() {
        let dir = scratch("missing");
        let settings = install_map(map_settings(&dir)).expect("fallback");
        assert_eq!(settings, ArenaSettings::fallback_platform());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_present_map_is_installed_and_kept() {
        let dir = scratch("present");
        std::fs::create_dir_all(dir.join("source").join("region")).expect("source");
        std::fs::write(dir.join("source").join("region").join("r.0.0.mca"), b"x").expect("mca");
        let wanted = map_settings(&dir);
        let settings = install_map(wanted.clone()).expect("install");
        assert_eq!(settings, wanted);
        let target = dir
            .join("server")
            .join("world_rspigot_arena")
            .join("dimensions")
            .join("rspigot")
            .join("arena")
            .join("region")
            .join("r.0.0.mca");
        assert!(target.is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_installed_world_is_kept_when_the_source_is_gone() {
        let dir = scratch("gone");
        let wanted = map_settings(&dir);
        let target = dir
            .join("server")
            .join("world_rspigot_arena")
            .join("dimensions")
            .join("rspigot")
            .join("arena")
            .join("region");
        std::fs::create_dir_all(&target).expect("target");
        std::fs::write(target.join("r.0.0.mca"), b"x").expect("mca");
        assert_eq!(install_map(wanted.clone()).expect("kept"), wanted);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
