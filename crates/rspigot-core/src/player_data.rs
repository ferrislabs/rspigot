use rspigot_domain::Identifier;
use rspigot_domain::effect::EffectInstance;
use rspigot_domain::entity::save::{
    effect_from_nbt, effect_to_nbt, item_stack_from_nbt, item_stack_to_nbt,
};
use rspigot_domain::experience::PlayerExperience;
use rspigot_domain::food::{FoodData, MAX_FOOD};
use rspigot_domain::item::{Inventory, Slot, menu_slot};
use rspigot_domain::player::GameMode;
use rspigot_domain::{Rotation, Vec3};
use rspigot_nbt::{Compound, Tag};

use crate::containers::{read_items_at, write_items_at};
use crate::player::Player;

const STORAGE_SLOTS: i64 = 36;
const EQUIPMENT: [(usize, &str); 5] = [
    (36, "feet"),
    (37, "legs"),
    (38, "chest"),
    (39, "head"),
    (40, "offhand"),
];
const MAX_COORDINATE: f64 = 3.0e7;
const MAX_HEALTH: f32 = 20.0;
const ENDER_SLOTS: usize = 27;
const ENDER_ITEMS: &str = "EnderItems";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PlayerDataError {
    #[error("no position")]
    MissingPosition,
    #[error("position is not a finite coordinate inside the world")]
    InvalidPosition,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSave {
    pub dimension: Identifier,
    pub position: Vec3,
    pub rotation: Rotation,
    pub game_mode: Option<GameMode>,
    pub previous_game_mode: Option<GameMode>,
    pub inventory: Inventory,
    pub health: f32,
    pub absorption: f32,
    pub food: FoodData,
    pub experience: PlayerExperience,
    pub effects: Vec<EffectInstance>,
    pub fire_ticks: i32,
    pub fall_distance: f64,
    pub on_ground: bool,
    pub flying: bool,
    pub ender_items: Vec<Slot>,
    pub seen_credits: bool,
    pub warden_tracker: rspigot_domain::warden::WardenSpawnTracker,
}

impl PlayerSave {
    pub fn capture<L>(player: &Player<L>) -> Self {
        let vitals = &player.vitals;
        Self {
            dimension: player.dim.clone(),
            position: player.position,
            rotation: player.rotation,
            game_mode: Some(player.game_mode),
            previous_game_mode: player.previous_game_mode,
            inventory: player.inventory.clone(),
            health: if vitals.dead { 0.0 } else { vitals.health },
            absorption: vitals.absorption,
            food: vitals.food,
            experience: vitals.experience,
            effects: player.effects.iter().copied().collect(),
            fire_ticks: vitals.fire_ticks,
            fall_distance: vitals.fall_distance,
            on_ground: player.on_ground,
            flying: player.flying,
            ender_items: player.ender_items.clone(),
            seen_credits: player.seen_credits,
            warden_tracker: vitals.warden_tracker,
        }
    }

    pub fn is_dead(&self) -> bool {
        self.health <= 0.0
    }

    pub fn restore<L>(&self, player: &mut Player<L>, spawn: Vec3, spawn_angle: f32) {
        if let Some(mode) = self.game_mode {
            player.game_mode = mode;
        }
        player.previous_game_mode = self.previous_game_mode;
        player.seen_credits = self.seen_credits;
        player.inventory = self.inventory.clone();
        player.ender_items = self.ender_items.clone();
        player.ender_items.resize(ENDER_SLOTS, None);
        player.vitals.experience = self.experience;
        player.vitals.warden_tracker = self.warden_tracker;
        if self.is_dead() {
            player.position = spawn;
            player.rotation = Rotation::new(spawn_angle, 0.0);
            return;
        }
        player.position = self.position;
        player.rotation = self.rotation;
        player.on_ground = self.on_ground;
        player.flying = self.flying && player.abilities().may_fly;
        player.vitals.health = self.health;
        player.vitals.absorption = self.absorption;
        player.vitals.food = self.food;
        player.vitals.fire_ticks = self.fire_ticks;
        player.vitals.fall_distance = self.fall_distance;
    }

    pub fn to_nbt(&self) -> Compound {
        let mut out = Compound::new();
        out.insert(
            "Pos",
            Tag::List(vec![
                Tag::Double(self.position.x),
                Tag::Double(self.position.y),
                Tag::Double(self.position.z),
            ]),
        );
        out.insert(
            "Rotation",
            Tag::List(vec![
                Tag::Float(self.rotation.yaw),
                Tag::Float(self.rotation.pitch),
            ]),
        );
        out.insert("Dimension", self.dimension.as_str());
        if let Some(mode) = self.game_mode {
            out.insert("playerGameType", i32::from(mode.id()));
        }
        if let Some(mode) = self.previous_game_mode {
            out.insert("previousPlayerGameType", i32::from(mode.id()));
        }
        out.insert("seenCredits", self.seen_credits);
        out.insert("Health", self.health);
        out.insert("AbsorptionAmount", self.absorption);
        out.insert(
            rspigot_domain::warden::TRACKER_KEY,
            self.warden_tracker.to_nbt(),
        );
        out.insert("foodLevel", self.food.food);
        out.insert("foodSaturationLevel", self.food.saturation);
        out.insert("foodExhaustionLevel", self.food.exhaustion);
        out.insert("foodTickTimer", self.food.tick_timer());
        out.insert("XpLevel", self.experience.level);
        out.insert("XpP", self.experience.progress);
        out.insert("XpTotal", self.experience.total);
        out.insert(
            "Fire",
            self.fire_ticks.clamp(-1, i32::from(i16::MAX)) as i16,
        );
        out.insert("fall_distance", self.fall_distance);
        out.insert("OnGround", self.on_ground);
        out.insert("abilities", Compound::new().with("flying", self.flying));
        out.insert(
            "active_effects",
            Tag::List(
                self.effects
                    .iter()
                    .map(|effect| Tag::Compound(effect_to_nbt(effect)))
                    .collect(),
            ),
        );
        let mut slots = Vec::new();
        let mut equipment = Compound::new();
        for index in 0..=40usize {
            let Some(stack) = menu_slot(index).and_then(|menu| self.inventory.get(menu)) else {
                continue;
            };
            let mut entry = item_stack_to_nbt(stack);
            if let Some((_, key)) = EQUIPMENT.iter().find(|(slot, _)| *slot == index) {
                equipment.insert(*key, entry);
            } else {
                entry.insert("Slot", index as i8);
                slots.push(Tag::Compound(entry));
            }
        }
        out.insert("Inventory", Tag::List(slots));
        out.insert(
            "SelectedItemSlot",
            i32::from(self.inventory.selected_hotbar),
        );
        if !equipment.is_empty() {
            out.insert("equipment", equipment);
        }
        write_items_at(&mut out, ENDER_ITEMS, &self.ender_items);
        out
    }

    pub fn from_nbt(data: &Compound) -> Result<Self, PlayerDataError> {
        let position = read_position(data)?;
        let rotation = read_rotation(data);
        let mut inventory = Inventory::default();
        if let Some(entries) = data.get_list("Inventory") {
            for entry in entries.iter().filter_map(Tag::as_compound) {
                let slot = entry
                    .get_i64("Slot")
                    .filter(|s| (0..STORAGE_SLOTS).contains(s));
                let stack = item_stack_from_nbt(entry);
                if let (Some(slot), Some(stack)) = (slot, stack)
                    && let Some(menu) = menu_slot(slot as usize)
                {
                    inventory.set(menu, Some(stack));
                }
            }
        }
        if let Some(equipment) = data.get_compound("equipment") {
            for (index, key) in EQUIPMENT {
                let stack = equipment.get_compound(key).and_then(item_stack_from_nbt);
                if let (Some(stack), Some(menu)) = (stack, menu_slot(index)) {
                    inventory.set(menu, Some(stack));
                }
            }
        }
        let selected = data.get_i64("SelectedItemSlot").unwrap_or(0).clamp(0, 8);
        inventory.selected_hotbar = selected as u8;
        let food = FoodData::restore(
            data.get_i32("foodLevel")
                .unwrap_or(MAX_FOOD)
                .clamp(0, MAX_FOOD),
            finite(data.get_f64("foodSaturationLevel"), 5.0).clamp(0.0, 20.0) as f32,
            finite(data.get_f64("foodExhaustionLevel"), 0.0).clamp(0.0, 40.0) as f32,
            data.get_i32("foodTickTimer").unwrap_or(0).max(0),
        );
        let experience = PlayerExperience {
            level: data.get_i32("XpLevel").unwrap_or(0).max(0),
            progress: finite(data.get_f64("XpP"), 0.0).clamp(0.0, 1.0) as f32,
            total: data.get_i32("XpTotal").unwrap_or(0).max(0),
            ..PlayerExperience::default()
        };
        let effects = data
            .get_list("active_effects")
            .unwrap_or_default()
            .iter()
            .filter_map(Tag::as_compound)
            .filter_map(effect_from_nbt)
            .collect();
        Ok(Self {
            dimension: data
                .get_str("Dimension")
                .and_then(|raw| Identifier::parse(raw).ok())
                .unwrap_or_else(|| Identifier::minecraft("overworld")),
            position,
            rotation,
            game_mode: data.get_i32("playerGameType").and_then(GameMode::from_id),
            previous_game_mode: data
                .get_i32("previousPlayerGameType")
                .and_then(GameMode::from_id),
            inventory,
            health: finite(data.get_f64("Health"), f64::from(MAX_HEALTH)).clamp(0.0, 1024.0) as f32,
            absorption: finite(data.get_f64("AbsorptionAmount"), 0.0).clamp(0.0, 1024.0) as f32,
            food,
            experience,
            effects,
            fire_ticks: data.get_i32("Fire").unwrap_or(0).max(-1),
            fall_distance: finite(data.get_f64("fall_distance"), 0.0).max(0.0),
            on_ground: data.get_bool("OnGround").unwrap_or(false),
            flying: data
                .get_compound("abilities")
                .and_then(|abilities| abilities.get_bool("flying"))
                .unwrap_or(false),
            ender_items: read_items_at(data, ENDER_ITEMS, ENDER_SLOTS),
            seen_credits: data.get_bool("seenCredits").unwrap_or(false),
            warden_tracker: rspigot_domain::warden::WardenSpawnTracker::read_or_default(
                data.get_compound(rspigot_domain::warden::TRACKER_KEY),
            ),
        })
    }
}

fn finite(value: Option<f64>, default: f64) -> f64 {
    value.filter(|v| v.is_finite()).unwrap_or(default)
}

fn read_position(data: &Compound) -> Result<Vec3, PlayerDataError> {
    let list = data
        .get_list("Pos")
        .filter(|list| list.len() == 3)
        .ok_or(PlayerDataError::MissingPosition)?;
    let mut axes = [0.0; 3];
    for (axis, tag) in axes.iter_mut().zip(list) {
        *axis = tag.as_f64().ok_or(PlayerDataError::MissingPosition)?;
        if !axis.is_finite() || axis.abs() > MAX_COORDINATE {
            return Err(PlayerDataError::InvalidPosition);
        }
    }
    Ok(Vec3::new(axes[0], axes[1], axes[2]))
}

fn read_rotation(data: &Compound) -> Rotation {
    let angles = data
        .get_list("Rotation")
        .filter(|list| list.len() == 2)
        .and_then(|list| Some((list[0].as_f64()?, list[1].as_f64()?)))
        .filter(|(yaw, pitch)| yaw.is_finite() && pitch.is_finite());
    angles.map_or_else(Rotation::default, |(yaw, pitch)| {
        Rotation::new(yaw as f32, (pitch as f32).clamp(-90.0, 90.0))
    })
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use rspigot_domain::effect::{EffectDuration, EffectId};
    use rspigot_domain::item::{HOTBAR_START, ItemStack, OFFHAND_SLOT};
    use rspigot_items::ItemKind;

    use super::*;

    fn stack(name: &str, count: u8) -> ItemStack {
        let kind = ItemKind::from_name(name).expect("known item");
        ItemStack::new(kind.id(), NonZeroU8::new(count).expect("non zero"))
    }

    fn sample() -> PlayerSave {
        let mut inventory = Inventory::default();
        inventory.set(HOTBAR_START + 2, Some(stack("minecraft:diamond", 3)));
        inventory.set(20, Some(stack("minecraft:cobblestone", 64)));
        inventory.set(5, Some(stack("minecraft:iron_helmet", 1)));
        inventory.set(8, Some(stack("minecraft:iron_boots", 1)));
        inventory.set(OFFHAND_SLOT, Some(stack("minecraft:shield", 1)));
        inventory.selected_hotbar = 2;
        let experience = PlayerExperience {
            level: 7,
            progress: 0.25,
            total: 91,
            ..PlayerExperience::default()
        };
        let speed = EffectId::from_name("minecraft:speed").expect("speed");
        PlayerSave {
            dimension: Identifier::minecraft("the_end"),
            position: Vec3::new(120.5, 71.0, -33.25),
            rotation: Rotation::new(90.0, -12.5),
            game_mode: Some(GameMode::Creative),
            previous_game_mode: Some(GameMode::Survival),
            inventory,
            health: 13.5,
            absorption: 2.0,
            food: FoodData::restore(14, 3.5, 1.25, 40),
            experience,
            effects: vec![EffectInstance::new(
                speed,
                EffectDuration::Ticks(600),
                1,
                true,
            )],
            fire_ticks: 20,
            fall_distance: 2.5,
            on_ground: true,
            flying: true,
            warden_tracker: rspigot_domain::warden::WardenSpawnTracker {
                ticks_since_last_warning: 30,
                warning_level: 2,
                cooldown_ticks: 9,
            },
            ender_items: {
                let mut ender = vec![None; ENDER_SLOTS];
                ender[3] = Some(stack("minecraft:diamond", 5));
                ender[26] = Some(stack("minecraft:emerald", 1));
                ender
            },
            seen_credits: true,
        }
    }

    #[test]
    fn nbt_round_trip_keeps_every_field() {
        let save = sample();
        let decoded = PlayerSave::from_nbt(&save.to_nbt()).expect("decodes");
        assert_eq!(decoded, save);
    }

    #[test]
    fn the_credits_flag_is_saved_as_seen_credits_and_defaults_to_unseen() {
        let nbt = sample().to_nbt();
        assert_eq!(nbt.get_bool("seenCredits"), Some(true));
        let mut bare = nbt.clone();
        bare.remove("seenCredits");
        assert!(!PlayerSave::from_nbt(&bare).expect("decodes").seen_credits);
    }

    #[test]
    fn a_save_without_a_dimension_is_in_the_overworld() {
        let mut nbt = sample().to_nbt();
        nbt.remove("Dimension");
        let decoded = PlayerSave::from_nbt(&nbt).expect("decodes");
        assert_eq!(decoded.dimension, Identifier::minecraft("overworld"));
    }

    #[test]
    fn the_ender_chest_items_are_saved_under_ender_items() {
        let nbt = sample().to_nbt();
        let slots: Vec<i64> = nbt
            .get_list("EnderItems")
            .expect("EnderItems")
            .iter()
            .filter_map(Tag::as_compound)
            .filter_map(|entry| entry.get_i64("Slot"))
            .collect();
        assert_eq!(slots, vec![3, 26]);
        let decoded = PlayerSave::from_nbt(&nbt).expect("decodes");
        assert_eq!(
            decoded.ender_items[3].as_ref().map(|s| s.count.get()),
            Some(5)
        );
    }

    #[test]
    fn a_save_without_ender_items_has_an_empty_ender_chest() {
        let mut nbt = sample().to_nbt();
        nbt.remove("EnderItems");
        let decoded = PlayerSave::from_nbt(&nbt).expect("decodes");
        assert_eq!(decoded.ender_items.len(), ENDER_SLOTS);
        assert!(decoded.ender_items.iter().all(Option::is_none));
    }

    #[test]
    fn nbt_uses_the_vanilla_field_names() {
        let nbt = sample().to_nbt();
        let pos = nbt.get_list("Pos").expect("Pos");
        assert_eq!(pos.len(), 3);
        assert_eq!(pos[0].as_f64(), Some(120.5));
        assert_eq!(nbt.get_str("Dimension"), Some("minecraft:the_end"));
        assert_eq!(nbt.get_i32("playerGameType"), Some(1));
        assert_eq!(nbt.get_i32("previousPlayerGameType"), Some(0));
        assert_eq!(nbt.get_i32("XpLevel"), Some(7));
        assert_eq!(nbt.get_i32("foodLevel"), Some(14));
        assert_eq!(nbt.get_i32("SelectedItemSlot"), Some(2));
        let slots: Vec<i64> = nbt
            .get_list("Inventory")
            .expect("Inventory")
            .iter()
            .filter_map(Tag::as_compound)
            .filter_map(|entry| entry.get_i64("Slot"))
            .collect();
        assert_eq!(slots, vec![2, 20]);
        let equipment = nbt.get_compound("equipment").expect("equipment");
        assert!(equipment.get_compound("head").is_some());
        assert!(equipment.get_compound("feet").is_some());
        assert!(equipment.get_compound("offhand").is_some());
        assert!(equipment.get_compound("chest").is_none());
    }

    #[test]
    fn missing_position_is_rejected() {
        let mut nbt = sample().to_nbt();
        nbt.remove("Pos");
        assert_eq!(
            PlayerSave::from_nbt(&nbt),
            Err(PlayerDataError::MissingPosition)
        );
    }

    #[test]
    fn position_outside_the_world_is_rejected() {
        let mut nbt = sample().to_nbt();
        nbt.insert(
            "Pos",
            Tag::List(vec![
                Tag::Double(f64::NAN),
                Tag::Double(64.0),
                Tag::Double(0.0),
            ]),
        );
        assert_eq!(
            PlayerSave::from_nbt(&nbt),
            Err(PlayerDataError::InvalidPosition)
        );
        nbt.insert(
            "Pos",
            Tag::List(vec![
                Tag::Double(5.0e9),
                Tag::Double(64.0),
                Tag::Double(0.0),
            ]),
        );
        assert_eq!(
            PlayerSave::from_nbt(&nbt),
            Err(PlayerDataError::InvalidPosition)
        );
    }

    #[test]
    fn only_the_position_is_required() {
        let nbt = Compound::new().with(
            "Pos",
            Tag::List(vec![Tag::Double(1.0), Tag::Double(2.0), Tag::Double(3.0)]),
        );
        let save = PlayerSave::from_nbt(&nbt).expect("decodes");
        assert_eq!(save.position, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(save.game_mode, None);
        assert_eq!(save.health, MAX_HEALTH);
        assert_eq!(save.food, FoodData::restore(MAX_FOOD, 5.0, 0.0, 0));
        assert_eq!(save.inventory, Inventory::default());
    }

    #[test]
    fn corrupt_values_fall_back_to_safe_ones() {
        let mut nbt = sample().to_nbt();
        nbt.insert("Health", f64::NAN);
        nbt.insert("foodLevel", 999);
        nbt.insert("XpP", 7.0f32);
        nbt.insert("SelectedItemSlot", 42);
        nbt.insert("playerGameType", 9);
        let save = PlayerSave::from_nbt(&nbt).expect("decodes");
        assert_eq!(save.health, MAX_HEALTH);
        assert_eq!(save.food.food, MAX_FOOD);
        assert_eq!(save.experience.progress, 1.0);
        assert_eq!(save.inventory.selected_hotbar, 8);
        assert_eq!(save.game_mode, None);
    }

    #[test]
    fn unknown_items_and_slots_are_skipped() {
        let mut nbt = sample().to_nbt();
        nbt.insert(
            "Inventory",
            Tag::List(vec![
                Tag::Compound(
                    Compound::new()
                        .with("Slot", 3i8)
                        .with("id", "minecraft:not_an_item")
                        .with("count", 1),
                ),
                Tag::Compound(
                    Compound::new()
                        .with("Slot", 90i8)
                        .with("id", "minecraft:diamond")
                        .with("count", 1),
                ),
            ]),
        );
        let save = PlayerSave::from_nbt(&nbt).expect("decodes");
        assert_eq!(save.inventory.get(HOTBAR_START + 3), None);
        assert!(save.inventory.get(HOTBAR_START + 2).is_none());
    }

    #[test]
    fn a_dead_save_is_reported_dead() {
        let mut save = sample();
        save.health = 0.0;
        let decoded = PlayerSave::from_nbt(&save.to_nbt()).expect("decodes");
        assert!(decoded.is_dead());
    }
}
