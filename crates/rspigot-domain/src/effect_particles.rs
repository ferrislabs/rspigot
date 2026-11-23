use crate::effect::ActiveEffects;
use crate::entity::ParticleSetting;

pub const ENTITY_EFFECT: &str = "minecraft:entity_effect";
const OPAQUE: u32 = 255;
const AMBIENT_ALPHA: u32 = 38;

pub fn synchronized(effects: &ActiveEffects) -> (Vec<ParticleSetting>, bool) {
    if effects.is_empty() {
        return (Vec::new(), false);
    }
    let particles = effects
        .iter()
        .filter(|effect| effect.visible)
        .filter_map(|effect| {
            let alpha = if effect.ambient {
                AMBIENT_ALPHA
            } else {
                OPAQUE
            };
            let argb = (alpha << 24) | (effect.effect.kind()?.color() & 0x00FF_FFFF);
            Some(ParticleSetting::Color {
                name: ENTITY_EFFECT.to_owned(),
                argb: argb as i32,
            })
        })
        .collect();
    let ambient = effects
        .iter()
        .all(|effect| !effect.visible || effect.ambient);
    (particles, ambient)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::{EffectDuration, EffectId, EffectInstance, EffectKind};

    fn id(kind: EffectKind) -> EffectId {
        kind.id()
    }

    fn apply(effects: &mut ActiveEffects, kind: EffectKind, visible: bool, ambient: bool) {
        let mut instance = EffectInstance::new(id(kind), EffectDuration::Ticks(100), 0, visible);
        instance.ambient = ambient;
        let _ = effects.apply(instance);
    }

    #[test]
    fn no_effects_means_no_particles() {
        assert_eq!(synchronized(&ActiveEffects::default()), (Vec::new(), false));
    }

    #[test]
    fn visible_effects_emit_their_color_and_hidden_ones_do_not() {
        let mut effects = ActiveEffects::default();
        apply(&mut effects, EffectKind::Speed, true, false);
        apply(&mut effects, EffectKind::Slowness, false, false);
        let (particles, ambient) = synchronized(&effects);
        assert_eq!(particles.len(), 1);
        assert!(!ambient);
        let expected = 0xFF00_0000u32 | EffectKind::Speed.color();
        assert!(matches!(
            &particles[0],
            ParticleSetting::Color { name, argb } if name == ENTITY_EFFECT && *argb == expected as i32
        ));
    }

    #[test]
    fn beacon_effects_are_ambient_and_faint() {
        let mut effects = ActiveEffects::default();
        apply(&mut effects, EffectKind::Speed, true, true);
        let (particles, ambient) = synchronized(&effects);
        assert!(ambient);
        assert!(matches!(
            &particles[0],
            ParticleSetting::Color { argb, .. } if (*argb as u32) >> 24 == 38
        ));
    }
}
