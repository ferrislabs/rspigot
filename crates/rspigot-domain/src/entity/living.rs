use std::borrow::Cow;

use rspigot_entities::EntityKind;
use rspigot_items::EquipmentSlot;

use super::ageable::AgeableState;
use super::base::EntityBase;
use super::data::{Pose, SyncedValue};
use super::fluid;
use super::neutral::{self, NeutralState};
use super::physics::{
    self, BlockView, Landing, MoveTraits, apply_inside_blocks, block_jump_factor, friction,
    has_tag, move_entity, movement_affecting_pos,
};
use super::species::Species;
use super::species::villager::MerchantState;
use super::tamable::{self, TamableState};
use crate::attribute;
use crate::combat::{CombatEntry, CombatTracker};
use crate::damage::{self, DamageTag, DamageType, java_clamp};
use crate::effect::{ActiveEffects, AttributeModifier, AttributeState, EffectId, EffectInstance};
use crate::item::ItemStack;
use crate::player::PlayerId;
use crate::text::Component;
use crate::{BlockPos, EntityId, Vec3, mth};

pub const DATA_LIVING_FLAGS: u8 = 8;
pub const DATA_HEALTH: u8 = 9;
pub const DATA_EFFECT_AMBIENCE: u8 = 11;
pub const DATA_ARROW_COUNT: u8 = 12;
pub const DATA_STINGER_COUNT: u8 = 13;
pub const DATA_MOB_FLAGS: u8 = 15;
pub const MOB_FLAG_NO_AI: u8 = 1;
pub const MOB_FLAG_LEFT_HANDED: u8 = 2;
pub const MOB_FLAG_AGGRESSIVE: u8 = 4;
pub const DATA_SPIDER_FLAGS: u8 = 16;
pub const SPIDER_FLAG_CLIMBING: u8 = 1;
pub const DEATH_DURATION: i32 = 20;
pub const HURT_DURATION: i32 = 10;
pub const DAMAGE_COOLDOWN: i32 = 20;
pub const PLAYER_HURT_MEMORY: i32 = 100;
pub const MOB_HURT_MEMORY: i32 = 100;
pub const ARMOR: &str = "minecraft:armor";
pub const ARMOR_TOUGHNESS: &str = "minecraft:armor_toughness";
pub const MAX_ABSORPTION: &str = "minecraft:max_absorption";
pub const KNOCKBACK_RESISTANCE: &str = "minecraft:knockback_resistance";
pub const ATTACK_DAMAGE: &str = "minecraft:attack_damage";
pub const ATTACK_KNOCKBACK: &str = "minecraft:attack_knockback";
pub const SAFE_FALL_DISTANCE: &str = "minecraft:safe_fall_distance";
pub const FALL_DAMAGE_MULTIPLIER: &str = "minecraft:fall_damage_multiplier";
pub const COOLDOWN_THRESHOLD: f32 = 10.0;
const HELMET_FACTOR: f32 = 0.75;
const FALL_DAMAGE_IMMUNE: &str = "minecraft:fall_damage_immune";
const ENTITY_TYPES: &str = "minecraft:entity_type";
pub const DEFAULT_DROP_CHANCE: f32 = 0.085;
pub const RANDOM_SPAWN_BONUS: &str = "minecraft:random_spawn_bonus";
pub const MAX_HEALTH: &str = "minecraft:max_health";
pub const FOLLOW_RANGE: &str = "minecraft:follow_range";
pub const GRAVITY: &str = "minecraft:gravity";
pub const STEP_HEIGHT: &str = "minecraft:step_height";
pub const FRICTION_MODIFIER: &str = "minecraft:friction_modifier";
pub const AIR_DRAG_MODIFIER: &str = "minecraft:air_drag_modifier";
pub const BOUNCINESS: &str = "minecraft:bounciness";
pub const MOVEMENT_EFFICIENCY: &str = "minecraft:movement_efficiency";
pub const WATER_MOVEMENT_EFFICIENCY: &str = "minecraft:water_movement_efficiency";
pub const JUMP_STRENGTH: &str = "minecraft:jump_strength";
const VELOCITY_THRESHOLD: f64 = 0.003;
const AIR_FRICTION: f32 = 0.91;
const AIR_DRAG: f32 = 0.98;
const FLYING_SPEED: f32 = 0.02;
const FLYING_WATER_DRAG: f64 = 0.8_f32 as f64;
const FLYING_LAVA_DRAG: f64 = 0.5;
const FLYING_AIR_DRAG: f64 = 0.91_f32 as f64;
const GHAST_FLYING_SPEEDS: FlyingSpeeds = FlyingSpeeds {
    water: 0.02,
    lava: 0.02,
    air: 0.02,
};
const PHANTOM_FLYING_SPEEDS: FlyingSpeeds = FlyingSpeeds {
    water: 0.02,
    lava: 0.02,
    air: 0.2,
};
const SLOW_FALLING_GRAVITY: f64 = 0.01;
const LEVITATION_SPEED: f64 = 0.05;
const LEVITATION_BLEND: f64 = 0.2;
const CLIMB_SPEED: f64 = 0.15000000596046448;
const CLIMB_BOOST: f64 = 0.2;
const APPLIED_INPUT_DECAY: f32 = 0.98;
const FLUID_SPEED: f32 = 0.02;
const WATER_SLOW_DOWN: f32 = 0.8;
const POLAR_BEAR_WATER_SLOW_DOWN: f32 = 0.98;
const WATER_EFFICIENT_SLOW_DOWN: f32 = 0.546_000_06;
const DOLPHIN_SLOW_DOWN: f32 = 0.96;
const FLUID_VERTICAL_DRAG: f64 = 0.800_000_011_920_929;
const LIQUID_JUMP_IMPULSE: f64 = 0.039_999_999_105_930_33;
const NO_JUMP_DELAY: i32 = 10;
const FLAG_SPRINTING: u8 = 8;
const SPRINT_JUMP_BOOST: f64 = 0.2;
const SPRINT_SPEED_BONUS: f64 = 0.300_000_011_920_928_96;
const SPRINTING_MODIFIER: &str = "minecraft:sprinting";
pub const MOVEMENT_SPEED: &str = "minecraft:movement_speed";
const HOP_SPEED: f32 = 0.1;
const HOP_HORIZONTAL_LIMIT: f64 = 0.01;
const JUMP_OUT_STEP: f64 = 0.600_000_023_841_857_9;
const JUMP_OUT_SPEED: f64 = 0.300_000_011_920_929;
const MIN_JUMP_POWER: f32 = 1.0e-5;
const JUMP_BOOST_STEP: f32 = 0.1;

const SYNCABLE: [&str; 32] = [
    "minecraft:air_drag_modifier",
    "minecraft:armor",
    "minecraft:armor_toughness",
    "minecraft:attack_speed",
    "minecraft:below_name_distance",
    "minecraft:block_break_speed",
    "minecraft:block_interaction_range",
    "minecraft:bounciness",
    "minecraft:burning_time",
    "minecraft:camera_distance",
    "minecraft:explosion_knockback_resistance",
    "minecraft:entity_interaction_range",
    "minecraft:fall_damage_multiplier",
    "minecraft:flying_speed",
    "minecraft:friction_modifier",
    "minecraft:gravity",
    "minecraft:jump_strength",
    "minecraft:luck",
    "minecraft:max_absorption",
    "minecraft:max_health",
    "minecraft:mining_efficiency",
    "minecraft:movement_efficiency",
    "minecraft:movement_speed",
    "minecraft:name_tag_distance",
    "minecraft:oxygen_bonus",
    "minecraft:safe_fall_distance",
    "minecraft:scale",
    "minecraft:sneaking_speed",
    "minecraft:step_height",
    "minecraft:submerged_mining_speed",
    "minecraft:sweeping_damage_ratio",
    "minecraft:water_movement_efficiency",
];

pub fn is_client_syncable(attribute: &str) -> bool {
    SYNCABLE.contains(&attribute)
}

pub fn compute_modified_friction(friction: f32, modifier: f32) -> f32 {
    (1.0 - (1.0 - friction) * modifier).clamp(0.0, 1.0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttributeInstance {
    pub attribute: &'static str,
    pub base: f64,
    pub modifiers: Vec<AttributeModifier>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LivingAttributes {
    kind: EntityKind,
    instances: Vec<AttributeInstance>,
    to_sync: Vec<&'static str>,
}

impl LivingAttributes {
    pub fn new(kind: EntityKind) -> Self {
        Self {
            kind,
            instances: Vec::new(),
            to_sync: Vec::new(),
        }
    }

    fn supplier(&self, name: &str) -> Option<(&'static str, f64)> {
        self.kind
            .default_attributes()
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .copied()
    }

    pub fn instance(&mut self, name: &str) -> Option<&mut AttributeInstance> {
        if let Some(index) = self.instances.iter().position(|i| i.attribute == name) {
            return self.instances.get_mut(index);
        }
        let (attribute, base) = self.supplier(name)?;
        self.instances.push(AttributeInstance {
            attribute,
            base,
            modifiers: Vec::new(),
        });
        self.instances.last_mut()
    }

    pub fn instances(&self) -> &[AttributeInstance] {
        &self.instances
    }

    pub fn state(&self, name: &str, effects: &ActiveEffects) -> Option<AttributeState> {
        let instance = self.instances.iter().find(|i| i.attribute == name)?;
        let mut modifiers = instance.modifiers.clone();
        modifiers.extend(effect_modifiers(effects, instance.attribute));
        Some(AttributeState {
            attribute: instance.attribute,
            base: instance.base,
            modifiers,
        })
    }

    pub fn value(&mut self, name: &str, effects: &ActiveEffects) -> f64 {
        if self.instance(name).is_none() {
            return 0.0;
        }
        self.state(name, effects)
            .map_or(0.0, |state| attribute::value(&state))
    }

    pub fn set_base(&mut self, name: &str, base: f64) -> bool {
        let Some(instance) = self.instance(name) else {
            return false;
        };
        if instance.base == base {
            return true;
        }
        instance.base = base;
        let attribute = instance.attribute;
        self.mark_dirty(attribute);
        true
    }

    pub fn add_modifier(&mut self, name: &str, modifier: AttributeModifier) -> bool {
        let Some(instance) = self.instance(name) else {
            return false;
        };
        if instance.modifiers.iter().any(|m| m.id == modifier.id) {
            return false;
        }
        instance.modifiers.push(modifier);
        let attribute = instance.attribute;
        self.mark_dirty(attribute);
        true
    }

    pub fn remove_modifier(&mut self, name: &str, id: &str) -> bool {
        let Some(index) = self.instances.iter().position(|i| i.attribute == name) else {
            return false;
        };
        let instance = &mut self.instances[index];
        let before = instance.modifiers.len();
        instance.modifiers.retain(|m| m.id != id);
        if instance.modifiers.len() == before {
            return false;
        }
        let attribute = instance.attribute;
        self.mark_dirty(attribute);
        true
    }

    pub fn add_or_replace_modifier(&mut self, name: &str, modifier: AttributeModifier) -> bool {
        self.remove_modifier(name, &modifier.id);
        self.add_modifier(name, modifier)
    }

    pub fn modifier_amount(&self, name: &str, id: &str) -> Option<f64> {
        self.instances
            .iter()
            .find(|i| i.attribute == name)?
            .modifiers
            .iter()
            .find(|m| m.id == id)
            .map(|m| m.amount)
    }

    pub fn has_modifier(&self, name: &str, id: &str) -> bool {
        self.instances
            .iter()
            .any(|i| i.attribute == name && i.modifiers.iter().any(|m| m.id == id))
    }

    pub fn mark_dirty(&mut self, attribute: &'static str) {
        if is_client_syncable(attribute) && !self.to_sync.contains(&attribute) {
            self.to_sync.push(attribute);
        }
    }

    pub fn take_dirty(&mut self) -> Vec<&'static str> {
        std::mem::take(&mut self.to_sync)
    }

    pub fn syncable(&self, effects: &ActiveEffects) -> Vec<AttributeState> {
        self.instances
            .iter()
            .filter(|i| is_client_syncable(i.attribute))
            .filter_map(|i| self.state(i.attribute, effects))
            .collect()
    }
}

fn effect_modifiers(
    effects: &ActiveEffects,
    attribute: &str,
) -> impl Iterator<Item = AttributeModifier> {
    let found: Vec<AttributeModifier> = effects
        .iter()
        .filter_map(|e| Some((e.amplifier, e.effect.kind()?.modifiers())))
        .flat_map(|(amplifier, templates)| templates.iter().map(move |t| (amplifier, t)))
        .filter(|(_, t)| t.attribute == attribute)
        .map(|(amplifier, t)| AttributeModifier {
            id: Cow::Borrowed(t.modifier),
            amount: t.amount_for(amplifier),
            operation: t.operation,
        })
        .collect();
    found.into_iter()
}

#[derive(Debug, Clone, PartialEq)]
pub struct MobState {
    pub can_pick_up_loot: bool,
    pub persistence_required: bool,
    pub drop_chances: [f32; 8],
    pub death_loot_table: Option<String>,
    pub death_loot_table_seed: i64,
    pub aware: bool,
    pub home_pos: BlockPos,
    pub home_radius: i32,
    pub ageable: Option<AgeableState>,
    pub neutral: Option<NeutralState>,
    pub tamable: Option<TamableState>,
    pub merchant: Option<Box<MerchantState>>,
    pub species: Species,
}

impl Default for MobState {
    fn default() -> Self {
        Self {
            can_pick_up_loot: false,
            persistence_required: false,
            drop_chances: [DEFAULT_DROP_CHANCE; 8],
            death_loot_table: None,
            death_loot_table_seed: 0,
            aware: true,
            home_pos: BlockPos::new(0, 0, 0),
            home_radius: -1,
            ageable: None,
            neutral: None,
            tamable: None,
            merchant: None,
            species: Species::Plain,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct DamageMemory {
    pub cooldown: i32,
    pub hurt_duration: i32,
    pub last_hurt_by_mob: Option<EntityId>,
    pub last_hurt_by_mob_at: i32,
    pub last_hurt_by_player: Option<PlayerId>,
    pub player_memory: i32,
    pub last_hurt_mob: Option<EntityId>,
    pub last_hurt_mob_at: i32,
    pub last_source: Option<(DamageType, i64)>,
    pub last_source_entity: Option<EntityId>,
    pub combat: CombatTracker,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JumpTuning {
    pub multiplier: f32,
    pub hop: Option<f32>,
}

impl Default for JumpTuning {
    fn default() -> Self {
        Self {
            multiplier: 1.0,
            hop: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LivingState {
    pub absorption: f32,
    pub hurt_time: i32,
    pub death_time: i32,
    pub dead: bool,
    pub dying: bool,
    pub body_yaw: f32,
    pub attributes: LivingAttributes,
    pub effects: ActiveEffects,
    pub equipment: [Option<ItemStack>; 8],
    pub fall_flying: bool,
    pub impulse_grace: i32,
    pub speed: f32,
    pub jumping: bool,
    pub xxa: f32,
    pub yya: f32,
    pub zza: f32,
    pub no_jump_delay: i32,
    pub jump_tuning: JumpTuning,
    pub jumped: bool,
    pub no_action_time: i32,
    pub last_hurt: f32,
    pub damage: DamageMemory,
    pub mob: Option<MobState>,
    pub using_item: Option<(EquipmentSlot, i32)>,
    pub swing: Option<(i32, i32)>,
    pub remove_arrow_time: i32,
    pub discard_friction: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HurtOutcome {
    Ignored,
    Partial,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HurtSource<'a> {
    pub kind: DamageType,
    pub causing: Option<&'a Component>,
    pub direct: Option<&'a Component>,
    pub living_cause: bool,
    pub creative_attacker: bool,
    pub reflected_fireball: bool,
    pub self_inflicted: bool,
}

impl HurtSource<'_> {
    pub const fn plain(kind: DamageType) -> Self {
        Self {
            kind,
            causing: None,
            direct: None,
            living_cause: false,
            creative_attacker: false,
            reflected_fireball: false,
            self_inflicted: false,
        }
    }
}

pub fn slot_index(slot: EquipmentSlot) -> usize {
    slot as usize
}

pub const EQUIPMENT_SLOTS: [EquipmentSlot; 8] = [
    EquipmentSlot::Mainhand,
    EquipmentSlot::Offhand,
    EquipmentSlot::Feet,
    EquipmentSlot::Legs,
    EquipmentSlot::Chest,
    EquipmentSlot::Head,
    EquipmentSlot::Body,
    EquipmentSlot::Saddle,
];

impl LivingState {
    pub fn new(base: &mut EntityBase, initial_yaw: f32) -> Self {
        let kind = base.kind;
        base.data.define(DATA_LIVING_FLAGS, SyncedValue::Byte(0));
        base.data.define(DATA_HEALTH, SyncedValue::Float(1.0));
        base.data
            .define(DATA_EFFECT_AMBIENCE, SyncedValue::Boolean(false));
        base.data.define(DATA_ARROW_COUNT, SyncedValue::Int(0));
        base.data.define(DATA_STINGER_COUNT, SyncedValue::Int(0));
        let is_mob = kind.extends("Mob");
        if is_mob {
            base.data.define(DATA_MOB_FLAGS, SyncedValue::Byte(0));
        }
        let mut mob = is_mob.then(MobState::default);
        if let Some(mob) = &mut mob {
            if kind.extends("AgeableMob") {
                mob.ageable = Some(AgeableState::new(base));
            }
            if kind.extends("TamableAnimal") {
                tamable::define(base);
                mob.tamable = Some(TamableState::default());
            }
            if neutral::is_neutral(kind) {
                if let Some(index) = neutral::synced_end_time_index(kind) {
                    base.data
                        .define(index, SyncedValue::Long(neutral::NO_ANGER_END_TIME));
                }
                mob.neutral = Some(NeutralState::default());
            }
            if kind.extends("AbstractVillager") {
                mob.merchant = Some(Box::new(MerchantState::new()));
            }
            mob.species = Species::new(base);
            mob.persistence_required = kind == EntityKind::CopperGolem;
            mob.can_pick_up_loot = matches!(
                kind,
                EntityKind::Fox
                    | EntityKind::Panda
                    | EntityKind::Piglin
                    | EntityKind::PiglinBrute
                    | EntityKind::Villager
            );
        }
        if kind.extends("Spider") {
            base.data.define(DATA_SPIDER_FLAGS, SyncedValue::Byte(0));
        }
        let mut state = Self {
            absorption: 0.0,
            hurt_time: 0,
            death_time: 0,
            dead: false,
            dying: false,
            body_yaw: 0.0,
            attributes: LivingAttributes::new(kind),
            effects: ActiveEffects::default(),
            equipment: Default::default(),
            fall_flying: false,
            impulse_grace: 0,
            speed: 0.0,
            jumping: false,
            xxa: 0.0,
            yya: 0.0,
            zza: 0.0,
            no_jump_delay: 0,
            jump_tuning: JumpTuning::default(),
            jumped: false,
            no_action_time: 0,
            last_hurt: 0.0,
            damage: DamageMemory::default(),
            mob,
            using_item: None,
            swing: None,
            remove_arrow_time: 0,
            discard_friction: false,
        };
        let max = state.max_health() as f32;
        base.data.set(DATA_HEALTH, SyncedValue::Float(max));
        state.attributes.take_dirty();
        base.set_yaw(initial_yaw);
        base.head_yaw = base.yaw;
        if is_mob {
            let _ = state.attributes.instance(FOLLOW_RANGE);
        }
        state
    }

    pub fn attribute(&mut self, name: &str) -> f64 {
        self.attributes.value(name, &self.effects)
    }

    pub fn max_health(&mut self) -> f64 {
        self.attribute(MAX_HEALTH)
    }

    pub fn is_mob(&self) -> bool {
        self.mob.is_some()
    }

    pub fn equipment(&self, slot: EquipmentSlot) -> Option<&ItemStack> {
        self.equipment[slot_index(slot)].as_ref()
    }

    pub fn has_effect(&self, name: &str) -> bool {
        self.effects.has(name)
    }

    pub fn effect(&self, name: &str) -> Option<&EffectInstance> {
        EffectId::from_name(name).and_then(|id| self.effects.get(id))
    }

    pub fn effective_gravity(&mut self, base: &EntityBase) -> f64 {
        let gravity = if base.is_no_gravity() {
            0.0
        } else {
            self.attribute(GRAVITY)
        };
        if base.velocity.y > 0.0 || !self.has_effect("minecraft:slow_falling") {
            gravity
        } else {
            gravity.min(SLOW_FALLING_GRAVITY)
        }
    }

    pub fn move_traits(&mut self, base: &EntityBase) -> MoveTraits {
        let air_drag =
            compute_modified_friction(AIR_DRAG, self.attribute(AIR_DRAG_MODIFIER) as f32);
        MoveTraits {
            max_up_step: self.attribute(STEP_HEIGHT) as f32,
            living: true,
            bounciness: self.attribute(BOUNCINESS),
            suppress_bounce: false,
            gravity: self.effective_gravity(base),
            air_drag,
            movement_efficiency: Some(self.attribute(MOVEMENT_EFFICIENCY) as f32),
        }
    }
}

pub fn health(base: &EntityBase) -> f32 {
    base.data.float(DATA_HEALTH)
}

pub fn set_health(base: &mut EntityBase, living: &mut LivingState, health: f32) {
    let max = living.max_health() as f32;
    base.data
        .set(DATA_HEALTH, SyncedValue::Float(health.clamp(0.0, max)));
}

pub fn is_no_ai(base: &EntityBase) -> bool {
    base.data.flag(DATA_MOB_FLAGS, MOB_FLAG_NO_AI)
}

pub fn set_no_ai(base: &mut EntityBase, no_ai: bool) {
    base.data.set_flag(DATA_MOB_FLAGS, MOB_FLAG_NO_AI, no_ai);
}

pub fn is_left_handed(base: &EntityBase) -> bool {
    base.data.flag(DATA_MOB_FLAGS, MOB_FLAG_LEFT_HANDED)
}

pub const LIVING_FLAG_USING: u8 = 1;
pub const LIVING_FLAG_OFFHAND: u8 = 2;
pub const SWING_DURATION: i32 = 6;
const ARROW_REMOVAL_BASE: i32 = 30;

pub fn start_using_item(base: &mut EntityBase, living: &mut LivingState, slot: EquipmentSlot) {
    if living.equipment(slot).is_none() || living.using_item.is_some() {
        return;
    }
    living.using_item = Some((slot, 0));
    base.data
        .set_flag(DATA_LIVING_FLAGS, LIVING_FLAG_USING, true);
    base.data.set_flag(
        DATA_LIVING_FLAGS,
        LIVING_FLAG_OFFHAND,
        slot == EquipmentSlot::Offhand,
    );
}

pub fn stop_using_item(base: &mut EntityBase, living: &mut LivingState) {
    base.data
        .set_flag(DATA_LIVING_FLAGS, LIVING_FLAG_USING, false);
    living.using_item = None;
}

pub fn tick_using_item(base: &mut EntityBase, living: &mut LivingState) {
    let Some((slot, ticks)) = living.using_item else {
        return;
    };
    if living.equipment(slot).is_none() {
        stop_using_item(base, living);
        return;
    }
    living.using_item = Some((slot, ticks + 1));
}

pub fn start_swing(living: &mut LivingState) -> bool {
    if let Some((ticks, duration)) = living.swing
        && ticks <= duration / 2
        && ticks > 0
    {
        return false;
    }
    living.swing = Some((0, SWING_DURATION));
    true
}

pub fn tick_swing(living: &mut LivingState) {
    if let Some((ticks, duration)) = living.swing {
        living.swing = if ticks > duration {
            None
        } else {
            Some((ticks + 1, duration))
        };
    }
}

pub fn arrow_count(base: &EntityBase) -> i32 {
    base.data.int(DATA_ARROW_COUNT)
}

pub fn set_arrow_count(base: &mut EntityBase, count: i32) {
    base.data.set(DATA_ARROW_COUNT, SyncedValue::Int(count));
}

pub fn tick_arrows(base: &mut EntityBase, living: &mut LivingState) {
    let arrows = arrow_count(base);
    if arrows <= 0 {
        return;
    }
    if living.remove_arrow_time <= 0 {
        living.remove_arrow_time = 20 * (ARROW_REMOVAL_BASE - arrows);
    }
    living.remove_arrow_time -= 1;
    if living.remove_arrow_time <= 0 {
        set_arrow_count(base, arrows - 1);
    }
}

pub fn is_aggressive(base: &EntityBase) -> bool {
    base.data.flag(DATA_MOB_FLAGS, MOB_FLAG_AGGRESSIVE)
}

pub fn set_aggressive(base: &mut EntityBase, aggressive: bool) {
    base.data
        .set_flag(DATA_MOB_FLAGS, MOB_FLAG_AGGRESSIVE, aggressive);
}

pub fn set_left_handed(base: &mut EntityBase, left: bool) {
    base.data
        .set_flag(DATA_MOB_FLAGS, MOB_FLAG_LEFT_HANDED, left);
}

pub fn is_dead_or_dying(base: &EntityBase, living: &LivingState) -> bool {
    living.dead || health(base) <= 0.0
}

pub fn is_sensitive_to_water(kind: rspigot_entities::EntityKind) -> bool {
    use rspigot_entities::EntityKind;
    matches!(
        kind,
        EntityKind::SnowGolem | EntityKind::Enderman | EntityKind::Strider | EntityKind::Blaze
    )
}

pub fn fall_damage_immune(kind: rspigot_entities::EntityKind) -> bool {
    rspigot_data::tags::tag(ENTITY_TYPES, FALL_DAMAGE_IMMUNE)
        .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
}

pub fn is_invulnerable(base: &EntityBase) -> bool {
    base.invulnerable || base.invulnerable_time > 0
}

pub fn is_invulnerable_to(base: &EntityBase, source: &HurtSource<'_>) -> bool {
    let kind = source.kind;
    let reflected = base.kind == EntityKind::Ghast && source.reflected_fireball;
    base.is_removed()
        || (is_invulnerable(base)
            && !kind.is(DamageTag::BypassesInvulnerability)
            && !source.creative_attacker)
        || (!reflected
            && ((kind.is(DamageTag::IsFire) && base.kind.fire_immune())
                || (kind.is(DamageTag::IsFall) && fall_damage_immune(base.kind))))
}

fn after_magic(living: &LivingState, kind: DamageType, amount: f32) -> f32 {
    if kind.is(DamageTag::BypassesEffects) {
        return amount;
    }
    let mut amount = amount;
    if let Some(resistance) = living.effect("minecraft:resistance")
        && !kind.is(DamageTag::BypassesResistance)
    {
        amount = damage::after_resistance(amount, resistance.amplifier);
    }
    if amount <= 0.0 { 0.0 } else { amount }
}

fn species_absorb(
    living: &LivingState,
    kind: DamageType,
    self_inflicted: bool,
    amount: f32,
) -> f32 {
    match living.mob.as_ref().map(|mob| &mob.species) {
        Some(Species::Witch(_)) => {
            super::species::witch::magic_absorb(amount, kind, self_inflicted)
        }
        _ => amount,
    }
}

pub fn actually_hurt(
    base: &mut EntityBase,
    living: &mut LivingState,
    source: &HurtSource<'_>,
    amount: f32,
    now: u64,
) -> f32 {
    if is_invulnerable_to(base, source) {
        return 0.0;
    }
    let kind = source.kind;
    let mut amount = amount;
    if !kind.is(DamageTag::BypassesArmor) {
        let armor = living.attribute(ARMOR).floor() as f32;
        let toughness = living.attribute(ARMOR_TOUGHNESS) as f32;
        amount = damage::after_armor(amount, armor, toughness);
    }
    amount = after_magic(living, kind, amount);
    amount = species_absorb(living, kind, source.self_inflicted, amount);
    let max_absorption = living.attribute(MAX_ABSORPTION) as f32;
    let before = amount;
    amount = (amount - living.absorption).max(0.0);
    living.absorption = java_clamp(living.absorption - (before - amount), 0.0, max_absorption);
    if amount == 0.0 {
        return 0.0;
    }
    let alive = health(base) > 0.0;
    living.damage.combat.record(
        CombatEntry {
            kind,
            causing: source.causing.cloned(),
            direct: source.direct.cloned(),
            living_cause: source.living_cause,
            damage: amount,
            fall_distance: base.fall_distance as f32,
            location: None,
        },
        now,
        alive,
    );
    let current = health(base);
    set_health(base, living, current - amount);
    living.absorption = java_clamp(living.absorption - amount, 0.0, max_absorption);
    amount
}

pub fn can_hurt(base: &EntityBase, living: &LivingState, source: &HurtSource<'_>) -> bool {
    !(base.is_removed()
        || is_dead_or_dying(base, living)
        || is_invulnerable_to(base, source)
        || (source.kind.is(DamageTag::IsFire) && living.has_effect("minecraft:fire_resistance")))
}

fn helmeted(living: &LivingState, kind: DamageType, amount: f32) -> f32 {
    let mut amount = amount;
    if kind.is(DamageTag::DamagesHelmet) && living.equipment(EquipmentSlot::Head).is_some() {
        amount *= HELMET_FACTOR;
    }
    if !amount.is_finite() {
        amount = f32::MAX;
    }
    amount
}

pub fn final_damage(
    living: &mut LivingState,
    kind: DamageType,
    self_inflicted: bool,
    amount: f32,
) -> f32 {
    let mut amount = helmeted(living, kind, floored(amount));
    if !kind.is(DamageTag::BypassesArmor) {
        let armor = living.attribute(ARMOR).floor() as f32;
        let toughness = living.attribute(ARMOR_TOUGHNESS) as f32;
        amount = damage::after_armor(amount, armor, toughness);
    }
    amount = after_magic(living, kind, amount);
    amount = species_absorb(living, kind, self_inflicted, amount);
    (amount - living.absorption).max(0.0)
}

pub fn hurt(
    base: &mut EntityBase,
    living: &mut LivingState,
    source: &HurtSource<'_>,
    amount: f32,
    now: u64,
) -> HurtOutcome {
    if !can_hurt(base, living, source) {
        return HurtOutcome::Ignored;
    }
    living.no_action_time = 0;
    hurt_settled(base, living, source, floored(amount), now)
}

pub fn floored(amount: f32) -> f32 {
    if amount < 0.0 { 0.0 } else { amount }
}

pub fn hurt_settled(
    base: &mut EntityBase,
    living: &mut LivingState,
    source: &HurtSource<'_>,
    amount: f32,
    now: u64,
) -> HurtOutcome {
    let kind = source.kind;
    let amount = helmeted(living, kind, amount);
    if living.damage.cooldown as f32 > COOLDOWN_THRESHOLD && !kind.is(DamageTag::BypassesCooldown) {
        if amount <= living.last_hurt {
            return HurtOutcome::Ignored;
        }
        actually_hurt(base, living, source, amount - living.last_hurt, now);
        living.last_hurt = amount;
        HurtOutcome::Partial
    } else {
        living.last_hurt = amount;
        living.damage.cooldown = DAMAGE_COOLDOWN;
        actually_hurt(base, living, source, amount, now);
        living.damage.hurt_duration = HURT_DURATION;
        living.hurt_time = HURT_DURATION;
        HurtOutcome::Full
    }
}

pub fn remember_attacker(
    living: &mut LivingState,
    kind: DamageType,
    mob: Option<EntityId>,
    player: Option<PlayerId>,
    tick_count: i32,
) {
    if let Some(mob) = mob
        && !kind.is(DamageTag::NoAnger)
    {
        living.damage.last_hurt_by_mob = Some(mob);
        living.damage.last_hurt_by_mob_at = tick_count;
    }
    if let Some(player) = player {
        living.damage.last_hurt_by_player = Some(player);
        living.damage.player_memory = PLAYER_HURT_MEMORY;
    }
}

pub fn die(base: &mut EntityBase, living: &mut LivingState) {
    if base.is_removed() || living.dead {
        return;
    }
    living.dead = true;
    base.set_pose(Pose::Dying);
}

pub fn fall_damage(
    base: &EntityBase,
    living: &mut LivingState,
    distance: f64,
    multiplier: f32,
) -> i32 {
    if fall_damage_immune(base.kind) {
        return 0;
    }
    let reduction = match base.kind {
        EntityKind::Goat => super::species::goat::EXTRA_FALL_DAMAGE_REDUCTION,
        EntityKind::Frog => super::species::frog::FALL_DAMAGE_REDUCTION,
        _ => 0,
    };
    damage::fall_damage(
        distance,
        multiplier,
        living.attribute(SAFE_FALL_DISTANCE),
        living.attribute(FALL_DAMAGE_MULTIPLIER),
    ) - reduction
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LivingTick {
    pub burn: bool,
    pub died: bool,
    pub removed: bool,
    pub below_world: bool,
}

pub fn base_tick(base: &mut EntityBase, living: &mut LivingState) -> LivingTick {
    let mut outcome = LivingTick::default();
    tick_swing(living);
    outcome.burn = super::fire::base_tick(base).burn;
    let on_fire = base.remaining_fire_ticks > 0;
    super::fire::set_on_fire_flag(base, on_fire);
    let air = base.air_supply();
    let max_air = base.max_air_ticks;
    if air < max_air {
        base.set_air_supply((air + 4).min(max_air));
    }
    if living.hurt_time > 0 {
        living.hurt_time -= 1;
    }
    if base.invulnerable_time > 0 {
        base.invulnerable_time -= 1;
    }
    if living.damage.cooldown > 0 {
        living.damage.cooldown -= 1;
    }
    if is_dead_or_dying(base, living) && !living.dying {
        living.death_time += 1;
        if living.death_time >= DEATH_DURATION && !base.is_removed() {
            outcome.removed = true;
        }
    }
    let memory = &mut living.damage;
    if memory.player_memory > 0 {
        memory.player_memory -= 1;
    } else {
        memory.last_hurt_by_player = None;
    }
    if memory.last_hurt_by_mob.is_some()
        && base.tick_count - memory.last_hurt_by_mob_at > MOB_HURT_MEMORY
    {
        memory.last_hurt_by_mob = None;
    }
    outcome
}

pub fn mark_effect_attributes(living: &mut LivingState, effect: EffectId) {
    let Some(kind) = effect.kind() else {
        return;
    };
    for template in kind.modifiers() {
        if let Some(instance) = living.attributes.instance(template.attribute) {
            let attribute = instance.attribute;
            living.attributes.mark_dirty(attribute);
        }
    }
}

pub fn set_climbing(base: &mut EntityBase, climbing: bool) {
    base.data
        .set_flag(DATA_SPIDER_FLAGS, SPIDER_FLAG_CLIMBING, climbing);
}

pub fn on_climbable(base: &EntityBase, view: &impl BlockView) -> bool {
    if base.kind.extends("Spider") {
        return base.data.flag(DATA_SPIDER_FLAGS, SPIDER_FLAG_CLIMBING);
    }
    has_tag(
        view.block_or_air(base.position.block()),
        "minecraft:climbable",
    )
}

fn input_vector(input: Vec3, speed: f32, yaw: f32) -> Vec3 {
    let length = input.length_sqr();
    if length < 1.0e-7 {
        return Vec3::ZERO;
    }
    let scaled = if length > 1.0 {
        input.normalize()
    } else {
        input
    }
    .scale(f64::from(speed));
    let radians = f64::from(yaw * mth::DEG_TO_RAD);
    let sin = f64::from(mth::sin(radians));
    let cos = f64::from(mth::cos(radians));
    Vec3::new(
        scaled.x * cos - scaled.z * sin,
        scaled.y,
        scaled.z * cos + scaled.x * sin,
    )
}

fn friction_influenced_speed(base: &EntityBase, living: &LivingState, friction: f32) -> f32 {
    if base.on_ground {
        if friction <= 0.6 {
            living.speed
        } else {
            living.speed * (0.216_000_02 / (friction * friction * friction))
        }
    } else {
        FLYING_SPEED
    }
}

pub fn travel_in_air<V: BlockView>(
    base: &mut EntityBase,
    living: &mut LivingState,
    view: &V,
    input: Vec3,
) -> Option<Landing> {
    let below = movement_affecting_pos(base, view);
    let block_friction = if base.on_ground {
        compute_modified_friction(
            friction(view.block_or_air(below)),
            living.attribute(FRICTION_MODIFIER) as f32,
        )
    } else {
        1.0
    };
    let speed = friction_influenced_speed(base, living, block_friction);
    base.velocity = base.velocity + input_vector(input, speed, base.yaw);
    let climbing = on_climbable(base, view);
    if climbing {
        base.fall_distance = 0.0;
        let v = base.velocity;
        base.velocity = Vec3::new(
            v.x.clamp(-CLIMB_SPEED, CLIMB_SPEED),
            v.y.max(-CLIMB_SPEED),
            v.z.clamp(-CLIMB_SPEED, CLIMB_SPEED),
        );
    }
    let traits = living.move_traits(base);
    let landing = move_entity(base, view, base.velocity, &traits);
    let mut movement = base.velocity;
    if (base.horizontal_collision || living.jumping) && climbing {
        movement = Vec3::new(movement.x, CLIMB_BOOST, movement.z);
    }
    let mut y = movement.y;
    match living.effect("minecraft:levitation").map(|e| e.amplifier) {
        Some(amplifier) => {
            y += (LEVITATION_SPEED * (f64::from(amplifier) + 1.0) - movement.y) * LEVITATION_BLEND;
        }
        None => y -= living.effective_gravity(base),
    }
    let drag_modifier = living.attribute(AIR_DRAG_MODIFIER) as f32;
    let horizontal_drag = compute_modified_friction(AIR_FRICTION, drag_modifier);
    let horizontal = block_friction * horizontal_drag;
    let vertical = if omnidirectional_air_mover(base.kind) {
        horizontal
    } else {
        compute_modified_friction(AIR_DRAG, drag_modifier)
    };
    base.velocity = if living.discard_friction {
        Vec3::new(movement.x, y, movement.z)
    } else {
        Vec3::new(
            movement.x * f64::from(horizontal),
            y * f64::from(vertical),
            movement.z * f64::from(horizontal),
        )
    };
    landing
}

pub fn omnidirectional_air_mover(kind: EntityKind) -> bool {
    matches!(kind, EntityKind::Parrot | EntityKind::Bee)
}

pub fn begin_ai_step(base: &mut EntityBase, living: &mut LivingState) {
    if living.no_jump_delay > 0 {
        living.no_jump_delay -= 1;
    }
    let v = base.velocity;
    let mut x = v.x;
    let mut y = v.y;
    let mut z = v.z;
    if x.abs() < VELOCITY_THRESHOLD {
        x = 0.0;
    }
    if z.abs() < VELOCITY_THRESHOLD {
        z = 0.0;
    }
    if y.abs() < VELOCITY_THRESHOLD {
        y = 0.0;
    }
    base.velocity = Vec3::new(x, y, z);
    living.xxa *= APPLIED_INPUT_DECAY;
    living.zza *= APPLIED_INPUT_DECAY;
    if health(base) <= 0.0 {
        living.jumping = false;
        living.xxa = 0.0;
        living.zza = 0.0;
    }
}

pub fn effective_ai(base: &EntityBase, living: &LivingState) -> bool {
    !living.is_mob() || !is_no_ai(base)
}

pub fn finish_ai_step<V: BlockView>(
    base: &mut EntityBase,
    living: &mut LivingState,
    view: &V,
    can_float: bool,
) -> Option<Landing> {
    if living.jumping {
        jump(base, living, view, can_float);
    } else {
        living.no_jump_delay = 0;
    }
    if living.has_effect("minecraft:slow_falling") || living.has_effect("minecraft:levitation") {
        base.fall_distance = 0.0;
    }
    let input = Vec3::new(
        f64::from(living.xxa),
        f64::from(living.yya),
        f64::from(living.zza),
    );
    let frozen = base.kind == EntityKind::Shulker;
    if frozen {
        base.velocity = Vec3::ZERO;
    }
    let landing = if effective_ai(base, living) && !frozen {
        travel(base, living, view, input)
    } else {
        None
    };
    let weaving = living.has_effect("minecraft:weaving");
    apply_inside_blocks(base, view, weaving);
    landing
}

pub fn ai_step<V: BlockView>(
    base: &mut EntityBase,
    living: &mut LivingState,
    view: &V,
) -> Option<Landing> {
    begin_ai_step(base, living);
    finish_ai_step(base, living, view, false)
}

fn jump<V: BlockView>(base: &mut EntityBase, living: &mut LivingState, view: &V, can_float: bool) {
    let contact = base.fluid;
    let height = if contact.in_lava() {
        contact.lava_height
    } else {
        contact.water_height
    };
    let in_water = contact.in_water && height > 0.0;
    let threshold = fluid::jump_threshold(base);
    let swim_up = in_water && (!base.on_ground || height > threshold);
    let lava_up = contact.in_lava() && (!base.on_ground || contact.lava_height > threshold);
    if swim_up || lava_up {
        jump_in_liquid(base, living, can_float);
    } else if (base.on_ground || (in_water && height <= threshold)) && living.no_jump_delay == 0 {
        jump_from_ground(base, living, view);
        living.jumped = true;
        if let Some(lift) = living.jump_tuning.hop
            && base.velocity.horizontal_distance_sqr() < HOP_HORIZONTAL_LIMIT
        {
            let boost = input_vector(Vec3::new(0.0, f64::from(lift), 1.0), HOP_SPEED, base.yaw);
            base.velocity = base.velocity + boost;
        }
        living.no_jump_delay = NO_JUMP_DELAY;
    }
}

fn jump_in_liquid(base: &mut EntityBase, living: &LivingState, can_float: bool) {
    let impulse = if living.is_mob() && !can_float {
        0.3
    } else {
        LIQUID_JUMP_IMPULSE
    };
    base.velocity = base.velocity + Vec3::new(0.0, impulse, 0.0);
}

pub fn jump_power(base: &EntityBase, living: &mut LivingState, view: &impl BlockView) -> f32 {
    let boost = living.effect("minecraft:jump_boost").map_or(0.0, |effect| {
        JUMP_BOOST_STEP * (effect.amplifier as f32 + 1.0)
    });
    living.attribute(JUMP_STRENGTH) as f32
        * living.jump_tuning.multiplier
        * block_jump_factor(base, view)
        + boost
}

pub fn jump_from_ground(base: &mut EntityBase, living: &mut LivingState, view: &impl BlockView) {
    let power = jump_power(base, living, view);
    if power <= MIN_JUMP_POWER {
        return;
    }
    let v = base.velocity;
    base.velocity = Vec3::new(v.x, f64::from(power).max(v.y), v.z);
    if is_sprinting(base) {
        let radians = base.yaw * mth::DEG_TO_RAD;
        base.velocity = base.velocity
            + Vec3::new(
                -f64::from(mth::sin(f64::from(radians))) * SPRINT_JUMP_BOOST,
                0.0,
                f64::from(mth::cos(f64::from(radians))) * SPRINT_JUMP_BOOST,
            );
    }
    base.needs_sync = true;
}

pub fn is_sprinting(base: &EntityBase) -> bool {
    base.data
        .flag(super::base::DATA_SHARED_FLAGS, FLAG_SPRINTING)
}

pub fn set_sprinting(base: &mut EntityBase, living: &mut LivingState, sprinting: bool) {
    base.set_shared_flag(FLAG_SPRINTING, sprinting);
    living
        .attributes
        .remove_modifier(MOVEMENT_SPEED, SPRINTING_MODIFIER);
    if sprinting {
        living.attributes.add_modifier(
            MOVEMENT_SPEED,
            AttributeModifier {
                id: Cow::Borrowed(SPRINTING_MODIFIER),
                amount: SPRINT_SPEED_BONUS,
                operation: crate::effect::AttributeOperation::AddMultipliedTotal,
            },
        );
    }
}

pub fn travel_flying<V: BlockView>(
    base: &mut EntityBase,
    living: &mut LivingState,
    view: &V,
    input: Vec3,
    speeds: FlyingSpeeds,
) -> Option<Landing> {
    let traits = living.move_traits(base);
    let (speed, drag) = if base.fluid.in_water {
        (speeds.water, FLYING_WATER_DRAG)
    } else if base.fluid.in_lava() {
        (speeds.lava, FLYING_LAVA_DRAG)
    } else {
        (speeds.air, FLYING_AIR_DRAG)
    };
    base.velocity = base.velocity + input_vector(input, speed, base.yaw);
    let landing = move_entity(base, view, base.velocity, &traits);
    base.velocity = base.velocity.scale(drag);
    landing
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlyingSpeeds {
    pub water: f32,
    pub lava: f32,
    pub air: f32,
}

pub fn travel<V: BlockView>(
    base: &mut EntityBase,
    living: &mut LivingState,
    view: &V,
    input: Vec3,
) -> Option<Landing> {
    if base.kind == EntityKind::Ghast {
        return travel_flying(base, living, view, input, GHAST_FLYING_SPEEDS);
    }
    if base.kind == EntityKind::Phantom {
        return travel_flying(base, living, view, input, PHANTOM_FLYING_SPEEDS);
    }
    if base.kind == EntityKind::Allay {
        let speeds = FlyingSpeeds {
            water: FLYING_SPEED,
            lava: FLYING_SPEED,
            air: living.speed,
        };
        return travel_flying(base, living, view, input, speeds);
    }
    if base.fluid.in_liquid() {
        travel_in_fluid(base, living, view, input)
    } else {
        travel_in_air(base, living, view, input)
    }
}

fn water_slow_down(kind: EntityKind) -> f32 {
    if kind == EntityKind::PolarBear {
        POLAR_BEAR_WATER_SLOW_DOWN
    } else {
        WATER_SLOW_DOWN
    }
}

fn fluid_falling_adjusted(gravity: f64, falling: bool, movement: Vec3) -> Vec3 {
    if gravity == 0.0 {
        return movement;
    }
    let y = if falling
        && (movement.y - 0.005).abs() >= VELOCITY_THRESHOLD
        && (movement.y - gravity / 16.0).abs() < VELOCITY_THRESHOLD
    {
        -VELOCITY_THRESHOLD
    } else {
        movement.y - gravity / 16.0
    };
    Vec3::new(movement.x, y, movement.z)
}

pub fn travel_in_fluid<V: BlockView>(
    base: &mut EntityBase,
    living: &mut LivingState,
    view: &V,
    input: Vec3,
) -> Option<Landing> {
    let falling = base.velocity.y <= 0.0;
    let old_y = base.position.y;
    let gravity = living.effective_gravity(base);
    let landing = if base.fluid.in_water {
        let mut slow_down = water_slow_down(base.kind);
        let mut speed = FLUID_SPEED;
        let mut efficiency = living.attribute(WATER_MOVEMENT_EFFICIENCY) as f32;
        if !base.on_ground {
            efficiency *= 0.5;
        }
        if efficiency > 0.0 {
            slow_down += (WATER_EFFICIENT_SLOW_DOWN - slow_down) * efficiency;
            speed += (living.speed - speed) * efficiency;
        }
        if living.has_effect("minecraft:dolphins_grace") {
            slow_down = DOLPHIN_SLOW_DOWN;
        }
        base.velocity = base.velocity + input_vector(input, speed, base.yaw);
        let traits = living.move_traits(base);
        let landing = move_entity(base, view, base.velocity, &traits);
        let mut movement = base.velocity;
        if base.horizontal_collision && on_climbable(base, view) {
            movement = Vec3::new(movement.x, CLIMB_BOOST, movement.z);
        }
        let slow = f64::from(slow_down);
        movement = movement.multiply(slow, FLUID_VERTICAL_DRAG, slow);
        base.velocity = fluid_falling_adjusted(gravity, falling, movement);
        landing
    } else {
        base.velocity = base.velocity + input_vector(input, FLUID_SPEED, base.yaw);
        let traits = living.move_traits(base);
        let landing = move_entity(base, view, base.velocity, &traits);
        if base.fluid.lava_height <= fluid::jump_threshold(base) {
            base.velocity = base.velocity.multiply(0.5, FLUID_VERTICAL_DRAG, 0.5);
            base.velocity = fluid_falling_adjusted(gravity, falling, base.velocity);
        } else {
            base.velocity = base.velocity.scale(0.5);
        }
        if gravity != 0.0 {
            base.velocity = base.velocity + Vec3::new(0.0, -gravity / 4.0, 0.0);
        }
        landing
    };
    let v = base.velocity;
    if base.horizontal_collision {
        let step = Vec3::new(v.x, v.y + JUMP_OUT_STEP - base.position.y + old_y, v.z);
        if fluid::is_free(view, &base.bounding_box().move_by(step)) {
            base.velocity = Vec3::new(v.x, JUMP_OUT_SPEED, v.z);
        }
    }
    landing
}

pub fn below_world(base: &EntityBase, min_y: i32) -> bool {
    base.position.y < f64::from(min_y - 64)
}

pub fn supporting_ground(base: &EntityBase, view: &impl BlockView) -> BlockPos {
    physics::ground_pos(base, view)
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;

    fn zombie() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Zombie);
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    fn witch() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Witch);
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    #[test]
    fn a_witch_keeps_fifteen_percent_of_magic_and_nothing_of_its_own_damage() {
        let hurt = |kind: DamageType, self_inflicted: bool| {
            let (mut base, mut living) = witch();
            let full = health(&base);
            let source = HurtSource {
                self_inflicted,
                ..HurtSource::plain(kind)
            };
            actually_hurt(&mut base, &mut living, &source, 10.0, 0);
            full - health(&base)
        };
        assert_eq!(hurt(DamageType::Magic, false), 10.0 * 0.15);
        assert_eq!(hurt(DamageType::IndirectMagic, false), 10.0 * 0.15);
        assert_eq!(hurt(DamageType::MobAttack, false), 10.0);
        assert_eq!(hurt(DamageType::MobAttack, true), 0.0);
        assert_eq!(hurt(DamageType::IndirectMagic, true), 0.0);
        let (_, mut living) = witch();
        assert_eq!(
            final_damage(&mut living, DamageType::Magic, false, 10.0),
            10.0 * 0.15
        );
        assert_eq!(
            final_damage(&mut living, DamageType::Magic, true, 10.0),
            0.0
        );
        let (_, mut zombie) = zombie();
        assert_eq!(
            final_damage(&mut zombie, DamageType::Magic, false, 10.0),
            10.0
        );
    }

    #[test]
    fn polar_bears_slow_down_less_in_water_than_other_mobs() {
        assert_eq!(water_slow_down(EntityKind::PolarBear), 0.98);
        assert_eq!(water_slow_down(EntityKind::Zombie), 0.8);
    }

    #[test]
    fn a_dying_mob_starts_its_death_countdown_only_once_its_death_applies() {
        let (mut base, mut living) = zombie();
        set_health(&mut base, &mut living, 0.0);
        living.dying = true;
        for _ in 0..DEATH_DURATION {
            assert!(!base_tick(&mut base, &mut living).removed);
        }
        assert_eq!(living.death_time, 0);
        living.dying = false;
        living.dead = true;
        let removed = (0..DEATH_DURATION).any(|_| base_tick(&mut base, &mut living).removed);
        assert!(removed);
    }

    #[test]
    fn the_final_damage_runs_the_armor_and_absorption_chain_without_touching_the_mob() {
        let (_, mut living) = zombie();
        let armor = living.attribute(ARMOR);
        let expected = damage::after_armor(6.0, armor.floor() as f32, 0.0);
        assert_eq!(
            final_damage(&mut living, DamageType::MobAttack, false, 6.0),
            expected
        );
        living.absorption = 1.5;
        assert_eq!(
            final_damage(&mut living, DamageType::MobAttack, false, 6.0),
            expected - 1.5
        );
        assert_eq!(
            final_damage(&mut living, DamageType::Starve, false, -3.0),
            0.0
        );
        assert_eq!(living.absorption, 1.5);
    }

    fn ghast() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Ghast);
        base.position = Vec3::new(0.5, 80.0, 0.5);
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    #[test]
    fn a_ghast_flies_without_gravity_and_slows_by_nine_tenths_in_the_air() {
        let (mut base, mut living) = ghast();
        base.velocity = Vec3::new(1.0, 0.5, 0.0);
        travel(&mut base, &mut living, &Air, Vec3::ZERO);
        let drag = f64::from(0.91f32);
        assert_eq!(base.velocity, Vec3::new(drag, 0.5 * drag, 0.0));
        assert!((base.position.x - 1.5).abs() < 1.0e-9);
        assert!((base.position.y - 80.5).abs() < 1.0e-9);
    }

    fn flier(kind: EntityKind) -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), kind);
        base.position = Vec3::new(0.5, 80.0, 0.5);
        base.old_position = base.position;
        base.set_no_gravity(true);
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    #[test]
    fn an_omnidirectional_mover_slows_vertically_like_horizontally_and_a_walker_does_not() {
        for (kind, vertical) in [
            (EntityKind::Parrot, f64::from(0.91f32)),
            (EntityKind::Bee, f64::from(0.91f32)),
            (EntityKind::Zombie, f64::from(0.98f32)),
        ] {
            let (mut base, mut living) = flier(kind);
            base.velocity = Vec3::new(1.0, 1.0, 0.0);
            travel(&mut base, &mut living, &Air, Vec3::ZERO);
            assert_eq!(base.velocity.x, f64::from(0.91f32), "{kind:?}");
            assert_eq!(base.velocity.y, vertical, "{kind:?}");
        }
    }

    #[test]
    fn a_flier_off_the_ground_accelerates_at_the_flying_speed_of_a_fiftieth() {
        let (mut base, mut living) = flier(EntityKind::Parrot);
        travel(&mut base, &mut living, &Air, Vec3::new(0.0, 0.0, 1.0));
        let expected = 0.02 * f64::from(0.91f32);
        assert!(
            (base.velocity.z - expected).abs() < 1.0e-9,
            "{}",
            base.velocity.z
        );
    }

    #[test]
    fn a_ghast_in_water_slows_to_four_fifths() {
        let (mut base, mut living) = ghast();
        base.fluid.in_water = true;
        base.velocity = Vec3::new(1.0, 0.0, 0.0);
        travel(&mut base, &mut living, &Air, Vec3::ZERO);
        assert_eq!(base.velocity.x, f64::from(0.8f32));
    }

    #[test]
    fn the_ghast_flying_input_is_added_at_a_fiftieth_along_the_facing() {
        let (mut base, mut living) = ghast();
        base.yaw = 0.0;
        travel(&mut base, &mut living, &Air, Vec3::new(0.0, 0.0, 1.0));
        let expected = f64::from(0.02f32) * f64::from(0.91f32);
        assert!(
            (base.velocity.z - expected).abs() < 1.0e-12,
            "{:?}",
            base.velocity
        );
    }

    #[test]
    fn a_ghast_is_immune_to_fireballs_except_the_ones_a_player_reflected() {
        let (base, _) = ghast();
        let plain = HurtSource::plain(DamageType::Fireball);
        assert!(is_invulnerable_to(&base, &plain));
        let reflected = HurtSource {
            reflected_fireball: true,
            ..plain
        };
        assert!(!is_invulnerable_to(&base, &reflected));
        let (mut sealed, _) = ghast();
        sealed.invulnerable = true;
        assert!(is_invulnerable_to(&sealed, &reflected));
        let (zombie_base, _) = zombie();
        assert!(!is_invulnerable_to(&zombie_base, &plain));
        let blaze = EntityBase::new(EntityId(2), Uuid::from_u128(2), EntityKind::Blaze);
        assert!(is_invulnerable_to(&blaze, &reflected));
    }

    #[test]
    fn a_settled_hurt_skips_the_checks_the_ask_already_made() {
        let (mut base, mut living) = zombie();
        let source = HurtSource::plain(DamageType::Generic);
        assert!(can_hurt(&base, &living, &source));
        assert_eq!(
            hurt_settled(&mut base, &mut living, &source, 4.0, 0),
            HurtOutcome::Full
        );
        assert_eq!(health(&base), 16.0);
        assert_eq!(
            hurt_settled(&mut base, &mut living, &source, 3.0, 0),
            HurtOutcome::Ignored
        );
        assert_eq!(
            hurt_settled(&mut base, &mut living, &source, 6.0, 0),
            HurtOutcome::Partial
        );
        assert_eq!(health(&base), 14.0);
    }

    struct Air;

    impl BlockView for Air {
        fn block(&self, _pos: BlockPos) -> Option<crate::BlockStateId> {
            None
        }
    }

    #[test]
    fn sprinting_sets_the_flag_and_the_transient_speed_modifier() {
        let (mut base, mut living) = zombie();
        let walking = living.attribute(MOVEMENT_SPEED);
        set_sprinting(&mut base, &mut living, true);
        assert!(is_sprinting(&base));
        let sprinting = living.attribute(MOVEMENT_SPEED);
        assert!((sprinting - walking * 1.3).abs() < 1.0e-6);
        set_sprinting(&mut base, &mut living, false);
        assert!(!is_sprinting(&base));
        assert_eq!(living.attribute(MOVEMENT_SPEED), walking);
    }

    #[test]
    fn a_sprinting_jump_pushes_forward_along_the_yaw() {
        let (mut base, mut living) = zombie();
        base.yaw = 0.0;
        set_sprinting(&mut base, &mut living, true);
        jump_from_ground(&mut base, &mut living, &Air);
        assert!(base.velocity.y > 0.0);
        assert!((base.velocity.z - 0.2).abs() < 1.0e-9);
        assert!(base.velocity.x.abs() < 1.0e-9);
    }

    #[test]
    fn discarding_friction_keeps_the_horizontal_and_vertical_motion_untouched() {
        let flying = |discard: bool| {
            let (mut base, mut living) = zombie();
            base.on_ground = false;
            base.velocity = Vec3::new(1.0, 0.5, -1.0);
            living.discard_friction = discard;
            let _ = travel_in_air(&mut base, &mut living, &Air, Vec3::ZERO);
            base.velocity
        };
        let kept = flying(true);
        assert_eq!(kept.x, 1.0);
        assert_eq!(kept.z, -1.0);
        assert!((kept.y - (0.5 - 0.08)).abs() < 1.0e-9);
        let slowed = flying(false);
        assert!(slowed.x < 1.0 && slowed.z > -1.0);
    }

    #[test]
    fn goats_take_ten_points_less_fall_damage_than_other_mobs() {
        let (zombie_base, mut zombie_state) = zombie();
        let mut goat_base = EntityBase::new(EntityId(2), Uuid::from_u128(2), EntityKind::Goat);
        let mut goat_state = LivingState::new(&mut goat_base, 0.0);
        let plain = fall_damage(&zombie_base, &mut zombie_state, 20.0, 1.0);
        let goat = fall_damage(&goat_base, &mut goat_state, 20.0, 1.0);
        assert_eq!(plain - goat, 10);
        assert!(fall_damage(&goat_base, &mut goat_state, 6.0, 1.0) <= 0);
    }

    #[test]
    fn frogs_take_five_points_less_fall_damage_than_other_mobs() {
        let (zombie_base, mut zombie_state) = zombie();
        let mut frog_base = EntityBase::new(EntityId(2), Uuid::from_u128(2), EntityKind::Frog);
        let mut frog_state = LivingState::new(&mut frog_base, 0.0);
        let plain = fall_damage(&zombie_base, &mut zombie_state, 20.0, 1.0);
        let frog = fall_damage(&frog_base, &mut frog_state, 20.0, 1.0);
        assert_eq!(plain - frog, 5);
    }

    #[test]
    fn the_jump_multiplier_scales_the_power() {
        let (base, mut living) = zombie();
        let plain = jump_power(&base, &mut living, &Air);
        living.jump_tuning.multiplier = 0.5;
        let scaled = jump_power(&base, &mut living, &Air);
        assert!((scaled - plain * 0.5).abs() < 1.0e-6);
    }

    #[test]
    fn a_hop_adds_a_forward_and_upward_push_only_when_nearly_still() {
        let (mut base, mut living) = zombie();
        base.on_ground = true;
        living.jumping = true;
        living.jump_tuning.hop = Some(1.5);
        let _ = finish_ai_step(&mut base, &mut living, &Air, false);
        assert!(living.jumped);
        let still = base.velocity;
        let (mut moving, mut living2) = zombie();
        moving.on_ground = true;
        moving.velocity = Vec3::new(0.0, 0.0, 0.5);
        living2.jumping = true;
        living2.jump_tuning.hop = Some(1.5);
        let _ = finish_ai_step(&mut moving, &mut living2, &Air, false);
        assert!(still.y > moving.velocity.y - 1.0e-9);
        assert!(still.z > 0.0);
    }
}
