use rspigot_domain::player::GameMode;
use rspigot_entities::SoundSource;

use crate::events::{ClientEvent, Direction};
use crate::game_events::GameEventContext;
use crate::plugins::block::{data_of, material_of, snapshot_of};
use crate::plugins::decisions::Continuation;
use crate::plugins::events::{
    BlockCanBuild, BlockCanBuildPatch, BlockMultiPlace, BlockPlace, EquipmentSlot as Slot,
};
use crate::plugins::objects::slot_of;
use crate::plugins::steps::{BuildStep, Change, PlaceStep, UseItem};
use crate::plugins::{BlockSnapshot, EventPayload, spec_of};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};
use rspigot_domain::BlockStateId;
use rspigot_domain::game_event::GameEvent;

const LEVEL_EVENT_RANGE: f64 = 64.0;
const AROUND: [Direction; 6] = [
    Direction::Down,
    Direction::Up,
    Direction::North,
    Direction::South,
    Direction::West,
    Direction::East,
];

impl<L: ClientLink> Server<L> {
    pub(crate) fn ask_can_build(&mut self, step: BuildStep) {
        let id = step.using.player;
        let (block, buildable, state) = (step.target, step.buildable, step.state);
        self.fire(id, Continuation::CanBuild(Box::new(step)), move |server| {
            Some(EventPayload::BlockCanBuild(BlockCanBuild {
                block,
                block_data: data_of(state),
                buildable,
                material: material_of(state).to_owned(),
                player: server.snapshot_of(id),
            }))
        });
    }

    pub(crate) fn finish_can_build(&mut self, step: BuildStep, patch: BlockCanBuildPatch) {
        let using = step.using;
        let buildable = patch.buildable.unwrap_or(step.buildable);
        if !buildable {
            self.ack_block(using.player, using.sequence);
            return;
        }
        if !self.still_using(&using) || self.world.chunks.block(step.target) != Some(step.old) {
            self.refuse_block(using.player, step.target, using.sequence);
            return;
        }
        let mut changes = Vec::with_capacity(2);
        if let Some((other, state)) = step.linked
            && let Some(old) = self
                .world
                .chunks
                .block(other)
                .filter(|old| crate::input::is_replaceable(*old))
        {
            changes.push(Change {
                pos: other,
                old,
                new: state,
            });
        }
        changes.push(Change {
            pos: step.target,
            old: step.old,
            new: step.state,
        });
        let build = !self.is_protected(using.player, using.against);
        self.ask_place(PlaceStep {
            using,
            changes,
            build,
            tool: None,
            loot: Vec::new(),
            forced: !step.buildable,
        });
    }

    pub(crate) fn ask_place(&mut self, step: PlaceStep) {
        let id = step.using.player;
        let Some(first) = step.changes.first().copied() else {
            return;
        };
        let second = step.changes.get(1).copied();
        let against = step.using.against;
        let build = step.build;
        let hand = slot_of(step.using.hand);
        let slot = step.using.slot;
        let changes = move || std::iter::once(first).chain(second);
        let item_in_hand =
            move |server: &Self| server.players.get(&id)?.inventory.get(slot).map(spec_of);
        let snapshots = move |of: fn(&Change) -> BlockStateId| -> Vec<BlockSnapshot> {
            changes()
                .map(|change| snapshot_of(change.pos, of(&change)))
                .collect()
        };
        if second.is_some() {
            self.fire(
                id,
                Continuation::MultiPlace(Box::new(step)),
                move |server| {
                    let replaced = snapshots(|change| change.old);
                    let placed = snapshots(|change| change.new);
                    Some(EventPayload::BlockMultiPlace(BlockMultiPlace {
                        block: first.pos,
                        block_against: against,
                        block_placed: first.pos,
                        block_replaced_state: replaced[0].clone(),
                        build,
                        hand: Slot::Hand,
                        item_in_hand: item_in_hand(server)?,
                        player: server.snapshot_of(id)?,
                        replaced_block_states: replaced,
                        placed_state: placed[0].clone(),
                        placed_states: placed,
                    }))
                },
            );
            return;
        }
        self.fire(id, Continuation::Place(Box::new(step)), move |server| {
            Some(EventPayload::BlockPlace(BlockPlace {
                block: first.pos,
                block_against: against,
                block_placed: first.pos,
                block_replaced_state: snapshot_of(first.pos, first.old),
                build,
                hand,
                item_in_hand: item_in_hand(server)?,
                player: server.snapshot_of(id)?,
                placed_state: snapshot_of(first.pos, first.new),
            }))
        });
    }

    fn still_using(&self, using: &UseItem) -> bool {
        self.players.get(&using.player).is_some_and(|player| {
            player.game_mode == using.game_mode
                && !matches!(player.game_mode, GameMode::Adventure | GameMode::Spectator)
                && !player.vitals.is_dead()
                && player.inventory.get(using.slot) == Some(&using.stack)
        })
    }

    pub(crate) fn finish_place(&mut self, step: PlaceStep, cancelled: bool, build: Option<bool>) {
        let build = build.unwrap_or(step.build);
        let unchanged = step
            .changes
            .iter()
            .all(|change| self.world.chunks.block(change.pos) == Some(change.old));
        let clear = step.forced
            || step.tool.is_some()
            || step
                .changes
                .last()
                .is_some_and(|target| !self.intersects_player(target.pos));
        if cancelled || !build || !unchanged || !clear || !self.still_using(&step.using) {
            self.refuse_place(&step);
            return;
        }
        for change in step.changes.iter().rev() {
            self.set_block(change.pos, change.new);
        }
        let using = step.using;
        let id = using.player;
        let actor = self.players.get(&id).map(|player| player.entity_id);
        if let Some(target) = step.changes.last() {
            let event = if step.tool.is_some() {
                GameEvent::BlockChange
            } else {
                GameEvent::BlockPlace
            };
            self.block_game_event(
                event,
                target.pos,
                GameEventContext::entity_in(actor, target.new),
            );
        }
        let Some(effects) = step.tool else {
            self.ack_block(id, using.sequence);
            if using.game_mode == GameMode::Survival {
                self.consume_one(id, using.slot, using.stack);
            }
            return;
        };
        let pos = using.against;
        for stack in step.loot {
            self.spawn_item_from_face(pos, effects.face.offset(), stack);
        }
        self.play_sound(
            Some(id),
            pos.center(),
            effects.sound,
            SoundSource::Blocks,
            1.0,
            1.0,
        );
        if let Some(event) = effects.particle {
            self.push(
                Audience::Nearby {
                    position: pos.center(),
                    range: LEVEL_EVENT_RANGE,
                    except: Some(id),
                },
                ClientEvent::LevelEvent {
                    event,
                    pos,
                    data: 0,
                },
            );
        }
        self.ack_block(id, using.sequence);
        self.hurt_and_break(id, using.hand, effects.damage);
    }

    fn refuse_place(&mut self, step: &PlaceStep) {
        let id = step.using.player;
        let Some(placed) = step.changes.first().map(|change| change.pos) else {
            return;
        };
        let around = AROUND.map(|face| {
            let (dx, dy, dz) = face.offset();
            placed.offset(dx, dy, dz)
        });
        let resent = step.changes.iter().map(|change| change.pos).chain(around);
        for pos in resent {
            if let Some(state) = self.world.chunks.block(pos) {
                self.push(Audience::One(id), ClientEvent::BlockUpdate { pos, state });
            }
        }
        self.send_inventory(id);
        self.ack_block(id, step.using.sequence);
    }
}
