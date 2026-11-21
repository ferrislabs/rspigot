use std::time::Duration;

use rspigot_data::map_colors::decoration_type_id;
use rspigot_domain::cartography::{self, ExplorationSpec, MAP_ID, MapSource};
use rspigot_domain::item::ItemStack;
use rspigot_domain::map_data::{MAX_SCALE, MapData};
use rspigot_domain::{BlockPos, Vec3};

use crate::chunks::{QueryAnswer, WorldQuery};
use crate::locate;
use crate::ports::ClientLink;
use crate::server::Server;

const QUERY_TIMEOUT: Duration = Duration::from_secs(30);
const CHUNK_SHIFT: i32 = 4;

pub(crate) struct Explorer<'a, L: ClientLink> {
    pub server: &'a mut Server<L>,
    pub origin: Vec3,
}

impl<L: ClientLink> MapSource for Explorer<'_, L> {
    fn explore(&mut self, spec: &ExplorationSpec, stack: ItemStack) -> ItemStack {
        self.server.explore_map(self.origin, spec, stack)
    }
}

fn within(origin: Vec3, at: BlockPos, chunks: i32) -> bool {
    let from = BlockPos::new(origin.x.floor() as i32, 0, origin.z.floor() as i32);
    let reach = |a: i32, b: i32| ((a >> CHUNK_SHIFT) - (b >> CHUNK_SHIFT)).abs();
    reach(at.x, from.x).max(reach(at.z, from.z)) <= chunks
}

impl<L: ClientLink> Server<L> {
    fn nearest_map_structure(
        &mut self,
        origin: Vec3,
        destination: &str,
        search_radius: i32,
    ) -> Option<BlockPos> {
        let structures = locate::resolve_structures(&format!("#{destination}")).ok()?;
        let answer = self.workers.request_query(
            &self.world.dimension,
            origin,
            WorldQuery::Structures(structures),
        );
        match answer.recv_timeout(QUERY_TIMEOUT) {
            Ok(QueryAnswer::Located(Some((_, at)))) => {
                let pos = BlockPos::new(
                    at.x.floor() as i32,
                    at.y.floor() as i32,
                    at.z.floor() as i32,
                );
                within(origin, pos, search_radius).then_some(pos)
            }
            _ => None,
        }
    }

    pub(crate) fn explore_map(
        &mut self,
        origin: Vec3,
        spec: &ExplorationSpec,
        mut stack: ItemStack,
    ) -> ItemStack {
        let Some(target) =
            self.nearest_map_structure(origin, &spec.destination, spec.search_radius)
        else {
            return stack;
        };
        if decoration_type_id(&spec.decoration).is_none() {
            return stack;
        }
        let scale = u8::try_from(spec.zoom.clamp(0, i32::from(MAX_SCALE))).unwrap_or(0);
        let data = MapData::fresh(
            f64::from(target.x),
            f64::from(target.z),
            scale,
            true,
            true,
            self.world.dimension.key.as_str(),
        );
        let id = self.new_map(data);
        stack.components.insert(MAP_ID, id);
        cartography::add_target_decoration(&mut stack, &spec.decoration, target.x, target.z);
        stack
    }

    pub(crate) fn resolve_loot_maps(
        &mut self,
        origin: Vec3,
        stacks: Vec<ItemStack>,
    ) -> Vec<ItemStack> {
        let mut resolved = Vec::with_capacity(stacks.len());
        for mut stack in stacks {
            let Some(spec) = cartography::take_pending_exploration(&mut stack) else {
                resolved.push(stack);
                continue;
            };
            let stack = self.explore_map(origin, &spec, stack);
            if stack.components.contains_key(MAP_ID) {
                resolved.push(stack);
            }
        }
        resolved
    }
}
