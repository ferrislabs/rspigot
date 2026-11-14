use rspigot_blocks::{BlockKind, StateFlags};
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;

use crate::action::MobAction;
use crate::brain::AiContext;
use crate::goal::adjusted_tick_delay;
use crate::random::RandomSource;
use crate::random_pos;
use crate::scan::manhattan_ordered;
use crate::targeting::TargetingConditions;
use crate::world::{AiWorld, Living, TargetKind, inflate};

const WATER_SEARCH_RANGE: i32 = 5;
const WATER_SEARCH_HEIGHT: i32 = 1;
const PARTNER_RANGE: f64 = 8.0;
const BREED_DELAY: i32 = 60;
const BREED_DISTANCE_SQR: f64 = 9.0;
const BREED_LOOK_SPEED: f32 = 10.0;
const PARENT_SCAN: (f64, f64, f64) = (8.0, 4.0, 8.0);
const PARENT_MIN_DISTANCE_SQR: f64 = 9.0;
const PARENT_MAX_DISTANCE_SQR: f64 = 256.0;
const PARENT_RECALC: i32 = 10;
const EAT_ANIMATION_TICKS: i32 = 40;
const EAT_TICK: i32 = 4;
const BABY_EAT_CHANCE: i32 = 50;
const ADULT_EAT_CHANCE: i32 = 1000;
pub const EAT_GRASS_EVENT: i8 = 10;
const EDIBLE_FOR_SHEEP: &str = "minecraft:edible_for_sheep";

pub(crate) fn kind_in_block_tag(kind: BlockKind, tag: &str) -> bool {
    rspigot_data::tags::tag("minecraft:block", tag)
        .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
}

pub(crate) fn block_tag(state: Option<rspigot_blocks::StateId>, tag: &str) -> bool {
    state.and_then(|state| state.kind()).is_some_and(|kind| {
        rspigot_data::tags::tag("minecraft:block", tag)
            .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanicGoal {
    speed_modifier: f64,
    wanted: Vec3,
    running: bool,
    rabbit: bool,
    tamable: bool,
    environmental: bool,
}

impl PanicGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            wanted: Vec3::ZERO,
            running: false,
            rabbit: false,
            tamable: false,
            environmental: false,
        }
    }

    pub fn tamable(speed_modifier: f64) -> Self {
        Self {
            tamable: true,
            ..Self::new(speed_modifier)
        }
    }

    pub fn tamable_environmental(speed_modifier: f64) -> Self {
        Self {
            environmental: true,
            ..Self::tamable(speed_modifier)
        }
    }

    pub fn rabbit(speed_modifier: f64) -> Self {
        Self {
            rabbit: true,
            ..Self::new(speed_modifier)
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    fn look_for_water<W: AiWorld>(context: &AiContext<'_, W>) -> Option<BlockPos> {
        let origin = context.mob.block_pos();
        let world = context.world;
        if world
            .state(origin)
            .is_none_or(|state| !state.collision_shape().is_empty())
        {
            return None;
        }
        manhattan_ordered(
            origin,
            WATER_SEARCH_RANGE,
            WATER_SEARCH_HEIGHT,
            WATER_SEARCH_RANGE,
        )
        .find(|pos| {
            world
                .state(*pos)
                .is_some_and(|state| state.flags().contains(StateFlags::WATER))
        })
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let panic = if self.environmental {
            context.mob.panic_environmental
        } else {
            context.mob.panic
        };
        if !panic {
            return false;
        }
        if context.mob.on_fire
            && let Some(water) = Self::look_for_water(context)
        {
            self.wanted = Vec3::new(f64::from(water.x), f64::from(water.y), f64::from(water.z));
            return true;
        }
        let Some(position) = random_pos::default_pos(context, 5, 4) else {
            return false;
        };
        self.wanted = position;
        true
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.move_to_pos(self.wanted, self.speed_modifier);
        self.running = true;
    }

    pub(crate) fn stop(&mut self) {
        self.running = false;
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        if self.tamable {
            crate::tame::teleport_to_distant_owner(context);
        }
        if !self.rabbit {
            return;
        }
        let speed = self.speed_modifier;
        context.brain.navigation.set_speed_modifier(speed);
        let wanted = context.brain.movement.wanted();
        context.brain.movement.set_wanted_position(wanted, speed);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BreedGoal {
    speed_modifier: f64,
    partner: Option<EntityId>,
    love_time: i32,
}

impl BreedGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            partner: None,
            love_time: 0,
        }
    }

    pub(crate) fn partner(&self) -> Option<EntityId> {
        self.partner
    }

    fn free_partner<W: AiWorld>(context: &mut AiContext<'_, W>) -> Option<Living> {
        let mob = context.mob;
        let area = inflate(&mob.bounds(), PARTNER_RANGE, PARTNER_RANGE, PARTNER_RANGE);
        let candidates = context
            .world
            .living_of(TargetKind::Class(mob.kind.class()), &area);
        let conditions = TargetingConditions::for_non_combat()
            .range(PARTNER_RANGE)
            .ignore_line_of_sight();
        let mut best = f64::MAX;
        let mut partner = None;
        for candidate in candidates {
            if !conditions.test(context, &candidate, |_| true) {
                continue;
            }
            let mob = context.mob;
            let can_mate = candidate.id != mob.id
                && candidate.kind == mob.kind
                && mob.in_love
                && candidate.in_love
                && (mob.kind != EntityKind::Wolf
                    || (mob.tamed && candidate.facts.tamed && !candidate.facts.sitting))
                && (mob.kind != EntityKind::Cat || (mob.tamed && candidate.facts.tamed));
            let distance = mob.distance_to_sqr(candidate.position);
            if can_mate && !candidate.panicking && distance < best {
                best = distance;
                partner = Some(candidate);
            }
        }
        partner
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if !context.mob.in_love {
            return false;
        }
        self.partner = Self::free_partner(context).map(|partner| partner.id);
        self.partner.is_some()
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        let Some(partner) = self.partner.and_then(|id| context.world.living(id)) else {
            return false;
        };
        partner.alive && partner.in_love && self.love_time < BREED_DELAY && !partner.panicking
    }

    pub(crate) fn stop(&mut self) {
        self.partner = None;
        self.love_time = 0;
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(partner) = self.partner.and_then(|id| context.world.living(id)) else {
            return;
        };
        let eye = Vec3::new(partner.position.x, partner.eye_y(), partner.position.z);
        context
            .brain
            .look
            .set_look_at(eye, BREED_LOOK_SPEED, context.mob.max_head_x_rot());
        context.move_to_entity(&partner, self.speed_modifier);
        self.love_time += 1;
        if self.love_time >= adjusted_tick_delay(false, BREED_DELAY)
            && context.mob.distance_to_sqr(partner.position) < BREED_DISTANCE_SQR
        {
            context.act(MobAction::Breed(partner.id));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FollowParentGoal {
    speed_modifier: f64,
    parent: Option<EntityId>,
    time_to_recalc_path: i32,
}

impl FollowParentGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            parent: None,
            time_to_recalc_path: 0,
        }
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let mob = context.mob;
        if mob.age >= 0 {
            return false;
        }
        let area = inflate(&mob.bounds(), PARENT_SCAN.0, PARENT_SCAN.1, PARENT_SCAN.2);
        let candidates = context
            .world
            .living_of(TargetKind::Class(mob.kind.class()), &area);
        let mut closest = None;
        let mut best = f64::MAX;
        for candidate in candidates {
            if candidate.age < 0 {
                continue;
            }
            let distance = mob.distance_to_sqr(candidate.position);
            if distance <= best {
                best = distance;
                closest = Some(candidate.id);
            }
        }
        if closest.is_none() || best < PARENT_MIN_DISTANCE_SQR {
            return false;
        }
        self.parent = closest;
        true
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        if context.mob.age >= 0 {
            return false;
        }
        let Some(parent) = self.parent.and_then(|id| context.world.living(id)) else {
            return false;
        };
        if !parent.alive {
            return false;
        }
        let distance = context.mob.distance_to_sqr(parent.position);
        (PARENT_MIN_DISTANCE_SQR..=PARENT_MAX_DISTANCE_SQR).contains(&distance)
    }

    pub(crate) fn start(&mut self) {
        self.time_to_recalc_path = 0;
    }

    pub(crate) fn stop(&mut self) {
        self.parent = None;
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.time_to_recalc_path -= 1;
        if self.time_to_recalc_path > 0 {
            return;
        }
        self.time_to_recalc_path = adjusted_tick_delay(false, PARENT_RECALC);
        if let Some(parent) = self.parent.and_then(|id| context.world.living(id)) {
            context.move_to_entity(&parent, self.speed_modifier);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EatBlockGoal {
    eat_animation_tick: i32,
}

impl EatBlockGoal {
    pub fn eat_animation_tick(&self) -> i32 {
        self.eat_animation_tick
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let chance = if context.mob.baby {
            BABY_EAT_CHANCE
        } else {
            ADULT_EAT_CHANCE
        };
        if context
            .brain
            .random
            .next_int(adjusted_tick_delay(false, chance))
            != 0
        {
            return false;
        }
        let pos = context.mob.block_pos();
        block_tag(context.world.state(pos), EDIBLE_FOR_SHEEP)
            || context
                .world
                .state(pos.offset(0, -1, 0))
                .is_some_and(|state| state.kind() == Some(BlockKind::GrassBlock))
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.eat_animation_tick = adjusted_tick_delay(false, EAT_ANIMATION_TICKS);
        context.act(MobAction::EntityEvent(EAT_GRASS_EVENT));
        context.stop_navigation();
    }

    pub(crate) fn stop(&mut self) {
        self.eat_animation_tick = 0;
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.eat_animation_tick = (self.eat_animation_tick - 1).max(0);
        if self.eat_animation_tick == adjusted_tick_delay(false, EAT_TICK) {
            context.act(MobAction::EatGrass);
        }
    }
}

pub fn is_edible_for_sheep(state: Option<rspigot_blocks::StateId>) -> bool {
    block_tag(state, EDIBLE_FOR_SHEEP)
}
