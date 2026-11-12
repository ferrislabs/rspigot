use rspigot_blocks::BlockKind;
use rspigot_domain::entity::EntityBase;
use rspigot_domain::entity::ageable::DATA_BABY;
use rspigot_domain::entity::living::{JumpTuning, LivingState};
use rspigot_domain::{BlockPos, Vec3, mth};

use crate::action::MobAction;
use crate::animal::block_tag;
use crate::brain::{AiContext, Brain};
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
const JUMP_DELAY_TICKS: i32 = 10;
const PANIC_JUMP_DELAY_TICKS: i32 = 3;
const PANIC_SPEED: f64 = 2.2;
const JUMP_DURATION_TICKS: i32 = 15;
const KILLER_POUNCE_RANGE_SQR: f64 = 16.0;
const STROLL_SPEED: f64 = 0.6;
const BASE_JUMP: f32 = 0.3;
const SLOW_JUMP: f32 = 0.2;
const CLIMB_JUMP: f32 = 0.5;
const CLIMB_MARGIN: f64 = 0.5;
const VANILLA_JUMP_STRENGTH: f32 = 0.42;
const BABY_HOP: f32 = 0.5;
const ADULT_HOP: f32 = 1.5;
const JUMP_PITCH: f32 = 0.8;

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
            self.wants_to_raid = context.mob.more_carrot_ticks < 1;
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
        }
        self.can_raid = false;
        self.next_start_tick = RAID_COOLDOWN;
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HopSound {
    pub position: Vec3,
    pub pitch: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RabbitHop {
    can_jump: bool,
    jump_ticks: i32,
    jump_duration: i32,
    was_on_ground: bool,
    jump_delay_ticks: i32,
    sounds: Vec<HopSound>,
}

impl RabbitHop {
    pub fn jump_ticks(&self) -> i32 {
        self.jump_ticks
    }

    pub fn jump_duration(&self) -> i32 {
        self.jump_duration
    }

    pub fn jump_delay_ticks(&self) -> i32 {
        self.jump_delay_ticks
    }

    pub fn can_jump(&self) -> bool {
        self.can_jump
    }

    fn start_jumping(
        &mut self,
        base: &EntityBase,
        living: &mut LivingState,
        brain_random: &mut impl RandomSource,
    ) {
        living.jumping = true;
        let first = brain_random.next_float();
        let second = brain_random.next_float();
        self.sounds.push(HopSound {
            position: base.position,
            pitch: ((first - second) * 0.2 + 1.0) * JUMP_PITCH,
        });
        self.jump_duration = JUMP_DURATION_TICKS;
        self.jump_ticks = 0;
    }
}

fn face_point(base: &mut EntityBase, x: f64, z: f64) {
    let yaw = (mth::atan2(z - base.position.z, x - base.position.x) * mth::RAD_TO_DEG) as f32;
    base.set_yaw(yaw - 90.0);
}

fn set_speed_modifier(brain: &mut Brain, speed: f64) {
    brain.navigation.set_speed_modifier(speed);
    let wanted = brain.movement.wanted();
    brain.movement.set_wanted_position(wanted, speed);
}

impl Brain {
    pub fn enable_hop(&mut self) {
        self.hop = Some(RabbitHop::default());
        self.movement.enable_hop();
    }

    pub fn hop(&self) -> Option<&RabbitHop> {
        self.hop.as_ref()
    }

    pub(crate) fn hop_move_pre_tick(&mut self, base: &EntityBase, living: &LivingState) {
        if self.hop.is_none() {
            return;
        }
        self.movement.set_in_water(base.fluid.in_water);
        if base.on_ground && !living.jumping && !self.jump.is_jumping() {
            set_speed_modifier(self, 0.0);
        } else if self.movement.has_wanted()
            || self.movement.operation() == crate::control::Operation::Jumping
        {
            let speed = self.movement.next_jump_speed();
            set_speed_modifier(self, speed);
        }
    }

    pub(crate) fn hop_jump_tick(&mut self, base: &EntityBase, living: &mut LivingState) {
        let Some(mut hop) = self.hop.take() else {
            return;
        };
        if self.jump.take() {
            hop.start_jumping(base, living, &mut self.random);
        }
        self.hop = Some(hop);
    }

    pub(crate) fn hop_tune_jump(&self, base: &EntityBase, living: &mut LivingState) {
        if self.hop.is_none() {
            return;
        }
        let mut power = BASE_JUMP;
        if self.movement.speed_modifier() <= STROLL_SPEED {
            power = SLOW_JUMP;
        }
        if let Some(path) = self.navigation.path().filter(|path| !path.is_done())
            && path.next_entity_pos(self.profile.width).y > base.position.y + CLIMB_MARGIN
        {
            power = CLIMB_JUMP;
        }
        if base.horizontal_collision
            || (living.jumping && self.movement.wanted().y > base.position.y + CLIMB_MARGIN)
        {
            power = CLIMB_JUMP;
        }
        let lift = (self.movement.speed_modifier() > 0.0).then(|| {
            if base.data.boolean(DATA_BABY) {
                BABY_HOP
            } else {
                ADULT_HOP
            }
        });
        living.jump_tuning = JumpTuning {
            multiplier: power / VANILLA_JUMP_STRENGTH,
            hop: lift,
        };
    }

    pub fn rabbit_ai_step(
        &mut self,
        base: &mut EntityBase,
        living: &mut LivingState,
        evil: bool,
        target: Option<Vec3>,
        more_carrot_ticks: &mut i32,
    ) {
        let Some(mut hop) = self.hop.take() else {
            return;
        };
        self.movement.set_in_water(base.fluid.in_water);
        if hop.jump_delay_ticks > 0 {
            hop.jump_delay_ticks -= 1;
        }
        if *more_carrot_ticks > 0 {
            *more_carrot_ticks -= self.random.next_int(3);
            if *more_carrot_ticks < 0 {
                *more_carrot_ticks = 0;
            }
        }
        if base.on_ground {
            if !hop.was_on_ground {
                living.jumping = false;
                hop.jump_delay_ticks = if self.movement.speed_modifier() < PANIC_SPEED {
                    JUMP_DELAY_TICKS
                } else {
                    PANIC_JUMP_DELAY_TICKS
                };
                hop.can_jump = false;
            }
            if evil
                && hop.jump_delay_ticks == 0
                && let Some(at) = target
                && base.position.distance_squared(at) < KILLER_POUNCE_RANGE_SQR
            {
                face_point(base, at.x, at.z);
                let speed = self.movement.speed_modifier();
                self.movement.set_wanted_position(at, speed);
                hop.start_jumping(base, living, &mut self.random);
                hop.was_on_ground = true;
            }
            if !self.jump.is_jumping() {
                if self.movement.has_wanted() && hop.jump_delay_ticks == 0 {
                    let mut at = self.movement.wanted();
                    if let Some(path) = self.navigation.path().filter(|path| !path.is_done()) {
                        at = path.next_entity_pos(self.profile.width);
                    }
                    face_point(base, at.x, at.z);
                    hop.start_jumping(base, living, &mut self.random);
                }
            } else if !hop.can_jump {
                hop.can_jump = true;
            }
        }
        hop.was_on_ground = base.on_ground;
        self.hop = Some(hop);
    }

    pub fn rabbit_post_ai_step(&mut self, living: &mut LivingState) {
        let Some(hop) = self.hop.as_mut() else {
            return;
        };
        if hop.jump_ticks != hop.jump_duration {
            hop.jump_ticks += 1;
        } else if hop.jump_duration != 0 {
            hop.jump_ticks = 0;
            hop.jump_duration = 0;
            living.jumping = false;
        }
    }

    pub fn take_hop_sounds(&mut self) -> Vec<HopSound> {
        self.hop
            .as_mut()
            .map(|hop| std::mem::take(&mut hop.sounds))
            .unwrap_or_default()
    }
}
