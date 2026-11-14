use crate::brain::AiContext;
use crate::combat::look_at_entity;
use crate::goal::{Flag, Flags, Goal, adjusted_tick_delay, reduced_tick_delay};
use crate::random::RandomSource;
use crate::targeting;
use crate::world::AiWorld;

const FLOAT_SPEED: f64 = 1.2;
const FLOAT_JUMP_CHANCE: f32 = 0.8;
const KEEP_JUMPING_SPEED: f64 = 1.0;
const GROW_TIRED_TICKS: i32 = 300;
const ATTACK_LOOK_SPEED: f32 = 10.0;
const DIRECTION_INTERVAL_RANGE: i32 = 60;
const DIRECTION_INTERVAL_BASE: i32 = 40;
const DIRECTION_DEGREES: i32 = 360;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CubeFacts {
    pub can_deal_damage: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubeSound {
    pub sound: &'static str,
    pub volume: f32,
    pub pitch: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloatGoal;

impl<W: AiWorld> Goal<AiContext<'_, W>> for FloatGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Jump).with(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.mob.in_liquid()
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        if context.brain.random.next_float() < FLOAT_JUMP_CHANCE {
            context.brain.jump.jump();
        }
        context.brain.movement.set_cube_wanted(FLOAT_SPEED);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RandomDirectionGoal {
    chosen_degrees: f32,
    next_randomize_time: i32,
}

impl RandomDirectionGoal {
    pub fn chosen_degrees(&self) -> f32 {
        self.chosen_degrees
    }

    pub fn next_randomize_time(&self) -> i32 {
        self.next_randomize_time
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for RandomDirectionGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.brain.target.is_none()
            && (context.mob.on_ground || context.mob.in_liquid() || context.mob.levitating)
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        self.next_randomize_time -= 1;
        if self.next_randomize_time < 1 {
            let interval =
                context.brain.random.next_int(DIRECTION_INTERVAL_RANGE) + DIRECTION_INTERVAL_BASE;
            self.next_randomize_time = adjusted_tick_delay(false, interval);
            self.chosen_degrees = context.brain.random.next_int(DIRECTION_DEGREES) as f32;
        }
        context
            .brain
            .movement
            .set_cube_direction(self.chosen_degrees, false);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttackGoal {
    grow_tired_timer: i32,
}

impl AttackGoal {
    pub fn grow_tired_timer(&self) -> i32 {
        self.grow_tired_timer
    }

    fn attackable<W: AiWorld>(context: &AiContext<'_, W>) -> Option<crate::Living> {
        let target = context.target()?;
        targeting::mob_can_attack(context.mob, context.world, &target).then_some(target)
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for AttackGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        Self::attackable(context).is_some()
    }

    fn start(&mut self, _context: &mut AiContext<'_, W>) {
        self.grow_tired_timer = reduced_tick_delay(GROW_TIRED_TICKS);
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if Self::attackable(context).is_none() {
            return false;
        }
        self.grow_tired_timer -= 1;
        self.grow_tired_timer >= 1
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        if let Some(target) = context.target() {
            look_at_entity(
                context.mob,
                context.body,
                &target,
                ATTACK_LOOK_SPEED,
                ATTACK_LOOK_SPEED,
            );
        }
        let aggressive = context.mob.species.cube.can_deal_damage;
        context
            .brain
            .movement
            .set_cube_direction(context.body.yaw, aggressive);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeepOnJumpingGoal;

impl<W: AiWorld> Goal<AiContext<'_, W>> for KeepOnJumpingGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Jump).with(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        !context.mob.passenger
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        context.brain.movement.set_cube_wanted(KEEP_JUMPING_SPEED);
    }
}

goal_enum!(CubeGoal {
    Float(FloatGoal),
    RandomDirection(RandomDirectionGoal),
    Attack(AttackGoal),
    KeepOnJumping(KeepOnJumpingGoal),
});
