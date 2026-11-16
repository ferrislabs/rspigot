pub const MAX_ITERATIONS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub z: f64,
}

impl Point {
    fn distance(self, other: Self) -> f64 {
        (self.x - other.x).hypot(self.z - other.z)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min_x: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_z: f64,
}

impl Bounds {
    pub fn around(center_x: f64, center_z: f64, range: f64) -> Self {
        Self {
            min_x: center_x - range,
            min_z: center_z - range,
            max_x: center_x + range,
            max_z: center_z + range,
        }
    }

    fn random(&self, unit: &mut impl FnMut() -> f64) -> Point {
        Point {
            x: self.min_x + unit() * (self.max_x - self.min_x),
            z: self.min_z + unit() * (self.max_z - self.min_z),
        }
    }

    fn clamp(&self, point: &mut Point) -> bool {
        let mut changed = false;
        if point.x < self.min_x {
            point.x = self.min_x;
            changed = true;
        } else if point.x > self.max_x {
            point.x = self.max_x;
            changed = true;
        }
        if point.z < self.min_z {
            point.z = self.min_z;
            changed = true;
        } else if point.z > self.max_z {
            point.z = self.max_z;
            changed = true;
        }
        changed
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpreadFailure {
    pub smallest_distance: f64,
}

pub fn spread(
    count: usize,
    spread_distance: f64,
    bounds: Bounds,
    mut unit: impl FnMut() -> f64,
    is_safe: impl Fn(Point) -> bool,
) -> Result<Vec<Point>, SpreadFailure> {
    let mut points: Vec<Point> = (0..count).map(|_| bounds.random(&mut unit)).collect();
    let mut smallest = f64::from(f32::MAX);
    let mut moved = true;
    let mut iterations = 0;
    while iterations < MAX_ITERATIONS && moved {
        moved = false;
        smallest = f64::from(f32::MAX);
        for index in 0..points.len() {
            let here = points[index];
            let mut near = 0u32;
            let mut push = Point { x: 0.0, z: 0.0 };
            for (other_index, other) in points.iter().enumerate() {
                if other_index == index {
                    continue;
                }
                let distance = here.distance(*other);
                smallest = smallest.min(distance);
                if distance < spread_distance {
                    near += 1;
                    push.x += other.x - here.x;
                    push.z += other.z - here.z;
                }
            }
            if near > 0 {
                push.x /= f64::from(near);
                push.z /= f64::from(near);
                let length = push.x.hypot(push.z);
                if length <= 0.0 {
                    points[index] = bounds.random(&mut unit);
                } else {
                    points[index].x -= push.x / length;
                    points[index].z -= push.z / length;
                }
                moved = true;
            }
            if bounds.clamp(&mut points[index]) {
                moved = true;
            }
        }
        if !moved {
            for point in &mut points {
                if !is_safe(*point) {
                    *point = bounds.random(&mut unit);
                    moved = true;
                }
            }
        }
        iterations += 1;
    }
    if smallest == f64::from(f32::MAX) {
        smallest = 0.0;
    }
    if iterations >= MAX_ITERATIONS {
        return Err(SpreadFailure {
            smallest_distance: smallest,
        });
    }
    Ok(points)
}

pub fn nearest_distance(points: &[Point], index: usize) -> f64 {
    points
        .iter()
        .enumerate()
        .filter(|(other, _)| *other != index)
        .map(|(_, other)| points[index].distance(*other))
        .fold(f64::MAX, f64::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lcg() -> impl FnMut() -> f64 {
        let mut state = 12_345u64;
        move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    #[test]
    fn points_end_up_at_least_the_spread_distance_apart_inside_the_bounds() {
        let bounds = Bounds::around(0.0, 0.0, 50.0);
        let points = spread(10, 10.0, bounds, lcg(), |_| true).expect("room for ten");
        assert_eq!(points.len(), 10);
        for (index, point) in points.iter().enumerate() {
            assert!(point.x >= bounds.min_x && point.x <= bounds.max_x);
            assert!(point.z >= bounds.min_z && point.z <= bounds.max_z);
            for other in &points[index + 1..] {
                assert!(point.distance(*other) >= 10.0 - 1e-6);
            }
        }
    }

    #[test]
    fn too_many_points_for_the_room_fail_with_the_smallest_distance() {
        let bounds = Bounds::around(0.0, 0.0, 1.0);
        let failure = spread(30, 50.0, bounds, lcg(), |_| true).expect_err("no room");
        assert!(failure.smallest_distance < 50.0);
    }

    #[test]
    fn an_unsafe_spot_is_redrawn() {
        let bounds = Bounds::around(0.0, 0.0, 100.0);
        let points = spread(3, 5.0, bounds, lcg(), |point| point.x > 0.0).expect("spread");
        assert!(points.iter().all(|point| point.x > 0.0));
    }

    #[test]
    fn a_single_point_needs_no_spreading() {
        let bounds = Bounds::around(10.0, -10.0, 5.0);
        let points = spread(1, 100.0, bounds, lcg(), |_| true).expect("one point");
        assert_eq!(points.len(), 1);
        assert_eq!(nearest_distance(&points, 0), f64::MAX);
    }

    #[test]
    fn the_nearest_distance_skips_the_point_itself() {
        let points = [
            Point { x: 0.0, z: 0.0 },
            Point { x: 3.0, z: 4.0 },
            Point { x: 3.0, z: 14.0 },
        ];
        assert_eq!(nearest_distance(&points, 0), 5.0);
        assert_eq!(nearest_distance(&points, 2), 10.0);
    }

    #[test]
    fn clamping_reports_a_move_only_when_a_point_left_the_bounds() {
        let bounds = Bounds::around(0.0, 0.0, 1.0);
        let mut inside = Point { x: 0.5, z: 0.5 };
        assert!(!bounds.clamp(&mut inside));
        let mut outside = Point { x: 5.0, z: -5.0 };
        assert!(bounds.clamp(&mut outside));
        assert_eq!(outside, Point { x: 1.0, z: -1.0 });
    }
}
