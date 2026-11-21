use rand::SeedableRng;
use rand::rngs::StdRng;
use rspigot_blocks::{BlockKind, Face};
use rspigot_domain::brushing::{self, BrushState, Brushed};
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::entity::raycast::{self, BlockHit};
use rspigot_domain::entity::save::{item_stack_from_nbt, item_stack_to_nbt};
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, Identifier, ItemId, Vec3, mth};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_items::ItemKind;
use rspigot_nbt::Compound;

use crate::bow::{UsingItem, hand_slot};
use crate::events::Hand;
use crate::loot::{self, LootContext, LootEntity};
use crate::mobs::ServerRandom;
use crate::ports::ClientLink;
use crate::server::Server;

const ENTITY_KIND: &str = "brushable_block";
const LOOT_TABLE: &str = "LootTable";
const LOOT_TABLE_SEED: &str = "LootTableSeed";
const ITEM: &str = "item";
const HIT_DIRECTION: &str = "hit_direction";
const PROGRESS: &str = "rspigot_brush";
const COUNT: &str = "count";
const LAST_BRUSHED_AT: &str = "last_brushed_at";
const COOLDOWN_ENDS_AT: &str = "cooldown_ends_at";
const DUSTED: &str = "dusted";
const SURVIVAL_REACH: f64 = 4.5;
const CREATIVE_REACH: f64 = 5.0;
const PICKUP_DELAY: u16 = 10;

pub(crate) fn is_brush(item: ItemId) -> bool {
    item == ItemKind::Brush.id()
}

fn brushable(kind: BlockKind) -> bool {
    matches!(
        kind,
        BlockKind::SuspiciousSand | BlockKind::SuspiciousGravel
    )
}

fn turns_into(kind: BlockKind) -> BlockKind {
    match kind {
        BlockKind::SuspiciousGravel => BlockKind::Gravel,
        _ => BlockKind::Sand,
    }
}

fn brush_sound(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::SuspiciousSand => brushing::SAND_SOUND,
        BlockKind::SuspiciousGravel => brushing::GRAVEL_SOUND,
        _ => brushing::GENERIC_SOUND,
    }
}

fn face_name(face: Face) -> &'static str {
    match face {
        Face::Down => "down",
        Face::Up => "up",
        Face::North => "north",
        Face::South => "south",
        Face::West => "west",
        Face::East => "east",
    }
}

fn face_named(name: &str) -> Face {
    match name {
        "down" => Face::Down,
        "north" => Face::North,
        "south" => Face::South,
        "west" => Face::West,
        "east" => Face::East,
        _ => Face::Up,
    }
}

fn read_state(data: &Compound) -> BrushState {
    let Some(progress) = data.get_compound(PROGRESS) else {
        return BrushState::default();
    };
    BrushState {
        count: progress.get_i32(COUNT).unwrap_or(0).max(0) as u32,
        last_brushed_at: progress.get_i64(LAST_BRUSHED_AT).unwrap_or(0),
        cooldown_ends_at: progress.get_i64(COOLDOWN_ENDS_AT).unwrap_or(0),
    }
}

fn write_state(data: &mut Compound, state: &BrushState) {
    if state.count == 0 {
        data.remove(PROGRESS);
        return;
    }
    let progress = Compound::new()
        .with(COUNT, state.count as i32)
        .with(LAST_BRUSHED_AT, state.last_brushed_at)
        .with(COOLDOWN_ENDS_AT, state.cooldown_ends_at);
    data.insert(PROGRESS, progress);
}

impl<L: ClientLink> Server<L> {
    fn brush_target(&self, id: PlayerId) -> Option<BlockHit> {
        let player = self.players.get(&id)?;
        let reach = if player.game_mode == GameMode::Creative {
            CREATIVE_REACH
        } else {
            SURVIVAL_REACH
        };
        let eye = player.eye_position();
        let look = mth::view_vector(player.rotation.pitch, player.rotation.yaw);
        raycast::clip_blocks(&self.world.chunks, eye, eye + look.scale(reach))
    }

    pub fn use_brush(&mut self, id: PlayerId, hand: Hand) -> bool {
        let tick = self.current_tick();
        let Some(player) = self.players.get(&id) else {
            return false;
        };
        let held = player
            .inventory
            .get(hand_slot(player, hand))
            .is_some_and(|stack| is_brush(stack.item));
        if !held
            || player.vitals.is_dead()
            || player.game_mode == GameMode::Spectator
            || player.using_item.is_some()
            || self.brush_target(id).is_none()
        {
            return false;
        }
        if let Some(player) = self.players.get_mut(&id) {
            player.using_item = Some(UsingItem { hand, since: tick });
        }
        self.show_use_status(id);
        true
    }

    pub(crate) fn tick_brush_use(&mut self, id: PlayerId, using: UsingItem, item: ItemId) {
        if !is_brush(item) {
            return;
        }
        let tick = self.current_tick();
        let used = i32::try_from(tick.saturating_sub(using.since)).unwrap_or(i32::MAX) + 1;
        let Some(hit) = self
            .brush_target(id)
            .filter(|_| used <= brushing::USE_DURATION)
        else {
            self.stop_using_now(id);
            return;
        };
        if !brushing::is_brush_tick(used) {
            return;
        }
        let Some(kind) = self
            .world
            .chunks
            .block(hit.pos)
            .and_then(|state| state.kind())
        else {
            return;
        };
        self.play_sound(
            Some(id),
            hit.pos.center(),
            brush_sound(kind),
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        if brushable(kind) {
            self.brush_block(id, using.hand, hit);
        }
    }

    fn brush_data(&self, pos: BlockPos) -> Compound {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .map(|entity| entity.data.clone())
            .unwrap_or_default()
    }

    fn store_brush_data(&mut self, pos: BlockPos, data: Compound) {
        if let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) {
            chunk.set_block_entity(BlockEntity {
                pos,
                kind: Identifier::minecraft(ENTITY_KIND),
                data,
            });
        }
    }

    fn unpack_brush_loot(&mut self, id: PlayerId, data: &mut Compound) {
        let Some(table) = data.get_str(LOOT_TABLE).map(str::to_owned) else {
            return;
        };
        let seed = data.get_i64(LOOT_TABLE_SEED).unwrap_or(0);
        data.remove(LOOT_TABLE);
        data.remove(LOOT_TABLE_SEED);
        let Some(loaded) = rspigot_data::loot::table(&table) else {
            tracing::warn!("Unknown loot table {table}");
            return;
        };
        let seed = if seed == 0 {
            rand::Rng::random(&mut self.rng)
        } else {
            seed.cast_unsigned()
        };
        let this = LootEntity::of(Kind::Player);
        let context = LootContext {
            this: Some(&this),
            ..LootContext::default()
        };
        let stacks = loot::table_drops(loaded, &context, &mut StdRng::seed_from_u64(seed));
        self.trigger_loot_generated(id, &table);
        if let Some(first) = stacks.first() {
            data.insert(ITEM, item_stack_to_nbt(first));
        }
    }

    pub(crate) fn brush_block(&mut self, id: PlayerId, hand: Hand, hit: BlockHit) {
        let pos = hit.pos;
        let now = self.current_tick() as i64;
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        let mut data = self.brush_data(pos);
        let mut progress = read_state(&data);
        progress.catch_up(now);
        let before = state
            .property(DUSTED)
            .and_then(|value| value.parse::<u8>().ok())
            .unwrap_or(0);
        self.unpack_brush_loot(id, &mut data);
        data.insert(HIT_DIRECTION, face_name(hit.face));
        let outcome = progress.brush(now);
        write_state(&mut data, &progress);
        match outcome {
            Brushed::Completed => self.finish_brushing(id, hand, pos, state, data),
            Brushed::Ignored | Brushed::Progress { .. } => {
                self.store_brush_data(pos, data);
                let stage = progress.completion();
                if stage != before
                    && let Some(dusted) = state.with_property(DUSTED, &stage.to_string())
                {
                    self.set_block(pos, dusted);
                }
            }
        }
    }

    fn finish_brushing(
        &mut self,
        id: PlayerId,
        hand: Hand,
        pos: BlockPos,
        state: rspigot_domain::BlockStateId,
        data: Compound,
    ) {
        self.drop_brushed_item(pos, &data);
        self.level_event(brushing::COMPLETED_EVENT, pos, i32::from(state.0));
        let replacement = state
            .kind()
            .map_or(BlockKind::Sand, turns_into)
            .default_state();
        self.set_block(pos, replacement);
        if let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) {
            chunk.remove_block_entity(pos);
        }
        self.wear(id, hand, 1);
    }

    fn drop_brushed_item(&mut self, pos: BlockPos, data: &Compound) {
        let Some(mut stack) = data.get_compound(ITEM).and_then(item_stack_from_nbt) else {
            return;
        };
        let wanted = brushing::drop_count(&mut ServerRandom(&mut self.rng));
        stack.count =
            std::num::NonZeroU8::new(stack.count.get().min(wanted)).unwrap_or(stack.count);
        let face = face_named(data.get_str(HIT_DIRECTION).unwrap_or("up"));
        let (dx, dy, dz) = face.offset();
        let at = pos.offset(dx, dy, dz);
        let margin = 1.0 - brushing::ITEM_SIZE;
        let position = Vec3::new(
            f64::from(at.x) + 0.5 * margin + brushing::ITEM_SIZE / 2.0,
            f64::from(at.y) + 0.5 + brushing::ITEM_SIZE / 2.0,
            f64::from(at.z) + 0.5 * margin + brushing::ITEM_SIZE / 2.0,
        );
        self.spawn_item_entity(position, Vec3::ZERO, PICKUP_DELAY, stack);
    }

    pub fn brush_progress(&self, pos: BlockPos) -> u32 {
        read_state(&self.brush_data(pos)).count
    }

    pub fn held_damage(&self, id: PlayerId) -> Option<i32> {
        let stack = self.players.get(&id)?.inventory.held()?;
        Some(stack.components.get_i32("minecraft:damage").unwrap_or(0))
    }

    pub fn brushed_stack(&self, pos: BlockPos) -> Option<ItemStack> {
        self.brush_data(pos)
            .get_compound(ITEM)
            .and_then(item_stack_from_nbt)
    }
}
