use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;

use crate::animal::{BreedGoal, EatBlockGoal, FollowParentGoal, PanicGoal};
use crate::blocks::{MoveThroughVillageGoal, RemoveTurtleEggGoal};
use crate::brain::Brain;
use crate::combat::{
    HurtByTargetGoal, LeapAtTargetGoal, MeleeAttackGoal, OcelotAttackGoal, SwellGoal,
};
use crate::evade::{AvoidEntityGoal, FleeSunGoal, RestrictSunGoal};
use crate::goal::GoalSelector;
use crate::goals::{
    ClimbOnTopOfPowderSnowGoal, FloatGoal, LookAtPlayerGoal, MobGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, RandomStrollGoal, TargetFilter, TemptGoal, TemptItems,
};
use crate::rabbit::RaidGardenGoal;
use crate::world::TargetKind;

pub const ZOMBIE_AGGRESSIVE_TOWARDS_VILLAGER: bool = true;
pub const WEAPON_PRIORITY: i32 = 4;
pub const BREAK_DOOR_PRIORITY: i32 = 1;
const ZOMBIE_ALERT_IGNORED: &[EntityKind] = &[EntityKind::ZombifiedPiglin];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoalSettings {
    pub zombie_aggressive_towards_villager: bool,
}

impl Default for GoalSettings {
    fn default() -> Self {
        Self {
            zombie_aggressive_towards_villager: ZOMBIE_AGGRESSIVE_TOWARDS_VILLAGER,
        }
    }
}

struct Goals<'a> {
    goals: &'a mut GoalSelector<MobGoal>,
    targets: &'a mut GoalSelector<MobGoal>,
    brain: &'a mut Brain,
}

impl Goals<'_> {
    fn goal(&mut self, priority: i32, goal: MobGoal) {
        self.goals.add_goal(priority, goal);
    }

    fn target_goal(&mut self, priority: i32, goal: MobGoal) {
        self.targets.add_goal(priority, goal);
    }

    fn float(&mut self, priority: i32) {
        self.brain.profile.can_float = true;
        self.goal(priority, MobGoal::Float(FloatGoal));
    }

    fn tempt(&mut self, priority: i32, speed: f64, items: TemptItems) {
        self.goal(priority, MobGoal::Tempt(TemptGoal::new(speed, items)));
    }

    fn stroll(&mut self, priority: i32, speed: f64) {
        self.goal(
            priority,
            MobGoal::Stroll(RandomStrollGoal::water_avoiding(speed)),
        );
    }

    fn stroll_rarely_in_water(&mut self, priority: i32, speed: f64, probability: f32) {
        self.goal(
            priority,
            MobGoal::Stroll(RandomStrollGoal::water_avoiding_with(speed, probability)),
        );
    }

    fn look(&mut self, priority: i32, distance: f32) {
        self.goal(
            priority,
            MobGoal::LookAtPlayer(LookAtPlayerGoal::new(distance)),
        );
    }

    fn look_around(&mut self, priority: i32) {
        self.goal(
            priority,
            MobGoal::RandomLookAround(RandomLookAroundGoal::default()),
        );
    }

    fn target(&mut self, priority: i32, goal: NearestAttackableTargetGoal) {
        self.target_goal(priority, MobGoal::NearestAttackableTarget(goal));
    }

    fn animal(
        &mut self,
        panic: f64,
        breed: i32,
        tempts: &[(i32, f64, TemptItems)],
        parent: (i32, f64),
    ) {
        self.float(0);
        self.goal(1, MobGoal::Panic(PanicGoal::new(panic)));
        self.goal(breed, MobGoal::Breed(BreedGoal::new(1.0)));
        for (priority, speed, items) in tempts {
            self.tempt(*priority, *speed, *items);
        }
        self.goal(
            parent.0,
            MobGoal::FollowParent(FollowParentGoal::new(parent.1)),
        );
    }
}

pub fn register_goals(
    kind: EntityKind,
    goals: &mut GoalSelector<MobGoal>,
    targets: &mut GoalSelector<MobGoal>,
    brain: &mut Brain,
    settings: GoalSettings,
) {
    let mut add = Goals {
        goals,
        targets,
        brain,
    };
    match kind {
        EntityKind::Cow | EntityKind::Mooshroom => {
            add.animal(
                2.0,
                2,
                &[(3, 1.25, TemptItems::Tag("minecraft:cow_food"))],
                (4, 1.25),
            );
            add.stroll(5, 1.0);
            add.look(6, 6.0);
            add.look_around(7);
        }
        EntityKind::Pig => {
            add.animal(
                1.25,
                3,
                &[
                    (4, 1.2, TemptItems::Item(ItemKind::CarrotOnAStick)),
                    (4, 1.2, TemptItems::Tag("minecraft:pig_food")),
                ],
                (5, 1.1),
            );
            add.stroll(6, 1.0);
            add.look(7, 6.0);
            add.look_around(8);
        }
        EntityKind::Sheep => {
            add.animal(
                1.25,
                2,
                &[(3, 1.1, TemptItems::Tag("minecraft:sheep_food"))],
                (4, 1.1),
            );
            add.goal(5, MobGoal::EatBlock(EatBlockGoal::default()));
            add.stroll(6, 1.0);
            add.look(7, 6.0);
            add.look_around(8);
        }
        EntityKind::Chicken => {
            add.animal(
                1.4,
                2,
                &[(3, 1.0, TemptItems::Tag("minecraft:chicken_food"))],
                (4, 1.1),
            );
            add.stroll(5, 1.0);
            add.look(6, 6.0);
            add.look_around(7);
        }
        EntityKind::Zombie | EntityKind::Husk => {
            add.goal(
                4,
                MobGoal::RemoveTurtleEgg(RemoveTurtleEggGoal::new(1.0, 3)),
            );
            add.look(8, 8.0);
            add.look_around(8);
            add.goal(3, MobGoal::MeleeAttack(MeleeAttackGoal::zombie(1.0)));
            add.goal(6, MobGoal::MoveThroughVillage(MoveThroughVillageGoal));
            add.stroll(7, 1.0);
            add.target_goal(
                1,
                MobGoal::HurtByTarget(HurtByTargetGoal::alerting(ZOMBIE_ALERT_IGNORED)),
            );
            add.target(
                2,
                NearestAttackableTargetGoal::new(TargetKind::Player, true),
            );
            if settings.zombie_aggressive_towards_villager {
                add.target(
                    3,
                    NearestAttackableTargetGoal::new(TargetKind::AbstractVillager, false),
                );
            }
            add.target(
                3,
                NearestAttackableTargetGoal::new(TargetKind::IronGolem, true),
            );
            add.target(
                5,
                NearestAttackableTargetGoal::with(
                    TargetKind::Turtle,
                    10,
                    true,
                    TargetFilter::BabyOnLand,
                ),
            );
        }
        EntityKind::Skeleton
        | EntityKind::WitherSkeleton
        | EntityKind::Stray
        | EntityKind::Bogged
        | EntityKind::Parched => {
            add.goal(2, MobGoal::RestrictSun(RestrictSunGoal));
            add.goal(3, MobGoal::FleeSun(FleeSunGoal::new(1.0)));
            add.goal(
                3,
                MobGoal::AvoidEntity(AvoidEntityGoal::new(
                    TargetKind::Class("Wolf"),
                    6.0,
                    1.0,
                    1.2,
                )),
            );
            add.stroll(5, 1.0);
            add.look(6, 8.0);
            add.look_around(6);
            add.target_goal(1, MobGoal::HurtByTarget(HurtByTargetGoal::new()));
            add.target(
                2,
                NearestAttackableTargetGoal::new(TargetKind::Player, true),
            );
            add.target(
                3,
                NearestAttackableTargetGoal::new(TargetKind::IronGolem, true),
            );
            add.target(
                3,
                NearestAttackableTargetGoal::with(
                    TargetKind::Turtle,
                    10,
                    true,
                    TargetFilter::BabyOnLand,
                ),
            );
            if kind == EntityKind::WitherSkeleton {
                add.target(
                    3,
                    NearestAttackableTargetGoal::new(TargetKind::Class("AbstractPiglin"), true),
                );
            }
            add.goal(
                WEAPON_PRIORITY,
                MobGoal::MeleeAttack(MeleeAttackGoal::skeleton(1.2)),
            );
        }
        EntityKind::Creeper => {
            add.float(1);
            add.goal(2, MobGoal::Swell(SwellGoal::default()));
            add.goal(
                3,
                MobGoal::AvoidEntity(AvoidEntityGoal::new(
                    TargetKind::Class("Ocelot"),
                    6.0,
                    1.0,
                    1.2,
                )),
            );
            add.goal(
                3,
                MobGoal::AvoidEntity(AvoidEntityGoal::new(
                    TargetKind::Class("Cat"),
                    6.0,
                    1.0,
                    1.2,
                )),
            );
            add.goal(4, MobGoal::MeleeAttack(MeleeAttackGoal::new(1.0, false)));
            add.stroll(5, 0.8);
            add.look(6, 8.0);
            add.look_around(6);
            add.target(
                1,
                NearestAttackableTargetGoal::new(TargetKind::Player, true),
            );
            add.target_goal(2, MobGoal::HurtByTarget(HurtByTargetGoal::new()));
        }
        EntityKind::Spider | EntityKind::CaveSpider => {
            add.brain.navigation.set_climber(true);
            add.float(1);
            add.goal(
                2,
                MobGoal::AvoidEntity(AvoidEntityGoal::unless_scared(
                    TargetKind::Class("Armadillo"),
                    6.0,
                    1.0,
                    1.2,
                )),
            );
            add.goal(3, MobGoal::LeapAtTarget(LeapAtTargetGoal::new(0.4)));
            add.goal(4, MobGoal::MeleeAttack(MeleeAttackGoal::spider()));
            add.stroll(5, 0.8);
            add.look(6, 8.0);
            add.look_around(6);
            add.target_goal(1, MobGoal::HurtByTarget(HurtByTargetGoal::new()));
            add.target(2, NearestAttackableTargetGoal::spider(TargetKind::Player));
            add.target(
                3,
                NearestAttackableTargetGoal::spider(TargetKind::IronGolem),
            );
        }
        EntityKind::Rabbit => {
            add.float(1);
            add.goal(
                1,
                MobGoal::ClimbOnTopOfPowderSnow(ClimbOnTopOfPowderSnowGoal),
            );
            add.goal(1, MobGoal::Panic(PanicGoal::rabbit(2.2)));
            add.goal(2, MobGoal::Breed(BreedGoal::new(0.8)));
            add.tempt(3, 1.0, TemptItems::Tag("minecraft:rabbit_food"));
            for (kind, distance) in [
                (TargetKind::Player, 8.0),
                (TargetKind::Class("Wolf"), 10.0),
                (TargetKind::Class("Monster"), 4.0),
            ] {
                add.goal(
                    4,
                    MobGoal::AvoidEntity(AvoidEntityGoal::unless_evil(kind, distance, 2.2, 2.2)),
                );
            }
            add.goal(5, MobGoal::RaidGarden(RaidGardenGoal::default()));
            add.stroll(6, 0.6);
            add.look(11, 10.0);
        }
        EntityKind::Ocelot => {
            add.float(1);
            add.goal(
                3,
                MobGoal::Tempt(TemptGoal::scared_by_movement(
                    0.6,
                    TemptItems::Tag("minecraft:ocelot_food"),
                )),
            );
            add.goal(
                4,
                MobGoal::AvoidEntity(AvoidEntityGoal::unless_trusting(
                    TargetKind::Player,
                    16.0,
                    0.8,
                    1.33,
                )),
            );
            add.goal(7, MobGoal::LeapAtTarget(LeapAtTargetGoal::new(0.3)));
            add.goal(8, MobGoal::OcelotAttack(OcelotAttackGoal::default()));
            add.goal(9, MobGoal::Breed(BreedGoal::new(0.8)));
            add.stroll_rarely_in_water(10, 0.8, 1.0e-5);
            add.look(11, 10.0);
            add.target(
                1,
                NearestAttackableTargetGoal::new(TargetKind::Class("Chicken"), false),
            );
            add.target(
                1,
                NearestAttackableTargetGoal::with(
                    TargetKind::Turtle,
                    10,
                    false,
                    TargetFilter::BabyOnLand,
                ),
            );
        }
        _ => {}
    }
}
