use crate::kind::NAMES;
use crate::{EffectId, EffectKind};

macro_rules! effect_kinds {
    (effects: $effects:literal, kinds: [$($kind:ident),* $(,)?] $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u16)]
        pub enum EffectKind {
            $($kind,)*
        }

        impl EffectKind {
            pub const ALL: [Self; $effects] = [$(Self::$kind,)*];

            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$kind => <$crate::effects::$kind as $crate::MobEffect>::NAME,)*
                }
            }

            pub const fn translation_key(self) -> &'static str {
                match self {
                    $(Self::$kind => <$crate::effects::$kind as $crate::MobEffect>::TRANSLATION_KEY,)*
                }
            }

            pub const fn id(self) -> $crate::EffectId {
                match self {
                    $(Self::$kind => <$crate::effects::$kind as $crate::MobEffect>::ID,)*
                }
            }

            pub const fn category(self) -> $crate::EffectCategory {
                match self {
                    $(Self::$kind => <$crate::effects::$kind as $crate::MobEffect>::CATEGORY,)*
                }
            }

            pub const fn color(self) -> u32 {
                match self {
                    $(Self::$kind => <$crate::effects::$kind as $crate::MobEffect>::COLOR,)*
                }
            }

            pub const fn instantaneous(self) -> bool {
                match self {
                    $(Self::$kind => <$crate::effects::$kind as $crate::MobEffect>::INSTANTANEOUS,)*
                }
            }

            pub const fn modifiers(self) -> &'static [$crate::AttributeTemplate] {
                match self {
                    $(Self::$kind => <$crate::effects::$kind as $crate::MobEffect>::MODIFIERS,)*
                }
            }

            pub const fn reaction(self) -> $crate::Reaction {
                match self {
                    $(Self::$kind => <<$crate::effects::$kind as $crate::MobEffect>::Behavior as $crate::EffectBehavior>::REACTION,)*
                }
            }

            pub fn should_tick(self, duration: i32, amplifier: u8) -> bool {
                match self {
                    $(Self::$kind => <<$crate::effects::$kind as $crate::MobEffect>::Behavior as $crate::EffectBehavior>::should_tick(duration, amplifier),)*
                }
            }
        }

        const _: () = {
            let mut index = 0;
            while index < EffectKind::ALL.len() {
                assert!(EffectKind::ALL[index].id().0 as usize == index);
                index += 1;
            }
        };
    };
}

pub(crate) use effect_kinds;

impl EffectKind {
    pub const fn silverfish_on_hurt(self) -> Option<(f32, i32, i32)> {
        match self.reaction() {
            crate::Reaction::SilverfishOnHurt { chance, min, max } => Some((chance, min, max)),
            _ => None,
        }
    }

    pub const fn slimes_on_death(self) -> Option<(i32, i32)> {
        match self.reaction() {
            crate::Reaction::SlimesOnDeath { count, size } => Some((count, size)),
            _ => None,
        }
    }

    pub const fn cobwebs_on_death(self) -> Option<(i32, i32)> {
        match self.reaction() {
            crate::Reaction::CobwebsOnDeath { min, max } => Some((min, max)),
            _ => None,
        }
    }

    pub const fn burst_on_death(self) -> Option<f32> {
        match self.reaction() {
            crate::Reaction::BurstOnDeath { radius } => Some(radius),
            _ => None,
        }
    }

    pub const fn keeps_air(self) -> bool {
        matches!(self.reaction(), crate::Reaction::KeepsAir)
    }

    pub fn from_id(id: EffectId) -> Option<Self> {
        Self::ALL.get(usize::from(id.0)).copied()
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let path = name.strip_prefix("minecraft:").unwrap_or(name);
        NAMES
            .binary_search_by_key(&path, |&(candidate, _)| candidate)
            .ok()
            .map(|index| NAMES[index].1)
    }
}

impl EffectId {
    pub fn from_name(name: &str) -> Option<Self> {
        EffectKind::from_name(name).map(EffectKind::id)
    }

    pub fn kind(self) -> Option<EffectKind> {
        EffectKind::from_id(self)
    }

    pub fn name(self) -> Option<&'static str> {
        self.kind().map(EffectKind::name)
    }

    pub fn translation_key(self) -> Option<String> {
        self.kind().map(|kind| kind.translation_key().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_table_is_sorted_and_complete() {
        assert_eq!(NAMES.len(), EffectKind::ALL.len());
        assert!(NAMES.windows(2).all(|pair| pair[0].0 < pair[1].0));
        for (path, kind) in NAMES {
            assert_eq!(kind.name().strip_prefix("minecraft:"), Some(path));
        }
    }
}
