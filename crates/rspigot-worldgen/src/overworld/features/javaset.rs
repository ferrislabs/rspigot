use super::prov::Pos;

fn spread((x, y, z): Pos) -> u32 {
    let hash = y
        .wrapping_add(z.wrapping_mul(31))
        .wrapping_mul(31)
        .wrapping_add(x) as u32;
    hash ^ (hash >> 16)
}

struct Table {
    buckets: Vec<Vec<(u32, Pos)>>,
    size: usize,
}

impl Table {
    fn new() -> Self {
        Self {
            buckets: vec![Vec::new(); 16],
            size: 0,
        }
    }

    fn resize(&mut self) {
        let capacity = self.buckets.len() * 2;
        let mut grown: Vec<Vec<(u32, Pos)>> = vec![Vec::new(); capacity];
        for bucket in std::mem::take(&mut self.buckets) {
            for (hash, pos) in bucket {
                grown[hash as usize & (capacity - 1)].push((hash, pos));
            }
        }
        self.buckets = grown;
    }

    fn insert(&mut self, pos: Pos) {
        let hash = spread(pos);
        let index = hash as usize & (self.buckets.len() - 1);
        if self.buckets[index].iter().any(|(_, held)| *held == pos) {
            return;
        }
        let before = self.buckets[index].len();
        self.buckets[index].push((hash, pos));
        if before >= 8 && self.buckets.len() < 64 {
            self.resize();
        }
        self.size += 1;
        if self.size > self.buckets.len() * 3 / 4 {
            self.resize();
        }
    }
}

pub(super) fn hash_order(items: impl IntoIterator<Item = Pos>) -> Vec<Pos> {
    let mut table = Table::new();
    for pos in items {
        table.insert(pos);
    }
    table
        .buckets
        .into_iter()
        .flat_map(|bucket| bucket.into_iter().map(|(_, pos)| pos))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order_hash(radius: i32, base: i32, scattered: bool) -> (usize, u64) {
        let mut items = Vec::new();
        let mut state: i64 = 12345;
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let corner = (dx == -radius || dx == radius) && (dz == -radius || dz == radius);
                if corner {
                    continue;
                }
                let mut y = base;
                if scattered {
                    state = (state * 1_103_515_245 + 12345) & 0x7fff_ffff;
                    y += ((state >> 16) & 3) as i32;
                }
                items.push((100 + dx, y, -40 + dz));
            }
        }
        let order = hash_order(items);
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for (x, y, z) in &order {
            for byte in format!("{x},{y},{z}\n").bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
        }
        (order.len(), hash)
    }

    #[test]
    fn iteration_order_follows_the_java_hash_set_of_block_positions() {
        assert_eq!(order_hash(8, 17, false), (285, 0xd5ef_e98a_b51c_8df3));
        assert_eq!(order_hash(3, 70, true), (45, 0xe722_7b98_293c_40b0));
        assert_eq!(order_hash(7, 17, true), (221, 0xc99c_7b16_2c4d_efbc));
        assert_eq!(order_hash(8, 17, true), (285, 0x8ee0_ef0f_cb22_b489));
    }
}
