use rspigot_domain::item::ItemStack;
use rspigot_domain::player::PlayerId;
use rspigot_domain::stack_components;
use rspigot_items::{ItemId, ItemKind};

use crate::events::{ClientEvent, Hand};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

pub(crate) fn group_of(item: ItemId) -> Option<&'static str> {
    item.kind().map(ItemKind::name)
}

impl<L: ClientLink> Server<L> {
    pub fn start_cooldown(&mut self, id: PlayerId, item: ItemId, ticks: i32) {
        let Some(group) = group_of(item) else {
            return;
        };
        self.start_group_cooldown(id, group, ticks);
    }

    pub(crate) fn start_group_cooldown(&mut self, id: PlayerId, group: &str, ticks: i32) {
        let now = self.current_tick();
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if ticks <= 0 {
            player.cooldowns.remove(group);
        } else {
            player
                .cooldowns
                .insert(group.to_owned(), now + u64::from(ticks.unsigned_abs()));
        }
        self.push(
            Audience::One(id),
            ClientEvent::Cooldown {
                group: group.to_owned(),
                ticks: ticks.max(0),
            },
        );
    }

    pub(crate) fn start_use_cooldown(&mut self, id: PlayerId, stack: &ItemStack) {
        if let Some(cooldown) = stack_components::use_cooldown(stack) {
            self.start_group_cooldown(id, cooldown.group, cooldown.ticks());
        }
    }

    pub fn is_on_cooldown(&self, id: PlayerId, item: ItemId) -> bool {
        group_of(item).is_some_and(|group| self.group_on_cooldown(id, group))
    }

    pub(crate) fn group_on_cooldown(&self, id: PlayerId, group: &str) -> bool {
        self.players
            .get(&id)
            .and_then(|player| player.cooldowns.get(group))
            .is_some_and(|expiry| *expiry > self.current_tick())
    }

    pub(crate) fn stack_on_cooldown(&self, id: PlayerId, stack: &ItemStack) -> bool {
        stack_components::cooldown_group(stack)
            .is_some_and(|group| self.group_on_cooldown(id, group))
    }

    pub(crate) fn held_on_cooldown(&self, id: PlayerId, hand: Hand) -> bool {
        self.held(id, hand)
            .is_some_and(|stack| self.stack_on_cooldown(id, &stack))
    }

    pub(crate) fn tick_cooldowns(&mut self) {
        let now = self.current_tick();
        let mut ended = Vec::new();
        for (id, player) in &mut self.players {
            player.cooldowns.retain(|group, expiry| {
                let live = *expiry > now;
                if !live {
                    ended.push((*id, group.clone()));
                }
                live
            });
        }
        for (id, group) in ended {
            self.push(Audience::One(id), ClientEvent::Cooldown { group, ticks: 0 });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use rspigot_domain::player::{ClientSettings, GameProfile};
    use rspigot_domain::text::Component;

    use super::*;
    use crate::config::{GameConfig, LevelData};
    use crate::events::Inbound;
    use crate::status::StatusBoard;
    use crate::world_state::tests::Quiet;

    #[derive(Clone, Default)]
    struct Record(Arc<Mutex<Vec<ClientEvent>>>);

    impl ClientLink for Record {
        fn send(&self, event: ClientEvent) {
            self.0.lock().unwrap().push(event);
        }

        fn disconnect(&self, _: Component) {}

        fn is_connected(&self) -> bool {
            true
        }
    }

    fn arrive() -> (Server<Record>, PlayerId, Record) {
        let config = GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 1,
            ..GameConfig::default()
        };
        let mut server = Server::new(
            config,
            LevelData::default(),
            Arc::new(Quiet),
            Arc::new(Quiet),
            StatusBoard::new(1),
        )
        .with_rng_seed(5);
        server.wait_for_spawn(Duration::from_secs(10));
        let id = PlayerId::offline("Alice");
        let link = Record::default();
        server.handle(Inbound::Join {
            profile: GameProfile {
                id,
                name: "Alice".to_owned(),
                properties: Vec::new(),
            },
            settings: ClientSettings::default(),
            link: link.clone(),
            address: [127, 0, 0, 1].into(),
            session: 1,
        });
        server.deliver();
        (server, id, link)
    }

    fn cooldown_events(link: &Record) -> Vec<(String, i32)> {
        link.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match event {
                ClientEvent::Cooldown { group, ticks } => Some((group.clone(), *ticks)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_started_cooldown_blocks_the_item_and_tells_the_client() {
        let (mut server, id, link) = arrive();
        let pearl = ItemKind::EnderPearl.id();
        assert!(!server.is_on_cooldown(id, pearl));
        server.start_cooldown(id, pearl, 20);
        server.deliver();
        assert!(server.is_on_cooldown(id, pearl));
        assert!(!server.is_on_cooldown(id, ItemKind::Snowball.id()));
        assert_eq!(
            cooldown_events(&link),
            vec![("minecraft:ender_pearl".to_owned(), 20)]
        );
    }

    #[test]
    fn the_cooldown_expires_after_its_ticks_and_the_client_is_told_zero() {
        let (mut server, id, link) = arrive();
        let pearl = ItemKind::EnderPearl.id();
        server.start_cooldown(id, pearl, 3);
        for _ in 0..2 {
            server.tick();
            assert!(server.is_on_cooldown(id, pearl));
        }
        server.tick();
        server.deliver();
        assert!(!server.is_on_cooldown(id, pearl));
        assert_eq!(
            cooldown_events(&link).last(),
            Some(&("minecraft:ender_pearl".to_owned(), 0))
        );
    }

    #[test]
    fn a_stack_cooldown_uses_its_own_seconds_and_group() {
        let (mut server, id, link) = arrive();
        let mut stick = ItemStack::new(ItemKind::Stick.id(), std::num::NonZeroU8::MIN);
        stick.components.insert(
            "minecraft:use_cooldown",
            rspigot_nbt::Compound::new()
                .with("seconds", 2.0f32)
                .with("cooldown_group", "minecraft:custom"),
        );
        server.start_use_cooldown(id, &stick);
        server.deliver();
        assert!(server.stack_on_cooldown(id, &stick));
        assert!(!server.is_on_cooldown(id, ItemKind::Stick.id()));
        assert_eq!(
            cooldown_events(&link),
            vec![("minecraft:custom".to_owned(), 40)]
        );
        let pearl = ItemStack::new(ItemKind::EnderPearl.id(), std::num::NonZeroU8::MIN);
        server.start_use_cooldown(id, &pearl);
        assert!(server.stack_on_cooldown(id, &pearl));
    }
}
