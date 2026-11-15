use std::collections::BTreeMap;

use rspigot_data::registries::static_index;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{EntityId, ItemId, Vec3};
use rspigot_entities::EntityKind as Kind;
use serde_json::{Map, Value};

use crate::plugins::events::{PlayerStatisticIncrement, Statistic};
use crate::plugins::steps::Continuation;
use crate::plugins::{EventKind, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

pub mod custom {
    pub const LEAVE_GAME: &str = "minecraft:leave_game";
    pub const PLAY_TIME: &str = "minecraft:play_time";
    pub const TOTAL_WORLD_TIME: &str = "minecraft:total_world_time";
    pub const TIME_SINCE_DEATH: &str = "minecraft:time_since_death";
    pub const TIME_SINCE_REST: &str = "minecraft:time_since_rest";
    pub const SNEAK_TIME: &str = "minecraft:sneak_time";
    pub const WALK_ONE_CM: &str = "minecraft:walk_one_cm";
    pub const CROUCH_ONE_CM: &str = "minecraft:crouch_one_cm";
    pub const SPRINT_ONE_CM: &str = "minecraft:sprint_one_cm";
    pub const WALK_ON_WATER_ONE_CM: &str = "minecraft:walk_on_water_one_cm";
    pub const FALL_ONE_CM: &str = "minecraft:fall_one_cm";
    pub const CLIMB_ONE_CM: &str = "minecraft:climb_one_cm";
    pub const FLY_ONE_CM: &str = "minecraft:fly_one_cm";
    pub const WALK_UNDER_WATER_ONE_CM: &str = "minecraft:walk_under_water_one_cm";
    pub const SWIM_ONE_CM: &str = "minecraft:swim_one_cm";
    pub const MINECART_ONE_CM: &str = "minecraft:minecart_one_cm";
    pub const BOAT_ONE_CM: &str = "minecraft:boat_one_cm";
    pub const PIG_ONE_CM: &str = "minecraft:pig_one_cm";
    pub const HORSE_ONE_CM: &str = "minecraft:horse_one_cm";
    pub const STRIDER_ONE_CM: &str = "minecraft:strider_one_cm";
    pub const JUMP: &str = "minecraft:jump";
    pub const DROP: &str = "minecraft:drop";
    pub const DAMAGE_DEALT: &str = "minecraft:damage_dealt";
    pub const DAMAGE_TAKEN: &str = "minecraft:damage_taken";
    pub const DEATHS: &str = "minecraft:deaths";
    pub const MOB_KILLS: &str = "minecraft:mob_kills";
    pub const PLAYER_KILLS: &str = "minecraft:player_kills";
    pub const ENCHANT_ITEM: &str = "minecraft:enchant_item";
    pub const TRADED_WITH_VILLAGER: &str = "minecraft:traded_with_villager";
    pub const SLEEP_IN_BED: &str = "minecraft:sleep_in_bed";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatKind {
    Mined,
    Crafted,
    Used,
    Broken,
    PickedUp,
    Dropped,
    Killed,
    KilledBy,
    Custom,
}

impl StatKind {
    pub const ALL: [Self; 9] = [
        Self::Mined,
        Self::Crafted,
        Self::Used,
        Self::Broken,
        Self::PickedUp,
        Self::Dropped,
        Self::Killed,
        Self::KilledBy,
        Self::Custom,
    ];

    pub const fn registry_name(self) -> &'static str {
        match self {
            Self::Mined => "minecraft:mined",
            Self::Crafted => "minecraft:crafted",
            Self::Used => "minecraft:used",
            Self::Broken => "minecraft:broken",
            Self::PickedUp => "minecraft:picked_up",
            Self::Dropped => "minecraft:dropped",
            Self::Killed => "minecraft:killed",
            Self::KilledBy => "minecraft:killed_by",
            Self::Custom => "minecraft:custom",
        }
    }

    const fn value_registry(self) -> &'static str {
        match self {
            Self::Mined => "minecraft:block",
            Self::Killed | Self::KilledBy => "minecraft:entity_type",
            Self::Custom => "minecraft:custom_stat",
            _ => "minecraft:item",
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

const CUSTOM_STATISTICS: &[(&str, Statistic)] = &[
    ("minecraft:leave_game", Statistic::LeaveGame),
    ("minecraft:play_time", Statistic::PlayOneMinute),
    ("minecraft:total_world_time", Statistic::TotalWorldTime),
    ("minecraft:time_since_death", Statistic::TimeSinceDeath),
    ("minecraft:time_since_rest", Statistic::TimeSinceRest),
    ("minecraft:sneak_time", Statistic::SneakTime),
    ("minecraft:walk_one_cm", Statistic::WalkOneCm),
    ("minecraft:crouch_one_cm", Statistic::CrouchOneCm),
    ("minecraft:sprint_one_cm", Statistic::SprintOneCm),
    (
        "minecraft:walk_on_water_one_cm",
        Statistic::WalkOnWaterOneCm,
    ),
    ("minecraft:fall_one_cm", Statistic::FallOneCm),
    ("minecraft:climb_one_cm", Statistic::ClimbOneCm),
    ("minecraft:fly_one_cm", Statistic::FlyOneCm),
    (
        "minecraft:walk_under_water_one_cm",
        Statistic::WalkUnderWaterOneCm,
    ),
    ("minecraft:minecart_one_cm", Statistic::MinecartOneCm),
    ("minecraft:boat_one_cm", Statistic::BoatOneCm),
    ("minecraft:pig_one_cm", Statistic::PigOneCm),
    ("minecraft:happy_ghast_one_cm", Statistic::HappyGhastOneCm),
    ("minecraft:horse_one_cm", Statistic::HorseOneCm),
    ("minecraft:aviate_one_cm", Statistic::AviateOneCm),
    ("minecraft:swim_one_cm", Statistic::SwimOneCm),
    ("minecraft:strider_one_cm", Statistic::StriderOneCm),
    ("minecraft:nautilus_one_cm", Statistic::NautilusOneCm),
    ("minecraft:jump", Statistic::Jump),
    ("minecraft:drop", Statistic::DropCount),
    ("minecraft:damage_dealt", Statistic::DamageDealt),
    (
        "minecraft:damage_dealt_absorbed",
        Statistic::DamageDealtAbsorbed,
    ),
    (
        "minecraft:damage_dealt_resisted",
        Statistic::DamageDealtResisted,
    ),
    ("minecraft:damage_taken", Statistic::DamageTaken),
    (
        "minecraft:damage_blocked_by_shield",
        Statistic::DamageBlockedByShield,
    ),
    ("minecraft:damage_absorbed", Statistic::DamageAbsorbed),
    ("minecraft:damage_resisted", Statistic::DamageResisted),
    ("minecraft:deaths", Statistic::Deaths),
    ("minecraft:mob_kills", Statistic::MobKills),
    ("minecraft:animals_bred", Statistic::AnimalsBred),
    ("minecraft:player_kills", Statistic::PlayerKills),
    ("minecraft:fish_caught", Statistic::FishCaught),
    ("minecraft:talked_to_villager", Statistic::TalkedToVillager),
    (
        "minecraft:traded_with_villager",
        Statistic::TradedWithVillager,
    ),
    ("minecraft:eat_cake_slice", Statistic::CakeSlicesEaten),
    ("minecraft:fill_cauldron", Statistic::CauldronFilled),
    ("minecraft:use_cauldron", Statistic::CauldronUsed),
    ("minecraft:clean_armor", Statistic::ArmorCleaned),
    ("minecraft:clean_banner", Statistic::BannerCleaned),
    ("minecraft:clean_shulker_box", Statistic::CleanShulkerBox),
    (
        "minecraft:interact_with_brewingstand",
        Statistic::BrewingstandInteraction,
    ),
    (
        "minecraft:interact_with_beacon",
        Statistic::BeaconInteraction,
    ),
    ("minecraft:inspect_dropper", Statistic::DropperInspected),
    ("minecraft:inspect_hopper", Statistic::HopperInspected),
    ("minecraft:inspect_dispenser", Statistic::DispenserInspected),
    ("minecraft:play_noteblock", Statistic::NoteblockPlayed),
    ("minecraft:tune_noteblock", Statistic::NoteblockTuned),
    ("minecraft:pot_flower", Statistic::FlowerPotted),
    (
        "minecraft:trigger_trapped_chest",
        Statistic::TrappedChestTriggered,
    ),
    ("minecraft:open_enderchest", Statistic::EnderchestOpened),
    ("minecraft:enchant_item", Statistic::ItemEnchanted),
    ("minecraft:play_record", Statistic::RecordPlayed),
    (
        "minecraft:interact_with_furnace",
        Statistic::FurnaceInteraction,
    ),
    (
        "minecraft:interact_with_crafting_table",
        Statistic::CraftingTableInteraction,
    ),
    ("minecraft:open_chest", Statistic::ChestOpened),
    ("minecraft:sleep_in_bed", Statistic::SleepInBed),
    ("minecraft:sleep_in_straw_bed", Statistic::SleepInStrawBed),
    ("minecraft:open_shulker_box", Statistic::ShulkerBoxOpened),
    ("minecraft:open_barrel", Statistic::OpenBarrel),
    (
        "minecraft:interact_with_blast_furnace",
        Statistic::InteractWithBlastFurnace,
    ),
    (
        "minecraft:interact_with_smoker",
        Statistic::InteractWithSmoker,
    ),
    (
        "minecraft:interact_with_lectern",
        Statistic::InteractWithLectern,
    ),
    (
        "minecraft:interact_with_campfire",
        Statistic::InteractWithCampfire,
    ),
    (
        "minecraft:interact_with_cartography_table",
        Statistic::InteractWithCartographyTable,
    ),
    ("minecraft:interact_with_loom", Statistic::InteractWithLoom),
    (
        "minecraft:interact_with_stonecutter",
        Statistic::InteractWithStonecutter,
    ),
    ("minecraft:bell_ring", Statistic::BellRing),
    ("minecraft:raid_trigger", Statistic::RaidTrigger),
    ("minecraft:raid_win", Statistic::RaidWin),
    (
        "minecraft:interact_with_anvil",
        Statistic::InteractWithAnvil,
    ),
    (
        "minecraft:interact_with_grindstone",
        Statistic::InteractWithGrindstone,
    ),
    ("minecraft:target_hit", Statistic::TargetHit),
    (
        "minecraft:interact_with_smithing_table",
        Statistic::InteractWithSmithingTable,
    ),
];

fn is_continuous(statistic: Statistic) -> bool {
    matches!(
        statistic,
        Statistic::PlayOneMinute
            | Statistic::TotalWorldTime
            | Statistic::WalkOneCm
            | Statistic::WalkOnWaterOneCm
            | Statistic::FallOneCm
            | Statistic::SneakTime
            | Statistic::ClimbOneCm
            | Statistic::FlyOneCm
            | Statistic::WalkUnderWaterOneCm
            | Statistic::MinecartOneCm
            | Statistic::BoatOneCm
            | Statistic::PigOneCm
            | Statistic::HorseOneCm
            | Statistic::SprintOneCm
            | Statistic::CrouchOneCm
            | Statistic::TimeSinceDeath
            | Statistic::SwimOneCm
            | Statistic::TimeSinceRest
            | Statistic::AviateOneCm
            | Statistic::StriderOneCm
            | Statistic::NautilusOneCm
    )
}

fn bukkit_statistic(kind: StatKind, key: &str) -> Option<Statistic> {
    Some(match kind {
        StatKind::Custom => CUSTOM_STATISTICS.iter().find(|(name, _)| *name == key)?.1,
        StatKind::Mined => Statistic::MineBlock,
        StatKind::Crafted => Statistic::CraftItem,
        StatKind::Used => Statistic::UseItem,
        StatKind::Broken => Statistic::BreakItem,
        StatKind::PickedUp => Statistic::Pickup,
        StatKind::Dropped => Statistic::Drop,
        StatKind::Killed => Statistic::KillEntity,
        StatKind::KilledBy => Statistic::EntityKilledBy,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlayerStats {
    counters: [BTreeMap<String, i32>; 9],
}

impl PlayerStats {
    pub fn get(&self, kind: StatKind, key: &str) -> i32 {
        self.counters[kind.index()].get(key).copied().unwrap_or(0)
    }

    pub fn add(&mut self, kind: StatKind, key: &str, amount: i32) -> i32 {
        let counters = &mut self.counters[kind.index()];
        if let Some(value) = counters.get_mut(key) {
            *value = value.saturating_add(amount);
            return *value;
        }
        counters.insert(key.to_owned(), amount);
        amount
    }

    pub fn reset(&mut self, kind: StatKind, key: &str) {
        if let Some(value) = self.counters[kind.index()].get_mut(key) {
            *value = 0;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.counters.iter().all(BTreeMap::is_empty)
    }

    pub fn entries(&self) -> Vec<(i32, i32, i32)> {
        let mut out = Vec::new();
        for kind in StatKind::ALL {
            for (key, amount) in &self.counters[kind.index()] {
                if let Some(value) = static_index(kind.value_registry(), key) {
                    out.push((kind.index() as i32, value as i32, *amount));
                }
            }
        }
        out
    }

    pub fn to_json(&self) -> String {
        let mut stats = Map::new();
        for kind in StatKind::ALL {
            let counters = &self.counters[kind.index()];
            if counters.is_empty() {
                continue;
            }
            let object: Map<String, Value> = counters
                .iter()
                .map(|(key, amount)| (key.clone(), Value::from(*amount)))
                .collect();
            stats.insert(kind.registry_name().to_owned(), Value::Object(object));
        }
        let mut root = Map::new();
        root.insert("stats".to_owned(), Value::Object(stats));
        Value::Object(root).to_string()
    }

    pub fn from_json(text: &str) -> Self {
        let mut stats = Self::default();
        let Ok(Value::Object(root)) = serde_json::from_str::<Value>(text) else {
            return stats;
        };
        let Some(Value::Object(by_kind)) = root.get("stats") else {
            return stats;
        };
        for kind in StatKind::ALL {
            let Some(Value::Object(counters)) = by_kind.get(kind.registry_name()) else {
                continue;
            };
            for (key, amount) in counters {
                let amount = amount
                    .as_i64()
                    .map(|value| value.clamp(0, i64::from(i32::MAX)) as i32);
                if let Some(amount) = amount {
                    stats.counters[kind.index()].insert(key.clone(), amount);
                }
            }
        }
        stats
    }
}

const FLY_THRESHOLD: i32 = 25;
const HORSES: [&str; 9] = [
    "minecraft:horse",
    "minecraft:donkey",
    "minecraft:mule",
    "minecraft:skeleton_horse",
    "minecraft:zombie_horse",
    "minecraft:camel",
    "minecraft:camel_husk",
    "minecraft:llama",
    "minecraft:trader_llama",
];

fn centimeters(distance: f64) -> i32 {
    (distance * 100.0).round() as i32
}

#[derive(Debug)]
pub(crate) struct StatStep {
    pub(crate) player: PlayerId,
    pub(crate) kind: StatKind,
    pub(crate) key: String,
    pub(crate) amount: i32,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn award(&mut self, id: PlayerId, kind: StatKind, key: &str, amount: i32) {
        if amount <= 0 {
            return;
        }
        let statistic = bukkit_statistic(kind, key);
        let fired = statistic.is_some_and(|statistic| !is_continuous(statistic))
            && self.listened(EventKind::PlayerStatisticIncrement);
        if !fired {
            self.add_stat(id, kind, key, amount);
            return;
        }
        let step = StatStep {
            player: id,
            kind,
            key: key.to_owned(),
            amount,
        };
        self.fire(id, Continuation::Stat(step), move |server| {
            let statistic = bukkit_statistic(kind, key)?;
            let previous = server.players.get(&id)?.stats.get(kind, key);
            let (entity_type, material) = match kind {
                StatKind::Custom => (None, None),
                StatKind::Killed | StatKind::KilledBy => (Some(key.to_owned()), None),
                _ => (None, Some(key.to_owned())),
            };
            Some(EventPayload::PlayerStatisticIncrement(
                PlayerStatisticIncrement {
                    entity_type,
                    material,
                    new_value: previous.saturating_add(amount),
                    player: server.snapshot_of(id)?,
                    previous_value: previous,
                    statistic,
                },
            ))
        });
    }

    pub(crate) fn award_custom(&mut self, id: PlayerId, key: &str, amount: i32) {
        self.award(id, StatKind::Custom, key, amount);
    }

    pub(crate) fn finish_stat(&mut self, step: StatStep, cancelled: bool) {
        if !cancelled {
            self.add_stat(step.player, step.kind, &step.key, step.amount);
        }
    }

    fn add_stat(&mut self, id: PlayerId, kind: StatKind, key: &str, amount: i32) {
        if let Some(player) = self.players.get_mut(&id) {
            player.stats.add(kind, key, amount);
        }
    }

    pub(crate) fn count_player_death(
        &mut self,
        id: PlayerId,
        killer: Option<PlayerId>,
        source: &crate::health::DamageSource,
    ) {
        self.award_custom(id, custom::DEATHS, 1);
        self.reset_stat(id, custom::TIME_SINCE_DEATH);
        self.reset_stat(id, custom::TIME_SINCE_REST);
        if let Some(cause) = &source.causing {
            self.award(id, StatKind::KilledBy, cause.kind.name(), 1);
        }
        if let Some(killer) = killer.or_else(|| source.attacker()) {
            self.award_custom(killer, custom::PLAYER_KILLS, 1);
            self.award(killer, StatKind::Killed, Kind::Player.name(), 1);
        }
    }

    pub(crate) fn count_mob_kill(&mut self, victim: EntityId, killer: Option<PlayerId>) {
        let (Some(killer), Some(kind)) = (killer, self.kind_of(victim)) else {
            return;
        };
        self.award_custom(killer, custom::MOB_KILLS, 1);
        self.award(killer, StatKind::Killed, kind.name(), 1);
    }

    pub(crate) fn count_movement(&mut self, id: PlayerId, from: Vec3, to: Vec3) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if player.vitals.is_dead()
            || player.sleeping.is_some()
            || self.entities.rides.vehicle_of(player.entity_id).is_some()
        {
            return;
        }
        let delta = to - from;
        let flat = centimeters(delta.x.hypot(delta.z));
        let full = centimeters((delta.x * delta.x + delta.y * delta.y + delta.z * delta.z).sqrt());
        let eye_in_water = self.block_has_water(player.eye_position().block());
        let (on_ground, sprinting, sneaking) =
            (player.on_ground, player.sprinting, player.sneaking);
        let key_and_amount = if eye_in_water {
            Some((custom::WALK_UNDER_WATER_ONE_CM, full))
        } else if self.block_has_water(to.block()) {
            Some((custom::WALK_ON_WATER_ONE_CM, flat))
        } else if self.is_climbing(to) {
            (delta.y > 0.0).then(|| (custom::CLIMB_ONE_CM, centimeters(delta.y)))
        } else if on_ground {
            let key = if sprinting {
                custom::SPRINT_ONE_CM
            } else if sneaking {
                custom::CROUCH_ONE_CM
            } else {
                custom::WALK_ONE_CM
            };
            Some((key, flat))
        } else {
            Some((custom::FLY_ONE_CM, flat)).filter(|(_, amount)| *amount > FLY_THRESHOLD)
        };
        if let Some((key, amount)) = key_and_amount {
            self.award_custom(id, key, amount);
        }
        if delta.y < 0.0 && !on_ground {
            self.award_custom(id, custom::FALL_ONE_CM, centimeters(-delta.y));
        }
    }

    pub(crate) fn count_vehicle_movement(&mut self, id: PlayerId, vehicle: EntityId, delta: Vec3) {
        let Some(name) = self.kind_of(vehicle).map(Kind::name) else {
            return;
        };
        let key = if name.ends_with("_boat") || name.ends_with("_raft") {
            custom::BOAT_ONE_CM
        } else if name.contains("minecart") {
            custom::MINECART_ONE_CM
        } else if name == "minecraft:pig" {
            custom::PIG_ONE_CM
        } else if name == "minecraft:strider" {
            custom::STRIDER_ONE_CM
        } else if HORSES.contains(&name) {
            custom::HORSE_ONE_CM
        } else {
            return;
        };
        let amount =
            centimeters((delta.x * delta.x + delta.y * delta.y + delta.z * delta.z).sqrt());
        self.award_custom(id, key, amount);
    }

    fn block_has_water(&self, pos: rspigot_domain::BlockPos) -> bool {
        self.world
            .chunks
            .block(pos)
            .is_some_and(|state| state.flags().contains(rspigot_blocks::StateFlags::WATER))
    }

    fn is_climbing(&self, position: Vec3) -> bool {
        self.world
            .chunks
            .block(position.block())
            .is_some_and(|state| {
                state.kind().is_some_and(|kind| {
                    rspigot_data::tags::tag("minecraft:block", "minecraft:climbable")
                        .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
                })
            })
    }

    pub(crate) fn count_used(&mut self, id: PlayerId, stack: &ItemStack) {
        if let Some(kind) = stack.item.kind() {
            self.award(id, StatKind::Used, kind.name(), 1);
        }
    }

    pub(crate) fn count_dropped(&mut self, id: PlayerId, stack: &ItemStack) {
        self.award_custom(id, custom::DROP, 1);
        if let Some(kind) = stack.item.kind() {
            self.award(
                id,
                StatKind::Dropped,
                kind.name(),
                i32::from(stack.count.get()),
            );
        }
    }

    pub(crate) fn count_picked_up(&mut self, id: PlayerId, stack: &ItemStack, amount: u32) {
        if let Some(kind) = stack.item.kind() {
            self.award(
                id,
                StatKind::PickedUp,
                kind.name(),
                i32::try_from(amount).unwrap_or(i32::MAX),
            );
        }
    }

    pub(crate) fn count_broken(&mut self, id: PlayerId, stack: &ItemStack) {
        if let Some(kind) = stack.item.kind() {
            self.award(id, StatKind::Broken, kind.name(), 1);
        }
    }

    pub(crate) fn count_crafted(&mut self, id: PlayerId, item: ItemId, amount: u32) {
        if let Some(kind) = item.kind() {
            self.award(
                id,
                StatKind::Crafted,
                kind.name(),
                i32::try_from(amount).unwrap_or(i32::MAX),
            );
        }
    }

    pub(crate) fn reset_stat(&mut self, id: PlayerId, key: &str) {
        if let Some(player) = self.players.get_mut(&id) {
            player.stats.reset(StatKind::Custom, key);
        }
    }

    pub(crate) fn send_stats(&self, id: PlayerId) {
        if let Some(player) = self.players.get(&id) {
            player
                .link
                .send(crate::events::ClientEvent::Stats(player.stats.entries()));
        }
    }

    pub(crate) fn tick_stat_clocks(&mut self) {
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for id in ids {
            let Some(player) = self.players.get_mut(&id) else {
                continue;
            };
            let stats = &mut player.stats;
            stats.add(StatKind::Custom, custom::PLAY_TIME, 1);
            stats.add(StatKind::Custom, custom::TOTAL_WORLD_TIME, 1);
            if player.vitals.is_dead() {
                continue;
            }
            stats.add(StatKind::Custom, custom::TIME_SINCE_DEATH, 1);
            if player.sneaking {
                stats.add(StatKind::Custom, custom::SNEAK_TIME, 1);
            }
            if player.sleeping.is_none() {
                stats.add(StatKind::Custom, custom::TIME_SINCE_REST, 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_round_trip_through_the_vanilla_layout() {
        let mut stats = PlayerStats::default();
        stats.add(StatKind::Custom, custom::JUMP, 3);
        stats.add(StatKind::Mined, "minecraft:stone", 2);
        stats.add(StatKind::Mined, "minecraft:stone", 1);
        let restored = PlayerStats::from_json(&stats.to_json());
        assert_eq!(restored, stats);
        assert_eq!(restored.get(StatKind::Mined, "minecraft:stone"), 3);
        assert!(stats.to_json().contains("\"minecraft:custom\""));
    }

    #[test]
    fn entries_use_the_registry_ids_of_the_stat_type_and_the_value() {
        let mut stats = PlayerStats::default();
        stats.add(StatKind::Custom, custom::JUMP, 5);
        assert_eq!(stats.entries(), vec![(8, 23, 5)]);
    }

    #[test]
    fn every_custom_statistic_in_the_table_is_a_registered_stat() {
        for (name, _) in CUSTOM_STATISTICS {
            assert!(
                static_index("minecraft:custom_stat", name).is_some(),
                "{name}"
            );
        }
    }

    #[test]
    fn unreadable_documents_load_as_empty_stats() {
        assert!(PlayerStats::from_json("nope").is_empty());
        assert!(PlayerStats::from_json("{\"stats\":3}").is_empty());
    }
}
