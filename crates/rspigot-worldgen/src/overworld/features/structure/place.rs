use rspigot_domain::BlockStateId;

use super::super::place::Run;
use super::super::prov::Pos;
use super::processor::Processor;
use super::rotation::Rotation;
use super::shape::settle;
use super::template::Template;

pub(crate) type Bounds = (Pos, Pos);

pub(crate) struct Settings<'a> {
    pub(crate) rotation: Rotation,
    pub(crate) bounds: Option<Bounds>,
    pub(crate) processors: &'a [Processor],
}

const SPREAD: [(i32, i32, i32); 5] = [(0, 1, 0), (0, 0, -1), (1, 0, 0), (0, 0, 1), (-1, 0, 0)];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fluid {
    None,
    Flowing,
    Source { water: bool },
}

impl Fluid {
    fn of(state: BlockStateId) -> Self {
        use rspigot_blocks::BlockKind as Kind;
        match state.kind() {
            Some(Kind::Water | Kind::Lava) => {
                let water = state.kind() == Some(Kind::Water);
                if state.property("level") == Some("0") {
                    Self::Source { water }
                } else {
                    Self::Flowing
                }
            }
            Some(
                Kind::Seagrass
                | Kind::TallSeagrass
                | Kind::Kelp
                | Kind::KelpPlant
                | Kind::BubbleColumn,
            ) => Self::Source { water: true },
            _ if state.property("waterlogged") == Some("true") => Self::Source { water: true },
            _ => Self::None,
        }
    }

    fn is_source(self) -> bool {
        matches!(self, Self::Source { .. })
    }
}

fn holds_liquid(state: BlockStateId) -> bool {
    state.property("waterlogged").is_some()
}

fn fill(state: BlockStateId, fluid: Fluid) -> Option<BlockStateId> {
    if state.property("waterlogged") == Some("false") && fluid == (Fluid::Source { water: true }) {
        state.with_property("waterlogged", "true")
    } else {
        None
    }
}

fn offset((x, y, z): Pos, (dx, dy, dz): Pos) -> Pos {
    (x + dx, y + dy, z + dz)
}

fn inside((low, high): Bounds, (x, y, z): Pos) -> bool {
    x >= low.0 && x <= high.0 && y >= low.1 && y <= high.1 && z >= low.2 && z <= high.2
}

fn processed(
    template: &Template,
    run: &mut Run,
    origin: Pos,
    settings: &Settings,
) -> Vec<(Pos, BlockStateId)> {
    let mut result = Vec::with_capacity(template.blocks().len());
    for &(local, state) in template.blocks() {
        let at = offset(settings.rotation.transform(local), origin);
        if settings.bounds.is_some_and(|bounds| !inside(bounds, at)) {
            continue;
        }
        let mut current = Some(state);
        for processor in settings.processors {
            let Some(held) = current else { break };
            current = processor.process(run, at, held);
        }
        if let Some(state) = current {
            result.push((at, state));
        }
    }
    result
}

fn fluid_at(run: &mut Run, (x, y, z): Pos) -> Fluid {
    Fluid::of(run.region.get(x, y, z))
}

pub(crate) fn place_in_world(
    template: &Template,
    run: &mut Run,
    origin: Pos,
    settings: &Settings,
) -> bool {
    if template.blocks().is_empty() {
        return false;
    }
    run.rng.below(1);
    let blocks = processed(template, run, origin, settings);
    let mut pending: Vec<Pos> = Vec::new();
    let mut sources: Vec<Pos> = Vec::new();
    let mut placed: Vec<Pos> = Vec::new();
    for (at, state) in blocks {
        let before = fluid_at(run, at);
        let state = settings.rotation.state(state);
        if !run.region.set(at.0, at.1, at.2, state) {
            continue;
        }
        placed.push(at);
        if Fluid::of(state).is_source() {
            sources.push(at);
        } else if holds_liquid(state) {
            if let Some(filled) = fill(state, before) {
                run.region.set(at.0, at.1, at.2, filled);
            }
            if !before.is_source() {
                pending.push(at);
            }
        }
    }
    spread(run, &mut pending, &sources);
    settle(run, &placed);
    true
}

fn spread(run: &mut Run, pending: &mut Vec<Pos>, sources: &[Pos]) {
    let mut changed = true;
    while changed && !pending.is_empty() {
        changed = false;
        let mut index = 0;
        while index < pending.len() {
            let at = pending[index];
            let mut fluid = fluid_at(run, at);
            for step in SPREAD {
                if fluid.is_source() {
                    break;
                }
                let next = offset(at, step);
                let around = fluid_at(run, next);
                if around.is_source() && !sources.contains(&next) {
                    fluid = around;
                }
            }
            if fluid.is_source() {
                let state = run.region.get(at.0, at.1, at.2);
                if holds_liquid(state) {
                    if let Some(filled) = fill(state, fluid) {
                        run.region.set(at.0, at.1, at.2, filled);
                    }
                    changed = true;
                    pending.remove(index);
                    continue;
                }
            }
            index += 1;
        }
    }
}
