use rspigot_blocks::Face;
use rspigot_domain::chunk::HeightmapKind;
use rspigot_domain::entity::species::shulker::is_face_full;
use rspigot_domain::player::GameMode;
use rspigot_domain::{BlockPos, BlockStateId, Vec3};

use crate::ports::ClientLink;
use crate::server::Server;

const MAX_CANDIDATES: i64 = 1024;
const COPRIME: i32 = 17;
const FALLBACK_RADIUS: i32 = 1;
const BORDER_TOO_CLOSE: i32 = 2;

const fn candidate_count(radius: i32) -> i32 {
    let side = radius as i64 * 2 + 1;
    let squared = side * side;
    (if squared < MAX_CANDIDATES {
        squared
    } else {
        MAX_CANDIDATES
    }) as i32
}

const fn coprime_of(count: i32) -> i32 {
    if count < COPRIME + 1 {
        count - 1
    } else {
        COPRIME
    }
}

pub(crate) const fn search_radius(rule: i32, border_distance: i32) -> i32 {
    let radius = if rule > 0 { rule } else { 0 };
    let radius = if border_distance < radius {
        border_distance
    } else {
        radius
    };
    if border_distance < BORDER_TOO_CLOSE {
        FALLBACK_RADIUS
    } else {
        radius
    }
}

pub(crate) fn candidate_offsets(radius: i32, offset: i32) -> Vec<(i32, i32)> {
    let side = radius * 2 + 1;
    let count = candidate_count(radius);
    let coprime = coprime_of(count);
    (0..count)
        .map(|i| {
            let index = (offset + coprime * i) % count;
            (index % side - radius, index / side - radius)
        })
        .collect()
}

fn passable(state: BlockStateId) -> bool {
    state.collision_shape().is_empty() && !state.flags().has_fluid()
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn find_spawn(&mut self, suggestion: BlockPos) -> Vec3 {
        if self.config.default_game_mode == GameMode::Adventure {
            return self.fixup_spawn_height(suggestion);
        }
        let now = self.world.level.game_time;
        let border = self
            .world
            .border
            .distance_to_border(f64::from(suggestion.x), f64::from(suggestion.z), now)
            .floor() as i32;
        let radius = search_radius(self.config.rules.respawn_radius, border);
        let count = candidate_count(radius);
        let offset = rand::Rng::random_range(&mut self.rng, 0..count);
        for (dx, dz) in candidate_offsets(radius, offset) {
            let (x, z) = (suggestion.x + dx, suggestion.z + dz);
            if let Some(found) = self.respawn_candidate(x, z) {
                return found;
            }
        }
        self.fixup_spawn_height(suggestion)
    }

    fn respawn_candidate(&mut self, x: i32, z: i32) -> Option<Vec3> {
        self.load_chunks_now(&[BlockPos::new(x, 0, z).chunk()]);
        let pos = self.level_respawn_pos(x, z)?;
        self.no_collision_no_liquid(pos).then(|| bottom_center(pos))
    }

    fn spawn_heightmap(&self, kind: HeightmapKind, x: i32, z: i32) -> Option<i32> {
        let column = self.world.chunks.get(BlockPos::new(x, 0, z).chunk())?;
        match column.heightmaps.get(kind) {
            Some(map) => {
                let value = map.get((x & 15) as usize, (z & 15) as usize);
                Some(column.height().min_y + i32::from(value))
            }
            None => Some(self.scanned_height(kind, x, z)),
        }
    }

    fn scanned_height(&self, kind: HeightmapKind, x: i32, z: i32) -> i32 {
        let height = self.world.dimension.height;
        let blocks = |state: BlockStateId| match kind {
            HeightmapKind::WorldSurface | HeightmapKind::WorldSurfaceWg => !state.is_air(),
            HeightmapKind::OceanFloor | HeightmapKind::OceanFloorWg => {
                !state.collision_shape().is_empty()
            }
            _ => !state.collision_shape().is_empty() || state.flags().has_fluid(),
        };
        (height.min_y..=height.max_y())
            .rev()
            .find(|y| blocks(self.block_or_air(BlockPos::new(x, *y, z))))
            .map_or(height.min_y, |y| y + 1)
    }

    fn level_respawn_pos(&self, x: i32, z: i32) -> Option<BlockPos> {
        let min_y = self.world.dimension.height.min_y;
        let top = if self.world.dimension.has_ceiling {
            self.world.dimension.sea_level + 1
        } else {
            self.spawn_heightmap(HeightmapKind::MotionBlocking, x, z)?
        };
        if top < min_y {
            return None;
        }
        let surface = self.spawn_heightmap(HeightmapKind::WorldSurface, x, z)?;
        if surface <= top && self.spawn_heightmap(HeightmapKind::OceanFloor, x, z)? < surface {
            return None;
        }
        let mut y = top + 1;
        while y >= min_y {
            let pos = BlockPos::new(x, y, z);
            let state = self.block_or_air(pos);
            if state.flags().has_fluid() {
                return None;
            }
            if is_face_full(state.collision_shape(), Face::Up) {
                return Some(pos.offset(0, 1, 0));
            }
            y -= 1;
        }
        None
    }

    fn no_collision_no_liquid(&self, pos: BlockPos) -> bool {
        passable(self.block_or_air(pos)) && passable(self.block_or_air(pos.offset(0, 1, 0)))
    }

    fn fixup_spawn_height(&mut self, suggestion: BlockPos) -> Vec3 {
        self.load_chunks_now(&[suggestion.chunk()]);
        let height = self.world.dimension.height;
        let min_y = height.min_y;
        let max_y = min_y + height.height as i32 - 1;
        let mut pos = suggestion;
        while !self.no_collision_no_liquid(pos) && pos.y < max_y {
            pos = pos.offset(0, 1, 0);
        }
        pos = pos.offset(0, -1, 0);
        while self.no_collision_no_liquid(pos) && pos.y > min_y {
            pos = pos.offset(0, -1, 0);
        }
        bottom_center(pos.offset(0, 1, 0))
    }
}

fn bottom_center(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y),
        f64::from(pos.z) + 0.5,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rule_sets_the_search_radius() {
        assert_eq!(search_radius(10, 1_000), 10);
        assert_eq!(search_radius(-5, 1_000), 0);
    }

    #[test]
    fn a_close_border_shrinks_the_radius() {
        assert_eq!(search_radius(10, 4), 4);
        assert_eq!(search_radius(10, 1), 1);
    }

    #[test]
    fn a_zero_radius_searches_only_the_origin() {
        assert_eq!(candidate_offsets(0, 0), vec![(0, 0)]);
    }

    #[test]
    fn every_candidate_of_the_square_is_visited_once() {
        let mut seen = candidate_offsets(2, 7);
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 25);
        assert!(seen.iter().all(|(x, z)| x.abs() <= 2 && z.abs() <= 2));
    }

    #[test]
    fn a_large_radius_is_capped_at_the_candidate_limit() {
        assert_eq!(candidate_offsets(100, 0).len(), 1024);
    }
}
