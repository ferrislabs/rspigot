use crate::random::RandomSource;
use crate::{BlockPos, EntityId, Vec3, mth};

use super::path::{DragonNode, DragonNodes, DragonPath};

const DEG_TO_RAD: f32 = 0.017_453_292;
const RAD_TO_DEG: f64 = 57.295_776_367_187_5;
const REACHED_SQR: f64 = 100.0;
const LOST_SQR: f64 = 22_500.0;
const FIREBALL_RANGE_SQR: f64 = 4_096.0;
const FIREBALL_CHARGE: i32 = 4;
const FIREBALL_ARC: f32 = 10.0;
const FIREBALL_EVENT: i32 = 1017;
const CHARGE_DURATION: i32 = 9;
const ATTACKING_TICKS: i32 = 39;
const FLAME_TICKS: i32 = 200;
const FLAME_START: i32 = 10;
const FLAME_REPEATS: i32 = 4;
const FLAME_REACH: f64 = 5.0;
const SCANNING_ALIGN_TICKS: i32 = 26;
const SCANNING_GIVE_UP_TICKS: i32 = 99;
const SCAN_RANGE: f64 = 20.0;
const SCAN_HEIGHT: f64 = 10.0;
const CHARGE_RANGE: f64 = 150.0;
const WAYPOINT_HEIGHT_RANGE: f32 = 20.0;
const BEHIND_FAR: f64 = 40.0;
const BEHIND_HEIGHT: f64 = 105.0;
const NEW_TARGET_FAR_DISTANCE: f64 = 64.0;
const LANDING_REACHED_SQR: f64 = 1.0;
const TAKEOFF_CLEAR_DISTANCE: f64 = 10.0;
const FLY_SLIDE_BASE: f64 = 0.8;
const FLY_SLIDE_RANGE: f64 = 0.15;
const FLY_VERTICAL_DRAG: f64 = 0.910_000_026_226_043_7;
const FLY_IN_WALL_SCALE: f64 = 0.800_000_011_920_929;
const FLY_ACCELERATION: f32 = 0.06;
const FLY_YAW_LIMIT: f32 = 50.0;
const FLY_VERTICAL_PULL: f64 = 0.01;
const ZERO_EPSILON: f32 = 1.0e-5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DragonPhase {
    HoldingPattern,
    StrafePlayer,
    LandingApproach,
    Landing,
    Takeoff,
    SittingFlaming,
    SittingScanning,
    SittingAttacking,
    ChargingPlayer,
    Dying,
    Hovering,
}

impl DragonPhase {
    pub const ALL: [Self; 11] = [
        Self::HoldingPattern,
        Self::StrafePlayer,
        Self::LandingApproach,
        Self::Landing,
        Self::Takeoff,
        Self::SittingFlaming,
        Self::SittingScanning,
        Self::SittingAttacking,
        Self::ChargingPlayer,
        Self::Dying,
        Self::Hovering,
    ];

    pub const fn id(self) -> i32 {
        match self {
            Self::HoldingPattern => 0,
            Self::StrafePlayer => 1,
            Self::LandingApproach => 2,
            Self::Landing => 3,
            Self::Takeoff => 4,
            Self::SittingFlaming => 5,
            Self::SittingScanning => 6,
            Self::SittingAttacking => 7,
            Self::ChargingPlayer => 8,
            Self::Dying => 9,
            Self::Hovering => 10,
        }
    }

    pub fn from_id(id: i32) -> Option<Self> {
        usize::try_from(id)
            .ok()
            .and_then(|index| Self::ALL.get(index).copied())
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::HoldingPattern => "HoldingPattern",
            Self::StrafePlayer => "StrafePlayer",
            Self::LandingApproach => "LandingApproach",
            Self::Landing => "Landing",
            Self::Takeoff => "Takeoff",
            Self::SittingFlaming => "SittingFlaming",
            Self::SittingScanning => "SittingScanning",
            Self::SittingAttacking => "SittingAttacking",
            Self::ChargingPlayer => "ChargingPlayer",
            Self::Dying => "Dying",
            Self::Hovering => "Hover",
        }
    }

    pub const fn is_sitting(self) -> bool {
        matches!(
            self,
            Self::SittingFlaming | Self::SittingScanning | Self::SittingAttacking | Self::Hovering
        )
    }

    pub const fn fly_speed(self) -> f32 {
        match self {
            Self::ChargingPlayer | Self::Dying => 3.0,
            Self::Hovering => 1.0,
            Self::Landing => 1.5,
            _ => 0.6,
        }
    }

    pub fn turn_speed(self, horizontal_speed: f64) -> f32 {
        let speed = horizontal_speed as f32 + 1.0;
        let capped = speed.min(40.0);
        match self {
            Self::Landing => capped / speed,
            _ => 0.7 / capped / speed,
        }
    }

    pub const fn sitting_attack_immune(self) -> bool {
        matches!(
            self,
            Self::SittingFlaming | Self::SittingScanning | Self::SittingAttacking
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ground {
    NoLeaves,
    Blocking,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Targeting {
    NewTarget,
    NearEgg,
    Scan,
    Charge,
}

impl Targeting {
    pub const fn range(self) -> f64 {
        match self {
            Self::NewTarget | Self::NearEgg => -1.0,
            Self::Scan => SCAN_RANGE,
            Self::Charge => CHARGE_RANGE,
        }
    }

    pub const fn height_limit(self) -> Option<f64> {
        match self {
            Self::Scan => Some(SCAN_HEIGHT),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Foe {
    pub id: EntityId,
    pub position: Vec3,
    pub height: f64,
}

impl Foe {
    pub fn y_at(&self, fraction: f64) -> f64 {
        self.position.y + self.height * fraction
    }
}

pub trait DragonEnv: RandomSource {
    fn ground(&self, ground: Ground, x: i32, z: i32) -> i32;
    fn nearest_player(&self, targeting: Targeting, near: Vec3) -> Option<Foe>;
    fn foe(&self, id: EntityId) -> Option<Foe>;
    fn has_line_of_sight(&self, id: EntityId) -> bool;
    fn is_empty_block(&self, pos: BlockPos) -> bool;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragonBody {
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub y_rot_a: f32,
    pub head: Vec3,
    pub silent: bool,
    pub origin: BlockPos,
}

impl DragonBody {
    pub fn view_vector(&self) -> Vec3 {
        mth::view_vector(0.0, self.yaw)
    }

    pub fn head_look_vector(&self, phase: DragonPhase, podium_ground: BlockPos) -> Vec3 {
        if matches!(phase, DragonPhase::Landing | DragonPhase::Takeoff) {
            let center = Vec3::new(
                f64::from(podium_ground.x) + 0.5,
                f64::from(podium_ground.y) + 0.5,
                f64::from(podium_ground.z) + 0.5,
            );
            let distance = center.distance_squared(self.position).sqrt();
            let scale = 6.0 / (distance as f32 / 4.0).max(1.0);
            return mth::view_vector(-scale * 1.5 * 5.0, self.yaw);
        }
        if phase.is_sitting() {
            return mth::view_vector(-45.0, self.yaw);
        }
        self.view_vector()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragonAction {
    LevelEvent { event: i32, pos: BlockPos },
    Fireball { from: Vec3, direction: Vec3 },
    SetHealth(f32),
    SpawnFlame { position: Vec3 },
    DiscardFlame,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Followup {
    StrafeTarget(Foe),
    ChargeTarget(Vec3),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhaseChange {
    pub to: DragonPhase,
    pub then: Option<Followup>,
}

impl PhaseChange {
    pub const fn to(to: DragonPhase) -> Self {
        Self { to, then: None }
    }
}

#[derive(Debug, Default)]
pub struct PhaseOutput {
    pub changes: Vec<PhaseChange>,
    pub actions: Vec<DragonAction>,
}

#[derive(Debug, Default)]
pub struct Holding {
    pub target: Option<Vec3>,
    pub path: Option<DragonPath>,
    pub clockwise: bool,
}

#[derive(Debug, Default)]
pub struct Strafe {
    pub target: Option<Vec3>,
    pub path: Option<DragonPath>,
    pub clockwise: bool,
    pub fireball_charge: i32,
    pub attack_target: Option<EntityId>,
}

#[derive(Debug, Default)]
pub struct Approach {
    pub target: Option<Vec3>,
    pub path: Option<DragonPath>,
}

#[derive(Debug, Default)]
pub struct Landing {
    pub target: Option<Vec3>,
}

#[derive(Debug, Default)]
pub struct Takeoff {
    pub target: Option<Vec3>,
    pub path: Option<DragonPath>,
    pub first_tick: bool,
}

#[derive(Debug, Default)]
pub struct Flaming {
    pub flame_ticks: i32,
    pub flame_count: i32,
    pub flame: Option<EntityId>,
}

#[derive(Debug, Default)]
pub struct Charge {
    pub target: Option<Vec3>,
    pub time_since_charge: i32,
}

#[derive(Debug, Default)]
pub struct Death {
    pub target: Option<Vec3>,
    pub time: i32,
}

#[derive(Debug, Default)]
pub struct Hover {
    pub target: Option<Vec3>,
}

#[derive(Debug, Default)]
pub struct PhaseSet {
    pub holding: Holding,
    pub strafe: Strafe,
    pub approach: Approach,
    pub landing: Landing,
    pub takeoff: Takeoff,
    pub flaming: Flaming,
    pub scanning_time: i32,
    pub attacking_ticks: i32,
    pub charge: Charge,
    pub death: Death,
    pub hover: Hover,
}

#[derive(Debug)]
pub struct DragonPhases {
    current: DragonPhase,
    pub set: PhaseSet,
}

impl Default for DragonPhases {
    fn default() -> Self {
        let mut phases = Self {
            current: DragonPhase::Hovering,
            set: PhaseSet::default(),
        };
        phases.set.begin(DragonPhase::Hovering);
        phases
    }
}

impl DragonPhases {
    pub fn current(&self) -> DragonPhase {
        self.current
    }

    pub fn is_sitting(&self) -> bool {
        self.current.is_sitting()
    }

    pub fn set_phase(&mut self, phase: DragonPhase) -> Vec<DragonAction> {
        if self.current == phase {
            return Vec::new();
        }
        let actions = self.set.end(self.current);
        self.current = phase;
        self.set.begin(phase);
        actions
    }

    pub fn restore(&mut self, phase: DragonPhase) {
        self.current = phase;
        self.set.begin(phase);
    }

    pub fn fly_target(&self) -> Option<Vec3> {
        self.set.target(self.current)
    }

    pub fn set_flame(&mut self, flame: EntityId) {
        self.set.flaming.flame = Some(flame);
    }

    pub fn flame(&self) -> Option<EntityId> {
        self.set.flaming.flame
    }

    pub fn tick(
        &mut self,
        body: &mut DragonBody,
        nodes: &mut DragonNodes,
        crystals: Option<i32>,
        env: &mut impl DragonEnv,
        out: &mut PhaseOutput,
    ) {
        let mut tick = Tick {
            body,
            nodes,
            crystals,
            env,
            out,
        };
        self.set.tick(self.current, &mut tick);
    }

    pub fn apply(
        &mut self,
        followup: Followup,
        body: &mut DragonBody,
        nodes: &mut DragonNodes,
        crystals: Option<i32>,
        env: &mut impl DragonEnv,
    ) {
        match followup {
            Followup::ChargeTarget(target) => self.set.charge.target = Some(target),
            Followup::StrafeTarget(foe) => {
                let mut out = PhaseOutput::default();
                let mut tick = Tick {
                    body,
                    nodes,
                    crystals,
                    env,
                    out: &mut out,
                };
                self.set.set_strafe_target(foe, &mut tick);
            }
        }
    }
}

struct Tick<'a, E: DragonEnv> {
    body: &'a mut DragonBody,
    nodes: &'a mut DragonNodes,
    crystals: Option<i32>,
    env: &'a mut E,
    out: &'a mut PhaseOutput,
}

impl<E: DragonEnv> Tick<'_, E> {
    fn podium(&self) -> BlockPos {
        self.body.origin
    }

    fn ground_at_podium(&self, ground: Ground) -> BlockPos {
        let podium = self.podium();
        BlockPos::new(
            podium.x,
            self.env.ground(ground, podium.x, podium.z),
            podium.z,
        )
    }

    fn closest_node(&mut self) -> usize {
        let position = (
            self.body.position.x,
            self.body.position.y,
            self.body.position.z,
        );
        self.closest_node_at(position)
    }

    fn closest_node_at(&mut self, position: (f64, f64, f64)) -> usize {
        let env = &*self.env;
        self.nodes
            .find_closest_node(position, self.crystals, |x, z| {
                env.ground(Ground::NoLeaves, x, z)
            })
    }

    fn find_path(
        &mut self,
        start: usize,
        end: usize,
        last: Option<DragonNode>,
    ) -> Option<DragonPath> {
        self.nodes.find_path(start, end, last, self.crystals)
    }

    fn change(&mut self, to: DragonPhase) {
        self.out.changes.push(PhaseChange::to(to));
    }

    fn change_with(&mut self, to: DragonPhase, then: Followup) {
        self.out.changes.push(PhaseChange {
            to,
            then: Some(then),
        });
    }

    fn off_course(&self, target: Option<Vec3>) -> bool {
        let distance = target.map_or(0.0, |target| target.distance_squared(self.body.position));
        !(REACHED_SQR..=LOST_SQR).contains(&distance)
    }

    fn waypoint(&mut self, path: &mut Option<DragonPath>, advance_first: bool) -> Option<Vec3> {
        let path = path.as_mut()?;
        if advance_first {
            path.advance();
        }
        if path.is_done() {
            return None;
        }
        let next = path.next_node_pos()?;
        path.advance();
        let x = f64::from(next.x);
        let z = f64::from(next.z);
        loop {
            let y = f64::from(next.y as f32 + self.env.next_float() * WAYPOINT_HEIGHT_RANGE);
            if y >= f64::from(next.y) {
                return Some(Vec3::new(x, y, z));
            }
        }
    }

    fn ring_target(index: i32, whole_ring: bool) -> usize {
        let mut index = index;
        if whole_ring {
            index %= 12;
            if index < 0 {
                index += 12;
            }
        } else {
            index = ((index - 12) & 7) + 12;
        }
        index as usize
    }

    fn behind_node(&mut self, from: Vec3) -> usize {
        let direction = Vec3::new(from.x, 0.0, from.z).normalize();
        self.closest_node_at((
            -direction.x * BEHIND_FAR,
            BEHIND_HEIGHT,
            -direction.z * BEHIND_FAR,
        ))
    }

    fn alignment_angle(&self, foe: Vec3) -> f32 {
        let aim = Vec3::new(
            foe.x - self.body.position.x,
            0.0,
            foe.z - self.body.position.z,
        )
        .normalize();
        let radians = self.body.yaw * DEG_TO_RAD;
        let facing = Vec3::new(
            f64::from(mth::sin(f64::from(radians))),
            0.0,
            f64::from(-mth::cos(f64::from(radians))),
        )
        .normalize();
        let dot = facing.x * aim.x + facing.y * aim.y + facing.z * aim.z;
        (f64::from(dot as f32).acos() * RAD_TO_DEG) as f32 + 0.5
    }
}

fn block_center_distance_sqr(block: BlockPos, at: Vec3) -> f64 {
    let dx = f64::from(block.x) + 0.5 - at.x;
    let dy = f64::from(block.y) + 0.5 - at.y;
    let dz = f64::from(block.z) + 0.5 - at.z;
    dx * dx + dy * dy + dz * dz
}

fn bottom_center(block: BlockPos) -> Vec3 {
    Vec3::new(
        f64::from(block.x) + 0.5,
        f64::from(block.y),
        f64::from(block.z) + 0.5,
    )
}

impl PhaseSet {
    fn begin(&mut self, phase: DragonPhase) {
        match phase {
            DragonPhase::HoldingPattern => {
                self.holding.path = None;
                self.holding.target = None;
            }
            DragonPhase::StrafePlayer => {
                self.strafe.fireball_charge = 0;
                self.strafe.target = None;
                self.strafe.path = None;
                self.strafe.attack_target = None;
            }
            DragonPhase::LandingApproach => {
                self.approach.path = None;
                self.approach.target = None;
            }
            DragonPhase::Landing => self.landing.target = None,
            DragonPhase::Takeoff => {
                self.takeoff.first_tick = true;
                self.takeoff.path = None;
                self.takeoff.target = None;
            }
            DragonPhase::SittingFlaming => {
                self.flaming.flame_ticks = 0;
                self.flaming.flame_count += 1;
            }
            DragonPhase::SittingScanning => self.scanning_time = 0,
            DragonPhase::SittingAttacking => self.attacking_ticks = 0,
            DragonPhase::ChargingPlayer => {
                self.charge.target = None;
                self.charge.time_since_charge = 0;
            }
            DragonPhase::Dying => {
                self.death.target = None;
                self.death.time = 0;
            }
            DragonPhase::Hovering => self.hover.target = None,
        }
    }

    fn end(&mut self, phase: DragonPhase) -> Vec<DragonAction> {
        if phase == DragonPhase::SittingFlaming && self.flaming.flame.take().is_some() {
            return vec![DragonAction::DiscardFlame];
        }
        Vec::new()
    }

    fn target(&self, phase: DragonPhase) -> Option<Vec3> {
        match phase {
            DragonPhase::HoldingPattern => self.holding.target,
            DragonPhase::StrafePlayer => self.strafe.target,
            DragonPhase::LandingApproach => self.approach.target,
            DragonPhase::Landing => self.landing.target,
            DragonPhase::Takeoff => self.takeoff.target,
            DragonPhase::ChargingPlayer => self.charge.target,
            DragonPhase::Dying => self.death.target,
            DragonPhase::Hovering => self.hover.target,
            _ => None,
        }
    }

    fn tick<E: DragonEnv>(&mut self, phase: DragonPhase, tick: &mut Tick<'_, E>) {
        match phase {
            DragonPhase::HoldingPattern => self.tick_holding(tick),
            DragonPhase::StrafePlayer => self.tick_strafe(tick),
            DragonPhase::LandingApproach => self.tick_approach(tick),
            DragonPhase::Landing => self.tick_landing(tick),
            DragonPhase::Takeoff => self.tick_takeoff(tick),
            DragonPhase::SittingFlaming => self.tick_flaming(tick),
            DragonPhase::SittingScanning => self.tick_scanning(tick),
            DragonPhase::SittingAttacking => {
                let ticks = self.attacking_ticks;
                self.attacking_ticks += 1;
                if ticks > ATTACKING_TICKS {
                    tick.change(DragonPhase::SittingFlaming);
                }
            }
            DragonPhase::ChargingPlayer => self.tick_charge(tick),
            DragonPhase::Dying => self.tick_death(tick),
            DragonPhase::Hovering => {
                if self.hover.target.is_none() {
                    self.hover.target = Some(tick.body.position);
                }
            }
        }
    }

    fn tick_holding<E: DragonEnv>(&mut self, tick: &mut Tick<'_, E>) {
        if tick.off_course(self.holding.target) {
            self.find_holding_target(tick);
        }
    }

    fn find_holding_target<E: DragonEnv>(&mut self, tick: &mut Tick<'_, E>) {
        let state = &mut self.holding;
        if state.path.as_ref().is_some_and(DragonPath::is_done) {
            let egg = tick.ground_at_podium(Ground::NoLeaves);
            let crystals = tick.crystals.unwrap_or(0);
            if tick.env.next_int(crystals + 3) == 0 {
                tick.change(DragonPhase::LandingApproach);
                return;
            }
            let egg_point = Vec3::new(f64::from(egg.x), f64::from(egg.y), f64::from(egg.z));
            let nearest = tick.env.nearest_player(Targeting::NewTarget, egg_point);
            let distance = nearest.map_or(NEW_TARGET_FAR_DISTANCE, |foe| {
                block_center_distance_sqr(egg, foe.position) / 512.0
            });
            if let Some(foe) = nearest
                && (tick.env.next_int((distance + 2.0) as i32) == 0
                    || tick.env.next_int(crystals + 2) == 0)
            {
                tick.change_with(DragonPhase::StrafePlayer, Followup::StrafeTarget(foe));
                return;
            }
        }
        if state.path.as_ref().is_none_or(DragonPath::is_done) {
            let closest = tick.closest_node();
            let mut target = closest as i32;
            if tick.env.next_int(8) == 0 {
                state.clockwise = !state.clockwise;
                target += 6;
            }
            target += if state.clockwise { 1 } else { -1 };
            let index = Tick::<E>::ring_target(target, tick.crystals.is_some());
            state.path = tick.find_path(closest, index, None);
            if let Some(path) = state.path.as_mut() {
                path.advance();
            }
        }
        if let Some(target) = tick.waypoint(&mut state.path, false) {
            state.target = Some(target);
        }
    }

    fn tick_strafe<E: DragonEnv>(&mut self, tick: &mut Tick<'_, E>) {
        let Some(foe) = self.strafe.attack_target.and_then(|id| tick.env.foe(id)) else {
            tick.change(DragonPhase::HoldingPattern);
            return;
        };
        let state = &mut self.strafe;
        if state.path.as_ref().is_some_and(DragonPath::is_done) {
            let dx = foe.position.x - tick.body.position.x;
            let dz = foe.position.z - tick.body.position.z;
            let lift =
                ((dx * dx + dz * dz).sqrt() / 80.0 + 0.400_000_005_960_464_5 - 1.0).min(10.0);
            state.target = Some(Vec3::new(
                foe.position.x,
                foe.position.y + lift,
                foe.position.z,
            ));
        }
        if tick.off_course(state.target) {
            Self::find_strafe_target(state, tick);
        }
        let distance = foe.position.distance_squared(tick.body.position);
        if distance >= FIREBALL_RANGE_SQR {
            if state.fireball_charge > 0 {
                state.fireball_charge -= 1;
            }
            return;
        }
        if !tick.env.has_line_of_sight(foe.id) {
            if state.fireball_charge > 0 {
                state.fireball_charge -= 1;
            }
            return;
        }
        state.fireball_charge += 1;
        let angle = tick.alignment_angle(foe.position);
        if state.fireball_charge > FIREBALL_CHARGE && (0.0..FIREBALL_ARC).contains(&angle) {
            let view = tick.body.view_vector();
            let from = Vec3::new(
                tick.body.head.x - view.x,
                tick.body.head.y + 0.5 + 0.5,
                tick.body.head.z - view.z,
            );
            let direction = Vec3::new(
                foe.position.x - from.x,
                foe.y_at(0.5) - from.y,
                foe.position.z - from.z,
            );
            if !tick.body.silent {
                tick.out.actions.push(DragonAction::LevelEvent {
                    event: FIREBALL_EVENT,
                    pos: tick.body.position.block(),
                });
            }
            tick.out
                .actions
                .push(DragonAction::Fireball { from, direction });
            state.fireball_charge = 0;
            if let Some(path) = state.path.as_mut() {
                while !path.is_done() {
                    path.advance();
                }
            }
            tick.change(DragonPhase::HoldingPattern);
        }
    }

    fn find_strafe_target<E: DragonEnv>(state: &mut Strafe, tick: &mut Tick<'_, E>) {
        if state.path.as_ref().is_none_or(DragonPath::is_done) {
            let closest = tick.closest_node();
            let mut target = closest as i32;
            if tick.env.next_int(8) == 0 {
                state.clockwise = !state.clockwise;
                target += 6;
            }
            target += if state.clockwise { 1 } else { -1 };
            let index = Tick::<E>::ring_target(target, tick.crystals.is_some_and(|c| c > 0));
            state.path = tick.find_path(closest, index, None);
            if let Some(path) = state.path.as_mut() {
                path.advance();
            }
        }
        if let Some(target) = tick.waypoint(&mut state.path, false) {
            state.target = Some(target);
        }
    }

    fn set_strafe_target<E: DragonEnv>(&mut self, foe: Foe, tick: &mut Tick<'_, E>) {
        let state = &mut self.strafe;
        state.attack_target = Some(foe.id);
        let closest = tick.closest_node();
        let end = tick.closest_node_at((foe.position.x, foe.position.y, foe.position.z));
        let block_x = mth::floor(foe.position.x);
        let block_z = mth::floor(foe.position.z);
        let dx = f64::from(block_x) - tick.body.position.x;
        let dz = f64::from(block_z) - tick.body.position.z;
        let lift = ((dx * dx + dz * dz).sqrt() / 80.0 + 0.400_000_005_960_464_5 - 1.0).min(10.0);
        let y = mth::floor(foe.position.y + lift);
        state.path = tick.find_path(closest, end, Some(DragonNode::new(block_x, y, block_z)));
        if let Some(path) = state.path.as_mut() {
            path.advance();
            if let Some(target) = tick.waypoint(&mut state.path, false) {
                state.target = Some(target);
            }
        }
    }

    fn tick_approach<E: DragonEnv>(&mut self, tick: &mut Tick<'_, E>) {
        if tick.off_course(self.approach.target) {
            self.find_approach_target(tick);
        }
    }

    fn find_approach_target<E: DragonEnv>(&mut self, tick: &mut Tick<'_, E>) {
        let state = &mut self.approach;
        if state.path.as_ref().is_none_or(DragonPath::is_done) {
            let closest = tick.closest_node();
            let egg = tick.ground_at_podium(Ground::NoLeaves);
            let egg_point = Vec3::new(f64::from(egg.x), f64::from(egg.y), f64::from(egg.z));
            let end = match tick.env.nearest_player(Targeting::NearEgg, egg_point) {
                Some(foe) => tick.behind_node(foe.position),
                None => tick.closest_node_at((egg_point.x, egg_point.y, egg_point.z)),
            };
            let last = DragonNode::new(egg.x, egg.y, egg.z);
            state.path = tick.find_path(closest, end, Some(last));
            if let Some(path) = state.path.as_mut() {
                path.advance();
            }
        }
        if let Some(target) = tick.waypoint(&mut state.path, false) {
            state.target = Some(target);
        }
        if state.path.as_ref().is_some_and(DragonPath::is_done) {
            tick.change(DragonPhase::Landing);
        }
    }

    fn tick_landing<E: DragonEnv>(&mut self, tick: &mut Tick<'_, E>) {
        let state = &mut self.landing;
        let target = *state
            .target
            .get_or_insert_with(|| bottom_center(tick.ground_at_podium(Ground::NoLeaves)));
        if target.distance_squared(tick.body.position) < LANDING_REACHED_SQR {
            self.flaming.flame_count = 0;
            tick.change(DragonPhase::SittingScanning);
        }
    }

    fn tick_takeoff<E: DragonEnv>(&mut self, tick: &mut Tick<'_, E>) {
        let state = &mut self.takeoff;
        if state.first_tick || state.path.is_none() {
            state.first_tick = false;
            Self::find_takeoff_target(state, tick);
            return;
        }
        let egg = tick.ground_at_podium(Ground::NoLeaves);
        if block_center_distance_sqr(egg, tick.body.position)
            < TAKEOFF_CLEAR_DISTANCE * TAKEOFF_CLEAR_DISTANCE
        {
            return;
        }
        tick.change(DragonPhase::HoldingPattern);
    }

    fn find_takeoff_target<E: DragonEnv>(state: &mut Takeoff, tick: &mut Tick<'_, E>) {
        let closest = tick.closest_node();
        let podium = tick.ground_at_podium(Ground::NoLeaves);
        let look = tick.body.head_look_vector(DragonPhase::Takeoff, podium);
        let end = tick.closest_node_at((-look.x * BEHIND_FAR, BEHIND_HEIGHT, -look.z * BEHIND_FAR));
        let index = Tick::<E>::ring_target(end as i32, tick.crystals.is_some_and(|c| c > 0));
        state.path = tick.find_path(closest, index, None);
        if let Some(target) = tick.waypoint(&mut state.path, true) {
            state.target = Some(target);
        }
    }

    fn tick_flaming<E: DragonEnv>(&mut self, tick: &mut Tick<'_, E>) {
        let state = &mut self.flaming;
        state.flame_ticks += 1;
        if state.flame_ticks < FLAME_TICKS {
            if state.flame_ticks == FLAME_START {
                let head = tick.body.head;
                let away = Vec3::new(
                    head.x - tick.body.position.x,
                    0.0,
                    head.z - tick.body.position.z,
                )
                .normalize();
                let x = head.x + away.x * FLAME_REACH / 2.0;
                let z = head.z + away.z * FLAME_REACH / 2.0;
                let start = head.y + 0.5;
                let mut y = start;
                while tick.env.is_empty_block(BlockPos::new(
                    mth::floor(x),
                    mth::floor(y),
                    mth::floor(z),
                )) {
                    y -= 1.0;
                    if y < 0.0 {
                        y = start;
                        break;
                    }
                }
                let ground = f64::from(mth::floor(y) + 1);
                tick.out.actions.push(DragonAction::SpawnFlame {
                    position: Vec3::new(x, ground, z),
                });
            }
        } else if state.flame_count < FLAME_REPEATS {
            tick.change(DragonPhase::SittingScanning);
        } else {
            tick.change(DragonPhase::Takeoff);
        }
    }

    fn tick_scanning<E: DragonEnv>(&mut self, tick: &mut Tick<'_, E>) {
        self.scanning_time += 1;
        let position = tick.body.position;
        match tick.env.nearest_player(Targeting::Scan, position) {
            None => {
                if self.scanning_time > SCANNING_GIVE_UP_TICKS {
                    let charge = tick.env.nearest_player(Targeting::Charge, position);
                    tick.change(DragonPhase::Takeoff);
                    if let Some(foe) = charge {
                        tick.change_with(
                            DragonPhase::ChargingPlayer,
                            Followup::ChargeTarget(foe.position),
                        );
                    }
                }
            }
            Some(foe) if self.scanning_time < SCANNING_ALIGN_TICKS => {
                let angle = tick.alignment_angle(foe.position);
                if !(0.0..=FIREBALL_ARC).contains(&angle) {
                    let dx = foe.position.x - tick.body.head.x;
                    let dz = foe.position.z - tick.body.head.z;
                    let turn = f64::from(mth::wrap_degrees(
                        (180.0 - mth::atan2(dx, dz) * RAD_TO_DEG - f64::from(tick.body.yaw)) as f32,
                    ))
                    .clamp(-100.0, 100.0);
                    tick.body.y_rot_a *= 0.8;
                    let distance = (dx * dx + dz * dz).sqrt() as f32 + 1.0;
                    let capped = distance.min(40.0);
                    tick.body.y_rot_a += turn as f32 * (0.7 / capped / distance);
                    tick.body.yaw += tick.body.y_rot_a;
                }
            }
            Some(_) => tick.change(DragonPhase::SittingAttacking),
        }
    }

    fn tick_charge<E: DragonEnv>(&mut self, tick: &mut Tick<'_, E>) {
        let state = &mut self.charge;
        let Some(target) = state.target else {
            tick.change(DragonPhase::HoldingPattern);
            return;
        };
        if state.time_since_charge > 0 {
            let previous = state.time_since_charge;
            state.time_since_charge += 1;
            if previous > CHARGE_DURATION {
                tick.change(DragonPhase::HoldingPattern);
                return;
            }
        }
        if tick.off_course(Some(target)) {
            state.time_since_charge += 1;
        }
    }

    fn tick_death<E: DragonEnv>(&mut self, tick: &mut Tick<'_, E>) {
        let state = &mut self.death;
        state.time += 1;
        let target = *state.target.get_or_insert_with(|| {
            let podium = tick.body.origin;
            let y = tick.env.ground(Ground::Blocking, podium.x, podium.z);
            bottom_center(BlockPos::new(podium.x, y, podium.z))
        });
        let health = if tick.off_course(Some(target)) {
            0.0
        } else {
            1.0
        };
        tick.out.actions.push(DragonAction::SetHealth(health));
    }
}

pub fn fly(body: &mut DragonBody, phase: DragonPhase, target: Vec3, in_wall: bool) {
    let dx = target.x - body.position.x;
    let mut dy = target.y - body.position.y;
    let dz = target.z - body.position.z;
    let distance = dx * dx + dy * dy + dz * dz;
    let max = f64::from(phase.fly_speed());
    let horizontal = (dx * dx + dz * dz).sqrt();
    if horizontal > 0.0 {
        dy = (dy / horizontal).clamp(-max, max);
    }
    body.velocity = Vec3::new(
        body.velocity.x,
        body.velocity.y + dy * FLY_VERTICAL_PULL,
        body.velocity.z,
    );
    body.yaw = mth::wrap_degrees(body.yaw);
    let aim = (target - body.position).normalize();
    let radians = body.yaw * DEG_TO_RAD;
    let direction = Vec3::new(
        f64::from(mth::sin(f64::from(radians))),
        body.velocity.y,
        f64::from(-mth::cos(f64::from(radians))),
    )
    .normalize();
    let dot = direction.x * aim.x + direction.y * aim.y + direction.z * aim.z;
    let alignment = ((dot as f32 + 0.5) / 1.5).max(0.0);
    if dx.abs() as f32 > ZERO_EPSILON || dz.abs() as f32 > ZERO_EPSILON {
        let turn = mth::clamp(
            mth::wrap_degrees(180.0 - (mth::atan2(dx, dz) as f32) * 57.295_776 - body.yaw),
            -FLY_YAW_LIMIT,
            FLY_YAW_LIMIT,
        );
        body.y_rot_a *= 0.8;
        let horizontal_speed = body.velocity.horizontal_distance_sqr().sqrt();
        body.y_rot_a += turn * phase.turn_speed(horizontal_speed);
        body.yaw += body.y_rot_a * 0.1;
    }
    let span = (2.0 / (distance + 1.0)) as f32;
    let speed = FLY_ACCELERATION * (alignment * span + (1.0 - span));
    let radians = body.yaw * DEG_TO_RAD;
    body.velocity = body.velocity
        + Vec3::new(
            f64::from(speed) * f64::from(mth::sin(f64::from(radians))),
            0.0,
            -(f64::from(speed) * f64::from(mth::cos(f64::from(radians)))),
        );
    let step = if in_wall {
        body.velocity.scale(FLY_IN_WALL_SCALE)
    } else {
        body.velocity
    };
    body.position = body.position + step;
    let heading = body.velocity.normalize();
    let slide = (heading.x * direction.x + heading.y * direction.y + heading.z * direction.z + 1.0)
        * FLY_SLIDE_RANGE
        / 2.0
        + FLY_SLIDE_BASE;
    body.velocity = body.velocity.multiply(slide, FLY_VERTICAL_DRAG, slide);
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    struct Script {
        ints: VecDeque<i32>,
        floats: VecDeque<f32>,
        ground: i32,
        players: Vec<Foe>,
        sight: bool,
        empty_below: i32,
    }

    impl Default for Script {
        fn default() -> Self {
            Self {
                ints: VecDeque::new(),
                floats: VecDeque::new(),
                ground: 64,
                players: Vec::new(),
                sight: true,
                empty_below: 0,
            }
        }
    }

    impl RandomSource for Script {
        fn next_float(&mut self) -> f32 {
            self.floats.pop_front().unwrap_or(0.0)
        }

        fn next_int(&mut self, _bound: i32) -> i32 {
            self.ints.pop_front().unwrap_or(1)
        }

        fn next_double(&mut self) -> f64 {
            0.0
        }

        fn next_boolean(&mut self) -> bool {
            false
        }

        fn next_gaussian(&mut self) -> f64 {
            0.0
        }
    }

    impl DragonEnv for Script {
        fn ground(&self, _ground: Ground, _x: i32, _z: i32) -> i32 {
            self.ground
        }

        fn nearest_player(&self, targeting: Targeting, near: Vec3) -> Option<Foe> {
            self.players
                .iter()
                .copied()
                .filter(|foe| {
                    targeting
                        .height_limit()
                        .is_none_or(|limit| (foe.position.y - near.y).abs() <= limit)
                })
                .min_by(|a, b| {
                    a.position
                        .distance_squared(near)
                        .total_cmp(&b.position.distance_squared(near))
                })
        }

        fn foe(&self, id: EntityId) -> Option<Foe> {
            self.players.iter().copied().find(|foe| foe.id == id)
        }

        fn has_line_of_sight(&self, _id: EntityId) -> bool {
            self.sight
        }

        fn is_empty_block(&self, pos: BlockPos) -> bool {
            pos.y > self.empty_below
        }
    }

    fn body() -> DragonBody {
        DragonBody {
            position: Vec3::new(0.0, 100.0, 0.0),
            velocity: Vec3::ZERO,
            yaw: 0.0,
            y_rot_a: 0.0,
            head: Vec3::new(0.0, 100.0, -6.5),
            silent: false,
            origin: BlockPos::new(0, 0, 0),
        }
    }

    fn player(id: i32, x: f64, y: f64, z: f64) -> Foe {
        Foe {
            id: EntityId(id),
            position: Vec3::new(x, y, z),
            height: 1.8,
        }
    }

    fn run(
        phases: &mut DragonPhases,
        body: &mut DragonBody,
        nodes: &mut DragonNodes,
        crystals: Option<i32>,
        env: &mut Script,
    ) -> PhaseOutput {
        let mut out = PhaseOutput::default();
        phases.tick(body, nodes, crystals, env, &mut out);
        out
    }

    fn enter(phases: &mut DragonPhases, phase: DragonPhase) {
        phases.set_phase(phase);
    }

    #[test]
    fn the_phase_ids_match_the_registration_order() {
        let ids: Vec<i32> = DragonPhase::ALL.iter().map(|phase| phase.id()).collect();
        assert_eq!(ids, (0..11).collect::<Vec<_>>());
        assert_eq!(DragonPhase::from_id(9), Some(DragonPhase::Dying));
        assert_eq!(DragonPhase::from_id(11), None);
        assert_eq!(DragonPhase::from_id(-1), None);
        assert_eq!(DragonPhase::HoldingPattern.name(), "HoldingPattern");
    }

    #[test]
    fn hover_and_the_three_sitting_phases_are_sitting() {
        let sitting: Vec<DragonPhase> = DragonPhase::ALL
            .into_iter()
            .filter(|phase| phase.is_sitting())
            .collect();
        assert_eq!(
            sitting,
            vec![
                DragonPhase::SittingFlaming,
                DragonPhase::SittingScanning,
                DragonPhase::SittingAttacking,
                DragonPhase::Hovering
            ]
        );
    }

    #[test]
    fn fly_speeds_are_vanilla() {
        assert_eq!(DragonPhase::HoldingPattern.fly_speed(), 0.6);
        assert_eq!(DragonPhase::ChargingPlayer.fly_speed(), 3.0);
        assert_eq!(DragonPhase::Dying.fly_speed(), 3.0);
        assert_eq!(DragonPhase::Hovering.fly_speed(), 1.0);
        assert_eq!(DragonPhase::Landing.fly_speed(), 1.5);
    }

    #[test]
    fn turn_speeds_slow_down_with_speed_except_when_landing() {
        assert!((DragonPhase::HoldingPattern.turn_speed(0.0) - 0.7).abs() < 1e-6);
        assert!((DragonPhase::HoldingPattern.turn_speed(1.0) - 0.7 / 2.0 / 2.0).abs() < 1e-6);
        assert!((DragonPhase::Landing.turn_speed(0.0) - 1.0).abs() < 1e-6);
        assert!((DragonPhase::Landing.turn_speed(99.0) - 0.4).abs() < 1e-6);
    }

    #[test]
    fn a_new_dragon_hovers() {
        let phases = DragonPhases::default();
        assert_eq!(phases.current(), DragonPhase::Hovering);
        assert!(phases.is_sitting());
    }

    #[test]
    fn hovering_targets_the_position_where_it_began() {
        let mut phases = DragonPhases::default();
        let mut env = Script::default();
        let out = run(
            &mut phases,
            &mut body(),
            &mut DragonNodes::default(),
            None,
            &mut env,
        );
        assert!(out.changes.is_empty());
        assert_eq!(phases.fly_target(), Some(Vec3::new(0.0, 100.0, 0.0)));
    }

    #[test]
    fn setting_the_same_phase_changes_nothing() {
        let mut phases = DragonPhases::default();
        phases.set.hover.target = Some(Vec3::new(1.0, 2.0, 3.0));
        phases.set_phase(DragonPhase::Hovering);
        assert_eq!(phases.fly_target(), Some(Vec3::new(1.0, 2.0, 3.0)));
    }

    #[test]
    fn entering_a_phase_resets_its_target_and_path() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::HoldingPattern);
        phases.set.holding.target = Some(Vec3::new(1.0, 2.0, 3.0));
        enter(&mut phases, DragonPhase::Landing);
        enter(&mut phases, DragonPhase::HoldingPattern);
        assert_eq!(phases.fly_target(), None);
    }

    #[test]
    fn each_flaming_phase_counts_one_and_leaving_it_discards_the_cloud() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::SittingFlaming);
        enter(&mut phases, DragonPhase::SittingScanning);
        enter(&mut phases, DragonPhase::SittingFlaming);
        assert_eq!(phases.set.flaming.flame_count, 2);
        phases.set_flame(EntityId(5));
        assert_eq!(phases.flame(), Some(EntityId(5)));
        let actions = phases.set_phase(DragonPhase::SittingScanning);
        assert_eq!(actions, vec![DragonAction::DiscardFlame]);
        assert_eq!(phases.flame(), None);
        assert!(phases.set_phase(DragonPhase::Takeoff).is_empty());
    }

    #[test]
    fn the_holding_pattern_picks_the_closest_node_and_flies_to_the_previous_one_when_counter_clockwise()
     {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::HoldingPattern);
        let mut nodes = DragonNodes::default();
        let mut env = Script {
            ints: VecDeque::from([1]),
            floats: VecDeque::from([0.5]),
            ..Script::default()
        };
        let mut moving = body();
        moving.position = Vec3::new(60.0, 80.0, 0.0);
        let out = run(&mut phases, &mut moving, &mut nodes, Some(1), &mut env);
        assert!(out.changes.is_empty());
        let target = phases.fly_target().unwrap();
        let wanted = nodes.node(11).unwrap();
        assert_eq!(
            (target.x, target.z),
            (f64::from(wanted.x), f64::from(wanted.z))
        );
        assert_eq!(target.y, f64::from(wanted.y) + 10.0);
    }

    #[test]
    fn the_holding_pattern_goes_the_other_way_when_the_clockwise_flag_flips_off() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::HoldingPattern);
        phases.set.holding.clockwise = true;
        let mut nodes = DragonNodes::default();
        let mut env = Script {
            ints: VecDeque::from([1]),
            ..Script::default()
        };
        let mut moving = body();
        moving.position = Vec3::new(60.0, 80.0, 0.0);
        run(&mut phases, &mut moving, &mut nodes, Some(1), &mut env);
        let target = phases.fly_target().unwrap();
        let wanted = nodes.node(1).unwrap();
        assert_eq!(target.x, f64::from(wanted.x));
        let mut again = DragonPhases::default();
        enter(&mut again, DragonPhase::HoldingPattern);
        let mut env = Script {
            ints: VecDeque::from([0]),
            ..Script::default()
        };
        run(&mut again, &mut moving, &mut nodes, Some(1), &mut env);
        assert!(again.set.holding.clockwise);
    }

    #[test]
    fn a_holding_dragon_far_from_its_target_keeps_flying_without_changing_anything() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::HoldingPattern);
        phases.set.holding.target = Some(Vec3::new(0.0, 100.0, 50.0));
        let mut env = Script::default();
        let out = run(
            &mut phases,
            &mut body(),
            &mut DragonNodes::default(),
            Some(1),
            &mut env,
        );
        assert!(out.changes.is_empty() && out.actions.is_empty());
        assert_eq!(phases.fly_target(), Some(Vec3::new(0.0, 100.0, 50.0)));
    }

    #[test]
    fn with_a_finished_path_a_zero_roll_sends_the_dragon_to_land() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::HoldingPattern);
        let mut nodes = DragonNodes::default();
        nodes.build(|_, _| 64);
        phases.set.holding.path = Some(DragonPath::new(Vec::new()));
        let mut env = Script {
            ints: VecDeque::from([0]),
            ..Script::default()
        };
        let out = run(&mut phases, &mut body(), &mut nodes, Some(2), &mut env);
        assert_eq!(
            out.changes,
            vec![PhaseChange::to(DragonPhase::LandingApproach)]
        );
    }

    #[test]
    fn a_nearby_player_makes_the_dragon_strafe_him() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::HoldingPattern);
        let mut nodes = DragonNodes::default();
        nodes.build(|_, _| 64);
        phases.set.holding.path = Some(DragonPath::new(Vec::new()));
        let victim = player(7, 10.0, 64.0, 0.0);
        let mut env = Script {
            ints: VecDeque::from([1, 0]),
            players: vec![victim],
            ..Script::default()
        };
        let out = run(&mut phases, &mut body(), &mut nodes, Some(1), &mut env);
        assert_eq!(
            out.changes,
            vec![PhaseChange {
                to: DragonPhase::StrafePlayer,
                then: Some(Followup::StrafeTarget(victim))
            }]
        );
    }

    #[test]
    fn strafing_without_a_target_goes_back_to_the_holding_pattern() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::StrafePlayer);
        let out = run(
            &mut phases,
            &mut body(),
            &mut DragonNodes::default(),
            Some(1),
            &mut Script::default(),
        );
        assert_eq!(
            out.changes,
            vec![PhaseChange::to(DragonPhase::HoldingPattern)]
        );
    }

    #[test]
    fn the_strafe_target_plans_a_path_that_ends_above_the_player() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::StrafePlayer);
        let mut nodes = DragonNodes::default();
        nodes.build(|_, _| 64);
        let victim = player(7, 40.0, 70.0, 0.0);
        let mut env = Script {
            players: vec![victim],
            ..Script::default()
        };
        let mut moving = body();
        moving.position = Vec3::new(60.0, 80.0, 0.0);
        phases.apply(
            Followup::StrafeTarget(victim),
            &mut moving,
            &mut nodes,
            Some(1),
            &mut env,
        );
        assert_eq!(phases.set.strafe.attack_target, Some(EntityId(7)));
        assert!(phases.set.strafe.target.is_some());
        let path = phases.set.strafe.path.as_ref().unwrap();
        let last = path.nodes().last().unwrap();
        assert_eq!((last.x, last.z), (40, 0));
    }

    #[test]
    fn a_strafing_dragon_in_range_and_in_sight_charges_then_throws_a_fireball() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::StrafePlayer);
        let victim = player(7, 0.0, 100.0, -40.0);
        phases.set.strafe.attack_target = Some(EntityId(7));
        phases.set.strafe.target = Some(Vec3::new(0.0, 100.0, -40.0));
        let mut nodes = DragonNodes::default();
        nodes.build(|_, _| 64);
        let mut env = Script {
            players: vec![victim],
            ..Script::default()
        };
        let mut moving = body();
        for tick in 1..=4 {
            let out = run(&mut phases, &mut moving, &mut nodes, Some(1), &mut env);
            assert!(out.actions.is_empty(), "tick {tick}");
            assert_eq!(phases.set.strafe.fireball_charge, tick);
        }
        let out = run(&mut phases, &mut moving, &mut nodes, Some(1), &mut env);
        assert_eq!(phases.set.strafe.fireball_charge, 0);
        assert_eq!(
            out.changes,
            vec![PhaseChange::to(DragonPhase::HoldingPattern)]
        );
        assert_eq!(out.actions.len(), 2);
        assert_eq!(
            out.actions[0],
            DragonAction::LevelEvent {
                event: 1017,
                pos: BlockPos::new(0, 100, 0)
            }
        );
        let DragonAction::Fireball { from, direction } = out.actions[1] else {
            panic!();
        };
        assert!((from.x - 0.0).abs() < 1e-5 && (from.y - 101.0).abs() < 1e-5);
        assert!((from.z - -6.5 + 1.0).abs() < 1e-5, "{from:?}");
        assert!(direction.z < 0.0);
    }

    #[test]
    fn a_silent_dragon_throws_its_fireball_without_the_sound_event() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::StrafePlayer);
        phases.set.strafe.attack_target = Some(EntityId(7));
        phases.set.strafe.target = Some(Vec3::new(0.0, 100.0, -40.0));
        phases.set.strafe.fireball_charge = 4;
        let mut env = Script {
            players: vec![player(7, 0.0, 100.0, -40.0)],
            ..Script::default()
        };
        let mut silent = body();
        silent.silent = true;
        let mut nodes = DragonNodes::default();
        nodes.build(|_, _| 64);
        let out = run(&mut phases, &mut silent, &mut nodes, Some(1), &mut env);
        assert_eq!(out.actions.len(), 1);
        assert!(matches!(out.actions[0], DragonAction::Fireball { .. }));
    }

    #[test]
    fn the_fireball_charge_decays_without_line_of_sight_or_beyond_sixty_four_blocks() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::StrafePlayer);
        phases.set.strafe.attack_target = Some(EntityId(7));
        phases.set.strafe.target = Some(Vec3::new(0.0, 100.0, -40.0));
        phases.set.strafe.fireball_charge = 3;
        let mut nodes = DragonNodes::default();
        nodes.build(|_, _| 64);
        let mut env = Script {
            players: vec![player(7, 0.0, 100.0, -40.0)],
            sight: false,
            ..Script::default()
        };
        run(&mut phases, &mut body(), &mut nodes, Some(1), &mut env);
        assert_eq!(phases.set.strafe.fireball_charge, 2);
        env.sight = true;
        env.players = vec![player(7, 0.0, 100.0, -70.0)];
        phases.set.strafe.target = Some(Vec3::new(0.0, 100.0, -70.0));
        run(&mut phases, &mut body(), &mut nodes, Some(1), &mut env);
        assert_eq!(phases.set.strafe.fireball_charge, 1);
    }

    #[test]
    fn a_player_outside_the_firing_arc_does_not_draw_a_fireball() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::StrafePlayer);
        phases.set.strafe.attack_target = Some(EntityId(7));
        phases.set.strafe.target = Some(Vec3::new(40.0, 100.0, 0.0));
        phases.set.strafe.fireball_charge = 9;
        let mut nodes = DragonNodes::default();
        nodes.build(|_, _| 64);
        let mut env = Script {
            players: vec![player(7, 40.0, 100.0, 0.0)],
            ..Script::default()
        };
        let out = run(&mut phases, &mut body(), &mut nodes, Some(1), &mut env);
        assert!(out.actions.is_empty());
        assert_eq!(phases.set.strafe.fireball_charge, 10);
    }

    #[test]
    fn the_landing_approach_ends_with_the_landing_phase_once_its_path_is_flown() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::LandingApproach);
        let mut nodes = DragonNodes::default();
        nodes.build(|_, _| 64);
        let mut moving = body();
        moving.position = Vec3::new(40.0, 80.0, 0.0);
        let mut env = Script::default();
        let mut changes = Vec::new();
        for _ in 0..12 {
            let out = run(&mut phases, &mut moving, &mut nodes, Some(1), &mut env);
            changes.extend(out.changes);
            if !changes.is_empty() {
                break;
            }
            phases.set.approach.target = None;
        }
        assert_eq!(changes, vec![PhaseChange::to(DragonPhase::Landing)]);
        let path = phases.set.approach.path.as_ref().unwrap();
        assert_eq!(
            path.nodes().last().map(|node| (node.x, node.y, node.z)),
            Some((0, 64, 0))
        );
    }

    #[test]
    fn landing_targets_the_bottom_center_of_the_podium_and_sits_when_it_arrives() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::SittingFlaming);
        enter(&mut phases, DragonPhase::Landing);
        let mut env = Script::default();
        let mut far = body();
        let out = run(
            &mut phases,
            &mut far,
            &mut DragonNodes::default(),
            Some(1),
            &mut env,
        );
        assert!(out.changes.is_empty());
        assert_eq!(phases.fly_target(), Some(Vec3::new(0.5, 64.0, 0.5)));
        far.position = Vec3::new(0.5, 64.5, 0.5);
        let out = run(
            &mut phases,
            &mut far,
            &mut DragonNodes::default(),
            Some(1),
            &mut env,
        );
        assert_eq!(
            out.changes,
            vec![PhaseChange::to(DragonPhase::SittingScanning)]
        );
        assert_eq!(phases.set.flaming.flame_count, 0);
    }

    #[test]
    fn the_takeoff_plans_a_path_on_its_first_tick_then_leaves_when_far_from_the_podium() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::Takeoff);
        let mut nodes = DragonNodes::default();
        nodes.build(|_, _| 64);
        let mut sitting = body();
        sitting.position = Vec3::new(0.5, 64.0, 0.5);
        let mut env = Script::default();
        let out = run(&mut phases, &mut sitting, &mut nodes, Some(1), &mut env);
        assert!(out.changes.is_empty());
        assert!(!phases.set.takeoff.first_tick);
        assert!(phases.set.takeoff.path.is_some());
        let out = run(&mut phases, &mut sitting, &mut nodes, Some(1), &mut env);
        assert!(out.changes.is_empty());
        sitting.position = Vec3::new(40.0, 80.0, 0.0);
        let out = run(&mut phases, &mut sitting, &mut nodes, Some(1), &mut env);
        assert_eq!(
            out.changes,
            vec![PhaseChange::to(DragonPhase::HoldingPattern)]
        );
    }

    #[test]
    fn scanning_turns_toward_a_player_outside_the_arc_and_then_attacks() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::SittingScanning);
        let mut sitting = body();
        let mut env = Script {
            players: vec![player(1, 10.0, 100.0, 0.0)],
            ..Script::default()
        };
        let mut nodes = DragonNodes::default();
        run(&mut phases, &mut sitting, &mut nodes, Some(1), &mut env);
        assert!(sitting.yaw != 0.0);
        for _ in 0..24 {
            run(&mut phases, &mut sitting, &mut nodes, Some(1), &mut env);
        }
        let out = run(&mut phases, &mut sitting, &mut nodes, Some(1), &mut env);
        assert_eq!(
            out.changes,
            vec![PhaseChange::to(DragonPhase::SittingAttacking)]
        );
    }

    #[test]
    fn scanning_with_nobody_around_takes_off_after_a_hundred_ticks_and_charges_far_players() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::SittingScanning);
        let mut sitting = body();
        let mut env = Script::default();
        let mut nodes = DragonNodes::default();
        for _ in 0..99 {
            let out = run(&mut phases, &mut sitting, &mut nodes, Some(1), &mut env);
            assert!(out.changes.is_empty());
        }
        let out = run(&mut phases, &mut sitting, &mut nodes, Some(1), &mut env);
        assert_eq!(out.changes, vec![PhaseChange::to(DragonPhase::Takeoff)]);
        let mut again = DragonPhases::default();
        enter(&mut again, DragonPhase::SittingScanning);
        again.set.scanning_time = 99;
        env.players = vec![player(3, 0.0, 200.0, 100.0)];
        let out = run(&mut again, &mut sitting, &mut nodes, Some(1), &mut env);
        assert_eq!(out.changes.len(), 2);
        assert_eq!(out.changes[0], PhaseChange::to(DragonPhase::Takeoff));
        assert_eq!(out.changes[1].to, DragonPhase::ChargingPlayer);
        assert_eq!(
            out.changes[1].then,
            Some(Followup::ChargeTarget(Vec3::new(0.0, 200.0, 100.0)))
        );
    }

    #[test]
    fn the_roar_lasts_forty_ticks_before_the_breath() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::SittingAttacking);
        let mut nodes = DragonNodes::default();
        let mut env = Script::default();
        for _ in 0..40 {
            let out = run(&mut phases, &mut body(), &mut nodes, Some(1), &mut env);
            assert!(out.changes.is_empty());
        }
        let out = run(&mut phases, &mut body(), &mut nodes, Some(1), &mut env);
        assert_eq!(
            out.changes,
            vec![PhaseChange::to(DragonPhase::SittingFlaming)]
        );
    }

    #[test]
    fn the_breath_spawns_its_cloud_at_tick_ten_on_the_ground_ahead_of_the_head() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::SittingFlaming);
        let mut sitting = body();
        sitting.head = Vec3::new(0.0, 65.0, -6.0);
        sitting.position = Vec3::new(0.0, 64.0, 0.0);
        let mut env = Script {
            empty_below: 63,
            ..Script::default()
        };
        let mut nodes = DragonNodes::default();
        let mut spawned = Vec::new();
        for tick in 1..=10 {
            let out = run(&mut phases, &mut sitting, &mut nodes, Some(1), &mut env);
            if tick < 10 {
                assert!(out.actions.is_empty());
            }
            spawned.extend(out.actions);
        }
        assert_eq!(
            spawned,
            vec![DragonAction::SpawnFlame {
                position: Vec3::new(0.0, 64.0, -8.5)
            }]
        );
    }

    #[test]
    fn the_breath_ends_after_two_hundred_ticks_and_repeats_up_to_four_times() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::SittingFlaming);
        let mut nodes = DragonNodes::default();
        let mut env = Script::default();
        for _ in 0..199 {
            run(&mut phases, &mut body(), &mut nodes, Some(1), &mut env);
        }
        let out = run(&mut phases, &mut body(), &mut nodes, Some(1), &mut env);
        assert_eq!(
            out.changes,
            vec![PhaseChange::to(DragonPhase::SittingScanning)]
        );
        let mut done = DragonPhases::default();
        enter(&mut done, DragonPhase::SittingFlaming);
        done.set.flaming.flame_count = 4;
        done.set.flaming.flame_ticks = 199;
        let out = run(&mut done, &mut body(), &mut nodes, Some(1), &mut env);
        assert_eq!(out.changes, vec![PhaseChange::to(DragonPhase::Takeoff)]);
    }

    #[test]
    fn charging_without_a_target_aborts() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::ChargingPlayer);
        let out = run(
            &mut phases,
            &mut body(),
            &mut DragonNodes::default(),
            Some(1),
            &mut Script::default(),
        );
        assert_eq!(
            out.changes,
            vec![PhaseChange::to(DragonPhase::HoldingPattern)]
        );
    }

    #[test]
    fn a_charge_ends_ten_ticks_after_reaching_its_target() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::ChargingPlayer);
        let mut charging = body();
        phases.apply(
            Followup::ChargeTarget(Vec3::new(0.0, 100.0, 5.0)),
            &mut charging,
            &mut DragonNodes::default(),
            Some(1),
            &mut Script::default(),
        );
        let mut nodes = DragonNodes::default();
        let mut env = Script::default();
        run(&mut phases, &mut charging, &mut nodes, Some(1), &mut env);
        assert_eq!(phases.set.charge.time_since_charge, 1);
        let mut ended = None;
        for tick in 0..20 {
            let out = run(&mut phases, &mut charging, &mut nodes, Some(1), &mut env);
            if !out.changes.is_empty() {
                ended = Some(tick);
                assert_eq!(
                    out.changes,
                    vec![PhaseChange::to(DragonPhase::HoldingPattern)]
                );
                break;
            }
        }
        assert!(ended.is_some());
    }

    #[test]
    fn a_charge_heading_far_away_does_not_start_its_countdown() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::ChargingPlayer);
        phases.set.charge.target = Some(Vec3::new(0.0, 100.0, 80.0));
        run(
            &mut phases,
            &mut body(),
            &mut DragonNodes::default(),
            Some(1),
            &mut Script::default(),
        );
        assert_eq!(phases.set.charge.time_since_charge, 0);
    }

    #[test]
    fn the_dying_dragon_flies_to_the_ground_and_dies_when_it_arrives() {
        let mut phases = DragonPhases::default();
        enter(&mut phases, DragonPhase::Dying);
        let mut dying = body();
        let mut env = Script::default();
        let out = run(
            &mut phases,
            &mut dying,
            &mut DragonNodes::default(),
            Some(1),
            &mut env,
        );
        assert_eq!(out.actions, vec![DragonAction::SetHealth(1.0)]);
        assert_eq!(phases.fly_target(), Some(Vec3::new(0.5, 64.0, 0.5)));
        assert_eq!(phases.set.death.time, 1);
        dying.position = Vec3::new(0.5, 65.0, 0.5);
        let out = run(
            &mut phases,
            &mut dying,
            &mut DragonNodes::default(),
            Some(1),
            &mut env,
        );
        assert_eq!(out.actions, vec![DragonAction::SetHealth(0.0)]);
    }

    #[test]
    fn the_head_looks_up_when_sitting_and_more_steeply_the_closer_it_is_to_the_podium() {
        let sitting = body();
        let up = sitting.head_look_vector(DragonPhase::SittingFlaming, BlockPos::new(0, 64, 0));
        assert!((up.y - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-3);
        let level = sitting.head_look_vector(DragonPhase::HoldingPattern, BlockPos::new(0, 64, 0));
        assert!(level.y.abs() < 1e-9);
        let landing = sitting.head_look_vector(DragonPhase::Landing, BlockPos::new(0, 64, 0));
        assert!(landing.y > 0.0 && landing.y < 0.2);
        let mut near = body();
        near.position = Vec3::new(0.5, 64.5, 0.5);
        let close = near.head_look_vector(DragonPhase::Takeoff, BlockPos::new(0, 64, 0));
        assert!((close.y - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-3);
    }

    #[test]
    fn flying_accelerates_toward_the_target_and_turns_the_body() {
        let mut flying = body();
        fly(
            &mut flying,
            DragonPhase::HoldingPattern,
            Vec3::new(50.0, 100.0, 0.0),
            false,
        );
        assert!(flying.velocity.x > 0.0 || flying.y_rot_a != 0.0);
        assert!(flying.y_rot_a != 0.0);
        let before = flying.position;
        for _ in 0..40 {
            fly(
                &mut flying,
                DragonPhase::HoldingPattern,
                Vec3::new(50.0, 100.0, 0.0),
                false,
            );
        }
        assert!(flying.position.x > before.x + 1.0);
    }

    #[test]
    fn flying_climbs_when_the_target_is_above_but_slowly() {
        let mut flying = body();
        fly(
            &mut flying,
            DragonPhase::HoldingPattern,
            Vec3::new(0.0, 150.0, -50.0),
            false,
        );
        assert!(flying.velocity.y > 0.0 && flying.velocity.y < 0.02);
    }

    #[test]
    fn flying_through_a_wall_moves_eighty_percent_as_far() {
        let mut free = body();
        let mut walled = body();
        free.velocity = Vec3::new(0.0, 0.0, -1.0);
        walled.velocity = Vec3::new(0.0, 0.0, -1.0);
        fly(
            &mut free,
            DragonPhase::HoldingPattern,
            Vec3::new(0.0, 100.0, -50.0),
            false,
        );
        fly(
            &mut walled,
            DragonPhase::HoldingPattern,
            Vec3::new(0.0, 100.0, -50.0),
            true,
        );
        let ratio = walled.position.z / free.position.z;
        assert!((ratio - 0.8).abs() < 1e-3, "{ratio}");
    }
}
