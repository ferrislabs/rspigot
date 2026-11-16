use rspigot_nbt::Tag;

use crate::Packet;
use crate::codec::{
    Decode, DecodeError, Encode, MAX_LIST_LEN, NetworkNbt, Position, VarLong, decode_len,
    encode_len,
};
use crate::ids::play::clientbound as id;
use crate::play::types::{Holder, SectionPos, SoundEvent, SoundSource};
use crate::play::wire::{
    List, Plain, UNBOUNDED, Var, VarL, read_var, unit_packet, wire_enum, wire_struct, write_var,
};

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SetChunkCacheCenter = id::SET_CHUNK_CACHE_CENTER {
        pub x: i32 as Var,
        pub z: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SetChunkCacheRadius = id::SET_CHUNK_CACHE_RADIUS {
        pub radius: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SetSimulationDistance = id::SET_SIMULATION_DISTANCE {
        pub simulation_distance: i32 as Var,
    }
}

unit_packet! {
    pub struct ChunkBatchStart = id::CHUNK_BATCH_START;
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ChunkBatchFinished = id::CHUNK_BATCH_FINISHED {
        pub batch_size: i32 as Var,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameEventKind {
    NoRespawnBlockAvailable,
    StartRaining,
    StopRaining,
    ChangeGameMode,
    WinGame,
    DemoEvent,
    PlayArrowHitSound,
    RainLevelChange,
    ThunderLevelChange,
    PufferFishSting,
    GuardianElderEffect,
    ImmediateRespawn,
    LimitedCrafting,
    LevelChunksLoadStart,
}

impl GameEventKind {
    pub const ALL: [Self; 14] = [
        Self::NoRespawnBlockAvailable,
        Self::StartRaining,
        Self::StopRaining,
        Self::ChangeGameMode,
        Self::WinGame,
        Self::DemoEvent,
        Self::PlayArrowHitSound,
        Self::RainLevelChange,
        Self::ThunderLevelChange,
        Self::PufferFishSting,
        Self::GuardianElderEffect,
        Self::ImmediateRespawn,
        Self::LimitedCrafting,
        Self::LevelChunksLoadStart,
    ];

    pub const fn id(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GameEvent {
    pub event: GameEventKind,
    pub param: f32,
}

impl Encode for GameEvent {
    fn encode(&self, out: &mut Vec<u8>) {
        out.push(self.event.id());
        self.param.encode(out);
    }
}

impl Decode for GameEvent {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        let raw = u8::decode(input)?;
        let event =
            GameEventKind::ALL
                .get(usize::from(raw))
                .copied()
                .ok_or(DecodeError::InvalidValue {
                    field: "game event",
                    value: i64::from(raw),
                })?;
        Ok(Self {
            event,
            param: f32::decode(input)?,
        })
    }
}

impl Packet for GameEvent {
    const ID: i32 = id::GAME_EVENT;
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct BlockUpdate = id::BLOCK_UPDATE {
        pub pos: Position,
        pub state: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct BlockEvent = id::BLOCK_EVENT {
        pub pos: Position,
        pub action: u8,
        pub data: u8,
        pub block: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct BlockChangedAck = id::BLOCK_CHANGED_ACK {
        pub sequence: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct BlockDestruction = id::BLOCK_DESTRUCTION {
        pub id: i32 as Var,
        pub pos: Position,
        pub progress: u8,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionBlockChange {
    pub local: u16,
    pub state: i32,
}

impl SectionBlockChange {
    pub const fn new(x: u8, y: u8, z: u8, state: i32) -> Self {
        Self {
            local: ((x as u16 & 15) << 8) | ((z as u16 & 15) << 4) | (y as u16 & 15),
            state,
        }
    }

    pub const fn pack(self) -> i64 {
        ((self.state as i64) << 12) | (self.local as i64 & 0xFFF)
    }

    pub const fn unpack(packed: i64) -> Self {
        Self {
            local: (packed & 0xFFF) as u16,
            state: (packed as u64 >> 12) as i32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionBlocksUpdate {
    pub section: SectionPos,
    pub changes: Vec<SectionBlockChange>,
}

impl Encode for SectionBlocksUpdate {
    fn encode(&self, out: &mut Vec<u8>) {
        self.section.encode(out);
        encode_len(self.changes.len(), out);
        for change in &self.changes {
            VarLong(change.pack()).encode(out);
        }
    }
}

impl Decode for SectionBlocksUpdate {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        let section = SectionPos::decode(input)?;
        let count = decode_len(input, MAX_LIST_LEN)?;
        let mut changes = Vec::with_capacity(count.min(input.len()));
        for _ in 0..count {
            changes.push(SectionBlockChange::unpack(VarLong::decode(input)?.0));
        }
        Ok(Self { section, changes })
    }
}

impl Packet for SectionBlocksUpdate {
    const ID: i32 = id::SECTION_BLOCKS_UPDATE;
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockEntityData {
    pub pos: Position,
    pub kind: i32,
    pub data: Tag,
}

impl Encode for BlockEntityData {
    fn encode(&self, out: &mut Vec<u8>) {
        self.pos.encode(out);
        write_var(self.kind, out);
        rspigot_nbt::write_network(out, &self.data);
    }
}

impl Decode for BlockEntityData {
    fn decode(input: &mut &[u8]) -> Result<Self, DecodeError> {
        let pos = Position::decode(input)?;
        let kind = read_var(input)?;
        match NetworkNbt::decode(input)?.0 {
            Some(data @ Tag::Compound(_)) => Ok(Self { pos, kind, data }),
            other => Err(DecodeError::InvalidValue {
                field: "block entity data tag",
                value: other.map_or(0, |tag| i64::from(tag.tag_type() as u8)),
            }),
        }
    }
}

impl Packet for BlockEntityData {
    const ID: i32 = id::BLOCK_ENTITY_DATA;
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct OpenSignEditor = id::OPEN_SIGN_EDITOR {
        pub pos: Position,
        pub front: bool,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct ClockState {
        pub total_ticks: i64 as VarL,
        pub partial_tick: f32,
        pub rate: f32,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct ClockUpdate {
        pub clock: i32 as Var,
        pub state: ClockState,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct SetTime = id::SET_TIME {
        pub game_time: i64,
        pub clocks: Vec<ClockUpdate> as List<Plain, UNBOUNDED>,
    }
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct InitializeBorder = id::INITIALIZE_BORDER {
        pub center_x: f64,
        pub center_z: f64,
        pub old_size: f64,
        pub new_size: f64,
        pub lerp_time: i64 as VarL,
        pub absolute_max_size: i32 as Var,
        pub warning_blocks: i32 as Var,
        pub warning_time: i32 as Var,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct GameRuleValue {
        pub rule: String,
        pub value: String,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct GameRuleValues = id::GAME_RULE_VALUES {
        pub values: Vec<GameRuleValue> as List<Plain, UNBOUNDED>,
    }
}

wire_struct! {
    #[derive(Debug, Clone, PartialEq)]
    pub struct Sound = id::SOUND {
        pub sound: Holder<SoundEvent>,
        pub source: SoundSource,
        pub x: i32,
        pub y: i32,
        pub z: i32,
        pub volume: f32,
        pub pitch: f32,
        pub seed: i64,
    }
}

impl Sound {
    pub const POSITION_SCALE: f64 = 8.0;

    pub fn at(
        sound: Holder<SoundEvent>,
        source: SoundSource,
        position: (f64, f64, f64),
        volume: f32,
        pitch: f32,
        seed: i64,
    ) -> Self {
        Self {
            sound,
            source,
            x: (position.0 * Self::POSITION_SCALE) as i32,
            y: (position.1 * Self::POSITION_SCALE) as i32,
            z: (position.2 * Self::POSITION_SCALE) as i32,
            volume,
            pitch,
            seed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopSound {
    pub source: Option<SoundSource>,
    pub sound: Option<String>,
}

impl Encode for StopSound {
    fn encode(&self, out: &mut Vec<u8>) {
        let flags = u8::from(self.source.is_some()) | (u8::from(self.sound.is_some()) << 1);
        out.push(flags);
        if let Some(source) = &self.source {
            source.encode(out);
        }
        if let Some(sound) = &self.sound {
            sound.encode(out);
        }
    }
}

impl Packet for StopSound {
    const ID: i32 = id::STOP_SOUND;
}

wire_struct! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct LevelEvent = id::LEVEL_EVENT {
        pub kind: i32,
        pub pos: Position,
        pub data: i32,
        pub global: bool,
    }
}

wire_enum! {
    pub enum ParticleRandomization: Zero { Default, Alternative, AlternativeWithSpeed }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticleOptions {
    pub kind: i32,
    pub data: Vec<u8>,
}

impl Encode for ParticleOptions {
    fn encode(&self, out: &mut Vec<u8>) {
        write_var(self.kind, out);
        out.extend_from_slice(&self.data);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LevelParticles {
    pub particle: ParticleOptions,
    pub override_limiter: bool,
    pub always_show: bool,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub x_dist: f32,
    pub y_dist: f32,
    pub z_dist: f32,
    pub x_max_speed: f32,
    pub y_max_speed: f32,
    pub z_max_speed: f32,
    pub count: i32,
    pub randomization: ParticleRandomization,
}

impl Encode for LevelParticles {
    fn encode(&self, out: &mut Vec<u8>) {
        self.particle.encode(out);
        self.override_limiter.encode(out);
        self.always_show.encode(out);
        self.x.encode(out);
        self.y.encode(out);
        self.z.encode(out);
        self.x_dist.encode(out);
        self.y_dist.encode(out);
        self.z_dist.encode(out);
        self.x_max_speed.encode(out);
        self.y_max_speed.encode(out);
        self.z_max_speed.encode(out);
        write_var(self.count, out);
        self.randomization.encode(out);
    }
}

impl Packet for LevelParticles {
    const ID: i32 = id::LEVEL_PARTICLES;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExplosionParticle {
    pub particle: ParticleOptions,
    pub scaling: f32,
    pub speed: f32,
    pub weight: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Explode {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub radius: f32,
    pub block_count: i32,
    pub knockback: Option<(f64, f64, f64)>,
    pub particle: ParticleOptions,
    pub sound: Holder<SoundEvent>,
    pub block_particles: Vec<ExplosionParticle>,
    pub play_sound: bool,
}

impl Encode for Explode {
    fn encode(&self, out: &mut Vec<u8>) {
        self.x.encode(out);
        self.y.encode(out);
        self.z.encode(out);
        self.radius.encode(out);
        self.block_count.encode(out);
        match self.knockback {
            Some((x, y, z)) => {
                true.encode(out);
                x.encode(out);
                y.encode(out);
                z.encode(out);
            }
            None => false.encode(out),
        }
        self.particle.encode(out);
        self.sound.encode(out);
        write_var(self.block_particles.len() as i32, out);
        for entry in &self.block_particles {
            entry.particle.encode(out);
            entry.scaling.encode(out);
            entry.speed.encode(out);
            write_var(entry.weight, out);
        }
        self.play_sound.encode(out);
    }
}

impl Packet for Explode {
    const ID: i32 = id::EXPLODE;
}

#[cfg(test)]
mod tests {
    use rspigot_nbt::Compound;

    use super::*;
    use crate::codec::decode_exact;
    use crate::play::wire::testing::{encoded, roundtrip};

    #[test]
    fn stop_sound_flags_tell_which_fields_follow() {
        let none = StopSound {
            source: None,
            sound: None,
        };
        assert_eq!(encoded(&none), vec![0]);
        let both = StopSound {
            source: Some(SoundSource::Records),
            sound: Some("a:b".to_owned()),
        };
        assert_eq!(encoded(&both), vec![3, 2, 3, b'a', b':', b'b']);
        let named = StopSound {
            source: None,
            sound: Some("a".to_owned()),
        };
        assert_eq!(encoded(&named), vec![2, 1, b'a']);
    }

    #[test]
    fn open_sign_editor_is_a_position_and_a_side() {
        let packet = OpenSignEditor {
            pos: Position { x: 1, y: 64, z: 2 },
            front: true,
        };
        let bytes = roundtrip(&packet);
        assert_eq!(bytes.len(), 9);
        assert_eq!(bytes[8], 1);
    }

    #[test]
    fn explode_matches_the_26_3_stream_codec() {
        let packet = Explode {
            x: 0.5,
            y: 64.0,
            z: -1.5,
            radius: 3.0,
            block_count: 7,
            knockback: Some((0.25, 0.5, 0.0)),
            particle: ParticleOptions {
                kind: 23,
                data: Vec::new(),
            },
            sound: Holder::Reference(511),
            block_particles: vec![ExplosionParticle {
                particle: ParticleOptions {
                    kind: 66,
                    data: Vec::new(),
                },
                scaling: 0.5,
                speed: 1.0,
                weight: 1,
            }],
            play_sound: true,
        };
        let mut expected = Vec::new();
        expected.extend(0.5f64.to_be_bytes());
        expected.extend(64.0f64.to_be_bytes());
        expected.extend((-1.5f64).to_be_bytes());
        expected.extend(3.0f32.to_be_bytes());
        expected.extend(7i32.to_be_bytes());
        expected.push(1);
        expected.extend(0.25f64.to_be_bytes());
        expected.extend(0.5f64.to_be_bytes());
        expected.extend(0.0f64.to_be_bytes());
        expected.push(23);
        expected.extend([0x80, 0x04]);
        expected.push(1);
        expected.push(66);
        expected.extend(0.5f32.to_be_bytes());
        expected.extend(1.0f32.to_be_bytes());
        expected.push(1);
        expected.push(1);
        assert_eq!(encoded(&packet), expected);
    }

    #[test]
    fn chunk_cache_and_batches() {
        assert_eq!(
            roundtrip(&SetChunkCacheCenter { x: -1, z: 2 }),
            vec![0xFF, 0xFF, 0xFF, 0xFF, 0x0F, 2]
        );
        assert_eq!(roundtrip(&SetChunkCacheRadius { radius: 12 }), vec![12]);
        assert_eq!(
            roundtrip(&SetSimulationDistance {
                simulation_distance: 10
            }),
            vec![10]
        );
        assert!(roundtrip(&ChunkBatchStart).is_empty());
        assert_eq!(roundtrip(&ChunkBatchFinished { batch_size: 25 }), vec![25]);
    }

    #[test]
    fn game_event_is_unsigned_byte_and_float() {
        let packet = GameEvent {
            event: GameEventKind::LevelChunksLoadStart,
            param: 0.0,
        };
        assert_eq!(roundtrip(&packet), vec![13, 0, 0, 0, 0]);
        let mode = GameEvent {
            event: GameEventKind::ChangeGameMode,
            param: 1.0,
        };
        assert_eq!(roundtrip(&mode), vec![3, 0x3F, 0x80, 0, 0]);
        assert!(decode_exact::<GameEvent>(&[14, 0, 0, 0, 0]).is_err());
    }

    #[test]
    fn block_packets() {
        let pos = Position { x: 1, y: 64, z: -1 };
        let bytes = roundtrip(&BlockUpdate { pos, state: 9 });
        assert_eq!(bytes.len(), 9);
        assert_eq!(bytes[8], 9);
        assert_eq!(
            roundtrip(&BlockChangedAck { sequence: 300 }),
            vec![0xAC, 0x02]
        );
        let bytes = roundtrip(&BlockDestruction {
            id: 1,
            pos,
            progress: 9,
        });
        assert_eq!((bytes[0], bytes[9]), (1, 9));
        let change = SectionBlockChange::new(1, 2, 3, 9);
        assert_eq!(change.local, 0x132);
        let packet = SectionBlocksUpdate {
            section: SectionPos { x: 0, y: 4, z: 0 },
            changes: vec![change],
        };
        assert_eq!(
            roundtrip(&packet),
            vec![0, 0, 0, 0, 0, 0, 0, 4, 1, 0xB2, 0xA2, 0x02]
        );
        let entity = BlockEntityData {
            pos,
            kind: 7,
            data: Tag::Compound(Compound::new()),
        };
        let bytes = roundtrip(&entity);
        assert_eq!(&bytes[8..], &[7, 0x0A, 0x00]);
        assert!(decode_exact::<BlockEntityData>(&[0, 0, 0, 0, 0, 0, 0, 0, 7, 0]).is_err());
    }

    #[test]
    fn set_time_with_world_clocks() {
        let packet = SetTime {
            game_time: 100,
            clocks: vec![ClockUpdate {
                clock: 0,
                state: ClockState {
                    total_ticks: 6000,
                    partial_tick: 0.0,
                    rate: 1.0,
                },
            }],
        };
        assert_eq!(
            roundtrip(&packet),
            vec![
                0, 0, 0, 0, 0, 0, 0, 100, 1, 0, 0xF0, 0x2E, 0, 0, 0, 0, 0x3F, 0x80, 0, 0
            ]
        );
    }

    #[test]
    fn border_rules_sound_and_events() {
        let border = InitializeBorder {
            center_x: 0.0,
            center_z: 0.0,
            old_size: 1.0,
            new_size: 1.0,
            lerp_time: 0,
            absolute_max_size: 29_999_984,
            warning_blocks: 5,
            warning_time: 15,
        };
        let bytes = roundtrip(&border);
        assert_eq!(bytes.len(), 32 + 1 + 4 + 1 + 1);
        let rules = GameRuleValues {
            values: vec![GameRuleValue {
                rule: "minecraft:keep_inventory".into(),
                value: "true".into(),
            }],
        };
        assert_eq!(roundtrip(&rules)[..2], [1, 24]);
        let sound = Sound::at(
            Holder::Reference(9),
            SoundSource::Players,
            (1.0, 2.5, -1.0),
            1.0,
            1.0,
            42,
        );
        let bytes = roundtrip(&sound);
        assert_eq!(
            &bytes[..14],
            &[10, 7, 0, 0, 0, 8, 0, 0, 0, 20, 0xFF, 0xFF, 0xFF, 0xF8]
        );
        let event = LevelEvent {
            kind: 2001,
            pos: Position { x: 0, y: 0, z: 0 },
            data: 1,
            global: false,
        };
        assert_eq!(
            roundtrip(&event),
            vec![0, 0, 0x07, 0xD1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0]
        );
    }

    #[test]
    fn particles_put_options_first() {
        let packet = LevelParticles {
            particle: ParticleOptions {
                kind: 1,
                data: vec![0x09],
            },
            override_limiter: false,
            always_show: true,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            x_dist: 0.0,
            y_dist: 0.0,
            z_dist: 0.0,
            x_max_speed: 0.0,
            y_max_speed: 0.0,
            z_max_speed: 0.0,
            count: 300,
            randomization: ParticleRandomization::AlternativeWithSpeed,
        };
        let bytes = encoded(&packet);
        assert_eq!(&bytes[..4], &[1, 0x09, 0, 1]);
        assert_eq!(bytes.len(), 4 + 24 + 24 + 2 + 1);
        assert_eq!(&bytes[bytes.len() - 3..], &[0xAC, 0x02, 2]);
    }
}
