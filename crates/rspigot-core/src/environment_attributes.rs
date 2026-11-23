use rspigot_data::environment::{self, Easing, Keyframe, Modifier, TimelineData, Track};
use rspigot_data::loot::AttributeValue;

const OVERWORLD: &str = "minecraft:overworld";
const BEZIER_STEPS: usize = 40;

const DEFAULTS: [(&str, AttributeValue); 17] = [
    (
        "minecraft:gameplay/bees_stay_in_hive",
        AttributeValue::Bool(false),
    ),
    (
        "minecraft:gameplay/can_pillager_patrol_spawn",
        AttributeValue::Bool(true),
    ),
    (
        "minecraft:gameplay/can_start_raid",
        AttributeValue::Bool(true),
    ),
    (
        "minecraft:gameplay/cat_waking_up_gift_chance",
        AttributeValue::Float(0.0),
    ),
    (
        "minecraft:gameplay/creaking_active",
        AttributeValue::Bool(false),
    ),
    (
        "minecraft:gameplay/eyeblossom_open",
        AttributeValue::Bool(false),
    ),
    ("minecraft:gameplay/fast_lava", AttributeValue::Bool(false)),
    (
        "minecraft:gameplay/increased_fire_burnout",
        AttributeValue::Bool(false),
    ),
    (
        "minecraft:gameplay/monsters_burn",
        AttributeValue::Bool(false),
    ),
    (
        "minecraft:gameplay/nether_portal_spawns_piglin",
        AttributeValue::Bool(false),
    ),
    (
        "minecraft:gameplay/piglins_zombify",
        AttributeValue::Bool(true),
    ),
    (
        "minecraft:gameplay/respawn_anchor_works",
        AttributeValue::Bool(false),
    ),
    (
        "minecraft:gameplay/sky_light_level",
        AttributeValue::Float(15.0),
    ),
    (
        "minecraft:gameplay/snow_golem_melts",
        AttributeValue::Bool(false),
    ),
    (
        "minecraft:gameplay/surface_slime_spawn_chance",
        AttributeValue::Float(0.0),
    ),
    (
        "minecraft:gameplay/turtle_egg_hatch_chance",
        AttributeValue::Float(0.0),
    ),
    (
        "minecraft:gameplay/water_evaporates",
        AttributeValue::Bool(false),
    ),
];

fn bezier_axis(first: f32, second: f32, t: f32) -> f32 {
    let inverse = 1.0 - t;
    3.0 * inverse * inverse * t * first + 3.0 * inverse * t * t * second + t * t * t
}

fn eased(ease: Easing, fraction: f32) -> f32 {
    match ease {
        Easing::Linear => fraction,
        Easing::Constant => 0.0,
        Easing::CubicBezier([x1, y1, x2, y2]) => {
            let (mut low, mut high) = (0.0_f32, 1.0_f32);
            for _ in 0..BEZIER_STEPS {
                let middle = (low + high) / 2.0;
                if bezier_axis(x1, x2, middle) < fraction {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            bezier_axis(y1, y2, (low + high) / 2.0)
        }
    }
}

fn interpolated(
    previous: &Keyframe,
    previous_at: i64,
    next: &Keyframe,
    next_at: i64,
    now: i64,
    ease: Easing,
) -> AttributeValue {
    match (previous.value, next.value) {
        (AttributeValue::Float(from), AttributeValue::Float(to)) if next_at > previous_at => {
            let fraction = (now - previous_at) as f32 / (next_at - previous_at) as f32;
            let weight = eased(ease, fraction.clamp(0.0, 1.0));
            AttributeValue::Float(from + (to - from) * weight)
        }
        _ => previous.value,
    }
}

fn track_value(track: &Track, period: Option<i64>, total: i64) -> Option<AttributeValue> {
    let keyframes = track.keyframes;
    let (first, last) = (keyframes.first()?, keyframes.last()?);
    let now = period.map_or(total, |period| total.rem_euclid(period.max(1)));
    let before = keyframes.iter().rposition(|frame| frame.ticks <= now);
    let (previous, previous_at, next, next_at) = match (before, period) {
        (Some(index), _) if index + 1 < keyframes.len() => (
            &keyframes[index],
            keyframes[index].ticks,
            &keyframes[index + 1],
            keyframes[index + 1].ticks,
        ),
        (Some(index), Some(period)) => (
            &keyframes[index],
            keyframes[index].ticks,
            first,
            first.ticks + period,
        ),
        (Some(index), None) => (
            &keyframes[index],
            keyframes[index].ticks,
            &keyframes[index],
            keyframes[index].ticks,
        ),
        (None, Some(period)) => (last, last.ticks - period, first, first.ticks),
        (None, None) => (first, first.ticks, first, first.ticks),
    };
    Some(interpolated(
        previous,
        previous_at,
        next,
        next_at,
        now,
        track.ease,
    ))
}

fn combined(
    modifier: Option<Modifier>,
    base: AttributeValue,
    found: AttributeValue,
) -> AttributeValue {
    match (modifier, base, found) {
        (Some(Modifier::Or), AttributeValue::Bool(base), AttributeValue::Bool(found)) => {
            AttributeValue::Bool(base || found)
        }
        (Some(Modifier::And), AttributeValue::Bool(base), AttributeValue::Bool(found)) => {
            AttributeValue::Bool(base && found)
        }
        (Some(Modifier::Maximum), AttributeValue::Float(base), AttributeValue::Float(found)) => {
            AttributeValue::Float(base.max(found))
        }
        (Some(Modifier::Minimum), AttributeValue::Float(base), AttributeValue::Float(found)) => {
            AttributeValue::Float(base.min(found))
        }
        (Some(Modifier::Multiply), AttributeValue::Float(base), AttributeValue::Float(found)) => {
            AttributeValue::Float(base * found)
        }
        (Some(Modifier::Add), AttributeValue::Float(base), AttributeValue::Float(found)) => {
            AttributeValue::Float(base + found)
        }
        (None, _, found) => found,
        (_, base, _) => base,
    }
}

fn ticks_of(clocks: &[(&str, i64)], timeline: &TimelineData) -> i64 {
    clocks
        .iter()
        .find(|(clock, _)| *clock == timeline.clock)
        .map_or(0, |(_, ticks)| *ticks)
}

pub fn value(attribute: &str, dimension: &str, clocks: &[(&str, i64)]) -> Option<AttributeValue> {
    let dimension_name = if dimension.is_empty() {
        OVERWORLD
    } else {
        dimension
    };
    let data = environment::dimension(dimension_name)?;
    let own = data
        .attributes
        .iter()
        .find(|(name, _)| *name == attribute)
        .map(|(_, value)| *value);
    let default = DEFAULTS
        .iter()
        .find(|(name, _)| *name == attribute)
        .map(|(_, value)| *value);
    let mut current = own.or(default);
    for id in data.timelines {
        let Some(timeline) = environment::timeline(id) else {
            continue;
        };
        let Some(track) = timeline
            .tracks
            .iter()
            .find(|track| track.attribute == attribute)
        else {
            continue;
        };
        let Some(found) = track_value(track, timeline.period_ticks, ticks_of(clocks, timeline))
        else {
            continue;
        };
        current = Some(match current {
            Some(base) => combined(track.modifier, base, found),
            None => found,
        });
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLOCK: &str = "minecraft:overworld";

    fn at(attribute: &str, dimension: &str, ticks: i64) -> Option<AttributeValue> {
        value(attribute, dimension, &[(CLOCK, ticks)])
    }

    #[test]
    fn pillager_patrols_may_spawn_once_the_early_game_is_over() {
        assert_eq!(
            at(
                "minecraft:gameplay/can_pillager_patrol_spawn",
                "minecraft:overworld",
                10_000_000
            ),
            Some(AttributeValue::Bool(true))
        );
    }

    #[test]
    fn monsters_burn_by_day_and_not_at_night() {
        let attribute = "minecraft:gameplay/monsters_burn";
        assert_eq!(
            at(attribute, "minecraft:overworld", 6000),
            Some(AttributeValue::Bool(true))
        );
        assert_eq!(
            at(attribute, "minecraft:overworld", 18_000),
            Some(AttributeValue::Bool(false))
        );
    }

    #[test]
    fn sky_light_dims_linearly_at_dusk() {
        let attribute = "minecraft:gameplay/sky_light_level";
        let Some(AttributeValue::Float(dusk)) = at(attribute, "minecraft:overworld", 12_768) else {
            panic!("no value")
        };
        assert!(dusk < 15.0 && dusk > 4.0, "{dusk}");
        assert_eq!(
            at(attribute, "minecraft:the_nether", 6000),
            Some(AttributeValue::Float(4.0))
        );
    }

    #[test]
    fn the_clock_wraps_by_the_timeline_period() {
        let attribute = "minecraft:gameplay/monsters_burn";
        assert_eq!(
            at(attribute, "minecraft:overworld", 24_000 + 18_000),
            Some(AttributeValue::Bool(false))
        );
    }

    #[test]
    fn unknown_attributes_have_no_value() {
        assert_eq!(
            at("minecraft:gameplay/nope", "minecraft:overworld", 0),
            None
        );
    }
}
