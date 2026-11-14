use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind;

use crate::action::MobAction;
use crate::brain::AiContext;
use crate::goal::{Flag, Flags, Goal, adjusted_tick_delay};
use crate::goals::{MobGoal, TemptItems};
use crate::random::RandomSource;
use crate::random_pos;
use crate::registry::Goals;
use crate::world::AiWorld;
use crate::{BreedGoal, FollowParentGoal, PanicGoal};

pub const RUN_AROUND_PRIORITY: i32 = 1;
pub const RUN_AROUND_SPEED: f64 = 1.2;
pub const BREED_PRIORITY: i32 = 2;
pub const TEMPT_PRIORITY: i32 = 3;
pub const TEMPT_SPEED: f64 = 1.25;
pub const FOLLOW_PARENT_PRIORITY: i32 = 4;
pub const STROLL_PRIORITY: i32 = 6;
pub const STROLL_SPEED: f64 = 0.7;
pub const LOOK_PRIORITY: i32 = 7;
pub const LOOK_DISTANCE: f32 = 6.0;
pub const LOOK_AROUND_PRIORITY: i32 = 8;
pub const STAND_PRIORITY: i32 = 9;
pub const TRAP_PRIORITY: i32 = 1;
pub const PANIC_SPEED: f64 = 1.2;
pub const TRAP_RANGE: f64 = 10.0;
pub const TEMPT_ITEMS: &str = "minecraft:horse_tempt_items";
pub const ZOMBIE_TEMPT_ITEMS: &str = "minecraft:zombie_horse_food";
pub const TAME_CHANCE_ROLL: i32 = 50;
pub const STAND_INTERVAL: i32 = 400;
const RANDOM_POS_HORIZONTAL: i32 = 5;
const RANDOM_POS_VERTICAL: i32 = 4;
const STAND_ROLL: i32 = 1000;
const STAND_CHANCE: i32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EquineFacts {
    pub temper: i32,
    pub max_temper: i32,
    pub standing: bool,
    pub eating: bool,
    pub saddled: bool,
    pub vehicle: bool,
    pub passenger_player: Option<EntityId>,
    pub has_passenger: bool,
    pub mob_controlled: bool,
    pub trap: bool,
    pub can_parent: bool,
}

impl EquineFacts {
    pub fn immobile(&self) -> bool {
        self.eating || self.standing
    }
}

pub fn can_mate(kind: EntityKind, partner: EntityKind) -> bool {
    if kind.extends("Llama") {
        return partner.extends("Llama");
    }
    matches!(kind, EntityKind::Horse | EntityKind::Donkey)
        && matches!(partner, EntityKind::Horse | EntityKind::Donkey)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EquineAction {
    TryTame { player: EntityId },
    Buck { raise_temper: bool },
    Stand,
    SpringTrap,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunAroundLikeCrazyGoal {
    speed_modifier: f64,
    target: Vec3,
}

impl RunAroundLikeCrazyGoal {
    pub const fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            target: Vec3::ZERO,
        }
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let facts = context.mob.species.equine;
        if facts.mob_controlled || context.mob.tamed || !facts.vehicle {
            return false;
        }
        let Some(position) =
            random_pos::default_pos(context, RANDOM_POS_HORIZONTAL, RANDOM_POS_VERTICAL)
        else {
            return false;
        };
        self.target = position;
        true
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.move_to_pos(self.target, self.speed_modifier);
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        !context.mob.tamed
            && !context.brain.navigation.is_done()
            && context.mob.species.equine.vehicle
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        if context.mob.tamed {
            return;
        }
        let bound = adjusted_tick_delay(false, TAME_CHANCE_ROLL);
        if context.brain.random.next_int(bound) != 0 {
            return;
        }
        let facts = context.mob.species.equine;
        if !facts.has_passenger {
            return;
        }
        if let Some(player) = facts.passenger_player {
            if facts.max_temper > 0
                && context.brain.random.next_int(facts.max_temper) < facts.temper
            {
                context.act(MobAction::Equine(EquineAction::TryTame { player }));
                return;
            }
            context.act(MobAction::Equine(EquineAction::Buck { raise_temper: true }));
            return;
        }
        context.act(MobAction::Equine(EquineAction::Buck {
            raise_temper: false,
        }));
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RandomStandGoal {
    next_stand: i32,
}

impl RandomStandGoal {
    pub const fn new() -> Self {
        Self {
            next_stand: -STAND_INTERVAL,
        }
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.next_stand += 1;
        if self.next_stand < 1 {
            return false;
        }
        if self.next_stand <= context.brain.random.next_int(STAND_ROLL) {
            return false;
        }
        self.next_stand = -STAND_INTERVAL;
        !context.mob.species.equine.immobile() && context.brain.random.next_int(STAND_CHANCE) == 0
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.act(MobAction::Equine(EquineAction::Stand));
    }
}

impl Default for RandomStandGoal {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SkeletonTrapGoal;

impl SkeletonTrapGoal {
    fn can_use<W: AiWorld>(context: &mut AiContext<'_, W>) -> bool {
        if !context.mob.species.equine.trap {
            return false;
        }
        let range = TRAP_RANGE * TRAP_RANGE;
        context.world.players().iter().any(|player| {
            player.alive
                && !player.spectator
                && context.mob.distance_to_sqr(player.position) < range
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EquineGoal {
    RunAround(RunAroundLikeCrazyGoal),
    RandomStand(RandomStandGoal),
    SkeletonTrap(SkeletonTrapGoal),
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for EquineGoal {
    fn flags(&self) -> Flags {
        match self {
            Self::RunAround(_) => Flags::of(Flag::Move),
            Self::RandomStand(_) | Self::SkeletonTrap(_) => Flags::NONE,
        }
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::RunAround(goal) => goal.can_use(context),
            Self::RandomStand(goal) => goal.can_use(context),
            Self::SkeletonTrap(_) => SkeletonTrapGoal::can_use(context),
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::RunAround(goal) => goal.can_continue_to_use(context),
            Self::RandomStand(_) => false,
            Self::SkeletonTrap(_) => SkeletonTrapGoal::can_use(context),
        }
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::RunAround(goal) => goal.start(context),
            Self::RandomStand(goal) => goal.start(context),
            Self::SkeletonTrap(_) => {}
        }
    }

    fn requires_update_every_tick(&self) -> bool {
        matches!(self, Self::RandomStand(_))
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::RunAround(goal) => goal.tick(context),
            Self::RandomStand(_) => {}
            Self::SkeletonTrap(_) => context.act(MobAction::Equine(EquineAction::SpringTrap)),
        }
    }
}

pub(crate) fn equine_goals(kind: EntityKind, add: &mut Goals<'_>) {
    add.goal(
        RUN_AROUND_PRIORITY,
        MobGoal::Equine(EquineGoal::RunAround(RunAroundLikeCrazyGoal::new(
            RUN_AROUND_SPEED,
        ))),
    );
    add.goal(BREED_PRIORITY, MobGoal::Breed(BreedGoal::equine(1.0)));
    add.goal(
        FOLLOW_PARENT_PRIORITY,
        MobGoal::FollowParent(FollowParentGoal::new(1.0)),
    );
    add.stroll(STROLL_PRIORITY, STROLL_SPEED);
    add.look(LOOK_PRIORITY, LOOK_DISTANCE);
    add.look_around(LOOK_AROUND_PRIORITY);
    add.goal(
        STAND_PRIORITY,
        MobGoal::Equine(EquineGoal::RandomStand(RandomStandGoal::new())),
    );
    match kind {
        EntityKind::SkeletonHorse => {}
        EntityKind::ZombieHorse => {
            add.float(0);
            add.tempt(
                TEMPT_PRIORITY,
                TEMPT_SPEED,
                TemptItems::Tag(ZOMBIE_TEMPT_ITEMS),
            );
        }
        _ => {
            add.float(0);
            add.goal(1, MobGoal::Panic(PanicGoal::new(PANIC_SPEED)));
            add.tempt(TEMPT_PRIORITY, TEMPT_SPEED, TemptItems::Tag(TEMPT_ITEMS));
        }
    }
    if kind == EntityKind::SkeletonHorse {
        add.goal(
            TRAP_PRIORITY,
            MobGoal::Equine(EquineGoal::SkeletonTrap(SkeletonTrapGoal)),
        );
    }
}
