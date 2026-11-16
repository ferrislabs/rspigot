use rspigot_blocks::BlockKind as Kind;
use rspigot_domain::BlockStateId;

use super::frame::{Frame, Guard, plain, with};
use super::scatter::Piece;
use crate::climate::Xoroshiro;
use crate::overworld::features::place::Run;
use crate::structures::bbox::Bbox;
use crate::structures::blueprint::Dir;

fn moss(random: &mut Xoroshiro, _edge: bool) -> BlockStateId {
    if random.next_float() < 0.4 {
        plain(Kind::Cobblestone)
    } else {
        plain(Kind::MossyCobblestone)
    }
}

fn body(f: &mut Frame, width: i32, depth: i32) {
    f.selected((0, -4, 0), (width - 1, 0, depth - 1), false, moss);
    f.selected((2, 1, 2), (9, 2, 2), false, moss);
    f.selected((2, 1, 12), (9, 2, 12), false, moss);
    f.selected((2, 1, 3), (2, 2, 11), false, moss);
    f.selected((9, 1, 3), (9, 2, 11), false, moss);
    f.selected((1, 3, 1), (10, 6, 1), false, moss);
    f.selected((1, 3, 13), (10, 6, 13), false, moss);
    f.selected((1, 3, 2), (1, 6, 12), false, moss);
    f.selected((10, 3, 2), (10, 6, 12), false, moss);
    f.selected((2, 3, 2), (9, 3, 12), false, moss);
    f.selected((2, 6, 2), (9, 6, 12), false, moss);
    f.selected((3, 7, 3), (8, 7, 11), false, moss);
    f.selected((4, 8, 4), (7, 8, 10), false, moss);
    f.bounds(
        (3, 1, 3),
        (8, 2, 11),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (4, 3, 6),
        (7, 3, 9),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (2, 4, 2),
        (9, 5, 12),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (4, 6, 5),
        (7, 6, 9),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (5, 7, 6),
        (6, 7, 8),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (5, 1, 2),
        (6, 2, 2),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (5, 2, 12),
        (6, 2, 12),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (5, 5, 1),
        (6, 5, 1),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (5, 5, 13),
        (6, 5, 13),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.place(plain(Kind::Air), 1, 5, 5);
    f.place(plain(Kind::Air), 10, 5, 5);
    f.place(plain(Kind::Air), 1, 5, 9);
    f.place(plain(Kind::Air), 10, 5, 9);
    let mut v8 = 0;
    while v8 <= 14 {
        f.selected((2, 4, v8), (2, 5, v8), false, moss);
        f.selected((4, 4, v8), (4, 5, v8), false, moss);
        f.selected((7, 4, v8), (7, 5, v8), false, moss);
        f.selected((9, 4, v8), (9, 5, v8), false, moss);
        v8 += 14;
    }
    f.selected((5, 6, 0), (6, 6, 0), false, moss);
    v8 = 0;
    while v8 <= 11 {
        let mut v9 = 2;
        while v9 <= 12 {
            f.selected((v8, 4, v9), (v8, 5, v9), false, moss);
            v9 += 2;
        }
        f.selected((v8, 6, 5), (v8, 6, 5), false, moss);
        f.selected((v8, 6, 9), (v8, 6, 9), false, moss);
        v8 += 11;
    }
    f.selected((2, 7, 2), (2, 9, 2), false, moss);
    f.selected((9, 7, 2), (9, 9, 2), false, moss);
    f.selected((2, 7, 12), (2, 9, 12), false, moss);
    f.selected((9, 7, 12), (9, 9, 12), false, moss);
    f.selected((4, 9, 4), (4, 9, 4), false, moss);
    f.selected((7, 9, 4), (7, 9, 4), false, moss);
    f.selected((4, 9, 10), (4, 9, 10), false, moss);
    f.selected((7, 9, 10), (7, 9, 10), false, moss);
    f.selected((5, 9, 7), (6, 9, 7), false, moss);
    let east_stairs = with(Kind::CobblestoneStairs, &[("facing", "east")]);
    let west_stairs = with(Kind::CobblestoneStairs, &[("facing", "west")]);
    let south_stairs = with(Kind::CobblestoneStairs, &[("facing", "south")]);
    let north_stairs = with(Kind::CobblestoneStairs, &[("facing", "north")]);
    f.place(north_stairs, 5, 9, 6);
    f.place(north_stairs, 6, 9, 6);
    f.place(south_stairs, 5, 9, 8);
    f.place(south_stairs, 6, 9, 8);
    f.place(north_stairs, 4, 0, 0);
    f.place(north_stairs, 5, 0, 0);
    f.place(north_stairs, 6, 0, 0);
    f.place(north_stairs, 7, 0, 0);
    f.place(north_stairs, 4, 1, 8);
    f.place(north_stairs, 4, 2, 9);
    f.place(north_stairs, 4, 3, 10);
    f.place(north_stairs, 7, 1, 8);
    f.place(north_stairs, 7, 2, 9);
    f.place(north_stairs, 7, 3, 10);
    f.selected((4, 1, 9), (4, 1, 9), false, moss);
    f.selected((7, 1, 9), (7, 1, 9), false, moss);
    f.selected((4, 1, 10), (7, 2, 10), false, moss);
    f.selected((5, 4, 5), (6, 4, 5), false, moss);
    f.place(east_stairs, 4, 4, 5);
    f.place(west_stairs, 7, 4, 5);
    let mut v12 = 0;
    while v12 < 4 {
        f.place(south_stairs, 5, 0 - v12, 6 + v12);
        f.place(south_stairs, 6, 0 - v12, 6 + v12);
        f.bounds(
            (5, 0 - v12, 7 + v12),
            (6, 0 - v12, 9 + v12),
            plain(Kind::Air),
            plain(Kind::Air),
            false,
        );
        v12 += 1;
    }
    f.bounds(
        (1, -3, 12),
        (10, -1, 13),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (1, -3, 1),
        (3, -1, 13),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.bounds(
        (1, -3, 1),
        (9, -1, 5),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    v12 = 1;
    while v12 <= 13 {
        f.selected((1, -3, v12), (1, -2, v12), false, moss);
        v12 += 2;
    }
    v12 = 2;
    while v12 <= 12 {
        f.selected((1, -1, v12), (3, -1, v12), false, moss);
        v12 += 2;
    }
    f.selected((2, -2, 1), (5, -2, 1), false, moss);
    f.selected((7, -2, 1), (9, -2, 1), false, moss);
    f.selected((6, -3, 1), (6, -3, 1), false, moss);
    f.selected((6, -1, 1), (6, -1, 1), false, moss);
    f.place(
        with(
            Kind::TripwireHook,
            &[("facing", "east"), ("attached", "true")],
        ),
        1,
        -3,
        8,
    );
    f.place(
        with(
            Kind::TripwireHook,
            &[("facing", "west"), ("attached", "true")],
        ),
        4,
        -3,
        8,
    );
    f.place(
        with(
            Kind::Tripwire,
            &[("east", "true"), ("west", "true"), ("attached", "true")],
        ),
        2,
        -3,
        8,
    );
    f.place(
        with(
            Kind::Tripwire,
            &[("east", "true"), ("west", "true"), ("attached", "true")],
        ),
        3,
        -3,
        8,
    );
    let v12_1 = with(Kind::RedstoneWire, &[("north", "side"), ("south", "side")]);
    f.place(v12_1, 5, -3, 7);
    f.place(v12_1, 5, -3, 6);
    f.place(v12_1, 5, -3, 5);
    f.place(v12_1, 5, -3, 4);
    f.place(v12_1, 5, -3, 3);
    f.place(v12_1, 5, -3, 2);
    f.place(
        with(Kind::RedstoneWire, &[("north", "side"), ("west", "side")]),
        5,
        -3,
        1,
    );
    f.place(
        with(Kind::RedstoneWire, &[("east", "side"), ("west", "side")]),
        4,
        -3,
        1,
    );
    f.place(plain(Kind::MossyCobblestone), 3, -3, 1);
    f.dispenser(
        3,
        -2,
        1,
        Dir::North,
        "minecraft:chests/jungle_temple_dispenser",
    );
    f.place(with(Kind::Vine, &[("south", "true")]), 3, -2, 2);
    f.place(
        with(
            Kind::TripwireHook,
            &[("facing", "north"), ("attached", "true")],
        ),
        7,
        -3,
        1,
    );
    f.place(
        with(
            Kind::TripwireHook,
            &[("facing", "south"), ("attached", "true")],
        ),
        7,
        -3,
        5,
    );
    f.place(
        with(
            Kind::Tripwire,
            &[("north", "true"), ("south", "true"), ("attached", "true")],
        ),
        7,
        -3,
        2,
    );
    f.place(
        with(
            Kind::Tripwire,
            &[("north", "true"), ("south", "true"), ("attached", "true")],
        ),
        7,
        -3,
        3,
    );
    f.place(
        with(
            Kind::Tripwire,
            &[("north", "true"), ("south", "true"), ("attached", "true")],
        ),
        7,
        -3,
        4,
    );
    f.place(
        with(Kind::RedstoneWire, &[("east", "side"), ("west", "side")]),
        8,
        -3,
        6,
    );
    f.place(
        with(Kind::RedstoneWire, &[("west", "side"), ("south", "side")]),
        9,
        -3,
        6,
    );
    f.place(
        with(Kind::RedstoneWire, &[("north", "side"), ("south", "up")]),
        9,
        -3,
        5,
    );
    f.place(plain(Kind::MossyCobblestone), 9, -3, 4);
    f.place(v12_1, 9, -2, 4);
    f.dispenser(
        9,
        -2,
        3,
        Dir::West,
        "minecraft:chests/jungle_temple_dispenser",
    );
    f.place(with(Kind::Vine, &[("east", "true")]), 8, -1, 3);
    f.place(with(Kind::Vine, &[("east", "true")]), 8, -2, 3);
    f.chest(8, -3, 3, "minecraft:chests/jungle_temple");
    f.place(plain(Kind::MossyCobblestone), 9, -3, 2);
    f.place(plain(Kind::MossyCobblestone), 8, -3, 1);
    f.place(plain(Kind::MossyCobblestone), 4, -3, 5);
    f.place(plain(Kind::MossyCobblestone), 5, -2, 5);
    f.place(plain(Kind::MossyCobblestone), 5, -1, 5);
    f.place(plain(Kind::MossyCobblestone), 6, -3, 5);
    f.place(plain(Kind::MossyCobblestone), 7, -2, 5);
    f.place(plain(Kind::MossyCobblestone), 7, -1, 5);
    f.place(plain(Kind::MossyCobblestone), 8, -3, 5);
    f.selected((9, -1, 1), (9, -1, 5), false, moss);
    f.bounds(
        (8, -3, 8),
        (10, -1, 10),
        plain(Kind::Air),
        plain(Kind::Air),
        false,
    );
    f.place(plain(Kind::ChiseledStoneBricks), 8, -2, 11);
    f.place(plain(Kind::ChiseledStoneBricks), 9, -2, 11);
    f.place(plain(Kind::ChiseledStoneBricks), 10, -2, 11);
    let v13 = with(Kind::Lever, &[("facing", "north"), ("face", "wall")]);
    f.place(v13, 8, -2, 12);
    f.place(v13, 9, -2, 12);
    f.place(v13, 10, -2, 12);
    f.selected((8, -3, 8), (8, -3, 10), false, moss);
    f.selected((10, -3, 8), (10, -3, 10), false, moss);
    f.place(plain(Kind::MossyCobblestone), 10, -2, 9);
    f.place(v12_1, 8, -2, 9);
    f.place(v12_1, 8, -2, 10);
    f.place(
        with(
            Kind::RedstoneWire,
            &[
                ("north", "side"),
                ("south", "side"),
                ("east", "side"),
                ("west", "side"),
            ],
        ),
        10,
        -1,
        9,
    );
    f.place(with(Kind::StickyPiston, &[("facing", "up")]), 9, -2, 8);
    f.place(with(Kind::StickyPiston, &[("facing", "west")]), 10, -2, 8);
    f.place(with(Kind::StickyPiston, &[("facing", "west")]), 10, -1, 8);
    f.place(with(Kind::Repeater, &[("facing", "north")]), 10, -2, 10);
    f.chest(9, -3, 10, "minecraft:chests/jungle_temple");
}

impl Piece {
    pub(crate) fn place_jungle(&self, run: &mut Run, write: &Bbox) {
        let Some(height) = self.average(run) else {
            return;
        };
        let bbox = self.moved(height, 0);
        let mut f = Frame::new(run, bbox, Some(self.facing), *write, Guard::Open).oriented();
        body(&mut f, 12, 15);
        f.finish();
    }
}
