use super::mask::Mask;

pub(super) const RANGE: i32 = 4;

pub(super) fn floor(value: f64) -> i32 {
    value.floor() as i32
}

pub(super) fn carve_ellipsoid(
    chunk: (i32, i32),
    center: (f64, f64, f64),
    radii: (f64, f64),
    mask: &mut Mask,
    skip: &dyn Fn(f64, f64, f64, i32) -> bool,
) {
    let (x, y, z) = center;
    let (horizontal, vertical) = radii;
    let (min_x, min_z) = (chunk.0 * 16, chunk.1 * 16);
    let reach = horizontal * 2.0 + 16.0;
    if (x - f64::from(min_x + 8)).abs() > reach || (z - f64::from(min_z + 8)).abs() > reach {
        return;
    }
    let from_x = (floor(x - horizontal) - min_x - 1).max(0);
    let to_x = (floor(x + horizontal) - min_x).min(15);
    let bottom = (floor(y - vertical) - 1).max(mask.min_y());
    let top = (floor(y + vertical) + 1).min(mask.max_y());
    let from_z = (floor(z - horizontal) - min_z - 1).max(0);
    let to_z = (floor(z + horizontal) - min_z).min(15);
    for lx in from_x..=to_x {
        let xd = (f64::from(min_x + lx) + 0.5 - x) / horizontal;
        for lz in from_z..=to_z {
            let zd = (f64::from(min_z + lz) + 0.5 - z) / horizontal;
            if xd * xd + zd * zd >= 1.0 {
                continue;
            }
            let mut by = top;
            while by > bottom {
                let yd = (f64::from(by) - 0.5 - y) / vertical;
                if !skip(xd, yd, zd, by) {
                    mask.carve(lx as usize, by, lz as usize);
                }
                by -= 1;
            }
        }
    }
}

pub(super) fn can_reach(
    chunk: (i32, i32),
    x: f64,
    z: f64,
    step: i32,
    distance: i32,
    thickness: f32,
) -> bool {
    let dx = x - f64::from(chunk.0 * 16 + 8);
    let dz = z - f64::from(chunk.1 * 16 + 8);
    let remaining = f64::from(distance - step);
    let limit = f64::from(thickness + 2.0 + 16.0);
    dx * dx + dz * dz - remaining * remaining <= limit * limit
}
