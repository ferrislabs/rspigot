use rspigot_blocks::{BlockKind, StateId};

use crate::random::RandomSource;

const BONEMEAL_MIN_INCREASE: i32 = 2;
const BONEMEAL_MAX_INCREASE: i32 = 5;
const BEETROOT_INCREASE_DIVISOR: i32 = 3;

pub fn max_age(kind: BlockKind) -> Option<i32> {
    match kind {
        BlockKind::Wheat | BlockKind::Carrots | BlockKind::Potatoes => Some(7),
        BlockKind::Beetroots => Some(3),
        _ => None,
    }
}

pub fn age(state: StateId) -> Option<i32> {
    state.property("age")?.parse().ok()
}

pub fn is_crop(state: StateId) -> bool {
    state.kind().and_then(max_age).is_some()
}

pub fn is_max_age(state: StateId) -> bool {
    match (state.kind().and_then(max_age), age(state)) {
        (Some(max), Some(age)) => age >= max,
        _ => false,
    }
}

pub fn is_valid_bonemeal_target(state: StateId) -> bool {
    is_crop(state) && !is_max_age(state)
}

pub fn bonemeal_age_increase(kind: BlockKind, random: &mut impl RandomSource) -> i32 {
    let increase = random.next_int_between(BONEMEAL_MIN_INCREASE, BONEMEAL_MAX_INCREASE);
    if kind == BlockKind::Beetroots {
        increase / BEETROOT_INCREASE_DIVISOR
    } else {
        increase
    }
}

pub fn state_for_age(state: StateId, age: i32) -> Option<StateId> {
    state.with_property("age", &age.to_string())
}

pub fn bonemealed_state(state: StateId, random: &mut impl RandomSource) -> Option<StateId> {
    let kind = state.kind()?;
    let max = max_age(kind)?;
    let current = age(state)?;
    let grown = max.min(current + bonemeal_age_increase(kind, random));
    state_for_age(state, grown)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(i32);

    impl RandomSource for Fixed {
        fn next_float(&mut self) -> f32 {
            0.0
        }

        fn next_int(&mut self, bound: i32) -> i32 {
            self.0.min(bound - 1)
        }

        fn next_double(&mut self) -> f64 {
            0.0
        }

        fn next_boolean(&mut self) -> bool {
            false
        }

        fn next_gaussian(&mut self) -> f64 {
            0.0
        }
    }

    fn wheat(age: i32) -> StateId {
        state_for_age(BlockKind::Wheat.default_state(), age).unwrap()
    }

    fn beetroots(age: i32) -> StateId {
        state_for_age(BlockKind::Beetroots.default_state(), age).unwrap()
    }

    #[test]
    fn max_ages_follow_the_crop_blocks() {
        assert_eq!(max_age(BlockKind::Wheat), Some(7));
        assert_eq!(max_age(BlockKind::Carrots), Some(7));
        assert_eq!(max_age(BlockKind::Potatoes), Some(7));
        assert_eq!(max_age(BlockKind::Beetroots), Some(3));
        assert_eq!(max_age(BlockKind::Stone), None);
    }

    #[test]
    fn only_immature_crops_are_bonemeal_targets() {
        assert!(is_valid_bonemeal_target(wheat(0)));
        assert!(is_valid_bonemeal_target(wheat(6)));
        assert!(!is_valid_bonemeal_target(wheat(7)));
        assert!(is_valid_bonemeal_target(beetroots(2)));
        assert!(!is_valid_bonemeal_target(beetroots(3)));
        assert!(!is_valid_bonemeal_target(BlockKind::Stone.default_state()));
    }

    #[test]
    fn bonemeal_adds_two_to_five_ages_and_caps_at_max() {
        assert_eq!(bonemealed_state(wheat(0), &mut Fixed(0)), Some(wheat(2)));
        assert_eq!(bonemealed_state(wheat(1), &mut Fixed(3)), Some(wheat(6)));
        assert_eq!(bonemealed_state(wheat(5), &mut Fixed(3)), Some(wheat(7)));
    }

    #[test]
    fn beetroot_bonemeal_divides_the_increase_by_three() {
        assert_eq!(
            bonemealed_state(beetroots(0), &mut Fixed(0)),
            Some(beetroots(0))
        );
        assert_eq!(
            bonemealed_state(beetroots(0), &mut Fixed(1)),
            Some(beetroots(1))
        );
        assert_eq!(
            bonemealed_state(beetroots(2), &mut Fixed(3)),
            Some(beetroots(3))
        );
    }
}
