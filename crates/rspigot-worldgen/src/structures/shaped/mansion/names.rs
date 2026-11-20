use crate::legacy::JavaRandom;

pub(crate) const ENTRANCE: &str = "minecraft:woodland_mansion/entrance";
pub(crate) const CORRIDOR: &str = "minecraft:woodland_mansion/corridor_floor";
pub(crate) const CARPET_NORTH: &str = "minecraft:woodland_mansion/carpet_north";
pub(crate) const CARPET_EAST: &str = "minecraft:woodland_mansion/carpet_east";
pub(crate) const WALL_FLAT: &str = "minecraft:woodland_mansion/wall_flat";
pub(crate) const WALL_WINDOW: &str = "minecraft:woodland_mansion/wall_window";
pub(crate) const WALL_CORNER: &str = "minecraft:woodland_mansion/wall_corner";
pub(crate) const SMALL_WALL: &str = "minecraft:woodland_mansion/small_wall";
pub(crate) const SMALL_WALL_CORNER: &str = "minecraft:woodland_mansion/small_wall_corner";
pub(crate) const ROOF: &str = "minecraft:woodland_mansion/roof";
pub(crate) const ROOF_FRONT: &str = "minecraft:woodland_mansion/roof_front";
pub(crate) const ROOF_CORNER: &str = "minecraft:woodland_mansion/roof_corner";
pub(crate) const ROOF_INNER_CORNER: &str = "minecraft:woodland_mansion/roof_inner_corner";
pub(crate) const SECRET_2X2: &str = "minecraft:woodland_mansion/2x2_s1";
const STAIRS_SIDE: &str = "minecraft:woodland_mansion/1x2_c_stairs";
const STAIRS_FRONT: &str = "minecraft:woodland_mansion/1x2_d_stairs";

pub(crate) const CARPETS_SOUTH: [&str; 2] = [
    "minecraft:woodland_mansion/carpet_south_1",
    "minecraft:woodland_mansion/carpet_south_2",
];
pub(crate) const CARPETS_WEST: [&str; 2] = [
    "minecraft:woodland_mansion/carpet_west_1",
    "minecraft:woodland_mansion/carpet_west_2",
];
pub(crate) const INDOOR_WALLS: [&str; 2] = [
    "minecraft:woodland_mansion/indoors_wall_1",
    "minecraft:woodland_mansion/indoors_wall_2",
];
pub(crate) const INDOOR_DOORS: [&str; 2] = [
    "minecraft:woodland_mansion/indoors_door_1",
    "minecraft:woodland_mansion/indoors_door_2",
];

const A_1X1: [&str; 5] = [
    "minecraft:woodland_mansion/1x1_a1",
    "minecraft:woodland_mansion/1x1_a2",
    "minecraft:woodland_mansion/1x1_a3",
    "minecraft:woodland_mansion/1x1_a4",
    "minecraft:woodland_mansion/1x1_a5",
];

const AS_1X1: [&str; 4] = [
    "minecraft:woodland_mansion/1x1_as1",
    "minecraft:woodland_mansion/1x1_as2",
    "minecraft:woodland_mansion/1x1_as3",
    "minecraft:woodland_mansion/1x1_as4",
];

const B_1X1: [&str; 5] = [
    "minecraft:woodland_mansion/1x1_b1",
    "minecraft:woodland_mansion/1x1_b2",
    "minecraft:woodland_mansion/1x1_b3",
    "minecraft:woodland_mansion/1x1_b4",
    "minecraft:woodland_mansion/1x1_b5",
];

const A_1X2: [&str; 9] = [
    "minecraft:woodland_mansion/1x2_a1",
    "minecraft:woodland_mansion/1x2_a2",
    "minecraft:woodland_mansion/1x2_a3",
    "minecraft:woodland_mansion/1x2_a4",
    "minecraft:woodland_mansion/1x2_a5",
    "minecraft:woodland_mansion/1x2_a6",
    "minecraft:woodland_mansion/1x2_a7",
    "minecraft:woodland_mansion/1x2_a8",
    "minecraft:woodland_mansion/1x2_a9",
];

const B_1X2: [&str; 5] = [
    "minecraft:woodland_mansion/1x2_b1",
    "minecraft:woodland_mansion/1x2_b2",
    "minecraft:woodland_mansion/1x2_b3",
    "minecraft:woodland_mansion/1x2_b4",
    "minecraft:woodland_mansion/1x2_b5",
];

const C_1X2: [&str; 4] = [
    "minecraft:woodland_mansion/1x2_c1",
    "minecraft:woodland_mansion/1x2_c2",
    "minecraft:woodland_mansion/1x2_c3",
    "minecraft:woodland_mansion/1x2_c4",
];

const D_1X2: [&str; 5] = [
    "minecraft:woodland_mansion/1x2_d1",
    "minecraft:woodland_mansion/1x2_d2",
    "minecraft:woodland_mansion/1x2_d3",
    "minecraft:woodland_mansion/1x2_d4",
    "minecraft:woodland_mansion/1x2_d5",
];

const S_1X2: [&str; 2] = [
    "minecraft:woodland_mansion/1x2_s1",
    "minecraft:woodland_mansion/1x2_s2",
];

const SE_1X2: [&str; 1] = ["minecraft:woodland_mansion/1x2_se1"];

const A_2X2: [&str; 4] = [
    "minecraft:woodland_mansion/2x2_a1",
    "minecraft:woodland_mansion/2x2_a2",
    "minecraft:woodland_mansion/2x2_a3",
    "minecraft:woodland_mansion/2x2_a4",
];

const B_2X2: [&str; 5] = [
    "minecraft:woodland_mansion/2x2_b1",
    "minecraft:woodland_mansion/2x2_b2",
    "minecraft:woodland_mansion/2x2_b3",
    "minecraft:woodland_mansion/2x2_b4",
    "minecraft:woodland_mansion/2x2_b5",
];

fn pick(names: &[&'static str], random: &mut JavaRandom) -> &'static str {
    names[random.next_int(names.len() as i32) as usize]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Floor {
    First,
    Upper,
}

impl Floor {
    pub(crate) fn of(index: usize) -> Self {
        if index == 0 { Self::First } else { Self::Upper }
    }

    pub(crate) fn one_by_one(self, random: &mut JavaRandom) -> &'static str {
        pick(
            match self {
                Self::First => &A_1X1,
                Self::Upper => &B_1X1,
            },
            random,
        )
    }

    pub(crate) fn one_by_one_secret(self, random: &mut JavaRandom) -> &'static str {
        pick(&AS_1X1, random)
    }

    pub(crate) fn side_entrance(self, random: &mut JavaRandom, stairs: bool) -> &'static str {
        match self {
            Self::First => pick(&A_1X2, random),
            Self::Upper if stairs => STAIRS_SIDE,
            Self::Upper => pick(&C_1X2, random),
        }
    }

    pub(crate) fn front_entrance(self, random: &mut JavaRandom, stairs: bool) -> &'static str {
        match self {
            Self::First => pick(&B_1X2, random),
            Self::Upper if stairs => STAIRS_FRONT,
            Self::Upper => pick(&D_1X2, random),
        }
    }

    pub(crate) fn one_by_two_secret(self, random: &mut JavaRandom) -> &'static str {
        match self {
            Self::First => pick(&S_1X2, random),
            Self::Upper => pick(&SE_1X2, random),
        }
    }

    pub(crate) fn two_by_two(self, random: &mut JavaRandom) -> &'static str {
        match self {
            Self::First => pick(&A_2X2, random),
            Self::Upper => pick(&B_2X2, random),
        }
    }
}
