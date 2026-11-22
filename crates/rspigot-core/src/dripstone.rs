use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rspigot_blocks::BlockKind;
use rspigot_domain::dripstone::{self, Drip, Fluid};
use rspigot_domain::{BlockPos, BlockStateId};

use crate::cauldron::Contents;
use crate::mobs::ServerRandom;
use crate::plugins::events::{CauldronLevelChange, CauldronLevelChangeReason};
use crate::plugins::steps::CauldronStep;
use crate::plugins::{Continuation, EventPayload, Lane, snapshot_of};
use crate::ports::ClientLink;
use crate::server::Server;

const DRIP_EVENT: i32 = 1504;
const WATER_INTO_CAULDRON_EVENT: i32 = 1047;
const LAVA_INTO_CAULDRON_EVENT: i32 = 1046;
const MAX_WATER_LEVEL: u8 = 3;

fn accepts(fluid: Fluid, state: BlockStateId) -> bool {
    match (fluid, Contents::of(state)) {
        (Fluid::Water, Some(Contents::Empty)) => true,
        (Fluid::Water, Some(Contents::Water(level))) => level < MAX_WATER_LEVEL,
        (Fluid::Lava, Some(Contents::Empty)) => true,
        _ => false,
    }
}

fn filled(fluid: Fluid, state: BlockStateId) -> Option<BlockStateId> {
    match (fluid, Contents::of(state)?) {
        (Fluid::Lava, Contents::Empty) => Some(BlockKind::LavaCauldron.default_state()),
        (Fluid::Water, Contents::Empty) => BlockKind::WaterCauldron
            .default_state()
            .with_property("level", "1"),
        (Fluid::Water, Contents::Water(level)) if level < MAX_WATER_LEVEL => {
            BlockKind::WaterCauldron
                .default_state()
                .with_property("level", &(level + 1).to_string())
        }
        _ => None,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn dripstone_random_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        let drip_roll: f32 = self.rng.random();
        let growth_roll: f32 = self.rng.random();
        self.maybe_start_drip(pos, state, drip_roll);
        let above = self.block_or_air(pos.offset(0, 1, 0));
        if growth_roll < dripstone::GROWTH_CHANCE && dripstone::is_stalactite_start(state, above) {
            self.grow_dripstone(pos, state);
        }
    }

    fn grow_dripstone(&mut self, pos: BlockPos, state: BlockStateId) {
        let seed: u64 = self.rng.random();
        let placed = {
            let block = |at: BlockPos| self.block_or_air(at);
            let mut rng = StdRng::seed_from_u64(seed);
            dripstone::plan_growth(&block, pos, state, &mut ServerRandom(&mut rng))
        };
        for (at, grown) in &placed {
            self.set_block(*at, *grown);
        }
        for (at, _) in &placed {
            self.update_neighbour_shapes(*at);
        }
    }

    fn maybe_start_drip(&mut self, pos: BlockPos, state: BlockStateId, roll: f32) {
        let found = {
            let block = |at: BlockPos| self.block_or_air(at);
            dripstone::find_drip(&block, pos, state)
        };
        let Some(Drip { fluid, source, tip }) = found else {
            return;
        };
        if roll >= dripstone::drip_chance(fluid) {
            return;
        }
        if fluid == Fluid::Water && self.block_or_air(source).kind() == Some(BlockKind::Mud) {
            self.set_block(source, BlockKind::Clay.default_state());
            self.level_event(DRIP_EVENT, tip, 0);
            return;
        }
        let cauldron = {
            let block = |at: BlockPos| self.block_or_air(at);
            dripstone::cauldron_below(&block, tip, |below| accepts(fluid, below))
        };
        let Some(cauldron) = cauldron else {
            return;
        };
        self.level_event(DRIP_EVENT, tip, 0);
        if let Some(kind) = self.block_or_air(cauldron).kind() {
            self.schedule(cauldron, kind, dripstone::drip_delay(tip, cauldron));
        }
    }

    pub(crate) fn cauldron_drip_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        let found = {
            let block = |at: BlockPos| self.block_or_air(at);
            dripstone::find_stalactite_above(&block, pos)
                .and_then(|(at, stalactite)| dripstone::find_drip(&block, at, stalactite))
        };
        let Some(Drip { fluid, .. }) = found else {
            return;
        };
        let Some(next) = filled(fluid, state) else {
            return;
        };
        let step = CauldronStep {
            pos,
            expected: state,
            state: next,
            reason: CauldronLevelChangeReason::NaturalFill,
            entity: None,
            extinguish: false,
            by: None,
        };
        self.fire(
            Lane::Block(pos),
            Continuation::Cauldron(Box::new(step)),
            move |_| {
                Some(EventPayload::CauldronLevelChange(CauldronLevelChange {
                    block: pos,
                    entity: None,
                    new_state: snapshot_of(pos, next),
                    reason: CauldronLevelChangeReason::NaturalFill,
                }))
            },
        );
        let event = match fluid {
            Fluid::Water => WATER_INTO_CAULDRON_EVENT,
            Fluid::Lava => LAVA_INTO_CAULDRON_EVENT,
        };
        self.level_event(event, pos, 0);
    }
}
