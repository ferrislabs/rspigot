use std::sync::Arc;

use rspigot_domain::Identifier;

use super::{DatapackLoader, FunctionState, SCHEDULED_DATA, ScheduledEvents};
use crate::ports::{ClientLink, DatapackSource};
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub fn with_datapacks<D: DatapackSource>(mut self, source: Arc<D>) -> Self {
        let loader = DatapackLoader::spawn(source);
        match loader.read() {
            Ok(contents) => {
                let scheduled = std::mem::take(&mut self.functions.scheduled);
                self.functions =
                    FunctionState::load(contents, self.world.level.data_packs.as_ref());
                self.functions.scheduled = scheduled;
            }
            Err(error) => tracing::warn!("Could not read the datapacks: {error}"),
        }
        self.world.level.data_packs = Some(self.functions.packs.lists());
        tracing::info!(
            "Loaded {} functions from {} data packs",
            self.functions.library.len(),
            self.functions.packs.enabled().len()
        );
        self.datapacks = Some(loader);
        self
    }

    pub(crate) fn function_names(&self) -> Vec<String> {
        let library = &self.functions.library;
        library
            .function_ids()
            .into_iter()
            .map(ToString::to_string)
            .chain(library.tag_ids().into_iter().map(|id| format!("#{id}")))
            .collect()
    }

    pub(crate) fn pack_names_after(&self, head: &str) -> Option<Vec<String>> {
        let words: Vec<String> = head
            .split_whitespace()
            .map(str::to_ascii_lowercase)
            .collect();
        let words: Vec<&str> = words.iter().map(String::as_str).collect();
        let packs = &self.functions.packs;
        let quoted = |ids: Vec<&str>| ids.into_iter().map(|id| format!("\"{id}\"")).collect();
        match words.as_slice() {
            ["datapack", "enable"] => {
                Some(quoted(packs.disabled().map(|p| p.id.as_str()).collect()))
            }
            ["datapack", "disable"] => Some(quoted(
                packs
                    .enabled()
                    .iter()
                    .map(String::as_str)
                    .filter(|id| *id != super::VANILLA_PACK)
                    .collect(),
            )),
            ["datapack", "enable", _, "before" | "after"] => {
                Some(quoted(packs.enabled().iter().map(String::as_str).collect()))
            }
            ["schedule", "clear"] => Some(
                self.functions
                    .scheduled
                    .names()
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            ),
            _ => None,
        }
    }

    pub(crate) fn reload_data_packs(&mut self) -> bool {
        let Some(loader) = &self.datapacks else {
            return false;
        };
        let contents = match loader.read() {
            Ok(contents) => contents,
            Err(error) => {
                tracing::warn!("Could not reload the datapacks: {error}");
                return false;
            }
        };
        self.functions.reload(contents);
        self.world.level.data_packs = Some(self.functions.packs.lists());
        self.run_load_functions();
        true
    }

    pub(crate) fn run_load_functions(&mut self) {
        crate::commands::run_server_tag(self, &Identifier::minecraft("load"));
    }

    pub(crate) fn tick_functions(&mut self) {
        crate::commands::run_server_tag(self, &Identifier::minecraft("tick"));
    }

    pub(crate) fn tick_scheduled_events(&mut self) {
        let now = self.world.level.game_time;
        for event in self.functions.scheduled.take_due(now) {
            crate::commands::run_scheduled(self, &event);
        }
    }

    pub(crate) fn load_scheduled_events(&mut self) {
        if let Some(data) = &self.world.level.scheduled_events {
            self.functions.scheduled = ScheduledEvents::from_nbt(data);
        }
    }

    pub(crate) fn save_scheduled_events(&mut self) {
        if self.functions.scheduled.is_empty() && self.world.level.scheduled_events.is_none() {
            return;
        }
        self.world.level.scheduled_events = Some(self.functions.scheduled.to_nbt());
    }

    pub(crate) fn save_scheduled_events_data(&mut self) {
        self.save_scheduled_events();
        if let Some(data) = &self.world.level.scheduled_events {
            self.workers.submit(
                crate::chunks::Priority::BACKGROUND,
                crate::chunks::ChunkJob::SaveData {
                    dimension: Arc::clone(&self.world.dimension),
                    name: SCHEDULED_DATA,
                    data: Arc::new(data.clone()),
                },
            );
        }
    }
}
