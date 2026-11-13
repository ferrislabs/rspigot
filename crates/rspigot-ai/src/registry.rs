use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;

use crate::animal::{BreedGoal, EatBlockGoal, FollowParentGoal, PanicGoal};
use crate::blaze::BlazeAttackGoal;
use crate::blocks::{MoveThroughVillageGoal, RemoveTurtleEggGoal};
use crate::brain::Brain;
use crate::combat::{
    HurtByTargetGoal, LeapAtTargetGoal, MeleeAttackGoal, OcelotAttackGoal, SwellGoal,
};
use crate::control::MoveStyle;
use crate::crossbow::{CrossbowGoal, HoldGroundAttackGoal, RangedCrossbowAttackGoal};
use crate::evade::{AvoidEntityGoal, FleeSunGoal, RestrictSunGoal};
use crate::ghast::{GhastGoal, GhastLookGoal, GhastShootFireballGoal, RandomFloatAroundGoal};
use crate::goal::GoalSelector;
use crate::goals::{
    ClimbOnTopOfPowderSnowGoal, FloatGoal, LookAtPlayerGoal, MobGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, RandomStrollGoal, TargetFilter, TemptGoal, TemptItems,
};
use crate::rabbit::RaidGardenGoal;
use crate::raider::{
    self, NearestAttackableWitchTargetGoal, NearestHealableRaiderTargetGoal, RaiderGoal,
};
use crate::ranged::RangedAttackGoal;
use crate::restriction::MoveTowardsRestrictionGoal;
use crate::shulker::{ShulkerAttackGoal, ShulkerGoal, ShulkerPeekGoal};
use crate::world::TargetKind;
use crate::{
    cat, enderman, fox, iron_golem, panda, polar_bear, silverfish, wolf, zombified_piglin,
};

pub const ZOMBIE_AGGRESSIVE_TOWARDS_VILLAGER: bool = true;
pub const WEAPON_PRIORITY: i32 = 4;
pub const BREAK_DOOR_PRIORITY: i32 = 1;
const SNOW_GOLEM_ATTACK_SPEED: f64 = 1.25;
const SNOW_GOLEM_ATTACK_INTERVAL: i32 = 20;
const SNOW_GOLEM_ATTACK_RADIUS: f32 = 10.0;
const WITCH_ATTACK_SPEED: f64 = 1.0;
const WITCH_ATTACK_INTERVAL: i32 = 60;
const WITCH_ATTACK_RADIUS: f32 = 10.0;
const WITCH_LOOK_DISTANCE: f32 = 8.0;
const WITCH_IGNORED_DAMAGE: &[&str] = &["Raider"];
const PILLAGER_AVOID_DISTANCE: f32 = 8.0;
const PILLAGER_AVOID_WALK_SPEED: f64 = 1.0;
const PILLAGER_AVOID_SPRINT_SPEED: f64 = 1.2;
const PILLAGER_HOLD_GROUND_RADIUS: f32 = 10.0;
const PILLAGER_CROSSBOW_SPEED: f64 = 1.0;
const PILLAGER_CROSSBOW_RADIUS: f32 = 8.0;
const PILLAGER_STROLL_SPEED: f64 = 0.6;
const PILLAGER_LOOK_DISTANCE: f32 = 15.0;
const PILLAGER_IGNORED_DAMAGE: &[&str] = &["Raider"];
const SHULKER_LOOK_DISTANCE: f32 = 8.0;
const SHULKER_LOOK_PROBABILITY: f32 = 0.02;
const SHULKER_IGNORED_DAMAGE: &[&str] = &["Shulker"];
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

pub(crate) struct Goals<'a> {
    pub(crate) goals: &'a mut GoalSelector<MobGoal>,
    pub(crate) targets: &'a mut GoalSelector<MobGoal>,
    pub(crate) brain: &'a mut Brain,
}

impl Goals<'_> {
    pub(crate) fn goal(&mut self, priority: i32, goal: MobGoal) {
        self.goals.add_goal(priority, goal);
    }

    pub(crate) fn target_goal(&mut self, priority: i32, goal: MobGoal) {
        self.targets.add_goal(priority, goal);
    }

    pub(crate) fn float(&mut self, priority: i32) {
        self.brain.profile.can_float = true;
        self.goal(priority, MobGoal::Float(FloatGoal));
    }

    pub(crate) fn tempt(&mut self, priority: i32, speed: f64, items: TemptItems) {
        self.goal(priority, MobGoal::Tempt(TemptGoal::new(speed, items)));
    }

    pub(crate) fn stroll(&mut self, priority: i32, speed: f64) {
        self.goal(
            priority,
            MobGoal::Stroll(RandomStrollGoal::water_avoiding(speed)),
        );
    }

    pub(crate) fn stroll_rarely_in_water(&mut self, priority: i32, speed: f64, probability: f32) {
        self.goal(
            priority,
            MobGoal::Stroll(RandomStrollGoal::water_avoiding_with(speed, probability)),
        );
    }

    pub(crate) fn look(&mut self, priority: i32, distance: f32) {
        self.goal(
            priority,
            MobGoal::LookAtPlayer(LookAtPlayerGoal::new(distance)),
        );
    }

    pub(crate) fn look_around(&mut self, priority: i32) {
        self.goal(
            priority,
            MobGoal::RandomLookAround(RandomLookAroundGoal::default()),
        );
    }

    pub(crate) fn target(&mut self, priority: i32, goal: NearestAttackableTargetGoal) {
        self.target_goal(priority, MobGoal::NearestAttackableTarget(goal));
    }

    pub(crate) fn animal(
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

const TRADER_AVOIDS: [(&str, f32); 7] = [
    ("Zombie", 8.0),
    ("Evoker", 12.0),
    ("Vindicator", 8.0),
    ("Vex", 8.0),
    ("Pillager", 15.0),
    ("Illusioner", 12.0),
    ("Zoglin", 10.0),
];
const TRADER_DISAPPEARED: &str = "minecraft:entity.wandering_trader.disappeared";
const TRADER_REAPPEARED: &str = "minecraft:entity.wandering_trader.reappeared";
const TRADER_AVOID_SPEED: f64 = 0.5;
const TRADER_PANIC_SPEED: f64 = 0.5;
const TRADER_WANDER_SPEED: f64 = 0.35;

pub fn register_brain(
    kind: EntityKind,
    random: &mut crate::random::MobRandom,
) -> Option<crate::behavior::BehaviorBrain<crate::behavior::MobControl>> {
    crate::piglin::brain_for(kind, random).or_else(|| crate::animals::brain_for(kind, random))
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
        EntityKind::Zombie | EntityKind::Husk | EntityKind::ZombieVillager => {
            add.goal(
                4,
                MobGoal::RemoveTurtleEgg(RemoveTurtleEggGoal::new(1.0, 3)),
            );
            add.look(8, 8.0);
            add.look_around(8);
            add.goal(3, MobGoal::MeleeAttack(MeleeAttackGoal::zombie(1.0)));
            add.goal(
                6,
                MobGoal::MoveThroughVillage(MoveThroughVillageGoal::new(1.0, true, 4)),
            );
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
            add.goal(7, MobGoal::LeapAtTarget(LeapAtTargetGoal::new(0.3)));
            add.goal(8, MobGoal::OcelotAttack(OcelotAttackGoal::default()));
            add.goal(9, MobGoal::Breed(BreedGoal::new(0.8)));
            add.stroll_rarely_in_water(10, 0.8, 1.000_000_1e-5);
            add.look(11, 10.0);
            add.goal(
                OCELOT_AVOID_PRIORITY,
                MobGoal::AvoidEntity(ocelot_avoid_players()),
            );
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

        EntityKind::Wolf => wolf::register(&mut add, kind),
        EntityKind::Cat => cat::register(&mut add, kind),
        EntityKind::Fox => fox::register(&mut add, kind),
        EntityKind::Panda => panda::register(&mut add, kind),
        EntityKind::PolarBear => polar_bear::register(&mut add, kind),
        EntityKind::ZombifiedPiglin => zombified_piglin::register(&mut add),
        EntityKind::Enderman => enderman::register(&mut add, kind),
        EntityKind::Endermite => {
            add.float(1);
            add.goal(
                1,
                MobGoal::ClimbOnTopOfPowderSnow(ClimbOnTopOfPowderSnowGoal),
            );
            add.goal(2, MobGoal::MeleeAttack(MeleeAttackGoal::new(1.0, false)));
            add.stroll(3, 1.0);
            add.look(7, 8.0);
            add.look_around(8);
            add.target_goal(1, MobGoal::HurtByTarget(HurtByTargetGoal::alerting(&[])));
            add.target(
                2,
                NearestAttackableTargetGoal::new(TargetKind::Player, true),
            );
        }
        EntityKind::Silverfish => silverfish::register(&mut add, kind),
        EntityKind::WanderingTrader => {
            add.float(0);
            add.goal(
                0,
                MobGoal::Trader(crate::villager::TraderGoal::use_item(
                    crate::villager::TraderItem::InvisibilityPotion,
                    TRADER_DISAPPEARED,
                )),
            );
            add.goal(
                0,
                MobGoal::Trader(crate::villager::TraderGoal::use_item(
                    crate::villager::TraderItem::MilkBucket,
                    TRADER_REAPPEARED,
                )),
            );
            add.goal(
                1,
                MobGoal::Trader(crate::villager::TraderGoal::trade_with_player()),
            );
            for (class, distance) in TRADER_AVOIDS {
                add.goal(
                    1,
                    MobGoal::AvoidEntity(AvoidEntityGoal::new(
                        TargetKind::Class(class),
                        distance,
                        TRADER_AVOID_SPEED,
                        TRADER_AVOID_SPEED,
                    )),
                );
            }
            add.goal(1, MobGoal::Panic(PanicGoal::new(TRADER_PANIC_SPEED)));
            add.goal(
                1,
                MobGoal::Trader(crate::villager::TraderGoal::look_at_trading_player()),
            );
            add.goal(
                2,
                MobGoal::Trader(crate::villager::TraderGoal::wander_to_position(
                    2.0,
                    TRADER_WANDER_SPEED,
                )),
            );
            add.goal(
                4,
                MobGoal::MoveTowardsRestriction(MoveTowardsRestrictionGoal::new(
                    TRADER_WANDER_SPEED,
                )),
            );
            add.stroll(8, TRADER_WANDER_SPEED);
            add.goal(
                9,
                MobGoal::LookAtPlayer(LookAtPlayerGoal::interact(3.0, 1.0)),
            );
            add.goal(10, MobGoal::LookAtPlayer(LookAtPlayerGoal::at_mobs(8.0)));
        }
        EntityKind::IronGolem => iron_golem::register(&mut add, kind),
        EntityKind::SnowGolem => {
            add.goal(
                1,
                MobGoal::RangedAttack(RangedAttackGoal::new(
                    SNOW_GOLEM_ATTACK_SPEED,
                    SNOW_GOLEM_ATTACK_INTERVAL,
                    SNOW_GOLEM_ATTACK_RADIUS,
                )),
            );
            add.stroll_rarely_in_water(2, 1.0, 1.000_000_1e-5);
            add.look(3, 6.0);
            add.look_around(4);
            add.target(
                1,
                NearestAttackableTargetGoal::with(
                    TargetKind::Class("Mob"),
                    10,
                    true,
                    TargetFilter::Enemy,
                ),
            );
        }
        EntityKind::Blaze => {
            add.goal(4, MobGoal::BlazeAttack(BlazeAttackGoal::default()));
            add.goal(
                5,
                MobGoal::MoveTowardsRestriction(MoveTowardsRestrictionGoal::new(1.0)),
            );
            add.stroll_rarely_in_water(7, 1.0, 0.0);
            add.look(8, 8.0);
            add.look_around(8);
            add.target_goal(1, MobGoal::HurtByTarget(HurtByTargetGoal::alerting(&[])));
            add.target(
                2,
                NearestAttackableTargetGoal::new(TargetKind::Player, true),
            );
        }
        EntityKind::Ghast => {
            add.brain.movement.set_style(MoveStyle::Ghast);
            add.goal(
                5,
                MobGoal::Ghast(GhastGoal::RandomFloatAround(RandomFloatAroundGoal::new())),
            );
            add.goal(7, MobGoal::Ghast(GhastGoal::Look(GhastLookGoal)));
            add.goal(
                7,
                MobGoal::Ghast(GhastGoal::ShootFireball(GhastShootFireballGoal::default())),
            );
            add.target(
                1,
                NearestAttackableTargetGoal::with(
                    TargetKind::Player,
                    10,
                    true,
                    TargetFilter::WithinFourBlocksVertically,
                ),
            );
        }
        EntityKind::Shulker => {
            add.goal(
                1,
                MobGoal::LookAtPlayer(LookAtPlayerGoal::horizontal(
                    SHULKER_LOOK_DISTANCE,
                    SHULKER_LOOK_PROBABILITY,
                )),
            );
            add.goal(
                4,
                MobGoal::Shulker(ShulkerGoal::Attack(ShulkerAttackGoal::default())),
            );
            add.goal(
                7,
                MobGoal::Shulker(ShulkerGoal::Peek(ShulkerPeekGoal::default())),
            );
            add.look_around(8);
            add.target_goal(
                1,
                MobGoal::HurtByTarget(HurtByTargetGoal::ignoring_damage_from_alerting(
                    SHULKER_IGNORED_DAMAGE,
                )),
            );
            add.target_goal(
                2,
                MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::shulker_nearest()),
            );
            add.target_goal(
                3,
                MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::shulker_defense()),
            );
        }
        EntityKind::Witch => {
            raider::register(&mut add);
            add.float(1);
            add.goal(
                2,
                MobGoal::RangedAttack(RangedAttackGoal::new(
                    WITCH_ATTACK_SPEED,
                    WITCH_ATTACK_INTERVAL,
                    WITCH_ATTACK_RADIUS,
                )),
            );
            add.stroll(2, 1.0);
            add.look(3, WITCH_LOOK_DISTANCE);
            add.look_around(3);
            add.target_goal(
                1,
                MobGoal::HurtByTarget(HurtByTargetGoal::ignoring_damage_from(WITCH_IGNORED_DAMAGE)),
            );
            add.target_goal(
                2,
                MobGoal::Raider(RaiderGoal::HealRaiders(
                    NearestHealableRaiderTargetGoal::default(),
                )),
            );
            add.target_goal(
                3,
                MobGoal::WitchTarget(NearestAttackableWitchTargetGoal::default()),
            );
        }
        EntityKind::Pillager => {
            raider::register(&mut add);
            add.float(0);
            add.goal(
                1,
                MobGoal::AvoidEntity(AvoidEntityGoal::new(
                    TargetKind::Class("Creaking"),
                    PILLAGER_AVOID_DISTANCE,
                    PILLAGER_AVOID_WALK_SPEED,
                    PILLAGER_AVOID_SPRINT_SPEED,
                )),
            );
            add.goal(
                2,
                MobGoal::Crossbow(CrossbowGoal::HoldGround(HoldGroundAttackGoal::new(
                    PILLAGER_HOLD_GROUND_RADIUS,
                ))),
            );
            add.goal(
                3,
                MobGoal::Crossbow(CrossbowGoal::RangedAttack(RangedCrossbowAttackGoal::new(
                    PILLAGER_CROSSBOW_SPEED,
                    PILLAGER_CROSSBOW_RADIUS,
                ))),
            );
            add.goal(
                8,
                MobGoal::Stroll(RandomStrollGoal::new(PILLAGER_STROLL_SPEED)),
            );
            add.goal(
                9,
                MobGoal::LookAtPlayer(LookAtPlayerGoal::with_probability(
                    PILLAGER_LOOK_DISTANCE,
                    1.0,
                )),
            );
            add.goal(
                10,
                MobGoal::LookAtPlayer(LookAtPlayerGoal::at_mobs(PILLAGER_LOOK_DISTANCE)),
            );
            add.target_goal(
                1,
                MobGoal::HurtByTarget(HurtByTargetGoal::ignoring_damage_from_alerting(
                    PILLAGER_IGNORED_DAMAGE,
                )),
            );
            add.target(
                2,
                NearestAttackableTargetGoal::new(TargetKind::Player, true),
            );
            add.target(
                3,
                NearestAttackableTargetGoal::new(TargetKind::AbstractVillager, false),
            );
            add.target(
                3,
                NearestAttackableTargetGoal::new(TargetKind::IronGolem, true),
            );
        }
        _ => {}
    }
}

pub(crate) const OCELOT_AVOID_PRIORITY: i32 = 4;
const KILLER_MELEE_PRIORITY: i32 = 4;
const KILLER_MELEE_SPEED: f64 = 1.4;

pub(crate) fn ocelot_avoid_players() -> AvoidEntityGoal {
    AvoidEntityGoal::unless_trusting(TargetKind::Player, 16.0, 0.8, 1.33)
}

pub(crate) fn add_killer_bunny_goals(
    goals: &mut GoalSelector<MobGoal>,
    targets: &mut GoalSelector<MobGoal>,
) {
    goals.add_goal(
        KILLER_MELEE_PRIORITY,
        MobGoal::MeleeAttack(MeleeAttackGoal::new(KILLER_MELEE_SPEED, true)),
    );
    targets.add_goal(1, MobGoal::HurtByTarget(HurtByTargetGoal::alerting(&[])));
    targets.add_goal(
        2,
        MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::new(
            TargetKind::Player,
            true,
        )),
    );
    targets.add_goal(
        2,
        MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::new(
            TargetKind::Class("Wolf"),
            true,
        )),
    );
}

#[cfg(test)]
mod tests {
    use rspigot_domain::{EntityId, Vec3};

    use super::*;
    use crate::brain::MobAi;
    use crate::control::MoveStyle;
    use crate::ghast::{GhastGoal, GhastLookGoal, GhastShootFireballGoal, RandomFloatAroundGoal};
    use crate::goal::{Flag, Goal};
    use crate::mock::{Harness, Mock, first_draw_zero, living, seed_where};

    type Registered = Vec<(i32, MobGoal)>;

    fn lists(kind: EntityKind) -> (Registered, Registered) {
        let ai = MobAi::new(kind, 1, GoalSettings::default());
        let collect = |selector: &GoalSelector<MobGoal>| {
            selector
                .goals()
                .iter()
                .map(|wrapped| (wrapped.priority(), wrapped.goal().clone()))
                .collect()
        };
        (collect(ai.goals()), collect(ai.targets()))
    }

    #[test]
    fn the_shulker_registers_the_vanilla_goals_and_targets() {
        let (goals, targets) = lists(EntityKind::Shulker);
        assert_eq!(
            goals,
            vec![
                (
                    1,
                    MobGoal::LookAtPlayer(LookAtPlayerGoal::horizontal(8.0, 0.02))
                ),
                (
                    4,
                    MobGoal::Shulker(ShulkerGoal::Attack(ShulkerAttackGoal::default()))
                ),
                (
                    7,
                    MobGoal::Shulker(ShulkerGoal::Peek(ShulkerPeekGoal::default()))
                ),
                (
                    8,
                    MobGoal::RandomLookAround(RandomLookAroundGoal::default())
                ),
            ]
        );
        assert_eq!(
            targets,
            vec![
                (
                    1,
                    MobGoal::HurtByTarget(HurtByTargetGoal::ignoring_damage_from_alerting(&[
                        "Shulker"
                    ]))
                ),
                (
                    2,
                    MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::shulker_nearest())
                ),
                (
                    3,
                    MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::shulker_defense())
                ),
            ]
        );
    }

    #[test]
    fn the_snow_golem_registers_the_vanilla_goals_and_targets() {
        let (goals, targets) = lists(EntityKind::SnowGolem);
        assert_eq!(
            goals,
            vec![
                (
                    1,
                    MobGoal::RangedAttack(RangedAttackGoal::new(1.25, 20, 10.0))
                ),
                (
                    2,
                    MobGoal::Stroll(RandomStrollGoal::water_avoiding_with(1.0, 1.000_000_1e-5))
                ),
                (3, MobGoal::LookAtPlayer(LookAtPlayerGoal::new(6.0))),
                (
                    4,
                    MobGoal::RandomLookAround(RandomLookAroundGoal::default())
                ),
            ]
        );
        assert_eq!(
            targets,
            vec![(
                1,
                MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::with(
                    TargetKind::Class("Mob"),
                    10,
                    true,
                    TargetFilter::Enemy,
                ))
            )]
        );
    }

    #[test]
    fn the_blaze_registers_the_vanilla_goals_and_targets() {
        let (goals, targets) = lists(EntityKind::Blaze);
        assert_eq!(
            goals,
            vec![
                (4, MobGoal::BlazeAttack(BlazeAttackGoal::default())),
                (
                    5,
                    MobGoal::MoveTowardsRestriction(MoveTowardsRestrictionGoal::new(1.0))
                ),
                (
                    7,
                    MobGoal::Stroll(RandomStrollGoal::water_avoiding_with(1.0, 0.0))
                ),
                (8, MobGoal::LookAtPlayer(LookAtPlayerGoal::new(8.0))),
                (
                    8,
                    MobGoal::RandomLookAround(RandomLookAroundGoal::default())
                ),
            ]
        );
        assert_eq!(
            targets,
            vec![
                (1, MobGoal::HurtByTarget(HurtByTargetGoal::alerting(&[]))),
                (
                    2,
                    MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::new(
                        TargetKind::Player,
                        true,
                    ))
                ),
            ]
        );
    }

    #[test]
    fn the_blaze_avoids_water_and_walks_through_fire_but_pays_for_lava() {
        let ai = MobAi::new(EntityKind::Blaze, 1, GoalSettings::default());
        let profile = &ai.brain().profile;
        assert_eq!(profile.malus(crate::PathType::Water), -1.0);
        assert_eq!(profile.malus(crate::PathType::Lava), 8.0);
        assert_eq!(profile.malus(crate::PathType::Fire), 0.0);
        assert_eq!(profile.malus(crate::PathType::FireInNeighbor), 0.0);
        assert_eq!(ai.brain().movement.style(), MoveStyle::Ground);
    }

    #[test]
    fn the_ghast_registers_the_vanilla_goals_targets_and_flying_control() {
        let (goals, targets) = lists(EntityKind::Ghast);
        assert_eq!(
            goals,
            vec![
                (
                    5,
                    MobGoal::Ghast(GhastGoal::RandomFloatAround(RandomFloatAroundGoal::new()))
                ),
                (7, MobGoal::Ghast(GhastGoal::Look(GhastLookGoal))),
                (
                    7,
                    MobGoal::Ghast(GhastGoal::ShootFireball(GhastShootFireballGoal::default()))
                ),
            ]
        );
        assert_eq!(
            targets,
            vec![(
                1,
                MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::with(
                    TargetKind::Player,
                    10,
                    true,
                    TargetFilter::WithinFourBlocksVertically,
                ))
            )]
        );
        let ai = MobAi::new(EntityKind::Ghast, 1, GoalSettings::default());
        assert_eq!(ai.brain().movement.style(), MoveStyle::Ghast);
    }

    #[test]
    fn the_ghast_only_targets_players_within_four_blocks_vertically() {
        let seed = seed_where(first_draw_zero(10));
        let world = Mock {
            players: vec![
                living(2, EntityKind::Player, Vec3::new(10.5, 85.5, 0.5)),
                living(3, EntityKind::Player, Vec3::new(30.5, 82.0, 0.5)),
            ],
            ..Mock::default()
        };
        let mut harness = Harness::new(EntityKind::Ghast, Vec3::new(0.5, 80.0, 0.5), world, seed);
        let mut goal = NearestAttackableTargetGoal::with(
            TargetKind::Player,
            10,
            true,
            TargetFilter::WithinFourBlocksVertically,
        );
        assert!(harness.run(|context| goal.can_use(context)));
        harness.run(|context| goal.start(context));
        assert_eq!(harness.brain.target_requests.len(), 1);
        assert_eq!(harness.brain.target_requests[0].to, Some(EntityId(3)));
        harness.world.players[0].position.y = 83.0;
        harness.world.players[0].bounds =
            rspigot_domain::entity::Aabb::of_size(harness.world.players[0].position, 0.6, 1.8);
        harness.brain.target_requests.clear();
        harness.brain.random = crate::random::MobRandom::new(seed);
        let mut again = NearestAttackableTargetGoal::with(
            TargetKind::Player,
            10,
            true,
            TargetFilter::WithinFourBlocksVertically,
        );
        assert!(harness.run(|context| again.can_use(context)));
        harness.run(|context| again.start(context));
        assert_eq!(harness.brain.target_requests[0].to, Some(EntityId(2)));
    }

    #[test]
    fn the_ranged_attack_claims_move_and_look() {
        let goal = MobGoal::RangedAttack(RangedAttackGoal::new(1.25, 20, 10.0));
        let flags = Goal::<crate::brain::AiContext<'_, Mock>>::flags(&goal);
        assert!(flags.contains(Flag::Move) && flags.contains(Flag::Look));
        assert!(!flags.contains(Flag::Jump));
        assert!(Goal::<crate::brain::AiContext<'_, Mock>>::requires_update_every_tick(&goal));
    }

    #[test]
    fn the_snow_golem_targets_enemies_and_nothing_else() {
        let seed = seed_where(first_draw_zero(10));
        let world = Mock {
            others: vec![
                living(2, EntityKind::Cow, Vec3::new(3.5, 64.0, 0.5)),
                living(3, EntityKind::Villager, Vec3::new(2.5, 64.0, 0.5)),
                living(4, EntityKind::Zombie, Vec3::new(6.5, 64.0, 0.5)),
                living(5, EntityKind::IronGolem, Vec3::new(2.5, 64.0, 1.5)),
            ],
            ..Mock::default()
        };
        let mut harness = Harness::new(
            EntityKind::SnowGolem,
            Vec3::new(0.5, 64.0, 0.5),
            world,
            seed,
        );
        let mut goal = NearestAttackableTargetGoal::with(
            TargetKind::Class("Mob"),
            10,
            true,
            TargetFilter::Enemy,
        );
        assert!(harness.run(|context| goal.can_use(context)));
        harness.run(|context| goal.start(context));
        assert_eq!(harness.brain.target_requests.len(), 1);
        assert_eq!(harness.brain.target_requests[0].to, Some(EntityId(4)));
    }
}
