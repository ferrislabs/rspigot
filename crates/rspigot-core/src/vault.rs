use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::chunk::BlockEntity;
use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::vault::{self, Config, ServerData, SharedData, State};
use rspigot_domain::{BlockPos, BlockStateId, Identifier};
use rspigot_entities::{EntityKind as Kind, SoundSource};
use rspigot_nbt::Compound;
use uuid::Uuid;

use crate::events::{ClientEvent, Hand};
use crate::loot::{self, LootContext, LootEntity};
use crate::plugins::events::{VaultDisplayItem, VaultDisplayItemPatch};
use crate::plugins::{Continuation, EventPayload, Lane, spec_of, stack_of};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use crate::spawner::middle;
use crate::trial_spawner::{Dispensing, LootDispenseStep};

const STATE: &str = "vault_state";
const OMINOUS: &str = "ominous";
const BLOCK_ENTITY: &str = "minecraft:vault";

pub(crate) fn is_vault(state: BlockStateId) -> bool {
    state.kind() == Some(BlockKind::Vault)
}

fn state_of(block: BlockStateId) -> State {
    block
        .property(STATE)
        .and_then(State::from_name)
        .unwrap_or(State::Inactive)
}

#[derive(Debug)]
pub(crate) struct VaultRun {
    pos: BlockPos,
    config: Config,
    shared: SharedData,
    server: ServerData,
    shared_dirty: bool,
    server_dirty: bool,
}

#[derive(Debug)]
pub(crate) struct VaultDisplayStep {
    pos: BlockPos,
    item: Option<ItemStack>,
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn ensure_vault_entity(&mut self, pos: BlockPos, state: BlockStateId) {
        if !is_vault(state) {
            return;
        }
        let present = self
            .world
            .chunks
            .get(pos.chunk())
            .is_some_and(|chunk| chunk.block_entity(pos).is_some());
        if !present {
            self.store_vault(&VaultRun {
                pos,
                config: Config::default(),
                shared: SharedData::default(),
                server: ServerData::default(),
                shared_dirty: false,
                server_dirty: false,
            });
        }
    }

    pub(crate) fn vault_at(&self, pos: BlockPos) -> Option<VaultRun> {
        self.world.chunks.block(pos).filter(|s| is_vault(*s))?;
        let entity = self.world.chunks.get(pos.chunk())?.block_entity(pos)?;
        let part = |key: &str| entity.data.get_compound(key).cloned().unwrap_or_default();
        Some(VaultRun {
            pos,
            config: Config::read(&part("config")),
            shared: SharedData::read(&part("shared_data")),
            server: ServerData::read(&part("server_data")),
            shared_dirty: false,
            server_dirty: false,
        })
    }

    fn store_vault(&mut self, run: &VaultRun) {
        let Some(chunk) = self.world.chunks.chunk_mut(run.pos.chunk()) else {
            return;
        };
        let (kind, mut data) = chunk.block_entity(run.pos).map_or_else(
            || (Identifier::minecraft(vault::ENTITY), Compound::new()),
            |existing| (existing.kind.clone(), existing.data.clone()),
        );
        data.insert("config", run.config.write());
        data.insert("shared_data", run.shared.write());
        data.insert("server_data", run.server.write());
        chunk.set_block_entity(BlockEntity {
            pos: run.pos,
            kind,
            data,
        });
    }

    fn send_vault_update(&mut self, run: &VaultRun) {
        self.push(
            Audience::ChunkViewers(run.pos.chunk()),
            ClientEvent::BlockEntityUpdate {
                pos: run.pos,
                kind: BLOCK_ENTITY,
                data: Compound::new().with("shared_data", run.shared.write()),
            },
        );
    }

    pub(crate) fn tick_vaults(&mut self) {
        let mut found: Vec<BlockPos> = Vec::new();
        for chunk_pos in self
            .world
            .chunks
            .chunks_with_block_entities(|kind| kind.path() == vault::ENTITY)
        {
            let Some(chunk) = self.world.chunks.get(chunk_pos) else {
                continue;
            };
            let mut here = chunk
                .block_entities
                .iter()
                .filter(|entity| entity.kind.path() == vault::ENTITY)
                .map(|entity| entity.pos)
                .peekable();
            if here.peek().is_some() && self.entity_ticking(chunk_pos) {
                found.extend(here);
            }
        }
        for pos in found {
            self.tick_vault(pos);
        }
    }

    fn tick_vault(&mut self, pos: BlockPos) {
        let Some(block) = self.world.chunks.block(pos).filter(|s| is_vault(*s)) else {
            return;
        };
        let Some(mut run) = self.vault_at(pos) else {
            return;
        };
        let state = state_of(block);
        let now = self.world.level.game_time;
        if now % vault::DISPLAY_CYCLE_INTERVAL == 0 && state == State::Active {
            self.cycle_vault_display(&mut run);
        }
        let mut next = state;
        if run.server.state_updating_resumes_at <= now {
            next = self.vault_next_state(&mut run, state);
        }
        self.store_vault(&run);
        if next != state {
            self.set_vault_state(&mut run, state, next);
        }
        if run.shared_dirty {
            let again = self.vault_at(pos).unwrap_or(run);
            self.send_vault_update(&again);
        }
    }

    fn vault_next_state(&mut self, run: &mut VaultRun, state: State) -> State {
        let now = self.world.level.game_time;
        match state {
            State::Inactive => self.vault_refresh_players(run, run.config.activation_range),
            State::Active => self.vault_refresh_players(run, run.config.deactivation_range),
            State::Unlocking => {
                run.server.state_updating_resumes_at = now + vault::EJECT_DELAY;
                run.server_dirty = true;
                State::Ejecting
            }
            State::Ejecting => {
                if run.server.items_to_eject.is_empty() {
                    run.server.mark_ejection_finished();
                    run.server_dirty = true;
                    return self.vault_refresh_players(run, run.config.deactivation_range);
                }
                let progress = run.server.ejection_progress();
                if let Some(item) = run.server.pop_next_item_to_eject() {
                    self.eject_vault_item(run.pos, item, progress);
                }
                let shown = run.server.next_item_to_eject().cloned();
                run.shared_dirty |= run.shared.set_display_item(shown);
                run.server.state_updating_resumes_at = now + vault::EJECT_DELAY;
                run.server_dirty = true;
                State::Ejecting
            }
        }
    }

    fn vault_refresh_players(&mut self, run: &mut VaultRun, range: f64) -> State {
        let now = self.world.level.game_time;
        self.vault_connect_players(run, range);
        run.server.state_updating_resumes_at = now + vault::STATE_CHECK_INTERVAL;
        run.server_dirty = true;
        if run.shared.has_connected_players() {
            State::Active
        } else {
            State::Inactive
        }
    }

    fn vault_connect_players(&mut self, run: &mut VaultRun, range: f64) {
        let pos = run.pos;
        let limit = range * range;
        let near: Vec<Uuid> = self
            .players
            .values()
            .filter(|player| {
                !player.vitals.removed
                    && !player.vitals.dead
                    && player.game_mode != GameMode::Spectator
                    && {
                        let at = player.position.block();
                        let (dx, dy, dz) = (
                            f64::from(at.x - pos.x),
                            f64::from(at.y - pos.y),
                            f64::from(at.z - pos.z),
                        );
                        dx * dx + dy * dy + dz * dz < limit
                    }
                    && !run.server.has_rewarded(player.id.0)
            })
            .map(|player| player.id.0)
            .collect();
        run.shared_dirty |= run.shared.connect(near);
    }

    fn set_vault_state(&mut self, run: &mut VaultRun, old: State, new: State) {
        let pos = run.pos;
        let Some(block) = self.world.chunks.block(pos) else {
            return;
        };
        let Some(changed) = block.with_property(STATE, new.name()) else {
            return;
        };
        self.set_block(pos, changed);
        let ominous = changed.property(OMINOUS) == Some("true");
        self.vault_exit(old, pos);
        self.vault_enter(run, new, ominous);
        self.store_vault(run);
    }

    fn vault_exit(&mut self, old: State, pos: BlockPos) {
        if old == State::Ejecting {
            self.play_sound(
                None,
                middle(pos),
                vault::CLOSE_SHUTTER_SOUND,
                SoundSource::Blocks,
                1.0,
                1.0,
            );
        }
    }

    fn vault_enter(&mut self, run: &mut VaultRun, new: State, ominous: bool) {
        let pos = run.pos;
        let flag = i32::from(ominous);
        match new {
            State::Inactive => {
                run.shared_dirty |= run.shared.set_display_item(None);
                self.level_event(vault::DEACTIVATE_EVENT, pos, flag);
            }
            State::Active => {
                if run.shared.display_item.is_none() {
                    self.cycle_vault_display(run);
                }
                self.level_event(vault::ACTIVATE_EVENT, pos, flag);
            }
            State::Unlocking => self.play_sound(
                None,
                middle(pos),
                vault::INSERT_ITEM_SOUND,
                SoundSource::Blocks,
                1.0,
                1.0,
            ),
            State::Ejecting => self.play_sound(
                None,
                middle(pos),
                vault::OPEN_SHUTTER_SOUND,
                SoundSource::Blocks,
                1.0,
                1.0,
            ),
        }
    }

    fn eject_vault_item(&mut self, pos: BlockPos, item: ItemStack, progress: f32) {
        self.eject_item_above(pos, item);
        self.level_event(vault::EJECT_ITEM_EVENT, pos, 0);
        self.play_sound(
            None,
            middle(pos),
            vault::EJECT_SOUND,
            SoundSource::Blocks,
            1.0,
            progress * 0.4 + 0.8,
        );
    }

    fn cycle_vault_display(&mut self, run: &mut VaultRun) {
        let state = self
            .world
            .chunks
            .block(run.pos)
            .map_or(State::Inactive, state_of);
        if !run.config.can_eject_reward(state) {
            run.shared_dirty |= run.shared.set_display_item(None);
            return;
        }
        let item = rspigot_data::loot::table(run.config.displayed_table()).and_then(|table| {
            let stacks = loot::table_drops(table, &LootContext::default(), &mut self.rng);
            match stacks.len() {
                0 => None,
                len => stacks.into_iter().nth(self.rng.random_range(0..len)),
            }
        });
        self.store_vault(run);
        let pos = run.pos;
        let shown = item.clone();
        self.fire(
            Lane::World,
            Continuation::VaultDisplay(Box::new(VaultDisplayStep { pos, item })),
            move |_| {
                Some(EventPayload::VaultDisplayItem(VaultDisplayItem {
                    block: pos,
                    display_item: shown.as_ref().map(spec_of),
                }))
            },
        );
        if let Some(current) = self.vault_at(pos) {
            run.shared = current.shared;
        }
    }

    pub(crate) fn finish_vault_display(
        &mut self,
        step: VaultDisplayStep,
        cancelled: bool,
        patch: VaultDisplayItemPatch,
    ) {
        if cancelled {
            return;
        }
        let item = match patch.display_item {
            Some(Some(spec)) => stack_of(&spec).ok().flatten(),
            Some(None) => None,
            None => step.item,
        };
        let Some(mut run) = self.vault_at(step.pos) else {
            return;
        };
        if run.shared.set_display_item(item) {
            self.store_vault(&run);
            self.send_vault_update(&run);
        }
    }

    pub(crate) fn use_vault(
        &mut self,
        id: PlayerId,
        hand: Hand,
        held: Option<&ItemStack>,
        pos: BlockPos,
        clicked: BlockStateId,
    ) -> bool {
        let Some(stack) = held else {
            return false;
        };
        if !is_vault(clicked) || state_of(clicked) != State::Active {
            return false;
        }
        self.vault_try_insert_key(id, hand, pos, stack);
        self.player_swing(id, hand, false);
        true
    }

    fn vault_try_insert_key(&mut self, id: PlayerId, hand: Hand, pos: BlockPos, stack: &ItemStack) {
        let Some(mut run) = self.vault_at(pos) else {
            return;
        };
        let state = self
            .world
            .chunks
            .block(pos)
            .map_or(State::Inactive, state_of);
        if !run.config.can_eject_reward(state) {
            return;
        }
        if !run.config.is_valid_key(stack) {
            self.vault_fail_sound(&mut run, vault::INSERT_FAIL_SOUND);
            return;
        }
        if run.server.has_rewarded(id.0) {
            self.vault_fail_sound(&mut run, vault::REJECT_SOUND);
            return;
        }
        let this = LootEntity::of(Kind::Player);
        let context = LootContext {
            this: Some(&this),
            tool: Some(stack),
            ..LootContext::default()
        };
        let Some(table) = rspigot_data::loot::table(&run.config.loot_table) else {
            return;
        };
        let stacks = loot::table_drops(table, &context, &mut self.rng);
        if stacks.is_empty() {
            return;
        }
        let count = run
            .config
            .key_item
            .as_ref()
            .map_or(1, |key| key.count.get());
        self.consume_held(id, hand, count);
        self.refresh_hands(id);
        self.offer_loot_dispense(LootDispenseStep {
            pos,
            player: Some(id),
            stacks,
            then: Dispensing::Vault,
        });
    }

    fn vault_fail_sound(&mut self, run: &mut VaultRun, sound: &'static str) {
        let now = self.world.level.game_time;
        if !run.server.can_play_insert_fail(now) {
            return;
        }
        self.play_sound(None, middle(run.pos), sound, SoundSource::Blocks, 1.0, 1.0);
        run.server.last_insert_fail = now;
        self.store_vault(run);
    }

    pub(crate) fn vault_unlock(
        &mut self,
        pos: BlockPos,
        player: Option<PlayerId>,
        stacks: Vec<ItemStack>,
    ) {
        let Some(mut run) = self.vault_at(pos) else {
            return;
        };
        let Some(block) = self.world.chunks.block(pos) else {
            return;
        };
        let now = self.world.level.game_time;
        let state = state_of(block);
        run.server.set_items_to_eject(stacks);
        let shown = run.server.next_item_to_eject().cloned();
        run.shared_dirty |= run.shared.set_display_item(shown);
        run.server.state_updating_resumes_at = now + vault::UNLOCK_DELAY;
        if let Some(player) = player {
            run.server.add_rewarded(player.0);
        }
        let range = run.config.deactivation_range;
        self.vault_connect_players(&mut run, range);
        self.set_vault_state(&mut run, state, State::Unlocking);
        self.store_vault(&run);
        if run.shared_dirty {
            self.send_vault_update(&run);
        }
    }
}
