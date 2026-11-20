use rspigot_domain::player::PlayerId;

use crate::permissions::Rights;
use crate::player::Player;
use crate::plugins::PlayerOrder;
use crate::ports::ClientLink;
use crate::server::Server;

impl<L: ClientLink> Server<L> {
    pub(crate) fn rights_for(&self, player: &Player<L>) -> Rights {
        let attachments = &self.plugins.attachments;
        self.permissions
            .rights_with(
                player.id,
                player.name(),
                player.permission_level(),
                &attachments.groups_of(player.id),
            )
            .extended(attachments.nodes_of(player.id))
    }

    pub(super) fn permission_order(
        &mut self,
        connection: u64,
        id: PlayerId,
        order: PlayerOrder,
    ) -> Result<(), String> {
        match order {
            PlayerOrder::SetPermission { node, value } => {
                if node.trim().is_empty() {
                    return Err("the permission node is empty".to_owned());
                }
                self.plugins.attachments.set(connection, id, &node, value);
            }
            PlayerOrder::UnsetPermission(node) => {
                self.plugins.attachments.unset(connection, id, &node);
            }
            PlayerOrder::AddGroup(group) => {
                if !self.permissions.has_group(&group) {
                    return Err(format!("there is no group {group} in permissions.yml"));
                }
                self.plugins.attachments.add_group(connection, id, &group);
            }
            PlayerOrder::RemoveGroup(group) => {
                self.plugins
                    .attachments
                    .remove_group(connection, id, &group);
            }
            PlayerOrder::RefreshCommands => {}
            _ => return Ok(()),
        }
        self.send_commands(id);
        Ok(())
    }

    pub(crate) fn forget_attachments_of(&mut self, id: PlayerId) {
        self.plugins.attachments.forget_player(id);
    }
}
