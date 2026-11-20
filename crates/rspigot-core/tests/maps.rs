mod common;

use std::collections::HashMap;
use std::num::NonZeroU8;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::*;
use rspigot_blocks::BlockKind;
use rspigot_core::config::{GameConfig, LevelData};
use rspigot_core::events::{ClientEvent, Direction, Hand, Inbound, PlayerInput};
use rspigot_core::plugins::{BlockOrder, Order};
use rspigot_core::ports::{ChunkGenerator, PlayerStore, StoreError};
use rspigot_core::server::Server;
use rspigot_core::status::StatusBoard;
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::item::ItemStack;
use rspigot_domain::map_data::{MapData, MapUpdate};
use rspigot_domain::player::PlayerId;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BiomeId, BlockPos, ChunkPos, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;
use rspigot_nbt::Compound;

const AIM: Rotation = Rotation::new(0.0, 0.0);
const MAP_ID: &str = "minecraft:map_id";
const WALL: BlockPos = BlockPos::new(2, 64, 0);
const STONE: u8 = 11;
const FRAME_KIND: u8 = 1;
const PLAYER_KIND: u8 = 0;
const RED_BANNER: u8 = 10 + 14;

struct Meadow;

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
        chunk
    }

    fn spawn_point(&self, _: &Dimension) -> Vec3 {
        Vec3::new(8.5, 64.0, 8.5)
    }
}

#[derive(Default)]
struct Atlas {
    players: Mutex<HashMap<PlayerId, Compound>>,
    files: Mutex<HashMap<String, Compound>>,
}

impl Atlas {
    fn file(&self, name: &str) -> Option<Compound> {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            let found = self.files.lock().ok().and_then(|f| f.get(name).cloned());
            if found.is_some() || std::time::Instant::now() > deadline {
                return found;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn wait(&self, name: &str, ready: impl Fn(&Compound) -> bool) -> Compound {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            let found = self.files.lock().ok().and_then(|f| f.get(name).cloned());
            match found {
                Some(file) if ready(&file) => return file,
                Some(file) if std::time::Instant::now() > deadline => return file,
                None if std::time::Instant::now() > deadline => panic!("{name} was not saved"),
                _ => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    }

    fn put(&self, name: &str, data: Compound) {
        self.files
            .lock()
            .expect("lock")
            .insert(name.to_owned(), data);
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

fn meadow_server(atlas: &Arc<Atlas>) -> Server<Link> {
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
        Arc::new(Meadow),
        StatusBoard::new(20),
    )
    .with_rng_seed(7)
    .with_player_store(Arc::clone(atlas));
    server.wait_for_spawn(Duration::from_secs(10));
    server
}

fn scene_with_atlas() -> (Scene, Arc<Atlas>) {
    let atlas = Arc::new(Atlas::default());
    let server = meadow_server(&atlas);
    let mut s = scene_on(server, &["Alice"], Vec::new());
    console(&mut s, "gamemode survival Alice");
    s.players[0].link.take();
    (s, atlas)
}

fn console(s: &mut Scene, line: &str) {
    s.server.handle(Inbound::Console(line.to_owned()));
    s.server.deliver();
}

fn use_item(s: &mut Scene) {
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItem {
            hand: Hand::Main,
            sequence: 1,
            rotation: AIM,
        },
    );
    s.tick(1);
}

fn filled_maps(events: &[ClientEvent]) -> Vec<ItemStack> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::InventorySlot { stack: Some(s), .. }
                if s.item.kind() == Some(ItemKind::FilledMap) =>
            {
                Some(s.clone())
            }
            _ => None,
        })
        .collect()
}

fn updates(events: &[ClientEvent]) -> Vec<MapUpdate> {
    events
        .iter()
        .filter_map(|event| match event {
            ClientEvent::MapData(update) => Some(update.clone()),
            _ => None,
        })
        .collect()
}

fn image(updates: &[MapUpdate]) -> Vec<u8> {
    let mut colors = vec![0u8; 16384];
    for update in updates {
        if let Some(patch) = &update.patch {
            for row in 0..usize::from(patch.height) {
                for column in 0..usize::from(patch.width) {
                    let at = (usize::from(patch.start_y) + row) * 128
                        + usize::from(patch.start_x)
                        + column;
                    colors[at] = patch.colors[row * usize::from(patch.width) + column];
                }
            }
        }
    }
    colors
}

fn filled_stack(id: i32) -> ItemStack {
    let mut stack = ItemStack::new(ItemKind::FilledMap.id(), NonZeroU8::MIN);
    stack.components.insert(MAP_ID, id);
    stack
}

fn put_in_slot(s: &mut Scene, slot: i16, stack: ItemStack) {
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

#[test]
fn using_an_empty_map_makes_a_filled_map_with_an_id_from_the_index() {
    let (mut s, atlas) = scene_with_atlas();
    console(&mut s, "give Alice minecraft:map 2");
    s.players[0].link.take();

    use_item(&mut s);

    let events = s.players[0].link.take();
    let maps = filled_maps(&events);
    assert_eq!(maps.len(), 1);
    assert_eq!(maps[0].components.get_i32(MAP_ID), Some(0));
    let index = atlas.file("maps/last_id").expect("the index is written");
    assert_eq!(index.get_i32("map"), Some(0));
    let saved = atlas.file("maps/0").expect("the map is written");
    let data = MapData::from_nbt(&saved).expect("a vanilla map");
    assert_eq!(data.scale, 0);
    assert_eq!(data.dimension, "minecraft:overworld");
    assert_eq!((data.center_x, data.center_z), (0, 0));
    assert!(data.tracking_position && !data.unlimited_tracking && !data.locked);

    use_item(&mut s);
    let again = filled_maps(&s.players[0].link.take());
    assert_eq!(
        again.last().and_then(|m| m.components.get_i32(MAP_ID)),
        Some(1)
    );
    assert_eq!(
        atlas.file("maps/last_id").and_then(|i| i.get_i32("map")),
        Some(1)
    );
}

#[test]
fn an_empty_map_is_centered_on_the_grid_around_where_it_was_used() {
    let (mut s, atlas) = scene_with_atlas();
    console(&mut s, "tp Alice 300 64 -200");
    s.tick(2);
    console(&mut s, "give Alice minecraft:map 1");
    use_item(&mut s);
    let saved = atlas.file("maps/0").expect("the map is written");
    assert_eq!(saved.get_i32("xCenter"), Some(256));
    assert_eq!(saved.get_i32("zCenter"), Some(-256));
}

#[test]
fn a_held_map_is_drawn_from_the_terrain_and_sent_with_the_player_marker() {
    let (mut s, _atlas) = scene_with_atlas();
    console(&mut s, "give Alice minecraft:map 1");
    use_item(&mut s);

    s.tick(20);

    let sent = updates(&s.players[0].link.take());
    assert!(!sent.is_empty());
    let first = &sent[0];
    assert_eq!(first.map_id, 0);
    assert!(!first.locked);
    let markers = first
        .decorations
        .as_ref()
        .expect("the first send has decorations");
    assert!(markers.iter().any(|m| m.kind == PLAYER_KIND));
    let colors = image(&sent);
    let painted: Vec<u8> = colors.iter().copied().filter(|c| *c != 0).collect();
    assert!(painted.len() > 5_000, "painted {}", painted.len());
    assert!(painted.iter().all(|c| c >> 2 == STONE));
    let later = &sent[1..];
    assert!(
        later
            .iter()
            .all(|u| u.patch.as_ref().is_none_or(|p| p.colors.len() < 16384))
    );
}

#[test]
fn a_map_that_is_not_held_is_sent_but_not_redrawn() {
    let (mut s, atlas) = scene_with_atlas();
    let mut stored = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
    stored.set_color(5, 6, 44);
    atlas.put("maps/7", stored.to_nbt());
    put_in_slot(&mut s, 9, filled_stack(7));
    s.players[0].link.take();

    s.tick(20);

    let colors = image(&updates(&s.players[0].link.take()));
    assert_eq!(colors[5 + 6 * 128], 44);
    assert_eq!(colors.iter().filter(|c| **c != 0).count(), 1);
}

#[test]
fn maps_are_persisted_in_the_vanilla_layout_and_reloaded() {
    let (mut s, atlas) = scene_with_atlas();
    console(&mut s, "give Alice minecraft:map 1");
    use_item(&mut s);
    s.tick(20);
    s.server.save_maps();
    let saved = atlas.wait("maps/0", |file| {
        file.get_byte_array("colors")
            .is_some_and(|c| c.iter().any(|b| *b != 0))
    });
    assert_eq!(saved.get_byte_array("colors").map(<[u8]>::len), Some(16384));
    let drawn = saved
        .get_byte_array("colors")
        .expect("colors")
        .iter()
        .filter(|c| **c != 0)
        .count();
    assert!(drawn > 5_000, "drawn {drawn}");
    assert_eq!(saved.get_str("dimension"), Some("minecraft:overworld"));
    assert!(saved.get_i32("xCenter").is_some() && saved.get_i32("zCenter").is_some());

    let server = meadow_server(&atlas);
    let mut again = scene_on(server, &["Alice"], Vec::new());
    put_in_slot(&mut again, 9, filled_stack(0));
    again.players[0].link.take();
    again.tick(5);
    let reloaded = image(&updates(&again.players[0].link.take()));
    assert_eq!(reloaded.iter().filter(|c| **c != 0).count(), drawn);
    console(&mut again, "give Alice minecraft:map 1");
    use_item(&mut again);
    let next = filled_maps(&again.players[0].link.take());
    assert_eq!(
        next.last().and_then(|m| m.components.get_i32(MAP_ID)),
        Some(1)
    );
}

fn framed_map(s: &mut Scene, atlas: &Atlas) -> rspigot_domain::EntityId {
    console(s, "tp Alice 2.5 64 3.5");
    s.tick(2);
    stream_chunk_of(s, WALL);
    s.act(Order::Block {
        pos: WALL,
        order: BlockOrder::SetWith {
            kind: "minecraft:stone".to_owned(),
            properties: Vec::new(),
        },
    });
    console(s, "give Alice minecraft:item_frame 1");
    let alice = s.players[0].clone();
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: WALL,
            face: Direction::North,
            cursor: Vec3::new(0.5, 0.5, 0.0),
            inside_block: false,
            sequence: 1,
        },
    );
    s.tick(1);
    let frame = s.server.entities_of(Kind::ItemFrame)[0].id;
    let mut stored = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
    stored.set_color(1, 1, 8);
    atlas.put("maps/3", stored.to_nbt());
    put_in_slot(s, 36, filled_stack(3));
    send_input(
        &mut s.server,
        &alice,
        PlayerInput::Interact {
            target: frame,
            hand: Hand::Main,
            sneaking: false,
            location: Vec3::new(0.0, 0.0, 0.0),
        },
    );
    s.tick(1);
    frame
}

#[test]
fn a_map_in_an_item_frame_shows_the_frame_to_a_nearby_player() {
    let (mut s, atlas) = scene_with_atlas();
    let frame = framed_map(&mut s, &atlas);
    s.players[0].link.take();

    s.tick(25);

    let sent = updates(&s.players[0].link.take());
    let with_frame = sent
        .iter()
        .filter_map(|u| u.decorations.as_ref())
        .flatten()
        .find(|d| d.kind == FRAME_KIND)
        .expect("a frame decoration is sent");
    assert_eq!((with_frame.x, with_frame.y), (4, -1));
    assert_eq!(with_frame.rot, 8);
    assert_eq!(image(&sent)[1 + 128], 8);
    s.server.save_maps();
    let saved = atlas.wait("maps/3", |file| file.get_list("frames").is_some());
    let frames = saved.get_list("frames").expect("the frame is persisted");
    assert_eq!(frames.len(), 1);
    let entry = frames[0].as_compound().expect("compound");
    assert_eq!(entry.get_i32("entity_id"), Some(frame.0));
    assert_eq!(entry.get_i32("rotation"), Some(180));

    send_input(
        &mut s.server,
        &s.players[0].clone(),
        PlayerInput::Attack(frame),
    );
    s.tick(25);
    s.server.save_maps();
    let after = atlas.wait("maps/3", |file| file.get_list("frames").is_none());
    assert!(after.get_list("frames").is_none_or(<[_]>::is_empty));
}

#[test]
fn a_banner_clicked_with_a_map_is_marked_on_it_and_persisted() {
    let (mut s, atlas) = scene_with_atlas();
    console(&mut s, "give Alice minecraft:map 1");
    use_item(&mut s);
    let banner = BlockPos::new(10, 64, 12);
    s.act(Order::Block {
        pos: banner,
        order: BlockOrder::SetWith {
            kind: "minecraft:red_banner".to_owned(),
            properties: Vec::new(),
        },
    });
    s.players[0].link.take();
    let alice = s.players[0].clone();

    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: banner,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 0.5, 0.5),
            inside_block: false,
            sequence: 2,
        },
    );
    s.tick(10);

    let sent = updates(&s.players[0].link.take());
    let marker = sent
        .iter()
        .filter_map(|u| u.decorations.as_ref())
        .flatten()
        .find(|d| d.kind == RED_BANNER)
        .expect("the banner marker is sent");
    assert_eq!((marker.x, marker.y), (21, 25));
    assert_eq!(marker.rot, 8);
    s.server.save_maps();
    let saved = atlas.wait("maps/0", |file| file.get_list("banners").is_some());
    assert_eq!(saved.get_list("banners").map(<[_]>::len), Some(1));
    let data = MapData::from_nbt(&saved).expect("loads");
    assert_eq!(data.banners()[0].color, 14);
    assert_eq!(data.banners()[0].pos, banner);

    send_input(
        &mut s.server,
        &alice,
        PlayerInput::UseItemOn {
            hand: Hand::Main,
            pos: banner,
            face: Direction::Up,
            cursor: Vec3::new(0.5, 0.5, 0.5),
            inside_block: false,
            sequence: 3,
        },
    );
    s.tick(10);
    s.server.save_maps();
    let cleared = atlas.wait("maps/0", |file| file.get_list("banners").is_none());
    assert!(cleared.get_list("banners").is_none_or(<[_]>::is_empty));
}
