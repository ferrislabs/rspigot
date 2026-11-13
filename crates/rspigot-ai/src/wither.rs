use rspigot_domain::entity::species::wither;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind;

use crate::brain::AiContext;
use crate::goal::{Flag, Flags, Goal};
use crate::random::RandomSource;
use crate::sensing::has_line_of_sight_between;
use crate::targeting::{self, TargetingConditions};
use crate::world::{AiWorld, Living, TargetKind, inflate};

const WITHER_FRIENDS: &str = "minecraft:wither_friends";
const ENTITY_TAGS: &str = "minecraft:entity_type";
const NORMAL_DIFFICULTY: i32 = 2;
const HEAD_COUNT: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WitherFacts {
    pub invulnerable_ticks: i32,
    pub targets: [i32; HEAD_COUNT],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WitherBrain {
    pub next_head_update: [i32; wither::SIDE_HEADS],
    pub idle_head_updates: [i32; wither::SIDE_HEADS],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WitherAction {
    Shoot {
        head: u8,
        target: Vec3,
        dangerous: bool,
    },
    ShootAt {
        head: u8,
        target: EntityId,
    },
    Aim {
        head: u8,
        target: EntityId,
    },
    Clear {
        head: u8,
    },
}

pub fn is_wither_friend(kind: EntityKind) -> bool {
    rspigot_data::tags::tag(ENTITY_TAGS, WITHER_FRIENDS)
        .is_some_and(|ids| ids.contains(&u32::from(kind.id().0)))
}

pub fn living_selector(target: &Living) -> bool {
    !is_wither_friend(target.kind) && target.kind != EntityKind::ArmorStand
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DoNothingGoal;

impl<W: AiWorld> Goal<AiContext<'_, W>> for DoNothingGoal {
    fn flags(&self) -> Flags {
        Flags::of(Flag::Jump).with(Flag::Move).with(Flag::Look)
    }

    fn can_use(&mut self, context: &mut AiContext<'_, W>) -> bool {
        context.mob.species.wither.invulnerable_ticks > 0
    }
}

fn act(context: &mut AiContext<'_, impl AiWorld>, action: WitherAction) {
    context.act(crate::MobAction::Air(crate::AirAction::Wither(action)));
}

fn next_double(random: &mut impl RandomSource, min: f64, max: f64) -> f64 {
    if min >= max {
        min
    } else {
        random.next_double() * (max - min) + min
    }
}

fn nearby_target<W: AiWorld>(context: &mut AiContext<'_, W>, head: usize) {
    let area = inflate(
        &context.mob.bounds(),
        wither::HEAD_SCAN_HORIZONTAL,
        wither::HEAD_SCAN_VERTICAL,
        wither::HEAD_SCAN_HORIZONTAL,
    );
    let conditions = TargetingConditions::for_combat().range(wither::HEAD_SCAN_HORIZONTAL);
    let candidates = context
        .world
        .living_of(TargetKind::Class("LivingEntity"), &area);
    let found: Vec<Living> = candidates
        .into_iter()
        .filter(|candidate| conditions.test(context, candidate, living_selector))
        .collect();
    if found.is_empty() {
        return;
    }
    let index = context.brain.random.next_int(found.len() as i32) as usize;
    act(
        context,
        WitherAction::Aim {
            head: head as u8,
            target: found[index].id,
        },
    );
    context.brain.wither_targets[head] = found[index].id.0;
}

fn can_shoot_at<W: AiWorld>(context: &AiContext<'_, W>, target: &Living) -> bool {
    targeting::mob_can_attack(context.mob, context.world, target)
        && context.mob.distance_to_sqr(target.position) <= wither::HEAD_RANGE_SQR
        && has_line_of_sight_between(
            context.world,
            context.mob.eye_position(),
            Vec3::new(target.position.x, target.eye_y(), target.position.z),
        )
}

pub(crate) fn custom_step<W: AiWorld>(context: &mut AiContext<'_, W>) {
    let facts = context.mob.species.wither;
    context.brain.wither_targets = facts.targets;
    if facts.invulnerable_ticks >= 1 {
        return;
    }
    let tick = context.mob.tick_count;
    for head in 1..=wither::SIDE_HEADS {
        let index = head - 1;
        if context.brain.wither.next_head_update[index] > tick {
            continue;
        }
        let jitter = context.brain.random.next_int(wither::NEXT_HEAD_RANDOM);
        context.brain.wither.next_head_update[index] = tick + wither::NEXT_HEAD_BASE + jitter;
        if context.world.difficulty() >= NORMAL_DIFFICULTY {
            let idle = context.brain.wither.idle_head_updates[index];
            context.brain.wither.idle_head_updates[index] = idle + 1;
            if idle > wither::IDLE_HEAD_THRESHOLD {
                let at = context.mob.position;
                let random = &mut context.brain.random;
                let x = next_double(
                    random,
                    at.x - wither::IDLE_AIM_HORIZONTAL,
                    at.x + wither::IDLE_AIM_HORIZONTAL,
                );
                let y = next_double(
                    random,
                    at.y - wither::IDLE_AIM_VERTICAL,
                    at.y + wither::IDLE_AIM_VERTICAL,
                );
                let z = next_double(
                    random,
                    at.z - wither::IDLE_AIM_HORIZONTAL,
                    at.z + wither::IDLE_AIM_HORIZONTAL,
                );
                act(
                    context,
                    WitherAction::Shoot {
                        head: head as u8 + 1,
                        target: Vec3::new(x, y, z),
                        dangerous: true,
                    },
                );
                context.brain.wither.idle_head_updates[index] = 0;
            }
        }
        let current = context.brain.wither_targets[head];
        if current < 1 {
            nearby_target(context, head);
            continue;
        }
        let target = context.world.living(EntityId(current));
        match target {
            Some(target) if can_shoot_at(context, &target) => {
                act(
                    context,
                    WitherAction::ShootAt {
                        head: head as u8 + 1,
                        target: target.id,
                    },
                );
                let jitter = context
                    .brain
                    .random
                    .next_int(wither::NEXT_HEAD_AFTER_SHOT_RANDOM);
                context.brain.wither.next_head_update[index] =
                    tick + wither::NEXT_HEAD_AFTER_SHOT + jitter;
                context.brain.wither.idle_head_updates[index] = 0;
            }
            _ => {
                act(context, WitherAction::Clear { head: head as u8 });
                context.brain.wither_targets[head] = 0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::{Harness, Mock, living};
    use crate::{AirAction, MobAction};

    fn harness(world: Mock, facts: WitherFacts, tick: i32) -> Harness {
        let mut harness = Harness::new(EntityKind::Wither, Vec3::new(0.5, 64.0, 0.5), world, 1);
        harness.mob.species.wither = facts;
        harness.mob.tick_count = tick;
        harness
    }

    fn actions(harness: &mut Harness) -> Vec<WitherAction> {
        std::mem::take(&mut harness.brain.actions)
            .into_iter()
            .filter_map(|action| match action {
                MobAction::Air(AirAction::Wither(action)) => Some(action),
                _ => None,
            })
            .collect()
    }

    fn zombie(id: i32, position: Vec3) -> Living {
        living(id, EntityKind::Zombie, position)
    }

    #[test]
    fn do_nothing_holds_jump_move_and_look_only_while_invulnerable() {
        let mut goal = DoNothingGoal;
        let flags = Goal::<AiContext<'_, Mock>>::flags(&goal);
        assert!(
            flags.contains(Flag::Jump) && flags.contains(Flag::Move) && flags.contains(Flag::Look)
        );
        assert!(!flags.contains(Flag::Target));
        let mut idle = harness(Mock::default(), WitherFacts::default(), 1);
        assert!(!idle.run(|context| goal.can_use(context)));
        let facts = WitherFacts {
            invulnerable_ticks: 1,
            ..WitherFacts::default()
        };
        let mut growing = harness(Mock::default(), facts, 1);
        assert!(growing.run(|context| goal.can_use(context)));
    }

    #[test]
    fn the_wither_selector_refuses_undead_friends_and_armor_stands() {
        assert!(!living_selector(&living(
            1,
            EntityKind::Skeleton,
            Vec3::ZERO
        )));
        assert!(!living_selector(&living(
            1,
            EntityKind::WitherSkeleton,
            Vec3::ZERO
        )));
        assert!(!living_selector(&living(1, EntityKind::Zombie, Vec3::ZERO)));
        assert!(!living_selector(&living(
            1,
            EntityKind::ArmorStand,
            Vec3::ZERO
        )));
        assert!(living_selector(&living(1, EntityKind::Cow, Vec3::ZERO)));
        assert!(living_selector(&living(1, EntityKind::Player, Vec3::ZERO)));
        assert!(living_selector(&living(1, EntityKind::Creeper, Vec3::ZERO)));
    }

    #[test]
    fn nothing_happens_while_the_spawn_invulnerability_lasts() {
        let facts = WitherFacts {
            invulnerable_ticks: 100,
            ..WitherFacts::default()
        };
        let mut harness = harness(Mock::default(), facts, 50);
        harness.run(custom_step);
        assert!(actions(&mut harness).is_empty());
        assert_eq!(harness.brain.wither, WitherBrain::default());
    }

    #[test]
    fn a_head_that_is_not_due_does_nothing_and_a_due_head_schedules_its_next_update_ten_to_nineteen_ticks_later()
     {
        let mut harness = harness(Mock::default(), WitherFacts::default(), 100);
        harness.brain.wither.next_head_update = [101, 0];
        harness.run(custom_step);
        assert_eq!(harness.brain.wither.next_head_update[0], 101);
        let next = harness.brain.wither.next_head_update[1];
        assert!((110..=119).contains(&next), "{next}");
    }

    #[test]
    fn a_due_head_without_a_target_picks_a_random_visible_entity_within_the_scan_box() {
        let world = Mock {
            others: vec![
                living(5, EntityKind::Cow, Vec3::new(5.5, 64.0, 0.5)),
                zombie(6, Vec3::new(3.5, 64.0, 0.5)),
                living(7, EntityKind::Cow, Vec3::new(80.5, 64.0, 0.5)),
            ],
            ..Mock::default()
        };
        let mut harness = harness(world, WitherFacts::default(), 0);
        harness.run(custom_step);
        let aims: Vec<WitherAction> = actions(&mut harness);
        assert_eq!(aims.len(), 2);
        for aim in aims {
            assert_eq!(
                aim,
                WitherAction::Aim {
                    head: match aim {
                        WitherAction::Aim { head, .. } => head,
                        _ => unreachable!(),
                    },
                    target: EntityId(5)
                }
            );
        }
    }

    #[test]
    fn a_head_with_a_visible_target_shoots_it_and_waits_forty_to_fifty_nine_ticks() {
        let world = Mock {
            others: vec![living(5, EntityKind::Cow, Vec3::new(5.5, 64.0, 0.5))],
            ..Mock::default()
        };
        let facts = WitherFacts {
            targets: [0, 5, 0],
            ..WitherFacts::default()
        };
        let mut harness = harness(world, facts, 10);
        harness.brain.wither.idle_head_updates = [4, 0];
        harness.brain.wither.next_head_update = [0, 1000];
        harness.run(custom_step);
        let shots = actions(&mut harness);
        assert_eq!(
            shots,
            vec![WitherAction::ShootAt {
                head: 2,
                target: EntityId(5)
            }]
        );
        let next = harness.brain.wither.next_head_update[0];
        assert!((50..=69).contains(&next), "{next}");
        assert_eq!(harness.brain.wither.idle_head_updates[0], 0);
    }

    #[test]
    fn a_head_loses_a_target_that_is_too_far_or_gone() {
        let world = Mock {
            others: vec![living(5, EntityKind::Cow, Vec3::new(40.5, 64.0, 0.5))],
            ..Mock::default()
        };
        let facts = WitherFacts {
            targets: [0, 5, 9],
            ..WitherFacts::default()
        };
        let mut harness = harness(world, facts, 10);
        harness.brain.wither.next_head_update = [0, 0];
        harness.run(custom_step);
        assert_eq!(
            actions(&mut harness),
            vec![
                WitherAction::Clear { head: 1 },
                WitherAction::Clear { head: 2 }
            ]
        );
    }

    #[test]
    fn the_idle_counter_fires_a_dangerous_skull_at_a_random_point_after_sixteen_idle_updates_on_normal()
     {
        let mut harness = harness(Mock::default(), WitherFacts::default(), 0);
        harness.brain.wither.idle_head_updates = [16, 0];
        harness.brain.wither.next_head_update = [0, 1000];
        harness.run(custom_step);
        let shots = actions(&mut harness);
        let WitherAction::Shoot {
            head,
            target,
            dangerous,
        } = shots[0]
        else {
            panic!("{shots:?}");
        };
        assert_eq!(head, 2);
        assert!(dangerous);
        assert!((-9.5..=10.5).contains(&target.x));
        assert!((59.0..=69.0).contains(&target.y));
        assert!((-9.5..=10.5).contains(&target.z));
        assert_eq!(harness.brain.wither.idle_head_updates[0], 0);
    }

    #[test]
    fn the_idle_counter_does_not_run_on_peaceful_or_easy_and_not_before_sixteen() {
        let mut harness = harness(
            Mock {
                peaceful: true,
                ..Mock::default()
            },
            WitherFacts::default(),
            0,
        );
        harness.brain.wither.idle_head_updates = [16, 0];
        harness.brain.wither.next_head_update = [0, 1000];
        harness.run(custom_step);
        assert!(actions(&mut harness).is_empty());
        assert_eq!(harness.brain.wither.idle_head_updates[0], 16);
        let mut normal = self::harness(Mock::default(), WitherFacts::default(), 0);
        normal.brain.wither.idle_head_updates = [15, 0];
        normal.brain.wither.next_head_update = [0, 1000];
        normal.run(custom_step);
        assert!(actions(&mut normal).is_empty());
        assert_eq!(normal.brain.wither.idle_head_updates[0], 16);
    }
}
