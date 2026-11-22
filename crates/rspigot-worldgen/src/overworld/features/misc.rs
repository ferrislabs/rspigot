use rspigot_blocks::{BlockKind as Kind, Face, Support};
use serde_json::json;

use super::cave::shuffle;
use super::place::Run;
use super::prov::{Pos, State};
use crate::overworld::region::Heightmap;

const HORIZONTAL: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
const BONUS_CHEST_TABLE: &str = "minecraft:chests/spawn_bonus_chest";
const PLATFORM_OFFSET_Y: i32 = 3;
const PLATFORM_CHUNK_REACH: i32 = 1;
const PLATFORM_RADIUS: i32 = 17;

pub(crate) const REACH: i32 = 16;

fn may_place_on(run: &mut Run, (x, y, z): Pos) -> bool {
    let below = run.region.get(x, y - 1, z);
    if below.kind() == Some(Kind::DirtPath) {
        return run.rng.next_boolean();
    }
    below.is_face_sturdy(Face::Up, Support::Full)
}

fn try_place(provider: &State, run: &mut Run, pos: Pos) {
    if !run.region.get(pos.0, pos.1, pos.2).is_air() || !may_place_on(run, pos) {
        return;
    }
    if let Some(state) = provider.resolve(run.region, run.rng, pos) {
        run.region.set(pos.0, pos.1, pos.2, state);
    }
}

pub(crate) fn place_pile(provider: &State, run: &mut Run, (ox, oy, oz): Pos) -> bool {
    if oy < run.region.height().min_y + 5 {
        return false;
    }
    let reach_x = run.rng.below(2) + 2;
    let reach_z = run.rng.below(2) + 2;
    for z in (oz - reach_z)..=(oz + reach_z) {
        for y in oy..=(oy + 1) {
            for x in (ox - reach_x)..=(ox + reach_x) {
                let (dx, dz) = (ox - x, oz - z);
                let first = run.rng.next_float();
                let second = run.rng.next_float();
                if first * 10.0 - second * 6.0 < (dx * dx + dz * dz) as f32 {
                    if run.rng.next_float() < 0.031 {
                        try_place(provider, run, (x, y, z));
                    }
                } else {
                    try_place(provider, run, (x, y, z));
                }
            }
        }
    }
    true
}

pub(crate) fn place_bonus_chest(run: &mut Run, (ox, _, oz): Pos) -> bool {
    let (min_x, min_z) = (ox & !15, oz & !15);
    let mut xs: Vec<i32> = (min_x..min_x + 16).collect();
    shuffle(run.rng, &mut xs);
    let mut zs: Vec<i32> = (min_z..min_z + 16).collect();
    shuffle(run.rng, &mut zs);
    for x in &xs {
        for z in &zs {
            let y = run
                .region
                .height_at(Heightmap::MotionBlockingNoLeaves, *x, *z);
            let here = run.region.get(*x, y, *z);
            if !here.is_air() && !here.collision_shape().is_empty() {
                continue;
            }
            run.region.set(*x, y, *z, Kind::Chest.default_state());
            let seed = run.rng.next_long() as i64;
            run.region
                .block_entities
                .retain(|(at, _)| *at != (*x, y, *z));
            run.region.block_entities.push((
                (*x, y, *z),
                json!({ "id": "minecraft:chest", "LootTable": BONUS_CHEST_TABLE, "LootTableSeed": { "$l": seed } }),
            ));
            let torch = Kind::Torch.default_state();
            for (dx, dz) in HORIZONTAL {
                let (tx, tz) = (*x + dx, *z + dz);
                let below = run.region.get(tx, y - 1, tz);
                if below.is_face_sturdy(Face::Up, Support::Center) {
                    run.region.set(tx, y, tz, torch);
                }
            }
            return true;
        }
    }
    false
}

pub(crate) fn place_void_platform(run: &mut Run, (ox, oy, oz): Pos) -> bool {
    let (chunk_x, chunk_z) = (ox >> 4, oz >> 4);
    if chunk_x.abs().max(chunk_z.abs()) > PLATFORM_CHUNK_REACH {
        return true;
    }
    let (px, py, pz) = (8, oy + PLATFORM_OFFSET_Y, 8);
    for z in (chunk_z << 4)..=((chunk_z << 4) + 15) {
        for x in (chunk_x << 4)..=((chunk_x << 4) + 15) {
            if (px - x).abs().max((pz - z).abs()) < PLATFORM_RADIUS {
                let block = if (x, z) == (px, pz) {
                    Kind::Cobblestone
                } else {
                    Kind::Stone
                };
                run.region.set(x, py, z, block.default_state());
            }
        }
    }
    true
}

#[cfg(test)]
mod tests;
