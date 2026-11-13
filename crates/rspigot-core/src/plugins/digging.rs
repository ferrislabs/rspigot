use rspigot_domain::effect::EffectId;
const GUARDED_BY_PIGLINS: &str = "minecraft:guarded_by_piglins";

use rspigot_domain::item::ItemStack;
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, BlockStateId, EntityId, Vec3};
use rspigot_entities::EntityKind as Kind;
use rspigot_items::ItemKind;
use rustc_hash::FxHashMap;
use uuid::Uuid;

use crate::events::{ClientEvent, Direction};
use crate::game_events::GameEventContext;
use crate::loot::{self, LootContext, LootEntity};
use crate::player::Player;
use crate::plugins::block::snapshot_of;
use crate::plugins::decisions::Continuation;
use crate::plugins::events::{
    BlockBreak, BlockBreakPatch, BlockDamage, BlockDamageAbort, BlockDamagePatch, BlockDropItem,
    BlockDropItemPatch, EntityExhaustionReason,
};
use crate::plugins::steps::{
    AbortStep, Announced, BreakStep, DamageStep, DropItemsStep, FinishStep,
};
use crate::plugins::{
    DroppedItem, EntityView, EventKind, EventPayload, ItemSpec, spec_of, stack_of,
};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use rspigot_domain::game_event::GameEvent;

const MINING_EXHAUSTION: f32 = 0.005;
const FINISH_PROGRESS: f32 = 0.7;
const NO_SEQUENCE: i32 = -1;
const CORRECT_TOOL_DIVISOR: f32 = 30.0;
const WRONG_TOOL_DIVISOR: f32 = 100.0;
const HASTE_PER_LEVEL: f32 = 0.2;
const SUBMERGED_MINING_SPEED: f32 = 0.2;
const AIRBORNE_DIVISOR: f32 = 5.0;
const NEIGHBOURS: [Direction; 6] = [
    Direction::Down,
    Direction::Up,
    Direction::North,
    Direction::South,
    Direction::West,
    Direction::East,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Mining {
    pub(crate) pos: BlockPos,
    pub(crate) since: u64,
}

pub(crate) fn hand_spec(held: Option<&ItemStack>) -> ItemSpec {
    held.map_or_else(|| ItemSpec::of("minecraft:air", 0), spec_of)
}

fn hardness_of(state: BlockStateId) -> f32 {
    state.kind().map_or(0.0, |kind| kind.settings().hardness)
}

fn fatigue_factor(amplifier: u8) -> f32 {
    match amplifier {
        0 => 0.3,
        1 => 0.09,
        2 => 0.0027,
        _ => 8.1e-4,
    }
}

fn amplifier_of<L>(player: &Player<L>, name: &str) -> Option<u8> {
    let effect = EffectId::from_name(name)?;
    player
        .effects
        .get(effect)
        .map(|instance| instance.amplifier)
}

fn efficiency_of(stack: &ItemStack) -> f32 {
    stack
        .enchantments()
        .into_iter()
        .find(|(key, _)| key == "minecraft:efficiency")
        .map_or(0.0, |(_, level)| {
            let level = f32::from(level);
            level * level + 1.0
        })
}

pub(crate) fn mines_in_creative(held: Option<&ItemStack>) -> bool {
    held.and_then(|stack| stack.item.kind())
        .and_then(ItemKind::tool)
        .is_none_or(|tool| tool.can_destroy_blocks_in_creative)
}

impl<L: ClientLink> Server<L> {
    fn destroy_speed(&self, player: &Player<L>, state: BlockStateId) -> f32 {
        let held = player.inventory.held();
        let tool = held
            .and_then(|stack| stack.item.kind())
            .and_then(ItemKind::tool);
        let mut speed = match (tool, state.kind()) {
            (Some(tool), Some(kind)) => tool.mining_speed(kind.id()),
            _ => 1.0,
        };
        if speed > 1.0
            && let Some(stack) = held
        {
            speed += efficiency_of(stack);
        }
        let haste = [
            amplifier_of(player, "minecraft:haste"),
            amplifier_of(player, "minecraft:conduit_power"),
        ]
        .into_iter()
        .flatten()
        .max();
        if let Some(amplifier) = haste {
            speed *= 1.0 + (f32::from(amplifier) + 1.0) * HASTE_PER_LEVEL;
        }
        if let Some(amplifier) = amplifier_of(player, "minecraft:mining_fatigue") {
            speed *= fatigue_factor(amplifier);
        }
        let eyes = player.eye_position();
        let submerged = self
            .world
            .chunks
            .block(eyes.block())
            .is_some_and(|state| state.flags().contains(rspigot_blocks::StateFlags::WATER));
        if submerged {
            speed *= SUBMERGED_MINING_SPEED;
        }
        if !player.on_ground {
            speed /= AIRBORNE_DIVISOR;
        }
        speed
    }

    pub(crate) fn destroy_progress(&self, id: PlayerId, state: BlockStateId) -> f32 {
        let Some(player) = self.players.get(&id) else {
            return 0.0;
        };
        let hardness = hardness_of(state);
        if hardness < 0.0 {
            return 0.0;
        }
        let divisor = if loot::tool_is_correct(player.inventory.held(), state) {
            CORRECT_TOOL_DIVISOR
        } else {
            WRONG_TOOL_DIVISOR
        };
        self.destroy_speed(player, state) / hardness / divisor
    }

    fn held_spec(&self, id: PlayerId) -> Option<ItemSpec> {
        Some(hand_spec(self.players.get(&id)?.inventory.held()))
    }

    pub(crate) fn ack_block(&mut self, id: PlayerId, sequence: i32) {
        if sequence > NO_SEQUENCE {
            self.push(Audience::One(id), ClientEvent::BlockChangedAck(sequence));
        }
    }

    pub(crate) fn start_destroying(&mut self, id: PlayerId, pos: BlockPos, sequence: i32) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let game_mode = player.game_mode;
        let held = player.inventory.held().cloned();
        let Some(state) = self.world.chunks.block(pos) else {
            self.ack_block(id, sequence);
            return;
        };
        match game_mode {
            GameMode::Creative => {
                let refused = !mines_in_creative(held.as_ref());
                self.ask_break(BreakStep {
                    player: id,
                    pos,
                    sequence,
                    state,
                    game_mode,
                    held,
                    refused,
                    exp: 0,
                });
            }
            GameMode::Survival if !state.is_air() => {
                let insta = self.destroy_progress(id, state) >= 1.0;
                let step = DamageStep {
                    player: id,
                    pos,
                    sequence,
                    state,
                    held,
                    insta,
                };
                self.fire(id, Continuation::Damage(Box::new(step)), move |server| {
                    Some(EventPayload::BlockDamage(BlockDamage {
                        block: pos,
                        insta_break: insta,
                        item_in_hand: server.held_spec(id)?,
                        player: server.snapshot_of(id)?,
                    }))
                });
            }
            _ => self.ack_block(id, sequence),
        }
    }

    pub(crate) fn finish_damage(
        &mut self,
        step: DamageStep,
        cancelled: bool,
        patch: BlockDamagePatch,
    ) {
        let (id, pos) = (step.player, step.pos);
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let valid = player.game_mode == GameMode::Survival
            && player.inventory.held() == step.held.as_ref()
            && self.world.chunks.block(pos) == Some(step.state);
        if cancelled || !valid {
            self.refuse_block(id, pos, step.sequence);
            return;
        }
        if step.insta || patch.insta_break == Some(true) {
            self.ask_break(BreakStep {
                player: id,
                pos,
                sequence: step.sequence,
                state: step.state,
                game_mode: GameMode::Survival,
                held: step.held,
                refused: false,
                exp: 0,
            });
            return;
        }
        let since = self.current_tick();
        if let Some(player) = self.players.get_mut(&id) {
            player.mining = Some(Mining { pos, since });
        }
        self.ack_block(id, step.sequence);
    }

    pub(crate) fn finish_digging(&mut self, step: FinishStep) {
        let (id, pos, sequence) = (step.player, step.pos, step.sequence);
        let now = self.current_tick();
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let started = player.mining.take_if(|mining| mining.pos == pos);
        let game_mode = player.game_mode;
        let state = self.world.chunks.block(pos);
        let Some((state, mining)) = state
            .zip(started)
            .filter(|_| game_mode == GameMode::Survival)
        else {
            self.refuse_block(id, pos, sequence);
            return;
        };
        if state.is_air() {
            self.ack_block(id, sequence);
            return;
        }
        let elapsed = now.saturating_sub(mining.since);
        if self.destroy_progress(id, state) * (elapsed as f32 + 1.0) < FINISH_PROGRESS {
            if let Some(player) = self.players.get_mut(&id)
                && player.delayed_destroy.is_none()
            {
                player.delayed_destroy = Some(mining);
            }
            self.ack_block(id, sequence);
            return;
        }
        self.break_mined(id, pos, sequence, state);
    }

    pub(crate) fn tick_delayed_destroys(&mut self) {
        let now = self.current_tick();
        let delayed: Vec<(PlayerId, Mining)> = self
            .players
            .values()
            .filter_map(|player| Some((player.id, player.delayed_destroy?)))
            .collect();
        for (id, mining) in delayed {
            let state = self.world.chunks.block(mining.pos);
            let due = state.filter(|state| !state.is_air()).map(|state| {
                let elapsed = now.saturating_sub(mining.since);
                (
                    state,
                    self.destroy_progress(id, state) * (elapsed as f32 + 1.0) >= 1.0,
                )
            });
            match due {
                Some((_, false)) => {}
                Some((state, true)) => {
                    if let Some(player) = self.players.get_mut(&id) {
                        player.delayed_destroy = None;
                    }
                    if self
                        .players
                        .get(&id)
                        .is_some_and(|player| player.game_mode == GameMode::Survival)
                    {
                        self.break_mined(id, mining.pos, NO_SEQUENCE, state);
                    }
                }
                None => {
                    if let Some(player) = self.players.get_mut(&id) {
                        player.delayed_destroy = None;
                    }
                }
            }
        }
    }

    fn break_mined(&mut self, id: PlayerId, pos: BlockPos, sequence: i32, state: BlockStateId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let game_mode = player.game_mode;
        let held = player.inventory.held().cloned();
        if hardness_of(state) < 0.0 || !self.within_reach(id, pos) || self.is_protected(id, pos) {
            self.refuse_block(id, pos, sequence);
            return;
        }
        self.ask_break(BreakStep {
            player: id,
            pos,
            sequence,
            state,
            game_mode,
            held,
            refused: false,
            exp: 0,
        });
    }

    fn ask_break(&mut self, step: BreakStep) {
        let (id, pos, exp) = (step.player, step.pos, step.exp);
        let describe = move |server: &Self| {
            Some(EventPayload::BlockBreak(BlockBreak {
                block: pos,
                drop_items: true,
                exp_to_drop: exp,
                player: server.snapshot_of(id)?,
            }))
        };
        let refused = step.refused;
        let then = Continuation::Break(Box::new(step));
        if refused {
            self.fire_refused(id, then, describe);
            return;
        }
        if self.asks(&then) && !self.has_block_entity(pos) {
            self.push(
                Audience::One(id),
                ClientEvent::BlockUpdate {
                    pos,
                    state: BlockStateId::AIR,
                },
            );
        }
        self.fire(id, then, describe);
    }

    pub(crate) fn finish_break(
        &mut self,
        step: BreakStep,
        cancelled: bool,
        patch: BlockBreakPatch,
    ) {
        let (id, pos, sequence) = (step.player, step.pos, step.sequence);
        if step.refused {
            self.ack_block(id, sequence);
            return;
        }
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let valid = player.game_mode == step.game_mode
            && player.inventory.held() == step.held.as_ref()
            && self.world.chunks.block(pos) == Some(step.state);
        if cancelled || !valid {
            self.refuse_break(id, pos, sequence);
            return;
        }
        let drop_items = patch.drop_items.unwrap_or(true);
        let exp = patch.exp_to_drop.unwrap_or(step.exp);
        if step.state.kind().is_some_and(|kind| {
            rspigot_data::tags::tag("minecraft:block", GUARDED_BY_PIGLINS)
                .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
        }) {
            self.anger_nearby_piglins(id, false);
        }
        let hive = self.hive_snapshot(pos);
        self.set_block(pos, BlockStateId::AIR);
        let actor = self.players.get(&id).map(|player| player.entity_id);
        self.block_game_event(
            GameEvent::BlockDestroy,
            pos,
            GameEventContext::entity_in(actor, step.state),
        );
        self.ack_block(id, sequence);
        let survival = step.game_mode == GameMode::Survival;
        if survival {
            self.wear_tool_for_mining(id, hardness_of(step.state));
        }
        if !drop_items {
            self.pay_block_exp(pos, exp);
            return;
        }
        let mut stacks = Vec::new();
        if survival && loot::tool_is_correct(step.held.as_ref(), step.state) {
            self.exhaust(id, MINING_EXHAUSTION, EntityExhaustionReason::BlockMined);
            if self.config.rules.block_drops {
                let breaker = LootEntity::of(Kind::Player);
                let context = LootContext::block(step.state, step.held.as_ref(), Some(&breaker));
                stacks = loot::block_drops(&context, &mut self.rng);
            }
            if let Some(hive) = hive {
                self.hive_broken(pos, step.state, hive, Some(id), step.held.as_ref());
            }
            self.spawn_after_break(pos, step.state, step.held.as_ref());
        }
        self.ask_drop_items(id, pos, step.state, stacks, exp);
    }

    fn refuse_break(&mut self, id: PlayerId, pos: BlockPos, sequence: i32) {
        let around = NEIGHBOURS.map(|face| {
            let (dx, dy, dz) = face.offset();
            pos.offset(dx, dy, dz)
        });
        for at in std::iter::once(pos).chain(around) {
            if let Some(state) = self.world.chunks.block(at) {
                self.push(
                    Audience::One(id),
                    ClientEvent::BlockUpdate { pos: at, state },
                );
            }
        }
        self.ack_block(id, sequence);
        if let Some(update) = self.block_entity_update(pos) {
            self.push(Audience::One(id), update);
        }
        self.ack_block(id, sequence);
    }

    fn ask_drop_items(
        &mut self,
        id: PlayerId,
        pos: BlockPos,
        state: BlockStateId,
        stacks: Vec<ItemStack>,
        exp: i32,
    ) {
        let drops: Vec<(Announced, ItemStack)> = stacks
            .into_iter()
            .map(|stack| (self.block_drop_launch(pos), stack))
            .collect();
        let handles: Option<Vec<(EntityId, Uuid, Vec3, ItemStack)>> =
            self.listened(EventKind::BlockDropItem).then(|| {
                drops
                    .iter()
                    .map(|(launch, stack)| {
                        let base = &launch.base;
                        (base.id, base.uuid, base.position, stack.clone())
                    })
                    .collect()
            });
        let step = DropItemsStep { pos, drops, exp };
        self.fire(id, Continuation::DropItems(Box::new(step)), move |server| {
            Some(EventPayload::BlockDropItem(BlockDropItem {
                block: pos,
                block_state: snapshot_of(pos, state),
                items: handles?
                    .into_iter()
                    .map(|(id, uuid, position, stack)| DroppedItem {
                        entity: EntityView {
                            id,
                            uuid,
                            kind: Kind::Item.name().to_owned(),
                            position,
                        },
                        item: spec_of(&stack),
                    })
                    .collect(),
                player: server.snapshot_of(id)?,
            }))
        });
    }

    pub(crate) fn finish_drop_items(
        &mut self,
        step: DropItemsStep,
        cancelled: bool,
        patch: BlockDropItemPatch,
    ) {
        if !cancelled {
            let added: FxHashMap<EntityId, &ItemSpec> = patch
                .items
                .added
                .iter()
                .map(|dropped| (dropped.entity.id, &dropped.item))
                .collect();
            for (index, (launch, stack)) in step.drops.into_iter().enumerate() {
                let settled = match (added.get(&launch.base.id), patch.items.replacement(index)) {
                    (Some(spec), _) => stack_of(spec).unwrap_or(Some(stack)),
                    (None, _) if !patch.items.keeps(index) => None,
                    (None, Some(dropped)) => stack_of(&dropped.item).unwrap_or(Some(stack)),
                    (None, None) => Some(stack),
                };
                if let Some(stack) = settled
                    && self.world.chunks.is_loaded(launch.base.position.chunk())
                {
                    self.drop_launched(launch, stack);
                }
            }
        }
        self.pay_block_exp(step.pos, step.exp);
    }

    fn pay_block_exp(&mut self, pos: BlockPos, exp: i32) {
        if exp > 0 && self.config.rules.block_drops {
            self.award_experience(pos.center(), exp);
        }
    }

    pub(crate) fn finish_aborting(&mut self, step: AbortStep) {
        let (id, pos) = (step.player, step.pos);
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let destroying = player.mining.take().is_some();
        if !destroying && !self.within_reach(id, pos) {
            self.ack_block(id, step.sequence);
            return;
        }
        self.fire(id, Continuation::Abort(step), move |server| {
            Some(EventPayload::BlockDamageAbort(BlockDamageAbort {
                block: pos,
                item_in_hand: server.held_spec(id)?,
                player: server.snapshot_of(id)?,
            }))
        });
    }

    pub(crate) fn finish_abort(&mut self, step: &AbortStep) {
        self.ack_block(step.player, step.sequence);
    }
}
