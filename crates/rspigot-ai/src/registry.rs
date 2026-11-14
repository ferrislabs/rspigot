use rspigot_entities::EntityKind;
use rspigot_items::ItemKind;

use crate::air::AirGoal;
use crate::animal::{BreedGoal, EatBlockGoal, FollowParentGoal, PanicGoal};
use crate::blaze::BlazeAttackGoal;
use crate::blocks::{MoveThroughVillageGoal, RemoveTurtleEggGoal};
use crate::brain::Brain;
use crate::combat::{
    HurtByTargetGoal, LeapAtTargetGoal, MeleeAttackGoal, OcelotAttackGoal, RangedBowAttackGoal,
    SwellGoal,
};
use crate::control::MoveStyle;
use crate::crossbow::{CrossbowGoal, HoldGroundAttackGoal, RangedCrossbowAttackGoal};
use crate::cube::{
    AttackGoal as CubeAttackGoal, CubeGoal, FloatGoal as CubeFloatGoal,
    KeepOnJumpingGoal as CubeKeepOnJumpingGoal, RandomDirectionGoal as CubeRandomDirectionGoal,
};
use crate::drowned::{
    DrownedAttackGoal, DrownedGoToBeachGoal, DrownedGoToWaterGoal, DrownedGoal, DrownedSwimUpGoal,
    DrownedTridentAttackGoal,
};
use crate::evade::{AvoidEntityGoal, FleeSunGoal, RestrictSunGoal};
use crate::ghast::{GhastGoal, GhastLookGoal, GhastShootFireballGoal, RandomFloatAroundGoal};
use crate::goal::GoalSelector;
use crate::goals::{
    ClimbOnTopOfPowderSnowGoal, FloatGoal, LookAtPlayerGoal, MobGoal, NearestAttackableTargetGoal,
    RandomLookAroundGoal, RandomStrollGoal, TargetFilter, TemptGoal, TemptItems,
};
use crate::guardian::{
    GuardianAttackGoal, GuardianGoal, GuardianRestrictionGoal, GuardianStrollGoal,
};
use crate::rabbit::RaidGardenGoal;
use crate::raider::{
    self, NearestAttackableWitchTargetGoal, NearestHealableRaiderTargetGoal, RaiderGoal,
};
use crate::ranged::RangedAttackGoal;
use crate::restriction::MoveTowardsRestrictionGoal;
use crate::shulker::{ShulkerAttackGoal, ShulkerGoal, ShulkerPeekGoal};
use crate::spell::{CastingGoal, SpellGoal, UseSpellGoal};
use crate::strider::{self, StriderGoToLavaGoal};
use crate::swimmers::{
    BreathAirGoal, DolphinJumpGoal, DolphinMoveToItemGoal, DolphinPlayWithItemsGoal,
    DolphinSwimToTreasureGoal, DolphinSwimWithPlayerGoal, FollowFlockLeaderGoal,
    PufferfishPuffGoal, RandomSwimmingGoal, SquidFleeGoal, SquidRandomMovementGoal,
    TryFindWaterGoal, WaterGoal,
};
use crate::turtle::{
    TurtleBreedGoal, TurtleGoHomeGoal, TurtleGoToWaterGoal, TurtleGoal, TurtleLayEggGoal,
    TurtlePanicGoal, TurtleRandomStrollGoal, TurtleTravelGoal,
};
use crate::world::TargetKind;
use crate::{
    cat, enderman, fox, iron_golem, panda, polar_bear, silverfish, wolf, zombified_piglin,
};
use rspigot_domain::entity::species::spellcaster::IllagerSpell;
use rspigot_domain::entity::species::{drowned, guardian};

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
const VINDICATOR_STROLL_SPEED: f64 = 0.6;
const VINDICATOR_LOOK_PLAYER_DISTANCE: f32 = 3.0;
const VINDICATOR_LOOK_MOB_DISTANCE: f32 = 8.0;
const VINDICATOR_MELEE_SPEED: f64 = 1.0;
const VINDICATOR_HOLD_GROUND_RADIUS: f32 = 10.0;
const SPELLCASTER_UNSEEN_MEMORY: i32 = 300;
const SPELLCASTER_STROLL_SPEED: f64 = 0.6;
const SPELLCASTER_LOOK_PLAYER_DISTANCE: f32 = 3.0;
const SPELLCASTER_LOOK_MOB_DISTANCE: f32 = 8.0;
const EVOKER_AVOID_DISTANCE: f32 = 8.0;
const EVOKER_AVOID_WALK_SPEED: f64 = 0.6;
const EVOKER_AVOID_SPRINT_SPEED: f64 = 1.0;
const ILLUSIONER_BOW_SPEED: f64 = 0.5;
const ILLUSIONER_BOW_INTERVAL: i32 = 20;
const ILLUSIONER_BOW_RADIUS: f32 = 15.0;
const RAVAGER_VILLAGER_INTERVAL: i32 = 10;
const RAVAGER_MELEE_SPEED: f64 = 1.0;
const RAVAGER_STROLL_SPEED: f64 = 0.4;
const RAVAGER_PLAYER_LOOK_DISTANCE: f32 = 6.0;
const RAVAGER_MOB_LOOK_DISTANCE: f32 = 8.0;
const WITHER_ATTACK_SPEED: f64 = 1.0;
const WITHER_ATTACK_INTERVAL: i32 = 40;
const WITHER_ATTACK_RADIUS: f32 = 20.0;
const WITHER_LOOK_DISTANCE: f32 = 8.0;
const PARROT_PANIC_SPEED: f64 = 1.25;
const PARROT_LOOK_DISTANCE: f32 = 8.0;
const PARROT_FOLLOW_START: f32 = 5.0;
const PARROT_FOLLOW_STOP: f32 = 1.0;
const PARROT_FOLLOW_MOB_STOP: f32 = 3.0;
const PARROT_FOLLOW_MOB_AREA: f32 = 7.0;
const VEX_LOOK_DISTANCE: f32 = 3.0;
const VEX_MOB_LOOK_DISTANCE: f32 = 8.0;
const VEX_IGNORED_DAMAGE: &[&str] = &["Raider"];
const SHULKER_LOOK_DISTANCE: f32 = 8.0;
const SHULKER_LOOK_PROBABILITY: f32 = 0.02;
const SHULKER_IGNORED_DAMAGE: &[&str] = &["Shulker"];
const ZOMBIE_ALERT_IGNORED: &[EntityKind] = &[EntityKind::ZombifiedPiglin];
const FISH_PANIC_SPEED: f64 = 1.25;
const FISH_AVOID_DISTANCE: f32 = 8.0;
const FISH_AVOID_WALK_SPEED: f64 = 1.6;
const FISH_AVOID_SPRINT_SPEED: f64 = 1.4;
const DOLPHIN_PLAYER_SPEED: f64 = 4.0;
const DOLPHIN_LOOK_DISTANCE: f32 = 6.0;
const DOLPHIN_JUMP_INTERVAL: i32 = 10;
const DOLPHIN_ATTACK_SPEED: f64 = 1.2_f32 as f64;
const DOLPHIN_AVOID_DISTANCE: f32 = 8.0;
const GUARDIAN_PLAYER_LOOK: f32 = 8.0;
const GUARDIAN_MOB_LOOK: f32 = 12.0;
const GUARDIAN_MOB_LOOK_PROBABILITY: f32 = 0.01;
const GUARDIAN_TARGET_INTERVAL: i32 = 10;
const TURTLE_PANIC_SPEED: f64 = 1.2;
const TURTLE_SPEED: f64 = 1.0;
const TURTLE_TEMPT_SPEED: f64 = 1.1;
const TURTLE_LOOK_DISTANCE: f32 = 8.0;
const DOLPHIN_IGNORED_DAMAGE: &[&str] = &["Guardian"];

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

fn fish_goals(add: &mut Goals<'_>) {
    add.goal(0, MobGoal::Panic(PanicGoal::new(FISH_PANIC_SPEED)));
    add.goal(
        2,
        MobGoal::AvoidEntity(AvoidEntityGoal::no_spectators(
            TargetKind::Player,
            FISH_AVOID_DISTANCE,
            FISH_AVOID_WALK_SPEED,
            FISH_AVOID_SPRINT_SPEED,
        )),
    );
    add.goal(
        4,
        MobGoal::Water(WaterGoal::RandomSwimming(RandomSwimmingGoal::fish())),
    );
}

fn drowned_goals(add: &mut Goals<'_>) {
    add.goal(
        4,
        MobGoal::RemoveTurtleEgg(RemoveTurtleEggGoal::new(1.0, 3)),
    );
    add.look(8, 8.0);
    add.look_around(8);
    add.goal(
        1,
        MobGoal::Drowned(DrownedGoal::GoToWater(DrownedGoToWaterGoal::new(1.0))),
    );
    add.goal(
        2,
        MobGoal::Drowned(DrownedGoal::TridentAttack(DrownedTridentAttackGoal::new(
            1.0,
            drowned::ATTACK_INTERVAL,
            drowned::ATTACK_RADIUS,
        ))),
    );
    add.goal(
        2,
        MobGoal::Drowned(DrownedGoal::Attack(DrownedAttackGoal::new(1.0))),
    );
    add.goal(
        5,
        MobGoal::Drowned(DrownedGoal::GoToBeach(DrownedGoToBeachGoal::new(1.0))),
    );
    add.goal(
        6,
        MobGoal::Drowned(DrownedGoal::SwimUp(DrownedSwimUpGoal::new(1.0))),
    );
    add.goal(7, MobGoal::Stroll(RandomStrollGoal::new(1.0)));
    add.target_goal(
        1,
        MobGoal::HurtByTarget(HurtByTargetGoal::ignoring_damage_from_alerting_except(
            &["Drowned"],
            ZOMBIE_ALERT_IGNORED,
        )),
    );
    add.target(
        2,
        NearestAttackableTargetGoal::with(TargetKind::Player, 10, true, TargetFilter::DrownedOk),
    );
    add.target(
        3,
        NearestAttackableTargetGoal::new(TargetKind::AbstractVillager, false),
    );
    add.target(
        3,
        NearestAttackableTargetGoal::new(TargetKind::IronGolem, true),
    );
    add.target(
        3,
        NearestAttackableTargetGoal::new(TargetKind::Class("Axolotl"), true),
    );
    add.target(
        5,
        NearestAttackableTargetGoal::with(TargetKind::Turtle, 10, true, TargetFilter::BabyOnLand),
    );
}

fn guardian_goals(add: &mut Goals<'_>, kind: EntityKind) {
    let interval = if kind == EntityKind::ElderGuardian {
        guardian::ELDER_STROLL_INTERVAL
    } else {
        guardian::STROLL_INTERVAL
    };
    add.goal(
        4,
        MobGoal::Guardian(GuardianGoal::Attack(GuardianAttackGoal::new())),
    );
    add.goal(
        5,
        MobGoal::Guardian(GuardianGoal::Restriction(GuardianRestrictionGoal::default())),
    );
    add.goal(
        7,
        MobGoal::Guardian(GuardianGoal::Stroll(GuardianStrollGoal::new(interval))),
    );
    add.look(8, GUARDIAN_PLAYER_LOOK);
    add.goal(
        8,
        MobGoal::LookAtPlayer(LookAtPlayerGoal::at_class(
            GUARDIAN_MOB_LOOK,
            GUARDIAN_MOB_LOOK_PROBABILITY,
            "Guardian",
        )),
    );
    add.look_around(9);
    add.target(
        1,
        NearestAttackableTargetGoal::with(
            TargetKind::Class("LivingEntity"),
            GUARDIAN_TARGET_INTERVAL,
            true,
            TargetFilter::GuardianAttack,
        ),
    );
}

fn turtle_goals(add: &mut Goals<'_>) {
    add.goal(
        0,
        MobGoal::Turtle(TurtleGoal::Panic(TurtlePanicGoal::new(TURTLE_PANIC_SPEED))),
    );
    add.goal(
        1,
        MobGoal::Turtle(TurtleGoal::Breed(TurtleBreedGoal::new(TURTLE_SPEED))),
    );
    add.goal(
        1,
        MobGoal::Turtle(TurtleGoal::LayEgg(TurtleLayEggGoal::new(TURTLE_SPEED))),
    );
    add.tempt(
        2,
        TURTLE_TEMPT_SPEED,
        TemptItems::Tag("minecraft:turtle_food"),
    );
    add.goal(
        3,
        MobGoal::Turtle(TurtleGoal::GoToWater(TurtleGoToWaterGoal::new(
            TURTLE_SPEED,
            false,
        ))),
    );
    add.goal(
        4,
        MobGoal::Turtle(TurtleGoal::GoHome(TurtleGoHomeGoal::new(TURTLE_SPEED))),
    );
    add.goal(
        7,
        MobGoal::Turtle(TurtleGoal::Travel(TurtleTravelGoal::new(TURTLE_SPEED))),
    );
    add.look(8, TURTLE_LOOK_DISTANCE);
    add.goal(
        9,
        MobGoal::Turtle(TurtleGoal::RandomStroll(TurtleRandomStrollGoal::default())),
    );
}

fn dolphin_goals(add: &mut Goals<'_>) {
    add.goal(0, MobGoal::Water(WaterGoal::BreathAir(BreathAirGoal)));
    add.goal(0, MobGoal::Water(WaterGoal::TryFindWater(TryFindWaterGoal)));
    add.goal(
        1,
        MobGoal::Water(WaterGoal::DolphinSwimToTreasure(
            DolphinSwimToTreasureGoal::default(),
        )),
    );
    add.goal(
        2,
        MobGoal::Water(WaterGoal::DolphinSwimWithPlayer(
            DolphinSwimWithPlayerGoal::new(DOLPHIN_PLAYER_SPEED),
        )),
    );
    add.goal(
        4,
        MobGoal::Water(WaterGoal::RandomSwimming(RandomSwimmingGoal::dolphin())),
    );
    add.look_around(4);
    add.look(5, DOLPHIN_LOOK_DISTANCE);
    add.goal(
        5,
        MobGoal::Water(WaterGoal::DolphinJump(DolphinJumpGoal::new(
            DOLPHIN_JUMP_INTERVAL,
        ))),
    );
    add.goal(
        6,
        MobGoal::MeleeAttack(MeleeAttackGoal::new(DOLPHIN_ATTACK_SPEED, true)),
    );
    add.goal(
        7,
        MobGoal::Water(WaterGoal::DolphinMoveToItem(
            DolphinMoveToItemGoal::default(),
        )),
    );
    add.goal(
        8,
        MobGoal::Water(WaterGoal::DolphinPlayWithItems(DolphinPlayWithItemsGoal)),
    );
    add.goal(
        9,
        MobGoal::AvoidEntity(AvoidEntityGoal::new(
            TargetKind::Class("Guardian"),
            DOLPHIN_AVOID_DISTANCE,
            1.0,
            1.0,
        )),
    );
    add.target_goal(
        1,
        MobGoal::HurtByTarget(HurtByTargetGoal::ignoring_damage_from_alerting(
            DOLPHIN_IGNORED_DAMAGE,
        )),
    );
}

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
        EntityKind::Horse
        | EntityKind::Donkey
        | EntityKind::Mule
        | EntityKind::SkeletonHorse
        | EntityKind::ZombieHorse => crate::equine::equine_goals(kind, &mut add),
        EntityKind::Llama | EntityKind::TraderLlama => crate::llama::llama_goals(kind, &mut add),
        EntityKind::Strider => {
            add.goal(1, MobGoal::Panic(PanicGoal::new(strider::PANIC_SPEED)));
            add.goal(strider::BREED_PRIORITY, MobGoal::Breed(BreedGoal::new(1.0)));
            add.tempt(
                3,
                strider::TEMPT_SPEED,
                TemptItems::Tag(strider::TEMPT_ITEMS),
            );
            add.goal(
                4,
                MobGoal::StriderGoToLava(StriderGoToLavaGoal::new(strider::GO_TO_LAVA_SPEED)),
            );
            add.goal(5, MobGoal::FollowParent(FollowParentGoal::new(1.0)));
            add.goal(
                7,
                MobGoal::Stroll(RandomStrollGoal::with_interval(
                    1.0,
                    strider::STROLL_INTERVAL,
                )),
            );
            add.look(8, 8.0);
            add.look_around(8);
            add.goal(
                9,
                MobGoal::LookAtPlayer(LookAtPlayerGoal::at_class(8.0, 0.02, "Strider")),
            );
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
        EntityKind::Slime | EntityKind::MagmaCube => {
            add.brain.movement.set_cube();
            add.brain.look.set_max_head_x_rot(0.0);
            add.brain.profile.can_float = true;
            add.goal(1, MobGoal::Cube(CubeGoal::Float(CubeFloatGoal)));
            add.goal(
                4,
                MobGoal::Cube(CubeGoal::RandomDirection(CubeRandomDirectionGoal::default())),
            );
            add.goal(
                5,
                MobGoal::Cube(CubeGoal::KeepOnJumping(CubeKeepOnJumpingGoal)),
            );
            add.goal(
                2,
                MobGoal::Cube(CubeGoal::Attack(CubeAttackGoal::default())),
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
            add.target(
                3,
                NearestAttackableTargetGoal::new(TargetKind::IronGolem, true),
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
        EntityKind::Parrot => {
            add.goal(0, MobGoal::Panic(PanicGoal::new(PARROT_PANIC_SPEED)));
            add.float(0);
            add.look(1, PARROT_LOOK_DISTANCE);
            add.goal(
                2,
                MobGoal::SitWhenOrdered(crate::tame::SitWhenOrderedToGoal),
            );
            add.goal(
                2,
                MobGoal::FollowOwner(crate::tame::FollowOwnerGoal::new(
                    1.0,
                    PARROT_FOLLOW_START,
                    PARROT_FOLLOW_STOP,
                )),
            );
            add.goal(
                2,
                MobGoal::Air(AirGoal::ParrotWander(crate::parrot::WanderGoal::new(1.0))),
            );
            add.goal(
                3,
                MobGoal::Air(AirGoal::FollowMob(crate::parrot::FollowMobGoal::new(
                    1.0,
                    PARROT_FOLLOW_MOB_STOP,
                    PARROT_FOLLOW_MOB_AREA,
                ))),
            );
        }
        EntityKind::Wither => {
            add.goal(
                0,
                MobGoal::Air(AirGoal::WitherDoNothing(crate::wither::DoNothingGoal)),
            );
            add.goal(
                2,
                MobGoal::RangedAttack(RangedAttackGoal::new(
                    WITHER_ATTACK_SPEED,
                    WITHER_ATTACK_INTERVAL,
                    WITHER_ATTACK_RADIUS,
                )),
            );
            add.goal(
                5,
                MobGoal::Stroll(RandomStrollGoal::water_avoiding_flying(1.0)),
            );
            add.look(6, WITHER_LOOK_DISTANCE);
            add.look_around(7);
            add.target_goal(1, MobGoal::HurtByTarget(HurtByTargetGoal::new()));
            add.target(
                2,
                NearestAttackableTargetGoal::with(
                    TargetKind::Class("LivingEntity"),
                    0,
                    false,
                    TargetFilter::WitherLiving,
                ),
            );
        }
        EntityKind::Bee => crate::bee::register(&mut add),
        EntityKind::Bat => add.goal(0, MobGoal::Air(AirGoal::Bat(crate::bat::BatGoal::new()))),
        EntityKind::Cod | EntityKind::Salmon | EntityKind::TropicalFish => {
            fish_goals(&mut add);
            let follow = FollowFlockLeaderGoal::new(&mut add.brain.random);
            add.goal(5, MobGoal::Water(WaterGoal::FollowFlockLeader(follow)));
        }
        EntityKind::Pufferfish => {
            fish_goals(&mut add);
            add.goal(
                1,
                MobGoal::Water(WaterGoal::PufferfishPuff(PufferfishPuffGoal)),
            );
        }
        EntityKind::Squid | EntityKind::GlowSquid => {
            add.goal(
                0,
                MobGoal::Water(WaterGoal::SquidRandomMovement(SquidRandomMovementGoal)),
            );
            add.goal(
                1,
                MobGoal::Water(WaterGoal::SquidFlee(SquidFleeGoal::default())),
            );
        }
        EntityKind::Dolphin => dolphin_goals(&mut add),
        EntityKind::Turtle => turtle_goals(&mut add),
        EntityKind::Drowned => drowned_goals(&mut add),
        EntityKind::Guardian | EntityKind::ElderGuardian => guardian_goals(&mut add, kind),
        EntityKind::Phantom => {
            add.brain.movement.set_style(MoveStyle::Phantom);
            add.goal(
                1,
                MobGoal::Air(AirGoal::PhantomAttackStrategy(
                    crate::phantom::AttackStrategyGoal::default(),
                )),
            );
            add.goal(
                2,
                MobGoal::Air(AirGoal::PhantomSweepAttack(
                    crate::phantom::SweepAttackGoal::default(),
                )),
            );
            add.goal(
                3,
                MobGoal::Air(AirGoal::PhantomCircle(
                    crate::phantom::CircleAroundAnchorGoal::new(),
                )),
            );
            add.target_goal(
                1,
                MobGoal::Air(AirGoal::PhantomAttackPlayer(
                    crate::phantom::AttackPlayerTargetGoal::new(),
                )),
            );
        }
        EntityKind::Vex => {
            add.brain.movement.set_style(MoveStyle::Vex);
            add.float(0);
            add.goal(
                4,
                MobGoal::Air(AirGoal::VexCharge(crate::vex::ChargeAttackGoal)),
            );
            add.goal(
                8,
                MobGoal::Air(AirGoal::VexRandomMove(crate::vex::RandomMoveGoal)),
            );
            add.goal(
                9,
                MobGoal::LookAtPlayer(LookAtPlayerGoal::with_probability(VEX_LOOK_DISTANCE, 1.0)),
            );
            add.goal(
                10,
                MobGoal::LookAtPlayer(LookAtPlayerGoal::at_mobs(VEX_MOB_LOOK_DISTANCE)),
            );
            add.target_goal(
                1,
                MobGoal::HurtByTarget(HurtByTargetGoal::ignoring_damage_from_alerting(
                    VEX_IGNORED_DAMAGE,
                )),
            );
            add.target_goal(
                2,
                MobGoal::Air(AirGoal::VexCopyOwnerTarget(
                    crate::vex::CopyOwnerTargetGoal::new(),
                )),
            );
            add.target(
                3,
                NearestAttackableTargetGoal::new(TargetKind::Player, true),
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
        EntityKind::Vindicator => {
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
                MobGoal::Raider(RaiderGoal::VindicatorBreakDoor(
                    crate::raider::VindicatorBreakDoorGoal::default(),
                )),
            );
            add.goal(
                3,
                MobGoal::Raider(RaiderGoal::OpenDoor(
                    crate::raider::RaiderOpenDoorGoal::default(),
                )),
            );
            add.goal(
                4,
                MobGoal::Crossbow(CrossbowGoal::HoldGround(HoldGroundAttackGoal::new(
                    VINDICATOR_HOLD_GROUND_RADIUS,
                ))),
            );
            add.goal(
                5,
                MobGoal::MeleeAttack(MeleeAttackGoal::new(VINDICATOR_MELEE_SPEED, false)),
            );
            add.goal(
                8,
                MobGoal::Stroll(RandomStrollGoal::new(VINDICATOR_STROLL_SPEED)),
            );
            add.goal(
                9,
                MobGoal::LookAtPlayer(LookAtPlayerGoal::with_probability(
                    VINDICATOR_LOOK_PLAYER_DISTANCE,
                    1.0,
                )),
            );
            add.goal(
                10,
                MobGoal::LookAtPlayer(LookAtPlayerGoal::at_mobs(VINDICATOR_LOOK_MOB_DISTANCE)),
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
                NearestAttackableTargetGoal::new(TargetKind::AbstractVillager, true),
            );
            add.target(
                3,
                NearestAttackableTargetGoal::new(TargetKind::IronGolem, true),
            );
            add.target_goal(
                4,
                MobGoal::Raider(RaiderGoal::JohnnyAttack(
                    crate::raider::JohnnyAttackGoal::default(),
                )),
            );
        }
        EntityKind::Evoker | EntityKind::Illusioner => {
            let evoker = kind == EntityKind::Evoker;
            raider::register(&mut add);
            add.float(0);
            add.goal(
                1,
                MobGoal::Spell(SpellGoal::Casting(CastingGoal::new(evoker))),
            );
            if evoker {
                add.goal(
                    2,
                    MobGoal::AvoidEntity(AvoidEntityGoal::new(
                        TargetKind::Player,
                        EVOKER_AVOID_DISTANCE,
                        EVOKER_AVOID_WALK_SPEED,
                        EVOKER_AVOID_SPRINT_SPEED,
                    )),
                );
                add.goal(
                    3,
                    MobGoal::AvoidEntity(AvoidEntityGoal::new(
                        TargetKind::Class("Creaking"),
                        EVOKER_AVOID_DISTANCE,
                        EVOKER_AVOID_WALK_SPEED,
                        EVOKER_AVOID_SPRINT_SPEED,
                    )),
                );
                for (priority, spell) in [
                    (4, IllagerSpell::SummonVex),
                    (5, IllagerSpell::Fangs),
                    (6, IllagerSpell::Wololo),
                ] {
                    add.goal(
                        priority,
                        MobGoal::Spell(SpellGoal::Use(UseSpellGoal::new(spell))),
                    );
                }
            } else {
                add.goal(
                    3,
                    MobGoal::AvoidEntity(AvoidEntityGoal::new(
                        TargetKind::Class("Creaking"),
                        PILLAGER_AVOID_DISTANCE,
                        PILLAGER_AVOID_WALK_SPEED,
                        PILLAGER_AVOID_SPRINT_SPEED,
                    )),
                );
                for (priority, spell) in
                    [(4, IllagerSpell::Disappear), (5, IllagerSpell::Blindness)]
                {
                    add.goal(
                        priority,
                        MobGoal::Spell(SpellGoal::Use(UseSpellGoal::new(spell))),
                    );
                }
                add.goal(
                    6,
                    MobGoal::RangedBowAttack(RangedBowAttackGoal::new(
                        ILLUSIONER_BOW_SPEED,
                        ILLUSIONER_BOW_INTERVAL,
                        ILLUSIONER_BOW_RADIUS,
                    )),
                );
            }
            add.goal(
                8,
                MobGoal::Stroll(RandomStrollGoal::new(SPELLCASTER_STROLL_SPEED)),
            );
            add.goal(
                9,
                MobGoal::LookAtPlayer(LookAtPlayerGoal::with_probability(
                    SPELLCASTER_LOOK_PLAYER_DISTANCE,
                    1.0,
                )),
            );
            add.goal(
                10,
                MobGoal::LookAtPlayer(LookAtPlayerGoal::at_mobs(SPELLCASTER_LOOK_MOB_DISTANCE)),
            );
            add.target_goal(
                1,
                MobGoal::HurtByTarget(HurtByTargetGoal::ignoring_damage_from_alerting(
                    PILLAGER_IGNORED_DAMAGE,
                )),
            );
            add.target(
                2,
                NearestAttackableTargetGoal::new(TargetKind::Player, true)
                    .with_unseen_memory(SPELLCASTER_UNSEEN_MEMORY),
            );
            add.target(
                3,
                NearestAttackableTargetGoal::new(TargetKind::AbstractVillager, false)
                    .with_unseen_memory(SPELLCASTER_UNSEEN_MEMORY),
            );
            let golems = NearestAttackableTargetGoal::new(TargetKind::IronGolem, false);
            add.target(
                3,
                if evoker {
                    golems
                } else {
                    golems.with_unseen_memory(SPELLCASTER_UNSEEN_MEMORY)
                },
            );
        }
        EntityKind::Ravager => {
            add.float(0);
            add.goal(
                4,
                MobGoal::MeleeAttack(MeleeAttackGoal::new(RAVAGER_MELEE_SPEED, true)),
            );
            add.stroll(5, RAVAGER_STROLL_SPEED);
            add.look(6, RAVAGER_PLAYER_LOOK_DISTANCE);
            add.goal(
                10,
                MobGoal::LookAtPlayer(LookAtPlayerGoal::at_mobs(RAVAGER_MOB_LOOK_DISTANCE)),
            );
            add.target_goal(
                2,
                MobGoal::HurtByTarget(HurtByTargetGoal::ignoring_damage_from_alerting(
                    PILLAGER_IGNORED_DAMAGE,
                )),
            );
            add.target(
                3,
                NearestAttackableTargetGoal::new(TargetKind::Player, true),
            );
            add.target(
                4,
                NearestAttackableTargetGoal::with(
                    TargetKind::AbstractVillager,
                    RAVAGER_VILLAGER_INTERVAL,
                    true,
                    TargetFilter::NotBaby,
                ),
            );
            add.target(
                4,
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
    fn the_wither_registers_the_vanilla_goals_targets_and_flying_control() {
        let (goals, targets) = lists(EntityKind::Wither);
        assert_eq!(
            goals,
            vec![
                (
                    0,
                    MobGoal::Air(AirGoal::WitherDoNothing(crate::wither::DoNothingGoal))
                ),
                (
                    2,
                    MobGoal::RangedAttack(RangedAttackGoal::new(1.0, 40, 20.0))
                ),
                (
                    5,
                    MobGoal::Stroll(RandomStrollGoal::water_avoiding_flying(1.0))
                ),
                (6, MobGoal::LookAtPlayer(LookAtPlayerGoal::new(8.0))),
                (
                    7,
                    MobGoal::RandomLookAround(RandomLookAroundGoal::default())
                ),
            ]
        );
        assert_eq!(
            targets,
            vec![
                (1, MobGoal::HurtByTarget(HurtByTargetGoal::new())),
                (
                    2,
                    MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::with(
                        TargetKind::Class("LivingEntity"),
                        0,
                        false,
                        TargetFilter::WitherLiving
                    ))
                ),
            ]
        );
        let ai = MobAi::new(EntityKind::Wither, 1, GoalSettings::default());
        let profile = &ai.brain().profile;
        assert!(profile.flying && profile.can_float && !profile.can_open_doors);
        assert_eq!(ai.brain().movement.style(), MoveStyle::Flying);
        assert_eq!(ai.brain().movement.max_turn(), 10);
        assert!(!ai.brain().movement.hovers_in_place());
    }

    fn bee_name(goal: &MobGoal) -> &'static str {
        match goal {
            MobGoal::Air(AirGoal::BeeAttack(_)) => "attack",
            MobGoal::Air(AirGoal::BeeEnterHive(_)) => "enter_hive",
            MobGoal::Breed(_) => "breed",
            MobGoal::Tempt(_) => "tempt",
            MobGoal::Air(AirGoal::BeeValidateHive(_)) => "validate_hive",
            MobGoal::Air(AirGoal::BeeValidateFlower(_)) => "validate_flower",
            MobGoal::Air(AirGoal::BeePollinate(_)) => "pollinate",
            MobGoal::FollowParent(_) => "follow_parent",
            MobGoal::Air(AirGoal::BeeLocateHive(_)) => "locate_hive",
            MobGoal::Air(AirGoal::BeeGoToHive(_)) => "go_to_hive",
            MobGoal::Air(AirGoal::BeeGoToKnownFlower(_)) => "go_to_flower",
            MobGoal::Air(AirGoal::BeeGrowCrop(_)) => "grow_crop",
            MobGoal::Air(AirGoal::BeeWander(_)) => "wander",
            MobGoal::Float(_) => "float",
            MobGoal::Air(AirGoal::BeeHurtBy(_)) => "hurt_by",
            MobGoal::Air(AirGoal::BeeBecomeAngry(_)) => "become_angry",
            MobGoal::ResetUniversalAnger(_) => "reset_anger",
            _ => "other",
        }
    }

    #[test]
    fn the_bee_registers_the_vanilla_goals_and_targets() {
        let (goals, targets) = lists(EntityKind::Bee);
        let named = |list: &Registered| -> Vec<(i32, &'static str)> {
            list.iter()
                .map(|(priority, goal)| (*priority, bee_name(goal)))
                .collect()
        };
        assert_eq!(
            named(&goals),
            vec![
                (0, "attack"),
                (1, "enter_hive"),
                (2, "breed"),
                (3, "tempt"),
                (3, "validate_hive"),
                (3, "validate_flower"),
                (4, "pollinate"),
                (5, "follow_parent"),
                (5, "locate_hive"),
                (5, "go_to_hive"),
                (6, "go_to_flower"),
                (7, "grow_crop"),
                (8, "wander"),
                (9, "float"),
            ]
        );
        assert_eq!(
            named(&targets),
            vec![(1, "hurt_by"), (2, "become_angry"), (3, "reset_anger")]
        );
        assert_eq!(
            goals[3].1,
            MobGoal::Tempt(TemptGoal::new(1.25, TemptItems::Tag("minecraft:bee_food")))
        );
        assert_eq!(goals[2].1, MobGoal::Breed(BreedGoal::new(1.0)));
        assert_eq!(
            goals[7].1,
            MobGoal::FollowParent(FollowParentGoal::new(1.25))
        );
        assert_eq!(
            targets[2].1,
            MobGoal::ResetUniversalAnger(crate::neutral::ResetUniversalAngerGoal::new(true))
        );
        let ai = MobAi::new(EntityKind::Bee, 1, GoalSettings::default());
        let profile = &ai.brain().profile;
        assert!(profile.flying && !profile.can_float && !profile.can_open_doors);
        assert_eq!(profile.malus(crate::PathType::Fire), -1.0);
        assert_eq!(profile.malus(crate::PathType::Water), -1.0);
        assert_eq!(profile.malus(crate::PathType::WaterBorder), 16.0);
        assert_eq!(profile.malus(crate::PathType::Cocoa), -1.0);
        assert_eq!(profile.malus(crate::PathType::Fence), -1.0);
        assert_eq!(ai.brain().movement.style(), MoveStyle::Flying);
        assert_eq!(ai.brain().movement.max_turn(), 20);
        assert!(ai.brain().movement.hovers_in_place());
    }

    #[test]
    fn the_parrot_registers_the_vanilla_goals_and_flies_with_a_flying_navigation() {
        let (goals, targets) = lists(EntityKind::Parrot);
        assert_eq!(
            goals,
            vec![
                (0, MobGoal::Panic(PanicGoal::new(1.25))),
                (0, MobGoal::Float(FloatGoal)),
                (1, MobGoal::LookAtPlayer(LookAtPlayerGoal::new(8.0))),
                (
                    2,
                    MobGoal::SitWhenOrdered(crate::tame::SitWhenOrderedToGoal)
                ),
                (
                    2,
                    MobGoal::FollowOwner(crate::tame::FollowOwnerGoal::new(1.0, 5.0, 1.0))
                ),
                (
                    2,
                    MobGoal::Air(AirGoal::ParrotWander(crate::parrot::WanderGoal::new(1.0)))
                ),
                (
                    3,
                    MobGoal::Air(AirGoal::FollowMob(crate::parrot::FollowMobGoal::new(
                        1.0, 3.0, 7.0
                    )))
                ),
            ]
        );
        assert!(targets.is_empty());
        let ai = MobAi::new(EntityKind::Parrot, 1, GoalSettings::default());
        let profile = &ai.brain().profile;
        assert!(profile.flying && profile.can_float && !profile.can_open_doors);
        assert_eq!(profile.malus(crate::PathType::Cocoa), -1.0);
        assert_eq!(profile.malus(crate::PathType::Fire), -1.0);
        assert_eq!(profile.malus(crate::PathType::FireInNeighbor), -1.0);
        assert_eq!(ai.brain().movement.style(), MoveStyle::Flying);
        assert_eq!(ai.brain().movement.max_turn(), 10);
        assert!(!ai.brain().movement.hovers_in_place());
    }

    #[test]
    fn the_phantom_registers_the_vanilla_goals_target_and_control() {
        let (goals, targets) = lists(EntityKind::Phantom);
        assert_eq!(
            goals,
            vec![
                (
                    1,
                    MobGoal::Air(AirGoal::PhantomAttackStrategy(
                        crate::phantom::AttackStrategyGoal::default()
                    ))
                ),
                (
                    2,
                    MobGoal::Air(AirGoal::PhantomSweepAttack(
                        crate::phantom::SweepAttackGoal::default()
                    ))
                ),
                (
                    3,
                    MobGoal::Air(AirGoal::PhantomCircle(
                        crate::phantom::CircleAroundAnchorGoal::new()
                    ))
                ),
            ]
        );
        assert_eq!(
            targets,
            vec![(
                1,
                MobGoal::Air(AirGoal::PhantomAttackPlayer(
                    crate::phantom::AttackPlayerTargetGoal::new()
                ))
            )]
        );
        let ai = MobAi::new(EntityKind::Phantom, 1, GoalSettings::default());
        assert_eq!(ai.brain().movement.style(), MoveStyle::Phantom);
        assert_eq!(ai.brain().movement.phantom_speed(), 0.1);
    }

    #[test]
    fn the_bat_runs_its_flight_and_rest_step_as_its_only_goal() {
        let (goals, targets) = lists(EntityKind::Bat);
        assert_eq!(
            goals,
            vec![(0, MobGoal::Air(AirGoal::Bat(crate::bat::BatGoal::new())))]
        );
        assert!(targets.is_empty());
    }

    #[test]
    fn the_vex_registers_the_vanilla_goals_targets_and_move_control() {
        let (goals, targets) = lists(EntityKind::Vex);
        assert_eq!(
            goals,
            vec![
                (0, MobGoal::Float(FloatGoal)),
                (
                    4,
                    MobGoal::Air(AirGoal::VexCharge(crate::vex::ChargeAttackGoal))
                ),
                (
                    8,
                    MobGoal::Air(AirGoal::VexRandomMove(crate::vex::RandomMoveGoal))
                ),
                (
                    9,
                    MobGoal::LookAtPlayer(LookAtPlayerGoal::with_probability(3.0, 1.0))
                ),
                (10, MobGoal::LookAtPlayer(LookAtPlayerGoal::at_mobs(8.0))),
            ]
        );
        assert_eq!(
            targets,
            vec![
                (
                    1,
                    MobGoal::HurtByTarget(HurtByTargetGoal::ignoring_damage_from_alerting(&[
                        "Raider"
                    ]))
                ),
                (
                    2,
                    MobGoal::Air(AirGoal::VexCopyOwnerTarget(
                        crate::vex::CopyOwnerTargetGoal::new()
                    ))
                ),
                (
                    3,
                    MobGoal::NearestAttackableTarget(NearestAttackableTargetGoal::new(
                        TargetKind::Player,
                        true
                    ))
                ),
            ]
        );
        let ai = MobAi::new(EntityKind::Vex, 1, GoalSettings::default());
        assert_eq!(ai.brain().movement.style(), MoveStyle::Vex);
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
