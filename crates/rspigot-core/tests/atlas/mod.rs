#![allow(dead_code)]

use std::collections::HashMap;
use std::num::NonZeroU8;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{
    ClickInput, ClientEvent, ContainerClick, HashedSlot, Inbound, PlayerInput,
};
use rspigot_core::ports::{ChunkGenerator, PlayerStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::{BlockEntity, ChunkColumn};
use rspigot_domain::item::ItemStack;
use rspigot_domain::map_data::MapData;
use rspigot_domain::player::PlayerId;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, Identifier, Vec3};
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};

use crate::common::{Link, NoStore, Scene, scene_on, send_input};

pub const MAP_ID: &str = "minecraft:map_id";
pub const POST_PROCESSING: &str = "minecraft:map_post_processing";
pub const OVERWORLD: &str = "minecraft:overworld";

#[derive(Default)]
pub struct Meadow {
    pub structure: Option<(String, Vec3)>,
    pub asked: Arc<Mutex<Vec<Vec<String>>>>,
    pub loot_chests: Vec<(BlockPos, &'static str)>,
}

impl Meadow {
    pub fn with_structure(id: &str, at: Vec3) -> Self {
        Self {
            structure: Some((id.to_owned(), at)),
            ..Self::default()
        }
    }
}

impl ChunkGenerator for Meadow {
    fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
        let biome = rspigot_data::registries::biome_id("minecraft:plains").unwrap();
        let mut chunk = ChunkColumn::empty(pos, dimension.height, BiomeId(biome));
        for x in 0..16 {
            for z in 0..16 {
                for y in 60..=63 {
                    chunk.set_block(
                        BlockPos::new(pos.min_block_x() + x, y, pos.min_block_z() + z),
                        BlockKind::Stone.default_state(),
                    );
                }
            }
        }
        let pos_of_chunk = pos;
        for (pos, table) in self
            .loot_chests
            .iter()
            .filter(|(pos, _)| pos.chunk() == pos_of_chunk)
        {
            chunk.set_block(*pos, BlockKind::Chest.default_state());
            chunk.set_block_entity(BlockEntity {
                pos: *pos,
                kind: Identifier::minecraft("chest"),
                data: Compound::new()
                    .with("Items", Tag::List(Vec::new()))
                    .with("LootTable", *table)
                    .with("LootTableSeed", 7i64),
            });
        }
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }

    fn nearest_structures(
        &self,
        _: &Dimension,
        _: Vec3,
        structure_ids: &[String],
    ) -> Option<(String, Vec3)> {
        self.asked.lock().unwrap().push(structure_ids.to_vec());
        self.structure.clone()
    }
}

#[derive(Default)]
pub struct Atlas {
    players: Mutex<HashMap<PlayerId, Compound>>,
    files: Mutex<HashMap<String, Compound>>,
}

impl Atlas {
    pub fn file(&self, name: &str) -> Option<Compound> {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let found = self.files.lock().ok().and_then(|f| f.get(name).cloned());
            if found.is_some() || std::time::Instant::now() > deadline {
                return found;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    pub fn map(&self, id: i32) -> MapData {
        let file = self.file(&format!("maps/{id}")).expect("the map is saved");
        MapData::from_nbt(&file).expect("a vanilla map")
    }

    pub fn put(&self, name: &str, data: Compound) {
        self.files
            .lock()
            .expect("lock")
            .insert(name.to_owned(), data);
    }

    pub fn put_map(&self, id: i32, data: &MapData) {
        self.put(&format!("maps/{id}"), data.to_nbt());
    }
}

impl PlayerStore for Atlas {
    fn load_player(&self, player: PlayerId) -> Result<Option<Compound>, StoreError> {
        Ok(self
            .players
            .lock()
            .ok()
            .and_then(|files| files.get(&player).cloned()))
    }

    fn save_player(&self, player: PlayerId, data: &Compound) -> Result<(), StoreError> {
        if let Ok(mut files) = self.players.lock() {
            files.insert(player, data.clone());
        }
        Ok(())
    }

    fn load_saved_data(&self, name: &str) -> Result<Option<Compound>, StoreError> {
        Ok(self
            .files
            .lock()
            .ok()
            .and_then(|files| files.get(name).cloned()))
    }

    fn save_saved_data(&self, name: &str, data: &Compound) -> Result<(), StoreError> {
        if let Ok(mut files) = self.files.lock() {
            files.insert(name.to_owned(), data.clone());
        }
        Ok(())
    }
}

pub fn meadow_server(atlas: &Arc<Atlas>, meadow: Meadow) -> Server<Link> {
    let config = GameConfig {
        view_distance: 3,
        chunk_workers: 2,
        spawn_protection: 0,
        ..GameConfig::default()
    };
    let data = LevelData {
        spawn_mobs: false,
        day_time: 6_000,
        daylight_cycle: false,
        ..LevelData::default()
    };
    let mut server = Server::new(
        config,
        data,
        Arc::new(NoStore),
        Arc::new(meadow),
        StatusBoard::new(20),
    )
    .with_rng_seed(7)
    .with_player_store(Arc::clone(atlas));
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

pub fn scene_with_atlas(meadow: Meadow) -> (Scene, Arc<Atlas>) {
    let atlas = Arc::new(Atlas::default());
    let server = meadow_server(&atlas, meadow);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    console(&mut s, "gamemode survival Alice");
    s.players[0].link.take();
    (s, atlas)
}

pub fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

pub fn filled_stack(id: i32) -> ItemStack {
    let mut stack = ItemStack::new(ItemKind::FilledMap.id(), NonZeroU8::MIN);
    stack.components.insert(MAP_ID, id);
    stack
}

pub fn put_in_slot(s: &mut Scene, slot: i16, stack: ItemStack) {
    console(s, "gamemode creative Alice");
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::CreativeSlot {
            slot,
            stack: Some(stack),
        },
    );
    console(s, "gamemode survival Alice");
}

pub struct Window {
    pub id: i32,
    pub state_id: i32,
    pub events: Vec<ClientEvent>,
}

impl Window {
    pub fn opened(events: Vec<ClientEvent>) -> Self {
        let mut window = Self {
            id: 0,
            state_id: 0,
            events: Vec::new(),
        };
        window.absorb(events);
        window
    }

    pub fn absorb(&mut self, events: Vec<ClientEvent>) {
        for event in &events {
            match event {
                ClientEvent::OpenScreen { window, .. } => self.id = *window,
                ClientEvent::ContainerContent {
                    window, state_id, ..
                }
                | ClientEvent::ContainerSlot {
                    window, state_id, ..
                } if *window == self.id => self.state_id = *state_id,
                _ => {}
            }
        }
        self.events.extend(events);
    }

    pub fn slot(&self, slot: usize) -> Option<ItemStack> {
        self.events
            .iter()
            .rev()
            .find_map(|event| match event {
                ClientEvent::ContainerSlot {
                    window,
                    slot: at,
                    stack,
                    ..
                } if *window == self.id && usize::try_from(*at) == Ok(slot) => Some(stack.clone()),
                ClientEvent::ContainerContent { window, slots, .. } if *window == self.id => {
                    Some(slots.get(slot).cloned().flatten())
                }
                _ => None,
            })
            .flatten()
    }

    pub fn carried(&self) -> Option<ItemStack> {
        self.events
            .iter()
            .rev()
            .find_map(|event| match event {
                ClientEvent::ContainerContent {
                    window, carried, ..
                } if *window == self.id => Some(carried.clone()),
                _ => None,
            })
            .flatten()
    }

    pub fn click(&mut self, s: &mut Scene, slot: i16, button: i8, input: ClickInput) {
        let alice = s.players[0].clone();
        send_input(
            &mut s.server,
            &alice,
            PlayerInput::ContainerClick(ContainerClick {
                container: self.id,
                state_id: self.state_id,
                slot,
                button,
                input,
                changed: Vec::new(),
                carried: HashedSlot::default(),
            }),
        );
        self.absorb(alice.link.take());
    }

    pub fn sounds(&self, name: &str) -> usize {
        self.events
            .iter()
            .filter(|event| matches!(event, ClientEvent::Sound { sound, .. } if *sound == name))
            .count()
    }
}
