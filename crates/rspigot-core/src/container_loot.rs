use std::num::NonZeroU8;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rspigot_domain::item::{ItemStack, Slot};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Rotation, Vec3};
use rspigot_entities::EntityKind as Kind;

use crate::containers::opening_of;
use crate::loot::{self, LootContext, LootEntity};
use crate::plugins::events::{LootGenerate, LootGeneratePatch};
use crate::plugins::{
    Continuation, EventPayload, HolderRef, ItemSpec, LootContext as ViewedContext, spec_of,
    view_of_entity,
};
use crate::plugins::{Lane, stack_of};
use crate::ports::ClientLink;
use crate::server::Server;
use crate::vehicles::{cargo_mut, cargo_of};

const LOOT_TABLE: &str = "LootTable";
const LOOT_TABLE_SEED: &str = "LootTableSeed";
const LOOT_LUCK: f32 = 0.0;

#[derive(Debug, Clone, Copy)]
pub(crate) enum LootSite {
    Block(BlockPos),
    Cargo(EntityId),
}

#[derive(Debug)]
pub(crate) struct LootStep {
    site: LootSite,
    player: PlayerId,
    stacks: Vec<ItemStack>,
    seed: u64,
}

struct Pending {
    table: String,
    seed: i64,
}

fn shuffle_and_split(items: &mut Vec<ItemStack>, slots: usize, rng: &mut StdRng) {
    let mut splittable: Vec<ItemStack> = Vec::new();
    let mut whole: Vec<ItemStack> = Vec::new();
    for stack in items.drain(..) {
        if stack.count.get() > 1 {
            splittable.push(stack);
        } else {
            whole.push(stack);
        }
    }
    while slots.saturating_sub(whole.len() + splittable.len()) > 0 && !splittable.is_empty() {
        let index = rng.random_range(0..splittable.len());
        let mut stack = splittable.remove(index);
        let taken = rng.random_range(1..=(stack.count.get() / 2).max(1));
        let Some(count) = NonZeroU8::new(stack.count.get() - taken) else {
            whole.push(stack);
            continue;
        };
        let part = ItemStack {
            count: NonZeroU8::new(taken).unwrap_or(NonZeroU8::MIN),
            ..stack.clone()
        };
        stack.count = count;
        for piece in [stack, part] {
            if piece.count.get() >= 2 && rng.random_bool(0.5) {
                splittable.push(piece);
            } else {
                whole.push(piece);
            }
        }
    }
    whole.append(&mut splittable);
    for index in (1..whole.len()).rev() {
        whole.swap(index, rng.random_range(0..=index));
    }
    *items = whole;
}

fn fill_empty_slots(container: &mut [Slot], mut stacks: Vec<ItemStack>, rng: &mut StdRng) {
    let mut free: Vec<usize> = container
        .iter()
        .enumerate()
        .filter_map(|(slot, held)| held.is_none().then_some(slot))
        .collect();
    for index in (1..free.len()).rev() {
        free.swap(index, rng.random_range(0..=index));
    }
    shuffle_and_split(&mut stacks, free.len(), rng);
    for stack in stacks {
        let Some(slot) = free.pop() else {
            tracing::warn!("Tried to over-fill a container");
            return;
        };
        container[slot] = Some(stack);
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn unpack_block_loot(
        &mut self,
        player: PlayerId,
        pos: BlockPos,
        state: BlockStateId,
    ) -> bool {
        let partner = crate::chests::pair_of(pos, state, |at| self.world.chunks.block(at))
            .map(|(right, left)| if pos == right { left } else { right });
        [Some(pos), partner]
            .into_iter()
            .flatten()
            .any(|at| self.begin_block_loot(player, at))
    }

    fn begin_block_loot(&mut self, player: PlayerId, pos: BlockPos) -> bool {
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return false;
        };
        let Some(entity) = chunk.block_entity(pos) else {
            return false;
        };
        let Some(table) = entity.data.get_str(LOOT_TABLE).map(str::to_owned) else {
            return false;
        };
        let seed = entity.data.get_i64(LOOT_TABLE_SEED).unwrap_or(0);
        let mut updated = entity.clone();
        updated.data.remove(LOOT_TABLE);
        updated.data.remove(LOOT_TABLE_SEED);
        chunk.set_block_entity(updated);
        self.generate_loot(player, LootSite::Block(pos), Pending { table, seed })
    }

    pub(crate) fn unpack_cargo_loot(&mut self, player: PlayerId, vehicle: EntityId) -> bool {
        let Some(cargo) = self
            .entities
            .store
            .get_mut(vehicle)
            .and_then(cargo_mut)
            .filter(|cargo| cargo.loot_table.is_some())
        else {
            return false;
        };
        let pending = Pending {
            table: cargo.loot_table.take().unwrap_or_default(),
            seed: std::mem::take(&mut cargo.loot_seed),
        };
        self.generate_loot(player, LootSite::Cargo(vehicle), pending)
    }

    fn generate_loot(&mut self, player: PlayerId, site: LootSite, pending: Pending) -> bool {
        let Some(table) = rspigot_data::loot::table(&pending.table) else {
            tracing::warn!("Unknown loot table {}", pending.table);
            return false;
        };
        let seed = if pending.seed == 0 {
            self.rng.random()
        } else {
            pending.seed.cast_unsigned()
        };
        let this = LootEntity::of(Kind::Player);
        let context = LootContext {
            this: Some(&this),
            luck: LOOT_LUCK,
            ..LootContext::default()
        };
        let stacks = loot::table_drops(table, &context, &mut StdRng::seed_from_u64(seed));
        let step = LootStep {
            site,
            player,
            stacks,
            seed,
        };
        self.offer_loot(step, pending.table);
        true
    }

    fn offer_loot(&mut self, step: LootStep, table: String) {
        let player = step.player;
        let site = step.site;
        let loot: Vec<ItemSpec> = step.stacks.iter().map(spec_of).collect();
        self.fire(
            Lane::Player(player),
            Continuation::LootGenerate(Box::new(step)),
            move |server| {
                let entity = server
                    .players
                    .get(&player)
                    .and_then(|p| server.subject_view(p.entity_id));
                let (holder, origin) = match site {
                    LootSite::Block(pos) => (
                        HolderRef::Block(pos),
                        Vec3::new(
                            f64::from(pos.x) + 0.5,
                            f64::from(pos.y) + 0.5,
                            f64::from(pos.z) + 0.5,
                        ),
                    ),
                    LootSite::Cargo(id) => {
                        let vehicle = server.entities.store.get(id)?;
                        (
                            HolderRef::Entity(view_of_entity(vehicle)),
                            vehicle.base.position,
                        )
                    }
                };
                Some(EventPayload::LootGenerate(LootGenerate {
                    entity: entity.clone(),
                    inventory_holder: Some(holder),
                    loot,
                    loot_context: ViewedContext {
                        location: server.location_of(origin, Rotation::new(0.0, 0.0)),
                        luck: LOOT_LUCK,
                        killer: None,
                        looted: entity,
                    },
                    loot_table: table,
                    plugin: false,
                    world: server.world_name(),
                }))
            },
        );
    }

    pub(crate) fn finish_loot_generate(
        &mut self,
        step: LootStep,
        cancelled: bool,
        patch: LootGeneratePatch,
    ) {
        let LootStep {
            site,
            player,
            stacks,
            seed,
        } = step;
        if !cancelled {
            let specs = patch.loot.apply(stacks.iter().map(spec_of).collect());
            let stacks: Vec<ItemStack> = specs
                .iter()
                .filter_map(|spec| stack_of(spec).ok().flatten())
                .collect();
            let mut rng = StdRng::seed_from_u64(seed.wrapping_add(1));
            self.store_loot(site, stacks, &mut rng);
        }
        self.reopen_after_loot(player, site);
    }

    fn store_loot(&mut self, site: LootSite, stacks: Vec<ItemStack>, rng: &mut StdRng) {
        match site {
            LootSite::Block(pos) => {
                let Some(state) = self.world.chunks.block(pos) else {
                    return;
                };
                let Some(opening) = opening_of(state) else {
                    return;
                };
                let Some(entity) = opening.entity else {
                    return;
                };
                let mut items = self.half_items(pos, opening.kind.container_size());
                fill_empty_slots(&mut items, stacks, rng);
                self.store_half(pos, entity, &items);
            }
            LootSite::Cargo(id) => {
                let Some(cargo) = self.entities.store.get_mut(id).and_then(cargo_mut) else {
                    return;
                };
                fill_empty_slots(&mut cargo.items, stacks, rng);
            }
        }
    }

    fn reopen_after_loot(&mut self, player: PlayerId, site: LootSite) {
        if !self.players.contains_key(&player) {
            return;
        }
        match site {
            LootSite::Block(pos) => {
                if let Some(state) = self.world.chunks.block(pos) {
                    self.open_block_menu(player, pos, state);
                }
            }
            LootSite::Cargo(id) => {
                if self.entities.store.get(id).and_then(cargo_of).is_some() {
                    self.open_cargo(player, id);
                }
            }
        }
    }
}
