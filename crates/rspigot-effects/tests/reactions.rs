use rspigot_effects::{EffectKind, Reaction};

#[test]
fn the_five_reactive_effects_carry_their_vanilla_reaction() {
    assert_eq!(
        EffectKind::Infested.reaction(),
        Reaction::SilverfishOnHurt {
            chance: 0.1,
            min: 1,
            max: 2
        }
    );
    assert_eq!(
        EffectKind::Oozing.reaction(),
        Reaction::SlimesOnDeath { count: 2, size: 2 }
    );
    assert_eq!(
        EffectKind::Weaving.reaction(),
        Reaction::CobwebsOnDeath { min: 2, max: 3 }
    );
    assert_eq!(
        EffectKind::WindCharged.reaction(),
        Reaction::BurstOnDeath { radius: 3.0 }
    );
    assert_eq!(
        EffectKind::BreathOfTheNautilus.reaction(),
        Reaction::KeepsAir
    );
}

#[test]
fn every_other_effect_reacts_to_nothing() {
    let reactive = [
        EffectKind::Infested,
        EffectKind::Oozing,
        EffectKind::Weaving,
        EffectKind::WindCharged,
        EffectKind::BreathOfTheNautilus,
    ];
    for kind in EffectKind::ALL {
        if !reactive.contains(&kind) {
            assert_eq!(kind.reaction(), Reaction::None, "{kind:?}");
        }
    }
}
