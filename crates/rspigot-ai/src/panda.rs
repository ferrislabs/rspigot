use rspigot_blocks::BlockKind;
use rspigot_domain::entity::species::panda::Gene;
use rspigot_domain::{BlockPos, EntityId, Vec3, mth};
use rspigot_entities::EntityKind;

use crate::action::{MobAction, TargetReason};
use crate::animal::{BreedGoal, FollowParentGoal, PanicGoal};
use crate::brain::AiContext;
use crate::combat::{HurtByTargetGoal, MeleeAttackGoal};
use crate::evade::AvoidEntityGoal;
use crate::goal::{Flag, Flags, Goal, adjusted_tick_delay, reduced_tick_delay};
use crate::goals::{MobGoal, TemptItems};
use crate::random::RandomSource;
use crate::registry::Goals;
use crate::targeting::{self, TargetingConditions};
use crate::world::{AiWorld, DroppedItem, TargetKind, inflate};

const EATS_FROM_GROUND: TemptItems = TemptItems::Tag("minecraft:panda_eats_from_ground");
const BAMBOO_RANGE: i32 = 8;
const BAMBOO_HEIGHT: i32 = 3;
const UNHAPPY_TIME: i32 = 32;
const UNHAPPY_COOLDOWN: i32 = 600;
const BREED_LOOK_RANGE: f64 = 8.0;
const LOOK_DISTANCE: f32 = 6.0;
const LOOK_PROBABILITY: f32 = 0.02;
const SIT_CHECK_RANGE: f64 = 6.0;
const SIT_FETCH_RANGE: f64 = 8.0;
const SPEED_1_2F: f64 = 1.2000000476837158;
const LIE_ON_BACK_COOLDOWN: i32 = 200;
const ROLL_EDGE_THRESHOLD: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PandaFacts {
    pub variant: Gene,
    pub unhappy_counter: i32,
    pub sitting: bool,
    pub on_back: bool,
    pub rolling: bool,
    pub eating: bool,
    pub thundering: bool,
    pub holding: bool,
    pub got_bamboo: bool,
    pub did_bite: bool,
}

impl PandaFacts {
    pub fn is_lazy(&self) -> bool {
        self.variant == Gene::Lazy
    }

    pub fn is_worried(&self) -> bool {
        self.variant == Gene::Worried
    }

    pub fn is_playful(&self) -> bool {
        self.variant == Gene::Playful
    }

    pub fn is_weak(&self) -> bool {
        self.variant == Gene::Weak
    }

    pub fn is_scared(&self) -> bool {
        self.is_worried() && self.thundering
    }

    pub fn can_perform_action(&self) -> bool {
        !self.on_back && !self.is_scared() && !self.eating && !self.rolling && !self.sitting
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PandaBrain {
    pub active: bool,
    pub facts: PandaFacts,
    pub look_at: Option<EntityId>,
}

impl PandaBrain {
    pub fn can_move(&self) -> bool {
        !self.active || self.facts.can_perform_action()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PandaAction {
    Sit(bool),
    OnBack(bool),
    Roll(bool),
    Sneeze(bool),
    Unhappy(i32),
    TryToSit,
    DropHeld,
    Alert { mob: EntityId, target: EntityId },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PandaBreedGoal {
    goal: BreedGoal,
    unhappy_cooldown: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PandaSitGoal {
    cooldown: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PandaLieOnBackGoal {
    cooldown: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PandaLookAtPlayerGoal {
    look_time: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PandaGoal {
    Panic(PanicGoal),
    Breed(PandaBreedGoal),
    Attack(MeleeAttackGoal),
    Avoid(AvoidEntityGoal),
    Sit(PandaSitGoal),
    LieOnBack(PandaLieOnBackGoal),
    Sneeze,
    LookAtPlayer(PandaLookAtPlayerGoal),
    Roll,
    HurtByTarget(HurtByTargetGoal),
}

impl PandaGoal {
    pub fn is_panic(&self) -> bool {
        matches!(self, Self::Panic(goal) if goal.is_running())
    }
}

fn facts<W: AiWorld>(context: &AiContext<'_, W>) -> PandaFacts {
    context.brain.panda.facts
}

fn can_pick_up_and_eat(item: &DroppedItem) -> bool {
    EATS_FROM_GROUND.matches(Some(item.item)) && item.alive && !item.has_pickup_delay
}

fn edible_items<W: AiWorld>(context: &AiContext<'_, W>, range: f64) -> Vec<DroppedItem> {
    let area = inflate(&context.mob.bounds(), range, range, range);
    context
        .world
        .items_in(&area)
        .into_iter()
        .filter(can_pick_up_and_eat)
        .collect()
}

fn set_unhappy<W: AiWorld>(context: &mut AiContext<'_, W>, value: i32) {
    context.brain.panda.facts.unhappy_counter = value;
    context.act(MobAction::Panda(PandaAction::Unhappy(value)));
}

fn set_on_back<W: AiWorld>(context: &mut AiContext<'_, W>, value: bool) {
    context.brain.panda.facts.on_back = value;
    context.act(MobAction::Panda(PandaAction::OnBack(value)));
}

fn sit<W: AiWorld>(context: &mut AiContext<'_, W>, value: bool) {
    context.brain.panda.facts.sitting = value;
    context.act(MobAction::Panda(PandaAction::Sit(value)));
}

fn try_to_sit<W: AiWorld>(context: &mut AiContext<'_, W>) {
    if context.mob.fluid.in_water {
        return;
    }
    context.stop_navigation();
    context.brain.panda.facts.sitting = true;
    context.act(MobAction::Panda(PandaAction::TryToSit));
}

fn can_find_bamboo<W: AiWorld>(context: &AiContext<'_, W>) -> bool {
    let origin = context.mob.block_pos();
    for y in 0..BAMBOO_HEIGHT {
        for range in 0..BAMBOO_RANGE {
            let mut x = 0;
            while x <= range {
                let mut z = if x < range && x > -range { range } else { 0 };
                while z <= range {
                    let pos = origin.offset(x, y, z);
                    let state = context.world.state(pos);
                    if state.and_then(|state| state.kind()) == Some(BlockKind::Bamboo) {
                        return true;
                    }
                    z = if z > 0 { -z } else { 1 - z };
                }
                x = if x > 0 { -x } else { 1 - x };
            }
        }
    }
    false
}

impl PandaBreedGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            goal: BreedGoal::new(speed_modifier),
            unhappy_cooldown: 0,
        }
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if !self.goal.can_use(context) || facts(context).unhappy_counter != 0 {
            return false;
        }
        if can_find_bamboo(context) {
            return true;
        }
        if self.unhappy_cooldown <= context.mob.tick_count {
            set_unhappy(context, UNHAPPY_TIME);
            self.unhappy_cooldown = context.mob.tick_count + UNHAPPY_COOLDOWN;
            let conditions = TargetingConditions::for_non_combat().range(BREED_LOOK_RANGE);
            let origin = context.mob.position;
            let player = targeting::nearest_player(context, conditions, |_| true, origin);
            context.brain.panda.look_at = player.map(|player| player.id);
        }
        false
    }
}

impl PandaSitGoal {
    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let panda = facts(context);
        if self.cooldown > context.mob.tick_count
            || context.mob.baby
            || context.mob.fluid.in_water
            || !panda.can_perform_action()
            || panda.unhappy_counter > 0
        {
            return false;
        }
        panda.holding || !edible_items(context, SIT_CHECK_RANGE).is_empty()
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if context.mob.fluid.in_water {
            return false;
        }
        let keeps =
            facts(context).is_lazy() || context.brain.random.next_int(reduced_tick_delay(600)) != 1;
        keeps && context.brain.random.next_int(reduced_tick_delay(2000)) != 1
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        if facts(context).holding {
            try_to_sit(context);
        } else if let Some(item) = edible_items(context, SIT_FETCH_RANGE).first() {
            context.move_to_entity_at(item.position, SPEED_1_2F);
        }
        self.cooldown = 0;
    }

    fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        if facts(context).holding {
            context.brain.panda.facts.holding = false;
            context.act(MobAction::Panda(PandaAction::DropHeld));
            let ticks = if facts(context).is_lazy() {
                context.brain.random.next_int(50) + 10
            } else {
                context.brain.random.next_int(150) + 10
            };
            self.cooldown = context.mob.tick_count + ticks * 20;
        }
        sit(context, false);
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let panda = facts(context);
        if !panda.sitting && panda.holding {
            try_to_sit(context);
        }
    }
}

impl PandaLieOnBackGoal {
    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let panda = facts(context);
        self.cooldown < context.mob.tick_count
            && panda.is_lazy()
            && panda.can_perform_action()
            && context.brain.random.next_int(reduced_tick_delay(400)) == 1
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if context.mob.fluid.in_water {
            return false;
        }
        let keeps =
            facts(context).is_lazy() || context.brain.random.next_int(reduced_tick_delay(600)) != 1;
        keeps && context.brain.random.next_int(reduced_tick_delay(2000)) != 1
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        set_on_back(context, true);
        self.cooldown = 0;
    }

    fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        set_on_back(context, false);
        self.cooldown = context.mob.tick_count + LIE_ON_BACK_COOLDOWN;
    }
}

impl PandaLookAtPlayerGoal {
    fn target<W: AiWorld>(context: &AiContext<'_, W>) -> Option<crate::world::Living> {
        let id = context.brain.panda.look_at?;
        context.world.living(id)
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if context.brain.random.next_float() >= LOOK_PROBABILITY {
            return false;
        }
        if context.brain.panda.look_at.is_none() {
            let conditions = TargetingConditions::for_non_combat().range(f64::from(LOOK_DISTANCE));
            let origin = context.mob.eye_position();
            let player = targeting::nearest_player(context, conditions, |_| true, origin);
            context.brain.panda.look_at = player.map(|player| player.id);
        }
        facts(context).can_perform_action() && context.brain.panda.look_at.is_some()
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if context.brain.panda.look_at.is_none() {
            return false;
        }
        let Some(target) = Self::target(context) else {
            return false;
        };
        if !target.alive {
            return false;
        }
        let distance = f64::from(LOOK_DISTANCE * LOOK_DISTANCE);
        context.mob.distance_to_sqr(target.position) <= distance && self.look_time > 0
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let ticks = 40 + context.brain.random.next_int(40);
        self.look_time = adjusted_tick_delay(false, ticks);
    }

    fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.brain.panda.look_at = None;
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(target) = Self::target(context) else {
            return;
        };
        if !target.alive {
            return;
        }
        context.brain.look.look_at(Vec3::new(
            target.position.x,
            target.eye_y(),
            target.position.z,
        ));
        self.look_time -= 1;
    }
}

fn roll_can_use<W: AiWorld>(context: &mut AiContext<'_, W>) -> bool {
    let panda = facts(context);
    if !((context.mob.baby || panda.is_playful()) && context.mob.on_ground) {
        return false;
    }
    if !panda.can_perform_action() {
        return false;
    }
    let radians = f64::from(context.mob.yaw * mth::DEG_TO_RAD);
    let sin = -mth::sin(radians);
    let cos = mth::cos(radians);
    let step = |value: f32| {
        if f64::from(value.abs()) > ROLL_EDGE_THRESHOLD {
            mth::sign(f64::from(value))
        } else {
            0
        }
    };
    let below: BlockPos = context.mob.block_pos().offset(step(sin), -1, step(cos));
    let air = context
        .world
        .state(below)
        .is_none_or(|state| state.is_air());
    if air {
        return true;
    }
    if panda.is_playful() && context.brain.random.next_int(reduced_tick_delay(60)) == 1 {
        return true;
    }
    context.brain.random.next_int(reduced_tick_delay(500)) == 1
}

fn sneeze_can_use<W: AiWorld>(context: &mut AiContext<'_, W>) -> bool {
    let panda = facts(context);
    if !(context.mob.baby && panda.can_perform_action()) {
        return false;
    }
    if panda.is_weak() && context.brain.random.next_int(reduced_tick_delay(500)) == 1 {
        return true;
    }
    context.brain.random.next_int(reduced_tick_delay(6000)) == 1
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for PandaGoal {
    fn flags(&self) -> Flags {
        let movement = Flags::of(Flag::Move);
        match self {
            Self::Panic(_) | Self::Avoid(_) | Self::Sit(_) => movement,
            Self::Breed(_) | Self::Attack(_) => movement.with(Flag::Look),
            Self::LieOnBack(_) | Self::Sneeze => Flags::NONE,
            Self::LookAtPlayer(_) => Flags::of(Flag::Look),
            Self::Roll => movement.with(Flag::Look).with(Flag::Jump),
            Self::HurtByTarget(_) => Flags::of(Flag::Target),
        }
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::Panic(goal) => goal.can_use(context),
            Self::Breed(goal) => goal.can_use(context),
            Self::Attack(goal) => facts(context).can_perform_action() && goal.can_use(context),
            Self::Avoid(goal) => {
                let panda = facts(context);
                panda.is_worried() && panda.can_perform_action() && goal.can_use(context)
            }
            Self::Sit(goal) => goal.can_use(context),
            Self::LieOnBack(goal) => goal.can_use(context),
            Self::Sneeze => sneeze_can_use(context),
            Self::LookAtPlayer(goal) => goal.can_use(context),
            Self::Roll => roll_can_use(context),
            Self::HurtByTarget(goal) => goal.can_use(context),
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::Panic(_) => {
                if facts(context).sitting {
                    context.stop_navigation();
                    false
                } else {
                    !context.brain.navigation.is_done()
                }
            }
            Self::Breed(goal) => goal.goal.can_continue_to_use(context),
            Self::Attack(goal) => goal.can_continue_to_use(context),
            Self::Avoid(goal) => goal.can_continue_to_use(context),
            Self::Sit(goal) => goal.can_continue_to_use(context),
            Self::LieOnBack(goal) => goal.can_continue_to_use(context),
            Self::Sneeze | Self::Roll => false,
            Self::LookAtPlayer(goal) => goal.can_continue_to_use(context),
            Self::HurtByTarget(goal) => {
                let panda = facts(context);
                if panda.got_bamboo || panda.did_bite {
                    context.set_target(None, TargetReason::Unknown);
                    false
                } else {
                    goal.memory().can_continue_to_use(context)
                }
            }
        }
    }

    fn is_interruptable(&self) -> bool {
        !matches!(self, Self::Roll)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Panic(goal) => goal.start(context),
            Self::Breed(_) => {}
            Self::Attack(goal) => goal.start(context),
            Self::Avoid(goal) => goal.start(context),
            Self::Sit(goal) => goal.start(context),
            Self::LieOnBack(goal) => goal.start(context),
            Self::Sneeze => context.act(MobAction::Panda(PandaAction::Sneeze(true))),
            Self::LookAtPlayer(goal) => goal.start(context),
            Self::Roll => {
                context.brain.panda.facts.rolling = true;
                context.act(MobAction::Panda(PandaAction::Roll(true)));
            }
            Self::HurtByTarget(goal) => goal.start(context),
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Panic(goal) => goal.stop(),
            Self::Breed(goal) => goal.goal.stop(),
            Self::Attack(goal) => goal.stop(context),
            Self::Avoid(goal) => goal.stop(),
            Self::Sit(goal) => goal.stop(context),
            Self::LieOnBack(goal) => goal.stop(context),
            Self::Sneeze | Self::Roll => {}
            Self::LookAtPlayer(goal) => goal.stop(context),
            Self::HurtByTarget(goal) => goal.memory().stop(context),
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        matches!(self, Self::Attack(_))
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::Panic(goal) => goal.tick(context),
            Self::Breed(goal) => goal.goal.tick(context),
            Self::Attack(goal) => goal.tick(context),
            Self::Avoid(goal) => goal.tick(context),
            Self::Sit(goal) => goal.tick(context),
            Self::LookAtPlayer(goal) => goal.tick(context),
            Self::LieOnBack(_) | Self::Sneeze | Self::Roll | Self::HurtByTarget(_) => {}
        }
    }
}

pub(crate) fn register(add: &mut Goals<'_>, _kind: EntityKind) {
    add.brain.panda.active = true;
    let panda = |goal| MobGoal::Panda(goal);
    add.float(0);
    add.goal(2, panda(PandaGoal::Panic(PanicGoal::new(2.0))));
    add.goal(2, panda(PandaGoal::Breed(PandaBreedGoal::new(1.0))));
    add.goal(
        3,
        panda(PandaGoal::Attack(MeleeAttackGoal::new(SPEED_1_2F, true))),
    );
    add.tempt(4, 1.0, TemptItems::Tag("minecraft:panda_food"));
    add.goal(
        6,
        panda(PandaGoal::Avoid(AvoidEntityGoal::no_spectators(
            TargetKind::Player,
            8.0,
            2.0,
            2.0,
        ))),
    );
    add.goal(
        6,
        panda(PandaGoal::Avoid(AvoidEntityGoal::no_spectators(
            TargetKind::Class("Monster"),
            4.0,
            2.0,
            2.0,
        ))),
    );
    add.goal(7, panda(PandaGoal::Sit(PandaSitGoal::default())));
    add.goal(
        8,
        panda(PandaGoal::LieOnBack(PandaLieOnBackGoal::default())),
    );
    add.goal(8, panda(PandaGoal::Sneeze));
    add.goal(
        9,
        panda(PandaGoal::LookAtPlayer(PandaLookAtPlayerGoal::default())),
    );
    add.look_around(10);
    add.goal(12, panda(PandaGoal::Roll));
    add.goal(13, MobGoal::FollowParent(FollowParentGoal::new(1.25)));
    add.stroll(14, 1.0);
    add.target_goal(
        1,
        panda(PandaGoal::HurtByTarget(HurtByTargetGoal::panda_alerting())),
    );
}
