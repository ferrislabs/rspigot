use rspigot_blocks::BlockKind;
use rspigot_domain::{BlockPos, Vec3};

use crate::action::MobAction;
use crate::animal::block_tag;
use crate::brain::AiContext;
use crate::control::MAX_HEAD_X_ROT;
use crate::goal::reduced_tick_delay;
use crate::random::RandomSource;
use crate::scan::closer_to_center_than;
use crate::world::AiWorld;

const SEARCH_RANGE: i32 = 16;
const VERTICAL_RANGE: i32 = 1;
const SPEED_MODIFIER: f64 = 0.7;
const ACCEPTED_DISTANCE: f64 = 1.0;
const RECALCULATE_PERIOD: i32 = 40;
const GIVE_UP_TICKS: i32 = 1200;
const RAID_COOLDOWN: i32 = 10;
const LOOK_SPEED: f32 = 10.0;
const SUPPORTS_CROPS: &str = "minecraft:supports_crops";
const MAX_CARROT_AGE: &str = "7";
pub const MORE_CARROT_TICKS: i32 = 40;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RaidGardenGoal {
    next_start_tick: i32,
    try_ticks: i32,
    max_stay_ticks: i32,
    block_pos: BlockPos,
    reached_target: bool,
    wants_to_raid: bool,
    can_raid: bool,
}

impl Default for RaidGardenGoal {
    fn default() -> Self {
        Self {
            next_start_tick: 0,
            try_ticks: 0,
            max_stay_ticks: 0,
            block_pos: BlockPos::new(0, 0, 0),
            reached_target: false,
            wants_to_raid: false,
            can_raid: false,
        }
    }
}

impl RaidGardenGoal {
    fn is_valid_target<W: AiWorld>(&mut self, context: &AiContext<'_, W>, pos: BlockPos) -> bool {
        let world = context.world;
        if !block_tag(world.state(pos), SUPPORTS_CROPS) || !self.wants_to_raid || self.can_raid {
            return false;
        }
        let ripe = world.state(pos.offset(0, 1, 0)).is_some_and(|state| {
            state.kind() == Some(BlockKind::Carrots)
                && state.property("age") == Some(MAX_CARROT_AGE)
        });
        if ripe {
            self.can_raid = true;
        }
        ripe
    }

    fn find_nearest_block<W: AiWorld>(&mut self, context: &AiContext<'_, W>) -> bool {
        let origin = context.mob.block_pos();
        let mut dy = 0;
        while dy <= VERTICAL_RANGE {
            for radius in 0..SEARCH_RANGE {
                let mut dx = 0;
                while dx <= radius {
                    let mut dz = if dx < radius && dx > -radius {
                        radius
                    } else {
                        0
                    };
                    while dz <= radius {
                        let pos = origin.offset(dx, dy - 1, dz);
                        if context.mob.is_within_home(pos) && self.is_valid_target(context, pos) {
                            self.block_pos = pos;
                            return true;
                        }
                        dz = if dz > 0 { -dz } else { 1 - dz };
                    }
                    dx = if dx > 0 { -dx } else { 1 - dx };
                }
            }
            dy = if dy > 0 { -dy } else { 1 - dy };
        }
        false
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if self.next_start_tick < 1 {
            if !context.world.mob_griefing() {
                return false;
            }
            self.can_raid = false;
            self.wants_to_raid = context.brain.more_carrot_ticks < 1;
        }
        if self.next_start_tick > 0 {
            self.next_start_tick -= 1;
            return false;
        }
        let delay = 200 + context.brain.random.next_int(200);
        self.next_start_tick = reduced_tick_delay(delay);
        self.find_nearest_block(context)
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        if !self.can_raid || self.try_ticks < -self.max_stay_ticks || self.try_ticks > GIVE_UP_TICKS
        {
            return false;
        }
        let pos = self.block_pos;
        self.is_valid_target(context, pos)
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let pos = self.block_pos;
        let at = Vec3::new(
            f64::from(pos.x) + 0.5,
            f64::from(pos.y + 1),
            f64::from(pos.z) + 0.5,
        );
        context.move_to_pos(at, SPEED_MODIFIER);
        self.try_ticks = 0;
        let inner = context.brain.random.next_int(1200) + 1200;
        self.max_stay_ticks = context.brain.random.next_int(inner) + 1200;
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let target = self.block_pos.offset(0, 1, 0);
        if closer_to_center_than(target, context.mob.position, ACCEPTED_DISTANCE) {
            self.reached_target = true;
            self.try_ticks -= 1;
        } else {
            self.reached_target = false;
            self.try_ticks += 1;
            if self.try_ticks % RECALCULATE_PERIOD == 0 {
                let at = Vec3::new(
                    f64::from(target.x) + 0.5,
                    f64::from(target.y),
                    f64::from(target.z) + 0.5,
                );
                context.move_to_pos(at, SPEED_MODIFIER);
            }
        }
        let look = Vec3::new(
            f64::from(self.block_pos.x) + 0.5,
            f64::from(self.block_pos.y + 1),
            f64::from(self.block_pos.z) + 0.5,
        );
        context
            .brain
            .look
            .set_look_at(look, LOOK_SPEED, MAX_HEAD_X_ROT);
        if !self.reached_target {
            return;
        }
        if self.can_raid
            && context
                .world
                .state(target)
                .is_some_and(|state| state.kind() == Some(BlockKind::Carrots))
        {
            context.act(MobAction::RaidCarrot(target));
            context.brain.more_carrot_ticks = MORE_CARROT_TICKS;
        }
        self.can_raid = false;
        self.next_start_tick = RAID_COOLDOWN;
    }
}
