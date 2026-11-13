use rspigot_data::variants::{SoundSet, VariantCondition, VariantFamily};
use rspigot_entities::EntityKind;

use std::borrow::Cow;

use super::base::EntityBase;
use super::data::{SyncedValue, VariantKind};
use super::living::LivingAttributes;
use super::variants::{self, VariantContext};
use crate::effect::{AttributeModifier, AttributeOperation};
use crate::random::RandomSource;
use crate::{BiomeId, biome};

pub mod armadillo;
pub mod blaze;
pub mod cat;
pub mod copper_golem;
pub mod creaking;
pub mod enderman;
pub mod endermite;
pub mod fox;
pub mod ghast;
pub mod goat;
pub mod hoglin;
pub mod iron_golem;
pub mod ocelot;
pub mod panda;
pub mod piglin;
pub mod pillager;
pub mod polar_bear;
pub mod rabbit;
pub mod raider;
pub mod shulker;
pub mod silverfish;
pub mod sniffer;
pub mod snow_golem;
pub mod villager;
pub mod wandering_trader;
pub mod warden;
pub mod witch;
pub mod wolf;
pub mod zombie_villager;
pub mod zombified_piglin;

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
pub const HUSK_CONVERSION_EVENT: i32 = 1041;
pub const STRAY_CONVERSION_EVENT: i32 = 1048;
pub const BABY_SPEED_MODIFIER: &str = "minecraft:baby";
const BABY_SPEED_BONUS: f64 = 0.5;
const MOVEMENT_SPEED: &str = "minecraft:movement_speed";
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
    pub piglin: zombified_piglin::ZombifiedPiglinState,
    pub villager: Option<zombie_villager::ZombieVillagerState>,
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

#[derive(Debug, Clone, Copy, PartialEq, Default)]
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
    SnowGolem(snow_golem::SnowGolemState),
    Blaze(blaze::BlazeState),
    Ghast(ghast::GhastState),
    Shulker(shulker::ShulkerState),
    Rabbit(rabbit::RabbitState),
    Ocelot(ocelot::OcelotState),
    Witch(witch::WitchState),
    Pillager(pillager::PillagerState),
    Hoglin(hoglin::HoglinState),
    Piglin(piglin::PiglinState),
    Goat(goat::GoatState),
    Armadillo(armadillo::ArmadilloState),
    Sniffer(sniffer::SnifferState),
    Creaking(creaking::CreakingState),
    Villager(villager::VillagerState),
    WanderingTrader(wandering_trader::WanderingTraderState),
    Warden(warden::WardenState),
    CopperGolem(copper_golem::CopperGolemState),
}

pub fn farm_of(kind: EntityKind) -> Option<VariantFamily> {
    match kind {
        EntityKind::Cow => Some(VariantFamily::Cow),
        EntityKind::Pig => Some(VariantFamily::Pig),
        EntityKind::Chicken => Some(VariantFamily::Chicken),
        _ => None,
    }
}

pub fn variant_family_of(kind: EntityKind) -> Option<VariantFamily> {
    match kind {
        EntityKind::Wolf => Some(VariantFamily::Wolf),
        EntityKind::Cat => Some(VariantFamily::Cat),
        _ => farm_of(kind),
    }
}

pub const fn variant_data(family: VariantFamily) -> u8 {
    match family {
        VariantFamily::Pig => 19,
        VariantFamily::Cow | VariantFamily::Chicken => 18,
        VariantFamily::Wolf => variants::WOLF_DATA_VARIANT,
        VariantFamily::Cat => variants::CAT_DATA_VARIANT,
    }
}

pub const fn sound_variant_data(family: VariantFamily) -> u8 {
    match family {
        VariantFamily::Wolf => variants::WOLF_DATA_SOUND_VARIANT,
        VariantFamily::Cat => variants::CAT_DATA_SOUND_VARIANT,
        VariantFamily::Cow | VariantFamily::Pig | VariantFamily::Chicken => {
            variant_data(family) + 1
        }
    }
}

pub fn is_zombie(kind: EntityKind) -> bool {
    kind.extends("Zombie")
}

impl Species {
    pub fn raider(&self) -> Option<&raider::RaiderState> {
        match self {
            Self::Witch(state) => Some(&state.raider),
            Self::Pillager(state) => Some(&state.raider),
            _ => None,
        }
    }

    pub fn raider_mut(&mut self) -> Option<&mut raider::RaiderState> {
        match self {
            Self::Witch(state) => Some(&mut state.raider),
            Self::Pillager(state) => Some(&mut state.raider),
            _ => None,
        }
    }

    pub fn farm(&self) -> Option<VariantFamily> {
        match self {
            Self::Cow => Some(VariantFamily::Cow),
            Self::Pig => Some(VariantFamily::Pig),
            Self::Chicken(_) => Some(VariantFamily::Chicken),
            _ => None,
        }
    }

    pub fn variant_family(&self) -> Option<VariantFamily> {
        match self {
            Self::Wolf(_) => Some(VariantFamily::Wolf),
            Self::Cat(_) => Some(VariantFamily::Cat),
            _ => self.farm(),
        }
    }

    pub fn new(base: &mut EntityBase) -> Self {
        let kind = base.kind;
        if let Some(family) = variant_family_of(kind) {
            define_variants(base, family);
        }
        if let Some(farm) = farm_of(kind) {
            if farm == VariantFamily::Pig {
                base.data.define(PIG_DATA_BOOST_TIME, SyncedValue::Int(0));
            }
            return match farm {
                VariantFamily::Cow => Self::Cow,
                VariantFamily::Pig => Self::Pig,
                VariantFamily::Chicken => Self::Chicken(ChickenState::default()),
                VariantFamily::Wolf | VariantFamily::Cat => Self::Plain,
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
            let villager =
                (kind == EntityKind::ZombieVillager).then(|| zombie_villager::define(base));
            return Self::Zombie(ZombieState {
                villager,
                ..ZombieState::default()
            });
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
        if kind == EntityKind::SnowGolem {
            return Self::SnowGolem(snow_golem::define(base));
        }
        if kind == EntityKind::Blaze {
            return Self::Blaze(blaze::define(base));
        }
        if kind == EntityKind::Ghast {
            return Self::Ghast(ghast::define(base));
        }
        if kind == EntityKind::Shulker {
            return Self::Shulker(shulker::define(base));
        }
        if kind == EntityKind::Rabbit {
            return Self::Rabbit(rabbit::define(base));
        }
        if kind == EntityKind::Ocelot {
            return Self::Ocelot(ocelot::define(base));
        }
        if kind == EntityKind::Witch {
            return Self::Witch(witch::define(base));
        }
        if kind == EntityKind::Pillager {
            return Self::Pillager(pillager::define(base));
        }
        if matches!(kind, EntityKind::Hoglin | EntityKind::Zoglin) {
            return Self::Hoglin(hoglin::define(base));
        }
        if matches!(kind, EntityKind::Piglin | EntityKind::PiglinBrute) {
            return Self::Piglin(piglin::define(base));
        }
        if kind == EntityKind::Goat {
            return Self::Goat(goat::define(base));
        }
        if kind == EntityKind::Armadillo {
            return Self::Armadillo(armadillo::define(base));
        }
        if kind == EntityKind::Sniffer {
            return Self::Sniffer(sniffer::define(base));
        }
        if kind == EntityKind::Creaking {
            return Self::Creaking(creaking::define(base));
        }
        if kind == EntityKind::Villager {
            return Self::Villager(villager::define(base));
        }
        if kind == EntityKind::WanderingTrader {
            return Self::WanderingTrader(wandering_trader::define(base));
        }
        if kind == EntityKind::Warden {
            return Self::Warden(warden::define(base));
        }
        if kind == EntityKind::CopperGolem {
            return Self::CopperGolem(copper_golem::define(base));
        }
        Self::Plain
    }
}

fn define_variants(base: &mut EntityBase, family: VariantFamily) {
    let variant = family
        .variant_index(variants::default_variant(family))
        .unwrap_or(0);
    let sound = family
        .sound_variant_index(variants::default_sound_variant(family))
        .unwrap_or(0);
    base.data.define(
        variant_data(family),
        SyncedValue::Variant(VariantKind::Mob(family), variant),
    );
    base.data.define(
        sound_variant_data(family),
        SyncedValue::Variant(VariantKind::Sound(family), sound),
    );
}

pub fn variant(base: &EntityBase, farm: VariantFamily) -> u32 {
    base.data.variant(variant_data(farm)).unwrap_or(0)
}

pub fn set_variant(base: &mut EntityBase, farm: VariantFamily, id: u32) {
    base.data.set(
        variant_data(farm),
        SyncedValue::Variant(VariantKind::Mob(farm), id),
    );
}

pub fn sound_variant(base: &EntityBase, farm: VariantFamily) -> u32 {
    base.data.variant(sound_variant_data(farm)).unwrap_or(0)
}

pub fn set_sound_variant(base: &mut EntityBase, farm: VariantFamily, id: u32) {
    base.data.set(
        sound_variant_data(farm),
        SyncedValue::Variant(VariantKind::Sound(farm), id),
    );
}

pub fn variant_name(farm: VariantFamily, id: u32) -> &'static str {
    farm.variants()
        .get(id as usize)
        .map_or(variants::default_variant(farm), |v| v.name)
}

pub fn sound_variant_name(farm: VariantFamily, id: u32) -> &'static str {
    farm.sound_variants()
        .get(id as usize)
        .map_or(variants::default_sound_variant(farm), |v| v.name)
}

pub fn sound_set(base: &EntityBase, farm: VariantFamily, baby: bool) -> Option<SoundSet> {
    let variant = farm
        .sound_variants()
        .get(sound_variant(base, farm) as usize)?;
    Some(if baby { variant.baby } else { variant.adult })
}

pub fn select_variant(
    family: VariantFamily,
    context: &VariantContext,
    random: &mut impl RandomSource,
) -> Option<u32> {
    let mut unpacked: Vec<(u32, i32, bool)> = Vec::new();
    for (index, variant) in family.variants().iter().enumerate() {
        for condition in variant.spawn_conditions {
            let matches = condition
                .condition
                .is_none_or(|condition| condition_matches(condition, context));
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

fn condition_matches(condition: VariantCondition, context: &VariantContext) -> bool {
    match condition {
        VariantCondition::Biome(biomes) => {
            biomes.contains("minecraft:worldgen/biome", u32::from(context.biome.0))
        }
        VariantCondition::Structure(tag) => context.structures.contains(tag),
        VariantCondition::MoonBrightness { min, max } => {
            VariantCondition::moon_brightness_matches(min, max, context.moon_brightness)
        }
    }
}

pub fn pick_sound_variant(farm: VariantFamily, random: &mut impl RandomSource) -> u32 {
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
        assert_eq!(variant(&pig, VariantFamily::Pig), 1);
        assert_eq!(sound_variant(&pig, VariantFamily::Pig), 1);
        assert!(pig.data.non_default().is_empty());
        let (cow, _) = base(EntityKind::Cow);
        assert_eq!(sound_variant(&cow, VariantFamily::Cow), 0);
    }

    #[test]
    fn wolf_and_cat_define_their_variant_data_at_the_java_indexes() {
        use super::variants as v;
        let (wolf, _) = base(EntityKind::Wolf);
        assert_eq!(variant_data(VariantFamily::Wolf), v::WOLF_DATA_VARIANT);
        assert_eq!(sound_variant_data(VariantFamily::Wolf), 24);
        assert_eq!(variant(&wolf, VariantFamily::Wolf), 3);
        assert_eq!(
            variant_name(VariantFamily::Wolf, variant(&wolf, VariantFamily::Wolf)),
            "minecraft:pale"
        );
        assert_eq!(sound_variant(&wolf, VariantFamily::Wolf), 2);
        assert!(wolf.data.non_default().is_empty());
        let (cat, _) = base(EntityKind::Cat);
        assert_eq!(variant_data(VariantFamily::Cat), 20);
        assert_eq!(sound_variant_data(VariantFamily::Cat), 24);
        assert_eq!(variant(&cat, VariantFamily::Cat), 1);
        assert_eq!(
            variant_name(VariantFamily::Cat, variant(&cat, VariantFamily::Cat)),
            "minecraft:black"
        );
        assert_eq!(sound_variant(&cat, VariantFamily::Cat), 0);
        let (mut cat, _) = base(EntityKind::Cat);
        set_variant(&mut cat, VariantFamily::Cat, 4);
        set_sound_variant(&mut cat, VariantFamily::Cat, 1);
        assert_eq!(
            cat.data.get(20),
            Some(&SyncedValue::Variant(
                VariantKind::Mob(VariantFamily::Cat),
                4
            ))
        );
        assert_eq!(
            cat.data.get(24),
            Some(&SyncedValue::Variant(
                VariantKind::Sound(VariantFamily::Cat),
                1
            ))
        );
        assert_eq!(
            sound_set(&cat, VariantFamily::Cat, false)
                .unwrap()
                .stray_ambient,
            Some("minecraft:entity.cat_royal.stray_ambient")
        );
    }

    #[test]
    fn wolves_pick_the_variant_of_their_biome() {
        let forest = BiomeId(rspigot_data::registries::biome_id("minecraft:forest").unwrap());
        let plains = BiomeId(rspigot_data::registries::biome_id("minecraft:plains").unwrap());
        let mut random = LegacyRandom::new(5);
        let woods = VariantFamily::Wolf.variant_index("minecraft:woods");
        assert_eq!(
            select_variant(
                VariantFamily::Wolf,
                &VariantContext::in_biome(forest),
                &mut random
            ),
            woods
        );
        assert_eq!(
            select_variant(
                VariantFamily::Wolf,
                &VariantContext::in_biome(plains),
                &mut random
            ),
            VariantFamily::Wolf.variant_index("minecraft:pale")
        );
    }

    #[test]
    fn cats_pick_black_by_structure_or_full_moon() {
        use super::variants::StructureProbe;
        let plains = BiomeId(rspigot_data::registries::biome_id("minecraft:plains").unwrap());
        let all_black = VariantFamily::Cat.variant_index("minecraft:all_black");
        let mut context = VariantContext::in_biome(plains);
        let pick = |context: &VariantContext, seed| {
            select_variant(VariantFamily::Cat, context, &mut LegacyRandom::new(seed))
        };
        context.moon_brightness = 0.75;
        for seed in 0..60 {
            let picked = pick(&context, seed);
            assert!(picked.is_some() && picked != all_black);
        }
        context.moon_brightness = 0.9;
        assert!((0..60).any(|seed| pick(&context, seed) == all_black));
        context.moon_brightness = 1.0;
        assert!((0..60).any(|seed| pick(&context, seed) == all_black));
        context.moon_brightness = 0.75;
        context.structures = StructureProbe::Within(|tag| tag == "minecraft:cats_spawn_as_black");
        for seed in 0..60 {
            assert_eq!(pick(&context, seed), all_black);
        }
        context.structures = StructureProbe::Within(|_| false);
        assert_ne!(pick(&context, 3), all_black);
    }

    #[test]
    fn biome_tags_pick_the_priority_variant() {
        let snowy = BiomeId(rspigot_data::registries::biome_id("minecraft:snowy_plains").unwrap());
        let plains = BiomeId(rspigot_data::registries::biome_id("minecraft:plains").unwrap());
        let mut random = LegacyRandom::new(5);
        let snowy = VariantContext::in_biome(snowy);
        let plains = VariantContext::in_biome(plains);
        assert_eq!(
            select_variant(VariantFamily::Cow, &snowy, &mut random),
            Some(0)
        );
        assert_eq!(
            select_variant(VariantFamily::Chicken, &plains, &mut random),
            Some(1)
        );
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
