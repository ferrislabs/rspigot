use rspigot_domain::poi::{PoiManager, PoiRecord, PoiType};
use rspigot_domain::world::WorldHeight;

use super::*;
use crate::mock::{Harness, Mock, first_draw_zero, living, seed_where};
use crate::random::{MobRandom, RandomSource};

fn here() -> Vec3 {
    Vec3::new(0.5, 64.0, 0.5)
}

fn section_here() -> SectionPos {
    SectionPos::of(here().block())
}

fn golem(world: Mock, seed: i64) -> Harness {
    Harness::new(EntityKind::IronGolem, here(), world, seed)
}

fn occupied(pos: BlockPos) -> PoiRecord {
    let mut manager = PoiManager::new(WorldHeight::OVERWORLD);
    manager.add(pos, PoiType::Meeting);
    manager.take(|_| true, |_, _| true, pos, 1);
    manager.record_at(pos).expect("record")
}

fn untouched(seed: i64, harness: &mut Harness) -> bool {
    let mut fresh = MobRandom::new(seed);
    harness.brain.random.next_int(1_000_000) == fresh.next_int(1_000_000)
}

fn take_actions(harness: &mut Harness) -> Vec<MobAction> {
    std::mem::take(&mut harness.brain.actions)
}

#[test]
fn enemy_is_every_monster_slime_ghast_phantom_dragon_shulker_and_hoglin() {
    for kind in [
        EntityKind::Zombie,
        EntityKind::Skeleton,
        EntityKind::Creeper,
        EntityKind::Spider,
        EntityKind::Zoglin,
        EntityKind::Witch,
        EntityKind::Slime,
        EntityKind::MagmaCube,
        EntityKind::Ghast,
        EntityKind::Phantom,
        EntityKind::EnderDragon,
        EntityKind::Shulker,
        EntityKind::Hoglin,
    ] {
        assert!(is_enemy(kind), "{kind:?}");
    }
    for kind in [
        EntityKind::Cow,
        EntityKind::Wolf,
        EntityKind::Villager,
        EntityKind::IronGolem,
        EntityKind::Player,
        EntityKind::PolarBear,
        EntityKind::Fox,
    ] {
        assert!(!is_enemy(kind), "{kind:?}");
    }
}

#[test]
fn the_poppy_is_only_accepted_by_the_copper_golem() {
    assert!(accepts_gift(EntityKind::CopperGolem));
    assert!(!accepts_gift(EntityKind::Villager));
    assert!(!accepts_gift(EntityKind::Zombie));
}

fn pursuit_with(distance: f64, seed: i64) -> Harness {
    let world = Mock {
        others: vec![living(
            2,
            EntityKind::Zombie,
            Vec3::new(0.5 + distance, 64.0, 0.5),
        )],
        ..Mock::default()
    };
    let mut harness = golem(world, seed);
    harness.brain.target = Some(EntityId(2));
    harness
}

fn pursuit(distance: f64) -> Harness {
    pursuit_with(distance, 5)
}

fn pursuing(distance: f64) -> (Harness, MoveTowardsTargetGoal) {
    (0..500)
        .find_map(|seed| {
            let mut harness = pursuit_with(distance, seed);
            let mut goal = MoveTowardsTargetGoal::new(0.9, 32.0);
            harness
                .run(|context| goal.can_use(context))
                .then_some((harness, goal))
        })
        .expect("a seed finds a position")
}

#[test]
fn moving_towards_the_target_needs_a_target_within_thirty_two_blocks() {
    let mut goal = MoveTowardsTargetGoal::new(0.9, 32.0);
    let mut none = golem(Mock::default(), 5);
    assert!(!none.run(|context| goal.can_use(context)));
    let mut far = pursuit(33.0);
    assert!(!far.run(|context| goal.can_use(context)));
    let (_, found) = pursuing(31.0);
    assert!(found.target.is_some());
    let mut beyond = pursuit(32.1);
    assert!(!beyond.run(|context| goal.can_use(context)));
}

#[test]
fn moving_towards_the_target_paths_to_a_point_in_its_direction() {
    let (mut harness, mut goal) = pursuing(20.0);
    assert!(goal.wanted.x >= 0.5);
    harness.run(|context| goal.start(context));
    assert!(!harness.brain.navigation.is_done());
}

#[test]
fn moving_towards_the_target_continues_while_it_lives_and_stays_close() {
    let (mut harness, mut goal) = pursuing(20.0);
    harness.run(|context| goal.start(context));
    assert!(harness.run(|context| goal.can_continue_to_use(context)));
    harness.world.others[0].position = Vec3::new(40.5, 64.0, 0.5);
    assert!(!harness.run(|context| goal.can_continue_to_use(context)));
    harness.world.others[0].position = Vec3::new(20.5, 64.0, 0.5);
    harness.world.others[0].alive = false;
    assert!(!harness.run(|context| goal.can_continue_to_use(context)));
    goal.stop();
    assert!(goal.target.is_none());
}

fn move_back() -> IronGolemGoal {
    IronGolemGoal::MoveBackToVillage(RandomStrollGoal::with_settings(
        VILLAGE_SPEED,
        MOVE_BACK_INTERVAL,
        false,
    ))
}

#[test]
fn a_golem_inside_a_village_never_walks_back() {
    let seed = seed_where(first_draw_zero(5));
    let mut world = Mock::default();
    world.levels.insert(section_here(), 0);
    let mut harness = golem(world, seed);
    let mut goal = move_back();
    assert!(!harness.run(|context| Goal::can_use(&mut goal, context)));
    assert!(untouched(seed, &mut harness));
}

#[test]
fn a_golem_one_section_from_a_village_is_still_inside() {
    let seed = seed_where(first_draw_zero(5));
    let mut world = Mock::default();
    world.levels.insert(section_here(), 1);
    let mut harness = golem(world, seed);
    let mut goal = move_back();
    assert!(!harness.run(|context| Goal::can_use(&mut goal, context)));
}

#[test]
fn walking_back_is_rolled_one_in_five_goal_ticks() {
    let seed = seed_where(|random| random.next_int(5) != 0);
    let mut world = Mock::default();
    world.levels.insert(section_here(), 3);
    world.levels.insert(section_here().offset(2, 0, 0), 1);
    let mut harness = golem(world, seed);
    let mut goal = move_back();
    assert!(!harness.run(|context| Goal::can_use(&mut goal, context)));
}

#[test]
fn a_golem_far_from_any_village_has_no_section_to_walk_to() {
    let seed = seed_where(first_draw_zero(5));
    let mut harness = golem(Mock::default(), seed);
    let mut goal = move_back();
    assert!(!harness.run(|context| Goal::can_use(&mut goal, context)));
}

#[test]
fn a_golem_outside_a_village_walks_towards_the_closest_village_section() {
    let seed = seed_where(first_draw_zero(5));
    let mut world = Mock::default();
    world.levels.insert(section_here(), 4);
    world.levels.insert(section_here().offset(1, 0, 0), 3);
    world.levels.insert(section_here().offset(2, 0, 0), 2);
    let mut harness = golem(world, seed);
    let mut goal = move_back();
    assert!(harness.run(|context| Goal::can_use(&mut goal, context)));
    harness.run(|context| Goal::start(&mut goal, context));
    let target = harness.brain.navigation.target_pos().expect("a path");
    assert!(target.x >= 0);
    assert!(!harness.brain.navigation.is_done());
}

fn village_stroll() -> IronGolemGoal {
    IronGolemGoal::StrollInVillage(RandomStrollGoal::with_settings(
        VILLAGE_SPEED,
        VILLAGE_STROLL_INTERVAL,
        false,
    ))
}

fn villager_first(random: &mut MobRandom) -> bool {
    random.next_int(120) == 0 && random.next_float() >= 0.3 && random.next_float() < 0.7
}

fn poi_first(random: &mut MobRandom) -> bool {
    random.next_int(120) == 0 && random.next_float() >= 0.3 && random.next_float() >= 0.7
}

fn anywhere(random: &mut MobRandom) -> bool {
    random.next_int(120) == 0 && random.next_float() < 0.3
}

fn east_villager() -> Living {
    living(3, EntityKind::Villager, Vec3::new(20.5, 64.0, 0.5))
}

fn stroll_target(harness: &mut Harness) -> Option<BlockPos> {
    let mut goal = village_stroll();
    if !harness.run(|context| Goal::can_use(&mut goal, context)) {
        return None;
    }
    harness.run(|context| Goal::start(&mut goal, context));
    harness.brain.navigation.target_pos()
}

#[test]
fn the_village_stroll_is_rolled_one_in_one_hundred_twenty_goal_ticks() {
    let seed = seed_where(|random| random.next_int(120) != 0);
    let mut harness = golem(Mock::default(), seed);
    assert_eq!(stroll_target(&mut harness), None);
}

#[test]
fn the_village_stroll_wanders_anywhere_three_times_in_ten() {
    let seed = seed_where(anywhere);
    let mut world = Mock::default();
    world.wanting.push(east_villager());
    let mut harness = golem(world, seed);
    let target = stroll_target(&mut harness).expect("a target");
    assert!((-11..=11).contains(&target.x) && (-11..=11).contains(&target.z));
}

#[test]
fn the_village_stroll_prefers_a_villager_who_wants_a_golem() {
    let seed = seed_where(villager_first);
    let mut world = Mock::default();
    world.wanting.push(east_villager());
    world.levels.insert(section_here(), 0);
    world.pois.push(occupied(BlockPos::new(8, 70, -4)));
    let mut harness = golem(world, seed);
    let target = stroll_target(&mut harness).expect("a target");
    assert!(target.x >= 0);
}

#[test]
fn the_village_stroll_falls_back_to_a_poi_when_no_villager_wants_a_golem() {
    let seed = seed_where(villager_first);
    let mut world = Mock::default();
    world.levels.insert(section_here(), 0);
    world.pois.push(occupied(BlockPos::new(14, 70, 8)));
    let mut harness = golem(world, seed);
    let target = stroll_target(&mut harness).expect("a target");
    assert!(target.x >= 0 && target.z >= 0);
}

#[test]
fn the_village_stroll_prefers_an_occupied_poi_of_a_village_section() {
    let seed = seed_where(poi_first);
    let mut world = Mock::default();
    world.levels.insert(section_here(), 0);
    world.pois.push(occupied(BlockPos::new(14, 70, 8)));
    world
        .wanting
        .push(living(3, EntityKind::Villager, Vec3::new(-20.5, 64.0, 0.5)));
    let mut harness = golem(world, seed);
    let target = stroll_target(&mut harness).expect("a target");
    assert!(target.x >= 0 && target.z >= 0);
}

#[test]
fn the_village_stroll_falls_back_to_the_villager_when_no_poi_is_occupied() {
    let seed = seed_where(poi_first);
    let mut world = Mock::default();
    world.levels.insert(section_here(), 0);
    world
        .pois
        .push(PoiRecord::new(BlockPos::new(14, 70, 8), PoiType::Meeting));
    world.wanting.push(east_villager());
    let mut harness = golem(world, seed);
    let target = stroll_target(&mut harness).expect("a target");
    assert!(target.x >= 0);
}

#[test]
fn the_village_stroll_wanders_when_there_is_nowhere_to_go() {
    let seed = seed_where(villager_first);
    let mut harness = golem(Mock::default(), seed);
    assert!(stroll_target(&mut harness).is_some());
}

#[test]
fn the_village_stroll_only_scans_sections_at_level_zero() {
    let seed = seed_where(poi_first);
    let mut with_poi = Mock::default();
    with_poi.levels.insert(section_here(), 1);
    with_poi.pois.push(occupied(BlockPos::new(14, 70, 8)));
    let mut without = Mock::default();
    without.levels.insert(section_here(), 1);
    let found = stroll_target(&mut golem(with_poi, seed));
    assert!(found.is_some());
    assert_eq!(found, stroll_target(&mut golem(without, seed)));
}

fn offer_seed() -> i64 {
    seed_where(first_draw_zero(OFFER_CHANCE))
}

fn offer_world(kind: EntityKind, distance: f64) -> Mock {
    Mock {
        bright: true,
        others: vec![living(4, kind, Vec3::new(0.5 + distance, 64.0, 0.5))],
        ..Mock::default()
    }
}

#[test]
fn offering_a_flower_needs_daylight_before_it_draws_any_random() {
    let seed = offer_seed();
    let mut world = offer_world(EntityKind::Villager, 3.0);
    world.bright = false;
    let mut harness = golem(world, seed);
    let mut goal = OfferFlowerGoal::new();
    assert!(!harness.run(|context| goal.can_use(context)));
    assert!(untouched(seed, &mut harness));
}

#[test]
fn offering_a_flower_is_rolled_one_in_eight_thousand() {
    let seed = seed_where(|random| random.next_int(OFFER_CHANCE) != 0);
    let mut harness = golem(offer_world(EntityKind::Villager, 3.0), seed);
    let mut goal = OfferFlowerGoal::new();
    assert!(!harness.run(|context| goal.can_use(context)));
}

#[test]
fn offering_a_flower_targets_the_nearest_villager_within_six_blocks() {
    let seed = offer_seed();
    let mut world = offer_world(EntityKind::Villager, 5.0);
    world
        .others
        .push(living(5, EntityKind::Villager, Vec3::new(2.5, 64.0, 0.5)));
    let mut harness = golem(world, seed);
    let mut goal = OfferFlowerGoal::new();
    assert!(harness.run(|context| goal.can_use(context)));
    assert_eq!(goal.entity.map(|entity| entity.id), Some(EntityId(5)));
}

#[test]
fn offering_a_flower_ignores_candidates_beyond_six_blocks_and_other_mobs() {
    let seed = offer_seed();
    let mut far = golem(offer_world(EntityKind::Villager, 8.0), seed);
    let mut goal = OfferFlowerGoal::new();
    assert!(!far.run(|context| goal.can_use(context)));
    let mut cow = golem(offer_world(EntityKind::Cow, 3.0), seed);
    assert!(!cow.run(|context| goal.can_use(context)));
}

#[test]
fn offering_a_flower_runs_for_four_hundred_ticks_and_announces_itself() {
    let seed = offer_seed();
    let mut harness = golem(offer_world(EntityKind::Villager, 3.0), seed);
    let mut goal = OfferFlowerGoal::new();
    assert!(harness.run(|context| goal.can_use(context)));
    harness.run(|context| goal.start(context));
    assert_eq!(goal.tick, 200);
    assert_eq!(
        take_actions(&mut harness),
        vec![MobAction::IronGolem(IronGolemAction::OfferFlower(true))]
    );
    for _ in 0..199 {
        harness.run(|context| goal.tick(context));
        assert!(goal.can_continue_to_use());
    }
    harness.run(|context| goal.tick(context));
    assert!(!goal.can_continue_to_use());
}

#[test]
fn offering_a_flower_stops_without_a_gift_for_a_villager() {
    let seed = offer_seed();
    let mut harness = golem(offer_world(EntityKind::Villager, 3.0), seed);
    let mut goal = OfferFlowerGoal::new();
    assert!(harness.run(|context| goal.can_use(context)));
    goal.tick = 0;
    harness.run(|context| goal.stop(context));
    assert_eq!(
        take_actions(&mut harness),
        vec![MobAction::IronGolem(IronGolemAction::OfferFlower(false))]
    );
    assert!(goal.entity.is_none());
}

#[test]
fn offering_a_flower_to_a_copper_golem_gives_it_a_poppy_only_when_it_finished() {
    let seed = offer_seed();
    let mut harness = golem(offer_world(EntityKind::CopperGolem, 3.0), seed);
    let mut goal = OfferFlowerGoal::new();
    assert!(harness.run(|context| goal.can_use(context)));
    goal.tick = 7;
    harness.run(|context| goal.stop(context));
    assert_eq!(
        take_actions(&mut harness),
        vec![MobAction::IronGolem(IronGolemAction::OfferFlower(false))]
    );
    let mut again = golem(offer_world(EntityKind::CopperGolem, 3.0), seed);
    let mut goal = OfferFlowerGoal::new();
    assert!(again.run(|context| goal.can_use(context)));
    goal.tick = 0;
    again.run(|context| goal.stop(context));
    assert_eq!(
        take_actions(&mut again),
        vec![
            MobAction::IronGolem(IronGolemAction::OfferFlower(false)),
            MobAction::IronGolem(IronGolemAction::GiftPoppy(EntityId(4))),
        ]
    );
}

#[test]
fn the_copper_golem_must_still_be_in_reach_to_receive_the_poppy() {
    let seed = offer_seed();
    let mut harness = golem(offer_world(EntityKind::CopperGolem, 3.0), seed);
    let mut goal = OfferFlowerGoal::new();
    assert!(harness.run(|context| goal.can_use(context)));
    goal.tick = 0;
    harness.world.others[0].bounds =
        rspigot_domain::entity::Aabb::of_size(Vec3::new(30.5, 64.0, 0.5), 0.5, 1.0);
    harness.run(|context| goal.stop(context));
    assert_eq!(
        take_actions(&mut harness),
        vec![MobAction::IronGolem(IronGolemAction::OfferFlower(false))]
    );
}

fn defense() -> (Harness, EntityId, EntityId) {
    let mut world = Mock {
        players: vec![living(7, EntityKind::Player, Vec3::new(3.5, 64.0, 0.5))],
        others: vec![living(8, EntityKind::Villager, Vec3::new(2.5, 64.0, 2.5))],
        ..Mock::default()
    };
    world.reputation.insert((EntityId(8), EntityId(7)), -100);
    (golem(world, 3), EntityId(8), EntityId(7))
}

#[test]
fn a_golem_defends_a_villager_against_a_player_it_hates() {
    let (mut harness, _, player) = defense();
    let mut goal = DefendVillageTargetGoal::new();
    assert!(harness.run(|context| goal.can_use(context)));
    harness.run(|context| goal.start(context));
    assert_eq!(harness.brain.target, Some(player));
    let request = harness
        .brain
        .target_requests
        .last()
        .copied()
        .expect("request");
    assert_eq!(request.reason, TargetReason::DefendVillage);
    assert_eq!(request.to, Some(player));
}

#[test]
fn a_golem_defends_only_at_minus_one_hundred_reputation() {
    let (mut harness, villager, player) = defense();
    harness.world.reputation.insert((villager, player), -99);
    let mut goal = DefendVillageTargetGoal::new();
    assert!(!harness.run(|context| goal.can_use(context)));
}

#[test]
fn a_golem_made_by_a_player_never_defends_against_players() {
    let (mut harness, _, _) = defense();
    harness.mob.player_created = true;
    let mut goal = DefendVillageTargetGoal::new();
    assert!(!harness.run(|context| goal.can_use(context)));
}

#[test]
fn a_golem_does_not_defend_against_creative_or_spectator_players() {
    let (mut harness, _, _) = defense();
    harness.world.players[0].creative = true;
    let mut goal = DefendVillageTargetGoal::new();
    assert!(!harness.run(|context| goal.can_use(context)));
    harness.world.players[0].creative = false;
    harness.world.players[0].spectator = true;
    assert!(!harness.run(|context| goal.can_use(context)));
}

#[test]
fn defending_only_looks_at_villagers_and_players_inside_the_area() {
    let (mut harness, _, _) = defense();
    harness.world.others[0].position = Vec3::new(2.5, 64.0, 14.5);
    harness.world.others[0].bounds =
        rspigot_domain::entity::Aabb::of_size(Vec3::new(2.5, 64.0, 14.5), 0.6, 1.95);
    let mut goal = DefendVillageTargetGoal::new();
    assert!(!harness.run(|context| goal.can_use(context)));
    let (mut harness, _, _) = defense();
    harness.world.players[0].position = Vec3::new(12.5, 64.0, 0.5);
    harness.world.players[0].bounds =
        rspigot_domain::entity::Aabb::of_size(Vec3::new(12.5, 64.0, 0.5), 0.6, 1.8);
    assert!(!harness.run(|context| goal.can_use(context)));
}

#[test]
fn the_last_hateful_player_found_stays_the_potential_target() {
    let (mut harness, villager, player) = defense();
    let mut goal = DefendVillageTargetGoal::new();
    assert!(harness.run(|context| goal.can_use(context)));
    harness.world.reputation.insert((villager, player), 0);
    assert!(harness.run(|context| goal.can_use(context)));
    assert_eq!(goal.potential, Some(player));
}

#[test]
fn defending_keeps_the_target_while_it_stays_valid_and_close() {
    let (mut harness, _, player) = defense();
    let mut goal = IronGolemGoal::DefendVillage(DefendVillageTargetGoal::new());
    assert!(harness.run(|context| Goal::can_use(&mut goal, context)));
    harness.run(|context| Goal::start(&mut goal, context));
    assert!(harness.run(|context| Goal::can_continue_to_use(&mut goal, context)));
    harness.world.players[0].position = Vec3::new(30.5, 64.0, 0.5);
    assert!(!harness.run(|context| Goal::can_continue_to_use(&mut goal, context)));
    harness.run(|context| Goal::stop(&mut goal, context));
    assert_eq!(harness.brain.target, None);
    let _ = player;
}

#[test]
fn goal_flags_follow_the_vanilla_flag_sets() {
    let flags = |goal: &IronGolemGoal| Goal::<AiContext<'_, Mock>>::flags(goal);
    assert_eq!(
        flags(&IronGolemGoal::MoveTowardsTarget(
            MoveTowardsTargetGoal::new(0.9, 32.0)
        )),
        Flags::of(Flag::Move)
    );
    assert_eq!(flags(&move_back()), Flags::of(Flag::Move));
    assert_eq!(flags(&village_stroll()), Flags::of(Flag::Move));
    assert_eq!(
        flags(&IronGolemGoal::OfferFlower(OfferFlowerGoal::new())),
        Flags::of(Flag::Move).with(Flag::Look)
    );
    assert_eq!(
        flags(&IronGolemGoal::DefendVillage(DefendVillageTargetGoal::new())),
        Flags::of(Flag::Target)
    );
}
