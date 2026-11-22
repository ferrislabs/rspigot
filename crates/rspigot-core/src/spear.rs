use std::collections::BTreeMap;

use rspigot_domain::attribute::{ATTACK_DAMAGE, ATTACK_SPEED};
use rspigot_domain::damage::DamageType;
use rspigot_domain::entity::raycast::{clip_blocks, clip_box};
use rspigot_domain::item::ItemStack;
use rspigot_domain::meta::MetaValue;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::spear::{self, Spear};
use rspigot_domain::{EntityId, ItemId, Vec3, mth};
use rspigot_entities::SoundSource;

use crate::advancements::AdvancementEvent;
use crate::bow::{UsingItem, hand_slot};
use crate::combat::attack_strength_scale;
use crate::commands::Target;
use crate::enchant_effects::{self, EquipSlot, Worn};
use crate::events::{ClientEvent, Hand, SwingStyle};
use crate::health::{DamageSource, attributes};
use crate::loot::LootContext;
use crate::plugins::AfterHurt;
use crate::plugins::events::EntityKnockbackCause;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const STABBED_KEY: &str = "rspigot:spear_stabbed";
const SPEAR_MOBS: &str = "minecraft:spear_mobs";
const JAB_LEEWAY_TICKS: f32 = 5.0;
const MINIMUM_ATTACK_CHARGE: f32 = 1.0;
const PLAYER_BASE_ATTACK: f32 = 1.0;

pub(crate) fn is_spear(item: ItemId) -> bool {
    item.kind().is_some_and(|kind| spear::of(kind).is_some())
}

fn spear_of(stack: &ItemStack) -> Option<Spear> {
    stack.item.kind().and_then(spear::of)
}

fn memory<L>(player: &crate::player::Player<L>) -> BTreeMap<String, MetaValue> {
    match player.volatile_meta.get(STABBED_KEY) {
        Some(MetaValue::Map(entries)) => entries,
        _ => BTreeMap::new(),
    }
}

fn remembers(entries: &BTreeMap<String, MetaValue>, entity: EntityId, now: u64) -> bool {
    entries.get(&entity.0.to_string()).is_some_and(|seen| {
        matches!(seen, MetaValue::Int(at)
            if (now as i64).saturating_sub(*at) < i64::from(spear::CONTACT_COOLDOWN_TICKS))
    })
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

struct Candidate {
    entity: EntityId,
    distance: f64,
    velocity: Vec3,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn begin_spear(&mut self, id: PlayerId, hand: Hand) -> bool {
        let tick = self.current_tick();
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        let Some(spear) = player
            .inventory
            .get(hand_slot(player, hand))
            .and_then(spear_of)
        else {
            return false;
        };
        player.using_item = Some(UsingItem { hand, since: tick });
        let position = player.position;
        self.play_sound(
            None,
            position,
            spear.use_sound(),
            SoundSource::Players,
            1.0,
            1.0,
        );
        self.show_use_status(id);
        true
    }

    fn spear_candidates(&self, id: PlayerId, min: f64, max: f64, margin: f64) -> Vec<Candidate> {
        let Some(player) = self.players.get(&id) else {
            return Vec::new();
        };
        let look = mth::view_vector(player.rotation.pitch, player.rotation.yaw);
        let eye = player.eye_position();
        let mut end = eye + look.scale(max);
        if let Some(block) = clip_blocks(&self.world.chunks, eye, end) {
            end = block.location;
        }
        let start = eye + look.scale(min);
        let own = player.entity_id;
        let mut found: Vec<Candidate> = Vec::new();
        let mut consider =
            |entity: EntityId, velocity: Vec3, bounds: &rspigot_domain::entity::Aabb| {
                if let Some(hit) = clip_box(&bounds.inflate(margin), start, end) {
                    found.push(Candidate {
                        entity,
                        distance: hit.distance_squared(eye),
                        velocity,
                    });
                }
            };
        for other in self.players.values() {
            if other.entity_id == own
                || other.vitals.is_dead()
                || other.game_mode == GameMode::Spectator
            {
                continue;
            }
            if let Some((_, _, bounds)) = self.entity_box(other.entity_id) {
                consider(other.entity_id, other.known_movement, &bounds);
            }
        }
        for entity in self.entities.store.iter() {
            if entity.base.is_removed() || !self.living_entity_alive(entity.base.id) {
                continue;
            }
            consider(
                entity.base.id,
                entity.base.velocity,
                &entity.base.bounding_box(),
            );
        }
        found.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        found
    }

    pub(crate) fn tick_spear_use(&mut self, id: PlayerId, using: UsingItem, item: ItemId) {
        let Some(spear) = item.kind().and_then(spear::of) else {
            return;
        };
        let tick = self.current_tick();
        let used = i32::try_from(tick.saturating_sub(using.since)).unwrap_or(i32::MAX);
        let Some(charged) = spear.charging(used) else {
            return;
        };
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if player.vitals.is_dead() || player.game_mode == GameMode::Spectator {
            return;
        }
        let look = mth::view_vector(player.rotation.pitch, player.rotation.yaw);
        let attacker_speed = dot(look, player.known_movement.scale(spear::MOTION_TO_SPEED));
        let (min, max) = spear::reach(player.game_mode == GameMode::Creative);
        let candidates = self.spear_candidates(id, min, max, spear::HITBOX_MARGIN);
        let mut entries = memory(player);
        let mut hits = 0;
        for candidate in candidates {
            if remembers(&entries, candidate.entity, tick) {
                continue;
            }
            entries.insert(candidate.entity.0.to_string(), MetaValue::Int(tick as i64));
            let target_speed = dot(look, candidate.velocity.scale(spear::MOTION_TO_SPEED));
            let relative = spear::relative_speed(attacker_speed, target_speed);
            let contact = spear::contact(
                &spear,
                charged,
                attacker_speed,
                relative,
                spear::PLAYER_FACTOR,
            );
            if !contact.any() {
                continue;
            }
            let bonus = if contact.deals_damage {
                spear.bonus_damage(relative)
            } else {
                0.0
            };
            let amount = PLAYER_BASE_ATTACK + bonus;
            if self.stab_attack(
                id,
                candidate.entity,
                amount,
                contact.deals_damage,
                contact.knocks_back,
                contact.dismounts,
            ) {
                hits += 1;
            }
        }
        entries.retain(|_, seen| {
            matches!(seen, MetaValue::Int(at)
                if (tick as i64).saturating_sub(*at) <= i64::from(spear::SPEAR_MOBS_WINDOW))
        });
        let count = entries.len();
        if let Some(player) = self.players.get_mut(&id) {
            player
                .volatile_meta
                .set(STABBED_KEY, MetaValue::Map(entries))
                .ok();
        }
        if hits > 0 {
            let mut event = AdvancementEvent::named(SPEAR_MOBS);
            event.amounts.push(("count", count as f64));
            self.trigger_event(id, event);
        }
    }

    fn stab_attack(
        &mut self,
        id: PlayerId,
        target: EntityId,
        damage: f32,
        deals_damage: bool,
        knocks_back: bool,
        dismounts: bool,
    ) -> bool {
        let Some(source) = self.source_of(id) else {
            return false;
        };
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let yaw = player.rotation.yaw;
        let attacker = player.entity_id;
        let held = player.inventory.held().cloned();
        let amount = match &held {
            Some(stack) => {
                let worn = [Worn {
                    stack,
                    slot: EquipSlot::Mainhand,
                }];
                enchant_effects::modify_damage(
                    &worn,
                    damage,
                    &LootContext::default(),
                    &mut self.rng,
                )
            }
            None => damage,
        };
        let mut hit = false;
        if deals_damage {
            let source = DamageSource::by(DamageType::PlayerAttack, source);
            let struck = self.struck(target);
            hit = self.strike(Some(id), struck, &source, amount, AfterHurt::Nothing);
        }
        if knocks_back {
            let victim = self
                .player_of_entity(target)
                .map_or(Target::Entity(target), Target::Player);
            self.knockback_target(
                victim,
                f64::from(spear::STAB_KNOCKBACK),
                mth::yaw_direction(yaw),
                EntityKnockbackCause::EntityAttack,
                attacker,
            );
            hit = true;
        }
        if dismounts && self.entities.rides.vehicle_of(target).is_some() {
            self.stop_riding(target);
            hit = true;
        }
        if hit && self.entities.store.get(target).is_some() {
            self.wear(id, Hand::Main, 1);
        }
        hit
    }

    pub fn spear_stab(&mut self, id: PlayerId) -> bool {
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        if player.vitals.is_dead() || player.game_mode == GameMode::Spectator {
            return false;
        }
        let Some(stack) = player.inventory.held().cloned() else {
            return false;
        };
        let Some(spear) = spear_of(&stack) else {
            return false;
        };
        let stats = attributes(player);
        let scale = attack_strength_scale(
            player.vitals.attack_ticker,
            stats.get(ATTACK_SPEED),
            JAB_LEEWAY_TICKS,
        );
        if scale < MINIMUM_ATTACK_CHARGE {
            return false;
        }
        let damage = stats.get(ATTACK_DAMAGE) as f32;
        let attacker = player.entity_id;
        let position = player.position;
        let (min, max) = spear::reach(player.game_mode == GameMode::Creative);
        let candidates = self.spear_candidates(id, min, max, spear::HITBOX_MARGIN);
        let mut hit = false;
        for candidate in candidates {
            hit |= self.stab_attack(id, candidate.entity, damage, true, true, false);
        }
        if hit {
            self.play_sound(
                None,
                position,
                spear.hit_sound(),
                SoundSource::Players,
                1.0,
                1.0,
            );
        }
        self.play_sound(
            None,
            position,
            spear.attack_sound(),
            SoundSource::Players,
            1.0,
            1.0,
        );
        if let Some(player) = self.players.get_mut(&id) {
            player.vitals.attack_ticker = 0;
        }
        self.push(
            Audience::Trackers(attacker),
            ClientEvent::EntitySwing {
                entity: attacker,
                hand: Hand::Main,
                style: SwingStyle::Stab,
                duration: spear.attack_ticks,
            },
        );
        self.run_post_piercing_attack(id);
        true
    }
}
