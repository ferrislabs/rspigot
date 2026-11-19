use super::impact::Vec3;

pub const DEFAULT_THROW_POWER: f64 = 1.2;

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
