use rand::Rng;
use rspigot_domain::attribute::{
    ARMOR, ARMOR_TOUGHNESS, Attributes, MAX_ABSORPTION, MAX_HEALTH, MOVEMENT_SPEED,
    equipment_modifiers, is_syncable,
};
use rspigot_domain::combat::{CombatEntry, CombatTracker};
use rspigot_domain::damage::{self, DamageTag, DamageType, java_clamp};
use rspigot_domain::effect::EffectId;
use rspigot_domain::experience::PlayerExperience;
use rspigot_domain::food::{FoodData, FoodEffect};
use rspigot_domain::item::{HOTBAR_START, Inventory, ItemStack, PLAYER_INVENTORY_SIZE};
use rspigot_domain::menu::InventoryMenu;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, ItemId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::EquipmentSlot as ItemSlot;

use crate::events::{ClientEvent, EntityStatus, GameEvent, MetadataEntry, MetadataValue};
use crate::player::Player;
use crate::plugins::events::{
    EntityExhaustionReason, EntityKnockbackCause, EntityRegainHealthRegainReason, PlayerDeath,
    PlayerDeathPatch, PlayerLevelChange, PlayerRespawn, PlayerRespawnPatch, PlayerRespawnReason,
};
use crate::plugins::{
    AfterHurt, Continuation, DeathStep, EventKind, EventPayload, Holder, Phase, RespawnStep,
    Struck, spec_of, stack_of,
};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

pub const HEAD_SLOT: usize = 5;
pub const CHEST_SLOT: usize = 6;
pub const LEGS_SLOT: usize = 7;
pub const FEET_SLOT: usize = 8;
pub const OFFHAND_SLOT: usize = 45;
const DAMAGE_COOLDOWN: u32 = 20;
const COOLDOWN_THRESHOLD: u32 = 10;
const HURT_DURATION: u32 = 10;
const KILL_CREDIT_TICKS: u32 = 100;
const DEATH_ANIMATION_TICKS: u32 = 20;
const CLIENT_LOAD_TIMEOUT: u32 = 60;
const VOID_DAMAGE: f32 = 4.0;
const VOID_DEPTH: i32 = 64;
const HELMET_FACTOR: f32 = 0.75;
const PEACEFUL_HEAL_INTERVAL: u32 = 20;
const PEACEFUL_FOOD_INTERVAL: u32 = 10;
const DROP_PICKUP_DELAY: u16 = 40;
const DROP_EYE_OFFSET: f64 = 0.3;
const DROP_SPEED: f32 = 0.5;
const DROP_LIFT: f64 = 0.2;
const HEALTH_DATA_INDEX: u8 = 9;
const ABSORPTION_DATA_INDEX: u8 = 17;
const ABSORPTION_PER_LEVEL: f32 = 4.0;

#[derive(Debug, Clone, PartialEq)]
pub struct SourceEntity {
    pub entity: EntityId,
    pub player: Option<PlayerId>,
    pub kind: Kind,
    pub creative: bool,
    pub name: Component,
    pub position: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DamageSource {
    pub kind: DamageType,
    pub causing: Option<SourceEntity>,
    pub direct: Option<SourceEntity>,
    pub position: Option<Vec3>,
    pub knockback: Option<(f64, f64)>,
    pub direct_block: Option<(BlockPos, BlockStateId)>,
}

impl DamageSource {
    pub fn new(kind: DamageType) -> Self {
        Self {
            kind,
            causing: None,
            direct: None,
            position: None,
            knockback: None,
            direct_block: None,
        }
    }

    pub fn by(kind: DamageType, source: SourceEntity) -> Self {
        Self {
            kind,
            causing: Some(source.clone()),
            direct: Some(source),
            position: None,
            knockback: None,
            direct_block: None,
        }
    }

    pub fn from_block(kind: DamageType, block: Option<(BlockPos, BlockStateId)>) -> Self {
        Self {
            direct_block: block,
            ..Self::new(kind)
        }
    }

    pub fn attacker(&self) -> Option<PlayerId> {
        self.causing.as_ref().and_then(|c| c.player)
    }

    pub fn living_cause(&self) -> bool {
        self.causing.as_ref().is_some_and(|c| c.kind.living())
    }

    pub fn scales_with_difficulty(&self) -> bool {
        self.kind.scales_always()
            || self
                .causing
                .as_ref()
                .is_some_and(|c| c.kind.living() && c.kind != Kind::Player)
    }

    pub fn creative_attacker(&self) -> bool {
        self.causing.as_ref().is_some_and(|c| c.creative)
    }

    pub(crate) fn source_position(&self) -> Option<Vec3> {
        self.position
            .or_else(|| self.direct.as_ref().map(|d| d.position))
    }

    pub(crate) fn knock_cause(&self) -> EntityKnockbackCause {
        if self.direct.is_some() {
            EntityKnockbackCause::EntityAttack
        } else {
            EntityKnockbackCause::Damage
        }
    }

    pub(crate) fn knockback_from(&self, victim: Vec3) -> (f64, f64) {
        if let Some(direction) = self.knockback {
            return direction;
        }
        self.source_position()
            .map_or((0.0, 0.0), |s| (s.x - victim.x, s.z - victim.z))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct KillCredit {
    pub player: PlayerId,
    pub name: Component,
    pub memory: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SentVitals {
    health: f32,
    food: i32,
    saturation_zero: bool,
    absorption: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Vitals {
    pub health: f32,
    pub absorption: f32,
    pub food: FoodData,
    pub fall_distance: f64,
    pub damage_cooldown: u32,
    pub last_hurt: f32,
    pub hurt_time: u32,
    pub dead: bool,
    pub dying: bool,
    pub death_time: u32,
    pub removed: bool,
    pub combat: CombatTracker,
    pub kill_credit: Option<KillCredit>,
    pub attack_ticker: u32,
    pub held_item: Option<ItemId>,
    pub velocity: Vec3,
    pub sync_velocity: bool,
    pub tick_count: u32,
    pub waiting_for_respawn: bool,
    pub load_timeout: u32,
    pub death_location: Option<BlockPos>,
    pub hurt_dir: f32,
    pub equipment: Vec<(ItemSlot, ItemId)>,
    pub experience: PlayerExperience,
    pub last_hurt_mob: Option<EntityId>,
    pub last_hurt_mob_at: i32,
    pub last_hurt_by_mob: Option<EntityId>,
    pub last_hurt_by_mob_at: i32,
    pub last_source: Option<(DamageType, i64)>,
    pub fire_ticks: i32,
    pub arrow_count: i32,
    pub remove_arrow_time: i32,
    pub death_experience: Option<DeathExperience>,
    pub warden_tracker: rspigot_domain::warden::WardenSpawnTracker,
    sent: Option<SentVitals>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeathExperience {
    pub keep_level: bool,
    pub level: i32,
    pub total: i32,
    pub points: i32,
}

impl Default for Vitals {
    fn default() -> Self {
        Self {
            health: 20.0,
            absorption: 0.0,
            food: FoodData::default(),
            fall_distance: 0.0,
            damage_cooldown: 0,
            last_hurt: 0.0,
            hurt_time: 0,
            dead: false,
            dying: false,
            death_time: 0,
            removed: false,
            combat: CombatTracker::default(),
            kill_credit: None,
            attack_ticker: 0,
            held_item: None,
            velocity: Vec3::ZERO,
            sync_velocity: false,
            tick_count: 0,
            waiting_for_respawn: false,
            load_timeout: CLIENT_LOAD_TIMEOUT,
            death_location: None,
            hurt_dir: 0.0,
            equipment: Vec::new(),
            experience: PlayerExperience::default(),
            last_hurt_mob: None,
            last_hurt_mob_at: 0,
            last_hurt_by_mob: None,
            last_hurt_by_mob_at: 0,
            last_source: None,
            fire_ticks: 0,
            arrow_count: 0,
            remove_arrow_time: 0,
            death_experience: None,
            warden_tracker: rspigot_domain::warden::WardenSpawnTracker::default(),
            sent: None,
        }
    }
}

impl Vitals {
    pub fn is_dead(&self) -> bool {
        self.dead || self.dying || self.health <= 0.0
    }

    pub fn client_loaded(&self) -> bool {
        !self.waiting_for_respawn && self.load_timeout == 0
    }

    pub(crate) fn mark_sent(&mut self) {
        self.sent = Some(self.snapshot());
        self.experience.last_sent = self.experience.total;
    }

    fn snapshot(&self) -> SentVitals {
        SentVitals {
            health: self.health,
            food: self.food.food,
            saturation_zero: self.food.saturation == 0.0,
            absorption: self.absorption,
        }
    }
}

pub(crate) fn equipment(inventory: &Inventory) -> Vec<(ItemSlot, ItemId)> {
    let main = HOTBAR_START + usize::from(inventory.selected_hotbar);
    [
        (ItemSlot::Mainhand, main),
        (ItemSlot::Offhand, OFFHAND_SLOT),
        (ItemSlot::Head, HEAD_SLOT),
        (ItemSlot::Chest, CHEST_SLOT),
        (ItemSlot::Legs, LEGS_SLOT),
        (ItemSlot::Feet, FEET_SLOT),
    ]
    .into_iter()
    .filter_map(|(slot, index)| inventory.get(index).map(|stack| (slot, stack.item)))
    .collect()
}

pub(crate) fn attributes<L>(player: &Player<L>) -> Attributes {
    Attributes::of(&player.effects, &equipment(&player.inventory))
        .with_base(MOVEMENT_SPEED, f64::from(player.walk_speed))
}

pub(crate) fn max_health<L>(player: &Player<L>) -> f32 {
    attributes(player).get(MAX_HEALTH) as f32
}

fn effect(name: &str) -> Option<EffectId> {
    EffectId::from_name(name)
}

fn amplifier<L>(player: &Player<L>, name: &str) -> Option<u8> {
    effect(name).and_then(|id| player.effects.get(id).map(|e| e.amplifier))
}

fn invulnerable_to<L>(player: &Player<L>, kind: DamageType, fall_damage: bool) -> bool {
    (kind.is(DamageTag::IsFall) && !fall_damage) || !player.vitals.client_loaded()
}

fn absorb_hurt<L>(
    player: &mut Player<L>,
    source: &DamageSource,
    amount: f32,
    attributes: &Attributes,
) -> f32 {
    let kind = source.kind;
    let mut amount = amount;
    if !kind.is(DamageTag::BypassesArmor) {
        amount = damage::after_armor(
            amount,
            attributes.get(ARMOR).floor() as f32,
            attributes.get(ARMOR_TOUGHNESS) as f32,
        );
    }
    if !kind.is(DamageTag::BypassesEffects) {
        if let Some(level) = amplifier(player, "minecraft:resistance")
            && !kind.is(DamageTag::BypassesResistance)
        {
            amount = damage::after_resistance(amount, level);
        }
        if amount <= 0.0 {
            amount = 0.0;
        }
    }
    let vitals = &mut player.vitals;
    let absorbed = damage::absorb(amount, vitals.absorption);
    vitals.absorption = java_clamp(
        absorbed.absorption,
        0.0,
        attributes.get(MAX_ABSORPTION) as f32,
    );
    absorbed.damage
}

fn commit_hurt<L>(
    player: &mut Player<L>,
    source: &DamageSource,
    amount: f32,
    attributes: &Attributes,
    now: u64,
) {
    let kind = source.kind;
    let vitals = &mut player.vitals;
    let alive = vitals.health > 0.0;
    vitals.combat.record(
        CombatEntry {
            kind,
            causing: source.causing.as_ref().map(|c| c.name.clone()),
            direct: source.direct.as_ref().map(|d| d.name.clone()),
            living_cause: source.living_cause(),
            damage: amount,
            fall_distance: vitals.fall_distance as f32,
            location: None,
        },
        now,
        alive,
    );
    vitals.health = java_clamp(
        vitals.health - amount,
        0.0,
        attributes.get(MAX_HEALTH) as f32,
    );
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn source_of(&self, id: PlayerId) -> Option<SourceEntity> {
        self.players.get(&id).map(|p| SourceEntity {
            entity: p.entity_id,
            player: Some(id),
            kind: Kind::Player,
            creative: p.game_mode == GameMode::Creative,
            name: Component::text(p.name()),
            position: p.position,
        })
    }

    pub(crate) fn hurt(&mut self, id: PlayerId, source: &DamageSource, amount: f32) -> bool {
        self.strike(None, Struck::Player(id), source, amount, AfterHurt::Nothing)
    }

    pub(crate) fn ask_hurt_player(
        &self,
        id: PlayerId,
        source: &DamageSource,
        amount: f32,
    ) -> Option<f32> {
        let kind = source.kind;
        let player = self.players.get(&id)?;
        if invulnerable_to(player, kind, self.config.rules.fall_damage)
            || (source.attacker().is_some() && !self.config.pvp)
            || (player.abilities().invulnerable && !kind.is(DamageTag::BypassesInvulnerability))
            || player.vitals.is_dead()
        {
            return None;
        }
        let scaled = source.scales_with_difficulty();
        if scaled && self.world.difficulty == Difficulty::Peaceful {
            return None;
        }
        let amount = if scaled {
            damage::scaled_for_difficulty(amount, self.world.difficulty)
        } else {
            amount
        };
        if kind.is(DamageTag::IsFire) && player.effects.has("minecraft:fire_resistance") {
            return None;
        }
        Some(if amount < 0.0 { 0.0 } else { amount })
    }

    pub(crate) fn player_final_damage(&self, id: PlayerId, kind: DamageType, amount: f32) -> f32 {
        let Some(player) = self.players.get(&id) else {
            return 0.0;
        };
        let mut amount = if amount < 0.0 { 0.0 } else { amount };
        if kind.is(DamageTag::DamagesHelmet) && player.inventory.get(HEAD_SLOT).is_some() {
            amount *= HELMET_FACTOR;
        }
        let attributes = attributes(player);
        if !kind.is(DamageTag::BypassesArmor) {
            amount = damage::after_armor(
                amount,
                attributes.get(ARMOR).floor() as f32,
                attributes.get(ARMOR_TOUGHNESS) as f32,
            );
        }
        if !kind.is(DamageTag::BypassesEffects) {
            if let Some(level) = amplifier(player, "minecraft:resistance")
                && !kind.is(DamageTag::BypassesResistance)
            {
                amount = damage::after_resistance(amount, level);
            }
            if amount <= 0.0 {
                amount = 0.0;
            }
        }
        damage::absorb(amount, player.vitals.absorption).damage
    }

    pub(crate) fn apply_hurt_player(
        &mut self,
        id: PlayerId,
        source: &DamageSource,
        amount: f32,
    ) -> bool {
        let fall_damage = self.config.rules.fall_damage;
        let now = self.current_tick();
        let kind = source.kind;
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        if invulnerable_to(player, kind, fall_damage)
            || (player.abilities().invulnerable && !kind.is(DamageTag::BypassesInvulnerability))
            || player.vitals.is_dead()
            || amount == 0.0
        {
            return false;
        }
        let mut amount = amount;
        if kind.is(DamageTag::DamagesHelmet) && player.inventory.get(HEAD_SLOT).is_some() {
            amount *= HELMET_FACTOR;
        }
        if !amount.is_finite() {
            amount = f32::MAX;
        }
        let attributes = attributes(player);
        let (full, dealt) = if player.vitals.damage_cooldown > COOLDOWN_THRESHOLD
            && !kind.is(DamageTag::BypassesCooldown)
        {
            if amount <= player.vitals.last_hurt {
                return false;
            }
            let extra = amount - player.vitals.last_hurt;
            let dealt = absorb_hurt(player, source, extra, &attributes);
            player.vitals.last_hurt = amount;
            (false, dealt)
        } else {
            player.vitals.last_hurt = amount;
            player.vitals.damage_cooldown = DAMAGE_COOLDOWN;
            let dealt = absorb_hurt(player, source, amount, &attributes);
            player.vitals.hurt_time = HURT_DURATION;
            (true, dealt)
        };
        if dealt != 0.0 {
            self.exhaust(id, kind.exhaustion(), EntityExhaustionReason::Damaged);
            if let Some(player) = self.players.get_mut(&id) {
                commit_hurt(player, source, dealt, &attributes, now);
            }
        }
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        if let Some(cause) = &source.causing
            && let Some(attacker) = cause.player
        {
            player.vitals.kill_credit = Some(KillCredit {
                player: attacker,
                name: cause.name.clone(),
                memory: KILL_CREDIT_TICKS,
            });
        }
        if let Some(cause) = &source.causing
            && cause.kind.living()
            && !kind.is(DamageTag::NoAnger)
        {
            let at = player.vitals.tick_count as i32;
            player.vitals.last_hurt_by_mob = Some(cause.entity);
            player.vitals.last_hurt_by_mob_at = at;
        }
        if dealt != 0.0 {
            player.vitals.last_source = Some((kind, now as i64));
        }
        if let Some(attacker) = source.attacker()
            && let Some(other) = self.players.get_mut(&attacker)
        {
            other.vitals.attack_ticker = 0;
        }
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let entity = player.entity_id;
        let position = player.position;
        let dead = player.vitals.health <= 0.0;
        if full {
            let event = ClientEvent::DamageEvent {
                entity,
                kind,
                cause: source.causing.as_ref().map(|c| c.entity),
                direct: source.direct.as_ref().map(|d| d.entity),
                position: source.position,
            };
            self.push(Audience::One(id), event.clone());
            self.push(Audience::Trackers(entity), event);
            if !kind.is(DamageTag::NoKnockback) {
                let (dx, dz) = source.knockback_from(position);
                self.knock(
                    Holder::Player(id),
                    crate::combat::DEFAULT_KNOCKBACK,
                    (dx, dz),
                    source.knock_cause(),
                    source.direct.as_ref().map(|direct| direct.entity),
                );
                self.indicate_damage(id, dx, dz);
            }
        }
        if dead {
            if full {
                self.play_player_sound(id, Kind::Player.sounds().map(|s| s.death));
            }
            self.die(id, source);
        } else if full {
            self.play_player_sound(id, Kind::Player.sounds().map(|s| s.hurt));
        }
        true
    }

    fn play_player_sound(&mut self, id: PlayerId, sound: Option<rspigot_entities::CreatureSound>) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let Some(sound) = sound.and_then(|s| s.get(rspigot_entities::SoundState::default())) else {
            return;
        };
        let position = player.position;
        let Some(sounds) = Kind::Player.sounds() else {
            return;
        };
        let pitch = self.voice_pitch(sounds.pitch, false);
        let volume = sounds.volume.get(false, 1);
        self.play_sound(Some(id), position, sound, sounds.source, volume, pitch);
    }

    pub(crate) fn kill(&mut self, id: PlayerId) -> bool {
        self.hurt(id, &DamageSource::new(DamageType::GenericKill), f32::MAX)
    }

    fn die(&mut self, id: PlayerId, source: &DamageSource) {
        let keep_rule = self.config.rules.keep_inventory;
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if player.vitals.dead || player.vitals.dying {
            return;
        }
        player.vitals.dying = true;
        let victim = Component::text(player.profile.name.clone());
        let credit = player.vitals.kill_credit.as_ref().map(|c| &c.name);
        let message = player.vitals.combat.death_message(&victim, credit);
        let keep = keep_rule || player.game_mode == GameMode::Spectator;
        let experience = if keep {
            0
        } else {
            player.vitals.experience.death_reward()
        };
        let inventory: Vec<(usize, ItemStack)> = (0..PLAYER_INVENTORY_SIZE)
            .filter_map(|slot| Some((slot, player.inventory.get(slot).cloned()?)))
            .collect();
        let text = crate::console::console_text(&message);
        let forgotten = self.decisions.forget(id);
        self.notify_undone(forgotten, Phase::Settled);
        let payload = {
            let text = text.clone();
            move |server: &Self| {
                let player = server.players.get(&id)?;
                let drops = if keep {
                    Vec::new()
                } else {
                    player.inventory.slots()[..PLAYER_INVENTORY_SIZE]
                        .iter()
                        .flatten()
                        .map(spec_of)
                        .collect()
                };
                Some(EventPayload::PlayerDeath(PlayerDeath {
                    damage_source: server.damage_view(source),
                    death_message: Some(text),
                    dropped_exp: experience,
                    drops,
                    entity: server.snapshot_of(id)?,
                    entity_type: Kind::Player.name().to_owned(),
                    keep_inventory: keep,
                    keep_level: keep,
                    new_exp: 0,
                    new_level: 0,
                    new_total_exp: 0,
                }))
            }
        };
        let step = DeathStep {
            player: id,
            message,
            text,
            show: self.config.rules.show_death_messages,
            keep,
            inventory,
            experience,
        };
        self.fire(id, Continuation::Death(Box::new(step)), payload);
    }

    pub(crate) fn finish_death(&mut self, step: DeathStep, patch: PlayerDeathPatch) {
        let id = step.player;
        let now = self.current_tick();
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let vitals = &mut player.vitals;
        vitals.dying = false;
        vitals.dead = true;
        vitals.death_time = 0;
        vitals.waiting_for_respawn = true;
        vitals.death_location = Some(player.position.block());
        vitals.combat.recheck(now, false);
        let entity = player.entity_id;
        let feet = player.position;
        let origin = Vec3::new(
            feet.x,
            feet.y + player.eye_height() - DROP_EYE_OFFSET,
            feet.z,
        );
        self.close_open_menu(id, true);
        let message = match patch
            .death_message
            .unwrap_or_else(|| Some(step.text.clone()))
        {
            Some(text) if text == step.text => Some(step.message),
            Some(text) if !text.is_empty() => Some(Component::from_section_codes(&text)),
            _ => None,
        };
        let spoken = message.filter(|_| step.show);
        self.push(
            Audience::One(id),
            ClientEvent::CombatKill {
                player: entity,
                message: spoken.clone().unwrap_or_else(|| Component::text("")),
            },
        );
        if let Some(message) = spoken {
            tracing::info!("{}", crate::console::console_text(&message));
            self.push(Audience::All, ClientEvent::system(message));
        }
        self.forgive_dead_player(id);
        let keep_inventory = patch.keep_inventory.unwrap_or(step.keep);
        let dropped: &[(usize, ItemStack)] = if step.keep { &[] } else { &step.inventory };
        let drops = patch.drops.apply_with(
            dropped
                .iter()
                .map(|(_, stack)| (stack.clone(), true))
                .collect(),
            |(_, from_inventory), spec| {
                stack_of(&spec)
                    .ok()
                    .flatten()
                    .map(|stack| (stack, from_inventory))
            },
            |spec| stack_of(&spec).ok().flatten().map(|stack| (stack, false)),
        );
        for (stack, from_inventory) in drops {
            if from_inventory {
                self.drop_randomly(origin, stack);
            } else {
                self.spawn_at_location(feet, 0.0, stack);
            }
        }
        let mut cleared = false;
        if let Some(player) = self.players.get_mut(&id) {
            if !keep_inventory {
                for (slot, stack) in &step.inventory {
                    if player.inventory.get(*slot) == Some(stack) {
                        player.inventory.set(*slot, None);
                        cleared = true;
                    }
                }
            }
            player.vitals.death_experience = Some(DeathExperience {
                keep_level: patch.keep_level.unwrap_or(step.keep),
                level: patch.new_level.unwrap_or(0),
                total: patch.new_total_exp.unwrap_or(0),
                points: patch.new_exp.unwrap_or(0),
            });
        }
        if cleared {
            self.send_inventory(id);
            self.broadcast_held_item(id);
        }
        self.award_experience(feet, patch.dropped_exp.unwrap_or(step.experience));
        let status = ClientEvent::EntityStatus {
            entity,
            status: EntityStatus::Death,
        };
        self.push(Audience::One(id), status.clone());
        self.push(Audience::Trackers(entity), status);
    }

    fn drop_randomly(&mut self, origin: Vec3, stack: ItemStack) {
        let speed = self.rng.random::<f32>() * DROP_SPEED;
        let angle = self.rng.random::<f32>() * std::f32::consts::TAU;
        let velocity = Vec3::new(
            f64::from(-angle.sin() * speed),
            DROP_LIFT,
            f64::from(angle.cos() * speed),
        );
        self.spawn_item_entity(origin, velocity, DROP_PICKUP_DELAY, stack);
    }

    pub(crate) fn respawn(&mut self, id: PlayerId, reason: PlayerRespawnReason) {
        if !self
            .players
            .get(&id)
            .is_some_and(|player| player.vitals.is_dead())
        {
            return;
        }
        let location = self.location_of(
            self.world.spawn,
            Rotation::new(self.world.level.spawn_angle, 0.0),
        );
        let step = RespawnStep { player: id };
        self.fire(id, Continuation::Respawn(step), move |server| {
            server.snapshot_of(id).map(|player| {
                EventPayload::PlayerRespawn(PlayerRespawn {
                    anchor_spawn: false,
                    bed_spawn: false,
                    player,
                    respawn_location: location,
                    respawn_reason: reason,
                })
            })
        });
    }

    pub(crate) fn finish_respawn(&mut self, step: RespawnStep, patch: PlayerRespawnPatch) {
        let id = step.player;
        let hardcore = self.config.hardcore;
        let keep_experience = self.config.rules.keep_inventory;
        let spawn = self.world.spawn;
        let angle = self.world.level.spawn_angle;
        let dimension = self.world.dimension.key.clone();
        let (position, rotation) = patch
            .respawn_location
            .map_or((spawn, Rotation::new(angle, 0.0)), |location| {
                (location.position, location.rotation)
            });
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if !player.vitals.dead || !player.link.is_connected() {
            return;
        }
        let death_location = player.vitals.death_location;
        let kept = player.vitals.experience;
        let heard_effects = self.listened(EventKind::EntityPotionEffect);
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let mut experience = PlayerExperience::default();
        let mut points = 0;
        match player.vitals.death_experience.take() {
            Some(stored) if !stored.keep_level => {
                experience.level = stored.level;
                experience.total = stored.total;
                points = stored.points;
            }
            Some(_) => {
                experience.level = kept.level;
                experience.progress = kept.progress;
                experience.total = kept.total;
            }
            None if keep_experience => {
                experience.level = kept.level;
                experience.progress = kept.progress;
                experience.total = kept.total;
            }
            None => {}
        }
        experience.give_points(points, 0);
        experience.last_sent = -1;
        player.vitals = Vitals {
            death_location,
            experience,
            warden_tracker: player.vitals.warden_tracker,
            ..Vitals::default()
        };
        let cleared: Vec<_> = if heard_effects {
            player.effects.iter().copied().collect()
        } else {
            let _ = player.effects.clear();
            Vec::new()
        };
        player.sneaking = false;
        player.sprinting = false;
        player.loaded = false;
        player.rotation = rotation;
        let entity = player.entity_id;
        for other in self.players.values_mut() {
            if other.tracked.remove(&entity) {
                other.link.send(ClientEvent::RemoveEntities(vec![entity]));
            }
        }
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let mut spawn_info = self.spawn_info(player);
        spawn_info.death_location = death_location.map(|pos| (dimension.clone(), pos));
        player.link.send(ClientEvent::Respawn {
            spawn: spawn_info,
            keep_data: 0,
        });
        self.teleport_now(id, position);
        let time = self.time_event();
        let difficulty = self.world.difficulty;
        let raining = self.world.level.raining;
        let thundering = self.world.level.thundering;
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let link = &player.link;
        link.send(ClientEvent::SpawnPosition {
            dimension,
            pos: spawn.block(),
            yaw: angle,
            pitch: 0.0,
        });
        link.send(ClientEvent::Difficulty {
            difficulty,
            locked: false,
        });
        let experience = player.vitals.experience;
        link.send(ClientEvent::Experience {
            progress: experience.progress,
            level: experience.level,
            total: experience.total,
        });
        link.send(ClientEvent::OperatorLevel {
            entity,
            level: if player.operator { 4 } else { 0 },
        });
        link.send(time);
        if raining {
            link.send(ClientEvent::GameEvent(GameEvent::StartRaining));
            link.send(ClientEvent::GameEvent(GameEvent::RainLevelChange(1.0)));
            link.send(ClientEvent::GameEvent(GameEvent::ThunderLevelChange(
                if thundering { 1.0 } else { 0.0 },
            )));
        }
        link.send(ClientEvent::GameEvent(GameEvent::LevelChunksLoadStart));
        link.send(ClientEvent::Abilities(player.abilities()));
        link.send(ClientEvent::HeldSlot(player.inventory.selected_hotbar));
        let state_id = player.next_inventory_state();
        player.link.send(ClientEvent::InventoryContent {
            state_id,
            slots: player.inventory.slots().to_vec(),
            carried: None,
        });
        player.menu = InventoryMenu::synced(&player.inventory);
        self.send_commands(id);
        self.clear_at_respawn(id, cleared);
        if hardcore {
            self.change_game_mode(id, GameMode::Spectator, Vec::new());
        }
    }

    pub(crate) fn tick_health(&mut self) {
        let now = self.current_tick();
        let floor = f64::from(self.world.dimension.height.min_y - VOID_DEPTH);
        let ids: Vec<PlayerId> = self.players.keys().copied().collect();
        for &id in &ids {
            let Some(player) = self.players.get(&id) else {
                continue;
            };
            let hurt_mob = player.vitals.last_hurt_mob;
            let hurt_by_mob = player.vitals.last_hurt_by_mob;
            let hurt_mob_gone = hurt_mob.is_some_and(|mob| !self.living_entity_alive(mob));
            let hurt_by_gone = hurt_by_mob.is_some_and(|mob| !self.living_entity_alive(mob));
            let Some(player) = self.players.get_mut(&id) else {
                continue;
            };
            let vitals = &mut player.vitals;
            vitals.tick_count = vitals.tick_count.wrapping_add(1);
            vitals.warden_tracker.tick();
            vitals.damage_cooldown = vitals.damage_cooldown.saturating_sub(1);
            if hurt_mob_gone {
                vitals.last_hurt_mob = None;
            }
            if hurt_by_gone {
                vitals.last_hurt_by_mob = None;
            }
            if vitals.last_hurt_by_mob.is_some()
                && vitals.tick_count as i32 - vitals.last_hurt_by_mob_at
                    > rspigot_domain::entity::living::MOB_HURT_MEMORY
            {
                vitals.last_hurt_by_mob = None;
            }
            vitals.load_timeout = if player.loaded {
                0
            } else {
                vitals.load_timeout.saturating_sub(1)
            };
            if vitals.is_dead() {
                vitals.death_time += 1;
                if vitals.death_time == DEATH_ANIMATION_TICKS && !vitals.removed {
                    vitals.removed = true;
                    let entity = player.entity_id;
                    self.remove_body(entity);
                }
                continue;
            }
            vitals.hurt_time = vitals.hurt_time.saturating_sub(1);
            if let Some(credit) = &mut vitals.kill_credit {
                credit.memory = credit.memory.saturating_sub(1);
                if credit.memory == 0 {
                    vitals.kill_credit = None;
                }
            }
            vitals.combat.recheck(now, true);
            if vitals.arrow_count > 0 {
                if vitals.remove_arrow_time <= 0 {
                    vitals.remove_arrow_time = 20 * (30 - vitals.arrow_count);
                }
                vitals.remove_arrow_time -= 1;
                if vitals.remove_arrow_time <= 0 {
                    vitals.arrow_count -= 1;
                    self.broadcast_metadata(id);
                }
            }
            self.tick_player_fire(id);
            if self.players.get(&id).is_some_and(|p| p.position.y < floor) {
                self.hurt(id, &DamageSource::new(DamageType::OutOfWorld), VOID_DAMAGE);
            }
        }
        self.tick_effects();
        let difficulty = self.world.difficulty;
        let regeneration = self.config.rules.natural_regeneration;
        let mut changed_equipment = Vec::new();
        for &id in &ids {
            let Some(player) = self.players.get_mut(&id) else {
                continue;
            };
            if player.vitals.is_dead() {
                continue;
            }
            let max = max_health(player);
            let resets_fall = player.abilities().flying
                || player.effects.has("minecraft:slow_falling")
                || player.effects.has("minecraft:levitation");
            let held = player.inventory.held().map(|s| s.item);
            let worn = equipment(&player.inventory);
            if worn != player.vitals.equipment {
                let names: Vec<&'static str> = worn
                    .iter()
                    .chain(player.vitals.equipment.iter())
                    .flat_map(|(slot, item)| {
                        equipment_modifiers(*slot, *item).map(|(name, _)| name)
                    })
                    .filter(|name| is_syncable(name))
                    .collect();
                player.vitals.equipment = worn;
                changed_equipment.push((id, names));
            }
            let vitals = &mut player.vitals;
            if resets_fall {
                vitals.fall_distance = 0.0;
            }
            let peaceful = difficulty == Difficulty::Peaceful && regeneration;
            let regen = peaceful
                && vitals.tick_count.is_multiple_of(PEACEFUL_HEAL_INTERVAL)
                && vitals.health < max;
            if regen {
                self.heal_player(id, 1.0, EntityRegainHealthRegainReason::Regen);
            }
            let Some(player) = self.players.get_mut(&id) else {
                continue;
            };
            let vitals = &mut player.vitals;
            if peaceful {
                if vitals.tick_count.is_multiple_of(PEACEFUL_HEAL_INTERVAL)
                    && vitals.food.saturation < rspigot_domain::food::MAX_SATURATION
                {
                    vitals.food.saturation += 1.0;
                }
                if vitals.tick_count.is_multiple_of(PEACEFUL_FOOD_INTERVAL)
                    && vitals.food.needs_food()
                {
                    vitals.food.food += 1;
                }
            }
            vitals.attack_ticker = vitals.attack_ticker.saturating_add(1);
            if held != vitals.held_item {
                vitals.attack_ticker = 0;
                vitals.held_item = held;
            }
            if let Some(level) = vitals.food.tick_exhaustion(difficulty) {
                self.change_food(id, level, None);
            }
            let Some(player) = self.players.get_mut(&id) else {
                continue;
            };
            let max = max_health(player);
            let vitals = &mut player.vitals;
            match vitals
                .food
                .tick_regeneration(difficulty, regeneration, vitals.health, max)
            {
                FoodEffect::None => {}
                FoodEffect::Heal { amount, exhaustion } => {
                    self.heal_player(id, amount, EntityRegainHealthRegainReason::Satiated);
                    self.exhaust(id, exhaustion, EntityExhaustionReason::Regen);
                }
                FoodEffect::Starve => {
                    self.hurt(id, &DamageSource::new(DamageType::Starve), 1.0);
                }
            }
        }
        for (id, names) in changed_equipment {
            self.sync_attributes(id, &names);
            self.clamp_vitals(id);
        }
    }

    fn remove_body(&mut self, entity: EntityId) {
        let poof = ClientEvent::EntityStatus {
            entity,
            status: EntityStatus::Poof,
        };
        for viewer in self.players.values_mut() {
            if viewer.tracked.remove(&entity) {
                viewer.link.send(poof.clone());
                viewer.link.send(ClientEvent::RemoveEntities(vec![entity]));
            }
        }
    }

    pub(crate) fn clamp_vitals(&mut self, id: PlayerId) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let attributes = attributes(player);
        let max_health = attributes.get(MAX_HEALTH) as f32;
        let max_absorption = attributes.get(MAX_ABSORPTION) as f32;
        let vitals = &mut player.vitals;
        if vitals.health > max_health {
            vitals.health = max_health;
        }
        if vitals.absorption > max_absorption {
            vitals.absorption = max_absorption;
        }
    }

    pub(crate) fn start_absorption(&mut self, id: PlayerId, amplifier: u8) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let max = attributes(player).get(MAX_ABSORPTION) as f32;
        let wanted = ABSORPTION_PER_LEVEL * (f32::from(amplifier) + 1.0);
        let vitals = &mut player.vitals;
        vitals.absorption = java_clamp(vitals.absorption.max(wanted), 0.0, max);
    }

    pub(crate) fn health_metadata(player: &Player<L>) -> Vec<MetadataEntry> {
        let mut entries = vec![MetadataEntry {
            index: HEALTH_DATA_INDEX,
            value: MetadataValue::Float(player.vitals.health),
        }];
        if player.vitals.absorption > 0.0 {
            entries.push(MetadataEntry {
                index: ABSORPTION_DATA_INDEX,
                value: MetadataValue::Float(player.vitals.absorption),
            });
        }
        entries
    }

    pub(crate) fn sync_vitals(&mut self) {
        let mut updates = Vec::new();
        let mut pushed = Vec::new();
        let mut levels = Vec::new();
        for player in self.players.values_mut() {
            let current = player.vitals.snapshot();
            let previous = player.vitals.sent;
            let entity = player.entity_id;
            let changed = previous.is_none_or(|sent| {
                sent.health != current.health
                    || sent.food != current.food
                    || sent.saturation_zero != current.saturation_zero
            });
            if changed {
                updates.push((
                    Audience::One(player.id),
                    ClientEvent::Health {
                        health: current.health,
                        food: current.food,
                        saturation: player.vitals.food.saturation,
                    },
                ));
            }
            let mut entries = Vec::new();
            if previous.is_none_or(|sent| sent.health != current.health) {
                entries.push(MetadataEntry {
                    index: HEALTH_DATA_INDEX,
                    value: MetadataValue::Float(current.health),
                });
            }
            if previous.is_some_and(|sent| sent.absorption != current.absorption) {
                entries.push(MetadataEntry {
                    index: ABSORPTION_DATA_INDEX,
                    value: MetadataValue::Float(current.absorption),
                });
            }
            if !entries.is_empty() {
                let event = ClientEvent::EntityMetadata { entity, entries };
                updates.push((Audience::One(player.id), event.clone()));
                updates.push((Audience::Trackers(entity), event));
            }
            if let Some((progress, level, total)) = player.vitals.experience.take_unsent() {
                updates.push((
                    Audience::One(player.id),
                    ClientEvent::Experience {
                        progress,
                        level,
                        total,
                    },
                ));
            }
            let experience = &mut player.vitals.experience;
            if experience.old_level == -1 {
                experience.old_level = experience.level;
            } else if experience.old_level != experience.level {
                levels.push((player.id, experience.old_level, experience.level));
                experience.old_level = experience.level;
            }
            if player.vitals.sync_velocity {
                pushed.push(player.id);
            }
            player.vitals.sent = Some(current);
        }
        for (audience, event) in updates {
            self.push(audience, event);
        }
        for id in pushed {
            self.sync_velocity(id);
        }
        for (id, old_level, new_level) in levels {
            self.fire(
                id,
                Continuation::Notice(EventKind::PlayerLevelChange),
                move |server| {
                    Some(EventPayload::PlayerLevelChange(PlayerLevelChange {
                        new_level,
                        old_level,
                        player: server.snapshot_of(id)?,
                    }))
                },
            );
        }
    }

    pub fn player_health(&self, id: PlayerId) -> Option<f32> {
        self.players.get(&id).map(|p| p.vitals.health)
    }

    pub fn player_last_hurt_mob(&self, id: PlayerId) -> Option<(Option<EntityId>, i32)> {
        self.players
            .get(&id)
            .map(|p| (p.vitals.last_hurt_mob, p.vitals.last_hurt_mob_at))
    }

    pub fn player_last_hurt_by_mob(&self, id: PlayerId) -> Option<(Option<EntityId>, i32)> {
        self.players
            .get(&id)
            .map(|p| (p.vitals.last_hurt_by_mob, p.vitals.last_hurt_by_mob_at))
    }

    pub fn player_food(&self, id: PlayerId) -> Option<(i32, f32, f32)> {
        self.players.get(&id).map(|p| {
            (
                p.vitals.food.food,
                p.vitals.food.saturation,
                p.vitals.food.exhaustion,
            )
        })
    }

    pub fn player_absorption(&self, id: PlayerId) -> Option<f32> {
        self.players.get(&id).map(|p| p.vitals.absorption)
    }

    pub fn player_location(&self, id: PlayerId) -> Option<Vec3> {
        self.players.get(&id).map(|p| p.position)
    }

    pub fn player_is_dead(&self, id: PlayerId) -> Option<bool> {
        self.players.get(&id).map(|p| p.vitals.is_dead())
    }
}
