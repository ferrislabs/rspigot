use rspigot_domain::Identifier;
use rspigot_nbt::{Compound, Tag};

use crate::ports::ClientLink;
use crate::server::Server;

const LEAF: &str = "command_storage";
const CONTENTS: &str = "contents";

fn file_name(namespace: &str) -> String {
    format!("{namespace}:{LEAF}")
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn load_command_storage(&mut self) {
        let Some(io) = &self.player_io else {
            return;
        };
        let namespaces = match io.saved_data_namespaces(LEAF) {
            Ok(namespaces) => namespaces,
            Err(error) => {
                tracing::warn!("Failed to list command storage files: {error}");
                return;
            }
        };
        for namespace in namespaces {
            let file = match io.load_saved_data(&file_name(&namespace)) {
                Ok(Some(file)) => file,
                Ok(None) => continue,
                Err(error) => {
                    tracing::warn!("Failed to load command storage {namespace}: {error}");
                    continue;
                }
            };
            let Some(contents) = file.get_compound(CONTENTS) else {
                continue;
            };
            for (path, tag) in contents.iter() {
                let (Tag::Compound(data), Ok(id)) =
                    (tag, Identifier::parse(&format!("{namespace}:{path}")))
                else {
                    continue;
                };
                self.command_storage.insert(id, data.clone());
            }
        }
    }

    pub(crate) fn save_command_storage(&mut self) {
        if self.command_storage_dirty.is_empty() {
            return;
        }
        let dirty = std::mem::take(&mut self.command_storage_dirty);
        let Some(io) = &self.player_io else {
            return;
        };
        for namespace in dirty {
            let mut contents = Compound::new();
            for (id, data) in &self.command_storage {
                if id.namespace() == namespace {
                    contents.insert(id.path(), data.clone());
                }
            }
            io.save_saved_data(
                &file_name(&namespace),
                Compound::new().with(CONTENTS, contents),
            );
        }
    }
}
