use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::spawn::placement::is_valid_spawn;
use rspigot_domain::world::Difficulty;
use rspigot_domain::{BlockPos, BlockStateId, Vec3};
use rspigot_entities::{EntityKind as Kind, SoundSource};

use crate::mob_actions::new_mob;
use crate::mobs::sky_darken;
use crate::plugins::events::CreatureSpawnReason;
use crate::plugins::steps::Birth;
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::{flag, level};

const BRIGHT_SKY_DARKEN: i32 = 4;
const OPEN_LONG: &str = "minecraft:block.eyeblossom.open_long";
const CLOSE_LONG: &str = "minecraft:block.eyeblossom.close_long";
const OPEN_SHORT: &str = "minecraft:block.eyeblossom.open";
const CLOSE_SHORT: &str = "minecraft:block.eyeblossom.close";
const SWITCH_RANGE_XZ: i32 = 3;
const SWITCH_RANGE_Y: i32 = 2;
const CACTUS_FLOWER_AGE: i32 = 8;
const CACTUS_LAST_AGE: i32 = 15;
const CACTUS_MAX_HEIGHT: i32 = 3;
const FLOWER_CHANCE_LOW: f64 = 0.1;
const FLOWER_CHANCE_TALL: f64 = 0.25;
const PORTAL_SPAWN_ODDS: i32 = 2000;
const PORTAL_SPAWN_RANGE: f64 = 128.0;
const PORTAL_COOLDOWN: i32 = 300;
const PORTAL_CHECK_LIMIT: i32 = 512;

pub(crate) fn eyeblossom_open(kind: BlockKind) -> Option<bool> {
    match kind {
        BlockKind::OpenEyeblossom | BlockKind::PottedOpenEyeblossom => Some(true),
        BlockKind::ClosedEyeblossom | BlockKind::PottedClosedEyeblossom => Some(false),
        _ => None,
    }
}

fn eyeblossom_flipped(kind: BlockKind) -> Option<BlockKind> {
    Some(match kind {
        BlockKind::OpenEyeblossom => BlockKind::ClosedEyeblossom,
        BlockKind::ClosedEyeblossom => BlockKind::OpenEyeblossom,
        BlockKind::PottedOpenEyeblossom => BlockKind::PottedClosedEyeblossom,
        BlockKind::PottedClosedEyeblossom => BlockKind::PottedOpenEyeblossom,
        _ => return None,
    })
}

pub(crate) fn is_snow(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(BlockKind::Snow | BlockKind::SnowBlock | BlockKind::PowderSnow)
    )
}

fn is_snowy_ground(state: BlockStateId) -> bool {
    matches!(
        state.kind(),
        Some(BlockKind::GrassBlock | BlockKind::Podzol | BlockKind::Mycelium)
    )
}

impl<L: ClientLink> Server<L> {
    fn eyeblossom_should_switch(&self, kind: BlockKind) -> bool {
        let Some(open) = eyeblossom_open(kind) else {
            return false;
        };
        if !self.world.dimension.has_day_cycle() {
            return false;
        }
        let bright = sky_darken(&self.world.level) < BRIGHT_SKY_DARKEN;
        bright == open
    }

    fn eyeblossom_switch(&mut self, pos: BlockPos, state: BlockStateId, long: bool) -> bool {
        let Some(kind) = state.kind() else {
            return false;
        };
        if !self.eyeblossom_should_switch(kind) {
            return false;
        }
        let Some(flipped) = eyeblossom_flipped(kind) else {
            return false;
        };
        let opening = eyeblossom_open(flipped).unwrap_or(false);
        self.set_block(pos, flipped.default_state());
        let sound = match (opening, long) {
            (true, true) => OPEN_LONG,
            (true, false) => OPEN_SHORT,
            (false, true) => CLOSE_LONG,
            (false, false) => CLOSE_SHORT,
        };
        self.play_sound(None, pos.center(), sound, SoundSource::Blocks, 1.0, 1.0);
        true
    }

    pub(crate) fn eyeblossom_random_tick(&mut self, pos: BlockPos, state: BlockStateId) {
        if !self.eyeblossom_switch(pos, state, true) {
            return;
        }
        for dx in -SWITCH_RANGE_XZ..=SWITCH_RANGE_XZ {
            for dy in -SWITCH_RANGE_Y..=SWITCH_RANGE_Y {
                for dz in -SWITCH_RANGE_XZ..=SWITCH_RANGE_XZ {
                    let at = pos.offset(dx, dy, dz);
                    if self.block_or_air(at) != state {
                        continue;
                    }
                    let Some(kind) = state.kind() else {
                        continue;
                    };
                    let distance = f64::from(dx * dx + dy * dy + dz * dz).sqrt();
                    let low = (distance * 5.0) as u64;
                    let high = (distance * 10.0) as u64;
                    let delay = self.rng.random_range(low..=high.max(low));
                    self.schedule(at, kind, delay);
                }
            }
        }
    }

    pub(crate) fn eyeblossom_scheduled(&mut self, pos: BlockPos, state: BlockStateId) {
        self.eyeblossom_switch(pos, state, false);
    }

    pub(crate) fn cactus_flower_tick(&mut self, pos: BlockPos, state: BlockStateId) -> bool {
        let above = pos.offset(0, 1, 0);
        if !self.block_or_air(above).is_air() {
            return true;
        }
        let age = level(state, "age");
        let mut height = 1;
        while self.block_or_air(pos.offset(0, -height, 0)).kind() == Some(BlockKind::Cactus) {
            height += 1;
        }
        let tall = height >= CACTUS_MAX_HEIGHT;
        if tall && age == CACTUS_LAST_AGE {
            return true;
        }
        if age != CACTUS_FLOWER_AGE && !tall {
            return false;
        }
        if age == CACTUS_FLOWER_AGE && self.can_survive_at(BlockKind::Cactus.default_state(), above)
        {
            let chance = if tall {
                FLOWER_CHANCE_TALL
            } else {
                FLOWER_CHANCE_LOW
            };
            if self.rng.random::<f64>() <= chance {
                self.set_block(above, BlockKind::CactusFlower.default_state());
            }
        }
        if let Some(next) = state.with_property("age", &(age + 1).to_string()) {
            self.set_block(pos, next);
        }
        true
    }

    fn player_close_to(&self, pos: BlockPos) -> bool {
        let center = pos.center();
        self.players.values().any(|player| {
            !player.vitals.is_dead()
                && player.game_mode != rspigot_domain::player::GameMode::Spectator
                && player.position.distance_squared(center)
                    <= PORTAL_SPAWN_RANGE * PORTAL_SPAWN_RANGE
        })
    }

    pub(crate) fn nether_portal_random_tick(&mut self, pos: BlockPos) {
        let difficulty = self.world.difficulty;
        let overworld = matches!(
            self.world.dimension.dimension_type.as_str(),
            "minecraft:overworld" | "minecraft:overworld_caves"
        );
        if !overworld || !self.world.level.spawn_monsters || difficulty == Difficulty::Peaceful {
            return;
        }
        if self.rng.random_range(0..PORTAL_SPAWN_ODDS) >= i32::from(difficulty.id())
            || !self.player_close_to(pos)
        {
            return;
        }
        let mut ground = pos;
        let mut guard = 0;
        while self.block_or_air(ground).kind() == Some(BlockKind::NetherPortal)
            && guard < PORTAL_CHECK_LIMIT
        {
            ground = ground.offset(0, -1, 0);
            guard += 1;
        }
        if !is_valid_spawn(self.block_or_air(ground), Kind::ZombifiedPiglin) {
            return;
        }
        let mut entity = new_mob(self.new_entity_base(Kind::ZombifiedPiglin));
        entity.base.snap_to(
            Vec3::new(
                f64::from(ground.x) + 0.5,
                f64::from(ground.y) + 1.0,
                f64::from(ground.z) + 0.5,
            ),
            0.0,
            0.0,
        );
        entity.base.portal_cooldown = PORTAL_COOLDOWN;
        self.spawn_entity(entity, Birth::of(CreatureSpawnReason::NetherPortal));
    }

    pub(crate) fn snowy_changed(&mut self, pos: BlockPos, old: BlockStateId, new: BlockStateId) {
        if old.kind() == new.kind() {
            return;
        }
        let below = pos.offset(0, -1, 0);
        let ground = self.block_or_air(below);
        if is_snowy_ground(ground) && flag(ground, "snowy") != is_snow(new) {
            let value = if is_snow(new) { "true" } else { "false" };
            if let Some(next) = ground.with_property("snowy", value) {
                self.set_block(below, next);
            }
        }
        if is_snowy_ground(new) {
            let above = self.block_or_air(pos.offset(0, 1, 0));
            if flag(new, "snowy") != is_snow(above) {
                let value = if is_snow(above) { "true" } else { "false" };
                if let Some(next) = new.with_property("snowy", value) {
                    self.set_block(pos, next);
                }
            }
        }
    }
}
