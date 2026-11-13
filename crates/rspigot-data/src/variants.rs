use crate::generated::variants::{
    CAT_SOUND_VARIANTS, CAT_VARIANTS, CHICKEN_SOUND_VARIANTS, CHICKEN_VARIANTS, COW_SOUND_VARIANTS,
    COW_VARIANTS, DYE_MIXES, FROG_VARIANTS, PIG_SOUND_VARIANTS, PIG_VARIANTS, WOLF_SOUND_VARIANTS,
    WOLF_VARIANTS,
};
use crate::tags::HolderSet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VariantCondition {
    Biome(HolderSet),
    Structure(&'static str),
    MoonBrightness { min: Option<f32>, max: Option<f32> },
}

impl VariantCondition {
    pub fn moon_brightness_matches(min: Option<f32>, max: Option<f32>, value: f32) -> bool {
        let value = f64::from(value);
        min.is_none_or(|min| f64::from(min) <= value)
            && max.is_none_or(|max| value <= f64::from(max))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpawnCondition {
    pub condition: Option<VariantCondition>,
    pub priority: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MobVariant {
    pub name: &'static str,
    pub spawn_conditions: &'static [SpawnCondition],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundSet {
    pub ambient: Option<&'static str>,
    pub beg_for_food: Option<&'static str>,
    pub death: Option<&'static str>,
    pub eat: Option<&'static str>,
    pub growl: Option<&'static str>,
    pub hiss: Option<&'static str>,
    pub hurt: Option<&'static str>,
    pub pant: Option<&'static str>,
    pub purr: Option<&'static str>,
    pub purreow: Option<&'static str>,
    pub step: Option<&'static str>,
    pub stray_ambient: Option<&'static str>,
    pub whine: Option<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundVariant {
    pub name: &'static str,
    pub adult: SoundSet,
    pub baby: SoundSet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VariantFamily {
    Cow,
    Pig,
    Chicken,
    Wolf,
    Cat,
    Frog,
}

impl VariantFamily {
    pub const ALL: [Self; 6] = [
        Self::Cow,
        Self::Pig,
        Self::Chicken,
        Self::Wolf,
        Self::Cat,
        Self::Frog,
    ];

    pub const fn has_sound_variants(self) -> bool {
        !matches!(self, Self::Frog)
    }

    pub const fn is_farm(self) -> bool {
        matches!(self, Self::Cow | Self::Pig | Self::Chicken)
    }

    pub const fn variant_registry(self) -> &'static str {
        match self {
            Self::Cow => "minecraft:cow_variant",
            Self::Pig => "minecraft:pig_variant",
            Self::Chicken => "minecraft:chicken_variant",
            Self::Wolf => "minecraft:wolf_variant",
            Self::Cat => "minecraft:cat_variant",
            Self::Frog => "minecraft:frog_variant",
        }
    }

    pub const fn sound_variant_registry(self) -> &'static str {
        match self {
            Self::Cow => "minecraft:cow_sound_variant",
            Self::Pig => "minecraft:pig_sound_variant",
            Self::Chicken => "minecraft:chicken_sound_variant",
            Self::Wolf => "minecraft:wolf_sound_variant",
            Self::Cat => "minecraft:cat_sound_variant",
            Self::Frog => "minecraft:frog_variant",
        }
    }

    pub fn variants(self) -> &'static [MobVariant] {
        match self {
            Self::Cow => &COW_VARIANTS,
            Self::Pig => &PIG_VARIANTS,
            Self::Chicken => &CHICKEN_VARIANTS,
            Self::Wolf => &WOLF_VARIANTS,
            Self::Cat => &CAT_VARIANTS,
            Self::Frog => &FROG_VARIANTS,
        }
    }

    pub fn sound_variants(self) -> &'static [SoundVariant] {
        match self {
            Self::Cow => &COW_SOUND_VARIANTS,
            Self::Pig => &PIG_SOUND_VARIANTS,
            Self::Chicken => &CHICKEN_SOUND_VARIANTS,
            Self::Wolf => &WOLF_SOUND_VARIANTS,
            Self::Cat => &CAT_SOUND_VARIANTS,
            Self::Frog => &[],
        }
    }

    pub fn variant_index(self, name: &str) -> Option<u32> {
        index_of(self.variants().iter().map(|v| v.name), name)
    }

    pub fn sound_variant_index(self, name: &str) -> Option<u32> {
        index_of(self.sound_variants().iter().map(|v| v.name), name)
    }
}

fn index_of<'a>(names: impl Iterator<Item = &'a str>, name: &str) -> Option<u32> {
    names
        .enumerate()
        .find(|(_, candidate)| *candidate == name)
        .and_then(|(index, _)| u32::try_from(index).ok())
}

pub fn dye_mix(first: u8, second: u8) -> Option<u8> {
    DYE_MIXES
        .binary_search_by(|(a, b, _)| (*a, *b).cmp(&(first, second)))
        .ok()
        .map(|index| DYE_MIXES[index].2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries;

    #[test]
    fn variants_follow_the_synced_registry_order() {
        for family in VariantFamily::ALL {
            let names: Vec<&str> = family.variants().iter().map(|v| v.name).collect();
            assert_eq!(
                Some(names.as_slice()),
                registries::synced_entries(family.variant_registry()),
                "{family:?}"
            );
            let names: Vec<&str> = family.sound_variants().iter().map(|v| v.name).collect();
            assert_eq!(
                Some(names.as_slice()),
                registries::synced_entries(family.sound_variant_registry()),
                "{family:?}"
            );
        }
        assert_eq!(
            VariantFamily::Cow.variant_index("minecraft:temperate"),
            Some(1)
        );
        assert_eq!(
            VariantFamily::Pig.sound_variant_index("minecraft:classic"),
            Some(1)
        );
        assert_eq!(VariantFamily::Wolf.variant_index("minecraft:pale"), Some(3));
        assert_eq!(
            VariantFamily::Wolf.sound_variant_index("minecraft:classic"),
            Some(2)
        );
        assert_eq!(VariantFamily::Cat.variant_index("minecraft:black"), Some(1));
        assert_eq!(
            VariantFamily::Cat.sound_variant_index("minecraft:classic"),
            Some(0)
        );
    }

    #[test]
    fn farm_variants_spawn_by_biome_tag() {
        let cold = VariantFamily::Cow.variants()[0];
        assert_eq!(cold.name, "minecraft:cold");
        assert_eq!(
            cold.spawn_conditions,
            &[SpawnCondition {
                condition: Some(VariantCondition::Biome(HolderSet::Tag(
                    "minecraft:spawns_cold_variant_farm_animals"
                ))),
                priority: 1,
            }]
        );
        let temperate = VariantFamily::Chicken.variants()[1];
        assert_eq!(
            temperate.spawn_conditions,
            &[SpawnCondition {
                condition: None,
                priority: 0
            }]
        );
    }

    #[test]
    fn wolf_variants_spawn_in_their_biome() {
        let snowy_taiga =
            registries::synced_index("minecraft:worldgen/biome", "minecraft:snowy_taiga").unwrap();
        let ashen = VariantFamily::Wolf.variants()[0];
        assert_eq!(ashen.name, "minecraft:ashen");
        let [condition] = ashen.spawn_conditions else {
            panic!("one condition");
        };
        assert_eq!(condition.priority, 1);
        let Some(VariantCondition::Biome(biomes)) = condition.condition else {
            panic!("biome condition");
        };
        assert_eq!(biomes.ids("minecraft:worldgen/biome"), &[snowy_taiga]);
        let pale = VariantFamily::Wolf.variants()[3];
        assert_eq!(pale.name, "minecraft:pale");
        assert_eq!(pale.spawn_conditions[0].condition, None);
        assert_eq!(pale.spawn_conditions[0].priority, 0);
    }

    #[test]
    fn cat_variants_have_structure_and_moon_conditions() {
        let black = VariantFamily::Cat.variants()[0];
        assert_eq!(black.name, "minecraft:all_black");
        assert_eq!(
            black.spawn_conditions,
            &[
                SpawnCondition {
                    condition: Some(VariantCondition::Structure("minecraft:cats_spawn_as_black")),
                    priority: 1,
                },
                SpawnCondition {
                    condition: Some(VariantCondition::MoonBrightness {
                        min: Some(0.9),
                        max: None
                    }),
                    priority: 0,
                },
            ]
        );
        assert!(VariantCondition::moon_brightness_matches(
            Some(0.9),
            None,
            1.0
        ));
        assert!(!VariantCondition::moon_brightness_matches(
            Some(0.9),
            None,
            0.75
        ));
        assert!(VariantCondition::moon_brightness_matches(
            None,
            Some(0.5),
            0.5
        ));
    }

    #[test]
    fn wolf_and_cat_sound_sets_carry_their_own_sounds() {
        let big = VariantFamily::Wolf.sound_variants()[1];
        assert_eq!(big.name, "minecraft:big");
        assert_eq!(big.adult.growl, Some("minecraft:entity.wolf_big.growl"));
        assert_eq!(big.adult.step, Some("minecraft:entity.wolf.step"));
        assert_eq!(big.baby.whine, Some("minecraft:entity.baby_wolf.whine"));
        assert_eq!(big.adult.purr, None);
        let royal = VariantFamily::Cat.sound_variants()[1];
        assert_eq!(royal.name, "minecraft:royal");
        assert_eq!(
            royal.adult.stray_ambient,
            Some("minecraft:entity.cat_royal.stray_ambient")
        );
        assert_eq!(
            royal.baby.beg_for_food,
            Some("minecraft:entity.baby_cat.beg_for_food")
        );
        assert_eq!(royal.adult.growl, None);
    }

    #[test]
    fn sound_sets_split_adults_and_babies() {
        let big = VariantFamily::Pig.sound_variants()[0];
        assert_eq!(big.adult.ambient, Some("minecraft:entity.pig_big.ambient"));
        assert_eq!(big.baby.ambient, Some("minecraft:entity.baby_pig.ambient"));
        let moody = VariantFamily::Cow.sound_variants()[1];
        assert_eq!(moody.adult.hurt, Some("minecraft:entity.cow_moody.hurt"));
        assert_eq!(moody.baby, moody.adult);
    }

    #[test]
    fn dye_mixes_come_from_two_dye_recipes() {
        assert_eq!(dye_mix(14, 4), Some(1));
        assert_eq!(dye_mix(4, 14), Some(1));
        assert_eq!(dye_mix(15, 0), Some(7));
        assert_eq!(dye_mix(0, 0), None);
        assert_eq!(dye_mix(11, 13), Some(9));
    }
}
