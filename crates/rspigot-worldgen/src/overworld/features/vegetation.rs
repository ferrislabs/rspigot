use std::sync::Arc;

use rspigot_blocks::{BlockKind as Kind, Face, Support};
use serde_json::Value;

use super::bodies::Feature;
use super::javaset::hash_order;
use super::place::{Placed, Run, max_abs};
use super::prov::{BlockSet, Int, Pos, State, integer, number, text};
use crate::overworld::region::Region;

#[derive(Debug, Clone)]
pub(crate) struct Patch {
    waterlogged: bool,
    ceiling: bool,
    replaceable: BlockSet,
    ground: State,
    vegetation: Arc<Placed>,
    depth: Int,
    bottom_chance: f32,
    range: i32,
    chance: f32,
    radius: Int,
    edge_chance: f32,
}

fn step(face: Face) -> Pos {
    face.offset()
}

fn at(pos: Pos, (dx, dy, dz): Pos) -> Pos {
    (pos.0 + dx, pos.1 + dy, pos.2 + dz)
}

impl Patch {
    pub(crate) fn compile(value: &Value, waterlogged: bool, vegetation: Arc<Placed>) -> Self {
        Self {
            waterlogged,
            ceiling: text(value, "surface") == "ceiling",
            replaceable: BlockSet::compile(value.get("replaceable").unwrap_or(&Value::Null)),
            ground: State::compile(value.get("ground_state").unwrap_or(&Value::Null)),
            vegetation,
            depth: Int::compile(value.get("depth").unwrap_or(&Value::Null)),
            bottom_chance: number(value, "extra_bottom_block_chance", 0.0) as f32,
            range: integer(value, "vertical_range", 0),
            chance: number(value, "vegetation_chance", 0.0) as f32,
            radius: Int::compile(value.get("xz_radius").unwrap_or(&Value::Null)),
            edge_chance: number(value, "extra_edge_column_chance", 0.0) as f32,
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        max_abs(&self.radius) + 1 + self.vegetation.margin
    }

    fn surface(&self) -> Face {
        if self.ceiling { Face::Up } else { Face::Down }
    }

    fn place_ground(&self, run: &mut Run, mut below: Pos, depth: i32) -> bool {
        for placed in 0..depth {
            let Some(state) = self.ground.resolve(run.region, run.rng, below) else {
                return placed != 0;
            };
            let old = run.region.get(below.0, below.1, below.2);
            if state.kind() != old.kind() {
                if !self.replaceable.contains(old) {
                    return placed != 0;
                }
                run.region.set(below.0, below.1, below.2, state);
                below = at(below, step(self.surface()));
            }
        }
        true
    }

    fn ground_patch(&self, run: &mut Run, origin: Pos, rx: i32, rz: i32) -> Vec<Pos> {
        let direction = self.surface();
        let toward = step(direction);
        let away = step(direction.opposite());
        let mut surface = Vec::new();
        for dx in -rx..=rx {
            let x_edge = dx == -rx || dx == rx;
            for dz in -rz..=rz {
                let z_edge = dz == -rz || dz == rz;
                let edge = x_edge || z_edge;
                let corner = x_edge && z_edge;
                let edge_only = edge && !corner;
                if corner {
                    continue;
                }
                if edge_only && (self.edge_chance == 0.0 || run.rng.next_float() > self.edge_chance)
                {
                    continue;
                }
                let mut pos = (origin.0 + dx, origin.1, origin.2 + dz);
                let mut moved = 0;
                while run.region.get(pos.0, pos.1, pos.2).is_air() && moved < self.range {
                    pos = at(pos, toward);
                    moved += 1;
                }
                moved = 0;
                while !run.region.get(pos.0, pos.1, pos.2).is_air() && moved < self.range {
                    pos = at(pos, away);
                    moved += 1;
                }
                let below = at(pos, toward);
                let state = run.region.get(below.0, below.1, below.2);
                if run.region.get(pos.0, pos.1, pos.2).is_air()
                    && state.is_face_sturdy(direction.opposite(), Support::Full)
                {
                    let mut depth = self.depth.sample(run.rng);
                    if self.bottom_chance > 0.0 && run.rng.next_float() < self.bottom_chance {
                        depth += 1;
                    }
                    if self.place_ground(run, below, depth) {
                        surface.push(below);
                    }
                }
            }
        }
        surface
    }

    fn exposed(region: &mut Region, pos: Pos) -> bool {
        [Face::North, Face::East, Face::South, Face::West, Face::Down]
            .into_iter()
            .any(|face| {
                let next = at(pos, step(face));
                !region
                    .get(next.0, next.1, next.2)
                    .is_face_sturdy(face.opposite(), Support::Full)
            })
    }

    fn flood(&self, run: &mut Run, surface: Vec<Pos>) -> Vec<Pos> {
        let ordered = hash_order(surface);
        let sealed: Vec<Pos> = ordered
            .into_iter()
            .filter(|pos| !Self::exposed(run.region, *pos))
            .collect();
        for pos in &sealed {
            run.region
                .set(pos.0, pos.1, pos.2, Kind::Water.default_state());
        }
        sealed
    }

    fn place_vegetation(&self, run: &mut Run, pos: Pos) -> bool {
        let (target, outer) = if self.waterlogged {
            ((pos.0, pos.1 - 1, pos.2), Some(pos))
        } else {
            (pos, None)
        };
        let up = step(self.surface().opposite());
        let placed = self.vegetation.place(run, at(target, up));
        if placed && let Some(pos) = outer {
            let state = run.region.get(pos.0, pos.1, pos.2);
            if state.property("waterlogged") == Some("false")
                && let Some(wet) = state.with_property("waterlogged", "true")
            {
                run.region.set(pos.0, pos.1, pos.2, wet);
            }
        }
        placed
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        let rx = self.radius.sample(run.rng) + 1;
        let rz = self.radius.sample(run.rng) + 1;
        let mut surface = self.ground_patch(run, origin, rx, rz);
        if self.waterlogged {
            surface = self.flood(run, surface);
        }
        let order = hash_order(surface.iter().copied());
        for pos in order {
            if self.chance > 0.0 && run.rng.next_float() < self.chance {
                self.place_vegetation(run, pos);
            }
        }
        !surface.is_empty()
    }
}

pub(crate) fn feature(value: &Value, vegetation: Arc<Placed>, waterlogged: bool) -> Feature {
    Feature::Vegetation(Box::new(Patch::compile(value, waterlogged, vegetation)))
}

#[cfg(test)]
mod tests;
