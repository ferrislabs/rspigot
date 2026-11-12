use rspigot_blocks::BlockKind;
use rspigot_items::ItemKind;
use rspigot_nbt::{Compound, Tag};

use super::zombie_villager::{data_compound, parse_data, type_for_biome};
use crate::BiomeId;
use crate::entity::base::EntityBase;
use crate::entity::data::SyncedValue;
use crate::entity::gossip::GossipContainer;
use crate::entity::trading::{MerchantOffer, MerchantOffers};
use crate::item::ItemStack;
use crate::poi::PoiType;
use crate::random::RandomSource;

pub const DATA_UNHAPPY_COUNTER: u8 = 18;
pub const DATA_VILLAGER_DATA: u8 = 19;
pub const DATA_VILLAGER_DATA_FINALIZED: u8 = 20;
pub const DEFAULT_TYPE: u32 = 2;
pub const MIN_LEVEL: i32 = 1;
pub const MAX_LEVEL: i32 = 5;
pub const NEXT_LEVEL_XP_THRESHOLDS: [i32; 5] = [0, 10, 70, 150, 250];
pub const BREEDING_FOOD_THRESHOLD: i32 = 12;
pub const FOOD_EXCESS_THRESHOLD: i32 = 23;
pub const INVENTORY_SIZE: usize = 8;
pub const GOSSIP_DECAY_INTERVAL: i64 = 24_000;
pub const GOSSIP_COOLDOWN: i64 = 1_200;
pub const RESTOCK_INTERVAL: i64 = 12_000;
pub const RESTOCK_COOLDOWN: i64 = 2_400;
pub const MAX_RESTOCKS_PER_DAY: i32 = 2;
pub const UNHAPPY_TICKS: i32 = 40;
pub const GOLEM_MEMORY_TICKS: i64 = 24_000;
pub const OFFSPRING_AGE_AFTER_BREEDING: i32 = 6_000;
pub const DAY_TICKS: i64 = 24_000;
pub const BABY_EYE_HEIGHT: f32 = 0.63;
pub const BABY_WIDTH: f32 = 0.49;
pub const BABY_HEIGHT: f32 = 0.98;
pub const KEYS: &[&str] = &[
    "VillagerData",
    "VillagerDataFinalized",
    "FoodLevel",
    "Gossips",
    "Xp",
    "LastRestock",
    "LastGossipDecay",
    "RestocksToday",
    "Offers",
    "Inventory",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Profession {
    #[default]
    None,
    Armorer,
    Butcher,
    Cartographer,
    Cleric,
    Farmer,
    Fisherman,
    Fletcher,
    Leatherworker,
    Librarian,
    Mason,
    Nitwit,
    Shepherd,
    Toolsmith,
    Weaponsmith,
}

impl Profession {
    pub const ALL: [Self; 15] = [
        Self::None,
        Self::Armorer,
        Self::Butcher,
        Self::Cartographer,
        Self::Cleric,
        Self::Farmer,
        Self::Fisherman,
        Self::Fletcher,
        Self::Leatherworker,
        Self::Librarian,
        Self::Mason,
        Self::Nitwit,
        Self::Shepherd,
        Self::Toolsmith,
        Self::Weaponsmith,
    ];

    pub fn from_index(index: u32) -> Self {
        Self::ALL.get(index as usize).copied().unwrap_or(Self::None)
    }

    pub const fn index(self) -> u32 {
        self as u32
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Armorer => "armorer",
            Self::Butcher => "butcher",
            Self::Cartographer => "cartographer",
            Self::Cleric => "cleric",
            Self::Farmer => "farmer",
            Self::Fisherman => "fisherman",
            Self::Fletcher => "fletcher",
            Self::Leatherworker => "leatherworker",
            Self::Librarian => "librarian",
            Self::Mason => "mason",
            Self::Nitwit => "nitwit",
            Self::Shepherd => "shepherd",
            Self::Toolsmith => "toolsmith",
            Self::Weaponsmith => "weaponsmith",
        }
    }

    pub const fn job_site(self) -> Option<PoiType> {
        match self {
            Self::Armorer => Some(PoiType::Armorer),
            Self::Butcher => Some(PoiType::Butcher),
            Self::Cartographer => Some(PoiType::Cartographer),
            Self::Cleric => Some(PoiType::Cleric),
            Self::Farmer => Some(PoiType::Farmer),
            Self::Fisherman => Some(PoiType::Fisherman),
            Self::Fletcher => Some(PoiType::Fletcher),
            Self::Leatherworker => Some(PoiType::Leatherworker),
            Self::Librarian => Some(PoiType::Librarian),
            Self::Mason => Some(PoiType::Mason),
            Self::Shepherd => Some(PoiType::Shepherd),
            Self::Toolsmith => Some(PoiType::Toolsmith),
            Self::Weaponsmith => Some(PoiType::Weaponsmith),
            Self::None | Self::Nitwit => None,
        }
    }

    pub fn holds_job_site(self, poi: PoiType) -> bool {
        self.job_site() == Some(poi)
    }

    pub fn acquires_job_site(self, poi: PoiType) -> bool {
        match self {
            Self::None => poi.is_acquirable_job_site(),
            _ => self.job_site() == Some(poi),
        }
    }

    pub fn from_job_site(poi: PoiType) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|profession| profession.job_site() == Some(poi))
    }

    pub fn work_sound(self) -> Option<&'static str> {
        Some(match self {
            Self::Armorer => "minecraft:entity.villager.work_armorer",
            Self::Butcher => "minecraft:entity.villager.work_butcher",
            Self::Cartographer => "minecraft:entity.villager.work_cartographer",
            Self::Cleric => "minecraft:entity.villager.work_cleric",
            Self::Farmer => "minecraft:entity.villager.work_farmer",
            Self::Fisherman => "minecraft:entity.villager.work_fisherman",
            Self::Fletcher => "minecraft:entity.villager.work_fletcher",
            Self::Leatherworker => "minecraft:entity.villager.work_leatherworker",
            Self::Librarian => "minecraft:entity.villager.work_librarian",
            Self::Mason => "minecraft:entity.villager.work_mason",
            Self::Shepherd => "minecraft:entity.villager.work_shepherd",
            Self::Toolsmith => "minecraft:entity.villager.work_toolsmith",
            Self::Weaponsmith => "minecraft:entity.villager.work_weaponsmith",
            Self::None | Self::Nitwit => return None,
        })
    }

    pub fn requested_items(self) -> &'static [ItemKind] {
        match self {
            Self::Farmer => &[
                ItemKind::Wheat,
                ItemKind::WheatSeeds,
                ItemKind::BeetrootSeeds,
                ItemKind::BoneMeal,
            ],
            _ => &[],
        }
    }

    pub fn secondary_pois(self) -> &'static [BlockKind] {
        match self {
            Self::Farmer => &[BlockKind::Farmland],
            _ => &[],
        }
    }

    pub fn trade_set(self, level: i32) -> Option<String> {
        if self.job_site().is_none() || !(MIN_LEVEL..=MAX_LEVEL).contains(&level) {
            return None;
        }
        Some(format!("minecraft:{}/level_{}", self.name(), level))
    }
}

pub fn can_level_up(level: i32) -> bool {
    (MIN_LEVEL..MAX_LEVEL).contains(&level)
}

pub fn min_xp_per_level(level: i32) -> i32 {
    if can_level_up(level) {
        NEXT_LEVEL_XP_THRESHOLDS[(level - 1) as usize]
    } else {
        0
    }
}

pub fn max_xp_per_level(level: i32) -> i32 {
    if can_level_up(level) {
        NEXT_LEVEL_XP_THRESHOLDS[level as usize]
    } else {
        0
    }
}

pub fn food_nutrition(item: ItemKind) -> i32 {
    match item {
        ItemKind::Bread => 4,
        ItemKind::Potato | ItemKind::Carrot | ItemKind::Beetroot => 1,
        _ => 0,
    }
}

pub fn is_plantable_seed(stack: &ItemStack) -> bool {
    rspigot_data::tags::tag("minecraft:item", "minecraft:villager_plantable_seeds")
        .is_some_and(|ids| ids.binary_search(&u32::from(stack.item.0)).is_ok())
}

pub fn picks_up(item: ItemKind, profession: Profession) -> bool {
    food_nutrition(item) > 0
        || matches!(item, ItemKind::Wheat)
        || rspigot_data::tags::tag("minecraft:item", "minecraft:villager_picks_up")
            .is_some_and(|ids| ids.binary_search(&u32::from(item.id().0)).is_ok())
        || profession.requested_items().contains(&item)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VillagerState {
    pub food_level: i32,
    pub xp: i32,
    pub last_restock: i64,
    pub last_gossip_decay: i64,
    pub restocks_today: i32,
    pub last_restock_check_day: i64,
    pub last_gossip_time: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MerchantState {
    pub offers: Option<MerchantOffers>,
    pub inventory: Vec<Option<ItemStack>>,
    pub gossips: GossipContainer,
    pub trading_player: Option<crate::EntityId>,
    pub last_traded_player: Option<crate::EntityId>,
}

impl MerchantState {
    pub fn new() -> Self {
        Self {
            inventory: vec![None; INVENTORY_SIZE],
            ..Self::default()
        }
    }

    pub fn food_points(&self) -> i32 {
        self.inventory
            .iter()
            .flatten()
            .filter_map(|stack| {
                stack
                    .item
                    .kind()
                    .map(|kind| food_nutrition(kind) * i32::from(stack.count.get()))
            })
            .sum()
    }

    pub fn has_farm_seeds(&self) -> bool {
        self.inventory.iter().flatten().any(is_plantable_seed)
    }

    pub fn count_item(&self, item: ItemKind) -> i32 {
        self.inventory
            .iter()
            .flatten()
            .filter(|stack| stack.item == item.id())
            .map(|stack| i32::from(stack.count.get()))
            .sum()
    }

    pub fn can_add(&self, stack: &ItemStack) -> bool {
        let max = u32::from(stack.item.max_stack_size());
        let mut remaining = u32::from(stack.count.get());
        for slot in &self.inventory {
            match slot {
                None => return true,
                Some(held) if held.item == stack.item && held.components == stack.components => {
                    let room = max.saturating_sub(u32::from(held.count.get()));
                    if room >= remaining {
                        return true;
                    }
                    remaining -= room;
                }
                Some(_) => {}
            }
        }
        remaining == 0
    }

    pub fn add_item(&mut self, stack: ItemStack) -> Option<ItemStack> {
        use std::num::NonZeroU8;
        let max = u32::from(stack.item.max_stack_size());
        let mut remaining = u32::from(stack.count.get());
        for slot in self.inventory.iter_mut() {
            if let Some(held) = slot
                && held.item == stack.item
                && held.components == stack.components
            {
                let room = max.saturating_sub(u32::from(held.count.get()));
                let moved = room.min(remaining);
                if let Some(count) = NonZeroU8::new((u32::from(held.count.get()) + moved) as u8) {
                    held.count = count;
                    remaining -= moved;
                }
            }
            if remaining == 0 {
                return None;
            }
        }
        for slot in self.inventory.iter_mut() {
            if slot.is_none() {
                let moved = max.min(remaining);
                *slot = NonZeroU8::new(moved as u8).map(|count| ItemStack {
                    count,
                    ..stack.clone()
                });
                remaining -= moved;
                if remaining == 0 {
                    return None;
                }
            }
        }
        NonZeroU8::new(remaining as u8).map(|count| ItemStack { count, ..stack })
    }

    pub fn remove_item(&mut self, slot: usize, amount: i32) -> Option<ItemStack> {
        use std::num::NonZeroU8;
        let held = self.inventory.get_mut(slot)?.as_mut()?;
        let taken = amount.clamp(0, i32::from(held.count.get())) as u8;
        let rest = held.count.get() - taken;
        let out = NonZeroU8::new(taken).map(|count| ItemStack {
            count,
            ..held.clone()
        });
        match NonZeroU8::new(rest) {
            Some(count) => held.count = count,
            None => self.inventory[slot] = None,
        }
        out
    }

    pub fn offers_mut(&mut self) -> Option<&mut MerchantOffers> {
        self.offers.as_mut()
    }

    pub fn inventory_nbt(&self) -> Tag {
        Tag::List(
            self.inventory
                .iter()
                .enumerate()
                .filter_map(|(index, stack)| {
                    let mut entry = crate::entity::save::item_stack_to_nbt(stack.as_ref()?);
                    entry.insert("Slot", index as i8);
                    Some(Tag::Compound(entry))
                })
                .collect(),
        )
    }

    pub fn load_inventory(&mut self, tag: Option<&Tag>) {
        self.inventory = vec![None; INVENTORY_SIZE];
        let Some(Tag::List(entries)) = tag else {
            return;
        };
        for entry in entries {
            let Tag::Compound(entry) = entry else {
                continue;
            };
            let slot = entry.get_i64("Slot").unwrap_or(0) as usize;
            if let (Some(stack), true) = (
                crate::entity::save::item_stack_from_nbt(entry),
                slot < INVENTORY_SIZE,
            ) {
                self.inventory[slot] = Some(stack);
            }
        }
    }
}

impl VillagerState {
    pub fn hungry(&self) -> bool {
        self.food_level < BREEDING_FOOD_THRESHOLD
    }

    pub fn can_breed(&self, merchant: &MerchantState, sleeping: bool, age: i32) -> bool {
        self.food_level + merchant.food_points() >= BREEDING_FOOD_THRESHOLD && !sleeping && age == 0
    }

    pub fn wants_more_food(merchant: &MerchantState) -> bool {
        merchant.food_points() < BREEDING_FOOD_THRESHOLD
    }

    pub fn has_excess_food(merchant: &MerchantState) -> bool {
        merchant.food_points() > FOOD_EXCESS_THRESHOLD
    }

    pub fn eat_until_full(&mut self, merchant: &mut MerchantState) {
        if !self.hungry() {
            return;
        }
        for slot in 0..merchant.inventory.len() {
            let Some(stack) = merchant.inventory[slot].as_ref() else {
                continue;
            };
            let nutrition = stack.item.kind().map_or(0, food_nutrition);
            if nutrition == 0 {
                continue;
            }
            let mut eaten = 0;
            for _ in 0..stack.count.get() {
                self.food_level += nutrition;
                eaten += 1;
                if !self.hungry() {
                    break;
                }
            }
            merchant.remove_item(slot, eaten);
            if !self.hungry() {
                return;
            }
        }
    }

    pub fn eat_and_digest_food(&mut self, merchant: &mut MerchantState) {
        self.eat_until_full(merchant);
        self.food_level -= BREEDING_FOOD_THRESHOLD;
    }

    pub fn should_increase_level(&self, level: i32) -> bool {
        can_level_up(level) && max_xp_per_level(level) <= self.xp
    }

    pub fn maybe_decay_gossip(&mut self, gossips: &mut GossipContainer, game_time: i64) {
        if self.last_gossip_decay == 0 {
            self.last_gossip_decay = game_time;
        } else if self.last_gossip_decay + GOSSIP_DECAY_INTERVAL <= game_time {
            gossips.decay();
            self.last_gossip_decay = game_time;
        }
    }

    pub fn gossip_ready(&self, game_time: i64) -> bool {
        !(self.last_gossip_time <= game_time && game_time < self.last_gossip_time + GOSSIP_COOLDOWN)
    }

    pub fn allowed_to_restock(&self, game_time: i64) -> bool {
        self.restocks_today == 0
            || (self.restocks_today < MAX_RESTOCKS_PER_DAY
                && self.last_restock + RESTOCK_COOLDOWN < game_time)
    }

    pub fn needs_to_restock(offers: &MerchantOffers) -> bool {
        offers.iter().any(MerchantOffer::needs_restock)
    }

    pub fn should_restock(
        &mut self,
        offers: &mut MerchantOffers,
        game_time: i64,
        day: i64,
    ) -> bool {
        let overdue = self.last_restock + RESTOCK_INTERVAL < game_time;
        let new_day = self.last_restock_check_day >= 1 && day > self.last_restock_check_day;
        self.last_restock_check_day = day;
        if overdue || new_day {
            self.last_restock = game_time;
            self.reset_number_of_restocks(offers);
        }
        self.allowed_to_restock(game_time) && Self::needs_to_restock(offers)
    }

    fn reset_number_of_restocks(&mut self, offers: &mut MerchantOffers) {
        self.catch_up_demand(offers);
        self.restocks_today = 0;
    }

    fn catch_up_demand(&mut self, offers: &mut MerchantOffers) {
        let missed = MAX_RESTOCKS_PER_DAY - self.restocks_today;
        if missed > 0 {
            for offer in offers.iter_mut() {
                offer.reset_uses();
            }
        }
        for _ in 0..missed {
            for offer in offers.iter_mut() {
                offer.update_demand();
            }
        }
    }

    pub fn restock(&mut self, offers: &mut MerchantOffers, game_time: i64) {
        for offer in offers.iter_mut() {
            offer.update_demand();
        }
        for offer in offers.iter_mut() {
            offer.reset_uses();
        }
        self.last_restock = game_time;
        self.restocks_today += 1;
    }

    pub fn reward_trade_xp(
        &mut self,
        offer: &MerchantOffer,
        level: i32,
        random: &mut impl RandomSource,
    ) -> TradeReward {
        let mut orb = random.next_int(4) + 3;
        self.xp += offer.xp;
        let level_up = self.should_increase_level(level);
        if level_up {
            orb += 5;
        }
        TradeReward {
            level_up,
            orb_xp: offer.reward_exp.then_some(orb),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeReward {
    pub level_up: bool,
    pub orb_xp: Option<i32>,
}

pub fn define(base: &mut EntityBase) -> VillagerState {
    base.data.define(DATA_UNHAPPY_COUNTER, SyncedValue::Int(0));
    base.data.define(
        DATA_VILLAGER_DATA,
        SyncedValue::VillagerData {
            kind: DEFAULT_TYPE,
            profession: Profession::None.index(),
            level: MIN_LEVEL,
        },
    );
    base.data
        .define(DATA_VILLAGER_DATA_FINALIZED, SyncedValue::Boolean(false));
    VillagerState::default()
}

pub fn villager_data(base: &EntityBase) -> (u32, Profession, i32) {
    let (kind, profession, level) =
        base.data
            .villager_data(DATA_VILLAGER_DATA)
            .unwrap_or((DEFAULT_TYPE, 0, MIN_LEVEL));
    (kind, Profession::from_index(profession), level)
}

pub fn set_villager_data(base: &mut EntityBase, kind: u32, profession: Profession, level: i32) {
    base.data.set(
        DATA_VILLAGER_DATA,
        SyncedValue::VillagerData {
            kind,
            profession: profession.index(),
            level: level.max(MIN_LEVEL),
        },
    );
}

pub fn profession(base: &EntityBase) -> Profession {
    villager_data(base).1
}

pub fn level(base: &EntityBase) -> i32 {
    villager_data(base).2
}

pub fn is_finalized(base: &EntityBase) -> bool {
    base.data.boolean(DATA_VILLAGER_DATA_FINALIZED)
}

pub fn set_finalized(base: &mut EntityBase, finalized: bool) {
    base.data.set(
        DATA_VILLAGER_DATA_FINALIZED,
        SyncedValue::Boolean(finalized),
    );
}

pub fn unhappy_counter(base: &EntityBase) -> i32 {
    base.data.int(DATA_UNHAPPY_COUNTER)
}

pub fn set_unhappy_counter(base: &mut EntityBase, value: i32) {
    base.data.set(DATA_UNHAPPY_COUNTER, SyncedValue::Int(value));
}

pub fn finalize_type(base: &mut EntityBase, biome: BiomeId) {
    if is_finalized(base) {
        return;
    }
    let (_, profession, level) = villager_data(base);
    set_villager_data(base, type_for_biome(biome), profession, level);
    set_finalized(base, true);
}

pub fn finalize(
    base: &mut EntityBase,
    living: &mut crate::entity::living::LivingState,
    environment: &crate::entity::spawn::SpawnEnvironment,
    reason: crate::spawn::SpawnReason,
    group: Option<crate::spawn::SpawnGroupData>,
    level: &mut impl RandomSource,
) -> Option<crate::spawn::SpawnGroupData> {
    let group = group.unwrap_or(crate::spawn::SpawnGroupData::ageable(false));
    if reason == crate::spawn::SpawnReason::Breeding {
        let (kind, _, level_value) = villager_data(base);
        if let Some(merchant) = living.mob.as_mut().and_then(|mob| mob.merchant.as_mut()) {
            set_profession(base, merchant, Profession::None, level_value);
        } else {
            set_villager_data(base, kind, Profession::None, level_value);
        }
    }
    finalize_type(base, environment.biome);
    crate::entity::spawn::ageable_finalize(base, living, Some(group), level)
}

pub fn set_profession(
    base: &mut EntityBase,
    merchant: &mut MerchantState,
    profession: Profession,
    level: i32,
) {
    let (kind, previous, _) = villager_data(base);
    if previous != profession {
        merchant.offers = None;
    }
    set_villager_data(base, kind, profession, level);
}

pub fn read(
    nbt: &Compound,
    base: &mut EntityBase,
    state: &mut VillagerState,
    merchant: &mut MerchantState,
) {
    let parsed = nbt.get_compound("VillagerData").and_then(parse_data);
    if nbt.get_bool("VillagerDataFinalized").unwrap_or(false) || parsed.is_some() {
        set_finalized(base, true);
        let (kind, profession, level) =
            parsed.unwrap_or((DEFAULT_TYPE, Profession::None.index(), MIN_LEVEL));
        set_villager_data(base, kind, Profession::from_index(profession), level);
    }
    state.food_level = nbt.get_i64("FoodLevel").map_or(0, |v| i32::from(v as i8));
    merchant.gossips = nbt
        .get("Gossips")
        .map(GossipContainer::from_nbt)
        .unwrap_or_default();
    state.xp = nbt.get_i32("Xp").unwrap_or(0);
    state.last_restock = nbt.get_i64("LastRestock").unwrap_or(0);
    state.last_gossip_decay = nbt.get_i64("LastGossipDecay").unwrap_or(0);
    state.restocks_today = nbt.get_i32("RestocksToday").unwrap_or(0);
    merchant.offers = nbt.get_compound("Offers").map(MerchantOffers::from_nbt);
    merchant.load_inventory(nbt.get("Inventory"));
}

pub fn write(
    base: &EntityBase,
    state: &VillagerState,
    merchant: &MerchantState,
    out: &mut Compound,
) {
    let (kind, profession, level) = villager_data(base);
    out.insert(
        "VillagerData",
        Tag::Compound(data_compound(kind, profession.index(), level)),
    );
    out.insert("VillagerDataFinalized", is_finalized(base));
    out.insert("FoodLevel", state.food_level as i8);
    out.insert("Gossips", merchant.gossips.to_nbt());
    out.insert("Xp", state.xp);
    out.insert("LastRestock", state.last_restock);
    out.insert("LastGossipDecay", state.last_gossip_decay);
    out.insert("RestocksToday", state.restocks_today);
    if let Some(offers) = &merchant.offers {
        out.insert("Offers", offers.to_nbt());
    }
    out.insert("Inventory", merchant.inventory_nbt());
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::trading::ItemCost;
    use crate::random::LegacyRandom;

    fn base() -> EntityBase {
        EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Villager)
    }

    fn stack(kind: ItemKind, count: u8) -> ItemStack {
        ItemStack::new(kind.id(), NonZeroU8::new(count).unwrap())
    }

    #[test]
    fn professions_follow_the_registry_order_and_their_job_sites() {
        assert_eq!(Profession::from_index(9), Profession::Librarian);
        assert_eq!(Profession::Librarian.job_site(), Some(PoiType::Librarian));
        assert_eq!(
            Profession::from_job_site(PoiType::Mason),
            Some(Profession::Mason)
        );
        assert!(Profession::None.acquires_job_site(PoiType::Cleric));
        assert!(!Profession::None.acquires_job_site(PoiType::Home));
        assert!(!Profession::Cleric.acquires_job_site(PoiType::Mason));
        assert!(!Profession::None.holds_job_site(PoiType::Cleric));
        assert!(!Profession::Nitwit.acquires_job_site(PoiType::Cleric));
        assert_eq!(
            Profession::Armorer.trade_set(2).as_deref(),
            Some("minecraft:armorer/level_2")
        );
        assert_eq!(Profession::Nitwit.trade_set(1), None);
    }

    #[test]
    fn the_levelling_thresholds_follow_vanilla() {
        assert!(can_level_up(1) && can_level_up(4) && !can_level_up(5));
        assert_eq!(max_xp_per_level(1), 10);
        assert_eq!(max_xp_per_level(2), 70);
        assert_eq!(max_xp_per_level(4), 250);
        assert_eq!(min_xp_per_level(3), 70);
        assert_eq!(max_xp_per_level(5), 0);
    }

    #[test]
    fn the_data_slots_follow_the_ageable_ones() {
        let mut base = base();
        define(&mut base);
        assert_eq!(base.data.get(18), Some(&SyncedValue::Int(0)));
        assert_eq!(
            base.data.get(19),
            Some(&SyncedValue::VillagerData {
                kind: 2,
                profession: 0,
                level: 1
            })
        );
        assert_eq!(base.data.get(20), Some(&SyncedValue::Boolean(false)));
    }

    #[test]
    fn food_in_the_inventory_drives_breeding() {
        let mut merchant = MerchantState::new();
        let mut state = VillagerState::default();
        assert!(!state.can_breed(&merchant, false, 0));
        merchant.add_item(stack(ItemKind::Bread, 3));
        assert_eq!(merchant.food_points(), 12);
        assert!(state.can_breed(&merchant, false, 0));
        assert!(!state.can_breed(&merchant, true, 0));
        assert!(!state.can_breed(&merchant, false, 100));
        assert!(!VillagerState::has_excess_food(&merchant));
        merchant.add_item(stack(ItemKind::Bread, 3));
        assert!(VillagerState::has_excess_food(&merchant));
        state.eat_and_digest_food(&mut merchant);
        assert_eq!(state.food_level, 0);
        assert_eq!(merchant.food_points(), 24 - 12);
    }

    #[test]
    fn eating_stops_once_the_villager_is_full() {
        let mut merchant = MerchantState::new();
        merchant.add_item(stack(ItemKind::Carrot, 20));
        let mut state = VillagerState::default();
        state.eat_until_full(&mut merchant);
        assert_eq!(state.food_level, 12);
        assert_eq!(merchant.count_item(ItemKind::Carrot), 8);
    }

    #[test]
    fn the_inventory_stacks_and_reports_overflow() {
        let mut merchant = MerchantState::new();
        for _ in 0..8 {
            assert!(merchant.add_item(stack(ItemKind::Wheat, 64)).is_none());
        }
        assert!(!merchant.can_add(&stack(ItemKind::Wheat, 1)));
        let rest = merchant.add_item(stack(ItemKind::Wheat, 5));
        assert_eq!(rest.map(|s| s.count.get()), Some(5));
    }

    #[test]
    fn gossip_decays_once_per_day_and_the_first_call_stamps_the_time() {
        let mut state = VillagerState::default();
        let mut gossips = GossipContainer::default();
        gossips.add(
            Uuid::from_u128(5),
            crate::entity::gossip::GossipType::MinorNegative,
            40,
        );
        state.maybe_decay_gossip(&mut gossips, 100);
        assert_eq!(state.last_gossip_decay, 100);
        assert_eq!(gossips.reputation(Uuid::from_u128(5)), -40);
        state.maybe_decay_gossip(&mut gossips, 100 + 23_999);
        assert_eq!(gossips.reputation(Uuid::from_u128(5)), -40);
        state.maybe_decay_gossip(&mut gossips, 100 + 24_000);
        assert_eq!(gossips.reputation(Uuid::from_u128(5)), -20);
    }

    #[test]
    fn gossip_is_rate_limited_to_one_exchange_per_1200_ticks() {
        let state = VillagerState {
            last_gossip_time: 5_000,
            ..VillagerState::default()
        };
        assert!(!state.gossip_ready(5_000));
        assert!(!state.gossip_ready(6_199));
        assert!(state.gossip_ready(6_200));
        assert!(state.gossip_ready(4_999));
    }

    fn used_offers() -> MerchantOffers {
        let mut offers = MerchantOffers::new();
        let mut offer = MerchantOffer::new(
            ItemCost::new(ItemKind::Emerald.id(), 1),
            None,
            stack(ItemKind::Bread, 2),
            10,
            2,
            0.05,
        );
        offer.uses = 4;
        offers.push(offer);
        offers
    }

    #[test]
    fn restocking_updates_demand_resets_uses_and_counts_the_restock() {
        let mut state = VillagerState::default();
        let mut offers = used_offers();
        state.restock(&mut offers, 9_000);
        let offer = offers.get(0).unwrap();
        assert_eq!(offer.uses, 0);
        assert_eq!(offer.demand, 4 - 6);
        assert_eq!(state.restocks_today, 1);
        assert_eq!(state.last_restock, 9_000);
    }

    #[test]
    fn restocking_is_limited_to_two_per_day_with_a_cooldown() {
        let mut state = VillagerState::default();
        assert!(state.allowed_to_restock(1));
        state.restocks_today = 1;
        state.last_restock = 1_000;
        assert!(!state.allowed_to_restock(3_400));
        assert!(state.allowed_to_restock(3_401));
        state.restocks_today = 2;
        assert!(!state.allowed_to_restock(100_000));
    }

    #[test]
    fn the_restock_check_resets_the_counter_after_twelve_thousand_ticks() {
        let mut state = VillagerState {
            restocks_today: 2,
            last_restock: 100,
            ..VillagerState::default()
        };
        let mut offers = used_offers();
        assert!(!state.should_restock(&mut offers, 5_000, 0));
        assert!(state.should_restock(&mut offers, 12_101, 0));
        assert_eq!(state.restocks_today, 0);
        assert_eq!(state.last_restock, 12_101);
    }

    #[test]
    fn a_new_day_also_resets_the_restock_counter() {
        let mut state = VillagerState {
            restocks_today: 2,
            last_restock: 20_000,
            last_restock_check_day: 1,
            ..VillagerState::default()
        };
        let mut offers = used_offers();
        state.should_restock(&mut offers, 24_100, 2);
        assert_eq!(state.restocks_today, 0);
    }

    #[test]
    fn trade_experience_levels_the_villager_up_at_the_threshold() {
        let mut state = VillagerState {
            xp: 9,
            ..VillagerState::default()
        };
        let offer = used_offers().get(0).unwrap().clone();
        let mut random = LegacyRandom::new(2);
        let reward = state.reward_trade_xp(&offer, 1, &mut random);
        assert!(reward.level_up);
        assert_eq!(state.xp, 11);
        assert!(reward.orb_xp.is_some_and(|xp| (8..=11).contains(&xp)));
        let reward = state.reward_trade_xp(&offer, 5, &mut random);
        assert!(!reward.level_up);
        assert!(reward.orb_xp.is_some_and(|xp| (3..=6).contains(&xp)));
    }

    #[test]
    fn villager_state_round_trips_through_the_vanilla_save_layout() {
        let mut base = base();
        let mut state = define(&mut base);
        let mut merchant = MerchantState::new();
        let owner = Uuid::from_u128(33);
        let nbt = Compound::new()
            .with(
                "VillagerData",
                Compound::new()
                    .with("type", "minecraft:snow")
                    .with("profession", "minecraft:farmer")
                    .with("level", 4),
            )
            .with("FoodLevel", 7i8)
            .with("Xp", 160)
            .with("LastRestock", 55i64)
            .with("LastGossipDecay", 66i64)
            .with("RestocksToday", 1)
            .with("Offers", used_offers().to_nbt());
        read(&nbt, &mut base, &mut state, &mut merchant);
        merchant
            .gossips
            .add(owner, crate::entity::gossip::GossipType::Trading, 9);
        merchant.add_item(stack(ItemKind::Bread, 3));
        assert_eq!(villager_data(&base), (4, Profession::Farmer, 4));
        assert_eq!(state.food_level, 7);
        let mut out = Compound::new();
        write(&base, &state, &merchant, &mut out);
        assert_eq!(out.get_i64("LastRestock"), Some(55));
        assert_eq!(out.get_i64("LastGossipDecay"), Some(66));
        assert_eq!(out.get_i32("RestocksToday"), Some(1));
        assert_eq!(out.get_i32("Xp"), Some(160));
        assert_eq!(out.get_bool("VillagerDataFinalized"), Some(true));
        assert!(out.get_compound("Offers").is_some());
        let mut again = MerchantState::new();
        let mut again_state = VillagerState::default();
        let mut other = EntityBase::new(EntityId(2), Uuid::from_u128(2), EntityKind::Villager);
        define(&mut other);
        read(&out, &mut other, &mut again_state, &mut again);
        assert_eq!(again, merchant);
        assert_eq!(again_state, state);
        assert_eq!(villager_data(&other), villager_data(&base));
    }

    #[test]
    fn changing_profession_discards_the_offers_but_keeping_it_does_not() {
        let mut base = base();
        define(&mut base);
        let mut merchant = MerchantState::new();
        merchant.offers = Some(used_offers());
        set_profession(&mut base, &mut merchant, Profession::None, 1);
        assert!(merchant.offers.is_some());
        set_profession(&mut base, &mut merchant, Profession::Mason, 1);
        assert!(merchant.offers.is_none());
        assert_eq!(profession(&base), Profession::Mason);
    }

    #[test]
    fn the_biome_type_is_chosen_once() {
        let mut base = base();
        define(&mut base);
        let desert = BiomeId(rspigot_data::registries::biome_id("minecraft:desert").unwrap());
        finalize_type(&mut base, desert);
        assert_eq!(villager_data(&base).0, 0);
        let snow = BiomeId(rspigot_data::registries::biome_id("minecraft:snowy_plains").unwrap());
        finalize_type(&mut base, snow);
        assert_eq!(villager_data(&base).0, 0);
    }
}
