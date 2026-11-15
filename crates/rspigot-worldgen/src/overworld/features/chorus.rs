use super::place::Run;
use super::prov::{BlockSet, Pos};
use crate::datapack::state_named;
use rspigot_blocks::BlockKind as Kind;
use rspigot_domain::BlockStateId;

const HORIZONTAL: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
const SPREAD: i32 = 8;

#[derive(Debug, Clone)]
pub(crate) struct Chorus {
    support: BlockSet,
    flower: BlockStateId,
}

fn is_part(state: BlockStateId) -> bool {
    matches!(state.kind(), Some(Kind::ChorusPlant | Kind::ChorusFlower))
}

impl Chorus {
    pub(crate) fn compile() -> Self {
        Self {
            support: BlockSet::tag("minecraft:supports_chorus_plant"),
            flower: state_named("minecraft:chorus_flower", &[("age", "5")])
                .unwrap_or(BlockStateId::AIR),
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        SPREAD
    }

    fn connected(&self, run: &mut Run, (x, y, z): Pos) -> BlockStateId {
        let below = run.region.get(x, y - 1, z);
        let down = is_part(below) || self.support.contains(below);
        let up = is_part(run.region.get(x, y + 1, z));
        let side = |run: &mut Run, dx: i32, dz: i32| is_part(run.region.get(x + dx, y, z + dz));
        let flags = [
            ("north", side(run, 0, -1)),
            ("east", side(run, 1, 0)),
            ("south", side(run, 0, 1)),
            ("west", side(run, -1, 0)),
            ("up", up),
            ("down", down),
        ];
        let properties: Vec<(&str, &str)> = flags
            .iter()
            .map(|(name, on)| (*name, if *on { "true" } else { "false" }))
            .collect();
        state_named("minecraft:chorus_plant", &properties).unwrap_or(BlockStateId::AIR)
    }

    fn put(&self, run: &mut Run, pos: Pos) {
        let state = self.connected(run, pos);
        run.region.set(pos.0, pos.1, pos.2, state);
    }

    fn neighbours_empty(run: &mut Run, (x, y, z): Pos, ignore: Option<usize>) -> bool {
        for (index, (dx, dz)) in HORIZONTAL.iter().enumerate() {
            if Some(index) != ignore && !run.region.get(x + dx, y, z + dz).is_air() {
                return false;
            }
        }
        true
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        if !run.region.get(origin.0, origin.1, origin.2).is_air() {
            return false;
        }
        let below = run.region.get(origin.0, origin.1 - 1, origin.2);
        if !self.support.contains(below) {
            return false;
        }
        self.put(run, origin);
        self.grow(run, origin, origin, 0);
        true
    }

    fn grow(&self, run: &mut Run, current: Pos, start: Pos, depth: i32) {
        let mut height = run.rng.below(4) + 1;
        if depth == 0 {
            height += 1;
        }
        for i in 0..height {
            let above = (current.0, current.1 + i + 1, current.2);
            if !Self::neighbours_empty(run, above, None) {
                return;
            }
            self.put(run, above);
            self.put(run, (above.0, above.1 - 1, above.2));
        }
        let mut branched = false;
        if depth < 4 {
            let mut branches = run.rng.below(4);
            if depth == 0 {
                branches += 1;
            }
            for _ in 0..branches {
                let direction = run.rng.below(4) as usize;
                let (dx, dz) = HORIZONTAL[direction];
                let next = (current.0 + dx, current.1 + height, current.2 + dz);
                let opposite = (direction + 2) % 4;
                if (next.0 - start.0).abs() < SPREAD
                    && (next.2 - start.2).abs() < SPREAD
                    && run.region.get(next.0, next.1, next.2).is_air()
                    && run.region.get(next.0, next.1 - 1, next.2).is_air()
                    && Self::neighbours_empty(run, next, Some(opposite))
                {
                    branched = true;
                    self.put(run, next);
                    let (bx, bz) = HORIZONTAL[opposite];
                    self.put(run, (next.0 + bx, next.1, next.2 + bz));
                    self.grow(run, next, start, depth + 1);
                }
            }
        }
        if !branched {
            run.region
                .set(current.0, current.1 + height, current.2, self.flower);
        }
    }
}

#[cfg(test)]
mod tests;
