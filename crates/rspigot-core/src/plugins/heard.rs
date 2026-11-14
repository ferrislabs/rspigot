use rspigot_domain::player::PlayerId;
use uuid::Uuid;

use crate::plugins::events::{PlayerCustomClick, PlayerResourcePackStatus, ResourcePackStatus};
use crate::plugins::steps::Continuation;
use crate::plugins::{EventKind, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn hear_resource_pack(
        &mut self,
        id: PlayerId,
        pack: Uuid,
        status: ResourcePackStatus,
    ) {
        self.fire(
            id,
            Continuation::Heard(EventKind::PlayerResourcePackStatus),
            move |server| {
                server.snapshot_of(id).map(|player| {
                    EventPayload::PlayerResourcePackStatus(PlayerResourcePackStatus {
                        pack_id: pack,
                        player,
                        status,
                    })
                })
            },
        );
    }

    pub(crate) fn hear_custom_click(&mut self, id: PlayerId, click: String, data: Option<String>) {
        self.fire(
            id,
            Continuation::Heard(EventKind::PlayerCustomClick),
            move |server| {
                server.snapshot_of(id).map(|player| {
                    EventPayload::PlayerCustomClick(PlayerCustomClick {
                        data,
                        click_id: click,
                        player,
                    })
                })
            },
        );
    }
}
