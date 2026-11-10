use std::num::NonZeroU8;

use rand::Rng;
use rspigot_domain::Vec3;
use rspigot_domain::entity::arrow::{self, ArrowState, Launch, Pickup};
use rspigot_domain::entity::{Entity, EntityState};
use rspigot_domain::item::{HOTBAR_START, ItemStack, OFFHAND_SLOT, menu_slot};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::ItemKind;

use crate::events::Hand;
use crate::player::Player;
use crate::plugins::steps::{Draw, Shooter};
use crate::ports::ClientLink;
use crate::projectiles::{EYE_OFFSET, is_arrow_item};
use crate::server::Server;

const MIN_POWER: f32 = 0.1;
const POWER_TO_SPEED: f32 = 3.0;
const INACCURACY: f32 = 1.0;
const SHOOT_SOUND: &str = "minecraft:entity.arrow.shoot";
const STORAGE_SLOTS: usize = 36;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsingItem {
    pub hand: Hand,
    pub since: u64,
}

pub(crate) fn power_for_time(ticks: u64) -> f32 {
    let seconds = ticks as f32 / 20.0;
    let power = (seconds * seconds + seconds * 2.0) / 3.0;
    power.min(1.0)
}

struct Ammo {
    slot: Option<usize>,
    stack: ItemStack,
}

struct Shot {
    weapon: ItemStack,
    hand: Hand,
    ammo: Ammo,
    power: f32,
    creative: bool,
    eye: Vec3,
    pitch: f32,
    yaw: f32,
    movement: Vec3,
    on_ground: bool,
    owner: uuid::Uuid,
}

pub(crate) fn hand_slot<L>(player: &Player<L>, hand: Hand) -> usize {
    match hand {
        Hand::Main => HOTBAR_START + usize::from(player.inventory.selected_hotbar),
        Hand::Off => OFFHAND_SLOT,
    }
}

fn find_ammo<L>(player: &Player<L>) -> Option<Ammo> {
    let held = [
        OFFHAND_SLOT,
        HOTBAR_START + usize::from(player.inventory.selected_hotbar),
    ];
    let found = held
        .into_iter()
        .chain((0..STORAGE_SLOTS).filter_map(menu_slot))
        .find(|slot| is_arrow_item(player.inventory.get(*slot)));
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

fn can_use_items<L>(player: &Player<L>) -> bool {
    !player.vitals.is_dead() && player.game_mode != GameMode::Spectator
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn start_using_item(&mut self, id: PlayerId, hand: Hand) {
        let tick = self.current_tick();
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if !can_use_items(player) || player.using_item.is_some() {
            return;
        }
        let is_bow = player
            .inventory
            .get(hand_slot(player, hand))
            .is_some_and(|stack| stack.item == ItemKind::Bow.id());
        if is_bow && find_ammo(player).is_some() {
            player.using_item = Some(UsingItem { hand, since: tick });
        } else if !is_bow {
            player.using_item = crate::eating::starts_eating(player, hand, tick);
        }
    }

    pub(crate) fn stop_using_main_hand(&mut self, id: PlayerId) {
        if let Some(player) = self.players.get_mut(&id)
            && player
                .using_item
                .is_some_and(|using| using.hand == Hand::Main)
        {
            player.using_item = None;
        }
    }

    pub(crate) fn release_use_item(&mut self, id: PlayerId) {
        if let Some(shot) = self.take_drawn_shot(id) {
            if let Some(player) = self.players.get_mut(&id)
                && !shot.creative
                && let Some(slot) = shot.ammo.slot
            {
                player.inventory.remove(slot, 1);
            }
            self.fire_arrow(id, &shot);
        }
        self.send_inventory(id);
    }

    fn take_drawn_shot(&mut self, id: PlayerId) -> Option<Shot> {
        let tick = self.current_tick();
        let player = self.players.get_mut(&id)?;
        let using = player.using_item.take()?;
        if !can_use_items(player) {
            return None;
        }
        let weapon = player
            .inventory
            .get(hand_slot(player, using.hand))
            .filter(|stack| stack.item == ItemKind::Bow.id())
            .cloned()?;
        let ammo = find_ammo(player)?;
        let power = power_for_time(tick.saturating_sub(using.since));
        if power < MIN_POWER {
            return None;
        }
        Some(Shot {
            weapon,
            hand: using.hand,
            ammo,
            power,
            creative: player.game_mode == GameMode::Creative,
            eye: player.eye_position(),
            pitch: player.rotation.pitch,
            yaw: player.rotation.yaw,
            movement: player.known_movement,
            on_ground: player.on_ground,
            owner: player.profile.id.0,
        })
    }

    fn fire_arrow(&mut self, id: PlayerId, shot: &Shot) {
        let mut base = self.new_entity_base(Kind::Arrow);
        base.position = Vec3::new(shot.eye.x, shot.eye.y - EYE_OFFSET, shot.eye.z);
        base.old_position = base.position;
        let seed = self.rng.random::<i64>();
        let mut state = ArrowState::new(&mut base, seed);
        state.set_owner(Some(shot.owner), true);
        state.item = ItemStack {
            count: NonZeroU8::MIN,
            ..shot.ammo.stack.clone()
        };
        state.weapon = Some(shot.weapon.clone());
        state.pickup = if shot.creative {
            Pickup::CreativeOnly
        } else {
            Pickup::Allowed
        };
        arrow::set_crit(&mut base, shot.power >= 1.0);
        arrow::shoot_from_rotation(
            &mut base,
            &mut state,
            Launch {
                pitch: shot.pitch,
                yaw: shot.yaw,
                roll: 0.0,
                power: shot.power * POWER_TO_SPEED,
                uncertainty: INACCURACY,
                shooter_movement: shot.movement,
                shooter_on_ground: shot.on_ground,
            },
        );
        let arrow = Entity::new(base, EntityState::Arrow(Box::new(state)));
        let ammo = ItemStack {
            count: NonZeroU8::MIN,
            ..shot.ammo.stack.clone()
        };
        let draw = Draw {
            weapon: Some(shot.weapon.clone()),
            ammo: Some(ammo),
            hand: shot.hand,
            force: shot.power * POWER_TO_SPEED,
        };
        self.shoot_bow(Shooter::Player(id), draw, arrow);
    }

    pub(crate) fn bow_sound(&mut self, id: PlayerId, power: f32) {
        let Some(position) = self.players.get(&id).map(|player| player.position) else {
            return;
        };
        let pitch = 1.0 / (self.rng.random::<f32>() * 0.4 + 1.2) + power / POWER_TO_SPEED * 0.5;
        self.play_sound(
            None,
            position,
            SHOOT_SOUND,
            SoundSource::Players,
            1.0,
            pitch,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_power_follows_the_vanilla_curve() {
        assert_eq!(power_for_time(0), 0.0);
        assert!(power_for_time(2) < MIN_POWER);
        assert!(power_for_time(3) >= MIN_POWER);
        assert!((power_for_time(10) - 5.0 / 12.0).abs() < 1e-6);
        assert_eq!(power_for_time(20), 1.0);
        assert_eq!(power_for_time(72_000), 1.0);
    }
}
