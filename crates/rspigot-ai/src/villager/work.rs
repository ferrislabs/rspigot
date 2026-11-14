use rspigot_blocks::BlockKind;
use rspigot_domain::entity::species::villager::Profession;
use rspigot_domain::{BlockPos, ItemId, crop};
use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;

use crate::action::MobAction;
use crate::behavior::{
    Behavior, BehaviorCx, Condition, MemoryModuleType, MemoryStatus, MemoryValue, PositionTracker,
    WalkTarget,
};
use crate::random::RandomSource;
use crate::scan::closer_to_center_than;
use crate::villager::VillagerAction;
use crate::villager::support::set_walk_and_look_targets_entity;
use crate::world::AiWorld;

const WORK_CHECK_INTERVAL: i64 = 300;
const WORK_DISTANCE: f64 = 1.73;
const SHOW_DISPLAY_TICKS: i32 = 900;
const SHOW_START_LOOK_TICKS: i32 = 40;
const SHOW_CYCLE_TICKS: i32 = 40;
const SHOW_MAX_DISTANCE_SQR: f64 = 17.0;
const GIFT_TIMEOUT: i32 = 100;
const GIFT_DELAY: i32 = 600;
const GIFT_DELAY_BOUND: i32 = 6001;
const GIFT_THROW_DISTANCE: f64 = 5.0;
const GIFT_THROW_WAIT: i64 = 20;
const GIFT_WALK_SPEED: f32 = 0.5;
const GIFT_WALK_CLOSE: i32 = 5;
const HARVEST_MAX_TICKS: i32 = 200;
const HARVEST_START_DELAY: i64 = 40;
const HARVEST_NEXT_CROP_DELAY: i64 = 20;
const HARVEST_SPEED: f32 = 0.5;
const HARVEST_DISTANCE: f64 = 1.0;
const BONEMEAL_SPEED: f32 = 0.5;
const BONEMEAL_TICK_INTERVAL: i32 = 10;
const BONEMEAL_COOLDOWN: i32 = 160;
const BONEMEAL_MAX_TICKS: i32 = 80;
const BONEMEAL_DISTANCE: f64 = 1.0;
const BONEMEAL_NEXT_CYCLE_DELAY: i64 = 40;
pub const BONEMEAL_EVENT: i32 = rspigot_domain::composter::BONE_MEAL_EVENT;
const BONEMEAL_EVENT_DATA: i32 = 15;

pub fn is_mature_crop<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
    world.state(pos).is_some_and(crop::is_max_age)
}

fn is_crop<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
    world.state(pos).is_some_and(crop::is_crop)
}

fn is_air<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
    world.state(pos).is_some_and(|state| state.is_air())
}

fn is_farmland<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
    world
        .state(pos)
        .and_then(|state| state.kind())
        .is_some_and(|kind| kind == BlockKind::Farmland)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Workstation {
    Plain,
    Composter,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkAtPoi {
    last_check: i64,
    workstation: Workstation,
}

impl WorkAtPoi {
    const CONDITION: [Condition; 2] = [
        (MemoryModuleType::JobSite, MemoryStatus::ValuePresent),
        (MemoryModuleType::LookTarget, MemoryStatus::Registered),
    ];

    pub fn new(workstation: Workstation) -> Self {
        Self {
            last_check: 0,
            workstation,
        }
    }

    fn near_job_site<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> bool {
        cx.brain
            .memories
            .get(MemoryModuleType::JobSite)
            .and_then(MemoryValue::as_global_pos)
            .is_some_and(|global| {
                global.dimension == cx.ai.world.dimension()
                    && closer_to_center_than(global.pos, cx.ai.mob.position, WORK_DISTANCE)
            })
    }
}

impl Behavior for WorkAtPoi {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        let game_time = cx.ai.world.game_time();
        if game_time - self.last_check < WORK_CHECK_INTERVAL {
            return false;
        }
        if cx.ai.brain.random.next_int(2) != 0 {
            return false;
        }
        self.last_check = game_time;
        Self::near_job_site(cx)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        cx.brain
            .memories
            .set(MemoryModuleType::LastWorkedAtPoi, MemoryValue::Long(now));
        let pos = cx
            .brain
            .memories
            .get(MemoryModuleType::JobSite)
            .and_then(MemoryValue::as_global_pos)
            .map(|global| global.pos);
        if let Some(pos) = pos {
            cx.brain.memories.set(
                MemoryModuleType::LookTarget,
                MemoryValue::Tracker(PositionTracker::of_block(pos)),
            );
        }
        cx.ai
            .act(MobAction::Villager(VillagerAction::PlayWorkSound));
        if let Some(pos) = pos
            && self.workstation == Workstation::Composter
        {
            cx.ai
                .act(MobAction::Villager(VillagerAction::WorkAtComposter(pos)));
        }
        cx.ai.act(MobAction::Villager(VillagerAction::Restock));
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        Self::near_job_site(cx)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShowTradesToPlayer {
    display: Vec<usize>,
    player_item: Option<Option<rspigot_domain::ItemId>>,
    cycle_counter: i32,
    display_index: usize,
    look_time: i32,
}

impl ShowTradesToPlayer {
    const CONDITION: [Condition; 1] = [(
        MemoryModuleType::InteractionTarget,
        MemoryStatus::ValuePresent,
    )];

    pub fn new() -> Self {
        Self {
            display: Vec::new(),
            player_item: None,
            cycle_counter: 0,
            display_index: 0,
            look_time: 0,
        }
    }

    fn player<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<crate::world::Living> {
        let id = cx
            .brain
            .memories
            .get(MemoryModuleType::InteractionTarget)
            .and_then(MemoryValue::as_entity)?;
        cx.ai.world.living(id)
    }

    fn look_at_target<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) -> Option<crate::world::Living> {
        let player = Self::player(cx)?;
        cx.brain.memories.set(
            MemoryModuleType::LookTarget,
            MemoryValue::Tracker(PositionTracker::of_entity(player.id, true)),
        );
        Some(player)
    }

    fn matches_cost<W: AiWorld>(
        &self,
        cx: &BehaviorCx<'_, '_, W>,
        item: rspigot_domain::ItemId,
        trade: &crate::villager::TradeView,
    ) -> bool {
        let _ = cx;
        trade.cost_a == item || trade.cost_b == Some(item)
    }

    fn update_display_items<W: AiWorld>(
        &mut self,
        cx: &BehaviorCx<'_, '_, W>,
        item: rspigot_domain::ItemId,
    ) {
        let trades = cx.ai.world.villager_trades(cx.ai.mob.id);
        for (index, trade) in trades.iter().enumerate() {
            if !trade.out_of_stock && self.matches_cost(cx, item, trade) {
                self.display.push(index);
            }
        }
    }

    fn find_items_to_display<W: AiWorld>(
        &mut self,
        cx: &mut BehaviorCx<'_, '_, W>,
        player: &crate::world::Living,
    ) {
        let held = player.main_hand;
        let mut changed = false;
        if self.player_item != Some(held) {
            self.player_item = Some(held);
            changed = true;
            self.display.clear();
        }
        if changed && let Some(item) = held.filter(|item| *item != rspigot_domain::ItemId::AIR) {
            self.update_display_items(cx, item);
            if !self.display.is_empty() {
                self.look_time = SHOW_DISPLAY_TICKS;
                cx.ai
                    .act(MobAction::Villager(VillagerAction::ShowTrade(Some(
                        self.display[0],
                    ))));
            }
        }
    }

    fn display_cycling_items<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) {
        if self.display.len() > 1 {
            self.cycle_counter += 1;
            if self.cycle_counter >= SHOW_CYCLE_TICKS {
                self.display_index += 1;
                self.cycle_counter = 0;
                if self.display_index > self.display.len() - 1 {
                    self.display_index = 0;
                }
                cx.ai
                    .act(MobAction::Villager(VillagerAction::ShowTrade(Some(
                        self.display[self.display_index],
                    ))));
            }
        }
    }
}

impl Default for ShowTradesToPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl Behavior for ShowTradesToPlayer {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        let Some(player) = Self::player(cx) else {
            return false;
        };
        player.kind == EntityKind::Player
            && player.alive
            && !cx.ai.mob.baby
            && player.position.distance_squared(cx.ai.mob.position) <= SHOW_MAX_DISTANCE_SQR
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        self.check_extra_start_conditions(cx)
            && 0 < self.look_time
            && cx
                .brain
                .memories
                .has_value(MemoryModuleType::InteractionTarget)
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        Self::look_at_target(cx);
        self.cycle_counter = 0;
        self.display_index = 0;
        self.look_time = SHOW_START_LOOK_TICKS;
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        let Some(player) = Self::look_at_target(cx) else {
            return;
        };
        self.find_items_to_display(cx, &player);
        if self.display.is_empty() {
            cx.ai
                .act(MobAction::Villager(VillagerAction::ShowTrade(None)));
            self.look_time = self.look_time.min(SHOW_START_LOOK_TICKS);
        } else {
            self.display_cycling_items(cx);
        }
        self.look_time -= 1;
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.brain.memories.erase(MemoryModuleType::InteractionTarget);
        cx.ai
            .act(MobAction::Villager(VillagerAction::ShowTrade(None)));
        self.player_item = None;
        self.display.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GiveGiftToHero {
    time_until_next_gift: i32,
    gift_given: bool,
    time_since_start: i64,
}

impl GiveGiftToHero {
    const CONDITION: [Condition; 4] = [
        (MemoryModuleType::WalkTarget, MemoryStatus::Registered),
        (MemoryModuleType::LookTarget, MemoryStatus::Registered),
        (
            MemoryModuleType::InteractionTarget,
            MemoryStatus::Registered,
        ),
        (
            MemoryModuleType::NearestVisiblePlayer,
            MemoryStatus::ValuePresent,
        ),
    ];

    pub const TIMEOUT: i32 = GIFT_TIMEOUT;

    pub fn new() -> Self {
        Self {
            time_until_next_gift: GIFT_DELAY,
            gift_given: false,
            time_since_start: 0,
        }
    }

    fn hero<W: AiWorld>(cx: &BehaviorCx<'_, '_, W>) -> Option<crate::world::Living> {
        let id = cx
            .brain
            .memories
            .get(MemoryModuleType::NearestVisiblePlayer)
            .and_then(MemoryValue::as_entity)?;
        cx.ai
            .world
            .living(id)
            .filter(|player| player.facts.hero_of_the_village)
    }
}

impl Default for GiveGiftToHero {
    fn default() -> Self {
        Self::new()
    }
}

impl Behavior for GiveGiftToHero {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        if Self::hero(cx).is_none() {
            return false;
        }
        if self.time_until_next_gift < 1 {
            return true;
        }
        self.time_until_next_gift -= 1;
        false
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        self.gift_given = false;
        self.time_since_start = now;
        if let Some(hero) = Self::hero(cx) {
            cx.brain.memories.set(
                MemoryModuleType::InteractionTarget,
                MemoryValue::Entity(hero.id),
            );
            crate::behavior::look_at_entity(cx, hero.id);
        }
    }

    fn can_still_use<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        Self::hero(cx).is_some() && !self.gift_given
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        let Some(hero) = Self::hero(cx) else {
            return;
        };
        crate::behavior::look_at_entity(cx, hero.id);
        let here = cx.ai.mob.block_pos();
        let there = hero.position.block();
        let distance = f64::from(
            (here.x - there.x).pow(2) + (here.y - there.y).pow(2) + (here.z - there.z).pow(2),
        );
        if distance < GIFT_THROW_DISTANCE * GIFT_THROW_DISTANCE {
            if now - self.time_since_start > GIFT_THROW_WAIT {
                cx.ai
                    .act(MobAction::Villager(VillagerAction::GiveGift(hero.id)));
                self.gift_given = true;
            }
        } else {
            set_walk_and_look_targets_entity(cx, hero.id, GIFT_WALK_SPEED, GIFT_WALK_CLOSE);
        }
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        self.time_until_next_gift = GIFT_DELAY + cx.ai.brain.random.next_int(GIFT_DELAY_BOUND);
        cx.brain.memories.erase(MemoryModuleType::InteractionTarget);
        cx.brain.memories.erase(MemoryModuleType::WalkTarget);
        cx.brain.memories.erase(MemoryModuleType::LookTarget);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HarvestFarmland {
    valid_farmland: Vec<BlockPos>,
    above_farmland: Option<BlockPos>,
    next_ok_start_time: i64,
    time_worked_so_far: i32,
}

impl HarvestFarmland {
    const CONDITION: [Condition; 3] = [
        (MemoryModuleType::LookTarget, MemoryStatus::ValueAbsent),
        (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
        (
            MemoryModuleType::SecondaryJobSite,
            MemoryStatus::ValuePresent,
        ),
    ];

    pub fn new() -> Self {
        Self {
            valid_farmland: Vec::new(),
            above_farmland: None,
            next_ok_start_time: 0,
            time_worked_so_far: 0,
        }
    }

    fn valid_pos<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
        is_mature_crop(world, pos)
            || (is_air(world, pos) && is_farmland(world, pos.offset(0, -1, 0)))
    }

    fn pick_farmland<W: AiWorld>(&self, cx: &mut BehaviorCx<'_, '_, W>) -> Option<BlockPos> {
        if self.valid_farmland.is_empty() {
            return None;
        }
        let index = cx
            .ai
            .brain
            .random
            .next_int(self.valid_farmland.len() as i32) as usize;
        Some(self.valid_farmland[index])
    }

    fn go_to<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>, pos: BlockPos) {
        cx.brain.memories.set(
            MemoryModuleType::LookTarget,
            MemoryValue::Tracker(PositionTracker::of_block(pos)),
        );
        cx.brain.memories.set(
            MemoryModuleType::WalkTarget,
            MemoryValue::WalkTarget(WalkTarget {
                target: PositionTracker::of_block(pos),
                speed_modifier: HARVEST_SPEED,
                close_enough_dist: 1,
            }),
        );
    }
}

impl Default for HarvestFarmland {
    fn default() -> Self {
        Self::new()
    }
}

impl Behavior for HarvestFarmland {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        if !cx.ai.world.mob_griefing() {
            return false;
        }
        if cx.ai.mob.species.villager.profession != Profession::Farmer {
            return false;
        }
        let origin = cx.ai.mob.position;
        let center = BlockPos::new(
            origin.x.floor() as i32,
            origin.y.floor() as i32,
            origin.z.floor() as i32,
        );
        self.valid_farmland.clear();
        for x in -1..2 {
            for y in -1..2 {
                for z in -1..2 {
                    let pos = center.offset(x, y, z);
                    if Self::valid_pos(cx.ai.world, pos) {
                        self.valid_farmland.push(pos);
                    }
                }
            }
        }
        self.above_farmland = self.pick_farmland(cx);
        self.above_farmland.is_some()
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        if self.next_ok_start_time < now
            && let Some(pos) = self.above_farmland
        {
            Self::go_to(cx, pos);
        }
    }

    fn can_still_use<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        self.time_worked_so_far < HARVEST_MAX_TICKS
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        if let Some(pos) = self.above_farmland
            && !closer_to_center_than(pos, cx.ai.mob.position, HARVEST_DISTANCE)
        {
            return;
        }
        if let Some(pos) = self.above_farmland
            && self.next_ok_start_time < now
        {
            let mature = is_mature_crop(cx.ai.world, pos);
            if mature {
                cx.ai
                    .act(MobAction::Villager(VillagerAction::HarvestCrop(pos)));
            }
            let air_after = mature || is_air(cx.ai.world, pos);
            if air_after
                && is_farmland(cx.ai.world, pos.offset(0, -1, 0))
                && cx.ai.mob.species.villager.has_farm_seeds
            {
                let slot = cx
                    .ai
                    .mob
                    .species
                    .villager
                    .inventory
                    .iter()
                    .position(|stack| {
                        stack.is_some_and(|stack| {
                            rspigot_data::tags::tag(
                                "minecraft:item",
                                "minecraft:villager_plantable_seeds",
                            )
                            .is_some_and(|ids| ids.binary_search(&u32::from(stack.item.0)).is_ok())
                        })
                    });
                if let Some(slot) = slot {
                    cx.ai
                        .act(MobAction::Villager(VillagerAction::PlantSeed { pos, slot }));
                }
            }
            if is_crop(cx.ai.world, pos) && !mature {
                self.valid_farmland.retain(|candidate| *candidate != pos);
                self.above_farmland = self.pick_farmland(cx);
                if let Some(next) = self.above_farmland {
                    self.next_ok_start_time = now + HARVEST_NEXT_CROP_DELAY;
                    cx.brain.memories.set(
                        MemoryModuleType::WalkTarget,
                        MemoryValue::WalkTarget(WalkTarget {
                            target: PositionTracker::of_block(next),
                            speed_modifier: HARVEST_SPEED,
                            close_enough_dist: 1,
                        }),
                    );
                    cx.brain.memories.set(
                        MemoryModuleType::LookTarget,
                        MemoryValue::Tracker(PositionTracker::of_block(next)),
                    );
                }
            }
        }
        self.time_worked_so_far += 1;
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        cx.brain.memories.erase(MemoryModuleType::LookTarget);
        cx.brain.memories.erase(MemoryModuleType::WalkTarget);
        self.time_worked_so_far = 0;
        self.next_ok_start_time = now + HARVEST_START_DELAY;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseBonemeal {
    crop_pos: Option<BlockPos>,
    next_work_cycle_time: i64,
    last_bonemealing_session: i64,
    time_worked_so_far: i32,
}

impl UseBonemeal {
    const CONDITION: [Condition; 2] = [
        (MemoryModuleType::LookTarget, MemoryStatus::ValueAbsent),
        (MemoryModuleType::WalkTarget, MemoryStatus::ValueAbsent),
    ];

    pub fn new() -> Self {
        Self {
            crop_pos: None,
            next_work_cycle_time: 0,
            last_bonemealing_session: 0,
            time_worked_so_far: 0,
        }
    }

    fn bone_meal() -> ItemId {
        ItemKind::BoneMeal.id()
    }

    fn pick_next_target<W: AiWorld>(cx: &mut BehaviorCx<'_, '_, W>) -> Option<BlockPos> {
        let origin = cx.ai.mob.block_pos();
        let mut found = None;
        let mut count = 0;
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    let pos = origin.offset(x, y, z);
                    if Self::valid_pos(cx.ai.world, pos) {
                        count += 1;
                        if cx.ai.brain.random.next_int(count) == 0 {
                            found = Some(pos);
                        }
                    }
                }
            }
        }
        found
    }

    fn valid_pos<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
        world.state(pos).is_some_and(crop::is_valid_bonemeal_target)
    }

    fn set_current_crop_as_target<W: AiWorld>(&self, cx: &mut BehaviorCx<'_, '_, W>) {
        if let Some(pos) = self.crop_pos {
            cx.brain.memories.set(
                MemoryModuleType::LookTarget,
                MemoryValue::Tracker(PositionTracker::of_block(pos)),
            );
            cx.brain.memories.set(
                MemoryModuleType::WalkTarget,
                MemoryValue::WalkTarget(WalkTarget {
                    target: PositionTracker::of_block(pos),
                    speed_modifier: BONEMEAL_SPEED,
                    close_enough_dist: 1,
                }),
            );
        }
    }
}

impl Default for UseBonemeal {
    fn default() -> Self {
        Self::new()
    }
}

impl Behavior for UseBonemeal {
    fn entry_condition(&self) -> &[Condition] {
        &Self::CONDITION
    }

    fn check_extra_start_conditions<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>) -> bool {
        let tick_count = cx.ai.mob.tick_count;
        if tick_count % BONEMEAL_TICK_INTERVAL != 0 {
            return false;
        }
        if self.last_bonemealing_session != 0
            && i64::from(tick_count) < self.last_bonemealing_session + i64::from(BONEMEAL_COOLDOWN)
        {
            return false;
        }
        if cx.ai.mob.species.villager.count_item(Self::bone_meal()) < 1 {
            return false;
        }
        self.crop_pos = Self::pick_next_target(cx);
        self.crop_pos.is_some()
    }

    fn can_still_use<W: AiWorld>(&mut self, _cx: &mut BehaviorCx<'_, '_, W>, _now: i64) -> bool {
        self.time_worked_so_far < BONEMEAL_MAX_TICKS && self.crop_pos.is_some()
    }

    fn start<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        self.set_current_crop_as_target(cx);
        cx.ai
            .act(MobAction::Villager(VillagerAction::SetMainHand(Some(
                Self::bone_meal(),
            ))));
        self.next_work_cycle_time = now;
        self.time_worked_so_far = 0;
    }

    fn tick<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, now: i64) {
        let Some(pos) = self.crop_pos else {
            return;
        };
        if now >= self.next_work_cycle_time
            && closer_to_center_than(pos, cx.ai.mob.position, BONEMEAL_DISTANCE)
        {
            let has_bone_meal = cx.ai.mob.species.villager.count_item(Self::bone_meal()) > 0;
            if has_bone_meal && Self::valid_pos(cx.ai.world, pos) {
                cx.ai
                    .act(MobAction::Villager(VillagerAction::UseBoneMeal(pos)));
                cx.ai.act(MobAction::LevelEvent {
                    event: BONEMEAL_EVENT,
                    pos,
                    data: BONEMEAL_EVENT_DATA,
                });
                self.crop_pos = Self::pick_next_target(cx);
                self.set_current_crop_as_target(cx);
                self.next_work_cycle_time = now + BONEMEAL_NEXT_CYCLE_DELAY;
            }
            self.time_worked_so_far += 1;
        }
    }

    fn stop<W: AiWorld>(&mut self, cx: &mut BehaviorCx<'_, '_, W>, _now: i64) {
        cx.ai
            .act(MobAction::Villager(VillagerAction::SetMainHand(None)));
        self.last_bonemealing_session = i64::from(cx.ai.mob.tick_count);
    }
}
