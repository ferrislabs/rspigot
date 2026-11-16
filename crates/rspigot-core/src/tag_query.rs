use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, EntityId};

use crate::events::ClientEvent;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

impl<L: ClientLink> Server<L> {
    pub(crate) fn handle_block_tag_query(&mut self, id: PlayerId, transaction: i32, pos: BlockPos) {
        if !self
            .players
            .get(&id)
            .is_some_and(|player| player.permission_level() >= 2)
        {
            return;
        }
        let tag = self.block_entity_nbt(pos);
        self.push(
            Audience::One(id),
            ClientEvent::TagQuery { transaction, tag },
        );
    }

    pub(crate) fn handle_entity_tag_query(
        &mut self,
        id: PlayerId,
        transaction: i32,
        entity: EntityId,
    ) {
        if !self
            .players
            .get(&id)
            .is_some_and(|player| player.permission_level() >= 2)
        {
            return;
        }
        let Some(mut tag) = self.saved_entity(entity) else {
            return;
        };
        tag.remove("id");
        self.push(
            Audience::One(id),
            ClientEvent::TagQuery {
                transaction,
                tag: Some(tag),
            },
        );
    }
}
