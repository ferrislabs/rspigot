use rspigot_data::variants::{Farm, SoundSet};
use rspigot_entities::EntityKind;

use std::borrow::Cow;

use super::base::EntityBase;
use super::data::{SyncedValue, VariantKind};
use super::living::LivingAttributes;
use crate::effect::{AttributeModifier, AttributeOperation};
use crate::random::RandomSource;
use crate::{BiomeId, biome};

pub mod cat;
pub mod enderman;
pub mod endermite;
pub mod fox;
pub mod iron_golem;
pub mod ocelot;
pub mod panda;
pub mod polar_bear;
pub mod rabbit;
pub mod silverfish;
pub mod wolf;

pub const PIG_DATA_BOOST_TIME: u8 = 18;
pub const DATA_WOOL: u8 = 18;
pub const ZOMBIE_DATA_BABY: u8 = 16;
pub const ZOMBIE_DATA_SPECIAL_TYPE: u8 = 17;
pub const ZOMBIE_DATA_DROWNED_CONVERSION: u8 = 18;
pub const SKELETON_DATA_STRAY_CONVERSION: u8 = 16;
pub const CREEPER_DATA_SWELL_DIR: u8 = 16;
pub const CREEPER_DATA_POWERED: u8 = 17;
pub const CREEPER_DATA_IGNITED: u8 = 18;
pub const WOOL_COLOR_MASK: u8 = 0x0F;
pub const WOOL_SHEARED: u8 = 0x10;
pub const DEFAULT_MAX_SWELL: i32 = 30;
pub const DEFAULT_EXPLOSION_RADIUS: i32 = 3;
pub const EGG_TIME_BASE: i32 = 6000;
pub const DROWNING_AFFLICTION_TICKS: i32 = 600;
pub const DROWNING_CONVERSION_TICKS: i32 = 300;
pub const FREEZING_AFFLICTION_TICKS: i32 = 140;
pub const FREEZING_CONVERSION_TICKS: i32 = 300;
pub const DROWNED_CONVERSION_EVENT: i32 = 1040;
pub const STRAY_CONVERSION_EVENT: i32 = 1048;
pub const BABY_SPEED_MODIFIER: &str = "minecraft:baby";
const BABY_SPEED_BONUS: f64 = 0.5;
const MOVEMENT_SPEED: &str = "minecraft:movement_speed";
const DEFAULT_VARIANT: &str = "minecraft:temperate";
const DEFAULT_SOUND_VARIANT: &str = "minecraft:classic";
const SWELL_FALL_FACTOR: f64 = 1.5;
const SWELL_FALL_MARGIN: i32 = 5;
const FLAP_DRAG: f64 = 0.6;
const WARM_FARM_ANIMALS: &str = "minecraft:spawns_warm_variant_farm_animals";
const COLD_FARM_ANIMALS: &str = "minecraft:spawns_cold_variant_farm_animals";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConversionTracker {
    pub affliction_time: i32,
    pub conversion_time: i32,
    pub last_tick: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversionStep {
    Idle,
    Started,
    Convert,
}

impl ConversionTracker {
    pub fn tick(
        &mut self,
        base: &mut EntityBase,
        data: u8,
        afflicted: bool,
        active: bool,
        limits: (i32, i32),
        now: i64,
    ) -> ConversionStep {
        let elapsed = (now - self.last_tick) as i32;
        let mut step = ConversionStep::Idle;
        if active {
            if afflicted {
                if base.data.boolean(data) {
                    self.conversion_time -= elapsed;
                    if self.conversion_time < 0 {
                        step = ConversionStep::Convert;
                    }
                } else {
                    self.affliction_time += elapsed;
                    if self.affliction_time >= limits.0 {
                        self.start(base, data, limits.1, now);
                        step = ConversionStep::Started;
                    }
                }
            } else {
                self.affliction_time = -1;
                base.data.set(data, SyncedValue::Boolean(false));
            }
        }
        self.last_tick = now;
        step
    }

    pub fn start(&mut self, base: &mut EntityBase, data: u8, time: i32, now: i64) {
        self.last_tick = now;
        self.conversion_time = time;
        base.data.set(data, SyncedValue::Boolean(true));
    }

    pub fn cancel(&mut self, base: &mut EntityBase, data: u8) {
        self.conversion_time = -1;
        base.data.set(data, SyncedValue::Boolean(false));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChickenState {
    pub egg_time: Option<i32>,
    pub jockey: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SheepState {
    pub eat_animation_tick: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ZombieState {
    pub can_break_doors: bool,
    pub drowning: ConversionTracker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SkeletonState {
    pub freezing: ConversionTracker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreeperState {
    pub old_swell: i32,
    pub swell: i32,
    pub max_swell: i32,
    pub explosion_radius: i32,
    pub dropped_skulls: bool,
}

impl Default for CreeperState {
    fn default() -> Self {
        Self {
            old_swell: 0,
            swell: 0,
            max_swell: DEFAULT_MAX_SWELL,
            explosion_radius: DEFAULT_EXPLOSION_RADIUS,
            dropped_skulls: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Species {
    #[default]
    Plain,
    Cow,
    Pig,
    Chicken(ChickenState),
    Sheep(SheepState),
    Zombie(ZombieState),
    Skeleton(SkeletonState),
    Creeper(CreeperState),
    Wolf(wolf::WolfState),
    Cat(cat::CatState),
    Fox(fox::FoxState),
    Panda(panda::PandaState),
    PolarBear(polar_bear::PolarBearState),
    Enderman(enderman::EndermanState),
    Endermite(endermite::EndermiteState),
    Silverfish(silverfish::SilverfishState),
    IronGolem(iron_golem::IronGolemState),
    Rabbit(rabbit::RabbitState),
    Ocelot(ocelot::OcelotState),
}

pub fn farm_of(kind: EntityKind) -> Option<Farm> {
    match kind {
        EntityKind::Cow => Some(Farm::Cow),
        EntityKind::Pig => Some(Farm::Pig),
        EntityKind::Chicken => Some(Farm::Chicken),
        _ => None,
    }
}

pub const fn variant_data(farm: Farm) -> u8 {
    match farm {
        Farm::Pig => 19,
        Farm::Cow | Farm::Chicken => 18,
    }
}

pub const fn sound_variant_data(farm: Farm) -> u8 {
    variant_data(farm) + 1
}

pub fn is_zombie(kind: EntityKind) -> bool {
    kind.extends("Zombie")
}

impl Species {
    pub fn farm(&self) -> Option<Farm> {
        match self {
            Self::Cow => Some(Farm::Cow),
            Self::Pig => Some(Farm::Pig),
            Self::Chicken(_) => Some(Farm::Chicken),
            _ => None,
        }
    }

    pub fn new(base: &mut EntityBase) -> Self {
        let kind = base.kind;
        if let Some(farm) = farm_of(kind) {
            if farm == Farm::Pig {
                base.data.define(PIG_DATA_BOOST_TIME, SyncedValue::Int(0));
            }
            let variant = farm.variant_index(DEFAULT_VARIANT).unwrap_or(0);
            let sound = farm.sound_variant_index(DEFAULT_SOUND_VARIANT).unwrap_or(0);
            base.data.define(
                variant_data(farm),
                SyncedValue::Variant(VariantKind::Mob(farm), variant),
            );
            base.data.define(
                sound_variant_data(farm),
                SyncedValue::Variant(VariantKind::Sound(farm), sound),
            );
            return match farm {
                Farm::Cow => Self::Cow,
                Farm::Pig => Self::Pig,
                Farm::Chicken => Self::Chicken(ChickenState::default()),
            };
        }
        if kind == EntityKind::Sheep {
            base.data.define(DATA_WOOL, SyncedValue::Byte(0));
            return Self::Sheep(SheepState::default());
        }
        if is_zombie(kind) {
            base.data
                .define(ZOMBIE_DATA_BABY, SyncedValue::Boolean(false));
            base.data
                .define(ZOMBIE_DATA_SPECIAL_TYPE, SyncedValue::Int(0));
            base.data
                .define(ZOMBIE_DATA_DROWNED_CONVERSION, SyncedValue::Boolean(false));
            return Self::Zombie(ZombieState::default());
        }
        if kind == EntityKind::Skeleton {
            base.data
                .define(SKELETON_DATA_STRAY_CONVERSION, SyncedValue::Boolean(false));
            return Self::Skeleton(SkeletonState::default());
        }
        if kind == EntityKind::Creeper {
            base.data
                .define(CREEPER_DATA_SWELL_DIR, SyncedValue::Int(-1));
            base.data
                .define(CREEPER_DATA_POWERED, SyncedValue::Boolean(false));
            base.data
                .define(CREEPER_DATA_IGNITED, SyncedValue::Boolean(false));
            return Self::Creeper(CreeperState::default());
        }
        if kind == EntityKind::Wolf {
            return Self::Wolf(wolf::define(base));
        }
        if kind == EntityKind::Cat {
            return Self::Cat(cat::define(base));
        }
        if kind == EntityKind::Fox {
            return Self::Fox(fox::define(base));
        }
        if kind == EntityKind::Panda {
            return Self::Panda(panda::define(base));
        }
        if kind == EntityKind::PolarBear {
            return Self::PolarBear(polar_bear::define(base));
        }
        if kind == EntityKind::Enderman {
            return Self::Enderman(enderman::define(base));
        }
        if kind == EntityKind::Endermite {
            return Self::Endermite(endermite::define(base));
        }
        if kind == EntityKind::Silverfish {
            return Self::Silverfish(silverfish::define(base));
        }
        if kind == EntityKind::IronGolem {
            return Self::IronGolem(iron_golem::define(base));
        }
        if kind == EntityKind::Rabbit {
            return Self::Rabbit(rabbit::define(base));
        }
        if kind == EntityKind::Ocelot {
            return Self::Ocelot(ocelot::define(base));
        }
        Self::Plain
    }
}

pub fn variant(base: &EntityBase, farm: Farm) -> u32 {
    base.data.variant(variant_data(farm)).unwrap_or(0)
}

pub fn set_variant(base: &mut EntityBase, farm: Farm, id: u32) {
    base.data.set(
        variant_data(farm),
        SyncedValue::Variant(VariantKind::Mob(farm), id),
    );
}

pub fn sound_variant(base: &EntityBase, farm: Farm) -> u32 {
    base.data.variant(sound_variant_data(farm)).unwrap_or(0)
}

pub fn set_sound_variant(base: &mut EntityBase, farm: Farm, id: u32) {
    base.data.set(
        sound_variant_data(farm),
        SyncedValue::Variant(VariantKind::Sound(farm), id),
    );
}

pub fn variant_name(farm: Farm, id: u32) -> &'static str {
    farm.variants()
        .get(id as usize)
        .map_or(DEFAULT_VARIANT, |v| v.name)
}

pub fn sound_variant_name(farm: Farm, id: u32) -> &'static str {
    farm.sound_variants()
        .get(id as usize)
        .map_or(DEFAULT_SOUND_VARIANT, |v| v.name)
}

pub fn sound_set(base: &EntityBase, farm: Farm, baby: bool) -> Option<SoundSet> {
    let variant = farm
        .sound_variants()
        .get(sound_variant(base, farm) as usize)?;
    Some(if baby { variant.baby } else { variant.adult })
}

pub fn select_variant(farm: Farm, biome: BiomeId, random: &mut impl RandomSource) -> Option<u32> {
    let mut unpacked: Vec<(u32, i32, bool)> = Vec::new();
    for (index, variant) in farm.variants().iter().enumerate() {
        for condition in variant.spawn_conditions {
            let matches = condition.biomes.is_none_or(|biomes| {
                biomes.contains("minecraft:worldgen/biome", u32::from(biome.0))
            });
            unpacked.push((index as u32, condition.priority, matches));
        }
    }
    unpacked.sort_by_key(|(_, priority, _)| std::cmp::Reverse(*priority));
    let mut highest = i32::MIN;
    let mut candidates = Vec::new();
    for (index, priority, matches) in unpacked {
        if priority < highest || !matches {
            continue;
        }
        highest = priority;
        candidates.push(index);
    }
    if candidates.is_empty() {
        return None;
    }
    let size = candidates.len() as i32;
    Some(candidates[random.next_int(size) as usize])
}

pub fn pick_sound_variant(farm: Farm, random: &mut impl RandomSource) -> u32 {
    random.next_int(farm.sound_variants().len() as i32) as u32
}

pub fn wool_color(base: &EntityBase) -> u8 {
    base.data.byte(DATA_WOOL) as u8 & WOOL_COLOR_MASK
}

pub fn set_wool_color(base: &mut EntityBase, color: u8) {
    let current = base.data.byte(DATA_WOOL) as u8;
    let next = (current & 0xF0) | (color & WOOL_COLOR_MASK);
    base.data.set(DATA_WOOL, SyncedValue::Byte(next as i8));
}

pub fn is_sheared(base: &EntityBase) -> bool {
    base.data.flag(DATA_WOOL, WOOL_SHEARED)
}

pub fn set_sheared(base: &mut EntityBase, sheared: bool) {
    base.data.set_flag(DATA_WOOL, WOOL_SHEARED, sheared);
}

pub fn dye_by_id(id: i32) -> u8 {
    if (0..16).contains(&id) { id as u8 } else { 0 }
}

pub const DYE_WHITE: u8 = 0;
pub const DYE_GRAY: u8 = 7;
pub const DYE_LIGHT_GRAY: u8 = 8;
pub const DYE_BROWN: u8 = 12;
pub const DYE_BLACK: u8 = 15;
pub const DYE_PINK: u8 = 6;

fn weighted_sheep_color(singles: [(u8, i32); 4], common: u8, random: &mut impl RandomSource) -> u8 {
    let mut roll = random.next_int(100);
    for (color, weight) in singles {
        if roll < weight {
            return color;
        }
        roll -= weight;
    }
    if random.next_int(500) < 499 {
        common
    } else {
        DYE_PINK
    }
}

pub fn sheep_spawn_color(biome: BiomeId, random: &mut impl RandomSource) -> u8 {
    if biome::in_tag(biome, WARM_FARM_ANIMALS) {
        weighted_sheep_color(
            [
                (DYE_GRAY, 5),
                (DYE_LIGHT_GRAY, 5),
                (DYE_WHITE, 5),
                (DYE_BLACK, 3),
            ],
            DYE_BROWN,
            random,
        )
    } else if biome::in_tag(biome, COLD_FARM_ANIMALS) {
        weighted_sheep_color(
            [
                (DYE_LIGHT_GRAY, 5),
                (DYE_GRAY, 5),
                (DYE_WHITE, 5),
                (DYE_BROWN, 3),
            ],
            DYE_BLACK,
            random,
        )
    } else {
        weighted_sheep_color(
            [
                (DYE_BLACK, 5),
                (DYE_GRAY, 5),
                (DYE_LIGHT_GRAY, 5),
                (DYE_BROWN, 3),
            ],
            DYE_WHITE,
            random,
        )
    }
}

pub fn mixed_wool_color(first: u8, second: u8, random: &mut impl RandomSource) -> u8 {
    rspigot_data::variants::dye_mix(first, second)
        .unwrap_or_else(|| if random.next_boolean() { first } else { second })
}

pub fn zombie_is_baby(base: &EntityBase) -> bool {
    base.data.boolean(ZOMBIE_DATA_BABY)
}

pub fn set_zombie_baby(base: &mut EntityBase, attributes: &mut LivingAttributes, baby: bool) {
    base.data.set(ZOMBIE_DATA_BABY, SyncedValue::Boolean(baby));
    super::ageable::refresh_dimensions(base, baby);
    attributes.remove_modifier(MOVEMENT_SPEED, BABY_SPEED_MODIFIER);
    if baby {
        attributes.add_modifier(
            MOVEMENT_SPEED,
            AttributeModifier {
                id: Cow::Borrowed(BABY_SPEED_MODIFIER),
                amount: BABY_SPEED_BONUS,
                operation: AttributeOperation::AddMultipliedBase,
            },
        );
    }
}

pub fn swell_dir(base: &EntityBase) -> i32 {
    base.data.int(CREEPER_DATA_SWELL_DIR)
}

pub fn set_swell_dir(base: &mut EntityBase, dir: i32) {
    base.data.set(CREEPER_DATA_SWELL_DIR, SyncedValue::Int(dir));
}

pub fn is_powered(base: &EntityBase) -> bool {
    base.data.boolean(CREEPER_DATA_POWERED)
}

pub fn set_powered(base: &mut EntityBase, powered: bool) {
    base.data
        .set(CREEPER_DATA_POWERED, SyncedValue::Boolean(powered));
}

pub fn is_ignited(base: &EntityBase) -> bool {
    base.data.boolean(CREEPER_DATA_IGNITED)
}

pub fn ignite(base: &mut EntityBase) {
    base.data
        .set(CREEPER_DATA_IGNITED, SyncedValue::Boolean(true));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SwellTick {
    pub primed: bool,
    pub explode: bool,
}

pub fn creeper_tick(base: &mut EntityBase, state: &mut CreeperState) -> SwellTick {
    let mut tick = SwellTick::default();
    state.old_swell = state.swell;
    if is_ignited(base) {
        set_swell_dir(base, 1);
    }
    let dir = swell_dir(base);
    if dir > 0 && state.swell == 0 {
        tick.primed = true;
    }
    state.swell += dir;
    if state.swell < 0 {
        state.swell = 0;
    }
    if state.swell >= state.max_swell {
        state.swell = state.max_swell;
        tick.explode = true;
    }
    tick
}

pub fn creeper_fall(state: &mut CreeperState, fall_distance: f64) {
    state.swell += (fall_distance * SWELL_FALL_FACTOR) as i32;
    if state.swell > state.max_swell - SWELL_FALL_MARGIN {
        state.swell = state.max_swell - SWELL_FALL_MARGIN;
    }
}

pub fn chicken_flap(base: &mut EntityBase) {
    let v = base.velocity;
    if !base.on_ground && v.y < 0.0 {
        base.velocity = v.multiply(1.0, FLAP_DRAG, 1.0);
    }
}

pub fn next_egg_time(random: &mut impl RandomSource) -> i32 {
    random.next_int(EGG_TIME_BASE) + EGG_TIME_BASE
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::random::LegacyRandom;

    fn base(kind: EntityKind) -> (EntityBase, Species) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
        let species = Species::new(&mut base);
        (base, species)
    }

    #[test]
    fn farm_animals_default_to_temperate_classic() {
        let (pig, species) = base(EntityKind::Pig);
        assert_eq!(species, Species::Pig);
        assert_eq!(variant(&pig, Farm::Pig), 1);
        assert_eq!(sound_variant(&pig, Farm::Pig), 1);
        assert!(pig.data.non_default().is_empty());
        let (cow, _) = base(EntityKind::Cow);
        assert_eq!(sound_variant(&cow, Farm::Cow), 0);
    }

    #[test]
    fn biome_tags_pick_the_priority_variant() {
        let snowy = BiomeId(rspigot_data::registries::biome_id("minecraft:snowy_plains").unwrap());
        let plains = BiomeId(rspigot_data::registries::biome_id("minecraft:plains").unwrap());
        let mut random = LegacyRandom::new(5);
        assert_eq!(select_variant(Farm::Cow, snowy, &mut random), Some(0));
        assert_eq!(select_variant(Farm::Chicken, plains, &mut random), Some(1));
    }

    #[test]
    fn wool_byte_keeps_colour_and_shear_bits_apart() {
        let (mut sheep, _) = base(EntityKind::Sheep);
        set_wool_color(&mut sheep, 14);
        set_sheared(&mut sheep, true);
        assert_eq!(wool_color(&sheep), 14);
        assert!(is_sheared(&sheep));
        assert_eq!(sheep.data.byte(DATA_WOOL), 0x1E);
        set_sheared(&mut sheep, false);
        assert_eq!(sheep.data.byte(DATA_WOOL), 14);
    }

    #[test]
    fn creepers_swell_to_their_fuse_and_explode() {
        let (mut creeper, species) = base(EntityKind::Creeper);
        let Species::Creeper(mut state) = species else {
            panic!("not a creeper");
        };
        assert_eq!(swell_dir(&creeper), -1);
        set_swell_dir(&mut creeper, 1);
        let first = creeper_tick(&mut creeper, &mut state);
        assert!(first.primed);
        for _ in 0..28 {
            assert!(!creeper_tick(&mut creeper, &mut state).explode);
        }
        assert!(creeper_tick(&mut creeper, &mut state).explode);
        let mut falling = CreeperState::default();
        creeper_fall(&mut falling, 40.0);
        assert_eq!(falling.swell, 25);
    }

    #[test]
    fn conversion_starts_after_the_affliction_time() {
        let (mut zombie, _) = base(EntityKind::Zombie);
        let mut tracker = ConversionTracker::default();
        let data = ZOMBIE_DATA_DROWNED_CONVERSION;
        let limits = (DROWNING_AFFLICTION_TICKS, DROWNING_CONVERSION_TICKS);
        for tick in 1..600 {
            assert_eq!(
                tracker.tick(&mut zombie, data, true, true, limits, tick),
                ConversionStep::Idle
            );
        }
        assert_eq!(
            tracker.tick(&mut zombie, data, true, true, limits, 600),
            ConversionStep::Started
        );
        assert!(zombie.data.boolean(data));
        assert_eq!(
            tracker.tick(&mut zombie, data, true, true, limits, 900),
            ConversionStep::Idle
        );
        assert_eq!(
            tracker.tick(&mut zombie, data, true, true, limits, 901),
            ConversionStep::Convert
        );
        tracker.tick(&mut zombie, data, false, true, limits, 902);
        assert!(!zombie.data.boolean(data));
        assert_eq!(tracker.affliction_time, -1);
    }
}
