use rspigot_domain::dragon::Part;
use rspigot_domain::dragon::parts;
use rspigot_domain::entity::Aabb;
use rspigot_domain::entity::physics::has_tag;
use rspigot_domain::random::RandomSource;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, mth};

use super::ai::state_of;
use crate::mobs::ServerRandom;
use crate::plugins::events::{EntityExplode, EntityExplodePatch, ExplosionResult};
use crate::plugins::steps::DragonWallStep;
use crate::plugins::{Continuation, EventPayload};
use crate::ports::ClientLink;
use crate::server::Server;

const TRANSPARENT: &str = "minecraft:dragon_transparent";
const IMMUNE: &str = "minecraft:dragon_immune";
const DESTROY_EVENT: i32 = 2008;

impl<L: ClientLink> Server<L> {
    pub(super) fn dragon_break_walls(&mut self, id: EntityId) {
        let Some(runtime) = self.entities.dragons.get(&id) else {
            return;
        };
        let boxes: Vec<Aabb> = [Part::Head, Part::Neck, Part::Body]
            .into_iter()
            .map(|part| parts::part_box(part, runtime.parts[part.index()]))
            .collect();
        let mut blocked = false;
        for area in boxes {
            blocked |= self.dragon_check_walls(id, &area);
        }
        if let Some(entity) = self.entities.store.get_mut(id)
            && let Some(state) = state_of(entity)
        {
            state.in_wall = blocked;
        }
    }

    fn dragon_check_walls(&mut self, id: EntityId, area: &Aabb) -> bool {
        let (x0, x1) = (mth::floor(area.min_x), mth::floor(area.max_x));
        let (y0, y1) = (mth::floor(area.min_y), mth::floor(area.max_y));
        let (z0, z1) = (mth::floor(area.min_z), mth::floor(area.max_z));
        let griefing = self.config.rules.mob_griefing;
        let mut blocked = false;
        let mut blocks = Vec::new();
        for x in x0..=x1 {
            for y in y0..=y1 {
                for z in z0..=z1 {
                    let pos = BlockPos::new(x, y, z);
                    let Some(state) = self.world.chunks.block(pos) else {
                        continue;
                    };
                    if state.is_air() || has_tag(state, TRANSPARENT) {
                        continue;
                    }
                    if !griefing || has_tag(state, IMMUNE) {
                        blocked = true;
                    } else {
                        blocks.push(pos);
                    }
                }
            }
        }
        if blocks.is_empty() {
            return blocked;
        }
        let (dx, dy, dz) = (x1 - x0 + 1, y1 - y0 + 1, z1 - z0 + 1);
        let mut random = ServerRandom(&mut self.rng);
        let at = BlockPos::new(
            x0 + random.next_int(dx),
            y0 + random.next_int(dy),
            z0 + random.next_int(dz),
        );
        let Some(view) = self.entity_view(id) else {
            return blocked;
        };
        let step = DragonWallStep {
            dragon: id,
            blocks: blocks.clone(),
            sound_at: at,
        };
        let lane = self.lane_for(None, id);
        self.fire(
            lane,
            Continuation::DragonWalls(Box::new(step)),
            move |server| {
                let location =
                    server.location_of(view.position, rspigot_domain::Rotation::default());
                Some(EventPayload::EntityExplode(EntityExplode {
                    block_list: blocks,
                    entity_type: view.kind.clone(),
                    entity: view,
                    explosion_result: ExplosionResult::Destroy,
                    location,
                    explosion_yield: 0.0,
                }))
            },
        );
        blocked
    }

    pub(crate) fn finish_dragon_walls(
        &mut self,
        step: DragonWallStep,
        cancelled: bool,
        patch: EntityExplodePatch,
    ) {
        let DragonWallStep {
            dragon,
            blocks,
            sound_at,
        } = step;
        if !cancelled {
            let blocks = patch.block_list.apply(blocks);
            let explosion_yield = patch.explosion_yield.unwrap_or(0.0);
            if explosion_yield == 0.0 {
                for pos in blocks {
                    self.set_block(pos, BlockStateId::AIR);
                }
            } else {
                self.blow_up_blocks(&blocks, Some(dragon), Some(1.0 / explosion_yield));
            }
        }
        self.level_event(DESTROY_EVENT, sound_at, 0);
    }
}
