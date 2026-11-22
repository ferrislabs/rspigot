pub const SLOTS: usize = 3;

pub fn hit_slot(facing: &str, hit_face: &str, x: f64, z: f64) -> Option<usize> {
    if facing != hit_face {
        return None;
    }
    let across = match facing {
        "north" => 1.0 - x,
        "south" => x,
        "west" => z,
        "east" => 1.0 - z,
        _ => return None,
    };
    if !(0.0..=1.0).contains(&across) {
        return None;
    }
    Some(((across * SLOTS as f64) as usize).min(SLOTS - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_front_face_hits_a_slot() {
        assert_eq!(hit_slot("north", "north", 0.5, 0.0), Some(1));
        assert_eq!(hit_slot("north", "up", 0.5, 0.0), None);
        assert_eq!(hit_slot("north", "south", 0.5, 0.0), None);
    }

    #[test]
    fn slots_run_left_to_right_as_the_viewer_sees_them() {
        assert_eq!(hit_slot("north", "north", 0.9, 0.0), Some(0));
        assert_eq!(hit_slot("north", "north", 0.1, 0.0), Some(2));
        assert_eq!(hit_slot("south", "south", 0.1, 1.0), Some(0));
        assert_eq!(hit_slot("south", "south", 0.9, 1.0), Some(2));
        assert_eq!(hit_slot("west", "west", 0.0, 0.1), Some(0));
        assert_eq!(hit_slot("west", "west", 0.0, 0.9), Some(2));
        assert_eq!(hit_slot("east", "east", 1.0, 0.9), Some(0));
        assert_eq!(hit_slot("east", "east", 1.0, 0.1), Some(2));
    }

    #[test]
    fn the_edge_belongs_to_the_last_slot() {
        assert_eq!(hit_slot("south", "south", 1.0, 1.0), Some(2));
    }
}
