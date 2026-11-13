use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::poi::Occupancy;
use rspigot_domain::{BlockPos, Vec3, mth};

use crate::action::MobAction;
use crate::air::AirAction;
use crate::air::AirGoal;
use crate::animal::block_tag;
use crate::animal::{BreedGoal, FollowParentGoal};
use crate::brain::AiContext;
use crate::combat::{HurtByTargetGoal, MeleeAttackGoal};
use crate::goal::{Flag, Flags, Goal, adjusted_tick_delay};
use crate::goals::{FloatGoal, MobGoal, NearestAttackableTargetGoal, TemptGoal, TemptItems};
use crate::neutral::ResetUniversalAngerGoal;
use crate::random::RandomSource;
use crate::random_pos;
use crate::registry::Goals;
use crate::scan::{closer_to_center_than, manhattan_ordered};
use crate::world::AiWorld;

pub const ATTACK_SPEED: f64 = 1.399_999_976_158_142;
pub const TEMPT_SPEED: f64 = 1.25;
pub const FOLLOW_PARENT_SPEED: f64 = 1.25;
pub const BEE_FOOD: &str = "minecraft:bee_food";
pub const BEE_ATTRACTIVE: &str = "minecraft:bee_attractive";
pub const BEE_GROWABLES: &str = "minecraft:bee_growables";
pub const BEEHIVES: &str = "minecraft:beehives";
pub const CROP_GROWTH_EVENT: i32 = 2011;
pub const CROP_GROWTH_PARTICLES: i32 = 15;
const MAX_TARGET_DISTANCE: i32 = 48;
const ENTER_DISTANCE: f64 = 2.0;
const REACH_DISTANCE: i32 = 2;
const DIRECT_PATH_DISTANCE: i32 = 16;
const FAR_PATH_DISTANCE: i32 = 3;
const TRAVEL_LIMIT: i32 = 2400;
const STUCK_LIMIT: i32 = 60;
const BLACKLIST_SIZE: usize = 3;
const HIVE_SEARCH_RADIUS: i32 = 20;
const FLOWER_RANGE: i32 = 5;
const FLOWER_SPEED: f64 = 1.2;
const FLOWER_MEMORY: i64 = 600;
const KNOWN_FLOWER_AFTER: i32 = 600;
const POLLINATE_LIMIT: i32 = 600;
const POLLINATE_ENOUGH: i32 = 400;
const POLLINATE_STOP_CHANCE: f32 = 0.2;
const POLLINATE_HEIGHT: f64 = 0.600_000_023_841_857_9;
const POLLINATE_REACH: f64 = 1.0;
const POLLINATE_HOVER_TOLERANCE: f64 = 0.1;
const POLLINATE_HOVER_ODDS: i32 = 25;
const POLLINATE_SOUND_CHANCE: f32 = 0.05;
const POLLINATE_SOUND_GAP: i32 = 60;
const POLLINATE_WANTED_SPEED: f64 = 0.349_999_994_039_535_5;
const VALIDATE_MIN: i32 = 20;
const VALIDATE_MAX: i32 = 40;
const GROW_ODDS: i32 = 30;
const GROW_CHANCE: f32 = 0.3;
const WANDER_ODDS: i32 = 10;
const WANDER_HORIZONTAL: i32 = 8;
const WANDER_HOVER_VERTICAL: i32 = 7;
const WANDER_HOVER_MAX: i32 = 3;
const WANDER_HOVER_MIN: i32 = 1;
const WANDER_AIR_VERTICAL: i32 = 4;
const WANDER_AIR_HEIGHT: i32 = -2;
const HALF_PI: f64 = 1.570_796_370_506_286_6;
const TENTH_PI: f64 = 0.314_159_274_101_257_3;
const ANGER_TARGET_INTERVAL: i32 = 10;
const SWEET_BERRY_MAX_AGE: i32 = 3;
const STEM_MAX_AGE: i32 = 7;
const SUNFLOWER_UPPER: &str = "upper";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BeeFacts {
    pub hive_pos: Option<BlockPos>,
    pub flower_pos: Option<BlockPos>,
    pub has_nectar: bool,
    pub has_stung: bool,
    pub ticks_without_nectar: i32,
    pub stay_out_countdown: i32,
    pub crops_grown: i32,
    pub locate_hive_cooldown: i32,
    pub locate_flower_cooldown: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HiveView {
    pub full: bool,
    pub fire_nearby: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BeeBrain {
    pub pollinating: bool,
    pub angry: bool,
    pub blacklist: Vec<BlockPos>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BeeAction {
    Alert {
        mob: rspigot_domain::EntityId,
        target: rspigot_domain::EntityId,
    },
    EnterHive(BlockPos),
    GrowCrop {
        pos: BlockPos,
        to: StateId,
    },
}

fn act<W: AiWorld>(context: &mut AiContext<'_, W>, action: BeeAction) {
    context.act(MobAction::Air(AirAction::Bee(action)));
}

fn dist_sqr(a: BlockPos, b: BlockPos) -> i64 {
    let dx = i64::from(a.x - b.x);
    let dy = i64::from(a.y - b.y);
    let dz = i64::from(a.z - b.z);
    dx * dx + dy * dy + dz * dz
}

fn manhattan(a: BlockPos, b: BlockPos) -> i32 {
    (a.x - b.x).abs() + (a.y - b.y).abs() + (a.z - b.z).abs()
}

fn pos_closer_than(a: BlockPos, b: BlockPos, distance: i32) -> bool {
    dist_sqr(a, b) < i64::from(distance) * i64::from(distance)
}

fn closer_than<W: AiWorld>(context: &AiContext<'_, W>, pos: BlockPos, distance: i32) -> bool {
    pos_closer_than(pos, context.mob.block_pos(), distance)
}

fn too_far_away<W: AiWorld>(context: &AiContext<'_, W>, pos: BlockPos) -> bool {
    !closer_than(context, pos, MAX_TARGET_DISTANCE)
}

fn hive_view<W: AiWorld>(context: &AiContext<'_, W>) -> Option<HiveView> {
    let pos = context.body.bee.hive_pos?;
    if too_far_away(context, pos) {
        return None;
    }
    context.world.beehive(pos)
}

fn is_hive_valid<W: AiWorld>(context: &AiContext<'_, W>) -> bool {
    hive_view(context).is_some()
}

fn is_hive_near_fire<W: AiWorld>(context: &AiContext<'_, W>) -> bool {
    hive_view(context).is_some_and(|hive| hive.fire_nearby)
}

pub fn attracts_bees(state: Option<StateId>) -> bool {
    let Some(state) = state else {
        return false;
    };
    if !block_tag(Some(state), BEE_ATTRACTIVE) {
        return false;
    }
    if state.property("waterlogged") == Some("true") {
        return false;
    }
    if state.kind() == Some(BlockKind::Sunflower) {
        return state.property("half") == Some(SUNFLOWER_UPPER);
    }
    true
}

pub fn wants_to_enter_hive<W: AiWorld>(context: &AiContext<'_, W>) -> bool {
    let facts = &context.body.bee;
    if facts.stay_out_countdown > 0
        || context.brain.bee.pollinating
        || facts.has_stung
        || context.brain.target.is_some()
    {
        return false;
    }
    let tired =
        facts.ticks_without_nectar > rspigot_domain::entity::species::bee::TIRED_OF_NECTAR_TICKS;
    let wants =
        facts.has_nectar || tired || context.world.bees_stay_in_hive(context.mob.block_pos());
    wants && !is_hive_near_fire(context)
}

fn drop_hive<W: AiWorld>(context: &mut AiContext<'_, W>) {
    let facts = &mut context.body.bee;
    facts.hive_pos = None;
    facts.locate_hive_cooldown = rspigot_domain::entity::species::bee::HIVE_COOLDOWN;
}

fn drop_flower<W: AiWorld>(context: &mut AiContext<'_, W>) {
    let cooldown = context.brain.random.next_int_between(
        rspigot_domain::entity::species::bee::LOCATE_COOLDOWN_MIN,
        rspigot_domain::entity::species::bee::LOCATE_COOLDOWN_MAX,
    );
    let facts = &mut context.body.bee;
    facts.flower_pos = None;
    facts.locate_flower_cooldown = cooldown;
}

fn bottom_center(pos: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y),
        f64::from(pos.z) + 0.5,
    )
}

fn distance_to(a: Vec3, b: Vec3) -> f64 {
    a.distance_squared(b).sqrt()
}

fn pathfind_randomly_towards<W: AiWorld>(context: &mut AiContext<'_, W>, target: BlockPos) {
    let towards = bottom_center(target);
    let here = context.mob.block_pos();
    let dy = target.y - here.y;
    let y_offset = if dy > 2 {
        4
    } else if dy < -2 {
        -4
    } else {
        0
    };
    let mut horizontal = 6;
    let mut vertical = 8;
    let distance = manhattan(here, target);
    if distance < 15 {
        horizontal = distance / 2;
        vertical = distance / 2;
    }
    if let Some(position) =
        random_pos::air_pos_towards(context, horizontal, vertical, y_offset, towards, TENTH_PI)
    {
        context
            .brain
            .navigation
            .set_max_visited_nodes_multiplier(0.5);
        context.move_to_pos(position, 1.0);
    }
}

fn target_gate<W: AiWorld>(context: &AiContext<'_, W>) -> bool {
    !context.mob.angry
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttackGoal {
    melee: MeleeAttackGoal,
}

impl AttackGoal {
    pub fn new() -> Self {
        Self {
            melee: MeleeAttackGoal::new(ATTACK_SPEED, true),
        }
    }
}

impl Default for AttackGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for AttackGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.melee.can_use(context) && context.mob.angry && !context.body.bee.has_stung
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.melee.can_continue_to_use(context) && context.mob.angry && !context.body.bee.has_stung
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.melee.start(context);
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        self.melee.stop(context);
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        self.melee.tick(context);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EnterHiveGoal;

impl<W: AiWorld> Goal<AiContext<'_, W>> for EnterHiveGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(hive) = context.body.bee.hive_pos else {
            return false;
        };
        if !wants_to_enter_hive(context)
            || !closer_to_center_than(hive, context.mob.position, ENTER_DISTANCE)
        {
            return false;
        }
        let Some(view) = hive_view(context) else {
            return false;
        };
        if !view.full {
            return target_gate(context);
        }
        context.body.bee.hive_pos = None;
        false
    }

    fn can_continue_to_use(&mut self, _context: &mut AiContext<'_, W>) -> bool {
        false
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        if let Some(hive) = context.body.bee.hive_pos
            && hive_view(context).is_some()
        {
            act(context, BeeAction::EnterHive(hive));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidateHiveGoal {
    cooldown: i32,
    last_validate: i64,
}

impl ValidateHiveGoal {
    pub fn new(random: &mut impl RandomSource) -> Self {
        Self {
            cooldown: random.next_int_between(VALIDATE_MIN, VALIDATE_MAX),
            last_validate: -1,
        }
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for ValidateHiveGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        target_gate(context)
            && self.last_validate + i64::from(self.cooldown) < context.world.game_time()
    }

    fn can_continue_to_use(&mut self, _context: &mut AiContext<'_, W>) -> bool {
        false
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        if let Some(hive) = context.body.bee.hive_pos
            && context.world.chunk_loaded(hive.chunk())
            && !is_hive_valid(context)
        {
            drop_hive(context);
        }
        self.last_validate = context.world.game_time();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidateFlowerGoal {
    cooldown: i32,
    last_validate: i64,
}

impl ValidateFlowerGoal {
    pub fn new(random: &mut impl RandomSource) -> Self {
        Self {
            cooldown: random.next_int_between(VALIDATE_MIN, VALIDATE_MAX),
            last_validate: -1,
        }
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for ValidateFlowerGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        target_gate(context)
            && self.last_validate + i64::from(self.cooldown) < context.world.game_time()
    }

    fn can_continue_to_use(&mut self, _context: &mut AiContext<'_, W>) -> bool {
        false
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        if let Some(flower) = context.body.bee.flower_pos
            && context.world.chunk_loaded(flower.chunk())
            && !attracts_bees(context.world.state(flower))
        {
            drop_flower(context);
        }
        self.last_validate = context.world.game_time();
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PollinateGoal {
    unreachable: Vec<(BlockPos, i64)>,
    successful_ticks: i32,
    pollinating_ticks: i32,
    last_sound: i32,
    hover: Option<Vec3>,
}

impl PollinateGoal {
    pub fn new() -> Self {
        Self::default()
    }

    fn pollinated_long_enough(&self) -> bool {
        POLLINATE_ENOUGH < self.successful_ticks
    }

    fn offset<W: AiWorld>(context: &mut AiContext<'_, W>) -> f64 {
        let value = context.brain.random.next_float();
        f64::from((value * 2.0 - 1.0) * 0.333_333_34)
    }

    fn set_wanted_pos<W: AiWorld>(&self, context: &mut AiContext<'_, W>) {
        if let Some(hover) = self.hover {
            context
                .brain
                .movement
                .set_wanted_position(hover, POLLINATE_WANTED_SPEED);
        }
    }

    fn find_nearby_flower<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> Option<BlockPos> {
        let origin = context.mob.block_pos();
        let now = context.world.game_time();
        let mut fresh: Vec<(BlockPos, i64)> = Vec::new();
        let mut found = None;
        for pos in manhattan_ordered(origin, FLOWER_RANGE, FLOWER_RANGE, FLOWER_RANGE) {
            if !attracts_bees(context.world.state(pos)) {
                continue;
            }
            let until = self
                .unreachable
                .iter()
                .find(|(cached, _)| *cached == pos)
                .map_or(i64::MIN, |(_, until)| *until);
            if now < until {
                fresh.push((pos, until));
                continue;
            }
            let reachable = context
                .create_path_to(pos, 1)
                .is_some_and(|path| path.can_reach());
            if reachable {
                found = Some(pos);
                break;
            }
            fresh.push((pos, now + FLOWER_MEMORY));
        }
        self.unreachable = fresh;
        found
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for PollinateGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let facts = context.body.bee;
        if facts.locate_flower_cooldown > 0 || facts.has_nectar || context.world.raining() {
            return false;
        }
        if let Some(flower) = self.find_nearby_flower(context) {
            context.body.bee.flower_pos = Some(flower);
            let at = Vec3::new(
                f64::from(flower.x) + 0.5,
                f64::from(flower.y) + 0.5,
                f64::from(flower.z) + 0.5,
            );
            context.move_to_pos(at, FLOWER_SPEED);
            return target_gate(context);
        }
        context.body.bee.locate_flower_cooldown = context.brain.random.next_int_between(
            rspigot_domain::entity::species::bee::LOCATE_COOLDOWN_MIN,
            rspigot_domain::entity::species::bee::LOCATE_COOLDOWN_MAX,
        );
        false
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if !context.brain.bee.pollinating
            || context.body.bee.flower_pos.is_none()
            || context.world.raining()
        {
            return false;
        }
        let keeps = if self.pollinated_long_enough() {
            context.brain.random.next_float() < POLLINATE_STOP_CHANCE
        } else {
            true
        };
        keeps && target_gate(context)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.successful_ticks = 0;
        self.pollinating_ticks = 0;
        self.last_sound = 0;
        context.brain.bee.pollinating = true;
        context.body.bee.ticks_without_nectar = 0;
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        if self.pollinated_long_enough() {
            context.body.bee.has_nectar = true;
            context.body.bee.ticks_without_nectar = 0;
        }
        context.brain.bee.pollinating = false;
        context.stop_navigation();
        context.body.bee.locate_flower_cooldown =
            rspigot_domain::entity::species::bee::FLOWER_COOLDOWN;
    }

    fn requires_update_every_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let Some(flower) = context.body.bee.flower_pos else {
            return;
        };
        self.pollinating_ticks += 1;
        if self.pollinating_ticks > POLLINATE_LIMIT {
            drop_flower(context);
            context.brain.bee.pollinating = false;
            context.body.bee.locate_flower_cooldown =
                rspigot_domain::entity::species::bee::FLOWER_COOLDOWN;
            return;
        }
        let target = bottom_center(flower) + Vec3::new(0.0, POLLINATE_HEIGHT, 0.0);
        let position = context.mob.position;
        if distance_to(target, position) > POLLINATE_REACH {
            self.hover = Some(target);
            self.set_wanted_pos(context);
            return;
        }
        let hover = *self.hover.get_or_insert(target);
        let at_hover = distance_to(position, hover) <= POLLINATE_HOVER_TOLERANCE;
        let mut update = true;
        if at_hover {
            if context.brain.random.next_int(POLLINATE_HOVER_ODDS) == 0 {
                let x = Self::offset(context);
                let z = Self::offset(context);
                self.hover = Some(Vec3::new(target.x + x, target.y, target.z + z));
                context.stop_navigation();
            } else {
                update = false;
            }
            context.brain.look.look_at(target);
        }
        if update {
            self.set_wanted_pos(context);
        }
        self.successful_ticks += 1;
        if context.brain.random.next_float() < POLLINATE_SOUND_CHANCE
            && self.last_sound + POLLINATE_SOUND_GAP < self.successful_ticks
        {
            self.last_sound = self.successful_ticks;
            context.act(MobAction::Sound {
                sound: rspigot_domain::entity::species::bee::POLLINATE_SOUND,
                volume: 1.0,
                pitch: 1.0,
            });
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LocateHiveGoal;

impl LocateHiveGoal {
    fn nearby_hives_with_space<W: AiWorld>(context: &AiContext<'_, W>) -> Vec<BlockPos> {
        let origin = context.mob.block_pos();
        let mut hives: Vec<BlockPos> = context
            .world
            .pois_in_range(origin, HIVE_SEARCH_RADIUS, Occupancy::Any)
            .iter()
            .filter(|record| record.kind().is_bee_home())
            .map(|record| record.pos())
            .filter(|pos| context.world.beehive(*pos).is_some_and(|hive| !hive.full))
            .collect();
        hives.sort_by_key(|pos| dist_sqr(*pos, origin));
        hives
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for LocateHiveGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.body.bee.locate_hive_cooldown == 0
            && context.body.bee.hive_pos.is_none()
            && wants_to_enter_hive(context)
            && target_gate(context)
    }

    fn can_continue_to_use(&mut self, _context: &mut AiContext<'_, W>) -> bool {
        false
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        context.body.bee.locate_hive_cooldown = rspigot_domain::entity::species::bee::HIVE_COOLDOWN;
        let hives = Self::nearby_hives_with_space(context);
        if hives.is_empty() {
            return;
        }
        for hive in &hives {
            if !context.brain.bee.blacklist.contains(hive) {
                context.body.bee.hive_pos = Some(*hive);
                return;
            }
        }
        context.brain.bee.blacklist.clear();
        context.body.bee.hive_pos = Some(hives[0]);
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct GoToHiveGoal {
    travelling_ticks: i32,
    ticks_stuck: i32,
    last_path: Option<crate::Path>,
}

impl GoToHiveGoal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn travelling_ticks(&self) -> i32 {
        self.travelling_ticks
    }

    fn has_reached_target<W: AiWorld>(context: &AiContext<'_, W>, pos: BlockPos) -> bool {
        if closer_than(context, pos, REACH_DISTANCE) {
            return true;
        }
        context
            .brain
            .navigation
            .path()
            .is_some_and(|path| path.target() == pos && path.can_reach() && path.is_done())
    }

    fn drop_and_blacklist<W: AiWorld>(context: &mut AiContext<'_, W>) {
        if let Some(hive) = context.body.bee.hive_pos {
            let blacklist = &mut context.brain.bee.blacklist;
            blacklist.push(hive);
            while blacklist.len() > BLACKLIST_SIZE {
                blacklist.remove(0);
            }
        }
        drop_hive(context);
    }

    fn pathfind_directly_towards<W: AiWorld>(
        context: &mut AiContext<'_, W>,
        pos: BlockPos,
    ) -> bool {
        let accuracy = if closer_than(context, pos, FAR_PATH_DISTANCE) {
            1
        } else {
            2
        };
        context
            .brain
            .navigation
            .set_max_visited_nodes_multiplier(10.0);
        let path = context.create_path_to(pos, accuracy);
        context.move_along(path, 1.0);
        context
            .brain
            .navigation
            .path()
            .is_some_and(crate::Path::can_reach)
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for GoToHiveGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(hive) = context.body.bee.hive_pos else {
            return false;
        };
        !too_far_away(context, hive)
            && !context.mob.has_home()
            && wants_to_enter_hive(context)
            && !Self::has_reached_target(context, hive)
            && block_tag(context.world.state(hive), BEEHIVES)
            && target_gate(context)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.travelling_ticks = 0;
        self.ticks_stuck = 0;
        let _ = context;
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        self.travelling_ticks = 0;
        self.ticks_stuck = 0;
        context.stop_navigation();
        context
            .brain
            .navigation
            .reset_max_visited_nodes_multiplier();
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let Some(hive) = context.body.bee.hive_pos else {
            return;
        };
        self.travelling_ticks += 1;
        if self.travelling_ticks > adjusted_tick_delay(false, TRAVEL_LIMIT) {
            Self::drop_and_blacklist(context);
            return;
        }
        if context.brain.navigation.is_in_progress() {
            return;
        }
        if closer_than(context, hive, DIRECT_PATH_DISTANCE) {
            if !Self::pathfind_directly_towards(context, hive) {
                Self::drop_and_blacklist(context);
                return;
            }
            if let Some(last) = &self.last_path
                && context
                    .brain
                    .navigation
                    .path()
                    .is_some_and(|path| path.same_as(Some(last)))
            {
                self.ticks_stuck += 1;
                if self.ticks_stuck > STUCK_LIMIT {
                    drop_hive(context);
                    self.ticks_stuck = 0;
                }
                return;
            }
            self.last_path = context.brain.navigation.path().cloned();
        } else if too_far_away(context, hive) {
            drop_hive(context);
        } else {
            pathfind_randomly_towards(context, hive);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GoToKnownFlowerGoal {
    travelling_ticks: i32,
}

impl GoToKnownFlowerGoal {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn travelling_ticks(&self) -> i32 {
        self.travelling_ticks
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for GoToKnownFlowerGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let Some(flower) = context.body.bee.flower_pos else {
            return false;
        };
        !context.mob.has_home()
            && context.body.bee.ticks_without_nectar > KNOWN_FLOWER_AFTER
            && !closer_than(context, flower, REACH_DISTANCE)
            && target_gate(context)
    }

    fn start(&mut self, _context: &mut AiContext<'_, W>) {
        self.travelling_ticks = 0;
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        self.travelling_ticks = 0;
        context.stop_navigation();
        context
            .brain
            .navigation
            .reset_max_visited_nodes_multiplier();
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let Some(flower) = context.body.bee.flower_pos else {
            return;
        };
        self.travelling_ticks += 1;
        if self.travelling_ticks > adjusted_tick_delay(false, TRAVEL_LIMIT) {
            drop_flower(context);
        } else if !context.brain.navigation.is_in_progress() {
            if too_far_away(context, flower) {
                drop_flower(context);
            } else {
                pathfind_randomly_towards(context, flower);
            }
        }
    }
}

fn grown_state(state: StateId, kind: BlockKind) -> Option<StateId> {
    let age = |name: &str| {
        state
            .property(name)
            .and_then(|value| value.parse::<i32>().ok())
    };
    match kind {
        BlockKind::Wheat | BlockKind::Carrots | BlockKind::Potatoes | BlockKind::Beetroots => {
            let max = rspigot_domain::crop::max_age(kind)?;
            let current = age("age")?;
            (current < max)
                .then(|| state.with_property("age", &(current + 1).to_string()))
                .flatten()
        }
        BlockKind::TorchflowerCrop => {
            let current = age("age")?;
            (current < 1)
                .then(|| state.with_property("age", &(current + 1).to_string()))
                .flatten()
        }
        BlockKind::PumpkinStem | BlockKind::MelonStem => {
            let current = age("age")?;
            (current < STEM_MAX_AGE)
                .then(|| state.with_property("age", &(current + 1).to_string()))
                .flatten()
        }
        BlockKind::SweetBerryBush => {
            let current = age("age")?;
            (current < SWEET_BERRY_MAX_AGE)
                .then(|| state.with_property("age", &(current + 1).to_string()))
                .flatten()
        }
        BlockKind::CaveVines | BlockKind::CaveVinesPlant => (state.property("berries")
            == Some("false"))
        .then(|| state.with_property("berries", "true"))
        .flatten(),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GrowCropGoal;

impl<W: AiWorld> Goal<AiContext<'_, W>> for GrowCropGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.body.bee.crops_grown < rspigot_domain::entity::species::bee::MAX_CROPS_GROWN
            && context.brain.random.next_float() >= GROW_CHANCE
            && context.body.bee.has_nectar
            && is_hive_valid(context)
            && target_gate(context)
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        if context
            .brain
            .random
            .next_int(adjusted_tick_delay(false, GROW_ODDS))
            != 0
        {
            return;
        }
        for depth in 1..3 {
            let pos = context.mob.block_pos().offset(0, -depth, 0);
            let Some(state) = context.world.state(pos) else {
                continue;
            };
            if !block_tag(Some(state), BEE_GROWABLES) {
                continue;
            }
            let Some(kind) = state.kind() else {
                continue;
            };
            if let Some(to) = grown_state(state, kind) {
                act(context, BeeAction::GrowCrop { pos, to });
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WanderGoal;

impl WanderGoal {
    fn wander_threshold<W: AiWorld>(context: &AiContext<'_, W>) -> i32 {
        let facts = &context.body.bee;
        let anchored = facts.hive_pos.is_some() || facts.flower_pos.is_some();
        MAX_TARGET_DISTANCE - if anchored { 24 } else { 16 }
    }

    fn find_pos<W: AiWorld>(context: &mut AiContext<'_, W>) -> Option<Vec3> {
        let direction = match context.body.bee.hive_pos {
            Some(hive)
                if is_hive_valid(context)
                    && !closer_than(context, hive, Self::wander_threshold(context)) =>
            {
                let center = hive.center();
                let toward = Vec3::new(
                    center.x - context.mob.position.x,
                    center.y - context.mob.position.y,
                    center.z - context.mob.position.z,
                );
                toward.normalize()
            }
            _ => mth::view_vector(context.mob.pitch, context.mob.head_yaw),
        };
        let direction = (direction.x, direction.z);
        random_pos::hover_pos(
            context,
            WANDER_HORIZONTAL,
            WANDER_HOVER_VERTICAL,
            direction,
            HALF_PI,
            WANDER_HOVER_MAX,
            WANDER_HOVER_MIN,
        )
        .or_else(|| {
            random_pos::air_and_water_pos(
                context,
                WANDER_HORIZONTAL,
                WANDER_AIR_VERTICAL,
                WANDER_AIR_HEIGHT,
                direction,
                HALF_PI,
            )
        })
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for WanderGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.brain.navigation.is_done() && context.brain.random.next_int(WANDER_ODDS) == 0
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.brain.navigation.is_in_progress()
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        if let Some(position) = Self::find_pos(context) {
            let path = context.create_path_to(position.block(), 1);
            context.move_along(path, 1.0);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HurtByOtherGoal {
    inner: HurtByTargetGoal,
}

impl HurtByOtherGoal {
    pub const fn new() -> Self {
        Self {
            inner: HurtByTargetGoal::bee(),
        }
    }
}

impl Default for HurtByOtherGoal {
    fn default() -> Self {
        Self::new()
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for HurtByOtherGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Target)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.inner.can_use(context)
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.mob.angry && self.inner.memory().can_continue_to_use(context)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.inner.start(context);
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        self.inner.memory().stop(context);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BecomeAngryTargetGoal {
    inner: NearestAttackableTargetGoal,
}

impl BecomeAngryTargetGoal {
    pub fn new() -> Self {
        Self {
            inner: NearestAttackableTargetGoal::with(
                crate::world::TargetKind::Player,
                ANGER_TARGET_INTERVAL,
                true,
                crate::goals::TargetFilter::AngryAt,
            ),
        }
    }
}

impl Default for BecomeAngryTargetGoal {
    fn default() -> Self {
        Self::new()
    }
}

fn bee_can_target<W: AiWorld>(context: &AiContext<'_, W>) -> bool {
    context.mob.angry && !context.body.bee.has_stung
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for BecomeAngryTargetGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Target)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        bee_can_target(context) && self.inner.can_use(context)
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if bee_can_target(context) && context.brain.target.is_some() {
            return self.inner.can_continue_to_use(context);
        }
        false
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.inner.start(context);
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        self.inner.stop(context);
    }
}

pub(crate) fn register(add: &mut Goals<'_>) {
    add.goal(0, MobGoal::Air(AirGoal::BeeAttack(AttackGoal::new())));
    add.goal(1, MobGoal::Air(AirGoal::BeeEnterHive(EnterHiveGoal)));
    add.goal(2, MobGoal::Breed(BreedGoal::new(1.0)));
    add.goal(
        3,
        MobGoal::Tempt(TemptGoal::new(TEMPT_SPEED, TemptItems::Tag(BEE_FOOD))),
    );
    let hive = ValidateHiveGoal::new(&mut add.brain.random);
    add.goal(3, MobGoal::Air(AirGoal::BeeValidateHive(hive)));
    let flower = ValidateFlowerGoal::new(&mut add.brain.random);
    add.goal(3, MobGoal::Air(AirGoal::BeeValidateFlower(flower)));
    add.goal(4, MobGoal::Air(AirGoal::BeePollinate(PollinateGoal::new())));
    add.goal(
        5,
        MobGoal::FollowParent(FollowParentGoal::new(FOLLOW_PARENT_SPEED)),
    );
    add.goal(5, MobGoal::Air(AirGoal::BeeLocateHive(LocateHiveGoal)));
    add.goal(5, MobGoal::Air(AirGoal::BeeGoToHive(GoToHiveGoal::new())));
    add.goal(
        6,
        MobGoal::Air(AirGoal::BeeGoToKnownFlower(GoToKnownFlowerGoal::new())),
    );
    add.goal(7, MobGoal::Air(AirGoal::BeeGrowCrop(GrowCropGoal)));
    add.goal(8, MobGoal::Air(AirGoal::BeeWander(WanderGoal)));
    add.goal(9, MobGoal::Float(FloatGoal));
    add.target_goal(1, MobGoal::Air(AirGoal::BeeHurtBy(HurtByOtherGoal::new())));
    add.target_goal(
        2,
        MobGoal::Air(AirGoal::BeeBecomeAngry(BecomeAngryTargetGoal::new())),
    );
    add.target_goal(
        3,
        MobGoal::ResetUniversalAnger(ResetUniversalAngerGoal::new(true)),
    );
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::mock::{Harness, Mock, living, seed_where};
    use crate::random::MobRandom;

    const HIVE: BlockPos = BlockPos::new(3, 70, 0);
    const FLOWER: BlockPos = BlockPos::new(2, 70, 2);

    fn harness_with(world: Mock, seed: i64) -> Harness {
        Harness::new(EntityKind::Bee, Vec3::new(0.5, 70.0, 0.5), world, seed)
    }

    fn new_bee() -> Harness {
        harness_with(Mock::default(), 1)
    }

    fn hive_world(full: bool, fire: bool) -> Mock {
        let mut world = Mock::default();
        world
            .blocks
            .insert(HIVE, BlockKind::Beehive.default_state());
        world.hives.insert(
            HIVE,
            HiveView {
                full,
                fire_nearby: fire,
            },
        );
        world
    }

    fn flags<G: Goal<AiContext<'static, Mock>>>(goal: &G) -> Flags {
        goal.flags()
    }

    #[test]
    fn the_attack_goal_needs_an_angry_bee_that_has_not_stung() {
        let mut goal = AttackGoal::new();
        let mut harness = new_bee();
        harness.mob.angry = true;
        assert!(!harness.run(|context| goal.can_use(context)));
        harness
            .world
            .others
            .push(living(2, EntityKind::Player, Vec3::new(1.5, 70.0, 0.5)));
        harness.brain.target = Some(rspigot_domain::EntityId(2));
        harness
            .world
            .players
            .push(living(2, EntityKind::Player, Vec3::new(1.5, 70.0, 0.5)));
        harness.world.others.clear();
        let mut goal = AttackGoal::new();
        assert!(harness.run(|context| goal.can_use(context)));
        harness.body.bee.has_stung = true;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.body.bee.has_stung = false;
        harness.mob.angry = false;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        assert_eq!(flags(&goal), Flags::of(Flag::Move).with(Flag::Look));
        assert!(Goal::<AiContext<'static, Mock>>::requires_update_every_tick(&goal));
    }

    #[test]
    fn an_attractive_flower_is_dry_and_a_sunflower_counts_by_its_upper_half() {
        let dandelion = BlockKind::Dandelion.default_state();
        assert!(attracts_bees(Some(dandelion)));
        assert!(!attracts_bees(Some(BlockKind::Stone.default_state())));
        assert!(!attracts_bees(None));
        let sunflower = BlockKind::Sunflower.default_state();
        let upper = sunflower.with_property("half", "upper").unwrap();
        let lower = sunflower.with_property("half", "lower").unwrap();
        assert!(attracts_bees(Some(upper)));
        assert!(!attracts_bees(Some(lower)));
        let wet = BlockKind::FloweringAzalea
            .default_state()
            .with_property("waterlogged", "true");
        if let Some(wet) = wet {
            assert!(!attracts_bees(Some(wet)));
        }
    }

    #[test]
    fn a_bee_wants_to_enter_with_nectar_at_night_or_when_tired_and_never_in_the_excluded_cases() {
        let mut harness = harness_with(hive_world(false, false), 1);
        harness.body.bee.hive_pos = Some(HIVE);
        assert!(!harness.run(|context| wants_to_enter_hive(context)));
        harness.body.bee.has_nectar = true;
        assert!(harness.run(|context| wants_to_enter_hive(context)));
        harness.body.bee.has_nectar = false;
        harness.world.stay_in_hive = true;
        assert!(harness.run(|context| wants_to_enter_hive(context)));
        harness.world.stay_in_hive = false;
        harness.body.bee.ticks_without_nectar = 3601;
        assert!(harness.run(|context| wants_to_enter_hive(context)));
        harness.body.bee.stay_out_countdown = 1;
        assert!(!harness.run(|context| wants_to_enter_hive(context)));
        harness.body.bee.stay_out_countdown = 0;
        harness.body.bee.has_stung = true;
        assert!(!harness.run(|context| wants_to_enter_hive(context)));
        harness.body.bee.has_stung = false;
        harness.brain.bee.pollinating = true;
        assert!(!harness.run(|context| wants_to_enter_hive(context)));
        harness.brain.bee.pollinating = false;
        harness.brain.target = Some(rspigot_domain::EntityId(9));
        assert!(!harness.run(|context| wants_to_enter_hive(context)));
    }

    #[test]
    fn a_hive_near_fire_keeps_the_bee_out() {
        let mut harness = harness_with(hive_world(false, true), 1);
        harness.body.bee.hive_pos = Some(HIVE);
        harness.body.bee.has_nectar = true;
        assert!(!harness.run(|context| wants_to_enter_hive(context)));
    }

    #[test]
    fn a_hive_beyond_forty_eight_blocks_is_not_a_valid_hive() {
        let far = BlockPos::new(60, 70, 0);
        let mut world = Mock::default();
        world.hives.insert(
            far,
            HiveView {
                full: false,
                fire_nearby: false,
            },
        );
        let mut harness = harness_with(world, 1);
        harness.body.bee.hive_pos = Some(far);
        assert!(!harness.run(|context| is_hive_valid(context)));
        harness.body.bee.hive_pos = Some(BlockPos::new(47, 70, 0));
        harness.world.hives.insert(
            BlockPos::new(47, 70, 0),
            HiveView {
                full: false,
                fire_nearby: false,
            },
        );
        assert!(harness.run(|context| is_hive_valid(context)));
    }

    #[test]
    fn entering_needs_a_hive_with_room_within_two_blocks_and_a_full_hive_is_forgotten() {
        let mut goal = EnterHiveGoal;
        let mut harness = harness_with(hive_world(false, false), 1);
        harness.body.bee.has_nectar = true;
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.body.bee.hive_pos = Some(HIVE);
        harness.mob.position = Vec3::new(0.5, 70.0, 0.5);
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.mob.position = Vec3::new(2.5, 70.0, 0.5);
        assert!(harness.run(|context| goal.can_use(context)));
        harness.run(|context| goal.start(context));
        assert!(matches!(
            harness.brain.actions.last(),
            Some(MobAction::Air(AirAction::Bee(BeeAction::EnterHive(pos)))) if *pos == HIVE
        ));
        let mut full = harness_with(hive_world(true, false), 1);
        full.body.bee.has_nectar = true;
        full.body.bee.hive_pos = Some(HIVE);
        full.mob.position = Vec3::new(2.5, 70.0, 0.5);
        assert!(!full.run(|context| goal.can_use(context)));
        assert_eq!(full.body.bee.hive_pos, None);
        assert!(!full.run(|context| goal.can_continue_to_use(context)));
        assert_eq!(flags(&goal), Flags::NONE);
    }

    #[test]
    fn an_angry_bee_never_enters_a_hive() {
        let mut goal = EnterHiveGoal;
        let mut harness = harness_with(hive_world(false, false), 1);
        harness.body.bee.has_nectar = true;
        harness.body.bee.hive_pos = Some(HIVE);
        harness.mob.position = Vec3::new(2.5, 70.0, 0.5);
        harness.mob.angry = true;
        assert!(!harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn the_hive_validation_waits_its_cooldown_then_forgets_a_hive_that_is_gone() {
        let mut random = MobRandom::new(3);
        let mut goal = ValidateHiveGoal::new(&mut random);
        let mut harness = new_bee();
        harness.body.bee.hive_pos = Some(HIVE);
        assert!(harness.run(|context| goal.can_use(context)));
        harness.run(|context| goal.start(context));
        assert_eq!(harness.body.bee.hive_pos, None);
        assert_eq!(harness.body.bee.locate_hive_cooldown, 200);
        assert!(!harness.run(|context| goal.can_use(context)));
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        let mut kept = harness_with(hive_world(false, false), 1);
        kept.body.bee.hive_pos = Some(HIVE);
        let mut again = ValidateHiveGoal::new(&mut random);
        kept.run(|context| again.start(context));
        assert_eq!(kept.body.bee.hive_pos, Some(HIVE));
    }

    #[test]
    fn the_flower_validation_forgets_a_block_that_stopped_being_a_flower() {
        let mut random = MobRandom::new(3);
        let mut goal = ValidateFlowerGoal::new(&mut random);
        let mut harness = new_bee();
        harness.body.bee.flower_pos = Some(FLOWER);
        harness.run(|context| goal.start(context));
        assert_eq!(harness.body.bee.flower_pos, None);
        assert!((20..=60).contains(&harness.body.bee.locate_flower_cooldown));
        let mut kept = new_bee();
        kept.world
            .blocks
            .insert(FLOWER, BlockKind::Poppy.default_state());
        kept.body.bee.flower_pos = Some(FLOWER);
        let mut again = ValidateFlowerGoal::new(&mut random);
        kept.run(|context| again.start(context));
        assert_eq!(kept.body.bee.flower_pos, Some(FLOWER));
    }

    fn poi(pos: BlockPos, kind: rspigot_domain::poi::PoiType) -> rspigot_domain::poi::PoiRecord {
        rspigot_domain::poi::PoiRecord::new(pos, kind)
    }

    #[test]
    fn the_locate_goal_waits_for_the_cooldown_and_a_reason_to_go_home() {
        let mut goal = LocateHiveGoal;
        let mut harness = new_bee();
        harness.body.bee.has_nectar = true;
        assert!(harness.run(|context| goal.can_use(context)));
        harness.body.bee.locate_hive_cooldown = 5;
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.body.bee.locate_hive_cooldown = 0;
        harness.body.bee.hive_pos = Some(HIVE);
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.body.bee.hive_pos = None;
        harness.body.bee.has_nectar = false;
        assert!(!harness.run(|context| goal.can_use(context)));
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
    }

    #[test]
    fn the_locate_goal_picks_the_nearest_hive_with_room_that_is_not_blacklisted() {
        use rspigot_domain::poi::PoiType;
        let near = BlockPos::new(2, 70, 0);
        let mid = BlockPos::new(5, 70, 0);
        let far = BlockPos::new(9, 70, 0);
        let full = BlockPos::new(1, 70, 0);
        let mut world = Mock::default();
        for pos in [near, mid, far, full] {
            world.hives.insert(
                pos,
                HiveView {
                    full: pos == full,
                    fire_nearby: false,
                },
            );
        }
        world.pois = vec![
            poi(far, PoiType::Beehive),
            poi(mid, PoiType::BeeNest),
            poi(near, PoiType::Beehive),
            poi(full, PoiType::Beehive),
            poi(BlockPos::new(3, 70, 3), PoiType::Meeting),
        ];
        let mut harness = harness_with(world, 1);
        let mut goal = LocateHiveGoal;
        harness.run(|context| goal.start(context));
        assert_eq!(harness.body.bee.hive_pos, Some(near));
        assert_eq!(harness.body.bee.locate_hive_cooldown, 200);
        harness.body.bee.hive_pos = None;
        harness.brain.bee.blacklist = vec![near];
        harness.run(|context| goal.start(context));
        assert_eq!(harness.body.bee.hive_pos, Some(mid));
        harness.body.bee.hive_pos = None;
        harness.brain.bee.blacklist = vec![near, mid, far];
        harness.run(|context| goal.start(context));
        assert_eq!(harness.body.bee.hive_pos, Some(near));
        assert!(harness.brain.bee.blacklist.is_empty());
    }

    #[test]
    fn the_go_to_hive_goal_has_the_vanilla_conditions() {
        let mut goal = GoToHiveGoal::new();
        let mut harness = harness_with(hive_world(false, false), 1);
        harness.body.bee.has_nectar = true;
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.body.bee.hive_pos = Some(HIVE);
        harness.mob.position = Vec3::new(-3.5, 70.0, 0.5);
        assert!(harness.run(|context| goal.can_use(context)));
        harness.mob.home_radius = 10;
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.mob.home_radius = -1;
        harness.mob.position = Vec3::new(2.5, 70.0, 0.5);
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.mob.position = Vec3::new(-3.5, 70.0, 0.5);
        harness
            .world
            .blocks
            .insert(HIVE, BlockKind::Stone.default_state());
        assert!(!harness.run(|context| goal.can_use(context)));
        assert_eq!(flags(&goal), Flags::of(Flag::Move));
    }

    #[test]
    fn the_go_to_hive_goal_gives_up_after_twelve_hundred_ticks_and_blacklists_the_hive() {
        let mut goal = GoToHiveGoal::new();
        let mut harness = harness_with(hive_world(false, false), 1);
        harness.body.bee.has_nectar = true;
        harness.body.bee.hive_pos = Some(HIVE);
        harness.run(|context| goal.start(context));
        for _ in 0..1200 {
            goal.travelling_ticks += 0;
            harness.run(|context| goal.tick(context));
            if harness.body.bee.hive_pos.is_none() {
                break;
            }
        }
        for _ in 0..1201 {
            if harness.body.bee.hive_pos.is_none() {
                break;
            }
            goal.travelling_ticks = goal.travelling_ticks.max(1200);
            harness.run(|context| goal.tick(context));
        }
        assert_eq!(harness.body.bee.hive_pos, None);
        assert_eq!(harness.brain.bee.blacklist, vec![HIVE]);
        assert_eq!(harness.body.bee.locate_hive_cooldown, 200);
        harness.run(|context| goal.stop(context));
        assert_eq!(goal.travelling_ticks(), 0);
    }

    #[test]
    fn the_blacklist_keeps_the_three_latest_hives() {
        let mut harness = new_bee();
        for x in 0..5 {
            harness.body.bee.hive_pos = Some(BlockPos::new(x, 70, 0));
            harness.run(GoToHiveGoal::drop_and_blacklist);
        }
        assert_eq!(
            harness.brain.bee.blacklist,
            vec![
                BlockPos::new(2, 70, 0),
                BlockPos::new(3, 70, 0),
                BlockPos::new(4, 70, 0)
            ]
        );
    }

    #[test]
    fn the_go_to_flower_goal_waits_for_six_hundred_ticks_without_nectar() {
        let mut goal = GoToKnownFlowerGoal::new();
        let mut harness = new_bee();
        harness.body.bee.flower_pos = Some(BlockPos::new(8, 70, 0));
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.body.bee.ticks_without_nectar = 601;
        assert!(harness.run(|context| goal.can_use(context)));
        harness.body.bee.flower_pos = Some(BlockPos::new(1, 70, 0));
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.body.bee.flower_pos = Some(BlockPos::new(8, 70, 0));
        harness.mob.home_radius = 5;
        assert!(!harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn the_go_to_flower_goal_drops_a_flower_it_cannot_reach_in_twelve_hundred_ticks() {
        let mut goal = GoToKnownFlowerGoal::new();
        let mut harness = new_bee();
        harness.body.bee.flower_pos = Some(BlockPos::new(8, 70, 0));
        harness.body.bee.ticks_without_nectar = 700;
        harness.run(|context| goal.start(context));
        goal.travelling_ticks = 1200;
        harness.run(|context| goal.tick(context));
        assert_eq!(harness.body.bee.flower_pos, None);
        harness
            .brain
            .navigation
            .set_max_visited_nodes_multiplier(0.5);
        harness.run(|context| goal.stop(context));
        assert_eq!(goal.travelling_ticks(), 0);
    }

    #[test]
    fn the_go_to_flower_goal_flies_towards_a_far_flower_with_a_smaller_search() {
        let mut goal = GoToKnownFlowerGoal::new();
        let mut harness = new_bee();
        harness.body.bee.flower_pos = Some(BlockPos::new(10, 70, 0));
        harness.body.bee.ticks_without_nectar = 700;
        harness.run(|context| goal.start(context));
        harness.run(|context| goal.tick(context));
        assert_eq!(goal.travelling_ticks(), 1);
        assert!(harness.brain.navigation.is_in_progress());
    }

    #[test]
    fn pollination_looks_for_a_reachable_flower_when_the_cooldown_is_over() {
        let mut goal = PollinateGoal::new();
        let mut harness = new_bee();
        harness.body.bee.locate_flower_cooldown = 3;
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.body.bee.locate_flower_cooldown = 0;
        assert!(!harness.run(|context| goal.can_use(context)));
        assert!((20..=60).contains(&harness.body.bee.locate_flower_cooldown));
        harness.body.bee.locate_flower_cooldown = 0;
        harness
            .world
            .blocks
            .insert(FLOWER, BlockKind::Dandelion.default_state());
        assert!(harness.run(|context| goal.can_use(context)));
        assert_eq!(harness.body.bee.flower_pos, Some(FLOWER));
        assert!(harness.brain.navigation.is_in_progress());
        harness.body.bee.has_nectar = true;
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.body.bee.has_nectar = false;
        harness.world.raining = true;
        assert!(!harness.run(|context| goal.can_use(context)));
        assert_eq!(flags(&goal), Flags::of(Flag::Move));
        assert!(Goal::<AiContext<'static, Mock>>::requires_update_every_tick(&goal));
    }

    #[test]
    fn pollination_prefers_the_nearest_flower_in_manhattan_order() {
        let mut goal = PollinateGoal::new();
        let mut harness = new_bee();
        harness
            .world
            .blocks
            .insert(BlockPos::new(4, 70, 0), BlockKind::Poppy.default_state());
        harness.world.blocks.insert(
            BlockPos::new(1, 70, 0),
            BlockKind::Dandelion.default_state(),
        );
        assert!(harness.run(|context| goal.can_use(context)));
        assert_eq!(harness.body.bee.flower_pos, Some(BlockPos::new(1, 70, 0)));
    }

    #[test]
    fn an_unreachable_flower_is_remembered_for_six_hundred_ticks() {
        let mut goal = PollinateGoal::new();
        let mut harness = new_bee();
        let wall = BlockKind::Stone.default_state();
        let flower = BlockPos::new(1, 74, 0);
        harness
            .world
            .blocks
            .insert(flower, BlockKind::Dandelion.default_state());
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let pos = flower.offset(dx, dy, dz);
                    if pos != flower {
                        harness.world.blocks.insert(pos, wall);
                    }
                }
            }
        }
        assert!(!harness.run(|context| goal.can_use(context)));
        assert_eq!(goal.unreachable, vec![(flower, 700)]);
        harness.body.bee.locate_flower_cooldown = 0;
        assert!(!harness.run(|context| goal.can_use(context)));
        assert_eq!(goal.unreachable, vec![(flower, 700)]);
    }

    #[test]
    fn pollinating_hovers_above_the_flower_and_gives_nectar_after_four_hundred_good_ticks() {
        let mut goal = PollinateGoal::new();
        let mut harness = new_bee();
        harness.body.bee.flower_pos = Some(FLOWER);
        harness.body.bee.ticks_without_nectar = 900;
        harness.run(|context| goal.start(context));
        assert!(harness.brain.bee.pollinating);
        assert_eq!(harness.body.bee.ticks_without_nectar, 0);
        harness.mob.position = Vec3::new(2.5, 70.6, 2.5);
        for _ in 0..401 {
            harness.run(|context| goal.tick(context));
        }
        assert!(goal.pollinated_long_enough());
        let seed = seed_where(|random| random.next_float() < 0.2);
        let mut stopper = harness_with(Mock::default(), seed);
        stopper.body.bee.flower_pos = Some(FLOWER);
        stopper.brain.bee.pollinating = true;
        goal.last_sound = 0;
        assert!(stopper.run(|context| goal.can_continue_to_use(context)));
        harness.run(|context| goal.stop(context));
        assert!(harness.body.bee.has_nectar);
        assert!(!harness.brain.bee.pollinating);
        assert_eq!(harness.body.bee.locate_flower_cooldown, 200);
    }

    #[test]
    fn pollinating_ends_without_nectar_before_four_hundred_ticks() {
        let mut goal = PollinateGoal::new();
        let mut harness = new_bee();
        harness.body.bee.flower_pos = Some(FLOWER);
        harness.run(|context| goal.start(context));
        harness.mob.position = Vec3::new(2.5, 70.6, 2.5);
        for _ in 0..100 {
            harness.run(|context| goal.tick(context));
        }
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        harness.run(|context| goal.stop(context));
        assert!(!harness.body.bee.has_nectar);
    }

    #[test]
    fn pollinating_stops_when_the_flower_is_gone_or_it_rains() {
        let mut goal = PollinateGoal::new();
        let mut harness = new_bee();
        harness.body.bee.flower_pos = Some(FLOWER);
        harness.run(|context| goal.start(context));
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        harness.world.raining = true;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.world.raining = false;
        harness.body.bee.flower_pos = None;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.body.bee.flower_pos = Some(FLOWER);
        harness.mob.angry = true;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
    }

    #[test]
    fn pollinating_for_over_six_hundred_ticks_drops_the_flower_and_rests() {
        let mut goal = PollinateGoal::new();
        let mut harness = new_bee();
        harness.body.bee.flower_pos = Some(FLOWER);
        harness.run(|context| goal.start(context));
        goal.pollinating_ticks = 600;
        harness.run(|context| goal.tick(context));
        assert_eq!(harness.body.bee.flower_pos, None);
        assert!(!harness.brain.bee.pollinating);
        assert_eq!(harness.body.bee.locate_flower_cooldown, 200);
    }

    #[test]
    fn a_far_bee_flies_to_a_point_above_the_flower_at_a_third_of_its_speed() {
        let mut goal = PollinateGoal::new();
        let mut harness = new_bee();
        harness.body.bee.flower_pos = Some(FLOWER);
        harness.run(|context| goal.start(context));
        harness.run(|context| goal.tick(context));
        assert_eq!(
            goal.hover,
            Some(Vec3::new(2.5, 70.0 + f64::from(0.6f32), 2.5))
        );
        assert!(harness.brain.movement.has_wanted());
    }

    #[test]
    fn growing_crops_needs_nectar_a_valid_hive_and_a_good_roll() {
        let mut goal = GrowCropGoal;
        let seed = seed_where(|random| random.next_float() >= 0.3);
        let mut harness = harness_with(hive_world(false, false), seed);
        harness.body.bee.hive_pos = Some(HIVE);
        assert!(!harness.run(|context| goal.can_use(context)));
        let mut harness = harness_with(hive_world(false, false), seed);
        harness.body.bee.hive_pos = Some(HIVE);
        harness.body.bee.has_nectar = true;
        assert!(harness.run(|context| goal.can_use(context)));
        let mut full = harness_with(hive_world(false, false), seed);
        full.body.bee.hive_pos = Some(HIVE);
        full.body.bee.has_nectar = true;
        full.body.bee.crops_grown = 10;
        assert!(!full.run(|context| goal.can_use(context)));
        let mut homeless = harness_with(Mock::default(), seed);
        homeless.body.bee.has_nectar = true;
        assert!(!homeless.run(|context| goal.can_use(context)));
        let low = seed_where(|random| random.next_float() < 0.3);
        let mut unlucky = harness_with(hive_world(false, false), low);
        unlucky.body.bee.hive_pos = Some(HIVE);
        unlucky.body.bee.has_nectar = true;
        assert!(!unlucky.run(|context| goal.can_use(context)));
    }

    #[test]
    fn growing_crops_advances_a_crop_one_or_two_blocks_below_by_one_stage() {
        let mut goal = GrowCropGoal;
        let seed = seed_where(|random| random.next_int(15) == 0);
        let mut harness = harness_with(Mock::default(), seed);
        let wheat = BlockKind::Wheat.default_state();
        let young = wheat.with_property("age", "3").unwrap();
        harness.world.blocks.insert(BlockPos::new(0, 69, 0), young);
        harness.run(|context| goal.tick(context));
        let expected = wheat.with_property("age", "4").unwrap();
        assert!(matches!(
            harness.brain.actions.last(),
            Some(MobAction::Air(AirAction::Bee(BeeAction::GrowCrop { pos, to })))
                if *pos == BlockPos::new(0, 69, 0) && *to == expected
        ));
    }

    #[test]
    fn grown_crops_stop_at_their_maximum_age() {
        let wheat = BlockKind::Wheat.default_state();
        assert_eq!(
            grown_state(wheat.with_property("age", "7").unwrap(), BlockKind::Wheat),
            None
        );
        let beet = BlockKind::Beetroots.default_state();
        assert_eq!(
            grown_state(
                beet.with_property("age", "2").unwrap(),
                BlockKind::Beetroots
            ),
            beet.with_property("age", "3")
        );
        assert_eq!(
            grown_state(
                beet.with_property("age", "3").unwrap(),
                BlockKind::Beetroots
            ),
            None
        );
        let stem = BlockKind::PumpkinStem.default_state();
        assert_eq!(
            grown_state(
                stem.with_property("age", "6").unwrap(),
                BlockKind::PumpkinStem
            ),
            stem.with_property("age", "7")
        );
        assert_eq!(
            grown_state(
                stem.with_property("age", "7").unwrap(),
                BlockKind::PumpkinStem
            ),
            None
        );
        let berries = BlockKind::SweetBerryBush.default_state();
        assert_eq!(
            grown_state(
                berries.with_property("age", "2").unwrap(),
                BlockKind::SweetBerryBush
            ),
            berries.with_property("age", "3")
        );
        assert_eq!(
            grown_state(
                berries.with_property("age", "3").unwrap(),
                BlockKind::SweetBerryBush
            ),
            None
        );
        let vines = BlockKind::CaveVines.default_state();
        assert_eq!(
            grown_state(
                vines.with_property("berries", "false").unwrap(),
                BlockKind::CaveVines
            ),
            vines.with_property("berries", "true")
        );
        assert_eq!(
            grown_state(
                vines.with_property("berries", "true").unwrap(),
                BlockKind::CaveVines
            ),
            None
        );
        let torch = BlockKind::TorchflowerCrop.default_state();
        assert_eq!(
            grown_state(
                torch.with_property("age", "0").unwrap(),
                BlockKind::TorchflowerCrop
            ),
            torch.with_property("age", "1")
        );
        assert_eq!(
            grown_state(BlockKind::Stone.default_state(), BlockKind::Stone),
            None
        );
    }

    #[test]
    fn the_wander_goal_starts_idle_one_time_in_ten_and_continues_while_moving() {
        let mut goal = WanderGoal;
        let seed = seed_where(|random| random.next_int(10) == 0);
        let mut harness = harness_with(Mock::default(), seed);
        assert!(harness.run(|context| goal.can_use(context)));
        let miss = seed_where(|random| random.next_int(10) != 0);
        let mut unlucky = harness_with(Mock::default(), miss);
        assert!(!unlucky.run(|context| goal.can_use(context)));
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.run(|context| goal.start(context));
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        assert!(!harness.run(|context| goal.can_use(context)));
        assert_eq!(flags(&goal), Flags::of(Flag::Move));
    }

    #[test]
    fn the_wander_threshold_shrinks_when_the_bee_has_a_hive_or_a_flower() {
        let mut harness = new_bee();
        assert_eq!(
            harness.run(|context| WanderGoal::wander_threshold(context)),
            32
        );
        harness.body.bee.flower_pos = Some(FLOWER);
        assert_eq!(
            harness.run(|context| WanderGoal::wander_threshold(context)),
            24
        );
        harness.body.bee.flower_pos = None;
        harness.body.bee.hive_pos = Some(HIVE);
        assert_eq!(
            harness.run(|context| WanderGoal::wander_threshold(context)),
            24
        );
    }

    #[test]
    fn a_far_hive_pulls_the_wander_direction_towards_it() {
        let hive = BlockPos::new(40, 70, 0);
        let mut world = Mock::default();
        world.hives.insert(
            hive,
            HiveView {
                full: false,
                fire_nearby: false,
            },
        );
        let mut harness = harness_with(world, 4);
        harness.body.bee.hive_pos = Some(hive);
        let mut towards = 0;
        for _ in 0..20 {
            if harness
                .run(WanderGoal::find_pos)
                .is_some_and(|position| position.x > 0.5)
            {
                towards += 1;
            }
        }
        assert!(towards >= 10, "{towards}");
    }

    #[test]
    fn the_hurt_by_goal_needs_the_bee_to_stay_angry_to_continue() {
        let mut goal = HurtByOtherGoal::new();
        let mut harness = new_bee();
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.mob.angry = true;
        assert_eq!(flags(&goal), Flags::of(Flag::Target));
    }

    #[test]
    fn a_hurt_bee_alerts_only_the_other_bees_that_see_the_attacker() {
        let attacker = living(2, EntityKind::Player, Vec3::new(3.5, 70.0, 0.5));
        let mut peer = living(3, EntityKind::Bee, Vec3::new(1.5, 70.0, 0.5));
        peer.has_target = false;
        let mut busy = living(4, EntityKind::Bee, Vec3::new(1.5, 71.0, 0.5));
        busy.has_target = true;
        let cow = living(5, EntityKind::Cow, Vec3::new(1.5, 70.0, 1.5));
        let world = Mock {
            players: vec![attacker],
            others: vec![peer, busy, cow],
            ..Mock::default()
        };
        let mut harness = harness_with(world, 1);
        harness.mob.last_hurt_by_mob = Some(rspigot_domain::EntityId(2));
        harness.mob.last_hurt_by_mob_at = 50;
        let mut goal = HurtByOtherGoal::new();
        harness.run(|context| goal.can_use(context));
        harness.run(|context| goal.start(context));
        let alerts: Vec<_> = harness
            .brain
            .actions
            .iter()
            .filter_map(|action| match action {
                MobAction::Air(AirAction::Bee(BeeAction::Alert { mob, target })) => {
                    Some((mob.0, target.0))
                }
                _ => None,
            })
            .collect();
        assert_eq!(alerts, vec![(3, 2)]);
    }

    #[test]
    fn a_hurt_bee_that_cannot_see_the_attacker_alerts_nobody() {
        let attacker = living(2, EntityKind::Player, Vec3::new(3.5, 70.0, 0.5));
        let peer = living(3, EntityKind::Bee, Vec3::new(1.5, 70.0, 0.5));
        let mut world = Mock {
            players: vec![attacker],
            others: vec![peer],
            ..Mock::default()
        };
        world
            .blocks
            .insert(BlockPos::new(1, 70, 0), BlockKind::Stone.default_state());
        world
            .blocks
            .insert(BlockPos::new(2, 70, 0), BlockKind::Stone.default_state());
        let mut harness = harness_with(world, 1);
        harness.mob.last_hurt_by_mob = Some(rspigot_domain::EntityId(2));
        harness.mob.last_hurt_by_mob_at = 50;
        let mut goal = HurtByOtherGoal::new();
        harness.run(|context| goal.can_use(context));
        harness.run(|context| goal.start(context));
        assert!(harness.brain.actions.iter().all(|action| !matches!(
            action,
            MobAction::Air(AirAction::Bee(BeeAction::Alert { .. }))
        )));
    }

    #[test]
    fn the_anger_target_goal_needs_an_angry_bee_that_has_not_stung() {
        let mut goal = BecomeAngryTargetGoal::new();
        let mut harness = new_bee();
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.mob.angry = true;
        harness.body.bee.has_stung = true;
        assert!(!harness.run(|context| goal.can_use(context)));
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.body.bee.has_stung = false;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        assert_eq!(flags(&goal), Flags::of(Flag::Target));
    }

    #[test]
    fn a_far_target_is_approached_with_a_search_that_shrinks_near_the_target() {
        let mut harness = new_bee();
        harness.run(|context| pathfind_randomly_towards(context, BlockPos::new(10, 70, 2)));
        assert!(harness.brain.navigation.is_in_progress());
    }
}
