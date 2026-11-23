use std::net::IpAddr;

use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::player::{GameProfile, PlayerId};
use rspigot_domain::poi::PoiChunkData;
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BlockPos, BlockStateId, ChunkPos, Identifier, Vec3};
use rspigot_nbt::Compound;

use crate::events::ClientEvent;
use crate::plugins::ToPlugin;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("corrupt chunk at {pos:?}: {reason}")]
    Corrupt { pos: ChunkPos, reason: String },
    #[error("corrupt player data for {player}: {reason}")]
    CorruptPlayer { player: PlayerId, reason: String },
}

pub trait ChunkStore: Send + Sync + 'static {
    fn load(
        &self,
        dimension: &Identifier,
        pos: ChunkPos,
    ) -> Result<Option<ChunkColumn>, StoreError>;
    fn save(&self, dimension: &Identifier, chunk: &ChunkColumn) -> Result<(), StoreError>;
    fn flush(&self) -> Result<(), StoreError>;
}

pub trait EntityStore: Send + Sync + 'static {
    fn load_entities(
        &self,
        dimension: &Identifier,
        pos: ChunkPos,
    ) -> Result<Option<Vec<Compound>>, StoreError>;
    fn save_entities(
        &self,
        dimension: &Identifier,
        pos: ChunkPos,
        entities: &[Compound],
    ) -> Result<(), StoreError>;

    fn load_poi(
        &self,
        _dimension: &Identifier,
        _pos: ChunkPos,
    ) -> Result<Option<PoiChunkData>, StoreError> {
        Ok(None)
    }

    fn save_poi(
        &self,
        _dimension: &Identifier,
        _pos: ChunkPos,
        _data: &PoiChunkData,
    ) -> Result<(), StoreError> {
        Ok(())
    }

    fn load_world_data(
        &self,
        _dimension: &Identifier,
        _name: &str,
    ) -> Result<Option<Compound>, StoreError> {
        Ok(None)
    }

    fn save_world_data(
        &self,
        _dimension: &Identifier,
        _name: &str,
        _data: &Compound,
    ) -> Result<(), StoreError> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProgressFile {
    Stats,
    Advancements,
}

pub trait PlayerStore: Send + Sync + 'static {
    fn load_player(&self, player: PlayerId) -> Result<Option<Compound>, StoreError>;
    fn save_player(&self, player: PlayerId, data: &Compound) -> Result<(), StoreError>;

    fn load_saved_data(&self, _name: &str) -> Result<Option<Compound>, StoreError> {
        Ok(None)
    }

    fn save_saved_data(&self, _name: &str, _data: &Compound) -> Result<(), StoreError> {
        Ok(())
    }

    fn saved_data_namespaces(&self, _leaf: &str) -> Result<Vec<String>, StoreError> {
        Ok(Vec::new())
    }

    fn load_progress(
        &self,
        _player: PlayerId,
        _file: ProgressFile,
    ) -> Result<Option<String>, StoreError> {
        Ok(None)
    }

    fn save_progress(
        &self,
        _player: PlayerId,
        _file: ProgressFile,
        _json: &str,
    ) -> Result<(), StoreError> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TemplateRotation {
    #[default]
    None,
    Clockwise90,
    Clockwise180,
    Counterclockwise90,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TemplateMirror {
    #[default]
    None,
    LeftRight,
    FrontBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemplateInfo {
    pub size: [i32; 3],
    pub blocks: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlaceRequest {
    Structure {
        id: String,
        pos: BlockPos,
    },
    Jigsaw {
        pool: String,
        target: String,
        max_depth: i32,
        pos: BlockPos,
    },
    Template {
        id: String,
        pos: BlockPos,
        rotation: TemplateRotation,
        mirror: TemplateMirror,
        integrity: f32,
        seed: i32,
    },
    Feature {
        id: String,
        pos: BlockPos,
    },
    Grow {
        feature: String,
        origin: BlockPos,
        clear: Vec<BlockPos>,
        seed: i64,
        window: crate::grow_window::GrowWindow,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacedBlock {
    pub pos: BlockPos,
    pub state: BlockStateId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceFailure {
    Unknown,
    Failed,
}

pub trait ChunkGenerator: Send + Sync + 'static {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn;
    fn spawn_point(&self, dimension: &Dimension) -> Vec3;

    fn spawns_original_mobs(&self, _dimension: &Dimension) -> bool {
        false
    }

    fn structure_entities(&self, _dimension: &Dimension, _pos: ChunkPos) -> Vec<Compound> {
        Vec::new()
    }

    fn nearest_stronghold(&self, _dimension: &Dimension, _from: Vec3) -> Option<Vec3> {
        None
    }

    fn nearest_structure(
        &self,
        _dimension: &Dimension,
        _from: Vec3,
        _structure_id: &str,
    ) -> Option<(String, Vec3)> {
        None
    }

    fn nearest_structures(
        &self,
        dimension: &Dimension,
        from: Vec3,
        structure_ids: &[String],
    ) -> Option<(String, Vec3)> {
        structure_ids
            .iter()
            .filter_map(|id| self.nearest_structure(dimension, from, id))
            .min_by(|(_, a), (_, b)| {
                let distance = |at: &Vec3| (at.x - from.x).powi(2) + (at.z - from.z).powi(2);
                distance(a).total_cmp(&distance(b))
            })
    }

    fn knows_structure(&self, _dimension: &Dimension, _structure_id: &str) -> bool {
        false
    }

    fn nearest_biome(
        &self,
        _dimension: &Dimension,
        _from: Vec3,
        _biomes: &[String],
    ) -> Option<(String, Vec3)> {
        None
    }

    fn place(
        &self,
        _dimension: &Dimension,
        _request: &PlaceRequest,
    ) -> Result<Vec<PlacedBlock>, PlaceFailure> {
        Err(PlaceFailure::Failed)
    }

    fn template_names(&self) -> Vec<String> {
        Vec::new()
    }

    fn template_info(&self, _id: &str) -> Option<TemplateInfo> {
        None
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("session server rejected {0}")]
    NotAuthenticated(String),
    #[error("session server unreachable: {0}")]
    Unreachable(String),
    #[error("invalid session server response: {0}")]
    InvalidResponse(String),
}

pub trait SessionVerifier: Send + Sync + 'static {
    fn verify(
        &self,
        username: &str,
        server_hash: &str,
        client_ip: Option<IpAddr>,
    ) -> Result<GameProfile, AuthError>;
}

pub trait ClientLink: Send + 'static {
    fn send(&self, event: ClientEvent);
    fn disconnect(&self, reason: Component);
    fn is_connected(&self) -> bool;
}

pub trait PluginOutbox: Send + 'static {
    fn send(&self, message: ToPlugin);
}

#[derive(Debug, thiserror::Error)]
pub enum DatapackError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PackMeta {
    pub description: String,
    pub pack_format: Option<i32>,
    pub min_format: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackFile {
    pub id: Identifier,
    pub text: String,
}

pub type PackItemModifier = PackFile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackTag {
    pub registry: String,
    pub file: PackFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackContents {
    pub name: String,
    pub meta: PackMeta,
    pub functions: Vec<PackFile>,
    pub function_tags: Vec<PackFile>,
    pub predicates: Vec<PackFile>,
    pub loot_tables: Vec<PackFile>,
    pub item_modifiers: Vec<PackItemModifier>,
    pub tags: Vec<PackTag>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackRegistryFile {
    pub registry: String,
    pub file: PackFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackExtras {
    pub name: String,
    pub registries: Vec<PackRegistryFile>,
    pub advancements: Vec<PackFile>,
    pub recipes: Vec<PackFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnchantmentEntry {
    pub id: Identifier,
    pub json: String,
}

pub trait DatapackSource: Send + Sync + 'static {
    fn read_packs(&self) -> Result<Vec<PackContents>, DatapackError>;

    fn read_extras(&self) -> Result<Vec<PackExtras>, DatapackError> {
        Ok(Vec::new())
    }

    fn create_pack(&self, _name: &str, _meta: &str) -> Result<bool, DatapackError> {
        Err(DatapackError::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "this datapack source cannot create packs",
        )))
    }
}

pub trait RconExecutor: Send + Sync + 'static {
    fn execute(&self, command: &str) -> Option<String>;
}
