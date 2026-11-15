use crate::Packet;
use crate::codec::{Encode, VarInt};
use crate::ids::play::clientbound as id;
use crate::types::Text;

#[derive(Debug, Clone, PartialEq)]
pub enum NumberFormat {
    Blank,
    Styled(Text),
    Fixed(Text),
}

impl Encode for NumberFormat {
    fn encode(&self, out: &mut Vec<u8>) {
        match self {
            Self::Blank => VarInt(0).encode(out),
            Self::Styled(style) => {
                VarInt(1).encode(out);
                style.encode(out);
            }
            Self::Fixed(text) => {
                VarInt(2).encode(out);
                text.encode(out);
            }
        }
    }
}

pub const OBJECTIVE_ADD: u8 = 0;
pub const OBJECTIVE_REMOVE: u8 = 1;
pub const OBJECTIVE_CHANGE: u8 = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectiveDetails {
    pub display_name: Text,
    pub render_type: i32,
    pub number_format: Option<NumberFormat>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetObjective {
    pub name: String,
    pub method: u8,
    pub details: Option<ObjectiveDetails>,
}

impl Encode for SetObjective {
    fn encode(&self, out: &mut Vec<u8>) {
        self.name.encode(out);
        self.method.encode(out);
        if let Some(details) = &self.details
            && self.method != OBJECTIVE_REMOVE
        {
            details.display_name.encode(out);
            VarInt(details.render_type).encode(out);
            details.number_format.encode(out);
        }
    }
}

impl Packet for SetObjective {
    const ID: i32 = id::SET_OBJECTIVE;
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetScore {
    pub owner: String,
    pub objective: String,
    pub value: i32,
    pub display: Option<Text>,
    pub number_format: Option<NumberFormat>,
}

impl Encode for SetScore {
    fn encode(&self, out: &mut Vec<u8>) {
        self.owner.encode(out);
        self.objective.encode(out);
        VarInt(self.value).encode(out);
        self.display.encode(out);
        self.number_format.encode(out);
    }
}

impl Packet for SetScore {
    const ID: i32 = id::SET_SCORE;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResetScore {
    pub owner: String,
    pub objective: Option<String>,
}

impl Encode for ResetScore {
    fn encode(&self, out: &mut Vec<u8>) {
        self.owner.encode(out);
        self.objective.encode(out);
    }
}

impl Packet for ResetScore {
    const ID: i32 = id::RESET_SCORE;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetDisplayObjective {
    pub slot: i32,
    pub objective: String,
}

impl Encode for SetDisplayObjective {
    fn encode(&self, out: &mut Vec<u8>) {
        VarInt(self.slot).encode(out);
        self.objective.encode(out);
    }
}

impl Packet for SetDisplayObjective {
    const ID: i32 = id::SET_DISPLAY_OBJECTIVE;
}

pub const TEAM_ADD: u8 = 0;
pub const TEAM_REMOVE: u8 = 1;
pub const TEAM_CHANGE: u8 = 2;
pub const TEAM_JOIN: u8 = 3;
pub const TEAM_LEAVE: u8 = 4;

#[derive(Debug, Clone, PartialEq)]
pub struct TeamParameters {
    pub display_name: Text,
    pub prefix: Text,
    pub suffix: Text,
    pub nametag_visibility: i32,
    pub collision_rule: i32,
    pub color: Option<i32>,
    pub options: u8,
}

impl Encode for TeamParameters {
    fn encode(&self, out: &mut Vec<u8>) {
        self.display_name.encode(out);
        self.prefix.encode(out);
        self.suffix.encode(out);
        VarInt(self.nametag_visibility).encode(out);
        VarInt(self.collision_rule).encode(out);
        match self.color {
            Some(color) => {
                true.encode(out);
                VarInt(color).encode(out);
            }
            None => false.encode(out),
        }
        self.options.encode(out);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetPlayerTeam {
    pub name: String,
    pub method: u8,
    pub parameters: Option<TeamParameters>,
    pub players: Vec<String>,
}

impl Encode for SetPlayerTeam {
    fn encode(&self, out: &mut Vec<u8>) {
        self.name.encode(out);
        self.method.encode(out);
        if matches!(self.method, TEAM_ADD | TEAM_CHANGE)
            && let Some(parameters) = &self.parameters
        {
            parameters.encode(out);
        }
        if matches!(self.method, TEAM_ADD | TEAM_JOIN | TEAM_LEAVE) {
            self.players.encode(out);
        }
    }
}

impl Packet for SetPlayerTeam {
    const ID: i32 = id::SET_PLAYER_TEAM;
}

#[cfg(test)]
mod tests {
    use rspigot_nbt::Tag;

    use super::*;

    fn text(s: &str) -> Text {
        Text(Tag::String(s.into()))
    }

    fn bytes(packet: &impl Encode) -> Vec<u8> {
        let mut out = Vec::new();
        packet.encode(&mut out);
        out
    }

    #[test]
    fn the_packet_ids_match_the_packets_report() {
        assert_eq!(<ResetScore as Packet>::ID, 80);
        assert_eq!(<SetDisplayObjective as Packet>::ID, 100);
        assert_eq!(<SetObjective as Packet>::ID, 108);
        assert_eq!(<SetPlayerTeam as Packet>::ID, 111);
        assert_eq!(<SetScore as Packet>::ID, 112);
    }

    #[test]
    fn removing_an_objective_writes_only_name_and_method() {
        let packet = SetObjective {
            name: "kills".into(),
            method: OBJECTIVE_REMOVE,
            details: None,
        };
        assert_eq!(bytes(&packet), [5, b'k', b'i', b'l', b'l', b's', 1]);
    }

    #[test]
    fn adding_an_objective_writes_name_render_type_and_the_optional_format() {
        let packet = SetObjective {
            name: "a".into(),
            method: OBJECTIVE_ADD,
            details: Some(ObjectiveDetails {
                display_name: text("A"),
                render_type: 1,
                number_format: Some(NumberFormat::Blank),
            }),
        };
        let out = bytes(&packet);
        assert_eq!(&out[..3], &[1, b'a', 0]);
        assert_eq!(&out[out.len() - 3..], &[1, 1, 0]);
    }

    #[test]
    fn a_score_writes_owner_objective_value_and_two_optionals() {
        let packet = SetScore {
            owner: "x".into(),
            objective: "o".into(),
            value: 300,
            display: None,
            number_format: Some(NumberFormat::Fixed(text("f"))),
        };
        let out = bytes(&packet);
        assert_eq!(&out[..6], &[1, b'x', 1, b'o', 0xAC, 0x02]);
        assert_eq!(out[6], 0);
        assert_eq!(&out[7..9], &[1, 2]);
    }

    #[test]
    fn a_reset_names_an_optional_objective() {
        let all = ResetScore {
            owner: "x".into(),
            objective: None,
        };
        assert_eq!(bytes(&all), [1, b'x', 0]);
        let one = ResetScore {
            owner: "x".into(),
            objective: Some("o".into()),
        };
        assert_eq!(bytes(&one), [1, b'x', 1, 1, b'o']);
    }

    #[test]
    fn a_display_slot_is_a_var_int_then_the_objective_name() {
        let packet = SetDisplayObjective {
            slot: 18,
            objective: String::new(),
        };
        assert_eq!(bytes(&packet), [18, 0]);
    }

    #[test]
    fn adding_a_team_writes_parameters_then_members() {
        let packet = SetPlayerTeam {
            name: "t".into(),
            method: TEAM_ADD,
            parameters: Some(TeamParameters {
                display_name: text("T"),
                prefix: text(""),
                suffix: text(""),
                nametag_visibility: 1,
                collision_rule: 2,
                color: Some(12),
                options: 3,
            }),
            players: vec!["a".into()],
        };
        let out = bytes(&packet);
        assert_eq!(&out[..2], &[1, b't']);
        assert_eq!(out[2], TEAM_ADD);
        assert_eq!(&out[out.len() - 8..], &[1, 2, 1, 12, 3, 1, 1, b'a']);
    }

    #[test]
    fn joining_and_leaving_write_only_the_members() {
        let join = SetPlayerTeam {
            name: "t".into(),
            method: TEAM_JOIN,
            parameters: None,
            players: vec!["a".into(), "b".into()],
        };
        assert_eq!(bytes(&join), [1, b't', 3, 2, 1, b'a', 1, b'b']);
        let remove = SetPlayerTeam {
            name: "t".into(),
            method: TEAM_REMOVE,
            parameters: None,
            players: Vec::new(),
        };
        assert_eq!(bytes(&remove), [1, b't', 1]);
    }
}
