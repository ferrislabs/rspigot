use rspigot_entities::{EntityKind, attachments};
use rustc_hash::FxHashMap;

use super::aabb::Aabb;
use super::fluid;
use super::physics::BlockView;
use crate::{BlockPos, EntityId, Vec3};

pub const BOARDING_COOLDOWN: i32 = 60;

#[derive(Debug, Default)]
pub struct Rides {
    vehicles: FxHashMap<EntityId, EntityId>,
    passengers: FxHashMap<EntityId, Vec<EntityId>>,
    cooldowns: FxHashMap<EntityId, i32>,
}

impl Rides {
    pub fn vehicle_of(&self, passenger: EntityId) -> Option<EntityId> {
        self.vehicles.get(&passenger).copied()
    }

    pub fn passengers_of(&self, vehicle: EntityId) -> &[EntityId] {
        self.passengers.get(&vehicle).map_or(&[], Vec::as_slice)
    }

    pub fn is_passenger(&self, entity: EntityId) -> bool {
        self.vehicles.contains_key(&entity)
    }

    pub fn is_vehicle(&self, entity: EntityId) -> bool {
        self.passengers.contains_key(&entity)
    }

    pub fn has_passenger(&self, vehicle: EntityId, passenger: EntityId) -> bool {
        self.passengers_of(vehicle).contains(&passenger)
    }

    pub fn first_passenger(&self, vehicle: EntityId) -> Option<EntityId> {
        self.passengers_of(vehicle).first().copied()
    }

    pub fn root_vehicle(&self, entity: EntityId) -> EntityId {
        let mut current = entity;
        while let Some(vehicle) = self.vehicle_of(current) {
            current = vehicle;
        }
        current
    }

    pub fn has_indirect_passenger(&self, vehicle: EntityId, passenger: EntityId) -> bool {
        let mut current = passenger;
        while let Some(parent) = self.vehicle_of(current) {
            if parent == vehicle {
                return true;
            }
            current = parent;
        }
        false
    }

    pub fn is_passenger_of_same_vehicle(&self, a: EntityId, b: EntityId) -> bool {
        self.root_vehicle(a) == self.root_vehicle(b)
    }

    pub fn indirect_passengers(&self, vehicle: EntityId) -> Vec<EntityId> {
        let mut out = Vec::new();
        self.collect_passengers(vehicle, &mut out);
        out
    }

    fn collect_passengers(&self, vehicle: EntityId, out: &mut Vec<EntityId>) {
        for passenger in self.passengers_of(vehicle) {
            out.push(*passenger);
            self.collect_passengers(*passenger, out);
        }
    }

    pub fn self_and_passengers(&self, entity: EntityId) -> Vec<EntityId> {
        let mut out = vec![entity];
        self.collect_passengers(entity, &mut out);
        out
    }

    pub fn attach(
        &mut self,
        vehicle: EntityId,
        passenger: EntityId,
        is_player: impl Fn(EntityId) -> bool,
    ) {
        self.vehicles.insert(passenger, vehicle);
        let list = self.passengers.entry(vehicle).or_default();
        let player_goes_first = !list.is_empty()
            && is_player(passenger)
            && list.first().is_some_and(|first| !is_player(*first));
        if player_goes_first {
            list.insert(0, passenger);
        } else {
            list.push(passenger);
        }
    }

    pub fn detach(&mut self, vehicle: EntityId, passenger: EntityId) {
        self.vehicles.remove(&passenger);
        if let Some(list) = self.passengers.get_mut(&vehicle) {
            list.retain(|candidate| *candidate != passenger);
            if list.is_empty() {
                self.passengers.remove(&vehicle);
            }
        }
        self.cooldowns.insert(passenger, BOARDING_COOLDOWN);
    }

    pub fn boarding_cooldown(&self, entity: EntityId) -> i32 {
        self.cooldowns.get(&entity).copied().unwrap_or(0)
    }

    pub fn set_boarding_cooldown(&mut self, entity: EntityId, ticks: i32) {
        if ticks > 0 {
            self.cooldowns.insert(entity, ticks);
        } else {
            self.cooldowns.remove(&entity);
        }
    }

    pub fn tick_cooldowns(&mut self) {
        self.cooldowns.retain(|_, ticks| {
            *ticks -= 1;
            *ticks > 0
        });
    }

    pub fn forget(&mut self, entity: EntityId) {
        if let Some(vehicle) = self.vehicles.remove(&entity)
            && let Some(list) = self.passengers.get_mut(&vehicle)
        {
            list.retain(|candidate| *candidate != entity);
            if list.is_empty() {
                self.passengers.remove(&vehicle);
            }
        }
        if let Some(list) = self.passengers.remove(&entity) {
            for passenger in list {
                self.vehicles.remove(&passenger);
            }
        }
        self.cooldowns.remove(&entity);
    }
}

pub fn max_passengers(kind: EntityKind) -> usize {
    if kind.extends("AbstractChestBoat") {
        1
    } else if kind.extends("AbstractBoat")
        || matches!(kind, EntityKind::Camel | EntityKind::CamelHusk)
    {
        2
    } else if kind == EntityKind::HappyGhast {
        4
    } else {
        1
    }
}

pub fn is_bukkit_vehicle(kind: EntityKind) -> bool {
    kind.extends("AbstractHorse")
        || kind.extends("AbstractBoat")
        || kind.extends("AbstractMinecart")
        || matches!(
            kind,
            EntityKind::HappyGhast | EntityKind::Pig | EntityKind::Strider
        )
}

fn rotate_y(point: Vec3, yaw_degrees: f32) -> Vec3 {
    let angle = f64::from(-yaw_degrees * (std::f32::consts::PI / 180.0));
    let (sin, cos) = (angle.sin(), angle.cos());
    Vec3::new(
        point.x * cos + point.z * sin,
        point.y,
        point.z * cos - point.x * sin,
    )
}

fn scaled(point: [f64; 3], scale: f64) -> Vec3 {
    Vec3::new(point[0] * scale, point[1] * scale, point[2] * scale)
}

pub fn passenger_attachment(
    kind: EntityKind,
    baby: bool,
    height: f32,
    scale: f64,
    yaw: f32,
    index: usize,
) -> Vec3 {
    let table = match baby.then(|| attachments::baby_points(kind)).flatten() {
        Some((passenger, _)) => Some((passenger, 1.0)),
        None => attachments::passenger_points(kind).map(|points| (points, scale)),
    };
    let point = match table {
        Some((points, factor)) => scaled(points[index.min(points.len() - 1)], factor),
        None => Vec3::new(0.0, f64::from(height), 0.0),
    };
    rotate_y(point, yaw)
}

pub fn vehicle_attachment(kind: EntityKind, baby: bool, scale: f64, yaw: f32) -> Vec3 {
    let default = [[0.0, 0.0, 0.0]];
    let (points, scale) = match baby.then(|| attachments::baby_points(kind)).flatten() {
        Some((_, vehicle)) => (vehicle, 1.0),
        None => (attachments::vehicle_points(kind).unwrap_or(&default), scale),
    };
    rotate_y(scaled(points[0], scale), yaw)
}

pub fn collision_escape(vehicle_width: f64, passenger_width: f64, yaw: f32) -> Vec3 {
    let reach = (vehicle_width + passenger_width + f64::from(1.0e-5_f32)) / 2.0;
    let radians = yaw * (std::f32::consts::PI / 180.0);
    let sin = -radians.sin();
    let cos = radians.cos();
    let scale = f64::from(sin.abs().max(cos.abs()));
    Vec3::new(
        f64::from(sin) * reach / scale,
        0.0,
        f64::from(cos) * reach / scale,
    )
}

pub fn facing(yaw: f32) -> (i32, i32) {
    match (f64::from(yaw) / 90.0 + 0.5).floor() as i32 & 3 {
        0 => (0, 1),
        1 => (-1, 0),
        2 => (0, -1),
        _ => (1, 0),
    }
}

pub fn dismount_offsets(forward: (i32, i32)) -> [(i32, i32); 8] {
    let right = (-forward.1, forward.0);
    let left = (forward.1, -forward.0);
    let back = (-forward.0, -forward.1);
    [
        right,
        left,
        (back.0 + right.0, back.1 + right.1),
        (back.0 + left.0, back.1 + left.1),
        (forward.0 + right.0, forward.1 + right.1),
        (forward.0 + left.0, forward.1 + left.1),
        back,
        forward,
    ]
}

fn shape_top(view: &impl BlockView, pos: BlockPos) -> f64 {
    view.block_or_air(pos)
        .collision_shape()
        .iter()
        .map(|shape| shape.max_y)
        .fold(f64::NEG_INFINITY, f64::max)
}

pub fn block_floor_height(view: &impl BlockView, pos: BlockPos) -> f64 {
    let top = shape_top(view, pos);
    if top.is_finite() {
        return top;
    }
    let below = shape_top(view, pos.offset(0, -1, 0));
    if below >= 1.0 {
        below - 1.0
    } else {
        f64::NEG_INFINITY
    }
}

pub fn is_block_floor_valid(height: f64) -> bool {
    !height.is_infinite() && height < 1.0
}

pub fn can_dismount_to(view: &impl BlockView, area: &Aabb) -> bool {
    fluid::is_free_of_collision(view, area)
}

pub fn up_from_bottom_center(pos: BlockPos, dy: f64) -> Vec3 {
    Vec3::new(
        f64::from(pos.x) + 0.5,
        f64::from(pos.y) + dy,
        f64::from(pos.z) + 0.5,
    )
}

pub fn animal_dismount(
    view: &impl BlockView,
    vehicle_block: BlockPos,
    yaw: f32,
    rider_width: f32,
    rider_height: f32,
) -> Option<Vec3> {
    let local = Aabb::of_size(Vec3::ZERO, rider_width, rider_height);
    for (dx, dz) in dismount_offsets(facing(yaw)) {
        let pos = vehicle_block.offset(dx, 0, dz);
        let height = block_floor_height(view, pos);
        if !is_block_floor_valid(height) {
            continue;
        }
        let spot = up_from_bottom_center(pos, height);
        if can_dismount_to(view, &local.move_by(spot)) {
            return Some(spot);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: i32) -> EntityId {
        EntityId(value)
    }

    #[test]
    fn a_player_boards_ahead_of_a_mob_passenger() {
        let mut rides = Rides::default();
        let is_player = |entity: EntityId| entity.0 >= 100;
        rides.attach(id(1), id(2), is_player);
        rides.attach(id(1), id(100), is_player);
        rides.attach(id(1), id(3), is_player);
        assert_eq!(rides.passengers_of(id(1)), &[id(100), id(2), id(3)]);
        assert_eq!(rides.first_passenger(id(1)), Some(id(100)));
        assert_eq!(rides.vehicle_of(id(3)), Some(id(1)));
    }

    #[test]
    fn leaving_starts_the_boarding_cooldown_that_counts_down() {
        let mut rides = Rides::default();
        rides.attach(id(1), id(2), |_| false);
        rides.detach(id(1), id(2));
        assert!(!rides.is_vehicle(id(1)));
        assert_eq!(rides.boarding_cooldown(id(2)), BOARDING_COOLDOWN);
        for _ in 0..BOARDING_COOLDOWN {
            rides.tick_cooldowns();
        }
        assert_eq!(rides.boarding_cooldown(id(2)), 0);
    }

    #[test]
    fn stacked_riders_are_found_through_the_whole_chain() {
        let mut rides = Rides::default();
        rides.attach(id(1), id(2), |_| false);
        rides.attach(id(2), id(3), |_| false);
        assert_eq!(rides.root_vehicle(id(3)), id(1));
        assert!(rides.has_indirect_passenger(id(1), id(3)));
        assert!(!rides.has_indirect_passenger(id(3), id(1)));
        assert_eq!(rides.indirect_passengers(id(1)), vec![id(2), id(3)]);
        assert_eq!(rides.self_and_passengers(id(2)), vec![id(2), id(3)]);
        assert!(rides.is_passenger_of_same_vehicle(id(2), id(3)));
        rides.forget(id(2));
        assert!(!rides.is_passenger(id(3)));
        assert!(!rides.is_vehicle(id(1)));
    }

    #[test]
    fn seats_follow_the_vehicle_kind() {
        assert_eq!(max_passengers(EntityKind::OakBoat), 2);
        assert_eq!(max_passengers(EntityKind::OakChestBoat), 1);
        assert_eq!(max_passengers(EntityKind::HappyGhast), 4);
        assert_eq!(max_passengers(EntityKind::Strider), 1);
        assert!(is_bukkit_vehicle(EntityKind::Strider));
        assert!(!is_bukkit_vehicle(EntityKind::Chicken));
    }

    #[test]
    fn attachment_points_rotate_with_the_vehicle_yaw() {
        let chicken = passenger_attachment(EntityKind::Chicken, false, 0.7, 1.0, 90.0, 0);
        assert!((chicken.x - 0.1).abs() < 1e-6 || (chicken.x + 0.1).abs() < 1e-6);
        assert!((chicken.y - 0.7).abs() < 1e-9);
        let zombie = vehicle_attachment(EntityKind::Zombie, false, 1.0, 0.0);
        assert!((zombie.y - 0.7).abs() < 1e-6);
        let baby = vehicle_attachment(EntityKind::Zombie, true, 0.5, 0.0);
        assert!((baby.y - 0.1875).abs() < 1e-9);
        let default = passenger_attachment(EntityKind::Strider, false, 1.7, 1.0, 0.0, 3);
        assert!((default.y - 1.7).abs() < 1e-6);
        let baby_hoglin = passenger_attachment(EntityKind::Hoglin, true, 0.85, 0.6, 0.0, 0);
        assert!((baby_hoglin.y - 0.875).abs() < 1e-9);
    }

    #[test]
    fn the_escape_vector_leaves_through_the_nearest_side() {
        let ahead = collision_escape(0.9, 0.6, 0.0);
        assert!(ahead.x.abs() < 1e-9);
        assert!((ahead.z - 0.75).abs() < 1e-4);
        let diagonal = collision_escape(0.9, 0.6, 45.0);
        assert!((diagonal.x.abs() - 0.75).abs() < 1e-4);
        assert!((diagonal.z - 0.75).abs() < 1e-4);
    }

    #[test]
    fn dismount_offsets_start_at_the_right_of_the_facing() {
        assert_eq!(facing(0.0), (0, 1));
        assert_eq!(facing(90.0), (-1, 0));
        assert_eq!(facing(180.0), (0, -1));
        assert_eq!(facing(-90.0), (1, 0));
        let offsets = dismount_offsets((0, 1));
        assert_eq!(offsets[0], (-1, 0));
        assert_eq!(offsets[1], (1, 0));
        assert_eq!(offsets[6], (0, -1));
        assert_eq!(offsets[7], (0, 1));
        assert_eq!(offsets[2], (-1, -1));
    }

    struct Floor;

    impl BlockView for Floor {
        fn block(&self, pos: BlockPos) -> Option<crate::BlockStateId> {
            Some(if pos.y < 64 {
                rspigot_blocks::BlockKind::Stone.default_state()
            } else {
                crate::BlockStateId::AIR
            })
        }
    }

    #[test]
    fn an_animal_lets_its_rider_down_beside_it_on_open_ground() {
        let spot = animal_dismount(&Floor, BlockPos::new(0, 64, 0), 0.0, 0.6, 1.8);
        assert_eq!(spot, Some(Vec3::new(-0.5, 64.0, 0.5)));
        assert!(is_block_floor_valid(block_floor_height(
            &Floor,
            BlockPos::new(0, 64, 0)
        )));
        assert!(!is_block_floor_valid(block_floor_height(
            &Floor,
            BlockPos::new(0, 70, 0)
        )));
    }
}
