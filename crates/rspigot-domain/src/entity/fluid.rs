use rspigot_blocks::{BlockKind, StateFlags};

use rspigot_entities::EntityKind;

use super::aabb::Aabb;
use super::base::EntityBase;
use super::physics::{BlockView, mth_floor, water_height};
use crate::BlockPos;

const INTERACTION_INSET: f64 = 0.001;
const EYE_FLUID_OFFSET: f64 = 0.111_111_111_938_953_4;
const JUMP_THRESHOLD: f64 = 0.4;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FluidContact {
    pub water_height: f64,
    pub lava_height: f64,
    pub in_water: bool,
}

impl FluidContact {
    pub fn in_lava(&self) -> bool {
        self.lava_height > 0.0
    }

    pub fn in_liquid(&self) -> bool {
        self.in_water || self.in_lava()
    }
}

pub fn lava_height(view: &impl BlockView, pos: BlockPos) -> Option<f64> {
    let state = view.block_or_air(pos);
    if !state.flags().contains(StateFlags::LAVA) {
        return None;
    }
    let above = view.block_or_air(pos.offset(0, 1, 0));
    if above.flags().contains(StateFlags::LAVA) {
        return Some(1.0);
    }
    let amount = match state.kind() {
        Some(BlockKind::Lava) => {
            let level = state
                .property("level")
                .and_then(|l| l.parse::<i32>().ok())
                .unwrap_or(0);
            if level >= 8 { 8 } else { 8 - level }
        }
        _ => 8,
    };
    Some(f64::from(amount as f32 / 9.0))
}

fn range(min: f64, max: f64) -> std::ops::RangeInclusive<i32> {
    mth_floor(min)..=(max.ceil() as i32 - 1)
}

pub fn fluid_heights(view: &impl BlockView, area: &Aabb) -> (f64, f64) {
    let mut water = 0.0f64;
    let mut lava = 0.0f64;
    for x in range(area.min_x, area.max_x) {
        for y in range(area.min_y, area.max_y) {
            for z in range(area.min_z, area.max_z) {
                let pos = BlockPos::new(x, y, z);
                if let Some(height) = water_height(view, pos) {
                    let top = f64::from(y) + height;
                    if top >= area.min_y {
                        water = water.max(top - area.min_y);
                    }
                }
                if let Some(height) = lava_height(view, pos) {
                    let top = f64::from(y) + height;
                    if top >= area.min_y {
                        lava = lava.max(top - area.min_y);
                    }
                }
            }
        }
    }
    (water, lava)
}

pub fn update(base: &mut EntityBase, view: &impl BlockView) {
    let area = base.bounding_box().deflate(INTERACTION_INSET);
    let (water, lava) = fluid_heights(view, &area);
    let in_water = water > 0.0;
    if in_water {
        base.fall_distance = 0.0;
    }
    base.fluid = FluidContact {
        water_height: water,
        lava_height: lava,
        in_water,
    };
    let eye = base.eye_y() - EYE_FLUID_OFFSET;
    let pos = crate::Vec3::new(base.position.x, eye, base.position.z).block();
    base.eye_in_water =
        water_height(view, pos).is_some_and(|height| f64::from(pos.y) + height > eye);
}

pub fn jump_threshold(base: &EntityBase) -> f64 {
    if base.kind == EntityKind::SulfurCube {
        super::species::sulfur_cube::jump_threshold(base)
    } else if f64::from(base.eye_height()) < JUMP_THRESHOLD {
        0.0
    } else {
        JUMP_THRESHOLD
    }
}

pub fn is_free(view: &impl BlockView, area: &Aabb) -> bool {
    for x in range(area.min_x, area.max_x) {
        for y in range(area.min_y, area.max_y) {
            for z in range(area.min_z, area.max_z) {
                if view
                    .block_or_air(BlockPos::new(x, y, z))
                    .flags()
                    .has_fluid()
                {
                    return false;
                }
            }
        }
    }
    let low = |value: f64| mth_floor(value - 1.0e-7) - 1;
    let high = |value: f64| mth_floor(value + 1.0e-7) + 1;
    for x in low(area.min_x)..=high(area.max_x) {
        for y in low(area.min_y)..=high(area.max_y) {
            for z in low(area.min_z)..=high(area.max_z) {
                let pos = BlockPos::new(x, y, z);
                let (dx, dy, dz) = (f64::from(x), f64::from(y), f64::from(z));
                let blocked = view
                    .block_or_air(pos)
                    .collision_shape()
                    .iter()
                    .any(|shape| {
                        shape.min_x + dx < area.max_x
                            && shape.max_x + dx > area.min_x
                            && shape.min_y + dy < area.max_y
                            && shape.max_y + dy > area.min_y
                            && shape.min_z + dz < area.max_z
                            && shape.max_z + dz > area.min_z
                    });
                if blocked {
                    return false;
                }
            }
        }
    }
    true
}

pub fn is_free_of_collision(view: &impl BlockView, area: &Aabb) -> bool {
    super::physics::block_colliders(view, area).is_empty()
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::{BlockStateId, EntityId, Vec3};

    struct Pool;

    impl BlockView for Pool {
        fn block(&self, pos: BlockPos) -> Option<BlockStateId> {
            Some(if pos.y <= 63 && pos.y >= 60 {
                BlockKind::Water.default_state()
            } else if pos.y < 60 {
                BlockKind::Stone.default_state()
            } else {
                BlockStateId::AIR
            })
        }
    }

    #[test]
    fn submerged_mob_measures_depth_from_its_feet() {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Cow);
        base.position = Vec3::new(0.5, 62.5, 0.5);
        base.fall_distance = 3.0;
        update(&mut base, &Pool);
        assert!(base.fluid.in_water);
        assert!(!base.fluid.in_lava());
        let surface = 63.0 + f64::from(8.0f32 / 9.0);
        assert!((base.fluid.water_height - (surface - 62.501)).abs() < 1.0e-9);
        assert_eq!(base.fall_distance, 0.0);
        base.position = Vec3::new(0.5, 64.0, 0.5);
        update(&mut base, &Pool);
        assert!(!base.fluid.in_water);
        assert_eq!(jump_threshold(&base), 0.4);
    }

    #[test]
    fn free_space_has_no_liquid_and_no_collision() {
        let air = Aabb::new(0.1, 65.0, 0.1, 0.9, 66.0, 0.9);
        assert!(is_free(&Pool, &air));
        let wet = Aabb::new(0.1, 63.5, 0.1, 0.9, 64.5, 0.9);
        assert!(!is_free(&Pool, &wet));
        let buried = Aabb::new(0.1, 58.5, 0.1, 0.9, 59.5, 0.9);
        assert!(!is_free(&Pool, &buried));
    }
}
