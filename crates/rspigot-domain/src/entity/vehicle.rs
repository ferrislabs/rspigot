use rspigot_nbt::{Compound, Tag};

use super::base::EntityBase;
use super::data::SyncedValue;
use super::physics::{BlockView, MoveTraits, move_entity};
use super::save::{item_stack_from_nbt, item_stack_to_nbt};
use crate::item::ItemStack;
use crate::{BlockPos, Vec3};

pub const DATA_HURT: u8 = 8;
pub const DATA_HURT_DIR: u8 = 9;
pub const DATA_DAMAGE: u8 = 10;
pub const HURT_TICKS: i32 = 10;
pub const BREAK_DAMAGE: f32 = 40.0;
pub const DAMAGE_PER_POINT: f32 = 10.0;
pub const CARGO_SIZE: usize = 27;
pub const CARGO_KEYS: &[&str] = &["Items", "LootTable", "LootTableSeed"];

pub fn define_hurt(base: &mut EntityBase) {
    base.data.define(DATA_HURT, SyncedValue::Int(0));
    base.data.define(DATA_HURT_DIR, SyncedValue::Int(1));
    base.data.define(DATA_DAMAGE, SyncedValue::Float(0.0));
}

pub fn hurt_time(base: &EntityBase) -> i32 {
    base.data.int(DATA_HURT)
}

pub fn hurt_dir(base: &EntityBase) -> i32 {
    base.data.int(DATA_HURT_DIR)
}

pub fn damage(base: &EntityBase) -> f32 {
    base.data.float(DATA_DAMAGE)
}

pub fn set_damage(base: &mut EntityBase, damage: f32) {
    base.data.set(DATA_DAMAGE, SyncedValue::Float(damage));
}

pub fn wobble(base: &mut EntityBase, amount: f32) {
    let direction = hurt_dir(base);
    base.data.set(DATA_HURT_DIR, SyncedValue::Int(-direction));
    base.data.set(DATA_HURT, SyncedValue::Int(HURT_TICKS));
    base.needs_sync = true;
    let total = damage(base) + amount * DAMAGE_PER_POINT;
    set_damage(base, total);
}

pub fn tick_hurt(base: &mut EntityBase) {
    let time = hurt_time(base);
    if time > 0 {
        base.data.set(DATA_HURT, SyncedValue::Int(time - 1));
    }
    let damage = damage(base);
    if damage > 0.0 {
        set_damage(base, damage - 1.0);
    }
}

pub fn blocked_block(
    base: &EntityBase,
    requested: Vec3,
    moved: Vec3,
    view: &impl BlockView,
) -> Option<BlockPos> {
    if !base.horizontal_collision {
        return None;
    }
    let here = base.position.block();
    let target = if requested.x > moved.x {
        here.offset(1, 0, 0)
    } else if requested.x < moved.x {
        here.offset(-1, 0, 0)
    } else if requested.z > moved.z {
        here.offset(0, 0, 1)
    } else if requested.z < moved.z {
        here.offset(0, 0, -1)
    } else {
        here
    };
    let state = view.block_or_air(target);
    (!state.is_air()).then_some(target)
}

pub fn travel(
    base: &mut EntityBase,
    view: &impl BlockView,
    movement: Vec3,
    gravity: f64,
) -> Option<BlockPos> {
    let before = base.position;
    move_entity(base, view, movement, &MoveTraits::plain(gravity));
    blocked_block(base, movement, base.position - before, view)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cargo {
    pub items: Vec<Option<ItemStack>>,
    pub loot_table: Option<String>,
    pub loot_seed: i64,
}

impl Cargo {
    pub fn new(size: usize) -> Self {
        Self {
            items: vec![None; size],
            loot_table: None,
            loot_seed: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.iter().all(Option::is_none)
    }

    pub fn read(&mut self, nbt: &Compound) {
        let size = self.items.len();
        if let Some(table) = nbt.get_str("LootTable") {
            self.loot_table = Some(table.to_owned());
            self.loot_seed = nbt.get_i64("LootTableSeed").unwrap_or(0);
            return;
        }
        for entry in nbt.get_list("Items").unwrap_or_default() {
            let Some(entry) = entry.as_compound() else {
                continue;
            };
            let slot = entry
                .get_i64("Slot")
                .and_then(|slot| usize::try_from(slot).ok())
                .filter(|slot| *slot < size);
            if let (Some(slot), Some(stack)) = (slot, item_stack_from_nbt(entry)) {
                self.items[slot] = Some(stack);
            }
        }
    }

    pub fn write(&self, out: &mut Compound) {
        if let Some(table) = &self.loot_table {
            out.insert("LootTable", table.as_str());
            if self.loot_seed != 0 {
                out.insert("LootTableSeed", self.loot_seed);
            }
            return;
        }
        let list = self
            .items
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

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use rspigot_entities::EntityKind;
    use rspigot_items::ItemKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;

    #[test]
    fn a_hit_flips_the_wobble_and_adds_ten_times_the_damage() {
        let mut base = EntityBase::new(EntityId(1), Uuid::nil(), EntityKind::OakBoat);
        define_hurt(&mut base);
        wobble(&mut base, 1.5);
        assert_eq!(hurt_dir(&base), -1);
        assert_eq!(hurt_time(&base), HURT_TICKS);
        assert_eq!(damage(&base), 15.0);
    }

    #[test]
    fn the_wobble_and_the_damage_fade_each_tick() {
        let mut base = EntityBase::new(EntityId(1), Uuid::nil(), EntityKind::OakBoat);
        define_hurt(&mut base);
        wobble(&mut base, 1.0);
        tick_hurt(&mut base);
        assert_eq!(hurt_time(&base), HURT_TICKS - 1);
        assert_eq!(damage(&base), 9.0);
    }

    #[test]
    fn cargo_round_trips_through_nbt() {
        let mut cargo = Cargo::new(CARGO_SIZE);
        cargo.items[3] = Some(ItemStack::new(
            ItemKind::Stick.id(),
            NonZeroU8::new(5).unwrap(),
        ));
        let mut nbt = Compound::new();
        cargo.write(&mut nbt);
        let mut loaded = Cargo::new(CARGO_SIZE);
        loaded.read(&nbt);
        assert_eq!(loaded, cargo);
    }

    #[test]
    fn a_loot_table_replaces_the_stored_items() {
        let mut cargo = Cargo::new(CARGO_SIZE);
        cargo.loot_table = Some("minecraft:chests/simple_dungeon".to_owned());
        cargo.loot_seed = 9;
        let mut nbt = Compound::new();
        cargo.write(&mut nbt);
        assert!(nbt.get_list("Items").is_none());
        let mut loaded = Cargo::new(CARGO_SIZE);
        loaded.read(&nbt);
        assert_eq!(loaded.loot_table, cargo.loot_table);
        assert_eq!(loaded.loot_seed, 9);
    }
}
