use rspigot_entities::EntityKind;

use crate::control::SmoothSwimming;

pub const FROG_MOVEMENT: SmoothSwimming = SmoothSwimming {
    max_turn_x: 85,
    max_turn_y: 10,
    in_water_speed_modifier: 0.02,
    outside_water_speed_modifier: 0.1,
    apply_gravity: true,
};
pub const TADPOLE_MOVEMENT: SmoothSwimming = FROG_MOVEMENT;
pub const TADPOLE_LOOK_FROM_CENTER: i32 = 10;
pub const AXOLOTL_MOVEMENT: SmoothSwimming = SmoothSwimming {
    max_turn_x: 85,
    max_turn_y: 10,
    in_water_speed_modifier: 0.1,
    outside_water_speed_modifier: 0.5,
    apply_gravity: false,
};
pub const AXOLOTL_LOOK_FROM_CENTER: i32 = 20;
pub const NAUTILUS_MOVEMENT: SmoothSwimming = SmoothSwimming {
    max_turn_x: 85,
    max_turn_y: 10,
    in_water_speed_modifier: 0.011,
    outside_water_speed_modifier: 0.0,
    apply_gravity: true,
};
pub const NAUTILUS_LOOK_FROM_CENTER: i32 = 10;

pub fn smooth_swimming_of(kind: EntityKind) -> Option<(SmoothSwimming, Option<i32>)> {
    match kind {
        EntityKind::Frog => Some((FROG_MOVEMENT, None)),
        EntityKind::Tadpole => Some((TADPOLE_MOVEMENT, Some(TADPOLE_LOOK_FROM_CENTER))),
        EntityKind::Axolotl => Some((AXOLOTL_MOVEMENT, Some(AXOLOTL_LOOK_FROM_CENTER))),
        EntityKind::Nautilus | EntityKind::ZombieNautilus => {
            Some((NAUTILUS_MOVEMENT, Some(NAUTILUS_LOOK_FROM_CENTER)))
        }
        _ => None,
    }
}
