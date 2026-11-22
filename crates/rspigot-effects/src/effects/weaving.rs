use crate::{AttributeTemplate, EffectCategory, EffectId, MobEffect};

pub struct Weaving;

impl MobEffect for Weaving {
    const NAME: &'static str = "minecraft:weaving";
    const TRANSLATION_KEY: &'static str = "effect.minecraft.weaving";
    const ID: EffectId = EffectId(36);
    const CATEGORY: EffectCategory = EffectCategory::Harmful;
    const COLOR: u32 = 0x78695A;
    const INSTANTANEOUS: bool = false;
    const MODIFIERS: &'static [AttributeTemplate] = &[];
    type Behavior = crate::family::CobwebsOnDeath;
}
