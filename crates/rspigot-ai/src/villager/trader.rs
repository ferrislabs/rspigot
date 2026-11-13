use rspigot_domain::{BlockPos, Vec3};

use crate::action::MobAction;
use crate::brain::AiContext;
use crate::goal::{Flag, Flags, Goal};
use crate::scan::closer_to_center_than;
use crate::villager::VillagerAction;
use crate::world::AiWorld;

const FAR_DISTANCE: f64 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TraderFacts {
    pub wander_target: Option<BlockPos>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TraderGoal {
    WanderToPosition {
        stop_distance: f64,
        speed_modifier: f64,
    },
}

impl TraderGoal {
    pub fn wander_to_position(stop_distance: f64, speed_modifier: f64) -> Self {
        Self::WanderToPosition {
            stop_distance,
            speed_modifier,
        }
    }
}

fn is_too_far_away<W: AiWorld>(context: &AiContext<'_, W>, pos: BlockPos, distance: f64) -> bool {
    !closer_to_center_than(pos, context.mob.position, distance)
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for TraderGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Self::WanderToPosition { stop_distance, .. } = *self;
        context
            .mob
            .species
            .trader
            .wander_target
            .is_some_and(|target| is_too_far_away(context, target, stop_distance))
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        context.act(MobAction::Villager(VillagerAction::ClearWanderTarget));
        context.stop_navigation();
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let Self::WanderToPosition { speed_modifier, .. } = *self;
        let Some(target) = context.mob.species.trader.wander_target else {
            return;
        };
        if !context.brain.navigation.is_done() {
            return;
        }
        let here = context.mob.position;
        if is_too_far_away(context, target, FAR_DISTANCE) {
            let delta = Vec3::new(
                f64::from(target.x) - here.x,
                f64::from(target.y) - here.y,
                f64::from(target.z) - here.z,
            );
            let length = (delta.x * delta.x + delta.y * delta.y + delta.z * delta.z).sqrt();
            let scale = if length > 0.0 {
                FAR_DISTANCE / length
            } else {
                0.0
            };
            let toward = Vec3::new(
                delta.x * scale + here.x,
                delta.y * scale + here.y,
                delta.z * scale + here.z,
            );
            context.move_to_pos(toward, speed_modifier);
        } else {
            context.move_to_pos(
                Vec3::new(
                    f64::from(target.x),
                    f64::from(target.y),
                    f64::from(target.z),
                ),
                speed_modifier,
            );
        }
    }
}
