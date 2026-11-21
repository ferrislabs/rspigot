use crate::random::RandomSource;

pub const USE_DURATION: i32 = 225;
pub const BRUSH_INTERVAL: i32 = 10;
pub const BRUSH_PHASE: i32 = 5;
pub const COOLDOWN_TICKS: i64 = 10;
pub const RESET_TICKS: i64 = 40;
pub const RESET_STEP_TICKS: i64 = 2;
pub const RESET_DECAY: u32 = 2;
pub const REQUIRED_BRUSHES: u32 = 10;
pub const GENERIC_SOUND: &str = "minecraft:item.brush.brushing.generic";
pub const SAND_SOUND: &str = "minecraft:item.brush.brushing.sand";
pub const GRAVEL_SOUND: &str = "minecraft:item.brush.brushing.gravel";
pub const SAND_COMPLETE_SOUND: &str = "minecraft:item.brush.brushing.sand.complete";
pub const GRAVEL_COMPLETE_SOUND: &str = "minecraft:item.brush.brushing.gravel.complete";
pub const COMPLETED_EVENT: i32 = 3008;
pub const DROP_MIN: i32 = 10;
pub const DROP_SPREAD: i32 = 21;
pub const ITEM_SIZE: f64 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BrushState {
    pub count: u32,
    pub last_brushed_at: i64,
    pub cooldown_ends_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Brushed {
    Ignored,
    Progress { completion_changed: bool },
    Completed,
}

pub fn completion(count: u32) -> u8 {
    match count {
        0 => 0,
        1..=2 => 1,
        3..=5 => 2,
        _ => 3,
    }
}

pub fn is_brush_tick(ticks_used: i32) -> bool {
    ticks_used % BRUSH_INTERVAL == BRUSH_PHASE
}

pub fn drop_count(random: &mut impl RandomSource) -> u8 {
    (random.next_int(DROP_SPREAD) + DROP_MIN) as u8
}

impl BrushState {
    pub fn completion(&self) -> u8 {
        completion(self.count)
    }

    pub fn brush(&mut self, now: i64) -> Brushed {
        self.last_brushed_at = now;
        if self.cooldown_ends_at >= now {
            return Brushed::Ignored;
        }
        self.cooldown_ends_at = now + COOLDOWN_TICKS;
        let before = self.completion();
        self.count += 1;
        if self.count >= REQUIRED_BRUSHES {
            return Brushed::Completed;
        }
        Brushed::Progress {
            completion_changed: before != self.completion(),
        }
    }

    pub fn catch_up(&mut self, now: i64) -> bool {
        if self.count == 0 || now < self.last_brushed_at + RESET_TICKS {
            return false;
        }
        let before = self.completion();
        let steps = (now - (self.last_brushed_at + RESET_TICKS)) / RESET_STEP_TICKS + 1;
        let decay = u32::try_from(steps)
            .unwrap_or(u32::MAX)
            .saturating_mul(RESET_DECAY);
        self.count = self.count.saturating_sub(decay);
        self.last_brushed_at += steps * RESET_STEP_TICKS;
        if self.count == 0 {
            self.last_brushed_at = 0;
            self.cooldown_ends_at = 0;
        } else {
            self.cooldown_ends_at = now + 4;
        }
        before != self.completion()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::random::LegacyRandom;

    #[test]
    fn the_dust_stage_follows_the_brush_count() {
        let stages: Vec<u8> = (0..=10).map(completion).collect();
        assert_eq!(stages, vec![0, 1, 1, 2, 2, 2, 3, 3, 3, 3, 3]);
    }

    #[test]
    fn brushing_happens_five_ticks_into_every_ten() {
        let ticks: Vec<i32> = (1..=30).filter(|tick| is_brush_tick(*tick)).collect();
        assert_eq!(ticks, vec![5, 15, 25]);
    }

    #[test]
    fn a_brush_inside_the_cooldown_is_ignored_but_refreshes_the_timer() {
        let mut state = BrushState::default();
        assert_eq!(
            state.brush(100),
            Brushed::Progress {
                completion_changed: true
            }
        );
        assert_eq!(state.brush(105), Brushed::Ignored);
        assert_eq!(state.count, 1);
        assert_eq!(state.last_brushed_at, 105);
        assert_eq!(
            state.brush(111),
            Brushed::Progress {
                completion_changed: false
            }
        );
    }

    #[test]
    fn the_tenth_brush_completes_the_block() {
        let mut state = BrushState::default();
        let mut last = Brushed::Ignored;
        for step in 0..10 {
            last = state.brush(100 + step * 11);
        }
        assert_eq!(last, Brushed::Completed);
    }

    #[test]
    fn the_dust_recedes_two_brushes_per_two_ticks_after_forty_idle_ticks() {
        let mut state = BrushState {
            count: 7,
            last_brushed_at: 100,
            cooldown_ends_at: 110,
        };
        assert!(!state.catch_up(139));
        assert_eq!(state.count, 7);
        assert!(state.catch_up(140));
        assert_eq!(state.count, 5);
        assert!(state.catch_up(144));
        assert_eq!(state.count, 1);
        state.catch_up(146);
        assert_eq!(state, BrushState::default());
    }

    #[test]
    fn the_drop_is_between_ten_and_thirty_items() {
        let mut random = LegacyRandom::new(3);
        for _ in 0..200 {
            assert!((10..=30).contains(&drop_count(&mut random)));
        }
    }
}
