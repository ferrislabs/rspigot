pub const OVERWORLD: &str = "minecraft:overworld";
pub const THE_END: &str = "minecraft:the_end";
pub const CLOCKS: [&str; 2] = [OVERWORLD, THE_END];

pub const MIN_RATE: f32 = 1.0e-5;
pub const MAX_RATE: f32 = 1000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClockRuntime {
    pub partial_tick: f32,
    pub rate: f32,
    pub paused: bool,
}

impl Default for ClockRuntime {
    fn default() -> Self {
        Self {
            partial_tick: 0.0,
            rate: 1.0,
            paused: false,
        }
    }
}

impl ClockRuntime {
    pub fn network_rate(&self) -> f32 {
        if self.paused { 0.0 } else { self.rate }
    }

    pub fn advance(&mut self) -> i64 {
        if self.paused {
            return 0;
        }
        self.partial_tick += self.rate;
        let whole = self.partial_tick.floor();
        self.partial_tick -= whole;
        whole as i64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeMarker {
    pub id: &'static str,
    pub ticks: i64,
    pub show_in_commands: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeline {
    pub id: &'static str,
    pub clock: &'static str,
    pub period_ticks: Option<i64>,
    pub markers: &'static [TimeMarker],
}

const fn marker(id: &'static str, ticks: i64, show_in_commands: bool) -> TimeMarker {
    TimeMarker {
        id,
        ticks,
        show_in_commands,
    }
}

pub const TIMELINES: [Timeline; 4] = [
    Timeline {
        id: "minecraft:day",
        clock: OVERWORLD,
        period_ticks: Some(24_000),
        markers: &[
            marker("minecraft:day", 1000, true),
            marker("minecraft:midnight", 18_000, true),
            marker("minecraft:night", 13_000, true),
            marker("minecraft:noon", 6000, true),
            marker("minecraft:roll_village_siege", 18_000, false),
            marker("minecraft:wake_up_from_sleep", 0, false),
        ],
    },
    Timeline {
        id: "minecraft:early_game",
        clock: OVERWORLD,
        period_ticks: None,
        markers: &[],
    },
    Timeline {
        id: "minecraft:moon",
        clock: OVERWORLD,
        period_ticks: Some(192_000),
        markers: &[],
    },
    Timeline {
        id: "minecraft:villager_schedule",
        clock: OVERWORLD,
        period_ticks: Some(24_000),
        markers: &[],
    },
];

pub fn normalize(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

pub fn clock_known(id: &str) -> bool {
    CLOCKS.contains(&normalize(id).as_str())
}

pub fn timeline(id: &str) -> Option<&'static Timeline> {
    let id = normalize(id);
    TIMELINES.iter().find(|timeline| timeline.id == id)
}

pub fn marker_of(clock: &str, marker_id: &str) -> Option<(&'static Timeline, &'static TimeMarker)> {
    let marker_id = normalize(marker_id);
    TIMELINES
        .iter()
        .filter(|timeline| timeline.clock == clock)
        .find_map(|timeline| {
            timeline
                .markers
                .iter()
                .find(|marker| marker.id == marker_id)
                .map(|marker| (timeline, marker))
        })
}

pub fn shown_markers() -> impl Iterator<Item = &'static str> {
    TIMELINES
        .iter()
        .flat_map(|timeline| timeline.markers)
        .filter(|marker| marker.show_in_commands)
        .map(|marker| marker.id)
}

impl Timeline {
    pub fn current_ticks(&self, total: i64) -> i64 {
        match self.period_ticks {
            Some(period) => total.rem_euclid(period),
            None => total,
        }
    }

    pub fn period_count(&self, total: i64) -> i64 {
        match self.period_ticks {
            Some(period) => total.div_euclid(period),
            None => 0,
        }
    }

    pub fn ticks_until(&self, total: i64, marker_ticks: i64) -> i64 {
        match self.period_ticks {
            Some(period) => (marker_ticks - total).rem_euclid(period),
            None => marker_ticks - total,
        }
    }
}

pub fn wrap(ticks: i64) -> i32 {
    (ticks % i64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_running_clock_advances_by_its_rate_with_a_fractional_carry() {
        let mut clock = ClockRuntime::default();
        assert_eq!(clock.advance(), 1);
        clock.rate = 0.5;
        assert_eq!(
            [clock.advance(), clock.advance(), clock.advance()],
            [0, 1, 0]
        );
        clock.rate = 3.0;
        assert_eq!(clock.advance(), 3);
        clock.paused = true;
        assert_eq!(clock.advance(), 0);
    }

    #[test]
    fn markers_resolve_in_their_timeline_and_move_forward() {
        let (day, noon) = marker_of(OVERWORLD, "noon").unwrap();
        assert_eq!(noon.ticks, 6000);
        assert_eq!(day.ticks_until(1000, noon.ticks), 5000);
        assert_eq!(day.ticks_until(7000, noon.ticks), 23_000);
        assert_eq!(day.ticks_until(6000, noon.ticks), 0);
        assert_eq!(day.current_ticks(30_000), 6000);
        assert_eq!(day.period_count(30_000), 1);
        assert!(marker_of(THE_END, "noon").is_none());
        assert!(timeline("moon").is_some());
        assert_eq!(shown_markers().count(), 4);
    }
}
