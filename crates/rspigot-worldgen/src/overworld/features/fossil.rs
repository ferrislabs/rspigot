use rspigot_blocks::BlockKind as Kind;
use serde_json::Value;

use super::place::Run;
use super::prov::{Pos, integer};
use super::structure::{Bounds, Processor, Rotation, Settings, Template, place_in_world};
use crate::datapack::worldgen;
use crate::overworld::region::Heightmap;

#[derive(Debug, Clone)]
pub(crate) struct Fossil {
    bodies: Vec<&'static Template>,
    overlays: Vec<&'static Template>,
    body_processors: Vec<Processor>,
    overlay_processors: Vec<Processor>,
    max_empty_corners: i32,
}

fn templates(value: Option<&Value>) -> Option<Vec<&'static Template>> {
    value?
        .as_array()?
        .iter()
        .map(|name| Template::named(name.as_str()?))
        .collect()
}

fn processors(value: Option<&Value>) -> Option<Vec<Processor>> {
    let name = value?.as_str()?;
    Processor::compile_list(worldgen().get("processor_lists")?.get(name)?)
}

fn empty_corner(run: &mut Run, (x, y, z): Pos) -> bool {
    let state = run.region.get(x, y, z);
    state.is_air() || matches!(state.kind(), Some(Kind::Lava | Kind::Water))
}

impl Fossil {
    pub(crate) fn compile(value: &Value) -> Option<Self> {
        let bodies = templates(value.get("fossil_structures"))?;
        let overlays = templates(value.get("overlay_structures"))?;
        if bodies.is_empty() || bodies.len() != overlays.len() {
            return None;
        }
        Some(Self {
            bodies,
            overlays,
            body_processors: processors(value.get("fossil_processors"))?,
            overlay_processors: processors(value.get("overlay_processors"))?,
            max_empty_corners: integer(value, "max_empty_corners_allowed", 0),
        })
    }

    pub(crate) fn reach(&self) -> i32 {
        self.bodies
            .iter()
            .map(|template| {
                let size = template.size(Rotation::None);
                size.0.max(size.2)
            })
            .max()
            .unwrap_or(1)
    }

    fn empty_corners(run: &mut Run, (low, high): Bounds) -> i32 {
        let mut count = 0;
        for x in [low.0, high.0] {
            for y in [low.1, high.1] {
                for z in [low.2, high.2] {
                    count += i32::from(empty_corner(run, (x, y, z)));
                }
            }
        }
        count
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        let rotation = Rotation::random(run.rng);
        let pick = run.rng.below(self.bodies.len() as i32) as usize;
        let (body, overlay) = (self.bodies[pick], self.overlays[pick]);
        let (chunk_x, chunk_z) = (origin.0 >> 4 << 4, origin.2 >> 4 << 4);
        let height = run.region.height();
        let (min_y, max_y) = (height.min_y, height.max_y());
        let bounds = (
            (chunk_x - 16, min_y, chunk_z - 16),
            (chunk_x + 31, max_y, chunk_z + 31),
        );
        let size = body.size(rotation);
        let corner = (origin.0 + -size.0 / 2, origin.1, origin.2 + -size.2 / 2);
        let mut floor = origin.1;
        for dx in 0..size.0 {
            for dz in 0..size.2 {
                let ground =
                    run.region
                        .height_at(Heightmap::OceanFloor, corner.0 + dx, corner.2 + dz);
                floor = floor.min(ground);
            }
        }
        let depth = run.rng.below(10);
        let level = (floor - 15 - depth).max(min_y + 10);
        let zero = body.zero_position((corner.0, level, corner.2), rotation);
        if Self::empty_corners(run, body.bounds(zero, rotation)) > self.max_empty_corners {
            return false;
        }
        let settings = Settings {
            rotation,
            bounds: Some(bounds),
            processors: &self.body_processors,
        };
        place_in_world(body, run, zero, &settings);
        let settings = Settings {
            rotation,
            bounds: Some(bounds),
            processors: &self.overlay_processors,
        };
        place_in_world(overlay, run, zero, &settings);
        true
    }
}

#[cfg(test)]
mod tests;
