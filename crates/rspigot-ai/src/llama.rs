use rspigot_domain::entity::species::llama;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind;

use crate::action::{MobAction, TargetReason};
use crate::brain::AiContext;
use crate::combat::{HurtByTargetGoal, TargetMemory};
use crate::goal::{Flag, Flags, Goal, reduced_tick_delay};
use crate::goals::{MobGoal, NearestAttackableTargetGoal, TargetFilter, TemptItems};
use crate::ranged::RangedAttackGoal;
use crate::registry::Goals;
use crate::targeting::{self, TargetingConditions};
use crate::world::{AiWorld, TargetKind, inflate};
use crate::{BreedGoal, FollowParentGoal, PanicGoal};

pub const FLOAT_PRIORITY: i32 = 0;
pub const RUN_AROUND_PRIORITY: i32 = 1;
pub const RUN_AROUND_SPEED: f64 = 1.2;
pub const CARAVAN_PRIORITY: i32 = 2;
pub const CARAVAN_SPEED: f64 = 2.1;
pub const RANGED_PRIORITY: i32 = 3;
pub const RANGED_SPEED: f64 = 1.25;
pub const RANGED_INTERVAL: i32 = 40;
pub const RANGED_RADIUS: f32 = 20.0;
pub const PANIC_PRIORITY: i32 = 3;
pub const PANIC_SPEED: f64 = 1.2;
pub const BREED_PRIORITY: i32 = 4;
pub const TEMPT_PRIORITY: i32 = 5;
pub const TEMPT_SPEED: f64 = 1.25;
pub const TEMPT_ITEMS: &str = "minecraft:llama_tempt_items";
pub const FOLLOW_PARENT_PRIORITY: i32 = 6;
pub const STROLL_PRIORITY: i32 = 7;
pub const STROLL_SPEED: f64 = 0.7;
pub const LOOK_PRIORITY: i32 = 8;
pub const LOOK_DISTANCE: f32 = 6.0;
pub const LOOK_AROUND_PRIORITY: i32 = 9;
pub const HURT_BY_PRIORITY: i32 = 1;
pub const WOLF_PRIORITY: i32 = 2;
pub const WOLF_INTERVAL: i32 = 16;
pub const WOLF_FOLLOW_SCALE: f64 = 0.25;
pub const TRADER_PANIC_PRIORITY: i32 = 1;
pub const TRADER_PANIC_SPEED: f64 = 2.0;
pub const TRADER_DEFEND_PRIORITY: i32 = 1;
pub const TRADER_ZOMBIE_PRIORITY: i32 = 2;
pub const TRADER_ILLAGER_PRIORITY: i32 = 2;
pub const MAX_CARAVAN_DEPTH: i32 = 8;
const TARGET_INTERVAL: i32 = 10;
const CARAVAN_MIN_DISTANCE_SQR: f64 = 4.0;
const CARAVAN_CATCH_UP_DISTANCE_SQR: f64 = 676.0;
const CARAVAN_MAX_SPEED: f64 = 3.0;
const CARAVAN_SPEED_GROWTH: f64 = 1.2;
const CARAVAN_CHECK_TICKS: i32 = 40;
const CARAVAN_GAP: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CaravanFacts {
    pub in_caravan: bool,
    pub head: Option<EntityId>,
    pub has_tail: bool,
    pub leashed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LlamaFacts {
    pub strength: i32,
    pub did_spit: bool,
    pub caravan: CaravanFacts,
    pub leashed_to_knot: bool,
    pub leashed_to_trader: Option<EntityId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlamaAction {
    JoinCaravan { head: EntityId },
    LeaveCaravan,
    ClearSpit,
}

fn first_is_leashed<W: AiWorld>(world: &W, head: Option<EntityId>, counter: i32) -> bool {
    if counter > MAX_CARAVAN_DEPTH {
        return false;
    }
    let Some(head) = head.and_then(|id| world.living(id)) else {
        return false;
    };
    if head.facts.caravan.leashed {
        return true;
    }
    first_is_leashed(world, head.facts.caravan.head, counter + 1)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LlamaFollowCaravanGoal {
    speed_modifier: f64,
    dist_check_counter: i32,
    head: Option<EntityId>,
}

impl LlamaFollowCaravanGoal {
    pub const fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            dist_check_counter: 0,
            head: None,
        }
    }

    pub fn speed_modifier(&self) -> f64 {
        self.speed_modifier
    }

    pub fn head(&self) -> Option<EntityId> {
        self.head
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let facts = context.mob.species.llama;
        if context.mob.leashed || facts.caravan.in_caravan {
            return false;
        }
        let mob = context.mob;
        let area = inflate(
            &mob.bounds(),
            llama::CARAVAN_SEARCH_XZ,
            llama::CARAVAN_SEARCH_Y,
            llama::CARAVAN_SEARCH_XZ,
        );
        let candidates: Vec<_> = context
            .world
            .living_of(TargetKind::Class("Llama"), &area)
            .into_iter()
            .filter(|other| other.id != mob.id)
            .collect();
        let mut best = None;
        let mut distance = f64::MAX;
        for other in &candidates {
            if other.facts.caravan.in_caravan && !other.facts.caravan.has_tail {
                let squared = mob.distance_to_sqr(other.position);
                if squared <= distance {
                    best = Some(*other);
                    distance = squared;
                }
            }
        }
        if best.is_none() {
            for other in &candidates {
                if other.facts.caravan.leashed && !other.facts.caravan.has_tail {
                    let squared = mob.distance_to_sqr(other.position);
                    if squared <= distance {
                        best = Some(*other);
                        distance = squared;
                    }
                }
            }
        }
        let Some(head) = best else {
            return false;
        };
        if distance < CARAVAN_MIN_DISTANCE_SQR {
            return false;
        }
        if head.facts.caravan.leashed || first_is_leashed(context.world, head.facts.caravan.head, 1)
        {
            self.head = Some(head.id);
            context.act(MobAction::Llama(LlamaAction::JoinCaravan { head: head.id }));
            return true;
        }
        false
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(head) = self.head.and_then(|id| context.world.living(id)) else {
            return false;
        };
        if !head.alive || !first_is_leashed(context.world, self.head, 0) {
            return false;
        }
        let distance = context.mob.distance_to_sqr(head.position);
        if distance > CARAVAN_CATCH_UP_DISTANCE_SQR {
            if self.speed_modifier <= CARAVAN_MAX_SPEED {
                self.speed_modifier *= CARAVAN_SPEED_GROWTH;
                self.dist_check_counter = reduced_tick_delay(CARAVAN_CHECK_TICKS);
                return true;
            }
            if self.dist_check_counter == 0 {
                return false;
            }
        }
        if self.dist_check_counter > 0 {
            self.dist_check_counter -= 1;
        }
        true
    }

    fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.act(MobAction::Llama(LlamaAction::LeaveCaravan));
        self.head = None;
        self.speed_modifier = CARAVAN_SPEED;
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(head) = self.head.and_then(|id| context.world.living(id)) else {
            return;
        };
        if context.mob.species.llama.leashed_to_knot {
            return;
        }
        let own = context.mob.position;
        let distance = f64::from(own.distance_squared(head.position).sqrt() as f32);
        let direction = Vec3::new(
            head.position.x - own.x,
            head.position.y - own.y,
            head.position.z - own.z,
        )
        .normalize()
        .scale((distance - CARAVAN_GAP).max(0.0));
        context.move_to_pos(
            Vec3::new(
                own.x + direction.x,
                own.y + direction.y,
                own.z + direction.z,
            ),
            self.speed_modifier,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LlamaHurtByTargetGoal {
    inner: HurtByTargetGoal,
}

impl LlamaHurtByTargetGoal {
    pub const fn new() -> Self {
        Self {
            inner: HurtByTargetGoal::new(),
        }
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if context.mob.species.llama.did_spit {
            context.act(MobAction::Llama(LlamaAction::ClearSpit));
            return false;
        }
        self.inner.memory().can_continue_to_use(context)
    }
}

impl Default for LlamaHurtByTargetGoal {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TraderLlamaDefendWanderingTraderGoal {
    memory: TargetMemory,
    attacker: Option<EntityId>,
    timestamp: i32,
}

impl TraderLlamaDefendWanderingTraderGoal {
    pub const fn new() -> Self {
        Self {
            memory: TargetMemory::new(false),
            attacker: None,
            timestamp: 0,
        }
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(trader) = context
            .mob
            .species
            .llama
            .leashed_to_trader
            .and_then(|id| context.world.living(id))
        else {
            return false;
        };
        self.attacker = trader.facts.last_hurt_by_mob;
        if trader.facts.last_hurt_by_mob_at == self.timestamp {
            return false;
        }
        let Some(attacker) = self.attacker.and_then(|id| context.world.living(id)) else {
            return false;
        };
        targeting::goal_can_attack(context, &attacker, TargetingConditions::for_combat())
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let attacker = self.attacker.and_then(|id| context.world.living(id));
        context.set_target(attacker, TargetReason::TargetAttackedOwner);
        if let Some(trader) = context
            .mob
            .species
            .llama
            .leashed_to_trader
            .and_then(|id| context.world.living(id))
        {
            self.timestamp = trader.facts.last_hurt_by_mob_at;
        }
        self.memory.start();
    }
}

impl Default for TraderLlamaDefendWanderingTraderGoal {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LlamaGoal {
    FollowCaravan(LlamaFollowCaravanGoal),
    HurtBy(LlamaHurtByTargetGoal),
    DefendTrader(TraderLlamaDefendWanderingTraderGoal),
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for LlamaGoal {
    fn flags(&self) -> Flags {
        match self {
            Self::FollowCaravan(_) => Flags::of(Flag::Move),
            Self::HurtBy(_) | Self::DefendTrader(_) => Flags::of(Flag::Target),
        }
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::FollowCaravan(goal) => goal.can_use(context),
            Self::HurtBy(goal) => goal.inner.can_use(context),
            Self::DefendTrader(goal) => goal.can_use(context),
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::FollowCaravan(goal) => goal.can_continue_to_use(context),
            Self::HurtBy(goal) => goal.can_continue_to_use(context),
            Self::DefendTrader(goal) => goal.memory.can_continue_to_use(context),
        }
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::FollowCaravan(_) => {}
            Self::HurtBy(goal) => goal.inner.start(context),
            Self::DefendTrader(goal) => goal.start(context),
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::FollowCaravan(goal) => goal.stop(context),
            Self::HurtBy(goal) => goal.inner.memory().stop(context),
            Self::DefendTrader(goal) => goal.memory.stop(context),
        }
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        if let Self::FollowCaravan(goal) = self {
            goal.tick(context);
        }
    }
}

pub(crate) fn llama_goals(kind: EntityKind, add: &mut Goals<'_>) {
    add.float(FLOAT_PRIORITY);
    add.goal(
        RUN_AROUND_PRIORITY,
        MobGoal::Equine(crate::equine::EquineGoal::RunAround(
            crate::equine::RunAroundLikeCrazyGoal::new(RUN_AROUND_SPEED),
        )),
    );
    add.goal(
        CARAVAN_PRIORITY,
        MobGoal::Llama(LlamaGoal::FollowCaravan(LlamaFollowCaravanGoal::new(
            CARAVAN_SPEED,
        ))),
    );
    add.goal(
        RANGED_PRIORITY,
        MobGoal::RangedAttack(RangedAttackGoal::new(
            RANGED_SPEED,
            RANGED_INTERVAL,
            RANGED_RADIUS,
        )),
    );
    add.goal(PANIC_PRIORITY, MobGoal::Panic(PanicGoal::new(PANIC_SPEED)));
    add.goal(BREED_PRIORITY, MobGoal::Breed(BreedGoal::llama(1.0)));
    add.tempt(TEMPT_PRIORITY, TEMPT_SPEED, TemptItems::Tag(TEMPT_ITEMS));
    add.goal(
        FOLLOW_PARENT_PRIORITY,
        MobGoal::FollowParent(FollowParentGoal::new(1.0)),
    );
    add.stroll(STROLL_PRIORITY, STROLL_SPEED);
    add.look(LOOK_PRIORITY, LOOK_DISTANCE);
    add.look_around(LOOK_AROUND_PRIORITY);
    add.target_goal(
        HURT_BY_PRIORITY,
        MobGoal::Llama(LlamaGoal::HurtBy(LlamaHurtByTargetGoal::new())),
    );
    add.target(
        WOLF_PRIORITY,
        NearestAttackableTargetGoal::with(
            TargetKind::Class("Wolf"),
            WOLF_INTERVAL,
            false,
            TargetFilter::Untamed,
        )
        .with_follow_scale(WOLF_FOLLOW_SCALE),
    );
    if kind == EntityKind::TraderLlama {
        add.goal(
            TRADER_PANIC_PRIORITY,
            MobGoal::Panic(PanicGoal::new(TRADER_PANIC_SPEED)),
        );
        add.target_goal(
            TRADER_DEFEND_PRIORITY,
            MobGoal::Llama(LlamaGoal::DefendTrader(
                TraderLlamaDefendWanderingTraderGoal::new(),
            )),
        );
        add.target(
            TRADER_ZOMBIE_PRIORITY,
            NearestAttackableTargetGoal::with(
                TargetKind::Class("Zombie"),
                TARGET_INTERVAL,
                true,
                TargetFilter::NotZombifiedPiglin,
            ),
        );
        add.target(
            TRADER_ILLAGER_PRIORITY,
            NearestAttackableTargetGoal::new(TargetKind::Class("AbstractIllager"), true),
        );
    }
}

#[cfg(test)]
mod tests {
    use rspigot_domain::Vec3;
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::GoalSettings;
    use crate::MobAi;
    use crate::goal::GoalSelector;
    use crate::goals::{LookAtPlayerGoal, RandomLookAroundGoal};
    use crate::mock::{Harness, Mock, living};

    type Registered = Vec<(i32, MobGoal)>;

    fn lists(kind: EntityKind) -> (Registered, Registered) {
        let ai = MobAi::new(kind, 1, GoalSettings::default());
        let collect = |selector: &GoalSelector<MobGoal>| {
            selector
                .goals()
                .iter()
                .map(|wrapped| (wrapped.priority(), wrapped.goal().clone()))
                .collect()
        };
        (collect(ai.goals()), collect(ai.targets()))
    }

    fn priorities(list: &Registered) -> Vec<i32> {
        list.iter().map(|(priority, _)| *priority).collect()
    }

    #[test]
    fn the_llama_registers_the_vanilla_goals_in_priority_order() {
        let (goals, targets) = lists(EntityKind::Llama);
        assert_eq!(priorities(&goals), vec![0, 1, 2, 3, 3, 4, 5, 6, 7, 8, 9]);
        assert!(matches!(goals[0].1, MobGoal::Float(_)));
        assert!(matches!(
            goals[1].1,
            MobGoal::Equine(crate::equine::EquineGoal::RunAround(_))
        ));
        assert_eq!(
            goals[2].1,
            MobGoal::Llama(LlamaGoal::FollowCaravan(LlamaFollowCaravanGoal::new(2.1)))
        );
        assert_eq!(
            goals[3].1,
            MobGoal::RangedAttack(RangedAttackGoal::new(1.25, 40, 20.0))
        );
        assert_eq!(goals[4].1, MobGoal::Panic(PanicGoal::new(1.2)));
        assert_eq!(goals[5].1, MobGoal::Breed(BreedGoal::llama(1.0)));
        assert!(matches!(goals[6].1, MobGoal::Tempt(_)));
        assert_eq!(
            goals[7].1,
            MobGoal::FollowParent(FollowParentGoal::new(1.0))
        );
        assert!(matches!(goals[8].1, MobGoal::Stroll(_)));
        assert_eq!(
            goals[9].1,
            MobGoal::LookAtPlayer(LookAtPlayerGoal::new(6.0))
        );
        assert_eq!(
            goals[10].1,
            MobGoal::RandomLookAround(RandomLookAroundGoal::default())
        );
        assert_eq!(priorities(&targets), vec![1, 2]);
        assert_eq!(
            targets[0].1,
            MobGoal::Llama(LlamaGoal::HurtBy(LlamaHurtByTargetGoal::new()))
        );
        assert_eq!(
            targets[1].1,
            MobGoal::NearestAttackableTarget(
                NearestAttackableTargetGoal::with(
                    TargetKind::Class("Wolf"),
                    16,
                    false,
                    TargetFilter::Untamed,
                )
                .with_follow_scale(0.25)
            )
        );
    }

    #[test]
    fn the_trader_llama_adds_a_fast_panic_and_the_trader_defence_targets() {
        let (goals, targets) = lists(EntityKind::TraderLlama);
        assert_eq!(priorities(&goals), vec![0, 1, 2, 3, 3, 4, 5, 6, 7, 8, 9, 1]);
        assert_eq!(goals[11].1, MobGoal::Panic(PanicGoal::new(2.0)));
        assert_eq!(priorities(&targets), vec![1, 2, 1, 2, 2]);
        assert!(matches!(
            targets[2].1,
            MobGoal::Llama(LlamaGoal::DefendTrader(_))
        ));
        assert_eq!(
            targets[3].1,
            MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::with(
                TargetKind::Class("Zombie"),
                10,
                true,
                TargetFilter::NotZombifiedPiglin,
            ))
        );
        assert_eq!(
            targets[4].1,
            MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::new(
                TargetKind::Class("AbstractIllager"),
                true,
            ))
        );
    }

    #[test]
    fn a_llama_waits_for_a_path_of_forty_blocks() {
        let ai = MobAi::new(EntityKind::Llama, 1, GoalSettings::default());
        assert_eq!(ai.brain().navigation.max_path_length(16.0), 40.0);
        assert_eq!(ai.brain().navigation.max_visited_nodes(), 640);
    }

    fn caravan_member(
        id: i32,
        at: Vec3,
        head: Option<i32>,
        tail: bool,
        leashed: bool,
    ) -> crate::Living {
        let mut member = living(id, EntityKind::Llama, at);
        member.facts.caravan = CaravanFacts {
            in_caravan: head.is_some(),
            head: head.map(EntityId),
            has_tail: tail,
            leashed,
        };
        member
    }

    fn follower(others: Vec<crate::Living>) -> Harness {
        let world = Mock {
            others,
            ..Mock::default()
        };
        Harness::new(EntityKind::Llama, Vec3::new(0.5, 64.0, 0.5), world, 3)
    }

    #[test]
    fn a_llama_joins_the_nearest_leashed_llama_without_a_tail() {
        let leader = caravan_member(2, Vec3::new(6.5, 64.0, 0.5), None, false, true);
        let far = caravan_member(3, Vec3::new(8.5, 64.0, 0.5), None, false, true);
        let mut harness = follower(vec![far, leader]);
        let mut goal = LlamaFollowCaravanGoal::new(2.1);
        assert!(harness.run(|context| goal.can_use(context)));
        assert_eq!(goal.head(), Some(EntityId(2)));
        assert!(
            harness.brain.actions.iter().any(|action| *action
                == MobAction::Llama(LlamaAction::JoinCaravan { head: EntityId(2) }))
        );
    }

    #[test]
    fn a_leashed_llama_or_one_already_in_a_caravan_never_joins() {
        let leader = caravan_member(2, Vec3::new(6.5, 64.0, 0.5), None, false, true);
        let mut harness = follower(vec![leader]);
        harness.mob.leashed = true;
        let mut goal = LlamaFollowCaravanGoal::new(2.1);
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.mob.leashed = false;
        harness.mob.species.llama.caravan.in_caravan = true;
        assert!(!harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn a_leader_with_a_tail_or_closer_than_two_blocks_is_ignored() {
        let taken = caravan_member(2, Vec3::new(6.5, 64.0, 0.5), None, true, true);
        let mut harness = follower(vec![taken]);
        let mut goal = LlamaFollowCaravanGoal::new(2.1);
        assert!(!harness.run(|context| goal.can_use(context)));
        let close = caravan_member(3, Vec3::new(1.5, 64.0, 0.5), None, false, true);
        let mut harness = follower(vec![close]);
        assert!(!harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn a_caravan_member_is_preferred_to_a_leashed_leader_when_its_chain_reaches_one() {
        let leader = caravan_member(2, Vec3::new(3.5, 64.0, 0.5), None, true, true);
        let middle = caravan_member(3, Vec3::new(6.5, 64.0, 0.5), Some(2), false, false);
        let mut harness = follower(vec![leader, middle]);
        let mut goal = LlamaFollowCaravanGoal::new(2.1);
        assert!(harness.run(|context| goal.can_use(context)));
        assert_eq!(goal.head(), Some(EntityId(3)));
    }

    #[test]
    fn a_chain_without_a_leashed_head_is_not_followed() {
        let tail = caravan_member(3, Vec3::new(6.5, 64.0, 0.5), Some(2), false, false);
        let orphan = caravan_member(2, Vec3::new(9.5, 64.0, 0.5), None, true, false);
        let mut harness = follower(vec![tail, orphan]);
        let mut goal = LlamaFollowCaravanGoal::new(2.1);
        assert!(!harness.run(|context| goal.can_use(context)));
    }

    fn joined(head_at: Vec3) -> (Harness, LlamaFollowCaravanGoal) {
        let leader = caravan_member(2, head_at, None, true, true);
        let mut harness = follower(vec![leader]);
        let mut goal = LlamaFollowCaravanGoal::new(2.1);
        goal.head = Some(EntityId(2));
        harness.brain.actions.clear();
        (harness, goal)
    }

    #[test]
    fn the_follower_speeds_up_by_a_fifth_when_the_head_is_far_and_gives_up_past_three() {
        let (mut harness, mut goal) = joined(Vec3::new(30.5, 64.0, 0.5));
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        assert!((goal.speed_modifier() - 2.1 * 1.2).abs() < 1e-9);
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        assert!(goal.speed_modifier() > 3.0);
        for _ in 0..30 {
            if !harness.run(|context| goal.can_continue_to_use(context)) {
                return;
            }
        }
        panic!("the follower should give up once the check counter runs out");
    }

    #[test]
    fn the_follower_stays_while_the_head_is_near_and_leaves_when_it_dies() {
        let (mut harness, mut goal) = joined(Vec3::new(8.5, 64.0, 0.5));
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        harness.world.others[0].alive = false;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
    }

    #[test]
    fn stopping_leaves_the_caravan_and_resets_the_speed() {
        let (mut harness, mut goal) = joined(Vec3::new(30.5, 64.0, 0.5));
        harness.run(|context| goal.can_continue_to_use(context));
        harness.run(|context| goal.stop(context));
        assert_eq!(goal.speed_modifier(), 2.1);
        assert_eq!(goal.head(), None);
        assert!(
            harness
                .brain
                .actions
                .contains(&MobAction::Llama(LlamaAction::LeaveCaravan))
        );
    }

    #[test]
    fn the_follower_walks_to_two_blocks_behind_the_head() {
        let (mut harness, mut goal) = joined(Vec3::new(10.5, 64.0, 0.5));
        harness.run(|context| goal.tick(context));
        assert!(!harness.brain.navigation.is_done());
    }

    #[test]
    fn a_spit_llama_drops_its_hurt_by_target_once() {
        let mut harness = follower(Vec::new());
        harness.mob.species.llama.did_spit = true;
        let mut goal = LlamaHurtByTargetGoal::new();
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        assert!(
            harness
                .brain
                .actions
                .contains(&MobAction::Llama(LlamaAction::ClearSpit))
        );
    }

    #[test]
    fn a_trader_llama_defends_the_trader_that_was_hurt_once_per_attack() {
        let mut trader = living(5, EntityKind::WanderingTrader, Vec3::new(3.5, 64.0, 0.5));
        trader.facts.last_hurt_by_mob = Some(EntityId(6));
        trader.facts.last_hurt_by_mob_at = 40;
        let attacker = living(6, EntityKind::Zombie, Vec3::new(4.5, 64.0, 0.5));
        let world = Mock {
            others: vec![trader, attacker],
            ..Mock::default()
        };
        let mut harness =
            Harness::new(EntityKind::TraderLlama, Vec3::new(0.5, 64.0, 0.5), world, 3);
        let mut goal = TraderLlamaDefendWanderingTraderGoal::new();
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.mob.species.llama.leashed_to_trader = Some(EntityId(5));
        assert!(harness.run(|context| goal.can_use(context)));
        harness.run(|context| goal.start(context));
        assert_eq!(harness.brain.target, Some(EntityId(6)));
        assert!(!harness.run(|context| goal.can_use(context)));
    }
}
