use std::sync::Arc;

use rspigot_domain::Identifier;
use rspigot_domain::world::Dimension;

use crate::plugins::{ActionError, ActionErrorKind};
use crate::ports::{ChunkGenerator, ChunkStore, ClientLink, EntityStore};
use crate::server::Server;
use crate::world_state::DimensionKey;

pub const MAX_CREATED_WORLDS: usize = 8;
pub const WORLD_NAMESPACE: &str = "rspigot";
const MAX_NAME_LENGTH: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldKind {
    Overworld,
    Nether,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerSpec {
    pub block: String,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratorSpec {
    Vanilla,
    Flat(Vec<LayerSpec>),
    Void,
    Datapack(Identifier),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldSpec {
    pub name: String,
    pub kind: WorldKind,
    pub seed: i64,
    pub generator: GeneratorSpec,
    pub structures: bool,
}

impl WorldSpec {
    pub fn key(&self) -> Identifier {
        Identifier::parse(&format!("{WORLD_NAMESPACE}:{}", self.name))
            .unwrap_or_else(|_| Identifier::minecraft("void"))
    }

    pub fn dimension(&self) -> Dimension {
        let mut dimension = match self.kind {
            WorldKind::Overworld => Dimension::overworld(),
            WorldKind::Nether => Dimension::nether(),
            WorldKind::End => Dimension::end(),
        };
        dimension.key = self.key();
        dimension
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldsChange {
    pub worlds: Vec<WorldSpec>,
    pub erased: Option<Identifier>,
}

pub type WorldFactory<L> = fn(&mut Server<L>, &WorldSpec) -> Result<(), String>;

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_NAME_LENGTH
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_-".contains(&byte))
}

impl<L: ClientLink> Server<L> {
    #[must_use]
    pub fn with_world_factory(mut self, factory: WorldFactory<L>) -> Self {
        self.world_factory = Some(factory);
        self
    }

    #[must_use]
    pub fn with_worlds_sink(mut self, sink: crossbeam_channel::Sender<WorldsChange>) -> Self {
        self.worlds_sink = Some(sink);
        self
    }

    pub fn load_world<S: ChunkStore + EntityStore, G: ChunkGenerator>(
        &mut self,
        spec: &WorldSpec,
        generator: Arc<G>,
        store: Arc<S>,
    ) {
        self.load_dimension_as(
            spec.key(),
            spec.dimension(),
            generator,
            store,
            Some((&spec.name, spec.seed)),
        );
    }

    pub fn restore_worlds(&mut self, specs: Vec<WorldSpec>) {
        let Some(factory) = self.world_factory else {
            return;
        };
        for spec in specs {
            if !valid_name(&spec.name) {
                tracing::warn!(name = %spec.name, "ignoring a created world with an invalid name");
                continue;
            }
            match factory(self, &spec) {
                Ok(()) => self.created_worlds.push(spec),
                Err(reason) => {
                    tracing::warn!(world = %spec.key(), %reason, "could not reload a created world");
                }
            }
        }
    }

    pub(crate) fn world_exists(&self, key: &DimensionKey) -> bool {
        self.world.dimension.key == *key || self.dormant.contains_key(key)
    }

    pub(crate) fn create_world(&mut self, spec: WorldSpec) -> Result<Identifier, ActionError> {
        if !valid_name(&spec.name) {
            return Err(ActionError::new(
                ActionErrorKind::InvalidArgument,
                format!(
                    "`{}` is not a world name: use 1 to {MAX_NAME_LENGTH} of a-z, 0-9, _ and -",
                    spec.name
                ),
            ));
        }
        let key = spec.key();
        if self.world_exists(&key) {
            return Err(ActionError::new(
                ActionErrorKind::InvalidArgument,
                format!("the world {key} already exists"),
            ));
        }
        if self.created_worlds.len() >= MAX_CREATED_WORLDS {
            return Err(ActionError::new(
                ActionErrorKind::LimitReached,
                format!("the server already holds {MAX_CREATED_WORLDS} created worlds"),
            ));
        }
        let Some(factory) = self.world_factory else {
            return Err(ActionError::new(
                ActionErrorKind::IncompatibleState,
                "this server cannot create worlds",
            ));
        };
        factory(self, &spec)
            .map_err(|reason| ActionError::new(ActionErrorKind::InvalidArgument, reason))?;
        if !self.dormant.contains_key(&key) {
            return Err(ActionError::new(
                ActionErrorKind::InvalidArgument,
                format!("the world {key} could not be loaded"),
            ));
        }
        self.created_worlds.push(spec);
        self.publish_worlds(None);
        Ok(key)
    }

    pub(crate) fn delete_world(
        &mut self,
        key: &DimensionKey,
        files: bool,
    ) -> Result<(), ActionError> {
        if !self.world_exists(key) {
            return Err(ActionError::new(
                ActionErrorKind::NotFound,
                format!("there is no world {key}"),
            ));
        }
        if !self.created_worlds.iter().any(|spec| spec.key() == *key) {
            return Err(ActionError::new(
                ActionErrorKind::IncompatibleState,
                format!("the world {key} was not created by a plugin and cannot be deleted"),
            ));
        }
        if self.players.values().any(|player| player.dim == *key) {
            return Err(ActionError::new(
                ActionErrorKind::IncompatibleState,
                format!("a player is still in the world {key}"),
            ));
        }
        let Some(mut state) = self.dormant.remove(key) else {
            return Err(ActionError::new(
                ActionErrorKind::NotFound,
                format!("there is no world {key}"),
            ));
        };
        if !files {
            self.swap_world(&mut state);
            self.save_all_entities();
            self.save_world_data();
            self.world.chunks.save_dirty(&self.workers);
            self.workers.submit(
                crate::chunks::Priority::BACKGROUND,
                crate::chunks::ChunkJob::Flush,
            );
            self.swap_world(&mut state);
        }
        state.workers.shutdown();
        self.created_worlds.retain(|spec| spec.key() != *key);
        self.publish_worlds(files.then(|| key.clone()));
        Ok(())
    }

    fn publish_worlds(&self, erased: Option<Identifier>) {
        if let Some(sink) = &self.worlds_sink {
            let _ = sink.send(WorldsChange {
                worlds: self.created_worlds.clone(),
                erased,
            });
        }
    }

    pub(crate) fn within_world<R>(
        &mut self,
        key: &DimensionKey,
        run: impl FnOnce(&mut Self) -> R,
    ) -> Option<R> {
        if !self.world_exists(key) {
            return None;
        }
        Some(self.in_dimension(key, |server| {
            server.drain_world_results();
            run(server)
        }))
    }
}
