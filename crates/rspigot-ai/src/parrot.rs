use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::EntityKind;

use crate::PathType;
use crate::animal::block_tag;
use crate::brain::AiContext;
use crate::goal::{Flag, Flags, Goal, adjusted_tick_delay};
use crate::goals::RandomStrollGoal;
use crate::random::RandomSource;
use crate::random_pos;
use crate::world::{AiWorld, Living, TargetKind, inflate};

pub const WANDER_PROBABILITY: f32 = 0.001;
const LAND_DISTANCE: i32 = 15;
const TREE_RADIUS: i32 = 3;
const TREE_HEIGHT: i32 = 6;
const LOGS: &str = "minecraft:logs";
const LEAVES: &str = "minecraft:leaves";
const FOLLOW_RECALC_TICKS: i32 = 10;
const FOLLOW_LOOK_SPEED: f32 = 10.0;

pub const IMITATIONS: &[(EntityKind, &str)] = &[
    (EntityKind::Blaze, "minecraft:entity.parrot.imitate.blaze"),
    (EntityKind::Bogged, "minecraft:entity.parrot.imitate.bogged"),
    (EntityKind::Breeze, "minecraft:entity.parrot.imitate.breeze"),
    (
        EntityKind::CamelHusk,
        "minecraft:entity.parrot.imitate.camel_husk",
    ),
    (
        EntityKind::CaveSpider,
        "minecraft:entity.parrot.imitate.spider",
    ),
    (
        EntityKind::Creaking,
        "minecraft:entity.parrot.imitate.creaking",
    ),
    (
        EntityKind::Creeper,
        "minecraft:entity.parrot.imitate.creeper",
    ),
    (
        EntityKind::Drowned,
        "minecraft:entity.parrot.imitate.drowned",
    ),
    (
        EntityKind::ElderGuardian,
        "minecraft:entity.parrot.imitate.elder_guardian",
    ),
    (
        EntityKind::EnderDragon,
        "minecraft:entity.parrot.imitate.ender_dragon",
    ),
    (
        EntityKind::Endermite,
        "minecraft:entity.parrot.imitate.endermite",
    ),
    (EntityKind::Evoker, "minecraft:entity.parrot.imitate.evoker"),
    (EntityKind::Ghast, "minecraft:entity.parrot.imitate.ghast"),
    (
        EntityKind::HappyGhast,
        "minecraft:entity.parrot.imitate.ghast",
    ),
    (
        EntityKind::Guardian,
        "minecraft:entity.parrot.imitate.guardian",
    ),
    (EntityKind::Hoglin, "minecraft:entity.parrot.imitate.hoglin"),
    (EntityKind::Husk, "minecraft:entity.parrot.imitate.husk"),
    (
        EntityKind::Illusioner,
        "minecraft:entity.parrot.imitate.illusioner",
    ),
    (
        EntityKind::MagmaCube,
        "minecraft:entity.parrot.imitate.magma_cube",
    ),
    (
        EntityKind::Parched,
        "minecraft:entity.parrot.imitate.parched",
    ),
    (
        EntityKind::Phantom,
        "minecraft:entity.parrot.imitate.phantom",
    ),
    (EntityKind::Piglin, "minecraft:entity.parrot.imitate.piglin"),
    (
        EntityKind::PiglinBrute,
        "minecraft:entity.parrot.imitate.piglin_brute",
    ),
    (
        EntityKind::Pillager,
        "minecraft:entity.parrot.imitate.pillager",
    ),
    (
        EntityKind::Ravager,
        "minecraft:entity.parrot.imitate.ravager",
    ),
    (
        EntityKind::Shulker,
        "minecraft:entity.parrot.imitate.shulker",
    ),
    (
        EntityKind::Silverfish,
        "minecraft:entity.parrot.imitate.silverfish",
    ),
    (
        EntityKind::Skeleton,
        "minecraft:entity.parrot.imitate.skeleton",
    ),
    (EntityKind::Slime, "minecraft:entity.parrot.imitate.slime"),
    (EntityKind::Spider, "minecraft:entity.parrot.imitate.spider"),
    (EntityKind::Stray, "minecraft:entity.parrot.imitate.stray"),
    (EntityKind::Vex, "minecraft:entity.parrot.imitate.vex"),
    (
        EntityKind::Vindicator,
        "minecraft:entity.parrot.imitate.vindicator",
    ),
    (EntityKind::Warden, "minecraft:entity.parrot.imitate.warden"),
    (EntityKind::Witch, "minecraft:entity.parrot.imitate.witch"),
    (EntityKind::Wither, "minecraft:entity.parrot.imitate.wither"),
    (
        EntityKind::WitherSkeleton,
        "minecraft:entity.parrot.imitate.wither_skeleton",
    ),
    (EntityKind::Zoglin, "minecraft:entity.parrot.imitate.zoglin"),
    (EntityKind::Zombie, "minecraft:entity.parrot.imitate.zombie"),
    (
        EntityKind::ZombieHorse,
        "minecraft:entity.parrot.imitate.zombie_horse",
    ),
    (
        EntityKind::ZombieNautilus,
        "minecraft:entity.parrot.imitate.zombie_nautilus",
    ),
    (
        EntityKind::ZombieVillager,
        "minecraft:entity.parrot.imitate.zombie_villager",
    ),
];
pub const AMBIENT_SOUND: &str = "minecraft:entity.parrot.ambient";

pub fn imitated_sound(kind: EntityKind) -> &'static str {
    IMITATIONS
        .iter()
        .find(|(imitated, _)| *imitated == kind)
        .map_or(AMBIENT_SOUND, |(_, sound)| *sound)
}

fn is_empty<W: AiWorld>(world: &W, pos: BlockPos) -> bool {
    world.state(pos).is_none_or(rspigot_blocks::StateId::is_air)
}

fn tree_position<W: AiWorld>(context: &AiContext<'_, W>) -> Option<Vec3> {
    let position = context.mob.position;
    let here = position.block();
    let low = BlockPos::new(
        (position.x - f64::from(TREE_RADIUS)).floor() as i32,
        (position.y - f64::from(TREE_HEIGHT)).floor() as i32,
        (position.z - f64::from(TREE_RADIUS)).floor() as i32,
    );
    let high = BlockPos::new(
        (position.x + f64::from(TREE_RADIUS)).floor() as i32,
        (position.y + f64::from(TREE_HEIGHT)).floor() as i32,
        (position.z + f64::from(TREE_RADIUS)).floor() as i32,
    );
    for z in low.z..=high.z {
        for y in low.y..=high.y {
            for x in low.x..=high.x {
                let pos = BlockPos::new(x, y, z);
                if pos == here {
                    continue;
                }
                let below = context.world.state(pos.offset(0, -1, 0));
                if !(block_tag(below, LEAVES) || block_tag(below, LOGS)) {
                    continue;
                }
                if is_empty(context.world, pos) && is_empty(context.world, pos.offset(0, 1, 0)) {
                    return Some(pos.center());
                }
            }
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WanderGoal {
    stroll: RandomStrollGoal,
}

impl WanderGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            stroll: RandomStrollGoal::water_avoiding_flying(speed_modifier),
        }
    }

    fn position<W: AiWorld>(&self, context: &mut AiContext<'_, W>) -> Option<Vec3> {
        let mut position = None;
        if context.mob.fluid.in_water {
            position = random_pos::land_pos(context, LAND_DISTANCE, LAND_DISTANCE);
        }
        if context.brain.random.next_float() >= WANDER_PROBABILITY {
            position = tree_position(context);
        }
        position.or_else(|| self.stroll.position(context))
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for WanderGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let goal = *self;
        self.stroll
            .can_use_with(context, |context| goal.position(context))
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        self.stroll.can_continue_to_use(context)
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.stroll.start(context);
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        self.stroll.stop(context);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FollowMobGoal {
    speed_modifier: f64,
    stop_distance: f32,
    area_size: f32,
    following: Option<rspigot_domain::EntityId>,
    time_to_recalc_path: i32,
    old_water_cost: f32,
}

impl FollowMobGoal {
    pub fn new(speed_modifier: f64, stop_distance: f32, area_size: f32) -> Self {
        Self {
            speed_modifier,
            stop_distance,
            area_size,
            following: None,
            time_to_recalc_path: 0,
            old_water_cost: 0.0,
        }
    }

    pub fn following(&self) -> Option<rspigot_domain::EntityId> {
        self.following
    }

    fn target<W: AiWorld>(&self, context: &AiContext<'_, W>) -> Option<Living> {
        self.following.and_then(|id| context.world.living(id))
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for FollowMobGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let area = inflate(
            &context.mob.bounds(),
            f64::from(self.area_size),
            f64::from(self.area_size),
            f64::from(self.area_size),
        );
        let kind = context.mob.kind;
        let found = context
            .world
            .living_of(TargetKind::Class("Mob"), &area)
            .into_iter()
            .find(|mob| mob.kind != kind && !mob.invisible);
        self.following = found.map(|mob| mob.id);
        found.is_some()
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        let stop = f64::from(self.stop_distance);
        self.target(context).is_some_and(|target| {
            !context.brain.navigation.is_done()
                && stop * stop < context.mob.distance_to_sqr(target.position)
        })
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.time_to_recalc_path = 0;
        self.old_water_cost = context.brain.profile.malus(PathType::Water);
        context.brain.profile.set_malus(PathType::Water, 0.0);
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        self.following = None;
        context.stop_navigation();
        context
            .brain
            .profile
            .set_malus(PathType::Water, self.old_water_cost);
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let Some(target) = self.target(context) else {
            return;
        };
        if context.mob.leashed {
            return;
        }
        let eye = Vec3::new(target.position.x, target.eye_y(), target.position.z);
        let max = context.mob.max_head_x_rot();
        context.brain.look.set_look_at(eye, FOLLOW_LOOK_SPEED, max);
        self.time_to_recalc_path -= 1;
        if self.time_to_recalc_path > 0 {
            return;
        }
        self.time_to_recalc_path = adjusted_tick_delay(false, FOLLOW_RECALC_TICKS);
        let delta = context.mob.position - target.position;
        let distance = delta.x * delta.x + delta.y * delta.y + delta.z * delta.z;
        let stop = f64::from(self.stop_distance);
        if distance > stop * stop {
            context.move_to_entity(&target, self.speed_modifier);
            return;
        }
        context.stop_navigation();
        if distance <= stop {
            let dx = target.position.x - context.mob.position.x;
            let dz = target.position.z - context.mob.position.z;
            let destination = Vec3::new(
                context.mob.position.x - dx,
                context.mob.position.y,
                context.mob.position.z - dz,
            );
            context.move_to_pos(destination, self.speed_modifier);
        }
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::BlockKind;
    use rspigot_domain::EntityId;

    use super::*;
    use crate::mock::{Harness, Mock, living, seed_where};
    use crate::random::MobRandom;

    fn harness(position: Vec3, others: Vec<Living>, seed: i64) -> Harness {
        let world = Mock {
            others,
            ..Mock::default()
        };
        Harness::new(EntityKind::Parrot, position, world, seed)
    }

    fn cow(id: i32, at: Vec3) -> Living {
        living(id, EntityKind::Cow, at)
    }

    #[test]
    fn every_imitation_is_a_parrot_sound_and_unknown_mobs_fall_back_to_the_ambient_one() {
        assert_eq!(
            imitated_sound(EntityKind::Creeper),
            "minecraft:entity.parrot.imitate.creeper"
        );
        assert_eq!(
            imitated_sound(EntityKind::CaveSpider),
            "minecraft:entity.parrot.imitate.spider"
        );
        assert_eq!(
            imitated_sound(EntityKind::HappyGhast),
            "minecraft:entity.parrot.imitate.ghast"
        );
        assert_eq!(imitated_sound(EntityKind::Cow), AMBIENT_SOUND);
        assert_eq!(IMITATIONS.len(), 42);
        assert!(
            IMITATIONS
                .iter()
                .all(|(_, sound)| sound.starts_with("minecraft:entity.parrot.imitate."))
        );
    }

    #[test]
    fn the_follow_goal_picks_the_first_visible_mob_of_another_kind_in_the_area() {
        let mut goal = FollowMobGoal::new(1.0, 3.0, 7.0);
        let mut empty = harness(Vec3::new(0.5, 70.0, 0.5), Vec::new(), 1);
        assert!(!empty.run(|context| goal.can_use(context)));
        let mut parrot = living(8, EntityKind::Parrot, Vec3::new(2.5, 70.0, 0.5));
        parrot.kind = EntityKind::Parrot;
        let mut hidden = cow(9, Vec3::new(3.5, 70.0, 0.5));
        hidden.invisible = true;
        let mut crowd = harness(
            Vec3::new(0.5, 70.0, 0.5),
            vec![
                parrot,
                hidden,
                cow(10, Vec3::new(4.5, 70.0, 0.5)),
                cow(11, Vec3::new(5.5, 70.0, 0.5)),
            ],
            1,
        );
        assert!(crowd.run(|context| goal.can_use(context)));
        assert_eq!(goal.following(), Some(EntityId(10)));
        let mut far = harness(
            Vec3::new(0.5, 70.0, 0.5),
            vec![cow(12, Vec3::new(9.5, 70.0, 0.5))],
            1,
        );
        assert!(!far.run(|context| goal.can_use(context)));
        assert_eq!(
            Goal::<AiContext<'_, Mock>>::flags(&goal),
            Flags::of(Flag::Move).with(Flag::Look)
        );
    }

    #[test]
    fn the_follow_goal_walks_on_water_costlessly_and_restores_the_cost_when_it_stops() {
        let mut goal = FollowMobGoal::new(1.0, 3.0, 7.0);
        let mut harness = harness(
            Vec3::new(0.5, 70.0, 0.5),
            vec![cow(9, Vec3::new(4.5, 70.0, 0.5))],
            1,
        );
        let before = harness.brain.profile.malus(PathType::Water);
        harness.run(|context| goal.can_use(context));
        harness.run(|context| goal.start(context));
        assert_eq!(harness.brain.profile.malus(PathType::Water), 0.0);
        harness.run(|context| goal.stop(context));
        assert_eq!(harness.brain.profile.malus(PathType::Water), before);
        assert_eq!(goal.following(), None);
    }

    #[test]
    fn the_follow_goal_looks_at_the_mob_and_paths_towards_it_every_ten_ticks() {
        let mut goal = FollowMobGoal::new(1.0, 3.0, 7.0);
        let mut harness = harness(
            Vec3::new(0.5, 70.0, 0.5),
            vec![cow(9, Vec3::new(5.5, 70.0, 0.5))],
            1,
        );
        harness.run(|context| goal.can_use(context));
        harness.run(|context| goal.start(context));
        harness.run(|context| goal.tick(context));
        assert!(harness.brain.look.is_looking_at_target());
        assert!(harness.brain.navigation.is_in_progress());
        let cow = harness.world.others[0];
        assert_eq!(
            harness.brain.look.wanted(),
            Vec3::new(5.5, cow.eye_y(), 0.5)
        );
    }

    #[test]
    fn a_follower_too_close_to_the_mob_steps_back_away_from_it() {
        let mut goal = FollowMobGoal::new(1.0, 3.0, 7.0);
        let mut harness = harness(
            Vec3::new(0.5, 70.0, 0.5),
            vec![cow(9, Vec3::new(1.5, 70.0, 0.5))],
            1,
        );
        harness.run(|context| goal.can_use(context));
        harness.run(|context| goal.start(context));
        harness.run(|context| goal.tick(context));
        let target = harness.brain.navigation.path().map(|path| path.target());
        assert_eq!(target, Some(BlockPos::new(-1, 70, 0)));
    }

    #[test]
    fn the_follow_goal_continues_while_the_path_runs_and_the_mob_is_beyond_the_stop_distance() {
        let mut goal = FollowMobGoal::new(1.0, 3.0, 7.0);
        let mut harness = harness(
            Vec3::new(0.5, 70.0, 0.5),
            vec![cow(9, Vec3::new(6.5, 70.0, 0.5))],
            1,
        );
        harness.run(|context| goal.can_use(context));
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.run(|context| goal.start(context));
        harness.run(|context| goal.tick(context));
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
    }

    #[test]
    fn the_wander_goal_perches_on_the_first_empty_block_above_leaves_or_a_log() {
        let seed = seed_where(|random| {
            random.next_int(reduced_interval()) == 0 && random.next_float() >= WANDER_PROBABILITY
        });
        let mut goal = WanderGoal::new(1.0);
        let mut harness = harness(Vec3::new(0.5, 70.0, 0.5), Vec::new(), seed);
        harness
            .world
            .blocks
            .insert(BlockPos::new(2, 69, 1), BlockKind::OakLog.default_state());
        assert!(harness.run(|context| goal.can_use(context)));
        harness.run(|context| goal.start(context));
        let end = harness.brain.navigation.path().map(|path| path.target());
        assert_eq!(end, Some(BlockPos::new(2, 70, 1)));
    }

    fn reduced_interval() -> i32 {
        crate::goal::reduced_tick_delay(120)
    }

    #[test]
    fn the_tree_search_skips_the_block_the_parrot_stands_in_and_needs_two_free_blocks() {
        let mut harness = harness(Vec3::new(0.5, 70.0, 0.5), Vec::new(), 1);
        harness.world.blocks.insert(
            BlockPos::new(0, 69, 0),
            BlockKind::OakLeaves.default_state(),
        );
        let none = harness.run(|context| tree_position(context));
        assert_eq!(none, None);
        harness.world.blocks.insert(
            BlockPos::new(1, 69, 0),
            BlockKind::OakLeaves.default_state(),
        );
        harness
            .world
            .blocks
            .insert(BlockPos::new(1, 71, 0), BlockKind::Stone.default_state());
        assert_eq!(harness.run(|context| tree_position(context)), None);
        harness
            .world
            .blocks
            .insert(BlockPos::new(1, 71, 0), BlockKind::Air.default_state());
        assert_eq!(
            harness.run(|context| tree_position(context)),
            Some(Vec3::new(1.5, 70.0, 0.5))
        );
        let _ = MobRandom::new(1);
    }
}
