mod common;

use common::*;
use rspigot_core::plugins::events::{ThunderChange, WeatherChange};
use rspigot_core::plugins::{EventKind, EventPayload};

fn natural_scene(handlers: Vec<rspigot_core::plugins::HandlerSpec>) -> Scene {
    let server = configured_level(
        |_| {},
        |level| {
            level.rain_time = 2;
            level.thunder_time = 2;
        },
    );
    scene_on(server, &["Alice"], handlers)
}

#[test]
fn the_natural_cycle_fires_weather_then_thunder_change() {
    let mut s = natural_scene(vec![
        observe(1, EventKind::WeatherChange),
        observe(2, EventKind::ThunderChange),
    ]);
    s.tick(3);
    let kinds: Vec<(EventKind, bool)> = fired(&s.plugin)
        .into_iter()
        .filter_map(|event| match event.payload {
            EventPayload::WeatherChange(WeatherChange {
                to_weather_state, ..
            }) => Some((EventKind::WeatherChange, to_weather_state)),
            EventPayload::ThunderChange(ThunderChange {
                to_thunder_state, ..
            }) => Some((EventKind::ThunderChange, to_thunder_state)),
            _ => None,
        })
        .collect();
    assert_eq!(
        kinds,
        [
            (EventKind::WeatherChange, true),
            (EventKind::ThunderChange, true)
        ]
    );
}

#[test]
fn a_cancelled_natural_rain_keeps_the_sky_clear() {
    let mut s = natural_scene(vec![deciding(1, EventKind::WeatherChange)]);
    s.tick(3);
    let asked = last_asked(&s.plugin, EventKind::WeatherChange);
    judge(&mut s.server, 1, asked.id, 1, true, None);
    assert!(!s.server.level_snapshot().raining);
}
