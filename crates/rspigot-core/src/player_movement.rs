use rspigot_blocks::StateFlags;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::physics::collide_bounding_box;
use rspigot_domain::entity::{Pose, living};
use rspigot_domain::movement::{
    Checks, FLOATING_TICKS, Pace, PacketRate, PoseInput, TELEPORT_RESEND_TICKS, is_floating,
    moved_too_quickly, player_box, rejects_move, settle_pose, swimming_after, wanted_pose,
};
use rspigot_domain::player::{ClientSettings, GameMode, MainHand, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::{BlockPos, EntityId, Vec3};

use crate::events::{ClientEvent, MetadataEntry, MetadataValue, StateKind};
use crate::health::DamageSource;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const LEVITATION: &str = "minecraft:levitation";
const MAX_HORIZONTAL: f64 = 3.0e7;
const MAX_VERTICAL: f64 = 2.0e7;
const WALK_SPEED_SCALE: f32 = 10.0;
const FLY_SPEED_SCALE: f32 = 20.0;
const WALL_DAMAGE_SCALE: f64 = 10.0;
const WALL_DAMAGE_OFFSET: f64 = 3.0;

#[derive(Default)]
pub(crate) struct MoveState {
    pub(crate) rate: PacketRate,
    pub(crate) first_good: Option<Vec3>,
    pub(crate) floating: bool,
    pub(crate) above_ground: i32,
    pub(crate) swimming: bool,
    pub(crate) squeeze: Option<Pose>,
    pub(crate) sent_stance: Option<(bool, Option<Pose>)>,
    pub(crate) sent_options: Option<(u8, MainHand)>,
    pub(crate) sent_effects: Option<(Vec<rspigot_domain::entity::ParticleSetting>, bool)>,
    pub(crate) teleport_watch: Option<(i32, u64)>,
    pub(crate) camera: Option<EntityId>,
    pub(crate) destroy_direction: Option<crate::events::Direction>,
}

pub(crate) fn invalid_coordinates(target: Vec3) -> bool {
    !target.is_finite()
        || target.x.abs() > MAX_HORIZONTAL
        || target.z.abs() > MAX_HORIZONTAL
        || target.y.abs() > MAX_VERTICAL
}

pub(crate) fn options_entries(settings: &ClientSettings) -> [crate::events::MetadataEntry; 2] {
    [
        MetadataEntry {
            index: MAIN_HAND_INDEX,
            value: MetadataValue::State(
                StateKind::HumanoidArm,
                match settings.main_hand {
                    MainHand::Left => 0,
                    MainHand::Right => 1,
                },
            ),
        },
        MetadataEntry {
            index: SKIN_PARTS_INDEX,
            value: MetadataValue::Byte(i8::from_ne_bytes([settings.skin_parts])),
        },
    ]
}

pub(crate) const PAIRED_ATTRIBUTES: [&str; 22] = [
    "minecraft:max_health",
    "minecraft:armor",
    "minecraft:armor_toughness",
    "minecraft:movement_speed",
    "minecraft:attack_speed",
    "minecraft:luck",
    "minecraft:scale",
    "minecraft:step_height",
    "minecraft:gravity",
    "minecraft:jump_strength",
    "minecraft:max_absorption",
    "minecraft:block_interaction_range",
    "minecraft:entity_interaction_range",
    "minecraft:block_break_speed",
    "minecraft:sneaking_speed",
    "minecraft:submerged_mining_speed",
    "minecraft:mining_efficiency",
    "minecraft:movement_efficiency",
    "minecraft:water_movement_efficiency",
    "minecraft:oxygen_bonus",
    "minecraft:safe_fall_distance",
    "minecraft:fall_damage_multiplier",
];
pub(crate) const MAIN_HAND_INDEX: u8 = 15;
pub(crate) const SKIN_PARTS_INDEX: u8 = 16;
pub(crate) const EFFECT_PARTICLES_INDEX: u8 = 10;
pub(crate) const EFFECT_AMBIENCE_INDEX: u8 = 11;

impl<L: ClientLink> Server<L> {
    pub(crate) fn water_at(&self, pos: BlockPos) -> bool {
        self.world
            .chunks
            .block(pos)
            .is_some_and(|state| state.flags().contains(StateFlags::WATER))
    }

    pub(crate) fn count_move_packet(&mut self, id: PlayerId, moved: bool) -> i32 {
        let tick = self.tick;
        self.players
            .get_mut(&id)
            .map_or(1, |player| player.motion.rate.on_packet(tick, moved))
    }

    pub(crate) fn rubber_band(&mut self, id: PlayerId, to: Vec3) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let teleport = player.next_teleport_id();
        player.link.send(ClientEvent::Teleport {
            id: teleport,
            position: to,
            rotation: player.rotation,
        });
    }

    pub(crate) fn validate_move(
        &mut self,
        id: PlayerId,
        from: Vec3,
        target: Vec3,
        packets: i32,
    ) -> bool {
        if invalid_coordinates(target) {
            self.disconnect_player(
                id,
                Component::translatable(
                    "multiplayer.disconnect.invalid_player_movement",
                    Vec::new(),
                ),
            );
            return false;
        }
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let rules = &self.config.rules;
        let fall_flying = player.fall_flying;
        let checked = rules.player_movement_check && (rules.elytra_movement_check || !fall_flying);
        let abilities = player.abilities();
        let speed = if abilities.flying {
            abilities.fly_speed * FLY_SPEED_SCALE
        } else {
            abilities.walk_speed * WALK_SPEED_SCALE
        };
        let anchor = player.motion.first_good.unwrap_or(from);
        let pace = Pace {
            distance_sq: anchor.distance_squared(target),
            velocity_sq: player.vitals.velocity.length_sqr(),
            fall_flying,
            packets,
            speed,
            multiplier: self.config.moved_too_quickly_multiplier,
        };
        if checked && moved_too_quickly(&pace) {
            tracing::warn!(
                "{} moved too quickly! {:.2},{:.2},{:.2}",
                player.profile.name,
                target.x - anchor.x,
                target.y - anchor.y,
                target.z - anchor.z
            );
            self.rubber_band(id, from);
            return false;
        }
        let creative = player.game_mode == GameMode::Creative;
        let spectator = player.game_mode == GameMode::Spectator;
        let checks = Checks {
            moved_wrongly: !creative && !spectator,
            collisions: !spectator,
        };
        if player.sleeping.is_none()
            && rejects_move(
                &self.world.chunks,
                from,
                target,
                player.pose(),
                player.on_ground,
                checks,
            )
        {
            tracing::warn!("{} moved wrongly!", player.profile.name);
            self.rubber_band(id, from);
            return false;
        }
        true
    }

    pub(crate) fn note_floating(&mut self, id: PlayerId, from: Vec3, to: Vec3, on_ground: bool) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let exempt = player.game_mode == GameMode::Spectator
            || self.config.allow_flight
            || player.abilities().may_fly
            || player.effects.has(LEVITATION)
            || player.fall_flying
            || player.spin.is_some()
            || self.entities.rides.is_passenger(player.entity_id);
        let floating = is_floating(&self.world.chunks, to, to.y - from.y, on_ground, exempt);
        if let Some(player) = self.players.get_mut(&id) {
            player.motion.floating = floating;
        }
    }

    pub(crate) fn fly_into_wall(&mut self, id: PlayerId, from: Vec3, previous: Vec3, now: Vec3) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if !player.fall_flying {
            return;
        }
        let old_speed = previous.horizontal_distance_sqr().sqrt();
        let new_speed = now.horizontal_distance_sqr().sqrt();
        let damage = (old_speed - new_speed) * WALL_DAMAGE_SCALE - WALL_DAMAGE_OFFSET;
        if damage <= 0.0 {
            return;
        }
        let ahead = Vec3::new(previous.x, 0.0, previous.z);
        let moved = collide_bounding_box(
            &self.world.chunks,
            ahead,
            &player_box(from, Pose::FallFlying),
        );
        if moved.x == ahead.x && moved.z == ahead.z {
            return;
        }
        self.hurt(
            id,
            &DamageSource::new(DamageType::FlyIntoWall),
            damage as f32,
        );
    }

    pub(crate) fn tick_player_movement(&mut self) {
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        let mut kicked = Vec::new();
        for id in &ids {
            let Some(player) = self.players.get_mut(id) else {
                continue;
            };
            player.motion.first_good = Some(player.position);
            player.motion.rate.end_tick();
            let idle = !player.motion.floating
                || player.sleeping.is_some()
                || player.vitals.is_dead()
                || self.entities.rides.is_passenger(player.entity_id);
            if idle {
                player.motion.floating = false;
                player.motion.above_ground = 0;
            } else {
                player.motion.above_ground += 1;
                if player.motion.above_ground > FLOATING_TICKS {
                    tracing::warn!("{} was kicked for floating too long!", player.profile.name);
                    kicked.push(*id);
                }
            }
        }
        for id in kicked {
            self.disconnect_player(
                id,
                Component::translatable("multiplayer.disconnect.flying", Vec::new()),
            );
        }
        for id in &ids {
            self.resend_stale_teleport(*id);
            self.follow_camera(*id);
            self.refresh_stance(*id);
            self.refresh_options(*id);
            self.refresh_effect_particles(*id);
        }
    }

    fn refresh_effect_particles(&mut self, id: PlayerId) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let current = rspigot_domain::effect_particles::synchronized(&player.effects);
        if player.motion.sent_effects.as_ref() == Some(&current) {
            return;
        }
        let first = player.motion.sent_effects.is_none();
        let (particles, ambient) = current.clone();
        player.motion.sent_effects = Some(current);
        if first && particles.is_empty() && !ambient {
            return;
        }
        let entity = player.entity_id;
        let event = ClientEvent::EntityMetadata {
            entity,
            entries: vec![
                MetadataEntry {
                    index: EFFECT_PARTICLES_INDEX,
                    value: MetadataValue::Particles(particles),
                },
                MetadataEntry {
                    index: EFFECT_AMBIENCE_INDEX,
                    value: MetadataValue::Boolean(ambient),
                },
            ],
        };
        self.push(Audience::One(id), event.clone());
        self.push(Audience::Trackers(entity), event);
    }

    fn resend_stale_teleport(&mut self, id: PlayerId) {
        let tick = self.tick;
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let Some(pending) = player.pending_teleport else {
            player.motion.teleport_watch = None;
            return;
        };
        match player.motion.teleport_watch {
            Some((watched, since)) if watched == pending => {
                if tick.saturating_sub(since) > TELEPORT_RESEND_TICKS {
                    player.motion.teleport_watch = Some((pending, tick));
                    player.link.send(ClientEvent::Teleport {
                        id: pending,
                        position: player.position,
                        rotation: player.rotation,
                    });
                }
            }
            _ => player.motion.teleport_watch = Some((pending, tick)),
        }
    }

    fn refresh_stance(&mut self, id: PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let passenger = self.entities.rides.is_passenger(player.entity_id);
        let feet_water = self.water_at(player.position.block());
        let eye_water = self.water_at(player.eye_position().block());
        let flying = player.flying || player.game_mode == GameMode::Spectator;
        let swimming = swimming_after(
            player.motion.swimming,
            flying,
            player.sprinting,
            passenger,
            feet_water,
            feet_water && eye_water,
            feet_water,
        );
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        player.motion.swimming = swimming;
        let input: PoseInput = player.pose_input(passenger);
        let settled = settle_pose(&self.world.chunks, player.position, &input);
        player.motion.squeeze = (settled != wanted_pose(&input)).then_some(settled);
        let stance = (swimming, player.motion.squeeze);
        if player.motion.sent_stance == Some(stance) {
            return;
        }
        let first = player.motion.sent_stance.is_none();
        player.motion.sent_stance = Some(stance);
        if first {
            return;
        }
        self.resend_metadata(id);
    }

    fn refresh_options(&mut self, id: PlayerId) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let options = (player.settings.skin_parts, player.settings.main_hand);
        if player.motion.sent_options == Some(options) {
            return;
        }
        let first = player.motion.sent_options.is_none();
        player.motion.sent_options = Some(options);
        if first {
            return;
        }
        let entity = player.entity_id;
        let event = ClientEvent::EntityMetadata {
            entity,
            entries: options_entries(&player.settings).to_vec(),
        };
        self.push(Audience::One(id), event.clone());
        self.push(Audience::Trackers(entity), event);
    }

    pub(crate) fn resend_metadata(&mut self, id: PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let entity = player.entity_id;
        let event = ClientEvent::EntityMetadata {
            entity,
            entries: Self::metadata(player),
        };
        self.push(Audience::One(id), event.clone());
        self.push(Audience::Trackers(entity), event);
    }

    fn follow_camera(&mut self, id: PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let Some(camera) = player.motion.camera else {
            return;
        };
        let target = self
            .players
            .values()
            .find(|other| other.entity_id == camera && other.id != id)
            .filter(|other| !other.vitals.is_dead() && !other.vitals.removed)
            .map(|other| (other.position, other.rotation))
            .or_else(|| {
                let entity = self.entities.store.get(camera)?;
                let alive = !entity.base.is_removed()
                    && entity
                        .living()
                        .is_none_or(|state| !living::is_dead_or_dying(&entity.base, state));
                alive.then(|| {
                    (
                        entity.base.position,
                        rspigot_domain::Rotation::new(entity.base.yaw, entity.base.pitch),
                    )
                })
            });
        let leave = player.sneaking
            || player.game_mode != GameMode::Spectator
            || player.vitals.is_dead()
            || target.is_none();
        if leave {
            self.leave_camera(id);
            return;
        }
        if let Some((position, rotation)) = target
            && (player.position != position || player.rotation != rotation)
        {
            self.apply_move(id, Some(position), Some(rotation), false);
        }
    }

    pub(crate) fn leave_camera(&mut self, id: PlayerId) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if player.motion.camera.take().is_none() {
            return;
        }
        let own = player.entity_id;
        self.push(Audience::One(id), ClientEvent::Camera(own));
    }
}
