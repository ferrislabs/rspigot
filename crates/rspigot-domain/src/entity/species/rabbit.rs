use std::borrow::Cow;

use rspigot_nbt::Compound;

use crate::BiomeId;
use crate::biome;
use crate::effect::{AttributeModifier, AttributeOperation};
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::{ARMOR, ATTACK_DAMAGE, LivingAttributes, LivingState};
use crate::entity::spawn::{SpawnEnvironment, ageable_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;
use crate::text::Component;

pub const DATA_TYPE: u8 = 18;
pub const EVIL_MODIFIER: &str = "minecraft:evil";
pub const EVIL_ATTACK_BONUS: f64 = 5.0;
pub const EVIL_ARMOR: f64 = 8.0;
pub const KILLER_BUNNY_NAME: &str = "entity.minecraft.killer_bunny";
pub const MORE_CARROT_TICKS: i32 = 40;
const WHITE_RABBITS: &str = "minecraft:spawns_white_rabbits";
const GOLD_RABBITS: &str = "minecraft:spawns_gold_rabbits";
const NEW_VARIANT_ONE_IN: i32 = 20;
const BABY_CHANCE: f32 = 1.0;

pub const KEYS: &[&str] = &["RabbitType", "MoreCarrotTicks"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Brown,
    White,
    Black,
    WhiteSplotched,
    Gold,
    Salt,
    Evil,
}

impl Variant {
    pub const DEFAULT: Self = Self::Brown;

    pub const fn id(self) -> i32 {
        match self {
            Self::Brown => 0,
            Self::White => 1,
            Self::Black => 2,
            Self::WhiteSplotched => 3,
            Self::Gold => 4,
            Self::Salt => 5,
            Self::Evil => 99,
        }
    }

    pub const fn by_id(id: i32) -> Self {
        match id {
            1 => Self::White,
            2 => Self::Black,
            3 => Self::WhiteSplotched,
            4 => Self::Gold,
            5 => Self::Salt,
            99 => Self::Evil,
            _ => Self::Brown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RabbitState {
    pub more_carrot_ticks: i32,
}

impl RabbitState {
    pub fn wants_more_food(&self) -> bool {
        self.more_carrot_ticks <= 0
    }
}

pub fn define(base: &mut EntityBase) -> RabbitState {
    base.data
        .define(DATA_TYPE, SyncedValue::Int(Variant::DEFAULT.id()));
    RabbitState::default()
}

pub fn variant(base: &EntityBase) -> Variant {
    Variant::by_id(base.data.int(DATA_TYPE))
}

pub fn is_evil(base: &EntityBase) -> bool {
    variant(base) == Variant::Evil
}

pub fn set_variant(
    base: &mut EntityBase,
    attributes: &mut LivingAttributes,
    variant: Variant,
) -> bool {
    let evil = variant == Variant::Evil;
    if evil {
        attributes.set_base(ARMOR, EVIL_ARMOR);
        attributes.add_or_replace_modifier(
            ATTACK_DAMAGE,
            AttributeModifier {
                id: Cow::Borrowed(EVIL_MODIFIER),
                amount: EVIL_ATTACK_BONUS,
                operation: AttributeOperation::AddValue,
            },
        );
        if base.custom_name().is_none() {
            base.set_custom_name(Some(Component::translatable(KILLER_BUNNY_NAME, Vec::new())));
        }
    } else {
        attributes.remove_modifier(ATTACK_DAMAGE, EVIL_MODIFIER);
    }
    base.data.set(DATA_TYPE, SyncedValue::Int(variant.id()));
    evil
}

pub fn random_variant(biome_id: BiomeId, level: &mut impl RandomSource) -> Variant {
    let roll = level.next_int(100);
    if biome::in_tag(biome_id, WHITE_RABBITS) {
        if roll < 80 {
            Variant::White
        } else {
            Variant::WhiteSplotched
        }
    } else if biome::in_tag(biome_id, GOLD_RABBITS) {
        Variant::Gold
    } else if roll < 50 {
        Variant::Brown
    } else if roll < 90 {
        Variant::Salt
    } else {
        Variant::Black
    }
}

pub fn offspring_variant(
    fallback: Variant,
    parent: Variant,
    partner: Variant,
    own: &mut impl RandomSource,
) -> Variant {
    if own.next_int(NEW_VARIANT_ONE_IN) == 0 {
        return fallback;
    }
    if own.next_boolean() { partner } else { parent }
}

pub fn read(
    nbt: &Compound,
    base: &mut EntityBase,
    attributes: &mut LivingAttributes,
    state: &mut RabbitState,
) -> bool {
    let variant = nbt
        .get_i32("RabbitType")
        .map_or(Variant::DEFAULT, Variant::by_id);
    let evil = set_variant(base, attributes, variant);
    state.more_carrot_ticks = nbt.get_i32("MoreCarrotTicks").unwrap_or(0);
    evil
}

pub fn write(base: &EntityBase, state: &RabbitState, out: &mut Compound) {
    out.insert("RabbitType", variant(base).id());
    out.insert("MoreCarrotTicks", state.more_carrot_ticks);
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    let mut chosen = random_variant(environment.biome, level);
    let ageable = match group {
        Some(SpawnGroupData::Rabbit {
            group_size,
            variant,
        }) => {
            chosen = Variant::by_id(variant);
            SpawnGroupData::Ageable {
                group_size,
                should_spawn_baby: true,
                baby_chance: BABY_CHANCE,
            }
        }
        _ => SpawnGroupData::Ageable {
            group_size: 0,
            should_spawn_baby: true,
            baby_chance: BABY_CHANCE,
        },
    };
    set_variant(base, &mut living.attributes, chosen);
    match ageable_finalize(base, living, Some(ageable), level) {
        Some(SpawnGroupData::Ageable { group_size, .. }) => Some(SpawnGroupData::Rabbit {
            group_size,
            variant: chosen.id(),
        }),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::EntityBase;
    use crate::random::LegacyRandom;
    use rspigot_entities::EntityKind;

    fn rabbit() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Rabbit);
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    #[test]
    fn variant_ids_follow_the_registry_and_unknown_ids_fall_back_to_brown() {
        for variant in [
            Variant::Brown,
            Variant::White,
            Variant::Black,
            Variant::WhiteSplotched,
            Variant::Gold,
            Variant::Salt,
            Variant::Evil,
        ] {
            assert_eq!(Variant::by_id(variant.id()), variant);
        }
        assert_eq!(Variant::Evil.id(), 99);
        assert_eq!(Variant::by_id(6), Variant::Brown);
        assert_eq!(Variant::by_id(-1), Variant::Brown);
    }

    #[test]
    fn the_variant_is_synchronised_at_index_18() {
        let (mut base, mut living) = rabbit();
        assert_eq!(base.data.int(DATA_TYPE), 0);
        set_variant(&mut base, &mut living.attributes, Variant::Gold);
        assert_eq!(base.data.int(DATA_TYPE), 4);
        assert_eq!(variant(&base), Variant::Gold);
    }

    #[test]
    fn the_killer_bunny_is_armoured_named_and_hits_harder() {
        let (mut base, mut living) = rabbit();
        let before = living.attribute(ATTACK_DAMAGE);
        assert_eq!(before, 3.0);
        assert!(set_variant(
            &mut base,
            &mut living.attributes,
            Variant::Evil
        ));
        assert_eq!(living.attribute(ARMOR), 8.0);
        assert_eq!(living.attribute(ATTACK_DAMAGE), 8.0);
        assert_eq!(
            base.custom_name().map(Component::plain_text),
            Some(KILLER_BUNNY_NAME.to_owned())
        );
        assert!(!set_variant(
            &mut base,
            &mut living.attributes,
            Variant::Brown
        ));
        assert_eq!(living.attribute(ATTACK_DAMAGE), 3.0);
        assert_eq!(living.attribute(ARMOR), 8.0);
    }

    #[test]
    fn a_named_killer_bunny_keeps_its_name() {
        let (mut base, mut living) = rabbit();
        base.set_custom_name(Some(Component::text("Caerbannog")));
        set_variant(&mut base, &mut living.attributes, Variant::Evil);
        assert_eq!(
            base.custom_name().map(Component::plain_text),
            Some("Caerbannog".to_owned())
        );
    }

    #[test]
    fn nbt_round_trips_the_variant_and_the_carrot_timer() {
        let (mut base, mut living) = rabbit();
        let mut nbt = Compound::new();
        nbt.insert("RabbitType", 5);
        nbt.insert("MoreCarrotTicks", 17);
        let mut state = RabbitState::default();
        assert!(!read(&nbt, &mut base, &mut living.attributes, &mut state));
        assert_eq!(variant(&base), Variant::Salt);
        assert_eq!(state.more_carrot_ticks, 17);
        let mut out = Compound::new();
        write(&base, &state, &mut out);
        assert_eq!(out.get_i32("RabbitType"), Some(5));
        assert_eq!(out.get_i32("MoreCarrotTicks"), Some(17));
    }

    #[test]
    fn reading_nothing_gives_a_brown_rabbit_that_wants_carrots() {
        let (mut base, mut living) = rabbit();
        let mut state = RabbitState {
            more_carrot_ticks: 9,
        };
        read(
            &Compound::new(),
            &mut base,
            &mut living.attributes,
            &mut state,
        );
        assert_eq!(variant(&base), Variant::Brown);
        assert!(state.wants_more_food());
    }

    #[test]
    fn reading_the_killer_variant_reports_it() {
        let (mut base, mut living) = rabbit();
        let mut nbt = Compound::new();
        nbt.insert("RabbitType", 99);
        let mut state = RabbitState::default();
        assert!(read(&nbt, &mut base, &mut living.attributes, &mut state));
        assert!(is_evil(&base));
        assert_eq!(living.attribute(ATTACK_DAMAGE), 8.0);
    }

    fn biome(name: &str) -> BiomeId {
        BiomeId(rspigot_data::registries::biome_id(name).unwrap())
    }

    #[test]
    fn biomes_choose_their_rabbits() {
        let mut level = LegacyRandom::new(1);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            seen.insert(random_variant(biome("minecraft:snowy_plains"), &mut level).id());
        }
        assert_eq!(seen, [1, 3].into_iter().collect());
        for _ in 0..50 {
            assert_eq!(
                random_variant(biome("minecraft:desert"), &mut level),
                Variant::Gold
            );
        }
        let mut seen = std::collections::HashSet::new();
        for _ in 0..400 {
            seen.insert(random_variant(biome("minecraft:plains"), &mut level).id());
        }
        assert_eq!(seen, [0, 2, 5].into_iter().collect());
    }

    struct Fixed(i32);

    impl RandomSource for Fixed {
        fn next_float(&mut self) -> f32 {
            0.0
        }
        fn next_int(&mut self, _bound: i32) -> i32 {
            self.0
        }
        fn next_double(&mut self) -> f64 {
            0.0
        }
        fn next_boolean(&mut self) -> bool {
            self.0 == 1
        }
        fn next_gaussian(&mut self) -> f64 {
            0.0
        }
    }

    #[test]
    fn the_roll_decides_the_biome_variant_cut_offs() {
        let plains = biome("minecraft:plains");
        assert_eq!(random_variant(plains, &mut Fixed(49)), Variant::Brown);
        assert_eq!(random_variant(plains, &mut Fixed(50)), Variant::Salt);
        assert_eq!(random_variant(plains, &mut Fixed(89)), Variant::Salt);
        assert_eq!(random_variant(plains, &mut Fixed(90)), Variant::Black);
        let snowy = biome("minecraft:snowy_plains");
        assert_eq!(random_variant(snowy, &mut Fixed(79)), Variant::White);
        assert_eq!(
            random_variant(snowy, &mut Fixed(80)),
            Variant::WhiteSplotched
        );
    }

    #[test]
    fn offspring_inherit_a_parent_or_rarely_a_fresh_variant() {
        let fresh = Variant::Gold;
        assert_eq!(
            offspring_variant(fresh, Variant::Brown, Variant::White, &mut Fixed(0)),
            fresh
        );
        assert_eq!(
            offspring_variant(fresh, Variant::Brown, Variant::White, &mut Fixed(1)),
            Variant::White
        );
        assert_eq!(
            offspring_variant(fresh, Variant::Brown, Variant::White, &mut Fixed(2)),
            Variant::Brown
        );
    }

    #[test]
    fn a_pack_shares_its_variant_and_every_follower_is_a_baby() {
        let environment = SpawnEnvironment {
            biome: biome("minecraft:plains"),
            difficulty: crate::difficulty::DifficultyInstance::new(
                crate::world::Difficulty::Normal,
                0,
                0,
                1.0,
            ),
            halloween: false,
            moon_brightness: 0.0,
        };
        let mut level = LegacyRandom::new(7);
        let (mut first_base, mut first) = rabbit();
        let group = finalize(&mut first_base, &mut first, &environment, None, &mut level);
        let Some(SpawnGroupData::Rabbit {
            group_size,
            variant,
        }) = group
        else {
            panic!("expected a rabbit group");
        };
        assert_eq!(group_size, 1);
        assert_eq!(first_base.data.int(DATA_TYPE), variant);
        assert!(!first_base.data.boolean(crate::entity::ageable::DATA_BABY));
        let (mut second_base, mut second) = rabbit();
        let group = finalize(
            &mut second_base,
            &mut second,
            &environment,
            group,
            &mut level,
        );
        assert_eq!(second_base.data.int(DATA_TYPE), variant);
        assert!(second_base.data.boolean(crate::entity::ageable::DATA_BABY));
        assert!(matches!(
            group,
            Some(SpawnGroupData::Rabbit { group_size: 2, .. })
        ));
    }
}
