use crate::{AttributeTemplate, EffectCategory, EffectId, MobEffect};

pub struct Infested;

impl MobEffect for Infested {
    const NAME: &'static str = "minecraft:infested";
    const TRANSLATION_KEY: &'static str = "effect.minecraft.infested";
    const ID: EffectId = EffectId(38);
    const CATEGORY: EffectCategory = EffectCategory::Harmful;
    const COLOR: u32 = 0x8C9B8C;
    const INSTANTANEOUS: bool = false;
    const MODIFIERS: &'static [AttributeTemplate] = &[];
    type Behavior = crate::family::SilverfishOnHurt;
}
