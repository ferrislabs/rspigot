use crate::tags::HolderSet;
use crate::variants::{MobVariant, SoundSet, SoundVariant, SpawnCondition, VariantCondition};

pub(crate) static COW_VARIANTS: [MobVariant; 3] = [
    MobVariant {
        name: "minecraft:cold",
        spawn_conditions: &[
            SpawnCondition {
                condition: Some(VariantCondition::Biome(HolderSet::Tag("minecraft:spawns_cold_variant_farm_animals"))),
                priority: 1,
            },
        ],
    },
    MobVariant {
        name: "minecraft:temperate",
        spawn_conditions: &[
            SpawnCondition {
                condition: None,
                priority: 0,
            },
        ],
    },
    MobVariant {
        name: "minecraft:warm",
        spawn_conditions: &[
            SpawnCondition {
                condition: Some(VariantCondition::Biome(HolderSet::Tag("minecraft:spawns_warm_variant_farm_animals"))),
                priority: 1,
            },
        ],
    },
];

pub(crate) static PIG_VARIANTS: [MobVariant; 3] = [
    MobVariant {
        name: "minecraft:cold",
        spawn_conditions: &[
            SpawnCondition {
                condition: Some(VariantCondition::Biome(HolderSet::Tag("minecraft:spawns_cold_variant_farm_animals"))),
                priority: 1,
            },
        ],
    },
    MobVariant {
        name: "minecraft:temperate",
        spawn_conditions: &[
            SpawnCondition {
                condition: None,
                priority: 0,
            },
        ],
    },
    MobVariant {
        name: "minecraft:warm",
        spawn_conditions: &[
            SpawnCondition {
                condition: Some(VariantCondition::Biome(HolderSet::Tag("minecraft:spawns_warm_variant_farm_animals"))),
                priority: 1,
            },
        ],
    },
];

pub(crate) static CHICKEN_VARIANTS: [MobVariant; 3] = [
    MobVariant {
        name: "minecraft:cold",
        spawn_conditions: &[
            SpawnCondition {
                condition: Some(VariantCondition::Biome(HolderSet::Tag("minecraft:spawns_cold_variant_farm_animals"))),
                priority: 1,
            },
        ],
    },
    MobVariant {
        name: "minecraft:temperate",
        spawn_conditions: &[
            SpawnCondition {
                condition: None,
                priority: 0,
            },
        ],
    },
    MobVariant {
        name: "minecraft:warm",
        spawn_conditions: &[
            SpawnCondition {
                condition: Some(VariantCondition::Biome(HolderSet::Tag("minecraft:spawns_warm_variant_farm_animals"))),
                priority: 1,
            },
        ],
    },
];

pub(crate) static WOLF_VARIANTS: [MobVariant; 9] = [
    MobVariant {
        name: "minecraft:ashen",
        spawn_conditions: &[
            SpawnCondition {
                condition: Some(VariantCondition::Biome(HolderSet::Direct(&[49]))),
                priority: 1,
            },
        ],
    },
    MobVariant {
        name: "minecraft:black",
        spawn_conditions: &[
            SpawnCondition {
                condition: Some(VariantCondition::Biome(HolderSet::Direct(&[38]))),
                priority: 1,
            },
        ],
    },
    MobVariant {
        name: "minecraft:chestnut",
        spawn_conditions: &[
            SpawnCondition {
                condition: Some(VariantCondition::Biome(HolderSet::Direct(&[39]))),
                priority: 1,
            },
        ],
    },
    MobVariant {
        name: "minecraft:pale",
        spawn_conditions: &[
            SpawnCondition {
                condition: None,
                priority: 0,
            },
        ],
    },
    MobVariant {
        name: "minecraft:rusty",
        spawn_conditions: &[
            SpawnCondition {
                condition: Some(VariantCondition::Biome(HolderSet::Tag("minecraft:is_jungle"))),
                priority: 1,
            },
        ],
    },
    MobVariant {
        name: "minecraft:snowy",
        spawn_conditions: &[
            SpawnCondition {
                condition: Some(VariantCondition::Biome(HolderSet::Direct(&[26]))),
                priority: 1,
            },
        ],
    },
    MobVariant {
        name: "minecraft:spotted",
        spawn_conditions: &[
            SpawnCondition {
                condition: Some(VariantCondition::Biome(HolderSet::Tag("minecraft:is_savanna"))),
                priority: 1,
            },
        ],
    },
    MobVariant {
        name: "minecraft:striped",
        spawn_conditions: &[
            SpawnCondition {
                condition: Some(VariantCondition::Biome(HolderSet::Tag("minecraft:is_badlands"))),
                priority: 1,
            },
        ],
    },
    MobVariant {
        name: "minecraft:woods",
        spawn_conditions: &[
            SpawnCondition {
                condition: Some(VariantCondition::Biome(HolderSet::Direct(&[22]))),
                priority: 1,
            },
        ],
    },
];

pub(crate) static CAT_VARIANTS: [MobVariant; 11] = [
    MobVariant {
        name: "minecraft:all_black",
        spawn_conditions: &[
            SpawnCondition {
                condition: Some(VariantCondition::Structure("minecraft:cats_spawn_as_black")),
                priority: 1,
            },
            SpawnCondition {
                condition: Some(VariantCondition::MoonBrightness { min: Some(0.9), max: None }),
                priority: 0,
            },
        ],
    },
    MobVariant {
        name: "minecraft:black",
        spawn_conditions: &[
            SpawnCondition {
                condition: None,
                priority: 0,
            },
        ],
    },
    MobVariant {
        name: "minecraft:british_shorthair",
        spawn_conditions: &[
            SpawnCondition {
                condition: None,
                priority: 0,
            },
        ],
    },
    MobVariant {
        name: "minecraft:calico",
        spawn_conditions: &[
            SpawnCondition {
                condition: None,
                priority: 0,
            },
        ],
    },
    MobVariant {
        name: "minecraft:jellie",
        spawn_conditions: &[
            SpawnCondition {
                condition: None,
                priority: 0,
            },
        ],
    },
    MobVariant {
        name: "minecraft:persian",
        spawn_conditions: &[
            SpawnCondition {
                condition: None,
                priority: 0,
            },
        ],
    },
    MobVariant {
        name: "minecraft:ragdoll",
        spawn_conditions: &[
            SpawnCondition {
                condition: None,
                priority: 0,
            },
        ],
    },
    MobVariant {
        name: "minecraft:red",
        spawn_conditions: &[
            SpawnCondition {
                condition: None,
                priority: 0,
            },
        ],
    },
    MobVariant {
        name: "minecraft:siamese",
        spawn_conditions: &[
            SpawnCondition {
                condition: None,
                priority: 0,
            },
        ],
    },
    MobVariant {
        name: "minecraft:tabby",
        spawn_conditions: &[
            SpawnCondition {
                condition: None,
                priority: 0,
            },
        ],
    },
    MobVariant {
        name: "minecraft:white",
        spawn_conditions: &[
            SpawnCondition {
                condition: None,
                priority: 0,
            },
        ],
    },
];

pub(crate) static COW_SOUND_VARIANTS: [SoundVariant; 2] = [
    SoundVariant {
        name: "minecraft:classic",
        adult: SoundSet {
            ambient: Some("minecraft:entity.cow.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.cow.death"),
            eat: None,
            growl: None,
            hiss: None,
            hurt: Some("minecraft:entity.cow.hurt"),
            pant: None,
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.cow.step"),
            stray_ambient: None,
            whine: None,
        },
        baby: SoundSet {
            ambient: Some("minecraft:entity.cow.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.cow.death"),
            eat: None,
            growl: None,
            hiss: None,
            hurt: Some("minecraft:entity.cow.hurt"),
            pant: None,
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.cow.step"),
            stray_ambient: None,
            whine: None,
        },
    },
    SoundVariant {
        name: "minecraft:moody",
        adult: SoundSet {
            ambient: Some("minecraft:entity.cow_moody.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.cow_moody.death"),
            eat: None,
            growl: None,
            hiss: None,
            hurt: Some("minecraft:entity.cow_moody.hurt"),
            pant: None,
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.cow_moody.step"),
            stray_ambient: None,
            whine: None,
        },
        baby: SoundSet {
            ambient: Some("minecraft:entity.cow_moody.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.cow_moody.death"),
            eat: None,
            growl: None,
            hiss: None,
            hurt: Some("minecraft:entity.cow_moody.hurt"),
            pant: None,
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.cow_moody.step"),
            stray_ambient: None,
            whine: None,
        },
    },
];

pub(crate) static PIG_SOUND_VARIANTS: [SoundVariant; 3] = [
    SoundVariant {
        name: "minecraft:big",
        adult: SoundSet {
            ambient: Some("minecraft:entity.pig_big.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.pig_big.death"),
            eat: Some("minecraft:entity.pig_big.eat"),
            growl: None,
            hiss: None,
            hurt: Some("minecraft:entity.pig_big.hurt"),
            pant: None,
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.pig.step"),
            stray_ambient: None,
            whine: None,
        },
        baby: SoundSet {
            ambient: Some("minecraft:entity.baby_pig.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.baby_pig.death"),
            eat: Some("minecraft:entity.baby_pig.eat"),
            growl: None,
            hiss: None,
            hurt: Some("minecraft:entity.baby_pig.hurt"),
            pant: None,
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.baby_pig.step"),
            stray_ambient: None,
            whine: None,
        },
    },
    SoundVariant {
        name: "minecraft:classic",
        adult: SoundSet {
            ambient: Some("minecraft:entity.pig.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.pig.death"),
            eat: Some("minecraft:entity.pig.eat"),
            growl: None,
            hiss: None,
            hurt: Some("minecraft:entity.pig.hurt"),
            pant: None,
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.pig.step"),
            stray_ambient: None,
            whine: None,
        },
        baby: SoundSet {
            ambient: Some("minecraft:entity.baby_pig.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.baby_pig.death"),
            eat: Some("minecraft:entity.baby_pig.eat"),
            growl: None,
            hiss: None,
            hurt: Some("minecraft:entity.baby_pig.hurt"),
            pant: None,
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.baby_pig.step"),
            stray_ambient: None,
            whine: None,
        },
    },
    SoundVariant {
        name: "minecraft:mini",
        adult: SoundSet {
            ambient: Some("minecraft:entity.pig_mini.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.pig_mini.death"),
            eat: Some("minecraft:entity.pig_mini.eat"),
            growl: None,
            hiss: None,
            hurt: Some("minecraft:entity.pig_mini.hurt"),
            pant: None,
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.pig.step"),
            stray_ambient: None,
            whine: None,
        },
        baby: SoundSet {
            ambient: Some("minecraft:entity.baby_pig.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.baby_pig.death"),
            eat: Some("minecraft:entity.baby_pig.eat"),
            growl: None,
            hiss: None,
            hurt: Some("minecraft:entity.baby_pig.hurt"),
            pant: None,
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.baby_pig.step"),
            stray_ambient: None,
            whine: None,
        },
    },
];

pub(crate) static CHICKEN_SOUND_VARIANTS: [SoundVariant; 2] = [
    SoundVariant {
        name: "minecraft:classic",
        adult: SoundSet {
            ambient: Some("minecraft:entity.chicken.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.chicken.death"),
            eat: None,
            growl: None,
            hiss: None,
            hurt: Some("minecraft:entity.chicken.hurt"),
            pant: None,
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.chicken.step"),
            stray_ambient: None,
            whine: None,
        },
        baby: SoundSet {
            ambient: Some("minecraft:entity.baby_chicken.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.baby_chicken.death"),
            eat: None,
            growl: None,
            hiss: None,
            hurt: Some("minecraft:entity.baby_chicken.hurt"),
            pant: None,
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.baby_chicken.step"),
            stray_ambient: None,
            whine: None,
        },
    },
    SoundVariant {
        name: "minecraft:picky",
        adult: SoundSet {
            ambient: Some("minecraft:entity.chicken_picky.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.chicken_picky.death"),
            eat: None,
            growl: None,
            hiss: None,
            hurt: Some("minecraft:entity.chicken_picky.hurt"),
            pant: None,
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.chicken.step"),
            stray_ambient: None,
            whine: None,
        },
        baby: SoundSet {
            ambient: Some("minecraft:entity.baby_chicken.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.baby_chicken.death"),
            eat: None,
            growl: None,
            hiss: None,
            hurt: Some("minecraft:entity.baby_chicken.hurt"),
            pant: None,
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.baby_chicken.step"),
            stray_ambient: None,
            whine: None,
        },
    },
];

pub(crate) static WOLF_SOUND_VARIANTS: [SoundVariant; 7] = [
    SoundVariant {
        name: "minecraft:angry",
        adult: SoundSet {
            ambient: Some("minecraft:entity.wolf_angry.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.wolf_angry.death"),
            eat: None,
            growl: Some("minecraft:entity.wolf_angry.growl"),
            hiss: None,
            hurt: Some("minecraft:entity.wolf_angry.hurt"),
            pant: Some("minecraft:entity.wolf_angry.pant"),
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.wolf.step"),
            stray_ambient: None,
            whine: Some("minecraft:entity.wolf_angry.whine"),
        },
        baby: SoundSet {
            ambient: Some("minecraft:entity.baby_wolf.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.baby_wolf.death"),
            eat: None,
            growl: Some("minecraft:entity.baby_wolf.growl"),
            hiss: None,
            hurt: Some("minecraft:entity.baby_wolf.hurt"),
            pant: Some("minecraft:entity.baby_wolf.pant"),
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.baby_wolf.step"),
            stray_ambient: None,
            whine: Some("minecraft:entity.baby_wolf.whine"),
        },
    },
    SoundVariant {
        name: "minecraft:big",
        adult: SoundSet {
            ambient: Some("minecraft:entity.wolf_big.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.wolf_big.death"),
            eat: None,
            growl: Some("minecraft:entity.wolf_big.growl"),
            hiss: None,
            hurt: Some("minecraft:entity.wolf_big.hurt"),
            pant: Some("minecraft:entity.wolf_big.pant"),
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.wolf.step"),
            stray_ambient: None,
            whine: Some("minecraft:entity.wolf_big.whine"),
        },
        baby: SoundSet {
            ambient: Some("minecraft:entity.baby_wolf.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.baby_wolf.death"),
            eat: None,
            growl: Some("minecraft:entity.baby_wolf.growl"),
            hiss: None,
            hurt: Some("minecraft:entity.baby_wolf.hurt"),
            pant: Some("minecraft:entity.baby_wolf.pant"),
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.baby_wolf.step"),
            stray_ambient: None,
            whine: Some("minecraft:entity.baby_wolf.whine"),
        },
    },
    SoundVariant {
        name: "minecraft:classic",
        adult: SoundSet {
            ambient: Some("minecraft:entity.wolf.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.wolf.death"),
            eat: None,
            growl: Some("minecraft:entity.wolf.growl"),
            hiss: None,
            hurt: Some("minecraft:entity.wolf.hurt"),
            pant: Some("minecraft:entity.wolf.pant"),
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.wolf.step"),
            stray_ambient: None,
            whine: Some("minecraft:entity.wolf.whine"),
        },
        baby: SoundSet {
            ambient: Some("minecraft:entity.baby_wolf.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.baby_wolf.death"),
            eat: None,
            growl: Some("minecraft:entity.baby_wolf.growl"),
            hiss: None,
            hurt: Some("minecraft:entity.baby_wolf.hurt"),
            pant: Some("minecraft:entity.baby_wolf.pant"),
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.baby_wolf.step"),
            stray_ambient: None,
            whine: Some("minecraft:entity.baby_wolf.whine"),
        },
    },
    SoundVariant {
        name: "minecraft:cute",
        adult: SoundSet {
            ambient: Some("minecraft:entity.wolf_cute.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.wolf_cute.death"),
            eat: None,
            growl: Some("minecraft:entity.wolf_cute.growl"),
            hiss: None,
            hurt: Some("minecraft:entity.wolf_cute.hurt"),
            pant: Some("minecraft:entity.wolf_cute.pant"),
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.wolf.step"),
            stray_ambient: None,
            whine: Some("minecraft:entity.wolf_cute.whine"),
        },
        baby: SoundSet {
            ambient: Some("minecraft:entity.baby_wolf.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.baby_wolf.death"),
            eat: None,
            growl: Some("minecraft:entity.baby_wolf.growl"),
            hiss: None,
            hurt: Some("minecraft:entity.baby_wolf.hurt"),
            pant: Some("minecraft:entity.baby_wolf.pant"),
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.baby_wolf.step"),
            stray_ambient: None,
            whine: Some("minecraft:entity.baby_wolf.whine"),
        },
    },
    SoundVariant {
        name: "minecraft:grumpy",
        adult: SoundSet {
            ambient: Some("minecraft:entity.wolf_grumpy.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.wolf_grumpy.death"),
            eat: None,
            growl: Some("minecraft:entity.wolf_grumpy.growl"),
            hiss: None,
            hurt: Some("minecraft:entity.wolf_grumpy.hurt"),
            pant: Some("minecraft:entity.wolf_grumpy.pant"),
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.wolf.step"),
            stray_ambient: None,
            whine: Some("minecraft:entity.wolf_grumpy.whine"),
        },
        baby: SoundSet {
            ambient: Some("minecraft:entity.baby_wolf.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.baby_wolf.death"),
            eat: None,
            growl: Some("minecraft:entity.baby_wolf.growl"),
            hiss: None,
            hurt: Some("minecraft:entity.baby_wolf.hurt"),
            pant: Some("minecraft:entity.baby_wolf.pant"),
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.baby_wolf.step"),
            stray_ambient: None,
            whine: Some("minecraft:entity.baby_wolf.whine"),
        },
    },
    SoundVariant {
        name: "minecraft:puglin",
        adult: SoundSet {
            ambient: Some("minecraft:entity.wolf_puglin.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.wolf_puglin.death"),
            eat: None,
            growl: Some("minecraft:entity.wolf_puglin.growl"),
            hiss: None,
            hurt: Some("minecraft:entity.wolf_puglin.hurt"),
            pant: Some("minecraft:entity.wolf_puglin.pant"),
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.wolf.step"),
            stray_ambient: None,
            whine: Some("minecraft:entity.wolf_puglin.whine"),
        },
        baby: SoundSet {
            ambient: Some("minecraft:entity.baby_wolf.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.baby_wolf.death"),
            eat: None,
            growl: Some("minecraft:entity.baby_wolf.growl"),
            hiss: None,
            hurt: Some("minecraft:entity.baby_wolf.hurt"),
            pant: Some("minecraft:entity.baby_wolf.pant"),
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.baby_wolf.step"),
            stray_ambient: None,
            whine: Some("minecraft:entity.baby_wolf.whine"),
        },
    },
    SoundVariant {
        name: "minecraft:sad",
        adult: SoundSet {
            ambient: Some("minecraft:entity.wolf_sad.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.wolf_sad.death"),
            eat: None,
            growl: Some("minecraft:entity.wolf_sad.growl"),
            hiss: None,
            hurt: Some("minecraft:entity.wolf_sad.hurt"),
            pant: Some("minecraft:entity.wolf_sad.pant"),
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.wolf.step"),
            stray_ambient: None,
            whine: Some("minecraft:entity.wolf_sad.whine"),
        },
        baby: SoundSet {
            ambient: Some("minecraft:entity.baby_wolf.ambient"),
            beg_for_food: None,
            death: Some("minecraft:entity.baby_wolf.death"),
            eat: None,
            growl: Some("minecraft:entity.baby_wolf.growl"),
            hiss: None,
            hurt: Some("minecraft:entity.baby_wolf.hurt"),
            pant: Some("minecraft:entity.baby_wolf.pant"),
            purr: None,
            purreow: None,
            step: Some("minecraft:entity.baby_wolf.step"),
            stray_ambient: None,
            whine: Some("minecraft:entity.baby_wolf.whine"),
        },
    },
];

pub(crate) static CAT_SOUND_VARIANTS: [SoundVariant; 2] = [
    SoundVariant {
        name: "minecraft:classic",
        adult: SoundSet {
            ambient: Some("minecraft:entity.cat.ambient"),
            beg_for_food: Some("minecraft:entity.cat.beg_for_food"),
            death: Some("minecraft:entity.cat.death"),
            eat: Some("minecraft:entity.cat.eat"),
            growl: None,
            hiss: Some("minecraft:entity.cat.hiss"),
            hurt: Some("minecraft:entity.cat.hurt"),
            pant: None,
            purr: Some("minecraft:entity.cat.purr"),
            purreow: Some("minecraft:entity.cat.purreow"),
            step: None,
            stray_ambient: Some("minecraft:entity.cat.stray_ambient"),
            whine: None,
        },
        baby: SoundSet {
            ambient: Some("minecraft:entity.baby_cat.ambient"),
            beg_for_food: Some("minecraft:entity.baby_cat.beg_for_food"),
            death: Some("minecraft:entity.baby_cat.death"),
            eat: Some("minecraft:entity.baby_cat.eat"),
            growl: None,
            hiss: Some("minecraft:entity.baby_cat.hiss"),
            hurt: Some("minecraft:entity.baby_cat.hurt"),
            pant: None,
            purr: Some("minecraft:entity.baby_cat.purr"),
            purreow: Some("minecraft:entity.baby_cat.purreow"),
            step: None,
            stray_ambient: Some("minecraft:entity.baby_cat.stray_ambient"),
            whine: None,
        },
    },
    SoundVariant {
        name: "minecraft:royal",
        adult: SoundSet {
            ambient: Some("minecraft:entity.cat_royal.ambient"),
            beg_for_food: Some("minecraft:entity.cat_royal.beg_for_food"),
            death: Some("minecraft:entity.cat_royal.death"),
            eat: Some("minecraft:entity.cat_royal.eat"),
            growl: None,
            hiss: Some("minecraft:entity.cat_royal.hiss"),
            hurt: Some("minecraft:entity.cat_royal.hurt"),
            pant: None,
            purr: Some("minecraft:entity.cat_royal.purr"),
            purreow: Some("minecraft:entity.cat_royal.purreow"),
            step: None,
            stray_ambient: Some("minecraft:entity.cat_royal.stray_ambient"),
            whine: None,
        },
        baby: SoundSet {
            ambient: Some("minecraft:entity.baby_cat.ambient"),
            beg_for_food: Some("minecraft:entity.baby_cat.beg_for_food"),
            death: Some("minecraft:entity.baby_cat.death"),
            eat: Some("minecraft:entity.baby_cat.eat"),
            growl: None,
            hiss: Some("minecraft:entity.baby_cat.hiss"),
            hurt: Some("minecraft:entity.baby_cat.hurt"),
            pant: None,
            purr: Some("minecraft:entity.baby_cat.purr"),
            purreow: Some("minecraft:entity.baby_cat.purreow"),
            step: None,
            stray_ambient: Some("minecraft:entity.baby_cat.stray_ambient"),
            whine: None,
        },
    },
];

pub(crate) static DYE_MIXES: [(u8, u8, u8); 18] = [
    (0, 7, 8), (0, 11, 3), (0, 13, 5), (0, 14, 6), (0, 15, 7), (4, 14, 1), (6, 10, 2), (7, 0, 8),
    (10, 6, 2), (11, 0, 3), (11, 13, 9), (11, 14, 10), (13, 0, 5), (13, 11, 9), (14, 0, 6),
    (14, 4, 1), (14, 11, 10), (15, 0, 7),
];
