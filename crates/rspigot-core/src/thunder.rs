use rand::Rng;
use rspigot_domain::entity::lightning::{self, LightningState};
use rspigot_domain::entity::{Aabb, Entity, EntityState, save};
use rspigot_domain::poi::{Occupancy, PoiType};
use rspigot_domain::{BlockPos, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_nbt::Compound;

use crate::plugins::Birth;
use crate::plugins::events::CreatureSpawnReason;
use crate::ports::ClientLink;
use crate::redstone::lightning_rod::is_lightning_rod;
use crate::server::Server;

const RANGE_CELLS: i32 = 16;
const ROD_RADIUS: i32 = 128;
const BYSTANDER_RADIUS: f64 = 3.0;
const FLASH_VARIANTS: i32 = 3;

impl<L: ClientLink> Server<L> {
    pub(crate) fn tick_thunder(&mut self) {
        let chance = self.config.thunder_chance;
        if chance < 1 || !self.world.level.raining || !self.world.level.thundering {
            return;
        }
        for chunk in self.ticking_chunks_near_players() {
            if self.rng.random_range(0..chance) != 0 {
                continue;
            }
            let x = chunk.x * RANGE_CELLS + self.rng.random_range(0..RANGE_CELLS);
            let z = chunk.z * RANGE_CELLS + self.rng.random_range(0..RANGE_CELLS);
            let target = self.lightning_target_around(x, z);
            if self.raining_at(target) {
                self.strike_from_storm(target);
            }
        }
    }

    fn lightning_target_around(&mut self, x: i32, z: i32) -> BlockPos {
        let top = self.motion_blocking_top(x, z);
        let surface = BlockPos::new(x, top, z);
        if let Some(rod) = self.lightning_rod_near(surface) {
            return rod.offset(0, 1, 0);
        }
        let height = self.world.dimension.height;
        let r = BYSTANDER_RADIUS;
        let column = Aabb::new(
            f64::from(x) - r,
            f64::from(top) - r,
            f64::from(z) - r,
            f64::from(x + 1) + r,
            f64::from(height.max_y() + 2) + r,
            f64::from(z + 1) + r,
        );
        let bystanders: Vec<BlockPos> = self
            .living_in(&column)
            .into_iter()
            .filter_map(|id| self.subject_view(id))
            .map(|view| view.position.block())
            .filter(|pos| self.chunk_view().can_see_sky_at(*pos))
            .collect();
        if !bystanders.is_empty() {
            return bystanders[self.rng.random_range(0..bystanders.len())];
        }
        if top == height.min_y - 1 {
            return surface.offset(0, 2, 0);
        }
        surface
    }

    fn lightning_rod_near(&self, surface: BlockPos) -> Option<BlockPos> {
        self.world
            .chunks
            .poi()
            .in_range(
                |kind| kind == PoiType::LightningRod,
                surface,
                ROD_RADIUS,
                Occupancy::Any,
            )
            .into_iter()
            .map(|record| record.pos())
            .filter(|pos| pos.y == self.motion_blocking_top(pos.x, pos.z) - 1)
            .min_by_key(|pos| {
                let (dx, dy, dz) = (
                    i64::from(pos.x - surface.x),
                    i64::from(pos.y - surface.y),
                    i64::from(pos.z - surface.z),
                );
                dx * dx + dy * dy + dz * dz
            })
    }

    fn strike_from_storm(&mut self, target: BlockPos) {
        let effective = self.difficulty_at(target).effective;
        let rod_below = self
            .block_or_air(target.offset(0, -1, 0))
            .kind()
            .is_some_and(is_lightning_rod);
        let spawn_mobs = self.world.level.spawn_mobs;
        let roll = if spawn_mobs {
            self.rng.random::<f64>()
        } else {
            1.0
        };
        let trap = lightning::is_skeleton_trap(spawn_mobs, effective, roll, rod_below);
        if trap {
            self.spawn_skeleton_trap(target);
        }
        self.spawn_bolt(target.center(), trap, CreatureSpawnReason::Natural);
    }

    fn spawn_skeleton_trap(&mut self, pos: BlockPos) {
        let id = self.allocate_entity_id();
        let uuid = self.random_uuid();
        let compound = Compound::new()
            .with("id", Kind::SkeletonHorse.name())
            .with("SkeletonTrap", true)
            .with("SkeletonTrapTime", 0i32);
        let Ok(mut entity) = save::load_entity(&compound, id, uuid) else {
            return;
        };
        entity.base.snap_to(
            Vec3::new(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z)),
            0.0,
            0.0,
        );
        self.spawn_entity(entity, Birth::of(CreatureSpawnReason::Lightning));
    }

    pub(crate) fn spawn_bolt(
        &mut self,
        position: Vec3,
        visual_only: bool,
        reason: CreatureSpawnReason,
    ) {
        let flashes = self.rng.random_range(0..FLASH_VARIANTS) + 1;
        let mut base = self.new_entity_base(Kind::LightningBolt);
        base.position = position;
        base.old_position = position;
        let mut state = LightningState::new(flashes);
        state.visual_only = visual_only;
        self.spawn_entity(
            Entity::new(base, EntityState::Lightning(state)),
            Birth::of(reason),
        );
    }
}
