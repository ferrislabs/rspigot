use rand::Rng;
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::arrow::{self, ArrowState, Pickup};
use rspigot_domain::entity::fire;
use rspigot_domain::entity::living;
use rspigot_domain::entity::raycast;
use rspigot_domain::entity::{Aabb, Entity, EntityState, RemovalReason, fluid};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::random::RandomSource;
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::{EquipmentSlot, ItemKind};

use crate::events::Hand;
use crate::events::{ClientEvent, GameEvent};
use crate::game_events::GameEventContext;
use crate::health::{DamageSource, SourceEntity};
use crate::plugins::events::{EntityRemoveCause, PlayerPickupArrow};
use crate::plugins::steps::ArrowStep;
use crate::plugins::steps::{Draw, Shooter};
use crate::plugins::{
    AfterHurt, ArrowFollow, Combuster, Continuation, EventPayload, Holder, spec_of,
};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use rspigot_domain::game_event::GameEvent as VibrationEvent;

pub(crate) const EYE_OFFSET: f64 = 0.100_000_001_490_116_12;
const TARGET_HEIGHT_FRACTION: f64 = 0.333_333_333_333_333_3;
const ARC: f64 = 0.200_000_002_980_232_24;
const MOB_ARROW_SPEED: f32 = 1.6;
const SKELETON_SHOOT: &str = "minecraft:entity.skeleton.shoot";
const ARROWS_TAG: &str = "minecraft:arrows";
const PICKUP_INFLATE_X: f64 = 1.0;
const PICKUP_INFLATE_Y: f64 = 0.5;
const PLAYER_WIDTH: f64 = 0.6;
const STOP_SQR: f64 = 1.0e-7;
const DEFLECT_SCALE: f64 = 0.2;
const ARROW_FIRE_SECONDS: f32 = 5.0;
const SKELETON_FORCE: f32 = 0.8;

#[derive(Debug, Clone, Copy)]
struct ArrowHit {
    target: EntityId,
    location: Vec3,
    position: Vec3,
}

fn hit_sound_of(name: &str) -> &'static str {
    if name == rspigot_domain::crossbow::HIT_SOUND {
        rspigot_domain::crossbow::HIT_SOUND
    } else {
        arrow::HIT_SOUND
    }
}

pub(crate) fn is_arrow_item(stack: Option<&ItemStack>) -> bool {
    stack.is_some_and(|stack| {
        rspigot_data::tags::tag("minecraft:item", ARROWS_TAG)
            .is_some_and(|ids| ids.binary_search(&u32::from(stack.item.0)).is_ok())
    })
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn perform_ranged_attack(
        &mut self,
        shooter: EntityId,
        target: EntityId,
        power: f32,
    ) {
        if self
            .entities
            .store
            .get(shooter)
            .is_some_and(|entity| entity.base.kind == Kind::SnowGolem)
        {
            self.snow_golem_throw(shooter, target);
            return;
        }
        if self
            .entities
            .store
            .get(shooter)
            .is_some_and(|entity| entity.base.kind == Kind::Witch)
        {
            self.witch_throw(shooter, target);
            return;
        }
        if self
            .entities
            .store
            .get(shooter)
            .is_some_and(|entity| matches!(entity.base.kind, Kind::Pillager | Kind::Piglin))
        {
            self.crossbow_attack(shooter, target);
            return;
        }
        let Some(target) = self.entity_box(target) else {
            return;
        };
        let Some(entity) = self.entities.store.get(shooter) else {
            return;
        };
        let Some(living_state) = entity.living() else {
            return;
        };
        let weapon_slot = if living_state
            .equipment(EquipmentSlot::Mainhand)
            .is_some_and(|s| s.item == ItemKind::Bow.id())
        {
            EquipmentSlot::Mainhand
        } else {
            EquipmentSlot::Offhand
        };
        let weapon = living_state.equipment(weapon_slot).cloned();
        let held = [EquipmentSlot::Offhand, EquipmentSlot::Mainhand]
            .into_iter()
            .map(|slot| living_state.equipment(slot))
            .find(|stack| is_arrow_item(*stack))
            .flatten()
            .cloned();
        let projectile = held.unwrap_or_else(arrow::default_item);
        let weapon_view = weapon.clone();
        let origin = entity.base.position;
        let eye = entity.base.eye_y();
        let owner_uuid = entity.base.uuid;
        let mut base = self.new_entity_base(Kind::Arrow);
        base.position = Vec3::new(origin.x, eye - EYE_OFFSET, origin.z);
        base.old_position = base.position;
        let seed = self.rng.random::<i64>();
        let mut state = ArrowState::new(&mut base, seed);
        state.set_owner(Some(owner_uuid), false);
        state.item = ItemStack {
            count: std::num::NonZeroU8::MIN,
            ..projectile
        };
        state.weapon = weapon;
        let difficulty = self.world.difficulty.id();
        state.base_damage = arrow::mob_base_damage(power, difficulty, &mut state.random);
        let dx = target.0.x - origin.x;
        let dy = target.0.y + target.1 * TARGET_HEIGHT_FRACTION - base.position.y;
        let dz = target.0.z - origin.z;
        let distance = (dx * dx + dz * dz).sqrt();
        let uncertainty = (14 - i32::from(difficulty) * 4) as f32;
        arrow::shoot(
            &mut base,
            &mut state,
            Vec3::new(dx, dy + distance * ARC, dz),
            MOB_ARROW_SPEED,
            uncertainty,
        );
        let arrow = Entity::new(base, EntityState::Arrow(Box::new(state)));
        let draw = Draw {
            weapon: weapon_view,
            ammo: None,
            hand: Hand::Main,
            force: SKELETON_FORCE,
        };
        self.shoot_bow(Shooter::Mob(shooter), draw, arrow);
    }

    pub(crate) fn skeleton_sound(&mut self, shooter: EntityId) {
        let mut random = self.mob_random(shooter);
        let pitch = 1.0 / (random.next_float() * 0.4 + 0.8);
        self.store_mob_random(shooter, random);
        self.entity_sound(shooter, SKELETON_SHOOT, 1.0, pitch);
    }

    pub(crate) fn entity_box(&self, id: EntityId) -> Option<(Vec3, f64, Aabb)> {
        if let Some(player) = self.players.values().find(|p| p.entity_id == id) {
            let height = player.height();
            return Some((
                player.position,
                height,
                Aabb::of_size(player.position, PLAYER_WIDTH as f32, height as f32),
            ));
        }
        let entity = self.entities.store.get(id)?;
        Some((
            entity.base.position,
            f64::from(entity.base.dimensions.height),
            entity.base.bounding_box(),
        ))
    }

    fn note_hurt_mob(&mut self, attacker: EntityId, target: EntityId) {
        let living = self.players.values().any(|p| p.entity_id == target)
            || self
                .entities
                .store
                .get(target)
                .is_some_and(|entity| entity.living().is_some());
        let victim = living.then_some(target);
        if let Some(player) = self.players.values_mut().find(|p| p.entity_id == attacker) {
            player.vitals.last_hurt_mob = victim;
            player.vitals.last_hurt_mob_at = player.vitals.tick_count as i32;
            return;
        }
        if let Some(Entity {
            base,
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(attacker)
        {
            living.damage.last_hurt_mob = victim;
            living.damage.last_hurt_mob_at = base.tick_count;
        }
    }

    pub(crate) fn owner_entity(&self, owner: Option<uuid::Uuid>) -> Option<EntityId> {
        let owner = owner?;
        if let Some(player) = self.players.values().find(|p| p.profile.id.0 == owner) {
            return Some(player.entity_id);
        }
        self.entities.store.by_uuid(owner)
    }

    pub(crate) fn projectile_owner_id(&self, entity: &Entity) -> i32 {
        match &entity.state {
            EntityState::Arrow(state) => self.owner_entity(state.owner).map_or(0, |id| id.0),
            EntityState::Thrown(state) => self.owner_entity(state.owner).map_or(0, |id| id.0),
            EntityState::Hurting(state) => self.owner_entity(state.owner).map_or(0, |id| id.0),
            EntityState::Bullet(state) => self.owner_entity(state.owner).map_or(0, |id| id.0),
            _ => entity.add_entity_data(),
        }
    }

    pub(crate) fn hit_candidates(
        &self,
        arrow_id: EntityId,
        owner: Option<EntityId>,
        left_owner: bool,
        pierced: &[EntityId],
        area: &Aabb,
    ) -> Vec<(EntityId, Aabb, Vec3)> {
        let mut out: Vec<(EntityId, Aabb, Vec3)> = self
            .entities
            .store
            .iter()
            .filter(|e| e.base.id != arrow_id && !e.base.is_removed())
            .filter(|e| {
                e.living()
                    .is_some_and(|state| !living::is_dead_or_dying(&e.base, state))
            })
            .filter(|e| e.base.bounding_box().intersects(area))
            .map(|e| (e.base.id, e.base.bounding_box(), e.base.position))
            .collect();
        out.extend(
            self.players
                .values()
                .filter(|p| p.game_mode != GameMode::Spectator && !p.vitals.is_dead())
                .map(|p| {
                    (
                        p.entity_id,
                        Aabb::of_size(p.position, PLAYER_WIDTH as f32, p.height() as f32),
                        p.position,
                    )
                })
                .filter(|(_, bounds, _)| bounds.intersects(area)),
        );
        out.retain(|(id, _, _)| (left_owner || Some(*id) != owner) && !pierced.contains(id));
        out
    }

    fn find_hits(&self, arrow_id: EntityId, from: Vec3, to: Vec3) -> Vec<ArrowHit> {
        let Some(Entity {
            base,
            state: EntityState::Arrow(state),
            ..
        }) = self.entities.store.get(arrow_id)
        else {
            return Vec::new();
        };
        let owner = self.owner_entity(state.owner);
        let area = arrow::search_box(base);
        let margin = f64::from(arrow::hit_margin(base.tick_count));
        let mut hits = Vec::new();
        for (target, bounds, position) in
            self.hit_candidates(arrow_id, owner, state.left_owner, &state.pierced, &area)
        {
            if let Some(location) = raycast::clip_box(&bounds, from, to) {
                hits.push(ArrowHit {
                    target,
                    location,
                    position,
                });
                continue;
            }
            if margin <= 0.0 {
                continue;
            }
            let Some(inflated) = raycast::clip_box(&bounds.inflate(margin), from, to) else {
                continue;
            };
            let mut center = bounds.center();
            if let Some(block) = raycast::clip_blocks(&self.world.chunks, inflated, center) {
                center = block.location;
            }
            if raycast::clip_box(&bounds, inflated, center).is_some() {
                hits.push(ArrowHit {
                    target,
                    location: inflated,
                    position,
                });
            }
        }
        hits.sort_by(|a, b| {
            from.distance_squared(a.position)
                .total_cmp(&from.distance_squared(b.position))
        });
        hits
    }

    fn check_left_owner(&mut self, id: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Arrow(state),
            ..
        }) = self.entities.store.get(id)
        else {
            return;
        };
        if state.left_owner || state.left_owner_checked {
            return;
        }
        let area = arrow::search_box(base);
        let owner = self.owner_entity(state.owner);
        let left = owner
            .and_then(|owner| self.entity_box(owner))
            .is_none_or(|(_, _, bounds)| !bounds.intersects(&area));
        if let Some(EntityState::Arrow(state)) =
            self.entities.store.get_mut(id).map(|e| &mut e.state)
        {
            state.left_owner = left;
            state.left_owner_checked = true;
        }
    }

    pub(crate) fn tick_arrow(&mut self, id: EntityId) {
        if self.projectile_waits(id) {
            return;
        }
        let despawn = self.config.mobs.arrow_despawn_rate;
        let in_rain = self
            .entities
            .store
            .get(id)
            .is_some_and(|e| self.is_in_rain(&e.base));
        let chunks = &self.world.chunks;
        let Some(Entity {
            base,
            state: EntityState::Arrow(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        base.old_position = base.position;
        base.tick_count = base.tick_count.wrapping_add(1);
        fluid::update(base, chunks);
        let movement = base.velocity;
        let ground = arrow::ground_tick(base, state, chunks, despawn);
        if base.fluid.in_water || in_rain {
            fire::clear_fire(base);
        }
        if ground.grounded {
            if ground.despawn {
                self.remove_entity(
                    id,
                    RemovalReason::Discarded,
                    Some(EntityRemoveCause::Despawn),
                );
                return;
            }
            self.apply_fire_inside(id);
            if let Some(entity) = self.entities.store.get_mut(id) {
                let on_fire = entity.base.remaining_fire_ticks > 0;
                fire::set_on_fire_flag(&mut entity.base, on_fire);
            }
            return;
        }
        let physics = !base.no_physics;
        if base.fluid.in_water {
            base.velocity = base.velocity.scale(f64::from(arrow::WATER_INERTIA));
        }
        arrow::flight_rotation(base, movement);
        self.check_left_owner(id);
        let Some(position) = self.entities.store.get(id).map(|e| e.base.position) else {
            return;
        };
        let end = position + movement;
        if physics {
            let block = raycast::clip_blocks(&self.world.chunks, position, end);
            self.step_move_and_hit(id, block, end);
        } else if let Some(entity) = self.entities.store.get_mut(id) {
            entity.base.position = end;
        }
        let Some(Entity {
            base,
            state: EntityState::Arrow(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        if !base.fluid.in_water {
            base.velocity = base.velocity.scale(f64::from(arrow::AIR_DRAG));
        }
        if physics && !arrow::in_ground(base) && !base.is_no_gravity() {
            base.velocity = base.velocity + Vec3::new(0.0, -arrow::GRAVITY, 0.0);
        }
        state.has_been_shot = true;
        state.left_owner_checked = false;
        fire::base_tick(base);
        let on_fire = base.remaining_fire_ticks > 0;
        fire::set_on_fire_flag(base, on_fire);
        self.apply_fire_inside(id);
    }

    fn step_move_and_hit(&mut self, id: EntityId, block: Option<raycast::BlockHit>, miss: Vec3) {
        loop {
            let Some(position) = self.entities.store.get(id).map(|e| e.base.position) else {
                return;
            };
            let target = block.map_or(miss, |hit| hit.location);
            let hits = self.find_hits(id, position, target);
            let location = hits.first().map_or(target, |hit| hit.location);
            if let Some(entity) = self.entities.store.get_mut(id) {
                entity.base.position = location;
            }
            if hits.is_empty() {
                if let Some(hit) = block
                    && self.entities.store.get(id).is_some()
                {
                    self.arrow_hit_block(id, hit);
                    if let Some(entity) = self.entities.store.get_mut(id) {
                        entity.base.needs_sync = true;
                    }
                }
                return;
            }
            let mut pierce = false;
            for hit in hits {
                if self.entities.store.get(id).is_none() {
                    return;
                }
                self.ask_projectile_hit(id, hit.target);
                if self.projectile_waits(id) {
                    return;
                }
            }
            if let Some(entity) = self.entities.store.get_mut(id) {
                entity.base.needs_sync = true;
                pierce = arrow::pierce_level(&entity.base) > 0;
            }
            if !pierce {
                return;
            }
        }
    }

    fn arrow_hit_block(&mut self, id: EntityId, hit: raycast::BlockHit) {
        self.notice_block_hit(id, hit.pos, hit.face);
        let state_at = self.world.chunks.block(hit.pos).unwrap_or_default();
        if state_at.is_air() {
            return;
        }
        let landed = self.entities.store.get(id).map(|e| e.base.position);
        if let Some(at) = landed {
            self.game_event(
                VibrationEvent::ProjectileLand,
                at,
                GameEventContext::of(Some(id)),
            );
        }
        let Some(Entity {
            base,
            state: EntityState::Arrow(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let sound = hit_sound_of(&state.sound);
        let pitch = arrow::hit_block(base, state, state_at);
        let position = base.position;
        let silent = base.is_silent();
        if !silent {
            self.play_sound(None, position, sound, SoundSource::Neutral, 1.0, pitch);
        }
    }

    pub(crate) fn arrow_hit_entity(&mut self, id: EntityId, target: EntityId) {
        let Some(Entity {
            base,
            state: EntityState::Arrow(state),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return;
        };
        let damage = arrow::hit_damage(base, state);
        let velocity = base.velocity;
        let on_fire = fire::is_on_fire(base);
        let owner = state.owner;
        let sound_pitch = 1.2 / (state.random.next_float() * 0.2 + 0.9);
        let owner_id = self.owner_entity(owner);
        let arrow_source = self.mob_source(id);
        let causing = match owner_id {
            Some(owner) => self.source_for(owner),
            None => arrow_source.clone(),
        };
        let Some(direct) = arrow_source else {
            return;
        };
        if let Some(owner) = owner_id {
            self.note_hurt_mob(owner, target);
        }
        let saved_fire = self.fire_ticks_of(target);
        let holder = self
            .player_of_entity(target)
            .map_or(Holder::Mob(target), Holder::Player);
        if on_fire && let Some(arrow) = self.entity_view(id) {
            self.combust(holder, ARROW_FIRE_SECONDS, Combuster::Entity(arrow));
        }
        let lane = self.lane_for(None, target);
        let ignition_waits = on_fire
            && self.decisions.in_flight(
                lane,
                |then| matches!(then, Continuation::Combust(step) if step.target == holder),
            );
        let source = DamageSource {
            kind: DamageType::Arrow,
            causing,
            direct: Some(direct),
            position: None,
            knockback: Some((-velocity.x, -velocity.z)),
            direct_block: None,
        };
        let follow = ArrowFollow {
            arrow: id,
            target,
            owner: owner_id,
            saved_fire,
            sound_pitch,
        };
        let victim = self.struck(target);
        let then = AfterHurt::Arrow(follow);
        if ignition_waits {
            self.strike_behind(lane, victim, source, damage as f32, then);
        } else {
            self.strike(None, victim, &source, damage as f32, then);
        }
    }

    pub(crate) fn finish_arrow_hit(&mut self, follow: ArrowFollow, hurt: bool) {
        let ArrowFollow {
            arrow: id,
            target,
            owner: owner_id,
            saved_fire,
            sound_pitch,
        } = follow;
        let target_player = self.player_of_entity(target);
        if hurt {
            if let Some(Entity {
                base,
                state: EntityState::Living(_),
                ..
            }) = self.entities.store.get_mut(target)
            {
                let count = living::arrow_count(base);
                living::set_arrow_count(base, count + 1);
            } else if let Some(player) = target_player {
                self.add_player_arrow(player);
            }
            if let (Some(_), Some(owner)) = (target_player, owner_id)
                && let Some(shooter) = self.player_of_entity(owner)
                && Some(shooter) != target_player
            {
                self.push(
                    Audience::One(shooter),
                    ClientEvent::GameEvent(GameEvent::ArrowHitPlayer),
                );
            }
            if let Some((position, sound)) = self.entities.store.get(id).map(|e| {
                let sound = match &e.state {
                    EntityState::Arrow(state) => hit_sound_of(&state.sound),
                    _ => arrow::HIT_SOUND,
                };
                (e.base.position, sound)
            }) {
                self.play_sound(
                    None,
                    position,
                    sound,
                    SoundSource::Neutral,
                    1.0,
                    sound_pitch,
                );
            }
            self.remove_entity(id, RemovalReason::Killed, Some(EntityRemoveCause::Hit));
        } else {
            self.restore_fire_ticks(target, saved_fire);
            let Some(Entity {
                base,
                state: EntityState::Arrow(state),
                ..
            }) = self.entities.store.get_mut(id)
            else {
                return;
            };
            let rotation = 170.0 + state.random.next_float() * 20.0;
            base.velocity = base.velocity.scale(-DEFLECT_SCALE * 0.5);
            let yaw = base.yaw + rotation;
            base.set_yaw(yaw);
            base.needs_sync = true;
            if base.velocity.length_sqr() < STOP_SQR {
                let pickup = state.pickup == Pickup::Allowed;
                let item = state.item.clone();
                if pickup {
                    self.drop_at_location(id, 0.1, item, Vec3::ZERO);
                }
                self.remove_entity(id, RemovalReason::Killed, Some(EntityRemoveCause::Hit));
            }
        }
    }

    pub(crate) fn source_for(&self, id: EntityId) -> Option<SourceEntity> {
        if let Some(player) = self.players.values().find(|p| p.entity_id == id) {
            return self.source_of(player.id);
        }
        self.mob_source(id)
    }

    pub(crate) fn fire_ticks_of(&self, id: EntityId) -> i32 {
        if let Some(player) = self.players.values().find(|p| p.entity_id == id) {
            return player.vitals.fire_ticks;
        }
        self.entities
            .store
            .get(id)
            .map_or(0, |e| e.base.remaining_fire_ticks)
    }

    pub(crate) fn restore_fire_ticks(&mut self, id: EntityId, ticks: i32) {
        if let Some(player) = self.players.values_mut().find(|p| p.entity_id == id) {
            player.vitals.fire_ticks = ticks;
            return;
        }
        if let Some(entity) = self.entities.store.get_mut(id) {
            entity.base.remaining_fire_ticks = ticks;
        }
    }

    fn add_player_arrow(&mut self, player: PlayerId) {
        if let Some(player) = self.players.get_mut(&player) {
            player.vitals.arrow_count += 1;
        }
        self.broadcast_metadata(player);
    }

    pub(crate) fn collect_arrows(&mut self) {
        let collectors: Vec<(PlayerId, EntityId, Vec3, f64, bool)> = self
            .players
            .values()
            .filter(|p| p.game_mode != GameMode::Spectator && !p.vitals.is_dead())
            .map(|p| {
                (
                    p.id,
                    p.entity_id,
                    p.position,
                    p.height(),
                    p.abilities().instant_build,
                )
            })
            .collect();
        for (player, collector, position, height, creative) in collectors {
            let reach = rspigot_ai::inflate(
                &Aabb::of_size(position, PLAYER_WIDTH as f32, height as f32),
                PICKUP_INFLATE_X,
                PICKUP_INFLATE_Y,
                PICKUP_INFLATE_X,
            );
            let touching: Vec<EntityId> = self
                .entities
                .store
                .iter()
                .filter(|e| match &e.state {
                    EntityState::Arrow(state) => {
                        (arrow::in_ground(&e.base) || e.base.no_physics)
                            && state.shake_time <= 0
                            && e.base.bounding_box().intersects(&reach)
                    }
                    _ => false,
                })
                .map(|e| e.base.id)
                .collect();
            for id in touching {
                self.pick_up_arrow(player, collector, id, creative);
            }
        }
    }

    fn pick_up_arrow(
        &mut self,
        player_id: PlayerId,
        collector: EntityId,
        id: EntityId,
        creative: bool,
    ) {
        let Some(EntityState::Arrow(state)) = self.entities.store.get(id).map(|e| &e.state) else {
            return;
        };
        let pickup = state.pickup;
        let item = state.item.clone();
        match pickup {
            Pickup::Allowed => {
                if self.can_hold(player_id, &item) {
                    self.ask_arrow_pickup(player_id, collector, id);
                }
            }
            Pickup::CreativeOnly if creative => self.take_arrow(player_id, collector, id, false),
            Pickup::CreativeOnly | Pickup::Disallowed => {}
        }
    }

    fn can_hold(&self, player: PlayerId, item: &ItemStack) -> bool {
        self.players.get(&player).is_some_and(|player| {
            let mut trial = player.inventory.clone();
            trial
                .insert(item.item, 1, item.item.max_stack_size())
                .remaining
                == 0
        })
    }

    fn ask_arrow_pickup(&mut self, player: PlayerId, collector: EntityId, arrow: EntityId) {
        let step = ArrowStep {
            player,
            collector,
            arrow,
        };
        self.fire(player, Continuation::PickupArrow(step), move |server| {
            let entity = server.entities.store.get(arrow)?;
            let EntityState::Arrow(state) = &entity.state else {
                return None;
            };
            Some(EventPayload::PlayerPickupArrow(PlayerPickupArrow {
                arrow: server.entity_view(arrow)?,
                item: spec_of(&state.item),
                player: server.snapshot_of(player)?,
                remaining: 0,
            }))
        });
    }

    pub(crate) fn finish_arrow_pickup(&mut self, step: ArrowStep) {
        let Some(Entity {
            base,
            state: EntityState::Arrow(state),
            ..
        }) = self.entities.store.get(step.arrow)
        else {
            return;
        };
        let lying = (arrow::in_ground(base) || base.no_physics)
            && state.shake_time <= 0
            && state.pickup == Pickup::Allowed
            && !base.is_removed();
        let item = state.item.clone();
        let able = self.players.get(&step.player).is_some_and(|player| {
            !player.vitals.is_dead() && player.game_mode != GameMode::Spectator
        });
        if lying && able && self.can_hold(step.player, &item) {
            self.take_arrow(step.player, step.collector, step.arrow, true);
        }
    }

    fn take_arrow(
        &mut self,
        player_id: PlayerId,
        collector: EntityId,
        id: EntityId,
        allowed: bool,
    ) {
        if allowed {
            let Some(EntityState::Arrow(state)) = self.entities.store.get(id).map(|e| &e.state)
            else {
                return;
            };
            let item = state.item.clone();
            let Some(player) = self.players.get_mut(&player_id) else {
                return;
            };
            player
                .inventory
                .insert(item.item, 1, item.item.max_stack_size());
        }
        let event = ClientEvent::ItemPickup {
            collected: id,
            collector,
            amount: 1,
        };
        if let Some(player) = self.players.get(&player_id) {
            player.link.send(event.clone());
        }
        self.push(Audience::TrackersExcept(id, player_id), event);
        if allowed {
            self.sync_inventory(player_id);
            self.broadcast_held_item(player_id);
        }
        self.remove_entity(
            id,
            RemovalReason::Discarded,
            Some(EntityRemoveCause::Pickup),
        );
    }

    pub fn arrows(&self) -> Vec<(EntityId, Vec3, bool)> {
        self.entities
            .store
            .iter()
            .filter(|e| matches!(e.state, EntityState::Arrow(_)))
            .map(|e| (e.base.id, e.base.position, arrow::in_ground(&e.base)))
            .collect()
    }
}
