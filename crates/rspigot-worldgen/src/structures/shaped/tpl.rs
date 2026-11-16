use rspigot_blocks::BlockKind as Kind;
use rspigot_domain::BlockStateId;
use serde_json::Value;

use crate::overworld::features::place::Run;
use crate::overworld::features::structure::{Rotation, commit};
use crate::structures::bbox::{Bbox, Pos};
use crate::structures::blueprint::Blueprint;
use crate::structures::library::Library;
use crate::structures::processors::{Step, Subject};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mirror {
    None,
    FrontBack,
}

pub(crate) fn transform(
    (x, y, z): Pos,
    mirror: Mirror,
    rotation: Rotation,
    (px, pz): (i32, i32),
) -> Pos {
    let x = if mirror == Mirror::FrontBack { -x } else { x };
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
    if mirror == Mirror::None {
        return state;
    }
    let Some(kind) = state.kind() else {
        return state;
    };
    let name = kind.name();
    let swap = |state: BlockStateId, first: &str, second: &str| match (
        state.property(first),
        state.property(second),
    ) {
        (Some(a), Some(b)) => state
            .with_property(first, b)
            .and_then(|next| next.with_property(second, a))
            .unwrap_or(state),
        _ => state,
    };
    if name.ends_with("_stairs") {
        let facing = state.property("facing").unwrap_or("north");
        if !matches!(facing, "east" | "west") {
            return state;
        }
        let turned = Rotation::Clockwise180.state(state);
        let shape = match state.property("shape") {
            Some("outer_left") => "outer_right",
            Some("outer_right") => "outer_left",
            Some(other) => other,
            None => "straight",
        };
        return turned.with_property("shape", shape).unwrap_or(turned);
    }
    if name.ends_with("_wall") || kind == Kind::IronBars || kind == Kind::IronChain {
        return swap(state, "east", "west");
    }
    match state.property("facing") {
        Some("east" | "west") => Rotation::Clockwise180.state(state),
        _ => state,
    }
}

#[derive(Debug)]
pub(crate) struct Marker {
    pub(crate) at: Pos,
    pub(crate) meta: String,
}

#[derive(Debug)]
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

    pub(crate) fn rotation_name(&self) -> &'static str {
        match self.rotation {
            Rotation::None => "NONE",
            Rotation::Clockwise90 => "CLOCKWISE_90",
            Rotation::Clockwise180 => "CLOCKWISE_180",
            Rotation::Counterclockwise90 => "COUNTERCLOCKWISE_90",
        }
    }

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

    pub(crate) fn markers(&self, area: &Bbox) -> Vec<Marker> {
        self.blueprint
            .blocks
            .iter()
            .filter(|block| block.state.kind() == Some(Kind::StructureBlock))
            .filter_map(|block| {
                let nbt = block.nbt.as_ref()?;
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
        let mut placed: Vec<(Pos, BlockStateId)> = Vec::new();
        let mut loot: Vec<(Pos, Value)> = Vec::new();
        for block in &self.blueprint.blocks {
            let mut subject = Subject {
                local: block.pos,
                at: self.world(block.pos),
                state: block.state,
                nbt: block.nbt.as_ref(),
            };
            if !steps.iter().all(|step| step.process(run, &mut subject)) {
                continue;
            }
            if !area.contains(subject.at) {
                continue;
            }
            placed.push((subject.at, mirrored(subject.state, self.mirror)));
            if let Some(nbt) = subject.nbt {
                loot.push((subject.at, nbt.clone()));
            }
        }
        commit(run, placed, self.rotation, true);
        for (at, mut nbt) in loot {
            if let Value::Object(map) = &mut nbt
                && map.contains_key("LootTable")
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
