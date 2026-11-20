use crate::events::ClientEvent;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use rspigot_domain::player::PlayerId;

const COMBAT_TIMEOUT: u64 = 300;
const DAMAGE_TIMEOUT: u64 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CombatPacket {
    Enter,
    End(i32),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CombatSignal {
    last_damage: u64,
    start: u64,
    end: u64,
    taking_damage: bool,
    in_combat: bool,
    pending: Vec<CombatPacket>,
}

impl CombatSignal {
    pub(crate) fn record(&mut self, now: u64, alive: bool, living_cause: bool) {
        self.recheck(now, alive);
        self.last_damage = now;
        self.taking_damage = true;
        if living_cause && !self.in_combat && alive {
            self.in_combat = true;
            self.start = now;
            self.end = now;
            self.pending.push(CombatPacket::Enter);
        }
    }

    pub(crate) fn recheck(&mut self, now: u64, alive: bool) {
        let timeout = if self.in_combat {
            COMBAT_TIMEOUT
        } else {
            DAMAGE_TIMEOUT
        };
        if self.taking_damage && (!alive || now.saturating_sub(self.last_damage) > timeout) {
            let was_in_combat = self.in_combat;
            self.taking_damage = false;
            self.in_combat = false;
            self.end = now;
            if was_in_combat {
                let duration = self.end.saturating_sub(self.start);
                self.pending.push(CombatPacket::End(
                    i32::try_from(duration).unwrap_or(i32::MAX),
                ));
            }
        }
    }

    pub(crate) fn take(&mut self) -> Vec<CombatPacket> {
        std::mem::take(&mut self.pending)
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn flush_combat(&mut self, id: PlayerId) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        for packet in player.vitals.combat_signal.take() {
            let event = match packet {
                CombatPacket::Enter => ClientEvent::CombatEnter,
                CombatPacket::End(duration) => ClientEvent::CombatEnd { duration },
            };
            self.push(Audience::One(id), event);
        }
    }
}

#[cfg(test)]
mod tests {
    use rspigot_domain::damage::DamageType;

    use super::*;
    use crate::end_tests::{Tap, arrive};
    use crate::health::DamageSource;

    fn survival() -> (Server<Tap>, PlayerId, Tap) {
        let (mut server, id, link) = arrive();
        crate::commands::execute(
            &mut server,
            crate::commands::CommandSender::Console,
            "gamemode survival Alice",
        );
        server.handle_input(id, crate::events::PlayerInput::Loaded);
        server.deliver();
        link.take();
        (server, id, link)
    }

    fn combat_events(link: &Tap) -> Vec<ClientEvent> {
        link.take()
            .into_iter()
            .filter(|event| {
                matches!(
                    event,
                    ClientEvent::CombatEnter
                        | ClientEvent::CombatEnd { .. }
                        | ClientEvent::CombatKill { .. }
                )
            })
            .collect()
    }

    #[test]
    fn a_living_attacker_enters_combat_once_and_it_ends_after_the_timeout() {
        let mut signal = CombatSignal::default();
        signal.record(10, true, true);
        signal.record(20, true, true);
        assert_eq!(signal.take(), [CombatPacket::Enter]);
        signal.recheck(310, true);
        assert!(signal.take().is_empty());
        signal.recheck(321, true);
        assert_eq!(signal.take(), [CombatPacket::End(311)]);
    }

    #[test]
    fn environmental_damage_never_enters_combat() {
        let mut signal = CombatSignal::default();
        signal.record(10, true, false);
        signal.recheck(500, true);
        assert!(signal.take().is_empty());
    }

    #[test]
    fn dying_ends_combat_at_once() {
        let mut signal = CombatSignal::default();
        signal.record(10, true, true);
        signal.take();
        signal.recheck(15, false);
        assert_eq!(signal.take(), [CombatPacket::End(5)]);
    }

    #[test]
    fn the_server_sends_enter_when_a_living_entity_hurts_and_end_when_it_times_out() {
        let (mut server, id, link) = survival();
        let attacker = server.source_of(id).expect("source");
        server.hurt(
            id,
            &DamageSource::by(DamageType::PlayerAttack, attacker),
            1.0,
        );
        server.deliver();
        assert_eq!(combat_events(&link), [ClientEvent::CombatEnter]);
        for _ in 0..301 {
            server.tick();
        }
        server.deliver();
        let events = combat_events(&link);
        assert!(matches!(
            events.as_slice(),
            [ClientEvent::CombatEnd { duration }] if *duration > 300
        ));
    }

    #[test]
    fn a_fatal_blow_sends_the_kill_packet_and_ends_combat() {
        let (mut server, id, link) = survival();
        let attacker = server.source_of(id).expect("source");
        server.hurt(
            id,
            &DamageSource::by(DamageType::PlayerAttack, attacker),
            1000.0,
        );
        for _ in 0..3 {
            server.tick();
        }
        server.deliver();
        let events = combat_events(&link);
        assert_eq!(events.first(), Some(&ClientEvent::CombatEnter));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, ClientEvent::CombatKill { .. }))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, ClientEvent::CombatEnd { .. }))
        );
    }
}
