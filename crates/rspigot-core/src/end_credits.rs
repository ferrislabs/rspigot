use rspigot_domain::Rotation;
use rspigot_domain::player::PlayerId;

use crate::end_portal::overworld;
use crate::events::{ClientEvent, GameEvent};
use crate::plugins::events::{PlayerRespawn, PlayerRespawnPatch, PlayerRespawnReason};
use crate::plugins::{Continuation, EventPayload, Location, RespawnStep};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::travel::Destination;

impl<L: ClientLink> Server<L> {
    pub(crate) fn show_end_credits(&mut self, id: PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if player.won_game {
            return;
        }
        self.leave_dimension(id);
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        player.won_game = true;
        player.link.send(ClientEvent::GameEvent(GameEvent::WinGame));
        player.seen_credits = true;
    }

    pub(crate) fn credits_respawn(&mut self, id: PlayerId) {
        let Some(spawn) = self.overworld_spawn() else {
            return;
        };
        let rotation = Rotation::new(self.world.level.spawn_angle, 0.0);
        let location = Location {
            world: overworld().to_string(),
            position: spawn,
            rotation,
        };
        let step = RespawnStep {
            player: id,
            credits: true,
        };
        self.fire(id, Continuation::Respawn(step), move |server| {
            server.snapshot_of(id).map(|player| {
                EventPayload::PlayerRespawn(PlayerRespawn {
                    anchor_spawn: false,
                    bed_spawn: false,
                    player,
                    respawn_location: location,
                    respawn_reason: PlayerRespawnReason::EndPortal,
                })
            })
        });
    }

    pub(crate) fn finish_credits(&mut self, id: PlayerId, patch: PlayerRespawnPatch) {
        let Some(spawn) = self.overworld_spawn() else {
            return;
        };
        let angle = self.world.level.spawn_angle;
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if !player.won_game || !player.link.is_connected() {
            return;
        }
        player.won_game = false;
        let destination = match &patch.respawn_location {
            Some(location) => Destination::from_location(location, &overworld()),
            None => Destination {
                dimension: overworld(),
                position: spawn,
                rotation: Some(Rotation::new(angle, 0.0)),
                platform: false,
            },
        };
        self.change_dimension_now(id, destination);
    }
}

#[cfg(test)]
mod tests {
    use rspigot_blocks::BlockKind;
    use rspigot_domain::{BlockPos, Identifier, Vec3};

    use super::*;
    use crate::end_tests::{Tap, arrive, end, enter_end};
    use crate::events::{Inbound, PlayerInput};
    use crate::player_data::PlayerSave;

    fn ready() -> (Server<Tap>, PlayerId, Tap) {
        let (mut server, id, link) = arrive();
        enter_end(&mut server, id);
        server.in_dimension(&end(), |server| {
            server.set_block(
                BlockPos::new(100, 63, 0),
                BlockKind::EndPortal.default_state(),
            );
        });
        link.take();
        (server, id, link)
    }

    fn step_into_portal(server: &mut Server<Tap>, id: PlayerId) {
        if let Some(player) = server.players.get_mut(&id) {
            player.position = Vec3::new(100.5, 63.0, 0.5);
            player.portal_cooldown = 0;
        }
        server.tick();
        server.deliver();
    }

    fn won(events: &[ClientEvent]) -> usize {
        events
            .iter()
            .filter(|event| matches!(event, ClientEvent::GameEvent(GameEvent::WinGame)))
            .count()
    }

    fn respawns(events: &[ClientEvent]) -> Vec<u8> {
        events
            .iter()
            .filter_map(|event| match event {
                ClientEvent::Respawn { keep_data, .. } => Some(*keep_data),
                _ => None,
            })
            .collect()
    }

    fn click_respawn(server: &mut Server<Tap>, id: PlayerId) {
        server.handle(Inbound::Input {
            player: id,
            input: PlayerInput::Respawn,
        });
        server.tick();
        server.deliver();
    }

    #[test]
    fn the_first_exit_shows_the_credits_and_keeps_the_player_in_the_end_until_the_respawn() {
        let (mut server, id, link) = ready();
        assert!(!server.players[&id].seen_credits);
        step_into_portal(&mut server, id);
        let events = link.take();
        assert_eq!(won(&events), 1);
        assert!(respawns(&events).is_empty());
        let player = &server.players[&id];
        assert!(player.won_game && player.seen_credits);
        assert_eq!(player.dim, end());
        server.tick();
        assert_eq!(won(&link.take()), 0);
    }

    #[test]
    fn the_respawn_after_the_credits_returns_to_the_overworld_spawn_keeping_data() {
        let (mut server, id, link) = ready();
        step_into_portal(&mut server, id);
        link.take();
        click_respawn(&mut server, id);
        let events = link.take();
        assert_eq!(respawns(&events), vec![3]);
        let player = &server.players[&id];
        assert_eq!(player.dim, Identifier::minecraft("overworld"));
        assert_eq!(player.position, server.world.spawn);
        assert!(!player.won_game && player.seen_credits);
        click_respawn(&mut server, id);
        assert!(respawns(&link.take()).is_empty());
    }

    #[test]
    fn the_second_exit_teleports_straight_home_without_credits() {
        let (mut server, id, link) = ready();
        step_into_portal(&mut server, id);
        click_respawn(&mut server, id);
        enter_end(&mut server, id);
        link.take();
        step_into_portal(&mut server, id);
        server.deliver();
        let events = link.take();
        assert_eq!(won(&events), 0);
        assert_eq!(respawns(&events), vec![3]);
        assert_eq!(server.players[&id].dim, Identifier::minecraft("overworld"));
    }

    #[test]
    fn the_seen_credits_flag_survives_a_save() {
        let (mut server, id, _) = ready();
        assert!(!PlayerSave::capture(&server.players[&id]).seen_credits);
        step_into_portal(&mut server, id);
        let save = PlayerSave::capture(&server.players[&id]);
        assert!(save.seen_credits);
        let restored = PlayerSave::from_nbt(&save.to_nbt()).expect("decodes");
        assert!(restored.seen_credits);
    }

    #[test]
    fn a_player_who_has_not_won_does_not_leave_the_end_by_clicking_respawn() {
        let (mut server, id, link) = ready();
        click_respawn(&mut server, id);
        assert!(respawns(&link.take()).is_empty());
        assert_eq!(server.players[&id].dim, end());
    }
}
