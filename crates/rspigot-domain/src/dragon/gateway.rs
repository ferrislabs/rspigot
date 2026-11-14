use rspigot_nbt::Compound;

use crate::random::RandomSource;
use crate::{BlockPos, ChunkPos};

pub const SPAWN_TIME: i64 = 200;
pub const COOLDOWN_TIME: i32 = 40;
pub const ATTENTION_INTERVAL: i64 = 2400;
pub const COOLDOWN_EVENT: u8 = 1;
pub const EXIT_HEIGHT: i32 = 10;
pub const ISLAND_HEIGHT: i32 = 75;
pub const EXIT_SEARCH_RADIUS: i32 = 5;
pub const ISLAND_SEARCH_RADIUS: i32 = 16;
pub const TENTATIVE_DISTANCE: f64 = 1024.0;
pub const TENTATIVE_STEP: f64 = 16.0;
pub const TENTATIVE_TRIES: i32 = 16;
pub const SPAWN_SCAN_FLOOR: i32 = 30;
pub const BLOCK_ENTITY: &str = "end_gateway";
const AGE: &str = "Age";
const EXIT: &str = "exit_portal";
const EXACT: &str = "ExactTeleport";
const WORLD_LIMIT: i32 = 30_000_000;
const HEIGHT_LIMIT: i32 = 20_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Gateway {
    pub age: i64,
    pub cooldown: i32,
    pub exit: Option<BlockPos>,
    pub exact: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GatewayTick {
    pub cooldown_started: bool,
    pub changed: bool,
}

impl Gateway {
    pub const fn is_spawning(&self) -> bool {
        self.age < SPAWN_TIME
    }

    pub const fn is_cooling_down(&self) -> bool {
        self.cooldown > 0
    }

    pub fn tick(&mut self) -> GatewayTick {
        let spawning = self.is_spawning();
        let cooling = self.is_cooling_down();
        self.age += 1;
        let mut cooldown_started = false;
        if cooling {
            self.cooldown -= 1;
        } else if self.age % ATTENTION_INTERVAL == 0 {
            self.cooldown = COOLDOWN_TIME;
            cooldown_started = true;
        }
        GatewayTick {
            cooldown_started,
            changed: spawning != self.is_spawning() || cooling != self.is_cooling_down(),
        }
    }

    pub fn trigger_cooldown(&mut self) {
        self.cooldown = COOLDOWN_TIME;
    }

    pub fn write(&self, data: &mut Compound) {
        data.insert(AGE, self.age);
        if let Some(pos) = self.exit {
            data.insert(EXIT, vec![pos.x, pos.y, pos.z]);
        } else {
            data.remove(EXIT);
        }
        if self.exact {
            data.insert(EXACT, true);
        } else {
            data.remove(EXACT);
        }
    }

    pub fn to_nbt(&self) -> Compound {
        let mut data = Compound::new();
        self.write(&mut data);
        data
    }

    pub fn from_nbt(data: &Compound) -> Self {
        Self {
            age: data.get_i64(AGE).unwrap_or(0),
            cooldown: 0,
            exit: data
                .get_int_array(EXIT)
                .filter(|parts| parts.len() == 3)
                .map(|parts| BlockPos::new(parts[0], parts[1], parts[2]))
                .filter(|pos| spawnable(*pos)),
            exact: data.get_bool(EXACT).unwrap_or(false),
        }
    }
}

fn spawnable(pos: BlockPos) -> bool {
    pos.x.abs() < WORLD_LIMIT && pos.z.abs() < WORLD_LIMIT && pos.y.abs() < HEIGHT_LIMIT
}

pub fn exit_direction(gateway: BlockPos) -> (f64, f64) {
    let x = f64::from(gateway.x);
    let z = f64::from(gateway.z);
    let length = x.hypot(z);
    if length < 1.0e-5 {
        (0.0, 0.0)
    } else {
        (x / length, z / length)
    }
}

pub fn tentative_position(
    gateway: BlockPos,
    mut chunk_is_empty: impl FnMut(ChunkPos) -> bool,
) -> (f64, f64) {
    let (dx, dz) = exit_direction(gateway);
    let mut x = dx * TENTATIVE_DISTANCE;
    let mut z = dz * TENTATIVE_DISTANCE;
    let chunk_of = |x: f64, z: f64| {
        ChunkPos::new(
            (x / TENTATIVE_STEP).floor() as i32,
            (z / TENTATIVE_STEP).floor() as i32,
        )
    };
    let mut tries = TENTATIVE_TRIES;
    while !chunk_is_empty(chunk_of(x, z)) {
        let more = tries > 0;
        tries -= 1;
        if !more {
            break;
        }
        x -= dx * TENTATIVE_STEP;
        z -= dz * TENTATIVE_STEP;
    }
    tries = TENTATIVE_TRIES;
    while chunk_is_empty(chunk_of(x, z)) {
        let more = tries > 0;
        tries -= 1;
        if !more {
            break;
        }
        x += dx * TENTATIVE_STEP;
        z += dz * TENTATIVE_STEP;
    }
    (x, z)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Solidity {
    pub full: bool,
    pub bedrock: bool,
}

pub fn find_tallest_block(
    around: BlockPos,
    radius: i32,
    allow_bedrock: bool,
    min_y: i32,
    max_y: i32,
    mut at: impl FnMut(BlockPos) -> Solidity,
) -> BlockPos {
    let mut tallest: Option<BlockPos> = None;
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            if dx != 0 || dz != 0 || allow_bedrock {
                let mut y = max_y;
                while y > tallest.map_or(min_y, |found| found.y) {
                    let pos = BlockPos::new(around.x + dx, y, around.z + dz);
                    let solid = at(pos);
                    if solid.full && (allow_bedrock || !solid.bedrock) {
                        tallest = Some(pos);
                        break;
                    }
                    y -= 1;
                }
            }
        }
    }
    tallest.unwrap_or(around)
}

pub fn exit_position(
    exit: BlockPos,
    min_y: i32,
    max_y: i32,
    at: impl FnMut(BlockPos) -> Solidity,
) -> BlockPos {
    find_tallest_block(
        exit.offset(0, 2, 0),
        EXIT_SEARCH_RADIUS,
        false,
        min_y,
        max_y,
        at,
    )
    .offset(0, 1, 0)
}

pub fn find_valid_spawn(
    chunk: ChunkPos,
    highest_section_y: i32,
    mut is_end_stone: impl FnMut(BlockPos) -> bool,
    mut is_full: impl FnMut(BlockPos) -> bool,
) -> Option<BlockPos> {
    let top = highest_section_y + 16 - 1;
    let mut best: Option<(BlockPos, f64)> = None;
    for z in chunk.min_block_z()..=chunk.min_block_z() + 15 {
        for y in SPAWN_SCAN_FLOOR..=top {
            for x in chunk.min_block_x()..=chunk.min_block_x() + 15 {
                let pos = BlockPos::new(x, y, z);
                if is_end_stone(pos)
                    && !is_full(pos.offset(0, 1, 0))
                    && !is_full(pos.offset(0, 2, 0))
                {
                    let distance = center_distance(pos);
                    if best.is_none_or(|(_, nearest)| distance < nearest) {
                        best = Some((pos, distance));
                    }
                }
            }
        }
    }
    best.map(|(pos, _)| pos)
}

fn center_distance(pos: BlockPos) -> f64 {
    let x = f64::from(pos.x) + 0.5;
    let y = f64::from(pos.y) + 0.5;
    let z = f64::from(pos.z) + 0.5;
    x * x + y * y + z * z
}

pub fn pos_seed(pos: BlockPos) -> i64 {
    ((i64::from(pos.x) & 0x3FF_FFFF) << 38)
        | ((i64::from(pos.z) & 0x3FF_FFFF) << 12)
        | (i64::from(pos.y) & 0xFFF)
}

pub fn island_blocks(origin: BlockPos, random: &mut impl RandomSource) -> Vec<BlockPos> {
    let mut out = Vec::new();
    let mut radius = random.next_int(3) as f32 + 4.0;
    let mut layer = 0;
    while radius > 0.5 {
        let low = (-radius).floor() as i32;
        let high = radius.ceil() as i32;
        for x in low..=high {
            for z in low..=high {
                if (x * x + z * z) as f32 <= (radius + 1.0) * (radius + 1.0) {
                    out.push(origin.offset(x, layer, z));
                }
            }
        }
        radius -= random.next_int(2) as f32 + 0.5;
        layer -= 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::random::LegacyRandom;

    #[test]
    fn a_gateway_spawns_for_two_hundred_ticks_and_cools_down_for_forty_every_two_thousand_four_hundred()
     {
        let mut gateway = Gateway::default();
        assert!(gateway.is_spawning());
        for _ in 0..200 {
            gateway.tick();
        }
        assert!(!gateway.is_spawning());
        assert!(!gateway.is_cooling_down());
        for _ in 200..2399 {
            assert!(!gateway.tick().cooldown_started);
        }
        let tick = gateway.tick();
        assert!(tick.cooldown_started && tick.changed);
        assert_eq!(gateway.cooldown, 40);
        for _ in 0..39 {
            assert!(!gateway.tick().changed);
        }
        let last = gateway.tick();
        assert!(last.changed && !gateway.is_cooling_down());
    }

    #[test]
    fn triggering_the_cooldown_arms_forty_ticks() {
        let mut gateway = Gateway::default();
        gateway.trigger_cooldown();
        assert!(gateway.is_cooling_down());
        assert_eq!(gateway.cooldown, COOLDOWN_TIME);
    }

    #[test]
    fn the_saved_layout_is_age_exit_portal_and_exact_teleport_only_when_set() {
        let gateway = Gateway {
            age: 77,
            cooldown: 12,
            exit: Some(BlockPos::new(3, 80, -9)),
            exact: true,
        };
        let nbt = gateway.to_nbt();
        assert_eq!(nbt.get_i64("Age"), Some(77));
        assert_eq!(nbt.get_int_array("exit_portal"), Some(&[3, 80, -9][..]));
        assert_eq!(nbt.get_bool("ExactTeleport"), Some(true));
        let read = Gateway::from_nbt(&nbt);
        assert_eq!(read.cooldown, 0);
        assert_eq!(
            Gateway {
                cooldown: 0,
                ..gateway
            },
            read
        );
        let bare = Gateway::default().to_nbt();
        assert!(bare.get("exit_portal").is_none());
        assert!(bare.get("ExactTeleport").is_none());
    }

    #[test]
    fn an_exit_outside_the_world_bounds_is_dropped() {
        let mut nbt = Compound::new();
        nbt.insert("exit_portal", vec![40_000_000, 80, 0]);
        assert_eq!(Gateway::from_nbt(&nbt).exit, None);
    }

    #[test]
    fn the_tentative_position_starts_a_kilometre_out_and_backs_off_then_walks_in() {
        let (x, z) = tentative_position(BlockPos::new(96, 75, 0), |chunk| chunk.x < 60);
        assert_eq!(z, 0.0);
        assert!((x - 960.0).abs() < 1.0e-9, "{x}");
        let (x, _) = tentative_position(BlockPos::new(96, 75, 0), |_| false);
        assert!((x - (1024.0 - 16.0 * 16.0)).abs() < 1.0e-9);
        let (x, _) = tentative_position(BlockPos::new(96, 75, 0), |_| true);
        assert!((x - (1024.0 + 16.0 * 16.0)).abs() < 1.0e-9);
    }

    #[test]
    fn the_tallest_block_search_skips_bedrock_and_the_centre_column() {
        let at = |pos: BlockPos| Solidity {
            full: pos.y <= 70 || (pos.x == 0 && pos.z == 0 && pos.y <= 90),
            bedrock: pos.x == 1 && pos.z == 0 && pos.y == 70,
        };
        let found = find_tallest_block(BlockPos::new(0, 80, 0), 5, false, 0, 128, at);
        assert_eq!(found.y, 70);
        assert!(!(found.x == 0 && found.z == 0));
        let with_center = find_tallest_block(BlockPos::new(0, 80, 0), 5, true, 0, 128, at);
        assert_eq!(with_center, BlockPos::new(0, 90, 0));
        let nothing = find_tallest_block(BlockPos::new(0, 80, 0), 5, false, 0, 128, |_| Solidity {
            full: false,
            bedrock: false,
        });
        assert_eq!(nothing, BlockPos::new(0, 80, 0));
    }

    #[test]
    fn the_exit_stands_one_above_the_tallest_block_around_the_exit_portal() {
        let at = |pos: BlockPos| Solidity {
            full: pos.y <= 70,
            bedrock: false,
        };
        assert_eq!(
            exit_position(BlockPos::new(0, 75, 0), 0, 128, at),
            BlockPos::new(-5, 71, -5)
        );
    }

    #[test]
    fn a_valid_spawn_is_end_stone_with_two_free_blocks_above_nearest_the_origin() {
        let stone = |pos: BlockPos| pos.y == 62;
        let full = |pos: BlockPos| pos.y <= 62;
        let found = find_valid_spawn(ChunkPos::new(10, 10), 48, stone, full).unwrap();
        assert_eq!(found, BlockPos::new(160, 62, 160));
        assert_eq!(
            find_valid_spawn(ChunkPos::new(10, 10), 48, |_| false, full),
            None
        );
        let blocked = |pos: BlockPos| pos.y <= 63;
        assert_eq!(
            find_valid_spawn(ChunkPos::new(0, 0), 48, stone, blocked),
            None
        );
    }

    #[test]
    fn the_island_is_a_tapering_stack_of_discs_from_the_origin_downwards() {
        let mut random = LegacyRandom::new(pos_seed(BlockPos::new(100, 75, 100)));
        let blocks = island_blocks(BlockPos::new(100, 75, 100), &mut random);
        assert!(blocks.contains(&BlockPos::new(100, 75, 100)));
        assert!(blocks.iter().all(|pos| pos.y <= 75));
        let widest = blocks.iter().filter(|pos| pos.y == 75).count();
        let lowest = blocks.iter().map(|pos| pos.y).min().unwrap();
        let narrowest = blocks.iter().filter(|pos| pos.y == lowest).count();
        assert!(widest > narrowest);
        let mut again = LegacyRandom::new(pos_seed(BlockPos::new(100, 75, 100)));
        assert_eq!(
            blocks,
            island_blocks(BlockPos::new(100, 75, 100), &mut again)
        );
    }

    #[test]
    fn the_position_seed_packs_the_coordinates_like_the_block_pos_long() {
        assert_eq!(pos_seed(BlockPos::new(0, 0, 0)), 0);
        assert_eq!(pos_seed(BlockPos::new(1, 0, 0)), 1 << 38);
        assert_eq!(pos_seed(BlockPos::new(0, 0, 1)), 1 << 12);
        assert_eq!(pos_seed(BlockPos::new(0, 1, 0)), 1);
        assert_eq!(pos_seed(BlockPos::new(-1, -1, -1)), -1);
    }
}
