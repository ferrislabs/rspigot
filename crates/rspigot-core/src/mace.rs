use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rspigot_domain::attribute::KNOCKBACK_RESISTANCE;
use rspigot_domain::entity::armor_stand;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{EntityId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::ItemKind;

use crate::enchant_effects::{self, EquipSlot, Worn};
use crate::enchant_hits::worn_gear;
use crate::health::{DamageSource, attributes};
use crate::loot::LootContext;
use crate::plugins::Holder;
use crate::plugins::events::EntityKnockbackCause;
use crate::ports::ClientLink;
use crate::server::Server;

const SMASH_FALL_THRESHOLD: f32 = 1.5;
const HEAVY_FALL_THRESHOLD: f32 = 5.0;
const KNOCKBACK_RADIUS: f64 = 3.5;
const KNOCKBACK_POWER: f64 = 0.7;
const KNOCKBACK_LIFT: f64 = 0.7;
const ATTACKER_LIFT: f64 = 0.01;
const SMASH_EVENT: i32 = 2013;
const SMASH_EVENT_DATA: i32 = 750;
const ON_POS_OFFSET: f64 = 0.2;
const SOUND_AIR: &str = "minecraft:item.mace.smash_air";
const SOUND_GROUND: &str = "minecraft:item.mace.smash_ground";
const SOUND_GROUND_HEAVY: &str = "minecraft:item.mace.smash_ground_heavy";

pub(crate) fn smashes<L>(player: &crate::player::Player<L>) -> bool {
    player.inventory.held().is_some_and(is_mace)
        && can_smash(player.vitals.fall_distance as f32, player.fall_flying)
}

pub(crate) fn can_smash(fall_distance: f32, fall_flying: bool) -> bool {
    fall_distance > SMASH_FALL_THRESHOLD && !fall_flying
}

pub(crate) fn fall_bonus(fall_distance: f32) -> f32 {
    if fall_distance <= 3.0 {
        4.0 * fall_distance
    } else if fall_distance <= 8.0 {
        12.0 + 2.0 * (fall_distance - 3.0)
    } else {
        22.0 + (fall_distance - 8.0)
    }
}

fn is_mace(stack: &ItemStack) -> bool {
    stack.item.kind() == Some(ItemKind::Mace)
}

fn holds_mace(weapon: &[Worn<'_>]) -> bool {
    weapon
        .iter()
        .any(|worn| worn.slot == EquipSlot::Mainhand && is_mace(worn.stack))
}

pub(crate) fn smash_bonus(
    weapon: &[Worn<'_>],
    fall_distance: f32,
    fall_flying: bool,
    context: &LootContext<'_>,
    rng: &mut (impl Rng + ?Sized),
) -> f32 {
    if !holds_mace(weapon) || !can_smash(fall_distance, fall_flying) {
        return 0.0;
    }
    let per_block = enchant_effects::smash_damage_per_fallen_block(weapon, context, rng);
    fall_bonus(fall_distance) + per_block * fall_distance
}

pub(crate) fn knockback_power(distance: f64, fall_distance: f32, resistance: f64) -> f64 {
    let heavy = if fall_distance > HEAVY_FALL_THRESHOLD {
        2.0
    } else {
        1.0
    };
    ((KNOCKBACK_RADIUS - distance) * KNOCKBACK_POWER * heavy * (1.0 - resistance)).max(0.0)
}

struct Shoved {
    player: Option<PlayerId>,
    entity: EntityId,
    offset: Vec3,
    resistance: Option<f64>,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn armor_effectiveness_against(
        &self,
        victim: EntityId,
        source: &DamageSource,
    ) -> f32 {
        let (Some(causing), Some(direct)) = (&source.causing, &source.direct) else {
            return 1.0;
        };
        if causing.entity != direct.entity {
            return 1.0;
        }
        let Some(attacker) = self
            .players
            .values()
            .find(|player| player.entity_id == causing.entity)
        else {
            return 1.0;
        };
        let weapon: Vec<Worn<'_>> = worn_gear(attacker)
            .into_iter()
            .filter(|worn| worn.slot == EquipSlot::Mainhand)
            .collect();
        let facts = self.entity_loot_facts(victim);
        let damage = self.damage_facts(source);
        let context = LootContext {
            this: facts.as_ref(),
            damage: Some(&damage),
            ..LootContext::default()
        };
        enchant_effects::armor_effectiveness(&weapon, &context, &mut StdRng::seed_from_u64(0))
    }

    pub(crate) fn mace_smash(&mut self, id: PlayerId, victim: EntityId, target_position: Vec3) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let fall = player.vitals.fall_distance as f32;
        if !smashes(player) {
            return;
        }
        let attacker = player.entity_id;
        let position = player.position;
        let grounded = self.smashed_on_ground(victim);
        let sound = if !grounded {
            SOUND_AIR
        } else if fall > HEAVY_FALL_THRESHOLD {
            SOUND_GROUND_HEAVY
        } else {
            SOUND_GROUND
        };
        self.play_sound(None, position, sound, SoundSource::Players, 1.0, 1.0);
        if let Some(player) = self.players.get_mut(&id) {
            player.vitals.velocity.y = ATTACKER_LIFT;
            player.vitals.sync_velocity = true;
        }
        self.smash_knockback(attacker, victim, target_position, fall);
        self.begin_impulse(id, position);
        if let Some(player) = self.players.get_mut(&id) {
            player.vitals.fall_distance = 0.0;
        }
    }

    fn smashed_on_ground(&self, victim: EntityId) -> bool {
        if let Some(player) = self.players.values().find(|p| p.entity_id == victim) {
            return player.on_ground;
        }
        self.entities
            .store
            .get(victim)
            .is_some_and(|entity| entity.base.on_ground)
    }

    fn smash_knockback(&mut self, attacker: EntityId, victim: EntityId, center: Vec3, fall: f32) {
        let floor = Vec3::new(center.x, center.y - ON_POS_OFFSET, center.z).block();
        self.level_event(SMASH_EVENT, floor, SMASH_EVENT_DATA);
        let range = KNOCKBACK_RADIUS * KNOCKBACK_RADIUS;
        let mut nearby: Vec<Shoved> = self
            .entities
            .store
            .iter()
            .filter(|e| e.base.id != attacker && e.base.id != victim)
            .filter(|e| e.living().is_some() && !e.base.is_removed() && e.is_simulated())
            .filter(|e| !(e.base.kind == Kind::ArmorStand && armor_stand::is_marker(&e.base)))
            .filter(|e| e.base.position.distance_squared(center) <= range)
            .map(|e| Shoved {
                player: None,
                entity: e.base.id,
                offset: e.base.position - center,
                resistance: None,
            })
            .collect();
        nearby.extend(
            self.players
                .values()
                .filter(|p| p.entity_id != attacker && p.entity_id != victim)
                .filter(|p| !p.vitals.is_dead() && !p.vitals.removed)
                .filter(|p| p.game_mode != GameMode::Spectator)
                .filter(|p| !(p.game_mode == GameMode::Creative && p.flying))
                .filter(|p| p.position.distance_squared(center) <= range)
                .map(|p| Shoved {
                    player: Some(p.id),
                    entity: p.entity_id,
                    offset: p.position - center,
                    resistance: Some(attributes(p).get(KNOCKBACK_RESISTANCE)),
                }),
        );
        for shoved in nearby {
            let resistance = shoved.resistance.unwrap_or_else(|| {
                self.entities
                    .store
                    .get_mut(shoved.entity)
                    .and_then(|entity| entity.living_mut())
                    .map_or(0.0, |living| living.attribute(KNOCKBACK_RESISTANCE))
            });
            let power = knockback_power(shoved.offset.length(), fall, resistance);
            if power <= 0.0 {
                continue;
            }
            let direction = shoved.offset.normalize().scale(power);
            let push = Vec3::new(direction.x, KNOCKBACK_LIFT, direction.z);
            let holder = shoved
                .player
                .map_or(Holder::Mob(shoved.entity), Holder::Player);
            self.push_for(
                holder,
                push,
                EntityKnockbackCause::EntityAttack,
                Some(attacker),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smashing_needs_a_fall_above_one_and_a_half_blocks_and_no_elytra() {
        assert!(!can_smash(1.5, false));
        assert!(can_smash(1.6, false));
        assert!(!can_smash(10.0, true));
    }

    #[test]
    fn the_fall_bonus_is_four_then_two_then_one_per_block() {
        assert_eq!(fall_bonus(2.0), 8.0);
        assert_eq!(fall_bonus(3.0), 12.0);
        assert_eq!(fall_bonus(8.0), 22.0);
        assert_eq!(fall_bonus(10.0), 24.0);
    }

    #[test]
    fn smash_knockback_fades_with_distance_and_doubles_when_heavy() {
        assert_eq!(knockback_power(3.5, 3.0, 0.0), 0.0);
        let light = knockback_power(1.0, 3.0, 0.0);
        assert!((light - 2.5 * 0.7).abs() < 1e-6);
        assert!((knockback_power(1.0, 6.0, 0.0) - light * 2.0).abs() < 1e-6);
        assert_eq!(knockback_power(1.0, 6.0, 1.0), 0.0);
    }
}
