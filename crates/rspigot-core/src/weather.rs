use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::commands::Weather;
use crate::config::LevelData;
use crate::ports::ClientLink;
use crate::server::Server;

const RAIN_DELAY: (i32, i32) = (12_000, 180_000);
const RAIN_DURATION: (i32, i32) = (12_000, 24_000);
const THUNDER_DELAY: (i32, i32) = (12_000, 180_000);
const THUNDER_DURATION: (i32, i32) = (3_600, 15_600);
const WEATHER_SALT: u64 = 0x57EA_7E12_C1C1_E5A1;

fn weather_rng(game_time: i64) -> StdRng {
    StdRng::seed_from_u64((game_time as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ WEATHER_SALT)
}

fn sample(rng: &mut impl Rng, (low, high): (i32, i32)) -> i32 {
    rng.random_range(low..=high)
}

fn count_down(
    time: &mut i32,
    on: &mut bool,
    rng: &mut impl Rng,
    delay: (i32, i32),
    duration: (i32, i32),
) {
    if *time > 0 {
        *time -= 1;
        if *time == 0 {
            *on = !*on;
        }
    } else {
        *time = sample(rng, if *on { duration } else { delay });
    }
}

pub(crate) fn advance(level: &mut LevelData, rng: &mut impl Rng) -> (bool, bool) {
    let mut raining = level.raining;
    let mut thundering = level.thundering;
    if level.clear_weather_time > 0 {
        level.clear_weather_time -= 1;
        level.thunder_time = i32::from(!thundering);
        level.rain_time = i32::from(!raining);
        return (false, false);
    }
    count_down(
        &mut level.thunder_time,
        &mut thundering,
        rng,
        THUNDER_DELAY,
        THUNDER_DURATION,
    );
    count_down(
        &mut level.rain_time,
        &mut raining,
        rng,
        RAIN_DELAY,
        RAIN_DURATION,
    );
    (raining, thundering)
}

pub(crate) fn set_cycle(
    level: &mut LevelData,
    rng: &mut impl Rng,
    weather: Weather,
    duration: Option<i32>,
) {
    let (clear, rain) = match weather {
        Weather::Clear => (duration.unwrap_or_else(|| sample(rng, RAIN_DELAY)), 0),
        Weather::Rain => (0, duration.unwrap_or_else(|| sample(rng, RAIN_DURATION))),
        Weather::Thunder => (0, duration.unwrap_or_else(|| sample(rng, THUNDER_DURATION))),
    };
    level.clear_weather_time = clear;
    level.rain_time = rain;
    level.thunder_time = rain;
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_weather(&mut self) {
        if !self.world.dimension.can_have_weather() {
            return;
        }
        let mut rng = weather_rng(self.world.level.game_time);
        let (raining, thundering) = advance(&mut self.world.level, &mut rng);
        if raining != self.world.level.raining || thundering != self.world.level.thundering {
            self.change_weather(raining, thundering);
        }
    }

    pub(crate) fn command_weather(&mut self, weather: Weather, duration: Option<i32>) {
        let mut rng = weather_rng(self.world.level.game_time);
        set_cycle(&mut self.world.level, &mut rng, weather, duration);
        self.change_weather(weather != Weather::Clear, weather == Weather::Thunder);
    }

    pub(crate) fn reset_weather_cycle(&mut self) {
        self.world.level.rain_time = 0;
        self.world.level.thunder_time = 0;
        self.change_weather(false, false);
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use super::*;

    fn rng() -> StdRng {
        StdRng::seed_from_u64(7)
    }

    #[test]
    fn the_rain_timer_counts_down_then_flips_the_rain_and_draws_a_duration() {
        let mut level = LevelData {
            rain_time: 2,
            thunder_time: 5,
            ..LevelData::default()
        };
        let mut rng = rng();
        assert_eq!(advance(&mut level, &mut rng), (false, false));
        assert_eq!(level.rain_time, 1);
        assert_eq!(advance(&mut level, &mut rng), (true, false));
        assert_eq!(level.rain_time, 0);
        level.raining = true;
        assert_eq!(advance(&mut level, &mut rng), (true, false));
        assert!((12_000..=24_000).contains(&level.rain_time));
    }

    #[test]
    fn the_rain_flips_back_when_the_duration_runs_out() {
        let mut level = LevelData {
            raining: true,
            rain_time: 1,
            thunder_time: 5,
            ..LevelData::default()
        };
        assert_eq!(advance(&mut level, &mut rng()), (false, false));
        assert_eq!(level.rain_time, 0);
        level.raining = false;
        advance(&mut level, &mut rng());
        assert!((12_000..=180_000).contains(&level.rain_time));
    }

    #[test]
    fn a_thunderstorm_draws_its_duration_from_the_thunder_range() {
        let mut level = LevelData {
            thundering: true,
            rain_time: 5,
            ..LevelData::default()
        };
        advance(&mut level, &mut rng());
        assert!((3_600..=15_600).contains(&level.thunder_time));
    }

    #[test]
    fn the_clear_weather_time_keeps_the_sky_clear_while_it_counts_down() {
        let mut level = LevelData {
            raining: true,
            thundering: true,
            clear_weather_time: 2,
            rain_time: 1,
            thunder_time: 1,
            ..LevelData::default()
        };
        let mut rng = rng();
        assert_eq!(advance(&mut level, &mut rng), (false, false));
        assert_eq!(level.clear_weather_time, 1);
        assert_eq!((level.rain_time, level.thunder_time), (0, 0));
        level.raining = false;
        level.thundering = false;
        assert_eq!(advance(&mut level, &mut rng), (false, false));
        assert_eq!(level.clear_weather_time, 0);
        assert_eq!((level.rain_time, level.thunder_time), (1, 1));
        assert_eq!(advance(&mut level, &mut rng), (true, true));
    }

    #[test]
    fn a_command_with_a_duration_sets_the_timers() {
        let mut level = LevelData::default();
        set_cycle(&mut level, &mut rng(), Weather::Clear, Some(100));
        assert_eq!(
            (
                level.clear_weather_time,
                level.rain_time,
                level.thunder_time
            ),
            (100, 0, 0)
        );
        set_cycle(&mut level, &mut rng(), Weather::Rain, Some(200));
        assert_eq!(
            (
                level.clear_weather_time,
                level.rain_time,
                level.thunder_time
            ),
            (0, 200, 200)
        );
        set_cycle(&mut level, &mut rng(), Weather::Thunder, Some(300));
        assert_eq!(
            (
                level.clear_weather_time,
                level.rain_time,
                level.thunder_time
            ),
            (0, 300, 300)
        );
    }

    #[test]
    fn a_command_without_a_duration_draws_the_vanilla_range() {
        let mut level = LevelData::default();
        for seed in 0..50 {
            let mut rng = StdRng::seed_from_u64(seed);
            set_cycle(&mut level, &mut rng, Weather::Clear, None);
            assert!((12_000..=180_000).contains(&level.clear_weather_time));
            set_cycle(&mut level, &mut rng, Weather::Rain, None);
            assert!((12_000..=24_000).contains(&level.rain_time));
            set_cycle(&mut level, &mut rng, Weather::Thunder, None);
            assert!((3_600..=15_600).contains(&level.thunder_time));
        }
    }
}
