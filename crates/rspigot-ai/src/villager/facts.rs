use rspigot_domain::entity::species::villager::{Profession, food_nutrition};
use rspigot_domain::{BlockPos, EntityId, ItemId};
use rspigot_items::ItemKind;

pub const INVENTORY_SLOTS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackFacts {
    pub item: ItemId,
    pub count: u8,
}

impl StackFacts {
    pub fn kind(&self) -> Option<ItemKind> {
        self.item.kind()
    }

    pub fn nutrition(&self) -> i32 {
        self.kind().map_or(0, food_nutrition)
    }

    pub fn max_stack(&self) -> i32 {
        i32::from(self.item.max_stack_size())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VillagerFacts {
    pub profession: Profession,
    pub level: i32,
    pub xp: i32,
    pub food_level: i32,
    pub food_points: i32,
    pub wheat: i32,
    pub has_farm_seeds: bool,
    pub can_breed: bool,
    pub sleeping: bool,
    pub trading_player: Option<EntityId>,
    pub gossip_ready: bool,
    pub hurt_recently: bool,
    pub inventory: [Option<StackFacts>; INVENTORY_SLOTS],
    pub trade_count: usize,
    pub job_site: Option<BlockPos>,
    pub has_potential_job_site: bool,
    pub interaction_target: Option<EntityId>,
    pub has_job_site_poi: bool,
}

impl VillagerFacts {
    pub fn wants_more_food(&self) -> bool {
        self.food_points < rspigot_domain::entity::species::villager::BREEDING_FOOD_THRESHOLD
    }

    pub fn has_excess_food(&self) -> bool {
        self.food_points > rspigot_domain::entity::species::villager::FOOD_EXCESS_THRESHOLD
    }

    pub fn can_add_item(&self, item: ItemId) -> bool {
        self.inventory.iter().any(|slot| match slot {
            None => true,
            Some(stack) => stack.item == item && stack.count < stack.item.max_stack_size(),
        })
    }

    pub fn is_farmer(&self) -> bool {
        self.profession == Profession::Farmer
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VillagerPeerFacts {
    pub profession: Profession,
    pub xp: i32,
    pub can_breed: bool,
    pub wants_more_food: bool,
    pub sleeping: bool,
    pub job_site: Option<BlockPos>,
    pub has_potential_job_site: bool,
    pub interaction_target: Option<EntityId>,
}

impl VillagerFacts {
    pub fn peer(&self) -> VillagerPeerFacts {
        VillagerPeerFacts {
            profession: self.profession,
            xp: self.xp,
            can_breed: self.can_breed,
            wants_more_food: self.wants_more_food(),
            sleeping: self.sleeping,
            job_site: self.job_site,
            has_potential_job_site: self.has_potential_job_site,
            interaction_target: self.interaction_target,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradeView {
    pub cost_a: ItemId,
    pub cost_b: Option<ItemId>,
    pub out_of_stock: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RaidFacts {
    pub active: bool,
    pub over: bool,
    pub victory: bool,
    pub loss: bool,
    pub stopped: bool,
    pub first_wave_spawned: bool,
    pub between_waves: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VillagerAction {
    TakePoi(BlockPos),
    ReleasePoi(BlockPos),
    SetProfession(Profession),
    RefreshBrain,
    PlayWorkSound,
    Restock,
    WorkAtComposter(BlockPos),
    ShowTrade(Option<usize>),
    ThrowSlot {
        slot: usize,
        amount: u8,
        target: rspigot_domain::Vec3,
    },
    GiveGift(EntityId),
    Gossip(EntityId),
    EatAndDigest,
    EatAndDigestPartner(EntityId),
    PartnerEvent {
        partner: EntityId,
        event: i8,
    },
    Breed {
        partner: EntityId,
        bed: BlockPos,
    },
    StartSleeping(BlockPos),
    StopSleeping,
    RingBell(BlockPos),
    SpawnGolem {
        timestamp: i64,
        villagers_needed: i32,
    },
    HarvestCrop(BlockPos),
    PlantSeed {
        pos: BlockPos,
        slot: usize,
    },
    ClearWanderTarget,
}
