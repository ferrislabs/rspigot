use crate::armadillo::ArmadilloScareSensor;
use crate::behavior::memory::{Memories, MemoryModuleType, MemoryValue};
use crate::behavior::sensors::{
    AdultSensor, DummySensor, HurtBySensor, IsInWaterSensor, NearestItemSensor,
    NearestLivingEntitySensor, PlayerSensor, TemptingSensor,
};
use crate::brain::AiContext;
use crate::breeze::BreezeAttackEntitySensor;
use crate::frog::FrogAttackablesSensor;
use crate::piglin::{HoglinSensor, PiglinBruteSensor, PiglinSensor};
use crate::random::RandomSource;
use crate::targeting::TargetingConditions;
use crate::villager::{
    GolemSensor, NearestBedSensor, SecondaryPoiSensor, VillagerBabiesSensor, VillagerHostilesSensor,
};
use crate::warden::WardenEntitySensor as WardenSensor;
use crate::world::{AiWorld, Living};

pub const DEFAULT_SCAN_RATE: i32 = 20;

macro_rules! sensor_types {
    ($($variant:ident = $name:literal;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum SensorType {
            $($variant,)*
        }

        impl SensorType {
            pub const ALL: &'static [Self] = &[$(Self::$variant,)*];

            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name,)*
                }
            }
        }
    };
}

sensor_types! {
    Dummy = "minecraft:dummy";
    NearestItems = "minecraft:nearest_items";
    NearestLivingEntities = "minecraft:nearest_living_entities";
    NearestPlayers = "minecraft:nearest_players";
    NearestBed = "minecraft:nearest_bed";
    HurtBy = "minecraft:hurt_by";
    VillagerHostiles = "minecraft:villager_hostiles";
    VillagerBabies = "minecraft:villager_babies";
    SecondaryPois = "minecraft:secondary_pois";
    GolemDetected = "minecraft:golem_detected";
    ArmadilloScareDetected = "minecraft:armadillo_scare_detected";
    PiglinSpecificSensor = "minecraft:piglin_specific_sensor";
    PiglinBruteSpecificSensor = "minecraft:piglin_brute_specific_sensor";
    HoglinSpecificSensor = "minecraft:hoglin_specific_sensor";
    NearestAdult = "minecraft:nearest_adult";
    NearestAdultAnyType = "minecraft:nearest_adult_any_type";
    AxolotlAttackables = "minecraft:axolotl_attackables";
    FoodTemptations = "minecraft:food_temptations";
    FrogTemptations = "minecraft:frog_temptations";
    NautilusTemptations = "minecraft:nautilus_temptations";
    FrogAttackables = "minecraft:frog_attackables";
    IsInWater = "minecraft:is_in_water";
    WardenEntitySensor = "minecraft:warden_entity_sensor";
    BreezeAttackEntitySensor = "minecraft:breeze_attack_entity_sensor";
}

pub trait Sensor {
    fn scan_rate(&self) -> i32 {
        DEFAULT_SCAN_RATE
    }

    fn requires(&self) -> &'static [MemoryModuleType];

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories);
}

#[derive(Debug, Clone, PartialEq)]
pub enum SensorKind {
    Dummy(DummySensor),
    NearestItems(NearestItemSensor),
    NearestLivingEntities(NearestLivingEntitySensor),
    NearestPlayers(PlayerSensor),
    HurtBy(HurtBySensor),
    IsInWater(IsInWaterSensor),
    NearestAdult(AdultSensor),
    Tempting(TemptingSensor),
    HoglinSpecific(HoglinSensor),
    PiglinSpecific(PiglinSensor),
    PiglinBruteSpecific(PiglinBruteSensor),
    ArmadilloScareDetected(ArmadilloScareSensor),
    BreezeAttackEntity(BreezeAttackEntitySensor),
    NearestBed(NearestBedSensor),
    SecondaryPois(SecondaryPoiSensor),
    VillagerBabies(VillagerBabiesSensor),
    VillagerHostiles(VillagerHostilesSensor),
    GolemDetected(GolemSensor),
    WardenEntity(WardenSensor),
    FrogAttackables(FrogAttackablesSensor),
}

impl SensorKind {
    pub fn sensor_type(&self) -> SensorType {
        match self {
            Self::Dummy(_) => SensorType::Dummy,
            Self::NearestItems(_) => SensorType::NearestItems,
            Self::NearestLivingEntities(_) => SensorType::NearestLivingEntities,
            Self::NearestPlayers(_) => SensorType::NearestPlayers,
            Self::HurtBy(_) => SensorType::HurtBy,
            Self::IsInWater(_) => SensorType::IsInWater,
            Self::NearestAdult(_) => SensorType::NearestAdult,
            Self::Tempting(sensor) => sensor.sensor_type(),
            Self::HoglinSpecific(_) => SensorType::HoglinSpecificSensor,
            Self::PiglinSpecific(_) => SensorType::PiglinSpecificSensor,
            Self::PiglinBruteSpecific(_) => SensorType::PiglinBruteSpecificSensor,
            Self::ArmadilloScareDetected(_) => SensorType::ArmadilloScareDetected,
            Self::BreezeAttackEntity(_) => SensorType::BreezeAttackEntitySensor,
            Self::NearestBed(_) => SensorType::NearestBed,
            Self::SecondaryPois(_) => SensorType::SecondaryPois,
            Self::VillagerBabies(_) => SensorType::VillagerBabies,
            Self::VillagerHostiles(_) => SensorType::VillagerHostiles,
            Self::GolemDetected(_) => SensorType::GolemDetected,
            Self::WardenEntity(_) => SensorType::WardenEntitySensor,
            Self::FrogAttackables(_) => SensorType::FrogAttackables,
        }
    }
}

impl Sensor for SensorKind {
    fn scan_rate(&self) -> i32 {
        match self {
            Self::Dummy(sensor) => sensor.scan_rate(),
            Self::NearestItems(sensor) => sensor.scan_rate(),
            Self::NearestLivingEntities(sensor) => sensor.scan_rate(),
            Self::NearestPlayers(sensor) => sensor.scan_rate(),
            Self::HurtBy(sensor) => sensor.scan_rate(),
            Self::IsInWater(sensor) => sensor.scan_rate(),
            Self::NearestAdult(sensor) => sensor.scan_rate(),
            Self::Tempting(sensor) => sensor.scan_rate(),
            Self::HoglinSpecific(sensor) => sensor.scan_rate(),
            Self::PiglinSpecific(sensor) => sensor.scan_rate(),
            Self::PiglinBruteSpecific(sensor) => sensor.scan_rate(),
            Self::ArmadilloScareDetected(sensor) => sensor.scan_rate(),
            Self::BreezeAttackEntity(sensor) => sensor.scan_rate(),
            Self::NearestBed(sensor) => sensor.scan_rate(),
            Self::SecondaryPois(sensor) => sensor.scan_rate(),
            Self::VillagerBabies(sensor) => sensor.scan_rate(),
            Self::VillagerHostiles(sensor) => sensor.scan_rate(),
            Self::GolemDetected(sensor) => sensor.scan_rate(),
            Self::WardenEntity(sensor) => sensor.scan_rate(),
            Self::FrogAttackables(sensor) => sensor.scan_rate(),
        }
    }

    fn requires(&self) -> &'static [MemoryModuleType] {
        match self {
            Self::Dummy(sensor) => sensor.requires(),
            Self::NearestItems(sensor) => sensor.requires(),
            Self::NearestLivingEntities(sensor) => sensor.requires(),
            Self::NearestPlayers(sensor) => sensor.requires(),
            Self::HurtBy(sensor) => sensor.requires(),
            Self::IsInWater(sensor) => sensor.requires(),
            Self::NearestAdult(sensor) => sensor.requires(),
            Self::Tempting(sensor) => sensor.requires(),
            Self::HoglinSpecific(sensor) => sensor.requires(),
            Self::PiglinSpecific(sensor) => sensor.requires(),
            Self::PiglinBruteSpecific(sensor) => sensor.requires(),
            Self::ArmadilloScareDetected(sensor) => sensor.requires(),
            Self::BreezeAttackEntity(sensor) => sensor.requires(),
            Self::NearestBed(sensor) => sensor.requires(),
            Self::SecondaryPois(sensor) => sensor.requires(),
            Self::VillagerBabies(sensor) => sensor.requires(),
            Self::VillagerHostiles(sensor) => sensor.requires(),
            Self::GolemDetected(sensor) => sensor.requires(),
            Self::WardenEntity(sensor) => sensor.requires(),
            Self::FrogAttackables(sensor) => sensor.requires(),
        }
    }

    fn do_tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        match self {
            Self::Dummy(sensor) => sensor.do_tick(cx, memories),
            Self::NearestItems(sensor) => sensor.do_tick(cx, memories),
            Self::NearestLivingEntities(sensor) => sensor.do_tick(cx, memories),
            Self::NearestPlayers(sensor) => sensor.do_tick(cx, memories),
            Self::HurtBy(sensor) => sensor.do_tick(cx, memories),
            Self::IsInWater(sensor) => sensor.do_tick(cx, memories),
            Self::NearestAdult(sensor) => sensor.do_tick(cx, memories),
            Self::Tempting(sensor) => sensor.do_tick(cx, memories),
            Self::HoglinSpecific(sensor) => sensor.do_tick(cx, memories),
            Self::PiglinSpecific(sensor) => sensor.do_tick(cx, memories),
            Self::PiglinBruteSpecific(sensor) => sensor.do_tick(cx, memories),
            Self::ArmadilloScareDetected(sensor) => sensor.do_tick(cx, memories),
            Self::BreezeAttackEntity(sensor) => sensor.do_tick(cx, memories),
            Self::NearestBed(sensor) => sensor.do_tick(cx, memories),
            Self::SecondaryPois(sensor) => sensor.do_tick(cx, memories),
            Self::VillagerBabies(sensor) => sensor.do_tick(cx, memories),
            Self::VillagerHostiles(sensor) => sensor.do_tick(cx, memories),
            Self::GolemDetected(sensor) => sensor.do_tick(cx, memories),
            Self::WardenEntity(sensor) => sensor.do_tick(cx, memories),
            Self::FrogAttackables(sensor) => sensor.do_tick(cx, memories),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SensorSlot {
    sensor: SensorKind,
    time_to_tick: i64,
}

impl SensorSlot {
    pub fn new(sensor: SensorKind, random: &mut impl RandomSource) -> Self {
        let time_to_tick = i64::from(random.next_int(sensor.scan_rate()));
        Self {
            sensor,
            time_to_tick,
        }
    }

    pub fn sensor(&self) -> &SensorKind {
        &self.sensor
    }

    pub fn time_to_tick(&self) -> i64 {
        self.time_to_tick
    }

    pub fn tick<W: AiWorld>(&mut self, cx: &mut AiContext<'_, W>, memories: &mut Memories) {
        self.time_to_tick -= 1;
        if self.time_to_tick < 1 {
            self.time_to_tick = i64::from(self.sensor.scan_rate());
            self.sensor.do_tick(cx, memories);
        }
    }
}

fn conditions(
    combat: bool,
    ignore_line_of_sight: bool,
    ignore_invisibility: bool,
    range: f64,
) -> TargetingConditions {
    let mut conditions = if combat {
        TargetingConditions::for_combat()
    } else {
        TargetingConditions::for_non_combat()
    }
    .range(range);
    if ignore_line_of_sight {
        conditions = conditions.ignore_line_of_sight();
    }
    if ignore_invisibility {
        conditions = conditions.ignore_invisibility_testing();
    }
    conditions
}

fn is_attack_target(memories: &Memories, entity: &Living) -> bool {
    memories.is_value(
        MemoryModuleType::AttackTarget,
        &MemoryValue::Entity(entity.id),
    )
}

pub fn is_entity_targetable<W: AiWorld>(
    cx: &mut AiContext<'_, W>,
    memories: &Memories,
    entity: &Living,
) -> bool {
    let range = cx.mob.follow_range;
    conditions(false, false, is_attack_target(memories, entity), range).test(cx, entity, |_| true)
}

pub fn is_entity_attackable<W: AiWorld>(
    cx: &mut AiContext<'_, W>,
    memories: &Memories,
    target: &Living,
) -> bool {
    let range = cx.mob.follow_range;
    conditions(true, false, is_attack_target(memories, target), range).test(cx, target, |_| true)
}

pub fn is_entity_attackable_ignoring_line_of_sight<W: AiWorld>(
    cx: &mut AiContext<'_, W>,
    memories: &Memories,
    target: &Living,
) -> bool {
    let range = cx.mob.follow_range;
    conditions(true, true, is_attack_target(memories, target), range).test(cx, target, |_| true)
}

pub fn nearest_visible_entity<W: AiWorld>(
    cx: &mut AiContext<'_, W>,
    memories: &mut Memories,
    memory_to_set: MemoryModuleType,
    is_matching_entity: impl Fn(&Living) -> bool,
) {
    let found = memories
        .get(MemoryModuleType::NearestVisibleLivingEntities)
        .and_then(MemoryValue::as_visible)
        .and_then(|visible| visible.find_closest(cx, memories, is_matching_entity));
    memories.set_option(
        memory_to_set,
        found.map(|living| MemoryValue::Entity(living.id)),
    );
}
