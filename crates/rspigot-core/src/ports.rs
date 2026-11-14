use std::net::IpAddr;

use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::player::{GameProfile, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Dimension;
use rspigot_domain::{ChunkPos, Identifier, Vec3};
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

pub trait PlayerStore: Send + Sync + 'static {
    fn load_player(&self, player: PlayerId) -> Result<Option<Compound>, StoreError>;
    fn save_player(&self, player: PlayerId, data: &Compound) -> Result<(), StoreError>;
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
