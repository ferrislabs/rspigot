use std::f64::consts::TAU;

use super::catalog;
use crate::domain::{SheepKind, TeamId};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub fn cell(self) -> Cell {
        Cell {
            x: self.x.floor() as i32,
            y: self.y.floor() as i32,
            z: self.z.floor() as i32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cell {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Fire,
    Ice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticleKind {
    Explosion,
    Flame,
    Snowflake,
    Crit,
    Heart,
    Portal,
    LargeSmoke,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundKind {
    Explode,
    Fire,
    Ice,
    Thunder,
    Heal,
    Teleport,
    Quake,
    Darkness,
    Split,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    Slowness,
    Regeneration,
    Blindness,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    Explode {
        at: Vec3,
        power: f32,
        breaks_blocks: bool,
    },
    SetBlock {
        at: Cell,
        block: BlockKind,
    },
    Particle {
        kind: ParticleKind,
        at: Vec3,
        count: i32,
    },
    Sound {
        kind: SoundKind,
        at: Vec3,
    },
    Effect {
        team: TeamId,
        center: Vec3,
        radius: f64,
        effect: EffectKind,
        duration_ticks: i32,
        amplifier: u8,
    },
    Velocity {
        team: TeamId,
        center: Vec3,
        radius: f64,
        velocity: Vec3,
    },
    Lightning {
        at: Vec3,
    },
    SpawnSheep {
        kind: SheepKind,
        at: Vec3,
        velocity: Vec3,
    },
    TeleportThrower {
        to: Vec3,
    },
}

pub fn plan(kind: SheepKind, at: Vec3, thrower_team: TeamId) -> Vec<Action> {
    let enemies = thrower_team.other();
    match kind {
        SheepKind::Explosive => vec![
            Action::Explode {
                at,
                power: catalog::EXPLOSIVE_POWER,
                breaks_blocks: catalog::EXPLOSIVE_BREAKS_BLOCKS,
            },
            particle(ParticleKind::Explosion, at, 3),
        ],
        SheepKind::Incendiary => {
            let mut actions = disc(at.cell(), catalog::INCENDIARY_RADIUS, 0, BlockKind::Fire);
            actions.push(particle(ParticleKind::Flame, at, 60));
            actions.push(sound(SoundKind::Fire, at));
            actions
        }
        SheepKind::Frozen => {
            let mut actions = disc(at.cell(), catalog::FROZEN_ICE_RADIUS, -1, BlockKind::Ice);
            actions.push(Action::Effect {
                team: enemies,
                center: at,
                radius: catalog::FROZEN_RADIUS,
                effect: EffectKind::Slowness,
                duration_ticks: catalog::FROZEN_SLOWNESS_TICKS,
                amplifier: catalog::FROZEN_SLOWNESS_AMPLIFIER,
            });
            actions.push(particle(ParticleKind::Snowflake, at, 80));
            actions.push(sound(SoundKind::Ice, at));
            actions
        }
        SheepKind::Seeker => vec![
            Action::Explode {
                at,
                power: catalog::SEEKER_POWER,
                breaks_blocks: false,
            },
            particle(ParticleKind::Crit, at, 30),
        ],
        SheepKind::Lightning => vec![Action::Lightning { at }, sound(SoundKind::Thunder, at)],
        SheepKind::Healing => vec![
            Action::Effect {
                team: thrower_team,
                center: at,
                radius: catalog::HEALING_RADIUS,
                effect: EffectKind::Regeneration,
                duration_ticks: catalog::HEALING_REGENERATION_TICKS,
                amplifier: catalog::HEALING_REGENERATION_AMPLIFIER,
            },
            particle(ParticleKind::Heart, at, 20),
            sound(SoundKind::Heal, at),
        ],
        SheepKind::Boarding => vec![
            Action::TeleportThrower { to: at },
            particle(ParticleKind::Portal, at, 50),
            sound(SoundKind::Teleport, at),
        ],
        SheepKind::Quake => vec![
            Action::Velocity {
                team: enemies,
                center: at,
                radius: catalog::QUAKE_RADIUS,
                velocity: Vec3 {
                    x: 0.0,
                    y: catalog::QUAKE_LIFT,
                    z: 0.0,
                },
            },
            particle(ParticleKind::Explosion, at, 10),
            sound(SoundKind::Quake, at),
        ],
        SheepKind::Darkness => vec![
            Action::Effect {
                team: enemies,
                center: at,
                radius: catalog::DARKNESS_RADIUS,
                effect: EffectKind::Blindness,
                duration_ticks: catalog::DARKNESS_BLINDNESS_TICKS,
                amplifier: catalog::DARKNESS_BLINDNESS_AMPLIFIER,
            },
            particle(ParticleKind::LargeSmoke, at, 60),
            sound(SoundKind::Darkness, at),
        ],
        SheepKind::Splitting => {
            let mut actions: Vec<Action> = (0..catalog::SPLIT_COUNT)
                .map(|index| {
                    let angle = TAU * f64::from(index) / f64::from(catalog::SPLIT_COUNT);
                    Action::SpawnSheep {
                        kind: catalog::SPLIT_CHILD,
                        at,
                        velocity: Vec3 {
                            x: angle.cos() * catalog::SPLIT_HORIZONTAL_SPEED,
                            y: catalog::SPLIT_VERTICAL_SPEED,
                            z: angle.sin() * catalog::SPLIT_HORIZONTAL_SPEED,
                        },
                    }
                })
                .collect();
            actions.push(sound(SoundKind::Split, at));
            actions
        }
    }
}

fn particle(kind: ParticleKind, at: Vec3, count: i32) -> Action {
    Action::Particle { kind, at, count }
}

fn sound(kind: SoundKind, at: Vec3) -> Action {
    Action::Sound { kind, at }
}

fn disc(center: Cell, radius: i32, dy: i32, block: BlockKind) -> Vec<Action> {
    let mut actions = Vec::new();
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            if dx * dx + dz * dz <= radius * radius {
                actions.push(Action::SetBlock {
                    at: Cell {
                        x: center.x + dx,
                        y: center.y + dy,
                        z: center.z + dz,
                    },
                    block,
                });
            }
        }
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    const AT: Vec3 = Vec3 {
        x: 10.5,
        y: 64.0,
        z: -3.5,
    };

    fn plans() -> Vec<(SheepKind, Vec<Action>)> {
        SheepKind::ALL
            .into_iter()
            .map(|kind| (kind, plan(kind, AT, TeamId::Red)))
            .collect()
    }

    #[test]
    fn each_of_the_ten_kinds_has_its_own_non_empty_plan() {
        let plans = plans();
        assert_eq!(plans.len(), 10);
        for (index, (kind, actions)) in plans.iter().enumerate() {
            assert!(!actions.is_empty(), "{kind:?} has an empty plan");
            for (other, other_actions) in &plans[index + 1..] {
                assert_ne!(actions, other_actions, "{kind:?} equals {other:?}");
            }
        }
    }

    #[test]
    fn explosive_explodes_at_the_impact_point() {
        assert!(
            plan(SheepKind::Explosive, AT, TeamId::Red).contains(&Action::Explode {
                at: AT,
                power: catalog::EXPLOSIVE_POWER,
                breaks_blocks: catalog::EXPLOSIVE_BREAKS_BLOCKS,
            })
        );
    }

    #[test]
    fn incendiary_sets_fire_on_a_disc_around_the_impact() {
        let fires = plan(SheepKind::Incendiary, AT, TeamId::Red)
            .into_iter()
            .filter(|a| {
                matches!(
                    a,
                    Action::SetBlock {
                        block: BlockKind::Fire,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(fires, 29);
    }

    #[test]
    fn frozen_slows_the_enemy_team_only() {
        let actions = plan(SheepKind::Frozen, AT, TeamId::Red);
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Effect {
                team: TeamId::Blue,
                effect: EffectKind::Slowness,
                ..
            }
        )));
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Effect {
                team: TeamId::Red,
                ..
            }
        )));
    }

    #[test]
    fn healing_targets_the_thrower_team() {
        assert!(
            plan(SheepKind::Healing, AT, TeamId::Blue)
                .iter()
                .any(|a| matches!(
                    a,
                    Action::Effect {
                        team: TeamId::Blue,
                        effect: EffectKind::Regeneration,
                        ..
                    }
                ))
        );
    }

    #[test]
    fn boarding_teleports_the_thrower_to_the_impact() {
        assert!(
            plan(SheepKind::Boarding, AT, TeamId::Red)
                .contains(&Action::TeleportThrower { to: AT })
        );
    }

    #[test]
    fn lightning_strikes_at_the_impact() {
        assert!(
            plan(SheepKind::Lightning, AT, TeamId::Red).contains(&Action::Lightning { at: AT })
        );
    }

    #[test]
    fn quake_lifts_enemies_and_darkness_blinds_them() {
        assert!(
            plan(SheepKind::Quake, AT, TeamId::Red)
                .iter()
                .any(|a| matches!(
                    a,
                    Action::Velocity { team: TeamId::Blue, velocity, .. } if velocity.y > 0.0
                ))
        );
        assert!(
            plan(SheepKind::Darkness, AT, TeamId::Red)
                .iter()
                .any(|a| matches!(
                    a,
                    Action::Effect {
                        team: TeamId::Blue,
                        effect: EffectKind::Blindness,
                        ..
                    }
                ))
        );
    }

    #[test]
    fn splitting_spawns_the_configured_number_of_children_in_distinct_directions() {
        let children: Vec<Vec3> = plan(SheepKind::Splitting, AT, TeamId::Red)
            .into_iter()
            .filter_map(|a| match a {
                Action::SpawnSheep { kind, velocity, .. } if kind == catalog::SPLIT_CHILD => {
                    Some(velocity)
                }
                _ => None,
            })
            .collect();
        assert_eq!(children.len(), catalog::SPLIT_COUNT as usize);
        for (index, child) in children.iter().enumerate() {
            assert!(!children[index + 1..].contains(child));
        }
    }
}
