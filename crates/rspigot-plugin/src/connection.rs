use rspigot_plugin_protocol::v1::{self, from_plugin, player_order};

use crate::{Error, PlayerConnectionHandle};

impl PlayerConnectionHandle {
    pub async fn transfer(&self, host: impl Into<String>, port: u16) -> Result<(), Error> {
        self.link
            .act(from_plugin::Body::Player(v1::PlayerOrder {
                player: self.target.clone(),
                order: Some(player_order::Order::Transfer(v1::Transfer {
                    host: host.into(),
                    port: u32::from(port),
                })),
            }))
            .await
    }
}
