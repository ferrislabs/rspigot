use rand::Rng;
use rspigot_domain::EntityId;
use rspigot_domain::player::PlayerId;
use rspigot_items::ItemKind;

use crate::events::{ClientEvent, Hand, SoundSource};
use crate::game_rule_packet::RULE_NAMES;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::sound::sound_range;

const GAMEMASTER: u8 = 2;

impl<L: ClientLink> Server<L> {
    pub(crate) fn send_game_rule_values(&mut self, id: PlayerId) {
        if !self
            .players
            .get(&id)
            .is_some_and(|player| player.permission_level() >= GAMEMASTER)
        {
            return;
        }
        let values = RULE_NAMES
            .iter()
            .filter_map(|name| Some((format!("minecraft:{name}"), self.game_rule_text(name)?)))
            .collect();
        self.push(Audience::One(id), ClientEvent::GameRuleValues(values));
    }

    pub(crate) fn use_written_book(&mut self, id: PlayerId, hand: Hand) -> bool {
        let is_written = self
            .held(id, hand)
            .is_some_and(|stack| stack.item.kind() == Some(ItemKind::WrittenBook));
        if is_written {
            self.push(Audience::One(id), ClientEvent::OpenBook(hand));
        }
        is_written
    }

    pub(crate) fn play_entity_sound(
        &mut self,
        entity: EntityId,
        sound: &'static str,
        source: SoundSource,
        volume: f32,
        pitch: f32,
    ) {
        let Some(found) = self.entities.store.get(entity) else {
            return;
        };
        if found.base.is_silent() {
            return;
        }
        let position = found.base.position;
        let seed = self.rng.random::<i64>();
        self.push(
            Audience::Nearby {
                position,
                range: sound_range(volume),
                except: None,
            },
            ClientEvent::EntitySound {
                sound,
                source,
                entity,
                volume,
                pitch,
                seed,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use rspigot_domain::Rotation;
    use rspigot_domain::item::ItemStack;
    use rspigot_items::ItemKind;

    use super::*;
    use crate::commands::CommandContext;
    use crate::end_tests::{Tap, arrive};
    use crate::events::PlayerInput;

    fn rig(level: u8) -> (Server<Tap>, PlayerId, Tap) {
        let (mut server, id, link) = arrive();
        if let Some(player) = server.players.get_mut(&id) {
            player.operator = level > 0;
            player.op_level = level;
        }
        server.deliver();
        link.take();
        (server, id, link)
    }

    fn give_main_hand(server: &mut Server<Tap>, id: PlayerId, kind: ItemKind) {
        let slot = server.hand_slot_of(id, Hand::Main).expect("slot");
        let player = server.players.get_mut(&id).expect("player");
        player
            .inventory
            .set(slot, Some(ItemStack::new(kind.id(), NonZeroU8::MIN)));
    }

    fn use_main_hand(server: &mut Server<Tap>, id: PlayerId) {
        server.handle_input(
            id,
            PlayerInput::UseItem {
                hand: Hand::Main,
                sequence: 1,
                rotation: Rotation::new(0.0, 0.0),
            },
        );
        server.deliver();
    }

    #[test]
    fn using_a_written_book_opens_it() {
        let (mut server, id, link) = rig(0);
        give_main_hand(&mut server, id, ItemKind::WrittenBook);
        use_main_hand(&mut server, id);
        assert!(link.take().contains(&ClientEvent::OpenBook(Hand::Main)));
    }

    #[test]
    fn a_writable_book_sends_no_open_book_packet() {
        let (mut server, id, link) = rig(0);
        give_main_hand(&mut server, id, ItemKind::WritableBook);
        use_main_hand(&mut server, id);
        assert!(
            !link
                .take()
                .iter()
                .any(|event| matches!(event, ClientEvent::OpenBook(_)))
        );
    }

    #[test]
    fn an_operator_requesting_the_rule_values_gets_every_rule() {
        let (mut server, id, link) = rig(2);
        server.handle_input(id, PlayerInput::RequestGameRuleValues);
        server.deliver();
        let values = link
            .take()
            .into_iter()
            .find_map(|event| match event {
                ClientEvent::GameRuleValues(values) => Some(values),
                _ => None,
            })
            .expect("rule values");
        assert_eq!(values.len(), RULE_NAMES.len());
        assert!(values.contains(&("minecraft:keep_inventory".to_owned(), "false".to_owned())));
        assert!(values.contains(&("minecraft:random_tick_speed".to_owned(), "3".to_owned())));
    }

    #[test]
    fn a_player_without_permission_gets_no_rule_values() {
        let (mut server, id, link) = rig(0);
        server.handle_input(id, PlayerInput::RequestGameRuleValues);
        server.deliver();
        assert!(
            !link
                .take()
                .iter()
                .any(|event| matches!(event, ClientEvent::GameRuleValues(_)))
        );
    }

    #[test]
    fn an_entity_sound_names_the_entity_and_reaches_nearby_players() {
        let (mut server, id, link) = rig(4);
        crate::commands::execute(
            &mut server,
            crate::commands::CommandSender::Player(id),
            "summon minecraft:sheep ~ ~ ~",
        );
        server.deliver();
        link.take();
        let sheep = match server
            .select_entities(crate::commands::CommandSender::Player(id), "@e[type=sheep]")
            .expect("selector")[0]
        {
            crate::commands::Target::Entity(found) => found,
            crate::commands::Target::Player(_) => panic!("a sheep"),
        };
        server.play_entity_sound(
            sheep,
            "minecraft:entity.sheep.shear",
            SoundSource::Players,
            1.0,
            1.0,
        );
        server.deliver();
        assert!(link.take().iter().any(|event| matches!(
            event,
            ClientEvent::EntitySound { entity, sound: "minecraft:entity.sheep.shear", .. } if *entity == sheep
        )));
    }
}
