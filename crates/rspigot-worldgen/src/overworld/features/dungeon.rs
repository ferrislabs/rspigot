use rspigot_blocks::{BlockKind as Kind, StateFlags};
use rspigot_domain::BlockStateId;

use super::place::Run;
use super::prov::{BlockSet, Pos};

const MOBS: i32 = 4;
const HORIZONTAL: [(&str, (i32, i32)); 4] = [
    ("north", (0, -1)),
    ("east", (1, 0)),
    ("south", (0, 1)),
    ("west", (-1, 0)),
];

#[derive(Debug, Clone)]
pub(crate) struct MonsterRoom {
    cannot_replace: BlockSet,
}

fn solid(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::LEGACY_SOLID)
}

fn solid_render(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::SOLID_RENDER)
}

fn opposite(name: &str) -> &'static str {
    match name {
        "north" => "south",
        "south" => "north",
        "east" => "west",
        _ => "east",
    }
}

fn clockwise(name: &str) -> &'static str {
    match name {
        "north" => "east",
        "east" => "south",
        "south" => "west",
        _ => "north",
    }
}

fn step(name: &str) -> (i32, i32) {
    HORIZONTAL
        .iter()
        .find(|(facing, _)| *facing == name)
        .map_or((0, 0), |(_, offset)| *offset)
}

fn is_kind(state: BlockStateId, kind: Kind) -> bool {
    state.kind() == Some(kind)
}

fn reoriented(run: &mut Run, (x, y, z): Pos, chest: BlockStateId) -> BlockStateId {
    let mut only: Option<&str> = None;
    for (name, (dx, dz)) in HORIZONTAL {
        let around = run.region.get(x + dx, y, z + dz);
        if is_kind(around, Kind::Chest) {
            return chest;
        }
        if solid_render(around) {
            if only.is_some() {
                only = None;
                break;
            }
            only = Some(name);
        }
    }
    if let Some(name) = only {
        return chest
            .with_property("facing", opposite(name))
            .unwrap_or(chest);
    }
    let mut facing = chest.property("facing").unwrap_or("north");
    let blocked = |run: &mut Run, name: &str| {
        let (dx, dz) = step(name);
        solid_render(run.region.get(x + dx, y, z + dz))
    };
    if blocked(run, facing) {
        facing = opposite(facing);
    }
    if blocked(run, facing) {
        facing = clockwise(facing);
    }
    if blocked(run, facing) {
        facing = opposite(facing);
    }
    chest.with_property("facing", facing).unwrap_or(chest)
}

impl MonsterRoom {
    pub(crate) fn compile() -> Self {
        Self {
            cannot_replace: BlockSet::tag("minecraft:features_cannot_replace"),
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        4
    }

    fn safe_set(&self, run: &mut Run, (x, y, z): Pos, state: BlockStateId) {
        if !self.cannot_replace.contains(run.region.get(x, y, z)) {
            run.region.set(x, y, z, state);
        }
    }

    fn holes(
        run: &mut Run,
        origin: Pos,
        (min_x, max_x): (i32, i32),
        (min_z, max_z): (i32, i32),
    ) -> Option<i32> {
        let mut holes = 0;
        for dx in min_x..=max_x {
            for dy in -1..=4 {
                for dz in min_z..=max_z {
                    let (x, y, z) = (origin.0 + dx, origin.1 + dy, origin.2 + dz);
                    let state = run.region.get(x, y, z);
                    if (dy == -1 || dy == 4) && !solid(state) {
                        return None;
                    }
                    let wall = dx == min_x || dx == max_x || dz == min_z || dz == max_z;
                    if wall && dy == 0 && state.is_air() && run.region.get(x, y + 1, z).is_air() {
                        holes += 1;
                    }
                }
            }
        }
        Some(holes)
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        let half_x = run.rng.below(2) + 2;
        let (min_x, max_x) = (-half_x - 1, half_x + 1);
        let half_z = run.rng.below(2) + 2;
        let (min_z, max_z) = (-half_z - 1, half_z + 1);
        match Self::holes(run, origin, (min_x, max_x), (min_z, max_z)) {
            Some(1..=5) => {}
            _ => return false,
        }
        let min_y = run.region.height().min_y;
        let cave_air = Kind::CaveAir.default_state();
        for dx in min_x..=max_x {
            for dy in (-1..=3).rev() {
                for dz in min_z..=max_z {
                    let at = (origin.0 + dx, origin.1 + dy, origin.2 + dz);
                    let state = run.region.get(at.0, at.1, at.2);
                    let shell =
                        dx == min_x || dy == -1 || dz == min_z || dx == max_x || dz == max_z;
                    if shell {
                        if at.1 >= min_y && !solid(run.region.get(at.0, at.1 - 1, at.2)) {
                            run.region.set(at.0, at.1, at.2, cave_air);
                        } else if solid(state) && !is_kind(state, Kind::Chest) {
                            let mossy = dy == -1 && run.rng.below(4) != 0;
                            let block = if mossy {
                                Kind::MossyCobblestone
                            } else {
                                Kind::Cobblestone
                            };
                            self.safe_set(run, at, block.default_state());
                        }
                    } else if !is_kind(state, Kind::Chest) && !is_kind(state, Kind::Spawner) {
                        self.safe_set(run, at, cave_air);
                    }
                }
            }
        }
        for _ in 0..2 {
            for _ in 0..3 {
                let x = origin.0 + run.rng.below(half_x * 2 + 1) - half_x;
                let z = origin.2 + run.rng.below(half_z * 2 + 1) - half_z;
                let at = (x, origin.1, z);
                if !run.region.get(at.0, at.1, at.2).is_air() {
                    continue;
                }
                let walls = HORIZONTAL
                    .iter()
                    .filter(|(_, (dx, dz))| solid(run.region.get(x + dx, at.1, z + dz)))
                    .count();
                if walls != 1 {
                    continue;
                }
                let chest = reoriented(run, at, Kind::Chest.default_state());
                self.safe_set(run, at, chest);
                if is_kind(run.region.get(at.0, at.1, at.2), Kind::Chest) {
                    run.rng.next_long();
                }
                break;
            }
        }
        self.safe_set(run, origin, Kind::Spawner.default_state());
        if is_kind(run.region.get(origin.0, origin.1, origin.2), Kind::Spawner) {
            run.rng.below(MOBS);
        }
        true
    }
}

#[cfg(test)]
mod tests;
