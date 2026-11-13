use rspigot_ai::{
    MobBody, SONIC_BOOM_HORIZONTAL_KNOCKBACK, SONIC_BOOM_VERTICAL_KNOCKBACK, WardenAction,
};
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::species::warden;
use rspigot_domain::entity::{Entity, RemovalReason};
use rspigot_domain::particle::Particle;
use rspigot_domain::spawn::SpawnReason;
use rspigot_domain::{BlockPos, EntityId, SectionPos, Vec3};
use rspigot_entities::EntityKind;

use crate::events::ClientEvent;
use crate::health::DamageSource;
use crate::mobs::sky_darken;
use crate::plugins::events::EntityRemoveCause;
use crate::plugins::steps::{AfterHurt, Struck};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use rspigot_domain::spawn::natural::NaturalSpawnHost;

const BOOM_PARTICLE: &str = "minecraft:sonic_boom";
const BOOM_SOUND: &str = "minecraft:entity.warden.sonic_boom";
const BOOM_PARTICLE_PADDING: i32 = 7;
const AGITATED: &str = "minecraft:entity.warden.agitated";
const ANGER_KEY: &str = rspigot_domain::warden::ANGER_KEY;
const LISTENER_KEY: &str = rspigot_domain::warden::LISTENER_KEY;

impl<L: ClientLink> Server<L> {
    pub(crate) fn warden_action(&mut self, id: EntityId, action: WardenAction) {
        match action {
            WardenAction::Discard => {
                self.remove_entity(
                    id,
                    RemovalReason::Discarded,
                    Some(EntityRemoveCause::Despawn),
                );
            }
            WardenAction::ClientAngerLevel(level) => {
                if let Some(entity) = self.entities.store.get_mut(id) {
                    warden::set_client_anger_level(&mut entity.base, level);
                }
            }
            WardenAction::Darkness => {
                if let Some(at) = self.sculk_entity_position(id) {
                    self.apply_darkness_around(at, Some(id), rspigot_ai::DARKNESS_RADIUS);
                }
            }
            WardenAction::Unmount => {}
            WardenAction::SonicBoom {
                target,
                origin,
                direction,
                length,
            } => self.sonic_boom(id, target, origin, direction, length),
        }
    }

    fn sonic_boom(
        &mut self,
        id: EntityId,
        target: EntityId,
        origin: Vec3,
        direction: Vec3,
        length: f64,
    ) {
        let steps = length.floor() as i32 + BOOM_PARTICLE_PADDING;
        for step in 1..steps {
            let at = origin + direction.scale(f64::from(step));
            self.push(
                Audience::Trackers(id),
                ClientEvent::Particle(Particle::single(BOOM_PARTICLE, at)),
            );
        }
        self.entity_sound(id, BOOM_SOUND, 3.0, 1.0);
        let Some(source) = self.mob_source(id) else {
            return;
        };
        let victim = match self.players.values().find(|p| p.entity_id == target) {
            Some(player) => Struck::Player(player.id),
            None => Struck::Mob(target),
        };
        let damage = DamageSource::by(DamageType::SonicBoom, source);
        let hurt = self.strike(
            None,
            victim,
            &damage,
            rspigot_ai::SONIC_BOOM_DAMAGE,
            AfterHurt::Nothing,
        );
        if hurt {
            self.push_by_sonic_boom(target, direction);
        }
    }

    fn push_by_sonic_boom(&mut self, target: EntityId, direction: Vec3) {
        let resistance = self.knockback_resistance_of(target);
        let vertical = (1.0 - resistance) * SONIC_BOOM_VERTICAL_KNOCKBACK;
        let horizontal = (1.0 - resistance) * SONIC_BOOM_HORIZONTAL_KNOCKBACK;
        let push = Vec3::new(
            direction.x * horizontal,
            direction.y * vertical,
            direction.z * horizontal,
        );
        if let Some(player) = self.players.values_mut().find(|p| p.entity_id == target) {
            player.vitals.velocity = player.vitals.velocity + push;
            player.vitals.sync_velocity = true;
        } else if let Some(entity) = self.entities.store.get_mut(target) {
            entity.base.velocity = entity.base.velocity + push;
            entity.base.needs_sync = true;
        }
    }

    fn knockback_resistance_of(&self, target: EntityId) -> f64 {
        const RESISTANCE: &str = "minecraft:knockback_resistance";
        self.entities
            .store
            .get(target)
            .and_then(Entity::living)
            .map_or(0.0, |state| {
                state.attributes.state(RESISTANCE, &state.effects).map_or(
                    EntityKind::Warden
                        .default_attribute(RESISTANCE)
                        .unwrap_or(0.0),
                    |attribute| rspigot_domain::attribute::value(&attribute),
                )
            })
    }

    pub(crate) fn warden_listener_section(&self, id: EntityId) -> Option<SectionPos> {
        let base = &self.entities.store.get(id)?.base;
        let eyes = Vec3::new(
            base.position.x,
            base.position.y + f64::from(base.eye_height()),
            base.position.z,
        );
        Some(SectionPos::of(eyes.block()))
    }

    pub(crate) fn warden_receive(
        &mut self,
        id: EntityId,
        pos: BlockPos,
        source: Option<EntityId>,
        owner: Option<EntityId>,
    ) {
        let Some(view) = self.mob_view(id) else {
            return;
        };
        let Some(mut entry) = self.entities.mobs.entries.remove(&id) else {
            return;
        };
        let mut body = MobBody::of(&view);
        {
            let world = self.mob_world(sky_darken(&self.world.level));
            entry
                .ai
                .warden_receive_vibration(&view, &world, &mut body, pos, source, owner);
        }
        self.finish_warden_ai(id, entry);
    }

    fn finish_warden_ai(&mut self, id: EntityId, mut entry: crate::mobs::MobEntry) {
        let actions = entry.ai.take_actions();
        self.entities.mobs.entries.insert(id, entry);
        for action in actions {
            self.apply_mob_action(id, action);
        }
    }

    pub(crate) fn warden_after_hurt(&mut self, id: EntityId, source: &DamageSource) {
        let Some(attacker) = source.causing.as_ref().map(|causing| causing.entity) else {
            return;
        };
        let direct = source.direct.is_none() || source.causing == source.direct;
        if self
            .entities
            .store
            .get(id)
            .is_none_or(|entity| rspigot_domain::entity::living::is_no_ai(&entity.base))
        {
            return;
        }
        let Some(view) = self.mob_view(id) else {
            return;
        };
        let Some(mut entry) = self.entities.mobs.entries.remove(&id) else {
            return;
        };
        let mut body = MobBody::of(&view);
        {
            let world = self.mob_world(sky_darken(&self.world.level));
            entry
                .ai
                .warden_hurt(&view, &world, &mut body, attacker, direct);
        }
        self.finish_warden_ai(id, entry);
    }

    pub(crate) fn warden_hurt_amount(&self, id: EntityId, source: &DamageSource) -> bool {
        use rspigot_domain::damage::DamageTag;
        let burrowed = self
            .entities
            .store
            .get(id)
            .is_some_and(|entity| warden::is_digging_or_emerging(entity.base.pose()));
        burrowed && !source.kind.is(DamageTag::BypassesInvulnerability)
    }

    pub(crate) fn warden_post_tick(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let persistent = entity
            .living()
            .and_then(|state| state.mob.as_ref())
            .is_some_and(|mob| mob.persistence_required);
        if persistent && let Some(entry) = self.entities.mobs.entries.get_mut(&id) {
            entry.ai.warden_hold_dig_cooldown();
        }
        if warden::is_digging_or_emerging(entity.base.pose())
            || rspigot_domain::entity::living::is_no_ai(&entity.base)
        {
            return;
        }
        let touched = self.warden_touched(id);
        for other in touched {
            self.warden_touch(id, other);
        }
    }

    fn warden_touched(&self, id: EntityId) -> Vec<EntityId> {
        let Some(entity) = self.entities.store.get(id) else {
            return Vec::new();
        };
        let bounds = entity.base.bounding_box();
        let mut touched: Vec<EntityId> = self
            .entities
            .mobs
            .players
            .iter()
            .filter(|player| !player.spectator && player.bounds.intersects(&bounds))
            .map(|player| player.id)
            .collect();
        touched.extend(
            self.entities
                .store
                .iter()
                .filter(|other| other.base.id != id && other.living().is_some())
                .filter(|other| other.base.bounding_box().intersects(&bounds))
                .map(|other| other.base.id),
        );
        touched
    }

    fn warden_touch(&mut self, id: EntityId, other: EntityId) {
        let Some(view) = self.mob_view(id) else {
            return;
        };
        let Some(mut entry) = self.entities.mobs.entries.remove(&id) else {
            return;
        };
        let mut body = MobBody::of(&view);
        {
            let world = self.mob_world(sky_darken(&self.world.level));
            entry.ai.warden_touch(&view, &world, &mut body, other);
        }
        self.finish_warden_ai(id, entry);
    }

    pub(crate) fn warden_registered(&mut self, id: EntityId, ai: &mut rspigot_ai::MobAi) {
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let anger = entity.extra.get_compound(ANGER_KEY).cloned();
        let listener = entity.extra.get_compound(LISTENER_KEY).cloned();
        ai.warden_load(anger.as_ref(), listener.as_ref());
        let pending = entity
            .living_mut()
            .and_then(|state| state.mob.as_mut())
            .and_then(|mob| match &mut mob.species {
                rspigot_domain::entity::species::Species::Warden(state) => {
                    state.pending_spawn.take()
                }
                _ => None,
            });
        if let Some(triggered) = pending {
            ai.warden_spawned(triggered);
            if triggered {
                self.entity_sound(id, AGITATED, 5.0, 1.0);
            }
        }
    }

    pub(crate) fn warden_nbt(&self, id: EntityId, saved: &mut rspigot_nbt::Compound) {
        if let Some(entry) = self.entities.mobs.entries.get(&id) {
            let (anger, listener) = entry.ai.warden_save();
            saved.insert(ANGER_KEY, anger);
            saved.insert(LISTENER_KEY, listener);
        }
    }

    pub(crate) fn warden_ambient_voice(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let pose = entity.base.pose();
        if matches!(
            pose,
            rspigot_domain::entity::Pose::Roaring
                | rspigot_domain::entity::Pose::Digging
                | rspigot_domain::entity::Pose::Emerging
        ) {
            return;
        }
        let Some(entry) = self.entities.mobs.entries.get(&id) else {
            return;
        };
        let level = rspigot_domain::warden::AngerLevel::by_anger(entry.ai.warden_active_anger());
        self.play_mob_voice(id, move |_| Some(level.ambient_sound()));
    }

    pub(crate) fn spawn_triggered_warden(&mut self, at: Vec3, yaw: f32) -> bool {
        if !self.warden_spot_is_free(at) {
            return false;
        }
        NaturalSpawnHost::spawn_mob(
            self,
            EntityKind::Warden,
            at,
            yaw,
            SpawnReason::Triggered,
            None,
        )
        .0
    }

    fn warden_spot_is_free(&self, at: Vec3) -> bool {
        let dimensions = warden::dimensions_in_pose(rspigot_domain::entity::Pose::Emerging);
        let half = f64::from(dimensions.width) / 2.0;
        let area = rspigot_domain::entity::Aabb::new(
            at.x - half,
            at.y,
            at.z - half,
            at.x + half,
            at.y + f64::from(dimensions.height),
            at.z + half,
        );
        !self
            .entities
            .store
            .iter()
            .any(|other| other.base.bounding_box().intersects(&area))
    }
}
