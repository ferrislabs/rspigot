use rspigot_blocks::{BlockKind, StateFlags};
use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::bbox::{Bbox, Pos};
use crate::datapack::state_named;
use crate::overworld::features::structure::Rotation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Dir {
    Down,
    Up,
    North,
    South,
    West,
    East,
}

impl Dir {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "down" => Self::Down,
            "up" => Self::Up,
            "north" => Self::North,
            "south" => Self::South,
            "west" => Self::West,
            "east" => Self::East,
            _ => return None,
        })
    }

    pub(crate) fn opposite(self) -> Self {
        match self {
            Self::Down => Self::Up,
            Self::Up => Self::Down,
            Self::North => Self::South,
            Self::South => Self::North,
            Self::West => Self::East,
            Self::East => Self::West,
        }
    }

    pub(crate) fn step(self) -> Pos {
        match self {
            Self::Down => (0, -1, 0),
            Self::Up => (0, 1, 0),
            Self::North => (0, 0, -1),
            Self::South => (0, 0, 1),
            Self::West => (-1, 0, 0),
            Self::East => (1, 0, 0),
        }
    }

    pub(crate) fn horizontal(self) -> bool {
        !matches!(self, Self::Down | Self::Up)
    }

    fn rotate(self, rotation: Rotation) -> Self {
        if !self.horizontal() {
            return self;
        }
        let (dx, _, dz) = self.step();
        let (rx, rz) = rotation.vector((dx, dz));
        match (rx, rz) {
            (0, -1) => Self::North,
            (0, 1) => Self::South,
            (-1, 0) => Self::West,
            _ => Self::East,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Jigsaw {
    pub(crate) pos: Pos,
    pub(crate) front: Dir,
    pub(crate) top: Dir,
    pub(crate) rollable: bool,
    pub(crate) name: String,
    pub(crate) target: String,
    pub(crate) pool: String,
    pub(crate) final_state: String,
    pub(crate) selection_priority: i32,
    pub(crate) placement_priority: i32,
}

impl Jigsaw {
    pub(crate) fn can_attach(&self, other: &Self) -> bool {
        self.front == other.front.opposite()
            && (self.rollable || self.top == other.top)
            && self.target == other.name
    }

    pub(crate) fn front_pos(&self) -> Pos {
        let (dx, dy, dz) = self.front.step();
        (self.pos.0 + dx, self.pos.1 + dy, self.pos.2 + dz)
    }
}

#[derive(Debug)]
pub(crate) struct Block {
    pub(crate) pos: Pos,
    pub(crate) state: BlockStateId,
    pub(crate) nbt: Option<Value>,
}

#[derive(Debug)]
pub(crate) struct Blueprint {
    pub(crate) size: Pos,
    pub(crate) blocks: Vec<Block>,
    alternates: Vec<Vec<Block>>,
    pub(crate) entities: Vec<Value>,
    jigsaws: Vec<Jigsaw>,
}

fn triple(value: &Value) -> Option<Pos> {
    let items = value.as_array()?;
    let at = |index: usize| {
        items
            .get(index)?
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
    };
    Some((at(0)?, at(1)?, at(2)?))
}

fn palette_state(entry: &Value) -> BlockStateId {
    let Some(items) = entry.as_array() else {
        return BlockStateId::AIR;
    };
    let name = items
        .first()
        .and_then(Value::as_str)
        .unwrap_or("minecraft:air");
    let properties: Vec<(&str, &str)> = items[1..]
        .iter()
        .filter_map(|pair| {
            let pair = pair.as_array()?;
            Some((pair.first()?.as_str()?, pair.get(1)?.as_str()?))
        })
        .collect();
    state_named(name, &properties).unwrap_or(BlockStateId::AIR)
}

fn text(nbt: &Value, key: &str) -> String {
    nbt.get(key)
        .and_then(Value::as_str)
        .unwrap_or("minecraft:empty")
        .to_owned()
}

fn orientation(state: BlockStateId) -> (Dir, Dir) {
    let value = state.property("orientation").unwrap_or("north_up");
    let (front, top) = value.split_once('_').unwrap_or(("north", "up"));
    (
        Dir::parse(front).unwrap_or(Dir::North),
        Dir::parse(top).unwrap_or(Dir::Up),
    )
}

fn jigsaw(pos: Pos, state: BlockStateId, nbt: &Value) -> Jigsaw {
    let (front, top) = orientation(state);
    let rollable = match nbt.get("joint").and_then(Value::as_str) {
        Some("rollable") => true,
        Some("aligned") => false,
        _ => !front.horizontal(),
    };
    let priority = |key: &str| {
        nbt.get(key)
            .and_then(Value::as_i64)
            .and_then(|n| i32::try_from(n).ok())
            .unwrap_or(0)
    };
    Jigsaw {
        pos,
        front,
        top,
        rollable,
        name: text(nbt, "name"),
        target: text(nbt, "target"),
        pool: text(nbt, "pool"),
        final_state: nbt
            .get("final_state")
            .and_then(Value::as_str)
            .unwrap_or("minecraft:air")
            .to_owned(),
        selection_priority: priority("selection_priority"),
        placement_priority: priority("placement_priority"),
    }
}

impl Blueprint {
    pub(crate) fn parse(value: &Value) -> Option<Self> {
        let size = triple(value.get("size")?)?;
        let palettes = value.get("palettes")?.as_array()?;
        let rows = value.get("blocks")?.as_array()?;
        let mut jigsaws = Vec::new();
        let (blocks, found) = Self::build(palettes.first()?, rows)?;
        jigsaws.extend(found);
        let alternates = palettes[1..]
            .iter()
            .map(|palette| Self::build(palette, rows).map(|(blocks, _)| blocks))
            .collect::<Option<Vec<_>>>()?;
        let entities = value
            .get("entities")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Some(Self {
            size,
            blocks,
            alternates,
            entities,
            jigsaws,
        })
    }

    fn build(palette: &Value, rows: &[Value]) -> Option<(Vec<Block>, Vec<Jigsaw>)> {
        let palette: Vec<BlockStateId> = palette.as_array()?.iter().map(palette_state).collect();
        let mut blocks = Vec::new();
        let mut jigsaws = Vec::new();
        for row in rows {
            let items = row.as_array()?;
            let pos = (
                i32::try_from(items.first()?.as_i64()?).ok()?,
                i32::try_from(items.get(1)?.as_i64()?).ok()?,
                i32::try_from(items.get(2)?.as_i64()?).ok()?,
            );
            let state = *palette.get(usize::try_from(items.get(3)?.as_i64()?).ok()?)?;
            let nbt = items.get(4).cloned();
            if state.kind() == Some(BlockKind::Jigsaw)
                && let Some(nbt) = &nbt
            {
                jigsaws.push(jigsaw(pos, state, nbt));
            }
            blocks.push(Block { pos, state, nbt });
        }
        let class = |block: &Block| match (
            &block.nbt,
            block
                .state
                .flags()
                .contains(StateFlags::FULL_COLLISION_CUBE),
        ) {
            (None, true) => 0,
            (None, false) => 1,
            (Some(_), _) => 2,
        };
        blocks.sort_by_key(|block| (class(block), block.pos.1, block.pos.0, block.pos.2));
        Some((blocks, jigsaws))
    }

    pub(crate) fn palette(&self, index: usize) -> &[Block] {
        match index.checked_sub(1) {
            None => &self.blocks,
            Some(alternate) => self
                .alternates
                .get(alternate)
                .map_or(&self.blocks, Vec::as_slice),
        }
    }

    pub(crate) fn palette_count(&self) -> usize {
        1 + self.alternates.len()
    }

    pub(crate) fn size(&self, rotation: Rotation) -> Pos {
        if rotation.swaps_axes() {
            (self.size.2, self.size.1, self.size.0)
        } else {
            self.size
        }
    }

    pub(crate) fn bounds(&self, (x, y, z): Pos, rotation: Rotation) -> Bbox {
        let far = (self.size.0 - 1, self.size.1 - 1, self.size.2 - 1);
        let a = rotation.transform((0, 0, 0));
        let b = rotation.transform(far);
        Bbox::from_corners((x + a.0, y + a.1, z + a.2), (x + b.0, y + b.1, z + b.2))
    }

    pub(crate) fn jigsaws(&self, (x, y, z): Pos, rotation: Rotation) -> Vec<Jigsaw> {
        self.jigsaws
            .iter()
            .map(|source| {
                let (rx, ry, rz) = rotation.transform(source.pos);
                let (front, top) = (source.front.rotate(rotation), source.top.rotate(rotation));
                Jigsaw {
                    pos: (rx + x, ry + y, rz + z),
                    front,
                    top,
                    ..source.clone()
                }
            })
            .collect()
    }
}
