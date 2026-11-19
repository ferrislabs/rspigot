use super::impact::Vec3;

pub const DEFAULT_THROW_POWER: f64 = 2.7;

pub fn launch_velocity(yaw_degrees: f64, pitch_degrees: f64, power: f64) -> Vec3 {
    let yaw = yaw_degrees.to_radians();
    let pitch = pitch_degrees.to_radians();
    Vec3 {
        x: -yaw.sin() * pitch.cos() * power,
        y: -pitch.sin() * power,
        z: yaw.cos() * pitch.cos() * power,
    }
}

pub fn steer(from: Vec3, target: Vec3, speed: f64) -> Vec3 {
    let (dx, dy, dz) = (target.x - from.x, target.y - from.y, target.z - from.z);
    let length = (dx * dx + dy * dy + dz * dz).sqrt();
    if length <= f64::EPSILON {
        return Vec3::ZERO;
    }
    Vec3 {
        x: dx / length * speed,
        y: dy / length * speed,
        z: dz / length * speed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::{ArenaSettings, Layout};
    use crate::domain::TeamId;

    const GRAVITY: f64 = 0.08;
    const HORIZONTAL_DRAG: f64 = 0.98;
    const THROW_PITCH_DEGREES: f64 = -40.0;
    const MINECRAFT_VELOCITY_CAP: f64 = 3.9;
    const POWER_STEP: f64 = 0.1;

    fn flat_range(power: f64, pitch_degrees: f64) -> f64 {
        let v = launch_velocity(0.0, pitch_degrees, power);
        let (mut z, mut y, mut vz, mut vy) = (0.0, 0.0, v.z, v.y);
        loop {
            let previous = (z, y);
            z += vz;
            y += vy;
            vz *= HORIZONTAL_DRAG;
            vy -= GRAVITY;
            if y < 0.0 {
                let share = previous.1 / (previous.1 - y);
                return previous.0 + (z - previous.0) * share;
            }
        }
    }

    fn spawn_distance() -> f64 {
        let layout = Layout::new(ArenaSettings::default());
        let red = layout.spawn_of(TeamId::Red);
        let blue = layout.spawn_of(TeamId::Blue);
        (red.x - blue.x).hypot(red.z - blue.z)
    }

    #[test]
    fn default_power_crosses_the_arena_at_forty_degrees() {
        assert!(flat_range(DEFAULT_THROW_POWER, THROW_PITCH_DEGREES) >= spawn_distance());
    }

    #[test]
    fn default_power_is_the_smallest_round_value_that_crosses() {
        let lower = DEFAULT_THROW_POWER - POWER_STEP;
        assert!(flat_range(lower, THROW_PITCH_DEGREES) < spawn_distance());
    }

    #[test]
    fn default_power_stays_under_the_minecraft_velocity_cap() {
        const { assert!(DEFAULT_THROW_POWER < MINECRAFT_VELOCITY_CAP) };
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn looking_south_level_throws_along_positive_z() {
        let v = launch_velocity(0.0, 0.0, 2.0);
        assert!(close(v.x, 0.0) && close(v.y, 0.0) && close(v.z, 2.0));
    }

    #[test]
    fn yaw_90_throws_along_negative_x() {
        let v = launch_velocity(90.0, 0.0, 1.0);
        assert!(close(v.x, -1.0) && close(v.z, 0.0));
    }

    #[test]
    fn looking_up_throws_upward() {
        let v = launch_velocity(0.0, -90.0, 1.0);
        assert!(close(v.y, 1.0));
    }

    #[test]
    fn power_scales_the_length() {
        let v = launch_velocity(37.0, -20.0, 1.5);
        let length = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt();
        assert!(close(length, 1.5));
    }

    #[test]
    fn steering_points_at_the_target_with_the_given_speed() {
        let v = steer(
            Vec3::ZERO,
            Vec3 {
                x: 0.0,
                y: 0.0,
                z: 10.0,
            },
            0.8,
        );
        assert!(close(v.z, 0.8) && close(v.x, 0.0));
        assert_eq!(steer(Vec3::ZERO, Vec3::ZERO, 1.0), Vec3::ZERO);
    }
}
