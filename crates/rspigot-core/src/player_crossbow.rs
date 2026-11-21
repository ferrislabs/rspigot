use std::num::NonZeroU8;

use rand::Rng;
use rspigot_domain::crossbow;
use rspigot_domain::entity::arrow::{self, ArrowState, Launch, Pickup};
use rspigot_domain::entity::firework::{self, FireworkState};
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::item::{HOTBAR_START, ItemStack, OFFHAND_SLOT, menu_slot};
use rspigot_domain::mth;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::random::RandomSource;
use rspigot_domain::{ItemId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::ItemKind;

use crate::advancements::AdvancementEvent;
use crate::bow::{UsingItem, hand_slot};
use crate::enchant_effects::{self, EquipSlot, Worn};
use crate::events::Hand;
use crate::loot::LootContext;
use crate::mobs::ServerRandom;
use crate::player::Player;
use crate::plugins::events::CreatureSpawnReason;
use crate::plugins::steps::Birth;
use crate::ports::ClientLink;
use crate::projectiles::EYE_OFFSET;
use crate::server::Server;

const INTANGIBLE_PROJECTILE: &str = "minecraft:intangible_projectile";
const STORAGE_SLOTS: usize = 36;
const UNCERTAINTY: f32 = 1.0;
const SHOT_ADVANCEMENT: &str = "minecraft:shot_crossbow";
const SHOOT_SPREAD: f64 = 0.017_227_5;
const DEGREES_TO_RADIANS: f32 = 0.017_453_292;

struct Ammo {
    slot: Option<usize>,
    stack: ItemStack,
}

struct Gun {
    owner: uuid::Uuid,
    eye: Vec3,
    pitch: f32,
    yaw: f32,
    movement: Vec3,
    on_ground: bool,
    creative: bool,
}

fn is_crossbow(item: ItemId) -> bool {
    item == ItemKind::Crossbow.id()
}

fn can_use<L>(player: &Player<L>) -> bool {
    !player.vitals.is_dead() && player.game_mode != GameMode::Spectator
}

fn find_ammo<L>(player: &Player<L>) -> Option<Ammo> {
    let held = [
        OFFHAND_SLOT,
        HOTBAR_START + usize::from(player.inventory.selected_hotbar),
    ];
    let found = held
        .into_iter()
        .chain((0..STORAGE_SLOTS).filter_map(menu_slot))
        .find(|slot| {
            player
                .inventory
                .get(*slot)
                .is_some_and(crossbow::is_arrow_or_firework)
        });
    match found {
        Some(slot) => player.inventory.get(slot).map(|stack| Ammo {
            slot: Some(slot),
            stack: stack.clone(),
        }),
        None if player.game_mode == GameMode::Creative => Some(Ammo {
            slot: None,
            stack: arrow::default_item(),
        }),
        None => None,
    }
}

fn intangible(stack: &ItemStack) -> ItemStack {
    let mut copy = ItemStack {
        count: NonZeroU8::MIN,
        ..stack.clone()
    };
    copy.components
        .insert(INTANGIBLE_PROJECTILE, rspigot_nbt::Compound::new());
    copy
}

fn crossing(elapsed: i32, charge: i32, threshold: f32) -> bool {
    crossbow::charge_progress(elapsed, charge) >= threshold
        && (elapsed == 0 || crossbow::charge_progress(elapsed - 1, charge) < threshold)
}

fn direction(pitch: f32, yaw: f32) -> Vec3 {
    let radians = |angle: f32| f64::from(angle * DEGREES_TO_RADIANS);
    Vec3::new(
        f64::from(-mth::sin(radians(yaw)) * mth::cos(radians(pitch))),
        f64::from(-mth::sin(radians(pitch))),
        f64::from(mth::cos(radians(yaw)) * mth::cos(radians(pitch))),
    )
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn use_crossbow(&mut self, id: PlayerId, hand: Hand) -> bool {
        let tick = self.current_tick();
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let Some(weapon) = player
            .inventory
            .get(hand_slot(player, hand))
            .filter(|stack| is_crossbow(stack.item))
            .cloned()
        else {
            return false;
        };
        if crossbow::is_charged(&weapon) {
            self.shoot_charged(id, hand);
            return true;
        }
        let Some(player) = self.players.get_mut(&id) else {
            return true;
        };
        if find_ammo(player).is_some() {
            player.using_item = Some(UsingItem { hand, since: tick });
            self.show_use_status(id);
        }
        true
    }

    pub(crate) fn tick_player_crossbow(&mut self, id: PlayerId, using: UsingItem, item: ItemId) {
        if !is_crossbow(item) {
            return;
        }
        let tick = self.current_tick();
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let Some(weapon) = player.inventory.get(hand_slot(player, using.hand)) else {
            return;
        };
        let charge = crossbow::charge_ticks(weapon);
        let sounds = crossbow::charging_sounds(weapon);
        let elapsed = i32::try_from(tick.saturating_sub(using.since)).unwrap_or(i32::MAX);
        let position = player.position;
        if crossing(elapsed, charge, crossbow::START_SOUND_PROGRESS) {
            self.play_sound(
                None,
                position,
                sounds.start,
                SoundSource::Players,
                crossbow::CHARGING_SOUND_VOLUME,
                1.0,
            );
        }
        if crossing(elapsed, charge, crossbow::MID_SOUND_PROGRESS) {
            self.play_sound(
                None,
                position,
                sounds.mid,
                SoundSource::Players,
                crossbow::CHARGING_SOUND_VOLUME,
                1.0,
            );
        }
    }

    pub(crate) fn release_player_crossbow(&mut self, id: PlayerId) -> bool {
        let tick = self.current_tick();
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let Some(using) = player.using_item else {
            return false;
        };
        let Some(weapon) = player
            .inventory
            .get(hand_slot(player, using.hand))
            .filter(|stack| is_crossbow(stack.item))
            .cloned()
        else {
            return false;
        };
        let usable = can_use(player);
        let position = player.position;
        self.stop_using_now(id);
        let elapsed = i32::try_from(tick.saturating_sub(using.since)).unwrap_or(i32::MAX);
        let ready = crossbow::charge_progress(elapsed, crossbow::charge_ticks(&weapon)) >= 1.0;
        if usable && ready && !crossbow::is_charged(&weapon) && self.load_crossbow(id, using.hand) {
            let pitch = crossbow::charging_sound_pitch(&mut ServerRandom(&mut self.rng));
            let sounds = crossbow::charging_sounds(&weapon);
            self.play_sound(None, position, sounds.end, SoundSource::Players, 1.0, pitch);
        }
        self.refresh_hands(id);
        true
    }

    fn load_crossbow(&mut self, id: PlayerId, hand: Hand) -> bool {
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let slot = hand_slot(player, hand);
        let Some(mut weapon) = player.inventory.get(slot).cloned() else {
            return false;
        };
        let Some(ammo) = find_ammo(player) else {
            return false;
        };
        let creative = player.game_mode == GameMode::Creative;
        let worn = [Worn {
            stack: &weapon,
            slot: EquipSlot::Mainhand,
        }];
        let first_cost = if creative {
            0
        } else {
            enchant_effects::modify_ammo_use(
                &worn,
                &ammo.stack,
                1.0,
                &LootContext::default(),
                &mut self.rng,
            )
            .max(0.0) as u32
        };
        let plan = crossbow::draw_plan(
            crossbow::projectile_count(&weapon),
            first_cost,
            u32::from(ammo.stack.count.get()),
        );
        if plan.is_empty() {
            return false;
        }
        let loaded: Vec<ItemStack> = plan
            .iter()
            .map(|entry| {
                if entry.intangible {
                    intangible(&ammo.stack)
                } else {
                    ItemStack {
                        count: NonZeroU8::new(entry.cost.min(u32::from(u8::MAX)) as u8)
                            .unwrap_or(NonZeroU8::MIN),
                        ..ammo.stack.clone()
                    }
                }
            })
            .collect();
        crossbow::set_charged(&mut weapon, &loaded);
        let spent = crossbow::spent(&plan);
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        if let Some(ammo_slot) = ammo.slot
            && spent > 0
        {
            player.inventory.remove(ammo_slot, spent);
        }
        player.inventory.set(slot, Some(weapon));
        true
    }

    fn shoot_charged(&mut self, id: PlayerId, hand: Hand) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if !can_use(player) {
            return;
        }
        let slot = hand_slot(player, hand);
        let Some(mut weapon) = player.inventory.get(slot).cloned() else {
            return;
        };
        let projectiles = crossbow::charged_projectiles(&weapon);
        crossbow::set_charged(&mut weapon, &[]);
        player.inventory.set(slot, Some(weapon.clone()));
        let gun = Gun {
            owner: player.profile.id.0,
            eye: player.eye_position(),
            pitch: player.rotation.pitch,
            yaw: player.rotation.yaw,
            movement: player.known_movement,
            on_ground: player.on_ground,
            creative: player.game_mode == GameMode::Creative,
        };
        let position = player.position;
        let power = crossbow::shooting_power(&projectiles);
        let angles = crossbow::shot_angles(projectiles.len(), crossbow::projectile_spread(&weapon));
        let pierce = crossbow::piercing_level(&weapon);
        for (index, (stack, angle)) in projectiles.iter().zip(angles).enumerate() {
            if crossbow::is_firework(stack) {
                self.shoot_firework(id, &gun, stack, angle, power);
            } else {
                self.shoot_arrow(&gun, stack, &weapon, angle, power, pierce);
            }
            let pitch = crossbow::shot_pitch(&mut ServerRandom(&mut self.rng), index);
            self.play_sound(
                None,
                position,
                crossbow::SHOOT_SOUND,
                SoundSource::Players,
                1.0,
                pitch,
            );
            self.wear(id, hand, crossbow::durability_use(stack));
            let broken = self
                .players
                .get(&id)
                .is_none_or(|player| player.inventory.get(slot).is_none());
            if broken {
                break;
            }
        }
        let mut event = AdvancementEvent::named(SHOT_ADVANCEMENT);
        event.item = Some(ItemKind::Crossbow.id().0);
        self.trigger_event(id, event);
        self.refresh_hands(id);
    }

    fn shoot_arrow(
        &mut self,
        gun: &Gun,
        stack: &ItemStack,
        weapon: &ItemStack,
        angle: f32,
        power: f32,
        pierce: i32,
    ) {
        let mut base = self.new_entity_base(Kind::Arrow);
        base.position = Vec3::new(gun.eye.x, gun.eye.y - EYE_OFFSET, gun.eye.z);
        base.old_position = base.position;
        let seed = self.rng.random::<i64>();
        let mut state = ArrowState::new(&mut base, seed);
        state.set_owner(Some(gun.owner), true);
        state.item = ItemStack {
            count: NonZeroU8::MIN,
            ..stack.clone()
        };
        state.weapon = Some(weapon.clone());
        state.sound = crossbow::HIT_SOUND.to_owned();
        state.pickup = if gun.creative || stack.components.contains_key(INTANGIBLE_PROJECTILE) {
            Pickup::CreativeOnly
        } else {
            Pickup::Allowed
        };
        arrow::set_crit(&mut base, true);
        if pierce > 0 {
            arrow::set_pierce_level(&mut base, pierce.min(i32::from(i8::MAX)) as i8);
        }
        arrow::shoot_from_rotation(
            &mut base,
            &mut state,
            Launch {
                pitch: gun.pitch,
                yaw: gun.yaw + angle,
                roll: 0.0,
                power,
                uncertainty: UNCERTAINTY,
                shooter_movement: gun.movement,
                shooter_on_ground: gun.on_ground,
            },
        );
        let arrow = Entity::new(base, EntityState::Arrow(Box::new(state)));
        self.add_entity(arrow, CreatureSpawnReason::Default);
    }

    fn shoot_firework(
        &mut self,
        id: PlayerId,
        gun: &Gun,
        stack: &ItemStack,
        angle: f32,
        power: f32,
    ) {
        let mut base = self.new_entity_base(Kind::FireworkRocket);
        base.position = Vec3::new(
            gun.eye.x,
            gun.eye.y - crossbow::FIREWORK_EYE_OFFSET,
            gun.eye.z,
        );
        base.old_position = base.position;
        let seed = self.rng.random::<i64>();
        let rocket = ItemStack {
            count: NonZeroU8::MIN,
            ..stack.clone()
        };
        let mut state = FireworkState::new(&mut base, seed, &rocket);
        state.owner = Some(gun.owner);
        firework::set_shot_at_angle(&mut base, true);
        let aim = direction(gun.pitch, gun.yaw + angle);
        let spread = SHOOT_SPREAD * f64::from(UNCERTAINTY);
        let jitter = Vec3::new(
            state.random.triangle(0.0, spread),
            state.random.triangle(0.0, spread),
            state.random.triangle(0.0, spread),
        );
        let movement = (aim.normalize() + jitter).scale(f64::from(power));
        let carried = Vec3::new(
            gun.movement.x,
            if gun.on_ground { 0.0 } else { gun.movement.y },
            gun.movement.z,
        );
        base.velocity = movement + carried;
        let horizontal = (movement.x * movement.x + movement.z * movement.z).sqrt();
        base.set_yaw((mth::atan2(movement.x, movement.z) * mth::RAD_TO_DEG) as f32);
        base.set_pitch((mth::atan2(movement.y, horizontal) * mth::RAD_TO_DEG) as f32);
        base.needs_sync = true;
        let entity = Entity::new(base, EntityState::Firework(Box::new(state)));
        self.spawn_entity(entity, Birth::of(CreatureSpawnReason::Default).by(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_charging_sounds_play_once_when_their_threshold_is_crossed() {
        let hits: Vec<i32> = (0..30).filter(|e| crossing(*e, 25, 0.2)).collect();
        assert_eq!(hits, vec![5]);
        let hits: Vec<i32> = (0..30).filter(|e| crossing(*e, 25, 0.5)).collect();
        assert_eq!(hits, vec![13]);
    }

    #[test]
    fn a_free_charge_sounds_everything_on_the_first_tick() {
        assert!(crossing(0, 0, 0.2));
        assert!(crossing(0, 0, 0.5));
        assert!(!crossing(1, 0, 0.2));
    }
}
