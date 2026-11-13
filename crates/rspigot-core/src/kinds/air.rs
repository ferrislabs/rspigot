use rspigot_ai::AirAction;
use rspigot_domain::EntityId;

use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn air_action(&mut self, id: EntityId, action: AirAction) {
        match action {
            AirAction::Vex(action) => self.vex_action(id, action),
            AirAction::Bat(action) => self.bat_action(id, action),
            AirAction::Phantom(action) => self.phantom_action(id, action),
            AirAction::Wither(action) => self.wither_action(id, action),
        }
    }
}
