use std::sync::Arc;

use rspigot_domain::boss_event::{BossMessages, ServerBossEvent};
use rspigot_domain::custom_boss::{CustomBossBar, CustomBossError};
use rspigot_domain::player::PlayerId;
use rspigot_domain::text::Component;
use rspigot_domain::{EntityId, Identifier};

use crate::chunks::{ChunkJob, Priority};
use crate::events::ClientEvent;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

impl<L: ClientLink> Server<L> {
    pub(crate) fn add_boss_bar(&mut self, id: EntityId, bar: ServerBossEvent) {
        self.entities.boss_bars.insert(id, bar);
        self.sync_boss_bar(id);
    }

    pub(crate) fn update_boss_bar(
        &mut self,
        id: EntityId,
        update: impl FnOnce(&mut ServerBossEvent) -> BossMessages,
    ) {
        let Some(bar) = self.entities.boss_bars.get_mut(&id) else {
            return;
        };
        let messages = update(bar);
        let uuid = bar.id();
        self.send_boss_messages(uuid, messages);
    }

    pub(crate) fn send_boss_messages(&mut self, uuid: uuid::Uuid, messages: BossMessages) {
        for (player, update) in messages {
            if self.players.contains_key(&player) {
                self.push(
                    Audience::One(player),
                    ClientEvent::BossEvent { id: uuid, update },
                );
            }
        }
    }

    pub(crate) fn remove_boss_bar(&mut self, id: EntityId) {
        let Some(mut bar) = self.entities.boss_bars.remove(&id) else {
            return;
        };
        let uuid = bar.id();
        let messages = bar.remove_all_players();
        self.send_boss_messages(uuid, messages);
    }

    fn sync_boss_bar(&mut self, id: EntityId) {
        let Some(bar) = self.entities.boss_bars.get(&id) else {
            return;
        };
        let seeing: Vec<PlayerId> = self
            .players
            .values()
            .filter(|player| player.tracked.contains(&id))
            .map(|player| player.id)
            .collect();
        let departed: Vec<PlayerId> = bar
            .players()
            .filter(|player| !seeing.contains(player))
            .collect();
        let name = self.entity_name(id);
        self.update_boss_bar(id, |bar| {
            let mut messages = BossMessages::new();
            for player in departed {
                messages.extend(bar.remove_player(player));
            }
            for player in seeing {
                messages.extend(bar.add_player(player));
            }
            if let Some(name) = name {
                messages.extend(bar.set_name(name));
            }
            messages
        });
    }

    pub(crate) fn sync_boss_bars(&mut self) {
        let ids: Vec<EntityId> = self.entities.boss_bars.keys().copied().collect();
        for id in ids {
            self.sync_boss_bar(id);
        }
    }
}

const CUSTOM_BOSS_DATA: &str = "custom_boss_events";

impl<L: ClientLink> Server<L> {
    pub(crate) fn load_custom_boss_bars(&mut self) {
        let Some(nbt) = self.world.level.custom_boss_events.take() else {
            return;
        };
        let mut uuids = Vec::new();
        for _ in 0..nbt.len() {
            uuids.push(self.random_uuid());
        }
        let mut uuids = uuids.into_iter();
        self.custom_bars
            .load_nbt(&nbt, || uuids.next().unwrap_or_default());
        self.world.level.custom_boss_events = Some(nbt);
    }

    pub(crate) fn save_custom_boss_bars(&mut self) {
        self.world.level.custom_boss_events = Some(self.custom_bars.to_nbt());
    }

    pub(crate) fn save_custom_boss_bars_data(&mut self) {
        self.save_custom_boss_bars();
        if let Some(data) = &self.world.level.custom_boss_events {
            self.workers.submit(
                Priority::BACKGROUND,
                ChunkJob::SaveData {
                    dimension: Arc::clone(&self.world.dimension),
                    name: CUSTOM_BOSS_DATA,
                    data: Arc::new(data.clone()),
                },
            );
        }
    }

    pub(crate) fn create_custom_boss_bar(
        &mut self,
        key: Option<Identifier>,
        name: Component,
    ) -> Result<u64, CustomBossError> {
        let uuid = self.random_uuid();
        self.custom_bars.create(key, uuid, name)
    }

    pub(crate) fn change_custom_boss_bar<R>(
        &mut self,
        handle: u64,
        change: impl FnOnce(&mut CustomBossBar) -> (R, BossMessages),
    ) -> Option<R> {
        let bar = self.custom_bars.get_mut(handle)?;
        let uuid = bar.event().id();
        let (result, messages) = change(bar);
        self.send_boss_messages(uuid, messages);
        Some(result)
    }

    pub(crate) fn remove_custom_boss_bar(&mut self, handle: u64) -> bool {
        let Some(mut bar) = self.custom_bars.remove(handle) else {
            return false;
        };
        let uuid = bar.event().id();
        let messages = bar.event_mut().remove_all_players();
        self.send_boss_messages(uuid, messages);
        true
    }

    pub(crate) fn custom_boss_join_events(&self, player: PlayerId) -> Vec<ClientEvent> {
        self.custom_bars
            .handles()
            .into_iter()
            .filter_map(|handle| self.custom_bars.get(handle))
            .filter(|bar| bar.event().has_player(player) && bar.event().is_visible())
            .map(|bar| ClientEvent::BossEvent {
                id: bar.event().id(),
                update: bar.event().add_packet(),
            })
            .collect()
    }

    pub(crate) fn forget_custom_boss_player(&mut self, player: PlayerId) {
        for handle in self.custom_bars.handles() {
            let loose = self
                .custom_bars
                .get(handle)
                .is_some_and(|bar| bar.key().is_none());
            if loose {
                self.change_custom_boss_bar(handle, |bar| {
                    ((), bar.event_mut().remove_player(player))
                });
            }
        }
    }

    pub(crate) fn boss_bar_view(&self, key: &Identifier) -> Option<CustomBossBar> {
        let handle = self.custom_bars.handle_of(key)?;
        self.custom_bars.get(handle).cloned()
    }

    pub(crate) fn boss_bar_views(&self) -> Vec<CustomBossBar> {
        self.custom_bars
            .keyed()
            .into_iter()
            .filter_map(|(_, handle)| self.custom_bars.get(handle).cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use rspigot_domain::boss_event::BossUpdate;
    use rspigot_domain::custom_boss::CustomBossBar;

    use super::*;
    use crate::config::{GameConfig, LevelData};
    use crate::end_tests::{Tap, arrive};
    use crate::status::StatusBoard;
    use crate::world_state::tests::Quiet;

    fn updates(link: &Tap) -> Vec<BossUpdate> {
        link.take()
            .into_iter()
            .filter_map(|event| match event {
                ClientEvent::BossEvent { update, .. } => Some(update),
                _ => None,
            })
            .collect()
    }

    fn key(path: &str) -> Identifier {
        Identifier::minecraft(path)
    }

    fn bar(server: &mut Server<Tap>, path: &str) -> u64 {
        server
            .create_custom_boss_bar(Some(key(path)), Component::text(path))
            .unwrap()
    }

    #[test]
    fn adding_a_player_shows_the_bar_and_changes_reach_them() {
        let (mut server, id, link) = arrive();
        link.take();
        let handle = bar(&mut server, "raid");
        server.change_custom_boss_bar(handle, |bar| ((), bar.event_mut().add_player(id)));
        server.deliver();
        assert!(matches!(updates(&link)[0], BossUpdate::Add { .. }));
        server.change_custom_boss_bar(handle, |bar| ((), bar.set_value(50)));
        server.deliver();
        assert_eq!(updates(&link), vec![BossUpdate::Progress(0.5)]);
    }

    #[test]
    fn removing_a_bar_removes_it_from_its_viewers() {
        let (mut server, id, link) = arrive();
        let handle = bar(&mut server, "raid");
        server.change_custom_boss_bar(handle, |bar| ((), bar.event_mut().add_player(id)));
        server.deliver();
        link.take();
        assert!(server.remove_custom_boss_bar(handle));
        server.deliver();
        assert_eq!(updates(&link), vec![BossUpdate::Remove]);
        assert!(!server.remove_custom_boss_bar(handle));
    }

    #[test]
    fn a_loose_bar_forgets_a_disconnected_player_but_a_keyed_one_keeps_them() {
        let (mut server, id, _) = arrive();
        let keyed = bar(&mut server, "raid");
        let loose = server
            .create_custom_boss_bar(None, Component::text("loose"))
            .unwrap();
        for handle in [keyed, loose] {
            server.change_custom_boss_bar(handle, |bar| ((), bar.event_mut().add_player(id)));
        }
        server.remove_player(id, false);
        assert!(
            server
                .custom_bars
                .get(keyed)
                .unwrap()
                .event()
                .has_player(id)
        );
        assert!(
            !server
                .custom_bars
                .get(loose)
                .unwrap()
                .event()
                .has_player(id)
        );
    }

    #[test]
    fn a_reconnecting_player_gets_their_keyed_bars_back() {
        let (mut server, id, _) = arrive();
        let handle = bar(&mut server, "raid");
        server.change_custom_boss_bar(handle, |bar| ((), bar.event_mut().add_player(id)));
        assert_eq!(server.custom_boss_join_events(id).len(), 1);
        server.change_custom_boss_bar(handle, |bar| ((), bar.event_mut().set_visible(false)));
        assert!(server.custom_boss_join_events(id).is_empty());
    }

    #[test]
    fn custom_bars_survive_a_restart_through_the_level() {
        let (mut server, id, _) = arrive();
        let handle = bar(&mut server, "raid");
        server.change_custom_boss_bar(handle, |bar: &mut CustomBossBar| {
            bar.set_max(20);
            ((), bar.set_value(5))
        });
        server.change_custom_boss_bar(handle, |bar| ((), bar.event_mut().add_player(id)));
        server.save_custom_boss_bars();
        let level = LevelData {
            custom_boss_events: server.world.level.custom_boss_events.clone(),
            ..LevelData::default()
        };
        let restored = Server::<Tap>::new(
            GameConfig {
                chunk_workers: 1,
                spawn_chunk_radius: 0,
                ..GameConfig::default()
            },
            level,
            std::sync::Arc::new(Quiet),
            std::sync::Arc::new(Quiet),
            StatusBoard::new(1),
        );
        let bar = restored.boss_bar_view(&key("raid")).unwrap();
        assert_eq!((bar.value(), bar.max()), (5, 20));
        assert!(bar.event().has_player(id));
    }
}
