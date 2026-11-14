use super::data::SyncedValue;
use super::{Entity, EntityState};
use crate::Vec3;
use crate::effect::AttributeState;

pub const DELTA_UNITS_PER_BLOCK: f64 = 4096.0;
const MIN_MOVE_SQR: f64 = 7.629_394_531_25e-6;
const FORCED_POSITION_TICKS: i32 = 60;
const TELEPORT_DELAY_LIMIT: i32 = 400;
const MOTION_EPSILON: f64 = 1.0e-7;

pub fn pack_degrees(degrees: f32) -> u8 {
    (degrees * 256.0 / 360.0).floor() as i32 as u8
}

pub fn unpack_degrees(packed: u8) -> f32 {
    f32::from(packed as i8) * 360.0 / 256.0
}

pub fn encode(value: f64) -> i64 {
    let scaled = value * DELTA_UNITS_PER_BLOCK;
    let rounded = (scaled + 0.5).floor();
    rounded as i64
}

pub fn decode(value: i64) -> f64 {
    value as f64 / DELTA_UNITS_PER_BLOCK
}

pub fn encoding_precision_loss(value: f64) -> f64 {
    decode(encode(value)) - value
}

fn too_big(delta: i64) -> bool {
    !(-0x8000..=0x7fff).contains(&delta)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeltaStep {
    pub ticks: i32,
    pub xa: i16,
    pub ya: i16,
    pub za: i16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveDelta {
    Linear { xa: i16, ya: i16, za: i16 },
    Stepped(Vec<DeltaStep>),
}

impl MoveDelta {
    fn has_x(&self) -> bool {
        match self {
            Self::Linear { xa, .. } => *xa != 0,
            Self::Stepped(steps) => steps.iter().any(|s| s.xa != 0),
        }
    }

    fn has_z(&self) -> bool {
        match self {
            Self::Linear { za, .. } => *za != 0,
            Self::Stepped(steps) => steps.iter().any(|s| s.za != 0),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PositionPath {
    Linear(Vec3),
    Stepped(Vec<(Vec3, i32)>),
}

impl PositionPath {
    pub fn end(&self) -> Vec3 {
        match self {
            Self::Linear(end) => *end,
            Self::Stepped(steps) => steps.last().map_or(Vec3::ZERO, |(p, _)| *p),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Movement {
    Position {
        delta: MoveDelta,
        on_ground: bool,
    },
    PositionRotation {
        delta: MoveDelta,
        yaw: u8,
        pitch: u8,
        on_ground: bool,
    },
    Rotation {
        yaw: u8,
        pitch: u8,
        on_ground: bool,
    },
    Sync {
        path: PositionPath,
        yaw: f32,
        pitch: f32,
        on_ground: bool,
    },
}

impl Movement {
    fn has_position(&self) -> bool {
        !matches!(self, Self::Rotation { .. })
    }

    fn has_rotation(&self) -> bool {
        !matches!(self, Self::Position { .. })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TrackerUpdate {
    Motion(Vec3),
    Move(Movement),
    Data(Vec<(u8, SyncedValue)>),
    Attributes(Vec<AttributeState>),
    HeadRotation(u8),
    SelfMotion(Vec3),
}

#[derive(Debug, Clone, PartialEq)]
struct SteppedTracker {
    steps: Vec<(Vec3, i32)>,
    ticks_since_last_step: i32,
}

impl SteppedTracker {
    fn add_step(&mut self, position: Vec3) {
        if self.ticks_since_last_step > 0 {
            self.steps.push((position, self.ticks_since_last_step));
            self.ticks_since_last_step = 0;
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pairing {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub head_yaw: f32,
    pub movement: Vec3,
    pub data: Vec<(u8, SyncedValue)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityTracker {
    base: Vec3,
    last_sent_movement: Vec3,
    last_yaw: u8,
    last_pitch: u8,
    last_head_yaw: u8,
    was_on_ground: bool,
    teleport_delay: i32,
    tick_count: i32,
    was_riding: bool,
    stepped: Option<SteppedTracker>,
    interval: Option<i32>,
    track_delta: bool,
    tracked_data: Vec<(u8, SyncedValue)>,
}

impl EntityTracker {
    pub fn new(entity: &Entity) -> Self {
        let base = &entity.base;
        let stepped = matches!(entity.state, EntityState::Living(_)).then(|| SteppedTracker {
            steps: Vec::new(),
            ticks_since_last_step: -1,
        });
        Self {
            base: base.position,
            last_sent_movement: base.velocity,
            last_yaw: pack_degrees(base.yaw),
            last_pitch: pack_degrees(base.pitch),
            last_head_yaw: pack_degrees(base.head_yaw),
            was_on_ground: base.on_ground,
            teleport_delay: 0,
            tick_count: 0,
            was_riding: false,
            stepped,
            interval: base.kind.update_interval().map(i32::from),
            track_delta: base.kind.track_deltas(),
            tracked_data: base.data.non_default(),
        }
    }

    pub fn pairing(&self) -> Pairing {
        Pairing {
            position: self.base,
            yaw: unpack_degrees(self.last_yaw),
            pitch: unpack_degrees(self.last_pitch),
            head_yaw: unpack_degrees(self.last_head_yaw),
            movement: self.last_sent_movement,
            data: self.tracked_data.clone(),
        }
    }

    fn test(&self) -> bool {
        self.interval
            .is_some_and(|period| period > 0 && self.tick_count % period == 0)
    }

    fn next_interval(&self) -> i32 {
        match self.interval {
            Some(period) if period > 0 => self.tick_count / period * period + period,
            _ => 0,
        }
    }

    fn try_encode(&self, path: &PositionPath) -> Option<MoveDelta> {
        let delta = |base: Vec3, to: Vec3| {
            let xa = encode(to.x) - encode(base.x);
            let ya = encode(to.y) - encode(base.y);
            let za = encode(to.z) - encode(base.z);
            if too_big(xa) || too_big(ya) || too_big(za) {
                None
            } else {
                Some((xa as i16, ya as i16, za as i16))
            }
        };
        match path {
            PositionPath::Linear(end) => {
                let (xa, ya, za) = delta(self.base, *end)?;
                Some(MoveDelta::Linear { xa, ya, za })
            }
            PositionPath::Stepped(steps) => {
                let mut base = self.base;
                let mut out = Vec::with_capacity(steps.len());
                for (position, ticks) in steps {
                    let (xa, ya, za) = delta(base, *position)?;
                    out.push(DeltaStep {
                        ticks: *ticks,
                        xa,
                        ya,
                        za,
                    });
                    base = *position;
                }
                Some(MoveDelta::Stepped(out))
            }
        }
    }

    fn full_precision_required(entity: &Entity, position: Vec3, delta: &MoveDelta) -> bool {
        if !matches!(entity.state, EntityState::Item(_)) {
            return false;
        }
        let base = &entity.base;
        if base.vertical_collision
            && ((delta.has_x() && encoding_precision_loss(position.x) != 0.0)
                || (delta.has_z() && encoding_precision_loss(position.z) != 0.0))
        {
            return true;
        }
        if !base.horizontal_collision {
            return false;
        }
        (delta.has_x() && encoding_precision_loss(position.z) != 0.0)
            || (delta.has_z() && encoding_precision_loss(position.x) != 0.0)
    }

    fn create_move(
        &mut self,
        entity: &Entity,
        path: PositionPath,
        yaw: u8,
        pitch: u8,
        send_position: bool,
        send_rotation: bool,
    ) -> Option<Movement> {
        let base = &entity.base;
        let sync = |path: PositionPath| Movement::Sync {
            path,
            yaw: base.yaw,
            pitch: base.pitch,
            on_ground: base.on_ground,
        };
        if self.teleport_delay > TELEPORT_DELAY_LIMIT
            || self.was_riding
            || self.was_on_ground != base.on_ground
        {
            self.was_on_ground = base.on_ground;
            self.teleport_delay = 0;
            return Some(sync(path));
        }
        if !send_position {
            return send_rotation.then_some(Movement::Rotation {
                yaw,
                pitch,
                on_ground: base.on_ground,
            });
        }
        if let Some(delta) = self.try_encode(&path)
            && !Self::full_precision_required(entity, path.end(), &delta)
        {
            return Some(if send_rotation {
                Movement::PositionRotation {
                    delta,
                    yaw,
                    pitch,
                    on_ground: base.on_ground,
                }
            } else {
                Movement::Position {
                    delta,
                    on_ground: base.on_ground,
                }
            });
        }
        Some(sync(path))
    }

    fn dirty_data(&mut self, entity: &mut Entity, out: &mut Vec<TrackerUpdate>) {
        if let Some(packed) = entity.base.data.pack_dirty() {
            self.tracked_data = entity.base.data.non_default();
            out.push(TrackerUpdate::Data(packed));
        }
        if let EntityState::Living(living) = &mut entity.state {
            let names = living.attributes.take_dirty();
            let states: Vec<AttributeState> = names
                .iter()
                .filter_map(|name| living.attributes.state(name, &living.effects))
                .collect();
            if !states.is_empty() {
                out.push(TrackerUpdate::Attributes(states));
            }
        }
    }

    fn finish_rotation(&mut self, entity: &mut Entity, out: &mut Vec<TrackerUpdate>) {
        let head = pack_degrees(entity.base.head_yaw);
        if head != self.last_head_yaw {
            out.push(TrackerUpdate::HeadRotation(head));
            self.last_head_yaw = head;
        }
        entity.base.needs_sync = false;
    }

    pub fn send_changes(&mut self, entity: &mut Entity, riding: bool) -> Vec<TrackerUpdate> {
        let mut out = Vec::new();
        let position = entity.base.position;
        if let Some(stepped) = &mut self.stepped {
            stepped.ticks_since_last_step += 1;
            if entity.base.sync_position {
                entity.base.sync_position = false;
                stepped.add_step(position);
            }
        }
        if entity.base.sync_position {
            self.tick_count = self.next_interval();
            entity.base.sync_position = false;
        }
        if entity.base.needs_sync || self.test() || entity.base.data.is_dirty() {
            let yaw = pack_degrees(entity.base.yaw);
            let pitch = pack_degrees(entity.base.pitch);
            let rotate = yaw != self.last_yaw || pitch != self.last_pitch;
            self.teleport_delay += 1;
            let path = match &mut self.stepped {
                Some(stepped) => {
                    stepped.add_step(position);
                    let path = if stepped.steps.is_empty() {
                        PositionPath::Linear(position)
                    } else {
                        PositionPath::Stepped(stepped.steps.clone())
                    };
                    stepped.steps.clear();
                    stepped.ticks_since_last_step = 0;
                    path
                }
                None => PositionPath::Linear(position),
            };
            let moved = (position - self.base).length_sqr();
            let send_position =
                moved >= MIN_MOVE_SQR || self.tick_count % FORCED_POSITION_TICKS == 0;
            if riding {
                let mut ride_out = Vec::new();
                if rotate {
                    ride_out.push(TrackerUpdate::Move(Movement::Rotation {
                        yaw,
                        pitch,
                        on_ground: entity.base.on_ground,
                    }));
                    self.last_yaw = yaw;
                    self.last_pitch = pitch;
                }
                self.base = position;
                self.dirty_data(entity, &mut ride_out);
                self.was_riding = true;
                self.finish_rotation(entity, &mut ride_out);
                self.tick_count += 1;
                if entity.base.sync_velocity {
                    entity.base.sync_velocity = false;
                    ride_out.push(TrackerUpdate::SelfMotion(entity.base.velocity));
                }
                return ride_out;
            }
            let packet = self.create_move(entity, path, yaw, pitch, send_position, rotate);
            let fall_flying = matches!(&entity.state, EntityState::Living(l) if l.fall_flying);
            if entity.base.needs_sync || self.track_delta || fall_flying {
                let motion = entity.base.velocity;
                let distance = motion.distance_squared(self.last_sent_movement);
                if distance > MOTION_EPSILON || (distance > 0.0 && motion.length_sqr() == 0.0) {
                    self.last_sent_movement = motion;
                    out.push(TrackerUpdate::Motion(motion));
                }
            }
            match packet {
                Some(movement) => {
                    let has_position = movement.has_position();
                    let has_rotation = movement.has_rotation();
                    out.push(TrackerUpdate::Move(movement));
                    self.dirty_data(entity, &mut out);
                    if has_position {
                        self.base = position;
                    }
                    if has_rotation {
                        self.last_yaw = yaw;
                        self.last_pitch = pitch;
                    }
                }
                None => self.dirty_data(entity, &mut out),
            }
            self.was_riding = false;
            self.finish_rotation(entity, &mut out);
        }
        self.tick_count += 1;
        if entity.base.sync_velocity {
            entity.base.sync_velocity = false;
            out.push(TrackerUpdate::SelfMotion(entity.base.velocity));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::{EntityBase, LivingState};

    fn cow() -> Entity {
        let mut base = EntityBase::new(EntityId(7), Uuid::from_u128(1), EntityKind::Cow);
        base.position = Vec3::new(0.5, 64.0, 0.5);
        let living = LivingState::new(&mut base, 0.0);
        Entity::new(base, EntityState::Living(Box::new(living)))
    }

    #[test]
    fn packing_matches_mth_pack_degrees() {
        assert_eq!(pack_degrees(0.0), 0);
        assert_eq!(pack_degrees(90.0), 64);
        assert_eq!(pack_degrees(-90.0), 192);
        assert_eq!(unpack_degrees(192), -90.0);
        assert_eq!(encode(0.5), 2048);
        assert_eq!(encode(-0.000_1), 0);
    }

    #[test]
    fn living_entities_send_stepped_deltas_on_their_interval() {
        let mut entity = cow();
        let mut tracker = EntityTracker::new(&entity);
        assert!(tracker.pairing().data.iter().any(|(i, _)| *i == 9));
        entity.base.data.pack_dirty();
        let first = tracker.send_changes(&mut entity, false);
        assert_eq!(
            first,
            vec![TrackerUpdate::Move(Movement::Position {
                delta: MoveDelta::Linear {
                    xa: 0,
                    ya: 0,
                    za: 0
                },
                on_ground: false
            })]
        );
        entity.base.position = Vec3::new(0.5, 63.5, 0.5);
        entity.base.velocity = Vec3::new(0.0, -0.5, 0.0);
        let mut updates = Vec::new();
        for _ in 0..3 {
            updates.extend(tracker.send_changes(&mut entity, false));
        }
        let steps = updates.iter().find_map(|u| match u {
            TrackerUpdate::Move(Movement::Position {
                delta: MoveDelta::Stepped(steps),
                ..
            }) => Some(steps.clone()),
            _ => None,
        });
        assert_eq!(
            steps,
            Some(vec![DeltaStep {
                ticks: 3,
                xa: 0,
                ya: -2048,
                za: 0
            }])
        );
        assert!(updates.contains(&TrackerUpdate::Motion(Vec3::new(0.0, -0.5, 0.0))));
    }

    #[test]
    fn ground_change_forces_a_position_sync() {
        let mut entity = cow();
        let mut tracker = EntityTracker::new(&entity);
        entity.base.on_ground = true;
        entity.base.needs_sync = true;
        let updates = tracker.send_changes(&mut entity, false);
        assert!(updates.iter().any(|u| matches!(
            u,
            TrackerUpdate::Move(Movement::Sync {
                on_ground: true,
                ..
            })
        )));
    }

    #[test]
    fn a_rider_sends_only_rotation_then_forces_a_position_sync_after_dismounting() {
        let mut entity = cow();
        let mut tracker = EntityTracker::new(&entity);
        entity.base.position = Vec3::new(4.0, 63.0, 0.5);
        entity.base.yaw = 90.0;
        entity.base.needs_sync = true;
        let updates = tracker.send_changes(&mut entity, true);
        assert!(
            updates
                .iter()
                .any(|u| matches!(u, TrackerUpdate::Move(Movement::Rotation { yaw: 64, .. })))
        );
        assert!(
            !updates
                .iter()
                .any(|u| matches!(u, TrackerUpdate::Move(m) if m.has_position()))
        );
        entity.base.position = Vec3::new(4.2, 63.0, 0.5);
        entity.base.needs_sync = true;
        let updates = tracker.send_changes(&mut entity, false);
        assert!(
            updates
                .iter()
                .any(|u| matches!(u, TrackerUpdate::Move(Movement::Sync { .. })))
        );
    }
}
