use rspigot_effects::EffectKind;
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::random::RandomSource;

pub const DATA_TYPE: u8 = 18;
pub const KEYS: &[&str] = &["Type", "stew_effects"];
pub const MUSHROOMS_PER_SHEARING: u8 = 5;
pub const SMALL_FLOWERS: &str = "minecraft:small_flowers";
pub const LIGHTNING_CONVERT_SOUND: &str = "minecraft:entity.mooshroom.convert";
pub const SHEAR_SOUND: &str = "minecraft:entity.mooshroom.shear";
pub const EAT_SOUND: &str = "minecraft:entity.mooshroom.eat";
pub const MILK_SOUND: &str = "minecraft:entity.mooshroom.milk";
pub const MILK_SUSPICIOUSLY_SOUND: &str = "minecraft:entity.mooshroom.suspicious_milk";
pub const DEFAULT_STEW_DURATION: i32 = 160;
pub const OFFSPRING_MUTATION_BOUND: i32 = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Variant {
    #[default]
    Red,
    Brown,
}

impl Variant {
    pub const fn id(self) -> i32 {
        match self {
            Self::Red => 0,
            Self::Brown => 1,
        }
    }

    pub const fn from_id(id: i32) -> Self {
        match id {
            1 => Self::Brown,
            _ => Self::Red,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Brown => "brown",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "red" => Some(Self::Red),
            "brown" => Some(Self::Brown),
            _ => None,
        }
    }

    pub const fn mushroom(self) -> &'static str {
        match self {
            Self::Red => "minecraft:red_mushroom",
            Self::Brown => "minecraft:brown_mushroom",
        }
    }

    pub const fn toggled(self) -> Self {
        match self {
            Self::Red => Self::Brown,
            Self::Brown => Self::Red,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StewEffect {
    pub effect: EffectKind,
    pub duration: i32,
}

impl StewEffect {
    pub fn named(name: &str, duration: i32) -> Option<Self> {
        Some(Self {
            effect: EffectKind::from_name(name)?,
            duration,
        })
    }

    pub const fn name(self) -> &'static str {
        self.effect.name()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MooshroomState {
    pub stew: Option<StewEffect>,
    pub last_bolt: Option<Uuid>,
}

pub fn define(base: &mut EntityBase) -> MooshroomState {
    base.data
        .define(DATA_TYPE, SyncedValue::Int(Variant::Red.id()));
    MooshroomState::default()
}

pub fn variant(base: &EntityBase) -> Variant {
    Variant::from_id(base.data.int(DATA_TYPE))
}

pub fn set_variant(base: &mut EntityBase, variant: Variant) {
    base.data.set(DATA_TYPE, SyncedValue::Int(variant.id()));
}

pub fn offspring_variant(
    own: Variant,
    partner: Variant,
    random: &mut impl RandomSource,
) -> Variant {
    if own == partner && random.next_int(OFFSPRING_MUTATION_BOUND) == 0 {
        return own.toggled();
    }
    if random.next_boolean() { own } else { partner }
}

pub fn struck_by_lightning(base: &mut EntityBase, state: &mut MooshroomState, bolt: Uuid) -> bool {
    if state.last_bolt == Some(bolt) {
        return false;
    }
    set_variant(base, variant(base).toggled());
    state.last_bolt = Some(bolt);
    true
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut MooshroomState) {
    if let Some(variant) = nbt.get_str("Type").and_then(Variant::from_name) {
        set_variant(base, variant);
    }
    state.stew = nbt.get_list("stew_effects").and_then(|list| {
        list.iter()
            .filter_map(Tag::as_compound)
            .find_map(read_effect)
    });
}

fn read_effect(entry: &Compound) -> Option<StewEffect> {
    Some(StewEffect {
        effect: EffectKind::from_name(entry.get_str("id")?)?,
        duration: entry.get_i32("duration").unwrap_or(DEFAULT_STEW_DURATION),
    })
}

pub fn write(base: &EntityBase, state: &MooshroomState, out: &mut Compound) {
    out.insert("Type", variant(base).name());
    if let Some(entry) = state.stew {
        out.insert(
            "stew_effects",
            Tag::List(vec![Tag::Compound(
                Compound::new()
                    .with("id", entry.effect.name())
                    .with("duration", entry.duration),
            )]),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityId;
    use crate::random::LegacyRandom;
    use rspigot_entities::EntityKind;

    fn mooshroom() -> (EntityBase, MooshroomState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Mooshroom);
        let state = define(&mut base);
        (base, state)
    }

    #[test]
    fn a_new_mooshroom_is_red() {
        let (base, _) = mooshroom();
        assert_eq!(variant(&base), Variant::Red);
    }

    #[test]
    fn the_type_and_stew_survive_a_save() {
        let (mut base, mut state) = mooshroom();
        set_variant(&mut base, Variant::Brown);
        state.stew = Some(StewEffect {
            effect: EffectKind::NightVision,
            duration: 100,
        });
        let mut saved = Compound::new();
        write(&base, &state, &mut saved);
        assert_eq!(saved.get_str("Type"), Some("brown"));
        let (mut loaded, mut loaded_state) = mooshroom();
        read(&saved, &mut loaded, &mut loaded_state);
        assert_eq!(variant(&loaded), Variant::Brown);
        assert_eq!(loaded_state, state);
    }

    #[test]
    fn a_bolt_toggles_the_colour_once() {
        let (mut base, mut state) = mooshroom();
        let bolt = Uuid::from_u128(5);
        assert!(struck_by_lightning(&mut base, &mut state, bolt));
        assert_eq!(variant(&base), Variant::Brown);
        assert!(!struck_by_lightning(&mut base, &mut state, bolt));
        assert_eq!(variant(&base), Variant::Brown);
        assert!(struck_by_lightning(
            &mut base,
            &mut state,
            Uuid::from_u128(6)
        ));
        assert_eq!(variant(&base), Variant::Red);
    }

    #[test]
    fn parents_of_different_colours_pass_one_of_them_on() {
        let mut random = LegacyRandom::new(7);
        for _ in 0..50 {
            let child = offspring_variant(Variant::Red, Variant::Brown, &mut random);
            assert!(matches!(child, Variant::Red | Variant::Brown));
        }
    }

    #[test]
    fn same_coloured_parents_rarely_mutate() {
        let mut random = LegacyRandom::new(7);
        let mutated = (0..20_000)
            .filter(|_| {
                offspring_variant(Variant::Red, Variant::Red, &mut random) == Variant::Brown
            })
            .count();
        assert!((1..100).contains(&mutated), "{mutated}");
    }
}
