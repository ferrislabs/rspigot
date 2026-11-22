use crate::{AttributeTemplate, EffectCategory, EffectId, MobEffect};

pub struct WindCharged;

impl MobEffect for WindCharged {
    const NAME: &'static str = "minecraft:wind_charged";
    const TRANSLATION_KEY: &'static str = "effect.minecraft.wind_charged";
    const ID: EffectId = EffectId(35);
    const CATEGORY: EffectCategory = EffectCategory::Harmful;
    const COLOR: u32 = 0xBDC9FF;
    const INSTANTANEOUS: bool = false;
    const MODIFIERS: &'static [AttributeTemplate] = &[];
    type Behavior = crate::family::BurstOnDeath;
}
