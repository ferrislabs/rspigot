pub const CATALYST_RANGE: i32 = 8;
pub const GROWTH_SPAWN_COST: i32 = 10;
pub const NO_GROWTH_RADIUS: i32 = 4;
pub const CHARGE_DECAY_RATE: i32 = 10;
pub const ADDITIONAL_DECAY_RATE: i32 = 5;
pub const MAX_CURSORS: usize = 32;
pub const MAX_CURSOR_CHARGE: i32 = 1000;
pub const BLOOM_TICKS: u64 = 8;
pub const FAR_RADIUS: f32 = 24.0;
pub const REPLACEABLE_TAG: &str = "minecraft:sculk_replaceable";
pub const BLOOM_SOUND: &str = "minecraft:block.sculk_catalyst.bloom";
pub const SPREAD_SOUND: &str = "minecraft:block.sculk.spread";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub charge: i32,
    pub update_delay: i32,
}

pub fn split_charge(charge: i32) -> Vec<i32> {
    let mut left = charge;
    let mut parts = Vec::new();
    while left > 0 {
        let take = left.min(MAX_CURSOR_CHARGE);
        parts.push(take);
        left -= take;
    }
    parts
}

pub fn decay_penalty(distance_squared: f64, charge: i32) -> i32 {
    let radius = NO_GROWTH_RADIUS as f32;
    let near = (distance_squared.sqrt() as f32 - radius).powi(2);
    let far = (FAR_RADIUS - radius).powi(2);
    let ratio = (near / far).min(1.0);
    1.max((charge as f32 * ratio * 0.5) as i32)
}

pub fn halved(charge: i32) -> i32 {
    (charge as f32 * 0.5).floor() as i32
}

pub fn merge(into: &mut Cursor, other: &Cursor) {
    into.charge = (into.charge + other.charge).min(MAX_CURSOR_CHARGE);
}

pub fn pack(cursors: &[Cursor]) -> Vec<i32> {
    cursors
        .iter()
        .flat_map(|c| [c.x, c.y, c.z, c.charge, c.update_delay])
        .collect()
}

pub fn unpack(data: &[i32]) -> Vec<Cursor> {
    data.as_chunks::<5>()
        .0
        .iter()
        .map(|c| Cursor {
            x: c[0],
            y: c[1],
            z: c[2],
            charge: c[3],
            update_delay: c[4],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn big_charges_split_into_cursors_of_at_most_a_thousand() {
        assert_eq!(split_charge(2500), vec![1000, 1000, 500]);
        assert_eq!(split_charge(0), Vec::<i32>::new());
        assert_eq!(split_charge(7), vec![7]);
    }

    #[test]
    fn the_decay_penalty_grows_with_distance_and_never_drops_below_one() {
        assert_eq!(decay_penalty(0.0, 1), 1);
        assert!(decay_penalty(400.0, 100) > decay_penalty(36.0, 100));
        assert_eq!(decay_penalty(10_000.0, 100), 50);
    }

    #[test]
    fn cursors_survive_a_round_trip_through_their_packed_form() {
        let cursors = [
            Cursor {
                x: -3,
                y: 64,
                z: 9,
                charge: 12,
                update_delay: 1,
            },
            Cursor {
                x: 0,
                y: 0,
                z: 0,
                charge: 1,
                update_delay: 0,
            },
        ];
        assert_eq!(unpack(&pack(&cursors)), cursors);
    }

    #[test]
    fn merging_adds_the_charges_up_to_the_cap() {
        let mut a = Cursor {
            x: 0,
            y: 0,
            z: 0,
            charge: 900,
            update_delay: 1,
        };
        let b = Cursor { charge: 300, ..a };
        merge(&mut a, &b);
        assert_eq!(a.charge, MAX_CURSOR_CHARGE);
        assert_eq!(halved(5), 2);
    }
}
