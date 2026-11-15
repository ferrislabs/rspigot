use std::sync::OnceLock;

use super::{Context, HORIZONTAL};
use crate::datapack::state_named;
use crate::overworld::features::prov::{BlockSet, Pos, is_replaceable};

fn below((x, y, z): Pos) -> Pos {
    (x, y - 1, z)
}

fn is_log(context: &mut Context, (x, y, z): Pos) -> bool {
    static LOGS: OnceLock<BlockSet> = OnceLock::new();
    let state = context.run.region.get(x, y, z);
    LOGS.get_or_init(|| BlockSet::tag("minecraft:logs"))
        .contains(state)
}

fn is_named(context: &mut Context, (x, y, z): Pos, name: &str) -> bool {
    context
        .run
        .region
        .get(x, y, z)
        .kind()
        .map(rspigot_blocks::BlockKind::name)
        == Some(name)
}

fn moss_hanger(context: &mut Context, mut pos: Pos) {
    while context.is_air(below(pos)) && f64::from(context.run.rng.next_float()) >= 0.5 {
        if let Some(state) = state_named("minecraft:pale_hanging_moss", &[("tip", "false")]) {
            context.put(pos, state);
        }
        pos = below(pos);
    }
    if let Some(state) = state_named("minecraft:pale_hanging_moss", &[("tip", "true")]) {
        context.put(pos, state);
    }
}

fn hang_below(context: &mut Context, pos: Pos, probability: f32) {
    if context.run.rng.next_float() < probability {
        let under = below(pos);
        if context.is_air(under) {
            moss_hanger(context, under);
        }
    }
}

pub(super) fn pale_moss(
    context: &mut Context,
    (leaves, trunk, ground): (f32, f32, f32),
    patch: &crate::overworld::features::bodies::Feature,
) {
    let mut shuffled = context.logs.clone();
    context.shuffle(&mut shuffled);
    let Some(lowest) = shuffled.iter().min_by_key(|pos| pos.1).copied() else {
        return;
    };
    if context.run.rng.next_float() < ground {
        patch.place(context.run, (lowest.0, lowest.1 + 1, lowest.2));
    }
    for pos in context.logs.clone() {
        hang_below(context, pos, trunk);
    }
    for pos in context.leaves.clone() {
        hang_below(context, pos, leaves);
    }
}

pub(super) fn creaking_heart(context: &mut Context, probability: f32) {
    if context.logs.is_empty() || context.run.rng.next_float() >= probability {
        return;
    }
    let mut shuffled = context.logs.clone();
    context.shuffle(&mut shuffled);
    let enclosed = shuffled.into_iter().find(|pos| {
        [
            (0, -1, 0),
            (0, 1, 0),
            (0, 0, -1),
            (0, 0, 1),
            (-1, 0, 0),
            (1, 0, 0),
        ]
        .into_iter()
        .all(|(dx, dy, dz)| is_log(context, (pos.0 + dx, pos.1 + dy, pos.2 + dz)))
    });
    if let Some(pos) = enclosed
        && let Some(state) = state_named(
            "minecraft:creaking_heart",
            &[("creaking_heart_state", "dormant"), ("natural", "true")],
        )
    {
        context.put(pos, state);
    }
}

fn replaceable_for_mushroom(context: &mut Context, pos: Pos) -> bool {
    let (x, y, z) = pos;
    if !is_replaceable(context.run.region.get(x, y, z)) {
        return false;
    }
    ![(0, 0), (1, 0), (-1, 0), (0, -1), (0, 1)]
        .into_iter()
        .any(|(dx, dz)| is_named(context, (x + dx, y, z + dz), "minecraft:water"))
}

fn mushroom_at(context: &mut Context, pos: Pos) -> bool {
    is_named(context, pos, "minecraft:shelf_mushroom")
}

fn mushroom_beside(context: &mut Context, (x, y, z): Pos) -> bool {
    HORIZONTAL
        .into_iter()
        .any(|(_, (dx, dz))| mushroom_at(context, (x + dx, y, z + dz)))
}

fn place_mushroom(context: &mut Context, pos: Pos, facing: &str) {
    let age = context.run.rng.below(2).to_string();
    if let Some(state) = state_named(
        "minecraft:shelf_mushroom",
        &[("age", age.as_str()), ("facing", facing)],
    ) {
        context.put(pos, state);
    }
}

fn place_on_standing(context: &mut Context, logs: &[Pos]) {
    let first = context.run.rng.below(4) as usize;
    let directions = [HORIZONTAL[first], HORIZONTAL[(first + 1) % 4]];
    let base = logs[0].1;
    for log in logs {
        let height = log.1 - base;
        if !(1..=4).contains(&height) {
            continue;
        }
        for (name, (dx, dz)) in directions {
            if context.run.rng.next_float() > 0.25 {
                continue;
            }
            let pos = (log.0 + dx, log.1, log.2 + dz);
            if replaceable_for_mushroom(context, pos) && !mushroom_at(context, below(pos)) {
                place_mushroom(context, pos, name);
                break;
            }
        }
    }
}

fn place_on_fallen(context: &mut Context, logs: &[Pos]) {
    let along_x = logs[0].0 != logs[logs.len() - 1].0;
    let directions: [(&str, (i32, i32)); 2] = if along_x {
        [HORIZONTAL[0], HORIZONTAL[2]]
    } else {
        [HORIZONTAL[1], HORIZONTAL[3]]
    };
    for log in logs {
        for (name, (dx, dz)) in directions {
            if context.run.rng.next_float() > 0.25 {
                continue;
            }
            let pos = (log.0 + dx, log.1, log.2 + dz);
            if replaceable_for_mushroom(context, pos)
                && !mushroom_beside(context, pos)
                && !mushroom_beside(context, *log)
            {
                place_mushroom(context, pos, name);
            }
        }
    }
}

pub(super) fn shelf_mushroom(context: &mut Context, probability: f32) {
    if context.run.rng.next_float() >= probability || context.logs.is_empty() {
        return;
    }
    let logs = context.logs.clone();
    if logs[0].1 == logs[logs.len() - 1].1 {
        place_on_fallen(context, &logs);
    } else {
        place_on_standing(context, &logs);
    }
}
