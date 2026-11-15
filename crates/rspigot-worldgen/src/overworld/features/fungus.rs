use serde_json::Value;

use super::place::Run;
use super::prov::{Pos, Pred};
use crate::datapack::{block_state, state_named};
use rspigot_blocks::BlockKind as Kind;
use rspigot_domain::BlockStateId;

#[derive(Debug, Clone)]
pub(crate) struct Fungus {
    base: BlockStateId,
    stem: BlockStateId,
    hat: BlockStateId,
    decor: BlockStateId,
    replaceable: Pred,
    planted: bool,
    weeping_head: Vec<BlockStateId>,
    weeping_plant: BlockStateId,
}

struct Hat {
    height: i32,
    huge: bool,
}

impl Fungus {
    pub(crate) fn compile(value: &Value) -> Self {
        let state = |key: &str| {
            value
                .get(key)
                .and_then(block_state)
                .unwrap_or(BlockStateId::AIR)
        };
        let air = BlockStateId::AIR;
        Self {
            base: value
                .get("valid_base_block")
                .and_then(block_state)
                .unwrap_or(air),
            stem: state("stem_state"),
            hat: state("hat_state"),
            decor: state("decor_state"),
            replaceable: Pred::compile(value.get("replaceable_blocks").unwrap_or(&Value::Null)),
            planted: value
                .get("planted")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            weeping_head: (23..=25)
                .map(|age| {
                    state_named("minecraft:weeping_vines", &[("age", &age.to_string())])
                        .unwrap_or(air)
                })
                .collect(),
            weeping_plant: state_named("minecraft:weeping_vines_plant", &[]).unwrap_or(air),
        }
    }

    pub(crate) fn reach(&self) -> i32 {
        8
    }

    fn replaceable(&self, run: &mut Run, pos: Pos, plants: bool) -> bool {
        if Pred::Replaceable((0, 0, 0)).test(run.region, pos) {
            return true;
        }
        plants && self.replaceable.test(run.region, pos)
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        let below = run.region.get(origin.0, origin.1 - 1, origin.2);
        if below.kind() != self.base.kind() {
            return false;
        }
        let mut height = run.rng.between(4, 13);
        if run.rng.below(12) == 0 {
            height *= 2;
        }
        if !self.planted && run.region.gen_depth() <= origin.1 + height + 1 {
            return false;
        }
        let huge = !self.planted && run.rng.next_float() < 0.06;
        run.region
            .set(origin.0, origin.1, origin.2, BlockStateId::AIR);
        self.stem(run, origin, height, huge);
        self.hat(run, origin, &Hat { height, huge });
        true
    }

    fn clear_below(&self, run: &mut Run, pos: Pos) {
        if !run.region.get(pos.0, pos.1 - 1, pos.2).is_air() {
            run.region.set(pos.0, pos.1, pos.2, BlockStateId::AIR);
        }
    }

    fn stem(&self, run: &mut Run, origin: Pos, height: i32, huge: bool) {
        let reach = i32::from(huge);
        for dx in -reach..=reach {
            for dz in -reach..=reach {
                let corner = huge && dx.abs() == reach && dz.abs() == reach;
                for dy in 0..height {
                    let pos = (origin.0 + dx, origin.1 + dy, origin.2 + dz);
                    if !self.replaceable(run, pos, true) {
                        continue;
                    }
                    if self.planted {
                        self.clear_below(run, pos);
                        run.region.set(pos.0, pos.1, pos.2, self.stem);
                    } else if !corner || run.rng.next_float() < 0.1 {
                        run.region.set(pos.0, pos.1, pos.2, self.stem);
                    }
                }
            }
        }
    }

    fn hat(&self, run: &mut Run, origin: Pos, shape: &Hat) {
        let total = shape.height;
        let wart = self.hat.kind() == Some(Kind::NetherWartBlock);
        let vines = |chance: f32| if wart { chance } else { 0.0 };
        let hat_height = (run.rng.below(total / 3 + 1) + 5).min(total);
        let start = total - hat_height;
        for dy in start..=total {
            let jitter = run.rng.below(3);
            let mut radius = if dy < total - jitter { 2 } else { 1 };
            if hat_height > 8 && dy < start + 4 {
                radius = 3;
            }
            if shape.huge {
                radius += 1;
            }
            for dx in -radius..=radius {
                for dz in -radius..=radius {
                    let x_edge = dx == -radius || dx == radius;
                    let z_edge = dz == -radius || dz == radius;
                    let inner = !(x_edge || z_edge || dy == total);
                    let corner = x_edge && z_edge;
                    let lower = dy < start + 3;
                    let pos = (origin.0 + dx, origin.1 + dy, origin.2 + dz);
                    if !self.replaceable(run, pos, false) {
                        continue;
                    }
                    if self.planted {
                        self.clear_below(run, pos);
                    }
                    if !lower {
                        if !inner {
                            if corner {
                                self.hat_block(run, pos, (0.01, 0.7, vines(0.083)));
                            } else {
                                self.hat_block(run, pos, (0.0005, 0.98, vines(0.07)));
                            }
                        } else {
                            self.hat_block(run, pos, (0.1, 0.2, vines(0.1)));
                        }
                    } else if !inner {
                        self.hat_drop(run, pos, wart);
                    }
                }
            }
        }
    }

    fn hat_block(&self, run: &mut Run, pos: Pos, (decor, hat, vines): (f32, f32, f32)) {
        if decor <= run.rng.next_float() {
            if run.rng.next_float() < hat {
                run.region.set(pos.0, pos.1, pos.2, self.hat);
                if run.rng.next_float() < vines {
                    self.weeping(run, pos);
                }
            }
        } else {
            run.region.set(pos.0, pos.1, pos.2, self.decor);
        }
    }

    fn hat_drop(&self, run: &mut Run, pos: Pos, vines: bool) {
        let below = run.region.get(pos.0, pos.1 - 1, pos.2);
        if below.kind() == self.hat.kind() {
            run.region.set(pos.0, pos.1, pos.2, self.hat);
        } else if run.rng.next_float() < 0.15 {
            run.region.set(pos.0, pos.1, pos.2, self.hat);
            if vines && run.rng.below(11) == 0 {
                self.weeping(run, pos);
            }
        }
    }

    fn weeping(&self, run: &mut Run, hat: Pos) {
        let start = (hat.0, hat.1 - 1, hat.2);
        if !run.region.get(start.0, start.1, start.2).is_air() {
            return;
        }
        let mut total = run.rng.between(1, 5);
        if run.rng.below(7) == 0 {
            total *= 2;
        }
        let mut cursor = start;
        for step in 0..=total {
            if run.region.get(cursor.0, cursor.1, cursor.2).is_air() {
                let last =
                    step == total || !run.region.get(cursor.0, cursor.1 - 1, cursor.2).is_air();
                if last {
                    let age = run.rng.between(23, 25);
                    let head = self.weeping_head[(age - 23) as usize];
                    run.region.set(cursor.0, cursor.1, cursor.2, head);
                    return;
                }
                run.region
                    .set(cursor.0, cursor.1, cursor.2, self.weeping_plant);
            }
            cursor.1 -= 1;
        }
    }
}

#[cfg(test)]
mod tests;
