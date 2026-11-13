use rspigot_domain::EntityId;
use rspigot_domain::entity::species::evoker;
use rspigot_domain::entity::species::illusioner;
use rspigot_domain::entity::species::spellcaster::{DEFAULT_WARMUP_TICKS, IllagerSpell};

use crate::action::MobAction;
use crate::brain::AiContext;
use crate::combat::eye_of;
use crate::control::MAX_HEAD_Y_ROT;
use crate::goal::{Flag, Flags, Goal, adjusted_tick_delay};
use crate::random::RandomSource;
use crate::targeting::TargetingConditions;
use crate::world::{AiWorld, TargetKind};

const BLUE: u8 = evoker::WOLOLO_FROM_COLOR;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SpellFacts {
    pub casting_ticks: i32,
    pub wololo_target: Option<EntityId>,
    pub invisible: bool,
    pub harder_than_normal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellAction {
    Begin {
        spell: IllagerSpell,
        casting_ticks: i32,
    },
    End,
    Perform(IllagerSpell),
    WololoTarget(Option<EntityId>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CastingGoal {
    wololo_aware: bool,
}

impl CastingGoal {
    pub const fn new(wololo_aware: bool) -> Self {
        Self { wololo_aware }
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for CastingGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.body.spell.casting_ticks > 0
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        context.stop_navigation();
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        context.act(MobAction::Spell(SpellAction::End));
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        let look_at = match context.target() {
            Some(target) => Some(eye_of(&target)),
            None if self.wololo_aware => context
                .body
                .spell
                .wololo_target
                .and_then(|id| context.world.living(id))
                .map(|sheep| eye_of(&sheep)),
            None => None,
        };
        if let Some(at) = look_at {
            let x_max = context.mob.max_head_x_rot();
            context.brain.look.set_look_at(at, MAX_HEAD_Y_ROT, x_max);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UseSpellGoal {
    spell: IllagerSpell,
    next_attack_tick: i32,
    warmup: i32,
    last_target: Option<EntityId>,
}

impl UseSpellGoal {
    pub const fn new(spell: IllagerSpell) -> Self {
        Self {
            spell,
            next_attack_tick: 0,
            warmup: 0,
            last_target: None,
        }
    }

    pub const fn spell(&self) -> IllagerSpell {
        self.spell
    }

    const fn warmup_ticks(&self) -> i32 {
        match self.spell {
            IllagerSpell::Wololo => evoker::WOLOLO_WARMUP_TICKS,
            _ => DEFAULT_WARMUP_TICKS,
        }
    }

    const fn casting_ticks(&self) -> i32 {
        match self.spell {
            IllagerSpell::SummonVex => evoker::SUMMON_CASTING_TICKS,
            IllagerSpell::Fangs => evoker::ATTACK_CASTING_TICKS,
            IllagerSpell::Wololo => evoker::WOLOLO_CASTING_TICKS,
            IllagerSpell::Disappear => illusioner::MIRROR_CASTING_TICKS,
            IllagerSpell::Blindness => illusioner::BLINDNESS_CASTING_TICKS,
            IllagerSpell::None => 0,
        }
    }

    const fn interval(&self) -> i32 {
        match self.spell {
            IllagerSpell::SummonVex => evoker::SUMMON_INTERVAL,
            IllagerSpell::Fangs => evoker::ATTACK_INTERVAL,
            IllagerSpell::Wololo => evoker::WOLOLO_INTERVAL,
            IllagerSpell::Disappear => illusioner::MIRROR_INTERVAL,
            IllagerSpell::Blindness => illusioner::BLINDNESS_INTERVAL,
            IllagerSpell::None => 0,
        }
    }

    const fn prepare_sound(&self) -> Option<&'static str> {
        match self.spell {
            IllagerSpell::SummonVex => Some(evoker::PREPARE_SUMMON_SOUND),
            IllagerSpell::Fangs => Some(evoker::PREPARE_ATTACK_SOUND),
            IllagerSpell::Wololo => Some(evoker::PREPARE_WOLOLO_SOUND),
            IllagerSpell::Disappear => Some(illusioner::PREPARE_MIRROR_SOUND),
            IllagerSpell::Blindness => Some(illusioner::PREPARE_BLINDNESS_SOUND),
            IllagerSpell::None => None,
        }
    }

    fn base_can_use<W: AiWorld>(&self, context: &mut AiContext<'_, W>) -> bool {
        if !context.target().is_some_and(|target| target.alive) {
            return false;
        }
        context.body.spell.casting_ticks <= 0 && context.mob.tick_count >= self.next_attack_tick
    }

    fn summon_can_use<W: AiWorld>(&self, context: &mut AiContext<'_, W>) -> bool {
        if !self.base_can_use(context) {
            return false;
        }
        let area = context.mob.bounds().inflate(evoker::SUMMON_COUNT_RANGE);
        let conditions = TargetingConditions::for_non_combat()
            .range(evoker::SUMMON_COUNT_RANGE)
            .ignore_line_of_sight()
            .ignore_invisibility_testing();
        let world = context.world;
        let count = world
            .living_of(TargetKind::Class("Vex"), &area)
            .iter()
            .filter(|vex| conditions.test(context, vex, |_| true))
            .count() as i32;
        let bound = context
            .brain
            .random
            .next_int(evoker::SUMMON_COUNT_RANDOM_BOUND)
            + 1;
        count < bound
    }

    fn wololo_can_use<W: AiWorld>(&self, context: &mut AiContext<'_, W>) -> bool {
        if context.target().is_some()
            || context.body.spell.casting_ticks > 0
            || context.mob.tick_count < self.next_attack_tick
            || !context.world.mob_griefing()
        {
            return false;
        }
        let area = context.mob.bounds().inflate_by(
            evoker::WOLOLO_RANGE,
            evoker::WOLOLO_INFLATE_Y,
            evoker::WOLOLO_RANGE,
        );
        let conditions = TargetingConditions::for_non_combat().range(evoker::WOLOLO_RANGE);
        let world = context.world;
        let sheep: Vec<_> = world
            .living_of(TargetKind::Class("Sheep"), &area)
            .into_iter()
            .filter(|sheep| conditions.test(context, sheep, |s| s.facts.wool_color == BLUE))
            .collect();
        if sheep.is_empty() {
            return false;
        }
        let index = context.brain.random.next_int(sheep.len() as i32) as usize;
        let chosen = sheep[index].id;
        context.body.spell.wololo_target = Some(chosen);
        context.act(MobAction::Spell(SpellAction::WololoTarget(Some(chosen))));
        true
    }
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for UseSpellGoal {
    fn flags(&self) -> Flags {
        Flags::NONE
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self.spell {
            IllagerSpell::SummonVex => self.summon_can_use(context),
            IllagerSpell::Wololo => self.wololo_can_use(context),
            IllagerSpell::Disappear => self.base_can_use(context) && !context.body.spell.invisible,
            IllagerSpell::Blindness => {
                self.base_can_use(context)
                    && context
                        .target()
                        .is_some_and(|target| Some(target.id) != self.last_target)
                    && context.body.spell.harder_than_normal
            }
            IllagerSpell::Fangs | IllagerSpell::None => self.base_can_use(context),
        }
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        match self.spell {
            IllagerSpell::Wololo => context.body.spell.wololo_target.is_some() && self.warmup > 0,
            _ => context.target().is_some_and(|target| target.alive) && self.warmup > 0,
        }
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        self.warmup = adjusted_tick_delay(false, self.warmup_ticks());
        let casting = self.casting_ticks();
        context.body.spell.casting_ticks = casting;
        self.next_attack_tick = context.mob.tick_count + self.interval();
        if let Some(sound) = self.prepare_sound() {
            context.act(MobAction::Sound {
                sound,
                volume: 1.0,
                pitch: 1.0,
            });
        }
        context.act(MobAction::Spell(SpellAction::Begin {
            spell: self.spell,
            casting_ticks: casting,
        }));
        if self.spell == IllagerSpell::Blindness {
            self.last_target = context.target().map(|target| target.id);
        }
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        if self.spell == IllagerSpell::Wololo {
            context.body.spell.wololo_target = None;
            context.act(MobAction::Spell(SpellAction::WololoTarget(None)));
        }
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        self.warmup -= 1;
        if self.warmup == 0 {
            context.act(MobAction::Spell(SpellAction::Perform(self.spell)));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellGoal {
    Casting(CastingGoal),
    Use(UseSpellGoal),
}

macro_rules! dispatch {
    ($self:ident, $goal:ident => $call:expr) => {
        match $self {
            Self::Casting($goal) => $call,
            Self::Use($goal) => $call,
        }
    };
}

impl<W: AiWorld> Goal<AiContext<'_, W>> for SpellGoal {
    fn flags(&self) -> Flags {
        dispatch!(self, goal => Goal::<AiContext<'_, W>>::flags(goal))
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        dispatch!(self, goal => Goal::can_use(goal, context))
    }

    fn can_continue_to_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        dispatch!(self, goal => Goal::can_continue_to_use(goal, context))
    }

    fn start(&mut self, context: &mut AiContext<'_, W>) {
        dispatch!(self, goal => Goal::start(goal, context))
    }

    fn stop(&mut self, context: &mut AiContext<'_, W>) {
        dispatch!(self, goal => Goal::stop(goal, context))
    }

    fn tick(&mut self, context: &mut AiContext<'_, W>) {
        dispatch!(self, goal => Goal::tick(goal, context))
    }
}

#[cfg(test)]
mod tests {
    use rspigot_domain::Vec3;
    use rspigot_entities::EntityKind;

    use super::*;
    use crate::mock::{Harness, Mock, living};
    use crate::random::MobRandom;

    fn caster(kind: EntityKind, world: Mock, target: Option<i32>) -> Harness {
        let mut harness = Harness::new(kind, Vec3::new(0.5, 70.0, 0.5), world, 5);
        harness.brain.target = target.map(EntityId);
        harness
    }

    fn with_player() -> Mock {
        Mock {
            players: vec![living(2, EntityKind::Player, Vec3::new(5.5, 70.0, 0.5))],
            ..Mock::default()
        }
    }

    fn drain(harness: &mut Harness) -> Vec<MobAction> {
        std::mem::take(&mut harness.brain.actions)
    }

    fn spell_actions(actions: &[MobAction]) -> Vec<SpellAction> {
        actions
            .iter()
            .filter_map(|action| match action {
                MobAction::Spell(spell) => Some(*spell),
                _ => None,
            })
            .collect()
    }

    fn sheep(id: i32, at: Vec3, color: u8) -> crate::world::Living {
        let mut sheep = living(id, EntityKind::Sheep, at);
        sheep.facts.wool_color = color;
        sheep
    }

    #[test]
    fn the_casting_goal_blocks_moves_and_looks_and_runs_while_ticks_remain() {
        let mut goal = CastingGoal::new(true);
        let mut harness = caster(EntityKind::Evoker, with_player(), None);
        assert_eq!(
            Goal::<AiContext<'_, Mock>>::flags(&goal),
            Flags::of(Flag::Move).with(Flag::Look)
        );
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.body.spell.casting_ticks = 1;
        assert!(harness.run(|context| goal.can_use(context)));
        harness.run(|context| goal.stop(context));
        assert_eq!(spell_actions(&drain(&mut harness)), vec![SpellAction::End]);
    }

    #[test]
    fn the_casting_goal_stares_at_the_target_or_else_the_wololo_sheep() {
        let mut goal = CastingGoal::new(true);
        let mut with_target = caster(EntityKind::Evoker, with_player(), Some(2));
        with_target.body.spell.casting_ticks = 5;
        with_target.run(|context| goal.tick(context));
        let eye = with_target.world.players[0].eye_y();
        assert_eq!(with_target.brain.look.wanted(), Vec3::new(5.5, eye, 0.5));
        let world = Mock {
            others: vec![sheep(9, Vec3::new(0.5, 70.0, 6.5), 11)],
            ..Mock::default()
        };
        let mut wololo = caster(EntityKind::Evoker, world, None);
        wololo.body.spell.wololo_target = Some(EntityId(9));
        wololo.run(|context| goal.tick(context));
        let eye = wololo.world.others[0].eye_y();
        assert_eq!(wololo.brain.look.wanted(), Vec3::new(0.5, eye, 6.5));
        let mut plain = CastingGoal::new(false);
        let mut ignoring = caster(EntityKind::Illusioner, Mock::default(), None);
        ignoring.body.spell.wololo_target = Some(EntityId(9));
        ignoring.run(|context| plain.tick(context));
        assert!(!ignoring.brain.look.is_looking_at_target());
    }

    #[test]
    fn a_spell_needs_a_living_target_no_running_cast_and_the_cooldown_over() {
        let mut goal = UseSpellGoal::new(IllagerSpell::Fangs);
        let mut none = caster(EntityKind::Evoker, with_player(), None);
        assert!(!none.run(|context| goal.can_use(context)));
        let mut ready = caster(EntityKind::Evoker, with_player(), Some(2));
        assert!(ready.run(|context| goal.can_use(context)));
        ready.body.spell.casting_ticks = 3;
        assert!(!ready.run(|context| goal.can_use(context)));
        ready.body.spell.casting_ticks = 0;
        ready.world.players[0].alive = false;
        assert!(!ready.run(|context| goal.can_use(context)));
        ready.world.players[0].alive = true;
        goal.next_attack_tick = ready.mob.tick_count + 1;
        assert!(!ready.run(|context| goal.can_use(context)));
        goal.next_attack_tick = ready.mob.tick_count;
        assert!(ready.run(|context| goal.can_use(context)));
    }

    #[test]
    fn starting_a_spell_arms_the_warmup_the_cooldown_the_cast_time_and_the_prepare_sound() {
        let table = [
            (
                IllagerSpell::SummonVex,
                100,
                340,
                evoker::PREPARE_SUMMON_SOUND,
            ),
            (IllagerSpell::Fangs, 40, 100, evoker::PREPARE_ATTACK_SOUND),
            (IllagerSpell::Wololo, 60, 140, evoker::PREPARE_WOLOLO_SOUND),
            (
                IllagerSpell::Disappear,
                20,
                340,
                illusioner::PREPARE_MIRROR_SOUND,
            ),
            (
                IllagerSpell::Blindness,
                20,
                180,
                illusioner::PREPARE_BLINDNESS_SOUND,
            ),
        ];
        for (spell, casting, interval, sound) in table {
            let mut goal = UseSpellGoal::new(spell);
            let mut harness = caster(EntityKind::Evoker, with_player(), Some(2));
            harness.run(|context| goal.start(context));
            assert_eq!(goal.next_attack_tick, harness.mob.tick_count + interval);
            assert_eq!(harness.body.spell.casting_ticks, casting);
            let actions = drain(&mut harness);
            assert!(actions.contains(&MobAction::Sound {
                sound,
                volume: 1.0,
                pitch: 1.0
            }));
            assert_eq!(
                spell_actions(&actions),
                vec![SpellAction::Begin {
                    spell,
                    casting_ticks: casting
                }]
            );
        }
        let mut wololo = UseSpellGoal::new(IllagerSpell::Wololo);
        let mut fangs = UseSpellGoal::new(IllagerSpell::Fangs);
        let mut harness = caster(EntityKind::Evoker, with_player(), Some(2));
        harness.run(|context| wololo.start(context));
        harness.run(|context| fangs.start(context));
        assert_eq!(wololo.warmup, 20);
        assert_eq!(fangs.warmup, 10);
    }

    #[test]
    fn the_spell_fires_when_the_warmup_reaches_zero_and_the_goal_ends_there() {
        let mut goal = UseSpellGoal::new(IllagerSpell::Fangs);
        let mut harness = caster(EntityKind::Evoker, with_player(), Some(2));
        harness.run(|context| goal.start(context));
        drain(&mut harness);
        for _ in 0..9 {
            harness.run(|context| goal.tick(context));
            assert!(harness.run(|context| goal.can_continue_to_use(context)));
        }
        assert!(spell_actions(&drain(&mut harness)).is_empty());
        harness.run(|context| goal.tick(context));
        assert_eq!(
            spell_actions(&drain(&mut harness)),
            vec![SpellAction::Perform(IllagerSpell::Fangs)]
        );
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.world.players[0].alive = false;
        let mut again = UseSpellGoal::new(IllagerSpell::Fangs);
        again.warmup = 4;
        assert!(!harness.run(|context| again.can_continue_to_use(context)));
    }

    #[test]
    fn the_summon_waits_for_fewer_vexes_than_one_plus_a_roll_of_eight() {
        let harness = |count: usize, seed: i64| {
            let world = Mock {
                players: vec![living(2, EntityKind::Player, Vec3::new(5.5, 70.0, 0.5))],
                others: (0..count)
                    .map(|i| living(10 + i as i32, EntityKind::Vex, Vec3::new(3.5, 70.0, 0.5)))
                    .collect(),
                ..Mock::default()
            };
            let mut h = Harness::new(EntityKind::Evoker, Vec3::new(0.5, 70.0, 0.5), world, seed);
            h.brain.target = Some(EntityId(2));
            h
        };
        for count in 0..5 {
            for seed in [1_i64, 2, 3, 4, 5, 6] {
                let roll = MobRandom::new(seed).next_int(8) + 1;
                let mut goal = UseSpellGoal::new(IllagerSpell::SummonVex);
                let allowed = harness(count, seed).run(|context| goal.can_use(context));
                assert_eq!(allowed, (count as i32) < roll, "{count} {seed}");
            }
        }
        let mut goal = UseSpellGoal::new(IllagerSpell::SummonVex);
        let mut far = harness(0, 1);
        far.world
            .others
            .push(living(30, EntityKind::Vex, Vec3::new(40.5, 70.0, 0.5)));
        assert!(far.run(|context| goal.can_use(context)));
    }

    #[test]
    fn the_wololo_needs_no_target_a_blue_sheep_in_sight_and_mob_griefing() {
        let blue = sheep(9, Vec3::new(6.5, 70.0, 0.5), 11);
        let mut goal = UseSpellGoal::new(IllagerSpell::Wololo);
        let world = Mock {
            others: vec![blue],
            ..Mock::default()
        };
        let mut harness = caster(EntityKind::Evoker, world, None);
        assert!(harness.run(|context| goal.can_use(context)));
        assert_eq!(harness.body.spell.wololo_target, Some(EntityId(9)));
        assert_eq!(
            spell_actions(&drain(&mut harness)),
            vec![SpellAction::WololoTarget(Some(EntityId(9)))]
        );
        let mut white = caster(
            EntityKind::Evoker,
            Mock {
                others: vec![sheep(9, Vec3::new(6.5, 70.0, 0.5), 0)],
                ..Mock::default()
            },
            None,
        );
        assert!(!white.run(|context| goal.can_use(context)));
        let mut targeted = caster(
            EntityKind::Evoker,
            Mock {
                players: vec![living(2, EntityKind::Player, Vec3::new(5.5, 70.0, 0.5))],
                others: vec![blue],
                ..Mock::default()
            },
            Some(2),
        );
        assert!(!targeted.run(|context| goal.can_use(context)));
        let mut far = caster(
            EntityKind::Evoker,
            Mock {
                others: vec![sheep(9, Vec3::new(40.5, 70.0, 0.5), 11)],
                ..Mock::default()
            },
            None,
        );
        assert!(!far.run(|context| goal.can_use(context)));
        let mut casting = caster(
            EntityKind::Evoker,
            Mock {
                others: vec![blue],
                ..Mock::default()
            },
            None,
        );
        casting.body.spell.casting_ticks = 2;
        assert!(!casting.run(|context| goal.can_use(context)));
        let mut peaceful_world = caster(
            EntityKind::Evoker,
            Mock {
                others: vec![blue],
                no_griefing: true,
                ..Mock::default()
            },
            None,
        );
        assert!(!peaceful_world.run(|context| goal.can_use(context)));
    }

    #[test]
    fn the_wololo_lasts_while_it_has_a_sheep_and_a_warmup_and_forgets_the_sheep_on_stop() {
        let mut goal = UseSpellGoal::new(IllagerSpell::Wololo);
        let mut harness = caster(EntityKind::Evoker, Mock::default(), None);
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.body.spell.wololo_target = Some(EntityId(9));
        goal.warmup = 1;
        assert!(harness.run(|context| goal.can_continue_to_use(context)));
        goal.warmup = 0;
        assert!(!harness.run(|context| goal.can_continue_to_use(context)));
        harness.run(|context| goal.stop(context));
        assert_eq!(harness.body.spell.wololo_target, None);
        assert_eq!(
            spell_actions(&drain(&mut harness)),
            vec![SpellAction::WololoTarget(None)]
        );
    }

    #[test]
    fn the_mirror_image_refuses_to_recast_while_invisible() {
        let mut goal = UseSpellGoal::new(IllagerSpell::Disappear);
        let mut harness = caster(EntityKind::Illusioner, with_player(), Some(2));
        assert!(harness.run(|context| goal.can_use(context)));
        harness.body.spell.invisible = true;
        assert!(!harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn the_blindness_needs_a_new_target_and_a_local_difficulty_above_normal() {
        let mut goal = UseSpellGoal::new(IllagerSpell::Blindness);
        let mut harness = caster(EntityKind::Illusioner, with_player(), Some(2));
        assert!(!harness.run(|context| goal.can_use(context)));
        harness.body.spell.harder_than_normal = true;
        assert!(harness.run(|context| goal.can_use(context)));
        harness.run(|context| goal.start(context));
        assert_eq!(goal.last_target, Some(EntityId(2)));
        assert!(!harness.run(|context| goal.can_use(context)));
        harness
            .world
            .players
            .push(living(3, EntityKind::Player, Vec3::new(4.5, 70.0, 0.5)));
        harness.brain.target = Some(EntityId(3));
        goal.next_attack_tick = 0;
        harness.body.spell.casting_ticks = 0;
        assert!(harness.run(|context| goal.can_use(context)));
    }

    #[test]
    fn the_evoker_registers_the_vanilla_goals_and_targets() {
        use crate::brain::MobAi;
        use crate::registry::GoalSettings;
        let ai = MobAi::new(EntityKind::Evoker, 1, GoalSettings::default());
        let spells: Vec<(i32, IllagerSpell)> = ai
            .goals()
            .goals()
            .iter()
            .filter_map(|wrapped| match wrapped.goal() {
                crate::goals::MobGoal::Spell(SpellGoal::Use(goal)) => {
                    Some((wrapped.priority(), goal.spell()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            spells,
            vec![
                (4, IllagerSpell::SummonVex),
                (5, IllagerSpell::Fangs),
                (6, IllagerSpell::Wololo)
            ]
        );
        let casting = ai.goals().goals().iter().find(|wrapped| {
            matches!(
                wrapped.goal(),
                crate::goals::MobGoal::Spell(SpellGoal::Casting(_))
            )
        });
        assert_eq!(casting.map(|wrapped| wrapped.priority()), Some(1));
        let target_priorities: Vec<i32> = ai
            .targets()
            .goals()
            .iter()
            .map(|wrapped| wrapped.priority())
            .collect();
        assert_eq!(target_priorities, vec![1, 2, 3, 3]);
        assert!(ai.has_ai());
    }

    #[test]
    fn the_illusioner_registers_the_mirror_the_blindness_and_the_bow() {
        use crate::brain::MobAi;
        use crate::goals::MobGoal;
        use crate::registry::GoalSettings;
        let ai = MobAi::new(EntityKind::Illusioner, 1, GoalSettings::default());
        let layout: Vec<(i32, &'static str)> = ai
            .goals()
            .goals()
            .iter()
            .filter_map(|wrapped| {
                let name = match wrapped.goal() {
                    MobGoal::Spell(SpellGoal::Casting(_)) => "casting",
                    MobGoal::Spell(SpellGoal::Use(goal)) => match goal.spell() {
                        IllagerSpell::Disappear => "mirror",
                        IllagerSpell::Blindness => "blindness",
                        _ => "other",
                    },
                    MobGoal::RangedBowAttack(_) => "bow",
                    MobGoal::AvoidEntity(_) => "avoid",
                    _ => return None,
                };
                Some((wrapped.priority(), name))
            })
            .collect();
        assert_eq!(
            layout,
            vec![
                (1, "casting"),
                (3, "avoid"),
                (4, "mirror"),
                (5, "blindness"),
                (6, "bow")
            ]
        );
    }
}
