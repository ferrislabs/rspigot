use rspigot_ai::{MobView, inflate};
use rspigot_domain::damage::{DamageTag, DamageType};
use rspigot_domain::entity::species::{cube, sulfur_cube};
use rspigot_domain::entity::{
    Aabb, Entity, EntityState, RemovalReason, ageable, fire, item, living,
};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::{EquipmentSlot, ItemKind};

use crate::events::{ClientEvent, Hand};
use crate::game_events::GameEventContext;
use crate::health::DamageSource;
use crate::interact::Interaction;
use crate::player::Player;
use crate::plugins::events::{EntityRemoveCause, EntityUnleashReason};
use crate::plugins::steps::{AfterHurt, Holder, Struck};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const PICKUP_REACH_HORIZONTAL: f64 = 1.0;
const TOUCH_INFLATE_X: f64 = 1.0;
const TOUCH_INFLATE_Y: f64 = 0.5;
const PLAYER_WIDTH: f32 = 0.6;
const IS_EXPLOSION: &str = "minecraft:is_explosion";
const IMMUNE_DAMAGE: &str = "minecraft:sulfur_cube_with_block_immune_to";

fn equipment_event(id: EntityId, stack: Option<ItemStack>) -> ClientEvent {
    ClientEvent::EntityEquipment {
        entity: id,
        slots: vec![(crate::entities::wire_slot(EquipmentSlot::Body), stack)],
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn sulfur_cube_view(&self, entity: &Entity, view: &mut MobView) {
        let Some(living) = entity.living() else {
            return;
        };
        view.species.cube.can_deal_damage = false;
        view.species.cube.has_body_item = sulfur_cube::has_body_item(&entity.base, living);
        view.species.cube.pickup_timer =
            sulfur_cube::state(living).map_or(0, |state| state.pickup_timer);
    }

    fn sulfur_cube_alive(&self, id: EntityId) -> bool {
        self.entities.store.get(id).is_some_and(|entity| {
            !entity.base.is_removed()
                && entity
                    .living()
                    .is_some_and(|state| !living::is_dead_or_dying(&entity.base, state))
        })
    }

    pub(crate) fn sulfur_cube_pre_tick(&mut self, id: EntityId) -> bool {
        let explode = match self.entities.store.get_mut(id) {
            Some(Entity {
                state: EntityState::Living(living),
                ..
            }) => sulfur_cube::tick_fuse(living),
            _ => false,
        };
        if explode {
            self.sulfur_cube_explode(id);
            return self.entities.store.get(id).is_some();
        }
        if self.sulfur_cube_on_powered_position(id) {
            self.sulfur_cube_prime(id, false);
        }
        if let Some(Entity {
            base,
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(id)
        {
            sulfur_cube::sync_equipment(base, living);
        }
        true
    }

    pub(crate) fn sulfur_cube_custom_ai_step(&mut self, id: EntityId) {
        if let Some(Entity {
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(id)
        {
            sulfur_cube::tick_timers(living);
        }
    }

    pub(crate) fn sulfur_cube_post_tick(&mut self, id: EntityId) {
        self.cube_landing(id);
        if !self.sulfur_cube_alive(id) {
            return;
        }
        self.sulfur_cube_pick_up_loot(id);
        self.sulfur_cube_contacts(id);
    }

    pub(crate) fn sulfur_cube_grew_up(&mut self, id: EntityId) {
        if let Some(Entity {
            base,
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(id)
        {
            sulfur_cube::age_boundary_reached(base, living);
        }
    }

    fn sulfur_cube_on_powered_position(&self, id: EntityId) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let Some(living) = entity.living() else {
            return false;
        };
        sulfur_cube::can_explode(living, self.sulfur_cube_alive(id))
            && self.best_own_or_neighbour_signal(entity.base.position.block()) != 0
    }

    pub(crate) fn sulfur_cube_prime(&mut self, id: EntityId, imminent: bool) -> bool {
        if !self.config.rules.tnt_explodes || !self.sulfur_cube_alive(id) {
            return false;
        }
        let mut random = self.mob_random(id);
        let Some(Entity {
            base,
            state: EntityState::Living(living),
            ..
        }) = self.entities.store.get_mut(id)
        else {
            return false;
        };
        if !sulfur_cube::can_explode(living, true) {
            return false;
        }
        sulfur_cube::prime(base, living, imminent, &mut random);
        let volume = cube::sound_volume(base);
        let pitch = cube::sound_pitch(base, &mut random);
        let position = base.position;
        self.store_mob_random(id, random);
        self.entity_sound(id, sulfur_cube::PRIMED_SOUND, volume, pitch);
        self.game_event(
            GameEvent::PrimeFuse,
            position,
            GameEventContext::of(Some(id)),
        );
        true
    }

    fn sulfur_cube_explode(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(living) = entity.living() else {
            return;
        };
        let Some(explosion) = sulfur_cube::applied_profile(living).explosion else {
            return;
        };
        let center = Vec3::new(
            entity.base.position.x,
            entity.base.position.y + f64::from(entity.base.dimensions.height) * 0.0625,
            entity.base.position.z,
        );
        self.drop_leash(id, EntityUnleashReason::Unknown);
        if let Some(living) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        {
            living.dead = true;
        }
        if !self.config.rules.tnt_explodes {
            self.remove_entity(
                id,
                RemovalReason::Discarded,
                Some(EntityRemoveCause::Explode),
            );
            return;
        }
        let Some(source) = self.mob_source(id) else {
            return;
        };
        let damage = DamageSource::by(DamageType::PlayerExplosion, source);
        let interaction = self.tnt_block_interaction();
        self.explode(
            Some(id),
            &damage,
            center,
            explosion.power as f32,
            interaction,
            explosion.causes_fire,
            Some(id),
        );
    }

    pub(crate) fn sulfur_cube_hurt_override(
        &mut self,
        id: EntityId,
        source: &DamageSource,
    ) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let Some(living) = entity.living() else {
            return false;
        };
        if !sulfur_cube::has_body_item(&entity.base, living) {
            return false;
        }
        let alive = self.sulfur_cube_alive(id);
        if sulfur_cube::can_explode(living, alive) {
            let burning_arrow = source.direct.as_ref().is_some_and(|direct| {
                self.entities.store.get(direct.entity).is_some_and(|arrow| {
                    arrow.base.kind.extends("AbstractArrow") && fire::is_on_fire(&arrow.base)
                })
            });
            if source.kind.is(DamageTag::IsFire) || burning_arrow {
                self.sulfur_cube_prime(id, false);
            } else if source.kind.in_tag(IS_EXPLOSION) {
                self.sulfur_cube_prime(id, true);
            }
        }
        if !source.kind.in_tag(IMMUNE_DAMAGE) {
            return false;
        }
        if !source.kind.is(DamageTag::NoKnockback)
            && let Some(position) = self
                .entities
                .store
                .get(id)
                .map(|entity| entity.base.position)
        {
            let (dx, dz) = source.knockback_from(position);
            self.knock(
                Holder::Mob(id),
                crate::mobs::DEFAULT_KNOCKBACK,
                (dx, dz),
                source.knock_cause(),
                source.direct.as_ref().map(|direct| direct.entity),
            );
        }
        true
    }

    fn set_sulfur_body(&mut self, id: EntityId, stack: Option<ItemStack>, guaranteed: bool) {
        let Some(living) = self
            .entities
            .store
            .get_mut(id)
            .and_then(|entity| entity.living_mut())
        else {
            return;
        };
        living.equipment[sulfur_cube::slot_index_of_body()] = stack.clone();
        if guaranteed
            && stack.is_some()
            && let Some(mob) = living.mob.as_mut()
        {
            mob.drop_chances[sulfur_cube::slot_index_of_body()] = sulfur_cube::GUARANTEED_DROP;
        }
        self.push(Audience::Trackers(id), equipment_event(id, stack));
    }

    fn sulfur_cube_sound(&mut self, id: EntityId, sound: &'static str) {
        self.entity_sound(id, sound, 1.0, 1.0);
    }

    fn sulfur_cube_voice(&mut self, id: EntityId, sound: &'static str) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let volume = cube::sound_volume(&entity.base);
        let mut random = self.mob_random(id);
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let pitch = cube::sound_pitch(&entity.base, &mut random);
        self.store_mob_random(id, random);
        self.entity_sound(id, sound, volume, pitch);
    }

    fn sulfur_cube_height(&self, id: EntityId) -> f64 {
        self.entities
            .store
            .get(id)
            .map_or(0.0, |entity| f64::from(entity.base.dimensions.height))
    }

    fn sulfur_cube_equip(&mut self, id: EntityId, held: &ItemStack) -> bool {
        let Some(entity) = self.entities.store.get(id) else {
            return false;
        };
        let Some(living) = entity.living() else {
            return false;
        };
        let baby = entity.is_baby();
        if baby {
            return false;
        }
        let previous = sulfur_cube::body_item(living).cloned();
        if previous
            .as_ref()
            .is_some_and(|stack| stack.item == held.item)
        {
            return false;
        }
        if let Some(previous) = previous {
            let height = self.sulfur_cube_height(id);
            self.drop_at_location(id, height, previous, Vec3::ZERO);
        }
        let single = ItemStack {
            count: std::num::NonZeroU8::MIN,
            ..held.clone()
        };
        self.set_sulfur_body(id, Some(single), true);
        self.sulfur_cube_sound(id, sulfur_cube::ABSORB_SOUND);
        true
    }

    fn sulfur_cube_shear(&mut self, player: PlayerId, target: EntityId, hand: Hand) {
        let Some(stack) = self
            .entities
            .store
            .get(target)
            .and_then(Entity::living)
            .and_then(sulfur_cube::body_item)
            .cloned()
        else {
            return;
        };
        self.set_sulfur_body(target, None, false);
        let height = self.sulfur_cube_height(target);
        self.drop_at_location(target, height, stack, Vec3::ZERO);
        self.sulfur_cube_sound(target, sulfur_cube::EJECT_SOUND);
        if let Some(living) = self
            .entities
            .store
            .get_mut(target)
            .and_then(|entity| entity.living_mut())
            && let Some(state) = sulfur_cube::state_mut(living)
        {
            state.pickup_timer = sulfur_cube::PICKUP_TIMER_DURATION;
        }
        let actor = self.players.get(&player).map(|player| player.entity_id);
        if let Some(position) = self.entity_position(target) {
            self.game_event(GameEvent::Shear, position, GameEventContext::of(actor));
        }
        self.hurt_held(player, hand, 1);
    }

    pub(crate) fn sulfur_cube_interact(
        &mut self,
        player: PlayerId,
        target: EntityId,
        hand: Hand,
        held: Option<&ItemStack>,
    ) -> Option<Interaction> {
        let entity = self.entities.store.get(target)?;
        let living = entity.living()?;
        let baby = entity.is_baby();
        let can_age_up = entity
            .living()
            .and_then(|living| living.mob.as_ref())
            .and_then(|mob| mob.ageable.as_ref())
            .is_some_and(|ageable| ageable::can_age_up(&entity.base, ageable));
        let age = living
            .mob
            .as_ref()
            .and_then(|mob| mob.ageable.as_ref())
            .map_or(0, |ageable| ageable.age);
        let primed = sulfur_cube::is_primed(living);
        let can_explode = sulfur_cube::can_explode(living, self.sulfur_cube_alive(target));
        let ready_for_shearing = sulfur_cube::body_item(living).is_some();
        if baby {
            if let Some(stack) = held
                && sulfur_cube::is_food(stack)
                && can_age_up
            {
                self.use_animal_item(player, hand, Kind::SulfurCube, stack);
                if let Some(Entity {
                    base,
                    state: EntityState::Living(state),
                    ..
                }) = self.entities.store.get_mut(target)
                    && let Some(ageable) = state.mob.as_mut().and_then(|mob| mob.ageable.as_mut())
                {
                    let seconds = ageable::speed_up_seconds_when_feeding(-age);
                    ageable::age_up(base, ageable, seconds, true);
                }
                self.sulfur_cube_voice(target, sulfur_cube::EAT_SOUND);
                return Some(Interaction::Predicted);
            }
            if held.is_some_and(|stack| stack.item == ItemKind::GoldenDandelion.id()) {
                return Some(self.golden_dandelion(player, target, hand));
            }
            return Some(Interaction::Pass);
        }
        if primed {
            return Some(Interaction::Pass);
        }
        let Some(stack) = held else {
            return Some(Interaction::Pass);
        };
        let igniter =
            stack.item == ItemKind::FlintAndSteel.id() || stack.item == ItemKind::FireCharge.id();
        if can_explode && igniter {
            if !self.config.rules.tnt_explodes {
                return Some(Interaction::Pass);
            }
            self.sulfur_cube_prime(target, false);
            if stack.item == ItemKind::FlintAndSteel.id() {
                self.hurt_held(player, hand, 1);
            } else {
                self.consume_held(player, hand, 1);
            }
            return Some(Interaction::ServerOnly);
        }
        if stack.item == ItemKind::Shears.id() && ready_for_shearing {
            self.sulfur_cube_shear(player, target, hand);
            return Some(Interaction::Predicted);
        }
        if sulfur_cube::is_swallowable(stack) {
            if !self.sulfur_cube_equip(target, stack) {
                return Some(Interaction::Pass);
            }
            self.consume_held(player, hand, 1);
            if let Some(position) = self.entity_position(target) {
                self.game_event(
                    GameEvent::EntityInteract,
                    position,
                    GameEventContext::of(Some(target)),
                );
            }
            return Some(Interaction::ServerOnly);
        }
        Some(
            self.fish_interact(player, target, hand, held)
                .unwrap_or(Interaction::Pass),
        )
    }

    fn sulfur_cube_pick_up_loot(&mut self, id: EntityId) {
        if !self.config.rules.mob_griefing {
            return;
        }
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(living) = entity.living() else {
            return;
        };
        if sulfur_cube::has_body_item(&entity.base, living) {
            return;
        }
        let area = inflate(
            &entity.base.bounding_box(),
            PICKUP_REACH_HORIZONTAL,
            0.0,
            PICKUP_REACH_HORIZONTAL,
        );
        let baby = entity.is_baby();
        let pickup_timer = sulfur_cube::state(living).map_or(0, |state| state.pickup_timer);
        let found = self.entities.store.iter().find_map(|other| {
            let EntityState::Item(state) = &other.state else {
                return None;
            };
            let stack = item::stack(&other.base)?;
            let ready = state.pickup_delay <= 0
                && !other.base.is_removed()
                && other.base.bounding_box().intersects(&area)
                && sulfur_cube::is_swallowable(stack)
                && !baby;
            ready.then(|| (other.base.id, stack.clone()))
        });
        let Some((item_id, mut stack)) = found else {
            return;
        };
        if pickup_timer >= 1 {
            return;
        }
        let single = ItemStack {
            count: std::num::NonZeroU8::MIN,
            ..stack.clone()
        };
        self.set_sulfur_body(id, Some(single), true);
        self.sulfur_cube_sound(id, sulfur_cube::ABSORB_SOUND);
        self.push(
            Audience::Trackers(item_id),
            ClientEvent::ItemPickup {
                collected: item_id,
                collector: id,
                amount: 1,
            },
        );
        match std::num::NonZeroU8::new(stack.count.get() - 1) {
            Some(count) => {
                stack.count = count;
                if let Some(entity) = self.entities.store.get_mut(item_id) {
                    item::set_stack(&mut entity.base, Some(stack));
                }
            }
            None => {
                self.remove_entity(
                    item_id,
                    RemovalReason::Discarded,
                    Some(EntityRemoveCause::Despawn),
                );
            }
        }
    }

    fn sulfur_cube_contacts(&mut self, id: EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(living) = entity.living() else {
            return;
        };
        let profile = sulfur_cube::applied_profile(living);
        let has_body_item = sulfur_cube::has_body_item(&entity.base, living);
        let area = entity.base.bounding_box();
        let touch: Vec<PlayerId> = self
            .players
            .values()
            .filter(|player| self.sulfur_player_can_touch(player, &area))
            .map(|player| player.id)
            .collect();
        if has_body_item {
            for player in touch {
                self.sulfur_cube_player_push(id, player);
            }
        }
        if profile.contact_damage.is_none() {
            return;
        }
        let pushed_players: Vec<PlayerId> = self
            .players
            .values()
            .filter(|player| {
                self.sulfur_player_alive(player) && self.sulfur_player_box(player).intersects(&area)
            })
            .map(|player| player.id)
            .collect();
        for player in pushed_players {
            self.sulfur_cube_contact_damage(id, Struck::Player(player));
        }
        let others: Vec<EntityId> = self
            .entities
            .store
            .iter()
            .filter(|other| {
                other.base.id != id
                    && !other.base.is_removed()
                    && other.base.kind != Kind::ArmorStand
                    && other
                        .living()
                        .is_some_and(|state| !living::is_dead_or_dying(&other.base, state))
                    && other.base.bounding_box().intersects(&area)
            })
            .map(|other| other.base.id)
            .collect();
        for other in others {
            self.sulfur_cube_contact_damage(id, Struck::Mob(other));
        }
    }

    fn sulfur_player_alive(&self, player: &Player<L>) -> bool {
        !player.vitals.removed
            && !player.vitals.dead
            && player.vitals.health > 0.0
            && player.game_mode != GameMode::Spectator
    }

    fn sulfur_player_box(&self, player: &Player<L>) -> Aabb {
        Aabb::of_size(player.position, PLAYER_WIDTH, player.height() as f32)
    }

    fn sulfur_player_can_touch(&self, player: &Player<L>, area: &Aabb) -> bool {
        if !self.sulfur_player_alive(player) {
            return false;
        }
        let own = self.sulfur_player_box(player);
        let vehicle = self
            .entities
            .rides
            .vehicle_of(player.entity_id)
            .and_then(|vehicle| self.entities.store.get(vehicle))
            .filter(|vehicle| !vehicle.base.is_removed());
        match vehicle {
            Some(vehicle) => {
                let both = Aabb::new(
                    own.min_x.min(vehicle.base.bounding_box().min_x),
                    own.min_y.min(vehicle.base.bounding_box().min_y),
                    own.min_z.min(vehicle.base.bounding_box().min_z),
                    own.max_x.max(vehicle.base.bounding_box().max_x),
                    own.max_y.max(vehicle.base.bounding_box().max_y),
                    own.max_z.max(vehicle.base.bounding_box().max_z),
                );
                inflate(&both, TOUCH_INFLATE_X, 0.0, TOUCH_INFLATE_X).intersects(area)
            }
            None => {
                inflate(&own, TOUCH_INFLATE_X, TOUCH_INFLATE_Y, TOUCH_INFLATE_X).intersects(area)
            }
        }
    }

    fn sulfur_cube_player_push(&mut self, id: EntityId, player_id: PlayerId) {
        let Some(player) = self.players.get(&player_id) else {
            return;
        };
        let riding = self.entities.rides.is_passenger(player.entity_id);
        let speed = player.known_movement.length();
        let (pusher_position, pusher_height) = if riding {
            let root = self.entities.rides.root_vehicle(player.entity_id);
            match self.entities.store.get(root) {
                Some(vehicle) => (vehicle.base.position, vehicle.base.dimensions.height),
                None => (player.position, player.height() as f32),
            }
        } else {
            (player.position, player.height() as f32)
        };
        let Some(entity) = self.entities.store.get_mut(id) else {
            return;
        };
        let Some(living) = entity.living_mut() else {
            return;
        };
        let resistance = living.attribute("minecraft:knockback_resistance");
        let profile = sulfur_cube::applied_profile(living);
        let cube_position = entity.base.position;
        let cube_height = entity.base.dimensions.height;
        let on_ground = entity.base.on_ground;
        let Some(push) = sulfur_cube::player_push(
            cube_position,
            cube_height,
            on_ground,
            resistance,
            pusher_position,
            pusher_height,
            riding,
            speed,
        ) else {
            return;
        };
        entity.base.needs_sync = true;
        let mut play = None;
        if let Some(living) = entity.living_mut()
            && let Some(state) = sulfur_cube::state_mut(living)
            && sulfur_cube::push_sound_audible(push.delta, &profile.sounds)
            && state.push_sound_cooldown <= 0
        {
            state.push_sound_cooldown = sulfur_cube::push_cooldown_ticks(&profile.sounds);
            play = Some(profile.sounds.push_sound);
        }
        entity.base.velocity = entity.base.velocity + push.delta;
        if let Some(sound) = play {
            self.sulfur_cube_sound(id, sound);
        }
        self.sulfur_cube_contact_damage(id, Struck::Player(player_id));
    }

    fn sulfur_cube_contact_damage(&mut self, id: EntityId, victim: Struck) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let Some(living) = entity.living() else {
            return;
        };
        let Some(contact) = sulfur_cube::applied_profile(living).contact_damage else {
            return;
        };
        let kind = match contact.damage_type {
            "minecraft:sulfur_cube_hot" => DamageType::SulfurCubeHot,
            _ => return,
        };
        let player_victim = matches!(victim, Struck::Player(_));
        let source = if contact.attribute_to_source || player_victim {
            match self.mob_source(id) {
                Some(source) => DamageSource::by(kind, source),
                None => return,
            }
        } else {
            DamageSource::new(kind)
        };
        self.strike(None, victim, &source, contact.amount, AfterHurt::Nothing);
    }
}
