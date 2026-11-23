use crate::environment::{DimensionData, Easing, Keyframe, Modifier, TimelineData, Track};
use crate::loot::AttributeValue;

pub(crate) static TIMELINES: [TimelineData; 4] = [
    TimelineData {
        id: "minecraft:day",
        clock: "minecraft:overworld",
        period_ticks: Some(24000),
        tracks: &[
            Track {
                attribute: "minecraft:audio/firefly_bush_sounds",
                ease: Easing::Linear,
                modifier: Some(Modifier::Or),
                keyframes: &[
                    Keyframe { ticks: 12600, value: AttributeValue::Bool(true) },
                    Keyframe { ticks: 23401, value: AttributeValue::Bool(false) },
                ],
            },
            Track {
                attribute: "minecraft:gameplay/bees_stay_in_hive",
                ease: Easing::Linear,
                modifier: Some(Modifier::Or),
                keyframes: &[
                    Keyframe { ticks: 12542, value: AttributeValue::Bool(true) },
                    Keyframe { ticks: 23460, value: AttributeValue::Bool(false) },
                ],
            },
            Track {
                attribute: "minecraft:gameplay/cat_waking_up_gift_chance",
                ease: Easing::Constant,
                modifier: Some(Modifier::Maximum),
                keyframes: &[
                    Keyframe { ticks: 362, value: AttributeValue::Float(0.0) },
                    Keyframe { ticks: 23667, value: AttributeValue::Float(0.7) },
                ],
            },
            Track {
                attribute: "minecraft:gameplay/creaking_active",
                ease: Easing::Linear,
                modifier: Some(Modifier::Or),
                keyframes: &[
                    Keyframe { ticks: 12600, value: AttributeValue::Bool(true) },
                    Keyframe { ticks: 23401, value: AttributeValue::Bool(false) },
                ],
            },
            Track {
                attribute: "minecraft:gameplay/eyeblossom_open",
                ease: Easing::Linear,
                modifier: None,
                keyframes: &[
                    Keyframe { ticks: 12600, value: AttributeValue::Bool(true) },
                    Keyframe { ticks: 23401, value: AttributeValue::Bool(false) },
                ],
            },
            Track {
                attribute: "minecraft:gameplay/monsters_burn",
                ease: Easing::Linear,
                modifier: Some(Modifier::Or),
                keyframes: &[
                    Keyframe { ticks: 12542, value: AttributeValue::Bool(false) },
                    Keyframe { ticks: 23460, value: AttributeValue::Bool(true) },
                ],
            },
            Track {
                attribute: "minecraft:gameplay/sky_light_level",
                ease: Easing::Linear,
                modifier: Some(Modifier::Multiply),
                keyframes: &[
                    Keyframe { ticks: 133, value: AttributeValue::Float(1.0) },
                    Keyframe { ticks: 11867, value: AttributeValue::Float(1.0) },
                    Keyframe { ticks: 13670, value: AttributeValue::Float(0.26666668) },
                    Keyframe { ticks: 22330, value: AttributeValue::Float(0.26666668) },
                ],
            },
            Track {
                attribute: "minecraft:gameplay/turtle_egg_hatch_chance",
                ease: Easing::Constant,
                modifier: Some(Modifier::Maximum),
                keyframes: &[
                    Keyframe { ticks: 21062, value: AttributeValue::Float(1.0) },
                    Keyframe { ticks: 21905, value: AttributeValue::Float(0.002) },
                ],
            },
            Track {
                attribute: "minecraft:visual/moon_angle",
                ease: Easing::CubicBezier([0.362, 0.241, 0.638, 0.759]),
                modifier: None,
                keyframes: &[
                    Keyframe { ticks: 6000, value: AttributeValue::Float(5.4e2) },
                    Keyframe { ticks: 6000, value: AttributeValue::Float(1.8e2) },
                ],
            },
            Track {
                attribute: "minecraft:visual/sky_light_factor",
                ease: Easing::Linear,
                modifier: Some(Modifier::Multiply),
                keyframes: &[
                    Keyframe { ticks: 730, value: AttributeValue::Float(1.0) },
                    Keyframe { ticks: 11270, value: AttributeValue::Float(1.0) },
                    Keyframe { ticks: 13140, value: AttributeValue::Float(0.24) },
                    Keyframe { ticks: 22860, value: AttributeValue::Float(0.24) },
                ],
            },
            Track {
                attribute: "minecraft:visual/star_angle",
                ease: Easing::CubicBezier([0.362, 0.241, 0.638, 0.759]),
                modifier: None,
                keyframes: &[
                    Keyframe { ticks: 6000, value: AttributeValue::Float(3.6e2) },
                    Keyframe { ticks: 6000, value: AttributeValue::Float(0.0) },
                ],
            },
            Track {
                attribute: "minecraft:visual/star_brightness",
                ease: Easing::Linear,
                modifier: Some(Modifier::Maximum),
                keyframes: &[
                    Keyframe { ticks: 92, value: AttributeValue::Float(0.037) },
                    Keyframe { ticks: 627, value: AttributeValue::Float(0.0) },
                    Keyframe { ticks: 11373, value: AttributeValue::Float(0.0) },
                    Keyframe { ticks: 11732, value: AttributeValue::Float(0.016) },
                    Keyframe { ticks: 11959, value: AttributeValue::Float(0.044) },
                    Keyframe { ticks: 12399, value: AttributeValue::Float(0.143) },
                    Keyframe { ticks: 12729, value: AttributeValue::Float(0.258) },
                    Keyframe { ticks: 13228, value: AttributeValue::Float(0.5) },
                    Keyframe { ticks: 22772, value: AttributeValue::Float(0.5) },
                    Keyframe { ticks: 23032, value: AttributeValue::Float(0.364) },
                    Keyframe { ticks: 23356, value: AttributeValue::Float(0.225) },
                    Keyframe { ticks: 23758, value: AttributeValue::Float(0.101) },
                ],
            },
            Track {
                attribute: "minecraft:visual/sun_angle",
                ease: Easing::CubicBezier([0.362, 0.241, 0.638, 0.759]),
                modifier: None,
                keyframes: &[
                    Keyframe { ticks: 6000, value: AttributeValue::Float(3.6e2) },
                    Keyframe { ticks: 6000, value: AttributeValue::Float(0.0) },
                ],
            },
        ],
    },
    TimelineData {
        id: "minecraft:early_game",
        clock: "minecraft:overworld",
        period_ticks: None,
        tracks: &[Track {
            attribute: "minecraft:gameplay/can_pillager_patrol_spawn",
            ease: Easing::Linear,
            modifier: Some(Modifier::And),
            keyframes: &[
                Keyframe { ticks: 0, value: AttributeValue::Bool(false) },
                Keyframe { ticks: 120000, value: AttributeValue::Bool(true) },
            ],
        }],
    },
    TimelineData {
        id: "minecraft:moon",
        clock: "minecraft:overworld",
        period_ticks: Some(192000),
        tracks: &[
            Track {
                attribute: "minecraft:gameplay/surface_slime_spawn_chance",
                ease: Easing::Constant,
                modifier: Some(Modifier::Maximum),
                keyframes: &[
                    Keyframe { ticks: 0, value: AttributeValue::Float(0.5) },
                    Keyframe { ticks: 24000, value: AttributeValue::Float(0.375) },
                    Keyframe { ticks: 48000, value: AttributeValue::Float(0.25) },
                    Keyframe { ticks: 72000, value: AttributeValue::Float(0.125) },
                    Keyframe { ticks: 96000, value: AttributeValue::Float(0.0) },
                    Keyframe { ticks: 120000, value: AttributeValue::Float(0.125) },
                    Keyframe { ticks: 144000, value: AttributeValue::Float(0.25) },
                    Keyframe { ticks: 168000, value: AttributeValue::Float(0.375) },
                ],
            },
            Track {
                attribute: "minecraft:visual/moon_phase",
                ease: Easing::Linear,
                modifier: None,
                keyframes: &[
                    Keyframe { ticks: 0, value: AttributeValue::Text("full_moon") },
                    Keyframe { ticks: 24000, value: AttributeValue::Text("waning_gibbous") },
                    Keyframe { ticks: 48000, value: AttributeValue::Text("third_quarter") },
                    Keyframe { ticks: 72000, value: AttributeValue::Text("waning_crescent") },
                    Keyframe { ticks: 96000, value: AttributeValue::Text("new_moon") },
                    Keyframe { ticks: 120000, value: AttributeValue::Text("waxing_crescent") },
                    Keyframe { ticks: 144000, value: AttributeValue::Text("first_quarter") },
                    Keyframe { ticks: 168000, value: AttributeValue::Text("waxing_gibbous") },
                ],
            },
        ],
    },
    TimelineData {
        id: "minecraft:villager_schedule",
        clock: "minecraft:overworld",
        period_ticks: Some(24000),
        tracks: &[
            Track {
                attribute: "minecraft:gameplay/baby_villager_activity",
                ease: Easing::Linear,
                modifier: None,
                keyframes: &[
                    Keyframe { ticks: 10, value: AttributeValue::Text("minecraft:idle") },
                    Keyframe { ticks: 3000, value: AttributeValue::Text("minecraft:play") },
                    Keyframe { ticks: 6000, value: AttributeValue::Text("minecraft:idle") },
                    Keyframe { ticks: 10000, value: AttributeValue::Text("minecraft:play") },
                    Keyframe { ticks: 12000, value: AttributeValue::Text("minecraft:rest") },
                ],
            },
            Track {
                attribute: "minecraft:gameplay/villager_activity",
                ease: Easing::Linear,
                modifier: None,
                keyframes: &[
                    Keyframe { ticks: 10, value: AttributeValue::Text("minecraft:idle") },
                    Keyframe { ticks: 2000, value: AttributeValue::Text("minecraft:work") },
                    Keyframe { ticks: 9000, value: AttributeValue::Text("minecraft:meet") },
                    Keyframe { ticks: 11000, value: AttributeValue::Text("minecraft:idle") },
                    Keyframe { ticks: 12000, value: AttributeValue::Text("minecraft:rest") },
                ],
            },
        ],
    },
];

pub(crate) static DIMENSIONS: [DimensionData; 4] = [
    DimensionData {
        id: "minecraft:overworld",
        timelines: &[
            "minecraft:day",
            "minecraft:early_game",
            "minecraft:moon",
            "minecraft:villager_schedule",
        ],
        attributes: &[
            ("minecraft:gameplay/nether_portal_spawns_piglin", AttributeValue::Bool(true)),
            ("minecraft:gameplay/respawn_anchor_works", AttributeValue::Bool(false)),
            ("minecraft:visual/cloud_height", AttributeValue::Float(192.33)),
        ],
    },
    DimensionData {
        id: "minecraft:overworld_caves",
        timelines: &[
            "minecraft:day",
            "minecraft:early_game",
            "minecraft:moon",
            "minecraft:villager_schedule",
        ],
        attributes: &[
            ("minecraft:gameplay/nether_portal_spawns_piglin", AttributeValue::Bool(true)),
            ("minecraft:gameplay/respawn_anchor_works", AttributeValue::Bool(false)),
            ("minecraft:visual/cloud_height", AttributeValue::Float(192.33)),
        ],
    },
    DimensionData {
        id: "minecraft:the_end",
        timelines: &["minecraft:villager_schedule"],
        attributes: &[
            ("minecraft:gameplay/respawn_anchor_works", AttributeValue::Bool(false)),
            ("minecraft:visual/sky_light_factor", AttributeValue::Float(0.0)),
        ],
    },
    DimensionData {
        id: "minecraft:the_nether",
        timelines: &["minecraft:villager_schedule"],
        attributes: &[
            ("minecraft:gameplay/can_start_raid", AttributeValue::Bool(false)),
            ("minecraft:gameplay/fast_lava", AttributeValue::Bool(true)),
            ("minecraft:gameplay/piglins_zombify", AttributeValue::Bool(false)),
            ("minecraft:gameplay/respawn_anchor_works", AttributeValue::Bool(true)),
            ("minecraft:gameplay/sky_light_level", AttributeValue::Float(4.0)),
            ("minecraft:gameplay/snow_golem_melts", AttributeValue::Bool(true)),
            ("minecraft:gameplay/water_evaporates", AttributeValue::Bool(true)),
            ("minecraft:visual/fog_end_distance", AttributeValue::Float(96.0)),
            ("minecraft:visual/fog_start_distance", AttributeValue::Float(1e1)),
            ("minecraft:visual/sky_light_factor", AttributeValue::Float(0.0)),
        ],
    },
];
