use rspigot_blocks::BlockKind;
use rspigot_domain::BlockPos;
use rspigot_domain::brewing::{
    Answers, Ask, BOTTLES, BREW_EVENT, BrewAnswer, BrewingStand, FuelAnswer, Progress, SLOTS, Tick,
};
use rspigot_domain::chunk::BlockEntity;
use rspigot_nbt::{Compound, Tag};

use crate::containers::{read_items, write_items};
use crate::plugins::events::{
    Brew, BrewingStandFuel, BrewingStandFuelPatch, BrewingStart, BrewingStartPatch,
};
use crate::plugins::{
    Continuation, EventKind, EventPayload, InventoryOwner, InventoryRef, Lane, spec_of,
};
use crate::ports::ClientLink;
use crate::server::Server;

const ENTITY: &str = "minecraft:brewing_stand";
const BREW_TIME: &str = "BrewTime";
const TOTAL_BREW_TIME: &str = "total_brew_time";
const FUEL: &str = "Fuel";
const TOTAL_FUEL: &str = "total_fuel";
const SPEED: &str = "speed_multiplier";
const HAS_BOTTLE: [&str; BOTTLES] = ["has_bottle_0", "has_bottle_1", "has_bottle_2"];

#[derive(Debug)]
pub(crate) struct BrewingStep {
    pub(crate) pos: BlockPos,
    pub(crate) snapshot: BrewingStand,
    pub(crate) answers: Answers,
    pub(crate) ask: Ask,
}

pub(crate) enum BrewingAnswer {
    Fuel(BrewingStandFuelPatch),
    Start(BrewingStartPatch),
    Brew,
}

const fn event_of(ask: &Ask) -> EventKind {
    match ask {
        Ask::Fuel { .. } => EventKind::BrewingStandFuel,
        Ask::Start { .. } => EventKind::BrewingStart,
        Ask::Brew { .. } => EventKind::Brew,
    }
}

pub(crate) fn read_stand(data: &Compound) -> BrewingStand {
    let mut items = read_items(data, SLOTS).into_iter();
    let ingredient = items
        .clone()
        .nth(rspigot_domain::brewing::INGREDIENT)
        .flatten()
        .and_then(|stack| stack.item.kind());
    BrewingStand {
        items: std::array::from_fn(|_| items.next().flatten()),
        brew_time: data.get_i32(BREW_TIME).unwrap_or(0),
        total_brew_time: data.get_i32(TOTAL_BREW_TIME).unwrap_or(0),
        fuel: data.get_i32(FUEL).unwrap_or(0),
        total_fuel: data.get_i32(TOTAL_FUEL).unwrap_or(0),
        speed: data.get_f64(SPEED).map_or(1.0, |speed| speed as f32),
        ingredient,
    }
}

pub(crate) fn write_stand(stand: &BrewingStand, data: &mut Compound) {
    write_items(data, &stand.items);
    data.insert(BREW_TIME, stand.brew_time);
    data.insert(TOTAL_BREW_TIME, stand.total_brew_time);
    data.insert(FUEL, stand.fuel);
    data.insert(TOTAL_FUEL, stand.total_fuel);
    data.insert(SPEED, Tag::Float(stand.speed));
}

fn payload_of(pos: BlockPos, ask: &Ask) -> EventPayload {
    match ask {
        Ask::Fuel { fuel, power } => EventPayload::BrewingStandFuel(BrewingStandFuel {
            block: pos,
            consuming: true,
            fuel: spec_of(fuel),
            fuel_power: *power,
        }),
        Ask::Start { source, total } => EventPayload::BrewingStart(BrewingStart {
            block: pos,
            source: spec_of(source),
            total_brew_time: *total,
        }),
        Ask::Brew { results, fuel } => EventPayload::Brew(Brew {
            block: pos,
            contents: InventoryRef {
                owner: InventoryOwner::Block(pos),
                size: SLOTS as i32,
                kind: "BREWING".to_owned(),
                id: 0,
            },
            fuel_level: *fuel,
            results: results.iter().flatten().map(spec_of).collect(),
        }),
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn brewing_stand_at(&self, pos: BlockPos) -> Option<BrewingStand> {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .map(|entity| read_stand(&entity.data))
    }

    fn store_stand(&mut self, pos: BlockPos, stand: &BrewingStand) {
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        let Some(existing) = chunk.block_entity(pos) else {
            return;
        };
        let (kind, mut data) = (existing.kind.clone(), existing.data.clone());
        write_stand(stand, &mut data);
        chunk.set_block_entity(BlockEntity { pos, kind, data });
    }

    pub(crate) fn brewing_data(&self, pos: BlockPos) -> Vec<i16> {
        self.brewing_stand_at(pos)
            .map_or_else(|| vec![0; 4], |stand| stand.data().to_vec())
    }

    fn brewing_asking(&self, pos: BlockPos) -> bool {
        self.decisions.in_flight(Lane::Block(pos), |then| {
            matches!(then, Continuation::Brewing(..))
        })
    }

    fn live_stand(&self, pos: BlockPos) -> Option<BrewingStand> {
        let state = self.world.chunks.block(pos)?;
        (state.kind() == Some(BlockKind::BrewingStand))
            .then(|| self.brewing_stand_at(pos))
            .flatten()
    }

    pub(crate) fn tick_brewing_stands(&mut self) {
        let mut found: Vec<BlockPos> = Vec::new();
        for chunk_pos in self
            .world
            .chunks
            .chunks_with_block_entities(|kind| kind.as_str() == ENTITY)
        {
            let Some(chunk) = self.world.chunks.get(chunk_pos) else {
                continue;
            };
            found.extend(
                chunk
                    .block_entities
                    .iter()
                    .filter(|entity| entity.kind.as_str() == ENTITY)
                    .map(|entity| entity.pos),
            );
        }
        for pos in found {
            if self.brewing_asking(pos) {
                continue;
            }
            if let Some(stand) = self.live_stand(pos) {
                self.run_brewing(pos, &stand, Answers::default());
            }
        }
    }

    fn run_brewing(&mut self, pos: BlockPos, stand: &BrewingStand, mut answers: Answers) {
        loop {
            let mut ticked = stand.clone();
            match ticked.step(&answers) {
                Progress::Done(tick) => {
                    if ticked != *stand {
                        self.stand_ticked(pos, &ticked, tick);
                    } else {
                        self.sync_stand_bottles(pos, &tick);
                    }
                    return;
                }
                Progress::Ask(ask) => {
                    let event = event_of(&ask);
                    if !self.listened(event) {
                        answers.settle_default(&ask);
                        continue;
                    }
                    let payload = payload_of(pos, &ask);
                    let step = BrewingStep {
                        pos,
                        snapshot: stand.clone(),
                        answers,
                        ask,
                    };
                    self.fire(
                        Lane::Block(pos),
                        Continuation::Brewing(event, Box::new(step)),
                        move |_| Some(payload),
                    );
                    return;
                }
            }
        }
    }

    fn stand_ticked(&mut self, pos: BlockPos, stand: &BrewingStand, tick: Tick) {
        self.store_stand(pos, stand);
        self.sync_stand_bottles(pos, &tick);
        if tick.brewed {
            self.level_event(BREW_EVENT, pos, 0);
        }
        self.sync_viewers(pos, None);
    }

    fn sync_stand_bottles(&mut self, pos: BlockPos, tick: &Tick) {
        let Some(mut state) = self.world.chunks.block(pos) else {
            return;
        };
        let before = state;
        for (name, has) in HAS_BOTTLE.iter().zip(tick.bottles) {
            if let Some(next) = state.with_property(name, if has { "true" } else { "false" }) {
                state = next;
            }
        }
        if state != before {
            self.set_block(pos, state);
        }
    }

    pub(crate) fn finish_brewing(
        &mut self,
        step: BrewingStep,
        cancelled: bool,
        answer: BrewingAnswer,
    ) {
        let BrewingStep {
            pos,
            snapshot,
            mut answers,
            ask,
        } = step;
        let Some(current) = self.live_stand(pos) else {
            return;
        };
        if current != snapshot {
            self.run_brewing(pos, &current, Answers::default());
            return;
        }
        match (ask, answer) {
            (Ask::Fuel { power, .. }, BrewingAnswer::Fuel(patch)) => {
                answers.fuel = Some(if cancelled {
                    FuelAnswer::Cancelled
                } else {
                    FuelAnswer::Use {
                        power: patch.fuel_power.unwrap_or(power),
                        consuming: patch.consuming.unwrap_or(true),
                    }
                });
            }
            (Ask::Start { total, .. }, BrewingAnswer::Start(patch)) => {
                answers.start = Some(patch.total_brew_time.unwrap_or(total));
            }
            (Ask::Brew { results, .. }, BrewingAnswer::Brew) => {
                answers.brew = Some(if cancelled {
                    BrewAnswer::Cancelled
                } else {
                    BrewAnswer::Results(results)
                });
            }
            _ => return,
        }
        self.run_brewing(pos, &current, answers);
    }
}
