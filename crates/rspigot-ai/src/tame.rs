use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;

use crate::MobState;
use crate::PathType;
use crate::action::{MobAction, TargetReason};
use crate::animal::block_tag;
use crate::brain::AiContext;
use crate::combat::TargetMemory;
use crate::goal::adjusted_tick_delay;
use crate::random::RandomSource;
use crate::targeting::{self, TargetingConditions};
use crate::view::collides;
use crate::walk::mob_path_type;
use crate::world::{AiWorld, Living, MobView};

const TELEPORT_DISTANCE_SQR: f64 = 144.0;
const TELEPORT_ATTEMPTS: usize = 10;
const TELEPORT_HORIZONTAL: i32 = 3;
const TELEPORT_MIN_HORIZONTAL: i32 = 2;
const TELEPORT_VERTICAL: i32 = 1;
const OWNER_LOOK_SPEED: f32 = 10.0;
const RECALC_PERIOD: i32 = 10;
const RETALIATION_MEMORY_DISTANCE_SQR: f64 = 144.0;
const LEAVES: &str = "minecraft:leaves";

pub fn unable_to_move_to_owner(mob: &MobView, owner: Option<&Living>) -> bool {
    mob.ordered_to_sit || mob.passenger || mob.leashed || owner.is_some_and(|owner| owner.spectator)
}

pub fn should_try_teleport_to_owner(mob: &MobView, owner: &Living) -> bool {
    mob.distance_to_sqr(owner.position) >= TELEPORT_DISTANCE_SQR
}

pub(crate) fn teleport_to_distant_owner<W: AiWorld>(context: &mut AiContext<'_, W>) {
    let owner = owner_of(context);
    if unable_to_move_to_owner(context.mob, owner.as_ref()) {
        return;
    }
    if let Some(owner) = owner
        && should_try_teleport_to_owner(context.mob, &owner)
    {
        try_to_teleport_to_owner(context, &owner);
    }
}

pub(crate) fn owner_of<W: AiWorld>(context: &AiContext<'_, W>) -> Option<Living> {
    context
        .mob
        .owner
        .and_then(|id| context.world.living(id))
        .filter(|owner| owner.alive)
}

pub fn wants_to_attack<W: AiWorld>(
    context: &AiContext<'_, W>,
    target: &Living,
    owner: &Living,
) -> bool {
    match context.mob.kind {
        EntityKind::Wolf => wolf_wants_to_attack(context, target, owner),
        _ => true,
    }
}

fn wolf_wants_to_attack<W: AiWorld>(
    context: &AiContext<'_, W>,
    target: &Living,
    owner: &Living,
) -> bool {
    if matches!(
        target.kind,
        EntityKind::Creeper | EntityKind::Ghast | EntityKind::ArmorStand
    ) {
        return false;
    }
    if target.kind == EntityKind::Wolf {
        return !target.facts.tamed || target.facts.owner != Some(owner.id);
    }
    if target.player && owner.player && !context.world.can_harm_player(owner.id, target.id) {
        return false;
    }
    if target.kind.extends("AbstractHorse") && target.facts.tamed {
        return false;
    }
    !(target.kind.extends("TamableAnimal") && target.facts.tamed)
}

fn can_teleport_to<W: AiWorld>(context: &AiContext<'_, W>, pos: BlockPos) -> bool {
    let mob = context.mob;
    let state = MobState {
        position: mob.position,
        on_ground: mob.on_ground,
        in_floatable_fluid: mob.fluid.in_water,
    };
    if mob_path_type(context.world, &context.brain.profile, state, pos) != PathType::Walkable {
        return false;
    }
    let can_fly = context.mob.kind == rspigot_entities::EntityKind::Parrot;
    if !can_fly && block_tag(context.world.state(pos.offset(0, -1, 0)), LEAVES) {
        return false;
    }
    let origin = mob.block_pos();
    let bounds = mob.bounds();
    let moved = rspigot_blocks::Aabb::new(
        bounds.min_x + f64::from(pos.x - origin.x),
        bounds.min_y + f64::from(pos.y - origin.y),
        bounds.min_z + f64::from(pos.z - origin.z),
        bounds.max_x + f64::from(pos.x - origin.x),
        bounds.max_y + f64::from(pos.y - origin.y),
        bounds.max_z + f64::from(pos.z - origin.z),
    );
    !collides(context.world, &moved)
}

pub fn try_to_teleport_to_owner<W: AiWorld>(context: &mut AiContext<'_, W>, owner: &Living) {
    let around = owner.position.block();
    for _ in 0..TELEPORT_ATTEMPTS {
        let dx = context
            .brain
            .random
            .next_int_between(-TELEPORT_HORIZONTAL, TELEPORT_HORIZONTAL);
        let dz = context
            .brain
            .random
            .next_int_between(-TELEPORT_HORIZONTAL, TELEPORT_HORIZONTAL);
        if dx.abs() < TELEPORT_MIN_HORIZONTAL && dz.abs() < TELEPORT_MIN_HORIZONTAL {
            continue;
        }
        let dy = context
            .brain
            .random
            .next_int_between(-TELEPORT_VERTICAL, TELEPORT_VERTICAL);
        let pos = around.offset(dx, dy, dz);
        if can_teleport_to(context, pos) {
            context.act(MobAction::TeleportTo(Vec3::new(
                f64::from(pos.x) + 0.5,
                f64::from(pos.y),
                f64::from(pos.z) + 0.5,
            )));
            context.stop_navigation();
            return;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SitWhenOrderedToGoal;

impl SitWhenOrderedToGoal {
    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let mob = context.mob;
        let ordered = mob.ordered_to_sit;
        if !ordered && !mob.tamed {
            return false;
        }
        if mob.fluid.in_water || !mob.on_ground {
            return false;
        }
        let Some(owner) = mob.owner.and_then(|id| context.world.living(id)) else {
            return true;
        };
        if mob.distance_to_sqr(owner.position) < RETALIATION_MEMORY_DISTANCE_SQR
            && owner.facts.last_hurt_by_mob.is_some()
        {
            return false;
        }
        ordered
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        context.mob.ordered_to_sit
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.stop_navigation();
        context.act(MobAction::SetSitting(true));
    }

    pub(crate) fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.act(MobAction::SetSitting(false));
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FollowOwnerGoal {
    speed_modifier: f64,
    start_distance: f32,
    stop_distance: f32,
    owner: Option<EntityId>,
    time_to_recalc_path: i32,
    old_water_cost: f32,
}

impl FollowOwnerGoal {
    pub fn new(speed_modifier: f64, start_distance: f32, stop_distance: f32) -> Self {
        Self {
            speed_modifier,
            start_distance,
            stop_distance,
            owner: None,
            time_to_recalc_path: 0,
            old_water_cost: 0.0,
        }
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(owner) = owner_of(context) else {
            return false;
        };
        if unable_to_move_to_owner(context.mob, Some(&owner)) {
            return false;
        }
        let start = f64::from(self.start_distance * self.start_distance);
        if context.mob.distance_to_sqr(owner.position) < start {
            return false;
        }
        self.owner = Some(owner.id);
        true
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        if context.brain.navigation.is_done() {
            return false;
        }
        let owner = owner_of(context);
        if unable_to_move_to_owner(context.mob, owner.as_ref()) {
            return false;
        }
        let Some(owner) = self.owner.and_then(|id| context.world.living(id)) else {
            return false;
        };
        let stop = f64::from(self.stop_distance * self.stop_distance);
        context.mob.distance_to_sqr(owner.position) > stop
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.time_to_recalc_path = 0;
        self.old_water_cost = context.brain.profile.malus(PathType::Water);
        context.brain.profile.set_malus(PathType::Water, 0.0);
    }

    pub(crate) fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.owner = None;
        context.stop_navigation();
        let cost = self.old_water_cost;
        context.brain.profile.set_malus(PathType::Water, cost);
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(owner) = self.owner.and_then(|id| context.world.living(id)) else {
            return;
        };
        let far = should_try_teleport_to_owner(context.mob, &owner);
        if !far {
            let at = Vec3::new(owner.position.x, owner.eye_y(), owner.position.z);
            context
                .brain
                .look
                .set_look_at(at, OWNER_LOOK_SPEED, context.mob.max_head_x_rot());
        }
        self.time_to_recalc_path -= 1;
        if self.time_to_recalc_path <= 0 {
            self.time_to_recalc_path = adjusted_tick_delay(false, RECALC_PERIOD);
            if far {
                try_to_teleport_to_owner(context, &owner);
            } else {
                context.move_to_entity(&owner, self.speed_modifier);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OwnerEvent {
    HurtBy,
    HurtTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerTargetGoal {
    event: OwnerEvent,
    memory: TargetMemory,
    victim: Option<EntityId>,
    timestamp: i32,
}

impl OwnerTargetGoal {
    pub const fn hurt_by() -> Self {
        Self::of(OwnerEvent::HurtBy)
    }

    pub const fn hurt_target() -> Self {
        Self::of(OwnerEvent::HurtTarget)
    }

    const fn of(event: OwnerEvent) -> Self {
        Self {
            event,
            memory: TargetMemory::new(false),
            victim: None,
            timestamp: 0,
        }
    }

    pub(crate) fn memory(&mut self) -> &mut TargetMemory {
        &mut self.memory
    }

    fn owner_stamp(&self, owner: &Living) -> i32 {
        match self.event {
            OwnerEvent::HurtBy => owner.facts.last_hurt_by_mob_at,
            OwnerEvent::HurtTarget => owner.facts.last_hurt_mob_at,
        }
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if !context.mob.tamed || context.mob.ordered_to_sit {
            return false;
        }
        let Some(owner) = owner_of(context) else {
            return false;
        };
        let victim = match self.event {
            OwnerEvent::HurtBy => {
                if !owner.facts.retaliable {
                    return false;
                }
                owner.facts.last_hurt_by_mob
            }
            OwnerEvent::HurtTarget => owner.facts.last_hurt_mob,
        };
        self.victim = victim;
        if self.owner_stamp(&owner) == self.timestamp {
            return false;
        }
        let Some(victim) = victim.and_then(|id| context.world.living(id)) else {
            return false;
        };
        targeting::goal_can_attack(context, &victim, TargetingConditions::for_combat())
            && wants_to_attack(context, &victim, &owner)
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let victim = self.victim.and_then(|id| context.world.living(id));
        let reason = match self.event {
            OwnerEvent::HurtBy => TargetReason::TargetAttackedOwner,
            OwnerEvent::HurtTarget => TargetReason::OwnerAttackedTarget,
        };
        context.set_target(victim, reason);
        if let Some(owner) = owner_of(context) {
            self.timestamp = self.owner_stamp(&owner);
        }
        self.memory.start();
    }
}
