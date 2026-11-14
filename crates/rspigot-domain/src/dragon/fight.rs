use std::f64::consts::PI;

use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use crate::BlockPos;
use crate::entity::save::{read_uuid, uuid_tag};
use crate::random::{LegacyRandom, RandomSource};

use super::spikes::EndSpike;

pub const FIGHT_FILE: &str = "ender_dragon_fight";
pub const GATEWAY_COUNT: i32 = 20;
pub const GATEWAY_RING: f64 = 96.0;
pub const GATEWAY_HEIGHT: i32 = 75;
pub const GATEWAY_STEP: f64 = 0.157_079_632_679_489_66;
pub const GATEWAY_EVENT: i32 = 3000;
pub const RESPAWN_EVENT: i32 = 3001;
pub const RESPAWN_TARGET: BlockPos = BlockPos::new(0, 128, 0);
pub const PLAYER_SCAN_TICKS: i32 = 21;
pub const DRAGON_SEEN_TICKS: i32 = 1200;
pub const CRYSTAL_SCAN_TICKS: i32 = 100;
pub const PLAYER_RANGE: f64 = 192.0;
pub const PLAYER_RANGE_Y: i32 = 128;
pub const TICKET_RADIUS: i32 = 9;
pub const SPAWN_HEIGHT: i32 = 128;
pub const SEARCH_CHUNKS: i32 = 8;
pub const PILLAR_WINDOW: i32 = 40;
pub const PILLAR_CLEAR: i32 = 10;
pub const PILLAR_BLAST: f32 = 5.0;
pub const CRYSTAL_BLAST: f32 = 6.0;
pub const CRYSTAL_DAMAGE_RADIUS: f32 = 6.0;
pub const NAME_KEY: &str = "entity.minecraft.ender_dragon";
pub const PORTAL_Y_FLOOR: i32 = 64;
pub const CRYSTAL_RING: i32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RespawnStage {
    Start,
    PreparingToSummonPillars,
    SummoningPillars,
    SummoningDragon,
    End,
}

impl RespawnStage {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::PreparingToSummonPillars => "preparing_to_summon_pillars",
            Self::SummoningPillars => "summoning_pillars",
            Self::SummoningDragon => "summoning_dragon",
            Self::End => "end",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [
            Self::Start,
            Self::PreparingToSummonPillars,
            Self::SummoningPillars,
            Self::SummoningDragon,
            Self::End,
        ]
        .into_iter()
        .find(|stage| stage.name() == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RespawnStep {
    BeamAll(Option<BlockPos>),
    Event(i32),
    Advance(RespawnStage),
    BeamPillar(usize),
    BlastPillar(usize),
    Summon,
}

pub fn respawn_steps(stage: RespawnStage, time: i32, spikes: usize) -> Vec<RespawnStep> {
    match stage {
        RespawnStage::Start => vec![
            RespawnStep::BeamAll(Some(RESPAWN_TARGET)),
            RespawnStep::Advance(RespawnStage::PreparingToSummonPillars),
        ],
        RespawnStage::PreparingToSummonPillars => {
            if time >= 100 {
                vec![RespawnStep::Advance(RespawnStage::SummoningPillars)]
            } else if matches!(time, 0 | 50 | 51 | 52) || time >= 95 {
                vec![RespawnStep::Event(RESPAWN_EVENT)]
            } else {
                Vec::new()
            }
        }
        RespawnStage::SummoningPillars => {
            let start = time % PILLAR_WINDOW == 0;
            if !start && time % PILLAR_WINDOW != PILLAR_WINDOW - 1 {
                return Vec::new();
            }
            let index = (time / PILLAR_WINDOW) as usize;
            if index < spikes {
                vec![if start {
                    RespawnStep::BeamPillar(index)
                } else {
                    RespawnStep::BlastPillar(index)
                }]
            } else if start {
                vec![RespawnStep::Advance(RespawnStage::SummoningDragon)]
            } else {
                Vec::new()
            }
        }
        RespawnStage::SummoningDragon => {
            if time >= 100 {
                vec![RespawnStep::Summon]
            } else if time < 80 {
                if time == 0 {
                    vec![RespawnStep::BeamAll(Some(RESPAWN_TARGET))]
                } else if time < 5 {
                    vec![RespawnStep::Event(RESPAWN_EVENT)]
                } else {
                    Vec::new()
                }
            } else {
                vec![RespawnStep::Event(RESPAWN_EVENT)]
            }
        }
        RespawnStage::End => Vec::new(),
    }
}

pub fn gateway_position(index: i32) -> BlockPos {
    let angle = (f64::from(index) * GATEWAY_STEP + -PI) * 2.0;
    BlockPos::new(
        (angle.cos() * GATEWAY_RING).floor() as i32,
        GATEWAY_HEIGHT,
        (angle.sin() * GATEWAY_RING).floor() as i32,
    )
}

pub fn shuffled_gateways(seed: i64) -> Vec<i32> {
    let mut random = LegacyRandom::new(seed);
    let mut list: Vec<i32> = (0..GATEWAY_COUNT).collect();
    let mut remaining = list.len();
    while remaining > 1 {
        let swap = random.next_int(remaining as i32) as usize;
        list.swap(remaining - 1, swap);
        remaining -= 1;
    }
    list
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayBlock {
    Gateway,
    Bedrock,
    Air,
}

pub fn gateway_blocks(center: BlockPos) -> Vec<(BlockPos, GatewayBlock)> {
    let mut out = Vec::new();
    for z in center.z - 1..=center.z + 1 {
        for y in center.y - 2..=center.y + 2 {
            for x in center.x - 1..=center.x + 1 {
                let same_x = x == center.x;
                let same_y = y == center.y;
                let same_z = z == center.z;
                let edge = (y - center.y).abs() == 2;
                let block = if same_x && same_y && same_z {
                    GatewayBlock::Gateway
                } else if same_y {
                    GatewayBlock::Air
                } else if (edge && same_x && same_z) || ((same_x || same_z) && !edge) {
                    GatewayBlock::Bedrock
                } else {
                    GatewayBlock::Air
                };
                out.push((BlockPos::new(x, y, z), block));
            }
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct FightData {
    pub needs_state_scanning: bool,
    pub dragon_killed: bool,
    pub previously_killed: bool,
    pub respawn_stage: Option<RespawnStage>,
    pub respawn_time: i32,
    pub dragon_uuid: Option<Uuid>,
    pub exit_portal_location: Option<BlockPos>,
    pub gateways: Vec<i32>,
    pub respawn_crystals: Vec<Uuid>,
}

impl Default for FightData {
    fn default() -> Self {
        Self {
            needs_state_scanning: true,
            dragon_killed: false,
            previously_killed: false,
            respawn_stage: None,
            respawn_time: 0,
            dragon_uuid: None,
            exit_portal_location: None,
            gateways: Vec::new(),
            respawn_crystals: Vec::new(),
        }
    }
}

impl FightData {
    pub fn to_nbt(&self) -> Compound {
        let mut out = Compound::new();
        out.insert("needs_state_scanning", self.needs_state_scanning);
        out.insert("dragon_killed", self.dragon_killed);
        out.insert("previously_killed", self.previously_killed);
        if let Some(stage) = self.respawn_stage {
            out.insert("respawn_stage", stage.name());
        }
        out.insert("respawn_time", self.respawn_time);
        if let Some(uuid) = self.dragon_uuid {
            out.insert("dragon_uuid", uuid_tag(uuid));
        }
        if let Some(pos) = self.exit_portal_location {
            out.insert("exit_portal_location", vec![pos.x, pos.y, pos.z]);
        }
        out.insert(
            "gateways",
            Tag::List(self.gateways.iter().map(|index| Tag::Int(*index)).collect()),
        );
        if !self.respawn_crystals.is_empty() {
            out.insert(
                "respawn_crystals",
                Tag::List(
                    self.respawn_crystals
                        .iter()
                        .map(|id| uuid_tag(*id))
                        .collect(),
                ),
            );
        }
        out
    }

    pub fn from_nbt(nbt: &Compound) -> Self {
        let defaults = Self::default();
        Self {
            needs_state_scanning: nbt
                .get_bool("needs_state_scanning")
                .unwrap_or(defaults.needs_state_scanning),
            dragon_killed: nbt.get_bool("dragon_killed").unwrap_or(false),
            previously_killed: nbt.get_bool("previously_killed").unwrap_or(false),
            respawn_stage: nbt
                .get_str("respawn_stage")
                .and_then(RespawnStage::from_name),
            respawn_time: nbt.get_i32("respawn_time").unwrap_or(0),
            dragon_uuid: nbt.get("dragon_uuid").and_then(read_uuid),
            exit_portal_location: nbt
                .get_int_array("exit_portal_location")
                .filter(|parts| parts.len() == 3)
                .map(|parts| BlockPos::new(parts[0], parts[1], parts[2])),
            gateways: nbt
                .get_list("gateways")
                .map(|list| {
                    list.iter()
                        .filter_map(|tag| tag.as_i64().map(|value| value as i32))
                        .collect()
                })
                .or_else(|| nbt.get_int_array("gateways").map(<[i32]>::to_vec))
                .unwrap_or_default(),
            respawn_crystals: nbt
                .get_list("respawn_crystals")
                .map(|list| list.iter().filter_map(read_uuid).collect())
                .unwrap_or_default(),
        }
    }
}

pub fn alive_crystals_in<'a>(
    spikes: impl IntoIterator<Item = &'a EndSpike>,
    mut count_in: impl FnMut(&EndSpike) -> usize,
) -> i32 {
    spikes.into_iter().map(|spike| count_in(spike) as i32).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_round_trip_through_their_serialized_names() {
        for stage in [
            RespawnStage::Start,
            RespawnStage::PreparingToSummonPillars,
            RespawnStage::SummoningPillars,
            RespawnStage::SummoningDragon,
            RespawnStage::End,
        ] {
            assert_eq!(RespawnStage::from_name(stage.name()), Some(stage));
        }
        assert_eq!(RespawnStage::from_name("nope"), None);
    }

    #[test]
    fn the_start_stage_aims_every_crystal_at_the_portal_then_prepares() {
        assert_eq!(
            respawn_steps(RespawnStage::Start, 0, 10),
            vec![
                RespawnStep::BeamAll(Some(BlockPos::new(0, 128, 0))),
                RespawnStep::Advance(RespawnStage::PreparingToSummonPillars)
            ]
        );
    }

    #[test]
    fn preparing_pulses_the_effect_at_the_vanilla_ticks_then_hands_over_at_one_hundred() {
        let pulses: Vec<i32> = (0..100)
            .filter(|time| {
                !respawn_steps(RespawnStage::PreparingToSummonPillars, *time, 10).is_empty()
            })
            .collect();
        assert_eq!(pulses, vec![0, 50, 51, 52, 95, 96, 97, 98, 99]);
        assert_eq!(
            respawn_steps(RespawnStage::PreparingToSummonPillars, 100, 10),
            vec![RespawnStep::Advance(RespawnStage::SummoningPillars)]
        );
    }

    #[test]
    fn each_pillar_gets_the_beam_at_the_start_of_its_window_and_the_blast_at_its_end() {
        assert_eq!(
            respawn_steps(RespawnStage::SummoningPillars, 0, 10),
            vec![RespawnStep::BeamPillar(0)]
        );
        assert_eq!(
            respawn_steps(RespawnStage::SummoningPillars, 39, 10),
            vec![RespawnStep::BlastPillar(0)]
        );
        assert_eq!(
            respawn_steps(RespawnStage::SummoningPillars, 40, 10),
            vec![RespawnStep::BeamPillar(1)]
        );
        assert!(respawn_steps(RespawnStage::SummoningPillars, 20, 10).is_empty());
        assert_eq!(
            respawn_steps(RespawnStage::SummoningPillars, 399, 10),
            vec![RespawnStep::BlastPillar(9)]
        );
    }

    #[test]
    fn after_the_last_pillar_the_dragon_summoning_begins() {
        assert_eq!(
            respawn_steps(RespawnStage::SummoningPillars, 400, 10),
            vec![RespawnStep::Advance(RespawnStage::SummoningDragon)]
        );
        assert!(respawn_steps(RespawnStage::SummoningPillars, 439, 10).is_empty());
    }

    #[test]
    fn the_summoning_aims_at_tick_zero_pulses_early_and_late_and_summons_at_one_hundred() {
        assert_eq!(
            respawn_steps(RespawnStage::SummoningDragon, 0, 10),
            vec![RespawnStep::BeamAll(Some(BlockPos::new(0, 128, 0)))]
        );
        assert_eq!(
            respawn_steps(RespawnStage::SummoningDragon, 4, 10),
            vec![RespawnStep::Event(3001)]
        );
        assert!(respawn_steps(RespawnStage::SummoningDragon, 5, 10).is_empty());
        assert!(respawn_steps(RespawnStage::SummoningDragon, 79, 10).is_empty());
        assert_eq!(
            respawn_steps(RespawnStage::SummoningDragon, 80, 10),
            vec![RespawnStep::Event(3001)]
        );
        assert_eq!(
            respawn_steps(RespawnStage::SummoningDragon, 100, 10),
            vec![RespawnStep::Summon]
        );
        assert!(respawn_steps(RespawnStage::End, 5, 10).is_empty());
    }

    #[test]
    fn gateways_sit_on_a_ring_of_ninety_six_blocks_at_height_seventy_five() {
        let first = gateway_position(0);
        assert_eq!(first, BlockPos::new(96, 75, 0));
        for index in 0..20 {
            let pos = gateway_position(index);
            let radius = f64::from(pos.x).hypot(f64::from(pos.z));
            assert!((radius - 96.0).abs() < 1.5);
            assert_eq!(pos.y, 75);
        }
    }

    #[test]
    fn the_gateway_order_is_a_seeded_shuffle_of_zero_to_nineteen() {
        let order = shuffled_gateways(42);
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, (0..20).collect::<Vec<_>>());
        assert_eq!(order, shuffled_gateways(42));
        assert_ne!(order, shuffled_gateways(43));
    }

    #[test]
    fn a_gateway_is_a_frame_of_bedrock_around_one_gateway_block() {
        let blocks = gateway_blocks(BlockPos::new(0, 75, 96));
        let at = |x: i32, y: i32, z: i32| {
            blocks
                .iter()
                .find(|(pos, _)| *pos == BlockPos::new(x, y, z))
                .unwrap()
                .1
        };
        assert_eq!(at(0, 75, 96), GatewayBlock::Gateway);
        assert_eq!(at(1, 75, 96), GatewayBlock::Air);
        assert_eq!(at(0, 77, 96), GatewayBlock::Bedrock);
        assert_eq!(at(0, 73, 96), GatewayBlock::Bedrock);
        assert_eq!(at(1, 76, 96), GatewayBlock::Bedrock);
        assert_eq!(at(0, 76, 95), GatewayBlock::Bedrock);
        assert_eq!(at(1, 76, 95), GatewayBlock::Air);
        assert_eq!(at(1, 77, 96), GatewayBlock::Air);
    }

    #[test]
    fn a_default_fight_needs_scanning_and_has_nothing_else() {
        let data = FightData::default();
        assert!(data.needs_state_scanning);
        assert!(!data.dragon_killed && !data.previously_killed);
        assert_eq!(data.respawn_stage, None);
        assert_eq!(data.respawn_time, 0);
        assert!(data.gateways.is_empty() && data.respawn_crystals.is_empty());
    }

    #[test]
    fn the_fight_round_trips_in_the_vanilla_layout() {
        let data = FightData {
            needs_state_scanning: false,
            dragon_killed: true,
            previously_killed: true,
            respawn_stage: Some(RespawnStage::SummoningPillars),
            respawn_time: 77,
            dragon_uuid: Some(Uuid::from_u128(0x1234_5678_9abc_def0_1122_3344_5566_7788)),
            exit_portal_location: Some(BlockPos::new(0, 63, 0)),
            gateways: vec![3, 1, 2],
            respawn_crystals: vec![Uuid::from_u128(1), Uuid::from_u128(2)],
        };
        let nbt = data.to_nbt();
        assert_eq!(nbt.get_bool("dragon_killed"), Some(true));
        assert_eq!(nbt.get_str("respawn_stage"), Some("summoning_pillars"));
        assert_eq!(
            nbt.get_int_array("exit_portal_location"),
            Some(&[0, 63, 0][..])
        );
        assert_eq!(
            nbt.get_list("gateways"),
            Some(&[Tag::Int(3), Tag::Int(1), Tag::Int(2)][..])
        );
        assert_eq!(FightData::from_nbt(&nbt), data);
    }

    #[test]
    fn missing_keys_take_the_vanilla_defaults() {
        let data = FightData::from_nbt(&Compound::new());
        assert_eq!(data, FightData::default());
    }

    #[test]
    fn alive_crystals_sum_the_counts_of_each_pillar() {
        let spikes = super::super::spikes::spikes_for_seed(1);
        let total = alive_crystals_in(&spikes, |spike| usize::from(spike.height > 90));
        assert_eq!(
            total,
            spikes.iter().filter(|spike| spike.height > 90).count() as i32
        );
    }
}
