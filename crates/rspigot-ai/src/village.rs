use rspigot_domain::SectionPos;

use crate::world::AiWorld;

pub fn sections_to_village(world: &impl AiWorld, section: SectionPos) -> i32 {
    world.sections_to_village(section.center())
}

pub fn find_section_closest_to_village(
    world: &impl AiWorld,
    center: SectionPos,
    radius: i32,
) -> SectionPos {
    let own = sections_to_village(world, center);
    let mut best: Option<(i32, SectionPos)> = None;
    for section in center.cube(radius) {
        let level = sections_to_village(world, section);
        if level >= own {
            continue;
        }
        if best.is_none_or(|(current, _)| level < current) {
            best = Some((level, section));
        }
    }
    best.map_or(center, |(_, section)| section)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::Mock;

    fn world(levels: &[(SectionPos, i32)]) -> Mock {
        let mut mock = Mock::default();
        mock.levels.extend(levels.iter().copied());
        mock
    }

    #[test]
    fn the_center_is_kept_when_no_section_is_closer() {
        let center = SectionPos::new(0, 4, 0);
        assert_eq!(
            find_section_closest_to_village(&world(&[(center, 3)]), center, 2),
            center
        );
        assert_eq!(
            find_section_closest_to_village(&world(&[]), center, 2),
            center
        );
    }

    #[test]
    fn the_section_with_the_lowest_level_wins() {
        let center = SectionPos::new(0, 4, 0);
        let near = SectionPos::new(1, 4, 0);
        let nearest = SectionPos::new(-2, 5, 2);
        let levels = [(center, 5), (near, 3), (nearest, 1)];
        assert_eq!(
            find_section_closest_to_village(&world(&levels), center, 2),
            nearest
        );
    }

    #[test]
    fn sections_outside_the_radius_are_ignored() {
        let center = SectionPos::new(0, 4, 0);
        let far = SectionPos::new(3, 4, 0);
        assert_eq!(
            find_section_closest_to_village(&world(&[(far, 0)]), center, 2),
            center
        );
    }

    #[test]
    fn ties_keep_the_first_section_in_cube_order() {
        let center = SectionPos::new(0, 4, 0);
        let first = SectionPos::new(-1, 3, -1);
        let later = SectionPos::new(1, 5, 1);
        let levels = [(first, 2), (later, 2)];
        assert_eq!(
            find_section_closest_to_village(&world(&levels), center, 2),
            first
        );
    }

    use rspigot_domain::poi::{PoiManager, PoiType};
    use rspigot_domain::world::WorldHeight;
    use rspigot_domain::{BlockPos, Vec3};
    use rspigot_entities::EntityKind;

    use crate::blocks::MoveThroughVillageGoal;
    use crate::mock::Harness;

    fn occupied(pos: BlockPos, kind: PoiType) -> rspigot_domain::poi::PoiRecord {
        let mut manager = PoiManager::new(WorldHeight::OVERWORLD);
        manager.add(pos, kind);
        manager.take(|_| true, |_, _| true, pos, 1);
        manager.record_at(pos).expect("record")
    }

    fn village_world(poi: BlockPos) -> Mock {
        let mut world = Mock::default();
        let here = SectionPos::of(BlockPos::new(0, 64, 0));
        for section in here.cube(2) {
            world.levels.insert(section, 0);
        }
        world.pois.push(occupied(poi, PoiType::Meeting));
        world
    }

    fn zombie(world: Mock, seed: i64) -> Harness {
        let mut harness = Harness::new(EntityKind::Zombie, Vec3::new(0.5, 64.0, 0.5), world, seed);
        harness.brain.can_break_doors = false;
        harness
    }

    fn ready(poi: BlockPos) -> (Harness, MoveThroughVillageGoal) {
        (0..400)
            .find_map(|seed| {
                let mut harness = zombie(village_world(poi), seed);
                let mut goal = MoveThroughVillageGoal::new(1.0, true, 4);
                harness
                    .run(|context| goal.can_use(context))
                    .then_some((harness, goal))
            })
            .expect("a seed finds a path")
    }

    #[test]
    fn walking_through_a_village_only_happens_at_night() {
        let mut world = village_world(BlockPos::new(6, 64, 0));
        world.bright = true;
        let mut harness = zombie(world, 1);
        let mut goal = MoveThroughVillageGoal::new(1.0, true, 4);
        assert!(!harness.run(|context| goal.can_use(context)));
        let mut day = MoveThroughVillageGoal::new(1.0, false, 4);
        let found = (0..400).any(|seed| {
            let mut world = village_world(BlockPos::new(6, 64, 0));
            world.bright = true;
            zombie(world, seed).run(|context| day.can_use(context))
        });
        assert!(found);
    }

    #[test]
    fn walking_through_a_village_needs_a_village_within_six_sections() {
        let mut harness = zombie(Mock::default(), 1);
        let mut goal = MoveThroughVillageGoal::new(1.0, true, 4);
        assert!(!harness.run(|context| goal.can_use(context)));
        let mut far = Mock::default();
        far.levels.insert(SectionPos::new(7, 4, 0), 0);
        let mut distant = zombie(far, 1);
        assert!(!distant.run(|context| goal.can_use(context)));
    }

    #[test]
    fn walking_through_a_village_needs_an_occupied_village_poi() {
        let mut free = village_world(BlockPos::new(6, 64, 0));
        free.pois = vec![rspigot_domain::poi::PoiRecord::new(
            BlockPos::new(6, 64, 0),
            PoiType::Meeting,
        )];
        let mut goal = MoveThroughVillageGoal::new(1.0, true, 4);
        for seed in 0..100 {
            let mut harness = zombie(
                {
                    let mut world = Mock::default();
                    world.levels.extend(free.levels.clone());
                    world.pois = free.pois.clone();
                    world
                },
                seed,
            );
            assert!(!harness.run(|context| goal.can_use(context)));
        }
        let mut hive = village_world(BlockPos::new(6, 64, 0));
        hive.pois = vec![occupied(BlockPos::new(6, 64, 0), PoiType::Beehive)];
        for seed in 0..100 {
            let mut world = Mock::default();
            world.levels.extend(hive.levels.clone());
            world.pois = hive.pois.clone();
            assert!(!zombie(world, seed).run(|context| goal.can_use(context)));
        }
    }

    #[test]
    fn walking_through_a_village_paths_to_the_poi_and_keeps_doors_openable_afterwards() {
        let (mut harness, mut goal) = ready(BlockPos::new(6, 64, 0));
        assert!(goal.is_ready());
        assert!(harness.brain.profile.can_open_doors);
        harness.run(|context| goal.start(context));
        assert!(!harness.brain.navigation.is_done());
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
    }

    #[test]
    fn walking_through_a_village_stops_when_the_poi_is_within_reach() {
        let (mut harness, mut goal) = ready(BlockPos::new(6, 64, 0));
        harness.run(|context| goal.start(context));
        harness.mob.position = Vec3::new(6.5, 64.0, 0.5);
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.run(|context| goal.stop(context));
        assert_eq!(goal.visited(), &[BlockPos::new(6, 64, 0)]);
    }

    #[test]
    fn walking_through_a_village_does_not_remember_a_poi_it_gave_up_on_far_away() {
        let (mut harness, mut goal) = ready(BlockPos::new(6, 64, 0));
        harness.run(|context| goal.start(context));
        harness.run(|context| goal.stop(context));
        assert!(goal.visited().is_empty());
    }

    #[test]
    fn walking_through_a_village_skips_visited_pois_and_forgets_the_oldest_after_fifteen() {
        let poi = BlockPos::new(6, 64, 0);
        let mut goal = MoveThroughVillageGoal::new(1.0, true, 4);
        goal.mark_visited(poi);
        for seed in 0..100 {
            let mut harness = zombie(village_world(poi), seed);
            assert!(!harness.run(|context| goal.can_use(context)));
        }
        for index in 0..16 {
            goal.mark_visited(BlockPos::new(100 + index, 64, 0));
        }
        let mut harness = zombie(village_world(poi), 1);
        let _ = harness.run(|context| goal.can_use(context));
        assert_eq!(goal.visited().len(), 16);
        assert!(!goal.visited().contains(&poi));
    }
}
