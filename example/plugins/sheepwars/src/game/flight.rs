use rspigot_plugin::Material;

use crate::domain::{PlayerId, SheepKind, TeamId};
use crate::sheep::{
    INCENDIARY_SCALE_END, INCENDIARY_SCALE_INTERVAL_TICKS, INCENDIARY_SCALE_START,
    PULSE_INTERVAL_TICKS, SEEKER_CONTACT_RADIUS, SEEKER_HOP, SEEKER_RUN_SPEED, SEEKER_SPEED,
    SPLIT_CHILD_FUSE_TICKS, Vec3, fuse_ticks, homes_in_flight, sheep_wool, steer,
};

pub const MAX_CONCURRENT_FLIGHTS: usize = 32;
pub const MAX_FLIGHT_TICKS: u32 = 20 * 15;
pub const LANDING_GRACE_TICKS: u32 = 2;
pub const EYE_HEIGHT: f64 = 1.62;
pub const BOARDING_RIDER_OFFSET: f64 = 1.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Flying,
    Landed { ticks: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flight {
    pub kind: SheepKind,
    pub team: TeamId,
    pub thrower: PlayerId,
    pub age: u32,
    pub stage: Stage,
    pub fuse: u32,
    pub fragment: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Observation {
    pub on_ground: bool,
    pub position: Vec3,
    pub vertical_speed: f64,
    pub enemy: Option<Vec3>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    Keep,
    Swell(f64),
    Steer(Vec3),
    Carry(Vec3),
    Detonate,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outcome {
    pub step: Step,
    pub pulse: bool,
}

impl Outcome {
    const fn quiet(step: Step) -> Self {
        Self { step, pulse: false }
    }
}

impl Flight {
    pub fn new(kind: SheepKind, team: TeamId, thrower: PlayerId) -> Self {
        Self::with_fuse(kind, team, thrower, fuse_ticks(kind))
    }

    pub const fn with_fuse(kind: SheepKind, team: TeamId, thrower: PlayerId, fuse: u32) -> Self {
        Self {
            kind,
            team,
            thrower,
            age: 0,
            stage: Stage::Flying,
            fuse,
            fragment: false,
        }
    }

    pub const fn fragment(kind: SheepKind, team: TeamId, thrower: PlayerId) -> Self {
        Self {
            fragment: true,
            ..Self::with_fuse(kind, team, thrower, SPLIT_CHILD_FUSE_TICKS)
        }
    }

    pub const fn is_detonating(&self) -> bool {
        match self.stage {
            Stage::Flying => false,
            Stage::Landed { ticks } => {
                ticks >= self.fuse || matches!(self.kind, SheepKind::Boarding)
            }
        }
    }

    pub const fn should_land(&self, on_ground: bool) -> bool {
        (on_ground && self.age >= LANDING_GRACE_TICKS) || self.age >= MAX_FLIGHT_TICKS
    }

    pub fn step(&mut self, observation: Observation) -> Outcome {
        match self.stage {
            Stage::Flying => self.fly(observation),
            Stage::Landed { ticks } => self.wait(ticks + 1, observation),
        }
    }

    fn fly(&mut self, observation: Observation) -> Outcome {
        self.age += 1;
        if self.should_land(observation.on_ground) {
            self.stage = Stage::Landed { ticks: 0 };
            let step = if self.kind == SheepKind::Boarding {
                Step::Release
            } else {
                Step::Keep
            };
            return Outcome::quiet(step);
        }
        if self.kind == SheepKind::Boarding {
            return Outcome::quiet(Step::Carry(observation.position));
        }
        let step = match observation.enemy {
            Some(target) if homes_in_flight(self.kind) => {
                Step::Steer(steer(observation.position, target, SEEKER_SPEED))
            }
            _ => Step::Keep,
        };
        Outcome::quiet(step)
    }

    fn wait(&mut self, ticks: u32, observation: Observation) -> Outcome {
        self.stage = Stage::Landed { ticks };
        let pulse = ticks < self.fuse && ticks.is_multiple_of(PULSE_INTERVAL_TICKS);
        let step = match self.kind {
            SheepKind::Boarding => Step::Release,
            SheepKind::Incendiary => self.swell(ticks),
            SheepKind::Seeker => self.run(ticks, observation),
            _ if ticks >= self.fuse => Step::Detonate,
            _ => Step::Keep,
        };
        Outcome { step, pulse }
    }

    fn swell(&self, ticks: u32) -> Step {
        if ticks >= self.fuse {
            Step::Detonate
        } else if ticks.is_multiple_of(INCENDIARY_SCALE_INTERVAL_TICKS) {
            let progress = f64::from(ticks) / f64::from(self.fuse.max(1));
            Step::Swell(
                (INCENDIARY_SCALE_END - INCENDIARY_SCALE_START)
                    .mul_add(progress, INCENDIARY_SCALE_START),
            )
        } else {
            Step::Keep
        }
    }

    fn run(&self, ticks: u32, observation: Observation) -> Step {
        let Some(target) = observation.enemy else {
            return if ticks >= self.fuse {
                Step::Detonate
            } else {
                Step::Keep
            };
        };
        let from = observation.position;
        let (dx, dy, dz) = (target.x - from.x, target.y - from.y, target.z - from.z);
        let distance = (dx * dx + dy * dy + dz * dz).sqrt();
        if distance <= SEEKER_CONTACT_RADIUS || ticks >= self.fuse {
            return Step::Detonate;
        }
        let flat = dx.hypot(dz);
        let (x, z) = if flat <= f64::EPSILON {
            (0.0, 0.0)
        } else {
            (dx / flat * SEEKER_RUN_SPEED, dz / flat * SEEKER_RUN_SPEED)
        };
        let y = if observation.on_ground {
            SEEKER_HOP
        } else {
            observation.vertical_speed
        };
        Step::Steer(Vec3 { x, y, z })
    }
}

pub fn kind_of(material: Material) -> Option<SheepKind> {
    SheepKind::ALL
        .into_iter()
        .find(|kind| sheep_wool(*kind) == material)
}

pub const fn can_launch(active: usize) -> bool {
    active < MAX_CONCURRENT_FLIGHTS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheep::{INCENDIARY_FUSE_TICKS, SEEKER_RUN_TICKS, SPLIT_CHILD_FUSE_TICKS};

    fn flight(age: u32) -> Flight {
        Flight {
            age,
            ..Flight::new(SheepKind::Lightning, TeamId::Red, PlayerId(1))
        }
    }

    const HERE: Vec3 = Vec3 {
        x: 0.0,
        y: 64.0,
        z: 0.0,
    };

    fn enemy_at(distance: f64) -> Vec3 {
        Vec3 {
            x: distance,
            y: 64.0,
            z: 0.0,
        }
    }

    fn observe(on_ground: bool, enemy: Option<Vec3>) -> Observation {
        Observation {
            on_ground,
            position: HERE,
            vertical_speed: -0.2,
            enemy,
        }
    }

    fn landed(kind: SheepKind) -> Flight {
        let mut flight = Flight::new(kind, TeamId::Red, PlayerId(1));
        flight.age = LANDING_GRACE_TICKS;
        let outcome = flight.step(observe(true, None));
        assert_eq!(flight.stage, Stage::Landed { ticks: 0 });
        assert_eq!(outcome.step, Step::Keep);
        flight
    }

    #[test]
    fn landing_never_detonates_on_the_landing_tick() {
        for kind in SheepKind::ALL {
            if kind == SheepKind::Boarding {
                continue;
            }
            let mut flight = Flight::new(kind, TeamId::Red, PlayerId(1));
            flight.age = MAX_FLIGHT_TICKS;
            let outcome = flight.step(observe(true, Some(enemy_at(0.1))));
            assert_ne!(outcome.step, Step::Detonate, "{kind:?}");
            assert_eq!(flight.stage, Stage::Landed { ticks: 0 });
        }
    }

    #[test]
    fn the_fuse_counts_down_only_after_landing() {
        let mut flight = Flight::new(SheepKind::Healing, TeamId::Red, PlayerId(1));
        for _ in 0..LANDING_GRACE_TICKS - 1 {
            assert_eq!(flight.step(observe(true, None)).step, Step::Keep);
            assert_eq!(flight.stage, Stage::Flying);
        }
        for _ in 0..10 {
            flight.step(observe(false, None));
            assert_eq!(flight.stage, Stage::Flying);
        }
        flight.step(observe(true, None));
        assert_eq!(flight.stage, Stage::Landed { ticks: 0 });
        for tick in 1..flight.fuse {
            let outcome = flight.step(observe(true, None));
            assert_eq!(outcome.step, Step::Keep, "tick {tick}");
        }
        assert_eq!(flight.step(observe(true, None)).step, Step::Detonate);
    }

    #[test]
    fn the_default_fuse_pulses_every_interval_until_the_end() {
        let mut flight = landed(SheepKind::Frozen);
        let pulses: Vec<u32> = (1..=flight.fuse)
            .filter(|_| flight.step(observe(true, None)).pulse)
            .collect();
        assert_eq!(pulses, vec![10, 20, 30]);
    }

    #[test]
    fn splitting_children_use_the_short_fuse() {
        let mut child = Flight::with_fuse(
            SheepKind::Healing,
            TeamId::Red,
            PlayerId(1),
            SPLIT_CHILD_FUSE_TICKS,
        );
        child.age = LANDING_GRACE_TICKS;
        child.step(observe(true, None));
        for _ in 1..SPLIT_CHILD_FUSE_TICKS {
            assert_eq!(child.step(observe(true, None)).step, Step::Keep);
        }
        assert_eq!(child.step(observe(true, None)).step, Step::Detonate);
    }

    #[test]
    fn the_seeker_homes_while_flying() {
        let mut flight = Flight::new(SheepKind::Seeker, TeamId::Red, PlayerId(1));
        let outcome = flight.step(observe(false, Some(enemy_at(10.0))));
        let Step::Steer(velocity) = outcome.step else {
            panic!("expected a steer, got {:?}", outcome.step);
        };
        assert!((velocity.x - SEEKER_SPEED).abs() < 1e-9);
        assert_eq!(flight.stage, Stage::Flying);
    }

    #[test]
    fn the_landed_seeker_runs_toward_the_nearest_enemy_and_hops() {
        let mut flight = landed(SheepKind::Seeker);
        let Step::Steer(velocity) = flight.step(observe(true, Some(enemy_at(10.0)))).step else {
            panic!("expected a steer");
        };
        assert!((velocity.x - SEEKER_RUN_SPEED).abs() < 1e-9);
        assert!(velocity.z.abs() < 1e-9);
        assert!((velocity.y - SEEKER_HOP).abs() < 1e-9);
        let Step::Steer(airborne) = flight.step(observe(false, Some(enemy_at(10.0)))).step else {
            panic!("expected a steer");
        };
        assert!((airborne.y - -0.2).abs() < 1e-9);
    }

    #[test]
    fn the_seeker_detonates_at_the_contact_radius() {
        let mut flight = landed(SheepKind::Seeker);
        let far = enemy_at(SEEKER_CONTACT_RADIUS + 0.5);
        assert!(matches!(
            flight.step(observe(true, Some(far))).step,
            Step::Steer(_)
        ));
        let near = enemy_at(SEEKER_CONTACT_RADIUS);
        assert_eq!(flight.step(observe(true, Some(near))).step, Step::Detonate);
    }

    #[test]
    fn the_seeker_without_enemy_waits_then_detonates_at_the_max_time() {
        let mut flight = landed(SheepKind::Seeker);
        for _ in 1..SEEKER_RUN_TICKS {
            assert_eq!(flight.step(observe(true, None)).step, Step::Keep);
        }
        assert_eq!(flight.step(observe(true, None)).step, Step::Detonate);
    }

    #[test]
    fn the_running_seeker_detonates_at_the_max_time_too() {
        let mut flight = landed(SheepKind::Seeker);
        for _ in 1..SEEKER_RUN_TICKS {
            assert!(matches!(
                flight.step(observe(true, Some(enemy_at(15.0)))).step,
                Step::Steer(_)
            ));
        }
        assert_eq!(
            flight.step(observe(true, Some(enemy_at(15.0)))).step,
            Step::Detonate
        );
    }

    #[test]
    fn the_incendiary_swells_monotonically_then_detonates() {
        let mut flight = landed(SheepKind::Incendiary);
        let mut scales = Vec::new();
        let mut last = Step::Keep;
        for _ in 1..=flight.fuse {
            last = flight.step(observe(true, None)).step;
            if let Step::Swell(scale) = last {
                scales.push(scale);
            }
        }
        assert_eq!(last, Step::Detonate);
        assert!(!scales.is_empty());
        assert!(scales.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(scales[0] > INCENDIARY_SCALE_START);
        assert!(*scales.last().unwrap() < INCENDIARY_SCALE_END);
        assert!(scales.last().unwrap() > &(INCENDIARY_SCALE_END * 0.9));
    }

    #[test]
    fn the_incendiary_updates_its_scale_every_interval() {
        let mut flight = landed(SheepKind::Incendiary);
        let swells = (1..flight.fuse)
            .filter(|_| matches!(flight.step(observe(true, None)).step, Step::Swell(_)))
            .count();
        assert_eq!(
            swells as u32,
            (INCENDIARY_FUSE_TICKS - 1) / INCENDIARY_SCALE_INTERVAL_TICKS
        );
    }

    #[test]
    fn the_boarding_sheep_carries_its_rider_then_releases_without_detonating() {
        let mut flight = Flight::new(SheepKind::Boarding, TeamId::Red, PlayerId(1));
        assert_eq!(flight.step(observe(false, None)).step, Step::Carry(HERE));
        assert_eq!(flight.step(observe(false, None)).step, Step::Carry(HERE));
        let outcome = flight.step(observe(true, None));
        assert_eq!(outcome.step, Step::Release);
        assert_eq!(flight.stage, Stage::Landed { ticks: 0 });
    }

    #[test]
    fn a_boarding_sheep_in_the_air_for_too_long_releases() {
        let mut flight = Flight::new(SheepKind::Boarding, TeamId::Red, PlayerId(1));
        flight.age = MAX_FLIGHT_TICKS;
        assert_eq!(flight.step(observe(false, None)).step, Step::Release);
    }

    #[test]
    fn every_sheep_item_maps_back_to_its_kind() {
        for kind in SheepKind::ALL {
            assert_eq!(kind_of(sheep_wool(kind)), Some(kind));
        }
    }

    #[test]
    fn other_materials_are_not_sheep() {
        assert_eq!(kind_of(Material::Compass), None);
    }

    #[test]
    fn lands_on_the_ground_after_the_grace() {
        assert!(!flight(0).should_land(true));
        assert!(flight(LANDING_GRACE_TICKS).should_land(true));
        assert!(!flight(LANDING_GRACE_TICKS).should_land(false));
    }

    #[test]
    fn lands_when_the_lifetime_is_over() {
        assert!(flight(MAX_FLIGHT_TICKS).should_land(false));
    }

    #[test]
    fn flights_are_capped() {
        assert!(can_launch(MAX_CONCURRENT_FLIGHTS - 1));
        assert!(!can_launch(MAX_CONCURRENT_FLIGHTS));
    }
}
