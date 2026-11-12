use rspigot_domain::{EntityId, Vec3};

use crate::Path;
use crate::brain::AiContext;
use crate::random::RandomSource;
use crate::random_pos::{self, walk_target_value};
use crate::targeting::TargetingConditions;
use crate::world::{AiWorld, Living, TargetKind, inflate};

const AVOID_HEIGHT: f64 = 3.0;
const AWAY_HORIZONTAL: i32 = 16;
const AWAY_VERTICAL: i32 = 7;
const SPRINT_DISTANCE_SQR: f64 = 49.0;
const HIDE_ATTEMPTS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvoidFilter {
    Any,
    NotScared,
    Fox(FoxAvoid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoxAvoid {
    Player,
    Wolf,
    PolarBear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AvoidGate {
    Open,
    NotEvil,
    NotTrusting,
    NotTame,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AvoidEntityGoal {
    kind: TargetKind,
    filter: AvoidFilter,
    gate: AvoidGate,
    max_distance: f32,
    walk_speed_modifier: f64,
    sprint_speed_modifier: f64,
    to_avoid: Option<EntityId>,
    path: Option<Path>,
}

impl AvoidEntityGoal {
    pub fn new(kind: TargetKind, max_distance: f32, walk: f64, sprint: f64) -> Self {
        Self {
            kind,
            filter: AvoidFilter::Any,
            gate: AvoidGate::Open,
            max_distance,
            walk_speed_modifier: walk,
            sprint_speed_modifier: sprint,
            to_avoid: None,
            path: None,
        }
    }

    pub fn unless_scared(kind: TargetKind, max_distance: f32, walk: f64, sprint: f64) -> Self {
        Self {
            filter: AvoidFilter::NotScared,
            ..Self::new(kind, max_distance, walk, sprint)
        }
    }

    pub fn unless_evil(kind: TargetKind, max_distance: f32, walk: f64, sprint: f64) -> Self {
        Self {
            gate: AvoidGate::NotEvil,
            ..Self::new(kind, max_distance, walk, sprint)
        }
    }

    pub fn unless_trusting(kind: TargetKind, max_distance: f32, walk: f64, sprint: f64) -> Self {
        Self {
            gate: AvoidGate::NotTrusting,
            ..Self::new(kind, max_distance, walk, sprint)
        }
    }

    pub fn unless_tame(kind: TargetKind, max_distance: f32, walk: f64, sprint: f64) -> Self {
        Self {
            gate: AvoidGate::NotTame,
            ..Self::new(kind, max_distance, walk, sprint)
        }
    }

    pub fn is_tame_gated(&self) -> bool {
        self.gate == AvoidGate::NotTame
    }

    fn open<W: AiWorld>(&self, context: &AiContext<'_, W>) -> bool {
        match self.gate {
            AvoidGate::Open => true,
            AvoidGate::NotEvil => !context.mob.evil,
            AvoidGate::NotTrusting => !context.mob.trusting,
            AvoidGate::NotTame => !context.mob.tamed,
        }
    }

    pub fn fox(
        kind: TargetKind,
        which: FoxAvoid,
        max_distance: f32,
        walk: f64,
        sprint: f64,
    ) -> Self {
        Self {
            filter: AvoidFilter::Fox(which),
            ..Self::new(kind, max_distance, walk, sprint)
        }
    }

    fn accepts<W: AiWorld>(&self, context: &AiContext<'_, W>, target: &Living) -> bool {
        match self.filter {
            AvoidFilter::Fox(which) => {
                let fox = &context.brain.fox;
                !fox.is_defending()
                    && match which {
                        FoxAvoid::Player => {
                            !target.discrete
                                && !(target.creative || target.spectator)
                                && !fox.trusts(target.facts.uuid)
                        }
                        FoxAvoid::Wolf => !target.facts.tamed,
                        FoxAvoid::PolarBear => true,
                    }
            }
            filter => {
                let creative_or_spectator = target.player && (target.creative || target.spectator);
                !creative_or_spectator
                    && match filter {
                        AvoidFilter::NotScared => !target.scared,
                        _ => true,
                    }
            }
        }
    }

    pub(crate) fn to_avoid(&self) -> Option<EntityId> {
        self.to_avoid
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        (self.gate != AvoidGate::NotTrusting || !context.mob.trusting)
            && (self.gate != AvoidGate::NotTame || !context.mob.tamed)
            && !context.brain.navigation.is_done()
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if !self.open(context) {
            return false;
        }
        let reach = f64::from(self.max_distance);
        let area = inflate(&context.mob.bounds(), reach, AVOID_HEIGHT, reach);
        let candidates = context.world.living_of(self.kind, &area);
        let conditions = TargetingConditions::for_combat().range(reach);
        let origin = context.mob.position;
        let mut best: Option<(f64, Living)> = None;
        for candidate in candidates {
            if !self.accepts(context, &candidate) || !conditions.test(context, &candidate, |_| true)
            {
                continue;
            }
            let distance = candidate.position.distance_squared(origin);
            if best.as_ref().is_none_or(|(current, _)| distance < *current) {
                best = Some((distance, candidate));
            }
        }
        let Some((_, avoid)) = best else {
            self.to_avoid = None;
            return false;
        };
        self.to_avoid = Some(avoid.id);
        let Some(away) =
            random_pos::pos_away(context, AWAY_HORIZONTAL, AWAY_VERTICAL, avoid.position)
        else {
            return false;
        };
        if avoid.position.distance_squared(away)
            < avoid.position.distance_squared(context.mob.position)
        {
            return false;
        }
        self.path = context.create_path_to(away.block(), 0);
        self.path.is_some()
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let path = self.path.take();
        context.move_along(path, self.walk_speed_modifier);
    }

    pub(crate) fn stop(&mut self) {
        self.to_avoid = None;
    }

    pub(crate) fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        let Some(avoid) = self.to_avoid.and_then(|id| context.world.living(id)) else {
            return;
        };
        let speed = if context.mob.distance_to_sqr(avoid.position) < SPRINT_DISTANCE_SQR {
            self.sprint_speed_modifier
        } else {
            self.walk_speed_modifier
        };
        context.brain.navigation.set_speed_modifier(speed);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FleeSunGoal {
    speed_modifier: f64,
    wanted: Vec3,
}

impl FleeSunGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            wanted: Vec3::ZERO,
        }
    }

    pub(crate) fn wanted(&self) -> Vec3 {
        self.wanted
    }

    pub(crate) fn set_wanted_pos<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(wanted) = Self::hide_pos(context) else {
            return false;
        };
        self.wanted = wanted;
        true
    }

    fn hide_pos<W: AiWorld>(context: &mut AiContext<'_, W>) -> Option<Vec3> {
        let origin = context.mob.block_pos();
        for _ in 0..HIDE_ATTEMPTS {
            let x = context.brain.random.next_int(20) - 10;
            let y = context.brain.random.next_int(6) - 3;
            let z = context.brain.random.next_int(20) - 10;
            let candidate = origin.offset(x, y, z);
            if !context.world.can_see_sky(candidate)
                && walk_target_value(context.mob.kind, context.world, candidate) < 0.0
            {
                return Some(Vec3::new(
                    f64::from(candidate.x) + 0.5,
                    f64::from(candidate.y),
                    f64::from(candidate.z) + 0.5,
                ));
            }
        }
        None
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if context.target().is_some()
            || !context.world.bright_outside()
            || !context.mob.on_fire
            || !context.world.can_see_sky(context.mob.block_pos())
            || !context.mob.head_empty
        {
            return false;
        }
        let Some(wanted) = Self::hide_pos(context) else {
            return false;
        };
        self.wanted = wanted;
        true
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.move_to_pos(self.wanted, self.speed_modifier);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RestrictSunGoal;

impl RestrictSunGoal {
    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.world.bright_outside() && context.mob.head_empty
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.brain.navigation.set_avoid_sun(true);
    }

    pub(crate) fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.brain.navigation.set_avoid_sun(false);
    }
}
