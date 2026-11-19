use rspigot_plugin::{
    ActionError, ActionErrorKind, BlockPos, BlockType, Dimension, Error, Generator, Server, World,
    WorldOptions,
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

pub struct Arena {
    pub world: World,
    pub layout: Layout,
}

impl Arena {
    pub async fn create(server: &Server, settings: ArenaSettings) -> Result<Self, ArenaError> {
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
        let layout = Layout::new(ArenaSettings::default());
        assert!(resolve(&layout.blocks()).is_ok());
    }
}
