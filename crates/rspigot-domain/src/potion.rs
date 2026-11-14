use std::num::NonZeroU8;

use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};

use crate::effect::{EffectDuration, EffectId, EffectInstance};
use crate::entity::Aabb;
use crate::item::ItemStack;
use crate::mth;

mod table;

pub use table::POTIONS;

pub const CONTENTS: &str = "minecraft:potion_contents";
pub const DURATION_SCALE: &str = "minecraft:potion_duration_scale";
pub const REGISTRY: &str = "minecraft:potion";
pub const DEFAULT_COLOR: i32 = -13_083_194;
pub const WATER: &str = "minecraft:water";
pub const HARMING: &str = "minecraft:harming";
pub const HEALING: &str = "minecraft:healing";
pub const REGENERATION: &str = "minecraft:regeneration";
pub const SLOWNESS: &str = "minecraft:slowness";
pub const POISON: &str = "minecraft:poison";
pub const WEAKNESS: &str = "minecraft:weakness";
pub const SWIFTNESS: &str = "minecraft:swiftness";
pub const FIRE_RESISTANCE: &str = "minecraft:fire_resistance";
pub const WATER_BREATHING: &str = "minecraft:water_breathing";
pub const DOUSES_FIRE: &str = "minecraft:douses_fire";
pub const EXTINGUISHES_ENTITIES: &str = "minecraft:extinguishes_entities";
pub const HURTS_WATER_SENSITIVE: &str = "minecraft:hurts_water_sensitive_entities";
pub const REHYDRATES_AXOLOTLS: &str = "minecraft:rehydrates_axolotls";
pub const INTENSITY_RADIUS: f64 = 4.0;
pub const SPLASH_REACH_SQR: f64 = 16.0;
pub const NEARBY_WIDTH: f64 = 4.0;
pub const NEARBY_HEIGHT: f64 = 2.0;
pub const WATER_DAMAGE: f32 = 1.0;
pub const MIN_SPLASH_DURATION: u32 = 20;
pub const GRAVITY: f64 = 0.05;
pub const SPLASH_EVENT: i32 = 2002;
pub const INSTANT_SPLASH_EVENT: i32 = 2007;
pub const SPLASH_SOUND_EVENT: i32 = 1053;
pub const INSTANT_SPLASH_SOUND_EVENT: i32 = 1054;
const HEAL_BASE: i32 = 4;
const HARM_BASE: i32 = 6;
const ALPHA: u32 = 0xFF00_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PotionEffect {
    pub effect: &'static str,
    pub duration: i32,
    pub amplifier: u8,
}

impl PotionEffect {
    pub fn instance(&self) -> Option<EffectInstance> {
        let effect = EffectId::from_name(self.effect)?;
        let duration = u32::try_from(self.duration).ok()?;
        Some(EffectInstance::new(
            effect,
            EffectDuration::Ticks(duration),
            self.amplifier,
            true,
        ))
    }
}

pub fn effects_of(potion: &str) -> Option<&'static [PotionEffect]> {
    POTIONS
        .iter()
        .find(|(name, _)| *name == potion)
        .map(|(_, effects)| *effects)
}

pub fn has_instant_effects(potion: &str) -> bool {
    effects_of(potion).is_some_and(|effects| {
        effects.iter().any(|effect| {
            EffectId::from_name(effect.effect)
                .and_then(EffectId::kind)
                .is_some_and(|kind| kind.instantaneous())
        })
    })
}

pub fn in_tag(potion: &str, tag: &str) -> bool {
    let Some(id) = rspigot_data::registries::static_index(REGISTRY, potion) else {
        return false;
    };
    rspigot_data::tags::tag(REGISTRY, tag).is_some_and(|ids| ids.binary_search(&id).is_ok())
}

pub fn stack(item: ItemKind, potion: &str) -> ItemStack {
    let mut stack = ItemStack::new(item.id(), NonZeroU8::MIN);
    stack
        .components
        .insert(CONTENTS, Compound::new().with("potion", potion));
    stack
}

pub fn duration_scale(stack: &ItemStack) -> f32 {
    stack
        .components
        .get(DURATION_SCALE)
        .and_then(Tag::as_f64)
        .map_or(1.0, |scale| scale as f32)
}

pub fn scale_duration(duration: EffectDuration, intensity: f64, scale: f32) -> EffectDuration {
    match duration {
        EffectDuration::Infinite | EffectDuration::Ticks(0) => duration,
        EffectDuration::Ticks(ticks) => {
            let scaled = intensity * f64::from(ticks as i32) * f64::from(scale) + 0.5;
            EffectDuration::Ticks(scaled as i32 as u32)
        }
    }
}

pub fn with_scaled_duration(instance: &EffectInstance, scale: f32) -> EffectInstance {
    let duration = match instance.duration {
        EffectDuration::Infinite | EffectDuration::Ticks(0) => instance.duration,
        EffectDuration::Ticks(ticks) => {
            let scaled = mth::floor(f64::from(ticks as f32 * scale)).max(1);
            EffectDuration::Ticks(scaled as u32)
        }
    };
    EffectInstance {
        duration,
        ..*instance
    }
}

pub fn ends_within(instance: &EffectInstance, ticks: u32) -> bool {
    match instance.duration {
        EffectDuration::Infinite => false,
        EffectDuration::Ticks(duration) => duration <= ticks,
    }
}

pub fn splash_intensity(potion: &Aabb, target: &Aabb, margin: f64) -> Option<f64> {
    let distance = potion.distance_to_sqr_box(&target.inflate(margin));
    (distance < SPLASH_REACH_SQR).then(|| 1.0 - distance.sqrt() / INTENSITY_RADIUS)
}

pub fn heal_amount(amplifier: u8, scale: f64) -> f32 {
    let base = f64::from(HEAL_BASE << (u32::from(amplifier) & 31));
    (scale * base + 0.5) as i32 as f32
}

pub fn harm_amount(amplifier: u8, scale: f64) -> f32 {
    let base = f64::from(HARM_BASE << (u32::from(amplifier) & 31));
    (scale * base + 0.5) as i32 as f32
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PotionContents<'a> {
    pub potion: Option<&'a str>,
    pub custom_color: Option<i32>,
    pub custom_effects: Vec<EffectInstance>,
}

fn custom_effect(tag: &Tag) -> Option<EffectInstance> {
    let compound = tag.as_compound()?;
    let effect = EffectId::from_name(compound.get_str("id")?)?;
    let amplifier = compound.get_i64("amplifier").map_or(0, |value| value as u8);
    let duration = match compound.get_i32("duration") {
        Some(-1) => EffectDuration::Infinite,
        Some(ticks) => EffectDuration::Ticks(u32::try_from(ticks).unwrap_or(0)),
        None => EffectDuration::Ticks(0),
    };
    let visible = compound.get_bool("show_particles").unwrap_or(true);
    let show_icon = compound.get_bool("show_icon").unwrap_or(visible);
    Some(EffectInstance {
        effect,
        amplifier,
        duration,
        ambient: compound.get_bool("ambient").unwrap_or(false),
        visible,
        show_icon,
    })
}

impl<'a> PotionContents<'a> {
    pub fn of(stack: &'a ItemStack) -> Self {
        let Some(contents) = stack.components.get_compound(CONTENTS) else {
            return Self::default();
        };
        Self::of_compound(contents)
    }

    pub fn of_compound(contents: &'a Compound) -> Self {
        Self {
            potion: contents.get_str("potion"),
            custom_color: contents.get_i32("custom_color"),
            custom_effects: contents
                .get_list("custom_effects")
                .map(|list| list.iter().filter_map(custom_effect).collect())
                .unwrap_or_default(),
        }
    }

    pub fn is_potion_without_custom_effects(&self) -> bool {
        self.potion.is_some() && self.custom_effects.is_empty()
    }

    pub fn is(&self, potion: &str) -> bool {
        self.is_potion_without_custom_effects() && self.potion == Some(potion)
    }

    pub fn is_in(&self, tag: &str) -> bool {
        self.is_potion_without_custom_effects() && self.potion.is_some_and(|name| in_tag(name, tag))
    }

    pub fn has_instant_potion(&self) -> bool {
        self.potion.is_some_and(has_instant_effects)
    }

    pub fn effects(&self) -> Vec<EffectInstance> {
        let base = self
            .potion
            .and_then(effects_of)
            .unwrap_or_default()
            .iter()
            .filter_map(PotionEffect::instance);
        base.chain(self.custom_effects.iter().copied()).collect()
    }

    pub fn scaled_effects(&self, scale: f32) -> Vec<EffectInstance> {
        self.effects()
            .iter()
            .map(|effect| with_scaled_duration(effect, scale))
            .collect()
    }

    pub fn color(&self) -> i32 {
        self.custom_color
            .unwrap_or_else(|| effects_color(&self.effects()).unwrap_or(DEFAULT_COLOR))
    }
}

pub fn effects_color(effects: &[EffectInstance]) -> Option<i32> {
    let (mut red, mut green, mut blue, mut weight) = (0_i32, 0_i32, 0_i32, 0_i32);
    for effect in effects.iter().filter(|effect| effect.visible) {
        let Some(kind) = effect.effect.kind() else {
            continue;
        };
        let color = kind.color();
        let level = i32::from(effect.amplifier) + 1;
        red += level * ((color >> 16) & 0xFF) as i32;
        green += level * ((color >> 8) & 0xFF) as i32;
        blue += level * (color & 0xFF) as i32;
        weight += level;
    }
    if weight == 0 {
        return None;
    }
    let packed = ALPHA
        | (((red / weight) as u32 & 0xFF) << 16)
        | (((green / weight) as u32 & 0xFF) << 8)
        | ((blue / weight) as u32 & 0xFF);
    Some(packed as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_lists_every_registered_potion_in_registry_order() {
        let names = rspigot_data::registries::static_entries(REGISTRY).unwrap();
        assert_eq!(POTIONS.len(), names.len());
        for (index, (name, _)) in POTIONS.iter().enumerate() {
            assert_eq!(names[index], *name);
        }
    }

    #[test]
    fn every_effect_of_every_potion_resolves() {
        for (name, effects) in POTIONS {
            for effect in *effects {
                assert!(effect.instance().is_some(), "{name} {}", effect.effect);
            }
        }
    }

    #[test]
    fn potion_effects_follow_the_decompiled_constants() {
        let effect = |name: &str| effects_of(name).unwrap()[0];
        assert_eq!(effect(SWIFTNESS).duration, 3600);
        assert_eq!(effect("minecraft:long_swiftness").duration, 9600);
        assert_eq!(effect("minecraft:strong_swiftness").amplifier, 1);
        assert_eq!(effect("minecraft:strong_swiftness").duration, 1800);
        assert_eq!(effect(SLOWNESS).duration, 1800);
        assert_eq!(effect(POISON).duration, 900);
        assert_eq!(effect(REGENERATION).duration, 900);
        assert_eq!(effect(WEAKNESS).duration, 1800);
        assert_eq!(effect(HARMING).duration, 1);
        assert_eq!(effect(HEALING).effect, "minecraft:instant_health");
        assert_eq!(effect(HARMING).effect, "minecraft:instant_damage");
        assert_eq!(effect(FIRE_RESISTANCE).duration, 3600);
        assert_eq!(effect(WATER_BREATHING).duration, 3600);
        assert_eq!(effect("minecraft:luck").duration, 6000);
        assert_eq!(effects_of("minecraft:turtle_master").unwrap().len(), 2);
        assert!(effects_of(WATER).unwrap().is_empty());
    }

    #[test]
    fn only_healing_and_harming_potions_have_instant_effects() {
        assert!(has_instant_effects(HEALING));
        assert!(has_instant_effects("minecraft:strong_harming"));
        assert!(!has_instant_effects(POISON));
        assert!(!has_instant_effects(WATER));
    }

    #[test]
    fn water_alone_carries_the_water_tags() {
        for tag in [
            DOUSES_FIRE,
            EXTINGUISHES_ENTITIES,
            HURTS_WATER_SENSITIVE,
            REHYDRATES_AXOLOTLS,
        ] {
            assert!(in_tag(WATER, tag), "{tag}");
            assert!(!in_tag("minecraft:awkward", tag), "{tag}");
            assert!(!in_tag(HEALING, tag), "{tag}");
        }
    }

    #[test]
    fn contents_read_the_potion_and_ignore_the_tag_with_custom_effects() {
        let plain = stack(ItemKind::SplashPotion, WATER);
        let contents = PotionContents::of(&plain);
        assert!(contents.is(WATER));
        assert!(contents.is_in(DOUSES_FIRE));
        let mut custom = plain.clone();
        let list = vec![Tag::Compound(
            Compound::new()
                .with("id", "minecraft:speed")
                .with("duration", 100)
                .with("amplifier", 1_i8),
        )];
        custom.components.insert(
            CONTENTS,
            Compound::new()
                .with("potion", WATER)
                .with("custom_effects", Tag::List(list)),
        );
        let contents = PotionContents::of(&custom);
        assert!(!contents.is_in(DOUSES_FIRE));
        assert!(!contents.is(WATER));
        assert_eq!(contents.effects().len(), 1);
        assert_eq!(contents.effects()[0].amplifier, 1);
    }

    #[test]
    fn an_empty_stack_has_no_potion_and_the_water_colour() {
        let empty = ItemStack::new(ItemKind::SplashPotion.id(), NonZeroU8::MIN);
        let contents = PotionContents::of(&empty);
        assert_eq!(contents.potion, None);
        assert!(contents.effects().is_empty());
        assert_eq!(contents.color(), DEFAULT_COLOR);
        assert!(!contents.is_in(DOUSES_FIRE));
    }

    #[test]
    fn the_colour_averages_the_visible_effects_weighted_by_level() {
        let healing = stack(ItemKind::SplashPotion, HEALING);
        let color = PotionContents::of(&healing).color() as u32;
        let kind = EffectId::from_name("minecraft:instant_health")
            .and_then(EffectId::kind)
            .unwrap();
        assert_eq!(color, 0xFF00_0000 | kind.color());
        let mixed = effects_color(&[
            PotionEffect {
                effect: "minecraft:speed",
                duration: 10,
                amplifier: 0,
            }
            .instance()
            .unwrap(),
            PotionEffect {
                effect: "minecraft:slowness",
                duration: 10,
                amplifier: 2,
            }
            .instance()
            .unwrap(),
        ])
        .unwrap() as u32;
        let speed = EffectId::from_name("minecraft:speed")
            .and_then(EffectId::kind)
            .unwrap()
            .color();
        let slow = EffectId::from_name("minecraft:slowness")
            .and_then(EffectId::kind)
            .unwrap()
            .color();
        let channel = |shift: u32| (((speed >> shift) & 255) + 3 * ((slow >> shift) & 255)) / 4;
        assert_eq!(
            mixed,
            0xFF00_0000 | (channel(16) << 16) | (channel(8) << 8) | channel(0)
        );
        let mut hidden = PotionEffect {
            effect: "minecraft:speed",
            duration: 10,
            amplifier: 0,
        }
        .instance()
        .unwrap();
        hidden.visible = false;
        assert_eq!(effects_color(&[hidden]), None);
    }

    #[test]
    fn splash_durations_scale_with_intensity_and_round_half_up() {
        let ticks = EffectDuration::Ticks(900);
        assert_eq!(scale_duration(ticks, 1.0, 1.0), EffectDuration::Ticks(900));
        assert_eq!(scale_duration(ticks, 0.5, 1.0), EffectDuration::Ticks(450));
        assert_eq!(
            scale_duration(EffectDuration::Ticks(3), 0.5, 1.0),
            EffectDuration::Ticks(2)
        );
        assert_eq!(
            scale_duration(EffectDuration::Infinite, 0.1, 1.0),
            EffectDuration::Infinite
        );
        assert_eq!(
            scale_duration(EffectDuration::Ticks(0), 0.1, 1.0),
            EffectDuration::Ticks(0)
        );
    }

    #[test]
    fn drinking_scales_the_duration_by_floor_with_a_floor_of_one() {
        let instance = effects_of(SWIFTNESS).unwrap()[0].instance().unwrap();
        assert_eq!(
            with_scaled_duration(&instance, 1.0).duration,
            EffectDuration::Ticks(3600)
        );
        assert_eq!(
            with_scaled_duration(&instance, 0.25).duration,
            EffectDuration::Ticks(900)
        );
        assert_eq!(
            with_scaled_duration(&instance, 0.0).duration,
            EffectDuration::Ticks(1)
        );
    }

    #[test]
    fn effects_that_end_within_a_second_are_skipped() {
        let mut instance = effects_of(POISON).unwrap()[0].instance().unwrap();
        instance.duration = EffectDuration::Ticks(20);
        assert!(ends_within(&instance, MIN_SPLASH_DURATION));
        instance.duration = EffectDuration::Ticks(21);
        assert!(!ends_within(&instance, MIN_SPLASH_DURATION));
        instance.duration = EffectDuration::Infinite;
        assert!(!ends_within(&instance, MIN_SPLASH_DURATION));
    }

    #[test]
    fn the_splash_intensity_falls_linearly_to_zero_four_blocks_away() {
        let potion = Aabb::new(0.0, 0.0, 0.0, 0.25, 0.25, 0.25);
        let at = |x: f64| Aabb::new(x, 0.0, 0.0, x + 0.6, 1.8, 0.6);
        assert_eq!(splash_intensity(&potion, &at(0.1), 0.0), Some(1.0));
        assert_eq!(splash_intensity(&potion, &at(2.25), 0.0), Some(0.5));
        assert_eq!(splash_intensity(&potion, &at(3.25), 0.0), Some(0.25));
        assert_eq!(splash_intensity(&potion, &at(4.25), 0.0), None);
        assert_eq!(splash_intensity(&potion, &at(4.5), 0.0), None);
    }

    #[test]
    fn the_target_box_is_inflated_by_the_projectile_margin_before_measuring() {
        let potion = Aabb::new(0.0, 0.0, 0.0, 0.25, 0.25, 0.25);
        let target = Aabb::new(2.25, 0.0, 0.0, 2.85, 1.8, 0.6);
        assert_eq!(splash_intensity(&potion, &target, 0.0), Some(0.5));
        assert_eq!(splash_intensity(&potion, &target, 1.0), Some(0.75));
    }

    #[test]
    fn instant_potions_heal_four_and_harm_six_scaled_by_intensity() {
        assert_eq!(heal_amount(0, 1.0), 4.0);
        assert_eq!(heal_amount(1, 1.0), 8.0);
        assert_eq!(harm_amount(0, 1.0), 6.0);
        assert_eq!(harm_amount(1, 1.0), 12.0);
        assert_eq!(harm_amount(0, 0.5), 3.0);
        assert_eq!(harm_amount(0, 0.25), 2.0);
        assert_eq!(heal_amount(0, 0.1), 0.0);
    }
}
