use rspigot_blocks::BlockKind;
use rspigot_domain::entity::physics::has_tag;
use rspigot_domain::entity::species::wither;
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, BlockStateId, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::mob_actions::new_mob;
use crate::plugins::events::CreatureSpawnReason;
use crate::plugins::steps::{Birth, Spawned};
use crate::ports::ClientLink;
use crate::server::Server;

const SUMMON_BASE: &str = "minecraft:wither_summon_base_blocks";
const BLOCK_BREAK_EVENT: i32 = 2001;
const SPAWN_HEIGHT: f64 = 0.55;
const WIDTH: i32 = 3;
const HEIGHT: i32 = 3;
const DEPTH: i32 = 1;
const ROWS: [&str; 3] = ["^^^", "###", "~#~"];
const DIRECTIONS: [(i32, i32, i32); 6] = [
    (0, -1, 0),
    (0, 1, 0),
    (0, 0, -1),
    (0, 0, 1),
    (-1, 0, 0),
    (1, 0, 0),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PatternMatch {
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

fn translate_and_rotate(
    origin: BlockPos,
    finger: (i32, i32, i32),
    thumb: (i32, i32, i32),
    palm_offset: i32,
    thumb_offset: i32,
    finger_offset: i32,
) -> BlockPos {
    let palm = cross(finger, thumb);
    origin.offset(
        thumb.0 * -thumb_offset + palm.0 * palm_offset + finger.0 * finger_offset,
        thumb.1 * -thumb_offset + palm.1 * palm_offset + finger.1 * finger_offset,
        thumb.2 * -thumb_offset + palm.2 * palm_offset + finger.2 * finger_offset,
    )
}

impl PatternMatch {
    pub(crate) fn block(&self, palm: i32, thumb: i32, finger: i32) -> BlockPos {
        translate_and_rotate(self.origin, self.forwards, self.up, palm, thumb, finger)
    }

    pub(crate) fn positions(&self) -> Vec<BlockPos> {
        let mut found = Vec::new();
        for i in 0..WIDTH {
            for j in 0..HEIGHT {
                for k in 0..DEPTH {
                    found.push(self.block(i, j, k));
                }
            }
        }
        found
    }

    pub(crate) fn faces_x(&self) -> bool {
        self.forwards.0 != 0
    }
}

fn accepts(symbol: u8, state: BlockStateId) -> bool {
    match symbol {
        b'^' => matches!(
            state.kind(),
            Some(BlockKind::WitherSkeletonSkull | BlockKind::WitherSkeletonWallSkull)
        ),
        b'#' => has_tag(state, SUMMON_BASE),
        b'~' => state.is_air(),
        _ => true,
    }
}

pub(crate) fn find_pattern(
    block_at: &impl Fn(BlockPos) -> Option<BlockStateId>,
    from: BlockPos,
) -> Option<PatternMatch> {
    let reach = WIDTH.max(HEIGHT).max(DEPTH) - 1;
    for dz in 0..=reach {
        for dy in 0..=reach {
            for dx in 0..=reach {
                let origin = from.offset(dx, dy, dz);
                for forwards in DIRECTIONS {
                    for up in DIRECTIONS {
                        if up == forwards || up == (-forwards.0, -forwards.1, -forwards.2) {
                            continue;
                        }
                        let candidate = PatternMatch {
                            origin,
                            forwards,
                            up,
                        };
                        if matches_pattern(block_at, &candidate) {
                            return Some(candidate);
                        }
                    }
                }
            }
        }
    }
    None
}

fn matches_pattern(
    block_at: &impl Fn(BlockPos) -> Option<BlockStateId>,
    candidate: &PatternMatch,
) -> bool {
    for i in 0..WIDTH {
        for j in 0..HEIGHT {
            for k in 0..DEPTH {
                let symbol = ROWS[j as usize].as_bytes()[i as usize];
                let Some(state) = block_at(candidate.block(i, j, k)) else {
                    return false;
                };
                if !accepts(symbol, state) {
                    return false;
                }
            }
        }
    }
    true
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn check_wither_summon(&mut self, pos: BlockPos) {
        let is_skull = self.world.chunks.block(pos).is_some_and(|state| {
            matches!(
                state.kind(),
                Some(BlockKind::WitherSkeletonSkull | BlockKind::WitherSkeletonWallSkull)
            )
        });
        if !is_skull
            || pos.y < self.world.dimension.height.min_y
            || self.world.difficulty == Difficulty::Peaceful
        {
            return;
        }
        let chunks = &self.world.chunks;
        let Some(found) = find_pattern(&|at| chunks.block(at), pos) else {
            return;
        };
        let center = found.block(1, 2, 0);
        let yaw = if found.faces_x() { 0.0 } else { 90.0 };
        let mut base = self.new_entity_base(Kind::Wither);
        base.snap_to(
            Vec3::new(
                f64::from(center.x) + 0.5,
                f64::from(center.y) + SPAWN_HEIGHT,
                f64::from(center.z) + 0.5,
            ),
            yaw,
            0.0,
        );
        let mut entity = new_mob(base);
        if let Some(living) = entity.living_mut() {
            living.body_yaw = yaw;
        }
        if let rspigot_domain::entity::EntityState::Living(state) = &mut entity.state {
            wither::make_invulnerable(&mut entity.base, state);
        }
        let blocks: Vec<(BlockPos, BlockStateId)> = found
            .positions()
            .into_iter()
            .map(|at| (at, self.world.chunks.block(at).unwrap_or(BlockStateId::AIR)))
            .collect();
        let birth = Birth::of(CreatureSpawnReason::BuildWither).then(Spawned::Pattern(blocks));
        self.spawn_entity(entity, birth);
    }

    pub(crate) fn clear_pattern_blocks(&mut self, blocks: &[(BlockPos, BlockStateId)]) {
        for (pos, state) in blocks {
            self.set_block(*pos, BlockStateId::AIR);
            self.level_event(BLOCK_BREAK_EVENT, *pos, i32::from(state.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use rustc_hash::FxHashMap;

    use super::*;

    fn soul_sand() -> BlockStateId {
        BlockKind::SoulSand.default_state()
    }

    fn skull() -> BlockStateId {
        BlockKind::WitherSkeletonSkull.default_state()
    }

    fn world(blocks: &[(BlockPos, BlockStateId)]) -> FxHashMap<BlockPos, BlockStateId> {
        blocks.iter().copied().collect()
    }

    fn lookup(
        map: &FxHashMap<BlockPos, BlockStateId>,
    ) -> impl Fn(BlockPos) -> Option<BlockStateId> {
        let map = map.clone();
        move |pos| Some(map.get(&pos).copied().unwrap_or(BlockStateId::AIR))
    }

    fn tee_along_x(at: BlockPos) -> Vec<(BlockPos, BlockStateId)> {
        let mut blocks = Vec::new();
        for dx in 0..3 {
            blocks.push((at.offset(dx, 1, 0), soul_sand()));
            blocks.push((at.offset(dx, 2, 0), skull()));
        }
        blocks.push((at.offset(1, 0, 0), soul_sand()));
        blocks
    }

    #[test]
    fn a_t_of_soul_sand_with_three_skulls_matches_from_any_skull() {
        let map = world(&tee_along_x(BlockPos::new(10, 64, 10)));
        let look = lookup(&map);
        for dx in 0..3 {
            let found = find_pattern(&look, BlockPos::new(10 + dx, 66, 10));
            assert!(found.is_some(), "skull {dx}");
        }
    }

    #[test]
    fn the_match_places_the_wither_on_the_bottom_middle_soul_sand_and_clears_nine_cells() {
        let map = world(&tee_along_x(BlockPos::new(10, 64, 10)));
        let look = lookup(&map);
        let found = find_pattern(&look, BlockPos::new(10, 66, 10)).unwrap();
        assert_eq!(found.block(1, 2, 0), BlockPos::new(11, 64, 10));
        let cleared = found.positions();
        assert_eq!(cleared.len(), 9);
        assert!(cleared.contains(&BlockPos::new(10, 64, 10)));
        assert!(cleared.contains(&BlockPos::new(12, 66, 10)));
    }

    #[test]
    fn the_t_also_matches_along_the_z_axis_and_an_upside_down_one_is_refused() {
        let at = BlockPos::new(0, 64, 0);
        let mut blocks = Vec::new();
        for dz in 0..3 {
            blocks.push((at.offset(0, 1, dz), soul_sand()));
            blocks.push((at.offset(0, 2, dz), skull()));
        }
        blocks.push((at.offset(0, 0, 1), soul_sand()));
        let map = world(&blocks);
        let look = lookup(&map);
        let found = find_pattern(&look, at.offset(0, 2, 0)).unwrap();
        assert_eq!(found.block(1, 2, 0), at.offset(0, 0, 1));
        assert!(!found.faces_x());
        let flipped = world(
            &blocks
                .iter()
                .map(|(pos, state)| (BlockPos::new(pos.x, 130 - pos.y, pos.z), *state))
                .collect::<Vec<_>>(),
        );
        assert!(find_pattern(&lookup(&flipped), BlockPos::new(0, 126, 0)).is_none());
    }

    #[test]
    fn a_missing_stem_a_blocked_corner_or_a_wrong_base_refuses_the_pattern() {
        let at = BlockPos::new(10, 64, 10);
        let full = tee_along_x(at);
        let without_stem: Vec<_> = full
            .iter()
            .copied()
            .filter(|(pos, _)| *pos != at.offset(1, 0, 0))
            .collect();
        assert!(find_pattern(&lookup(&world(&without_stem)), at.offset(0, 2, 0)).is_none());
        let mut blocked = full.clone();
        blocked.push((at.offset(0, 0, 0), BlockKind::Stone.default_state()));
        assert!(find_pattern(&lookup(&world(&blocked)), at.offset(0, 2, 0)).is_none());
        let wrong: Vec<_> = full
            .iter()
            .map(|(pos, state)| {
                if *state == soul_sand() {
                    (*pos, BlockKind::Stone.default_state())
                } else {
                    (*pos, *state)
                }
            })
            .collect();
        assert!(find_pattern(&lookup(&world(&wrong)), at.offset(0, 2, 0)).is_none());
    }

    #[test]
    fn soul_soil_is_a_valid_base_and_a_wall_skull_a_valid_head() {
        let at = BlockPos::new(0, 64, 0);
        let mut blocks = tee_along_x(at);
        for (_, state) in &mut blocks {
            if *state == soul_sand() {
                *state = BlockKind::SoulSoil.default_state();
            }
        }
        blocks.retain(|(pos, _)| *pos != at.offset(1, 2, 0));
        blocks.push((
            at.offset(1, 2, 0),
            BlockKind::WitherSkeletonWallSkull.default_state(),
        ));
        assert!(find_pattern(&lookup(&world(&blocks)), at.offset(0, 2, 0)).is_some());
    }
}
