use rspigot_domain::entity::EntityBase;
use rspigot_domain::entity::living::LivingState;
use rspigot_domain::entity::species::fox;
use rspigot_domain::{BlockPos, EntityId, Vec3};
use rspigot_entities::EntityKind;

use crate::action::BrainMessage;
use crate::action::{MobAction, TargetReason, TargetRequest};
use crate::behavior::{BehaviorBrain, BehaviorCx, MemoryModuleType, MemoryValue, MobControl};
use crate::blocks::BreakDoorGoal;
use crate::combat::{MeleeAttackGoal, RangedBowAttackGoal};
use crate::control::{
    BodyRotationControl, JumpControl, LookControl, MoveControl, MoveStyle, SmoothSwimming,
};
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
const WARDEN_CONVERSION_DELAYS: i32 = 3;
const DOLPHIN_MAX_TURN_X: i32 = 85;
const DOLPHIN_MAX_TURN_Y: i32 = 10;
const DOLPHIN_IN_WATER_SPEED: f32 = 0.02;
const DOLPHIN_OUTSIDE_WATER_SPEED: f32 = 0.1;
const DOLPHIN_LOOK_FROM_CENTER: i32 = 10;
const DOLPHIN_MAX_HEAD_Y_ROT: f32 = 1.0;

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
    pub fox: crate::FoxFacts,
    pub can_break_doors: bool,
    pub hop: Option<crate::rabbit::RabbitHop>,
    pub panda: crate::PandaBrain,
    pub repellent: Option<BlockPos>,
    pub creaking: crate::CreakingState,
    pub warden: crate::WardenState,
    pub target_position: Option<Vec3>,
    pub phantom: crate::PhantomBrain,
    pub cube_sound: Option<crate::cube::CubeSound>,
    pub bee: crate::BeeBrain,
    pub wither: crate::WitherBrain,
    pub wither_targets: [i32; 3],
    pub guardian: crate::GuardianBrain,
    pub drowned_target: Option<(f64, bool)>,
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
        if matches!(kind, EntityKind::Piglin | EntityKind::PiglinBrute) {
            profile.set_malus(PathType::FireInNeighbor, 16.0);
            profile.set_malus(PathType::Fire, -1.0);
            profile.can_open_doors = true;
        }
        if kind.extends("AbstractVillager") {
            profile.set_malus(PathType::FireInNeighbor, 16.0);
            profile.set_malus(PathType::Fire, -1.0);
        }
        if kind == EntityKind::Villager {
            profile.can_open_doors = true;
            profile.can_float = true;
        }
        if kind == EntityKind::Chicken {
            profile.set_malus(PathType::Water, 0.0);
        }
        if kind == EntityKind::Axolotl {
            profile = profile.amphibious(false);
            profile.set_malus(PathType::Water, 0.0);
        }
        if matches!(kind, EntityKind::Nautilus | EntityKind::ZombieNautilus) {
            profile = profile.water_bound(false);
            profile.set_malus(PathType::Water, 0.0);
        }
        if kind == EntityKind::Frog {
            profile = profile.amphibious(true).frog_nodes();
            profile.set_malus(PathType::Water, 4.0);
            profile.set_malus(PathType::Trapdoor, -1.0);
        }
        if kind == EntityKind::Sniffer {
            profile.can_float = true;
            profile.set_malus(PathType::Water, -1.0);
            profile.set_malus(PathType::OnTopOfPowderSnow, -1.0);
            profile.set_malus(PathType::DamageCautious, -1.0);
        }
        if kind == EntityKind::Creaking {
            profile.can_float = true;
        }
        if kind == EntityKind::Warden {
            profile.can_float = true;
            profile.horizontal_step_cost = true;
            profile.set_malus(PathType::UnpassableRail, 0.0);
            profile.set_malus(PathType::Damaging, 8.0);
            profile.set_malus(PathType::PowderSnow, 8.0);
            profile.set_malus(PathType::Lava, 8.0);
            profile.set_malus(PathType::Fire, 0.0);
            profile.set_malus(PathType::FireInNeighbor, 0.0);
        }
        if kind == EntityKind::CopperGolem {
            profile.can_open_doors = true;
            profile.set_malus(PathType::FireInNeighbor, 16.0);
            profile.set_malus(PathType::DamagingInNeighbor, 16.0);
            profile.set_malus(PathType::Fire, -1.0);
        }
        if kind == EntityKind::Parrot {
            profile = profile.flying(true);
            profile.set_malus(PathType::FireInNeighbor, -1.0);
            profile.set_malus(PathType::Fire, -1.0);
            profile.set_malus(PathType::Cocoa, -1.0);
        }
        if kind == EntityKind::Bee {
            profile = profile.flying(false);
            profile.set_malus(PathType::Fire, -1.0);
            profile.set_malus(PathType::Water, -1.0);
            profile.set_malus(PathType::WaterBorder, 16.0);
            profile.set_malus(PathType::Cocoa, -1.0);
            profile.set_malus(PathType::Fence, -1.0);
        }
        if kind == EntityKind::Wither {
            profile = profile.flying(true);
        }
        if kind == EntityKind::Allay {
            profile = profile.flying(true);
        }
        if kind.extends("AbstractFish") {
            profile = profile.water_bound(false);
            profile.set_malus(PathType::Water, 0.0);
        }
        if kind == EntityKind::Dolphin {
            profile = profile.water_bound(true);
            profile.set_malus(PathType::Water, 0.0);
        }
        if kind == EntityKind::Drowned {
            profile = profile.amphibious(false);
            profile.set_malus(PathType::Water, 0.0);
        }
        if kind.extends("Guardian") {
            profile = profile.water_bound(false);
            profile.set_malus(PathType::Water, 0.0);
        }
        if kind == EntityKind::Turtle {
            profile = profile.amphibious(false);
            profile.set_malus(PathType::Water, 0.0);
            profile.set_malus(PathType::DoorIronClosed, -1.0);
            profile.set_malus(PathType::DoorWoodClosed, -1.0);
            profile.set_malus(PathType::DoorOpen, -1.0);
        }
        if kind == EntityKind::Blaze {
            profile.set_malus(PathType::Water, -1.0);
            profile.set_malus(PathType::Lava, 8.0);
            profile.set_malus(PathType::FireInNeighbor, 0.0);
            profile.set_malus(PathType::Fire, 0.0);
        }
        let mut brain = Self {
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
            fox: crate::FoxFacts::default(),
            can_break_doors: false,
            hop: None,
            panda: crate::PandaBrain::default(),
            repellent: None,
            creaking: crate::CreakingState::default(),
            warden: crate::WardenState::default(),
            target_position: None,
            phantom: crate::PhantomBrain::default(),
            cube_sound: None,
            bee: crate::BeeBrain::default(),
            wither: crate::WitherBrain::default(),
            wither_targets: [0; 3],
            guardian: crate::GuardianBrain::default(),
            drowned_target: None,
            follow_range,
        };
        if kind == EntityKind::Warden {
            brain.warden = crate::WardenState::new(brain.random.next_int(WARDEN_CONVERSION_DELAYS));
        }
        if kind == EntityKind::Rabbit {
            brain.enable_hop();
        }
        if kind.extends("AbstractFish") && kind != EntityKind::Tadpole {
            brain.movement.set_fish();
        }
        if let Some((movement, look_from_center)) = crate::amphibians::smooth_swimming_of(kind) {
            brain.movement.set_smooth_swimming(movement);
            if let Some(max_from_center) = look_from_center {
                brain.look.set_smooth_swimming(max_from_center);
            }
        }
        if kind == EntityKind::Turtle {
            brain.movement.set_turtle();
        }
        if kind.extends("Guardian") {
            brain.movement.set_guardian();
        }
        if kind == EntityKind::Drowned {
            brain.movement.set_drowned();
        }
        if kind == EntityKind::Dolphin {
            brain.movement.set_smooth_swimming(SmoothSwimming {
                max_turn_x: DOLPHIN_MAX_TURN_X,
                max_turn_y: DOLPHIN_MAX_TURN_Y,
                in_water_speed_modifier: DOLPHIN_IN_WATER_SPEED,
                outside_water_speed_modifier: DOLPHIN_OUTSIDE_WATER_SPEED,
                apply_gravity: true,
            });
            brain.look.set_smooth_swimming(DOLPHIN_LOOK_FROM_CENTER);
        }
        if matches!(kind, EntityKind::Parrot | EntityKind::Wither) {
            brain.movement.set_flying(10, false);
        }
        if kind == EntityKind::Bee {
            brain.movement.set_flying(20, true);
            brain.navigation.set_required_path_length(
                rspigot_domain::entity::species::bee::PATH_LENGTH,
                follow_range,
            );
        }
        if kind == EntityKind::Allay {
            brain.movement.set_flying(20, true);
            brain
                .navigation
                .set_required_path_length(48.0, follow_range);
        }
        if kind == EntityKind::Villager {
            brain
                .navigation
                .set_required_path_length(48.0, follow_range);
        }
        if kind == EntityKind::CopperGolem {
            brain.navigation.set_required_path_length(
                rspigot_domain::entity::species::copper_golem::PATH_LENGTH,
                follow_range,
            );
        }
        brain
    }
}

pub struct AiContext<'a, W> {
    pub mob: &'a MobView,
    pub world: &'a W,
    pub brain: &'a mut Brain,
    pub body: &'a mut MobBody,
}

fn path_profile_of<W: AiWorld>(
    base: &MobProfile,
    has_target: bool,
    mob: &MobView,
    world: &W,
) -> MobProfile {
    let mut profile = base.clone();
    if mob.kind == EntityKind::Sniffer && (mob.on_fire || mob.fluid.in_water) {
        profile.set_malus(PathType::Water, 0.0);
    }
    profile.step_height = mob.step_height;
    profile.sea_level = world.sea_level();
    profile.follow_range = mob.follow_range as f32;
    profile.max_fall_distance = if !has_target {
        MobProfile::DEFAULT_MAX_FALL_DISTANCE
    } else if mob.kind == EntityKind::Creeper {
        (mob.health - 1.0 + 3.0).floor() as i32
    } else {
        combat_fall_distance(mob.health, mob.max_health, world.difficulty())
    };
    profile
}

pub fn mob_tick(mob: &MobView, game_time: i64) -> MobTick {
    MobTick {
        state: MobState {
            position: mob.position,
            on_ground: mob.on_ground,
            in_floatable_fluid: mob.fluid.in_water,
        },
        in_liquid: mob.in_liquid(),
        passenger: mob.passenger,
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
        targeting::valid_mob_target(self.mob, self.world, living)
    }

    pub fn set_target(&mut self, target: Option<Living>, reason: TargetReason) {
        let to = target
            .and_then(|living| targeting::valid_mob_target(self.mob, self.world, living))
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

    pub fn request_brain_target(
        &mut self,
        from: Option<EntityId>,
        to: Option<EntityId>,
        reason: TargetReason,
    ) -> bool {
        self.brain.target_requests.push(TargetRequest {
            from,
            to,
            reason,
            tempt: false,
        });
        !self.brain.defer_targets
    }

    fn path_profile(&mut self) -> MobProfile {
        path_profile_of(
            &self.brain.profile,
            self.target().is_some(),
            self.mob,
            self.world,
        )
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

    pub fn create_path_to_any(&mut self, targets: &[BlockPos], reach: i32) -> Option<crate::Path> {
        let profile = self.path_profile();
        let tick = self.tick_state();
        let follow = self.mob.follow_range;
        self.brain
            .navigation
            .create_path_to_any(self.world, &profile, &tick, targets, reach, follow)
    }

    pub fn create_path_to_limited(
        &mut self,
        pos: BlockPos,
        reach: i32,
        max_path_length: f32,
    ) -> Option<crate::Path> {
        let profile = self.path_profile();
        let tick = self.tick_state();
        self.brain.navigation.create_path_limited(
            self.world,
            &profile,
            &tick,
            pos,
            reach,
            max_path_length,
        )
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
        self.move_to_entity_at(target.position, speed_modifier)
    }

    pub fn move_to_entity_at(&mut self, position: Vec3, speed_modifier: f64) -> bool {
        let profile = self.path_profile();
        let tick = self.tick_state();
        let follow = self.mob.follow_range;
        self.brain.navigation.move_to_entity(
            self.world,
            &profile,
            &tick,
            position,
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
    CatTame(bool),
    Trusting(bool),
    KillerBunny,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MobAi {
    goals: GoalSelector<MobGoal>,
    targets: GoalSelector<MobGoal>,
    brain: Brain,
    behavior: Option<BehaviorBrain<MobControl>>,
    pending: Vec<GoalChange>,
    can_break_doors: bool,
}

impl MobAi {
    pub fn new(kind: EntityKind, seed: i64, settings: registry::GoalSettings) -> Self {
        let mut ai = Self {
            goals: GoalSelector::new(),
            targets: GoalSelector::new(),
            brain: Brain::new(kind, seed),
            behavior: None,
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
        ai.behavior = registry::register_brain(kind, &mut ai.brain.random);
        if kind == EntityKind::CopperGolem
            && let Some(behavior) = ai.behavior.as_mut()
        {
            let cooldown = crate::copper_golem::initial_cooldown(&mut ai.brain.random);
            behavior.memories_mut().set(
                MemoryModuleType::TransportItemsCooldownTicks,
                MemoryValue::Int(cooldown),
            );
        }
        ai
    }

    pub fn has_ai(&self) -> bool {
        self.behavior.is_some()
            || !self.goals.goals().is_empty()
            || !self.targets.goals().is_empty()
    }

    pub fn init_frog_memories(&mut self, long_jump: i32) {
        if let Some(behavior) = self.behavior.as_mut() {
            crate::frog::init_memories(behavior.memories_mut(), long_jump);
        }
    }

    pub fn init_nautilus_memories(&mut self, attack_cooldown: i32) {
        if let Some(behavior) = self.behavior.as_mut() {
            crate::nautilus::init_memories(behavior.memories_mut(), attack_cooldown);
        }
    }

    pub fn init_goat_memories(&mut self, long_jump: i32, ram: i32) {
        if let Some(behavior) = self.behavior.as_mut() {
            crate::goat::init_memories(behavior.memories_mut(), long_jump, ram);
        }
    }

    pub fn rebuild_villager_brain(
        &mut self,
        profession: rspigot_domain::entity::species::villager::Profession,
        baby: bool,
    ) {
        let packed = self
            .behavior
            .as_ref()
            .map(BehaviorBrain::pack)
            .unwrap_or_default();
        self.behavior = Some(crate::villager::villager_brain(
            profession,
            baby,
            &packed,
            &mut self.brain.random,
        ));
    }

    pub fn update_villager_schedule(&mut self, clock_ticks: i64, game_time: i64) {
        if let Some(behavior) = self.behavior.as_mut() {
            behavior
                .state_mut()
                .update_activity_from_schedule(clock_ticks, game_time);
        }
    }

    pub fn can_reach_pos<W: AiWorld>(
        &self,
        mob: &MobView,
        world: &W,
        pos: BlockPos,
        reach: i32,
    ) -> bool {
        let has_target = self.brain.target.is_some_and(|id| {
            world
                .living(id)
                .and_then(|living| targeting::valid_mob_target(mob, world, living))
                .is_some()
        });
        let profile = path_profile_of(&self.brain.profile, has_target, mob, world);
        let tick = mob_tick(mob, world.game_time());
        let mut navigation = self.brain.navigation.clone();
        navigation
            .create_path(world, &profile, &tick, pos, reach, mob.follow_range)
            .is_some_and(|path| path.can_reach())
    }

    pub fn set_transport_verdict(&mut self, verdict: crate::copper_golem::TargetVerdict) {
        if let Some(behavior) = self.behavior.as_mut() {
            behavior.for_each_behavior_mut(|control| control.set_transport_verdict(verdict));
        }
    }

    pub fn behavior_brain(&self) -> Option<&BehaviorBrain<MobControl>> {
        self.behavior.as_ref()
    }

    pub fn behavior_brain_mut(&mut self) -> Option<&mut BehaviorBrain<MobControl>> {
        self.behavior.as_mut()
    }

    pub fn set_behavior_brain(&mut self, behavior: Option<BehaviorBrain<MobControl>>) {
        self.behavior = behavior;
    }

    pub fn save_behavior_brain(&self) -> Option<rspigot_nbt::Compound> {
        self.behavior.as_ref().map(BehaviorBrain::save)
    }

    pub fn load_behavior_brain(&mut self, nbt: &rspigot_nbt::Compound) {
        if let Some(behavior) = self.behavior.as_mut() {
            behavior.load(nbt);
        }
    }

    pub fn stop_behavior_brain<W: AiWorld>(
        &mut self,
        mob: &MobView,
        world: &W,
        body: &mut MobBody,
    ) {
        if let Some(behavior) = self.behavior.as_mut() {
            let mut context = AiContext {
                mob,
                world,
                brain: &mut self.brain,
                body,
            };
            behavior.stop_all(&mut context);
        }
    }

    pub fn reassess_weapon(&mut self, bow: bool, hard: bool) {
        let interval = if hard {
            HARD_BOW_INTERVAL
        } else {
            NORMAL_BOW_INTERVAL
        };
        self.pending.push(GoalChange::Weapon { bow, interval });
    }

    pub fn reassess_trusting(&mut self, trusting: bool) {
        self.pending.push(GoalChange::Trusting(trusting));
    }

    pub fn become_killer_bunny(&mut self) {
        self.pending.push(GoalChange::KillerBunny);
    }

    pub fn set_can_break_doors(&mut self, can_break_doors: bool) {
        if self.brain.navigation.is_climber() || self.can_break_doors == can_break_doors {
            return;
        }
        self.can_break_doors = can_break_doors;
        self.brain.can_break_doors = can_break_doors;
        self.brain.profile.can_open_doors = can_break_doors;
        self.pending.push(GoalChange::BreakDoors(can_break_doors));
    }

    pub fn reassess_cat_tame_goals(&mut self, tame: bool) {
        self.pending.push(GoalChange::CatTame(tame));
    }

    pub fn raider_ai_step(&mut self) {
        let mut cooldown = None;
        for goal in self.targets.goals_mut() {
            if let MobGoal::Raider(crate::RaiderGoal::HealRaiders(heal)) = goal {
                heal.decrement_cooldown();
                cooldown = Some(heal.cooldown());
            }
        }
        let Some(cooldown) = cooldown else {
            return;
        };
        for goal in self.targets.goals_mut() {
            if let MobGoal::WitchTarget(attack) = goal {
                attack.set_can_attack(cooldown < 1);
            }
        }
    }

    pub fn is_tempting(&self) -> bool {
        self.goals.goals().iter().any(|goal| {
            goal.is_running() && matches!(goal.goal(), MobGoal::Tempt(tempt) if tempt.is_running())
        })
    }

    pub fn can_break_doors(&self) -> bool {
        self.can_break_doors
    }

    pub fn is_panicking(&self) -> bool {
        self.goals.goals().iter().any(|goal| {
            goal.is_running()
                && match goal.goal() {
                    MobGoal::Panic(_) => true,
                    MobGoal::Panda(panda) => panda.is_panic(),
                    _ => false,
                }
        })
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
        if let Some(behavior) = self.behavior.as_mut() {
            let memories = behavior.memories_mut();
            match target {
                Some(target) => {
                    memories.set(MemoryModuleType::AttackTarget, MemoryValue::Entity(target));
                    memories.erase(MemoryModuleType::CantReachWalkTargetSince);
                }
                None => memories.erase(MemoryModuleType::AttackTarget),
            }
            return;
        }
        self.brain.target = target;
    }

    pub fn goals(&self) -> &GoalSelector<MobGoal> {
        &self.goals
    }

    pub fn targets(&self) -> &GoalSelector<MobGoal> {
        &self.targets
    }

    pub fn targets_mut(&mut self) -> &mut GoalSelector<MobGoal> {
        &mut self.targets
    }

    pub fn brain(&self) -> &Brain {
        &self.brain
    }

    pub fn brain_mut(&mut self) -> &mut Brain {
        &mut self.brain
    }

    pub fn wither_hurt(&mut self) {
        for idle in &mut self.brain.wither.idle_head_updates {
            *idle += rspigot_domain::entity::species::wither::IDLE_HEAD_HURT_BONUS;
        }
    }

    pub fn target(&self) -> Option<EntityId> {
        match self.behavior.as_ref() {
            Some(behavior) => behavior
                .memories()
                .get(MemoryModuleType::AttackTarget)
                .and_then(MemoryValue::as_entity),
            None => self.brain.target,
        }
    }

    pub fn deliver<W: AiWorld>(
        &mut self,
        mob: &MobView,
        world: &W,
        body: &mut MobBody,
        message: BrainMessage,
    ) {
        let Some(behavior) = self.behavior.as_mut() else {
            return;
        };
        let mut context = AiContext {
            mob,
            world,
            brain: &mut self.brain,
            body,
        };
        let mut cx = BehaviorCx {
            ai: &mut context,
            brain: behavior.state_mut(),
        };
        crate::piglin::deliver(&mut cx, message);
        crate::animals::deliver(&mut cx, message);
    }

    pub fn warden_state(&self) -> &crate::WardenState {
        &self.brain.warden
    }

    pub fn warden_state_mut(&mut self) -> &mut crate::WardenState {
        &mut self.brain.warden
    }

    fn with_warden_cx<W: AiWorld, R>(
        &mut self,
        mob: &MobView,
        world: &W,
        body: &mut MobBody,
        run: impl FnOnce(&mut BehaviorCx<'_, '_, W>) -> R,
    ) -> Option<R> {
        let behavior = self.behavior.as_mut()?;
        let mut context = AiContext {
            mob,
            world,
            brain: &mut self.brain,
            body,
        };
        let mut cx = BehaviorCx {
            ai: &mut context,
            brain: behavior.state_mut(),
        };
        Some(run(&mut cx))
    }

    pub fn warden_can_receive_vibration<W: AiWorld>(
        &self,
        mob: &MobView,
        world: &W,
        pos: BlockPos,
        source: Option<&Living>,
    ) -> bool {
        self.behavior.as_ref().is_some_and(|behavior| {
            crate::warden::can_receive_vibration(mob, world, behavior.memories(), pos, source)
        })
    }

    pub fn warden_receive_vibration<W: AiWorld>(
        &mut self,
        mob: &MobView,
        world: &W,
        body: &mut MobBody,
        pos: BlockPos,
        source: Option<EntityId>,
        projectile_owner: Option<EntityId>,
    ) {
        self.with_warden_cx(mob, world, body, |cx| {
            crate::warden::on_vibration_received(cx, pos, source, projectile_owner);
        });
    }

    pub fn warden_increase_anger<W: AiWorld>(
        &mut self,
        mob: &MobView,
        world: &W,
        body: &mut MobBody,
        target: EntityId,
        amount: i32,
        play_sound: bool,
    ) {
        self.with_warden_cx(mob, world, body, |cx| {
            crate::warden::increase_anger_at(cx, target, amount, play_sound);
        });
    }

    pub fn warden_hurt<W: AiWorld>(
        &mut self,
        mob: &MobView,
        world: &W,
        body: &mut MobBody,
        attacker: EntityId,
        direct: bool,
    ) {
        self.with_warden_cx(mob, world, body, |cx| {
            crate::warden::on_hurt(cx, attacker, direct);
        });
    }

    pub fn warden_touch<W: AiWorld>(
        &mut self,
        mob: &MobView,
        world: &W,
        body: &mut MobBody,
        touched: EntityId,
    ) {
        self.with_warden_cx(mob, world, body, |cx| crate::warden::touch(cx, touched));
    }

    pub fn warden_hold_dig_cooldown(&mut self) {
        if let Some(behavior) = self.behavior.as_mut() {
            let memories = behavior.memories_mut();
            if memories.has_value(MemoryModuleType::DigCooldown) {
                memories.set_with_expiry(
                    MemoryModuleType::DigCooldown,
                    MemoryValue::Unit,
                    rspigot_domain::warden::DIG_COOLDOWN_TICKS,
                );
            }
        }
    }

    pub fn warden_spawned(&mut self, triggered: bool) {
        let Some(behavior) = self.behavior.as_mut() else {
            return;
        };
        let memories = behavior.memories_mut();
        memories.set_with_expiry(
            MemoryModuleType::DigCooldown,
            MemoryValue::Unit,
            rspigot_domain::warden::INITIAL_DIG_COOLDOWN_TICKS,
        );
        if triggered {
            memories.set_with_expiry(
                MemoryModuleType::IsEmerging,
                MemoryValue::Unit,
                i64::from(rspigot_domain::warden::EMERGE_DURATION),
            );
        }
    }

    pub fn warden_active_anger(&self) -> i32 {
        self.brain.warden.anger.active_anger(self.target())
    }

    pub fn warden_save(&self) -> (rspigot_nbt::Compound, rspigot_nbt::Compound) {
        (
            self.brain.warden.anger.to_nbt(),
            self.brain.warden.vibration.to_nbt(),
        )
    }

    pub fn warden_load(
        &mut self,
        anger: Option<&rspigot_nbt::Compound>,
        listener: Option<&rspigot_nbt::Compound>,
    ) {
        let delay = self.brain.random.next_int(WARDEN_CONVERSION_DELAYS);
        self.brain.warden.anger = anger.map_or_else(
            || rspigot_domain::warden::AngerManagement::empty(delay),
            |nbt| rspigot_domain::warden::AngerManagement::from_nbt(nbt, delay),
        );
        self.brain.warden.vibration =
            rspigot_domain::game_event::VibrationData::read_or_default(listener);
    }

    pub fn piglin_wants_to_pickup(&self, mob: &MobView, item: rspigot_domain::ItemId) -> bool {
        self.behavior
            .as_ref()
            .is_some_and(|behavior| crate::piglin::wants_to_pickup(mob, behavior.memories(), item))
    }

    pub fn random_nearby_position<W: AiWorld>(
        &mut self,
        mob: &MobView,
        world: &W,
        body: &mut MobBody,
        horizontal: i32,
        vertical: i32,
    ) -> Option<Vec3> {
        let mut context = AiContext {
            mob,
            world,
            brain: &mut self.brain,
            body,
        };
        crate::random_pos::land_pos(&mut context, horizontal, vertical)
    }

    pub fn piglin_can_admire(&self, mob: &MobView, held: rspigot_domain::ItemId) -> bool {
        self.behavior
            .as_ref()
            .is_some_and(|behavior| crate::piglin::can_admire(mob, behavior.memories(), held))
    }

    pub fn ambient_sound<W: AiWorld>(
        &mut self,
        mob: &MobView,
        world: &W,
        body: &mut MobBody,
    ) -> Option<&'static str> {
        let behavior = self.behavior.as_mut()?;
        let mut context = AiContext {
            mob,
            world,
            brain: &mut self.brain,
            body,
        };
        let cx = BehaviorCx {
            ai: &mut context,
            brain: behavior.state_mut(),
        };
        crate::piglin::ambient_sound(&cx)
    }

    fn update_creaking_home(&mut self, mob: &MobView) {
        if mob.kind != EntityKind::Creaking {
            return;
        }
        let home = mob.species.creaking.home_pos;
        if home.is_some() && self.brain.profile.creaking_home.is_none() {
            let profile = &mut self.brain.profile;
            profile.set_malus(PathType::Damaging, 8.0);
            profile.set_malus(PathType::PowderSnow, 8.0);
            profile.set_malus(PathType::Lava, 8.0);
            profile.set_malus(PathType::Fire, 0.0);
            profile.set_malus(PathType::FireInNeighbor, 0.0);
        }
        self.brain.profile.creaking_home = home;
    }

    pub fn creaking_state(&self) -> crate::CreakingState {
        self.brain.creaking
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

    pub fn stop_pollinating(&mut self) {
        self.brain.bee.pollinating = false;
    }

    pub fn is_pollinating(&self) -> bool {
        self.brain.bee.pollinating
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
        self.brain.movement.set_in_water(mob.fluid.in_water);
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
        let mut breeze_view;
        let mob = if mob.kind == EntityKind::Breeze {
            breeze_view = *mob;
            breeze_view.species.breeze.attack_target = self.target();
            &breeze_view
        } else {
            mob
        };
        self.brain.fox = mob.species.fox;
        self.brain.bee.angry = mob.angry;
        if mob.kind == EntityKind::Vex {
            self.brain.target_position = self
                .brain
                .target
                .and_then(|id| world.living(id))
                .map(|living| living.position);
        }
        self.update_creaking_home(mob);
        self.brain.look.set_max_head_x_rot(mob.max_head_x_rot());
        if mob.kind == EntityKind::Shulker {
            self.brain
                .look
                .set_shulker_face(Some(mob.species.shulker.attach_face));
        }
        if self.brain.panda.active {
            self.brain.panda.facts = mob.species.panda;
        }
        let mut context = AiContext {
            mob,
            world,
            brain: &mut self.brain,
            body,
        };
        apply_pending(
            &mut self.goals,
            &mut self.targets,
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
        if mob.kind == EntityKind::Creaking
            && let Some(behavior) = self.behavior.as_mut()
        {
            let mut cx = BehaviorCx {
                ai: &mut context,
                brain: behavior.state_mut(),
            };
            crate::creaking::apply_can_move(&mut cx);
        }
        let pollinating = mob.kind == EntityKind::Bee && context.brain.bee.pollinating;
        if (context.brain.creaking.can_move || mob.kind != EntityKind::Creaking) && !pollinating {
            context.tick_navigation();
        }
        if mob.kind == EntityKind::Wither {
            crate::wither::custom_step(&mut context);
        }
        if mob.kind == EntityKind::Drowned {
            context.brain.drowned_target = context
                .brain
                .target
                .and_then(|id| world.living(id))
                .map(|target| (target.position.y, target.in_water));
        }
        if let Some(behavior) = self.behavior.as_mut() {
            sync_path_memory(behavior.memories_mut(), context.brain.navigation.path());
            behavior.tick(&mut context);
            if mob.kind == EntityKind::CopperGolem {
                crate::copper_golem::update_activity(behavior);
            }
            crate::piglin::after_brain_tick(behavior, &mut context);
            crate::animals::after_brain_tick(behavior, &mut context);
        }
        if mob.kind == EntityKind::Fox {
            self.settle_fox(mob);
        }
    }

    fn settle_fox(&mut self, mob: &MobView) {
        let brain = &mut self.brain;
        let retargeting = brain
            .target_requests
            .iter()
            .any(|request| request.to.is_some());
        if brain.target.is_none() && !brain.defer_targets && !retargeting {
            brain.fox.set_defending(false);
        }
        if brain.fox.flags != mob.species.fox.flags {
            let flags = brain.fox.flags;
            brain
                .actions
                .insert(0, MobAction::Fox(crate::FoxAction::SetFlags(flags)));
        }
    }

    pub fn trigger_guardian_stroll(&mut self) {
        self.brain.guardian.stroll_trigger = true;
    }

    pub fn fox_target_goals(&mut self, red: bool) {
        crate::fox::add_target_goals(&mut self.targets, red);
    }

    pub fn act(&mut self, base: &mut EntityBase, living: &mut LivingState, view: &impl BlockView) {
        let brain = &mut self.brain;
        brain.hop_move_pre_tick(base, living);
        let profile = &brain.profile;
        let state = MobState {
            position: base.position,
            on_ground: base.on_ground,
            in_floatable_fluid: base.fluid.in_water,
        };
        let frozen = (base.kind == EntityKind::Creaking && !brain.creaking.can_move)
            || (base.kind == EntityKind::Axolotl
                && rspigot_domain::entity::species::axolotl::is_playing_dead(base));
        if (base.kind != EntityKind::Fox || fox::can_move(base))
            && brain.panda.can_move()
            && !frozen
        {
            match brain.movement.style() {
                MoveStyle::Ground => {
                    brain
                        .movement
                        .tick(base, living, view, &mut brain.jump, |pos| {
                            crate::walk::mob_path_type(view, profile, state, pos)
                                == PathType::Walkable
                        });
                }
                MoveStyle::Ghast => {
                    brain
                        .movement
                        .tick_ghast(base, living, view, &mut brain.random);
                }
                MoveStyle::Flying => brain.movement.tick_flying(base, living),
                MoveStyle::SmoothSwimming => {
                    let done = brain.navigation.is_done();
                    brain.movement.tick_smooth_swimming(base, living, done);
                }
                MoveStyle::Fish => {
                    let done = brain.navigation.is_done();
                    brain.movement.tick_fish(base, living, done);
                }
                MoveStyle::Drowned => {
                    let done = brain.navigation.is_done();
                    let swimming = drowned_swim_of(living, brain.drowned_target, base.position.y);
                    if !brain.movement.tick_drowned(base, living, done, swimming) {
                        brain
                            .movement
                            .tick(base, living, view, &mut brain.jump, |pos| {
                                crate::walk::mob_path_type(view, profile, state, pos)
                                    == PathType::Walkable
                            });
                    }
                }
                MoveStyle::Guardian => {
                    let done = brain.navigation.is_done();
                    brain
                        .movement
                        .tick_guardian(base, living, done, &mut brain.look);
                }
                MoveStyle::Turtle => {
                    let done = brain.navigation.is_done();
                    let (home, baby) = turtle_home_of(base, living);
                    brain.movement.tick_turtle(base, living, done, home, baby);
                }
                MoveStyle::Phantom => {
                    let target = brain.phantom.move_target;
                    brain.movement.tick_phantom(base, living, target);
                }
                MoveStyle::Cube => {
                    if let Some(sound) =
                        brain
                            .movement
                            .tick_cube(base, living, &mut brain.jump, &mut brain.random)
                    {
                        brain.cube_sound = Some(sound);
                    }
                }
                MoveStyle::Vex => {
                    let target = brain.target_position;
                    brain.movement.tick_vex(base, living, target);
                }
            }
        }
        let navigating = brain.navigation.is_in_progress();
        if frozen {
            brain.jump = JumpControl::default();
            return;
        }
        if base.kind == EntityKind::Fox {
            crate::fox::tick_look(&mut brain.look, base, living, navigating);
        } else if base.kind == EntityKind::Bee {
            if !brain.bee.angry {
                brain
                    .look
                    .tick_with(base, living, navigating, !brain.bee.pollinating);
            }
        } else if base.kind == EntityKind::Frog {
            let reset_x_rot = rspigot_domain::entity::species::frog::tongue_target(base).is_none();
            brain.look.tick_with(base, living, navigating, reset_x_rot);
        } else if brain.look.smooth_swimming().is_some() {
            brain.look.tick_smooth(base, living, !navigating);
        } else if base.kind != EntityKind::Phantom {
            brain.look.tick(base, living, navigating);
        }
        if brain.hop.is_some() {
            brain.hop_jump_tick(base, living);
            brain.hop_tune_jump(base, living);
        } else {
            brain.jump.tick(living);
        }
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
        if base.kind == EntityKind::Shulker
            || (base.kind == EntityKind::Creaking && !self.brain.creaking.can_move)
        {
            return;
        }
        if base.kind == EntityKind::Phantom {
            base.head_yaw = living.body_yaw;
            living.body_yaw = base.yaw;
            return;
        }
        let max_head_y_rot = if base.kind == EntityKind::Dolphin {
            DOLPHIN_MAX_HEAD_Y_ROT
        } else {
            crate::control::MAX_HEAD_Y_ROT
        };
        self.brain
            .body
            .client_tick_max(base, living, max_head_y_rot);
    }

    pub fn take_cube_sound(&mut self) -> Option<crate::cube::CubeSound> {
        self.brain.cube_sound.take()
    }

    pub fn update_control_flags(&mut self) {
        self.goals.set_control_flag(Flag::Move, true);
        self.goals.set_control_flag(Flag::Jump, true);
        self.goals.set_control_flag(Flag::Look, true);
    }
}

fn drowned_swim_of(
    living: &LivingState,
    target: Option<(f64, bool)>,
    y: f64,
) -> crate::DrownedSwim {
    let searching_for_land = match living.mob.as_ref().map(|mob| mob.species) {
        Some(rspigot_domain::entity::species::Species::Zombie(zombie)) => zombie.searching_for_land,
        _ => false,
    };
    let in_water = target.is_some_and(|(_, in_water)| in_water);
    crate::DrownedSwim {
        wants_to_swim: searching_for_land || in_water,
        searching_for_land,
        target_above: target.is_some_and(|(target_y, _)| target_y > y),
    }
}

fn turtle_home_of(base: &EntityBase, living: &LivingState) -> (BlockPos, bool) {
    let Some(mob) = living.mob.as_ref() else {
        return (BlockPos::new(0, 0, 0), false);
    };
    let home = match mob.species {
        rspigot_domain::entity::species::Species::Turtle(state) => state.home_pos,
        _ => BlockPos::new(0, 0, 0),
    };
    let baby = mob
        .ageable
        .as_ref()
        .is_some_and(|ageable| rspigot_domain::entity::ageable::is_baby(base, ageable));
    (home, baby)
}

fn apply_pending<W: AiWorld>(
    goals: &mut GoalSelector<MobGoal>,
    targets: &mut GoalSelector<MobGoal>,
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
            GoalChange::CatTame(tame) => {
                goals.remove_goals(
                    context,
                    |goal| matches!(goal, MobGoal::AvoidEntity(avoid) if avoid.is_tame_gated()),
                );
                if !tame {
                    goals.add_goal(crate::cat::AVOID_PRIORITY, crate::cat::avoid_players());
                }
            }
            GoalChange::BreakDoors(false) => {
                goals.remove_goals(context, |goal| matches!(goal, MobGoal::BreakDoor(_)));
            }
            GoalChange::Trusting(trusting) => {
                goals.remove_goals(
                    context,
                    |goal| matches!(goal, MobGoal::AvoidEntity(avoid) if avoid.is_trust_gated()),
                );
                if !trusting {
                    goals.add_goal(
                        registry::OCELOT_AVOID_PRIORITY,
                        MobGoal::AvoidEntity(registry::ocelot_avoid_players()),
                    );
                }
            }
            GoalChange::KillerBunny => registry::add_killer_bunny_goals(goals, targets),
        }
    }
}

fn sync_path_memory(memories: &mut crate::behavior::Memories, path: Option<&crate::Path>) {
    if memories.has_value(MemoryModuleType::Path)
        && let Some(path) = path
    {
        memories.set(MemoryModuleType::Path, MemoryValue::Path(path.clone()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behavior::Memories;
    use crate::{Path, PathNode};

    fn path(advanced: usize) -> Path {
        let nodes = (0..4)
            .map(|x| PathNode::new(BlockPos::new(x, 64, 0), PathType::Walkable))
            .collect();
        let mut path = Path::new(nodes, BlockPos::new(3, 64, 0), true);
        for _ in 0..advanced {
            path.advance();
        }
        path
    }

    fn memories() -> Memories {
        let mut memories = Memories::default();
        memories.register(MemoryModuleType::Path);
        memories
    }

    fn stored(memories: &Memories) -> Option<usize> {
        memories
            .get(MemoryModuleType::Path)
            .and_then(MemoryValue::as_path)
            .map(Path::next_node_index)
    }

    #[test]
    fn the_path_memory_follows_the_navigation_progress() {
        let mut memories = memories();
        memories.set(MemoryModuleType::Path, MemoryValue::Path(path(1)));
        sync_path_memory(&mut memories, Some(&path(3)));
        assert_eq!(stored(&memories), Some(3));
    }

    #[test]
    fn an_absent_path_memory_is_not_created_from_the_navigation() {
        let mut memories = memories();
        sync_path_memory(&mut memories, Some(&path(2)));
        assert_eq!(stored(&memories), None);
    }

    #[test]
    fn a_stopped_navigation_leaves_the_memory_alone() {
        let mut memories = memories();
        memories.set(MemoryModuleType::Path, MemoryValue::Path(path(2)));
        sync_path_memory(&mut memories, None);
        assert_eq!(stored(&memories), Some(2));
    }
}
