use rspigot_domain::poi::Occupancy;
use rspigot_domain::{BlockPos, EntityId, SectionPos, Vec3};
use rspigot_entities::{EntityKind, EntityTypeId};

use crate::action::{MobAction, TargetReason};
use crate::brain::AiContext;
use crate::combat::{HurtByTargetGoal, MeleeAttackGoal, TargetMemory};
use crate::goal::{Flag, Flags, Goal, adjusted_tick_delay};
use crate::goals::{MobGoal, NearestAttackableTargetGoal, RandomStrollGoal, TargetFilter};
use crate::neutral::ResetUniversalAngerGoal;
use crate::random::RandomSource;
use crate::random_pos::{HALF_PI, land_pos, land_pos_towards, pos_towards};
use crate::registry::Goals;
use crate::targeting::{self, TargetingConditions};
use crate::village;
use crate::world::{AiWorld, Living, TargetKind, inflate};

pub const OFFER_TICKS: i32 = 400;
pub const POPPY_OFFER_RANGE: f64 = 6.0;
const ENTITY_TAGS: &str = "minecraft:entity_type";
const CANDIDATE_FOR_GIFT: &str = "minecraft:candidate_for_iron_golem_gift";
const ACCEPTS_GIFT: &str = "minecraft:accepts_iron_golem_gift";
const ENEMY_CLASSES: [&str; 8] = [
    "Monster",
    "Slime",
    "MagmaCube",
    "Ghast",
    "Phantom",
    "EnderDragon",
    "Shulker",
    "Hoglin",
];
const MOVE_TOWARDS_SPEED: f64 = 0.9;
const MOVE_TOWARDS_WITHIN: f32 = 32.0;
const VILLAGE_SPEED: f64 = 0.6;
const MOVE_BACK_INTERVAL: i32 = 10;
const VILLAGE_STROLL_INTERVAL: i32 = 240;
const POI_SECTION_SCAN_RADIUS: i32 = 2;
const POI_RANGE: i32 = 8;
const VILLAGER_SCAN_RADIUS: f64 = 32.0;
const RANDOM_POS_XZ: i32 = 10;
const RANDOM_POS_Y: i32 = 7;
const WANDER_ANYWHERE_CHANCE: f32 = 0.3;
const VILLAGER_FIRST_CHANCE: f32 = 0.7;
const OFFER_CHANCE: i32 = 8000;
const OFFER_LOOK: f32 = 30.0;
const DEFEND_REPUTATION: i32 = -100;
const DEFEND_RANGE: f64 = 64.0;

pub fn is_enemy(kind: EntityKind) -> bool {
    ENEMY_CLASSES.iter().any(|class| kind.extends(class))
}

fn tagged_kinds(tag: &'static str) -> impl Iterator<Item = EntityKind> {
    rspigot_data::tags::tag(ENTITY_TAGS, tag)
        .into_iter()
        .flatten()
        .filter_map(|id| u16::try_from(*id).ok())
        .filter_map(|id| EntityKind::from_id(EntityTypeId(id)))
}

pub fn accepts_gift(kind: EntityKind) -> bool {
    tagged_kinds(ACCEPTS_GIFT).any(|accepted| accepted == kind)
}

fn contains(area: &rspigot_domain::entity::Aabb, position: Vec3) -> bool {
    position.x >= area.min_x
        && position.x < area.max_x
        && position.y >= area.min_y
        && position.y < area.max_y
        && position.z >= area.min_z
        && position.z < area.max_z
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IronGolemFacts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IronGolemAction {
    OfferFlower(bool),
    GiftPoppy(EntityId),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveTowardsTargetGoal {
    speed_modifier: f64,
    within: f32,
    target: Option<EntityId>,
    wanted: Vec3,
}

impl MoveTowardsTargetGoal {
    pub const fn new(speed_modifier: f64, within: f32) -> Self {
        Self {
            speed_modifier,
            within,
            target: None,
            wanted: Vec3::ZERO,
        }
    }

    fn reach_sqr(&self) -> f64 {
        f64::from(self.within * self.within)
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(target) = context.target() else {
            self.target = None;
            return false;
        };
        self.target = Some(target.id);
        if context.mob.distance_to_sqr(target.position) > self.reach_sqr() {
            return false;
        }
        let Some(pos) = pos_towards(context, 16, 7, target.position, HALF_PI) else {
            return false;
        };
        self.wanted = pos;
        true
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if context.brain.navigation.is_done() {
            return false;
        }
        let Some(target) = self.target.and_then(|id| context.world.living(id)) else {
            return false;
        };
        target.alive && context.mob.distance_to_sqr(target.position) < self.reach_sqr()
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.move_to_pos(self.wanted, self.speed_modifier);
    }

    fn stop(&mut self) {
        self.target = None;
    }
}

fn move_back_position<W: AiWorld>(context: &mut AiContext<'_, W>) -> Option<Vec3> {
    let section = SectionPos::of(context.mob.block_pos());
    let optimal =
        village::find_section_closest_to_village(context.world, section, POI_SECTION_SCAN_RADIUS);
    if optimal == section {
        return None;
    }
    pos_towards(
        context,
        RANDOM_POS_XZ,
        RANDOM_POS_Y,
        optimal.center().center(),
        HALF_PI,
    )
}

fn pick<T: Copy, W: AiWorld>(context: &mut AiContext<'_, W>, items: &[T]) -> T {
    let index = context.brain.random.next_int(items.len() as i32) as usize;
    items[index]
}

fn position_towards_villager<W: AiWorld>(context: &mut AiContext<'_, W>) -> Option<Vec3> {
    let area = inflate(
        &context.mob.bounds(),
        VILLAGER_SCAN_RADIUS,
        VILLAGER_SCAN_RADIUS,
        VILLAGER_SCAN_RADIUS,
    );
    let villagers = context.world.villagers_wanting_golem(&area);
    if villagers.is_empty() {
        return None;
    }
    let villager = pick(context, &villagers);
    land_pos_towards(context, RANDOM_POS_XZ, RANDOM_POS_Y, villager.position)
}

fn position_towards_poi<W: AiWorld>(context: &mut AiContext<'_, W>) -> Option<Vec3> {
    let origin = SectionPos::of(context.mob.block_pos());
    let sections: Vec<SectionPos> = origin
        .cube(POI_SECTION_SCAN_RADIUS)
        .filter(|section| village::sections_to_village(context.world, *section) == 0)
        .collect();
    if sections.is_empty() {
        return None;
    }
    let section = pick(context, &sections);
    let pois: Vec<BlockPos> = context
        .world
        .pois_in_range(section.center(), POI_RANGE, Occupancy::IsOccupied)
        .into_iter()
        .map(|record| record.pos())
        .collect();
    if pois.is_empty() {
        return None;
    }
    let target = pick(context, &pois);
    land_pos_towards(context, RANDOM_POS_XZ, RANDOM_POS_Y, target.center())
}

fn position_anywhere<W: AiWorld>(context: &mut AiContext<'_, W>) -> Option<Vec3> {
    land_pos(context, RANDOM_POS_XZ, RANDOM_POS_Y)
}

fn village_stroll_position<W: AiWorld>(context: &mut AiContext<'_, W>) -> Option<Vec3> {
    if context.brain.random.next_float() < WANDER_ANYWHERE_CHANCE {
        return position_anywhere(context);
    }
    let target = if context.brain.random.next_float() < VILLAGER_FIRST_CHANCE {
        match position_towards_villager(context) {
            Some(found) => Some(found),
            None => position_towards_poi(context),
        }
    } else {
        match position_towards_poi(context) {
            Some(found) => Some(found),
            None => position_towards_villager(context),
        }
    };
    match target {
        Some(found) => Some(found),
        None => position_anywhere(context),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OfferFlowerGoal {
    entity: Option<Living>,
    tick: i32,
}

impl OfferFlowerGoal {
    pub const fn new() -> Self {
        Self {
            entity: None,
            tick: 0,
        }
    }

    fn gift_area<W: AiWorld>(context: &AiContext<'_, W>) -> rspigot_domain::entity::Aabb {
        inflate(&context.mob.bounds(), 6.0, 2.0, 6.0)
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if !context.world.bright_outside() {
            return false;
        }
        if context.brain.random.next_int(OFFER_CHANCE) != 0 {
            return false;
        }
        let area = Self::gift_area(context);
        let mut candidates = Vec::new();
        for kind in tagged_kinds(CANDIDATE_FOR_GIFT) {
            candidates.extend(
                context
                    .world
                    .living_of(TargetKind::Class(kind.class()), &area),
            );
        }
        let origin = context.mob.position;
        self.entity = targeting::nearest(
            context,
            candidates,
            TargetingConditions::for_non_combat().range(POPPY_OFFER_RANGE),
            |_| true,
            origin,
        );
        self.entity.is_some()
    }

    fn can_continue_to_use(&self) -> bool {
        self.tick > 0
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.tick = adjusted_tick_delay(false, OFFER_TICKS);
        context.act(MobAction::IronGolem(IronGolemAction::OfferFlower(true)));
    }

    fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.act(MobAction::IronGolem(IronGolemAction::OfferFlower(false)));
        let live = self
            .entity
            .and_then(|entity| context.world.living(entity.id));
        if self.tick == 0
            && let Some(entity) = live
            && accepts_gift(entity.kind)
            && Self::gift_area(context).intersects(&entity.bounds)
        {
            context.act(MobAction::IronGolem(IronGolemAction::GiftPoppy(entity.id)));
        }
        self.entity = None;
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let live = self
            .entity
            .and_then(|entity| context.world.living(entity.id));
        if let Some(entity) = live {
            let at = Vec3::new(entity.position.x, entity.eye_y(), entity.position.z);
            context.brain.look.set_look_at(at, OFFER_LOOK, OFFER_LOOK);
        }
        self.tick -= 1;
    }
}

impl Default for OfferFlowerGoal {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DefendVillageTargetGoal {
    memory: TargetMemory,
    potential: Option<EntityId>,
}

impl DefendVillageTargetGoal {
    pub const fn new() -> Self {
        Self {
            memory: TargetMemory::new(false),
            potential: None,
        }
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let area = inflate(&context.mob.bounds(), 10.0, 8.0, 10.0);
        let conditions = TargetingConditions::for_combat().range(DEFEND_RANGE);
        let mut villagers = Vec::new();
        for villager in context
            .world
            .living_of(TargetKind::Class("Villager"), &area)
        {
            if conditions.test(context, &villager, |_| true) {
                villagers.push(villager);
            }
        }
        let mut players = Vec::new();
        let candidates: Vec<Living> = context.world.players().to_vec();
        for player in candidates {
            if contains(&area, player.position) && conditions.test(context, &player, |_| true) {
                players.push(player);
            }
        }
        for villager in &villagers {
            for player in &players {
                if context.world.villager_reputation(villager.id, player.id) <= DEFEND_REPUTATION {
                    self.potential = Some(player.id);
                }
            }
        }
        self.potential
            .and_then(|id| context.world.living(id))
            .is_some_and(|target| !(target.player && (target.spectator || target.creative)))
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let target = self.potential.and_then(|id| context.world.living(id));
        context.set_target(target, TargetReason::DefendVillage);
        self.memory.start();
    }
}

impl Default for DefendVillageTargetGoal {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum IronGolemGoal {
    MoveTowardsTarget(MoveTowardsTargetGoal),
    MoveBackToVillage(RandomStrollGoal),
    StrollInVillage(RandomStrollGoal),
    OfferFlower(OfferFlowerGoal),
    DefendVillage(DefendVillageTargetGoal),
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for IronGolemGoal {
    fn flags(&self) -> Flags {
        match self {
            Self::MoveTowardsTarget(_) | Self::MoveBackToVillage(_) | Self::StrollInVillage(_) => {
                Flags::of(Flag::Move)
            }
            Self::OfferFlower(_) => Flags::of(Flag::Move).with(Flag::Look),
            Self::DefendVillage(_) => Flags::of(Flag::Target),
        }
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::MoveTowardsTarget(goal) => goal.can_use(context),
            Self::MoveBackToVillage(goal) => {
                !context.world.is_village(context.mob.block_pos())
                    && goal.can_use_with(context, move_back_position)
            }
            Self::StrollInVillage(goal) => goal.can_use_with(context, village_stroll_position),
            Self::OfferFlower(goal) => goal.can_use(context),
            Self::DefendVillage(goal) => goal.can_use(context),
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::MoveTowardsTarget(goal) => goal.can_continue_to_use(context),
            Self::MoveBackToVillage(goal) | Self::StrollInVillage(goal) => {
                goal.can_continue_to_use(context)
            }
            Self::OfferFlower(goal) => goal.can_continue_to_use(),
            Self::DefendVillage(goal) => goal.memory.can_continue_to_use(context),
        }
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::MoveTowardsTarget(goal) => goal.start(context),
            Self::MoveBackToVillage(goal) | Self::StrollInVillage(goal) => goal.start(context),
            Self::OfferFlower(goal) => goal.start(context),
            Self::DefendVillage(goal) => goal.start(context),
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::MoveTowardsTarget(goal) => goal.stop(),
            Self::MoveBackToVillage(goal) | Self::StrollInVillage(goal) => goal.stop(context),
            Self::OfferFlower(goal) => goal.stop(context),
            Self::DefendVillage(goal) => goal.memory.stop(context),
        }
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        if let Self::OfferFlower(goal) = self {
            goal.tick(context);
        }
    }
}

pub(crate) fn register(add: &mut Goals<'_>, _kind: EntityKind) {
    add.goal(1, MobGoal::MeleeAttack(MeleeAttackGoal::new(1.0, true)));
    add.goal(
        2,
        MobGoal::IronGolem(IronGolemGoal::MoveTowardsTarget(
            MoveTowardsTargetGoal::new(MOVE_TOWARDS_SPEED, MOVE_TOWARDS_WITHIN),
        )),
    );
    add.goal(
        2,
        MobGoal::IronGolem(IronGolemGoal::MoveBackToVillage(
            RandomStrollGoal::with_settings(VILLAGE_SPEED, MOVE_BACK_INTERVAL, false),
        )),
    );
    add.goal(
        4,
        MobGoal::IronGolem(IronGolemGoal::StrollInVillage(
            RandomStrollGoal::with_settings(VILLAGE_SPEED, VILLAGE_STROLL_INTERVAL, false),
        )),
    );
    add.goal(
        5,
        MobGoal::IronGolem(IronGolemGoal::OfferFlower(OfferFlowerGoal::new())),
    );
    add.look(7, 6.0);
    add.look_around(8);
    add.target_goal(
        1,
        MobGoal::IronGolem(IronGolemGoal::DefendVillage(DefendVillageTargetGoal::new())),
    );
    add.target_goal(2, MobGoal::HurtByTarget(HurtByTargetGoal::new()));
    add.target(
        3,
        NearestAttackableTargetGoal::with(TargetKind::Player, 10, true, TargetFilter::AngryAt),
    );
    add.target(
        3,
        NearestAttackableTargetGoal::with(
            TargetKind::Class("Mob"),
            5,
            false,
            TargetFilter::EnemyNonCreeper,
        ),
    );
    add.target_goal(
        4,
        MobGoal::ResetUniversalAnger(ResetUniversalAngerGoal::new(false)),
    );
}

#[cfg(test)]
mod tests;
