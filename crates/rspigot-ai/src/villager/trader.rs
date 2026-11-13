use rspigot_domain::random::RandomSource;
use rspigot_domain::{BlockPos, EntityId, Vec3};

use crate::action::MobAction;
use crate::brain::AiContext;
use crate::goal::{Flag, Flags, Goal};
use crate::goals::LookAtPlayerGoal;
use crate::scan::closer_to_center_than;
use crate::villager::VillagerAction;
use crate::world::AiWorld;

const FAR_DISTANCE: f64 = 10.0;
const TRADE_DISTANCE_SQR: f64 = 16.0;
const LOOK_DISTANCE: f32 = 8.0;
const SOUND_PITCH_SPREAD: f32 = 0.2;
const SOUND_PITCH_BASE: f32 = 0.9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TraderFacts {
    pub wander_target: Option<BlockPos>,
    pub trading_player: Option<EntityId>,
    pub hurt_recently: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraderItem {
    InvisibilityPotion,
    MilkBucket,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TraderGoal {
    WanderToPosition {
        stop_distance: f64,
        speed_modifier: f64,
    },
    TradeWithPlayer,
    LookAtTradingPlayer(LookAtPlayerGoal),
    UseItem {
        item: TraderItem,
        finish_sound: &'static str,
    },
}

impl TraderGoal {
    pub fn wander_to_position(stop_distance: f64, speed_modifier: f64) -> Self {
        Self::WanderToPosition {
            stop_distance,
            speed_modifier,
        }
    }

    pub fn trade_with_player() -> Self {
        Self::TradeWithPlayer
    }

    pub fn look_at_trading_player() -> Self {
        Self::LookAtTradingPlayer(LookAtPlayerGoal::new(LOOK_DISTANCE))
    }

    pub fn use_item(item: TraderItem, finish_sound: &'static str) -> Self {
        Self::UseItem { item, finish_sound }
    }
}

fn selector_allows<W: AiWorld>(context: &AiContext<'_, W>, item: TraderItem) -> bool {
    let invisible = context
        .world
        .living(context.mob.id)
        .is_some_and(|living| living.invisible);
    match item {
        TraderItem::InvisibilityPotion => context.world.dark_outside() && !invisible,
        TraderItem::MilkBucket => context.world.bright_outside() && invisible,
    }
}

fn can_trade_with_player<W: AiWorld>(context: &AiContext<'_, W>) -> bool {
    let facts = context.mob.species.trader;
    if context.mob.fluid.in_water || !context.mob.on_ground || facts.hurt_recently {
        return false;
    }
    let Some(player) = facts
        .trading_player
        .and_then(|player| context.world.living(player))
    else {
        return false;
    };
    context.mob.distance_to_sqr(player.position) <= TRADE_DISTANCE_SQR
}

fn is_too_far_away<W: AiWorld>(context: &AiContext<'_, W>, pos: BlockPos, distance: f64) -> bool {
    !closer_to_center_than(pos, context.mob.position, distance)
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for TraderGoal {
    fn flags(&self) -> Flags {
        match self {
            Self::WanderToPosition { .. } => Flags::of(Flag::Move),
            Self::TradeWithPlayer => Flags::of(Flag::Jump).with(Flag::Move),
            Self::LookAtTradingPlayer(_) => Flags::of(Flag::Look),
            Self::UseItem { .. } => Flags::NONE,
        }
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::WanderToPosition { stop_distance, .. } => {
                let stop_distance = *stop_distance;
                context
                    .mob
                    .species
                    .trader
                    .wander_target
                    .is_some_and(|target| is_too_far_away(context, target, stop_distance))
            }
            Self::TradeWithPlayer => can_trade_with_player(context),
            Self::LookAtTradingPlayer(look) => {
                look.can_use_trading(context.mob.species.trader.trading_player)
            }
            Self::UseItem { item, .. } => selector_allows(context, *item),
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::LookAtTradingPlayer(look) => look.can_continue_to_use(context),
            Self::UseItem { .. } => context.mob.using_item.is_some(),
            _ => self.can_use(context),
        }
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::TradeWithPlayer => context.stop_navigation(),
            Self::LookAtTradingPlayer(look) => look.start(context),
            Self::UseItem { item, .. } => {
                context.act(MobAction::Villager(VillagerAction::StartUsing(*item)));
            }
            Self::WanderToPosition { .. } => {}
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        match self {
            Self::WanderToPosition { .. } => {
                context.act(MobAction::Villager(VillagerAction::ClearWanderTarget));
                context.stop_navigation();
            }
            Self::TradeWithPlayer => {
                context.act(MobAction::Villager(VillagerAction::StopTrading));
            }
            Self::LookAtTradingPlayer(look) => look.stop(),
            Self::UseItem { finish_sound, .. } => {
                context.act(MobAction::Villager(VillagerAction::SetMainHand(None)));
                let pitch =
                    context.brain.random.next_float() * SOUND_PITCH_SPREAD + SOUND_PITCH_BASE;
                context.act(MobAction::Sound {
                    sound: finish_sound,
                    volume: 1.0,
                    pitch,
                });
            }
        }
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let speed_modifier = match self {
            Self::WanderToPosition { speed_modifier, .. } => *speed_modifier,
            Self::LookAtTradingPlayer(look) => {
                look.tick(context);
                return;
            }
            _ => return,
        };
        let Some(target) = context.mob.species.trader.wander_target else {
            return;
        };
        if !context.brain.navigation.is_done() {
            return;
        }
        let here = context.mob.position;
        if is_too_far_away(context, target, FAR_DISTANCE) {
            let delta = Vec3::new(
                f64::from(target.x) - here.x,
                f64::from(target.y) - here.y,
                f64::from(target.z) - here.z,
            );
            let length = (delta.x * delta.x + delta.y * delta.y + delta.z * delta.z).sqrt();
            let scale = if length > 0.0 {
                FAR_DISTANCE / length
            } else {
                0.0
            };
            let toward = Vec3::new(
                delta.x * scale + here.x,
                delta.y * scale + here.y,
                delta.z * scale + here.z,
            );
            context.move_to_pos(toward, speed_modifier);
        } else {
            context.move_to_pos(
                Vec3::new(
                    f64::from(target.x),
                    f64::from(target.y),
                    f64::from(target.z),
                ),
                speed_modifier,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::mock::{Harness, Mock, living};
    use crate::world::Hand;

    const PLAYER: EntityId = EntityId(2);

    fn harness(player_at: f64) -> Harness {
        let mut world = Mock::default();
        world.players.push(living(
            2,
            EntityKind::Player,
            Vec3::new(player_at, 64.0, 0.0),
        ));
        world.others.push(living(
            1,
            EntityKind::WanderingTrader,
            Vec3::new(0.0, 64.0, 0.0),
        ));
        let mut harness = Harness::new(
            EntityKind::WanderingTrader,
            Vec3::new(0.0, 64.0, 0.0),
            world,
            3,
        );
        harness.mob.species.trader.trading_player = Some(PLAYER);
        harness
    }

    fn can_use(harness: &mut Harness, goal: &mut TraderGoal) -> bool {
        harness.run(|context| Goal::can_use(goal, context))
    }

    #[test]
    fn trading_stops_the_trader_when_the_player_is_close_and_it_is_calm() {
        let mut goal = TraderGoal::trade_with_player();
        let mut harness = harness(3.9);
        assert!(can_use(&mut harness, &mut goal));
        harness.world.players[0].position = Vec3::new(4.1, 64.0, 0.0);
        assert!(!can_use(&mut harness, &mut goal));
        harness.world.players[0].position = Vec3::new(1.0, 64.0, 0.0);
        harness.mob.on_ground = false;
        assert!(!can_use(&mut harness, &mut goal));
        harness.mob.on_ground = true;
        harness.mob.fluid.in_water = true;
        assert!(!can_use(&mut harness, &mut goal));
        harness.mob.fluid.in_water = false;
        harness.mob.species.trader.hurt_recently = true;
        assert!(!can_use(&mut harness, &mut goal));
        harness.mob.species.trader.hurt_recently = false;
        harness.mob.species.trader.trading_player = None;
        assert!(!can_use(&mut harness, &mut goal));
    }

    #[test]
    fn trading_blocks_moving_and_jumping_and_lets_go_of_the_player_when_it_ends() {
        let mut goal = TraderGoal::trade_with_player();
        let flags = Goal::<AiContext<'_, Mock>>::flags(&goal);
        assert!(flags.contains(Flag::Move) && flags.contains(Flag::Jump));
        assert!(!flags.contains(Flag::Look));
        let mut harness = harness(2.0);
        harness.run(|context| Goal::stop(&mut goal, context));
        assert_eq!(
            harness.brain.actions,
            vec![MobAction::Villager(VillagerAction::StopTrading)]
        );
    }

    #[test]
    fn the_trader_looks_at_its_trading_player_only_while_trading() {
        let mut goal = TraderGoal::look_at_trading_player();
        let flags = Goal::<AiContext<'_, Mock>>::flags(&goal);
        assert!(flags.contains(Flag::Look) && !flags.contains(Flag::Move));
        let mut harness = harness(20.0);
        assert!(can_use(&mut harness, &mut goal));
        harness.run(|context| Goal::start(&mut goal, context));
        assert!(!harness.run(|context| Goal::can_continue_to_use(&mut goal, context)));
        harness.world.players[0].position = Vec3::new(6.0, 64.0, 0.0);
        harness.run(|context| Goal::start(&mut goal, context));
        assert!(harness.run(|context| Goal::can_continue_to_use(&mut goal, context)));
        harness.mob.species.trader.trading_player = None;
        assert!(!can_use(&mut harness, &mut goal));
    }

    #[test]
    fn the_trader_drinks_invisibility_in_the_dark_and_milk_in_daylight_once_invisible() {
        let mut potion = TraderGoal::use_item(TraderItem::InvisibilityPotion, "a");
        let mut milk = TraderGoal::use_item(TraderItem::MilkBucket, "b");
        let mut harness = harness(2.0);
        assert!(!can_use(&mut harness, &mut potion));
        assert!(!can_use(&mut harness, &mut milk));
        harness.world.dark = true;
        assert!(can_use(&mut harness, &mut potion));
        assert!(!can_use(&mut harness, &mut milk));
        harness.world.others[0].invisible = true;
        assert!(!can_use(&mut harness, &mut potion));
        harness.world.dark = false;
        harness.world.bright = true;
        assert!(can_use(&mut harness, &mut milk));
        harness.world.others[0].invisible = false;
        assert!(!can_use(&mut harness, &mut milk));
    }

    #[test]
    fn using_an_item_claims_no_control_and_runs_while_the_item_is_in_use() {
        let mut goal = TraderGoal::use_item(TraderItem::MilkBucket, "minecraft:x");
        assert_eq!(Goal::<AiContext<'_, Mock>>::flags(&goal), Flags::NONE);
        let mut harness = harness(2.0);
        harness.run(|context| Goal::start(&mut goal, context));
        assert_eq!(
            harness.brain.actions,
            vec![MobAction::Villager(VillagerAction::StartUsing(
                TraderItem::MilkBucket
            ))]
        );
        assert!(!harness.run(|context| Goal::can_continue_to_use(&mut goal, context)));
        harness.mob.using_item = Some(Hand::Main);
        assert!(harness.run(|context| Goal::can_continue_to_use(&mut goal, context)));
    }

    #[test]
    fn finishing_clears_the_hand_and_plays_the_sound_with_a_varied_pitch() {
        let mut goal = TraderGoal::use_item(
            TraderItem::InvisibilityPotion,
            "minecraft:entity.wandering_trader.disappeared",
        );
        let mut harness = harness(2.0);
        harness.run(|context| Goal::stop(&mut goal, context));
        assert_eq!(
            harness.brain.actions[0],
            MobAction::Villager(VillagerAction::SetMainHand(None))
        );
        let MobAction::Sound {
            sound,
            volume,
            pitch,
        } = harness.brain.actions[1]
        else {
            panic!("a sound");
        };
        assert_eq!(sound, "minecraft:entity.wandering_trader.disappeared");
        assert_eq!(volume, 1.0);
        assert!((0.9..1.1).contains(&pitch));
    }
}
