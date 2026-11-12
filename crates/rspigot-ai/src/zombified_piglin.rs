use crate::PathType;
use crate::blocks::RemoveTurtleEggGoal;
use crate::combat::{HurtByTargetGoal, MeleeAttackGoal};
use crate::goals::{MobGoal, NearestAttackableTargetGoal, TargetFilter};
use crate::neutral::ResetUniversalAngerGoal;
use crate::registry::Goals;
use crate::world::TargetKind;

const LAVA_MALUS: f32 = 8.0;
const TARGET_INTERVAL: i32 = 10;

pub(crate) fn register(add: &mut Goals<'_>) {
    add.brain.profile.set_malus(PathType::Lava, LAVA_MALUS);
    add.goal(
        4,
        MobGoal::RemoveTurtleEgg(RemoveTurtleEggGoal::new(1.0, 3)),
    );
    add.look(8, 8.0);
    add.look_around(8);
    add.goal(2, MobGoal::MeleeAttack(MeleeAttackGoal::zombie(1.0)));
    add.stroll(7, 1.0);
    add.target_goal(1, MobGoal::HurtByTarget(HurtByTargetGoal::alerting(&[])));
    add.target(
        2,
        NearestAttackableTargetGoal::with(
            TargetKind::Player,
            TARGET_INTERVAL,
            true,
            TargetFilter::AngryAt,
        ),
    );
    add.target_goal(
        3,
        MobGoal::ResetUniversalAnger(ResetUniversalAngerGoal::new(true)),
    );
}
