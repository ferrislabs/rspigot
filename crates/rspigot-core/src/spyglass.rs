use rspigot_domain::entity::living::{LIVING_FLAG_OFFHAND, LIVING_FLAG_USING};
use rspigot_domain::entity::raycast::clip_box;
use rspigot_domain::mth;
use rspigot_domain::player::PlayerId;
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;

use crate::advancements::AdvancementEvent;
use crate::bow::{UsingItem, hand_slot};
use crate::events::{ClientEvent, Hand, MetadataEntry, MetadataValue};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const USE_SOUND: &str = "minecraft:item.spyglass.use";
const STOP_SOUND: &str = "minecraft:item.spyglass.stop_using";
const USE_DURATION: u64 = 1200;
const LOOK_RANGE: f64 = 100.0;
const LIVING_FLAGS: u8 = 8;
const USING_ITEM_TRIGGER: &str = "minecraft:using_item";

fn is_spyglass(item: rspigot_domain::ItemId) -> bool {
    item == ItemKind::Spyglass.id()
}

fn flags_of(hand: Hand) -> i8 {
    let offhand = if hand == Hand::Off {
        LIVING_FLAG_OFFHAND
    } else {
        0
    };
    (LIVING_FLAG_USING | offhand) as i8
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn begin_spyglass(&mut self, id: PlayerId, hand: Hand) -> bool {
        let tick = self.current_tick();
        let Some(player) = self.players.get_mut(&id) else {
            return false;
        };
        let held = player
            .inventory
            .get(hand_slot(player, hand))
            .is_some_and(|stack| is_spyglass(stack.item));
        if !held {
            return false;
        }
        player.using_item = Some(UsingItem { hand, since: tick });
        let position = player.position;
        self.play_sound(None, position, USE_SOUND, SoundSource::Players, 1.0, 1.0);
        self.show_use_status(id);
        true
    }

    pub(crate) fn show_use_status(&mut self, id: PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let Some(using) = player.using_item else {
            return;
        };
        self.send_use_flags(id, flags_of(using.hand));
    }

    pub(crate) fn hide_use_status(&mut self, id: PlayerId, was_spyglass: bool) {
        self.send_use_flags(id, 0);
        if was_spyglass && let Some(player) = self.players.get(&id) {
            let position = player.position;
            self.play_sound(None, position, STOP_SOUND, SoundSource::Players, 1.0, 1.0);
        }
    }

    pub(crate) fn used_item_is_spyglass(&self, id: PlayerId) -> bool {
        self.players.get(&id).is_some_and(|player| {
            player.using_item.is_some_and(|using| {
                player
                    .inventory
                    .get(hand_slot(player, using.hand))
                    .is_some_and(|stack| is_spyglass(stack.item))
            })
        })
    }

    fn send_use_flags(&mut self, id: PlayerId, flags: i8) {
        let Some(entity) = self.players.get(&id).map(|player| player.entity_id) else {
            return;
        };
        let event = ClientEvent::EntityMetadata {
            entity,
            entries: vec![MetadataEntry {
                index: LIVING_FLAGS,
                value: MetadataValue::Byte(flags),
            }],
        };
        self.push(Audience::One(id), event.clone());
        self.push(Audience::Trackers(entity), event);
    }

    pub(crate) fn looking_at(&self, id: PlayerId) -> Option<&'static str> {
        let player = self.players.get(&id)?;
        let from = player.eye_position();
        let look = mth::view_vector(player.rotation.pitch, player.rotation.yaw);
        let to = from + look.scale(LOOK_RANGE);
        self.entities
            .store
            .iter()
            .filter(|entity| entity.base.id != player.entity_id)
            .filter_map(|entity| {
                let hit = clip_box(&entity.base.bounding_box(), from, to)?;
                Some((hit.distance_squared(from), entity.base.kind))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, kind)| kind.name())
    }

    pub(crate) fn tick_use_effects(&mut self) {
        let tick = self.current_tick();
        let using: Vec<(PlayerId, UsingItem)> = self
            .players
            .iter()
            .filter_map(|(id, player)| Some((*id, player.using_item?)))
            .collect();
        for (id, using) in using {
            let Some(item) = self.players.get(&id).and_then(|player| {
                player
                    .inventory
                    .get(hand_slot(player, using.hand))
                    .map(|stack| stack.item)
            }) else {
                continue;
            };
            let spyglass = is_spyglass(item);
            let mut event = AdvancementEvent::named(USING_ITEM_TRIGGER);
            event.item = Some(item.0);
            if spyglass {
                event.looking_at = self.looking_at(id);
            }
            self.trigger_event(id, event);
            if spyglass && tick.saturating_sub(using.since) >= USE_DURATION {
                self.stop_using_now(id);
                continue;
            }
            self.tick_player_crossbow(id, using, item);
            self.tick_spear_use(id, using, item);
            self.tick_brush_use(id, using, item);
        }
    }

    pub(crate) fn stop_using_now(&mut self, id: PlayerId) {
        let spyglass = self.used_item_is_spyglass(id);
        let had = self
            .players
            .get_mut(&id)
            .is_some_and(|player| player.using_item.take().is_some());
        if had {
            self.hide_use_status(id, spyglass);
        }
    }

    pub(crate) fn release_status_item(&mut self, id: PlayerId) -> bool {
        let status = self.players.get(&id).is_some_and(|player| {
            player.using_item.is_some_and(|using| {
                player
                    .inventory
                    .get(hand_slot(player, using.hand))
                    .is_some_and(|stack| {
                        is_spyglass(stack.item)
                            || crate::spear::is_spear(stack.item)
                            || crate::brushing::is_brush(stack.item)
                    })
            })
        });
        if status {
            self.stop_using_now(id);
        }
        status
    }
}
