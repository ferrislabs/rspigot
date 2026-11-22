use crate::{AttributeTemplate, EffectCategory, EffectId, MobEffect};

pub struct BreathOfTheNautilus;

impl MobEffect for BreathOfTheNautilus {
    const NAME: &'static str = "minecraft:breath_of_the_nautilus";
    const TRANSLATION_KEY: &'static str = "effect.minecraft.breath_of_the_nautilus";
    const ID: EffectId = EffectId(39);
    const CATEGORY: EffectCategory = EffectCategory::Beneficial;
    const COLOR: u32 = 0x00FFEE;
    const INSTANTANEOUS: bool = false;
    const MODIFIERS: &'static [AttributeTemplate] = &[];
    type Behavior = crate::family::KeepsAir;
}
