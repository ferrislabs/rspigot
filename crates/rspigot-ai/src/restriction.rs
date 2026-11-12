use rspigot_domain::Vec3;

use crate::brain::AiContext;
use crate::random_pos::{HALF_PI, pos_towards};
use crate::world::AiWorld;

const HORIZONTAL: i32 = 16;
const VERTICAL: i32 = 7;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MoveTowardsRestrictionGoal {
    speed_modifier: f64,
    wanted: Vec3,
}

impl MoveTowardsRestrictionGoal {
    pub fn new(speed_modifier: f64) -> Self {
        Self {
            speed_modifier,
            wanted: Vec3::ZERO,
        }
    }

    pub fn wanted(&self) -> Vec3 {
        self.wanted
    }

    pub(crate) fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if context.mob.is_within_home(context.mob.block_pos()) {
            return false;
        }
        let home = context.mob.home_pos;
        let towards = Vec3::new(
            f64::from(home.x) + 0.5,
            f64::from(home.y),
            f64::from(home.z) + 0.5,
        );
        let Some(position) = pos_towards(context, HORIZONTAL, VERTICAL, towards, HALF_PI) else {
            return false;
        };
        self.wanted = position;
        true
    }

    pub(crate) fn can_continue_to_use<W: AiWorld>(
        &mut self,
        context: &mut AiContext<'_, W>,
    ) -> bool {
        !context.brain.navigation.is_done()
    }

    pub(crate) fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        context.move_to_pos(self.wanted, self.speed_modifier);
    }
}

#[cfg(test)]
mod tests {
    use rspigot_domain::BlockPos;
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::mock::{Harness, Mock};

    fn blaze_at(position: Vec3) -> Harness {
        Harness::new(EntityKind::Blaze, position, Mock::default(), 3)
    }

    #[test]
    fn a_mob_without_a_home_never_wants_to_go_back() {
        let mut harness = blaze_at(Vec3::new(0.5, 64.0, 0.5));
        let mut goal = MoveTowardsRestrictionGoal::new(1.0);
        assert!(!harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn a_mob_inside_its_home_radius_stays() {
        let mut harness = blaze_at(Vec3::new(0.5, 64.0, 0.5));
        harness.mob.home_pos = BlockPos::new(2, 64, 2);
        harness.mob.home_radius = 10;
        let mut goal = MoveTowardsRestrictionGoal::new(1.0);
        assert!(!harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn a_mob_outside_its_home_picks_a_spot_towards_it_and_walks_there() {
        let mut harness = blaze_at(Vec3::new(40.5, 64.0, 0.5));
        harness.mob.home_pos = BlockPos::new(0, 64, 0);
        harness.mob.home_radius = 5;
        let mut goal = MoveTowardsRestrictionGoal::new(1.0);
        let mut found = false;
        for _ in 0..50 {
            if harness.run(|context| goal.can_use(context)) {
                found = true;
                break;
            }
        }
        assert!(found);
        assert!(goal.wanted().x < 40.5);
        harness.run(|context| goal.start(context));
        assert!(!harness.brain.navigation.is_done());
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
    }
}
