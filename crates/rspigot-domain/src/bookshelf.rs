pub const SLOTS: usize = 6;
const COLUMNS: usize = 3;
const LEFT_EDGE: f64 = 0.375;
const RIGHT_EDGE: f64 = 0.6875;

pub fn hit_slot(facing: &str, hit_face: &str, x: f64, y: f64, z: f64) -> Option<usize> {
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
    let row = usize::from(y < 0.5);
    let column = if across < LEFT_EDGE {
        0
    } else if across < RIGHT_EDGE {
        1
    } else {
        2
    };
    Some(column + row * COLUMNS)
}

pub fn comparator_signal(last_interacted: i32) -> i32 {
    last_interacted + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_front_face_hits_a_slot() {
        assert_eq!(hit_slot("north", "north", 0.5, 0.75, 0.0), Some(1));
        assert_eq!(hit_slot("north", "up", 0.5, 0.75, 0.0), None);
    }

    #[test]
    fn the_top_row_is_zero_to_two_and_the_bottom_row_three_to_five() {
        assert_eq!(hit_slot("south", "south", 0.1, 0.9, 1.0), Some(0));
        assert_eq!(hit_slot("south", "south", 0.9, 0.9, 1.0), Some(2));
        assert_eq!(hit_slot("south", "south", 0.1, 0.1, 1.0), Some(3));
        assert_eq!(hit_slot("south", "south", 0.9, 0.1, 1.0), Some(5));
    }

    #[test]
    fn the_columns_are_read_from_the_viewers_left() {
        assert_eq!(hit_slot("north", "north", 0.9, 0.75, 0.0), Some(0));
        assert_eq!(hit_slot("east", "east", 1.0, 0.75, 0.9), Some(0));
        assert_eq!(hit_slot("west", "west", 0.0, 0.75, 0.9), Some(2));
    }

    #[test]
    fn the_comparator_is_one_more_than_the_slot() {
        assert_eq!(comparator_signal(-1), 0);
        assert_eq!(comparator_signal(4), 5);
    }
}
