use rspigot_blocks::{BlockKind as Kind, Face, Support};
use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::place::Run;
use super::prov::{Pos, integer, number};

#[derive(Debug, Clone, Copy)]
pub(crate) struct Magma {
    search: i32,
    radius: i32,
    chance: f32,
}

fn water_or_air(state: BlockStateId) -> bool {
    state.is_air() || state.kind() == Some(Kind::Water)
}

fn visible(run: &mut Run, (x, y, z): Pos, covered: Face) -> bool {
    !run.region
        .get(x, y, z)
        .is_face_sturdy(covered, Support::Full)
}

fn valid(run: &mut Run, (x, y, z): Pos) -> bool {
    if water_or_air(run.region.get(x, y, z)) || visible(run, (x, y - 1, z), Face::Up) {
        return false;
    }
    let sides = [
        ((0, -1), Face::South),
        ((1, 0), Face::West),
        ((0, 1), Face::North),
        ((-1, 0), Face::East),
    ];
    !sides
        .iter()
        .any(|((dx, dz), facing)| visible(run, (x + dx, y, z + dz), *facing))
}

impl Magma {
    pub(crate) fn compile(value: &Value) -> Self {
        Self {
            search: integer(value, "floor_search_range", 5),
            radius: integer(value, "placement_radius_around_floor", 1),
            chance: number(value, "placement_probability_per_valid_position", 0.5) as f32,
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        self.radius
    }

    fn floor(&self, run: &mut Run, (x, y, z): Pos) -> Option<i32> {
        let water = |run: &mut Run, at: i32| run.region.get(x, at, z).kind() == Some(Kind::Water);
        if !water(run, y) {
            return None;
        }
        let mut at = y;
        let mut steps = 1;
        while steps < self.search && water(run, at) {
            at -= 1;
            steps += 1;
        }
        (!water(run, at)).then_some(at)
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        let Some(floor) = self.floor(run, origin) else {
            return false;
        };
        let r = self.radius;
        let mut placed = 0;
        for dz in -r..=r {
            for dy in -r..=r {
                for dx in -r..=r {
                    if run.rng.next_float() >= self.chance {
                        continue;
                    }
                    let at = (origin.0 + dx, floor + dy, origin.2 + dz);
                    if valid(run, at) {
                        run.region
                            .set(at.0, at.1, at.2, Kind::MagmaBlock.default_state());
                        placed += 1;
                    }
                }
            }
        }
        placed > 0
    }
}

#[cfg(test)]
mod tests;
