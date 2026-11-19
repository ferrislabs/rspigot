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
    Ice,
    Thunder,
    Heal,
    Teleport,
    Quake,
    Darkness,
    Split,
    Warp,
    Intergalactic,
    Hiss,
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
    Push {
        team: TeamId,
        center: Vec3,
        radius: f64,
        strength: f64,
        lift: f64,
    },
    Lightning {
        at: Vec3,
    },
    SpawnSheep {
        kind: SheepKind,
        at: Vec3,
        velocity: Vec3,
    },
}

pub fn plan_pulse(at: Vec3) -> Vec<Action> {
    vec![
        particle(ParticleKind::LargeSmoke, at, catalog::PULSE_PARTICLES),
        sound(SoundKind::Hiss, at),
    ]
}

pub fn plan(kind: SheepKind, at: Vec3, thrower_team: TeamId) -> Vec<Action> {
    let enemies = thrower_team.other();
    match kind {
        SheepKind::Incendiary => vec![
            Action::Explode {
                at,
                power: catalog::INCENDIARY_POWER,
                breaks_blocks: catalog::INCENDIARY_BREAKS_BLOCKS,
            },
            particle(ParticleKind::Explosion, at, 5),
            particle(ParticleKind::Flame, at, 60),
            sound(SoundKind::Explode, at),
        ],
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
            let mut actions = vec![Action::Explode {
                at,
                power: catalog::SPLIT_PARENT_POWER,
                breaks_blocks: false,
            }];
            actions.extend((0..catalog::SPLIT_COUNT).map(|index| {
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
            }));
            actions.push(sound(SoundKind::Split, at));
            actions
        }
        SheepKind::Warp => vec![
            Action::Push {
                team: enemies,
                center: at,
                radius: catalog::WARP_RADIUS,
                strength: catalog::WARP_PUSH,
                lift: catalog::WARP_LIFT,
            },
            Action::Explode {
                at,
                power: catalog::WARP_BLAST_POWER,
                breaks_blocks: catalog::WARP_BLAST_BREAKS_BLOCKS,
            },
            particle(ParticleKind::Portal, at, 80),
            sound(SoundKind::Warp, at),
        ],
        SheepKind::Intergalactic => {
            let mut actions: Vec<Action> = (0..catalog::INTERGALACTIC_METEORS)
                .map(|index| Action::Explode {
                    at: meteor_point(at, index),
                    power: catalog::INTERGALACTIC_POWER,
                    breaks_blocks: catalog::INTERGALACTIC_BREAKS_BLOCKS,
                })
                .collect();
            actions.push(Action::Lightning { at });
            actions.push(particle(ParticleKind::Explosion, at, 20));
            actions.push(sound(SoundKind::Intergalactic, at));
            actions
        }
    }
}

const GOLDEN_ANGLE: f64 = 2.399_963_229_728_653;

fn meteor_point(center: Vec3, index: u32) -> Vec3 {
    let share = f64::from(index + 1) / f64::from(catalog::INTERGALACTIC_METEORS);
    let distance = catalog::INTERGALACTIC_SPREAD * share.sqrt();
    let angle = GOLDEN_ANGLE * f64::from(index);
    Vec3 {
        x: center.x + angle.cos() * distance,
        y: center.y,
        z: center.z + angle.sin() * distance,
    }
}

pub fn outward_velocity(center: Vec3, target: Vec3, strength: f64, lift: f64) -> Vec3 {
    let (dx, dz) = (target.x - center.x, target.z - center.z);
    let length = dx.hypot(dz);
    if length <= f64::EPSILON {
        return Vec3 {
            x: 0.0,
            y: lift,
            z: 0.0,
        };
    }
    Vec3 {
        x: dx / length * strength,
        y: lift,
        z: dz / length * strength,
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
    fn each_of_the_eleven_kinds_has_its_own_non_empty_plan() {
        let plans = plans();
        assert_eq!(plans.len(), 11);
        for (index, (kind, actions)) in plans.iter().enumerate() {
            assert!(!actions.is_empty(), "{kind:?} has an empty plan");
            for (other, other_actions) in &plans[index + 1..] {
                assert_ne!(actions, other_actions, "{kind:?} equals {other:?}");
            }
        }
    }

    #[test]
    fn incendiary_is_a_large_block_breaking_explosion_without_fire() {
        let actions = plan(SheepKind::Incendiary, AT, TeamId::Red);
        assert!(actions.contains(&Action::Explode {
            at: AT,
            power: catalog::INCENDIARY_POWER,
            breaks_blocks: catalog::INCENDIARY_BREAKS_BLOCKS,
        }));
        assert!(!actions.iter().any(|a| matches!(a, Action::SetBlock { .. })));
        const { assert!(catalog::INCENDIARY_POWER > catalog::SEEKER_POWER) };
    }

    #[test]
    fn warp_pushes_enemies_away_and_blasts_the_impact() {
        let actions = plan(SheepKind::Warp, AT, TeamId::Red);
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Push {
                team: TeamId::Blue,
                ..
            }
        )));
        assert!(actions.contains(&Action::Explode {
            at: AT,
            power: catalog::WARP_BLAST_POWER,
            breaks_blocks: catalog::WARP_BLAST_BREAKS_BLOCKS,
        }));
    }

    #[test]
    fn outward_velocity_points_away_from_the_center() {
        let v = outward_velocity(
            Vec3::ZERO,
            Vec3 {
                x: 3.0,
                y: 0.0,
                z: 0.0,
            },
            2.0,
            0.5,
        );
        assert_eq!(
            v,
            Vec3 {
                x: 2.0,
                y: 0.5,
                z: 0.0
            }
        );
        let on_center = outward_velocity(Vec3::ZERO, Vec3::ZERO, 2.0, 0.5);
        assert_eq!(
            on_center,
            Vec3 {
                x: 0.0,
                y: 0.5,
                z: 0.0
            }
        );
    }

    #[test]
    fn intergalactic_drops_distinct_lethal_meteors_within_the_spread() {
        let actions = plan(SheepKind::Intergalactic, AT, TeamId::Red);
        let points: Vec<Vec3> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Explode { at, power, .. } if *power >= catalog::INTERGALACTIC_POWER => {
                    Some(*at)
                }
                _ => None,
            })
            .collect();
        assert_eq!(points.len(), catalog::INTERGALACTIC_METEORS as usize);
        for (index, point) in points.iter().enumerate() {
            assert!(!points[index + 1..].contains(point));
            let distance = (point.x - AT.x).hypot(point.z - AT.z);
            assert!(distance <= catalog::INTERGALACTIC_SPREAD + 1e-9);
        }
        assert!(actions.contains(&Action::Sound {
            kind: SoundKind::Intergalactic,
            at: AT
        }));
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
    fn boarding_only_shows_particles_and_a_sound() {
        let actions = plan(SheepKind::Boarding, AT, TeamId::Red);
        assert!(actions.iter().any(|a| matches!(a, Action::Particle { .. })));
        assert!(actions.iter().any(|a| matches!(a, Action::Sound { .. })));
        assert!(
            actions
                .iter()
                .all(|a| matches!(a, Action::Particle { .. } | Action::Sound { .. }))
        );
    }

    #[test]
    fn a_pulse_is_a_particle_and_a_hiss() {
        let actions = plan_pulse(AT);
        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0], Action::Particle { at, .. } if at == AT));
        assert!(matches!(
            actions[1],
            Action::Sound {
                kind: SoundKind::Hiss,
                ..
            }
        ));
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
    fn splitting_also_explodes_itself() {
        assert!(
            plan(SheepKind::Splitting, AT, TeamId::Red)
                .iter()
                .any(|a| matches!(a, Action::Explode { at, .. } if *at == AT))
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
