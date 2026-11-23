use std::sync::Arc;

use rspigot_domain::{BlockPos, BlockStateId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrowWindow {
    pub min: BlockPos,
    pub size: (i32, i32, i32),
    pub states: Arc<[BlockStateId]>,
}

impl GrowWindow {
    pub fn around(
        origin: BlockPos,
        below: i32,
        above: i32,
        radius: i32,
    ) -> (BlockPos, (i32, i32, i32)) {
        let min = origin.offset(-radius, -below, -radius);
        (min, (radius * 2 + 1, below + above + 1, radius * 2 + 1))
    }

    pub fn index(&self, pos: BlockPos) -> Option<usize> {
        let (dx, dy, dz) = (pos.x - self.min.x, pos.y - self.min.y, pos.z - self.min.z);
        let (sx, sy, sz) = self.size;
        if !(0..sx).contains(&dx) || !(0..sy).contains(&dy) || !(0..sz).contains(&dz) {
            return None;
        }
        Some(((dy * sz + dz) * sx + dx) as usize)
    }

    pub fn get(&self, pos: BlockPos) -> Option<BlockStateId> {
        self.index(pos).map(|index| self.states[index])
    }

    pub fn positions(&self) -> impl Iterator<Item = BlockPos> + '_ {
        let (sx, sy, sz) = self.size;
        (0..sy).flat_map(move |dy| {
            (0..sz).flat_map(move |dz| (0..sx).map(move |dx| self.min.offset(dx, dy, dz)))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_indexes_its_cells_in_y_z_x_order_and_refuses_outside_ones() {
        let (min, size) = GrowWindow::around(BlockPos::new(10, 64, 10), 2, 3, 1);
        assert_eq!(size, (3, 6, 3));
        let states: Vec<BlockStateId> = (0..54).map(BlockStateId).collect();
        let window = GrowWindow {
            min,
            size,
            states: states.into(),
        };
        assert_eq!(window.index(min), Some(0));
        assert_eq!(window.index(min.offset(2, 0, 0)), Some(2));
        assert_eq!(window.index(min.offset(0, 0, 1)), Some(3));
        assert_eq!(window.index(min.offset(0, 1, 0)), Some(9));
        assert_eq!(window.index(min.offset(3, 0, 0)), None);
        assert_eq!(window.index(min.offset(0, -1, 0)), None);
        assert_eq!(window.positions().count(), 54);
        assert_eq!(window.get(min.offset(2, 5, 2)), Some(BlockStateId(53)));
    }
}
