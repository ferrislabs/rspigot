use std::num::NonZeroU8;

use rspigot_blocks::BlockKind;
use rspigot_data::map_colors::decoration_type_id;
use rspigot_domain::cartography::{self, MapFacts};
use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::dye::DYE_COLORS;
use rspigot_domain::entity::hanging;
use rspigot_domain::entity::save::component_from_tag;
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::item::{ItemStack, OFFHAND_SLOT};
use rspigot_domain::map_data::{
    Carrier, FrameRef, LevelRef, MapBanner, MapData, MapIndex, StackDecoration,
};
use rspigot_domain::map_render::{MapTerrain, update_map};
use rspigot_domain::player::PlayerId;
use rspigot_domain::world::Dimension;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Identifier};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::ItemKind;
use rustc_hash::FxHashMap;

use crate::bow::hand_slot;
use crate::chunks::ChunkManager;
use crate::events::{ClientEvent, Hand};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const MAP_ID: &str = "minecraft:map_id";
const MAP_DECORATIONS: &str = "minecraft:map_decorations";
const INDEX_FILE: &str = "maps/last_id";
const TAKE_RESULT: &str = "minecraft:ui.cartography_table.take_result";
const FRAME_PERIOD: u64 = 10;
const AUTOSAVE_PERIOD: u64 = 6000;

#[derive(Default)]
pub(crate) struct MapRuntime {
    index: Option<MapIndex>,
    maps: FxHashMap<i32, MapData>,
}

struct WorldTerrain<'a> {
    chunks: &'a ChunkManager,
    dimension: &'a Dimension,
}

fn banner_color(name: &str) -> Option<u8> {
    let bare = name.strip_prefix("minecraft:").unwrap_or(name);
    let color = bare
        .strip_suffix("_wall_banner")
        .or_else(|| bare.strip_suffix("_banner"))?;
    DYE_COLORS
        .iter()
        .position(|dye| dye.name == color)
        .map(|index| index as u8)
}

fn banner_in(chunks: &ChunkManager, pos: BlockPos) -> Option<MapBanner> {
    let state = chunks.block(pos)?;
    let color = banner_color(state.kind()?.name())?;
    let name = chunks
        .get(pos.chunk())
        .and_then(|column| column.block_entity(pos))
        .and_then(|entity| entity.data.get("CustomName"))
        .and_then(component_from_tag);
    Some(MapBanner { pos, color, name })
}

impl MapTerrain for WorldTerrain<'_> {
    fn has_ceiling(&self) -> bool {
        self.dimension.has_ceiling
    }

    fn min_y(&self) -> i32 {
        self.dimension.height.min_y
    }

    fn is_loaded(&self, x: i32, z: i32) -> bool {
        self.chunks.get(BlockPos::new(x, 0, z).chunk()).is_some()
    }

    fn surface(&self, x: i32, z: i32) -> i32 {
        let min = self.dimension.height.min_y;
        self.chunks
            .get(BlockPos::new(x, 0, z).chunk())
            .and_then(|column: &std::sync::Arc<ChunkColumn>| {
                column.highest_block((x & 15) as usize, (z & 15) as usize, |s| !s.is_air())
            })
            .map_or(min, |y| y + 1)
    }

    fn state(&self, x: i32, y: i32, z: i32) -> BlockStateId {
        self.chunks
            .block(BlockPos::new(x, y, z))
            .unwrap_or_else(|| BlockKind::Air.default_state())
    }

    fn banner(&self, pos: BlockPos) -> Option<MapBanner> {
        banner_in(self.chunks, pos)
    }
}

fn map_id_of(stack: &ItemStack) -> Option<i32> {
    if !cartography::is_map_item(stack) {
        return None;
    }
    stack.components.get_i32(MAP_ID)
}

fn stack_decorations(stack: &ItemStack) -> Vec<StackDecoration> {
    let Some(entries) = stack.components.get_compound(MAP_DECORATIONS) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|(key, tag)| {
            let entry = tag.as_compound()?;
            Some(StackDecoration {
                key: key.to_owned(),
                kind: decoration_type_id(entry.get_str("type")?)?,
                x: entry.get_f64("x")?,
                z: entry.get_f64("z")?,
                rotation: entry.get_f64("rotation").unwrap_or(0.0),
            })
        })
        .collect()
}

fn direction_value(face: rspigot_blocks::Face) -> i32 {
    use rspigot_blocks::Face;
    match face {
        Face::South => 0,
        Face::West => 1,
        Face::North => 2,
        Face::East => 3,
        Face::Up | Face::Down => -1,
    }
}

struct FrameView {
    entity: EntityId,
    pos: BlockPos,
    direction: i32,
    map: i32,
    extra: Vec<StackDecoration>,
}

impl<L: ClientLink> Server<L> {
    fn load_map(&mut self, id: i32) -> bool {
        if self.maps.maps.contains_key(&id) {
            return true;
        }
        let Some(io) = &self.player_io else {
            return false;
        };
        match io.load_saved_data(&format!("maps/{id}")) {
            Ok(Some(nbt)) => match MapData::from_nbt(&nbt) {
                Some(data) => {
                    self.maps.maps.insert(id, data);
                    true
                }
                None => {
                    tracing::warn!("Ignoring unreadable map {id}");
                    false
                }
            },
            Ok(None) => false,
            Err(error) => {
                tracing::warn!("Failed to load map {id}: {error}");
                false
            }
        }
    }

    fn next_map_id(&mut self) -> i32 {
        let mut index = self.maps.index.take().unwrap_or_else(|| {
            let stored = self
                .player_io
                .as_ref()
                .and_then(|io| io.load_saved_data(INDEX_FILE).ok().flatten());
            stored
                .as_ref()
                .map_or_else(MapIndex::default, MapIndex::from_nbt)
        });
        let id = index.next_id();
        if let Some(io) = &self.player_io {
            io.save_saved_data(INDEX_FILE, index.to_nbt());
        }
        self.maps.index = Some(index);
        id
    }

    pub(crate) fn map_facts(&mut self, id: i32) -> Option<MapFacts> {
        if !self.load_map(id) {
            return None;
        }
        self.maps.maps.get(&id).map(MapFacts::of)
    }

    pub(crate) fn new_map(&mut self, data: MapData) -> i32 {
        let id = self.next_map_id();
        self.maps.maps.insert(id, data);
        self.save_maps();
        id
    }

    pub(crate) fn derived_map(
        &mut self,
        from: i32,
        derive: impl FnOnce(&MapData) -> Option<MapData>,
    ) -> Option<i32> {
        if !self.load_map(from) {
            return None;
        }
        let data = derive(self.maps.maps.get(&from)?)?;
        Some(self.new_map(data))
    }

    pub fn save_maps(&mut self) {
        let Some(io) = &self.player_io else {
            return;
        };
        for (id, data) in &mut self.maps.maps {
            if data.is_dirty() {
                io.save_saved_data(&format!("maps/{id}"), data.to_nbt());
                data.mark_saved();
            }
        }
    }

    pub(crate) fn use_empty_map(&mut self, id: PlayerId, hand: Hand) -> bool {
        let Some(held) = self
            .held(id, hand)
            .filter(|stack| stack.item.kind() == Some(ItemKind::Map))
        else {
            return false;
        };
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let position = player.position;
        let x = f64::from(rspigot_domain::mth::floor(position.x));
        let z = f64::from(rspigot_domain::mth::floor(position.z));
        self.consume_held(id, hand, 1);
        self.count_used(id, &held);
        self.play_sound(None, position, TAKE_RESULT, SoundSource::Players, 1.0, 1.0);
        let map_id = self.next_map_id();
        let data = MapData::fresh(x, z, 0, true, false, self.world.dimension.key.as_str());
        self.maps.maps.insert(map_id, data);
        let mut filled = ItemStack::new(ItemKind::FilledMap.id(), NonZeroU8::MIN);
        filled.components.insert(MAP_ID, map_id);
        if self.held(id, hand).is_none() {
            self.set_held(id, hand, Some(filled));
            self.refresh_hands(id);
        } else {
            self.give_or_drop(id, filled);
        }
        self.player_swing(id, hand, false);
        self.save_maps();
        true
    }

    pub(crate) fn use_map_on(&mut self, id: PlayerId, hand: Hand, pos: BlockPos) -> bool {
        let Some(map_id) = self.held(id, hand).as_ref().and_then(map_id_of) else {
            return false;
        };
        let Some(banner) = banner_in(&self.world.chunks, pos) else {
            return false;
        };
        if self.load_map(map_id)
            && let Some(data) = self.maps.maps.get_mut(&map_id)
        {
            let (x, z) = (f64::from(pos.x) + 0.5, f64::from(pos.z) + 0.5);
            data.toggle_banner(banner, x, z);
        }
        true
    }

    pub(crate) fn tick_maps(&mut self) {
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        let mut out: Vec<(PlayerId, ClientEvent)> = Vec::new();
        for id in ids {
            self.tick_player_maps(id, &mut out);
        }
        if self.tick.is_multiple_of(FRAME_PERIOD) {
            self.tick_frame_maps(&mut out);
        }
        for (id, event) in out {
            self.push(Audience::One(id), event);
        }
        if self.tick.is_multiple_of(AUTOSAVE_PERIOD) {
            self.save_maps();
        }
    }

    fn level_ref(&self) -> LevelRef {
        LevelRef {
            nether: self.world.dimension.key == Identifier::minecraft("the_nether"),
            game_time: self.world.level.game_time,
        }
    }

    fn tick_player_maps(&mut self, id: PlayerId, out: &mut Vec<(PlayerId, ClientEvent)>) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let carried: Vec<(usize, i32, Vec<StackDecoration>)> = player
            .inventory
            .slots()
            .iter()
            .enumerate()
            .filter_map(|(slot, stack)| {
                let stack = stack.as_ref()?;
                Some((slot, map_id_of(stack)?, stack_decorations(stack)))
            })
            .collect();
        let hands = [hand_slot(player, Hand::Main), OFFHAND_SLOT];
        for (_, map_id, _) in &carried {
            self.load_map(*map_id);
        }
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let name = player.profile.name.clone();
        let dimension = player.dim.clone();
        let (px, pz, yaw) = (player.position.x, player.position.z, player.rotation.yaw);
        let level = self.level_ref();
        let terrain = WorldTerrain {
            chunks: &self.world.chunks,
            dimension: &self.world.dimension,
        };
        let known: Vec<i32> = self.maps.maps.keys().copied().collect();
        for map_id in known {
            let carries = carried.iter().any(|(_, m, _)| *m == map_id);
            let Some(data) = self.maps.maps.get_mut(&map_id) else {
                continue;
            };
            if !carries && !data.has_decoration(&name) {
                continue;
            }
            let who = Carrier {
                id,
                name: &name,
                x: px,
                z: pz,
                yaw,
                dimension: dimension.as_str(),
                carries,
            };
            let extra = carried
                .iter()
                .find(|(_, m, _)| *m == map_id)
                .map(|(_, _, extra)| extra.as_slice())
                .unwrap_or_default();
            data.tick_carried_by(&who, None, &level, extra);
            if !carries {
                continue;
            }
            let in_hand = carried
                .iter()
                .filter(|(slot, m, _)| *m == map_id && hands.contains(slot))
                .count();
            if !data.locked && data.dimension == dimension.as_str() {
                for _ in 0..in_hand {
                    update_map(data, &terrain, id, px, pz);
                }
            }
            if let Some(update) = data.next_update(id, map_id) {
                out.push((id, ClientEvent::MapData(update)));
            }
        }
    }

    fn frame_views(&self) -> Vec<FrameView> {
        self.entities
            .store
            .iter()
            .filter(|entity| !entity.base.is_removed())
            .filter(|entity| matches!(entity.base.kind, Kind::ItemFrame | Kind::GlowItemFrame))
            .filter_map(|entity| {
                let EntityState::Hanging(state) = &entity.state else {
                    return None;
                };
                let stack = hanging::item(&entity.base)?;
                Some(FrameView {
                    entity: entity.base.id,
                    pos: state.pos,
                    direction: direction_value(hanging::facing(&entity.base)),
                    map: map_id_of(stack)?,
                    extra: stack_decorations(stack),
                })
            })
            .collect()
    }

    fn drop_stale_frames(&mut self) {
        let stale: Vec<(i32, BlockPos, i32)> = self
            .maps
            .maps
            .iter()
            .flat_map(|(map_id, data)| {
                data.frames()
                    .iter()
                    .map(move |frame| (*map_id, frame.pos, frame.entity_id))
            })
            .filter(|(map_id, _, entity)| {
                !self
                    .entities
                    .store
                    .get(EntityId(*entity))
                    .is_some_and(|found: &Entity| {
                        !found.base.is_removed()
                            && hanging::item(&found.base).and_then(map_id_of) == Some(*map_id)
                    })
            })
            .collect();
        for (map_id, pos, entity) in stale {
            if let Some(data) = self.maps.maps.get_mut(&map_id) {
                data.removed_from_frame(pos, entity);
            }
        }
    }

    fn tick_frame_maps(&mut self, out: &mut Vec<(PlayerId, ClientEvent)>) {
        let frames = self.frame_views();
        for frame in &frames {
            self.load_map(frame.map);
        }
        self.drop_stale_frames();
        let level = self.level_ref();
        for frame in frames {
            let Some(data) = self.maps.maps.get_mut(&frame.map) else {
                continue;
            };
            let at = FrameRef {
                pos: frame.pos,
                direction: frame.direction,
                entity_id: frame.entity.0,
            };
            for (id, player) in &self.players {
                if !player.tracked.contains(&frame.entity) {
                    continue;
                }
                let carries = player
                    .inventory
                    .slots()
                    .iter()
                    .flatten()
                    .any(|stack| map_id_of(stack) == Some(frame.map));
                let who = Carrier {
                    id: *id,
                    name: &player.profile.name,
                    x: player.position.x,
                    z: player.position.z,
                    yaw: player.rotation.yaw,
                    dimension: player.dim.as_str(),
                    carries,
                };
                data.tick_carried_by(&who, Some(&at), &level, &frame.extra);
                if let Some(update) = data.next_update(*id, frame.map) {
                    out.push((*id, ClientEvent::MapData(update)));
                }
            }
        }
    }
}
