use rspigot_domain::EntityId;
use rspigot_domain::boss_event::{BossMessages, ServerBossEvent};
use rspigot_domain::player::PlayerId;

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

    fn send_boss_messages(&mut self, uuid: uuid::Uuid, messages: BossMessages) {
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
