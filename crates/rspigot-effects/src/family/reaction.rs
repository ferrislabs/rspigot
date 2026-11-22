use super::EffectBehavior;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Reaction {
    None,
    SilverfishOnHurt { chance: f32, min: i32, max: i32 },
    SlimesOnDeath { count: i32, size: i32 },
    CobwebsOnDeath { min: i32, max: i32 },
    BurstOnDeath { radius: f32 },
    KeepsAir,
}

pub struct SilverfishOnHurt;

impl EffectBehavior for SilverfishOnHurt {
    const REACTION: Reaction = Reaction::SilverfishOnHurt {
        chance: 0.1,
        min: 1,
        max: 2,
    };
}

pub struct SlimesOnDeath;

impl EffectBehavior for SlimesOnDeath {
    const REACTION: Reaction = Reaction::SlimesOnDeath { count: 2, size: 2 };
}

pub struct CobwebsOnDeath;

impl EffectBehavior for CobwebsOnDeath {
    const REACTION: Reaction = Reaction::CobwebsOnDeath { min: 2, max: 3 };
}

pub struct BurstOnDeath;

impl EffectBehavior for BurstOnDeath {
    const REACTION: Reaction = Reaction::BurstOnDeath { radius: 3.0 };
}

pub struct KeepsAir;

impl EffectBehavior for KeepsAir {
    const REACTION: Reaction = Reaction::KeepsAir;
}
