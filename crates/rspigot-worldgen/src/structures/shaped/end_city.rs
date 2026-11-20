use serde_json::json;

use super::start;
use super::tpl::{Mirror, Tpl, set_loot, transform};
use crate::legacy::JavaRandom;
use crate::overworld::features::place::Run;
use crate::overworld::features::structure::Rotation;
use crate::structures::bbox::{Bbox, Pos};
use crate::structures::jigsaw::Ground;
use crate::structures::library::Library;
use crate::structures::placement::BiomeProbe;
use crate::structures::processors::Step;

const LOOT: &str = "minecraft:chests/end_city_treasure";

mod name {
    pub(super) const BASE_FLOOR: &str = "minecraft:end_city/base_floor";
    pub(super) const BASE_ROOF: &str = "minecraft:end_city/base_roof";
    pub(super) const SECOND_FLOOR_1: &str = "minecraft:end_city/second_floor_1";
    pub(super) const SECOND_FLOOR_2: &str = "minecraft:end_city/second_floor_2";
    pub(super) const SECOND_ROOF: &str = "minecraft:end_city/second_roof";
    pub(super) const THIRD_FLOOR_1: &str = "minecraft:end_city/third_floor_1";
    pub(super) const THIRD_FLOOR_2: &str = "minecraft:end_city/third_floor_2";
    pub(super) const THIRD_ROOF: &str = "minecraft:end_city/third_roof";
    pub(super) const TOWER_BASE: &str = "minecraft:end_city/tower_base";
    pub(super) const TOWER_PIECE: &str = "minecraft:end_city/tower_piece";
    pub(super) const TOWER_TOP: &str = "minecraft:end_city/tower_top";
    pub(super) const FAT_BASE: &str = "minecraft:end_city/fat_tower_base";
    pub(super) const FAT_MIDDLE: &str = "minecraft:end_city/fat_tower_middle";
    pub(super) const FAT_TOP: &str = "minecraft:end_city/fat_tower_top";
    pub(super) const BRIDGE_PIECE: &str = "minecraft:end_city/bridge_piece";
    pub(super) const BRIDGE_STEEP: &str = "minecraft:end_city/bridge_steep_stairs";
    pub(super) const BRIDGE_GENTLE: &str = "minecraft:end_city/bridge_gentle_stairs";
    pub(super) const BRIDGE_END: &str = "minecraft:end_city/bridge_end";
    pub(super) const SHIP: &str = "minecraft:end_city/ship";
}

const TOWER_BRIDGES: [(Rotation, Pos); 4] = [
    (Rotation::None, (1, -1, 0)),
    (Rotation::Clockwise90, (6, -1, 1)),
    (Rotation::Counterclockwise90, (0, -1, 5)),
    (Rotation::Clockwise180, (5, -1, 6)),
];

const FAT_TOWER_BRIDGES: [(Rotation, Pos); 4] = [
    (Rotation::None, (4, -1, 0)),
    (Rotation::Clockwise90, (12, -1, 4)),
    (Rotation::Counterclockwise90, (0, -1, 8)),
    (Rotation::Clockwise180, (8, -1, 12)),
];

#[derive(Debug, Clone, Copy)]
pub(crate) struct Piece {
    pub(crate) tpl: Tpl,
    pub(crate) bbox: Bbox,
    pub(crate) overwrite: bool,
    depth: i32,
}

#[derive(Debug, Clone, Copy)]
enum Section {
    HouseTower,
    Tower,
    TowerBridge,
    FatTower,
}

fn turns(rotation: Rotation) -> usize {
    match rotation {
        Rotation::None => 0,
        Rotation::Clockwise90 => 1,
        Rotation::Clockwise180 => 2,
        Rotation::Counterclockwise90 => 3,
    }
}

fn combine(first: Rotation, second: Rotation) -> Rotation {
    Rotation::ALL[(turns(first) + turns(second)) & 3]
}

struct Builder<'a> {
    library: &'static Library,
    random: &'a mut JavaRandom,
    ship_created: bool,
}

impl Builder<'_> {
    fn piece(
        &self,
        name: &'static str,
        pos: Pos,
        rotation: Rotation,
        overwrite: bool,
    ) -> Option<Piece> {
        let tpl = Tpl::new(self.library, name, pos, rotation, Mirror::None, (0, 0))?;
        Some(Piece {
            bbox: tpl.bbox(),
            tpl,
            overwrite,
            depth: 0,
        })
    }

    fn add(
        &self,
        parent: &Piece,
        offset: Pos,
        name: &'static str,
        rotation: Rotation,
        overwrite: bool,
    ) -> Option<Piece> {
        let shift = transform(offset, Mirror::None, parent.tpl.rotation, (0, 0));
        let at = parent.tpl.pos;
        self.piece(
            name,
            (at.0 + shift.0, at.1 + shift.1, at.2 + shift.2),
            rotation,
            overwrite,
        )
    }

    fn chain(
        &self,
        list: &mut Vec<Piece>,
        parent: &Piece,
        offset: Pos,
        name: &'static str,
        rotation: Rotation,
        overwrite: bool,
    ) -> Option<Piece> {
        let piece = self.add(parent, offset, name, rotation, overwrite)?;
        list.push(piece);
        Some(piece)
    }

    fn children(
        &mut self,
        section: Section,
        depth: i32,
        parent: &Piece,
        offset: Pos,
        pieces: &mut Vec<Piece>,
    ) -> Option<bool> {
        if depth > 8 {
            return Some(false);
        }
        let mut list = Vec::new();
        if !self.section(section, depth, parent, offset, &mut list)? {
            return Some(false);
        }
        let tag = self.random.next_i32();
        for piece in &mut list {
            piece.depth = tag;
            if let Some(hit) = pieces
                .iter()
                .find(|known| known.bbox.intersects(&piece.bbox))
                && hit.depth != parent.depth
            {
                return Some(false);
            }
        }
        pieces.extend(list);
        Some(true)
    }

    fn section(
        &mut self,
        section: Section,
        depth: i32,
        parent: &Piece,
        offset: Pos,
        pieces: &mut Vec<Piece>,
    ) -> Option<bool> {
        match section {
            Section::HouseTower => self.house_tower(depth, parent, offset, pieces),
            Section::Tower => self.tower(depth, parent, pieces),
            Section::TowerBridge => self.tower_bridge(depth, parent, pieces),
            Section::FatTower => self.fat_tower(depth, parent, pieces),
        }
    }

    fn house_tower(
        &mut self,
        depth: i32,
        parent: &Piece,
        offset: Pos,
        pieces: &mut Vec<Piece>,
    ) -> Option<bool> {
        if depth > 8 {
            return Some(false);
        }
        let rotation = parent.tpl.rotation;
        let mut piece = self.chain(pieces, parent, offset, name::BASE_FLOOR, rotation, true)?;
        match self.random.next_int(3) {
            0 => {
                self.chain(pieces, &piece, (-1, 4, -1), name::BASE_ROOF, rotation, true)?;
            }
            1 => {
                piece = self.chain(
                    pieces,
                    &piece,
                    (-1, 0, -1),
                    name::SECOND_FLOOR_2,
                    rotation,
                    false,
                )?;
                piece = self.chain(
                    pieces,
                    &piece,
                    (-1, 8, -1),
                    name::SECOND_ROOF,
                    rotation,
                    false,
                )?;
                self.children(Section::Tower, depth + 1, &piece, (0, 0, 0), pieces)?;
            }
            2 => {
                piece = self.chain(
                    pieces,
                    &piece,
                    (-1, 0, -1),
                    name::SECOND_FLOOR_2,
                    rotation,
                    false,
                )?;
                piece = self.chain(
                    pieces,
                    &piece,
                    (-1, 4, -1),
                    name::THIRD_FLOOR_2,
                    rotation,
                    false,
                )?;
                piece = self.chain(
                    pieces,
                    &piece,
                    (-1, 8, -1),
                    name::THIRD_ROOF,
                    rotation,
                    true,
                )?;
                self.children(Section::Tower, depth + 1, &piece, (0, 0, 0), pieces)?;
            }
            _ => {}
        }
        Some(true)
    }

    fn bridges(
        &mut self,
        bridges: &[(Rotation, Pos); 4],
        source: &Piece,
        rotation: Rotation,
        depth: i32,
        pieces: &mut Vec<Piece>,
    ) -> Option<()> {
        for (turn, offset) in bridges {
            if self.random.next_boolean() {
                let end = self.chain(
                    pieces,
                    source,
                    *offset,
                    name::BRIDGE_END,
                    combine(rotation, *turn),
                    true,
                )?;
                self.children(Section::TowerBridge, depth + 1, &end, (0, 0, 0), pieces)?;
            }
        }
        Some(())
    }

    fn tower(&mut self, depth: i32, parent: &Piece, pieces: &mut Vec<Piece>) -> Option<bool> {
        let rotation = parent.tpl.rotation;
        let east = self.random.next_int(2);
        let south = self.random.next_int(2);
        let mut piece = self.chain(
            pieces,
            parent,
            (3 + east, -3, 3 + south),
            name::TOWER_BASE,
            rotation,
            true,
        )?;
        piece = self.chain(pieces, &piece, (0, 7, 0), name::TOWER_PIECE, rotation, true)?;
        let mut source = (self.random.next_int(3) == 0).then_some(piece);
        let count = self.random.next_int(3) + 1;
        for index in 0..count {
            piece = self.chain(pieces, &piece, (0, 4, 0), name::TOWER_PIECE, rotation, true)?;
            if index < count - 1 && self.random.next_boolean() {
                source = Some(piece);
            }
        }
        match source {
            Some(source) => {
                self.bridges(&TOWER_BRIDGES, &source, rotation, depth, pieces)?;
                self.chain(pieces, &piece, (-1, 4, -1), name::TOWER_TOP, rotation, true)?;
            }
            None if depth == 7 => {
                self.chain(pieces, &piece, (-1, 4, -1), name::TOWER_TOP, rotation, true)?;
            }
            None => return self.children(Section::FatTower, depth + 1, &piece, (0, 0, 0), pieces),
        }
        Some(true)
    }

    fn fat_tower(&mut self, depth: i32, parent: &Piece, pieces: &mut Vec<Piece>) -> Option<bool> {
        let rotation = parent.tpl.rotation;
        let mut piece = self.chain(pieces, parent, (-3, 4, -3), name::FAT_BASE, rotation, true)?;
        piece = self.chain(pieces, &piece, (0, 4, 0), name::FAT_MIDDLE, rotation, true)?;
        let mut level = 0;
        while level < 2 && self.random.next_int(3) != 0 {
            piece = self.chain(pieces, &piece, (0, 8, 0), name::FAT_MIDDLE, rotation, true)?;
            self.bridges(&FAT_TOWER_BRIDGES, &piece, rotation, depth, pieces)?;
            level += 1;
        }
        self.chain(pieces, &piece, (-2, 8, -2), name::FAT_TOP, rotation, true)?;
        Some(true)
    }

    fn tower_bridge(
        &mut self,
        depth: i32,
        parent: &Piece,
        pieces: &mut Vec<Piece>,
    ) -> Option<bool> {
        let rotation = parent.tpl.rotation;
        let count = self.random.next_int(4) + 1;
        let first = self.chain(
            pieces,
            parent,
            (0, 0, -4),
            name::BRIDGE_PIECE,
            rotation,
            true,
        )?;
        if let Some(last) = pieces.last_mut() {
            last.depth = -1;
        }
        let mut piece = first;
        let mut rise = 0;
        for _ in 0..count {
            if self.random.next_boolean() {
                piece = self.chain(
                    pieces,
                    &piece,
                    (0, rise, -4),
                    name::BRIDGE_PIECE,
                    rotation,
                    true,
                )?;
                rise = 0;
            } else {
                if self.random.next_boolean() {
                    piece = self.chain(
                        pieces,
                        &piece,
                        (0, rise, -4),
                        name::BRIDGE_STEEP,
                        rotation,
                        true,
                    )?;
                } else {
                    piece = self.chain(
                        pieces,
                        &piece,
                        (0, rise, -8),
                        name::BRIDGE_GENTLE,
                        rotation,
                        true,
                    )?;
                }
                rise = 4;
            }
        }
        if !self.ship_created && self.random.next_int(10 - depth) == 0 {
            let east = self.random.next_int(8);
            let south = self.random.next_int(10);
            piece = self.chain(
                pieces,
                &piece,
                (east - 8, rise, south - 70),
                name::SHIP,
                rotation,
                true,
            )?;
            self.ship_created = true;
        } else if !self.children(
            Section::HouseTower,
            depth + 1,
            &piece,
            (-3, rise + 1, -11),
            pieces,
        )? {
            return Some(false);
        }
        self.chain(
            pieces,
            &piece,
            (4, rise, 0),
            name::BRIDGE_END,
            combine(rotation, Rotation::Clockwise180),
            true,
        )?;
        if let Some(last) = pieces.last_mut() {
            last.depth = -1;
        }
        Some(true)
    }
}

pub(crate) fn generate<G: Ground>(
    library: &'static Library,
    ground: &G,
    valid: &[u8],
    probe: &impl BiomeProbe,
    seed: i64,
    chunk: (i32, i32),
) -> Option<Vec<Piece>> {
    let mut random = start::random(seed, chunk);
    let (x, z) = (chunk.0 * 16 + 7, chunk.1 * 16 + 7);
    if !start::could_exist(ground, valid, probe, x, z) {
        return None;
    }
    let rotation = Rotation::ALL[random.next_int(4) as usize];
    let (width, depth) = match rotation {
        Rotation::None => (5, 5),
        Rotation::Clockwise90 => (-5, 5),
        Rotation::Clockwise180 => (-5, -5),
        Rotation::Counterclockwise90 => (5, -5),
    };
    let y = start::lowest_y(ground, x, width, z, depth);
    if y < 60 {
        return None;
    }
    let origin = (x, y, z);
    if !start::valid_at(valid, probe, origin) {
        return None;
    }
    let mut builder = Builder {
        library,
        random: &mut random,
        ship_created: false,
    };
    let mut pieces = Vec::new();
    let mut piece = builder.piece(name::BASE_FLOOR, origin, rotation, true)?;
    pieces.push(piece);
    piece = builder.chain(
        &mut pieces,
        &piece,
        (-1, 0, -1),
        name::SECOND_FLOOR_1,
        rotation,
        false,
    )?;
    piece = builder.chain(
        &mut pieces,
        &piece,
        (-1, 4, -1),
        name::THIRD_FLOOR_1,
        rotation,
        false,
    )?;
    piece = builder.chain(
        &mut pieces,
        &piece,
        (-1, 8, -1),
        name::THIRD_ROOF,
        rotation,
        true,
    )?;
    builder.children(Section::Tower, 1, &piece, (0, 0, 0), &mut pieces)?;
    Some(pieces)
}

fn facing_data(rotation: Rotation) -> i8 {
    match rotation {
        Rotation::None => 3,
        Rotation::Clockwise90 => 4,
        Rotation::Clockwise180 => 2,
        Rotation::Counterclockwise90 => 5,
    }
}

fn facing_step(rotation: Rotation) -> (f64, f64) {
    match rotation {
        Rotation::None => (0.0, 1.0),
        Rotation::Clockwise90 => (-1.0, 0.0),
        Rotation::Clockwise180 => (0.0, -1.0),
        Rotation::Counterclockwise90 => (1.0, 0.0),
    }
}

impl Piece {
    pub(crate) fn id(&self) -> &'static str {
        "ECP"
    }

    pub(crate) fn template(&self) -> &'static str {
        self.tpl.name.rsplit('/').next().unwrap_or(self.tpl.name)
    }

    pub(crate) fn place(&self, run: &mut Run, write: &Bbox) {
        let steps = if self.overwrite {
            [Step::IgnoreStructureBlock]
        } else {
            [Step::IgnoreAir]
        };
        let markers = self.tpl.place(run, write, &steps);
        for marker in &markers {
            let at = marker.at;
            if marker.meta.starts_with("Chest") {
                set_loot(run, (at.0, at.1 - 1, at.2), LOOT);
            } else if marker.meta.starts_with("Sentry") {
                run.region.entities.push(json!({
                    "id": "minecraft:shulker",
                    "Pos": [f64::from(at.0) + 0.5, f64::from(at.1), f64::from(at.2) + 0.5],
                    "Motion": [0.0, 0.0, 0.0],
                    "Rotation": [0.0f32, 0.0f32],
                }));
            } else if marker.meta.starts_with("Elytra") {
                let (dx, dz) = facing_step(self.tpl.rotation);
                run.region.entities.push(json!({
                    "id": "minecraft:item_frame",
                    "Pos": [
                        f64::from(at.0) + 0.5 - dx * 0.46875,
                        f64::from(at.1) + 0.5,
                        f64::from(at.2) + 0.5 - dz * 0.46875,
                    ],
                    "Motion": [0.0, 0.0, 0.0],
                    "Rotation": [0.0f32, 0.0f32],
                    "block_pos": { "$ia": [at.0, at.1, at.2] },
                    "Facing": { "$b": facing_data(self.tpl.rotation) },
                    "ItemRotation": { "$b": 0 },
                    "Item": { "id": "minecraft:elytra", "count": 1 },
                }));
            }
        }
    }
}
