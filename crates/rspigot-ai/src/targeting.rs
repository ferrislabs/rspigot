use rspigot_domain::Vec3;
use rspigot_entities::EntityKind;

use crate::brain::AiContext;
use crate::world::{AiWorld, Living, MobView};

const MIN_VISIBILITY_DISTANCE: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TargetingConditions {
    combat: bool,
    range: f64,
    check_line_of_sight: bool,
    test_invisible: bool,
}

impl TargetingConditions {
    pub const fn for_combat() -> Self {
        Self {
            combat: true,
            range: -1.0,
            check_line_of_sight: true,
            test_invisible: true,
        }
    }

    pub const fn for_non_combat() -> Self {
        Self {
            combat: false,
            ..Self::for_combat()
        }
    }

    pub const fn range(self, range: f64) -> Self {
        Self { range, ..self }
    }

    pub const fn ignore_line_of_sight(self) -> Self {
        Self {
            check_line_of_sight: false,
            ..self
        }
    }

    pub const fn ignore_invisibility_testing(self) -> Self {
        Self {
            test_invisible: false,
            ..self
        }
    }

    pub fn test<W: AiWorld>(
        &self,
        context: &mut AiContext<'_, W>,
        target: &Living,
        selector: impl Fn(&Living) -> bool,
    ) -> bool {
        let mob = context.mob;
        if target.id == mob.id || !target.can_be_seen_by_anyone() || !selector(target) {
            return false;
        }
        if self.combat && !mob_can_attack(mob, context.world, target) {
            return false;
        }
        if self.range > 0.0 {
            let visibility = if self.test_invisible {
                target.visibility_percent()
            } else {
                1.0
            };
            let reach = (self.range * visibility).max(MIN_VISIBILITY_DISTANCE);
            if mob.distance_to_sqr(target.position) > reach * reach {
                return false;
            }
        }
        !self.check_line_of_sight
            || context
                .brain
                .sensing
                .has_line_of_sight(context.world, mob, target)
    }
}

pub fn can_attack(world: &impl AiWorld, target: &Living) -> bool {
    target.kind != EntityKind::Ghast
        && !(target.player && world.peaceful())
        && target.can_be_seen_as_enemy()
}

pub fn mob_can_attack(mob: &MobView, world: &impl AiWorld, target: &Living) -> bool {
    if mob.kind == EntityKind::Breeze {
        return match mob.species.breeze.attack_target {
            Some(current) => target.id == current,
            None => {
                (target.player || target.kind == EntityKind::IronGolem) && can_attack(world, target)
            }
        };
    }
    can_attack(world, target) && !refuses(mob, target)
}

fn refuses(mob: &MobView, target: &Living) -> bool {
    if mob.tamed && mob.owner == Some(target.id) {
        return true;
    }
    mob.kind == EntityKind::IronGolem
        && ((mob.player_created && target.player) || target.kind == EntityKind::Creeper)
}

pub fn valid_player_target(world: &impl AiWorld, target: &Living) -> bool {
    target.player && !target.creative && !target.spectator && !world.peaceful()
}

pub fn is_angry_at(mob: &MobView, world: &impl AiWorld, target: &Living) -> bool {
    if !mob_can_attack(mob, world, target) {
        return false;
    }
    if valid_player_target(world, target)
        && world.universal_anger()
        && mob.angry
        && mob.anger_target.is_none()
    {
        return true;
    }
    mob.anger_target == Some(target.id)
}

pub fn goal_can_attack<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    target: &Living,
    conditions: TargetingConditions,
) -> bool {
    conditions.test(context, target, |_| true)
        && context.mob.is_within_home(target.position.block())
}

pub fn valid_target(world: &impl AiWorld, target: Living) -> Option<Living> {
    if target.player && (target.creative || target.spectator) {
        return None;
    }
    can_attack(world, &target).then_some(target)
}

pub fn valid_mob_target(mob: &MobView, world: &impl AiWorld, target: Living) -> Option<Living> {
    if target.player && (target.creative || target.spectator) {
        return None;
    }
    mob_can_attack(mob, world, &target).then_some(target)
}

pub fn nearest<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    candidates: impl IntoIterator<Item = Living>,
    conditions: TargetingConditions,
    selector: impl Fn(&Living) -> bool,
    origin: Vec3,
) -> Option<Living> {
    let mut best: Option<(f64, Living)> = None;
    for candidate in candidates {
        if !conditions.test(context, &candidate, &selector) {
            continue;
        }
        let distance = candidate.position.distance_squared(origin);
        if best.as_ref().is_none_or(|(current, _)| distance < *current) {
            best = Some((distance, candidate));
        }
    }
    best.map(|(_, living)| living)
}

pub fn nearest_player<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    conditions: TargetingConditions,
    selector: impl Fn(&Living) -> bool,
    origin: Vec3,
) -> Option<Living> {
    let world = context.world;
    nearest(
        context,
        world.players().iter().copied(),
        conditions,
        selector,
        origin,
    )
}
