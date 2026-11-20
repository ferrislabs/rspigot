pub(crate) const BIOME_RADIUS: i32 = 6400;
pub(crate) const HORIZONTAL_STEP: i32 = 32;
pub(crate) const VERTICAL_STEP: i32 = 64;

pub(crate) struct Spiral {
    x: i32,
    z: i32,
    direction: usize,
    leg: i32,
    walked: i32,
    legs_at_length: u8,
    remaining: i64,
}

const DIRECTIONS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

impl Spiral {
    pub(crate) fn new(radius: i32) -> Self {
        let side = i64::from(radius) * 2 + 1;
        Self {
            x: 0,
            z: 0,
            direction: 0,
            leg: 1,
            walked: -1,
            legs_at_length: 0,
            remaining: side * side,
        }
    }
}

impl Iterator for Spiral {
    type Item = (i32, i32);

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        if self.walked < 0 {
            self.walked = 0;
            return Some((0, 0));
        }
        let (dx, dz) = DIRECTIONS[self.direction];
        self.x += dx;
        self.z += dz;
        self.walked += 1;
        if self.walked == self.leg {
            self.walked = 0;
            self.direction = (self.direction + 1) % 4;
            self.legs_at_length += 1;
            if self.legs_at_length == 2 {
                self.legs_at_length = 0;
                self.leg += 1;
            }
        }
        Some((self.x, self.z))
    }
}

pub(crate) fn heights(origin: i32, min_y: i32, max_y: i32, step: i32) -> Vec<i32> {
    let inside = |y: i32| (min_y..=max_y).contains(&y);
    let mut out = Vec::new();
    if inside(origin) {
        out.push(origin);
    }
    let mut offset = step;
    while origin + offset <= max_y || origin - offset >= min_y {
        for y in [origin + offset, origin - offset] {
            if inside(y) {
                out.push(y);
            }
        }
        offset += step;
    }
    out
}

pub(crate) struct Search<'a, F: Fn(i32, i32, i32) -> u8> {
    pub(crate) sample: F,
    pub(crate) names: &'a [&'a str],
    pub(crate) min_y: i32,
    pub(crate) max_y: i32,
}

impl<F: Fn(i32, i32, i32) -> u8> Search<'_, F> {
    pub(crate) fn closest(
        &self,
        wanted: &[String],
        from: (i32, i32, i32),
        radius: i32,
    ) -> Option<(String, (i32, i32, i32))> {
        let ids: Vec<(u8, &String)> = wanted
            .iter()
            .filter_map(|name| {
                self.names
                    .iter()
                    .position(|candidate| candidate == name)
                    .map(|index| (index as u8, name))
            })
            .collect();
        if ids.is_empty() {
            return None;
        }
        let ys = heights(from.1, self.min_y + 1, self.max_y, VERTICAL_STEP);
        for (sx, sz) in Spiral::new(radius / HORIZONTAL_STEP) {
            let x = from.0 + sx * HORIZONTAL_STEP;
            let z = from.2 + sz * HORIZONTAL_STEP;
            for &y in &ys {
                let found = (self.sample)(x >> 2, y >> 2, z >> 2);
                if let Some((_, name)) = ids.iter().find(|(id, _)| *id == found) {
                    return Some(((*name).clone(), (x, y, z)));
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spiral_covers_the_square_once_from_the_centre_outwards() {
        let cells: Vec<(i32, i32)> = Spiral::new(3).collect();
        assert_eq!(cells.len(), 49);
        assert_eq!(&cells[..5], &[(0, 0), (1, 0), (1, 1), (0, 1), (-1, 1)]);
        let mut unique = cells.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), 49);
        assert!(cells.iter().all(|(x, z)| x.abs() <= 3 && z.abs() <= 3));
    }

    #[test]
    fn heights_go_out_from_the_origin_within_the_bounds() {
        assert_eq!(heights(64, -63, 320, 64), [64, 128, 0, 192, 256, 320]);
    }

    #[test]
    fn the_nearest_sampled_biome_wins() {
        let names = ["minecraft:plains", "minecraft:desert", "minecraft:ocean"];
        let search = Search {
            sample: |qx, _, qz| {
                if qx >= 40 && qz == 0 {
                    1
                } else if qx <= -64 {
                    2
                } else {
                    0
                }
            },
            names: &names,
            min_y: -64,
            max_y: 320,
        };
        let wanted = vec!["minecraft:desert".to_owned(), "minecraft:ocean".to_owned()];
        let (name, at) = search.closest(&wanted, (0, 64, 0), 6400).unwrap();
        assert_eq!(name, "minecraft:desert");
        assert_eq!(at, (160, 64, 0));
        let none = vec!["minecraft:the_end".to_owned()];
        assert!(search.closest(&none, (0, 64, 0), 6400).is_none());
    }
}
