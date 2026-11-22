#[rustfmt::skip]
mod effects;
pub mod family;
#[rustfmt::skip]
mod kind;
mod registry;

pub use effects::*;
pub use family::{EffectBehavior, Reaction};
pub use kind::EffectKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EffectId(pub u16);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectCategory {
    Beneficial,
    Harmful,
    Neutral,
}

impl EffectCategory {
    pub const fn is_beneficial(self) -> bool {
        matches!(self, Self::Beneficial)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttributeOperation {
    AddValue,
    AddMultipliedBase,
    AddMultipliedTotal,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttributeTemplate {
    pub attribute: &'static str,
    pub modifier: &'static str,
    pub amount: f64,
    pub operation: AttributeOperation,
}

impl AttributeTemplate {
    pub const fn amount_for(&self, amplifier: u8) -> f64 {
        self.amount * (amplifier as f64 + 1.0)
    }
}

pub trait MobEffect: 'static {
    const NAME: &'static str;
    const TRANSLATION_KEY: &'static str;
    const ID: EffectId;
    const CATEGORY: EffectCategory;
    const COLOR: u32;
    const INSTANTANEOUS: bool;
    const MODIFIERS: &'static [AttributeTemplate];
    type Behavior: EffectBehavior;
}
