use rspigot_blocks::BlockKind as Kind;
use serde_json::json;

use super::frame::{Frame, Guard, plain, with};
use super::scatter::Piece;
use crate::legacy::JavaRandom;
use crate::overworld::features::place::Run;
use crate::overworld::features::structure::Rotation;
use crate::structures::bbox::{Bbox, Pos};
use crate::structures::blueprint::Dir;
use crate::structures::processors::position_seed;

const CHEST: &str = "minecraft:chests/desert_pyramid";
const ARCHAEOLOGY: &str = "minecraft:archaeology/desert_pyramid";

fn sand_box(f: &mut Frame, sand: &mut Vec<Pos>, from: Pos, to: Pos) {
    for y in from.1..=to.1 {
        for x in from.0..=to.0 {
            for z in from.2..=to.2 {
                sand.push(f.world_pos(x, y, z));
            }
        }
    }
}

fn collapse(f: &mut Frame, from: Pos, to: (i32, i32)) -> Pos {
    let y = from.1;
    for x in from.0..=to.0 {
        for z in from.2..=to.1 {
            if f.run.region.random().next_float() < 0.33 {
                f.place(plain(Kind::Sandstone), x, y, z);
            } else {
                f.place(plain(Kind::Sand), x, y, z);
            }
        }
    }
    let mut base = JavaRandom::new(f.run.seed);
    let factory = base.next_long();
    let mut random = JavaRandom::new(position_seed(f.world_pos(from.0, y, from.2)) ^ factory);
    let x = random.next_int(to.0 - from.0 + 1) + from.0;
    let z = random.next_int(to.1 - from.2 + 1) + from.2;
    f.world_pos(x, y, z)
}

fn cellar_stairs(f: &mut Frame, (v4, v5, v6): Pos) {
    let turned = Rotation::Counterclockwise90.state(plain(Kind::SandstoneStairs));
    f.place(turned, 13, -1, 17);
    f.place(turned, 14, -2, 17);
    f.place(turned, 15, -3, 17);
    let v8 = plain(Kind::Sand);
    let v9 = plain(Kind::Sandstone);
    let v10 = f.run.region.random().next_boolean();
    f.place(v8, v4 - 4, v5 + 4, v6 + 4);
    f.place(v8, v4 - 3, v5 + 4, v6 + 4);
    f.place(v8, v4 - 2, v5 + 4, v6 + 4);
    f.place(v8, v4 - 1, v5 + 4, v6 + 4);
    f.place(v8, v4, v5 + 4, v6 + 4);
    f.place(v8, v4 - 2, v5 + 3, v6 + 4);
    f.place(if v10 { v8 } else { v9 }, v4 - 1, v5 + 3, v6 + 4);
    f.place(if v10 { v9 } else { v8 }, v4, v5 + 3, v6 + 4);
    f.place(v8, v4 - 1, v5 + 2, v6 + 4);
    f.place(v9, v4, v5 + 2, v6 + 4);
    f.place(v8, v4, v5 + 1, v6 + 4);
}

fn cellar_room(f: &mut Frame, sand: &mut Vec<Pos>, roof: &mut Pos, (v4, v5, v6): Pos) {
    let v7 = plain(Kind::CutSandstone);
    let v8 = plain(Kind::ChiseledSandstone);
    f.bounds(
        (v4 - 3, v5 + 1, v6 - 3),
        (v4 - 3, v5 + 1, v6 + 2),
        v7,
        v7,
        true,
    );
    f.bounds(
        (v4 + 3, v5 + 1, v6 - 3),
        (v4 + 3, v5 + 1, v6 + 2),
        v7,
        v7,
        true,
    );
    f.bounds(
        (v4 - 3, v5 + 1, v6 - 3),
        (v4 + 3, v5 + 1, v6 - 2),
        v7,
        v7,
        true,
    );
    f.bounds(
        (v4 - 3, v5 + 1, v6 + 3),
        (v4 + 3, v5 + 1, v6 + 3),
        v7,
        v7,
        true,
    );
    f.bounds(
        (v4 - 3, v5 + 2, v6 - 3),
        (v4 - 3, v5 + 2, v6 + 2),
        v8,
        v8,
        true,
    );
    f.bounds(
        (v4 + 3, v5 + 2, v6 - 3),
        (v4 + 3, v5 + 2, v6 + 2),
        v8,
        v8,
        true,
    );
    f.bounds(
        (v4 - 3, v5 + 2, v6 - 3),
        (v4 + 3, v5 + 2, v6 - 2),
        v8,
        v8,
        true,
    );
    f.bounds(
        (v4 - 3, v5 + 2, v6 + 3),
        (v4 + 3, v5 + 2, v6 + 3),
        v8,
        v8,
        true,
    );
    f.bounds((v4 - 3, -1, v6 - 3), (v4 - 3, -1, v6 + 2), v7, v7, true);
    f.bounds((v4 + 3, -1, v6 - 3), (v4 + 3, -1, v6 + 2), v7, v7, true);
    f.bounds((v4 - 3, -1, v6 - 3), (v4 + 3, -1, v6 - 2), v7, v7, true);
    f.bounds((v4 - 3, -1, v6 + 3), (v4 + 3, -1, v6 + 3), v7, v7, true);
    sand_box(f, sand, (v4 - 2, v5 + 1, v6 - 2), (v4 + 2, v5 + 3, v6 + 2));
    *roof = collapse(f, (v4 - 2, v5 + 4, v6 - 2), (v4 + 2, v6 + 2));
    let v9 = plain(Kind::OrangeTerracotta);
    let v10 = plain(Kind::BlueTerracotta);
    f.place(v10, v4, v5, v6);
    f.place(v9, v4 + 1, v5, v6 - 1);
    f.place(v9, v4 + 1, v5, v6 + 1);
    f.place(v9, v4 - 1, v5, v6 - 1);
    f.place(v9, v4 - 1, v5, v6 + 1);
    f.place(v9, v4 + 2, v5, v6);
    f.place(v9, v4 - 2, v5, v6);
    f.place(v9, v4, v5, v6 + 2);
    f.place(v9, v4, v5, v6 - 2);
    f.place(v9, v4 + 3, v5, v6);
    sand.push(f.world_pos(v4 + 3, v5 + 1, v6));
    sand.push(f.world_pos(v4 + 3, v5 + 2, v6));
    f.place(v7, v4 + 4, v5 + 1, v6);
    f.place(v8, v4 + 4, v5 + 2, v6);
    f.place(v9, v4 - 3, v5, v6);
    sand.push(f.world_pos(v4 - 3, v5 + 1, v6));
    sand.push(f.world_pos(v4 - 3, v5 + 2, v6));
    f.place(v7, v4 - 4, v5 + 1, v6);
    f.place(v8, v4 - 4, v5 + 2, v6);
    f.place(v9, v4, v5, v6 + 3);
    sand.push(f.world_pos(v4, v5 + 1, v6 + 3));
    sand.push(f.world_pos(v4, v5 + 2, v6 + 3));
    f.place(v9, v4, v5, v6 - 3);
    sand.push(f.world_pos(v4, v5 + 1, v6 - 3));
    sand.push(f.world_pos(v4, v5 + 2, v6 - 3));
    f.place(v7, v4, v5 + 1, v6 - 4);
    f.place(v8, v4, -2, v6 - 4);
}

fn body(f: &mut Frame, width: i32, depth: i32) {
    f.bounds(
        (0, -4, 0),
        (width - 1, 0, depth - 1),
        plain(Kind::Sandstone),
        plain(Kind::Sandstone),
        false,
    );
    let mut v8 = 1;
    while v8 <= 9 {
        f.bounds(
            (v8, v8, v8),
            ((width - 1) - v8, v8, (depth - 1) - v8),
            plain(Kind::Sandstone),
            plain(Kind::Sandstone),
            false,
        );
        f.bounds(
            (v8 + 1, v8, v8 + 1),
            ((width - 2) - v8, v8, (depth - 2) - v8),
            plain(Kind::Air),
            plain(Kind::Air),
            false,
        );
        v8 += 1;
    }
    v8 = 0;
    while v8 < width {
        let mut v9 = 0;
        while v9 < depth {
            f.column_down(plain(Kind::Sandstone), v8, -5, v9);
            v9 += 1;
        }
        v8 += 1;
    }
    let north = with(Kind::SandstoneStairs, &[("facing", "north")]);
    let south = with(Kind::SandstoneStairs, &[("facing", "south")]);
    let east = with(Kind::SandstoneStairs, &[("facing", "east")]);
    let west = with(Kind::SandstoneStairs, &[("facing", "west")]);
    f.bounds(
        (0, 0, 0),
        (4, 9, 4),
        plain(Kind::Sandstone),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (1, 10, 1),
        (3, 10, 3),
        plain(Kind::Sandstone),
        plain(Kind::Sandstone),
        false,
    );
    f.place(north, 2, 10, 0);
    f.place(south, 2, 10, 4);
    f.place(east, 0, 10, 2);
    f.place(west, 4, 10, 2);
    f.bounds(
        (width - 5, 0, 0),
        (width - 1, 9, 4),
        plain(Kind::Sandstone),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (width - 4, 10, 1),
        (width - 2, 10, 3),
        plain(Kind::Sandstone),
        plain(Kind::Sandstone),
        false,
    );
    f.place(north, width - 3, 10, 0);
    f.place(south, width - 3, 10, 4);
    f.place(east, width - 5, 10, 2);
    f.place(west, width - 1, 10, 2);
    f.bounds(
        (8, 0, 0),
        (12, 4, 4),
        plain(Kind::Sandstone),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (9, 1, 0),
        (11, 3, 4),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.place(plain(Kind::CutSandstone), 9, 1, 1);
    f.place(plain(Kind::CutSandstone), 9, 2, 1);
    f.place(plain(Kind::CutSandstone), 9, 3, 1);
    f.place(plain(Kind::CutSandstone), 10, 3, 1);
    f.place(plain(Kind::CutSandstone), 11, 3, 1);
    f.place(plain(Kind::CutSandstone), 11, 2, 1);
    f.place(plain(Kind::CutSandstone), 11, 1, 1);
    f.bounds(
        (4, 1, 1),
        (8, 3, 3),
        plain(Kind::Sandstone),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (4, 1, 2),
        (8, 2, 2),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (12, 1, 1),
        (16, 3, 3),
        plain(Kind::Sandstone),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (12, 1, 2),
        (16, 2, 2),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (5, 4, 5),
        (width - 6, 4, depth - 6),
        plain(Kind::Sandstone),
        plain(Kind::Sandstone),
        false,
    );
    f.bounds(
        (9, 4, 9),
        (11, 4, 11),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (8, 1, 8),
        (8, 3, 8),
        plain(Kind::CutSandstone),
        plain(Kind::CutSandstone),
        false,
    );
    f.bounds(
        (12, 1, 8),
        (12, 3, 8),
        plain(Kind::CutSandstone),
        plain(Kind::CutSandstone),
        false,
    );
    f.bounds(
        (8, 1, 12),
        (8, 3, 12),
        plain(Kind::CutSandstone),
        plain(Kind::CutSandstone),
        false,
    );
    f.bounds(
        (12, 1, 12),
        (12, 3, 12),
        plain(Kind::CutSandstone),
        plain(Kind::CutSandstone),
        false,
    );
    f.bounds(
        (1, 1, 5),
        (4, 4, 11),
        plain(Kind::Sandstone),
        plain(Kind::Sandstone),
        false,
    );
    f.bounds(
        (width - 5, 1, 5),
        (width - 2, 4, 11),
        plain(Kind::Sandstone),
        plain(Kind::Sandstone),
        false,
    );
    f.bounds(
        (6, 7, 9),
        (6, 7, 11),
        plain(Kind::Sandstone),
        plain(Kind::Sandstone),
        false,
    );
    f.bounds(
        (width - 7, 7, 9),
        (width - 7, 7, 11),
        plain(Kind::Sandstone),
        plain(Kind::Sandstone),
        false,
    );
    f.bounds(
        (5, 5, 9),
        (5, 7, 11),
        plain(Kind::CutSandstone),
        plain(Kind::CutSandstone),
        false,
    );
    f.bounds(
        (width - 6, 5, 9),
        (width - 6, 7, 11),
        plain(Kind::CutSandstone),
        plain(Kind::CutSandstone),
        false,
    );
    f.place(plain(Kind::Air), 5, 5, 10);
    f.place(plain(Kind::Air), 5, 6, 10);
    f.place(plain(Kind::Air), 6, 6, 10);
    f.place(plain(Kind::Air), width - 6, 5, 10);
    f.place(plain(Kind::Air), width - 6, 6, 10);
    f.place(plain(Kind::Air), width - 7, 6, 10);
    f.bounds(
        (2, 4, 4),
        (2, 6, 4),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (width - 3, 4, 4),
        (width - 3, 6, 4),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.place(north, 2, 4, 5);
    f.place(north, 2, 3, 4);
    f.place(north, width - 3, 4, 5);
    f.place(north, width - 3, 3, 4);
    f.bounds(
        (1, 1, 3),
        (2, 2, 3),
        plain(Kind::Sandstone),
        plain(Kind::Sandstone),
        false,
    );
    f.bounds(
        (width - 3, 1, 3),
        (width - 2, 2, 3),
        plain(Kind::Sandstone),
        plain(Kind::Sandstone),
        false,
    );
    f.place(plain(Kind::Sandstone), 1, 1, 2);
    f.place(plain(Kind::Sandstone), width - 2, 1, 2);
    f.place(plain(Kind::SandstoneSlab), 1, 2, 2);
    f.place(plain(Kind::SandstoneSlab), width - 2, 2, 2);
    f.place(west, 2, 1, 2);
    f.place(east, width - 3, 1, 2);
    f.bounds(
        (4, 3, 5),
        (4, 3, 17),
        plain(Kind::Sandstone),
        plain(Kind::Sandstone),
        false,
    );
    f.bounds(
        (width - 5, 3, 5),
        (width - 5, 3, 17),
        plain(Kind::Sandstone),
        plain(Kind::Sandstone),
        false,
    );
    f.bounds(
        (3, 1, 5),
        (4, 2, 16),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (width - 6, 1, 5),
        (width - 5, 2, 16),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    let mut v12 = 5;
    while v12 <= 17 {
        f.place(plain(Kind::CutSandstone), 4, 1, v12);
        f.place(plain(Kind::ChiseledSandstone), 4, 2, v12);
        f.place(plain(Kind::CutSandstone), width - 5, 1, v12);
        f.place(plain(Kind::ChiseledSandstone), width - 5, 2, v12);
        v12 += 2;
    }
    f.place(plain(Kind::OrangeTerracotta), 10, 0, 7);
    f.place(plain(Kind::OrangeTerracotta), 10, 0, 8);
    f.place(plain(Kind::OrangeTerracotta), 9, 0, 9);
    f.place(plain(Kind::OrangeTerracotta), 11, 0, 9);
    f.place(plain(Kind::OrangeTerracotta), 8, 0, 10);
    f.place(plain(Kind::OrangeTerracotta), 12, 0, 10);
    f.place(plain(Kind::OrangeTerracotta), 7, 0, 10);
    f.place(plain(Kind::OrangeTerracotta), 13, 0, 10);
    f.place(plain(Kind::OrangeTerracotta), 9, 0, 11);
    f.place(plain(Kind::OrangeTerracotta), 11, 0, 11);
    f.place(plain(Kind::OrangeTerracotta), 10, 0, 12);
    f.place(plain(Kind::OrangeTerracotta), 10, 0, 13);
    f.place(plain(Kind::BlueTerracotta), 10, 0, 10);
    v12 = 0;
    while v12 < width {
        f.place(plain(Kind::CutSandstone), v12, 2, 1);
        f.place(plain(Kind::OrangeTerracotta), v12, 2, 2);
        f.place(plain(Kind::CutSandstone), v12, 2, 3);
        f.place(plain(Kind::CutSandstone), v12, 3, 1);
        f.place(plain(Kind::OrangeTerracotta), v12, 3, 2);
        f.place(plain(Kind::CutSandstone), v12, 3, 3);
        f.place(plain(Kind::OrangeTerracotta), v12, 4, 1);
        f.place(plain(Kind::ChiseledSandstone), v12, 4, 2);
        f.place(plain(Kind::OrangeTerracotta), v12, 4, 3);
        f.place(plain(Kind::CutSandstone), v12, 5, 1);
        f.place(plain(Kind::OrangeTerracotta), v12, 5, 2);
        f.place(plain(Kind::CutSandstone), v12, 5, 3);
        f.place(plain(Kind::OrangeTerracotta), v12, 6, 1);
        f.place(plain(Kind::ChiseledSandstone), v12, 6, 2);
        f.place(plain(Kind::OrangeTerracotta), v12, 6, 3);
        f.place(plain(Kind::OrangeTerracotta), v12, 7, 1);
        f.place(plain(Kind::OrangeTerracotta), v12, 7, 2);
        f.place(plain(Kind::OrangeTerracotta), v12, 7, 3);
        f.place(plain(Kind::CutSandstone), v12, 8, 1);
        f.place(plain(Kind::CutSandstone), v12, 8, 2);
        f.place(plain(Kind::CutSandstone), v12, 8, 3);
        v12 += width - 1;
    }
    v12 = 2;
    while v12 <= width - 3 {
        f.place(plain(Kind::CutSandstone), v12 - 1, 2, 0);
        f.place(plain(Kind::OrangeTerracotta), v12, 2, 0);
        f.place(plain(Kind::CutSandstone), v12 + 1, 2, 0);
        f.place(plain(Kind::CutSandstone), v12 - 1, 3, 0);
        f.place(plain(Kind::OrangeTerracotta), v12, 3, 0);
        f.place(plain(Kind::CutSandstone), v12 + 1, 3, 0);
        f.place(plain(Kind::OrangeTerracotta), v12 - 1, 4, 0);
        f.place(plain(Kind::ChiseledSandstone), v12, 4, 0);
        f.place(plain(Kind::OrangeTerracotta), v12 + 1, 4, 0);
        f.place(plain(Kind::CutSandstone), v12 - 1, 5, 0);
        f.place(plain(Kind::OrangeTerracotta), v12, 5, 0);
        f.place(plain(Kind::CutSandstone), v12 + 1, 5, 0);
        f.place(plain(Kind::OrangeTerracotta), v12 - 1, 6, 0);
        f.place(plain(Kind::ChiseledSandstone), v12, 6, 0);
        f.place(plain(Kind::OrangeTerracotta), v12 + 1, 6, 0);
        f.place(plain(Kind::OrangeTerracotta), v12 - 1, 7, 0);
        f.place(plain(Kind::OrangeTerracotta), v12, 7, 0);
        f.place(plain(Kind::OrangeTerracotta), v12 + 1, 7, 0);
        f.place(plain(Kind::CutSandstone), v12 - 1, 8, 0);
        f.place(plain(Kind::CutSandstone), v12, 8, 0);
        f.place(plain(Kind::CutSandstone), v12 + 1, 8, 0);
        v12 += (width - 3) - 2;
    }
    f.bounds(
        (8, 4, 0),
        (12, 6, 0),
        plain(Kind::CutSandstone),
        plain(Kind::CutSandstone),
        false,
    );
    f.place(plain(Kind::Air), 8, 6, 0);
    f.place(plain(Kind::Air), 12, 6, 0);
    f.place(plain(Kind::OrangeTerracotta), 9, 5, 0);
    f.place(plain(Kind::ChiseledSandstone), 10, 5, 0);
    f.place(plain(Kind::OrangeTerracotta), 11, 5, 0);
    f.bounds(
        (8, -14, 8),
        (12, -11, 12),
        plain(Kind::CutSandstone),
        plain(Kind::CutSandstone),
        false,
    );
    f.bounds(
        (8, -10, 8),
        (12, -10, 12),
        plain(Kind::ChiseledSandstone),
        plain(Kind::ChiseledSandstone),
        false,
    );
    f.bounds(
        (8, -9, 8),
        (12, -9, 12),
        plain(Kind::CutSandstone),
        plain(Kind::CutSandstone),
        false,
    );
    f.bounds(
        (8, -8, 8),
        (12, -1, 12),
        plain(Kind::Sandstone),
        plain(Kind::Sandstone),
        false,
    );
    f.bounds(
        (9, -11, 9),
        (11, -1, 11),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.place(plain(Kind::StonePressurePlate), 10, -11, 10);
    f.bounds(
        (9, -13, 9),
        (11, -13, 11),
        plain(Kind::Tnt),
        plain(Kind::Air),
        false,
    );
    f.place(plain(Kind::Air), 8, -11, 10);
    f.place(plain(Kind::Air), 8, -10, 10);
    f.place(plain(Kind::ChiseledSandstone), 7, -10, 10);
    f.place(plain(Kind::CutSandstone), 7, -11, 10);
    f.place(plain(Kind::Air), 12, -11, 10);
    f.place(plain(Kind::Air), 12, -10, 10);
    f.place(plain(Kind::ChiseledSandstone), 13, -10, 10);
    f.place(plain(Kind::CutSandstone), 13, -11, 10);
    f.place(plain(Kind::Air), 10, -11, 8);
    f.place(plain(Kind::Air), 10, -10, 8);
    f.place(plain(Kind::ChiseledSandstone), 10, -10, 7);
    f.place(plain(Kind::CutSandstone), 10, -11, 7);
    f.place(plain(Kind::Air), 10, -11, 12);
    f.place(plain(Kind::Air), 10, -10, 12);
    f.place(plain(Kind::ChiseledSandstone), 10, -10, 13);
    f.place(plain(Kind::CutSandstone), 10, -11, 13);
}

fn suspicious(f: &mut Frame, at: Pos) {
    if !f.chunk.contains(at) {
        return;
    }
    f.set_raw(at, plain(Kind::SuspiciousSand));
    let seed = ((i64::from(at.0) & 0x3FF_FFFF) << 38)
        | ((i64::from(at.2) & 0x3FF_FFFF) << 12)
        | (i64::from(at.1) & 0xFFF);
    f.run.region.block_entities.push((
        at,
        json!({ "id": "minecraft:brushable_block", "LootTable": ARCHAEOLOGY, "LootTableSeed": { "$l": seed } }),
    ));
}

fn dust(f: &mut Frame, bbox: &Bbox, sand: Vec<Pos>, roof: Pos) {
    suspicious(f, roof);
    let mut list = sand;
    list.sort_by_key(|at| (at.1, at.2, at.0));
    list.dedup();
    let mut base = JavaRandom::new(f.run.seed);
    let factory = base.next_long();
    let middle = (
        bbox.min.0 + (bbox.max.0 - bbox.min.0 + 1) / 2,
        bbox.min.1 + (bbox.max.1 - bbox.min.1 + 1) / 2,
        bbox.min.2 + (bbox.max.2 - bbox.min.2 + 1) / 2,
    );
    let mut random = JavaRandom::new(position_seed(middle) ^ factory);
    for i in (2..=list.len()).rev() {
        let j = random.next_int(i as i32) as usize;
        list.swap(i - 1, j);
    }
    let mut count = (list.len() as i32).min(5 + random.next_int(3));
    for at in list {
        if count < 1 {
            if f.chunk.contains(at) {
                f.set_raw(at, plain(Kind::Sand));
            }
        } else {
            count -= 1;
            suspicious(f, at);
        }
    }
}

impl Piece {
    pub(crate) fn place_desert(&self, run: &mut Run, write: &Bbox) {
        let _ = run.rng.below(3);
        let height = self.lowest(run);
        let bbox = self.moved(height, -self.drop);
        let (width, depth) = (21, 21);
        let mut sand = Vec::new();
        let mut roof = (0, 0, 0);
        let mut f = Frame::new(run, bbox, Some(self.facing), *write, Guard::Open).oriented();
        body(&mut f, width, depth);
        for dir in [Dir::North, Dir::East, Dir::South, Dir::West] {
            let (dx, _, dz) = dir.step();
            f.chest(10 + dx * 2, -11, 10 + dz * 2, CHEST);
        }
        let at = (16, -4, 13);
        cellar_stairs(&mut f, at);
        cellar_room(&mut f, &mut sand, &mut roof, at);
        dust(&mut f, &bbox, sand, roof);
        f.finish();
    }
}
