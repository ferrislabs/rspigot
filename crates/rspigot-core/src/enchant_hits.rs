use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rspigot_blocks::BlockKind;
use rspigot_data::enchantments::{
    self, BlockCondition, EffectRole, EnchantmentEffects, EntityEffect,
};
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{
    AttributeModifier, AttributeOperation, EffectDuration, EffectId, EffectInstance,
};
use rspigot_domain::entity::{Entities, Entity, EntityState, arrow};
use rspigot_domain::particle::Particle;
use rspigot_domain::player::GameMode;
use rspigot_domain::{BlockPos, BlockStateId, Vec3, mth};
use rspigot_domain::{EntityId, player::PlayerId};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::EquipmentSlot;
use rustc_hash::FxHashMap;

use crate::enchant_effects::{self, EquipSlot, Planned, Worn};
use crate::enchant_scene::{EffectScene, EntityFacts, LocationFacts, PlayerFacts};
use crate::events::{ClientEvent, Hand};
use crate::game_events::GameEventContext;
use crate::health::{
    CHEST_SLOT, DamageSource, FEET_SLOT, HEAD_SLOT, LEGS_SLOT, OFFHAND_SLOT, attributes,
};
use crate::loot::{LootContext, LootDamage};
use crate::player::Player;
use crate::plugins::events::{
    CreatureSpawnReason, EntityExhaustionReason, EntityPotionEffectCause,
};
use crate::plugins::{Birth, Combuster, Holder};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const TICKS_PER_SECOND: f32 = 20.0;
const PUNCH_SCALE: f64 = 0.6;
const PUNCH_LIFT: f64 = 0.1;
const PARTICLE_RANGE: f64 = 32.0;
const MOVEMENT_BLOCK_DEPTH: f64 = 0.500_001;
const LIGHTNING_BOLT: &str = "minecraft:lightning_bolt";

pub(crate) fn worn_gear<L>(player: &Player<L>) -> Vec<Worn<'_>> {
    let inventory = &player.inventory;
    [
        (inventory.held(), EquipSlot::Mainhand),
        (inventory.get(OFFHAND_SLOT), EquipSlot::Offhand),
        (inventory.get(HEAD_SLOT), EquipSlot::Head),
        (inventory.get(CHEST_SLOT), EquipSlot::Chest),
        (inventory.get(LEGS_SLOT), EquipSlot::Legs),
        (inventory.get(FEET_SLOT), EquipSlot::Feet),
    ]
    .into_iter()
    .filter_map(|(stack, slot)| {
        Some(Worn {
            stack: stack?,
            slot,
        })
    })
    .collect()
}

const MOB_GEAR: [(EquipmentSlot, EquipSlot); 6] = [
    (EquipmentSlot::Mainhand, EquipSlot::Mainhand),
    (EquipmentSlot::Offhand, EquipSlot::Offhand),
    (EquipmentSlot::Head, EquipSlot::Head),
    (EquipmentSlot::Chest, EquipSlot::Chest),
    (EquipmentSlot::Legs, EquipSlot::Legs),
    (EquipmentSlot::Feet, EquipSlot::Feet),
];

fn gear_in<'a, L>(
    players: &'a FxHashMap<PlayerId, Player<L>>,
    entities: &'a Entities,
    entity: EntityId,
) -> Vec<Worn<'a>> {
    if let Some(player) = players.values().find(|p| p.entity_id == entity) {
        return worn_gear(player);
    }
    let Some(living) = entities.get(entity).and_then(Entity::living) else {
        return Vec::new();
    };
    MOB_GEAR
        .iter()
        .filter_map(|(slot, equip)| {
            Some(Worn {
                stack: living.equipment(*slot)?,
                slot: *equip,
            })
        })
        .collect()
}

struct ReplaceDisk<'a> {
    radius: f32,
    height: f32,
    offset: [i32; 3],
    block: &'a str,
    state: &'a [(&'a str, &'a str)],
    predicate: Option<&'a BlockCondition>,
    trigger_game_event: Option<&'a str>,
}

fn collect_attribute_grants(effect: &EntityEffect, out: &mut Vec<(&'static str, &'static str)>) {
    match effect {
        EntityEffect::Attribute { id, attribute, .. } => out.push((attribute, id)),
        EntityEffect::AllOf(effects) => {
            for effect in *effects {
                collect_attribute_grants(effect, out);
            }
        }
        _ => {}
    }
}

fn local_vector(yaw: f32, pitch: f32, direction: [f32; 3]) -> Vec3 {
    let forward = mth::view_vector(pitch, yaw);
    let up = mth::view_vector(pitch - 90.0, yaw);
    let left = Vec3::new(
        forward.y * up.z - forward.z * up.y,
        forward.z * up.x - forward.x * up.z,
        forward.x * up.y - forward.y * up.x,
    );
    forward.scale(f64::from(direction[2]))
        + up.scale(f64::from(direction[1]))
        + left.scale(f64::from(direction[0]))
}

fn between(rng: &mut impl Rng, min: f32, max: f32) -> f32 {
    rng.random::<f32>() * (max - min) + min
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn player_protection(&self, id: PlayerId, source: &DamageSource) -> f32 {
        let Some(player) = self.players.get(&id) else {
            return 0.0;
        };
        let this = crate::loot::LootEntity::of(Kind::Player);
        let damage = self.damage_facts(source);
        let context = LootContext {
            this: Some(&this),
            damage: Some(&damage),
            ..LootContext::default()
        };
        enchant_effects::damage_protection(
            &worn_gear(player),
            &context,
            &mut StdRng::seed_from_u64(0),
        )
    }

    pub(crate) fn mob_protection(&self, id: EntityId, source: &DamageSource) -> f32 {
        let Some(facts) = self.entity_loot_facts(id) else {
            return 0.0;
        };
        let damage = self.damage_facts(source);
        let context = LootContext {
            this: Some(&facts),
            damage: Some(&damage),
            ..LootContext::default()
        };
        enchant_effects::damage_protection(
            &gear_in(&self.players, &self.entities.store, id),
            &context,
            &mut StdRng::seed_from_u64(0),
        )
    }

    pub(crate) fn run_post_attack(&mut self, source: &DamageSource, victim: EntityId) {
        if source.kind == DamageType::Thorns || source.causing.is_none() {
            return;
        }
        let Some(facts) = self.entity_loot_facts(victim) else {
            return;
        };
        let damage = self.damage_facts(source);
        let attacker = source.causing.as_ref().map(|c| c.entity);
        let direct = source.direct.as_ref().map(|d| d.entity);
        let victim_state = self.enchant_facts(victim);
        let attacker_state = attacker.and_then(|entity| self.enchant_facts(entity));
        let direct_state = direct.and_then(|entity| self.enchant_facts(entity));
        let weather = Some((self.world.level.raining, self.world.level.thundering));
        let planned = {
            let gear_of = |entity: Option<EntityId>| {
                entity
                    .map(|entity| gear_in(&self.players, &self.entities.store, entity))
                    .unwrap_or_default()
            };
            let attacker_gear = gear_of(attacker);
            let victim_gear = gear_of(Some(victim));
            if attacker_gear.is_empty() && victim_gear.is_empty() {
                return;
            }
            let scene = EffectScene {
                loot: LootContext {
                    this: Some(&facts),
                    damage: Some(&damage),
                    weather,
                    ..LootContext::default()
                },
                this: victim_state.as_ref(),
                attacker: attacker_state.as_ref(),
                direct_attacker: direct_state.as_ref(),
                location: None,
                active: true,
            };
            enchant_effects::post_attack_in(&attacker_gear, &victim_gear, &scene, &mut self.rng)
        };
        for plan in planned {
            self.apply_planned(&plan, victim, attacker, direct);
        }
    }

    pub(crate) fn run_weapon_post_attack(
        &mut self,
        weapon: &rspigot_domain::item::ItemStack,
        source: &DamageSource,
        victim: EntityId,
    ) {
        let Some(facts) = self.entity_loot_facts(victim) else {
            return;
        };
        let damage = self.damage_facts(source);
        let attacker = source.causing.as_ref().map(|c| c.entity);
        let direct = source.direct.as_ref().map(|d| d.entity);
        let victim_state = self.enchant_facts(victim);
        let attacker_state = attacker.and_then(|entity| self.enchant_facts(entity));
        let direct_state = direct.and_then(|entity| self.enchant_facts(entity));
        let weather = Some((self.world.level.raining, self.world.level.thundering));
        let held = [Worn {
            stack: weapon,
            slot: EquipSlot::Mainhand,
        }];
        let scene = EffectScene {
            loot: LootContext {
                this: Some(&facts),
                damage: Some(&damage),
                weather,
                ..LootContext::default()
            },
            this: victim_state.as_ref(),
            attacker: attacker_state.as_ref(),
            direct_attacker: direct_state.as_ref(),
            location: None,
            active: true,
        };
        let planned = enchant_effects::post_attack_in(&held, &[], &scene, &mut self.rng);
        for plan in planned {
            self.apply_planned(&plan, victim, attacker, direct);
        }
    }

    pub(crate) fn enchant_facts(&self, entity: EntityId) -> Option<EntityFacts> {
        if let Some(player) = self.players.values().find(|p| p.entity_id == entity) {
            let position = player.position;
            let cell = position.block();
            let gamemode = match player.game_mode {
                GameMode::Survival => "survival",
                GameMode::Creative => "creative",
                GameMode::Adventure => "adventure",
                GameMode::Spectator => "spectator",
            };
            return Some(EntityFacts {
                kind: Some("minecraft:player"),
                on_ground: player.on_ground,
                flying: player.flying,
                fall_flying: player.fall_flying,
                in_water: self.in_water(position),
                has_vehicle: self.entities.rides.vehicle_of(entity).is_some(),
                movement_block: self.movement_block_below(position),
                age: i64::from(player.vitals.tick_count),
                horizontal_speed: player.known_movement.horizontal_distance_sqr().sqrt() as f32,
                fall_distance: player.vitals.fall_distance as f32,
                player: Some(PlayerFacts {
                    gamemode,
                    food: player.vitals.food.food,
                }),
                sees_sky: self.motion_blocking_top(cell.x, cell.z) <= cell.y,
            });
        }
        let found = self.entities.store.get(entity)?;
        let base = &found.base;
        let cell = base.position.block();
        Some(EntityFacts {
            kind: Some(base.kind.name()),
            on_ground: base.on_ground,
            flying: false,
            fall_flying: false,
            in_water: self.in_water(base.position),
            has_vehicle: self.entities.rides.vehicle_of(entity).is_some(),
            movement_block: self.movement_block_below(base.position),
            age: i64::from(base.tick_count),
            horizontal_speed: base.velocity.horizontal_distance_sqr().sqrt() as f32,
            fall_distance: base.fall_distance as f32,
            player: None,
            sees_sky: self.motion_blocking_top(cell.x, cell.z) <= cell.y,
        })
    }

    fn movement_block_below(&self, position: Vec3) -> Option<u32> {
        let below = Vec3::new(position.x, position.y - MOVEMENT_BLOCK_DEPTH, position.z).block();
        self.world
            .chunks
            .block(below)
            .and_then(BlockStateId::kind)
            .map(|kind| u32::from(kind.id().0))
    }

    fn position_of(&self, entity: EntityId) -> Option<Vec3> {
        self.players
            .values()
            .find(|player| player.entity_id == entity)
            .map(|player| player.position)
            .or_else(|| {
                self.entities
                    .store
                    .get(entity)
                    .map(|found| found.base.position)
            })
    }

    fn scene_effects(
        &mut self,
        entity: EntityId,
        select: fn(&'static EnchantmentEffects) -> &'static [enchantments::ConditionalEffect],
        active: bool,
    ) -> Vec<Planned> {
        let Some(facts) = self.enchant_facts(entity) else {
            return Vec::new();
        };
        let weather = Some((self.world.level.raining, self.world.level.thundering));
        let location = self.position_of(entity).map(|position| {
            let cell = position.block();
            LocationFacts {
                block: self
                    .world
                    .chunks
                    .block(cell)
                    .and_then(BlockStateId::kind)
                    .map(|kind| u32::from(kind.id().0)),
                sees_sky: facts.sees_sky,
            }
        });
        let loot_facts = self.entity_loot_facts(entity);
        let scene = EffectScene {
            loot: LootContext {
                this: loot_facts.as_ref(),
                weather,
                ..LootContext::default()
            },
            this: Some(&facts),
            attacker: None,
            direct_attacker: Some(&facts),
            location,
            active,
        };
        let gear = gear_in(&self.players, &self.entities.store, entity);
        enchant_effects::conditional(&gear, select, EffectRole::Attacker, &scene, &mut self.rng)
    }

    fn run_own_effects(
        &mut self,
        entity: EntityId,
        select: fn(&'static EnchantmentEffects) -> &'static [enchantments::ConditionalEffect],
    ) -> Vec<Planned> {
        let planned = self.scene_effects(entity, select, true);
        for plan in &planned {
            self.apply_entity_effect_plan(plan.effect, plan.level, entity, entity, Some(plan.slot));
        }
        planned
    }

    pub(crate) fn tick_enchantment_effects(&mut self) {
        let ids: Vec<(EntityId, bool)> = self
            .players
            .values()
            .filter(|player| !player.vitals.is_dead() && player.game_mode != GameMode::Spectator)
            .map(|player| {
                let before = (player.position - player.known_movement).block();
                (
                    player.entity_id,
                    player.moved_this_tick && before != player.position.block(),
                )
            })
            .collect();
        for (entity, moved_block) in ids {
            self.run_own_effects(entity, |e| e.tick);
            if moved_block {
                self.run_location_changed(entity);
            }
        }
    }

    pub(crate) fn run_location_changed(&mut self, entity: EntityId) {
        let planned = self.run_own_effects(entity, |e| e.location_changed);
        let Some(player) = self.player_of(entity) else {
            return;
        };
        let mut granted: Vec<(&'static str, &'static str)> = Vec::new();
        for plan in &planned {
            collect_attribute_grants(plan.effect, &mut granted);
        }
        let mut stale: Vec<(&'static str, &'static str)> = Vec::new();
        for def in enchantments::all() {
            if let Some(effects) = enchantments::effects(def.name) {
                for entry in effects.location_changed {
                    collect_attribute_grants(&entry.effect, &mut stale);
                }
            }
        }
        for (attribute, id) in stale {
            if !granted.contains(&(attribute, id)) {
                self.remove_enchant_modifier(player, attribute, id);
            }
        }
    }

    pub fn run_post_piercing_attack(&mut self, player: PlayerId) {
        let Some(entity) = self.players.get(&player).map(|p| p.entity_id) else {
            return;
        };
        let Some(facts) = self.enchant_facts(entity) else {
            return;
        };
        let weather = Some((self.world.level.raining, self.world.level.thundering));
        let loot_facts = self.entity_loot_facts(entity);
        let planned = {
            let Some(held) = self.players.get(&player).and_then(|p| {
                p.inventory.held().map(|stack| Worn {
                    stack,
                    slot: EquipSlot::Mainhand,
                })
            }) else {
                return;
            };
            let scene = EffectScene {
                loot: LootContext {
                    this: loot_facts.as_ref(),
                    weather,
                    ..LootContext::default()
                },
                this: Some(&facts),
                attacker: Some(&facts),
                direct_attacker: Some(&facts),
                location: None,
                active: true,
            };
            enchant_effects::conditional(
                &[held],
                |e| e.post_piercing_attack,
                EffectRole::Attacker,
                &scene,
                &mut self.rng,
            )
        };
        for plan in planned {
            self.apply_entity_effect_plan(plan.effect, plan.level, entity, entity, Some(plan.slot));
        }
    }

    pub(crate) fn run_hit_block(
        &mut self,
        weapon: &rspigot_domain::item::ItemStack,
        projectile: EntityId,
        pos: BlockPos,
    ) {
        let Some(facts) = self.enchant_facts(projectile) else {
            return;
        };
        let weather = Some((self.world.level.raining, self.world.level.thundering));
        let top = self.motion_blocking_top(pos.x, pos.z);
        let location = LocationFacts {
            block: self
                .world
                .chunks
                .block(pos)
                .and_then(BlockStateId::kind)
                .map(|kind| u32::from(kind.id().0)),
            sees_sky: top <= pos.y + 1,
        };
        let loot_facts = self.entity_loot_facts(projectile);
        let held = [Worn {
            stack: weapon,
            slot: EquipSlot::Mainhand,
        }];
        let scene = EffectScene {
            loot: LootContext {
                this: loot_facts.as_ref(),
                weather,
                ..LootContext::default()
            },
            this: Some(&facts),
            attacker: None,
            direct_attacker: Some(&facts),
            location: Some(location),
            active: true,
        };
        let planned = enchant_effects::conditional(
            &held,
            |e| e.hit_block,
            EffectRole::DamagingEntity,
            &scene,
            &mut self.rng,
        );
        for plan in planned {
            self.apply_entity_effect_plan(plan.effect, plan.level, projectile, projectile, None);
        }
    }

    fn remove_enchant_modifier(&mut self, player: PlayerId, attribute: &'static str, id: &str) {
        let Some(entry) = self.players.get_mut(&player) else {
            return;
        };
        let Some(edit) = entry
            .attribute_edits
            .iter_mut()
            .find(|edit| edit.attribute == attribute)
        else {
            return;
        };
        let before = edit.modifiers.len();
        edit.modifiers.retain(|modifier| modifier.id != id);
        if edit.modifiers.len() != before {
            self.clamp_vitals(player);
            self.sync_attributes(player, &[attribute]);
        }
    }

    fn add_enchant_modifier(
        &mut self,
        player: PlayerId,
        attribute: &'static str,
        modifier: AttributeModifier,
    ) {
        let Some(entry) = self.players.get_mut(&player) else {
            return;
        };
        let base = attributes(entry).state(attribute).base;
        match entry
            .attribute_edits
            .iter_mut()
            .find(|edit| edit.attribute == attribute)
        {
            Some(edit) => {
                edit.modifiers.retain(|existing| existing.id != modifier.id);
                edit.modifiers.push(modifier);
            }
            None => entry
                .attribute_edits
                .push(rspigot_domain::entity::living::AttributeInstance {
                    attribute,
                    base,
                    modifiers: vec![modifier],
                }),
        }
        self.clamp_vitals(player);
        self.sync_attributes(player, &[attribute]);
    }

    fn entity_for(
        role: EffectRole,
        victim: EntityId,
        attacker: Option<EntityId>,
        direct: Option<EntityId>,
    ) -> Option<EntityId> {
        match role {
            EffectRole::Victim => Some(victim),
            EffectRole::Attacker => attacker,
            EffectRole::DamagingEntity => direct,
        }
    }

    fn apply_planned(
        &mut self,
        plan: &Planned,
        victim: EntityId,
        attacker: Option<EntityId>,
        direct: Option<EntityId>,
    ) {
        let target = Self::entity_for(plan.affected, victim, attacker, direct);
        let owner = Self::entity_for(plan.enchanted, victim, attacker, direct);
        if let (Some(target), Some(owner)) = (target, owner) {
            self.apply_entity_effect_plan(plan.effect, plan.level, target, owner, Some(plan.slot));
        }
    }

    pub(crate) fn apply_entity_effect_plan(
        &mut self,
        effect: &EntityEffect,
        level: i32,
        target: EntityId,
        owner: EntityId,
        worn: Option<EquipSlot>,
    ) {
        match effect {
            EntityEffect::AllOf(effects) => {
                for effect in *effects {
                    self.apply_entity_effect_plan(effect, level, target, owner, worn);
                }
            }
            EntityEffect::Ignite(seconds) => {
                let seconds = seconds.calculate(level);
                let holder = self
                    .player_of(target)
                    .map_or(Holder::Mob(target), Holder::Player);
                let combuster = self
                    .subject_view(owner)
                    .map_or(Combuster::Nothing, Combuster::Entity);
                self.combust(holder, seconds, combuster);
            }
            EntityEffect::ApplyMobEffect {
                effect,
                min_duration,
                max_duration,
                min_amplifier,
                max_amplifier,
            } => {
                let Some(id) = EffectId::from_name(effect) else {
                    return;
                };
                let seconds = between(
                    &mut self.rng,
                    min_duration.calculate(level),
                    max_duration.calculate(level),
                );
                let ticks = (seconds * TICKS_PER_SECOND).round().max(0.0) as u32;
                let amplifier = between(
                    &mut self.rng,
                    min_amplifier.calculate(level),
                    max_amplifier.calculate(level),
                )
                .round()
                .clamp(0.0, 255.0) as u8;
                let instance =
                    EffectInstance::new(id, EffectDuration::Ticks(ticks), amplifier, true);
                self.add_effect_to(target, instance, EntityPotionEffectCause::Attack);
            }
            EntityEffect::DamageEntity {
                damage_type,
                min_damage,
                max_damage,
            } => {
                let Some(kind) = DamageType::from_name(damage_type) else {
                    return;
                };
                let amount = between(
                    &mut self.rng,
                    min_damage.calculate(level),
                    max_damage.calculate(level),
                );
                let source = match self.player_of(owner) {
                    Some(player) => self.source_of(player),
                    None => self.mob_source(owner),
                };
                let source = match source {
                    Some(source) => DamageSource::by(kind, source),
                    None => DamageSource::new(kind),
                };
                match self.player_of(target) {
                    Some(player) => {
                        self.hurt(player, &source, amount);
                    }
                    None => {
                        self.hurt_mob(target, &source, amount);
                    }
                }
            }
            EntityEffect::ChangeItemDamage(amount) => {
                if let Some(slot) = worn {
                    self.wear_enchanted(owner, slot, amount.calculate(level) as i32);
                }
            }
            EntityEffect::ApplyImpulse {
                direction,
                coordinate_scale,
                magnitude,
            } => {
                self.apply_impulse(
                    target,
                    *direction,
                    *coordinate_scale,
                    magnitude.calculate(level),
                );
            }
            EntityEffect::ApplyExhaustion(amount) => {
                if let Some(player) = self.player_of(target) {
                    self.exhaust(
                        player,
                        amount.calculate(level),
                        EntityExhaustionReason::Unknown,
                    );
                }
            }
            EntityEffect::PlaySound {
                sounds,
                volume,
                pitch_min,
                pitch_max,
            } => {
                let Some(position) = self.position_of(target) else {
                    return;
                };
                if sounds.is_empty() {
                    return;
                }
                let sound = sounds[self.rng.random_range(0..sounds.len())];
                let pitch = between(&mut self.rng, *pitch_min, *pitch_max);
                let source = if self.player_of(target).is_some() {
                    SoundSource::Players
                } else {
                    SoundSource::Neutral
                };
                self.play_sound(None, position, sound, source, *volume, pitch);
            }
            EntityEffect::SummonEntity { entity, .. } => {
                self.summon_by_effect(entity, target, owner);
            }
            EntityEffect::SpawnParticles {
                particle,
                speed,
                horizontal_movement_scale,
                vertical_base,
                vertical_offset,
            } => {
                self.spawn_effect_particles(
                    particle,
                    target,
                    *speed,
                    *horizontal_movement_scale,
                    *vertical_base,
                    *vertical_offset,
                );
            }
            EntityEffect::Explode {
                knockback_multiplier,
                radius,
                sound,
                ..
            } => {
                let Some(center) = self.position_of(target) else {
                    return;
                };
                self.wind_burst(
                    Some(owner),
                    center,
                    radius.calculate(level),
                    knockback_multiplier.calculate(level),
                    sound,
                );
            }
            EntityEffect::ReplaceDisk {
                radius,
                height,
                offset,
                block,
                state,
                predicate,
                trigger_game_event,
            } => {
                self.replace_disk(
                    target,
                    ReplaceDisk {
                        radius: radius.calculate(level),
                        height: height.calculate(level),
                        offset: *offset,
                        block,
                        state,
                        predicate: predicate.as_ref(),
                        trigger_game_event: *trigger_game_event,
                    },
                );
            }
            EntityEffect::Attribute {
                id,
                attribute,
                amount,
                operation,
            } => {
                let Some(player) = self.player_of(target) else {
                    return;
                };
                let operation = match *operation {
                    "add_multiplied_base" => AttributeOperation::AddMultipliedBase,
                    "add_multiplied_total" => AttributeOperation::AddMultipliedTotal,
                    _ => AttributeOperation::AddValue,
                };
                let modifier = AttributeModifier {
                    id: std::borrow::Cow::Borrowed(id),
                    amount: f64::from(amount.calculate(level)),
                    operation,
                };
                self.add_enchant_modifier(player, attribute, modifier);
            }
        }
    }

    fn apply_impulse(
        &mut self,
        target: EntityId,
        direction: [f32; 3],
        scale: [f32; 3],
        magnitude: f32,
    ) {
        let (yaw, pitch) = match self.players.values().find(|p| p.entity_id == target) {
            Some(player) => (player.rotation.yaw, player.rotation.pitch),
            None => match self.entities.store.get(target) {
                Some(found) => (found.base.yaw, found.base.pitch),
                None => return,
            },
        };
        let local = local_vector(yaw, pitch, direction);
        let push = Vec3::new(
            local.x * f64::from(scale[0]),
            local.y * f64::from(scale[1]),
            local.z * f64::from(scale[2]),
        )
        .scale(f64::from(magnitude));
        if let Some(player) = self.players.values_mut().find(|p| p.entity_id == target) {
            player.vitals.velocity = player.vitals.velocity + push;
            player.vitals.sync_velocity = true;
        } else if let Some(found) = self.entities.store.get_mut(target) {
            found.base.velocity = found.base.velocity + push;
            found.base.needs_sync = true;
        }
    }

    fn summon_by_effect(&mut self, name: &str, target: EntityId, owner: EntityId) {
        let Some(at) = self.position_of(target) else {
            return;
        };
        if name == LIGHTNING_BOLT {
            self.spawn_bolt(at, false, CreatureSpawnReason::Enchantment);
            if let Some(player) = self.player_of(owner) {
                self.trigger_channeled_lightning(player, at);
            }
            return;
        }
        let Some(kind) = Kind::from_name(name) else {
            return;
        };
        let id = self.allocate_entity_id_for(kind);
        let uuid = self.random_uuid();
        let compound = rspigot_nbt::Compound::new().with("id", kind.name());
        let Ok(mut entity) = rspigot_domain::entity::save::load_entity(&compound, id, uuid) else {
            return;
        };
        entity.base.snap_to(at, 0.0, 0.0);
        self.spawn_entity(entity, Birth::of(CreatureSpawnReason::Enchantment));
    }

    fn spawn_effect_particles(
        &mut self,
        particle: &'static str,
        target: EntityId,
        speed: f32,
        movement_scale: f32,
        vertical_base: f32,
        vertical_offset: f32,
    ) {
        let (position, movement, width) =
            match self.players.values().find(|p| p.entity_id == target) {
                Some(player) => (player.position, player.known_movement, 0.6),
                None => match self.entities.store.get(target) {
                    Some(found) => (
                        found.base.position,
                        found.base.velocity,
                        f64::from(found.base.dimensions.width),
                    ),
                    None => return,
                },
            };
        let x = position.x + (f64::from(self.rng.random::<f32>()) - 0.5) * width;
        let z = position.z + (f64::from(self.rng.random::<f32>()) - 0.5) * width;
        let at = Vec3::new(x, position.y + f64::from(vertical_offset), z);
        let velocity = Vec3::new(
            movement.x * f64::from(movement_scale),
            f64::from(vertical_base),
            movement.z * f64::from(movement_scale),
        );
        self.push(
            Audience::Nearby {
                position: at,
                range: PARTICLE_RANGE,
                except: None,
            },
            ClientEvent::Particle(Particle {
                count: 0,
                spread: velocity,
                speed: f64::from(speed),
                ..Particle::single(particle, at)
            }),
        );
    }

    fn replace_disk(&mut self, target: EntityId, disk: ReplaceDisk<'_>) {
        let Some(position) = self.position_of(target) else {
            return;
        };
        let Some(kind) = BlockKind::from_name(disk.block) else {
            return;
        };
        let Some(replacement) = kind.state_with(disk.state) else {
            return;
        };
        let origin = position
            .block()
            .offset(disk.offset[0], disk.offset[1], disk.offset[2]);
        let radius = disk.radius as i32;
        let layers = (disk.height as i32 - 1).min(0);
        let limit = f64::from(radius) * f64::from(radius);
        let mut cells = Vec::new();
        for x in -radius..=radius {
            for z in -radius..=radius {
                for y in layers..=0 {
                    let pos = origin.offset(x, y, z);
                    let dx = f64::from(pos.x) + 0.5 - position.x;
                    let dz = f64::from(pos.z) + 0.5 - position.z;
                    if dx * dx + dz * dz < limit {
                        cells.push(pos);
                    }
                }
            }
        }
        for pos in cells {
            if disk
                .predicate
                .is_some_and(|predicate| !self.block_condition_holds(predicate, pos))
            {
                continue;
            }
            if !self.set_block(pos, replacement) {
                continue;
            }
            if kind == BlockKind::FrostedIce {
                self.schedule_frosted_ice(pos, crate::frosted_ice::FORM_DELAY);
            }
            if let Some(event) = disk
                .trigger_game_event
                .and_then(rspigot_domain::game_event::GameEvent::from_name)
            {
                self.game_event(event, pos.center(), GameEventContext::of(Some(target)));
            }
        }
    }

    fn block_condition_holds(&self, condition: &BlockCondition, pos: BlockPos) -> bool {
        let state_at = |offset: &[i32; 3]| {
            self.world
                .chunks
                .block(pos.offset(offset[0], offset[1], offset[2]))
        };
        match condition {
            BlockCondition::AllOf(terms) => terms
                .iter()
                .all(|term| self.block_condition_holds(term, pos)),
            BlockCondition::MatchingBlockTag { offset, tag } => state_at(offset)
                .and_then(BlockStateId::kind)
                .is_some_and(|kind| {
                    rspigot_data::tags::tag("minecraft:block", tag)
                        .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
                }),
            BlockCondition::MatchingBlocks { offset, blocks } => state_at(offset)
                .and_then(BlockStateId::kind)
                .is_some_and(|kind| {
                    blocks.iter().any(|name| {
                        name.trim_start_matches("minecraft:")
                            == kind.name().trim_start_matches("minecraft:")
                    })
                }),
            BlockCondition::MatchingFluids { offset, fluids } => {
                let source = state_at(offset).is_some_and(|state| {
                    state.kind() == Some(BlockKind::Water) && state.property("level") == Some("0")
                });
                source
                    && fluids
                        .iter()
                        .any(|name| name.trim_start_matches("minecraft:") == "water")
            }
            BlockCondition::Unobstructed => true,
        }
    }

    fn wear_enchanted(&mut self, owner: EntityId, slot: EquipSlot, amount: i32) {
        if let Some(player) = self.player_of(owner) {
            let armor = match slot {
                EquipSlot::Head => HEAD_SLOT,
                EquipSlot::Chest => CHEST_SLOT,
                EquipSlot::Legs => LEGS_SLOT,
                EquipSlot::Feet => FEET_SLOT,
                EquipSlot::Mainhand => return self.wear(player, Hand::Main, amount),
                EquipSlot::Offhand => return self.wear(player, Hand::Off, amount),
            };
            self.wear_slot(player, Hand::Main, armor, amount);
        } else if let Some((worn, _)) = MOB_GEAR.iter().find(|(_, equip)| *equip == slot) {
            let worn_stack = self
                .entities
                .store
                .get(owner)
                .and_then(Entity::living)
                .and_then(|living| living.equipment(*worn))
                .cloned();
            let amount = match worn_stack {
                Some(stack) => enchant_effects::modify_item_damage(&stack, amount, &mut self.rng),
                None => amount,
            };
            self.damage_mob_equipment(owner, *worn, amount);
        }
    }

    pub(crate) fn equipment_drop_bonus(&mut self, victim: EntityId, source: &DamageSource) -> f32 {
        let Some(attacker) = source.causing.as_ref().map(|cause| cause.entity) else {
            return 0.0;
        };
        let Some(facts) = self.entity_loot_facts(victim) else {
            return 0.0;
        };
        let damage = self.damage_facts(source);
        let gear = gear_in(&self.players, &self.entities.store, attacker);
        let context = LootContext {
            this: Some(&facts),
            damage: Some(&damage),
            ..LootContext::default()
        };
        enchant_effects::equipment_drop_bonus(&gear, 0.0, &context, &mut self.rng)
    }

    pub fn enchanted_mob_experience(
        &mut self,
        attacker: EntityId,
        victim: EntityId,
        base: i32,
    ) -> i32 {
        let Some(facts) = self.entity_loot_facts(victim) else {
            return base;
        };
        let context = LootContext {
            this: Some(&facts),
            ..LootContext::default()
        };
        let gear = gear_in(&self.players, &self.entities.store, attacker);
        enchant_effects::modify_mob_experience(&gear, base, &context, &mut self.rng)
    }

    pub fn is_enchant_immune(&self, entity: EntityId, source: &DamageSource) -> bool {
        let Some(facts) = self.entity_loot_facts(entity) else {
            return false;
        };
        let damage = self.damage_facts(source);
        let context = LootContext {
            this: Some(&facts),
            damage: Some(&damage),
            ..LootContext::default()
        };
        enchant_effects::damage_immune(
            &gear_in(&self.players, &self.entities.store, entity),
            &context,
            &mut StdRng::seed_from_u64(0),
        )
    }

    pub(crate) fn arrow_enchantments(&mut self, arrow: EntityId, target: EntityId) -> (f64, f32) {
        let Some(Entity {
            base,
            state: EntityState::Arrow(state),
            ..
        }) = self.entities.store.get(arrow)
        else {
            return (0.0, 0.0);
        };
        let trident = base.kind == Kind::Trident;
        let base_damage = if trident {
            f64::from(arrow::TRIDENT_DAMAGE)
        } else {
            state.base_damage
        };
        let kind = if trident {
            DamageType::Trident
        } else {
            DamageType::Arrow
        };
        let owner = state.owner;
        let Some(weapon) = state.weapon.clone() else {
            return (base_damage, 0.0);
        };
        let owner_id = self.owner_entity(owner);
        let damage = LootDamage {
            kind,
            source: owner_id.and_then(|owner| self.entity_loot_facts(owner)),
            direct: self.entity_loot_facts(arrow),
            is_direct: false,
        };
        let facts = self.entity_loot_facts(target);
        let context = LootContext {
            this: facts.as_ref(),
            damage: Some(&damage),
            ..LootContext::default()
        };
        let worn = [Worn {
            stack: &weapon,
            slot: EquipSlot::Mainhand,
        }];
        let damage =
            enchant_effects::modify_damage(&worn, base_damage as f32, &context, &mut self.rng);
        let knockback = enchant_effects::modify_knockback(&worn, 0.0, &context, &mut self.rng);
        (f64::from(damage), knockback)
    }

    pub(crate) fn push_by_arrow(&mut self, target: EntityId, heading: Vec3, punch: f32) {
        if punch <= 0.0 {
            return;
        }
        let horizontal = (heading.x * heading.x + heading.z * heading.z).sqrt();
        if horizontal <= 0.0 {
            return;
        }
        let scale = (1.0 - self.knockback_resistance_of(target)).max(0.0);
        let strength = f64::from(punch) * PUNCH_SCALE * scale / horizontal;
        let push = Vec3::new(heading.x * strength, PUNCH_LIFT, heading.z * strength);
        if let Some(player) = self.players.values_mut().find(|p| p.entity_id == target) {
            player.vitals.velocity = player.vitals.velocity + push;
            player.vitals.sync_velocity = true;
        } else if let Some(entity) = self.entities.store.get_mut(target) {
            entity.base.velocity = entity.base.velocity + push;
            entity.base.needs_sync = true;
        }
    }

    fn player_of(&self, entity: EntityId) -> Option<PlayerId> {
        self.players
            .values()
            .find(|player| player.entity_id == entity)
            .map(|player| player.id)
    }
}
