use std::collections::BTreeSet;

use rspigot_nbt::Tag;
use rspigot_protocol::codec::{Decode, VarInt, decode_string};
use rspigot_protocol::ids::play::clientbound as id;
use rspigot_protocol::play::boss::{BossEvent, BossOperation};
use rspigot_protocol::play::chat::ClearTitles;
use rspigot_protocol::play::player::{PlayerInfoRemove, PlayerInfoUpdate, TabList};
use rspigot_protocol::play::scoreboard::{SetObjective, SetPlayerTeam};
use rspigot_protocol::types::Text;
use rspigot_protocol::{Packet, write_packet};
use uuid::Uuid;

const ADD_PLAYER: u8 = 1;
const OBJECTIVE_ADD: u8 = 0;
const OBJECTIVE_REMOVE: u8 = 1;
const TEAM_ADD: u8 = 0;
const TEAM_REMOVE: u8 = 1;
const BOSS_ADD: i32 = 0;
const BOSS_REMOVE: i32 = 1;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DisplayState {
    players: BTreeSet<Uuid>,
    boss_bars: BTreeSet<Uuid>,
    objectives: BTreeSet<String>,
    teams: BTreeSet<String>,
    tab_list: bool,
}

impl DisplayState {
    pub fn observe(&mut self, packet_id: i32, mut body: &[u8]) {
        let input = &mut body;
        match packet_id {
            id::PLAYER_INFO_UPDATE => {
                if let Ok(update) = PlayerInfoUpdate::decode(input)
                    && update.actions.0 & ADD_PLAYER != 0
                {
                    self.players
                        .extend(update.entries.iter().map(|entry| entry.profile_id));
                }
            }
            id::PLAYER_INFO_REMOVE => {
                if let Ok(remove) = PlayerInfoRemove::decode(input) {
                    for profile in remove.profile_ids {
                        self.players.remove(&profile);
                    }
                }
            }
            id::BOSS_EVENT => {
                if let (Ok(bar), Ok(VarInt(operation))) =
                    (Uuid::decode(input), VarInt::decode(input))
                {
                    match operation {
                        BOSS_ADD => {
                            self.boss_bars.insert(bar);
                        }
                        BOSS_REMOVE => {
                            self.boss_bars.remove(&bar);
                        }
                        _ => {}
                    }
                }
            }
            id::SET_OBJECTIVE => {
                if let Some((name, method)) = name_and_method(input) {
                    match method {
                        OBJECTIVE_ADD => {
                            self.objectives.insert(name);
                        }
                        OBJECTIVE_REMOVE => {
                            self.objectives.remove(&name);
                        }
                        _ => {}
                    }
                }
            }
            id::SET_PLAYER_TEAM => {
                if let Some((name, method)) = name_and_method(input) {
                    match method {
                        TEAM_ADD => {
                            self.teams.insert(name);
                        }
                        TEAM_REMOVE => {
                            self.teams.remove(&name);
                        }
                        _ => {}
                    }
                }
            }
            id::TAB_LIST => self.tab_list = true,
            _ => {}
        }
    }

    pub fn clear_packets(&mut self) -> Vec<Vec<u8>> {
        let mut packets = Vec::new();
        if !self.players.is_empty() {
            packets.push(encode(&PlayerInfoRemove {
                profile_ids: self.players.iter().copied().collect(),
            }));
        }
        for bar in &self.boss_bars {
            packets.push(encode(&BossEvent {
                id: *bar,
                operation: BossOperation::Remove,
            }));
        }
        for name in &self.objectives {
            packets.push(encode(&SetObjective {
                name: name.clone(),
                method: OBJECTIVE_REMOVE,
                details: None,
            }));
        }
        for name in &self.teams {
            packets.push(encode(&SetPlayerTeam {
                name: name.clone(),
                method: TEAM_REMOVE,
                parameters: None,
                players: Vec::new(),
            }));
        }
        if self.tab_list {
            packets.push(encode(&TabList {
                header: empty(),
                footer: empty(),
            }));
        }
        packets.push(encode(&ClearTitles { reset_times: true }));
        *self = Self::default();
        packets
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

fn name_and_method(input: &mut &[u8]) -> Option<(String, u8)> {
    let name = decode_string(input, 32_767).ok()?;
    let method = u8::decode(input).ok()?;
    Some((name, method))
}

fn empty() -> Text {
    Text(Tag::String(String::new()))
}

fn encode<P: Packet>(packet: &P) -> Vec<u8> {
    let mut out = Vec::new();
    write_packet(packet, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use rspigot_protocol::play::boss::{BossColor, BossOverlay};
    use rspigot_protocol::play::player::{PlayerInfoActions, PlayerInfoEntry};
    use rspigot_protocol::play::scoreboard::{ObjectiveDetails, TeamParameters};
    use rspigot_protocol::play::types::GameMode;
    use rspigot_protocol::split_packet_id;

    use super::*;

    fn feed<P: Packet>(state: &mut DisplayState, packet: &P) {
        let bytes = encode(packet);
        let (id, body) = split_packet_id(&bytes).unwrap();
        state.observe(id, body);
    }

    fn entry(id: u128) -> PlayerInfoEntry {
        PlayerInfoEntry {
            profile_id: Uuid::from_u128(id),
            name: format!("p{id}"),
            properties: Vec::new(),
            chat_session: None,
            game_mode: GameMode::Survival,
            listed: true,
            latency: 0,
            display_name: None,
            list_order: 0,
            show_hat: true,
        }
    }

    fn ids(packets: &[Vec<u8>]) -> Vec<i32> {
        packets
            .iter()
            .map(|packet| split_packet_id(packet).unwrap().0)
            .collect()
    }

    #[test]
    fn nothing_tracked_still_clears_the_titles() {
        let mut state = DisplayState::default();
        assert_eq!(ids(&state.clear_packets()), [id::CLEAR_TITLES]);
    }

    #[test]
    fn added_players_are_removed_from_the_tab_list_and_removed_ones_are_forgotten() {
        let mut state = DisplayState::default();
        feed(
            &mut state,
            &PlayerInfoUpdate {
                actions: PlayerInfoActions(ADD_PLAYER | PlayerInfoActions::UPDATE_LISTED.0),
                entries: vec![entry(1), entry(2), entry(3)],
            },
        );
        feed(
            &mut state,
            &PlayerInfoRemove {
                profile_ids: vec![Uuid::from_u128(2)],
            },
        );
        let packets = state.clear_packets();
        let (packet_id, body) = split_packet_id(&packets[0]).unwrap();
        assert_eq!(packet_id, id::PLAYER_INFO_REMOVE);
        let remove = PlayerInfoRemove::decode(&mut &body[..]).unwrap();
        assert_eq!(remove.profile_ids, [Uuid::from_u128(1), Uuid::from_u128(3)]);
        assert!(state.is_empty());
    }

    #[test]
    fn updates_without_add_player_add_nobody() {
        let mut state = DisplayState::default();
        feed(
            &mut state,
            &PlayerInfoUpdate {
                actions: PlayerInfoActions::UPDATE_LATENCY,
                entries: vec![entry(1)],
            },
        );
        assert!(state.is_empty());
    }

    #[test]
    fn boss_bars_objectives_and_teams_still_shown_are_removed() {
        let mut state = DisplayState::default();
        let bar = BossEvent {
            id: Uuid::from_u128(9),
            operation: BossOperation::Add {
                name: empty(),
                progress: 1.0,
                color: BossColor::Red,
                overlay: BossOverlay::Progress,
                flags: 0,
            },
        };
        feed(&mut state, &bar);
        feed(
            &mut state,
            &BossEvent {
                id: Uuid::from_u128(8),
                operation: BossOperation::Remove,
            },
        );
        for (name, method) in [
            ("sidebar", OBJECTIVE_ADD),
            ("gone", OBJECTIVE_ADD),
            ("gone", OBJECTIVE_REMOVE),
        ] {
            feed(
                &mut state,
                &SetObjective {
                    name: name.to_owned(),
                    method,
                    details: Some(ObjectiveDetails {
                        display_name: empty(),
                        render_type: 0,
                        number_format: None,
                    }),
                },
            );
        }
        feed(
            &mut state,
            &SetPlayerTeam {
                name: "red".to_owned(),
                method: TEAM_ADD,
                parameters: Some(TeamParameters {
                    display_name: empty(),
                    prefix: empty(),
                    suffix: empty(),
                    nametag_visibility: 0,
                    collision_rule: 0,
                    color: None,
                    options: 0,
                }),
                players: vec!["Steve".to_owned()],
            },
        );
        feed(
            &mut state,
            &TabList {
                header: empty(),
                footer: empty(),
            },
        );
        let packets = state.clear_packets();
        assert_eq!(
            ids(&packets),
            [
                id::BOSS_EVENT,
                id::SET_OBJECTIVE,
                id::SET_PLAYER_TEAM,
                id::TAB_LIST,
                id::CLEAR_TITLES
            ]
        );
        let (_, body) = split_packet_id(&packets[0]).unwrap();
        let removed = BossEvent::decode(&mut &body[..]).unwrap();
        assert_eq!(removed.id, Uuid::from_u128(9));
        assert_eq!(removed.operation, BossOperation::Remove);
        let (_, body) = split_packet_id(&packets[1]).unwrap();
        assert_eq!(
            name_and_method(&mut &body[..]),
            Some(("sidebar".to_owned(), OBJECTIVE_REMOVE))
        );
        let (_, body) = split_packet_id(&packets[2]).unwrap();
        assert_eq!(
            name_and_method(&mut &body[..]),
            Some(("red".to_owned(), TEAM_REMOVE))
        );
    }

    #[test]
    fn garbage_is_ignored() {
        let mut state = DisplayState::default();
        state.observe(id::SET_OBJECTIVE, &[0xff, 0xff]);
        state.observe(id::BOSS_EVENT, &[1, 2]);
        state.observe(id::PLAYER_INFO_UPDATE, &[1, 5]);
        assert!(state.is_empty());
    }
}
