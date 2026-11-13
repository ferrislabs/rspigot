use std::borrow::Cow;
use std::num::NonZeroU8;

use rspigot_blocks::BlockKind;
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use super::ageable::{self, AgeableState};
use super::arrow::{self, ArrowState};
use super::base::{EntityBase, MAX_AIR_SUPPLY, RemovalReason};
use super::bullet::{self, BulletState};
use super::falling::FallingState;
use super::hurting::{self, HurtingState};
use super::item::{self, ItemState};
use super::living::{
    self, EQUIPMENT_SLOTS, LivingState, MobState, health, is_left_handed, is_no_ai, set_health,
    set_left_handed, set_no_ai,
};
use super::orb::{self, OrbState};
use super::species::{self, Species};
use super::thrown::{self, ThrownState};
use super::{Entity, EntityState};
use super::{neutral, tamable};
use crate::effect::{
    AttributeModifier, AttributeOperation, EffectDuration, EffectId, EffectInstance,
};
use crate::item::ItemStack;
use crate::text::{Color, Component, Content, Style};
use crate::{BlockPos, BlockStateId, EntityId, ItemId, Vec3};

const MOTION_LIMIT: f64 = 10.0;
const HORIZONTAL_LIMIT: f64 = 30_000_512.0;
const VERTICAL_LIMIT: f64 = 20_000_000.0;
const BUKKIT_UPDATE_LEVEL: i32 = 2;
const STYLE_FLAGS: [&str; 5] = [
    "bold",
    "italic",
    "underlined",
    "strikethrough",
    "obfuscated",
];
const ENTITY_KEYS: [&str; 29] = [
    "id",
    "Pos",
    "Motion",
    "Rotation",
    "fall_distance",
    "Fire",
    "Air",
    "OnGround",
    "Invulnerable",
    "PortalCooldown",
    "invulnerable_time",
    "UUID",
    "WorldUUIDLeast",
    "WorldUUIDMost",
    "Bukkit.updateLevel",
    "Bukkit.persist",
    "Bukkit.visibleByDefault",
    "Bukkit.invisible",
    "Bukkit.MaxAirSupply",
    "Spigot.ticksLived",
    "CustomName",
    "CustomNameVisible",
    "Silent",
    "NoGravity",
    "Glowing",
    "TicksFrozen",
    "HasVisualFire",
    "Tags",
    "data",
];
const LIVING_KEYS: [&str; 18] = [
    "Health",
    "HurtTime",
    "DeathTime",
    "AbsorptionAmount",
    "current_impulse_context_reset_grace_time",
    "attributes",
    "active_effects",
    "FallFlying",
    "equipment",
    "CanPickUpLoot",
    "PersistenceRequired",
    "drop_chances",
    "LeftHanded",
    "DeathLootTable",
    "DeathLootTableSeed",
    "NoAI",
    "home_pos",
    "home_radius",
];
const AGEABLE_KEYS: [&str; 3] = ["Age", "ForcedAge", "AgeLocked"];
const ANIMAL_KEYS: [&str; 2] = ["InLove", "LoveCause"];
const TAMABLE_KEYS: [&str; 2] = ["Owner", "Sitting"];
const NEUTRAL_KEYS: [&str; 4] = ["anger_end_time", "angry_at", "AngerTime", "AngryAt"];
const FARM_KEYS: [&str; 2] = ["variant", "sound_variant"];
const CHICKEN_KEYS: [&str; 4] = ["variant", "sound_variant", "IsChickenJockey", "EggLayTime"];
const SHEEP_KEYS: [&str; 2] = ["Sheared", "Color"];
const ZOMBIE_KEYS: [&str; 4] = [
    "IsBaby",
    "CanBreakDoors",
    "InWaterTime",
    "DrownedConversionTime",
];
const SKELETON_KEYS: [&str; 2] = ["FreezingTime", "StrayConversionTime"];
const CREEPER_KEYS: [&str; 4] = ["powered", "Fuse", "ExplosionRadius", "ignited"];
const TRANSIENT_MODIFIERS: &[&str] = &[
    species::BABY_SPEED_MODIFIER,
    species::cat::SPRINTING_MODIFIER,
    species::zombified_piglin::ATTACKING_MODIFIER,
    species::rabbit::EVIL_MODIFIER,
    species::enderman::ATTACKING_MODIFIER,
    species::witch::DRINK_MODIFIER,
];
const ARROW_KEYS: [&str; 14] = [
    "Owner",
    "LeftOwner",
    "HasBeenShot",
    "life",
    "inBlockState",
    "shake",
    "inGround",
    "pickup",
    "damage",
    "crit",
    "PierceLevel",
    "SoundEvent",
    "item",
    "weapon",
];
const THROWN_KEYS: [&str; 4] = ["Owner", "LeftOwner", "HasBeenShot", "Item"];
const HURTING_KEYS: [&str; 7] = [
    "Owner",
    "LeftOwner",
    "HasBeenShot",
    "acceleration_power",
    "Item",
    "ExplosionPower",
    "dangerous",
];
const BULLET_KEYS: [&str; 3] = ["Owner", "LeftOwner", "HasBeenShot"];
const ITEM_KEYS: [&str; 6] = ["Health", "Age", "PickupDelay", "Thrower", "Owner", "Item"];
const ORB_KEYS: [&str; 4] = ["Health", "Age", "Value", "Count"];
const FALLING_KEYS: [&str; 8] = [
    "BlockState",
    "Time",
    "DropItem",
    "HurtEntities",
    "FallHurtAmount",
    "FallHurtMax",
    "TileEntityData",
    "CancelDrop",
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LoadError {
    #[error("entity has no id")]
    MissingId,
    #[error("unknown entity type {0}")]
    UnknownType(String),
    #[error("entity type {0} cannot be serialized")]
    NotSerializable(&'static str),
    #[error("entity has an invalid position")]
    InvalidPosition,
    #[error("item entity without an item")]
    EmptyItem,
}

pub fn is_simulated(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Item
            | EntityKind::FallingBlock
            | EntityKind::ExperienceOrb
            | EntityKind::Arrow
            | EntityKind::Snowball
            | EntityKind::SplashPotion
            | EntityKind::ShulkerBullet
    ) || hurting::is_hurting_projectile(kind)
        || (kind.extends("Mob") && kind != EntityKind::EnderDragon)
}

fn doubles(v: Vec3) -> Tag {
    Tag::List(vec![Tag::Double(v.x), Tag::Double(v.y), Tag::Double(v.z)])
}

fn read_vec3(compound: &Compound, key: &str) -> Option<Vec3> {
    let list = compound.get_list(key)?;
    if list.len() != 3 {
        return None;
    }
    let value = |i: usize| list[i].as_f64();
    Some(Vec3::new(value(0)?, value(1)?, value(2)?))
}

fn read_rotation(compound: &Compound) -> Option<(f32, f32)> {
    let list = compound.get_list("Rotation")?;
    if list.len() != 2 {
        return None;
    }
    Some((list[0].as_f64()? as f32, list[1].as_f64()? as f32))
}

pub fn uuid_tag(uuid: Uuid) -> Tag {
    let value = uuid.as_u128();
    Tag::IntArray(vec![
        (value >> 96) as i32,
        (value >> 64) as i32,
        (value >> 32) as i32,
        value as i32,
    ])
}

pub fn read_uuid(tag: &Tag) -> Option<Uuid> {
    match tag {
        Tag::IntArray(parts) if parts.len() == 4 => {
            let value = parts
                .iter()
                .fold(0u128, |acc, part| (acc << 32) | u128::from(*part as u32));
            Some(Uuid::from_u128(value))
        }
        _ => None,
    }
}

fn bool_tag(value: bool) -> Tag {
    Tag::Byte(i8::from(value))
}

pub fn item_stack_to_nbt(stack: &ItemStack) -> Compound {
    let mut out = Compound::new();
    out.insert(
        "id",
        stack.item.kind().map_or("minecraft:air", ItemKind::name),
    );
    out.insert("count", i32::from(stack.count.get()));
    if !stack.components.is_empty() {
        out.insert("components", stack.components.clone());
    }
    out
}

pub fn item_stack_from_nbt(compound: &Compound) -> Option<ItemStack> {
    let kind = ItemKind::from_name(compound.get_str("id")?)?;
    if kind == ItemKind::Air {
        return None;
    }
    let count = compound.get_i32("count").unwrap_or(1).clamp(1, 99);
    let mut stack = ItemStack::new(kind.id(), NonZeroU8::new(count as u8)?);
    if let Some(components) = compound.get_compound("components") {
        stack.components = components.clone();
    }
    Some(stack)
}

pub fn block_state_to_nbt(state: BlockStateId) -> Compound {
    let mut out = Compound::new();
    out.insert(
        "Name",
        state.kind().map_or("minecraft:air", BlockKind::name),
    );
    let properties: Compound = state
        .properties()
        .map(|(name, value)| (name.to_owned(), Tag::String(value.to_owned())))
        .collect();
    if !properties.is_empty() {
        out.insert("Properties", properties);
    }
    out
}

pub fn block_state_from_nbt(compound: &Compound) -> BlockStateId {
    let Some(kind) = compound.get_str("Name").and_then(BlockKind::from_name) else {
        return BlockStateId::AIR;
    };
    let mut state = kind.default_state();
    if let Some(properties) = compound.get_compound("Properties") {
        for (name, value) in properties.iter() {
            if let Some(value) = value.as_str()
                && let Some(next) = state.with_property(name, value)
            {
                state = next;
            }
        }
    }
    state
}

pub fn component_from_tag(tag: &Tag) -> Option<Component> {
    match tag {
        Tag::String(text) => Some(Component::text(text.clone())),
        Tag::Compound(compound) => {
            if let Some(wrapped) = list_wrapped(compound) {
                return component_from_tag(wrapped);
            }
            let mut component = if let Some(text) = compound.get_str("text") {
                Component::text(text)
            } else if let Some(key) = compound.get_str("translate") {
                let args = compound
                    .get_list("with")
                    .map(|list| list.iter().filter_map(component_from_tag).collect())
                    .unwrap_or_default();
                Component::translatable(key, args)
            } else {
                Component::text("")
            };
            component.style = Style::from_parts(
                compound.get_str("color").and_then(Color::parse),
                STYLE_FLAGS.map(|flag| compound.get_bool(flag)),
            );
            if let Some(extra) = compound.get_list("extra") {
                component.extra = extra.iter().filter_map(component_from_tag).collect();
            }
            Some(component)
        }
        Tag::List(list) => {
            let mut parts = list.iter().filter_map(component_from_tag);
            let mut first = parts.next()?;
            first.extra.extend(parts);
            Some(first)
        }
        other => other
            .as_i64()
            .map(|v| Component::text(v.to_string()))
            .or_else(|| other.as_f64().map(|v| Component::text(v.to_string()))),
    }
}

pub fn component_to_tag(component: &Component) -> Tag {
    let plain = component.extra.is_empty() && component.style == Style::default();
    let mut out = match &component.content {
        Content::Text(text) if plain => return Tag::String(text.clone()),
        Content::Text(text) => Compound::new().with("text", text.as_str()),
        Content::Translatable { key, args } => {
            let mut out = Compound::new().with("translate", key.as_str());
            if !args.is_empty() {
                out.insert(
                    "with",
                    Tag::List(args.iter().map(component_to_tag).collect()),
                );
            }
            out
        }
    };
    if let Some(color) = component.style.color {
        out.insert("color", color.serialize());
    }
    for (flag, value) in STYLE_FLAGS.into_iter().zip(component.style.flags()) {
        if let Some(value) = value {
            out.insert(flag, value);
        }
    }
    if !component.extra.is_empty() {
        out.insert(
            "extra",
            Tag::List(component.extra.iter().map(component_to_tag).collect()),
        );
    }
    Tag::Compound(out)
}

fn list_wrapped(compound: &Compound) -> Option<&Tag> {
    compound.get("").filter(|_| compound.len() == 1)
}

fn operation_name(operation: AttributeOperation) -> &'static str {
    match operation {
        AttributeOperation::AddValue => "add_value",
        AttributeOperation::AddMultipliedBase => "add_multiplied_base",
        AttributeOperation::AddMultipliedTotal => "add_multiplied_total",
    }
}

fn operation_from(name: &str) -> Option<AttributeOperation> {
    Some(match name {
        "add_value" => AttributeOperation::AddValue,
        "add_multiplied_base" => AttributeOperation::AddMultipliedBase,
        "add_multiplied_total" => AttributeOperation::AddMultipliedTotal,
        _ => return None,
    })
}

pub fn effect_to_nbt(effect: &EffectInstance) -> Compound {
    let mut out = Compound::new();
    out.insert("id", effect.effect.name().unwrap_or_default());
    if effect.amplifier != 0 {
        out.insert("amplifier", effect.amplifier as i8);
    }
    let duration = effect.duration.wire_ticks();
    if duration != 0 {
        out.insert("duration", duration);
    }
    if effect.ambient {
        out.insert("ambient", true);
    }
    if !effect.visible {
        out.insert("show_particles", false);
    }
    out.insert("show_icon", effect.show_icon);
    out
}

pub fn effect_from_nbt(compound: &Compound) -> Option<EffectInstance> {
    let effect = EffectId::from_name(compound.get_str("id")?)?;
    let amplifier = compound.get_i64("amplifier").unwrap_or(0) as u8;
    let duration = match compound.get_i64("duration").unwrap_or(0) {
        -1 => EffectDuration::Infinite,
        ticks => EffectDuration::Ticks(ticks.max(0) as u32),
    };
    let visible = compound.get_bool("show_particles").unwrap_or(true);
    let mut instance = EffectInstance::new(effect, duration, amplifier, visible);
    instance.ambient = compound.get_bool("ambient").unwrap_or(false);
    instance.show_icon = compound.get_bool("show_icon").unwrap_or(visible);
    Some(instance)
}

fn read_base(nbt: &Compound, base: &mut EntityBase) -> Result<(), LoadError> {
    let position = read_vec3(nbt, "Pos").unwrap_or(Vec3::ZERO);
    let motion = read_vec3(nbt, "Motion").unwrap_or(Vec3::ZERO);
    let (yaw, pitch) = read_rotation(nbt).unwrap_or((0.0, 0.0));
    let clamp_motion = |v: f64| if v.abs() > MOTION_LIMIT { 0.0 } else { v };
    base.velocity = Vec3::new(
        clamp_motion(motion.x),
        clamp_motion(motion.y),
        clamp_motion(motion.z),
    );
    base.needs_sync = true;
    base.position = Vec3::new(
        position.x.clamp(-HORIZONTAL_LIMIT, HORIZONTAL_LIMIT),
        position.y.clamp(-VERTICAL_LIMIT, VERTICAL_LIMIT),
        position.z.clamp(-HORIZONTAL_LIMIT, HORIZONTAL_LIMIT),
    );
    if !base.position.is_finite() {
        return Err(LoadError::InvalidPosition);
    }
    base.set_yaw(yaw);
    base.set_pitch(pitch);
    base.old_position = base.position;
    base.head_yaw = base.yaw;
    base.fall_distance = nbt.get_f64("fall_distance").unwrap_or(0.0);
    base.remaining_fire_ticks = nbt.get_i64("Fire").map_or(0, |v| v as i16 as i32);
    let air = nbt.get_i64("Air").map_or(MAX_AIR_SUPPLY, |v| v as i32);
    base.set_air_supply(air);
    base.on_ground = nbt.get_bool("OnGround").unwrap_or(false);
    base.invulnerable = nbt.get_bool("Invulnerable").unwrap_or(false);
    base.invulnerable_time = nbt.get_i32("invulnerable_time").unwrap_or(0);
    base.portal_cooldown = nbt.get_i32("PortalCooldown").unwrap_or(0);
    if let Some(uuid) = nbt.get("UUID").and_then(read_uuid) {
        base.uuid = uuid;
    }
    base.set_rotation(base.yaw, base.pitch);
    if let Some(name) = nbt.get("CustomName").and_then(component_from_tag) {
        base.set_custom_name(Some(name));
    }
    base.set_custom_name_visible(nbt.get_bool("CustomNameVisible").unwrap_or(false));
    base.set_silent(nbt.get_bool("Silent").unwrap_or(false));
    base.set_no_gravity(nbt.get_bool("NoGravity").unwrap_or(false));
    base.set_glowing_tag(nbt.get_bool("Glowing").unwrap_or(false));
    base.set_ticks_frozen(nbt.get_i32("TicksFrozen").unwrap_or(0));
    base.has_visual_fire = nbt.get_bool("HasVisualFire").unwrap_or(false);
    base.custom_data = nbt.get_compound("data").cloned();
    base.tags = nbt
        .get_list("Tags")
        .map(|tags| {
            tags.iter()
                .filter_map(|t| t.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    if base.kind.living() {
        base.tick_count = nbt.get_i32("Spigot.ticksLived").unwrap_or(base.tick_count);
    }
    base.persist = nbt.get_bool("Bukkit.persist").unwrap_or(base.persist);
    base.visible_by_default = nbt
        .get_bool("Bukkit.visibleByDefault")
        .unwrap_or(base.visible_by_default);
    base.max_air_ticks = nbt
        .get_i32("Bukkit.MaxAirSupply")
        .unwrap_or(base.max_air_ticks);
    if nbt.get_bool("Bukkit.invisible").unwrap_or(false) {
        base.set_shared_flag(super::base::FLAG_INVISIBLE, true);
        base.persistent_invisibility = true;
    }
    Ok(())
}

fn read_living(nbt: &Compound, base: &mut EntityBase, living: &mut LivingState) {
    living.absorption = nbt
        .get_f64("AbsorptionAmount")
        .map_or(0.0, |v| (v as f32).max(0.0));
    if let Some(list) = nbt.get_list("attributes") {
        for entry in list.iter().filter_map(Tag::as_compound) {
            let Some(name) = entry.get_str("id") else {
                continue;
            };
            let name = if name.contains(':') {
                Cow::Borrowed(name)
            } else {
                Cow::Owned(format!("minecraft:{name}"))
            };
            let Some(instance) = living.attributes.instance(&name) else {
                continue;
            };
            if let Some(base_value) = entry.get_f64("base") {
                instance.base = base_value;
            }
            instance.modifiers = entry
                .get_list("modifiers")
                .map(|mods| {
                    mods.iter()
                        .filter_map(Tag::as_compound)
                        .filter_map(|m| {
                            Some(AttributeModifier {
                                id: Cow::Owned(m.get_str("id")?.to_owned()),
                                amount: m.get_f64("amount")?,
                                operation: operation_from(m.get_str("operation")?)?,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        living.attributes.take_dirty();
    }
    if let Some(effects) = nbt.get_list("active_effects") {
        for effect in effects
            .iter()
            .filter_map(Tag::as_compound)
            .filter_map(effect_from_nbt)
        {
            let _ = living.effects.apply(effect);
        }
    }
    let max = living.max_health() as f32;
    let health_value = nbt.get_f64("Health").map_or(max, |v| v as f32);
    set_health(base, living, health_value);
    living.hurt_time = nbt.get_i64("HurtTime").map_or(0, |v| v as i32);
    living.death_time = nbt.get_i64("DeathTime").map_or(0, |v| v as i32);
    living.impulse_grace = nbt
        .get_i32("current_impulse_context_reset_grace_time")
        .unwrap_or(0);
    living.fall_flying = nbt.get_bool("FallFlying").unwrap_or(false);
    if let Some(equipment) = nbt.get_compound("equipment") {
        for slot in EQUIPMENT_SLOTS {
            living.equipment[living::slot_index(slot)] = equipment
                .get_compound(slot.name())
                .and_then(item_stack_from_nbt);
        }
    }
    if let Some(mob) = &mut living.mob {
        let can_pick_up_loot = nbt.get_bool("CanPickUpLoot").unwrap_or(false);
        let current_level = nbt
            .get_i32("Bukkit.updateLevel")
            .is_some_and(|level| level >= 1);
        if current_level || can_pick_up_loot {
            mob.can_pick_up_loot = can_pick_up_loot;
        }
        mob.persistence_required = nbt.get_bool("PersistenceRequired").unwrap_or(false);
        mob.drop_chances = [living::DEFAULT_DROP_CHANCE; 8];
        if let Some(chances) = nbt.get_compound("drop_chances") {
            for slot in EQUIPMENT_SLOTS {
                if let Some(chance) = chances.get_f64(slot.name()) {
                    mob.drop_chances[living::slot_index(slot)] = (chance as f32).max(0.0);
                }
            }
        }
        mob.death_loot_table = nbt.get_str("DeathLootTable").map(str::to_owned);
        mob.death_loot_table_seed = nbt.get_i64("DeathLootTableSeed").unwrap_or(0);
        mob.aware = nbt.get_bool("Bukkit.Aware").unwrap_or(true);
        mob.home_radius = nbt.get_i32("home_radius").unwrap_or(-1);
        if mob.home_radius >= 0 {
            mob.home_pos = nbt
                .get_int_array("home_pos")
                .filter(|pos| pos.len() == 3)
                .map_or(BlockPos::new(0, 0, 0), |pos| {
                    BlockPos::new(pos[0], pos[1], pos[2])
                });
        }
        set_left_handed(base, nbt.get_bool("LeftHanded").unwrap_or(false));
        set_no_ai(base, nbt.get_bool("NoAI").unwrap_or(false));
    }
    read_species(nbt, base, living);
}

fn read_ageable(nbt: &Compound, base: &mut EntityBase, ageable: &mut AgeableState) {
    if !ageable::can_be_baby(base.kind) {
        return;
    }
    ageable::set_age(base, ageable, nbt.get_i32("Age").unwrap_or(0));
    ageable.forced_age = nbt.get_i32("ForcedAge").unwrap_or(0);
    ageable::set_age_locked(base, nbt.get_bool("AgeLocked").unwrap_or(false));
    if let Some(love) = &mut ageable.love {
        love.ticks = nbt.get_i32("InLove").unwrap_or(0);
        love.cause = nbt.get("LoveCause").and_then(read_uuid);
    }
}

fn read_tracker(
    nbt: &Compound,
    base: &mut EntityBase,
    keys: (&str, &str),
    data: u8,
) -> species::ConversionTracker {
    let mut tracker = species::ConversionTracker {
        affliction_time: nbt.get_i32(keys.0).unwrap_or(0),
        ..species::ConversionTracker::default()
    };
    match nbt.get_i32(keys.1).unwrap_or(-1) {
        -1 => {
            base.data
                .set(data, super::data::SyncedValue::Boolean(false));
        }
        time => tracker.start(base, data, time, 0),
    }
    tracker
}

fn read_owner(tag: &Tag) -> Option<Uuid> {
    match tag {
        Tag::String(text) => Uuid::parse_str(text).ok(),
        other => read_uuid(other),
    }
}

fn read_tamable(nbt: &Compound, base: &mut EntityBase, mob: &mut MobState) {
    let Some(state) = &mut mob.tamable else {
        return;
    };
    let owner = nbt.get("Owner").and_then(read_owner);
    tamable::set_owner(base, owner);
    tamable::set_tame(base, owner.is_some());
    state.ordered_to_sit = nbt.get_bool("Sitting").unwrap_or(false);
    tamable::set_in_sitting_pose(base, state.ordered_to_sit);
}

fn read_neutral(nbt: &Compound, base: &mut EntityBase, mob: &mut MobState) {
    let Some(state) = &mut mob.neutral else {
        return;
    };
    state.legacy_remaining = None;
    state.end_time = match nbt.get_i64("anger_end_time") {
        Some(end) => end,
        None => {
            state.legacy_remaining = nbt.get_i32("AngerTime");
            neutral::NO_ANGER_END_TIME
        }
    };
    state.target = nbt
        .get("angry_at")
        .or_else(|| nbt.get("AngryAt"))
        .and_then(read_uuid);
    if let Some(index) = neutral::synced_end_time_index(base.kind) {
        base.data
            .set(index, super::data::SyncedValue::Long(state.end_time));
    }
}

fn read_species(nbt: &Compound, base: &mut EntityBase, living: &mut LivingState) {
    let Some(mob) = &mut living.mob else {
        return;
    };
    if let Some(ageable) = &mut mob.ageable {
        read_ageable(nbt, base, ageable);
    }
    read_tamable(nbt, base, mob);
    read_neutral(nbt, base, mob);
    if let Some(farm) = mob.species.variant_family() {
        if let Some(id) = nbt
            .get_str("variant")
            .and_then(|name| farm.variant_index(name))
        {
            species::set_variant(base, farm, id);
        }
        if let Some(id) = nbt
            .get_str("sound_variant")
            .and_then(|name| farm.sound_variant_index(name))
        {
            species::set_sound_variant(base, farm, id);
        }
    }
    match &mut mob.species {
        Species::Chicken(chicken) => {
            chicken.jockey = nbt.get_bool("IsChickenJockey").unwrap_or(false);
            if let Some(time) = nbt.get_i32("EggLayTime") {
                chicken.egg_time = Some(time);
            }
        }
        Species::Sheep(_) => {
            species::set_sheared(base, nbt.get_bool("Sheared").unwrap_or(false));
            let color = nbt
                .get_i64("Color")
                .map_or(0, |v| species::dye_by_id(i32::from(v as i8)));
            species::set_wool_color(base, color);
        }
        Species::Zombie(zombie) => {
            let baby = nbt.get_bool("IsBaby").unwrap_or(false);
            species::set_zombie_baby(base, &mut living.attributes, baby);
            zombie.can_break_doors = nbt.get_bool("CanBreakDoors").unwrap_or(false);
            zombie.drowning = read_tracker(
                nbt,
                base,
                ("InWaterTime", "DrownedConversionTime"),
                species::ZOMBIE_DATA_DROWNED_CONVERSION,
            );
            if let Some(villager) = &mut zombie.villager {
                species::zombie_villager::read(nbt, base, villager);
            }
        }
        Species::Skeleton(skeleton) => {
            skeleton.freezing = read_tracker(
                nbt,
                base,
                ("FreezingTime", "StrayConversionTime"),
                species::SKELETON_DATA_STRAY_CONVERSION,
            );
        }
        Species::Creeper(creeper) => {
            species::set_powered(base, nbt.get_bool("powered").unwrap_or(false));
            creeper.max_swell = nbt
                .get_i64("Fuse")
                .map_or(species::DEFAULT_MAX_SWELL, |v| i32::from(v as i16));
            creeper.explosion_radius = nbt
                .get_i64("ExplosionRadius")
                .map_or(species::DEFAULT_EXPLOSION_RADIUS, |v| i32::from(v as i8));
            if nbt.get_bool("ignited").unwrap_or(false) {
                species::ignite(base);
            }
        }
        Species::Wolf(state) => species::wolf::read(nbt, base, state),
        Species::Cat(state) => species::cat::read(nbt, base, state),
        Species::Fox(state) => species::fox::read(nbt, base, state),
        Species::Panda(state) => species::panda::read(nbt, base, state),
        Species::PolarBear(state) => species::polar_bear::read(nbt, base, state),
        Species::Enderman(state) => species::enderman::read(nbt, base, state),
        Species::Endermite(state) => species::endermite::read(nbt, base, state),
        Species::Silverfish(state) => species::silverfish::read(nbt, base, state),
        Species::IronGolem(state) => species::iron_golem::read(nbt, base, state),
        Species::SnowGolem(state) => species::snow_golem::read(nbt, base, state),
        Species::Blaze(state) => species::blaze::read(nbt, base, state),
        Species::Ghast(state) => species::ghast::read(nbt, base, state),
        Species::Shulker(state) => species::shulker::read(nbt, base, state),
        Species::Rabbit(state) => {
            species::rabbit::read(nbt, base, &mut living.attributes, state);
        }
        Species::Ocelot(state) => species::ocelot::read(nbt, base, state),
        Species::Witch(state) => species::witch::read(nbt, base, state),
        Species::Pillager(state) => {
            species::pillager::read(nbt, base, state);
            mob.can_pick_up_loot = true;
        }
        Species::Hoglin(state) => {
            species::hoglin::read(nbt, base, &mut living.attributes, state);
        }
        Species::Piglin(state) => {
            species::piglin::read(nbt, base, &mut living.attributes, state);
            mob.can_pick_up_loot = nbt.get_bool("CanPickUpLoot").unwrap_or(true);
        }
        Species::Goat(state) => species::goat::read(nbt, base, state),
        Species::Armadillo(state) => species::armadillo::read(nbt, base, state),
        Species::Creaking(state) => species::creaking::read(nbt, base, state),
        Species::Villager(state) => {
            if let Some(merchant) = mob.merchant.as_mut() {
                species::villager::read(nbt, base, state, merchant);
            }
        }
        Species::WanderingTrader(state) => {
            if let Some(merchant) = mob.merchant.as_mut() {
                species::wandering_trader::read(nbt, state, merchant);
            }
            if let Some(ageable) = mob.ageable.as_mut() {
                ageable.age = ageable.age.max(0);
            }
        }
        Species::Plain | Species::Cow | Species::Pig => {}
    }
}

fn read_arrow(nbt: &Compound, base: &mut EntityBase, state: &mut ArrowState) {
    state.owner = nbt.get("Owner").and_then(read_uuid);
    state.left_owner = nbt.get_bool("LeftOwner").unwrap_or(false);
    state.has_been_shot = nbt.get_bool("HasBeenShot").unwrap_or(false);
    state.life = nbt.get_i64("life").map_or(0, |v| i32::from(v as i16));
    state.last_state = nbt.get_compound("inBlockState").map(block_state_from_nbt);
    state.shake_time = nbt.get_i64("shake").map_or(0, |v| i32::from(v as u8));
    arrow::set_in_ground(base, nbt.get_bool("inGround").unwrap_or(false));
    state.base_damage = nbt.get_f64("damage").unwrap_or(arrow::BASE_DAMAGE);
    state.pickup = arrow::Pickup::from_id(nbt.get_i64("pickup").map_or(0, |v| v as i8));
    arrow::set_crit(base, nbt.get_bool("crit").unwrap_or(false));
    arrow::set_pierce_level(base, nbt.get_i64("PierceLevel").map_or(0, |v| v as i8));
    state.sound = nbt
        .get_str("SoundEvent")
        .map_or(arrow::HIT_SOUND.to_owned(), str::to_owned);
    state.item = nbt
        .get_compound("item")
        .and_then(item_stack_from_nbt)
        .unwrap_or_else(arrow::default_item);
    state.weapon = nbt.get_compound("weapon").and_then(item_stack_from_nbt);
}

fn read_thrown(nbt: &Compound, base: &mut EntityBase, state: &mut ThrownState) {
    state.owner = nbt.get("Owner").and_then(read_uuid);
    state.left_owner = nbt.get_bool("LeftOwner").unwrap_or(false);
    state.has_been_shot = nbt.get_bool("HasBeenShot").unwrap_or(false);
    let stack = nbt
        .get_compound("Item")
        .and_then(item_stack_from_nbt)
        .unwrap_or_else(|| thrown::default_item_of(base.kind));
    thrown::set_item(base, &stack);
}

fn write_thrown(base: &EntityBase, state: &ThrownState, out: &mut Compound) {
    if let Some(owner) = state.owner {
        out.insert("Owner", uuid_tag(owner));
    }
    if state.left_owner {
        out.insert("LeftOwner", true);
    }
    out.insert("HasBeenShot", bool_tag(state.has_been_shot));
    let stack = thrown::item(base).unwrap_or_else(|| thrown::default_item_of(base.kind));
    out.insert("Item", item_stack_to_nbt(&stack));
}

fn read_bullet(nbt: &Compound, state: &mut BulletState) {
    state.owner = nbt.get("Owner").and_then(read_uuid);
    state.left_owner = nbt.get_bool("LeftOwner").unwrap_or(false);
    state.has_been_shot = nbt.get_bool("HasBeenShot").unwrap_or(false);
    state.flight_steps = nbt.get_i32("Steps").unwrap_or(0);
    state.target_delta = Vec3::new(
        nbt.get_f64("TXD").unwrap_or(0.0),
        nbt.get_f64("TYD").unwrap_or(0.0),
        nbt.get_f64("TZD").unwrap_or(0.0),
    );
    state.move_direction = nbt
        .get_i64("Dir")
        .map(|id| bullet::from_legacy_id(id as i8));
    state.target = nbt.get("Target").and_then(read_uuid);
}

fn write_bullet(state: &BulletState, out: &mut Compound) {
    if let Some(owner) = state.owner {
        out.insert("Owner", uuid_tag(owner));
    }
    if state.left_owner {
        out.insert("LeftOwner", true);
    }
    out.insert("HasBeenShot", bool_tag(state.has_been_shot));
    if let Some(target) = state.target {
        out.insert("Target", uuid_tag(target));
    }
    out.insert("Steps", state.flight_steps);
    out.insert("TXD", state.target_delta.x);
    out.insert("TYD", state.target_delta.y);
    out.insert("TZD", state.target_delta.z);
    if let Some(direction) = state.move_direction {
        out.insert("Dir", bullet::legacy_id(direction));
    }
}

fn read_hurting(nbt: &Compound, base: &mut EntityBase, state: &mut HurtingState) {
    state.owner = nbt.get("Owner").and_then(read_uuid);
    state.left_owner = nbt.get_bool("LeftOwner").unwrap_or(false);
    state.has_been_shot = nbt.get_bool("HasBeenShot").unwrap_or(false);
    state.acceleration_power = nbt
        .get_f64("acceleration_power")
        .unwrap_or(state.acceleration_power);
    if hurting::is_fireball(base.kind) {
        let stack = nbt
            .get_compound("Item")
            .and_then(item_stack_from_nbt)
            .unwrap_or_else(hurting::default_item);
        hurting::set_item(base, &stack);
    }
    if base.kind == EntityKind::Fireball {
        state.explosion_power = nbt
            .get_i64("ExplosionPower")
            .map_or(hurting::DEFAULT_EXPLOSION_POWER, |power| {
                i64::from(power as i8) as i32
            });
    }
    if base.kind == EntityKind::WitherSkull {
        hurting::set_dangerous(base, nbt.get_bool("dangerous").unwrap_or(false));
    }
}

fn write_hurting(base: &EntityBase, state: &HurtingState, out: &mut Compound) {
    if let Some(owner) = state.owner {
        out.insert("Owner", uuid_tag(owner));
    }
    if state.left_owner {
        out.insert("LeftOwner", true);
    }
    out.insert("HasBeenShot", bool_tag(state.has_been_shot));
    out.insert("acceleration_power", state.acceleration_power);
    if hurting::is_fireball(base.kind) {
        let stack = hurting::item(base).unwrap_or_else(hurting::default_item);
        out.insert("Item", item_stack_to_nbt(&stack));
    }
    if base.kind == EntityKind::Fireball {
        out.insert("ExplosionPower", state.explosion_power as i8);
    }
    if base.kind == EntityKind::WitherSkull {
        out.insert("dangerous", bool_tag(hurting::is_dangerous(base)));
    }
}

fn write_arrow(base: &EntityBase, state: &ArrowState, out: &mut Compound) {
    if let Some(owner) = state.owner {
        out.insert("Owner", uuid_tag(owner));
    }
    if state.left_owner {
        out.insert("LeftOwner", true);
    }
    out.insert("HasBeenShot", bool_tag(state.has_been_shot));
    out.insert("life", state.life as i16);
    if let Some(last) = state.last_state {
        out.insert("inBlockState", block_state_to_nbt(last));
    }
    out.insert("shake", state.shake_time as i8);
    out.insert("inGround", bool_tag(arrow::in_ground(base)));
    out.insert("pickup", state.pickup.id());
    out.insert("damage", state.base_damage);
    out.insert("crit", bool_tag(arrow::is_crit(base)));
    out.insert("PierceLevel", arrow::pierce_level(base));
    out.insert("SoundEvent", state.sound.as_str());
    out.insert("item", item_stack_to_nbt(&state.item));
    if let Some(weapon) = &state.weapon {
        out.insert("weapon", item_stack_to_nbt(weapon));
    }
}

fn species_keys(mob: &MobState) -> Vec<&'static [&'static str]> {
    let mut keys: Vec<&'static [&'static str]> = Vec::new();
    if let Some(ageable) = &mob.ageable {
        keys.push(&AGEABLE_KEYS);
        if ageable.love.is_some() {
            keys.push(&ANIMAL_KEYS);
        }
    }
    if mob.tamable.is_some() {
        keys.push(&TAMABLE_KEYS);
    }
    if mob.neutral.is_some() {
        keys.push(&NEUTRAL_KEYS);
    }
    if matches!(mob.species, Species::Wolf(_) | Species::Cat(_)) {
        keys.push(&FARM_KEYS);
    }
    if let Species::Zombie(zombie) = &mob.species
        && zombie.villager.is_some()
    {
        keys.push(species::zombie_villager::KEYS);
    }
    keys.push(match mob.species {
        Species::Cow | Species::Pig => &FARM_KEYS,
        Species::Chicken(_) => &CHICKEN_KEYS,
        Species::Sheep(_) => &SHEEP_KEYS,
        Species::Zombie(_) => &ZOMBIE_KEYS,
        Species::Skeleton(_) => &SKELETON_KEYS,
        Species::Creeper(_) => &CREEPER_KEYS,
        Species::Wolf(_) => species::wolf::KEYS,
        Species::Cat(_) => species::cat::KEYS,
        Species::Fox(_) => species::fox::KEYS,
        Species::Panda(_) => species::panda::KEYS,
        Species::PolarBear(_) => species::polar_bear::KEYS,
        Species::Enderman(_) => species::enderman::KEYS,
        Species::Endermite(_) => species::endermite::KEYS,
        Species::Silverfish(_) => species::silverfish::KEYS,
        Species::IronGolem(_) => species::iron_golem::KEYS,
        Species::SnowGolem(_) => species::snow_golem::KEYS,
        Species::Blaze(_) => species::blaze::KEYS,
        Species::Ghast(_) => species::ghast::KEYS,
        Species::Shulker(_) => species::shulker::KEYS,
        Species::Rabbit(_) => species::rabbit::KEYS,
        Species::Ocelot(_) => species::ocelot::KEYS,
        Species::Witch(_) => species::witch::KEYS,
        Species::Pillager(_) => species::pillager::KEYS,
        Species::Hoglin(_) => species::hoglin::KEYS,
        Species::Piglin(_) => species::piglin::KEYS,
        Species::Goat(_) => species::goat::KEYS,
        Species::Armadillo(_) => species::armadillo::KEYS,
        Species::Creaking(_) => species::creaking::KEYS,
        Species::Villager(_) => species::villager::KEYS,
        Species::WanderingTrader(_) => species::wandering_trader::KEYS,
        Species::Plain => &[],
    });
    keys
}

fn extra_keys(nbt: &Compound, known: &[&[&str]]) -> Compound {
    nbt.iter()
        .filter(|(key, _)| !known.iter().any(|keys| keys.contains(key)))
        .map(|(key, value)| (key.to_owned(), value.clone()))
        .collect()
}

pub fn load_entity(nbt: &Compound, id: EntityId, fallback_uuid: Uuid) -> Result<Entity, LoadError> {
    let name = nbt.get_str("id").ok_or(LoadError::MissingId)?;
    let kind =
        EntityKind::from_name(name).ok_or_else(|| LoadError::UnknownType(name.to_owned()))?;
    if !kind.can_serialize() {
        return Err(LoadError::NotSerializable(kind.name()));
    }
    let mut base = EntityBase::new(id, fallback_uuid, kind);
    if !is_simulated(kind) {
        read_base(nbt, &mut base)?;
        return Ok(Entity::new(base, EntityState::Inert(nbt.clone())));
    }
    let mut state = match kind {
        EntityKind::Item => EntityState::Item(ItemState::new(
            &mut base,
            ItemStack::new(ItemId(0), NonZeroU8::MIN),
            0,
        )),
        EntityKind::FallingBlock => EntityState::Falling(FallingState::new(
            &mut base,
            BlockKind::Sand.default_state(),
            BlockPos::new(0, 0, 0),
        )),
        EntityKind::ExperienceOrb => EntityState::Orb(OrbState::new(&mut base, 0)),
        EntityKind::Arrow => {
            let seed = fallback_uuid.as_u64_pair().0 as i64;
            EntityState::Arrow(Box::new(ArrowState::new(&mut base, seed)))
        }
        EntityKind::Snowball | EntityKind::SplashPotion => {
            let seed = fallback_uuid.as_u64_pair().0 as i64;
            EntityState::Thrown(Box::new(ThrownState::new(&mut base, seed)))
        }
        _ if hurting::is_hurting_projectile(kind) => {
            EntityState::Hurting(Box::new(HurtingState::new(&mut base)))
        }
        EntityKind::ShulkerBullet => {
            let seed = fallback_uuid.as_u64_pair().0 as i64;
            EntityState::Bullet(Box::new(BulletState::new(&mut base, seed)))
        }
        _ => EntityState::Living(Box::new(LivingState::new(&mut base, 0.0))),
    };
    read_base(nbt, &mut base)?;
    let known: &[&[&str]] = match &mut state {
        EntityState::Item(item_state) => {
            item_state.health = nbt.get_i64("Health").map_or(5, |v| v as i16 as i32);
            item_state.age = nbt.get_i64("Age").map_or(0, |v| v as i16 as i32);
            item_state.pickup_delay = nbt.get_i64("PickupDelay").map_or(0, |v| v as i16 as i32);
            item_state.owner = nbt.get("Owner").and_then(read_uuid);
            item_state.thrower = nbt.get("Thrower").and_then(read_uuid);
            let stack = nbt
                .get_compound("Item")
                .and_then(item_stack_from_nbt)
                .ok_or(LoadError::EmptyItem)?;
            item::set_stack(&mut base, Some(stack));
            &[&ENTITY_KEYS, &ITEM_KEYS]
        }
        EntityState::Falling(falling) => {
            falling.state = nbt
                .get_compound("BlockState")
                .map_or(BlockKind::Sand.default_state(), block_state_from_nbt);
            falling.time = nbt.get_i32("Time").unwrap_or(0);
            if let Some(hurt) = nbt.get_bool("HurtEntities") {
                falling.hurt_entities = hurt;
                falling.fall_hurt_amount = nbt.get_f64("FallHurtAmount").map_or(0.0, |v| v as f32);
                falling.fall_hurt_max = nbt.get_i32("FallHurtMax").unwrap_or(40);
            }
            falling.drop_item = nbt.get_bool("DropItem").unwrap_or(true);
            falling.block_data = nbt.get_compound("TileEntityData").cloned();
            falling.cancel_drop = nbt.get_bool("CancelDrop").unwrap_or(false);
            &[&ENTITY_KEYS, &FALLING_KEYS]
        }
        EntityState::Living(living_state) => {
            read_living(nbt, &mut base, living_state);
            let mut known: Vec<&[&str]> = vec![&ENTITY_KEYS, &LIVING_KEYS];
            if let Some(mob) = &living_state.mob {
                known.extend(species_keys(mob));
            }
            let extra = extra_keys(nbt, &known);
            return Ok(Entity { base, state, extra });
        }
        EntityState::Orb(orb_state) => {
            orb_state.health = nbt
                .get_i64("Health")
                .map_or(orb::DEFAULT_HEALTH, |v| i32::from(v as i16));
            orb_state.age = nbt.get_i64("Age").map_or(0, |v| i32::from(v as i16));
            orb::set_value(
                &mut base,
                nbt.get_i64("Value").map_or(0, |v| i32::from(v as i16)),
            );
            orb_state.count = nbt.get_i32("Count").filter(|count| *count > 0).unwrap_or(1);
            &[&ENTITY_KEYS, &ORB_KEYS]
        }
        EntityState::Arrow(arrow_state) => {
            read_arrow(nbt, &mut base, arrow_state);
            &[&ENTITY_KEYS, &ARROW_KEYS]
        }
        EntityState::Thrown(thrown_state) => {
            read_thrown(nbt, &mut base, thrown_state);
            &[&ENTITY_KEYS, &THROWN_KEYS]
        }
        EntityState::Hurting(hurting_state) => {
            read_hurting(nbt, &mut base, hurting_state);
            &[&ENTITY_KEYS, &HURTING_KEYS]
        }
        EntityState::Bullet(bullet_state) => {
            read_bullet(nbt, bullet_state);
            &[&ENTITY_KEYS, &BULLET_KEYS, bullet::KEYS]
        }
        EntityState::Inert(_) => &[],
    };
    let extra = extra_keys(nbt, known);
    Ok(Entity { base, state, extra })
}

fn write_base(entity: &Entity, out: &mut Compound, world: Option<Uuid>) {
    let base = &entity.base;
    out.insert("Pos", doubles(base.position));
    out.insert("Motion", doubles(base.velocity));
    let yaw = if base.yaw.is_nan() { 0.0 } else { base.yaw };
    let pitch = if base.pitch.is_nan() { 0.0 } else { base.pitch };
    out.insert(
        "Rotation",
        Tag::List(vec![Tag::Float(yaw), Tag::Float(pitch)]),
    );
    out.insert("fall_distance", base.fall_distance);
    out.insert("Fire", base.remaining_fire_ticks as i16);
    out.insert("Air", base.air_supply() as i16);
    out.insert("OnGround", bool_tag(base.on_ground));
    out.insert("Invulnerable", bool_tag(base.invulnerable));
    out.insert("PortalCooldown", base.portal_cooldown);
    if base.invulnerable_time > 0 {
        out.insert("invulnerable_time", base.invulnerable_time);
    }
    out.insert("UUID", uuid_tag(base.uuid));
    if let Some(world) = world {
        let (most, least) = world.as_u64_pair();
        out.insert("WorldUUIDLeast", least as i64);
        out.insert("WorldUUIDMost", most as i64);
    }
    out.insert("Bukkit.updateLevel", BUKKIT_UPDATE_LEVEL);
    if !base.persist {
        out.insert("Bukkit.persist", false);
    }
    if !base.visible_by_default {
        out.insert("Bukkit.visibleByDefault", false);
    }
    if base.persistent_invisibility {
        out.insert("Bukkit.invisible", true);
    }
    if base.max_air_ticks != MAX_AIR_SUPPLY {
        out.insert("Bukkit.MaxAirSupply", base.max_air_ticks);
    }
    out.insert("Spigot.ticksLived", base.tick_count);
    if let Some(name) = base.custom_name() {
        out.insert("CustomName", component_to_tag(name));
    }
    if base.custom_name_visible() {
        out.insert("CustomNameVisible", true);
    }
    if base.is_silent() {
        out.insert("Silent", true);
    }
    if base.is_no_gravity() {
        out.insert("NoGravity", true);
    }
    if base.has_glowing_tag {
        out.insert("Glowing", true);
    }
    if base.ticks_frozen() > 0 {
        out.insert("TicksFrozen", base.ticks_frozen());
    }
    if base.has_visual_fire {
        out.insert("HasVisualFire", true);
    }
    if !base.tags.is_empty() {
        out.insert(
            "Tags",
            Tag::List(base.tags.iter().map(|t| Tag::String(t.clone())).collect()),
        );
    }
    if let Some(data) = &base.custom_data
        && !data.is_empty()
    {
        out.insert("data", data.clone());
    }
}

fn write_living(base: &EntityBase, living: &LivingState, out: &mut Compound) {
    out.insert("Health", health(base));
    out.insert("HurtTime", living.hurt_time as i16);
    out.insert("DeathTime", living.death_time as i16);
    out.insert("AbsorptionAmount", living.absorption);
    out.insert(
        "current_impulse_context_reset_grace_time",
        living.impulse_grace,
    );
    let attributes: Vec<Tag> = living
        .attributes
        .instances()
        .iter()
        .map(|instance| {
            let mut entry = Compound::new()
                .with("id", instance.attribute)
                .with("base", instance.base);
            let saved: Vec<&AttributeModifier> = instance
                .modifiers
                .iter()
                .filter(|m| !TRANSIENT_MODIFIERS.contains(&m.id.as_ref()))
                .collect();
            if !saved.is_empty() {
                entry.insert(
                    "modifiers",
                    Tag::List(
                        saved
                            .iter()
                            .map(|m| {
                                Tag::Compound(
                                    Compound::new()
                                        .with("id", m.id.as_ref())
                                        .with("amount", m.amount)
                                        .with("operation", operation_name(m.operation)),
                                )
                            })
                            .collect(),
                    ),
                );
            }
            Tag::Compound(entry)
        })
        .collect();
    out.insert("attributes", Tag::List(attributes));
    if !living.effects.is_empty() {
        out.insert(
            "active_effects",
            Tag::List(
                living
                    .effects
                    .iter()
                    .map(|e| Tag::Compound(effect_to_nbt(e)))
                    .collect(),
            ),
        );
    }
    out.insert("FallFlying", bool_tag(living.fall_flying));
    let equipment: Compound = EQUIPMENT_SLOTS
        .iter()
        .filter_map(|slot| {
            living.equipment[living::slot_index(*slot)]
                .as_ref()
                .map(|stack| {
                    (
                        slot.name().to_owned(),
                        Tag::Compound(item_stack_to_nbt(stack)),
                    )
                })
        })
        .collect();
    if !equipment.is_empty() {
        out.insert("equipment", equipment);
    }
    if let Some(mob) = &living.mob {
        write_mob(base, mob, out);
    }
}

fn write_mob(base: &EntityBase, mob: &MobState, out: &mut Compound) {
    out.insert("CanPickUpLoot", bool_tag(mob.can_pick_up_loot));
    out.insert("PersistenceRequired", bool_tag(mob.persistence_required));
    let chances: Compound = EQUIPMENT_SLOTS
        .iter()
        .filter(|slot| mob.drop_chances[living::slot_index(**slot)] != living::DEFAULT_DROP_CHANCE)
        .map(|slot| {
            (
                slot.name().to_owned(),
                Tag::Float(mob.drop_chances[living::slot_index(*slot)]),
            )
        })
        .collect();
    if !chances.is_empty() {
        out.insert("drop_chances", chances);
    }
    if mob.home_radius != -1 {
        out.insert("home_radius", mob.home_radius);
        let pos = mob.home_pos;
        out.insert("home_pos", Tag::IntArray(vec![pos.x, pos.y, pos.z]));
    }
    out.insert("LeftHanded", bool_tag(is_left_handed(base)));
    if let Some(table) = &mob.death_loot_table {
        out.insert("DeathLootTable", table.as_str());
    }
    if mob.death_loot_table_seed != 0 {
        out.insert("DeathLootTableSeed", mob.death_loot_table_seed);
    }
    if is_no_ai(base) {
        out.insert("NoAI", true);
    }
    out.insert("Bukkit.Aware", bool_tag(mob.aware));
    write_species(base, mob, out);
}

fn write_tracker(
    out: &mut Compound,
    keys: (&str, &str),
    tracker: &species::ConversionTracker,
    converting: bool,
    afflicted: bool,
) {
    out.insert(
        keys.1,
        if converting {
            tracker.conversion_time
        } else {
            -1
        },
    );
    out.insert(
        keys.0,
        if afflicted {
            tracker.affliction_time
        } else {
            -1
        },
    );
}

fn write_species(base: &EntityBase, mob: &MobState, out: &mut Compound) {
    if let Some(ageable) = &mob.ageable {
        if ageable::can_be_baby(base.kind) {
            out.insert("Age", ageable.age);
            out.insert("ForcedAge", ageable.forced_age);
            out.insert("AgeLocked", bool_tag(ageable::is_age_locked(base)));
        }
        if let Some(love) = &ageable.love {
            out.insert("InLove", love.ticks);
            if let Some(cause) = love.cause {
                out.insert("LoveCause", uuid_tag(cause));
            }
        }
    }
    if let Some(state) = &mob.tamable {
        if let Some(owner) = tamable::owner(base) {
            out.insert("Owner", uuid_tag(owner));
        }
        out.insert("Sitting", bool_tag(state.ordered_to_sit));
    }
    if let Some(state) = &mob.neutral {
        out.insert("anger_end_time", state.end_time);
        if let Some(target) = state.target {
            out.insert("angry_at", uuid_tag(target));
        }
    }
    if let Some(farm) = mob.species.variant_family() {
        out.insert(
            "variant",
            species::variant_name(farm, species::variant(base, farm)),
        );
        out.insert(
            "sound_variant",
            species::sound_variant_name(farm, species::sound_variant(base, farm)),
        );
    }
    match &mob.species {
        Species::Chicken(chicken) => {
            out.insert("IsChickenJockey", bool_tag(chicken.jockey));
            if let Some(time) = chicken.egg_time {
                out.insert("EggLayTime", time);
            }
        }
        Species::Sheep(_) => {
            out.insert("Sheared", bool_tag(species::is_sheared(base)));
            out.insert("Color", species::wool_color(base) as i8);
        }
        Species::Zombie(zombie) => {
            out.insert("IsBaby", bool_tag(species::zombie_is_baby(base)));
            out.insert("CanBreakDoors", bool_tag(zombie.can_break_doors));
            write_tracker(
                out,
                ("InWaterTime", "DrownedConversionTime"),
                &zombie.drowning,
                base.data.boolean(species::ZOMBIE_DATA_DROWNED_CONVERSION),
                base.eye_in_water,
            );
            if let Some(villager) = &zombie.villager {
                species::zombie_villager::write(base, villager, out);
            }
        }
        Species::Skeleton(skeleton) => {
            write_tracker(
                out,
                ("FreezingTime", "StrayConversionTime"),
                &skeleton.freezing,
                base.data.boolean(species::SKELETON_DATA_STRAY_CONVERSION),
                base.in_powder_snow,
            );
        }
        Species::Creeper(creeper) => {
            out.insert("powered", bool_tag(species::is_powered(base)));
            out.insert("Fuse", creeper.max_swell as i16);
            out.insert("ExplosionRadius", creeper.explosion_radius as i8);
            out.insert("ignited", bool_tag(species::is_ignited(base)));
        }
        Species::Wolf(state) => species::wolf::write(base, state, out),
        Species::Cat(state) => species::cat::write(base, state, out),
        Species::Fox(state) => species::fox::write(base, state, out),
        Species::Panda(state) => species::panda::write(base, state, out),
        Species::PolarBear(state) => species::polar_bear::write(base, state, out),
        Species::Enderman(state) => species::enderman::write(base, state, out),
        Species::Endermite(state) => species::endermite::write(base, state, out),
        Species::Silverfish(state) => species::silverfish::write(base, state, out),
        Species::IronGolem(state) => species::iron_golem::write(base, state, out),
        Species::SnowGolem(state) => species::snow_golem::write(base, state, out),
        Species::Blaze(state) => species::blaze::write(base, state, out),
        Species::Ghast(state) => species::ghast::write(base, state, out),
        Species::Shulker(state) => species::shulker::write(base, state, out),
        Species::Rabbit(state) => species::rabbit::write(base, state, out),
        Species::Ocelot(state) => species::ocelot::write(base, state, out),
        Species::Witch(state) => species::witch::write(base, state, out),
        Species::Pillager(state) => species::pillager::write(base, state, out),
        Species::Hoglin(state) => species::hoglin::write(base, state, out),
        Species::Piglin(state) => species::piglin::write(base, state, out),
        Species::Goat(state) => species::goat::write(base, state, out),
        Species::Armadillo(state) => species::armadillo::write(base, state, out),
        Species::Creaking(state) => species::creaking::write(base, state, out),
        Species::Villager(state) => {
            if let Some(merchant) = mob.merchant.as_deref() {
                species::villager::write(base, state, merchant, out);
            }
        }
        Species::WanderingTrader(state) => {
            if let Some(merchant) = mob.merchant.as_deref() {
                species::wandering_trader::write(state, merchant, out);
            }
        }
        Species::Plain | Species::Cow | Species::Pig => {}
    }
}

pub fn save_entity(entity: &Entity, world: Option<Uuid>) -> Option<Compound> {
    if let Some(reason) = entity.base.removal
        && !matches!(reason, RemovalReason::UnloadedToChunk)
    {
        return None;
    }
    if !entity.base.persist || !entity.base.kind.can_serialize() {
        return None;
    }
    if let EntityState::Inert(nbt) = &entity.state {
        return Some(nbt.clone());
    }
    let mut out = Compound::new();
    out.insert("id", entity.base.kind.name());
    write_base(entity, &mut out, world);
    match &entity.state {
        EntityState::Item(item_state) => {
            out.insert("Health", item_state.health as i16);
            out.insert("Age", item_state.age as i16);
            out.insert("PickupDelay", item_state.pickup_delay as i16);
            if let Some(thrower) = item_state.thrower {
                out.insert("Thrower", uuid_tag(thrower));
            }
            if let Some(owner) = item_state.owner {
                out.insert("Owner", uuid_tag(owner));
            }
            if let Some(stack) = item::stack(&entity.base) {
                out.insert("Item", item_stack_to_nbt(stack));
            }
        }
        EntityState::Falling(falling) => {
            out.insert("BlockState", block_state_to_nbt(falling.state));
            out.insert("Time", falling.time);
            out.insert("DropItem", bool_tag(falling.drop_item));
            out.insert("HurtEntities", bool_tag(falling.hurt_entities));
            out.insert("FallHurtAmount", falling.fall_hurt_amount);
            out.insert("FallHurtMax", falling.fall_hurt_max);
            if let Some(data) = &falling.block_data {
                out.insert("TileEntityData", data.clone());
            }
            out.insert("CancelDrop", bool_tag(falling.cancel_drop));
        }
        EntityState::Living(living_state) => write_living(&entity.base, living_state, &mut out),
        EntityState::Orb(orb_state) => {
            out.insert("Health", orb_state.health as i16);
            out.insert("Age", orb_state.age as i16);
            out.insert("Value", orb::value(&entity.base) as i16);
            out.insert("Count", orb_state.count);
        }
        EntityState::Arrow(arrow_state) => write_arrow(&entity.base, arrow_state, &mut out),
        EntityState::Thrown(thrown_state) => {
            write_thrown(&entity.base, thrown_state, &mut out);
        }
        EntityState::Hurting(hurting_state) => {
            write_hurting(&entity.base, hurting_state, &mut out);
        }
        EntityState::Bullet(bullet_state) => write_bullet(bullet_state, &mut out),
        EntityState::Inert(_) => {}
    }
    for (key, value) in entity.extra.iter() {
        if !out.contains_key(key) {
            out.insert(key, value.clone());
        }
    }
    Some(out)
}

pub fn entity_position(nbt: &Compound) -> Option<Vec3> {
    read_vec3(nbt, "Pos")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::NamedColor;
    use rspigot_data::variants::VariantFamily;

    fn cow_nbt() -> Compound {
        Compound::new()
            .with("id", "minecraft:cow")
            .with("Pos", doubles(Vec3::new(1.5, 64.0, 2.5)))
            .with(
                "Rotation",
                Tag::List(vec![Tag::Float(90.0), Tag::Float(0.0)]),
            )
            .with("UUID", uuid_tag(Uuid::from_u128(42)))
            .with("Health", 7.5f32)
            .with("NoAI", true)
            .with("variant", "minecraft:warm")
    }

    #[test]
    fn mob_round_trip_keeps_known_and_unknown_keys() {
        let entity = load_entity(&cow_nbt(), EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(entity.base.uuid, Uuid::from_u128(42));
        assert_eq!(entity.base.position, Vec3::new(1.5, 64.0, 2.5));
        assert_eq!(entity.base.yaw, 90.0);
        assert_eq!(health(&entity.base), 7.5);
        assert!(is_no_ai(&entity.base));
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_str("id"), Some("minecraft:cow"));
        assert_eq!(saved.get_str("variant"), Some("minecraft:warm"));
        assert_eq!(saved.get_bool("NoAI"), Some(true));
        assert_eq!(saved.get_f64("Health"), Some(7.5));
        let again = load_entity(&saved, EntityId(2), Uuid::from_u128(2)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn mob_home_restriction_survives_a_round_trip() {
        let nbt = cow_nbt()
            .with("home_radius", 6)
            .with("home_pos", Tag::IntArray(vec![3, 64, -2]));
        let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        let mob = entity.living().and_then(|l| l.mob.as_ref()).unwrap();
        assert_eq!(
            (mob.home_pos, mob.home_radius),
            (BlockPos::new(3, 64, -2), 6)
        );
        assert!(!entity.extra.contains_key("home_pos"));
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("home_radius"), Some(6));
        assert_eq!(saved.get_int_array("home_pos"), Some(&[3, 64, -2][..]));
        let free = load_entity(&cow_nbt(), EntityId(2), Uuid::from_u128(2)).unwrap();
        let saved = save_entity(&free, None).unwrap();
        assert!(!saved.contains_key("home_radius"));
        assert!(!saved.contains_key("home_pos"));
    }

    fn wolf_nbt() -> Compound {
        Compound::new()
            .with("id", "minecraft:wolf")
            .with("Pos", doubles(Vec3::new(0.5, 64.0, 0.5)))
            .with("UUID", uuid_tag(Uuid::from_u128(7)))
            .with("Owner", uuid_tag(Uuid::from_u128(99)))
            .with("Sitting", true)
            .with("anger_end_time", 4000i64)
            .with("angry_at", uuid_tag(Uuid::from_u128(5)))
    }

    #[test]
    fn wolf_and_cat_variants_round_trip_through_nbt() {
        for (id, variant, sound, family) in [
            (
                "minecraft:wolf",
                "minecraft:ashen",
                "minecraft:grumpy",
                VariantFamily::Wolf,
            ),
            (
                "minecraft:cat",
                "minecraft:calico",
                "minecraft:royal",
                VariantFamily::Cat,
            ),
        ] {
            let nbt = Compound::new()
                .with("id", id)
                .with("Pos", doubles(Vec3::new(0.5, 64.0, 0.5)))
                .with("UUID", uuid_tag(Uuid::from_u128(7)))
                .with("variant", variant)
                .with("sound_variant", sound);
            let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
            assert_eq!(
                species::variant_name(family, species::variant(&entity.base, family)),
                variant
            );
            assert_eq!(
                species::sound_variant_name(family, species::sound_variant(&entity.base, family)),
                sound
            );
            let saved = save_entity(&entity, None).unwrap();
            assert_eq!(saved.get_str("variant"), Some(variant));
            assert_eq!(saved.get_str("sound_variant"), Some(sound));
            assert_eq!(saved.iter().filter(|(k, _)| *k == "variant").count(), 1);
            let fresh = load_entity(
                &Compound::new()
                    .with("id", id)
                    .with("Pos", doubles(Vec3::new(0.5, 64.0, 0.5)))
                    .with("UUID", uuid_tag(Uuid::from_u128(8))),
                EntityId(2),
                Uuid::from_u128(2),
            )
            .unwrap();
            let saved = save_entity(&fresh, None).unwrap();
            let default = if family == VariantFamily::Wolf {
                "minecraft:pale"
            } else {
                "minecraft:black"
            };
            assert_eq!(saved.get_str("variant"), Some(default));
            assert_eq!(saved.get_str("sound_variant"), Some("minecraft:classic"));
        }
    }

    #[test]
    fn a_wolf_keeps_its_collar_and_body_armor() {
        let nbt = wolf_nbt().with("CollarColor", 3i8).with(
            "equipment",
            Compound::new().with(
                "body",
                Compound::new()
                    .with("id", "minecraft:wolf_armor")
                    .with("count", 1),
            ),
        );
        let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        let living = entity.living().unwrap();
        assert_eq!(species::wolf::collar_color(&entity.base), 3);
        assert!(
            living
                .equipment(rspigot_items::EquipmentSlot::Body)
                .is_some()
        );
        assert!(entity.extra.is_empty());
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i64("CollarColor"), Some(3));
        let body = saved
            .get_compound("equipment")
            .and_then(|equipment| equipment.get_compound("body"))
            .and_then(|body| body.get_str("id"));
        assert_eq!(body, Some("minecraft:wolf_armor"));
    }

    #[test]
    fn tamed_and_angry_mobs_keep_their_owner_and_anger() {
        let entity = load_entity(&wolf_nbt(), EntityId(1), Uuid::from_u128(1)).unwrap();
        assert!(tamable::is_tame(&entity.base));
        assert!(tamable::is_in_sitting_pose(&entity.base));
        assert_eq!(tamable::owner(&entity.base), Some(Uuid::from_u128(99)));
        assert_eq!(
            entity.base.data.long(neutral::WOLF_DATA_ANGER_END_TIME),
            4000
        );
        let mob = entity.living().and_then(|l| l.mob.as_ref()).unwrap();
        let anger = mob.neutral.unwrap();
        assert_eq!(anger.end_time, 4000);
        assert_eq!(anger.target, Some(Uuid::from_u128(5)));
        assert!(entity.extra.is_empty());
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get("Owner"), Some(&uuid_tag(Uuid::from_u128(99))));
        assert_eq!(saved.get_bool("Sitting"), Some(true));
        assert_eq!(saved.get_i64("anger_end_time"), Some(4000));
        assert_eq!(saved.get("angry_at"), Some(&uuid_tag(Uuid::from_u128(5))));
    }

    #[test]
    fn a_wild_calm_wolf_writes_no_owner_and_no_target() {
        let nbt = Compound::new()
            .with("id", "minecraft:wolf")
            .with("Pos", doubles(Vec3::new(0.5, 64.0, 0.5)));
        let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert!(!tamable::is_tame(&entity.base));
        let saved = save_entity(&entity, None).unwrap();
        assert!(!saved.contains_key("Owner"));
        assert!(!saved.contains_key("angry_at"));
        assert_eq!(saved.get_i64("anger_end_time"), Some(-1));
        assert_eq!(saved.get_bool("Sitting"), Some(false));
    }

    #[test]
    fn the_legacy_anger_time_waits_for_the_game_time() {
        let mut nbt = wolf_nbt().with("AngerTime", 60);
        nbt.remove("anger_end_time");
        let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        let mob = entity.living().and_then(|l| l.mob.as_ref()).unwrap();
        assert_eq!(mob.neutral.unwrap().legacy_remaining, Some(60));
    }

    #[test]
    fn unknown_types_are_kept_verbatim() {
        let nbt = Compound::new()
            .with("id", "minecraft:armor_stand")
            .with("Pos", doubles(Vec3::new(0.0, 70.0, 0.0)))
            .with("ShowArms", true)
            .with("Pose", Compound::new().with("Head", Tag::List(Vec::new())));
        let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert!(!entity.is_simulated());
        assert_eq!(save_entity(&entity, None), Some(nbt));
    }

    #[test]
    fn excessive_motion_is_dropped_like_vanilla() {
        let nbt = cow_nbt().with("Motion", doubles(Vec3::new(11.0, -0.5, 3.0)));
        let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(entity.base.velocity, Vec3::new(0.0, -0.5, 3.0));
    }

    #[test]
    fn experience_orbs_keep_value_count_and_age() {
        let nbt = Compound::new()
            .with("id", "minecraft:experience_orb")
            .with("Pos", doubles(Vec3::new(0.5, 65.0, 0.5)))
            .with("Value", 7i16)
            .with("Count", 3)
            .with("Age", 120i16)
            .with("Health", 4i16);
        let entity = load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        let EntityState::Orb(state) = &entity.state else {
            panic!("not an orb");
        };
        assert_eq!((state.count, state.age, state.health), (3, 120, 4));
        assert_eq!(orb::value(&entity.base), 7);
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get("Value"), Some(&Tag::Short(7)));
        assert_eq!(saved.get("Count"), Some(&Tag::Int(3)));
        assert_eq!(saved.get("Age"), Some(&Tag::Short(120)));
        assert_eq!(saved.get("Health"), Some(&Tag::Short(4)));
        let invalid = load_entity(
            &nbt.clone().with("Count", 0),
            EntityId(2),
            Uuid::from_u128(2),
        )
        .unwrap();
        assert!(matches!(
            invalid.state,
            EntityState::Orb(OrbState { count: 1, .. })
        ));
    }

    #[test]
    fn styled_names_survive_a_save_and_reload() {
        let mut stack = ItemStack::new(ItemKind::DiamondSword.id(), NonZeroU8::MIN);
        let name = Component::text("Excalibur").color(NamedColor::Gold).bold();
        let mut teal = Component::text("Teal");
        teal.style.color = Some(Color::Rgb(0x008080));
        teal.style.italic = Some(false);
        stack.set_custom_name(Some(&name));
        stack.set_lore(&[Component::from_section_codes("§6A§rB"), teal.clone()]);
        stack.set_enchantments(&[("minecraft:sharpness".into(), 5)]);
        let saved = item_stack_to_nbt(&stack);
        assert_eq!(item_stack_from_nbt(&saved), Some(stack.clone()));

        let mut bytes = Vec::new();
        rspigot_nbt::write_named(&mut bytes, "", &saved);
        let (_, read) = rspigot_nbt::read_named(&mut bytes.as_slice()).unwrap();
        let reloaded = item_stack_from_nbt(&read).unwrap();
        assert_eq!(reloaded.custom_name(), Some(name));
        assert_eq!(
            reloaded.lore(),
            [Component::from_section_codes("§6A§rB"), teal]
        );
        assert_eq!(
            reloaded.enchantments(),
            [("minecraft:sharpness".to_owned(), 5)]
        );
    }

    #[test]
    fn a_reset_name_survives_a_save_and_reload() {
        let mut stack = ItemStack::new(ItemKind::DiamondSword.id(), NonZeroU8::MIN);
        let plain = Component::from_section_codes("§rPlain");
        stack.set_custom_name(Some(&plain));
        let Some(Tag::Compound(name)) = stack.components.get(crate::item::CUSTOM_NAME) else {
            panic!("a styled name is a compound");
        };
        assert_eq!(name.get("italic"), Some(&Tag::Byte(0)));
        let mut bytes = Vec::new();
        rspigot_nbt::write_named(&mut bytes, "", &item_stack_to_nbt(&stack));
        let (_, read) = rspigot_nbt::read_named(&mut bytes.as_slice()).unwrap();
        let reloaded = item_stack_from_nbt(&read).unwrap();
        assert_eq!(reloaded.custom_name(), Some(plain));
        assert_eq!(
            reloaded
                .custom_name()
                .map(|name| name.to_legacy())
                .as_deref(),
            Some("§rPlain")
        );
    }

    #[test]
    fn item_and_falling_block_keys() {
        let item_nbt = Compound::new()
            .with("id", "minecraft:item")
            .with("Pos", doubles(Vec3::new(0.0, 70.0, 0.0)))
            .with(
                "Item",
                Compound::new()
                    .with("id", "minecraft:stone")
                    .with("count", 3),
            )
            .with("PickupDelay", 10i16);
        let entity = load_entity(&item_nbt, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(item::stack(&entity.base).map(|s| s.count.get()), Some(3));
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i64("PickupDelay"), Some(10));
        let falling_nbt = Compound::new()
            .with("id", "minecraft:falling_block")
            .with("Pos", doubles(Vec3::new(0.0, 70.0, 0.0)))
            .with(
                "BlockState",
                Compound::new()
                    .with("Name", "minecraft:anvil")
                    .with("Properties", Compound::new().with("facing", "east")),
            );
        let falling = load_entity(&falling_nbt, EntityId(2), Uuid::from_u128(2)).unwrap();
        let EntityState::Falling(state) = &falling.state else {
            panic!("not a falling block");
        };
        assert_eq!(state.state.property("facing"), Some("east"));
    }
}
