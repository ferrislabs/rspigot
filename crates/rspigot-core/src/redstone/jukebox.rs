use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::entity::save::{item_stack_from_nbt, item_stack_to_nbt};
use rspigot_domain::game_event::GameEvent;
use rspigot_domain::item::{ItemStack, Slot, with_count};
use rspigot_domain::jukebox::{self, Song};
use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, BlockStateId, Identifier, Vec3};
use rspigot_nbt::{Compound, Tag};

use crate::events::Hand;
use crate::game_events::GameEventContext;
use crate::items::PICKUP_DELAY;
use crate::ports::ClientLink;
use crate::server::Server;
use crate::signal::flag;

const ENTITY: &str = "jukebox";
const RECORD: &str = "RecordItem";
const TICKS: &str = "ticks_since_song_started";
const HAS_RECORD: &str = "has_record";
const POP_SPREAD: f64 = 0.7;
const POP_HEIGHT: f64 = 1.01;
const POP_JITTER: f64 = 0.2;

fn is_jukebox(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::Jukebox)
}

impl<L: ClientLink> Server<L> {
    fn jukebox_data(&self, pos: BlockPos) -> Option<&Compound> {
        self.world
            .chunks
            .get(pos.chunk())
            .and_then(|chunk| chunk.block_entity(pos))
            .map(|entity| &entity.data)
    }

    pub(crate) fn jukebox_record(&self, pos: BlockPos) -> Slot {
        self.jukebox_data(pos)
            .and_then(|data| data.get_compound(RECORD))
            .and_then(item_stack_from_nbt)
    }

    fn jukebox_ticks(&self, pos: BlockPos) -> Option<i64> {
        self.jukebox_data(pos).and_then(|data| data.get_i64(TICKS))
    }

    pub(crate) fn jukebox_playing(&self, pos: BlockPos) -> bool {
        self.jukebox_ticks(pos).is_some()
    }

    pub(crate) fn jukebox_comparator(&self, pos: BlockPos) -> i32 {
        self.jukebox_record(pos)
            .and_then(|record| jukebox::song_of(&record))
            .map_or(0, |song| song.comparator)
    }

    pub(crate) fn ensure_jukebox_entity(&mut self, pos: BlockPos, state: BlockStateId) {
        if !is_jukebox(state) || self.jukebox_data(pos).is_some() {
            return;
        }
        self.store_jukebox(pos, None, None);
    }

    fn store_jukebox(&mut self, pos: BlockPos, record: Option<&ItemStack>, ticks: Option<i64>) {
        let Some(chunk) = self.world.chunks.chunk_mut(pos.chunk()) else {
            return;
        };
        let mut data = Compound::new();
        if let Some(record) = record {
            data.insert(RECORD, item_stack_to_nbt(record));
        }
        if let Some(ticks) = ticks {
            data.insert(TICKS, Tag::Long(ticks));
        }
        chunk.set_block_entity(BlockEntity {
            pos,
            kind: Identifier::minecraft(ENTITY),
            data,
        });
    }

    fn jukebox_changed(&mut self, pos: BlockPos) {
        self.update_neighbors_at(pos);
        self.update_neighbour_for_output_signal(pos);
    }

    fn stop_song(&mut self, pos: BlockPos) {
        if self.jukebox_ticks(pos).is_none() {
            return;
        }
        let record = self.jukebox_record(pos);
        self.store_jukebox(pos, record.as_ref(), None);
        let state = self.block_or_air(pos);
        self.block_game_event(
            GameEvent::JukeboxStopPlay,
            pos,
            GameEventContext::block(state),
        );
        self.level_event(jukebox::STOP_EVENT, pos, 0);
        self.jukebox_changed(pos);
    }

    fn start_song(&mut self, pos: BlockPos, song: &Song) {
        let record = self.jukebox_record(pos);
        self.store_jukebox(pos, record.as_ref(), Some(0));
        self.level_event(jukebox::START_EVENT, pos, song.registry_id().unwrap_or(0));
        self.jukebox_changed(pos);
    }

    pub(crate) fn set_jukebox_record(&mut self, pos: BlockPos, record: Option<ItemStack>) {
        let state = self.block_or_air(pos);
        if !is_jukebox(state) {
            return;
        }
        let song = record.as_ref().and_then(jukebox::song_of);
        let was_playing = self.jukebox_playing(pos);
        self.store_jukebox(pos, record.as_ref(), was_playing.then_some(0));
        let has = record.is_some();
        if flag(state, HAS_RECORD) != has
            && let Some(next) = state.with_property(HAS_RECORD, if has { "true" } else { "false" })
        {
            self.set_block_quiet(pos, next);
        }
        self.block_game_event(
            GameEvent::BlockChange,
            pos,
            GameEventContext::block(self.block_or_air(pos)),
        );
        match song {
            Some(song) => self.start_song(pos, song),
            None => self.stop_song(pos),
        }
        self.update_neighbour_for_output_signal(pos);
    }

    pub(crate) fn pop_jukebox_record(&mut self, pos: BlockPos) {
        let Some(record) = self.jukebox_record(pos) else {
            return;
        };
        self.set_jukebox_record(pos, None);
        let spot = Vec3::new(
            f64::from(pos.x) + 0.5 + (self.rng.random::<f64>() - 0.5) * POP_SPREAD,
            f64::from(pos.y) + POP_HEIGHT,
            f64::from(pos.z) + 0.5 + (self.rng.random::<f64>() - 0.5) * POP_SPREAD,
        );
        let velocity = Vec3::new(
            self.rng.random::<f64>() * POP_JITTER - POP_JITTER / 2.0,
            POP_JITTER,
            self.rng.random::<f64>() * POP_JITTER - POP_JITTER / 2.0,
        );
        self.spawn_item_entity(spot, velocity, PICKUP_DELAY, record);
    }

    pub(crate) fn jukebox_removed(
        &mut self,
        pos: BlockPos,
        old: BlockStateId,
        record: Option<ItemStack>,
    ) {
        if let Some(record) = record {
            let spot = Vec3::new(
                f64::from(pos.x) + 0.5 + (self.rng.random::<f64>() - 0.5) * POP_SPREAD,
                f64::from(pos.y) + POP_HEIGHT,
                f64::from(pos.z) + 0.5 + (self.rng.random::<f64>() - 0.5) * POP_SPREAD,
            );
            let velocity = Vec3::new(0.0, POP_JITTER, 0.0);
            self.spawn_item_entity(spot, velocity, PICKUP_DELAY, record);
        }
        self.block_game_event(
            GameEvent::JukeboxStopPlay,
            pos,
            GameEventContext::block(old),
        );
        self.level_event(jukebox::STOP_EVENT, pos, 0);
    }

    pub(crate) fn use_jukebox(
        &mut self,
        id: PlayerId,
        hand: Hand,
        held: Option<&ItemStack>,
        pos: BlockPos,
        clicked: BlockStateId,
        sequence: i32,
    ) -> bool {
        if !is_jukebox(clicked) {
            return false;
        }
        if flag(clicked, HAS_RECORD) {
            self.pop_jukebox_record(pos);
        } else if let Some(disc) = held.filter(|stack| jukebox::is_playable(stack)) {
            let one = with_count(disc, 1);
            self.consume_held(id, hand, 1);
            self.set_jukebox_record(pos, one);
            self.award_custom(id, "minecraft:play_record", 1);
            self.refresh_hands(id);
        } else {
            return false;
        }
        self.player_swing(id, hand, false);
        self.ack_block(id, sequence);
        true
    }

    pub(crate) fn tick_jukeboxes(&mut self) {
        let mut found: Vec<BlockPos> = Vec::new();
        for chunk_pos in self
            .world
            .chunks
            .chunks_with_block_entities(|kind| kind.as_str() == "minecraft:jukebox")
        {
            let Some(chunk) = self.world.chunks.get(chunk_pos) else {
                continue;
            };
            found.extend(
                chunk
                    .block_entities
                    .iter()
                    .filter(|entity| entity.kind.as_str() == "minecraft:jukebox")
                    .map(|entity| entity.pos),
            );
        }
        for pos in found {
            self.jukebox_tick(pos);
        }
    }

    fn jukebox_tick(&mut self, pos: BlockPos) {
        let state = self.block_or_air(pos);
        if !is_jukebox(state) || !flag(state, HAS_RECORD) {
            return;
        }
        let Some(ticks) = self.jukebox_ticks(pos) else {
            return;
        };
        let Some(song) = self
            .jukebox_record(pos)
            .and_then(|record| jukebox::song_of(&record))
        else {
            return;
        };
        if song.has_finished(ticks) {
            return self.stop_song(pos);
        }
        if jukebox::emits_note(ticks) {
            self.block_game_event(GameEvent::JukeboxPlay, pos, GameEventContext::block(state));
        }
        let record = self.jukebox_record(pos);
        self.store_jukebox(pos, record.as_ref(), Some(ticks + 1));
    }
}
