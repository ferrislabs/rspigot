use std::sync::{Arc, OnceLock};

use rspigot_anvil::AnvilStore;
use rspigot_core::ports::ClientLink;
use rspigot_core::server::Server;
use rspigot_domain::Identifier;
use rspigot_domain::world::Dimension;
use rspigot_worldgen::WorldGenerator;

struct Worlds {
    store: Arc<AnvilStore>,
    generator: Arc<WorldGenerator>,
}

static WORLDS: OnceLock<Worlds> = OnceLock::new();

pub fn install(store: Arc<AnvilStore>, generator: Arc<WorldGenerator>) {
    let _ = WORLDS.set(Worlds { store, generator });
}

pub fn provide<L: ClientLink>(server: &mut Server<L>, key: &Identifier) {
    let Some(worlds) = WORLDS.get() else {
        return;
    };
    if *key == Identifier::minecraft("the_end") {
        server.load_dimension(
            key.clone(),
            Dimension::end(),
            Arc::clone(&worlds.generator),
            Arc::clone(&worlds.store),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use rspigot_anvil::DimensionLayout;
    use rspigot_core::config::{GameConfig, LevelData};
    use rspigot_core::events::{ClientEvent, Inbound};
    use rspigot_core::plugins::events::PlayerTeleportCause;
    use rspigot_core::ports::{ChunkStore, EntityStore};
    use rspigot_core::status::StatusBoard;
    use rspigot_domain::chunk::ChunkColumn;
    use rspigot_domain::dragon::spikes::spikes_for_seed;
    use rspigot_domain::player::{ClientSettings, GameProfile, PlayerId};
    use rspigot_domain::text::Component;
    use rspigot_domain::{BlockPos, ChunkPos, Vec3};
    use rspigot_nbt::Compound;

    use super::*;

    const SEED: i64 = 20_261_114;

    #[derive(Clone)]
    struct Mute;

    impl ClientLink for Mute {
        fn send(&self, _: ClientEvent) {}

        fn disconnect(&self, _: Component) {}

        fn is_connected(&self) -> bool {
            true
        }
    }

    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn end() -> Identifier {
        Identifier::minecraft("the_end")
    }

    fn open(root: &std::path::Path) -> Arc<AnvilStore> {
        Arc::new(AnvilStore::open(root, DimensionLayout::Spigot).expect("store"))
    }

    fn crystals(entities: &[Compound]) -> usize {
        entities
            .iter()
            .filter(|nbt| nbt.get_str("id") == Some("minecraft:end_crystal"))
            .count()
    }

    #[test]
    fn the_end_is_created_on_demand_persists_and_reloads_from_its_own_folder() {
        let parent = std::env::temp_dir().join(format!("rspigot-end-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&parent);
        let _scratch = Scratch(parent.clone());
        let root = parent.join("world");
        std::fs::create_dir_all(&root).expect("world folder");
        let store = open(&root);
        let generator = Arc::new(WorldGenerator::from_level_type("default", SEED, ""));
        install(Arc::clone(&store), Arc::clone(&generator));
        let config = GameConfig {
            chunk_workers: 2,
            spawn_chunk_radius: 4,
            ..GameConfig::default()
        };
        let level = LevelData {
            seed: SEED,
            spawn_mobs: false,
            ..LevelData::default()
        };
        let mut server = Server::new(
            config,
            level,
            Arc::clone(&store),
            Arc::clone(&generator),
            StatusBoard::new(1),
        )
        .with_dimension_provider(provide::<Mute>);
        server.wait_for_spawn(Duration::from_secs(60));
        let id = PlayerId::offline("Alice");
        server.handle(Inbound::Join {
            profile: GameProfile {
                id,
                name: "Alice".to_owned(),
                properties: Vec::new(),
            },
            settings: ClientSettings::default(),
            link: Mute,
            address: [127, 0, 0, 1].into(),
            session: 1,
        });
        server.deliver();
        server.change_dimension(
            id,
            end(),
            Vec3::new(0.0, 0.0, 0.0),
            None,
            PlayerTeleportCause::EndPortal,
        );
        server.deliver();
        assert_eq!(server.player_dimension(id), Some(end()));
        let (sender, inbound) = crossbeam_channel::unbounded::<Inbound<Mute>>();
        sender.send(Inbound::Shutdown).expect("open");
        server.run(&inbound);
        let reopened = open(&root);
        let region = DimensionLayout::Spigot
            .region_dir(&root, &end())
            .expect("end folder");
        assert!(region.starts_with(&parent) && !region.starts_with(&root));
        assert!(region.ends_with("minecraft/the_end/region"));
        assert!(std::fs::read_dir(&region).expect("region folder").count() > 0);
        let obsidian = rspigot_blocks::BlockKind::Obsidian.default_state();
        let mut saved = 0;
        for spike in spikes_for_seed(SEED) {
            let pos = BlockPos::new(spike.center_x, 0, spike.center_z).chunk();
            let column: ChunkColumn = reopened
                .load(&end(), pos)
                .expect("readable")
                .expect("chunk saved");
            assert_eq!(
                column.block(BlockPos::new(
                    spike.center_x,
                    spike.height - 1,
                    spike.center_z
                )),
                Some(obsidian)
            );
            saved += crystals(
                &reopened
                    .load_entities(&end(), pos)
                    .expect("readable")
                    .unwrap_or_default(),
            );
        }
        assert_eq!(saved, 10);
        let platform = reopened
            .load(&end(), ChunkPos::new(6, 0))
            .expect("readable")
            .expect("platform chunk saved");
        assert_eq!(platform.block(BlockPos::new(100, 48, 0)), Some(obsidian));
    }
}
