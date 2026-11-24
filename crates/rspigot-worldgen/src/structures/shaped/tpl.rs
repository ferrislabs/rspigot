use rspigot_blocks::BlockKind as Kind;
use rspigot_domain::BlockStateId;
use serde_json::Value;

use crate::legacy::JavaRandom;
use crate::overworld::features::place::Run;
use crate::overworld::features::structure::{Rotation, commit_with};
use crate::structures::bbox::{Bbox, Pos};
use crate::structures::blueprint::Blueprint;
use crate::structures::library::Library;
use crate::structures::processors::{Step, Subject, position_seed};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mirror {
    None,
    FrontBack,
    LeftRight,
}

pub(crate) fn transform(
    (x, y, z): Pos,
    mirror: Mirror,
    rotation: Rotation,
    (px, pz): (i32, i32),
) -> Pos {
    let x = if mirror == Mirror::FrontBack { -x } else { x };
    let z = if mirror == Mirror::LeftRight { -z } else { z };
    match rotation {
        Rotation::Counterclockwise90 => (px - pz + z, y, px + pz - x),
        Rotation::Clockwise90 => (px + pz - z, y, pz - px + x),
        Rotation::Clockwise180 => (px * 2 - x, y, pz * 2 - z),
        Rotation::None => (x, y, z),
    }
}

pub(crate) fn bounds(
    size: Pos,
    origin: Pos,
    mirror: Mirror,
    rotation: Rotation,
    pivot: (i32, i32),
) -> Bbox {
    let a = transform((0, 0, 0), mirror, rotation, pivot);
    let b = transform(
        (size.0 - 1, size.1 - 1, size.2 - 1),
        mirror,
        rotation,
        pivot,
    );
    Bbox::from_corners(a, b).moved(origin.0, origin.1, origin.2)
}

pub(crate) fn center(bbox: &Bbox) -> Pos {
    (
        bbox.min.0 + (bbox.max.0 - bbox.min.0 + 1) / 2,
        bbox.min.1 + (bbox.max.1 - bbox.min.1 + 1) / 2,
        bbox.min.2 + (bbox.max.2 - bbox.min.2 + 1) / 2,
    )
}

pub(crate) fn mirrored(state: BlockStateId, mirror: Mirror) -> BlockStateId {
    let (along, first, second) = match mirror {
        Mirror::None => return state,
        Mirror::FrontBack => (["east", "west"], "east", "west"),
        Mirror::LeftRight => (["north", "south"], "north", "south"),
    };
    let Some(kind) = state.kind() else {
        return state;
    };
    let swap = |state: BlockStateId| match (state.property(first), state.property(second)) {
        (Some(a), Some(b)) => state
            .with_property(first, b)
            .and_then(|next| next.with_property(second, a))
            .unwrap_or(state),
        _ => state,
    };
    let facing = state.property("facing");
    if kind.name().ends_with("_stairs") {
        if !facing.is_some_and(|facing| along.contains(&facing)) {
            return state;
        }
        let turned = Rotation::Clockwise180.state(state);
        let shape = match state.property("shape") {
            Some("outer_left") => "outer_right",
            Some("outer_right") => "outer_left",
            Some("inner_left") => "inner_right",
            Some("inner_right") => "inner_left",
            Some(other) => other,
            None => "straight",
        };
        return turned.with_property("shape", shape).unwrap_or(turned);
    }
    let sides = ["north", "east", "south", "west"];
    if sides.iter().all(|side| state.property(side).is_some()) {
        return swap(state);
    }
    match facing {
        Some(facing) if along.contains(&facing) => Rotation::Clockwise180.state(state),
        _ => state,
    }
}

#[derive(Debug)]
pub(crate) struct Marker {
    pub(crate) at: Pos,
    pub(crate) meta: String,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Tpl {
    pub(crate) name: &'static str,
    pub(crate) blueprint: &'static Blueprint,
    pub(crate) pos: Pos,
    pub(crate) rotation: Rotation,
    pub(crate) mirror: Mirror,
    pub(crate) pivot: (i32, i32),
}

impl Tpl {
    pub(crate) fn new(
        library: &'static Library,
        name: &'static str,
        pos: Pos,
        rotation: Rotation,
        mirror: Mirror,
        pivot: (i32, i32),
    ) -> Option<Self> {
        Some(Self {
            name,
            blueprint: library.blueprint(name)?,
            pos,
            rotation,
            mirror,
            pivot,
        })
    }

    pub(crate) fn at(&self, pos: Pos) -> Self {
        Self { pos, ..*self }
    }

    pub(crate) fn rotation_name(&self) -> &'static str {
        match self.rotation {
            Rotation::None => "NONE",
            Rotation::Clockwise90 => "CLOCKWISE_90",
            Rotation::Clockwise180 => "CLOCKWISE_180",
            Rotation::Counterclockwise90 => "COUNTERCLOCKWISE_90",
        }
    }

    #[cfg(test)]
    pub(crate) fn line(&self) -> String {
        format!(
            "{} {} {} {} {}",
            self.name,
            self.pos.0,
            self.pos.1,
            self.pos.2,
            self.rotation_name()
        )
    }

    pub(crate) fn size(&self) -> Pos {
        self.blueprint.size
    }

    pub(crate) fn bbox(&self) -> Bbox {
        bounds(
            self.size(),
            self.pos,
            self.mirror,
            self.rotation,
            self.pivot,
        )
    }

    pub(crate) fn world(&self, local: Pos) -> Pos {
        let (x, y, z) = transform(local, self.mirror, self.rotation, self.pivot);
        (x + self.pos.0, y + self.pos.1, z + self.pos.2)
    }

    fn palette(&self) -> &'static [crate::structures::blueprint::Block] {
        let count = self.blueprint.palette_count();
        let index = if count == 1 {
            0
        } else {
            JavaRandom::new(position_seed(self.pos)).next_int(count as i32) as usize
        };
        self.blueprint.palette(index)
    }

    pub(crate) fn markers(&self, area: &Bbox) -> Vec<Marker> {
        self.palette()
            .iter()
            .filter(|block| block.state.kind() == Some(Kind::StructureBlock))
            .filter_map(|block| {
                let nbt = block.nbt.as_deref()?;
                if nbt.get("mode").and_then(Value::as_str) != Some("DATA") {
                    return None;
                }
                let at = self.world(block.pos);
                area.contains(at).then(|| Marker {
                    at,
                    meta: nbt
                        .get("metadata")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned(),
                })
            })
            .collect()
    }

    pub(crate) fn place(&self, run: &mut Run, area: &Bbox, steps: &[Step]) -> Vec<Marker> {
        self.place_with(run, area, steps, true, None)
    }

    pub(crate) fn place_with(
        &self,
        run: &mut Run,
        area: &Bbox,
        steps: &[Step],
        waterlogging: bool,
        archaeology: Option<&Archaeology>,
    ) -> Vec<Marker> {
        let mut entries: Vec<Entry> = Vec::new();
        for block in self.palette() {
            let mut subject = Subject {
                local: block.pos,
                at: self.world(block.pos),
                reference: self.pos,
                state: block.state,
                nbt: block.nbt.as_deref().map(std::borrow::Cow::Borrowed),
            };
            if !steps.iter().all(|step| step.process(run, &mut subject)) {
                continue;
            }
            entries.push(Entry {
                at: subject.at,
                state: subject.state,
                nbt: subject.nbt.map(std::borrow::Cow::into_owned),
            });
        }
        if let Some(archaeology) = archaeology {
            self.dust(run, &mut entries, archaeology);
        }
        let mut placed: Vec<(Pos, BlockStateId)> = Vec::new();
        let mut loot: Vec<(Pos, BlockStateId, Value)> = Vec::new();
        for entry in entries {
            if !area.contains(entry.at) {
                continue;
            }
            let state = mirrored(entry.state, self.mirror);
            placed.push((entry.at, state));
            if let Some(nbt) = entry.nbt {
                loot.push((entry.at, state, nbt));
            }
        }
        commit_with(run, placed, self.rotation, true, waterlogging);
        for (at, state, mut nbt) in loot {
            if let Value::Object(map) = &mut nbt
                && map.contains_key("LootTable")
                && container(state)
            {
                let seed = run.rng.next_long() as i64;
                map.insert(
                    "LootTableSeed".to_owned(),
                    serde_json::json!({ "$l": seed }),
                );
            }
            run.region.block_entities.push((at, nbt));
        }
        self.markers(area)
    }

    fn dust(&self, run: &mut Run, entries: &mut [Entry], archaeology: &Archaeology) {
        let count = entries.len();
        let limit = archaeology.limit.min(count as i32);
        if limit < 1 {
            return;
        }
        let mut base = JavaRandom::new(run.seed);
        let factory = base.next_long();
        let mut random = JavaRandom::new(position_seed(self.pos) ^ factory);
        let mut order: Vec<usize> = (0..count).collect();
        for i in (2..=count).rev() {
            let j = random.next_int(i as i32) as usize;
            order.swap(i - 1, j);
        }
        let mut changed = 0;
        for index in order {
            if changed >= limit {
                break;
            }
            let entry = &mut entries[index];
            if entry.state.kind() != Some(archaeology.input) {
                continue;
            }
            let mut random = JavaRandom::new(position_seed(entry.at));
            entry.state = archaeology.output.default_state();
            entry.nbt = Some(serde_json::json!({
                "id": "minecraft:brushable_block",
                "LootTable": archaeology.table,
                "LootTableSeed": { "$l": random.next_long() },
            }));
            changed += 1;
        }
    }
}

#[derive(Debug)]
struct Entry {
    at: Pos,
    state: BlockStateId,
    nbt: Option<Value>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Archaeology {
    pub(crate) input: Kind,
    pub(crate) output: Kind,
    pub(crate) table: &'static str,
    pub(crate) limit: i32,
}

fn container(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(Kind::Chest | Kind::TrappedChest | Kind::Barrel)
    )
}

pub(crate) fn set_loot(run: &mut Run, at: Pos, table: &str) {
    let state = run.region.get(at.0, at.1, at.2);
    let id = match state.kind() {
        Some(Kind::Chest) => "minecraft:chest",
        Some(Kind::TrappedChest) => "minecraft:trapped_chest",
        Some(Kind::Barrel) => "minecraft:barrel",
        _ => return,
    };
    let seed = run.rng.next_long() as i64;
    run.region.block_entities.retain(|(pos, _)| *pos != at);
    run.region.block_entities.push((
        at,
        serde_json::json!({ "id": id, "LootTable": table, "LootTableSeed": { "$l": seed } }),
    ));
}
