use rspigot_domain::entity::EntityBase;
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;

use crate::action::{MobAction, TargetReason, TargetRequest};
use crate::blocks::BreakDoorGoal;
use crate::combat::{MeleeAttackGoal, RangedBowAttackGoal};
use crate::control::{BodyRotationControl, JumpControl, LookControl, MoveControl};
use crate::goal::{Flag, GoalSelector};
use crate::goals::MobGoal;
use crate::random::{MobRandom, RandomSource};
use crate::registry;
use crate::sensing::Sensing;
use crate::targeting;
use crate::world::{AiWorld, Living, MobBody, MobView};
use crate::{BlockView, MobProfile, MobState, MobTick, Navigation, PathType, combat_fall_distance};

pub const UPDATE_GOAL_SELECTOR_EVERY_N_TICKS: i32 = 2;
pub const CONTROL_FLAG_INTERVAL: i32 = 5;
const HARD_BOW_INTERVAL: i32 = 20;
const NORMAL_BOW_INTERVAL: i32 = 40;
const BOW_ATTACK_RADIUS: f32 = 15.0;
const SKELETON_MELEE_SPEED: f64 = 1.2;

#[derive(Debug, Clone, PartialEq)]
pub struct Brain {
    pub random: MobRandom,
    pub look: LookControl,
    pub movement: MoveControl,
    pub jump: JumpControl,
    pub body: BodyRotationControl,
    pub navigation: Navigation,
    pub sensing: Sensing,
    pub target: Option<EntityId>,
    pub profile: MobProfile,
    pub actions: Vec<MobAction>,
    pub defer_targets: bool,
    pub target_requests: Vec<TargetRequest>,
    pub tempt_verdict: Option<Option<EntityId>>,
    pub more_carrot_ticks: i32,
    follow_range: f64,
}

impl Brain {
    pub fn new(kind: EntityKind, seed: i64) -> Self {
        let dimensions = kind.dimensions();
        let follow_range = kind
            .default_attribute("minecraft:follow_range")
            .unwrap_or(0.0);
        let mut profile = MobProfile::new(dimensions.width, dimensions.height);
        if kind.extends("Animal") {
            profile.set_malus(PathType::FireInNeighbor, 16.0);
            profile.set_malus(PathType::Fire, -1.0);
        }
        if kind == EntityKind::Chicken {
            profile.set_malus(PathType::Water, 0.0);
        }
        Self {
            random: MobRandom::new(seed),
            look: LookControl::default(),
            movement: MoveControl::default(),
            jump: JumpControl::default(),
            body: BodyRotationControl::default(),
            navigation: Navigation::with_follow_range(follow_range),
            sensing: Sensing::default(),
            target: None,
            profile,
            actions: Vec::new(),
            defer_targets: false,
            target_requests: Vec::new(),
            tempt_verdict: None,
            more_carrot_ticks: 0,
            follow_range,
        }
    }
}

pub struct AiContext<'a, W> {
    pub mob: &'a MobView,
    pub world: &'a W,
    pub brain: &'a mut Brain,
    pub body: &'a mut MobBody,
}

pub fn mob_tick(mob: &MobView, game_time: i64) -> MobTick {
    MobTick {
        state: MobState {
            position: mob.position,
            on_ground: mob.on_ground,
            in_floatable_fluid: mob.fluid.in_water,
        },
        in_liquid: mob.in_liquid(),
        passenger: false,
        speed: mob.speed,
        game_time,
    }
}

impl<W: AiWorld> AiContext<'_, W> {
    pub fn tick_state(&self) -> MobTick {
        mob_tick(self.mob, self.world.game_time())
    }

    pub fn target(&self) -> Option<Living> {
        let id = self.brain.target?;
        let living = self.world.living(id)?;
        targeting::valid_target(self.world, living)
    }

    pub fn set_target(&mut self, target: Option<Living>, reason: TargetReason) {
        let to = target
            .and_then(|living| targeting::valid_target(self.world, living))
            .map(|living| living.id);
        let from = self.brain.target;
        if to == from {
            return;
        }
        self.brain.target_requests.push(TargetRequest {
            from,
            to,
            reason,
            tempt: false,
        });
        if !self.brain.defer_targets {
            self.brain.target = to;
        }
    }

    fn path_profile(&mut self) -> MobProfile {
        let mob = self.mob;
        let mut profile = self.brain.profile.clone();
        profile.step_height = mob.step_height;
        profile.follow_range = mob.follow_range as f32;
        profile.max_fall_distance = if self.target().is_none() {
            MobProfile::DEFAULT_MAX_FALL_DISTANCE
        } else if mob.kind == EntityKind::Creeper {
            (mob.health - 1.0 + 3.0).floor() as i32
        } else {
            combat_fall_distance(mob.health, mob.max_health, self.world.difficulty())
        };
        profile
    }

    pub fn move_to_pos(&mut self, position: Vec3, speed_modifier: f64) -> bool {
        let path = self.create_path_to(position.block(), 1);
        self.move_along(path, speed_modifier)
    }

    pub fn act(&mut self, action: MobAction) {
        self.brain.actions.push(action);
    }

    pub fn create_path_to(&mut self, pos: BlockPos, reach: i32) -> Option<crate::Path> {
        let profile = self.path_profile();
        let tick = self.tick_state();
        let follow = self.mob.follow_range;
        self.brain
            .navigation
            .create_path(self.world, &profile, &tick, pos, reach, follow)
    }

    pub fn move_along(&mut self, path: Option<crate::Path>, speed_modifier: f64) -> bool {
        let profile = self.path_profile();
        let tick = self.tick_state();
        self.brain
            .navigation
            .move_to(path, speed_modifier, self.world, &profile, &tick)
    }

    pub fn move_to_entity_within(
        &mut self,
        target: &Living,
        reach: i32,
        speed_modifier: f64,
    ) -> bool {
        let path = self.create_path_to(target.position.block(), reach);
        path.is_some() && self.move_along(path, speed_modifier)
    }

    pub fn move_to_entity(&mut self, target: &Living, speed_modifier: f64) -> bool {
        let profile = self.path_profile();
        let tick = self.tick_state();
        let follow = self.mob.follow_range;
        self.brain.navigation.move_to_entity(
            self.world,
            &profile,
            &tick,
            target.position,
            speed_modifier,
            follow,
        )
    }

    pub fn stop_navigation(&mut self) {
        self.brain.navigation.stop();
    }

    fn recompute_path(&mut self) {
        let profile = self.path_profile();
        let tick = self.tick_state();
        let follow = self.mob.follow_range;
        self.brain
            .navigation
            .recompute_path(self.world, &profile, &tick, follow);
    }

    fn tick_navigation(&mut self) {
        let profile = self.path_profile();
        let tick = self.tick_state();
        let follow = self.mob.follow_range;
        let navigation = &self.brain.navigation;
        let climbing = navigation.is_climber() && navigation.is_done();
        if navigation.has_delayed_recomputation() && !climbing {
            self.brain
                .navigation
                .recompute_path(self.world, &profile, &tick, follow);
        }
        if let Some(wanted) = self.brain.navigation.tick(self.world, &profile, &tick) {
            self.brain
                .movement
                .set_wanted_position(wanted.position, wanted.speed_modifier);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GoalChange {
    Weapon { bow: bool, interval: i32 },
    BreakDoors(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MobAi {
    goals: GoalSelector<MobGoal>,
    targets: GoalSelector<MobGoal>,
    brain: Brain,
    pending: Vec<GoalChange>,
    can_break_doors: bool,
}

impl MobAi {
    pub fn new(kind: EntityKind, seed: i64, settings: registry::GoalSettings) -> Self {
        let mut ai = Self {
            goals: GoalSelector::new(),
            targets: GoalSelector::new(),
            brain: Brain::new(kind, seed),
            pending: Vec::new(),
            can_break_doors: false,
        };
        registry::register_goals(
            kind,
            &mut ai.goals,
            &mut ai.targets,
            &mut ai.brain,
            settings,
        );
        ai
    }

    pub fn reassess_weapon(&mut self, bow: bool, hard: bool) {
        let interval = if hard {
            HARD_BOW_INTERVAL
        } else {
            NORMAL_BOW_INTERVAL
        };
        self.pending.push(GoalChange::Weapon { bow, interval });
    }

    pub fn set_can_break_doors(&mut self, can_break_doors: bool) {
        if self.brain.navigation.is_climber() || self.can_break_doors == can_break_doors {
            return;
        }
        self.can_break_doors = can_break_doors;
        self.brain.profile.can_open_doors = can_break_doors;
        self.pending.push(GoalChange::BreakDoors(can_break_doors));
    }

    pub fn can_break_doors(&self) -> bool {
        self.can_break_doors
    }

    pub fn is_panicking(&self) -> bool {
        self.goals
            .goals()
            .iter()
            .any(|goal| goal.is_running() && matches!(goal.goal(), MobGoal::Panic(_)))
    }

    pub fn eat_animation_tick(&self) -> i32 {
        self.goals
            .goals()
            .iter()
            .find_map(|goal| match goal.goal() {
                MobGoal::EatBlock(eat) => Some(eat.eat_animation_tick()),
                _ => None,
            })
            .unwrap_or(0)
    }

    pub fn set_target(&mut self, target: Option<EntityId>) {
        self.brain.target = target;
    }

    pub fn goals(&self) -> &GoalSelector<MobGoal> {
        &self.goals
    }

    pub fn targets(&self) -> &GoalSelector<MobGoal> {
        &self.targets
    }

    pub fn brain(&self) -> &Brain {
        &self.brain
    }

    pub fn brain_mut(&mut self) -> &mut Brain {
        &mut self.brain
    }

    pub fn target(&self) -> Option<EntityId> {
        self.brain.target
    }

    pub fn can_float(&self) -> bool {
        self.brain.profile.can_float
    }

    pub fn take_actions(&mut self) -> Vec<MobAction> {
        std::mem::take(&mut self.brain.actions)
    }

    pub fn take_target_requests(&mut self) -> Vec<TargetRequest> {
        std::mem::take(&mut self.brain.target_requests)
    }

    pub fn defer_targets(&mut self, defer: bool) {
        self.brain.defer_targets = defer;
    }

    pub fn settle_tempt(&mut self, verdict: Option<EntityId>) {
        self.brain.tempt_verdict = Some(verdict);
    }

    pub fn goals_mut(&mut self) -> &mut GoalSelector<MobGoal> {
        &mut self.goals
    }

    pub fn think<W: AiWorld>(&mut self, mob: &MobView, world: &W, body: &mut MobBody) {
        self.brain.sensing.tick();
        if self.brain.follow_range != mob.follow_range {
            self.brain.follow_range = mob.follow_range;
            self.brain
                .navigation
                .update_max_visited_nodes(mob.follow_range);
        }
        if self.brain.profile.width != mob.width || self.brain.profile.height != mob.height {
            self.brain.profile.width = mob.width;
            self.brain.profile.height = mob.height;
        }
        let mut context = AiContext {
            mob,
            world,
            brain: &mut self.brain,
            body,
        };
        apply_pending(
            &mut self.goals,
            std::mem::take(&mut self.pending),
            &mut context,
        );
        let phase = mob.tick_count.wrapping_add(mob.id.0);
        if phase % UPDATE_GOAL_SELECTOR_EVERY_N_TICKS != 0 && mob.tick_count > 1 {
            self.targets.tick_running_goals(&mut context, false);
            self.goals.tick_running_goals(&mut context, false);
        } else {
            self.targets.tick(&mut context);
            self.goals.tick(&mut context);
        }
        context.tick_navigation();
        if self.brain.more_carrot_ticks > 0 {
            let decay = self.brain.random.next_int(3);
            self.brain.more_carrot_ticks = (self.brain.more_carrot_ticks - decay).max(0);
        }
    }

    pub fn act(&mut self, base: &mut EntityBase, living: &mut LivingState, view: &impl BlockView) {
        let brain = &mut self.brain;
        let profile = &brain.profile;
        let state = MobState {
            position: base.position,
            on_ground: base.on_ground,
            in_floatable_fluid: base.fluid.in_water,
        };
        brain
            .movement
            .tick(base, living, view, &mut brain.jump, |pos| {
                crate::walk::mob_path_type(view, profile, state, pos) == PathType::Walkable
            });
        let navigating = brain.navigation.is_in_progress();
        brain.look.tick(base, living, navigating);
        brain.jump.tick(living);
    }

    pub fn block_changed<W: AiWorld>(&mut self, pos: BlockPos, mob: &MobView, world: &W) {
        if !self
            .brain
            .navigation
            .should_recompute_path(pos, mob.position)
        {
            return;
        }
        let mut body = MobBody::of(mob);
        let mut context = AiContext {
            mob,
            world,
            brain: &mut self.brain,
            body: &mut body,
        };
        context.recompute_path();
    }

    pub fn turn_body(&mut self, base: &mut EntityBase, living: &mut LivingState) {
        self.brain.body.client_tick(base, living);
    }

    pub fn update_control_flags(&mut self) {
        self.goals.set_control_flag(Flag::Move, true);
        self.goals.set_control_flag(Flag::Jump, true);
        self.goals.set_control_flag(Flag::Look, true);
    }
}

fn apply_pending<W: AiWorld>(
    goals: &mut GoalSelector<MobGoal>,
    pending: Vec<GoalChange>,
    context: &mut AiContext<'_, W>,
) {
    for change in pending {
        match change {
            GoalChange::Weapon { bow, interval } => {
                goals.remove_goals(context, |goal| {
                    matches!(goal, MobGoal::MeleeAttack(_) | MobGoal::RangedBowAttack(_))
                });
                let goal = if bow {
                    MobGoal::RangedBowAttack(RangedBowAttackGoal::new(
                        1.0,
                        interval,
                        BOW_ATTACK_RADIUS,
                    ))
                } else {
                    MobGoal::MeleeAttack(MeleeAttackGoal::skeleton(SKELETON_MELEE_SPEED))
                };
                goals.add_goal(registry::WEAPON_PRIORITY, goal);
            }
            GoalChange::BreakDoors(true) => {
                goals.add_goal(
                    registry::BREAK_DOOR_PRIORITY,
                    MobGoal::BreakDoor(BreakDoorGoal::default()),
                );
            }
            GoalChange::BreakDoors(false) => {
                goals.remove_goals(context, |goal| matches!(goal, MobGoal::BreakDoor(_)));
            }
        }
    }
}
