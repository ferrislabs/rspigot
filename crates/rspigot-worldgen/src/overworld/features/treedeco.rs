use std::sync::Arc;

use rspigot_blocks::{Face, Support};
use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::bodies::{Compiler, Feature};
use super::place::Run;
use super::prov::{Pos, State, integer, number, short};
use crate::datapack::state_named;
use crate::overworld::region::Heightmap;

mod forest;

const HORIZONTAL: [(&str, (i32, i32)); 4] = [
    ("north", (0, -1)),
    ("east", (1, 0)),
    ("south", (0, 1)),
    ("west", (-1, 0)),
];

#[derive(Debug, Clone)]
pub(crate) enum Decorator {
    Beehive(f32),
    LeaveVine(f32),
    TrunkVine,
    Cocoa(f32),
    AlterGround(State),
    PlaceOnGround {
        tries: i32,
        radius: i32,
        height: i32,
        state: State,
    },
    AttachedToLeaves {
        probability: f32,
        state: State,
        directions: Vec<(i32, i32, i32)>,
        empty: i32,
        radius_xz: i32,
        radius_y: i32,
    },
    PaleMoss {
        chances: (f32, f32, f32),
        patch: Arc<Feature>,
    },
    CreakingHeart(f32),
    ShelfMushroom(f32),
    AttachedToLogs {
        probability: f32,
        state: State,
        directions: Vec<(i32, i32, i32)>,
    },
}

fn direction(name: &str) -> (i32, i32, i32) {
    match name {
        "up" => (0, 1, 0),
        "down" => (0, -1, 0),
        "north" => (0, 0, -1),
        "south" => (0, 0, 1),
        "west" => (-1, 0, 0),
        _ => (1, 0, 0),
    }
}

fn directions(value: &Value) -> Vec<(i32, i32, i32)> {
    value
        .get("directions")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(direction)
                .collect()
        })
        .unwrap_or_default()
}

impl Decorator {
    pub(crate) fn compile(value: &Value, compiler: &mut Compiler) -> Option<Self> {
        let probability = number(value, "probability", 0.0) as f32;
        let provider = |key: &str| State::compile(value.get(key).unwrap_or(&Value::Null));
        Some(match short(value) {
            "beehive" => Self::Beehive(probability),
            "leave_vine" => Self::LeaveVine(probability),
            "trunk_vine" => Self::TrunkVine,
            "cocoa" => Self::Cocoa(probability),
            "alter_ground" => Self::AlterGround(provider("provider")),
            "place_on_ground" => Self::PlaceOnGround {
                tries: integer(value, "tries", 128),
                radius: integer(value, "radius", 2),
                height: integer(value, "height", 1),
                state: provider("block_state_provider"),
            },
            "attached_to_logs" => Self::AttachedToLogs {
                probability,
                state: provider("block_provider"),
                directions: directions(value),
            },
            "attached_to_leaves" => Self::AttachedToLeaves {
                probability,
                state: provider("block_provider"),
                directions: directions(value),
                empty: integer(value, "required_empty_blocks", 1),
                radius_xz: integer(value, "exclusion_radius_xz", 0),
                radius_y: integer(value, "exclusion_radius_y", 0),
            },
            "pale_moss" => Self::PaleMoss {
                chances: (
                    number(value, "leaves_probability", 0.0) as f32,
                    number(value, "trunk_probability", 0.0) as f32,
                    number(value, "ground_probability", 0.0) as f32,
                ),
                patch: compiler.configured_ref(&Value::String("minecraft:pale_moss_patch".into())),
            },
            "creaking_heart" => Self::CreakingHeart(probability),
            "shelf_mushroom" => Self::ShelfMushroom(probability),
            _ => return None,
        })
    }
}

pub(crate) struct Context<'a, 'b, 'r> {
    pub(crate) run: &'a mut Run<'b, 'r>,
    pub(crate) logs: Vec<Pos>,
    pub(crate) leaves: Vec<Pos>,
    pub(crate) roots: Vec<Pos>,
    pub(crate) placed: Vec<Pos>,
}

pub(crate) fn java_hash_order(positions: &[Pos]) -> Vec<Pos> {
    let mut unique: Vec<Pos> = Vec::with_capacity(positions.len());
    for pos in positions {
        if !unique.contains(pos) {
            unique.push(*pos);
        }
    }
    let mut capacity = 16usize;
    while unique.len() > capacity * 3 / 4 {
        capacity *= 2;
    }
    let bucket = |(x, y, z): Pos| {
        let hash = y
            .wrapping_add(z.wrapping_mul(31))
            .wrapping_mul(31)
            .wrapping_add(x) as u32;
        ((hash ^ (hash >> 16)) as usize) & (capacity - 1)
    };
    unique.sort_by_key(|pos| bucket(*pos));
    unique
}

fn sorted_by_height(positions: &[Pos]) -> Vec<Pos> {
    let mut ordered = java_hash_order(positions);
    ordered.sort_by_key(|pos| pos.1);
    ordered
}

impl Context<'_, '_, '_> {
    pub(crate) fn new<'a, 'b, 'r>(
        run: &'a mut Run<'b, 'r>,
        logs: &[Pos],
        leaves: &[Pos],
        roots: &[Pos],
    ) -> Context<'a, 'b, 'r> {
        Context {
            run,
            logs: sorted_by_height(logs),
            leaves: sorted_by_height(leaves),
            roots: sorted_by_height(roots),
            placed: Vec::new(),
        }
    }

    fn is_air(&mut self, (x, y, z): Pos) -> bool {
        self.run.region.get(x, y, z).is_air()
    }

    fn put(&mut self, (x, y, z): Pos, state: BlockStateId) {
        if self.run.region.set(x, y, z, state) {
            self.placed.push((x, y, z));
        }
    }

    fn vine(&mut self, pos: Pos, side: &str) {
        if let Some(state) = state_named("minecraft:vine", &[(side, "true")]) {
            self.put(pos, state);
        }
    }

    fn shuffle(&mut self, items: &mut [Pos]) {
        for index in (2..=items.len()).rev() {
            let other = self.run.rng.below(index as i32) as usize;
            items.swap(index - 1, other);
        }
    }

    fn lowest(&self) -> Vec<Pos> {
        match (self.roots.is_empty(), self.logs.is_empty()) {
            (true, _) => self.logs.clone(),
            (false, false) if self.roots[0].1 == self.logs[0].1 => {
                self.logs.iter().chain(&self.roots).copied().collect()
            }
            _ => self.roots.clone(),
        }
    }
}

fn add(pos: Pos, (dx, dy, dz): (i32, i32, i32)) -> Pos {
    (pos.0 + dx, pos.1 + dy, pos.2 + dz)
}

pub(crate) fn run(decorators: &[Decorator], context: &mut Context) {
    for decorator in decorators {
        match decorator {
            Decorator::Beehive(probability) => beehive(context, *probability),
            Decorator::LeaveVine(probability) => leave_vine(context, *probability),
            Decorator::TrunkVine => trunk_vine(context),
            Decorator::Cocoa(probability) => cocoa(context, *probability),
            Decorator::AlterGround(state) => alter_ground(context, state),
            Decorator::PlaceOnGround {
                tries,
                radius,
                height,
                state,
            } => place_on_ground(context, *tries, *radius, *height, state),
            Decorator::AttachedToLeaves {
                probability,
                state,
                directions,
                empty,
                radius_xz,
                radius_y,
            } => attached_to_leaves(
                context,
                state,
                directions,
                (*probability, *empty, *radius_xz, *radius_y),
            ),
            Decorator::PaleMoss { chances, patch } => forest::pale_moss(context, *chances, patch),
            Decorator::CreakingHeart(probability) => forest::creaking_heart(context, *probability),
            Decorator::ShelfMushroom(probability) => forest::shelf_mushroom(context, *probability),
            Decorator::AttachedToLogs {
                probability,
                state,
                directions,
            } => attached_to_logs(context, state, directions, *probability),
        }
    }
}

fn beehive(context: &mut Context, probability: f32) {
    if context.logs.is_empty() || context.run.rng.next_float() >= probability {
        return;
    }
    let lowest = context.logs[0].1;
    let highest = context.logs[context.logs.len() - 1].1;
    let hive_y = match context.leaves.first() {
        Some(leaf) => (leaf.1 - 1).max(lowest + 1),
        None => (lowest + 1 + context.run.rng.below(3)).min(highest),
    };
    let mut candidates: Vec<Pos> = context
        .logs
        .iter()
        .filter(|log| log.1 == hive_y)
        .flat_map(|log| {
            [(1, 0), (0, 1), (-1, 0)]
                .into_iter()
                .map(move |(dx, dz)| (log.0 + dx, log.1, log.2 + dz))
        })
        .collect();
    if candidates.is_empty() {
        return;
    }
    context.shuffle(&mut candidates);
    let chosen = candidates
        .into_iter()
        .find(|pos| context.is_air(*pos) && context.is_air((pos.0, pos.1, pos.2 + 1)));
    if let Some(pos) = chosen
        && let Some(nest) = state_named("minecraft:bee_nest", &[("facing", "south")])
    {
        context.put(pos, nest);
        let bees = 2 + context.run.rng.below(2);
        for _ in 0..bees {
            context.run.rng.below(599);
        }
    }
}

fn hanging_vine(context: &mut Context, pos: Pos, side: &str) {
    context.vine(pos, side);
    let mut at = (pos.0, pos.1 - 1, pos.2);
    let mut left = 4;
    while context.is_air(at) && left > 0 {
        context.vine(at, side);
        at.1 -= 1;
        left -= 1;
    }
}

fn leave_vine(context: &mut Context, probability: f32) {
    let leaves = context.leaves.clone();
    for leaf in leaves {
        for (dx, dz, side) in [
            (-1, 0, "east"),
            (1, 0, "west"),
            (0, -1, "south"),
            (0, 1, "north"),
        ] {
            if context.run.rng.next_float() < probability {
                let at = (leaf.0 + dx, leaf.1, leaf.2 + dz);
                if context.is_air(at) {
                    hanging_vine(context, at, side);
                }
            }
        }
    }
}

fn trunk_vine(context: &mut Context) {
    let logs = context.logs.clone();
    for log in logs {
        for (dx, dz, side) in [
            (-1, 0, "east"),
            (1, 0, "west"),
            (0, -1, "south"),
            (0, 1, "north"),
        ] {
            if context.run.rng.below(3) > 0 {
                let at = (log.0 + dx, log.1, log.2 + dz);
                if context.is_air(at) {
                    context.vine(at, side);
                }
            }
        }
    }
}

fn cocoa(context: &mut Context, probability: f32) {
    if context.run.rng.next_float() >= probability || context.logs.is_empty() {
        return;
    }
    let base = context.logs[0].1;
    let logs: Vec<Pos> = context
        .logs
        .iter()
        .copied()
        .filter(|log| log.1 - base < 3)
        .collect();
    for log in logs {
        for (name, (dx, dz)) in HORIZONTAL {
            if context.run.rng.next_float() <= 0.25 {
                let at = (log.0 - dx, log.1, log.2 - dz);
                if context.is_air(at) {
                    let age = context.run.rng.below(3).to_string();
                    if let Some(state) = state_named(
                        "minecraft:cocoa",
                        &[("age", age.as_str()), ("facing", name)],
                    ) {
                        context.put(at, state);
                    }
                }
            }
        }
    }
}

fn alter_ground(context: &mut Context, state: &State) {
    let lowest = context.lowest();
    let Some(first) = lowest.first() else {
        return;
    };
    let base = first.1;
    for log in lowest.iter().filter(|log| log.1 == base) {
        let log = *log;
        alter_circle(context, state, (log.0 - 1, log.1, log.2 - 1));
        alter_circle(context, state, (log.0 + 2, log.1, log.2 - 1));
        alter_circle(context, state, (log.0 - 1, log.1, log.2 + 2));
        alter_circle(context, state, (log.0 + 2, log.1, log.2 + 2));
        for _ in 0..5 {
            let roll = context.run.rng.below(64);
            let (a, b) = (roll % 8, roll / 8);
            if a == 0 || a == 7 || b == 0 || b == 7 {
                alter_circle(context, state, (log.0 + a - 3, log.1, log.2 + b - 3));
            }
        }
    }
}

fn alter_circle(context: &mut Context, state: &State, pos: Pos) {
    for dx in -2..=2i32 {
        for dz in -2..=2i32 {
            if dx.abs() == 2 && dz.abs() == 2 {
                continue;
            }
            alter_at(context, state, (pos.0 + dx, pos.1, pos.2 + dz));
        }
    }
}

fn alter_at(context: &mut Context, state: &State, pos: Pos) {
    for dy in (-3..=2).rev() {
        let at = (pos.0, pos.1 + dy, pos.2);
        if let Some(resolved) = state.resolve(context.run.region, context.run.rng, at) {
            context.put(at, resolved);
            return;
        }
        if !context.is_air(at) && dy < 0 {
            return;
        }
    }
}

fn place_on_ground(context: &mut Context, tries: i32, radius: i32, height: i32, state: &State) {
    let lowest = context.lowest();
    let Some(first) = lowest.first() else {
        return;
    };
    let base = first.1;
    let (mut x0, mut x1, mut z0, mut z1) = (first.0, first.0, first.2, first.2);
    for log in lowest.iter().filter(|log| log.1 == base) {
        x0 = x0.min(log.0);
        x1 = x1.max(log.0);
        z0 = z0.min(log.2);
        z1 = z1.max(log.2);
    }
    for _ in 0..tries {
        let x = context.run.rng.between(x0 - radius, x1 + radius);
        let y = context.run.rng.between(base - height, base + height);
        let z = context.run.rng.between(z0 - radius, z1 + radius);
        let above = (x, y + 1, z);
        let free = {
            let here = context.run.region.get(above.0, above.1, above.2);
            here.is_air() || here.kind() == Some(rspigot_blocks::BlockKind::Vine)
        };
        if !free {
            continue;
        }
        let ground = context.run.region.get(x, y, z);
        if !ground.is_face_sturdy(Face::Up, Support::Full) {
            continue;
        }
        let top = context
            .run
            .region
            .height_at(Heightmap::MotionBlockingNoLeaves, x, z);
        if top <= above.1
            && let Some(resolved) = state.resolve(context.run.region, context.run.rng, above)
        {
            context.put(above, resolved);
        }
    }
}

fn attached_to_leaves(
    context: &mut Context,
    state: &State,
    directions: &[(i32, i32, i32)],
    (probability, empty, radius_xz, radius_y): (f32, i32, i32, i32),
) {
    if directions.is_empty() {
        return;
    }
    let mut leaves = context.leaves.clone();
    context.shuffle(&mut leaves);
    let mut excluded: Vec<Pos> = Vec::new();
    for leaf in leaves {
        let step = directions[context.run.rng.below(directions.len() as i32) as usize];
        let target = add(leaf, step);
        if excluded.contains(&target) || context.run.rng.next_float() >= probability {
            continue;
        }
        let clear = (1..=empty).all(|k| {
            context.is_air((
                leaf.0 + step.0 * k,
                leaf.1 + step.1 * k,
                leaf.2 + step.2 * k,
            ))
        });
        if !clear {
            continue;
        }
        for x in target.0 - radius_xz..=target.0 + radius_xz {
            for y in target.1 - radius_y..=target.1 + radius_y {
                for z in target.2 - radius_xz..=target.2 + radius_xz {
                    excluded.push((x, y, z));
                }
            }
        }
        if let Some(resolved) = state.resolve(context.run.region, context.run.rng, target) {
            context.put(target, resolved);
        }
    }
}

fn attached_to_logs(
    context: &mut Context,
    state: &State,
    directions: &[(i32, i32, i32)],
    probability: f32,
) {
    if directions.is_empty() {
        return;
    }
    let mut logs = context.logs.clone();
    context.shuffle(&mut logs);
    for log in logs {
        let step = directions[context.run.rng.below(directions.len() as i32) as usize];
        let target = add(log, step);
        if context.run.rng.next_float() <= probability
            && context.is_air(target)
            && let Some(resolved) = state.resolve(context.run.region, context.run.rng, target)
        {
            context.put(target, resolved);
        }
    }
}

pub(crate) fn compile_all(value: &Value, compiler: &mut Compiler) -> Vec<Decorator> {
    compile_list(value.get("decorators"), compiler)
}

pub(crate) fn compile_list(list: Option<&Value>, compiler: &mut Compiler) -> Vec<Decorator> {
    list.and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| Decorator::compile(item, compiler))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overworld::features::tests::Bench;
    use crate::resources::BlockKind;

    #[test]
    fn hash_order_matches_java_hash_set_0() {
        let parse = |text: &str| -> Vec<Pos> {
            text.split(' ')
                .map(|item| {
                    let mut parts = item.split(',').map(|n| n.parse::<i32>().unwrap_or(0));
                    (
                        parts.next().unwrap_or(0),
                        parts.next().unwrap_or(0),
                        parts.next().unwrap_or(0),
                    )
                })
                .collect()
        };
        let input = parse(
            "1001,64,-293 1004,63,-300 1002,62,-293 1004,72,-293 1007,64,-295 1002,65,-298 1008,72,-299 1001,72,-299 1005,60,-297",
        );
        let expected = parse(
            "1007,64,-295 1005,60,-297 1004,72,-293 1002,65,-298 1008,72,-299 1004,63,-300 1002,62,-293 1001,72,-299 1001,64,-293",
        );
        assert_eq!(java_hash_order(&input), expected);
    }

    #[test]
    fn hash_order_matches_java_hash_set_1() {
        let parse = |text: &str| -> Vec<Pos> {
            text.split(' ')
                .map(|item| {
                    let mut parts = item.split(',').map(|n| n.parse::<i32>().unwrap_or(0));
                    (
                        parts.next().unwrap_or(0),
                        parts.next().unwrap_or(0),
                        parts.next().unwrap_or(0),
                    )
                })
                .collect()
        };
        let input = parse(
            "1002,68,-295 1008,72,-298 1008,67,-299 1003,61,-295 1004,66,-294 1006,69,-293 1000,61,-292 1007,67,-296 1002,66,-297 1005,62,-298 1001,69,-296 1006,68,-296 1002,72,-293 1005,69,-293",
        );
        let expected = parse(
            "1002,68,-295 1002,66,-297 1008,72,-298 1002,72,-293 1001,69,-296 1005,69,-293 1008,67,-299 1006,68,-296 1003,61,-295 1000,61,-292 1005,62,-298 1004,66,-294 1006,69,-293 1007,67,-296",
        );
        assert_eq!(java_hash_order(&input), expected);
    }

    #[test]
    fn hash_order_matches_java_hash_set_2() {
        let parse = |text: &str| -> Vec<Pos> {
            text.split(' ')
                .map(|item| {
                    let mut parts = item.split(',').map(|n| n.parse::<i32>().unwrap_or(0));
                    (
                        parts.next().unwrap_or(0),
                        parts.next().unwrap_or(0),
                        parts.next().unwrap_or(0),
                    )
                })
                .collect()
        };
        let input = parse(
            "1001,73,-298 1008,64,-297 1001,69,-294 1000,64,-295 1002,66,-300 1002,69,-297 1004,63,-292 1001,71,-296 1006,69,-294 1003,65,-300 1003,73,-294 1001,66,-298 1000,61,-299 1000,71,-299 1004,67,-294 1000,67,-295 1004,72,-299 1004,68,-300 1006,69,-294 1001,70,-295 1005,65,-295 1007,68,-295 1003,63,-300 1003,62,-293 1005,70,-300 1003,63,-295 1007,63,-297 1008,60,-300 1006,68,-297 1004,68,-296 1006,68,-298 1003,70,-297 1003,68,-293 1008,61,-297 1006,65,-300 1002,66,-294 1008,72,-292 1006,61,-295 1008,73,-300 1000,61,-292",
        );
        let expected = parse(
            "1005,70,-300 1004,72,-299 1001,66,-298 1002,66,-300 1004,68,-300 1001,70,-295 1003,70,-297 1008,61,-297 1006,61,-295 1004,63,-292 1006,69,-294 1004,67,-294 1006,65,-300 1000,61,-299 1003,63,-300 1007,63,-297 1000,61,-292 1005,65,-295 1003,63,-295 1008,73,-300 1001,71,-296 1001,69,-294 1003,65,-300 1000,67,-295 1002,69,-297 1003,73,-294 1001,73,-298 1000,71,-299 1003,62,-293 1008,60,-300 1003,68,-293 1002,66,-294 1000,64,-295 1006,68,-297 1004,68,-296 1006,68,-298 1008,64,-297 1007,68,-295 1008,72,-292",
        );
        assert_eq!(java_hash_order(&input), expected);
    }

    fn named_count(
        bench: &Bench,
        region: &mut crate::overworld::region::Region,
        name: &str,
    ) -> usize {
        let mut count = 0;
        for x in -16..32 {
            for z in -16..32 {
                for y in 60..110 {
                    if region
                        .get(x, y, z)
                        .kind()
                        .map(rspigot_blocks::BlockKind::name)
                        == Some(name)
                    {
                        count += 1;
                    }
                }
            }
        }
        let _ = bench;
        count
    }

    fn grow(tree: &str, ground: BlockKind, name: &str, seeds: i64) -> usize {
        let bench = Bench::new();
        let mut total = 0;
        for seed in 0..seeds {
            let mut region = bench.floor(ground, 69);
            bench.place(&mut region, tree, (8, 70, 8), seed);
            total += named_count(&bench, &mut region, name);
        }
        total
    }

    #[test]
    fn jungle_trees_hang_vines_and_cocoa() {
        assert!(grow("jungle_tree", BlockKind::GrassBlock, "minecraft:vine", 6) > 10);
        assert!(grow("jungle_tree", BlockKind::GrassBlock, "minecraft:cocoa", 40) > 0);
    }

    #[test]
    fn bee_trees_sometimes_carry_a_nest() {
        assert!(
            grow(
                "oak_bees_005",
                BlockKind::GrassBlock,
                "minecraft:bee_nest",
                80
            ) > 0
        );
    }

    #[test]
    fn mega_pines_turn_the_ground_to_podzol() {
        assert!(grow("mega_pine", BlockKind::GrassBlock, "minecraft:podzol", 4) > 8);
    }

    #[test]
    fn leaf_litter_trees_scatter_litter_on_the_ground() {
        assert!(
            grow(
                "oak_leaf_litter",
                BlockKind::GrassBlock,
                "minecraft:leaf_litter",
                6
            ) > 0
        );
    }

    #[test]
    fn pale_oaks_hang_moss_from_trunk_and_leaves() {
        assert!(
            grow(
                "pale_oak",
                BlockKind::GrassBlock,
                "minecraft:pale_hanging_moss",
                6
            ) > 5
        );
    }

    #[test]
    fn pale_oaks_sometimes_hide_a_creaking_heart() {
        assert!(
            grow(
                "pale_oak_creaking",
                BlockKind::GrassBlock,
                "minecraft:creaking_heart",
                40
            ) > 0
        );
        assert_eq!(
            grow(
                "pale_oak",
                BlockKind::GrassBlock,
                "minecraft:creaking_heart",
                40
            ),
            0
        );
    }

    #[test]
    fn poplars_grow_shelf_mushrooms_on_their_trunk() {
        assert!(
            grow(
                "orange_poplar",
                BlockKind::GrassBlock,
                "minecraft:shelf_mushroom",
                30
            ) > 0
        );
    }
}
