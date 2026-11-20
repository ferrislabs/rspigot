use rspigot_data::map_colors::{
    BANNER_BASE, FRAME, PLAYER, PLAYER_OFF_LIMITS, PLAYER_OFF_MAP, decoration_type,
};
use rspigot_nbt::{Compound, Tag};

use crate::BlockPos;
use crate::dye::DYE_COLORS;
use crate::entity::save::{component_from_tag, component_to_tag};
use crate::player::PlayerId;
use crate::text::Component;

pub const MAP_SIZE: usize = 128;
pub const MAP_COLORS: usize = MAP_SIZE * MAP_SIZE;
pub const MAX_SCALE: u8 = 4;
pub const TRACKED_DECORATION_LIMIT: i32 = 256;
const HALF: i32 = 64;
const DECORATION_SYNC_PERIOD: i32 = 5;

#[derive(Debug, Clone, PartialEq)]
pub struct MapDecoration {
    pub kind: u8,
    pub x: i8,
    pub y: i8,
    pub rot: i8,
    pub name: Option<Component>,
}

impl MapDecoration {
    pub fn new(kind: u8, x: i8, y: i8, rot: i8, name: Option<Component>) -> Self {
        Self {
            kind,
            x,
            y,
            rot: rot & 15,
            name,
        }
    }

    fn tracks(&self) -> bool {
        decoration_type(self.kind).is_some_and(|kind| kind.track_count)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapBanner {
    pub pos: BlockPos,
    pub color: u8,
    pub name: Option<Component>,
}

impl MapBanner {
    pub fn id(&self) -> String {
        format!("banner-{},{},{}", self.pos.x, self.pos.y, self.pos.z)
    }

    pub fn decoration_kind(&self) -> u8 {
        BANNER_BASE + self.color.min(15)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapFrame {
    pub pos: BlockPos,
    pub rotation: i32,
    pub entity_id: i32,
}

impl MapFrame {
    pub fn id(&self) -> String {
        frame_id(self.pos)
    }
}

fn frame_id(pos: BlockPos) -> String {
    format!("frame-{},{},{}", pos.x, pos.y, pos.z)
}

fn frame_key(entity_id: i32) -> String {
    format!("frame-{entity_id}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapPatch {
    pub start_x: u8,
    pub start_y: u8,
    pub width: u8,
    pub height: u8,
    pub colors: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MapUpdate {
    pub map_id: i32,
    pub scale: u8,
    pub locked: bool,
    pub decorations: Option<Vec<MapDecoration>>,
    pub patch: Option<MapPatch>,
}

#[derive(Debug, Clone, Copy)]
pub struct Carrier<'a> {
    pub id: PlayerId,
    pub name: &'a str,
    pub x: f64,
    pub z: f64,
    pub yaw: f32,
    pub dimension: &'a str,
    pub carries: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct FrameRef {
    pub pos: BlockPos,
    pub direction: i32,
    pub entity_id: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct LevelRef {
    pub nether: bool,
    pub game_time: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StackDecoration {
    pub key: String,
    pub kind: u8,
    pub x: f64,
    pub z: f64,
    pub rotation: f64,
}

#[derive(Debug, Clone, Copy)]
struct Spot {
    x: f64,
    z: f64,
    rotation: f64,
}

#[derive(Debug, Clone)]
struct Holder {
    player: PlayerId,
    dirty_data: bool,
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
    dirty_decorations: bool,
    tick: i32,
    step: i32,
}

impl Holder {
    fn new(player: PlayerId) -> Self {
        Self {
            player,
            dirty_data: true,
            min_x: 0,
            min_y: 0,
            max_x: 127,
            max_y: 127,
            dirty_decorations: true,
            tick: 0,
            step: 0,
        }
    }

    fn mark_colors(&mut self, x: i32, y: i32) {
        if self.dirty_data {
            self.min_x = self.min_x.min(x);
            self.min_y = self.min_y.min(y);
            self.max_x = self.max_x.max(x);
            self.max_y = self.max_y.max(y);
        } else {
            self.dirty_data = true;
            self.min_x = x;
            self.min_y = y;
            self.max_x = x;
            self.max_y = y;
        }
    }
}

#[derive(Debug, Clone)]
pub struct MapData {
    pub scale: u8,
    pub dimension: String,
    pub tracking_position: bool,
    pub unlimited_tracking: bool,
    pub locked: bool,
    pub center_x: i32,
    pub center_z: i32,
    colors: Vec<u8>,
    banners: Vec<MapBanner>,
    frames: Vec<MapFrame>,
    decorations: Vec<(String, MapDecoration)>,
    tracked: i32,
    holders: Vec<Holder>,
    dirty: bool,
}

impl MapData {
    pub fn fresh(
        x: f64,
        z: f64,
        scale: u8,
        tracking_position: bool,
        unlimited_tracking: bool,
        dimension: &str,
    ) -> Self {
        let span = MAP_SIZE as i32 * (1 << scale);
        let cell = |value: f64| crate::mth::floor((value + f64::from(HALF)) / f64::from(span));
        let center_x = cell(x) * span + span / 2 - HALF;
        let center_z = cell(z) * span + span / 2 - HALF;
        Self {
            scale,
            dimension: dimension.to_owned(),
            tracking_position,
            unlimited_tracking,
            locked: false,
            center_x,
            center_z,
            colors: vec![0; MAP_COLORS],
            banners: Vec::new(),
            frames: Vec::new(),
            decorations: Vec::new(),
            tracked: 0,
            holders: Vec::new(),
            dirty: true,
        }
    }

    pub fn colors(&self) -> &[u8] {
        &self.colors
    }

    pub fn banners(&self) -> &[MapBanner] {
        &self.banners
    }

    pub fn frames(&self) -> &[MapFrame] {
        &self.frames
    }

    pub fn decorations(&self) -> impl Iterator<Item = &MapDecoration> {
        self.decorations.iter().map(|(_, decoration)| decoration)
    }

    pub fn tracked_count_over(&self, limit: i32) -> bool {
        self.tracked > limit
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn has_decoration(&self, key: &str) -> bool {
        self.decorations.iter().any(|(k, _)| k == key)
    }

    pub fn step_of(&mut self, player: PlayerId) -> i32 {
        let holder = self.holder_mut(player);
        holder.step += 1;
        holder.step
    }

    fn holder_mut(&mut self, player: PlayerId) -> &mut Holder {
        let index = match self.holders.iter().position(|h| h.player == player) {
            Some(index) => index,
            None => {
                self.holders.push(Holder::new(player));
                self.holders.len() - 1
            }
        };
        &mut self.holders[index]
    }

    pub fn update_color(&mut self, x: i32, y: i32, color: u8) -> bool {
        let index = (x + y * MAP_SIZE as i32) as usize;
        if self.colors[index] == color {
            return false;
        }
        self.set_color(x, y, color);
        true
    }

    pub fn set_color(&mut self, x: i32, y: i32, color: u8) {
        self.colors[(x + y * MAP_SIZE as i32) as usize] = color;
        for holder in &mut self.holders {
            holder.mark_colors(x, y);
        }
        self.dirty = true;
    }

    fn mark_decorations(&mut self) {
        for holder in &mut self.holders {
            holder.dirty_decorations = true;
        }
    }

    fn remove_decoration(&mut self, key: &str) {
        if let Some(index) = self.decorations.iter().position(|(k, _)| k == key) {
            let (_, removed) = self.decorations.remove(index);
            if removed.tracks() {
                self.tracked -= 1;
            }
            self.mark_decorations();
        }
    }

    pub fn tick_carried_by(
        &mut self,
        who: &Carrier<'_>,
        frame: Option<&FrameRef>,
        level: &LevelRef,
        from_stack: &[StackDecoration],
    ) {
        self.holder_mut(who.id);
        if !who.carries {
            self.remove_decoration(who.name);
        }
        if !who.carries && frame.is_none() {
            self.holders.retain(|h| h.player != who.id);
            self.remove_decoration(who.name);
        } else if frame.is_none() && who.dimension == self.dimension && self.tracking_position {
            self.add_decoration(
                PLAYER,
                level,
                who.name,
                Spot {
                    x: who.x,
                    z: who.z,
                    rotation: f64::from(who.yaw),
                },
                None,
            );
        }
        if let Some(frame) = frame
            && self.tracking_position
        {
            self.track_frame(frame, level);
        }
        let missing: Vec<&StackDecoration> = from_stack
            .iter()
            .filter(|entry| !self.decorations.iter().any(|(k, _)| *k == entry.key))
            .collect();
        for entry in missing {
            self.add_decoration(
                entry.kind,
                level,
                &entry.key,
                Spot {
                    x: entry.x,
                    z: entry.z,
                    rotation: entry.rotation,
                },
                None,
            );
        }
    }

    fn track_frame(&mut self, frame: &FrameRef, level: &LevelRef) {
        let id = frame_id(frame.pos);
        let existing = self.frames.iter().find(|f| f.id() == id).copied();
        if let Some(old) = existing
            && old.entity_id != frame.entity_id
        {
            self.remove_decoration(&frame_key(old.entity_id));
        }
        let rotation = frame.direction * 90;
        let tracked = MapFrame {
            pos: frame.pos,
            rotation,
            entity_id: frame.entity_id,
        };
        self.add_decoration(
            FRAME,
            level,
            &frame_key(frame.entity_id),
            Spot {
                x: f64::from(frame.pos.x),
                z: f64::from(frame.pos.z),
                rotation: f64::from(rotation),
            },
            None,
        );
        self.frames.retain(|f| f.id() != id);
        self.frames.push(tracked);
        if existing != Some(tracked) {
            self.dirty = true;
        }
    }

    fn add_decoration(
        &mut self,
        kind: u8,
        level: &LevelRef,
        key: &str,
        spot: Spot,
        name: Option<Component>,
    ) {
        let Spot { x, z, rotation } = spot;
        let span = f64::from(1 << self.scale);
        let fx = ((x - f64::from(self.center_x)) / span) as f32;
        let fz = ((z - f64::from(self.center_z)) / span) as f32;
        let Some((placed, rot)) = self.locate(kind, level, rotation, fx, fz) else {
            self.remove_decoration(key);
            return;
        };
        let decoration = MapDecoration::new(placed, clamp(fx), clamp(fz), rot, name);
        match self.decorations.iter().position(|(k, _)| k == key) {
            Some(index) => {
                if self.decorations[index].1 == decoration {
                    return;
                }
                if self.decorations[index].1.tracks() {
                    self.tracked -= 1;
                }
                self.decorations[index].1 = decoration.clone();
            }
            None => self.decorations.push((key.to_owned(), decoration.clone())),
        }
        if decoration.tracks() {
            self.tracked += 1;
        }
        self.mark_decorations();
    }

    fn locate(
        &self,
        kind: u8,
        level: &LevelRef,
        rotation: f64,
        fx: f32,
        fz: f32,
    ) -> Option<(u8, i8)> {
        let rot = self.rotation(level, rotation);
        if kind == PLAYER {
            if inside(fx, fz) {
                return Some((PLAYER, rot));
            }
            return self.outside_kind(fx, fz).map(|kind| (kind, rot));
        }
        (inside(fx, fz) || self.unlimited_tracking).then_some((kind, rot))
    }

    fn outside_kind(&self, fx: f32, fz: f32) -> Option<u8> {
        if fx.abs() < 320.0 && fz.abs() < 320.0 {
            Some(PLAYER_OFF_MAP)
        } else if self.unlimited_tracking {
            Some(PLAYER_OFF_LIMITS)
        } else {
            None
        }
    }

    fn rotation(&self, level: &LevelRef, rotation: f64) -> i8 {
        if self.dimension == "minecraft:the_nether" && level.nether {
            let step = (level.game_time / 10) as i32;
            return (step
                .wrapping_mul(step)
                .wrapping_mul(34_187_121)
                .wrapping_add(step.wrapping_mul(121))
                >> 15
                & 15) as i8;
        }
        let shifted = if rotation < 0.0 {
            rotation - 8.0
        } else {
            rotation + 8.0
        };
        (shifted * 16.0 / 360.0) as i32 as i8
    }

    pub fn toggle_banner(&mut self, banner: MapBanner, x: f64, z: f64) -> bool {
        let span = f64::from(1 << self.scale);
        let fx = (x - f64::from(self.center_x)) / span;
        let fz = (z - f64::from(self.center_z)) / span;
        if !(-63.0..=63.0).contains(&fx) || !(-63.0..=63.0).contains(&fz) {
            return false;
        }
        let id = banner.id();
        if let Some(index) = self
            .banners
            .iter()
            .position(|b| b.id() == id && *b == banner)
        {
            self.banners.remove(index);
            self.remove_decoration(&id);
            self.dirty = true;
            return true;
        }
        if self.tracked_count_over(TRACKED_DECORATION_LIMIT) {
            return false;
        }
        self.banners.retain(|b| b.id() != id);
        let kind = banner.decoration_kind();
        let name = banner.name.clone();
        self.banners.push(banner);
        let level = LevelRef {
            nether: false,
            game_time: 0,
        };
        self.add_decoration(
            kind,
            &level,
            &id,
            Spot {
                x,
                z,
                rotation: 180.0,
            },
            name,
        );
        self.dirty = true;
        true
    }

    pub fn check_banners(
        &mut self,
        x: i32,
        z: i32,
        current: impl Fn(BlockPos) -> Option<MapBanner>,
    ) {
        let stale: Vec<String> = self
            .banners
            .iter()
            .filter(|b| b.pos.x == x && b.pos.z == z)
            .filter(|b| current(b.pos).as_ref() != Some(*b))
            .map(MapBanner::id)
            .collect();
        for id in stale {
            self.banners.retain(|b| b.id() != id);
            self.remove_decoration(&id);
            self.dirty = true;
        }
    }

    pub fn removed_from_frame(&mut self, pos: BlockPos, entity_id: i32) {
        self.remove_decoration(&frame_key(entity_id));
        let id = frame_id(pos);
        self.frames.retain(|f| f.id() != id);
        self.dirty = true;
    }

    pub fn next_update(&mut self, player: PlayerId, map_id: i32) -> Option<MapUpdate> {
        let index = self.holders.iter().position(|h| h.player == player)?;
        let patch = if self.holders[index].dirty_data {
            let holder = &mut self.holders[index];
            holder.dirty_data = false;
            let (min_x, min_y) = (holder.min_x, holder.min_y);
            let width = holder.max_x + 1 - min_x;
            let height = holder.max_y + 1 - min_y;
            let mut colors = Vec::with_capacity((width * height) as usize);
            for row in 0..height {
                for column in 0..width {
                    colors.push(self.colors[(min_x + column + (min_y + row) * 128) as usize]);
                }
            }
            Some(MapPatch {
                start_x: min_x as u8,
                start_y: min_y as u8,
                width: width as u8,
                height: height as u8,
                colors,
            })
        } else {
            None
        };
        let holder = &mut self.holders[index];
        let decorations = if holder.dirty_decorations && {
            let due = holder.tick % DECORATION_SYNC_PERIOD == 0;
            holder.tick += 1;
            due
        } {
            holder.dirty_decorations = false;
            Some(self.decorations().cloned().collect())
        } else {
            None
        };
        if patch.is_none() && decorations.is_none() {
            return None;
        }
        Some(MapUpdate {
            map_id,
            scale: self.scale,
            locked: self.locked,
            decorations,
            patch,
        })
    }

    pub fn to_nbt(&self) -> Compound {
        let mut out = Compound::new()
            .with("dimension", self.dimension.as_str())
            .with("xCenter", self.center_x)
            .with("zCenter", self.center_z);
        if self.scale != 0 {
            out.insert("scale", Tag::Byte(self.scale as i8));
        }
        out.insert("colors", Tag::ByteArray(self.colors.clone()));
        if !self.tracking_position {
            out.insert("trackingPosition", Tag::Byte(0));
        }
        if self.unlimited_tracking {
            out.insert("unlimitedTracking", Tag::Byte(1));
        }
        if self.locked {
            out.insert("locked", Tag::Byte(1));
        }
        if !self.banners.is_empty() {
            out.insert(
                "banners",
                Tag::List(self.banners.iter().map(banner_to_nbt).collect()),
            );
        }
        if !self.frames.is_empty() {
            out.insert(
                "frames",
                Tag::List(self.frames.iter().map(frame_to_nbt).collect()),
            );
        }
        out
    }

    pub fn from_nbt(data: &Compound) -> Option<Self> {
        let colors = data.get_byte_array("colors")?;
        if colors.len() != MAP_COLORS {
            return None;
        }
        let scale = data
            .get_i32("scale")
            .and_then(|s| u8::try_from(s).ok())
            .unwrap_or(0)
            .min(MAX_SCALE);
        let mut map = Self {
            scale,
            dimension: data.get_str("dimension")?.to_owned(),
            tracking_position: data.get_bool("trackingPosition").unwrap_or(true),
            unlimited_tracking: data.get_bool("unlimitedTracking").unwrap_or(false),
            locked: data.get_bool("locked").unwrap_or(false),
            center_x: data.get_i32("xCenter")?,
            center_z: data.get_i32("zCenter")?,
            colors: colors.to_vec(),
            banners: data
                .get_list("banners")
                .unwrap_or_default()
                .iter()
                .filter_map(banner_from_nbt)
                .collect(),
            frames: data
                .get_list("frames")
                .unwrap_or_default()
                .iter()
                .filter_map(frame_from_nbt)
                .collect(),
            decorations: Vec::new(),
            tracked: 0,
            holders: Vec::new(),
            dirty: false,
        };
        let level = LevelRef {
            nether: false,
            game_time: 0,
        };
        for banner in map.banners.clone() {
            let (x, z) = (f64::from(banner.pos.x) + 0.5, f64::from(banner.pos.z) + 0.5);
            map.add_decoration(
                banner.decoration_kind(),
                &level,
                &banner.id(),
                Spot {
                    x,
                    z,
                    rotation: 180.0,
                },
                banner.name.clone(),
            );
        }
        for frame in map.frames.clone() {
            map.add_decoration(
                FRAME,
                &level,
                &frame_key(frame.entity_id),
                Spot {
                    x: f64::from(frame.pos.x),
                    z: f64::from(frame.pos.z),
                    rotation: f64::from(frame.rotation),
                },
                None,
            );
        }
        Some(map)
    }
}

fn inside(x: f32, z: f32) -> bool {
    (-63.0..=63.0).contains(&x) && (-63.0..=63.0).contains(&z)
}

fn clamp(value: f32) -> i8 {
    if value <= -63.0 {
        -128
    } else if value >= 63.0 {
        127
    } else {
        (f64::from(value) * 2.0 + 0.5) as i32 as i8
    }
}

fn pos_to_nbt(pos: BlockPos) -> Tag {
    Tag::IntArray(vec![pos.x, pos.y, pos.z])
}

fn pos_from_nbt(tag: Option<&Tag>) -> Option<BlockPos> {
    match tag? {
        Tag::IntArray(values) if values.len() == 3 => {
            Some(BlockPos::new(values[0], values[1], values[2]))
        }
        _ => None,
    }
}

fn banner_to_nbt(banner: &MapBanner) -> Tag {
    let mut out = Compound::new().with("pos", pos_to_nbt(banner.pos));
    if banner.color != 0 {
        out.insert("color", DYE_COLORS[usize::from(banner.color.min(15))].name);
    }
    if let Some(name) = &banner.name {
        out.insert("name", component_to_tag(name));
    }
    Tag::Compound(out)
}

fn banner_from_nbt(tag: &Tag) -> Option<MapBanner> {
    let data = tag.as_compound()?;
    let color = data
        .get_str("color")
        .and_then(|name| DYE_COLORS.iter().position(|dye| dye.name == name))
        .unwrap_or(0) as u8;
    Some(MapBanner {
        pos: pos_from_nbt(data.get("pos"))?,
        color,
        name: data.get("name").and_then(component_from_tag),
    })
}

fn frame_to_nbt(frame: &MapFrame) -> Tag {
    Tag::Compound(
        Compound::new()
            .with("pos", pos_to_nbt(frame.pos))
            .with("rotation", frame.rotation)
            .with("entity_id", frame.entity_id),
    )
}

fn frame_from_nbt(tag: &Tag) -> Option<MapFrame> {
    let data = tag.as_compound()?;
    Some(MapFrame {
        pos: pos_from_nbt(data.get("pos"))?,
        rotation: data.get_i32("rotation")?,
        entity_id: data.get_i32("entity_id")?,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapIndex {
    pub last_id: i32,
}

impl Default for MapIndex {
    fn default() -> Self {
        Self { last_id: -1 }
    }
}

impl MapIndex {
    pub fn next_id(&mut self) -> i32 {
        self.last_id += 1;
        self.last_id
    }

    pub fn to_nbt(self) -> Compound {
        Compound::new().with("map", self.last_id)
    }

    pub fn from_nbt(data: &Compound) -> Self {
        Self {
            last_id: data.get_i32("map").unwrap_or(-1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rspigot_data::map_colors::{FRAME, PLAYER, PLAYER_OFF_LIMITS, PLAYER_OFF_MAP};

    fn player(id: u8, name: &str, x: f64, z: f64) -> Carrier<'_> {
        Carrier {
            id: PlayerId::offline(&format!("p{id}")),
            name,
            x,
            z,
            yaw: 0.0,
            dimension: "minecraft:overworld",
            carries: true,
        }
    }

    fn overworld() -> LevelRef {
        LevelRef {
            nether: false,
            game_time: 0,
        }
    }

    fn fresh(scale: u8) -> MapData {
        MapData::fresh(0.0, 0.0, scale, true, false, "minecraft:overworld")
    }

    #[test]
    fn a_fresh_map_is_centered_on_the_grid_like_vanilla() {
        let map = MapData::fresh(100.5, -30.0, 0, true, false, "minecraft:overworld");
        assert_eq!((map.center_x, map.center_z), (128, 0));
        let far = MapData::fresh(1000.0, -1000.0, 2, true, false, "minecraft:overworld");
        assert_eq!((far.center_x, far.center_z), (1216, -832));
        let origin = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:overworld");
        assert_eq!((origin.center_x, origin.center_z), (0, 0));
        assert_eq!(origin.colors().len(), 16384);
    }

    #[test]
    fn the_first_update_is_the_whole_map_then_only_changed_rows() {
        let mut map = fresh(0);
        let who = player(1, "Alex", 0.0, 0.0);
        map.tick_carried_by(&who, None, &overworld(), &[]);
        let first = map.next_update(who.id, 7).expect("first update");
        let patch = first.patch.expect("full patch");
        assert_eq!(
            (patch.start_x, patch.start_y, patch.width, patch.height),
            (0, 0, 128, 128)
        );
        assert_eq!(patch.colors.len(), 16384);
        assert!(first.decorations.is_some());
        assert_eq!(first.map_id, 7);
        assert!(map.next_update(who.id, 7).is_none());
        assert!(map.update_color(10, 20, 9));
        assert!(!map.update_color(10, 20, 9));
        assert!(map.update_color(12, 22, 9));
        let next = map.next_update(who.id, 7).expect("changed region");
        let patch = next.patch.expect("patch");
        assert_eq!(
            (patch.start_x, patch.start_y, patch.width, patch.height),
            (10, 20, 3, 3)
        );
        assert_eq!(patch.colors.len(), 9);
        assert_eq!(patch.colors[0], 9);
        assert_eq!(patch.colors[8], 9);
        assert!(next.decorations.is_none());
    }

    #[test]
    fn the_player_marker_follows_the_player_and_syncs_every_fifth_request() {
        let mut map = fresh(0);
        let mut who = player(1, "Alex", 10.0, -20.0);
        who.yaw = 90.0;
        map.tick_carried_by(&who, None, &overworld(), &[]);
        let all = map.next_update(who.id, 1).expect("update");
        let markers = all.decorations.expect("decorations");
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].kind, PLAYER);
        assert_eq!((markers[0].x, markers[0].y), (20, -39));
        assert_eq!(markers[0].rot, 4);
        who.x = 11.0;
        map.tick_carried_by(&who, None, &overworld(), &[]);
        let mut sent = 0;
        for _ in 0..5 {
            if map
                .next_update(who.id, 1)
                .is_some_and(|u| u.decorations.is_some())
            {
                sent += 1;
            }
        }
        assert_eq!(sent, 1);
    }

    #[test]
    fn a_player_far_from_the_map_edge_shows_as_off_map_then_disappears() {
        let mut map = fresh(0);
        let who = player(1, "Alex", 100.0, 0.0);
        map.tick_carried_by(&who, None, &overworld(), &[]);
        let update = map.next_update(who.id, 1).expect("update");
        let markers = update.decorations.expect("decorations");
        assert_eq!(markers[0].kind, PLAYER_OFF_MAP);
        assert_eq!(markers[0].x, 127);
        let gone = player(1, "Alex", 1000.0, 0.0);
        map.tick_carried_by(&gone, None, &overworld(), &[]);
        assert_eq!(map.decorations().count(), 0);
        let mut unlimited = MapData::fresh(0.0, 0.0, 0, true, true, "minecraft:overworld");
        unlimited.tick_carried_by(&gone, None, &overworld(), &[]);
        let marker = unlimited.decorations().next().expect("marker");
        assert_eq!(marker.kind, PLAYER_OFF_LIMITS);
    }

    #[test]
    fn a_player_in_another_dimension_or_without_the_map_has_no_marker() {
        let mut map = fresh(0);
        let mut who = player(1, "Alex", 0.0, 0.0);
        who.dimension = "minecraft:the_nether";
        map.tick_carried_by(&who, None, &overworld(), &[]);
        assert_eq!(map.decorations().count(), 0);
        let near = player(1, "Alex", 0.0, 0.0);
        map.tick_carried_by(&near, None, &overworld(), &[]);
        assert_eq!(map.decorations().count(), 1);
        let mut dropped = player(1, "Alex", 0.0, 0.0);
        dropped.carries = false;
        map.tick_carried_by(&dropped, None, &overworld(), &[]);
        assert_eq!(map.decorations().count(), 0);
        assert!(map.next_update(dropped.id, 1).is_none());
    }

    #[test]
    fn an_item_frame_adds_a_frame_decoration_facing_its_wall() {
        let mut map = fresh(0);
        let who = player(1, "Alex", 500.0, 500.0);
        let frame = FrameRef {
            pos: BlockPos::new(8, 70, -4),
            direction: 2,
            entity_id: 42,
        };
        map.tick_carried_by(&who, Some(&frame), &overworld(), &[]);
        let decorations: Vec<_> = map.decorations().collect();
        assert_eq!(decorations.len(), 1);
        assert_eq!(decorations[0].kind, FRAME);
        assert_eq!((decorations[0].x, decorations[0].y), (16, -7));
        assert_eq!(decorations[0].rot, 8);
        let saved = map.to_nbt();
        let frames = saved.get_list("frames").expect("frames");
        assert_eq!(frames.len(), 1);
        let again = MapData::from_nbt(&saved).expect("loads");
        assert_eq!(again.frames().len(), 1);
        assert_eq!(again.frames()[0].entity_id, 42);
        map.removed_from_frame(frame.pos, 42);
        assert_eq!(map.decorations().count(), 0);
        assert!(map.frames().is_empty());
    }

    #[test]
    fn banners_toggle_on_the_map_with_their_colour_and_name() {
        let mut map = fresh(0);
        let banner = MapBanner {
            pos: BlockPos::new(3, 64, 3),
            color: 14,
            name: Some(Component::text("Home")),
        };
        assert!(map.toggle_banner(banner.clone(), 3.5, 3.5));
        let marker = map.decorations().next().expect("banner marker");
        assert_eq!(marker.kind, rspigot_data::map_colors::BANNER_BASE + 14);
        assert_eq!(marker.name, Some(Component::text("Home")));
        assert_eq!(marker.rot, 8);
        assert!(map.toggle_banner(banner, 3.5, 3.5));
        assert_eq!(map.decorations().count(), 0);
        let outside = MapBanner {
            pos: BlockPos::new(900, 64, 3),
            color: 0,
            name: None,
        };
        assert!(!map.toggle_banner(outside, 900.5, 3.5));
    }

    #[test]
    fn saved_data_uses_the_vanilla_layout_and_omits_defaults() {
        let mut map = MapData::fresh(0.0, 0.0, 2, true, false, "minecraft:the_nether");
        map.set_color(0, 0, 5);
        map.set_color(127, 127, 9);
        let nbt = map.to_nbt();
        assert_eq!(nbt.get_str("dimension"), Some("minecraft:the_nether"));
        assert_eq!(nbt.get_i32("xCenter"), Some(192));
        assert_eq!(nbt.get_i32("zCenter"), Some(192));
        assert_eq!(nbt.get("scale"), Some(&Tag::Byte(2)));
        assert_eq!(nbt.get_byte_array("colors").map(<[u8]>::len), Some(16384));
        assert!(!nbt.contains_key("trackingPosition"));
        assert!(!nbt.contains_key("locked"));
        assert!(!nbt.contains_key("banners"));
        let back = MapData::from_nbt(&nbt).expect("loads");
        assert_eq!(back.scale, 2);
        assert_eq!(back.colors()[0], 5);
        assert_eq!(back.colors()[16383], 9);
        assert!(back.tracking_position);
        assert!(!back.locked);
        let mut flags = MapData::fresh(0.0, 0.0, 0, false, true, "minecraft:overworld");
        flags.locked = true;
        let nbt = flags.to_nbt();
        assert_eq!(nbt.get("trackingPosition"), Some(&Tag::Byte(0)));
        assert_eq!(nbt.get("unlimitedTracking"), Some(&Tag::Byte(1)));
        assert_eq!(nbt.get("locked"), Some(&Tag::Byte(1)));
        let back = MapData::from_nbt(&nbt).expect("loads");
        assert!(!back.tracking_position && back.unlimited_tracking && back.locked);
    }

    #[test]
    fn banners_survive_a_save_and_a_load() {
        let mut map = fresh(0);
        let banner = MapBanner {
            pos: BlockPos::new(3, 64, 3),
            color: 3,
            name: Some(Component::text("Home")),
        };
        map.toggle_banner(banner.clone(), 3.5, 3.5);
        let back = MapData::from_nbt(&map.to_nbt()).expect("loads");
        assert_eq!(back.banners(), &[banner]);
        assert_eq!(back.decorations().count(), 1);
    }

    #[test]
    fn an_index_counts_up_from_zero_and_round_trips() {
        let mut index = MapIndex::default();
        assert_eq!(index.next_id(), 0);
        assert_eq!(index.next_id(), 1);
        let back = MapIndex::from_nbt(&index.to_nbt());
        assert_eq!(back.last_id, 1);
        assert_eq!(MapIndex::from_nbt(&Compound::new()).next_id(), 0);
    }

    #[test]
    fn the_nether_rotation_wobbles_with_the_game_time() {
        let mut map = MapData::fresh(0.0, 0.0, 0, true, false, "minecraft:the_nether");
        let who = Carrier {
            dimension: "minecraft:the_nether",
            ..player(1, "Alex", 0.0, 0.0)
        };
        let level = LevelRef {
            nether: true,
            game_time: 1230,
        };
        map.tick_carried_by(&who, None, &level, &[]);
        let marker = map.decorations().next().expect("marker");
        let step = 123i32;
        let expected = ((step.wrapping_mul(step).wrapping_mul(34_187_121))
            .wrapping_add(step.wrapping_mul(121))
            >> 15)
            & 15;
        assert_eq!(i32::from(marker.rot), expected);
    }
}
