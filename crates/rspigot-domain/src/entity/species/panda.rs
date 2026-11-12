use rspigot_nbt::Compound;

use crate::Vec3;
use crate::attribute::MOVEMENT_SPEED;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living::{self, LivingState};
use crate::entity::spawn::{SpawnEnvironment, ageable_finalize};
use crate::random::RandomSource;
use crate::spawn::SpawnGroupData;

pub const DATA_UNHAPPY_COUNTER: u8 = 18;
pub const DATA_SNEEZE_COUNTER: u8 = 19;
pub const DATA_EAT_COUNTER: u8 = 20;
pub const DATA_MAIN_GENE: u8 = 21;
pub const DATA_HIDDEN_GENE: u8 = 22;
pub const DATA_FLAGS: u8 = 23;
pub const FLAG_SNEEZE: u8 = 2;
pub const FLAG_ROLL: u8 = 4;
pub const FLAG_SIT: u8 = 8;
pub const FLAG_ON_BACK: u8 = 16;
pub const TOTAL_ROLL_STEPS: i32 = 32;
pub const TOTAL_UNHAPPY_TIME: i32 = 32;
pub const SNEEZE_TICKS: i32 = 20;
pub const EAT_TICK_INTERVAL: i32 = 5;
pub const WEAK_MAX_HEALTH: f64 = 10.0;
pub const LAZY_MOVEMENT_SPEED: f64 = 0.07f32 as f64;
pub const KEYS: &[&str] = &["MainGene", "HiddenGene"];
const BREED_MUTATION_ROLL: i32 = 32;
const BABY_CHANCE: f32 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Gene {
    #[default]
    Normal,
    Lazy,
    Worried,
    Playful,
    Brown,
    Weak,
    Aggressive,
}

impl Gene {
    pub const ALL: [Self; 7] = [
        Self::Normal,
        Self::Lazy,
        Self::Worried,
        Self::Playful,
        Self::Brown,
        Self::Weak,
        Self::Aggressive,
    ];

    pub const fn id(self) -> u8 {
        self as u8
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Lazy => "lazy",
            Self::Worried => "worried",
            Self::Playful => "playful",
            Self::Brown => "brown",
            Self::Weak => "weak",
            Self::Aggressive => "aggressive",
        }
    }

    pub const fn is_recessive(self) -> bool {
        matches!(self, Self::Brown | Self::Weak)
    }

    pub fn by_id(id: i32) -> Self {
        usize::try_from(id)
            .ok()
            .and_then(|index| Self::ALL.get(index).copied())
            .unwrap_or(Self::Normal)
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|gene| gene.name() == name)
    }

    pub fn random(random: &mut impl RandomSource) -> Self {
        match random.next_int(16) {
            0 => Self::Lazy,
            1 => Self::Worried,
            2 => Self::Playful,
            4 => Self::Aggressive,
            i if i < 9 => Self::Weak,
            i if i < 11 => Self::Brown,
            _ => Self::Normal,
        }
    }

    pub fn variant_from_genes(main: Self, hidden: Self) -> Self {
        if main.is_recessive() {
            if main == hidden { main } else { Self::Normal }
        } else {
            main
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PandaState {
    pub got_bamboo: bool,
    pub did_bite: bool,
    pub roll_counter: i32,
    roll_delta: (u64, u64),
}

impl PandaState {
    pub fn roll_delta(&self) -> (f64, f64) {
        (
            f64::from_bits(self.roll_delta.0),
            f64::from_bits(self.roll_delta.1),
        )
    }

    pub fn set_roll_delta(&mut self, delta: Vec3) {
        self.roll_delta = (delta.x.to_bits(), delta.z.to_bits());
    }
}

pub fn define(base: &mut EntityBase) -> PandaState {
    base.data.define(DATA_UNHAPPY_COUNTER, SyncedValue::Int(0));
    base.data.define(DATA_SNEEZE_COUNTER, SyncedValue::Int(0));
    base.data.define(DATA_EAT_COUNTER, SyncedValue::Int(0));
    base.data.define(DATA_MAIN_GENE, SyncedValue::Byte(0));
    base.data.define(DATA_HIDDEN_GENE, SyncedValue::Byte(0));
    base.data.define(DATA_FLAGS, SyncedValue::Byte(0));
    PandaState::default()
}

pub fn unhappy_counter(base: &EntityBase) -> i32 {
    base.data.int(DATA_UNHAPPY_COUNTER)
}

pub fn set_unhappy_counter(base: &mut EntityBase, value: i32) {
    base.data.set(DATA_UNHAPPY_COUNTER, SyncedValue::Int(value));
}

pub fn sneeze_counter(base: &EntityBase) -> i32 {
    base.data.int(DATA_SNEEZE_COUNTER)
}

pub fn set_sneeze_counter(base: &mut EntityBase, value: i32) {
    base.data.set(DATA_SNEEZE_COUNTER, SyncedValue::Int(value));
}

pub fn eat_counter(base: &EntityBase) -> i32 {
    base.data.int(DATA_EAT_COUNTER)
}

pub fn set_eat_counter(base: &mut EntityBase, value: i32) {
    base.data.set(DATA_EAT_COUNTER, SyncedValue::Int(value));
}

pub fn is_eating(base: &EntityBase) -> bool {
    eat_counter(base) > 0
}

pub fn eat(base: &mut EntityBase, value: bool) {
    set_eat_counter(base, i32::from(value));
}

pub fn main_gene(base: &EntityBase) -> Gene {
    Gene::by_id(i32::from(base.data.byte(DATA_MAIN_GENE)))
}

pub fn hidden_gene(base: &EntityBase) -> Gene {
    Gene::by_id(i32::from(base.data.byte(DATA_HIDDEN_GENE)))
}

pub fn set_main_gene(base: &mut EntityBase, gene: Gene) {
    base.data
        .set(DATA_MAIN_GENE, SyncedValue::Byte(gene.id() as i8));
}

pub fn set_hidden_gene(base: &mut EntityBase, gene: Gene) {
    base.data
        .set(DATA_HIDDEN_GENE, SyncedValue::Byte(gene.id() as i8));
}

pub fn variant(base: &EntityBase) -> Gene {
    Gene::variant_from_genes(main_gene(base), hidden_gene(base))
}

pub fn is_lazy(base: &EntityBase) -> bool {
    variant(base) == Gene::Lazy
}

pub fn is_worried(base: &EntityBase) -> bool {
    variant(base) == Gene::Worried
}

pub fn is_playful(base: &EntityBase) -> bool {
    variant(base) == Gene::Playful
}

pub fn is_brown(base: &EntityBase) -> bool {
    variant(base) == Gene::Brown
}

pub fn is_weak(base: &EntityBase) -> bool {
    variant(base) == Gene::Weak
}

pub fn is_aggressive(base: &EntityBase) -> bool {
    variant(base) == Gene::Aggressive
}

fn flag(base: &EntityBase, mask: u8) -> bool {
    base.data.flag(DATA_FLAGS, mask)
}

fn set_flag(base: &mut EntityBase, mask: u8, value: bool) {
    base.data.set_flag(DATA_FLAGS, mask, value);
}

pub fn is_sneezing(base: &EntityBase) -> bool {
    flag(base, FLAG_SNEEZE)
}

pub fn sneeze(base: &mut EntityBase, value: bool) {
    set_flag(base, FLAG_SNEEZE, value);
    if !value {
        set_sneeze_counter(base, 0);
    }
}

pub fn is_rolling(base: &EntityBase) -> bool {
    flag(base, FLAG_ROLL)
}

pub fn roll(base: &mut EntityBase, value: bool) {
    set_flag(base, FLAG_ROLL, value);
}

pub fn is_sitting(base: &EntityBase) -> bool {
    flag(base, FLAG_SIT)
}

pub fn sit(base: &mut EntityBase, value: bool) {
    set_flag(base, FLAG_SIT, value);
}

pub fn is_on_back(base: &EntityBase) -> bool {
    flag(base, FLAG_ON_BACK)
}

pub fn set_on_back(base: &mut EntityBase, value: bool) {
    set_flag(base, FLAG_ON_BACK, value);
}

pub fn is_scared(base: &EntityBase, thundering: bool) -> bool {
    is_worried(base) && thundering
}

pub fn can_perform_action(base: &EntityBase, thundering: bool) -> bool {
    !is_on_back(base)
        && !is_scared(base, thundering)
        && !is_eating(base)
        && !is_rolling(base)
        && !is_sitting(base)
}

fn one_of_genes_randomly(genes: (Gene, Gene), random: &mut impl RandomSource) -> Gene {
    if random.next_boolean() {
        genes.0
    } else {
        genes.1
    }
}

pub fn set_gene_from_parents(
    base: &mut EntityBase,
    parent1: (Gene, Gene),
    parent2: Option<(Gene, Gene)>,
    random: &mut impl RandomSource,
) {
    match parent2 {
        None => {
            if random.next_boolean() {
                set_main_gene(base, one_of_genes_randomly(parent1, random));
                set_hidden_gene(base, Gene::random(random));
            } else {
                set_main_gene(base, Gene::random(random));
                set_hidden_gene(base, one_of_genes_randomly(parent1, random));
            }
        }
        Some(parent2) => {
            if random.next_boolean() {
                set_main_gene(base, one_of_genes_randomly(parent1, random));
                set_hidden_gene(base, one_of_genes_randomly(parent2, random));
            } else {
                set_main_gene(base, one_of_genes_randomly(parent2, random));
                set_hidden_gene(base, one_of_genes_randomly(parent1, random));
            }
        }
    }
    if random.next_int(BREED_MUTATION_ROLL) == 0 {
        set_main_gene(base, Gene::random(random));
    }
    if random.next_int(BREED_MUTATION_ROLL) == 0 {
        set_hidden_gene(base, Gene::random(random));
    }
}

pub fn genes(base: &EntityBase) -> (Gene, Gene) {
    (main_gene(base), hidden_gene(base))
}

pub fn set_attributes(base: &mut EntityBase, living: &mut LivingState) {
    if is_weak(base) {
        living
            .attributes
            .set_base(living::MAX_HEALTH, WEAK_MAX_HEALTH);
        let health = living::health(base);
        living::set_health(base, living, health);
    }
    if is_lazy(base) {
        living
            .attributes
            .set_base(MOVEMENT_SPEED, LAZY_MOVEMENT_SPEED);
    }
}

pub fn ambient_sound(base: &EntityBase) -> &'static str {
    match variant(base) {
        Gene::Aggressive => "minecraft:entity.panda.aggressive_ambient",
        Gene::Worried => "minecraft:entity.panda.worried_ambient",
        _ => "minecraft:entity.panda.ambient",
    }
}

pub fn read(nbt: &Compound, base: &mut EntityBase, _state: &mut PandaState) {
    let gene = |key: &str| {
        nbt.get_str(key)
            .and_then(Gene::from_name)
            .unwrap_or(Gene::Normal)
    };
    set_main_gene(base, gene("MainGene"));
    set_hidden_gene(base, gene("HiddenGene"));
}

pub fn write(base: &EntityBase, _state: &PandaState, out: &mut Compound) {
    out.insert("MainGene", main_gene(base).name());
    out.insert("HiddenGene", hidden_gene(base).name());
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    _environment: &SpawnEnvironment,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    set_main_gene(base, Gene::random(level));
    set_hidden_gene(base, Gene::random(level));
    set_attributes(base, living);
    let group = group.unwrap_or(SpawnGroupData::Ageable {
        group_size: 0,
        should_spawn_baby: true,
        baby_chance: BABY_CHANCE,
    });
    ageable_finalize(base, living, Some(group), level)
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::difficulty::DifficultyInstance;
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::species::Species;
    use crate::entity::{Entity, EntityState};
    use crate::world::Difficulty;

    struct Script(Vec<i32>);

    impl RandomSource for Script {
        fn next_float(&mut self) -> f32 {
            0.0
        }

        fn next_int(&mut self, _bound: i32) -> i32 {
            self.0.remove(0)
        }

        fn next_double(&mut self) -> f64 {
            0.0
        }

        fn next_boolean(&mut self) -> bool {
            self.0.remove(0) != 0
        }

        fn next_gaussian(&mut self) -> f64 {
            0.0
        }
    }

    fn panda() -> Entity {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Panda);
        let living = LivingState::new(&mut base, 0.0);
        Entity::new(base, EntityState::Living(Box::new(living)))
    }

    fn living_mut(entity: &mut Entity) -> (&mut EntityBase, &mut LivingState) {
        let EntityState::Living(state) = &mut entity.state else {
            panic!("living");
        };
        (&mut entity.base, state)
    }

    #[test]
    fn genes_map_ids_names_and_dominance() {
        for gene in Gene::ALL {
            assert_eq!(Gene::by_id(i32::from(gene.id())), gene);
            assert_eq!(Gene::from_name(gene.name()), Some(gene));
        }
        assert_eq!(Gene::by_id(7), Gene::Normal);
        assert_eq!(Gene::by_id(-1), Gene::Normal);
        assert_eq!(Gene::from_name("giant"), None);
        assert!(Gene::Brown.is_recessive() && Gene::Weak.is_recessive());
        assert!(!Gene::Lazy.is_recessive() && !Gene::Aggressive.is_recessive());
    }

    #[test]
    fn recessive_main_gene_needs_the_same_hidden_gene() {
        assert_eq!(
            Gene::variant_from_genes(Gene::Brown, Gene::Brown),
            Gene::Brown
        );
        assert_eq!(
            Gene::variant_from_genes(Gene::Brown, Gene::Weak),
            Gene::Normal
        );
        assert_eq!(
            Gene::variant_from_genes(Gene::Weak, Gene::Normal),
            Gene::Normal
        );
        assert_eq!(
            Gene::variant_from_genes(Gene::Lazy, Gene::Brown),
            Gene::Lazy
        );
        assert_eq!(
            Gene::variant_from_genes(Gene::Normal, Gene::Weak),
            Gene::Normal
        );
    }

    #[test]
    fn random_gene_follows_the_sixteen_way_table() {
        let expected = [
            Gene::Lazy,
            Gene::Worried,
            Gene::Playful,
            Gene::Weak,
            Gene::Aggressive,
            Gene::Weak,
            Gene::Weak,
            Gene::Weak,
            Gene::Weak,
            Gene::Brown,
            Gene::Brown,
            Gene::Normal,
            Gene::Normal,
            Gene::Normal,
            Gene::Normal,
            Gene::Normal,
        ];
        for (roll, gene) in expected.into_iter().enumerate() {
            assert_eq!(Gene::random(&mut Script(vec![roll as i32])), gene, "{roll}");
        }
    }

    #[test]
    fn synced_data_uses_the_vanilla_indices_and_defaults() {
        let entity = panda();
        let data = &entity.base.data;
        for (index, value) in [
            (18, SyncedValue::Int(0)),
            (19, SyncedValue::Int(0)),
            (20, SyncedValue::Int(0)),
            (21, SyncedValue::Byte(0)),
            (22, SyncedValue::Byte(0)),
            (23, SyncedValue::Byte(0)),
        ] {
            assert_eq!(data.get(index), Some(&value), "{index}");
        }
        assert_eq!(data.get(24), None);
    }

    #[test]
    fn new_pandas_can_pick_up_loot() {
        let entity = panda();
        let mob = entity.living().and_then(|l| l.mob.as_ref()).unwrap();
        assert!(mob.can_pick_up_loot);
        assert!(matches!(mob.species, Species::Panda(_)));
    }

    #[test]
    fn flags_share_one_byte_and_sneezing_off_resets_the_counter() {
        let mut entity = panda();
        let base = &mut entity.base;
        sit(base, true);
        roll(base, true);
        set_on_back(base, true);
        sneeze(base, true);
        assert_eq!(base.data.byte(DATA_FLAGS), 2 | 4 | 8 | 16);
        set_sneeze_counter(base, 7);
        sneeze(base, false);
        assert_eq!(sneeze_counter(base), 0);
        assert!(!is_sneezing(base));
        assert!(is_sitting(base) && is_rolling(base) && is_on_back(base));
        sit(base, false);
        assert!(!is_sitting(base));
        assert_eq!(base.data.byte(DATA_FLAGS), 4 | 16);
    }

    #[test]
    fn eating_is_a_positive_counter() {
        let mut entity = panda();
        let base = &mut entity.base;
        assert!(!is_eating(base));
        eat(base, true);
        assert_eq!(eat_counter(base), 1);
        assert!(is_eating(base));
        set_eat_counter(base, 40);
        eat(base, false);
        assert_eq!(eat_counter(base), 0);
    }

    #[test]
    fn can_perform_action_needs_every_blocker_clear() {
        let mut entity = panda();
        let base = &mut entity.base;
        assert!(can_perform_action(base, true));
        for blocked in [
            |b: &mut EntityBase| sit(b, true),
            |b: &mut EntityBase| roll(b, true),
            |b: &mut EntityBase| set_on_back(b, true),
            |b: &mut EntityBase| eat(b, true),
        ] {
            let mut other = panda();
            blocked(&mut other.base);
            assert!(!can_perform_action(&other.base, false));
        }
        set_main_gene(base, Gene::Worried);
        assert!(can_perform_action(base, false));
        assert!(!can_perform_action(base, true));
        assert!(is_scared(base, true) && !is_scared(base, false));
    }

    #[test]
    fn variant_predicates_follow_the_genes() {
        let mut entity = panda();
        let base = &mut entity.base;
        set_main_gene(base, Gene::Brown);
        set_hidden_gene(base, Gene::Brown);
        assert!(is_brown(base) && !is_weak(base));
        set_hidden_gene(base, Gene::Weak);
        assert_eq!(variant(base), Gene::Normal);
        set_main_gene(base, Gene::Aggressive);
        assert!(is_aggressive(base));
        set_main_gene(base, Gene::Playful);
        assert!(is_playful(base));
        set_main_gene(base, Gene::Worried);
        assert!(is_worried(base));
        set_main_gene(base, Gene::Lazy);
        assert!(is_lazy(base));
    }

    #[test]
    fn two_parents_pick_a_side_then_one_gene_each() {
        let mut entity = panda();
        let base = &mut entity.base;
        let a = (Gene::Lazy, Gene::Brown);
        let b = (Gene::Worried, Gene::Weak);
        set_gene_from_parents(base, a, Some(b), &mut Script(vec![1, 1, 0, 1, 1]));
        assert_eq!(genes(base), (Gene::Lazy, Gene::Weak));
        set_gene_from_parents(base, a, Some(b), &mut Script(vec![0, 0, 1, 1, 1]));
        assert_eq!(genes(base), (Gene::Weak, Gene::Lazy));
    }

    #[test]
    fn single_parent_randomizes_the_other_gene() {
        let mut entity = panda();
        let base = &mut entity.base;
        let a = (Gene::Lazy, Gene::Brown);
        set_gene_from_parents(base, a, None, &mut Script(vec![1, 1, 2, 1, 1]));
        assert_eq!(genes(base), (Gene::Lazy, Gene::Playful));
        set_gene_from_parents(base, a, None, &mut Script(vec![0, 3, 0, 1, 1]));
        assert_eq!(genes(base), (Gene::Weak, Gene::Brown));
    }

    #[test]
    fn mutation_rolls_replace_genes_independently() {
        let mut entity = panda();
        let base = &mut entity.base;
        let a = (Gene::Lazy, Gene::Lazy);
        set_gene_from_parents(base, a, Some(a), &mut Script(vec![1, 1, 1, 0, 4, 0, 2]));
        assert_eq!(genes(base), (Gene::Aggressive, Gene::Playful));
    }

    #[test]
    fn weak_pandas_get_ten_health_and_lazy_ones_slow_down() {
        let mut entity = panda();
        let (base, living) = living_mut(&mut entity);
        assert_eq!(living::health(base), 20.0);
        set_main_gene(base, Gene::Weak);
        set_hidden_gene(base, Gene::Weak);
        set_attributes(base, living);
        assert_eq!(living.max_health(), 10.0);
        assert_eq!(living::health(base), 10.0);
        set_main_gene(base, Gene::Lazy);
        set_attributes(base, living);
        assert_eq!(living.attribute(MOVEMENT_SPEED), f64::from(0.07f32));
    }

    #[test]
    fn finalize_rolls_both_genes_from_the_level_random() {
        let mut entity = panda();
        let environment = SpawnEnvironment {
            biome: crate::BiomeId(0),
            difficulty: DifficultyInstance::new(Difficulty::Normal, 0, 0, 1.0),
            halloween: false,
            moon_brightness: 0.0,
        };
        let (base, living) = living_mut(&mut entity);
        let mut level = Script(vec![0, 4]);
        let group = finalize(base, living, &environment, None, &mut level);
        assert_eq!(genes(base), (Gene::Lazy, Gene::Aggressive));
        assert!(matches!(
            group,
            Some(SpawnGroupData::Ageable {
                group_size: 1,
                should_spawn_baby: true,
                baby_chance,
            }) if baby_chance == 0.2
        ));
    }

    #[test]
    fn genes_survive_a_save_and_load_round_trip() {
        let mut entity = panda();
        set_main_gene(&mut entity.base, Gene::Playful);
        set_hidden_gene(&mut entity.base, Gene::Brown);
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_str("MainGene"), Some("playful"));
        assert_eq!(saved.get_str("HiddenGene"), Some("brown"));
        let loaded = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        assert_eq!(genes(&loaded.base), (Gene::Playful, Gene::Brown));
        assert_eq!(save_entity(&loaded, None), Some(saved));
    }

    #[test]
    fn missing_or_unknown_gene_names_load_as_normal() {
        let mut entity = panda();
        set_main_gene(&mut entity.base, Gene::Weak);
        let mut saved = save_entity(&entity, None).unwrap();
        saved.insert("MainGene", "giant");
        saved.remove("HiddenGene");
        let loaded = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        assert_eq!(genes(&loaded.base), (Gene::Normal, Gene::Normal));
    }

    #[test]
    fn loot_pickup_is_kept_unless_the_nbt_is_current_and_says_otherwise() {
        let mut saved = save_entity(&panda(), None).unwrap();
        assert_eq!(saved.get_bool("CanPickUpLoot"), Some(true));
        saved.insert("CanPickUpLoot", false);
        saved.remove("Bukkit.updateLevel");
        let legacy = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        assert!(
            legacy
                .living()
                .and_then(|l| l.mob.as_ref())
                .unwrap()
                .can_pick_up_loot
        );
        saved.insert("Bukkit.updateLevel", 2);
        let current = load_entity(&saved, EntityId(3), Uuid::from_u128(3)).unwrap();
        assert!(
            !current
                .living()
                .and_then(|l| l.mob.as_ref())
                .unwrap()
                .can_pick_up_loot
        );
    }

    #[test]
    fn babies_are_half_size_with_a_fixed_eye_height() {
        let mut entity = panda();
        let EntityState::Living(state) = &mut entity.state else {
            panic!("living");
        };
        let ageable = state
            .mob
            .as_mut()
            .and_then(|mob| mob.ageable.as_mut())
            .unwrap();
        crate::entity::ageable::set_baby(&mut entity.base, ageable, true);
        let dimensions = entity.base.dimensions;
        assert_eq!(dimensions.width, 0.65);
        assert_eq!(dimensions.height, 0.625);
        assert_eq!(dimensions.eye_height, 0.28125);
    }
}
