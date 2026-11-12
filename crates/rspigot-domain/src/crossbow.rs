use std::num::NonZeroU8;

use rspigot_items::ItemKind;
use rspigot_nbt::Tag;

use crate::Vec3;
use crate::entity::save::{item_stack_from_nbt, item_stack_to_nbt};
use crate::item::ItemStack;
use crate::mth;
use crate::random::RandomSource;

pub const CHARGED_PROJECTILES: &str = "minecraft:charged_projectiles";
pub const QUICK_CHARGE: &str = "minecraft:quick_charge";
pub const MULTISHOT: &str = "minecraft:multishot";
pub const PIERCING: &str = "minecraft:piercing";
pub const ARROWS_TAG: &str = "minecraft:arrows";
pub const USE_DURATION: i32 = 72_000;
pub const MOB_POWER: f32 = 1.6;
pub const ARROW_POWER: f32 = 3.15;
pub const FIREWORK_POWER: f32 = 1.6;
pub const ARROW_DURABILITY: i32 = 1;
pub const FIREWORK_DURABILITY: i32 = 3;
pub const MAX_DAMAGE: i32 = 465;
pub const SHOOT_SOUND: &str = "minecraft:item.crossbow.shoot";
pub const HIT_SOUND: &str = "minecraft:item.crossbow.hit";
pub const LOADING_END_SOUND: &str = "minecraft:item.crossbow.loading_end";
pub const LOADING_MIDDLE_SOUND: &str = "minecraft:item.crossbow.loading_middle";
pub const LOADING_START_SOUND: &str = "minecraft:item.crossbow.loading_start";
pub const QUICK_CHARGE_SOUNDS: [&str; 3] = [
    "minecraft:item.crossbow.quick_charge_1",
    "minecraft:item.crossbow.quick_charge_2",
    "minecraft:item.crossbow.quick_charge_3",
];
pub const START_SOUND_PROGRESS: f32 = 0.2;
pub const MID_SOUND_PROGRESS: f32 = 0.5;
pub const CHARGING_SOUND_VOLUME: f32 = 0.5;
pub const RANGED_UNCERTAINTY_BASE: i32 = 14;
pub const RANGED_UNCERTAINTY_PER_DIFFICULTY: i32 = 4;
pub const TARGET_HEIGHT_FRACTION: f64 = 0.333_333_333_333_333_3;
pub const ARC: f64 = 0.200_000_002_980_232_24;
pub const FIREWORK_EYE_OFFSET: f64 = 0.150_000_005_960_464_48;
const BASE_CHARGE_SECONDS: f32 = 1.25;
const QUICK_CHARGE_BASE: f32 = -0.25;
const QUICK_CHARGE_PER_LEVEL: f32 = -0.25;
const MULTISHOT_COUNT_BASE: f32 = 2.0;
const MULTISHOT_COUNT_PER_LEVEL: f32 = 2.0;
const MULTISHOT_SPREAD_BASE: f32 = 10.0;
const MULTISHOT_SPREAD_PER_LEVEL: f32 = 10.0;
const HIGH_PITCH: f32 = 0.63;
const LOW_PITCH: f32 = 0.43;
const PITCH_DIVISOR_BASE: f32 = 1.8;
const PITCH_DIVISOR_SPREAD: f32 = 0.5;
const SOUND_PITCH_BASE: f32 = 0.2;
const SOUND_PITCH_SPREAD: f32 = 0.5;
const DEGREES_TO_RADIANS: f32 = 0.017_453_292;
const HALF_PI: f32 = 1.570_796_4;
const PARALLEL_LIMIT: f32 = 1.0e-7;

pub fn level_of(stack: &ItemStack, enchantment: &str) -> i32 {
    stack
        .enchantments()
        .iter()
        .find(|(name, _)| name == enchantment)
        .map_or(0, |(_, level)| i32::from(*level))
}

fn linear(base: f32, per_level: f32, level: i32) -> f32 {
    base + per_level * (level - 1) as f32
}

pub fn charge_ticks(crossbow: &ItemStack) -> i32 {
    let level = level_of(crossbow, QUICK_CHARGE);
    let mut seconds = BASE_CHARGE_SECONDS;
    if level > 0 {
        seconds += linear(QUICK_CHARGE_BASE, QUICK_CHARGE_PER_LEVEL, level);
    }
    mth::floor(f64::from(seconds.max(0.0) * 20.0))
}

pub fn projectile_count(crossbow: &ItemStack) -> i32 {
    let level = level_of(crossbow, MULTISHOT);
    let mut count = 1.0_f32;
    if level > 0 {
        count += linear(MULTISHOT_COUNT_BASE, MULTISHOT_COUNT_PER_LEVEL, level);
    }
    (count as i32).max(0)
}

pub fn projectile_spread(crossbow: &ItemStack) -> f32 {
    let level = level_of(crossbow, MULTISHOT);
    let mut spread = 0.0_f32;
    if level > 0 {
        spread += linear(MULTISHOT_SPREAD_BASE, MULTISHOT_SPREAD_PER_LEVEL, level);
    }
    spread.max(0.0)
}

pub fn piercing_level(crossbow: &ItemStack) -> i32 {
    level_of(crossbow, PIERCING).max(0)
}

pub fn shot_angles(count: usize, spread: f32) -> Vec<f32> {
    let step = if count == 1 {
        0.0
    } else {
        (spread * 2.0) / (count as f32 - 1.0)
    };
    let base = ((count as i32 - 1) % 2) as f32 * step / 2.0;
    let mut sign = 1.0_f32;
    (0..count)
        .map(|index| {
            let angle = base + sign * index.div_ceil(2) as f32 * step;
            sign = -sign;
            angle
        })
        .collect()
}

pub fn shot_pitch(random: &mut impl RandomSource, index: usize) -> f32 {
    if index == 0 {
        return 1.0;
    }
    let offset = if index & 1 == 1 {
        HIGH_PITCH
    } else {
        LOW_PITCH
    };
    1.0 / (random.next_float() * PITCH_DIVISOR_SPREAD + PITCH_DIVISOR_BASE) + offset
}

pub fn charging_sound_pitch(random: &mut impl RandomSource) -> f32 {
    1.0 / (random.next_float() * SOUND_PITCH_SPREAD + 1.0) + SOUND_PITCH_BASE
}

pub fn ranged_uncertainty(difficulty: u8) -> f32 {
    (RANGED_UNCERTAINTY_BASE - i32::from(difficulty) * RANGED_UNCERTAINTY_PER_DIFFICULTY) as f32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChargingSounds {
    pub start: &'static str,
    pub mid: &'static str,
    pub end: &'static str,
}

pub fn charging_sounds(crossbow: &ItemStack) -> ChargingSounds {
    let level = level_of(crossbow, QUICK_CHARGE);
    let start = if level > 0 {
        QUICK_CHARGE_SOUNDS[(level as usize - 1).min(QUICK_CHARGE_SOUNDS.len() - 1)]
    } else {
        LOADING_START_SOUND
    };
    ChargingSounds {
        start,
        mid: LOADING_MIDDLE_SOUND,
        end: LOADING_END_SOUND,
    }
}

pub fn is_firework(stack: &ItemStack) -> bool {
    stack.item == ItemKind::FireworkRocket.id()
}

pub fn is_arrow(stack: &ItemStack) -> bool {
    rspigot_data::tags::tag("minecraft:item", ARROWS_TAG)
        .is_some_and(|ids| ids.binary_search(&u32::from(stack.item.0)).is_ok())
}

pub fn is_arrow_or_firework(stack: &ItemStack) -> bool {
    is_arrow(stack) || is_firework(stack)
}

pub fn shooting_power(projectiles: &[ItemStack]) -> f32 {
    if projectiles.iter().any(is_firework) {
        FIREWORK_POWER
    } else {
        ARROW_POWER
    }
}

pub fn durability_use(projectile: &ItemStack) -> i32 {
    if is_firework(projectile) {
        FIREWORK_DURABILITY
    } else {
        ARROW_DURABILITY
    }
}

pub fn charged_projectiles(crossbow: &ItemStack) -> Vec<ItemStack> {
    crossbow
        .components
        .get_list(CHARGED_PROJECTILES)
        .map(|list| {
            list.iter()
                .filter_map(|tag| match tag {
                    Tag::Compound(compound) => item_stack_from_nbt(compound),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn is_charged(crossbow: &ItemStack) -> bool {
    crossbow
        .components
        .get_list(CHARGED_PROJECTILES)
        .is_some_and(|list| !list.is_empty())
}

pub fn set_charged(crossbow: &mut ItemStack, projectiles: &[ItemStack]) {
    if projectiles.is_empty() {
        crossbow.components.remove(CHARGED_PROJECTILES);
        return;
    }
    let list = projectiles
        .iter()
        .map(|stack| Tag::Compound(item_stack_to_nbt(stack)))
        .collect();
    crossbow
        .components
        .insert(CHARGED_PROJECTILES, Tag::List(list));
}

pub fn single(item: ItemKind) -> ItemStack {
    ItemStack::new(item.id(), NonZeroU8::MIN)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Vec3f {
    x: f32,
    y: f32,
    z: f32,
}

impl Vec3f {
    fn of(vector: Vec3) -> Self {
        Self {
            x: vector.x as f32,
            y: vector.y as f32,
            z: vector.z as f32,
        }
    }

    fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    fn normalize(self) -> Self {
        let inverse = 1.0 / self.length_squared().sqrt();
        Self {
            x: self.x * inverse,
            y: self.y * inverse,
            z: self.z * inverse,
        }
    }

    fn cross(self, other: Self) -> Self {
        Self {
            x: self.y * other.z - self.z * other.y,
            y: self.z * other.x - self.x * other.z,
            z: self.x * other.y - self.y * other.x,
        }
    }

    fn rotate_axis(self, angle: f32, axis: Self) -> Self {
        let half = angle * 0.5;
        let sin = f64::from(half).sin() as f32;
        let (qx, qy, qz) = (axis.x * sin, axis.y * sin, axis.z * sin);
        let qw = cos_from_sin(sin, half);
        let (w2, x2, y2, z2) = (qw * qw, qx * qx, qy * qy, qz * qz);
        let zw = qz * qw;
        let (xy, xz, yw, yz, xw) = (qx * qy, qx * qz, qy * qw, qy * qz, qx * qw);
        Self {
            x: (w2 + x2 - z2 - y2) * self.x
                + (-zw + xy - zw + xy) * self.y
                + (yw + xz + yw + xz) * self.z,
            y: (xy + zw + zw + xy) * self.x
                + (y2 - z2 + w2 - x2) * self.y
                + (yz + yz - xw - xw) * self.z,
            z: (xz - yw + xz - yw) * self.x
                + (yz + yz + xw + xw) * self.y
                + (z2 - y2 - x2 + w2) * self.z,
        }
    }

    fn widen(self) -> Vec3 {
        Vec3::new(f64::from(self.x), f64::from(self.y), f64::from(self.z))
    }
}

fn cos_from_sin(sin: f32, angle: f32) -> f32 {
    let sin = f64::from(sin);
    let cos = (1.0 - sin * sin).sqrt();
    let shifted = f64::from(angle) + std::f64::consts::FRAC_PI_2;
    let period = std::f64::consts::TAU;
    let mut wrapped = shifted - f64::from((shifted / period) as i32) * period;
    if wrapped < 0.0 {
        wrapped += period;
    }
    if wrapped >= std::f64::consts::PI {
        -cos as f32
    } else {
        cos as f32
    }
}

pub fn shot_vector(original: Vec3, angle: f32, up: Vec3) -> Vec3 {
    let direction = Vec3f::of(original).normalize();
    let mut axis = direction.cross(Vec3f {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    });
    if axis.length_squared() <= PARALLEL_LIMIT {
        axis = direction.cross(Vec3f::of(up));
    }
    let perpendicular = direction.rotate_axis(HALF_PI, axis);
    direction
        .rotate_axis(angle * DEGREES_TO_RADIANS, perpendicular)
        .widen()
}

pub fn aim_vector(shooter: Vec3, target: Vec3, target_height: f64, projectile_y: f64) -> Vec3 {
    let dx = target.x - shooter.x;
    let dz = target.z - shooter.z;
    let horizontal = (dx * dx + dz * dz).sqrt();
    let dy = (target.y + target_height * TARGET_HEIGHT_FRACTION - projectile_y) + horizontal * ARC;
    Vec3::new(dx, dy, dz)
}

pub fn up_vector(pitch: f32, yaw: f32) -> Vec3 {
    let pitch = f64::from((pitch - 90.0) * DEGREES_TO_RADIANS);
    let yaw = f64::from(-yaw * DEGREES_TO_RADIANS);
    let (cos_yaw, sin_yaw) = (f64::from(mth::cos(yaw)), f64::from(mth::sin(yaw)));
    let (cos_pitch, sin_pitch) = (f64::from(mth::cos(pitch)), f64::from(mth::sin(pitch)));
    Vec3::new(sin_yaw * cos_pitch, -sin_pitch, cos_yaw * cos_pitch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::random::LegacyRandom;

    fn crossbow() -> ItemStack {
        single(ItemKind::Crossbow)
    }

    fn enchanted(name: &str, level: u8) -> ItemStack {
        let mut stack = crossbow();
        stack.set_enchantments(&[(name.to_owned(), level)]);
        stack
    }

    #[test]
    fn a_plain_crossbow_charges_in_twenty_five_ticks() {
        assert_eq!(charge_ticks(&crossbow()), 25);
    }

    #[test]
    fn quick_charge_removes_a_quarter_second_per_level_down_to_nothing() {
        let ticks: Vec<i32> = (1..=6)
            .map(|level| charge_ticks(&enchanted(QUICK_CHARGE, level)))
            .collect();
        assert_eq!(ticks, vec![20, 15, 10, 5, 0, 0]);
    }

    #[test]
    fn multishot_adds_two_projectiles_and_ten_degrees_per_level() {
        assert_eq!(projectile_count(&crossbow()), 1);
        assert_eq!(projectile_spread(&crossbow()), 0.0);
        let one = enchanted(MULTISHOT, 1);
        assert_eq!((projectile_count(&one), projectile_spread(&one)), (3, 10.0));
        let two = enchanted(MULTISHOT, 2);
        assert_eq!((projectile_count(&two), projectile_spread(&two)), (5, 20.0));
    }

    #[test]
    fn piercing_counts_its_level() {
        assert_eq!(piercing_level(&crossbow()), 0);
        assert_eq!(piercing_level(&enchanted(PIERCING, 4)), 4);
    }

    #[test]
    fn shot_angles_fan_out_around_the_centre_in_the_vanilla_order() {
        assert_eq!(shot_angles(1, 0.0), vec![0.0]);
        assert_eq!(shot_angles(3, 10.0), vec![0.0, -10.0, 10.0]);
        assert_eq!(shot_angles(5, 20.0), vec![0.0, -10.0, 10.0, -20.0, 20.0]);
        assert_eq!(shot_angles(2, 10.0), vec![10.0, -10.0]);
    }

    #[test]
    fn the_first_shot_is_silent_of_pitch_noise_and_the_others_alternate_high_and_low() {
        let mut random = LegacyRandom::new(5);
        assert_eq!(shot_pitch(&mut random, 0), 1.0);
        let mut mirror = LegacyRandom::new(5);
        let low = 1.0 / (mirror.next_float() * 0.5 + 1.8) + 0.43;
        let high = 1.0 / (mirror.next_float() * 0.5 + 1.8) + 0.63;
        assert_eq!(shot_pitch(&mut random, 2), low);
        assert_eq!(shot_pitch(&mut random, 1), high);
    }

    #[test]
    fn the_uncertainty_shrinks_with_the_difficulty() {
        assert_eq!(
            (0..=3).map(ranged_uncertainty).collect::<Vec<_>>(),
            vec![14.0, 10.0, 6.0, 2.0]
        );
    }

    #[test]
    fn charging_sounds_follow_the_quick_charge_level() {
        assert_eq!(
            charging_sounds(&crossbow()),
            ChargingSounds {
                start: LOADING_START_SOUND,
                mid: LOADING_MIDDLE_SOUND,
                end: LOADING_END_SOUND
            }
        );
        assert_eq!(
            charging_sounds(&enchanted(QUICK_CHARGE, 2)).start,
            QUICK_CHARGE_SOUNDS[1]
        );
        assert_eq!(
            charging_sounds(&enchanted(QUICK_CHARGE, 9)).start,
            QUICK_CHARGE_SOUNDS[2]
        );
    }

    #[test]
    fn the_charging_sound_pitch_follows_the_vanilla_curve() {
        let mut random = LegacyRandom::new(8);
        let mut mirror = LegacyRandom::new(8);
        assert_eq!(
            charging_sound_pitch(&mut random),
            1.0 / (mirror.next_float() * 0.5 + 1.0) + 0.2
        );
    }

    #[test]
    fn projectiles_are_stored_in_the_charged_component_and_cleared_with_it() {
        let mut weapon = crossbow();
        assert!(!is_charged(&weapon));
        let arrow = single(ItemKind::Arrow);
        let rocket = single(ItemKind::FireworkRocket);
        set_charged(&mut weapon, &[arrow.clone(), rocket.clone()]);
        assert!(is_charged(&weapon));
        assert_eq!(charged_projectiles(&weapon), vec![arrow, rocket]);
        set_charged(&mut weapon, &[]);
        assert!(!is_charged(&weapon));
        assert!(charged_projectiles(&weapon).is_empty());
    }

    #[test]
    fn fireworks_shoot_slower_and_wear_the_crossbow_three_times_as_much() {
        let arrow = single(ItemKind::Arrow);
        let rocket = single(ItemKind::FireworkRocket);
        assert_eq!(shooting_power(std::slice::from_ref(&arrow)), 3.15);
        assert_eq!(shooting_power(&[arrow.clone(), rocket.clone()]), 1.6);
        assert_eq!(durability_use(&arrow), 1);
        assert_eq!(durability_use(&rocket), 3);
        assert!(is_arrow_or_firework(&arrow) && is_arrow_or_firework(&rocket));
        assert!(!is_arrow_or_firework(&single(ItemKind::Stick)));
    }

    #[test]
    fn a_straight_shot_is_the_normalised_aim_in_single_precision() {
        let aim = Vec3::new(3.0, 1.0, 4.0);
        let shot = shot_vector(aim, 0.0, Vec3::new(0.0, 1.0, 0.0));
        let length = (9.0_f32 + 1.0 + 16.0).sqrt();
        let expected = Vec3::new(
            f64::from(3.0_f32 * (1.0 / length)),
            f64::from(1.0_f32 * (1.0 / length)),
            f64::from(4.0_f32 * (1.0 / length)),
        );
        assert_eq!(shot, expected);
    }

    #[test]
    fn a_fanned_shot_turns_the_aim_sideways_by_the_requested_angle() {
        let aim = Vec3::new(0.0, 0.0, 5.0);
        let up = Vec3::new(0.0, 1.0, 0.0);
        let left = shot_vector(aim, 10.0, up);
        let right = shot_vector(aim, -10.0, up);
        let radians = f64::from(10.0_f32 * DEGREES_TO_RADIANS);
        assert!((left.x.abs() - radians.sin()).abs() < 1.0e-5);
        assert!((left.z - radians.cos()).abs() < 1.0e-5);
        assert!((left.x + right.x).abs() < 1.0e-5);
        assert!(left.y.abs() < 1.0e-6);
        assert!((left.length() - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn a_vertical_aim_falls_back_to_the_shooter_up_vector_for_the_fan_axis() {
        let aim = Vec3::new(0.0, 5.0, 0.0);
        let shot = shot_vector(aim, 10.0, Vec3::new(0.0, 0.0, 1.0));
        assert!((shot.length() - 1.0).abs() < 1.0e-5);
        assert!(shot.y < 1.0);
    }

    #[test]
    fn the_aim_rises_with_the_horizontal_distance_and_targets_a_third_of_the_height() {
        let aim = aim_vector(
            Vec3::new(0.0, 64.0, 0.0),
            Vec3::new(6.0, 64.0, 8.0),
            1.8,
            64.0 + 1.62 - 0.1,
        );
        assert_eq!((aim.x, aim.z), (6.0, 8.0));
        let expected = (64.0 + 1.8 * TARGET_HEIGHT_FRACTION - (64.0 + 1.62 - 0.1)) + 10.0 * ARC;
        assert_eq!(aim.y, expected);
    }

    #[test]
    fn the_up_vector_of_a_level_gaze_points_straight_up() {
        let up = up_vector(0.0, 0.0);
        assert!(up.x.abs() < 1.0e-6 && up.z.abs() < 1.0e-6);
        assert!((up.y - 1.0).abs() < 1.0e-6);
    }
}
