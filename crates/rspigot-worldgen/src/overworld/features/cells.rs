use super::prov::Pos;

pub(super) fn within_manhattan(origin: Pos, reach: Pos) -> Vec<Pos> {
    let (rx, ry, rz) = reach;
    let limit = rx + ry + rz;
    let mut cells = Vec::new();
    let (mut depth, mut max_x, mut max_y, mut x, mut y) = (0i32, 0i32, 0i32, 0i32, 0i32);
    loop {
        let mut found = None;
        while found.is_none() {
            if y > max_y {
                x += 1;
                if x > max_x {
                    depth += 1;
                    if depth > limit {
                        return cells;
                    }
                    max_x = rx.min(depth);
                    x = -max_x;
                }
                max_y = ry.min(depth - x.abs());
                y = -max_y;
            }
            let z = depth - x.abs() - y.abs();
            if z <= rz {
                found = Some((x, y, z));
            }
            y += 1;
        }
        if let Some((dx, dy, dz)) = found {
            cells.push((origin.0 + dx, origin.1 + dy, origin.2 + dz));
            if dz != 0 {
                cells.push((origin.0 + dx, origin.1 + dy, origin.2 - dz));
            }
        }
    }
}
