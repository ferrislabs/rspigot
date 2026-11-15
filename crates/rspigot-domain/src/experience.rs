const INITIAL_SENT: i32 = -99_999_999;
const LEVEL_UP_EVERY: i32 = 5;
const LEVEL_UP_SOUND_COOLDOWN: f32 = 100.0;
const LEVEL_UP_FULL_VOLUME: i32 = 30;
const LEVEL_UP_VOLUME: f32 = 0.75;
pub const PICKUP_DELAY: i32 = 2;
pub const DEATH_REWARD_PER_LEVEL: i32 = 7;
pub const MAX_DEATH_REWARD: i32 = 100;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerExperience {
    pub level: i32,
    pub progress: f32,
    pub total: i32,
    pub take_delay: i32,
    pub last_sent: i32,
    pub last_level_up: i32,
    pub old_level: i32,
    pub seed: i32,
}

impl Default for PlayerExperience {
    fn default() -> Self {
        Self {
            level: 0,
            progress: 0.0,
            total: 0,
            take_delay: 0,
            last_sent: INITIAL_SENT,
            last_level_up: 0,
            old_level: -1,
            seed: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelUp {
    pub volume: f32,
}

pub fn xp_needed_for_next_level(level: i32) -> i32 {
    if level >= 30 {
        112 + (level - 30) * 9
    } else if level >= 15 {
        37 + (level - 15) * 5
    } else {
        7 + level * 2
    }
}

impl PlayerExperience {
    pub fn needed(&self) -> i32 {
        xp_needed_for_next_level(self.level)
    }

    pub fn give_points(&mut self, amount: i32, tick_count: i32) -> Option<LevelUp> {
        if amount == 0 {
            return None;
        }
        let mut sound = None;
        self.progress += amount as f32 / self.needed() as f32;
        self.total = self.total.wrapping_add(amount).max(0);
        while self.progress < 0.0 {
            let remainder = self.progress * self.needed() as f32;
            if self.level > 0 {
                sound = sound.or(self.give_levels(-1, tick_count));
                self.progress = 1.0 + remainder / self.needed() as f32;
            } else {
                sound = sound.or(self.give_levels(-1, tick_count));
                self.progress = 0.0;
            }
        }
        while self.progress >= 1.0 {
            self.progress = (self.progress - 1.0) * self.needed() as f32;
            sound = sound.or(self.give_levels(1, tick_count));
            self.progress /= self.needed() as f32;
        }
        self.last_sent = -1;
        sound
    }

    pub fn give_levels(&mut self, levels: i32, tick_count: i32) -> Option<LevelUp> {
        if levels == 0 {
            return None;
        }
        self.level = self.level.saturating_add(levels);
        if self.level < 0 {
            self.level = 0;
            self.progress = 0.0;
            self.total = 0;
        }
        self.last_sent = -1;
        if levels > 0
            && self.level % LEVEL_UP_EVERY == 0
            && (self.last_level_up as f32) < tick_count as f32 - LEVEL_UP_SOUND_COOLDOWN
        {
            let volume = if self.level > LEVEL_UP_FULL_VOLUME {
                1.0
            } else {
                self.level as f32 / LEVEL_UP_FULL_VOLUME as f32
            };
            self.last_level_up = tick_count;
            return Some(LevelUp {
                volume: volume * LEVEL_UP_VOLUME,
            });
        }
        None
    }

    pub fn death_reward(&self) -> i32 {
        (self.level * DEATH_REWARD_PER_LEVEL).min(MAX_DEATH_REWARD)
    }

    pub fn take_unsent(&mut self) -> Option<(f32, i32, i32)> {
        if self.total == self.last_sent {
            return None;
        }
        self.last_sent = self.total;
        Some((self.progress, self.level, self.total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_curve_follows_the_three_vanilla_segments() {
        assert_eq!(xp_needed_for_next_level(0), 7);
        assert_eq!(xp_needed_for_next_level(14), 35);
        assert_eq!(xp_needed_for_next_level(15), 37);
        assert_eq!(xp_needed_for_next_level(29), 107);
        assert_eq!(xp_needed_for_next_level(30), 112);
        assert_eq!(xp_needed_for_next_level(31), 121);
    }

    #[test]
    fn points_fill_the_bar_and_carry_over_levels() {
        let mut xp = PlayerExperience::default();
        assert_eq!(xp.give_points(3, 0), None);
        assert_eq!(xp.level, 0);
        assert!((xp.progress - 3.0 / 7.0).abs() < 1e-6);
        assert_eq!(xp.total, 3);
        let mut exact = PlayerExperience::default();
        exact.give_points(7, 0);
        assert_eq!((exact.level, exact.progress, exact.total), (1, 0.0, 7));
        exact.give_points(9, 0);
        assert_eq!((exact.level, exact.progress, exact.total), (2, 0.0, 16));
        exact.give_points(4, 0);
        assert_eq!(
            (exact.level, exact.progress, exact.total),
            (2, 4.0 / 11.0, 20)
        );
    }

    #[test]
    fn every_fifth_level_plays_the_level_up_sound_once_per_hundred_ticks() {
        let mut xp = PlayerExperience::default();
        let sound = xp.give_levels(5, 200);
        assert_eq!(
            sound,
            Some(LevelUp {
                volume: 5.0 / 30.0 * 0.75
            })
        );
        assert_eq!(xp.give_levels(5, 250), None);
        assert_eq!(xp.level, 10);
        assert!(xp.give_levels(25, 400).is_some());
        assert_eq!(xp.give_levels(-50, 900), None);
        assert_eq!((xp.level, xp.progress, xp.total), (0, 0.0, 0));
    }

    #[test]
    fn unsent_totals_are_reported_once() {
        let mut xp = PlayerExperience::default();
        assert_eq!(xp.take_unsent(), Some((0.0, 0, 0)));
        assert_eq!(xp.take_unsent(), None);
        xp.give_points(2, 0);
        assert_eq!(xp.take_unsent(), Some((2.0 / 7.0, 0, 2)));
        assert_eq!(xp.take_unsent(), None);
    }

    #[test]
    fn death_reward_is_seven_per_level_capped_at_one_hundred() {
        let mut xp = PlayerExperience::default();
        assert_eq!(xp.death_reward(), 0);
        xp.level = 3;
        assert_eq!(xp.death_reward(), 21);
        xp.level = 40;
        assert_eq!(xp.death_reward(), 100);
    }
}
