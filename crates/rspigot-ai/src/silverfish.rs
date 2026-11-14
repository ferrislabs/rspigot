use rspigot_blocks::Face;
use rspigot_blocks::infested::is_compatible_host_block;
use rspigot_domain::BlockPos;
use rspigot_entities::EntityKind;

use crate::action::MobAction;
use crate::brain::AiContext;
use crate::goal::{Flag, Flags, Goal, adjusted_tick_delay, reduced_tick_delay};
use crate::goals::{
    ClimbOnTopOfPowderSnowGoal, MobGoal, NearestAttackableTargetGoal, RandomStrollGoal,
};
use crate::random::RandomSource;
use crate::registry::Goals;
use crate::world::{AiWorld, TargetKind};
use crate::{HurtByTargetGoal, MeleeAttackGoal};

const FACES: [Face; 6] = [
    Face::Down,
    Face::Up,
    Face::North,
    Face::South,
    Face::West,
    Face::East,
];
const WAKE_UP_DELAY: i32 = 20;
const MERGE_CHANCE: i32 = 10;
const MERGE_INTERVAL: i32 = 10;
const MERGE_SPEED: f64 = 1.0;
const MELEE_SPEED: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WakeUpFriendsGoal {
    look_for_friends: i32,
}

impl WakeUpFriendsGoal {
    pub fn notify_hurt(&mut self) {
        if self.look_for_friends == 0 {
            self.look_for_friends = adjusted_tick_delay(false, WAKE_UP_DELAY);
        }
    }

    pub fn look_for_friends(&self) -> i32 {
        self.look_for_friends
    }

    fn tick<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.look_for_friends -= 1;
        if self.look_for_friends <= 0 {
            context.act(MobAction::Silverfish(SilverfishAction::WakeUpFriends(
                context.mob.block_pos(),
            )));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MergeWithStoneGoal {
    stroll: RandomStrollGoal,
    selected_direction: Option<Face>,
    do_merge: bool,
}

impl Default for MergeWithStoneGoal {
    fn default() -> Self {
        Self {
            stroll: RandomStrollGoal::with_interval(MERGE_SPEED, MERGE_INTERVAL),
            selected_direction: None,
            do_merge: false,
        }
    }
}

impl MergeWithStoneGoal {
    pub fn do_merge(&self) -> bool {
        self.do_merge
    }

    fn neighbour<W: AiWorld>(context: &AiContext<'_, W>, face: Face) -> BlockPos {
        let (dx, dy, dz) = face.offset();
        let position = context.mob.position;
        BlockPos::new(
            position.x.floor() as i32,
            (position.y + 0.5).floor() as i32,
            position.z.floor() as i32,
        )
        .offset(dx, dy, dz)
    }

    fn can_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if context.target().is_some() {
            return false;
        }
        if !context.brain.navigation.is_done() {
            return false;
        }
        if context.world.mob_griefing()
            && context
                .brain
                .random
                .next_int(reduced_tick_delay(MERGE_CHANCE))
                == 0
        {
            let face = FACES[context.brain.random.next_int(FACES.len() as i32) as usize];
            self.selected_direction = Some(face);
            let pos = Self::neighbour(context, face);
            if context
                .world
                .state(pos)
                .is_some_and(is_compatible_host_block)
            {
                self.do_merge = true;
                return true;
            }
        }
        self.do_merge = false;
        self.stroll.can_use(context)
    }

    fn can_continue_to_use<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) -> bool {
        if self.do_merge {
            false
        } else {
            self.stroll.can_continue_to_use(context)
        }
    }

    fn start<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        if !self.do_merge {
            self.stroll.start(context);
            return;
        }
        let Some(face) = self.selected_direction else {
            return;
        };
        let pos = Self::neighbour(context, face);
        if context
            .world
            .state(pos)
            .is_some_and(is_compatible_host_block)
        {
            context.act(MobAction::Silverfish(SilverfishAction::Merge(pos)));
        }
    }

    fn stop<W: AiWorld>(&mut self, context: &mut AiContext<'_, W>) {
        self.stroll.stop(context);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SilverfishGoal {
    WakeUpFriends(WakeUpFriendsGoal),
    MergeWithStone(MergeWithStoneGoal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SilverfishFacts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SilverfishAction {
    WakeUpFriends(BlockPos),
    Merge(BlockPos),
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for SilverfishGoal {
    fn flags(&self) -> Flags {
        match self {
            Self::WakeUpFriends(_) => Flags::NONE,
            Self::MergeWithStone(_) => Flags::of(Flag::Move),
        }
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::WakeUpFriends(goal) => goal.look_for_friends > 0,
            Self::MergeWithStone(goal) => goal.can_use(context),
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self {
            Self::WakeUpFriends(goal) => goal.look_for_friends > 0,
            Self::MergeWithStone(goal) => goal.can_continue_to_use(context),
        }
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        if let Self::MergeWithStone(goal) = self {
            goal.start(context);
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        if let Self::MergeWithStone(goal) = self {
            goal.stop(context);
        }
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        if let Self::WakeUpFriends(goal) = self {
            goal.tick(context);
        }
    }
}

pub(crate) fn register(add: &mut Goals<'_>, _kind: EntityKind) {
    add.float(1);
    add.goal(
        1,
        MobGoal::ClimbOnTopOfPowderSnow(ClimbOnTopOfPowderSnowGoal),
    );
    add.goal(
        3,
        MobGoal::Silverfish(SilverfishGoal::WakeUpFriends(WakeUpFriendsGoal::default())),
    );
    add.goal(
        4,
        MobGoal::MeleeAttack(MeleeAttackGoal::new(MELEE_SPEED, false)),
    );
    add.goal(
        5,
        MobGoal::Silverfish(SilverfishGoal::MergeWithStone(MergeWithStoneGoal::default())),
    );
    add.target_goal(1, MobGoal::HurtByTarget(HurtByTargetGoal::alerting(&[])));
    add.target(
        2,
        NearestAttackableTargetGoal::new(TargetKind::Player, true),
    );
}
