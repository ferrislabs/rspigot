use rspigot_domain::entity::hurting::{self, HurtingState};
use rspigot_domain::entity::thrown;
use rspigot_domain::entity::{Entity, EntityState, RemovalReason, raycast};
use rspigot_domain::explosion::{self, PACKET_RANGE_SQR};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{EntityId, Vec3, mth};
use rspigot_entities::EntityKind as Kind;

use crate::events::ClientEvent;
use crate::mobs::ServerRandom;
use crate::plugins::events::{CreatureSpawnReason, EntityRemoveCause};
use crate::plugins::steps::HurtingFollow;
use crate::plugins::{AfterHurt, BlastFollow, Struck};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const EXPLOSION_KNOCKBACK_RESISTANCE: &str = "minecraft:explosion_knockback_resistance";
const BREEZE_BURST_SOUND: &str = "minecraft:entity.breeze.wind_burst";
const WIND_CHARGE_BURST_SOUND: &str = "minecraft:entity.wind_charge.wind_burst";
const LARGE_BURST_RADIUS: f32 = 2.0;
const MIN_BURST_RADIUS: f32 = 1.0e-5;

fn burst_of(kind: Kind) -> (f32, f32, &'static str) {
    match kind {
        Kind::WindCharge => (
            hurting::WIND_CHARGE_RADIUS,
            hurting::WIND_CHARGE_KNOCKBACK,
            WIND_CHARGE_BURST_SOUND,
        ),
        _ => (hurting::BREEZE_WIND_CHARGE_RADIUS, 1.0, BREEZE_BURST_SOUND),
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn launch_wind_charge(
        &mut self,
        kind: Kind,
        owner: EntityId,
        position: Vec3,
        direction: Vec3,
        power: f32,
        uncertainty: f32,
    ) -> Option<EntityId> {
        let owner_uuid = self.entities.store.get(owner)?.base.uuid;
        let mut base = self.new_entity_base(kind);
        base.position = position;
        base.old_position = position;
        let mut state = HurtingState::new(&mut base);
        state.owner = Some(owner_uuid);
        let movement = thrown::movement_to_shoot(
            &mut ServerRandom(&mut self.rng),
            direction,
            power,
            uncertainty,
        );
        base.velocity = movement;
        base.needs_sync = true;
        let horizontal = (movement.x * movement.x + movement.z * movement.z).sqrt();
        base.set_yaw((mth::atan2(movement.x, movement.z) * mth::RAD_TO_DEG) as f32);
        base.set_pitch((mth::atan2(movement.y, horizontal) * mth::RAD_TO_DEG) as f32);
        let id = base.id;
        let projectile = Entity::new(base, EntityState::Hurting(Box::new(state)));
        self.add_entity(projectile, CreatureSpawnReason::Default);
        Some(id)
    }

    pub(crate) fn wind_charge_out_of_world(&mut self, id: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        if !hurting::is_wind_charge(entity.base.kind) {
            return false;
        }
        let height = self.world.dimension.height;
        let limit = height.min_y + height.height as i32 - 1;
        if entity.base.position.block().y <= limit + hurting::WIND_CHARGE_OUT_OF_WORLD_MARGIN {
            return false;
        }
        let at = entity.base.position;
        self.wind_charge_explode(id, at);
        self.remove_entity(
            id,
            RemovalReason::Discarded,
            Some(EntityRemoveCause::OutOfWorld),
        );
        true
    }

    pub(crate) fn wind_charge_age(&mut self, id: EntityId) {
        if let Some(Entity {
            state: EntityState::Hurting(state),
            base,
            ..
        }) = self.entities.store.get_mut(id)
            && base.kind == Kind::WindCharge
            && state.no_deflect_ticks > 0
        {
            state.no_deflect_ticks -= 1;
        }
    }

    pub(crate) fn wind_charge_hit_entity(
        &mut self,
        id: EntityId,
        victim: Struck,
        follow: HurtingFollow,
        causing: Option<crate::health::SourceEntity>,
        direct: crate::health::SourceEntity,
    ) {
        let velocity = self
            .entities
            .store
            .get(id)
            .map_or(Vec3::ZERO, |entity| entity.base.velocity);
        let source = crate::health::DamageSource {
            kind: rspigot_domain::damage::DamageType::WindCharge,
            causing,
            direct: Some(direct),
            position: None,
            knockback: Some((-velocity.x, -velocity.z)),
            direct_block: None,
        };
        self.strike(
            None,
            victim,
            &source,
            hurting::WIND_CHARGE_DAMAGE,
            AfterHurt::Hurting(follow),
        );
    }

    pub(crate) fn wind_charge_hit_block(&mut self, id: EntityId, hit: raycast::BlockHit) {
        let (nx, ny, nz) = hit.face.offset();
        let offset = Vec3::new(f64::from(nx), f64::from(ny), f64::from(nz))
            .scale(hurting::WIND_CHARGE_BLOCK_OFFSET);
        self.wind_charge_explode(id, hit.location + offset);
        self.remove_entity(id, RemovalReason::Discarded, Some(EntityRemoveCause::Hit));
    }

    pub(crate) fn wind_charge_explode(&mut self, id: EntityId, center: Vec3) {
        let Some(kind) = self.entities.store.get(id).map(|entity| entity.base.kind) else {
            return;
        };
        let (radius, multiplier, sound) = burst_of(kind);
        self.wind_burst(Some(id), center, radius, multiplier, sound);
    }

    pub(crate) fn wind_burst(
        &mut self,
        source: Option<EntityId>,
        center: Vec3,
        radius: f32,
        knockback_multiplier: f32,
        sound: &'static str,
    ) {
        if radius < MIN_BURST_RADIUS {
            return;
        }
        let outer = self.impacts.knocked.replace(Vec::new());
        let area = explosion::hurt_area(center, radius);
        for target in self.explosion_targets(source, &area) {
            let distance =
                target.position.distance_squared(center).sqrt() / f64::from(radius * 2.0);
            if distance > 1.0 {
                continue;
            }
            let flying = target.player.is_some_and(|player| {
                self.players
                    .get(&player)
                    .is_some_and(|player| player.flying)
            });
            let multiplier = if flying { 0.0 } else { knockback_multiplier };
            let exposure = if multiplier == 0.0 {
                0.0
            } else {
                explosion::seen_percent(&self.world.chunks, center, &target.bounds)
            };
            let resistance = self
                .entities
                .store
                .get_mut(target.id)
                .and_then(|entity| entity.living_mut())
                .map_or(0.0, |living| {
                    living.attribute(EXPLOSION_KNOCKBACK_RESISTANCE)
                });
            let direction = (target.eye - center).normalize();
            let power =
                (1.0 - distance) * f64::from(exposure) * f64::from(multiplier) * (1.0 - resistance);
            self.blast_knockback(BlastFollow {
                target: target.id,
                player: target.player,
                knockback: direction.scale(power),
                source,
            });
        }
        let knocked = std::mem::replace(&mut self.impacts.knocked, outer).unwrap_or_default();
        self.send_burst(center, radius, &knocked, sound);
    }

    fn send_burst(
        &mut self,
        center: Vec3,
        radius: f32,
        knocked: &[(PlayerId, Vec3)],
        sound: &'static str,
    ) {
        let viewers: Vec<PlayerId> = self
            .players
            .values()
            .filter(|player| player.position.distance_squared(center) < PACKET_RANGE_SQR)
            .map(|player| player.id)
            .collect();
        for player in viewers {
            let knockback = knocked
                .iter()
                .find(|(id, _)| *id == player)
                .map(|(_, push)| *push);
            self.push(
                Audience::One(player),
                ClientEvent::Explode {
                    center,
                    radius,
                    block_count: 0,
                    knockback,
                    large: radius >= LARGE_BURST_RADIUS,
                    play_sound: true,
                    gust: Some(sound),
                },
            );
        }
    }
}
