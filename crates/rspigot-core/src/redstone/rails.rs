use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::entity::{Aabb, EntityState};
use rspigot_domain::{BlockPos, BlockStateId, EntityId};
use rspigot_entities::EntityKind;
use rustc_hash::FxHashSet;

use super::{MAX_POWER, RedstoneChange, RedstoneStep, step};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::flag;

const SEARCH_DEPTH: i32 = 8;
const DETECTOR_PERIOD: u64 = 20;
const INSET: f64 = 0.2;
const HEIGHT: f64 = 0.7;

fn is_ascending(shape: &str) -> bool {
    shape.starts_with("ascending_")
}

fn detector_area(pos: BlockPos) -> Aabb {
    let (x, y, z) = (f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
    Aabb::new(
        x + INSET,
        y,
        z + INSET,
        x + 1.0 - INSET,
        y + HEIGHT,
        z + 1.0 - INSET,
    )
}

impl<L: ClientLink> Server<L> {
    fn rail_has_power(
        &self,
        pos: BlockPos,
        kind: BlockKind,
        shape: &str,
        forward: bool,
        depth: i32,
    ) -> bool {
        if depth >= SEARCH_DEPTH {
            return false;
        }
        let (mut x, mut y, z_start) = (pos.x, pos.y, pos.z);
        let mut z = z_start;
        let mut flat = true;
        let mut along = shape;
        match shape {
            "north_south" => z += if forward { 1 } else { -1 },
            "east_west" => x += if forward { -1 } else { 1 },
            "ascending_east" => {
                if forward {
                    x -= 1;
                } else {
                    x += 1;
                    y += 1;
                    flat = false;
                }
                along = "east_west";
            }
            "ascending_west" => {
                if forward {
                    x -= 1;
                    y += 1;
                    flat = false;
                } else {
                    x += 1;
                }
                along = "east_west";
            }
            "ascending_north" => {
                if forward {
                    z += 1;
                } else {
                    z -= 1;
                    y += 1;
                    flat = false;
                }
                along = "north_south";
            }
            "ascending_south" => {
                if forward {
                    z += 1;
                    y += 1;
                    flat = false;
                } else {
                    z -= 1;
                }
                along = "north_south";
            }
            _ => {}
        }
        self.same_rail_with_power(BlockPos::new(x, y, z), kind, forward, depth, along)
            || (flat
                && self.same_rail_with_power(
                    BlockPos::new(x, y - 1, z),
                    kind,
                    forward,
                    depth,
                    along,
                ))
    }

    fn same_rail_with_power(
        &self,
        pos: BlockPos,
        kind: BlockKind,
        forward: bool,
        depth: i32,
        along: &str,
    ) -> bool {
        let state = self.block_or_air(pos);
        if state.kind() != Some(kind) {
            return false;
        }
        let Some(shape) = state.property("shape") else {
            return false;
        };
        let east_west_blocked = along == "east_west"
            && matches!(shape, "north_south" | "ascending_north" | "ascending_south");
        let north_south_blocked = along == "north_south"
            && matches!(shape, "east_west" | "ascending_east" | "ascending_west");
        if east_west_blocked || north_south_blocked || !flag(state, "powered") {
            return false;
        }
        self.has_neighbor_signal(pos) || self.rail_has_power(pos, kind, shape, forward, depth + 1)
    }

    pub(crate) fn powered_rail_changed(&mut self, pos: BlockPos, state: BlockStateId) {
        let (Some(kind), Some(shape)) = (state.kind(), state.property("shape")) else {
            return;
        };
        let powered = self.has_neighbor_signal(pos)
            || self.rail_has_power(pos, kind, shape, true, 0)
            || self.rail_has_power(pos, kind, shape, false, 0);
        if powered == flag(state, "powered") {
            return;
        }
        let Some(next) = state.with_property("powered", if powered { "true" } else { "false" })
        else {
            return;
        };
        self.set_block_quiet(pos, next);
        self.update_neighbors_at(pos);
        self.update_neighbors_at(step(pos, Face::Down));
        if is_ascending(shape) {
            self.update_neighbors_at(step(pos, Face::Up));
        }
    }

    fn carts_on(&self, pos: BlockPos) -> bool {
        let area = detector_area(pos);
        self.entities.store.iter().any(|entity| {
            entity.base.removal.is_none()
                && matches!(entity.state, EntityState::Minecart(_))
                && entity.base.bounding_box().intersects(&area)
        })
    }

    pub(crate) fn detector_check(&mut self, pos: BlockPos) {
        let state = self.block_or_air(pos);
        if state.kind() != Some(BlockKind::DetectorRail) {
            return;
        }
        let was = flag(state, "powered");
        let wants = self.carts_on(pos);
        if was != wants {
            self.ask_redstone(RedstoneStep {
                pos,
                old: if was { MAX_POWER } else { 0 },
                new: if wants { MAX_POWER } else { 0 },
                change: RedstoneChange::Detector,
            });
        } else if wants {
            self.schedule(pos, BlockKind::DetectorRail, DETECTOR_PERIOD);
        }
    }

    pub(crate) fn finish_detector(&mut self, pos: BlockPos, new: i32) {
        let state = self.block_or_air(pos);
        if state.kind() != Some(BlockKind::DetectorRail) {
            return;
        }
        let wants = new > 0;
        if wants != flag(state, "powered")
            && let Some(next) = state.with_property("powered", if wants { "true" } else { "false" })
        {
            self.set_block(pos, next);
            self.update_neighbors_at(step(pos, Face::Down));
        }
        if wants {
            self.schedule(pos, BlockKind::DetectorRail, DETECTOR_PERIOD);
        }
    }

    pub(crate) fn detector_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        if flag(state, "powered") {
            self.detector_check(pos);
        }
    }

    pub(crate) fn tick_rails(&mut self) {
        let mut detectors: FxHashSet<BlockPos> = FxHashSet::default();
        let mut activated: Vec<EntityId> = Vec::new();
        for entity in self.entities.store.iter() {
            if entity.base.removal.is_some() || !matches!(entity.state, EntityState::Minecart(_)) {
                continue;
            }
            let area = entity.base.bounding_box();
            let low = rspigot_domain::Vec3::new(area.min_x, area.min_y, area.min_z).block();
            let high = rspigot_domain::Vec3::new(area.max_x, area.max_y, area.max_z).block();
            for x in low.x..=high.x {
                for y in low.y..=high.y {
                    for z in low.z..=high.z {
                        let at = BlockPos::new(x, y, z);
                        let state = self.block_or_air(at);
                        match state.kind() {
                            Some(BlockKind::DetectorRail) if !flag(state, "powered") => {
                                detectors.insert(at);
                            }
                            Some(BlockKind::ActivatorRail)
                                if flag(state, "powered")
                                    && entity.base.kind == EntityKind::Minecart =>
                            {
                                activated.push(entity.base.id);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        for pos in detectors {
            self.detector_check(pos);
        }
        for cart in activated {
            self.eject_passengers(cart);
        }
    }
}
