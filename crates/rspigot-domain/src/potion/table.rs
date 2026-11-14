use super::PotionEffect;

pub const POTIONS: &[(&str, &[PotionEffect])] = &[
    ("minecraft:water", &[]),
    ("minecraft:mundane", &[]),
    ("minecraft:thick", &[]),
    ("minecraft:awkward", &[]),
    (
        "minecraft:night_vision",
        &[PotionEffect {
            effect: "minecraft:night_vision",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:long_night_vision",
        &[PotionEffect {
            effect: "minecraft:night_vision",
            duration: 9600,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:invisibility",
        &[PotionEffect {
            effect: "minecraft:invisibility",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:long_invisibility",
        &[PotionEffect {
            effect: "minecraft:invisibility",
            duration: 9600,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:leaping",
        &[PotionEffect {
            effect: "minecraft:jump_boost",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:long_leaping",
        &[PotionEffect {
            effect: "minecraft:jump_boost",
            duration: 9600,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:strong_leaping",
        &[PotionEffect {
            effect: "minecraft:jump_boost",
            duration: 1800,
            amplifier: 1,
        }],
    ),
    (
        "minecraft:fire_resistance",
        &[PotionEffect {
            effect: "minecraft:fire_resistance",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:long_fire_resistance",
        &[PotionEffect {
            effect: "minecraft:fire_resistance",
            duration: 9600,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:swiftness",
        &[PotionEffect {
            effect: "minecraft:speed",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:long_swiftness",
        &[PotionEffect {
            effect: "minecraft:speed",
            duration: 9600,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:strong_swiftness",
        &[PotionEffect {
            effect: "minecraft:speed",
            duration: 1800,
            amplifier: 1,
        }],
    ),
    (
        "minecraft:slowness",
        &[PotionEffect {
            effect: "minecraft:slowness",
            duration: 1800,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:long_slowness",
        &[PotionEffect {
            effect: "minecraft:slowness",
            duration: 4800,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:strong_slowness",
        &[PotionEffect {
            effect: "minecraft:slowness",
            duration: 400,
            amplifier: 3,
        }],
    ),
    (
        "minecraft:turtle_master",
        &[
            PotionEffect {
                effect: "minecraft:slowness",
                duration: 400,
                amplifier: 3,
            },
            PotionEffect {
                effect: "minecraft:resistance",
                duration: 400,
                amplifier: 2,
            },
        ],
    ),
    (
        "minecraft:long_turtle_master",
        &[
            PotionEffect {
                effect: "minecraft:slowness",
                duration: 800,
                amplifier: 3,
            },
            PotionEffect {
                effect: "minecraft:resistance",
                duration: 800,
                amplifier: 2,
            },
        ],
    ),
    (
        "minecraft:strong_turtle_master",
        &[
            PotionEffect {
                effect: "minecraft:slowness",
                duration: 400,
                amplifier: 5,
            },
            PotionEffect {
                effect: "minecraft:resistance",
                duration: 400,
                amplifier: 3,
            },
        ],
    ),
    (
        "minecraft:water_breathing",
        &[PotionEffect {
            effect: "minecraft:water_breathing",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:long_water_breathing",
        &[PotionEffect {
            effect: "minecraft:water_breathing",
            duration: 9600,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:healing",
        &[PotionEffect {
            effect: "minecraft:instant_health",
            duration: 1,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:strong_healing",
        &[PotionEffect {
            effect: "minecraft:instant_health",
            duration: 1,
            amplifier: 1,
        }],
    ),
    (
        "minecraft:harming",
        &[PotionEffect {
            effect: "minecraft:instant_damage",
            duration: 1,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:strong_harming",
        &[PotionEffect {
            effect: "minecraft:instant_damage",
            duration: 1,
            amplifier: 1,
        }],
    ),
    (
        "minecraft:poison",
        &[PotionEffect {
            effect: "minecraft:poison",
            duration: 900,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:long_poison",
        &[PotionEffect {
            effect: "minecraft:poison",
            duration: 1800,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:strong_poison",
        &[PotionEffect {
            effect: "minecraft:poison",
            duration: 432,
            amplifier: 1,
        }],
    ),
    (
        "minecraft:regeneration",
        &[PotionEffect {
            effect: "minecraft:regeneration",
            duration: 900,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:long_regeneration",
        &[PotionEffect {
            effect: "minecraft:regeneration",
            duration: 1800,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:strong_regeneration",
        &[PotionEffect {
            effect: "minecraft:regeneration",
            duration: 450,
            amplifier: 1,
        }],
    ),
    (
        "minecraft:strength",
        &[PotionEffect {
            effect: "minecraft:strength",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:long_strength",
        &[PotionEffect {
            effect: "minecraft:strength",
            duration: 9600,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:strong_strength",
        &[PotionEffect {
            effect: "minecraft:strength",
            duration: 1800,
            amplifier: 1,
        }],
    ),
    (
        "minecraft:weakness",
        &[PotionEffect {
            effect: "minecraft:weakness",
            duration: 1800,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:long_weakness",
        &[PotionEffect {
            effect: "minecraft:weakness",
            duration: 4800,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:luck",
        &[PotionEffect {
            effect: "minecraft:luck",
            duration: 6000,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:slow_falling",
        &[PotionEffect {
            effect: "minecraft:slow_falling",
            duration: 1800,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:long_slow_falling",
        &[PotionEffect {
            effect: "minecraft:slow_falling",
            duration: 4800,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:wind_charged",
        &[PotionEffect {
            effect: "minecraft:wind_charged",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:weaving",
        &[PotionEffect {
            effect: "minecraft:weaving",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:oozing",
        &[PotionEffect {
            effect: "minecraft:oozing",
            duration: 3600,
            amplifier: 0,
        }],
    ),
    (
        "minecraft:infested",
        &[PotionEffect {
            effect: "minecraft:infested",
            duration: 3600,
            amplifier: 0,
        }],
    ),
];
