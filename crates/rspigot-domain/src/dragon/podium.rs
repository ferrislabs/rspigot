use crate::BlockPos;

pub const PORTAL_RING_NEAR: f64 = 2.5;
pub const PORTAL_RING_FAR: f64 = 3.5;
pub const PILLAR_HEIGHT: i32 = 4;
pub const PODIUM_TORCH_HEIGHT: i32 = 2;
const COLUMN_TOP: i32 = 32;
const EDGE: i32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Podium {
    Bedrock,
    EndStone,
    Air,
    EndPortal,
    WallTorch(Facing),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Facing {
    North,
    South,
    West,
    East,
}

impl Facing {
    pub const HORIZONTAL: [Self; 4] = [Self::North, Self::East, Self::South, Self::West];

    pub const fn offset(self) -> (i32, i32) {
        match self {
            Self::North => (0, -1),
            Self::South => (0, 1),
            Self::West => (-1, 0),
            Self::East => (1, 0),
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::North => "north",
            Self::South => "south",
            Self::West => "west",
            Self::East => "east",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PodiumBlock {
    pub pos: BlockPos,
    pub block: Podium,
    pub drop_previous: bool,
}

fn closer_than(pos: BlockPos, origin: BlockPos, distance: f64) -> bool {
    let dx = f64::from(pos.x - origin.x);
    let dy = f64::from(pos.y - origin.y);
    let dz = f64::from(pos.z - origin.z);
    dx * dx + dy * dy + dz * dz < distance * distance
}

pub fn podium_blocks(origin: BlockPos, active: bool) -> Vec<PodiumBlock> {
    let mut out = Vec::new();
    for z in origin.z - EDGE..=origin.z + EDGE {
        for y in origin.y - 1..=origin.y + COLUMN_TOP {
            for x in origin.x - EDGE..=origin.x + EDGE {
                let pos = BlockPos::new(x, y, z);
                let near = closer_than(pos, origin, PORTAL_RING_NEAR);
                if !(near || closer_than(pos, origin, PORTAL_RING_FAR)) {
                    continue;
                }
                let (block, drop_previous) = if pos.y < origin.y {
                    if near {
                        (Podium::Bedrock, false)
                    } else {
                        (Podium::EndStone, active)
                    }
                } else if pos.y > origin.y {
                    (Podium::Air, active)
                } else if !near {
                    (Podium::Bedrock, false)
                } else if active {
                    (Podium::EndPortal, true)
                } else {
                    (Podium::Air, false)
                };
                out.push(PodiumBlock {
                    pos,
                    block,
                    drop_previous,
                });
            }
        }
    }
    for height in 0..PILLAR_HEIGHT {
        out.push(PodiumBlock {
            pos: origin.offset(0, height, 0),
            block: Podium::Bedrock,
            drop_previous: false,
        });
    }
    let torch_row = origin.offset(0, PODIUM_TORCH_HEIGHT, 0);
    for facing in Facing::HORIZONTAL {
        let (dx, dz) = facing.offset();
        out.push(PodiumBlock {
            pos: torch_row.offset(dx, 0, dz),
            block: Podium::WallTorch(facing),
            drop_previous: false,
        });
    }
    out
}

pub fn exit_portal_bedrock() -> Vec<(i32, i32, i32)> {
    const BELOW: [&str; 7] = [
        "       ", "  ###  ", " ##### ", " ##### ", " ##### ", "  ###  ", "       ",
    ];
    const LEVEL: [&str; 7] = [
        "  ###  ", " #   # ", "#     #", "#  #  #", "#     #", " #   # ", "  ###  ",
    ];
    let mut out = Vec::new();
    for (dy, rows) in [(-1, BELOW), (0, LEVEL)] {
        for (row, text) in rows.iter().enumerate() {
            for (column, ch) in text.chars().enumerate() {
                if ch == '#' {
                    out.push((column as i32 - 3, dy, row as i32 - 3));
                }
            }
        }
    }
    for dy in 1..=3 {
        out.push((0, dy, 0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(blocks: &[PodiumBlock], x: i32, y: i32, z: i32) -> Option<PodiumBlock> {
        blocks
            .iter()
            .copied()
            .rev()
            .find(|block| block.pos == BlockPos::new(x, y, z))
    }

    #[test]
    fn the_inactive_podium_is_a_bedrock_ring_around_an_empty_hole() {
        let blocks = podium_blocks(BlockPos::new(0, 64, 0), false);
        assert_eq!(at(&blocks, 1, 64, 0).unwrap().block, Podium::Air);
        assert_eq!(at(&blocks, 3, 64, 0).unwrap().block, Podium::Bedrock);
        assert_eq!(at(&blocks, 2, 64, 2).unwrap().block, Podium::Bedrock);
        assert_eq!(at(&blocks, 1, 63, 0).unwrap().block, Podium::Bedrock);
        assert_eq!(at(&blocks, 3, 63, 0).unwrap().block, Podium::EndStone);
        assert_eq!(at(&blocks, 1, 66, 1).unwrap().block, Podium::Air);
    }

    #[test]
    fn the_active_podium_has_end_portal_blocks_and_drops_what_it_replaces() {
        let blocks = podium_blocks(BlockPos::new(0, 64, 0), true);
        let portal = at(&blocks, 1, 64, 0).unwrap();
        assert_eq!(portal.block, Podium::EndPortal);
        assert!(portal.drop_previous);
        assert!(at(&blocks, 3, 63, 0).unwrap().drop_previous);
        assert!(at(&blocks, 1, 66, 1).unwrap().drop_previous);
        assert!(!at(&blocks, 3, 64, 0).unwrap().drop_previous);
    }

    #[test]
    fn a_bedrock_pillar_with_four_blocks_and_four_wall_torches_stand_in_the_middle() {
        let blocks = podium_blocks(BlockPos::new(0, 64, 0), true);
        for y in 64..68 {
            assert_eq!(at(&blocks, 0, y, 0).unwrap().block, Podium::Bedrock);
        }
        assert_eq!(
            at(&blocks, 1, 66, 0).unwrap().block,
            Podium::WallTorch(Facing::East)
        );
        assert_eq!(
            at(&blocks, 0, 66, -1).unwrap().block,
            Podium::WallTorch(Facing::North)
        );
        assert_eq!(
            at(&blocks, -1, 66, 0).unwrap().block,
            Podium::WallTorch(Facing::West)
        );
        assert_eq!(
            at(&blocks, 0, 66, 1).unwrap().block,
            Podium::WallTorch(Facing::South)
        );
    }

    #[test]
    fn only_a_sphere_of_three_and_a_half_blocks_is_rewritten() {
        let blocks = podium_blocks(BlockPos::new(0, 64, 0), false);
        assert!(at(&blocks, 0, 67, 0).is_some());
        assert!(at(&blocks, 0, 68, 0).is_none());
        assert!(at(&blocks, 0, 63, 0).is_some());
        assert!(at(&blocks, 0, 62, 0).is_none());
    }

    #[test]
    fn the_exit_portal_pattern_has_a_three_block_pillar_a_ring_and_a_disc_below() {
        let cells = exit_portal_bedrock();
        assert!(cells.contains(&(0, 3, 0)));
        assert!(cells.contains(&(0, 0, 0)));
        assert!(cells.contains(&(3, 0, 0)));
        assert!(!cells.contains(&(1, 0, 0)));
        assert!(cells.contains(&(1, -1, 1)));
        assert!(!cells.contains(&(3, -1, 0)));
        assert_eq!(cells.len(), 17 + 21 + 3);
    }
}
