use rspigot_blocks::{BlockKind as Kind, Face, StateFlags, Support};
use rspigot_domain::BlockStateId;
use serde_json::{Value, json};

use super::tpl::{Mirror, mirrored};
use crate::climate::Xoroshiro;
use crate::overworld::features::place::Run;
use crate::overworld::features::structure::Rotation;
use crate::overworld::region::Heightmap;
use crate::structures::bbox::{Bbox, Pos};
use crate::structures::blueprint::Dir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Wood {
    pub(crate) planks: Kind,
    pub(crate) log: Kind,
    pub(crate) fence: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Guard {
    Open,
    Wood(Wood),
}

pub(crate) fn dir_name(dir: Dir) -> &'static str {
    match dir {
        Dir::Down => "down",
        Dir::Up => "up",
        Dir::North => "north",
        Dir::South => "south",
        Dir::West => "west",
        Dir::East => "east",
    }
}

pub(crate) fn clockwise(dir: Dir) -> Dir {
    match dir {
        Dir::North => Dir::East,
        Dir::East => Dir::South,
        Dir::South => Dir::West,
        Dir::West => Dir::North,
        other => other,
    }
}

pub(crate) fn face_of(dir: Dir) -> Face {
    match dir {
        Dir::Down => Face::Down,
        Dir::Up => Face::Up,
        Dir::North => Face::North,
        Dir::South => Face::South,
        Dir::West => Face::West,
        Dir::East => Face::East,
    }
}

pub(crate) const HORIZONTAL: [Dir; 4] = [Dir::North, Dir::East, Dir::South, Dir::West];

pub(crate) fn plain(kind: Kind) -> BlockStateId {
    kind.default_state()
}

pub(crate) fn with(kind: Kind, properties: &[(&str, &str)]) -> BlockStateId {
    kind.state_with(properties).unwrap_or(kind.default_state())
}

pub(crate) fn cave_air() -> BlockStateId {
    plain(Kind::CaveAir)
}

pub(crate) fn offset(at: Pos, dir: Dir) -> Pos {
    let (dx, dy, dz) = dir.step();
    (at.0 + dx, at.1 + dy, at.2 + dz)
}

fn is_fence(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::OakFence
            | Kind::SpruceFence
            | Kind::BirchFence
            | Kind::JungleFence
            | Kind::AcaciaFence
            | Kind::CherryFence
            | Kind::DarkOakFence
            | Kind::PaleOakFence
            | Kind::MangroveFence
            | Kind::PoplarFence
            | Kind::BambooFence
            | Kind::CrimsonFence
            | Kind::WarpedFence
            | Kind::NetherBrickFence
    )
}

fn shape_checked(kind: Kind) -> bool {
    is_fence(kind)
        && !matches!(
            kind,
            Kind::BambooFence | Kind::CrimsonFence | Kind::WarpedFence | Kind::PoplarFence
        )
        || matches!(
            kind,
            Kind::Torch | Kind::WallTorch | Kind::Ladder | Kind::IronBars
        )
}

fn liquid(state: BlockStateId) -> bool {
    state.flags().has_fluid() || state.kind() == Some(Kind::BubbleColumn)
}

pub(crate) fn replaceable_by_structures(state: BlockStateId) -> bool {
    state.is_air()
        || liquid(state)
        || matches!(
            state.kind(),
            Some(Kind::GlowLichen | Kind::Seagrass | Kind::TallSeagrass)
        )
}

fn solid_render(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::SOLID_RENDER)
}

pub(crate) struct Frame<'a, 'b, 'r> {
    pub(crate) run: &'a mut Run<'b, 'r>,
    pub(crate) bbox: Bbox,
    pub(crate) facing: Option<Dir>,
    pub(crate) chunk: Bbox,
    guard: Guard,
    orient: bool,
    marks: Vec<Pos>,
}

impl<'a, 'b, 'r> Frame<'a, 'b, 'r> {
    pub(crate) fn new(
        run: &'a mut Run<'b, 'r>,
        bbox: Bbox,
        facing: Option<Dir>,
        chunk: Bbox,
        guard: Guard,
    ) -> Self {
        Self {
            run,
            bbox,
            facing,
            chunk,
            guard,
            orient: false,
            marks: Vec::new(),
        }
    }

    pub(crate) fn oriented(mut self) -> Self {
        self.orient = true;
        self
    }

    fn turned(&self, state: BlockStateId) -> BlockStateId {
        if !self.orient {
            return state;
        }
        let (mirror, rotation) = match self.facing {
            Some(Dir::South) => (Mirror::LeftRight, Rotation::None),
            Some(Dir::West) => (Mirror::LeftRight, Rotation::Clockwise90),
            Some(Dir::East) => (Mirror::None, Rotation::Clockwise90),
            _ => (Mirror::None, Rotation::None),
        };
        rotation.state(mirrored(state, mirror))
    }

    pub(crate) fn world_x(&self, x: i32, z: i32) -> i32 {
        match self.facing {
            Some(Dir::North | Dir::South) => self.bbox.min.0 + x,
            Some(Dir::West) => self.bbox.max.0 - z,
            Some(Dir::East) => self.bbox.min.0 + z,
            _ => x,
        }
    }

    pub(crate) fn world_y(&self, y: i32) -> i32 {
        if self.facing.is_some() {
            self.bbox.min.1 + y
        } else {
            y
        }
    }

    pub(crate) fn world_z(&self, x: i32, z: i32) -> i32 {
        match self.facing {
            Some(Dir::North) => self.bbox.max.2 - z,
            Some(Dir::South) => self.bbox.min.2 + z,
            Some(Dir::West | Dir::East) => self.bbox.min.2 + x,
            _ => z,
        }
    }

    pub(crate) fn world_pos(&self, x: i32, y: i32, z: i32) -> Pos {
        (self.world_x(x, z), self.world_y(y), self.world_z(x, z))
    }

    pub(crate) fn state_at(&mut self, at: Pos) -> BlockStateId {
        self.run.region.get(at.0, at.1, at.2)
    }

    pub(crate) fn block(&mut self, x: i32, y: i32, z: i32) -> BlockStateId {
        let at = self.world_pos(x, y, z);
        if self.chunk.contains(at) {
            self.state_at(at)
        } else {
            BlockStateId::AIR
        }
    }

    fn replaceable(&mut self, x: i32, y: i32, z: i32) -> bool {
        match self.guard {
            Guard::Open => true,
            Guard::Wood(wood) => {
                let kind = self.block(x, y, z).kind();
                !(kind == Some(wood.planks)
                    || kind == Some(wood.log)
                    || kind == Some(wood.fence)
                    || kind == Some(Kind::IronChain))
            }
        }
    }

    pub(crate) fn place(&mut self, state: BlockStateId, x: i32, y: i32, z: i32) {
        let at = self.world_pos(x, y, z);
        if !self.chunk.contains(at) || !self.replaceable(x, y, z) {
            return;
        }
        let state = self.turned(state);
        if self.run.region.set(at.0, at.1, at.2, state) && state.kind().is_some_and(shape_checked) {
            self.marks.push(at);
        }
    }

    pub(crate) fn column_down(&mut self, state: BlockStateId, x: i32, y: i32, z: i32) {
        let mut at = self.world_pos(x, y, z);
        if !self.chunk.contains(at) {
            return;
        }
        let floor = self.run.region.height().min_y + 1;
        while replaceable_by_structures(self.state_at(at)) && at.1 > floor {
            self.run.region.set(at.0, at.1, at.2, state);
            at.1 -= 1;
        }
    }

    pub(crate) fn dispenser(&mut self, x: i32, y: i32, z: i32, facing: Dir, table: &str) -> bool {
        let at = self.world_pos(x, y, z);
        if !self.chunk.contains(at) || self.state_at(at).kind() == Some(Kind::Dispenser) {
            return false;
        }
        self.place(
            with(Kind::Dispenser, &[("facing", dir_name(facing))]),
            x,
            y,
            z,
        );
        self.add_loot_entity(at, "minecraft:dispenser", table);
        true
    }

    pub(crate) fn set_raw(&mut self, at: Pos, state: BlockStateId) {
        self.run.region.set(at.0, at.1, at.2, state);
    }

    pub(crate) fn is_interior(&mut self, x: i32, y: i32, z: i32) -> bool {
        let at = self.world_pos(x, y + 1, z);
        if !self.chunk.contains(at) {
            return false;
        }
        at.1 < self.run.region.height_at(Heightmap::OceanFloor, at.0, at.2)
    }

    pub(crate) fn bounds(
        &mut self,
        from: Pos,
        to: Pos,
        edge: BlockStateId,
        fill: BlockStateId,
        skip_air: bool,
    ) {
        for y in from.1..=to.1 {
            for x in from.0..=to.0 {
                for z in from.2..=to.2 {
                    if skip_air && self.block(x, y, z).is_air() {
                        continue;
                    }
                    let shell = y == from.1
                        || y == to.1
                        || x == from.0
                        || x == to.0
                        || z == from.2
                        || z == to.2;
                    self.place(if shell { edge } else { fill }, x, y, z);
                }
            }
        }
    }

    pub(crate) fn selected(
        &mut self,
        from: Pos,
        to: Pos,
        skip_air: bool,
        mut select: impl FnMut(&mut Xoroshiro, bool) -> BlockStateId,
    ) {
        for y in from.1..=to.1 {
            for x in from.0..=to.0 {
                for z in from.2..=to.2 {
                    if skip_air && self.block(x, y, z).is_air() {
                        continue;
                    }
                    let shell = y == from.1
                        || y == to.1
                        || x == from.0
                        || x == to.0
                        || z == from.2
                        || z == to.2;
                    let state = select(self.run.rng, shell);
                    self.place(state, x, y, z);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn maybe_bounds(
        &mut self,
        probability: f32,
        from: Pos,
        to: Pos,
        edge: BlockStateId,
        fill: BlockStateId,
        skip_air: bool,
        inside: bool,
    ) {
        for y in from.1..=to.1 {
            for x in from.0..=to.0 {
                for z in from.2..=to.2 {
                    if self.run.rng.next_float() > probability {
                        continue;
                    }
                    if skip_air && self.block(x, y, z).is_air() {
                        continue;
                    }
                    if inside && !self.is_interior(x, y, z) {
                        continue;
                    }
                    let shell = y == from.1
                        || y == to.1
                        || x == from.0
                        || x == to.0
                        || z == from.2
                        || z == to.2;
                    self.place(if shell { edge } else { fill }, x, y, z);
                }
            }
        }
    }

    pub(crate) fn maybe_block(&mut self, probability: f32, at: Pos, state: BlockStateId) {
        if self.run.rng.next_float() < probability {
            self.place(state, at.0, at.1, at.2);
        }
    }

    pub(crate) fn upper_half_sphere(
        &mut self,
        from: Pos,
        to: Pos,
        fill: BlockStateId,
        skip_air: bool,
    ) {
        let x_span = (to.0 - from.0 + 1) as f32;
        let y_span = (to.1 - from.1 + 1) as f32;
        let z_span = (to.2 - from.2 + 1) as f32;
        let x_center = from.0 as f32 + x_span / 2.0;
        let z_center = from.2 as f32 + z_span / 2.0;
        for y in from.1..=to.1 {
            let yd = (y - from.1) as f32 / y_span;
            for x in from.0..=to.0 {
                let xd = (x as f32 - x_center) / (x_span * 0.5);
                for z in from.2..=to.2 {
                    let zd = (z as f32 - z_center) / (z_span * 0.5);
                    if (!skip_air || !self.block(x, y, z).is_air())
                        && xd * xd + yd * yd + zd * zd <= 1.05
                    {
                        self.place(fill, x, y, z);
                    }
                }
            }
        }
    }

    pub(crate) fn sturdy(&mut self, at: Pos, face: Dir) -> bool {
        self.state_at(at)
            .is_face_sturdy(face_of(face), Support::Full)
    }

    pub(crate) fn chest_state(&mut self, at: Pos) -> BlockStateId {
        let chest = plain(Kind::Chest);
        for dir in HORIZONTAL {
            let near = self.state_at(offset(at, dir));
            if near.kind() == Some(Kind::Chest) {
                return chest;
            }
        }
        let mut found: Option<Dir> = None;
        for dir in HORIZONTAL {
            let near = self.state_at(offset(at, dir));
            if solid_render(near) {
                if found.is_some() {
                    found = None;
                    break;
                }
                found = Some(dir);
            }
        }
        let facing = match found {
            Some(dir) => dir.opposite(),
            None => {
                let mut dir = Dir::North;
                if solid_render(self.state_at(offset(at, dir))) {
                    dir = dir.opposite();
                }
                if solid_render(self.state_at(offset(at, dir))) {
                    dir = clockwise(dir);
                }
                if solid_render(self.state_at(offset(at, dir))) {
                    dir = dir.opposite();
                }
                dir
            }
        };
        with(Kind::Chest, &[("facing", dir_name(facing))])
    }

    pub(crate) fn loot_seed(&mut self) -> Value {
        let seed = self.run.rng.next_long() as i64;
        json!({ "$l": seed })
    }

    pub(crate) fn chest(&mut self, x: i32, y: i32, z: i32, table: &str) -> bool {
        let at = self.world_pos(x, y, z);
        if !self.chunk.contains(at) || self.state_at(at).kind() == Some(Kind::Chest) {
            return false;
        }
        let state = self.chest_state(at);
        self.set_raw(at, state);
        self.add_loot_entity(at, "minecraft:chest", table);
        true
    }

    pub(crate) fn add_loot_entity(&mut self, at: Pos, id: &str, table: &str) {
        let seed = self.loot_seed();
        self.run.region.block_entities.push((
            at,
            json!({ "id": id, "LootTable": table, "LootTableSeed": seed }),
        ));
    }

    pub(crate) fn spawner(&mut self, at: Pos, entity: &str) {
        self.set_raw(at, plain(Kind::Spawner));
        self.run.region.block_entities.push((
            at,
            json!({
                "id": "minecraft:mob_spawner",
                "SpawnData": { "entity": { "id": entity } },
                "SpawnPotentials": [],
                "Delay": { "$s": 20 },
                "MinSpawnDelay": { "$s": 200 },
                "MaxSpawnDelay": { "$s": 800 },
                "SpawnCount": { "$s": 4 },
                "MaxNearbyEntities": { "$s": 6 },
                "RequiredPlayerRange": { "$s": 16 },
                "SpawnRange": { "$s": 4 },
            }),
        ));
    }

    pub(crate) fn finish(mut self) {
        let marks = std::mem::take(&mut self.marks);
        for at in marks {
            let state = self.state_at(at);
            let next = self.updated(state, at);
            if next != state {
                self.set_raw(at, next);
            }
        }
    }

    fn connects(&mut self, state: BlockStateId, at: Pos, dir: Dir) -> bool {
        let near = self.state_at(offset(at, dir));
        let kind = near.kind();
        let bars = state.kind() == Some(Kind::IronBars);
        let side_solid = near.is_face_sturdy(face_of(dir.opposite()), Support::Full);
        let exception = kind.is_some_and(|kind| {
            matches!(
                kind,
                Kind::Barrier
                    | Kind::Pumpkin
                    | Kind::CarvedPumpkin
                    | Kind::JackOLantern
                    | Kind::Melon
            ) || leaves(kind)
        });
        if bars {
            return !exception && side_solid || kind == Some(Kind::IronBars);
        }
        let same = kind.is_some_and(|other| {
            is_fence(other)
                && (other == Kind::NetherBrickFence)
                    == (state.kind() == Some(Kind::NetherBrickFence))
        });
        !exception && side_solid || same
    }

    fn updated(&mut self, state: BlockStateId, at: Pos) -> BlockStateId {
        match state.kind() {
            Some(kind) if is_fence(kind) || kind == Kind::IronBars => {
                let mut next = state;
                for dir in HORIZONTAL {
                    let value = if self.connects(state, at, dir) {
                        "true"
                    } else {
                        "false"
                    };
                    next = next.with_property(dir_name(dir), value).unwrap_or(next);
                }
                next
            }
            Some(Kind::Torch) => {
                if self
                    .state_at(offset(at, Dir::Down))
                    .is_face_sturdy(Face::Up, Support::Center)
                {
                    state
                } else {
                    BlockStateId::AIR
                }
            }
            Some(Kind::WallTorch | Kind::Ladder) => {
                let facing = state
                    .property("facing")
                    .and_then(Dir::parse)
                    .unwrap_or(Dir::North);
                if self
                    .state_at(offset(at, facing.opposite()))
                    .is_face_sturdy(face_of(facing), Support::Full)
                {
                    state
                } else {
                    BlockStateId::AIR
                }
            }
            _ => state,
        }
    }
}

fn leaves(kind: Kind) -> bool {
    kind.name().ends_with("_leaves")
}
