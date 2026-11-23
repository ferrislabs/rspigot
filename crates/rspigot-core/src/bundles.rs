use rand::Rng;
use rspigot_domain::bundle;
use rspigot_domain::menu::BundleSound;
use rspigot_domain::player::PlayerId;
use rspigot_entities::SoundSource;

use crate::events::Hand;
use crate::ports::ClientLink;
use crate::server::Server;

const INSERT_SOUND: &str = "minecraft:item.bundle.insert";
const INSERT_FAIL_SOUND: &str = "minecraft:item.bundle.insert_fail";
const REMOVE_ONE_SOUND: &str = "minecraft:item.bundle.remove_one";
const DROP_CONTENTS_SOUND: &str = "minecraft:item.bundle.drop_contents";
const VOLUME: f32 = 0.8;
const PITCH_BASE: f32 = 0.8;
const PITCH_SPREAD: f32 = 0.4;

impl<L: ClientLink> Server<L> {
    pub(crate) fn play_bundle_sound(&mut self, id: PlayerId, sound: BundleSound) {
        let Some(position) = self.players.get(&id).map(|player| player.position) else {
            return;
        };
        let spread = self.rng.random::<f32>() * PITCH_SPREAD;
        let (name, volume, pitch) = match sound {
            BundleSound::Insert => (INSERT_SOUND, VOLUME, PITCH_BASE + spread),
            BundleSound::InsertFail => (INSERT_FAIL_SOUND, 1.0, 1.0),
            BundleSound::RemoveOne => (REMOVE_ONE_SOUND, VOLUME, PITCH_BASE + spread),
        };
        self.play_sound(None, position, name, SoundSource::Players, volume, pitch);
    }

    pub(crate) fn use_bundle(&mut self, id: PlayerId, hand: Hand) -> bool {
        let Some(mut stack) = self.held(id, hand) else {
            return false;
        };
        if !bundle::is_bundle(&stack) {
            return false;
        }
        let dropped = bundle::drop_contents(&mut stack);
        if dropped.is_empty() {
            return true;
        }
        let Some(position) = self.players.get(&id).map(|player| player.position) else {
            return true;
        };
        let spread = self.rng.random::<f32>() * PITCH_SPREAD;
        self.count_used(id, &stack);
        self.set_held(id, hand, Some(stack));
        self.play_sound(
            None,
            position,
            DROP_CONTENTS_SOUND,
            SoundSource::Players,
            VOLUME,
            PITCH_BASE + spread,
        );
        for item in dropped {
            self.drop_from_player(id, item, true, false);
        }
        self.refresh_hands(id);
        true
    }

    pub(crate) fn spill_destroyed_item(&mut self, id: rspigot_domain::EntityId) {
        let Some(entity) = self.entities.store.get(id) else {
            return;
        };
        let position = entity.base.position;
        let contents = rspigot_domain::entity::item::stack(&entity.base)
            .map(bundle::destroyed_contents)
            .unwrap_or_default();
        for stack in contents {
            let velocity = rspigot_domain::Vec3::new(
                self.rng.random::<f64>() * 0.2 - 0.1,
                0.2,
                self.rng.random::<f64>() * 0.2 - 0.1,
            );
            self.spawn_item_entity(position, velocity, crate::items::PICKUP_DELAY, stack);
        }
    }

    pub(crate) fn select_bundle_item(&mut self, id: PlayerId, slot: i32, index: i32) {
        let Ok(slot) = usize::try_from(slot) else {
            return;
        };
        let opened = self
            .players
            .get(&id)
            .and_then(|player| player.open_menu.as_ref())
            .map(|open| self.open_items(open));
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        match (opened, player.open_menu.as_mut()) {
            (Some(mut items), Some(open)) => {
                let before = items.clone();
                open.menu
                    .select_bundle_item(&mut items, &mut player.inventory, slot, index);
                self.store_open_items(id, &before, items);
            }
            _ => player
                .menu
                .select_bundle_item(&mut player.inventory, slot, index),
        }
    }
}
