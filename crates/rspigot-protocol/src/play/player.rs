use uuid::Uuid;

use crate::Packet;
use crate::codec::{
    Decode, DecodeError, Encode, MAX_LIST_LEN, NetworkNbt, decode_len, decode_string, encode_len,
};
use crate::ids::play::clientbound as id;
use crate::play::chat::ChatSession;
use crate::play::types::{
    Difficulty, GameMode, GlobalPos, OptionalGameMode, PositionMoveRotation, Relatives,
};
use crate::play::wire::{
    Bytes, List, Opt, Plain, UNBOUNDED, Var, read_var, unit_packet, wire_struct, write_var,
};
use crate::types::{ProfileProperty, Text};

pub const MAX_PLAYER_NAME_CHARS: usize = 16;
pub const MAX_PROFILE_PROPERTIES: usize = 16;
pub const MAX_SERVER_ICON_BYTES: usize = 1 << 21;

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct CommonPlayerSpawnInfo {
        pub dimension_type: i32 as Var,
        pub dimension: String,
        pub seed: i64,
        pub game_mode: GameMode,
        pub previous_game_mode: Option<GameMode> as OptionalGameMode,
        pub is_debug: bool,
        pub is_flat: bool,
        pub last_death_location: Option<GlobalPos>,
        pub portal_cooldown: i32 as Var,
        pub sea_level: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Login = id::LOGIN {
        pub player_id: i32,
        pub hardcore: bool,
        pub levels: Vec<String> as List<Plain, UNBOUNDED>,
        pub max_players: i32 as Var,
        pub chunk_radius: i32 as Var,
        pub simulation_distance: i32 as Var,
        pub reduced_debug_info: bool,
        pub show_death_screen: bool,
        pub do_limited_crafting: bool,
        pub spawn_info: CommonPlayerSpawnInfo,
        pub online_mode: bool,
        pub enforces_secure_chat: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Respawn = id::RESPAWN {
        pub spawn_info: CommonPlayerSpawnInfo,
        pub data_to_keep: u8,
    }
}

impl Respawn {
    pub const KEEP_ATTRIBUTE_MODIFIERS: u8 = 0x01;
    pub const KEEP_ENTITY_DATA: u8 = 0x02;
    pub const KEEP_ALL_DATA: u8 = 0x03;
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct PlayerPosition = id::PLAYER_POSITION {
        pub teleport_id: i32 as Var,
        pub change: PositionMoveRotation,
        pub relatives: Relatives,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct PlayerRotation = id::PLAYER_ROTATION {
        pub y_rot: f32,
        pub relative_y: bool,
        pub x_rot: f32,
        pub relative_x: bool,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PlayerAbilities {
    pub invulnerable: bool,
    pub flying: bool,
    pub can_fly: bool,
    pub instabuild: bool,
    pub flying_speed: f32,
    pub walking_speed: f32,
}

impl PlayerAbilities {
    pub const FLAG_INVULNERABLE: u8 = 0x01;
    pub const FLAG_FLYING: u8 = 0x02;
    pub const FLAG_CAN_FLY: u8 = 0x04;
    pub const FLAG_INSTABUILD: u8 = 0x08;

    pub fn flags(&self) -> u8 {
        let mut flags = 0;
        if self.invulnerable {
            flags |= Self::FLAG_INVULNERABLE;
        }
        if self.flying {
            flags |= Self::FLAG_FLYING;
        }
        if self.can_fly {
            flags |= Self::FLAG_CAN_FLY;
        }
        if self.instabuild {
            flags |= Self::FLAG_INSTABUILD;
        }
        flags
    }
}

impl Encode for PlayerAbilities {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.flags());
        self.flying_speed.encode(out);
        self.walking_speed.encode(out);
    }
}

impl Decode for PlayerAbilities {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        let flags = u8::decode(input)?;
        Ok(Self {
            invulnerable: flags & Self::FLAG_INVULNERABLE != 0,
            flying: flags & Self::FLAG_FLYING != 0,
            can_fly: flags & Self::FLAG_CAN_FLY != 0,
            instabuild: flags & Self::FLAG_INSTABUILD != 0,
            flying_speed: f32::decode(input)?,
            walking_speed: f32::decode(input)?,
        })
    }
}

impl Packet for PlayerAbilities {
    const ID: i32 = id::PLAYER_ABILITIES;
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SetHeldSlot = id::SET_HELD_SLOT {
        pub slot: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ChangeDifficulty = id::CHANGE_DIFFICULTY {
        pub difficulty: Difficulty,
        pub locked: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct TagQuery = id::TAG_QUERY {
        pub transaction_id: i32 as Var,
        pub tag: NetworkNbt,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct SetDefaultSpawnPosition = id::SET_DEFAULT_SPAWN_POSITION {
        pub pos: GlobalPos,
        pub yaw: f32,
        pub pitch: f32,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct SetHealth = id::SET_HEALTH {
        pub health: f32,
        pub food: i32 as Var,
        pub saturation: f32,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct SetExperience = id::SET_EXPERIENCE {
        pub progress: f32,
        pub level: i32 as Var,
        pub total: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct Cooldown = id::COOLDOWN {
        pub group: String,
        pub ticks: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct TabList = id::TAB_LIST {
        pub header: Text,
        pub footer: Text,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct ServerData = id::SERVER_DATA {
        pub motd: Text,
        pub icon: Option<Vec<u8>> as Opt<Bytes<MAX_SERVER_ICON_BYTES>>,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Transfer = id::TRANSFER {
        pub host: String,
        pub port: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct TickingState = id::TICKING_STATE {
        pub tick_rate: f32,
        pub is_frozen: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct TickingStep = id::TICKING_STEP {
        pub tick_steps: i32 as Var,
    }
}

unit_packet! {
    pub struct StartConfiguration = id::START_CONFIGURATION;
}

unit_packet! {
    pub struct BundleDelimiter = id::BUNDLE_DELIMITER;
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PlayerInfoRemove = id::PLAYER_INFO_REMOVE {
        pub profile_ids: Vec<Uuid> as List<Plain, UNBOUNDED>,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct PlayerInfoActions(pub u8);

impl PlayerInfoActions {
    pub const ADD_PLAYER: Self = Self(1);
    pub const INITIALIZE_CHAT: Self = Self(1 << 1);
    pub const UPDATE_GAME_MODE: Self = Self(1 << 2);
    pub const UPDATE_LISTED: Self = Self(1 << 3);
    pub const UPDATE_LATENCY: Self = Self(1 << 4);
    pub const UPDATE_DISPLAY_NAME: Self = Self(1 << 5);
    pub const UPDATE_LIST_ORDER: Self = Self(1 << 6);
    pub const UPDATE_HAT: Self = Self(1 << 7);
    pub const ALL: Self = Self(0xFF);

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlayerInfoEntry {
    pub profile_id: Uuid,
    pub name: String,
    pub properties: Vec<ProfileProperty>,
    pub chat_session: Option<ChatSession>,
    pub game_mode: GameMode,
    pub listed: bool,
    pub latency: i32,
    pub display_name: Option<Text>,
    pub list_order: i32,
    pub show_hat: bool,
}

impl PlayerInfoEntry {
    fn write(&self, actions: PlayerInfoActions, out: &mut Vec<u8>) {
        self.profile_id.encode(out);
        if actions.contains(PlayerInfoActions::ADD_PLAYER) {
            self.name.encode(out);
            self.properties.encode(out);
        }
        if actions.contains(PlayerInfoActions::INITIALIZE_CHAT) {
            self.chat_session.encode(out);
        }
        if actions.contains(PlayerInfoActions::UPDATE_GAME_MODE) {
            self.game_mode.encode(out);
        }
        if actions.contains(PlayerInfoActions::UPDATE_LISTED) {
            self.listed.encode(out);
        }
        if actions.contains(PlayerInfoActions::UPDATE_LATENCY) {
            write_var(self.latency, out);
        }
        if actions.contains(PlayerInfoActions::UPDATE_DISPLAY_NAME) {
            self.display_name.encode(out);
        }
        if actions.contains(PlayerInfoActions::UPDATE_LIST_ORDER) {
            write_var(self.list_order, out);
        }
        if actions.contains(PlayerInfoActions::UPDATE_HAT) {
            self.show_hat.encode(out);
        }
    }

    fn read(actions: PlayerInfoActions, input: &mut &[u8]) -> Result<Self, DecodeError> {
        let mut entry = Self {
            profile_id: Uuid::decode(input)?,
            ..Self::default()
        };
        if actions.contains(PlayerInfoActions::ADD_PLAYER) {
            entry.name = decode_string(input, MAX_PLAYER_NAME_CHARS)?;
            let count = decode_len(input, MAX_PROFILE_PROPERTIES)?;
            for _ in 0..count {
                entry.properties.push(ProfileProperty::decode(input)?);
            }
        }
        if actions.contains(PlayerInfoActions::INITIALIZE_CHAT) {
            entry.chat_session = Option::decode(input)?;
        }
        if actions.contains(PlayerInfoActions::UPDATE_GAME_MODE) {
            entry.game_mode = GameMode::decode(input)?;
        }
        if actions.contains(PlayerInfoActions::UPDATE_LISTED) {
            entry.listed = bool::decode(input)?;
        }
        if actions.contains(PlayerInfoActions::UPDATE_LATENCY) {
            entry.latency = read_var(input)?;
        }
        if actions.contains(PlayerInfoActions::UPDATE_DISPLAY_NAME) {
            entry.display_name = Option::decode(input)?;
        }
        if actions.contains(PlayerInfoActions::UPDATE_LIST_ORDER) {
            entry.list_order = read_var(input)?;
        }
        if actions.contains(PlayerInfoActions::UPDATE_HAT) {
            entry.show_hat = bool::decode(input)?;
        }
        Ok(entry)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInfoUpdate {
    pub actions: PlayerInfoActions,
    pub entries: Vec<PlayerInfoEntry>,
}

impl Encode for PlayerInfoUpdate {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.actions.0);
        encode_len(self.entries.len(), out);
        for entry in &self.entries {
            entry.write(self.actions, out);
        }
    }
}

impl Decode for PlayerInfoUpdate {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        let actions = PlayerInfoActions(u8::decode(input)?);
        let count = decode_len(input, MAX_LIST_LEN)?;
        let mut entries = Vec::with_capacity(count.min(input.len()));
        for _ in 0..count {
            entries.push(PlayerInfoEntry::read(actions, input)?);
        }
        Ok(Self { actions, entries })
    }
}

impl Packet for PlayerInfoUpdate {
    const ID: i32 = id::PLAYER_INFO_UPDATE;
}

#[cfg(test)]
mod tests {
    use rspigot_nbt::Tag;

    use super::*;
    use crate::codec::{Position, decode_exact};
    use crate::play::types::Vec3;
    use crate::play::wire::testing::{encoded, roundtrip};

    #[test]
    fn a_transfer_is_a_host_and_a_var_int_port() {
        let packet = Transfer {
            host: "a.b".to_owned(),
            port: 25565,
        };
        assert_eq!(
            encoded(&packet),
            vec![3, b'a', b'.', b'b', 0xdd, 0xc7, 0x01]
        );
    }

    #[test]
    fn a_tag_query_answer_carries_the_transaction_and_a_nameless_compound_or_nothing() {
        let found = TagQuery {
            transaction_id: 3,
            tag: NetworkNbt(Some(Tag::Compound(
                rspigot_nbt::Compound::new().with("n", 5i32),
            ))),
        };
        assert_eq!(roundtrip(&found), [3, 10, 3, 0, 1, b'n', 0, 0, 0, 5, 0]);
        let missing = TagQuery {
            transaction_id: 3,
            tag: NetworkNbt(None),
        };
        assert_eq!(roundtrip(&missing), [3, 0]);
    }

    fn spawn_info() -> CommonPlayerSpawnInfo {
        CommonPlayerSpawnInfo {
            dimension_type: 0,
            dimension: "minecraft:overworld".into(),
            seed: 0x0102030405060708,
            game_mode: GameMode::Creative,
            previous_game_mode: None,
            is_debug: false,
            is_flat: true,
            last_death_location: None,
            portal_cooldown: 0,
            sea_level: 63,
        }
    }

    #[test]
    fn spawn_info_golden() {
        let bytes = roundtrip(&spawn_info());
        let mut expected = vec![0, 19];
        expected.extend_from_slice(b"minecraft:overworld");
        expected.extend([1, 2, 3, 4, 5, 6, 7, 8, 1, 0, 0, 1, 0, 0, 63]);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn login_golden_prefix() {
        let login = Login {
            player_id: 1,
            hardcore: false,
            levels: vec!["minecraft:overworld".into()],
            max_players: 20,
            chunk_radius: 10,
            simulation_distance: 8,
            reduced_debug_info: false,
            show_death_screen: true,
            do_limited_crafting: false,
            spawn_info: CommonPlayerSpawnInfo {
                previous_game_mode: Some(GameMode::Survival),
                last_death_location: Some(GlobalPos {
                    dimension: "minecraft:the_nether".into(),
                    pos: Position { x: 1, y: 2, z: 3 },
                }),
                ..spawn_info()
            },
            online_mode: true,
            enforces_secure_chat: false,
        };
        let bytes = roundtrip(&login);
        assert_eq!(&bytes[..6], &[0, 0, 0, 1, 0, 1]);
        assert_eq!(bytes[6], 19);
        assert_eq!(&bytes[26..32], &[20, 10, 8, 0, 1, 0]);
        assert_eq!(&bytes[bytes.len() - 2..], &[1, 0]);
    }

    #[test]
    fn respawn_and_simple_packets() {
        let respawn = Respawn {
            spawn_info: spawn_info(),
            data_to_keep: Respawn::KEEP_ALL_DATA,
        };
        assert_eq!(*roundtrip(&respawn).last().unwrap(), 3);
        assert_eq!(roundtrip(&SetHeldSlot { slot: 8 }), vec![8]);
        assert_eq!(
            roundtrip(&ChangeDifficulty {
                difficulty: Difficulty::Hard,
                locked: true
            }),
            vec![3, 1]
        );
        assert_eq!(
            roundtrip(&SetHealth {
                health: 20.0,
                food: 20,
                saturation: 5.0
            }),
            vec![0x41, 0xA0, 0, 0, 20, 0x40, 0xA0, 0, 0]
        );
        assert_eq!(
            roundtrip(&Cooldown {
                group: "a:b".into(),
                ticks: 300
            }),
            vec![3, b'a', b':', b'b', 0xAC, 0x02]
        );
        assert_eq!(
            roundtrip(&SetExperience {
                progress: 0.0,
                level: 300,
                total: 1
            }),
            vec![0, 0, 0, 0, 0xAC, 0x02, 1]
        );
        assert_eq!(
            roundtrip(&TickingState {
                tick_rate: 20.0,
                is_frozen: false
            }),
            vec![0x41, 0xA0, 0, 0, 0]
        );
        assert_eq!(roundtrip(&TickingStep { tick_steps: 2 }), vec![2]);
        assert!(roundtrip(&StartConfiguration).is_empty());
        assert!(roundtrip(&BundleDelimiter).is_empty());
    }

    #[test]
    fn position_and_rotation_layout() {
        let packet = PlayerPosition {
            teleport_id: 1,
            change: PositionMoveRotation {
                position: Vec3::new(0.5, 64.0, -0.5),
                delta: Vec3::ZERO,
                y_rot: 90.0,
                x_rot: 0.0,
            },
            relatives: Relatives::DELTA_X
                .union(Relatives::DELTA_Y)
                .union(Relatives::DELTA_Z),
        };
        let bytes = roundtrip(&packet);
        assert_eq!(bytes.len(), 1 + 48 + 8 + 4);
        assert_eq!(&bytes[1..9], &0.5f64.to_be_bytes());
        assert_eq!(&bytes[bytes.len() - 4..], &[0, 0, 0, 0xE0]);
        let rotation = PlayerRotation {
            y_rot: 1.0,
            relative_y: true,
            x_rot: -1.0,
            relative_x: false,
        };
        assert_eq!(
            roundtrip(&rotation),
            vec![0x3F, 0x80, 0, 0, 1, 0xBF, 0x80, 0, 0, 0]
        );
    }

    #[test]
    fn abilities_flags() {
        let abilities = PlayerAbilities {
            invulnerable: true,
            flying: false,
            can_fly: true,
            instabuild: true,
            flying_speed: 0.05,
            walking_speed: 0.1,
        };
        let bytes = roundtrip(&abilities);
        assert_eq!(bytes[0], 0x0D);
        assert_eq!(&bytes[1..5], &0.05f32.to_be_bytes());
    }

    #[test]
    fn spawn_position_and_tab_list() {
        let packet = SetDefaultSpawnPosition {
            pos: GlobalPos {
                dimension: "a:b".into(),
                pos: Position { x: 0, y: 64, z: 0 },
            },
            yaw: 0.0,
            pitch: 0.0,
        };
        assert_eq!(
            roundtrip(&packet),
            vec![
                3, b'a', b':', b'b', 0, 0, 0, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0, 0, 0, 0
            ]
        );
        let tab = TabList {
            header: Text(Tag::String("h".into())),
            footer: Text(Tag::String(String::new())),
        };
        assert_eq!(roundtrip(&tab), vec![8, 0, 1, b'h', 8, 0, 0]);
        let data = ServerData {
            motd: Text(Tag::String("m".into())),
            icon: Some(vec![0x89, 0x50]),
        };
        assert_eq!(roundtrip(&data), vec![8, 0, 1, b'm', 1, 2, 0x89, 0x50]);
    }

    #[test]
    fn player_info_update_writes_only_selected_actions() {
        let entry = PlayerInfoEntry {
            profile_id: Uuid::from_u128(9),
            name: "Steve".into(),
            properties: vec![ProfileProperty {
                name: "textures".into(),
                value: "v".into(),
                signature: None,
            }],
            game_mode: GameMode::Creative,
            listed: true,
            latency: 42,
            ..PlayerInfoEntry::default()
        };
        let actions = PlayerInfoActions::ADD_PLAYER
            .union(PlayerInfoActions::UPDATE_GAME_MODE)
            .union(PlayerInfoActions::UPDATE_LISTED)
            .union(PlayerInfoActions::UPDATE_LATENCY);
        let packet = PlayerInfoUpdate {
            actions,
            entries: vec![entry],
        };
        let bytes = roundtrip(&packet);
        assert_eq!(&bytes[..2], &[0x1D, 1]);
        let after_uuid = 2 + 16;
        assert_eq!(
            &bytes[after_uuid..after_uuid + 6],
            &[5, b'S', b't', b'e', b'v', b'e']
        );
        let tail = &bytes[bytes.len() - 3..];
        assert_eq!(tail, &[1, 1, 42]);
        let full = PlayerInfoUpdate {
            actions: PlayerInfoActions::ALL,
            entries: vec![PlayerInfoEntry {
                profile_id: Uuid::from_u128(1),
                name: "Alex".into(),
                chat_session: Some(ChatSession {
                    session_id: Uuid::from_u128(2),
                    expires_at: 3,
                    public_key: vec![1],
                    key_signature: vec![2],
                }),
                display_name: Some(Text(Tag::String("A".into()))),
                list_order: 5,
                show_hat: true,
                ..PlayerInfoEntry::default()
            }],
        };
        let bytes = roundtrip(&full);
        assert_eq!(&bytes[bytes.len() - 2..], &[5, 1]);
        let removed = PlayerInfoRemove {
            profile_ids: vec![Uuid::from_u128(1)],
        };
        assert_eq!(encoded(&removed).len(), 17);
        assert!(decode_exact::<PlayerInfoUpdate>(&[0x01, 1]).is_err());
    }

    #[test]
    fn initialize_chat_action_golden() {
        let init = |session: Option<ChatSession>| PlayerInfoUpdate {
            actions: PlayerInfoActions::INITIALIZE_CHAT,
            entries: vec![PlayerInfoEntry {
                profile_id: Uuid::from_u128(1),
                chat_session: session,
                ..PlayerInfoEntry::default()
            }],
        };
        let with = init(Some(ChatSession {
            session_id: Uuid::from_u128(2),
            expires_at: 3,
            public_key: vec![9, 8],
            key_signature: vec![7],
        }));
        let mut expected = vec![0x02, 1];
        expected.extend(Uuid::from_u128(1).into_bytes());
        expected.push(1);
        expected.extend(Uuid::from_u128(2).into_bytes());
        expected.extend(3i64.to_be_bytes());
        expected.extend([2, 9, 8, 1, 7]);
        assert_eq!(roundtrip(&with), expected);
        let mut empty = vec![0x02, 1];
        empty.extend(Uuid::from_u128(1).into_bytes());
        empty.push(0);
        assert_eq!(roundtrip(&init(None)), empty);
    }
}
