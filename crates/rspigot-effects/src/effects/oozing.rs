use crate::{AttributeTemplate, EffectCategory, EffectId, MobEffect};

pub struct Oozing;

impl MobEffect for Oozing {
    const NAME: &'static str = "minecraft:oozing";
    const TRANSLATION_KEY: &'static str = "effect.minecraft.oozing";
    const ID: EffectId = EffectId(37);
    const CATEGORY: EffectCategory = EffectCategory::Harmful;
    const COLOR: u32 = 0x99FFA3;
    const INSTANTANEOUS: bool = false;
    const MODIFIERS: &'static [AttributeTemplate] = &[];
    type Behavior = crate::family::SlimesOnDeath;
}
