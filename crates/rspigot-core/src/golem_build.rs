use rspigot_blocks::BlockKind;
use rspigot_domain::entity::physics::has_tag;
use rspigot_domain::entity::species::iron_golem;
use rspigot_domain::{BlockPos, BlockStateId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::mob_actions::new_mob;
use crate::plugins::events::CreatureSpawnReason;
use crate::plugins::steps::{Birth, Spawned};
use crate::ports::ClientLink;
use crate::server::Server;

const BUILD_HEIGHT_OFFSET: f64 = 0.05;
const COPPER_TAG: &str = "minecraft:copper";
const DIRECTIONS: [(i32, i32, i32); 6] = [
    (0, -1, 0),
    (0, 1, 0),
    (0, 0, -1),
    (0, 0, 1),
    (-1, 0, 0),
    (1, 0, 0),
];

const SNOW_FULL: &[&str] = &["^", "#", "#"];
const SNOW_BASE: &[&str] = &[" ", "#", "#"];
const IRON_FULL: &[&str] = &["~^~", "###", "~#~"];
const IRON_BASE: &[&str] = &["~ ~", "###", "~#~"];
const COPPER_BASE: &[&str] = &[" ", "#"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Golem {
    Snow,
    Iron,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Found {
    origin: BlockPos,
    forwards: (i32, i32, i32),
    up: (i32, i32, i32),
}

fn cross(a: (i32, i32, i32), b: (i32, i32, i32)) -> (i32, i32, i32) {
    (
        a.1 * b.2 - a.2 * b.1,
        a.2 * b.0 - a.0 * b.2,
        a.0 * b.1 - a.1 * b.0,
    )
}

impl Found {
    fn block(&self, palm: i32, thumb: i32, finger: i32) -> BlockPos {
        let palm_axis = cross(self.forwards, self.up);
        self.origin.offset(
            self.up.0 * -thumb + palm_axis.0 * palm + self.forwards.0 * finger,
            self.up.1 * -thumb + palm_axis.1 * palm + self.forwards.1 * finger,
            self.up.2 * -thumb + palm_axis.2 * palm + self.forwards.2 * finger,
        )
    }

    fn cells(&self, rows: &[&str]) -> Vec<BlockPos> {
        let mut cells = Vec::new();
        for (row, line) in rows.iter().enumerate() {
            for column in 0..line.len() {
                cells.push(self.block(column as i32, row as i32, 0));
            }
        }
        cells
    }
}

fn accepts(symbol: u8, state: BlockStateId, solid: BlockKind) -> bool {
    match symbol {
        b'^' => matches!(
            state.kind(),
            Some(BlockKind::CarvedPumpkin | BlockKind::JackOLantern)
        ),
        b'#' => state.kind() == Some(solid),
        b'~' => state.kind() == Some(BlockKind::Air),
        _ => true,
    }
}

fn accepts_copper(symbol: u8, state: BlockStateId) -> bool {
    match symbol {
        b'#' => has_tag(state, COPPER_TAG),
        _ => true,
    }
}

fn find_rows(
    block_at: &impl Fn(BlockPos) -> Option<BlockStateId>,
    from: BlockPos,
    rows: &[&str],
    accepts: &impl Fn(u8, BlockStateId) -> bool,
) -> Option<Found> {
    let reach = rows.len().max(rows[0].len()) as i32 - 1;
    for dz in 0..=reach {
        for dy in 0..=reach {
            for dx in 0..=reach {
                let origin = from.offset(dx, dy, dz);
                for forwards in DIRECTIONS {
                    for up in DIRECTIONS {
                        if up == forwards || up == (-forwards.0, -forwards.1, -forwards.2) {
                            continue;
                        }
                        let candidate = Found {
                            origin,
                            forwards,
                            up,
                        };
                        if fits(block_at, &candidate, rows, accepts) {
                            return Some(candidate);
                        }
                    }
                }
            }
        }
    }
    None
}

fn fits(
    block_at: &impl Fn(BlockPos) -> Option<BlockStateId>,
    candidate: &Found,
    rows: &[&str],
    accepts: &impl Fn(u8, BlockStateId) -> bool,
) -> bool {
    rows.iter().enumerate().all(|(row, line)| {
        line.bytes().enumerate().all(|(column, symbol)| {
            block_at(candidate.block(column as i32, row as i32, 0))
                .is_some_and(|state| accepts(symbol, state))
        })
    })
}

fn find_golem(
    block_at: &impl Fn(BlockPos) -> Option<BlockStateId>,
    from: BlockPos,
) -> Option<(Golem, Found, &'static [&'static str])> {
    let snow = |symbol, state| accepts(symbol, state, BlockKind::SnowBlock);
    if let Some(found) = find_rows(block_at, from, SNOW_FULL, &snow) {
        return Some((Golem::Snow, found, SNOW_FULL));
    }
    let iron = |symbol, state| accepts(symbol, state, BlockKind::IronBlock);
    find_rows(block_at, from, IRON_FULL, &iron).map(|found| (Golem::Iron, found, IRON_FULL))
}

pub(crate) fn can_spawn_golem(
    block_at: &impl Fn(BlockPos) -> Option<BlockStateId>,
    at: BlockPos,
) -> bool {
    let snow = |symbol, state| accepts(symbol, state, BlockKind::SnowBlock);
    let iron = |symbol, state| accepts(symbol, state, BlockKind::IronBlock);
    find_rows(block_at, at, SNOW_BASE, &snow).is_some()
        || find_rows(block_at, at, IRON_BASE, &iron).is_some()
        || find_rows(block_at, at, COPPER_BASE, &accepts_copper).is_some()
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn can_spawn_golem_at(&self, at: BlockPos) -> bool {
        let chunks = &self.world.chunks;
        can_spawn_golem(&|pos| chunks.block(pos), at)
    }

    pub(crate) fn try_build_snow_or_iron_golem(&mut self, pos: BlockPos) -> bool {
        let chunks = &self.world.chunks;
        let Some((golem, found, rows)) = find_golem(&|at| chunks.block(at), pos) else {
            return false;
        };
        let (kind, reason, bottom) = match golem {
            Golem::Snow => (
                Kind::SnowGolem,
                CreatureSpawnReason::BuildSnowman,
                found.block(0, 2, 0),
            ),
            Golem::Iron => (
                Kind::IronGolem,
                CreatureSpawnReason::BuildIrongolem,
                found.block(1, 2, 0),
            ),
        };
        let mut entity = new_mob(self.new_entity_base(kind));
        entity.base.snap_to(
            Vec3::new(
                f64::from(bottom.x) + 0.5,
                f64::from(bottom.y) + BUILD_HEIGHT_OFFSET,
                f64::from(bottom.z) + 0.5,
            ),
            0.0,
            0.0,
        );
        if golem == Golem::Iron {
            iron_golem::set_player_created(&mut entity.base, true);
        }
        let blocks: Vec<(BlockPos, BlockStateId)> = found
            .cells(rows)
            .into_iter()
            .map(|at| (at, self.world.chunks.block(at).unwrap_or(BlockStateId::AIR)))
            .collect();
        self.spawn_entity(entity, Birth::of(reason).then(Spawned::Pattern(blocks)));
        true
    }
}
