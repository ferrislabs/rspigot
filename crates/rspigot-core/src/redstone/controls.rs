use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::entity::Aabb;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId, Vec3};
use rspigot_entities::{EntityKind, SoundSource};
use rustc_hash::FxHashSet;

use super::{MAX_POWER, RedstoneChange, RedstoneStep, connected_direction, is_button, is_plate};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::{flag, level};

const LEVER_ON_PITCH: f32 = 0.6;
const LEVER_OFF_PITCH: f32 = 0.5;
const LEVER_VOLUME: f32 = 0.3;
pub(super) const PLAYER_WIDTH: f32 = 0.6;
const LIGHT_WEIGHT: i32 = 15;
const HEAVY_WEIGHT: i32 = 150;
const WEIGHTED_PRESSED_TIME: u64 = 10;
const PRESSED_TIME: u64 = 20;
const STONE_BUTTON_TIME: u64 = 20;
const WOOD_BUTTON_TIME: u64 = 30;
const BUTTON_DEPTH: f64 = 0.125;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Stone,
    Wood,
    Cherry,
    Bamboo,
    Nether,
    Metal,
}

fn family_of(kind: BlockKind) -> Family {
    let name = kind.name();
    if name.contains("weighted") {
        Family::Metal
    } else if name.contains("cherry") {
        Family::Cherry
    } else if name.contains("bamboo") {
        Family::Bamboo
    } else if name.contains("crimson") || name.contains("warped") {
        Family::Nether
    } else if name.contains("stone") {
        Family::Stone
    } else {
        Family::Wood
    }
}

fn button_sound(kind: BlockKind, on: bool) -> &'static str {
    match (family_of(kind), on) {
        (Family::Stone | Family::Metal, true) => "minecraft:block.stone_button.click_on",
        (Family::Stone | Family::Metal, false) => "minecraft:block.stone_button.click_off",
        (Family::Wood, true) => "minecraft:block.wooden_button.click_on",
        (Family::Wood, false) => "minecraft:block.wooden_button.click_off",
        (Family::Cherry, true) => "minecraft:block.cherry_wood_button.click_on",
        (Family::Cherry, false) => "minecraft:block.cherry_wood_button.click_off",
        (Family::Bamboo, true) => "minecraft:block.bamboo_wood_button.click_on",
        (Family::Bamboo, false) => "minecraft:block.bamboo_wood_button.click_off",
        (Family::Nether, true) => "minecraft:block.nether_wood_button.click_on",
        (Family::Nether, false) => "minecraft:block.nether_wood_button.click_off",
    }
}

fn plate_sound(kind: BlockKind, on: bool) -> &'static str {
    match (family_of(kind), on) {
        (Family::Stone, true) => "minecraft:block.stone_pressure_plate.click_on",
        (Family::Stone, false) => "minecraft:block.stone_pressure_plate.click_off",
        (Family::Wood, true) => "minecraft:block.wooden_pressure_plate.click_on",
        (Family::Wood, false) => "minecraft:block.wooden_pressure_plate.click_off",
        (Family::Cherry, true) => "minecraft:block.cherry_wood_pressure_plate.click_on",
        (Family::Cherry, false) => "minecraft:block.cherry_wood_pressure_plate.click_off",
        (Family::Bamboo, true) => "minecraft:block.bamboo_wood_pressure_plate.click_on",
        (Family::Bamboo, false) => "minecraft:block.bamboo_wood_pressure_plate.click_off",
        (Family::Nether, true) => "minecraft:block.nether_wood_pressure_plate.click_on",
        (Family::Nether, false) => "minecraft:block.nether_wood_pressure_plate.click_off",
        (Family::Metal, true) => "minecraft:block.metal_pressure_plate.click_on",
        (Family::Metal, false) => "minecraft:block.metal_pressure_plate.click_off",
    }
}

fn hit_by_arrows(kind: BlockKind) -> bool {
    family_of(kind) != Family::Stone
}

fn button_time(kind: BlockKind) -> u64 {
    if hit_by_arrows(kind) {
        WOOD_BUTTON_TIME
    } else {
        STONE_BUTTON_TIME
    }
}

fn plate_signal(state: BlockStateId) -> i32 {
    if state.property("power").is_some() {
        level(state, "power")
    } else if flag(state, "powered") {
        MAX_POWER
    } else {
        0
    }
}

fn with_plate_signal(state: BlockStateId, signal: i32) -> BlockStateId {
    let next = if state.property("power").is_some() {
        state.with_property("power", &signal.to_string())
    } else {
        state.with_property("powered", if signal > 0 { "true" } else { "false" })
    };
    next.unwrap_or(state)
}

fn is_arrow(kind: EntityKind) -> bool {
    matches!(
        kind,
        EntityKind::Arrow | EntityKind::SpectralArrow | EntityKind::Trident
    )
}

fn button_bounds(pos: BlockPos, state: BlockStateId) -> Aabb {
    let cell = Aabb::block(pos);
    let thickness = BUTTON_DEPTH;
    match connected_direction(state) {
        Face::Up => Aabb {
            max_y: cell.min_y + thickness,
            ..cell
        },
        Face::Down => Aabb {
            min_y: cell.max_y - thickness,
            ..cell
        },
        Face::North => Aabb {
            max_z: cell.min_z + thickness,
            ..cell
        },
        Face::South => Aabb {
            min_z: cell.max_z - thickness,
            ..cell
        },
        Face::West => Aabb {
            max_x: cell.min_x + thickness,
            ..cell
        },
        Face::East => Aabb {
            min_x: cell.max_x - thickness,
            ..cell
        },
    }
}

fn touch_bounds(pos: BlockPos) -> Aabb {
    let (x, y, z) = (f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
    Aabb::new(x + 0.0625, y, z + 0.0625, x + 0.9375, y + 0.25, z + 0.9375)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn use_redstone(
        &mut self,
        id: PlayerId,
        pos: BlockPos,
        state: BlockStateId,
    ) -> bool {
        let Some(kind) = state.kind() else {
            return false;
        };
        if kind == BlockKind::MovingPiston {
            if self.falls.redstone.moving.contains_key(&pos) {
                return false;
            }
            self.set_block(pos, BlockStateId::AIR);
            return true;
        }
        let powered = flag(state, "powered");
        if kind == BlockKind::Lever {
            let (old, new) = if powered {
                (MAX_POWER, 0)
            } else {
                (0, MAX_POWER)
            };
            self.ask_redstone(RedstoneStep {
                pos,
                old,
                new,
                change: RedstoneChange::Lever { player: Some(id) },
            });
            return true;
        }
        if is_button(kind) {
            if !powered {
                self.ask_redstone(RedstoneStep {
                    pos,
                    old: 0,
                    new: MAX_POWER,
                    change: RedstoneChange::Button {
                        powered: true,
                        player: Some(id),
                    },
                });
            }
            return true;
        }
        if matches!(kind, BlockKind::Repeater | BlockKind::Comparator) {
            return self.use_diode(id, pos, state);
        }
        if kind == BlockKind::NoteBlock {
            return self.use_note_block(pos, state);
        }
        if kind == BlockKind::DaylightDetector {
            return self.use_daylight_detector(id, pos, state);
        }
        false
    }

    pub(crate) fn finish_lever(
        &mut self,
        pos: BlockPos,
        old: i32,
        new: i32,
        player: Option<PlayerId>,
    ) {
        if (new > 0) != (old == 0) {
            return;
        }
        let state = self.block_or_air(pos);
        if state.kind() != Some(BlockKind::Lever) {
            return;
        }
        let on = !flag(state, "powered");
        let Some(next) = state.with_property("powered", if on { "true" } else { "false" }) else {
            return;
        };
        self.set_block(pos, next);
        let pitch = if on { LEVER_ON_PITCH } else { LEVER_OFF_PITCH };
        self.play_sound(
            player,
            pos.center(),
            "minecraft:block.lever.click",
            SoundSource::Blocks,
            LEVER_VOLUME,
            pitch,
        );
    }

    pub(crate) fn finish_button(
        &mut self,
        pos: BlockPos,
        new: i32,
        powered: bool,
        player: Option<PlayerId>,
    ) {
        if (new > 0) != powered {
            return;
        }
        let state = self.block_or_air(pos);
        let Some(kind) = state.kind().filter(|kind| is_button(*kind)) else {
            return;
        };
        let Some(next) = state.with_property("powered", if powered { "true" } else { "false" })
        else {
            return;
        };
        self.set_block(pos, next);
        if powered {
            self.schedule(pos, kind, button_time(kind));
        }
        self.play_sound(
            player,
            pos.center(),
            button_sound(kind, powered),
            SoundSource::Blocks,
            1.0,
            1.0,
        );
    }

    fn arrow_on_button(&self, pos: BlockPos, state: BlockStateId) -> bool {
        let bounds = button_bounds(pos, state);
        self.entities.store.iter().any(|entity| {
            entity.base.removal.is_none()
                && is_arrow(entity.base.kind)
                && entity.base.bounding_box().intersects(&bounds)
        })
    }

    fn check_button(&mut self, pos: BlockPos, state: BlockStateId) {
        let Some(kind) = state.kind() else {
            return;
        };
        let arrow = hit_by_arrows(kind) && self.arrow_on_button(pos, state);
        let powered = flag(state, "powered");
        if arrow != powered {
            let (old, new) = if powered {
                (MAX_POWER, 0)
            } else {
                (0, MAX_POWER)
            };
            self.ask_redstone(RedstoneStep {
                pos,
                old,
                new,
                change: RedstoneChange::Button {
                    powered: arrow,
                    player: None,
                },
            });
        } else if arrow {
            self.schedule(pos, kind, button_time(kind));
        }
    }

    pub(crate) fn button_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        if flag(state, "powered") {
            self.check_button(pos, state);
        }
    }

    pub(super) fn count_touching(&self, bounds: &Aabb, living_only: bool) -> i32 {
        let players = self
            .players
            .values()
            .filter(|player| {
                player.game_mode != GameMode::Spectator
                    && !player.vitals.is_dead()
                    && Aabb::of_size(player.position, PLAYER_WIDTH, player.height() as f32)
                        .intersects(bounds)
            })
            .count();
        let others = self
            .entities
            .store
            .iter()
            .filter(|entity| {
                entity.base.removal.is_none()
                    && (!living_only || entity.living().is_some())
                    && entity.base.bounding_box().intersects(bounds)
            })
            .count();
        (players + others) as i32
    }

    fn check_plate(&mut self, pos: BlockPos, state: BlockStateId, old: i32) {
        let Some(kind) = state.kind() else {
            return;
        };
        let name = kind.name();
        let weighted = name.contains("weighted");
        let limit = if !weighted {
            0
        } else if name.contains("heavy") {
            HEAVY_WEIGHT
        } else {
            LIGHT_WEIGHT
        };
        let living_only = !weighted && family_of(kind) == Family::Stone;
        let touchers = self.touchers_in(&touch_bounds(pos), living_only);
        if touchers.is_empty() {
            return self.plate_settled(pos, old, 0);
        }
        if !weighted && old > 0 {
            return self.plate_settled(pos, old, MAX_POWER);
        }
        self.ask_touch(super::touch::TouchStep {
            pos,
            old,
            what: super::touch::Touched::Plate { weighted, limit },
            touchers,
            at: 0,
            accepted: 0,
        });
    }

    pub(super) fn plate_settled(&mut self, pos: BlockPos, old: i32, new: i32) {
        if (old > 0) != (new > 0) {
            self.ask_redstone(RedstoneStep {
                pos,
                old,
                new,
                change: RedstoneChange::Plate,
            });
        } else {
            self.finish_plate(pos, old, new);
        }
    }

    pub(crate) fn finish_plate(&mut self, pos: BlockPos, old: i32, new: i32) {
        let state = self.block_or_air(pos);
        let Some(kind) = state.kind().filter(|kind| is_plate(*kind)) else {
            return;
        };
        let new = new.clamp(0, MAX_POWER);
        if old != new {
            self.set_block(pos, with_plate_signal(state, new));
        }
        if old > 0 && new == 0 {
            self.play_sound(
                None,
                pos.center(),
                plate_sound(kind, false),
                SoundSource::Blocks,
                1.0,
                1.0,
            );
        } else if new > 0 && old == 0 {
            self.play_sound(
                None,
                pos.center(),
                plate_sound(kind, true),
                SoundSource::Blocks,
                1.0,
                1.0,
            );
        }
        if new > 0 {
            let time = if kind.name().contains("weighted") {
                WEIGHTED_PRESSED_TIME
            } else {
                PRESSED_TIME
            };
            self.schedule(pos, kind, time);
        }
    }

    pub(crate) fn plate_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        let signal = plate_signal(state);
        if signal > 0 {
            self.check_plate(pos, state, signal);
        }
    }

    pub(crate) fn tick_pressure_plates(&mut self) {
        let mut bodies: Vec<(Aabb, bool)> = Vec::new();
        for player in self.players.values() {
            if player.game_mode != GameMode::Spectator && !player.vitals.is_dead() {
                let bounds = Aabb::of_size(player.position, PLAYER_WIDTH, player.height() as f32);
                bodies.push((bounds, false));
            }
        }
        for entity in self.entities.store.iter() {
            if entity.base.removal.is_none() {
                bodies.push((entity.base.bounding_box(), is_arrow(entity.base.kind)));
            }
        }
        let mut touched: FxHashSet<BlockPos> = FxHashSet::default();
        for (bounds, _) in &bodies {
            let low = Vec3::new(bounds.min_x, bounds.min_y, bounds.min_z).block();
            let high = Vec3::new(bounds.max_x, bounds.max_y, bounds.max_z).block();
            if (high.x - low.x + 1) * (high.y - low.y + 1) * (high.z - low.z + 1) > 512 {
                continue;
            }
            for x in low.x..=high.x {
                for y in low.y..=high.y {
                    for z in low.z..=high.z {
                        let pos = BlockPos::new(x, y, z);
                        if self.block_or_air(pos).kind().is_some_and(|kind| {
                            is_plate(kind) || is_button(kind) || kind == BlockKind::Tripwire
                        }) {
                            touched.insert(pos);
                        }
                    }
                }
            }
        }
        for pos in touched {
            let state = self.block_or_air(pos);
            let Some(kind) = state.kind() else {
                continue;
            };
            if is_plate(kind) && plate_signal(state) == 0 {
                self.check_plate(pos, state, 0);
            } else if is_button(kind) && hit_by_arrows(kind) && !flag(state, "powered") {
                self.check_button(pos, state);
            } else if kind == BlockKind::Tripwire {
                self.tripwire_stepped_on(pos, state);
            }
        }
    }
}
