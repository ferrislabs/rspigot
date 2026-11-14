use rspigot_entities::EntityKind;
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use crate::entity::ageable::DEFAULT_BABY_CHANCE;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::living;
use crate::entity::living::{
    JUMP_STRENGTH, LivingAttributes, LivingState, MAX_HEALTH, MOVEMENT_SPEED,
};
use crate::entity::save::{item_stack_from_nbt, item_stack_to_nbt, read_uuid, uuid_tag};
use crate::entity::spawn::{Jockey, ageable_finalize};
use crate::item::Slot;
use crate::random::RandomSource;
use crate::spawn::{SpawnGroupData, SpawnReason};

pub const DATA_FLAGS: u8 = 18;
pub const DATA_VARIANT: u8 = 19;
pub const DATA_CHEST: u8 = 19;
pub const FLAG_TAME: u8 = 2;
pub const FLAG_BRED: u8 = 8;
pub const FLAG_EATING: u8 = 16;
pub const FLAG_STANDING: u8 = 32;
pub const FLAG_OPEN_MOUTH: u8 = 64;
pub const DEFAULT_MAX_DOMESTICATION: i32 = 100;
pub const INVENTORY_ROWS: usize = 3;
pub const CHEST_COLUMNS: usize = 5;
pub const STAND_TICKS: i32 = 20;
pub const BABY_SCALE: f32 = 0.7;
pub const TRAP_LIFETIME: i32 = 18_000;
pub const AMBIENT_STAND_INTERVAL: i32 = 400;
pub const SADDLE_SLOT: usize = 0;
pub const BODY_SLOT: usize = 1;
pub const BREEDING_CROSS_FACTOR: f64 = 0.15;
pub const KEYS: &[&str] = &[
    "EatingHaystack",
    "Bred",
    "Temper",
    "Tame",
    "Owner",
    "Bukkit.MaxDomestication",
    "Variant",
    "ChestedHorse",
    "Items",
    "SkeletonTrap",
    "SkeletonTrapTime",
];

const MIN_HEALTH: f32 = 15.0;
const MAX_HEALTH_ROLL: f32 = 30.0;
const MIN_JUMP: f32 = 0.4;
const MAX_JUMP: f32 = 1.0;
const MIN_SPEED: f32 = 0.1125;
const MAX_SPEED: f32 = 0.3375;
const SPAWN_BABY_CHANCE: f32 = 0.2;
const HORSE_VARIANTS: i32 = 7;
const HORSE_MARKINGS: i32 = 5;
const JUMP_BASE: f64 = 0.400_000_005_960_464_5;
const SPEED_BASE: f64 = 0.449_999_988_079_071_04;
const ZOMBIE_JUMP_STEP: f64 = 0.066_666_666_666_666_67;
const ZOMBIE_SPEED_DIVISOR: f64 = 42.159_999_847_412_11;

#[derive(Debug, Clone, PartialEq)]
pub struct MountState {
    pub temper: i32,
    pub max_domestication: i32,
    pub fixed_max_temper: Option<i32>,
    pub owner: Option<Uuid>,
    pub can_gallop: bool,
    pub inventory: Vec<Slot>,
    pub eating_counter: i32,
    pub mouth_counter: i32,
    pub stand_counter: i32,
    pub tail_counter: i32,
    pub sprint_counter: i32,
    pub gallop_sound_counter: i32,
    pub eat_anim: f32,
    pub eat_anim_o: f32,
    pub stand_anim: f32,
    pub stand_anim_o: f32,
    pub mouth_anim: f32,
    pub mouth_anim_o: f32,
    pub allow_stand_sliding: bool,
    pub player_jump_pending_scale: f32,
    pub trap: bool,
    pub trap_time: i32,
}

pub fn is_equine(kind: EntityKind) -> bool {
    kind.extends("AbstractHorse")
}

pub fn is_chested(kind: EntityKind) -> bool {
    kind.extends("AbstractChestedHorse")
}

pub fn has_horse_variant(kind: EntityKind) -> bool {
    kind == EntityKind::Horse
}

pub fn can_perform_rearing(kind: EntityKind) -> bool {
    is_equine(kind) && !super::llama::is_llama(kind) && !is_camel(kind)
}

pub fn is_camel(kind: EntityKind) -> bool {
    super::camel::is_camel(kind)
}

pub fn can_eat_grass(kind: EntityKind) -> bool {
    is_equine(kind) && !super::llama::is_llama(kind)
}

impl MountState {
    pub fn new(base: &mut EntityBase) -> Self {
        let kind = base.kind;
        base.data.define(DATA_FLAGS, SyncedValue::Byte(0));
        if has_horse_variant(kind) {
            base.data.define(DATA_VARIANT, SyncedValue::Int(0));
        }
        if is_chested(kind) {
            base.data.define(DATA_CHEST, SyncedValue::Boolean(false));
        }
        Self {
            temper: 0,
            max_domestication: DEFAULT_MAX_DOMESTICATION,
            fixed_max_temper: super::llama::is_llama(kind).then_some(super::llama::MAX_TEMPER),
            owner: None,
            can_gallop: !is_chested(kind),
            inventory: Vec::new(),
            eating_counter: 0,
            mouth_counter: 0,
            stand_counter: 0,
            tail_counter: 0,
            sprint_counter: 0,
            gallop_sound_counter: 0,
            eat_anim: 0.0,
            eat_anim_o: 0.0,
            stand_anim: 0.0,
            stand_anim_o: 0.0,
            mouth_anim: 0.0,
            mouth_anim_o: 0.0,
            allow_stand_sliding: false,
            player_jump_pending_scale: 0.0,
            trap: false,
            trap_time: 0,
        }
    }

    pub fn max_temper(&self) -> i32 {
        self.fixed_max_temper.unwrap_or(self.max_domestication)
    }

    pub fn modify_temper(&mut self, amount: i32) -> i32 {
        self.temper = (self.temper + amount).clamp(0, self.max_temper().max(0));
        self.temper
    }

    pub fn inventory_columns(&self, base: &EntityBase) -> usize {
        if super::llama::is_llama(base.kind) {
            return usize::try_from(super::llama::inventory_columns(base)).unwrap_or(0);
        }
        if is_chested(base.kind) && has_chest(base) {
            CHEST_COLUMNS
        } else {
            0
        }
    }

    pub fn inventory_size(&self, base: &EntityBase) -> usize {
        self.inventory_columns(base) * INVENTORY_ROWS
    }

    pub fn create_inventory(&mut self, base: &EntityBase) {
        let size = self.inventory_size(base);
        let old = std::mem::take(&mut self.inventory);
        let mut inventory: Vec<Slot> = vec![None; size];
        for (slot, stack) in old.into_iter().take(size).enumerate() {
            inventory[slot] = stack;
        }
        self.inventory = inventory;
    }

    pub fn mark_standing(&mut self, base: &mut EntityBase, ticks: i32) {
        set_eating(base, false);
        set_flag(base, FLAG_STANDING, true);
        self.stand_counter = ticks;
    }

    pub fn clear_standing(&mut self, base: &mut EntityBase) {
        set_flag(base, FLAG_STANDING, false);
        self.stand_counter = 0;
    }

    pub fn open_mouth(&mut self, base: &mut EntityBase) {
        self.mouth_counter = 1;
        set_flag(base, FLAG_OPEN_MOUTH, true);
    }

    pub fn move_tail(&mut self) {
        self.tail_counter = 1;
    }

    pub fn tick(&mut self, base: &mut EntityBase) {
        if self.mouth_counter > 0 {
            self.mouth_counter += 1;
            if self.mouth_counter > 30 {
                self.mouth_counter = 0;
                set_flag(base, FLAG_OPEN_MOUTH, false);
            }
        }
        if self.stand_counter > 0 {
            self.stand_counter -= 1;
            if self.stand_counter < 1 {
                self.clear_standing(base);
            }
        }
        if self.tail_counter > 0 {
            self.tail_counter += 1;
            if self.tail_counter > 8 {
                self.tail_counter = 0;
            }
        }
        if self.sprint_counter > 0 {
            self.sprint_counter += 1;
            if self.sprint_counter > 300 {
                self.sprint_counter = 0;
            }
        }
        self.eat_anim_o = self.eat_anim;
        if is_eating(base) {
            self.eat_anim += (1.0 - self.eat_anim) * 0.4 + 0.05;
            if self.eat_anim > 1.0 {
                self.eat_anim = 1.0;
            }
        } else {
            self.eat_anim += (0.0 - self.eat_anim) * 0.4 - 0.05;
            if self.eat_anim < 0.0 {
                self.eat_anim = 0.0;
            }
        }
        self.stand_anim_o = self.stand_anim;
        if is_standing(base) {
            self.eat_anim = 0.0;
            self.eat_anim_o = self.eat_anim;
            self.stand_anim += (1.0 - self.stand_anim) * 0.4 + 0.05;
            if self.stand_anim > 1.0 {
                self.stand_anim = 1.0;
            }
        } else {
            self.allow_stand_sliding = false;
            self.stand_anim +=
                (self.stand_anim * 0.8 * self.stand_anim * self.stand_anim - self.stand_anim) * 0.6
                    - 0.05;
            if self.stand_anim < 0.0 {
                self.stand_anim = 0.0;
            }
        }
        self.mouth_anim_o = self.mouth_anim;
        if flag(base, FLAG_OPEN_MOUTH) {
            self.mouth_anim += (1.0 - self.mouth_anim) * 0.7 + 0.05;
            if self.mouth_anim > 1.0 {
                self.mouth_anim = 1.0;
            }
        } else {
            self.mouth_anim += (0.0 - self.mouth_anim) * 0.7 - 0.05;
            if self.mouth_anim < 0.0 {
                self.mouth_anim = 0.0;
            }
        }
    }

    pub fn riders_jump_scale(jump_amount: i32) -> f32 {
        if jump_amount >= 90 {
            1.0
        } else {
            0.4 + 0.4 * jump_amount as f32 / 90.0
        }
    }

    pub fn start_jump_scale(jump_amount: i32) -> f32 {
        if jump_amount < 90 {
            (jump_amount as f32 * 0.4) / 90.0 + 0.4
        } else {
            1.0
        }
    }

    pub fn passenger_offset(&self, scale: f64, yaw: f32) -> crate::Vec3 {
        let raw = crate::Vec3::new(
            0.0,
            f64::from(self.stand_anim_o) * 0.15 * scale,
            f64::from(self.stand_anim_o) * -0.7 * scale,
        );
        let angle = f64::from(-yaw * (std::f32::consts::PI / 180.0));
        let (sin, cos) = (angle.sin(), angle.cos());
        crate::Vec3::new(raw.x * cos + raw.z * sin, raw.y, raw.z * cos - raw.x * sin)
    }
}

pub fn flag(base: &EntityBase, mask: u8) -> bool {
    base.data.flag(DATA_FLAGS, mask)
}

pub fn set_flag(base: &mut EntityBase, mask: u8, value: bool) {
    base.data.set_flag(DATA_FLAGS, mask, value);
}

pub fn is_tamed(base: &EntityBase) -> bool {
    is_camel(base.kind) || flag(base, FLAG_TAME)
}

pub fn set_tamed(base: &mut EntityBase, value: bool) {
    set_flag(base, FLAG_TAME, value);
}

pub fn is_bred(base: &EntityBase) -> bool {
    flag(base, FLAG_BRED)
}

pub fn set_bred(base: &mut EntityBase, value: bool) {
    set_flag(base, FLAG_BRED, value);
}

pub fn is_eating(base: &EntityBase) -> bool {
    flag(base, FLAG_EATING)
}

pub fn set_eating(base: &mut EntityBase, value: bool) {
    set_flag(base, FLAG_EATING, value);
}

pub fn is_standing(base: &EntityBase) -> bool {
    flag(base, FLAG_STANDING)
}

pub fn has_chest(base: &EntityBase) -> bool {
    is_chested(base.kind) && base.data.boolean(DATA_CHEST)
}

pub fn set_chest(base: &mut EntityBase, value: bool) {
    base.data.set(DATA_CHEST, SyncedValue::Boolean(value));
}

pub fn type_variant(base: &EntityBase) -> i32 {
    base.data.int(DATA_VARIANT)
}

pub fn set_type_variant(base: &mut EntityBase, value: i32) {
    base.data.set(DATA_VARIANT, SyncedValue::Int(value));
}

pub fn variant_id(base: &EntityBase) -> i32 {
    wrap(type_variant(base) & 0xff, HORSE_VARIANTS)
}

pub fn markings_id(base: &EntityBase) -> i32 {
    wrap((type_variant(base) & 0xff00) >> 8, HORSE_MARKINGS)
}

pub fn set_variant_and_markings(base: &mut EntityBase, variant: i32, markings: i32) {
    set_type_variant(base, variant & 0xff | (markings & 0xff) << 8);
}

pub fn set_variant(base: &mut EntityBase, variant: i32) {
    let current = type_variant(base);
    set_type_variant(base, variant & 0xff | current & !0xff);
}

fn wrap(id: i32, count: i32) -> i32 {
    id.rem_euclid(count)
}

pub const VARIANT_NAMES: [&str; 7] = [
    "white",
    "creamy",
    "chestnut",
    "brown",
    "black",
    "gray",
    "dark_brown",
];

pub const MARKING_NAMES: [&str; 5] = ["none", "white", "white_field", "white_dots", "black_dots"];

pub fn random_variant(random: &mut impl RandomSource) -> i32 {
    random.next_int(HORSE_VARIANTS)
}

pub fn random_markings(random: &mut impl RandomSource) -> i32 {
    random.next_int(HORSE_MARKINGS)
}

pub fn offspring_variant(own: i32, other: i32, random: &mut impl RandomSource) -> i32 {
    let roll = random.next_int(9);
    if roll < 4 {
        own
    } else if roll < 8 {
        other
    } else {
        random_variant(random)
    }
}

pub fn offspring_markings(own: i32, other: i32, random: &mut impl RandomSource) -> i32 {
    let roll = random.next_int(5);
    if roll < 2 {
        own
    } else if roll < 4 {
        other
    } else {
        random_markings(random)
    }
}

pub fn generate_max_health(mut provider: impl FnMut(i32) -> i32) -> f32 {
    let first = provider(8);
    let second = provider(9);
    first as f32 + 15.0 + second as f32
}

pub fn generate_jump_strength(mut provider: impl FnMut() -> f64) -> f64 {
    let first = provider();
    let second = provider();
    let third = provider();
    first * 0.2 + JUMP_BASE + second * 0.2 + third * 0.2
}

pub fn generate_speed(mut provider: impl FnMut() -> f64) -> f64 {
    let first = provider();
    let second = provider();
    let third = provider();
    (first * 0.3 + SPEED_BASE + second * 0.3 + third * 0.3) * 0.25
}

fn generate_zombie_jump_strength(mut provider: impl FnMut() -> f64) -> f64 {
    let first = provider();
    let second = provider();
    let third = provider();
    first * ZOMBIE_JUMP_STEP + 0.5 + second * ZOMBIE_JUMP_STEP + third * ZOMBIE_JUMP_STEP
}

fn generate_zombie_speed(mut provider: impl FnMut() -> f64) -> f64 {
    let first = provider();
    let second = provider();
    let third = provider();
    (first * 1.0 + 9.0 + second * 1.0 + third * 1.0) / ZOMBIE_SPEED_DIVISOR
}

pub fn randomize_attributes(
    kind: EntityKind,
    attributes: &mut LivingAttributes,
    random: &mut impl RandomSource,
) {
    match kind {
        EntityKind::Horse => {
            let health = generate_max_health(|bound| random.next_int(bound));
            attributes.set_base(MAX_HEALTH, f64::from(health));
            let speed = generate_speed(|| random.next_double());
            attributes.set_base(MOVEMENT_SPEED, speed);
            let jump = generate_jump_strength(|| random.next_double());
            attributes.set_base(JUMP_STRENGTH, jump);
        }
        EntityKind::SkeletonHorse => {
            let jump = generate_jump_strength(|| random.next_double());
            attributes.set_base(JUMP_STRENGTH, jump);
        }
        EntityKind::ZombieHorse => {
            let jump = generate_zombie_jump_strength(|| random.next_double());
            attributes.set_base(JUMP_STRENGTH, jump);
            let speed = generate_zombie_speed(|| random.next_double());
            attributes.set_base(MOVEMENT_SPEED, speed);
        }
        kind if is_chested(kind) => {
            let health = generate_max_health(|bound| random.next_int(bound));
            attributes.set_base(MAX_HEALTH, f64::from(health));
        }
        _ => {}
    }
}

pub fn clamp_health(base: &mut EntityBase, living: &mut LivingState) {
    let max = living.max_health() as f32;
    if living::health(base) > max {
        living::set_health(base, living, max);
    }
}

pub fn create_offspring_attribute(
    parent_a: f64,
    parent_b: f64,
    min: f64,
    max: f64,
    random: &mut impl RandomSource,
) -> f64 {
    let a = parent_a.clamp(min, max);
    let b = parent_b.clamp(min, max);
    let spread = (max - min) * BREEDING_CROSS_FACTOR;
    let range = (a - b).abs() + spread * 2.0;
    let average = (a + b) / 2.0;
    let first = random.next_double();
    let second = random.next_double();
    let third = random.next_double();
    let value = average + range * ((first + second + third) / 3.0 - 0.5);
    if value > max {
        max - (value - max)
    } else if value < min {
        min + (min - value)
    } else {
        value
    }
}

pub fn set_offspring_attributes(
    own: &mut LivingAttributes,
    partner: &mut LivingAttributes,
    baby: &mut LivingAttributes,
    random: &mut impl RandomSource,
) {
    for (name, min, max) in [
        (
            MAX_HEALTH,
            f64::from(MIN_HEALTH),
            f64::from(MAX_HEALTH_ROLL),
        ),
        (JUMP_STRENGTH, f64::from(MIN_JUMP), f64::from(MAX_JUMP)),
        (MOVEMENT_SPEED, f64::from(MIN_SPEED), f64::from(MAX_SPEED)),
    ] {
        let a = base_of(own, name);
        let b = base_of(partner, name);
        let value = create_offspring_attribute(a, b, min, max, random);
        baby.set_base(name, value);
    }
}

fn base_of(attributes: &mut LivingAttributes, name: &str) -> f64 {
    attributes
        .instance(name)
        .map_or(0.0, |instance| instance.base)
}

pub fn offspring_kind(parent: EntityKind, partner: EntityKind) -> Option<EntityKind> {
    match (parent, partner) {
        (EntityKind::Horse, EntityKind::Donkey) | (EntityKind::Donkey, EntityKind::Horse) => {
            Some(EntityKind::Mule)
        }
        (EntityKind::Horse, EntityKind::Horse) => Some(EntityKind::Horse),
        (EntityKind::Donkey, EntityKind::Donkey) => Some(EntityKind::Donkey),
        (EntityKind::Mule, _) => Some(EntityKind::Mule),
        (EntityKind::SkeletonHorse, _) => Some(EntityKind::SkeletonHorse),
        (EntityKind::ZombieHorse, _) => Some(EntityKind::ZombieHorse),
        (EntityKind::Camel, EntityKind::Camel) => Some(EntityKind::Camel),
        _ => None,
    }
}

pub fn can_mate_kind(kind: EntityKind, partner: EntityKind) -> bool {
    matches!(kind, EntityKind::Horse | EntityKind::Donkey)
        && matches!(partner, EntityKind::Horse | EntityKind::Donkey)
}

pub fn can_fall_in_love(kind: EntityKind) -> bool {
    kind != EntityKind::ZombieHorse && super::camel::can_fall_in_love(kind)
}

pub fn can_age_up(kind: EntityKind) -> bool {
    !matches!(kind, EntityKind::SkeletonHorse | EntityKind::ZombieHorse)
        && super::camel::can_be_baby(kind)
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut MountState) {
    set_eating(base, nbt.get_bool("EatingHaystack").unwrap_or(false));
    set_bred(base, nbt.get_bool("Bred").unwrap_or(false));
    state.temper = nbt.get_i32("Temper").unwrap_or(0);
    set_tamed(base, nbt.get_bool("Tame").unwrap_or(false));
    state.owner = nbt.get("Owner").and_then(read_owner);
    state.max_domestication = nbt
        .get_i32("Bukkit.MaxDomestication")
        .unwrap_or(state.max_domestication);
    if has_horse_variant(base.kind) {
        set_type_variant(base, nbt.get_i32("Variant").unwrap_or(0));
    }
    if super::llama::is_llama(base.kind) {
        super::llama::read_strength(nbt, base);
    }
    if is_chested(base.kind) {
        set_chest(base, nbt.get_bool("ChestedHorse").unwrap_or(false));
        state.create_inventory(base);
        if has_chest(base) {
            let size = state.inventory.len();
            for entry in nbt.get_list("Items").unwrap_or_default() {
                let Some(entry) = entry.as_compound() else {
                    continue;
                };
                let slot = entry
                    .get_i64("Slot")
                    .and_then(|slot| usize::try_from(slot).ok())
                    .filter(|slot| *slot < size);
                if let (Some(slot), Some(stack)) = (slot, item_stack_from_nbt(entry)) {
                    state.inventory[slot] = Some(stack);
                }
            }
        }
    }
    if base.kind == EntityKind::SkeletonHorse {
        state.trap = nbt.get_bool("SkeletonTrap").unwrap_or(false);
        state.trap_time = nbt.get_i32("SkeletonTrapTime").unwrap_or(0);
    }
}

fn read_owner(tag: &Tag) -> Option<Uuid> {
    match tag {
        Tag::String(text) => Uuid::parse_str(text).ok(),
        other => read_uuid(other),
    }
}

pub fn write(base: &EntityBase, state: &MountState, out: &mut Compound) {
    out.insert("EatingHaystack", is_eating(base));
    out.insert("Bred", is_bred(base));
    out.insert("Temper", state.temper);
    out.insert("Tame", is_tamed(base));
    if let Some(owner) = state.owner {
        out.insert("Owner", uuid_tag(owner));
    }
    out.insert("Bukkit.MaxDomestication", state.max_domestication);
    if has_horse_variant(base.kind) {
        out.insert("Variant", type_variant(base));
    }
    if is_chested(base.kind) {
        out.insert("ChestedHorse", has_chest(base));
        if has_chest(base) {
            let list = state
                .inventory
                .iter()
                .enumerate()
                .filter_map(|(slot, stack)| {
                    let mut entry = item_stack_to_nbt(stack.as_ref()?);
                    entry.insert("Slot", slot as i8);
                    Some(Tag::Compound(entry))
                })
                .collect();
            out.insert("Items", Tag::List(list));
        }
    }
    if base.kind == EntityKind::SkeletonHorse {
        out.insert("SkeletonTrap", state.trap);
        out.insert("SkeletonTrapTime", state.trap_time);
    }
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut LivingState,
    reason: SpawnReason,
    group: Option<SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<SpawnGroupData> {
    if base.kind == EntityKind::ZombieHorse
        && reason == SpawnReason::Natural
        && let Some(mob) = living.mob.as_mut()
    {
        mob.jockey = Some(Jockey::ZombieHorseRider);
    }
    let mut variant_group = None;
    let mut inherited = group;
    if base.kind == EntityKind::Horse {
        let (variant, size) = match group {
            Some(SpawnGroupData::Horse {
                variant,
                group_size,
            }) => (variant, group_size),
            _ => (random_variant(level), 0),
        };
        let markings = random_markings(level);
        set_variant_and_markings(base, variant, markings);
        variant_group = Some(variant);
        inherited = Some(SpawnGroupData::Ageable {
            group_size: size,
            should_spawn_baby: true,
            baby_chance: DEFAULT_BABY_CHANCE,
        });
    } else if let Some(SpawnGroupData::Ageable { .. }) = group {
    } else {
        inherited = Some(SpawnGroupData::Ageable {
            group_size: 0,
            should_spawn_baby: true,
            baby_chance: SPAWN_BABY_CHANCE,
        });
    }
    randomize_attributes(base.kind, &mut living.attributes, level);
    clamp_health(base, living);
    let result = ageable_finalize(base, living, inherited, level);
    match (variant_group, result) {
        (Some(variant), Some(SpawnGroupData::Ageable { group_size, .. })) => {
            Some(SpawnGroupData::Horse {
                variant,
                group_size,
            })
        }
        (_, result) => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EntityId;
    use crate::entity::living::LivingState;
    use crate::random::LegacyRandom;

    fn base(kind: EntityKind) -> EntityBase {
        EntityBase::new(EntityId(1), Uuid::from_u128(1), kind)
    }

    #[test]
    fn horse_rolls_follow_the_vanilla_generators() {
        let mut max = LegacyRandom::new(5);
        let health = generate_max_health(|bound| max.next_int(bound));
        assert!((15.0..=30.0).contains(&health));
        assert_eq!(generate_jump_strength(|| 0.0), JUMP_BASE);
        assert!((generate_jump_strength(|| 1.0) - 1.0).abs() < 1e-6);
        assert!((generate_speed(|| 0.0) - 0.1125).abs() < 1e-6);
        assert!((generate_speed(|| 1.0) - 0.3375).abs() < 1e-6);
        assert!((generate_zombie_speed(|| 0.0) - 9.0 / ZOMBIE_SPEED_DIVISOR).abs() < 1e-9);
        assert!((generate_zombie_jump_strength(|| 1.0) - 0.7).abs() < 1e-9);
    }

    #[test]
    fn the_variant_and_markings_share_the_variant_int() {
        let mut base = base(EntityKind::Horse);
        MountState::new(&mut base);
        set_variant_and_markings(&mut base, 3, 4);
        assert_eq!(type_variant(&base), 3 | 4 << 8);
        assert_eq!(variant_id(&base), 3);
        assert_eq!(markings_id(&base), 4);
        set_variant(&mut base, 6);
        assert_eq!(markings_id(&base), 4);
        assert_eq!(variant_id(&base), 6);
    }

    #[test]
    fn offspring_attributes_stay_inside_the_range_and_reflect_at_the_edges() {
        let mut random = LegacyRandom::new(9);
        for _ in 0..200 {
            let value = create_offspring_attribute(15.0, 30.0, 15.0, 30.0, &mut random);
            assert!((15.0..=30.0).contains(&value));
        }
        struct Fixed(f64);
        impl RandomSource for Fixed {
            fn next_float(&mut self) -> f32 {
                0.0
            }
            fn next_int(&mut self, _: i32) -> i32 {
                0
            }
            fn next_double(&mut self) -> f64 {
                self.0
            }
            fn next_boolean(&mut self) -> bool {
                false
            }
            fn next_gaussian(&mut self) -> f64 {
                0.0
            }
        }
        let high = create_offspring_attribute(30.0, 30.0, 15.0, 30.0, &mut Fixed(1.0));
        assert!((high - (30.0 - (30.0 + 4.5 * 0.5 - 30.0))).abs() < 1e-9);
        let low = create_offspring_attribute(15.0, 15.0, 15.0, 30.0, &mut Fixed(0.0));
        assert!((low - (15.0 + (15.0 - (15.0 - 4.5 * 0.5)))).abs() < 1e-9);
    }

    #[test]
    fn the_flags_share_one_byte_and_the_temper_is_clamped() {
        let mut base = base(EntityKind::Donkey);
        let mut state = MountState::new(&mut base);
        set_tamed(&mut base, true);
        set_bred(&mut base, true);
        set_eating(&mut base, true);
        state.mark_standing(&mut base, STAND_TICKS);
        assert_eq!(base.data.byte(DATA_FLAGS), 2 | 8 | 32);
        assert_eq!(state.modify_temper(500), 100);
        assert_eq!(state.modify_temper(-500), 0);
        state.clear_standing(&mut base);
        assert!(!is_standing(&base));
    }

    #[test]
    fn a_chest_makes_a_fifteen_slot_inventory_that_keeps_its_items() {
        let mut base = base(EntityKind::Mule);
        let mut state = MountState::new(&mut base);
        assert_eq!(state.inventory_size(&base), 0);
        set_chest(&mut base, true);
        state.create_inventory(&base);
        assert_eq!(state.inventory.len(), 15);
        state.inventory[4] = Some(crate::item::ItemStack::new(
            rspigot_items::ItemKind::Apple.id(),
            std::num::NonZeroU8::MIN,
        ));
        state.create_inventory(&base);
        assert!(state.inventory[4].is_some());
    }

    #[test]
    fn the_state_round_trips_in_the_vanilla_layout() {
        let mut base = base(EntityKind::Donkey);
        let mut state = MountState::new(&mut base);
        set_tamed(&mut base, true);
        set_chest(&mut base, true);
        state.temper = 42;
        state.owner = Some(Uuid::from_u128(77));
        state.create_inventory(&base);
        state.inventory[3] = Some(crate::item::ItemStack::new(
            rspigot_items::ItemKind::Apple.id(),
            std::num::NonZeroU8::new(5).unwrap(),
        ));
        let mut out = Compound::new();
        write(&base, &state, &mut out);
        assert_eq!(out.get_i32("Temper"), Some(42));
        assert_eq!(out.get_bool("ChestedHorse"), Some(true));
        let mut again_base = self::base(EntityKind::Donkey);
        let mut again = MountState::new(&mut again_base);
        read(&out, &mut again_base, &mut again);
        assert!(is_tamed(&again_base));
        assert!(has_chest(&again_base));
        assert_eq!(again.temper, 42);
        assert_eq!(again.owner, Some(Uuid::from_u128(77)));
        assert_eq!(again.inventory, state.inventory);
    }

    #[test]
    fn the_skeleton_trap_persists_with_its_time() {
        let mut base = base(EntityKind::SkeletonHorse);
        let mut state = MountState::new(&mut base);
        state.trap = true;
        state.trap_time = 120;
        let mut out = Compound::new();
        write(&base, &state, &mut out);
        let mut again_base = self::base(EntityKind::SkeletonHorse);
        let mut again = MountState::new(&mut again_base);
        read(&out, &mut again_base, &mut again);
        assert!(again.trap);
        assert_eq!(again.trap_time, 120);
    }

    #[test]
    fn breeding_picks_the_vanilla_offspring_kind() {
        assert_eq!(
            offspring_kind(EntityKind::Horse, EntityKind::Donkey),
            Some(EntityKind::Mule)
        );
        assert_eq!(
            offspring_kind(EntityKind::Donkey, EntityKind::Horse),
            Some(EntityKind::Mule)
        );
        assert_eq!(
            offspring_kind(EntityKind::Donkey, EntityKind::Donkey),
            Some(EntityKind::Donkey)
        );
        assert!(can_mate_kind(EntityKind::Horse, EntityKind::Donkey));
        assert!(!can_mate_kind(EntityKind::Mule, EntityKind::Horse));
        assert!(!can_fall_in_love(EntityKind::ZombieHorse));
        assert!(!can_age_up(EntityKind::SkeletonHorse));
    }

    #[test]
    fn the_standing_animation_rises_while_standing_and_decays_after() {
        let mut base = base(EntityKind::Horse);
        let mut state = MountState::new(&mut base);
        state.mark_standing(&mut base, STAND_TICKS);
        for _ in 0..4 {
            state.tick(&mut base);
        }
        assert!(state.stand_anim > 0.5);
        for _ in 0..30 {
            state.tick(&mut base);
        }
        assert_eq!(state.stand_anim, 0.0);
        assert!(!is_standing(&base));
    }

    #[test]
    fn a_new_mob_is_created_with_a_mount_state_for_every_equine() {
        for kind in [
            EntityKind::Horse,
            EntityKind::Donkey,
            EntityKind::Mule,
            EntityKind::SkeletonHorse,
            EntityKind::ZombieHorse,
        ] {
            let mut base = base(kind);
            let living = LivingState::new(&mut base, 0.0);
            assert!(living.mob.as_ref().is_some_and(|mob| mob.mount.is_some()));
        }
    }

    #[test]
    fn a_horse_group_shares_its_variant_and_every_horse_rolls_its_markings_and_stats() {
        let mut first_base = base(EntityKind::Horse);
        let mut first = LivingState::new(&mut first_base, 0.0);
        let mut level = LegacyRandom::new(11);
        let group = finalize(
            &mut first_base,
            &mut first,
            SpawnReason::Natural,
            None,
            &mut level,
        );
        let Some(SpawnGroupData::Horse {
            variant,
            group_size,
        }) = group
        else {
            panic!("a horse group is expected");
        };
        assert_eq!(group_size, 1);
        assert_eq!(variant_id(&first_base), variant);
        let health = first.attribute(MAX_HEALTH);
        assert!((15.0..=30.0).contains(&health));
        let mut second_base = self::base(EntityKind::Horse);
        let mut second = LivingState::new(&mut second_base, 0.0);
        let again = finalize(
            &mut second_base,
            &mut second,
            SpawnReason::Natural,
            group,
            &mut level,
        );
        assert_eq!(variant_id(&second_base), variant);
        assert!(matches!(
            again,
            Some(SpawnGroupData::Horse { group_size: 2, .. })
        ));
    }

    #[test]
    fn a_donkey_only_rolls_its_health_and_a_zombie_horse_its_jump_and_speed() {
        let mut donkey_base = base(EntityKind::Donkey);
        let mut donkey = LivingState::new(&mut donkey_base, 0.0);
        let speed = donkey.attribute(MOVEMENT_SPEED);
        let mut level = LegacyRandom::new(3);
        finalize(
            &mut donkey_base,
            &mut donkey,
            SpawnReason::Natural,
            None,
            &mut level,
        );
        assert_eq!(donkey.attribute(MOVEMENT_SPEED), speed);
        assert!((15.0..=30.0).contains(&donkey.attribute(MAX_HEALTH)));
        let mut zombie_base = self::base(EntityKind::ZombieHorse);
        let mut zombie = LivingState::new(&mut zombie_base, 0.0);
        finalize(
            &mut zombie_base,
            &mut zombie,
            SpawnReason::Natural,
            None,
            &mut level,
        );
        assert!((0.5..=0.7).contains(&zombie.attribute(JUMP_STRENGTH)));
        assert!((0.2135..=0.2371).contains(&zombie.attribute(MOVEMENT_SPEED)));
    }

    #[test]
    fn only_a_naturally_spawned_zombie_horse_gets_a_zombie_rider() {
        for (reason, rides) in [
            (SpawnReason::Natural, true),
            (SpawnReason::Spawner, false),
            (SpawnReason::Command, false),
        ] {
            let mut base = base(EntityKind::ZombieHorse);
            let mut living = LivingState::new(&mut base, 0.0);
            let mut level = LegacyRandom::new(2);
            finalize(&mut base, &mut living, reason, None, &mut level);
            let jockey = living.mob.as_ref().and_then(|mob| mob.jockey);
            assert_eq!(
                jockey == Some(Jockey::ZombieHorseRider),
                rides,
                "{reason:?}"
            );
        }
    }
}
