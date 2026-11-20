use std::collections::{BTreeMap, VecDeque};

use rspigot_domain::player::PlayerId;
use rspigot_domain::{BlockPos, ChunkPos, EntityId, Vec3};
use rustc_hash::FxHashMap;

pub(crate) use crate::plugins::steps::{Continuation, Held, MoveStep, Undo};
use crate::plugins::{EventKind, EventPatch, EventPayload, Verdict};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Lane {
    Player(PlayerId),
    Login(u64),
    Block(BlockPos),
    Chunk(ChunkPos),
    Entity(EntityId),
    World,
    Console,
    Broadcast,
    Ping,
}

impl From<PlayerId> for Lane {
    fn from(player: PlayerId) -> Self {
        Self::Player(player)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Asked {
    pub(crate) connection: u64,
    pub(crate) handler: u32,
    pub(crate) kind: EventKind,
}

#[derive(Debug, Clone, PartialEq)]
struct Answer {
    cancel: bool,
    patch: Option<EventPatch>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Settlement {
    Cancelled(Option<EventPatch>),
    Applied(Option<EventPatch>),
}

pub(crate) struct Pending {
    lane: Lane,
    payload: EventPayload,
    asked: Vec<Asked>,
    answers: Vec<Option<Answer>>,
    waiting: Vec<u64>,
    deadline: u64,
    until: u64,
    then: Continuation,
    held: Vec<Held>,
    refused: bool,
}

impl Pending {
    pub(crate) fn new(
        lane: impl Into<Lane>,
        payload: EventPayload,
        asked: Vec<Asked>,
        deadline: u64,
        until: u64,
        then: Continuation,
    ) -> Self {
        let mut waiting: Vec<u64> = Vec::new();
        for ask in &asked {
            if !waiting.contains(&ask.connection) {
                waiting.push(ask.connection);
            }
        }
        Self {
            lane: lane.into(),
            payload,
            answers: vec![None; asked.len()],
            asked,
            waiting,
            deadline,
            until,
            then,
            held: Vec::new(),
            refused: false,
        }
    }

    pub(crate) fn refused(mut self) -> Self {
        self.refused = true;
        self
    }

    fn cancelled(&self) -> bool {
        self.refused || self.answers.iter().flatten().any(|answer| answer.cancel)
    }

    fn ready(&self, tick: u64) -> bool {
        let final_cancel = self.cancelled() && !self.payload.kind().writes_when_cancelled();
        final_cancel || self.waiting.is_empty() || tick >= self.deadline
    }

    fn take(
        &mut self,
        connection: u64,
        event: u64,
        verdicts: Vec<Verdict>,
        warnings: &mut Vec<String>,
    ) -> Taken {
        let Some(index) = self
            .waiting
            .iter()
            .position(|waiting| *waiting == connection)
        else {
            if self.asked.iter().any(|ask| ask.connection == connection) {
                warnings.push(answered_twice(event));
            }
            return Taken::Ignored;
        };
        self.waiting.swap_remove(index);
        let kind = self.payload.kind();
        let admitted = admit(&self.asked, connection, event, verdicts, warnings);
        let cancelled = kind.cancellable() && admitted.iter().any(|(_, verdict)| verdict.cancel);
        for (slot, verdict) in admitted {
            let taken_as = self.asked[slot].kind;
            let mut patch = match verdict.patch {
                Some(patch) if patch.kind() == taken_as => patch.lift(kind),
                Some(misfit) => {
                    warnings.push(format!(
                        "a {:?} patch does not fit event {event}, which is a {taken_as:?}",
                        misfit.kind()
                    ));
                    None
                }
                None => None,
            };
            if let Some(reason) = patch
                .as_ref()
                .and_then(|patch| patch.refusal(&self.payload))
            {
                warnings.push(format!("patch of event {event} was refused: {reason}"));
                patch = None;
            }
            let mut cancel = verdict.cancel;
            if cancel && !kind.cancellable() {
                warnings.push(format!(
                    "event {event} cannot be cancelled, the cancel of handler {} is ignored",
                    verdict.handler
                ));
                cancel = false;
            }
            if cancel && let Some(denial) = EventPatch::denial(kind) {
                patch = Some(match patch {
                    Some(own) => own.then(denial),
                    None => denial,
                });
                cancel = false;
            }
            self.answers[slot] = Some(Answer { cancel, patch });
        }
        if !cancelled {
            for (ask, answer) in self.asked.iter().zip(&self.answers) {
                if ask.connection == connection && answer.is_none() {
                    warnings.push(format!(
                        "handler {} of event {event} gave no verdict",
                        ask.handler
                    ));
                }
            }
        }
        Taken::Counted
    }

    fn record(&self) -> Record {
        let (silent, answered) = self
            .asked
            .iter()
            .partition(|ask| self.waiting.contains(&ask.connection));
        Record {
            lane: self.lane,
            undo: self.then.undo(),
            silent,
            answered,
            until: self.until,
            deadline: self.deadline,
            applied: None,
        }
    }

    pub(crate) fn settle(self, event: u64) -> Ready {
        let record = self.record();
        let (cancelled, patch) = fold(self.answers);
        let settlement = if cancelled || self.refused || denied(&self.payload, patch.as_ref()) {
            Settlement::Cancelled(patch.and_then(EventPatch::when_cancelled))
        } else {
            Settlement::Applied(patch)
        };
        Ready {
            event,
            lane: self.lane,
            payload: self.payload,
            settlement,
            then: self.then,
            held: self.held,
            record,
        }
    }
}

fn answered_twice(event: u64) -> String {
    format!("event {event} was already answered by this plugin")
}

fn admit(
    asked: &[Asked],
    connection: u64,
    event: u64,
    verdicts: Vec<Verdict>,
    warnings: &mut Vec<String>,
) -> Vec<(usize, Verdict)> {
    let mut admitted: Vec<(usize, Verdict)> = Vec::with_capacity(verdicts.len());
    for verdict in verdicts {
        let handler = verdict.handler;
        match asked
            .iter()
            .position(|ask| ask.connection == connection && ask.handler == handler)
        {
            None => warnings.push(format!(
                "handler {handler} was not asked to decide event {event}"
            )),
            Some(slot) if admitted.iter().any(|(seen, _)| *seen == slot) => warnings.push(format!(
                "handler {handler} answered event {event} more than once"
            )),
            Some(slot) => admitted.push((slot, verdict)),
        }
    }
    admitted
}

fn fold(answers: Vec<Option<Answer>>) -> (bool, Option<EventPatch>) {
    let mut cancelled = false;
    let mut patch: Option<EventPatch> = None;
    for answer in answers.into_iter().flatten() {
        cancelled |= answer.cancel;
        patch = match (patch, answer.patch) {
            (Some(earlier), Some(later)) => Some(earlier.then(later)),
            (earlier, later) => later.or(earlier),
        };
    }
    (cancelled, patch)
}

fn denied(payload: &EventPayload, patch: Option<&EventPatch>) -> bool {
    match patch {
        Some(patch) if payload.kind().cancels_by_result() => {
            let mut settled = payload.clone();
            patch.clone().settle(&mut settled);
            settled.cancelled()
        }
        _ => payload.cancelled(),
    }
}

pub(crate) struct Ready {
    pub(crate) event: u64,
    pub(crate) lane: Lane,
    pub(crate) payload: EventPayload,
    pub(crate) settlement: Settlement,
    pub(crate) then: Continuation,
    pub(crate) held: Vec<Held>,
    pub(crate) record: Record,
}

pub(crate) struct Due {
    pub(crate) event: u64,
    pub(crate) lane: Lane,
    pub(crate) then: Continuation,
}

pub(crate) enum Flushed {
    Ready(Box<Ready>),
    Due(Due),
}

pub(crate) struct Record {
    lane: Lane,
    undo: Option<Undo>,
    silent: Vec<Asked>,
    answered: Vec<Asked>,
    until: u64,
    deadline: u64,
    pub(crate) applied: Option<EventPayload>,
}

impl Record {
    fn silent_from(&self, connection: u64) -> bool {
        self.silent.iter().any(|ask| ask.connection == connection)
    }

    fn answered_by(&self, connection: u64) -> bool {
        self.answered.iter().any(|ask| ask.connection == connection)
    }

    fn kept(&self, tick: u64) -> bool {
        tick <= self.until.max(self.deadline)
    }
}

pub(crate) type Undone = Vec<(u64, EventPayload)>;

pub(crate) struct Rollback {
    pub(crate) lane: Lane,
    pub(crate) undo: Undo,
    pub(crate) undone: Undone,
    pub(crate) dropped: Undone,
}

pub(crate) enum Taken {
    Counted,
    Ignored,
    RolledBack(Rollback),
    TooLate(&'static str),
}

enum Queued {
    Asked(u64, Box<Pending>),
    Due(u64, Continuation),
}

impl Queued {
    const fn event(&self) -> u64 {
        match self {
            Self::Asked(event, _) | Self::Due(event, _) => *event,
        }
    }

    const fn then(&self) -> &Continuation {
        match self {
            Self::Asked(_, pending) => &pending.then,
            Self::Due(_, then) => then,
        }
    }

    fn moving(&self) -> bool {
        matches!(self, Self::Asked(_, pending) if pending.then.is_move())
    }
}

#[derive(Default)]
pub(crate) struct Decisions {
    lanes: FxHashMap<Lane, VecDeque<Queued>>,
    settled: BTreeMap<u64, Record>,
    heads: Vec<(Lane, usize)>,
}

impl Decisions {
    pub(crate) fn reference(&self, lane: impl Into<Lane>) -> Option<Vec3> {
        self.lanes
            .get(&lane.into())?
            .iter()
            .rev()
            .find_map(|queued| queued.then().destination())
    }

    pub(crate) fn busy(&self, lane: impl Into<Lane>) -> bool {
        let lane = lane.into();
        match self.heads.iter().rev().find(|(head, _)| *head == lane) {
            Some((_, cursor)) => *cursor > 0,
            None => self.lanes.get(&lane).is_some_and(|queue| !queue.is_empty()),
        }
    }

    pub(crate) fn in_flight(&self, lane: Lane, matches: impl Fn(&Continuation) -> bool) -> bool {
        self.lanes
            .get(&lane)
            .is_some_and(|queue| queue.iter().any(|queued| matches(queued.then())))
    }

    pub(crate) fn anywhere(&self, matches: impl Fn(&Continuation) -> bool) -> bool {
        self.lanes
            .values()
            .flatten()
            .any(|queued| matches(queued.then()))
    }

    pub(crate) fn waiting(&self) -> impl Iterator<Item = &Continuation> {
        self.lanes.values().flatten().map(Queued::then)
    }

    pub(crate) fn current(&self) -> Option<Lane> {
        self.heads.last().map(|(lane, _)| *lane)
    }

    pub(crate) fn queue(&mut self, event: u64, pending: Pending) {
        let lane = pending.lane;
        self.insert(lane, Queued::Asked(event, Box::new(pending)));
    }

    pub(crate) fn defer(&mut self, event: u64, lane: Lane, then: Continuation) {
        self.insert(lane, Queued::Due(event, then));
    }

    fn insert(&mut self, lane: Lane, queued: Queued) {
        let queue = self.lanes.entry(lane).or_default();
        match self.heads.iter_mut().rev().find(|(head, _)| *head == lane) {
            Some((_, cursor)) => {
                queue.insert((*cursor).min(queue.len()), queued);
                *cursor += 1;
            }
            None => queue.push_back(queued),
        }
    }

    pub(crate) fn enter(&mut self, lane: Lane) {
        self.heads.push((lane, 0));
    }

    pub(crate) fn leave(&mut self) {
        self.heads.pop();
    }

    pub(crate) fn hold(&mut self, lane: impl Into<Lane>, held: Held) -> Result<(), Held> {
        let last = self.lanes.get_mut(&lane.into()).and_then(|queue| {
            queue.iter_mut().rev().find_map(|queued| match queued {
                Queued::Asked(_, pending) if pending.then.is_move() => Some(pending),
                _ => None,
            })
        });
        match last {
            Some(last) => {
                last.held.push(held);
                Ok(())
            }
            None => Err(held),
        }
    }

    fn pending_mut(&mut self, event: u64) -> Option<&mut Pending> {
        self.lanes
            .values_mut()
            .flatten()
            .find_map(|queued| match queued {
                Queued::Asked(order, pending) if *order == event => Some(pending.as_mut()),
                _ => None,
            })
    }

    pub(crate) fn decide(
        &mut self,
        connection: u64,
        event: u64,
        verdicts: Vec<Verdict>,
        tick: u64,
    ) -> (Taken, Vec<String>) {
        let mut warnings = Vec::new();
        let taken = match self.pending_mut(event) {
            Some(pending) => pending.take(connection, event, verdicts, &mut warnings),
            None => self.late(connection, event, verdicts, tick, &mut warnings),
        };
        (taken, warnings)
    }

    fn late(
        &mut self,
        connection: u64,
        event: u64,
        verdicts: Vec<Verdict>,
        tick: u64,
        warnings: &mut Vec<String>,
    ) -> Taken {
        let Some(record) = self.settled.get(&event).filter(|record| record.kept(tick)) else {
            return if verdicts.iter().any(|verdict| verdict.cancel) {
                Taken::TooLate("cancel")
            } else if verdicts.iter().any(|verdict| verdict.patch.is_some()) {
                Taken::TooLate("patch")
            } else {
                Taken::Ignored
            };
        };
        if record.applied.is_none() {
            return Taken::Ignored;
        }
        if !record.silent_from(connection) {
            if record.answered_by(connection) {
                warnings.push(answered_twice(event));
            }
            return Taken::Ignored;
        }
        let admitted = admit(&record.silent, connection, event, verdicts, warnings);
        let (lane, undo) = (record.lane, record.undo.filter(|_| tick <= record.until));
        let cancel = admitted.iter().any(|(_, verdict)| verdict.cancel);
        let patched = admitted.iter().any(|(_, verdict)| verdict.patch.is_some());
        match (cancel, undo) {
            (true, Some(undo)) => Taken::RolledBack(self.roll_back(event, lane, undo)),
            (true, None) => Taken::TooLate("cancel"),
            (false, _) if patched => Taken::TooLate("patch"),
            (false, _) => Taken::Ignored,
        }
    }

    fn roll_back(&mut self, event: u64, lane: Lane, undo: Undo) -> Rollback {
        let undone = self
            .settled
            .range_mut(event..)
            .filter(|(_, later)| later.lane == lane && later.undo.is_some())
            .filter_map(|(id, later)| later.applied.take().map(|payload| (*id, payload)))
            .collect();
        let dropped = self.drop_pending(lane);
        Rollback {
            lane,
            undo,
            undone,
            dropped,
        }
    }

    pub(crate) fn release(&mut self, present: impl Fn(u64) -> bool) {
        for queued in self.lanes.values_mut().flatten() {
            if let Queued::Asked(_, pending) = queued {
                pending.waiting.retain(|connection| present(*connection));
            }
        }
    }

    pub(crate) fn next_settled(&mut self, tick: u64) -> Option<Ready> {
        let (lane, event) = self
            .lanes
            .iter()
            .filter_map(|(lane, queue)| match queue.front()? {
                Queued::Asked(event, pending) if pending.ready(tick) => Some((*lane, *event)),
                _ => None,
            })
            .min_by_key(|(_, event)| *event)?;
        match self.pop(lane)? {
            Queued::Asked(_, pending) => Some((*pending).settle(event)),
            Queued::Due(..) => None,
        }
    }

    pub(crate) fn next_due(&mut self) -> Option<Due> {
        let (lane, event) = self
            .lanes
            .iter()
            .filter_map(|(lane, queue)| match queue.front()? {
                Queued::Due(event, _) => Some((*lane, *event)),
                Queued::Asked(..) => None,
            })
            .min_by_key(|(_, event)| *event)?;
        match self.pop(lane)? {
            Queued::Due(_, then) => Some(Due { event, lane, then }),
            Queued::Asked(..) => None,
        }
    }

    pub(crate) fn lane_where(&self, matches: impl Fn(Lane, &Continuation) -> bool) -> Option<Lane> {
        self.lanes
            .iter()
            .filter(|(lane, queue)| queue.iter().any(|queued| matches(**lane, queued.then())))
            .filter_map(|(lane, queue)| Some((queue.front()?.event(), *lane)))
            .min()
            .map(|(_, lane)| lane)
    }

    pub(crate) fn first_lane(&self) -> Option<Lane> {
        self.lanes.keys().min().copied()
    }

    pub(crate) fn flush(&mut self, lane: Lane, logins: bool) -> Option<Flushed> {
        let front = self.lanes.get(&lane)?.front()?;
        if !logins && matches!(front.then(), Continuation::Login(_)) {
            return None;
        }
        let queued = self.pop(lane)?;
        for (head, cursor) in &mut self.heads {
            if *head == lane {
                *cursor = cursor.saturating_sub(1);
            }
        }
        Some(match queued {
            Queued::Asked(event, pending) => Flushed::Ready(Box::new((*pending).settle(event))),
            Queued::Due(event, then) => Flushed::Due(Due { event, lane, then }),
        })
    }

    fn pop(&mut self, lane: Lane) -> Option<Queued> {
        let queue = self.lanes.get_mut(&lane)?;
        let front = queue.pop_front();
        if queue.is_empty() {
            self.lanes.remove(&lane);
        }
        front
    }

    pub(crate) fn remember(&mut self, event: u64, record: Record) {
        self.settled.insert(event, record);
    }

    pub(crate) fn expire(&mut self, tick: u64) {
        self.settled.retain(|_, record| record.kept(tick));
    }

    pub(crate) fn drop_pending(&mut self, lane: impl Into<Lane>) -> Undone {
        let lane = lane.into();
        let Some(queue) = self.lanes.get_mut(&lane) else {
            return Vec::new();
        };
        let (moves, rest): (VecDeque<Queued>, VecDeque<Queued>) =
            std::mem::take(queue).into_iter().partition(Queued::moving);
        *queue = rest;
        if queue.is_empty() {
            self.lanes.remove(&lane);
        }
        let mut dropped = Vec::with_capacity(moves.len());
        for queued in moves {
            if let Queued::Asked(event, pending) = queued {
                self.remember(event, pending.record());
                dropped.push((event, pending.payload));
            }
        }
        dropped
    }

    pub(crate) fn forget(&mut self, lane: impl Into<Lane>) -> Undone {
        let lane = lane.into();
        self.settled.retain(|_, record| {
            record.lane != lane || record.undo.is_none() || record.applied.is_none()
        });
        self.drop_pending(lane)
    }
}

#[cfg(test)]
mod tests {
    use rspigot_domain::Rotation;
    use rspigot_domain::player::GameMode;

    use super::*;
    use crate::plugins::events::{
        Action, AsyncPlayerChat, AsyncPlayerChatPatch, BlockDropItem, BlockDropItemPatch,
        BlockExplode, BlockExplodePatch, BroadcastMessage, BroadcastMessagePatch, ClickKind,
        EntityDamageByEntity, EntityDamageByEntityPatch, EntityDamageCause, EntityDamagePatch,
        EntityDeathPatch, EventResult, ExplosionResult, Face, InventoryAction, InventoryClick,
        InventoryClickPatch, InventoryTypeSlotType, PlayerCommandSend, PlayerCommandSendPatch,
        PlayerDeath, PlayerDeathPatch, PlayerInteract, PlayerInteractPatch, PlayerJoin, PlayerMove,
        PlayerMovePatch, PlayerRespawn, PlayerRespawnPatch, PlayerRespawnReason, PlayerTeleport,
        PlayerTeleportCause, PlayerTeleportPatch, PlayerVelocity, PlayerVelocityPatch,
        ServerListPing, ServerListPingPatch, TabComplete, TabCompletePatch,
    };
    use crate::plugins::{
        BlockSnapshot, BlockState, DamageSourceView, DroppedItem, EntityView, InventoryOwner,
        InventoryRef, InventoryView, ItemSpec, ListDiff, Location, PlayerSnapshot, SenderView,
    };

    fn snapshot(player: PlayerId, position: Vec3) -> PlayerSnapshot {
        PlayerSnapshot {
            meta: Default::default(),
            permissions: Vec::new(),
            id: player,
            name: "Alice".to_owned(),
            entity_id: 1,
            world: "minecraft:overworld".to_owned(),
            position,
            rotation: Rotation::default(),
            game_mode: GameMode::Survival,
        }
    }

    fn location(position: Vec3) -> Location {
        Location {
            world: "minecraft:overworld".to_owned(),
            position,
            rotation: Rotation::default(),
        }
    }

    fn payload(player: PlayerId, from: Vec3, to: Vec3) -> EventPayload {
        EventPayload::PlayerMove(PlayerMove {
            player: snapshot(player, from),
            from: location(from),
            to: Some(location(to)),
        })
    }

    fn step(player: PlayerId, from: Vec3, to: Vec3) -> Continuation {
        Continuation::PlayerMove(MoveStep {
            player,
            from,
            to,
            rotation: None,
            on_ground: true,
        })
    }

    fn handlers(asked: &[(u64, u32)]) -> Vec<Asked> {
        asked
            .iter()
            .map(|&(connection, handler)| Asked {
                connection,
                handler,
                kind: EventKind::PlayerMove,
            })
            .collect()
    }

    fn asking(player: PlayerId, x: f64, asked: &[(u64, u32)]) -> Pending {
        let from = Vec3::new(x - 1.0, 64.0, 0.0);
        let to = Vec3::new(x, 64.0, 0.0);
        Pending::new(
            player,
            payload(player, from, to),
            handlers(asked),
            5,
            20,
            step(player, from, to),
        )
    }

    fn pending(player: PlayerId, x: f64, waiting: &[u64]) -> Pending {
        let asked: Vec<(u64, u32)> = waiting.iter().map(|&connection| (connection, 1)).collect();
        asking(player, x, &asked)
    }

    fn record(player: PlayerId, silent: &[u64], until: u64) -> Record {
        let from = Vec3::new(0.0, 64.0, 0.0);
        let silent: Vec<(u64, u32)> = silent.iter().map(|&connection| (connection, 1)).collect();
        Record {
            lane: player.into(),
            undo: Some(Undo::SendBack(from)),
            silent: handlers(&silent),
            answered: Vec::new(),
            until,
            deadline: 5,
            applied: Some(payload(player, from, from)),
        }
    }

    fn verdict(handler: u32, cancel: bool, x: Option<f64>) -> Verdict {
        Verdict {
            handler,
            cancel,
            patch: x.map(|x| {
                EventPatch::PlayerMove(PlayerMovePatch {
                    to: Some(location(Vec3::new(x, 64.0, 0.0))),
                })
            }),
        }
    }

    fn cancel(cancel: bool) -> Vec<Verdict> {
        vec![verdict(1, cancel, None)]
    }

    fn taken(
        decisions: &mut Decisions,
        connection: u64,
        event: u64,
        verdicts: Vec<Verdict>,
        tick: u64,
    ) -> Taken {
        decisions.decide(connection, event, verdicts, tick).0
    }

    fn settled_ids(decisions: &mut Decisions, tick: u64) -> Vec<u64> {
        std::iter::from_fn(|| decisions.next_settled(tick).map(|ready| ready.event)).collect()
    }

    fn ids(undone: &Undone) -> Vec<u64> {
        undone.iter().map(|(event, _)| *event).collect()
    }

    fn rolled_back(taken: Taken) -> (Vec<u64>, Vec<u64>) {
        match taken {
            Taken::RolledBack(rollback) => (ids(&rollback.undone), ids(&rollback.dropped)),
            _ => panic!("expected a rollback"),
        }
    }

    fn arrivals(count: usize) -> Vec<Vec<usize>> {
        (0..count).fold(vec![Vec::new()], |known, next| {
            known
                .into_iter()
                .flat_map(|order: Vec<usize>| {
                    (0..=order.len()).map(move |at| {
                        let mut longer = order.clone();
                        longer.insert(at, next);
                        longer
                    })
                })
                .collect()
        })
    }

    #[test]
    fn a_move_waits_for_the_earlier_moves_of_its_player_only() {
        let alice = PlayerId::offline("Alice");
        let bob = PlayerId::offline("Bob");
        let mut decisions = Decisions::default();
        decisions.queue(1, pending(alice, 1.0, &[7]));
        decisions.queue(2, pending(alice, 2.0, &[]));
        decisions.queue(3, pending(bob, 1.0, &[]));
        assert_eq!(decisions.reference(alice), Some(Vec3::new(2.0, 64.0, 0.0)));
        assert!(decisions.busy(alice));
        assert_eq!(settled_ids(&mut decisions, 0), [3]);
        assert!(!decisions.busy(bob));
        assert!(matches!(
            taken(&mut decisions, 7, 1, cancel(false), 0),
            Taken::Counted
        ));
        assert_eq!(settled_ids(&mut decisions, 0), [1, 2]);
        assert_eq!(decisions.reference(alice), None);
        assert!(!decisions.busy(alice));
    }

    fn notice(player: PlayerId, waiting: &[u64]) -> Pending {
        let at = Vec3::new(0.0, 64.0, 0.0);
        let asked = waiting
            .iter()
            .map(|&connection| Asked {
                connection,
                handler: 1,
                kind: EventKind::PlayerJoin,
            })
            .collect();
        Pending::new(
            player,
            EventPayload::PlayerJoin(PlayerJoin {
                player: snapshot(player, at),
                join_message: None,
            }),
            asked,
            5,
            20,
            Continuation::Notice(EventKind::PlayerJoin),
        )
    }

    #[test]
    fn an_event_born_while_the_head_of_its_lane_applies_goes_before_the_rest_of_the_lane() {
        let alice = PlayerId::offline("Alice");
        let lane = Lane::Player(alice);
        let mut decisions = Decisions::default();
        decisions.queue(1, pending(alice, 1.0, &[]));
        decisions.queue(2, notice(alice, &[]));
        let head = decisions.next_settled(0).expect("the first move is ready");
        assert_eq!(head.event, 1);
        decisions.enter(head.lane);
        assert!(!decisions.busy(alice));
        decisions.queue(3, notice(alice, &[7]));
        assert!(decisions.busy(alice));
        decisions.queue(4, notice(alice, &[]));
        decisions.leave();
        assert!(decisions.busy(lane));
        assert!(decisions.next_settled(0).is_none());
        assert!(matches!(
            taken(&mut decisions, 7, 3, cancel(false), 0),
            Taken::Counted
        ));
        assert_eq!(settled_ids(&mut decisions, 0), [3, 4, 2]);
    }

    #[test]
    fn an_event_born_cancelled_settles_cancelled_in_its_turn_without_a_verdict() {
        let alice = PlayerId::offline("Alice");
        let mut decisions = Decisions::default();
        decisions.queue(1, pending(alice, 1.0, &[7]));
        decisions.queue(2, notice(alice, &[]).refused());
        assert!(decisions.next_settled(0).is_none());
        assert!(matches!(
            taken(&mut decisions, 7, 1, cancel(false), 0),
            Taken::Counted
        ));
        let first = decisions.next_settled(0).expect("the move settles");
        assert_eq!(first.settlement, Settlement::Applied(None));
        let second = decisions
            .next_settled(0)
            .expect("the refused event follows");
        assert_eq!(
            (second.event, second.settlement),
            (2, Settlement::Cancelled(None))
        );
    }

    #[test]
    fn a_grandchild_settles_before_the_second_child_of_its_grandparent() {
        let alice = PlayerId::offline("Alice");
        let mut decisions = Decisions::default();
        decisions.queue(1, notice(alice, &[]));
        decisions.queue(9, notice(alice, &[]));
        let parent = decisions.next_settled(0).expect("the parent is ready");
        decisions.enter(parent.lane);
        decisions.queue(2, notice(alice, &[]));
        decisions.queue(3, notice(alice, &[]));
        decisions.leave();
        let child = decisions.next_settled(0).expect("the first child is ready");
        assert_eq!(child.event, 2);
        decisions.enter(child.lane);
        decisions.queue(4, notice(alice, &[]));
        decisions.leave();
        assert_eq!(settled_ids(&mut decisions, 0), [4, 3, 9]);
    }

    #[test]
    fn a_deferred_entry_comes_out_at_the_head_of_its_lane_to_be_described() {
        let alice = PlayerId::offline("Alice");
        let bob = PlayerId::offline("Bob");
        let mut decisions = Decisions::default();
        decisions.queue(1, pending(alice, 1.0, &[7]));
        decisions.defer(
            2,
            alice.into(),
            Continuation::Notice(EventKind::PlayerInput),
        );
        decisions.defer(3, bob.into(), Continuation::Notice(EventKind::PlayerInput));
        let due = decisions.next_due().expect("Bob's entry is at the head");
        assert_eq!((due.event, due.lane), (3, Lane::Player(bob)));
        assert!(decisions.next_due().is_none());
        assert!(matches!(
            taken(&mut decisions, 7, 1, cancel(false), 0),
            Taken::Counted
        ));
        assert!(decisions.next_due().is_none());
        assert_eq!(settled_ids(&mut decisions, 0), [1]);
        let due = decisions
            .next_due()
            .expect("Alice's entry reached the head");
        assert_eq!(due.event, 2);
        assert!(matches!(
            due.then,
            Continuation::Notice(EventKind::PlayerInput)
        ));
        assert!(!decisions.busy(alice));
    }

    #[test]
    fn dropping_or_forgetting_a_lane_removes_its_moves_only() {
        let alice = PlayerId::offline("Alice");
        let mut decisions = Decisions::default();
        decisions.queue(1, notice(alice, &[7]));
        decisions.queue(2, pending(alice, 1.0, &[7]));
        decisions.queue(3, notice(alice, &[7]));
        decisions.queue(4, pending(alice, 2.0, &[7]));
        let dropped: Vec<u64> = ids(&decisions.drop_pending(alice));
        assert_eq!(dropped, [2, 4]);
        assert_eq!(decisions.reference(alice), None);
        decisions.queue(5, pending(alice, 3.0, &[7]));
        assert_eq!(ids(&decisions.forget(alice)), [5]);
        assert_eq!(decisions.reference(alice), None);
        for event in [1, 3] {
            assert!(matches!(
                taken(&mut decisions, 7, event, cancel(false), 0),
                Taken::Counted
            ));
        }
        assert_eq!(settled_ids(&mut decisions, 0), [1, 3]);
        assert!(!decisions.busy(alice));
    }

    #[test]
    fn an_entry_in_flight_is_found_by_its_continuation() {
        let alice = PlayerId::offline("Alice");
        let mut decisions = Decisions::default();
        decisions.queue(1, notice(alice, &[7]));
        let lane = Lane::Player(alice);
        assert!(decisions.in_flight(lane, |then| then.kind() == EventKind::PlayerJoin));
        assert!(!decisions.in_flight(lane, Continuation::is_move));
        assert!(!decisions.in_flight(Lane::Login(1), |_| true));
    }

    #[test]
    fn the_matching_lane_to_settle_first_is_the_one_whose_head_is_oldest() {
        let alice = PlayerId::offline("Alice");
        let mob = Lane::Entity(rspigot_domain::EntityId(7));
        let mut decisions = Decisions::default();
        decisions.defer(
            5,
            alice.into(),
            Continuation::Notice(EventKind::EntityRemove),
        );
        decisions.defer(3, mob, Continuation::Notice(EventKind::EntityRemove));
        decisions.defer(4, Lane::World, Continuation::Notice(EventKind::WorldSave));
        let removal = |_: Lane, then: &Continuation| then.kind() == EventKind::EntityRemove;
        assert_eq!(decisions.lane_where(removal), Some(mob));
        let _ = decisions.flush(mob, false);
        assert_eq!(decisions.lane_where(removal), Some(Lane::Player(alice)));
    }

    #[test]
    fn the_lane_being_applied_is_the_current_one_and_entries_are_found_in_any_lane() {
        let alice = PlayerId::offline("Alice");
        let mob = Lane::Entity(rspigot_domain::EntityId(7));
        let mut decisions = Decisions::default();
        assert_eq!(decisions.current(), None);
        decisions.queue(1, notice(alice, &[7]));
        decisions.defer(2, mob, Continuation::Notice(EventKind::EntityRemove));
        assert!(decisions.anywhere(|then| then.kind() == EventKind::EntityRemove));
        assert!(!decisions.anywhere(|then| then.kind() == EventKind::EntityDeath));
        assert_eq!(decisions.waiting().count(), 2);
        decisions.enter(mob);
        decisions.enter(alice.into());
        assert_eq!(decisions.current(), Some(Lane::Player(alice)));
        decisions.leave();
        assert_eq!(decisions.current(), Some(mob));
        decisions.leave();
        assert_eq!(decisions.current(), None);
    }

    #[test]
    fn a_packet_that_is_not_an_event_waits_behind_the_last_pending_move() {
        let alice = PlayerId::offline("Alice");
        let mut decisions = Decisions::default();
        let turn = Held {
            position: None,
            rotation: Some(Rotation::new(90.0, 0.0)),
            on_ground: true,
        };
        assert_eq!(decisions.hold(alice, turn), Err(turn));
        decisions.queue(1, pending(alice, 1.0, &[]));
        decisions.queue(2, pending(alice, 2.0, &[]));
        assert_eq!(decisions.hold(alice, turn), Ok(()));
        let settled: Vec<Vec<Held>> =
            std::iter::from_fn(|| decisions.next_settled(0).map(|ready| ready.held)).collect();
        assert_eq!(settled, [Vec::new(), vec![turn]]);
    }

    #[test]
    fn a_late_cancel_counts_only_from_a_connection_that_stayed_silent() {
        let alice = PlayerId::offline("Alice");
        let mut decisions = Decisions::default();
        decisions.remember(4, record(alice, &[1, 2], 20));
        decisions.remember(5, record(alice, &[], 20));
        assert!(matches!(
            taken(&mut decisions, 3, 4, cancel(true), 3),
            Taken::Ignored
        ));
        assert!(matches!(
            taken(&mut decisions, 1, 4, cancel(false), 3),
            Taken::Ignored
        ));
        assert!(matches!(
            taken(&mut decisions, 1, 4, cancel(true), 21),
            Taken::TooLate("cancel")
        ));
        assert!(matches!(
            taken(&mut decisions, 1, 5, cancel(true), 3),
            Taken::Ignored
        ));
        assert_eq!(
            rolled_back(taken(&mut decisions, 1, 4, cancel(true), 20)),
            (vec![4, 5], vec![])
        );
        assert!(matches!(
            taken(&mut decisions, 2, 4, cancel(true), 20),
            Taken::Ignored
        ));
        decisions.expire(21);
        assert!(matches!(
            taken(&mut decisions, 2, 4, cancel(true), 21),
            Taken::TooLate("cancel")
        ));
    }

    #[test]
    fn a_rollback_undoes_what_came_after_it_for_that_player_only() {
        let alice = PlayerId::offline("Alice");
        let bob = PlayerId::offline("Bob");
        let mut decisions = Decisions::default();
        decisions.remember(1, record(alice, &[1], 20));
        decisions.remember(2, record(alice, &[1], 20));
        decisions.remember(3, record(bob, &[1], 20));
        decisions.remember(4, record(alice, &[1], 20));
        decisions.queue(5, pending(alice, 1.0, &[1]));
        decisions.queue(6, pending(bob, 1.0, &[1]));
        decisions.queue(7, pending(alice, 2.0, &[]));
        assert_eq!(
            rolled_back(taken(&mut decisions, 1, 2, cancel(true), 0)),
            (vec![2, 4], vec![5, 7])
        );
        assert!(matches!(
            taken(&mut decisions, 1, 4, cancel(true), 0),
            Taken::Ignored
        ));
        assert!(matches!(
            taken(&mut decisions, 1, 5, cancel(true), 0),
            Taken::Ignored
        ));
        assert!(matches!(
            taken(&mut decisions, 1, 7, cancel(true), 0),
            Taken::Ignored
        ));
        assert_eq!(decisions.reference(alice), None);
        assert!(decisions.reference(bob).is_some());
        assert_eq!(
            rolled_back(taken(&mut decisions, 1, 3, cancel(true), 0)),
            (vec![3], vec![6])
        );
        assert_eq!(
            rolled_back(taken(&mut decisions, 1, 1, cancel(true), 0)),
            (vec![1], vec![])
        );
    }

    #[test]
    fn answers_fold_in_the_order_handlers_were_asked_whatever_order_they_arrive() {
        let alice = PlayerId::offline("Alice");
        let asked = [(3, 1), (1, 1), (4, 1), (2, 1), (2, 2)];
        let patches = [
            (1, vec![verdict(1, false, Some(1.0))]),
            (
                2,
                vec![verdict(2, false, Some(3.0)), verdict(1, false, Some(2.0))],
            ),
            (3, vec![verdict(1, false, None)]),
            (4, vec![verdict(1, false, Some(4.0))]),
        ];
        let mut with_a_cancel = patches.clone();
        with_a_cancel[2].1 = vec![verdict(1, true, Some(9.0))];
        let three = EventPatch::PlayerMove(PlayerMovePatch {
            to: Some(location(Vec3::new(3.0, 64.0, 0.0))),
        });
        for (rulings, expected) in [
            (&patches, Settlement::Applied(Some(three))),
            (&with_a_cancel, Settlement::Cancelled(None)),
        ] {
            for arrival in arrivals(rulings.len()) {
                let mut decisions = Decisions::default();
                decisions.queue(1, asking(alice, 1.0, &asked));
                let mut settled = Vec::new();
                for (count, index) in arrival.iter().enumerate() {
                    let tick = if count < 2 { 0 } else { 1 };
                    let (connection, verdicts) = &rulings[*index];
                    let (_, warnings) = decisions.decide(*connection, 1, verdicts.clone(), tick);
                    assert!(warnings.is_empty());
                    while let Some(ready) = decisions.next_settled(tick) {
                        settled.push(ready.settlement);
                        decisions.remember(ready.event, ready.record);
                    }
                }
                assert_eq!(
                    settled,
                    std::slice::from_ref(&expected),
                    "arrival order {arrival:?}"
                );
            }
        }
    }

    #[test]
    fn a_patch_of_another_kind_is_ignored_with_a_warning() {
        let alice = PlayerId::offline("Alice");
        let at = Vec3::new(0.0, 64.0, 0.0);
        let mut decisions = Decisions::default();
        decisions.queue(
            1,
            Pending::new(
                alice,
                EventPayload::PlayerJoin(PlayerJoin {
                    player: snapshot(alice, at),
                    join_message: None,
                }),
                vec![Asked {
                    connection: 1,
                    handler: 1,
                    kind: EventKind::PlayerJoin,
                }],
                5,
                20,
                step(alice, at, at),
            ),
        );
        let (taken, warnings) = decisions.decide(1, 1, vec![verdict(1, false, Some(4.0))], 0);
        assert!(matches!(taken, Taken::Counted));
        assert_eq!(
            warnings,
            ["a PlayerMove patch does not fit event 1, which is a PlayerJoin"]
        );
        let ready = decisions.next_settled(0).expect("the event settles");
        assert_eq!(ready.settlement, Settlement::Applied(None));
    }

    fn refused_and_dropped(payload: &EventPayload, patching: impl Fn(Location) -> EventPatch) {
        let alice = PlayerId::offline("Alice");
        let at = Vec3::new(0.0, 64.0, 0.0);
        let nowhere = location(Vec3::new(f64::NAN, 64.0, 0.0));
        let elsewhere = location(Vec3::new(10.0, 64.0, 0.0));
        let kind = payload.kind();
        for (later, expected) in [
            (
                Some(elsewhere.clone()),
                Settlement::Applied(Some(patching(elsewhere.clone()))),
            ),
            (None, Settlement::Applied(None)),
        ] {
            let asked = [1, 2]
                .map(|connection| Asked {
                    connection,
                    handler: 1,
                    kind,
                })
                .to_vec();
            let mut decisions = Decisions::default();
            decisions.queue(
                1,
                Pending::new(alice, payload.clone(), asked, 5, 20, step(alice, at, at)),
            );
            let mut heard = Vec::new();
            for (connection, to) in [(1, Some(nowhere.clone())), (2, later)] {
                let ruling = Verdict {
                    handler: 1,
                    cancel: false,
                    patch: to.map(&patching),
                };
                let (taken, warnings) = decisions.decide(connection, 1, vec![ruling], 0);
                assert!(matches!(taken, Taken::Counted));
                heard.push(warnings);
            }
            assert_eq!(
                heard,
                [
                    vec![
                        "patch of event 1 was refused: position NaN 64 0 is outside the world"
                            .to_owned()
                    ],
                    Vec::new(),
                ],
                "{kind:?}"
            );
            let ready = decisions.next_settled(0).expect("the event settles");
            assert_eq!(ready.settlement, expected, "{kind:?}");
        }
    }

    #[test]
    fn a_refused_value_in_any_generated_patch_is_dropped_and_never_cancels() {
        let alice = PlayerId::offline("Alice");
        let at = Vec3::new(0.0, 64.0, 0.0);
        let respawning = EventPayload::PlayerRespawn(PlayerRespawn {
            anchor_spawn: false,
            bed_spawn: false,
            player: snapshot(alice, at),
            respawn_location: location(at),
            respawn_reason: PlayerRespawnReason::Death,
        });
        refused_and_dropped(&respawning, |to| {
            EventPatch::PlayerRespawn(PlayerRespawnPatch {
                respawn_location: Some(to),
            })
        });
        let teleporting = EventPayload::PlayerTeleport(PlayerTeleport {
            cause: PlayerTeleportCause::Command,
            from: location(at),
            player: snapshot(alice, at),
            to: Some(location(at)),
        });
        refused_and_dropped(&teleporting, |to| {
            EventPatch::PlayerTeleport(PlayerTeleportPatch {
                from: None,
                to: Some(to),
            })
        });
    }

    fn interacting(player: PlayerId) -> EventPayload {
        let at = Vec3::new(0.0, 64.0, 0.0);
        EventPayload::PlayerInteract(PlayerInteract {
            action: Action::RightClickBlock,
            block: true,
            block_face: Face::Up,
            block_in_hand: false,
            clicked_block: None,
            clicked_position: None,
            hand: None,
            has_item: false,
            item: None,
            material: "minecraft:air".to_owned(),
            player: snapshot(player, at),
            use_interacted_block: EventResult::Default,
            use_item_in_hand: EventResult::Default,
        })
    }

    fn clicking(player: PlayerId) -> EventPayload {
        let at = Vec3::new(0.0, 64.0, 0.0);
        let menu = InventoryRef {
            owner: InventoryOwner::Custom {
                plugin: "shop".to_owned(),
                id: 7,
            },
            size: 27,
            kind: "CHEST".to_owned(),
            id: 7,
        };
        EventPayload::InventoryClick(InventoryClick {
            action: InventoryAction::Unknown,
            click: ClickKind::Left,
            clicked_inventory: Some(menu.clone()),
            current_item: None,
            cursor: None,
            hotbar_button: -1,
            inventory: menu.clone(),
            left_click: true,
            raw_slot: 4,
            result: EventResult::Default,
            right_click: false,
            shift_click: false,
            slot: 4,
            slot_type: InventoryTypeSlotType::Container,
            view: InventoryView {
                top: menu.clone(),
                bottom: menu,
                title: "Menu".to_owned(),
                player: snapshot(player, at),
            },
            viewers: Vec::new(),
            who_clicked: snapshot(player, at),
        })
    }

    fn ruling(cancel: bool, patch: Option<EventPatch>) -> Vec<Verdict> {
        vec![Verdict {
            handler: 1,
            cancel,
            patch,
        }]
    }

    fn deciding(
        payload: EventPayload,
        connections: u64,
        rulings: Vec<Vec<Verdict>>,
    ) -> (Decisions, Vec<String>) {
        let alice = PlayerId::offline("Alice");
        let at = Vec3::new(0.0, 64.0, 0.0);
        let kind = payload.kind();
        let asked = (1..=connections)
            .map(|connection| Asked {
                connection,
                handler: 1,
                kind,
            })
            .collect();
        let mut decisions = Decisions::default();
        decisions.queue(
            1,
            Pending::new(alice, payload, asked, 5, 20, step(alice, at, at)),
        );
        let mut warnings = Vec::new();
        for (connection, verdicts) in (1..).zip(rulings) {
            let (taken, heard) = decisions.decide(connection, 1, verdicts, 0);
            assert!(matches!(taken, Taken::Counted));
            warnings.extend(heard);
        }
        (decisions, warnings)
    }

    fn settled(payload: EventPayload, rulings: Vec<Vec<Verdict>>) -> (Settlement, Vec<String>) {
        let connections = rulings.len() as u64;
        let (mut decisions, warnings) = deciding(payload, connections, rulings);
        let ready = decisions.next_settled(0).expect("every handler answered");
        (ready.settlement, warnings)
    }

    #[test]
    fn a_result_event_is_cancelled_by_its_main_result_only() {
        let alice = PlayerId::offline("Alice");
        let item_denied = EventPatch::PlayerInteract(PlayerInteractPatch {
            use_item_in_hand: Some(EventResult::Deny),
            ..PlayerInteractPatch::default()
        });
        assert_eq!(
            settled(
                interacting(alice),
                vec![ruling(false, Some(item_denied.clone()))]
            ),
            (Settlement::Applied(Some(item_denied)), Vec::new())
        );
        let denied = EventPatch::PlayerInteract(PlayerInteractPatch {
            use_interacted_block: Some(EventResult::Deny),
            use_item_in_hand: Some(EventResult::Deny),
        });
        assert_eq!(
            settled(interacting(alice), vec![ruling(true, None)]),
            (Settlement::Cancelled(Some(denied)), Vec::new())
        );
    }

    #[test]
    fn a_later_handler_lifts_the_cancel_of_a_result_event() {
        let alice = PlayerId::offline("Alice");
        let result = |result| {
            EventPatch::InventoryClick(InventoryClickPatch {
                result: Some(result),
                ..InventoryClickPatch::default()
            })
        };
        let (mut decisions, warnings) = deciding(
            clicking(alice),
            2,
            vec![
                ruling(true, None),
                ruling(false, Some(result(EventResult::Allow))),
            ],
        );
        assert!(warnings.is_empty());
        let ready = decisions.next_settled(0).expect("every handler answered");
        assert_eq!(
            ready.settlement,
            Settlement::Applied(Some(result(EventResult::Allow)))
        );
        assert_eq!(
            settled(
                clicking(alice),
                vec![
                    ruling(false, Some(result(EventResult::Allow))),
                    ruling(true, None)
                ]
            ),
            (
                Settlement::Cancelled(Some(result(EventResult::Deny))),
                Vec::new()
            )
        );
    }

    #[test]
    fn a_cancel_on_an_event_that_cannot_be_cancelled_is_ignored_with_a_warning() {
        let alice = PlayerId::offline("Alice");
        let at = Vec3::new(0.0, 64.0, 0.0);
        let respawning = EventPayload::PlayerRespawn(PlayerRespawn {
            anchor_spawn: false,
            bed_spawn: false,
            player: snapshot(alice, at),
            respawn_location: location(at),
            respawn_reason: PlayerRespawnReason::Death,
        });
        let elsewhere = EventPatch::PlayerRespawn(PlayerRespawnPatch {
            respawn_location: Some(location(Vec3::new(10.0, 64.0, 0.0))),
        });
        assert_eq!(
            settled(respawning, vec![ruling(true, Some(elsewhere.clone()))]),
            (
                Settlement::Applied(Some(elsewhere)),
                vec!["event 1 cannot be cancelled, the cancel of handler 1 is ignored".to_owned()]
            )
        );
    }

    #[test]
    fn a_velocity_that_is_not_finite_is_dropped_and_does_not_cancel() {
        let alice = PlayerId::offline("Alice");
        let pushed = EventPayload::PlayerVelocity(PlayerVelocity {
            player: snapshot(alice, Vec3::new(0.0, 64.0, 0.0)),
            velocity: Vec3::new(0.0, 0.5, 0.0),
        });
        let nowhere = EventPatch::PlayerVelocity(PlayerVelocityPatch {
            velocity: Some(Vec3::new(f64::NAN, 0.5, 0.0)),
        });
        assert_eq!(
            settled(pushed, vec![ruling(false, Some(nowhere))]),
            (
                Settlement::Applied(None),
                vec!["patch of event 1 was refused: velocity NaN 0.5 0 is not finite".to_owned()]
            )
        );
    }

    #[test]
    fn a_cancelled_click_keeps_its_direct_writes_and_waits_for_every_handler() {
        let alice = PlayerId::offline("Alice");
        let cleared = EventPatch::InventoryClick(InventoryClickPatch {
            current_item: Some(None),
            ..InventoryClickPatch::default()
        });
        let (mut decisions, warnings) =
            deciding(clicking(alice), 2, vec![ruling(true, Some(cleared))]);
        assert!(warnings.is_empty());
        assert!(decisions.next_settled(0).is_none());
        let (taken, _) = decisions.decide(2, 1, ruling(false, None), 0);
        assert!(matches!(taken, Taken::Counted));
        let ready = decisions.next_settled(0).expect("every handler answered");
        assert_eq!(
            ready.settlement,
            Settlement::Cancelled(Some(EventPatch::InventoryClick(InventoryClickPatch {
                current_item: Some(None),
                cursor: None,
                result: Some(EventResult::Deny),
            })))
        );
    }

    #[test]
    fn a_decision_without_a_cancel_warns_for_each_handler_left_without_a_verdict() {
        let alice = PlayerId::offline("Alice");
        let mut decisions = Decisions::default();
        decisions.queue(1, asking(alice, 1.0, &[(1, 1), (2, 1), (1, 2), (1, 3)]));
        let (taken, warnings) = decisions.decide(1, 1, vec![verdict(2, false, None)], 0);
        assert!(matches!(taken, Taken::Counted));
        assert_eq!(
            warnings,
            [
                "handler 1 of event 1 gave no verdict",
                "handler 3 of event 1 gave no verdict",
            ]
        );
        let (_, warnings) = decisions.decide(2, 1, Vec::new(), 0);
        assert_eq!(warnings, ["handler 1 of event 1 gave no verdict"]);
    }

    #[test]
    fn a_decision_with_a_cancel_leaves_its_other_handlers_without_a_warning() {
        let alice = PlayerId::offline("Alice");
        let mut decisions = Decisions::default();
        decisions.queue(1, asking(alice, 1.0, &[(1, 1), (1, 2), (1, 3)]));
        let (taken, warnings) = decisions.decide(1, 1, vec![verdict(2, true, None)], 0);
        assert!(matches!(taken, Taken::Counted));
        assert!(warnings.is_empty());
        let ready = decisions.next_settled(0).expect("the event settles");
        assert_eq!(ready.settlement, Settlement::Cancelled(None));
    }

    #[test]
    fn a_second_decision_from_a_plugin_is_ignored_with_a_warning() {
        let alice = PlayerId::offline("Alice");
        let mut decisions = Decisions::default();
        decisions.queue(1, pending(alice, 1.0, &[1, 2]));
        assert!(matches!(
            taken(&mut decisions, 1, 1, cancel(false), 0),
            Taken::Counted
        ));
        let (again, warnings) = decisions.decide(1, 1, cancel(true), 0);
        assert!(matches!(again, Taken::Ignored));
        assert_eq!(warnings, ["event 1 was already answered by this plugin"]);
        let (stranger, warnings) = decisions.decide(3, 1, cancel(true), 0);
        assert!(matches!(stranger, Taken::Ignored));
        assert!(warnings.is_empty());
        assert!(decisions.next_settled(0).is_none());
    }

    #[test]
    fn a_late_verdict_on_an_event_that_did_not_go_through_is_ignored_silently() {
        let alice = PlayerId::offline("Alice");
        let mut decisions = Decisions::default();
        let mut called_off = record(alice, &[1], 20);
        called_off.answered = handlers(&[(2, 1)]);
        called_off.applied = None;
        decisions.remember(1, called_off);
        for (connection, verdicts) in [
            (1, vec![verdict(1, true, Some(4.0))]),
            (1, vec![verdict(7, false, None)]),
            (2, cancel(true)),
            (3, vec![verdict(1, false, Some(4.0))]),
        ] {
            let (taken, warnings) = decisions.decide(connection, 1, verdicts, 20);
            assert!(matches!(taken, Taken::Ignored));
            assert!(warnings.is_empty());
        }
    }

    #[test]
    fn a_second_decision_on_an_applied_event_warns_the_plugin_that_answered() {
        let alice = PlayerId::offline("Alice");
        let mut decisions = Decisions::default();
        let mut applied = record(alice, &[1], 20);
        applied.answered = handlers(&[(2, 1)]);
        decisions.remember(1, applied);
        let (again, warnings) = decisions.decide(2, 1, cancel(true), 3);
        assert!(matches!(again, Taken::Ignored));
        assert_eq!(warnings, ["event 1 was already answered by this plugin"]);
        let (stranger, warnings) = decisions.decide(3, 1, cancel(true), 3);
        assert!(matches!(stranger, Taken::Ignored));
        assert!(warnings.is_empty());
    }

    #[test]
    fn a_settled_event_is_kept_until_its_deadline_but_rolls_back_only_within_its_window() {
        let alice = PlayerId::offline("Alice");
        let mut decisions = Decisions::default();
        let mut short = record(alice, &[1], 2);
        short.deadline = 5;
        decisions.remember(1, short);
        decisions.expire(5);
        assert!(matches!(
            taken(&mut decisions, 1, 1, cancel(true), 4),
            Taken::TooLate("cancel")
        ));
        assert!(matches!(
            taken(&mut decisions, 1, 1, vec![verdict(1, false, Some(4.0))], 4),
            Taken::TooLate("patch")
        ));
        assert_eq!(
            rolled_back(taken(&mut decisions, 1, 1, cancel(true), 2)),
            (vec![1], vec![])
        );
        let mut settled = Decisions::default();
        settled.queue(2, asking(alice, 1.0, &[(1, 1)]));
        let ready = settled.next_settled(9).expect("the deadline has passed");
        let mut applied = ready.record;
        applied.applied = Some(ready.payload);
        settled.remember(ready.event, applied);
        settled.expire(20);
        assert!(matches!(
            taken(&mut settled, 1, 2, cancel(true), 20),
            Taken::RolledBack(_)
        ));
        settled.expire(21);
        assert!(matches!(
            taken(&mut settled, 1, 2, cancel(true), 21),
            Taken::TooLate("cancel")
        ));
    }

    #[test]
    fn a_late_patch_is_too_late_unless_the_event_was_called_off() {
        let alice = PlayerId::offline("Alice");
        let mut decisions = Decisions::default();
        decisions.remember(1, record(alice, &[1], 20));
        let mut cancelled = record(alice, &[1], 20);
        cancelled.applied = None;
        decisions.remember(2, cancelled);
        let patch = || vec![verdict(1, false, Some(4.0))];
        assert!(matches!(
            taken(&mut decisions, 1, 1, patch(), 3),
            Taken::TooLate("patch")
        ));
        assert!(matches!(
            taken(&mut decisions, 2, 1, patch(), 3),
            Taken::Ignored
        ));
        assert!(matches!(
            taken(&mut decisions, 1, 2, patch(), 3),
            Taken::Ignored
        ));
        assert!(matches!(
            taken(&mut decisions, 1, 9, patch(), 3),
            Taken::TooLate("patch")
        ));
        assert!(matches!(
            taken(&mut decisions, 1, 1, Vec::new(), 3),
            Taken::Ignored
        ));
        let (unknown, warnings) = decisions.decide(1, 1, vec![verdict(5, true, None)], 3);
        assert!(matches!(unknown, Taken::Ignored));
        assert_eq!(warnings, ["handler 5 was not asked to decide event 1"]);
        assert_eq!(
            rolled_back(taken(
                &mut decisions,
                1,
                1,
                vec![verdict(1, true, Some(4.0))],
                3
            )),
            (vec![1], vec![])
        );
    }

    fn entity(id: i32) -> EntityView {
        EntityView {
            id: rspigot_domain::EntityId(id),
            uuid: uuid::Uuid::from_u128(u128::try_from(id).unwrap_or_default()),
            kind: "minecraft:zombie".to_owned(),
            position: Vec3::new(0.0, 64.0, 0.0),
            player: None,
            meta: crate::plugins::MetaSnapshot::default(),
        }
    }

    fn hit(damage: f64) -> EventPayload {
        EventPayload::EntityDamageByEntity(EntityDamageByEntity {
            cause: EntityDamageCause::EntityAttack,
            damage,
            damage_source: DamageSourceView {
                kind: "minecraft:player_attack".to_owned(),
                causing: Some(entity(2)),
                direct: Some(entity(2)),
                location: None,
            },
            damager: entity(2),
            entity: entity(1),
            entity_type: "minecraft:zombie".to_owned(),
            final_damage: damage,
        })
    }

    fn damage(kind: EventKind, handler: u32, amount: f64) -> Verdict {
        let patch = if kind == EventKind::EntityDamage {
            EventPatch::EntityDamage(EntityDamagePatch {
                damage: Some(amount),
            })
        } else {
            EventPatch::EntityDamageByEntity(EntityDamageByEntityPatch {
                damage: Some(amount),
            })
        };
        Verdict {
            handler,
            cancel: false,
            patch: Some(patch),
        }
    }

    fn asked_in(order: &[(u64, u32, EventKind)]) -> Vec<Asked> {
        order
            .iter()
            .map(|&(connection, handler, kind)| Asked {
                connection,
                handler,
                kind,
            })
            .collect()
    }

    fn settle_hit(order: &[(u64, u32, EventKind)], arrival: &[usize]) -> (Settlement, Vec<String>) {
        let alice = PlayerId::offline("Alice");
        let at = Vec3::new(0.0, 64.0, 0.0);
        let mut decisions = Decisions::default();
        decisions.queue(
            1,
            Pending::new(alice, hit(4.0), asked_in(order), 5, 20, step(alice, at, at)),
        );
        let mut warnings = Vec::new();
        for &index in arrival {
            let (connection, handler, kind) = order[index];
            let amount = if kind == EventKind::EntityDamage {
                1.0
            } else {
                9.0
            };
            let (_, said) = decisions.decide(connection, 1, vec![damage(kind, handler, amount)], 0);
            warnings.extend(said);
        }
        let ready = decisions.next_settled(0).expect("the hit settles");
        (ready.settlement, warnings)
    }

    #[test]
    fn a_family_folds_the_patches_of_a_parent_and_a_child_handler_in_registry_order() {
        let parent_first = [
            (1, 1, EventKind::EntityDamage),
            (2, 1, EventKind::EntityDamageByEntity),
        ];
        let child_first = [parent_first[1], parent_first[0]];
        let by = |amount| {
            Settlement::Applied(Some(EventPatch::EntityDamageByEntity(
                EntityDamageByEntityPatch {
                    damage: Some(amount),
                },
            )))
        };
        for arrival in arrivals(2) {
            assert_eq!(settle_hit(&parent_first, &arrival), (by(9.0), Vec::new()));
            assert_eq!(settle_hit(&child_first, &arrival), (by(1.0), Vec::new()));
        }
    }

    #[test]
    fn a_handler_patches_only_the_event_it_registered_for() {
        let alice = PlayerId::offline("Alice");
        let at = Vec3::new(0.0, 64.0, 0.0);
        let mut decisions = Decisions::default();
        decisions.queue(
            1,
            Pending::new(
                alice,
                hit(4.0),
                asked_in(&[(1, 1, EventKind::EntityDamage)]),
                5,
                20,
                step(alice, at, at),
            ),
        );
        let (_, warnings) = decisions.decide(
            1,
            1,
            vec![damage(EventKind::EntityDamageByEntity, 1, 2.0)],
            0,
        );
        assert_eq!(
            warnings,
            ["a EntityDamageByEntity patch does not fit event 1, which is a EntityDamage"]
        );
        let ready = decisions.next_settled(0).expect("the hit settles");
        assert_eq!(ready.settlement, Settlement::Applied(None));
    }

    fn listener(name: &str) -> PlayerSnapshot {
        PlayerSnapshot {
            permissions: Vec::new(),
            name: name.to_owned(),
            ..snapshot(PlayerId::offline(name), Vec3::new(0.0, 64.0, 0.0))
        }
    }

    fn chat(names: &[&str]) -> EventPayload {
        EventPayload::AsyncPlayerChat(AsyncPlayerChat {
            format: "<%1$s> %2$s".to_owned(),
            message: "hello".to_owned(),
            player: listener("Alice"),
            recipients: names.iter().map(|name| listener(name)).collect(),
        })
    }

    fn recipients(removed: &[u32], added: &[&str]) -> EventPatch {
        EventPatch::AsyncPlayerChat(AsyncPlayerChatPatch {
            recipients: ListDiff {
                removed: removed.to_vec(),
                replaced: Vec::new(),
                added: added.iter().map(|name| listener(name)).collect(),
            },
            ..AsyncPlayerChatPatch::default()
        })
    }

    fn lists_settled(payload: &EventPayload, rulings: &[Verdict]) -> Vec<Settlement> {
        let alice = PlayerId::offline("Alice");
        let at = Vec3::new(0.0, 64.0, 0.0);
        let asked: Vec<Asked> = rulings
            .iter()
            .zip(1..)
            .map(|(ruling, connection)| Asked {
                connection,
                handler: 1,
                kind: ruling
                    .patch
                    .as_ref()
                    .map_or(payload.kind(), EventPatch::kind),
            })
            .collect();
        arrivals(rulings.len())
            .into_iter()
            .map(|arrival| {
                let mut decisions = Decisions::default();
                let pending = Pending::new(
                    alice,
                    payload.clone(),
                    asked.clone(),
                    5,
                    20,
                    step(alice, at, at),
                );
                decisions.queue(1, pending);
                for index in arrival {
                    let ruling = rulings[index].clone();
                    let (_, warnings) =
                        decisions.decide(asked[index].connection, 1, vec![ruling], 0);
                    assert!(warnings.is_empty(), "{warnings:?}");
                }
                decisions
                    .next_settled(0)
                    .expect("the event settles")
                    .settlement
            })
            .collect()
    }

    fn settled_lists(payload: &EventPayload, patches: &[EventPatch]) -> EventPayload {
        let rulings: Vec<Verdict> = patches
            .iter()
            .map(|patch| Verdict {
                handler: 1,
                cancel: false,
                patch: Some(patch.clone()),
            })
            .collect();
        let outcomes: Vec<EventPayload> = lists_settled(payload, &rulings)
            .into_iter()
            .map(|settlement| {
                let Settlement::Applied(Some(patch)) = settlement else {
                    panic!("expected an applied patch, got {settlement:?}");
                };
                let mut settled = payload.clone();
                patch.settle(&mut settled);
                settled
            })
            .collect();
        assert!(outcomes.windows(2).all(|pair| pair[0] == pair[1]));
        outcomes[0].clone()
    }

    fn names_of(payload: &EventPayload) -> Vec<&str> {
        let EventPayload::AsyncPlayerChat(chat) = payload else {
            panic!("expected a chat");
        };
        chat.recipients
            .iter()
            .map(|player| player.name.as_str())
            .collect()
    }

    #[test]
    fn the_removals_of_every_handler_apply_whatever_order_the_answers_arrive_in() {
        let payload = chat(&["Alice", "Bob", "Carol", "Dave"]);
        for patches in [
            [recipients(&[1], &[]), recipients(&[2], &[])],
            [recipients(&[2], &[]), recipients(&[1], &[])],
        ] {
            assert_eq!(
                names_of(&settled_lists(&payload, &patches)),
                ["Alice", "Dave"]
            );
        }
    }

    #[test]
    fn additions_follow_the_registry_order_and_an_element_added_back_is_present_once() {
        let payload = chat(&["Alice", "Bob", "Carol", "Dave"]);
        let patches = [
            recipients(&[1], &["Erin"]),
            recipients(&[3], &[]),
            recipients(&[], &["Bob", "Frank"]),
        ];
        assert_eq!(
            names_of(&settled_lists(&payload, &patches)),
            ["Alice", "Carol", "Erin", "Bob", "Frank"]
        );
        let reversed = [patches[2].clone(), patches[1].clone(), patches[0].clone()];
        assert_eq!(
            names_of(&settled_lists(&payload, &reversed)),
            ["Alice", "Carol", "Bob", "Frank", "Erin"]
        );
    }

    #[test]
    fn completions_commands_and_hidden_players_accumulate_too() {
        let console = SenderView::Console;
        let completions = |removed: &[u32], added: &[&str]| {
            EventPatch::TabComplete(TabCompletePatch {
                completions: ListDiff {
                    removed: removed.to_vec(),
                    replaced: Vec::new(),
                    added: added.iter().map(|text| (*text).to_owned()).collect(),
                },
            })
        };
        let typed = EventPayload::TabComplete(TabComplete {
            buffer: "/give ".to_owned(),
            completions: ["alice", "bob", "carol"].map(str::to_owned).to_vec(),
            sender: console,
        });
        let EventPayload::TabComplete(settled) = settled_lists(
            &typed,
            &[completions(&[0], &["erin"]), completions(&[2], &[])],
        ) else {
            panic!("expected a tab completion");
        };
        assert_eq!(settled.completions, ["bob", "erin"]);
        let sent = EventPayload::PlayerCommandSend(PlayerCommandSend {
            commands: ["help", "me", "msg", "tell"].map(str::to_owned).to_vec(),
            player: listener("Alice"),
        });
        let removing = |index: u32| {
            EventPatch::PlayerCommandSend(PlayerCommandSendPatch {
                commands: ListDiff {
                    removed: vec![index],
                    replaced: Vec::new(),
                    added: Vec::new(),
                },
            })
        };
        let EventPayload::PlayerCommandSend(settled) =
            settled_lists(&sent, &[removing(1), removing(3)])
        else {
            panic!("expected a command send");
        };
        assert_eq!(settled.commands, ["help", "msg"]);
        let bob = uuid::Uuid::from_u128(2);
        let carol = uuid::Uuid::from_u128(3);
        let pinged = EventPayload::ServerListPing(ServerListPing {
            address: std::net::IpAddr::from([127, 0, 0, 1]),
            hostname: "localhost".to_owned(),
            players: vec![listener("Bob"), listener("Carol")],
            max_players: 20,
            motd: "A Minecraft Server".to_owned(),
            num_players: 2,
            hidden_players: Vec::new(),
        });
        let hiding = |uuid| {
            EventPatch::ServerListPing(ServerListPingPatch {
                hidden_players: ListDiff {
                    removed: Vec::new(),
                    replaced: Vec::new(),
                    added: vec![uuid],
                },
                ..ServerListPingPatch::default()
            })
        };
        let EventPayload::ServerListPing(settled) =
            settled_lists(&pinged, &[hiding(bob), hiding(carol)])
        else {
            panic!("expected a ping");
        };
        assert_eq!(settled.hidden_players, [bob, carol]);
    }

    #[test]
    fn a_parent_handler_and_a_child_handler_both_remove_a_drop() {
        let dying = EventPayload::PlayerDeath(PlayerDeath {
            damage_source: DamageSourceView {
                kind: "minecraft:generic_kill".to_owned(),
                causing: None,
                direct: None,
                location: None,
            },
            death_message: None,
            dropped_exp: 0,
            drops: vec![
                ItemSpec::of("minecraft:diamond", 5),
                ItemSpec::of("minecraft:stick", 2),
                ItemSpec::of("minecraft:dirt", 1),
            ],
            entity: listener("Alice"),
            entity_type: "minecraft:player".to_owned(),
            keep_inventory: false,
            keep_level: false,
            new_exp: 0,
            new_level: 0,
            new_total_exp: 0,
        });
        let parent = EventPatch::EntityDeath(EntityDeathPatch {
            drops: ListDiff {
                removed: vec![0],
                replaced: Vec::new(),
                added: vec![ItemSpec::of("minecraft:apple", 1)],
            },
            ..EntityDeathPatch::default()
        });
        let child = EventPatch::PlayerDeath(PlayerDeathPatch {
            drops: ListDiff {
                removed: vec![2],
                replaced: Vec::new(),
                added: Vec::new(),
            },
            ..PlayerDeathPatch::default()
        });
        for patches in [[parent.clone(), child.clone()], [child, parent]] {
            let EventPayload::PlayerDeath(settled) = settled_lists(&dying, &patches) else {
                panic!("expected a player death");
            };
            assert_eq!(
                settled.drops,
                [
                    ItemSpec::of("minecraft:stick", 2),
                    ItemSpec::of("minecraft:apple", 1)
                ]
            );
        }
    }

    fn dropped(id: i32, key: &str, count: i32) -> DroppedItem {
        DroppedItem {
            entity: entity(id),
            item: ItemSpec::of(key, count),
        }
    }

    fn dropping() -> EventPayload {
        let at = rspigot_domain::BlockPos::new(1, 63, 0);
        EventPayload::BlockDropItem(BlockDropItem {
            block: at,
            block_state: BlockSnapshot {
                pos: at,
                state: BlockState {
                    kind: "minecraft:air".to_owned(),
                    properties: Vec::new(),
                },
            },
            items: vec![
                dropped(7, "minecraft:dirt", 1),
                dropped(8, "minecraft:dirt", 1),
            ],
            player: listener("Alice"),
        })
    }

    fn items(removed: &[u32], replaced: Vec<(u32, DroppedItem)>) -> EventPatch {
        EventPatch::BlockDropItem(BlockDropItemPatch {
            items: ListDiff {
                removed: removed.to_vec(),
                replaced,
                added: Vec::new(),
            },
        })
    }

    fn items_of(payload: &EventPayload) -> Vec<DroppedItem> {
        let EventPayload::BlockDropItem(dropping) = payload else {
            panic!("expected a block drop");
        };
        dropping.items.clone()
    }

    #[test]
    fn an_item_cleared_by_one_handler_is_gone_even_if_another_doubles_it() {
        let cleared = items(&[0, 1], Vec::new());
        let doubled = items(&[], vec![(0, dropped(7, "minecraft:dirt", 2))]);
        for patches in [[cleared.clone(), doubled.clone()], [doubled, cleared]] {
            assert!(items_of(&settled_lists(&dropping(), &patches)).is_empty());
        }
    }

    #[test]
    fn two_replacements_of_one_item_keep_the_later_handler_one() {
        let diamond = items(&[], vec![(0, dropped(7, "minecraft:diamond", 1))]);
        let emerald = items(&[], vec![(0, dropped(7, "minecraft:emerald", 3))]);
        assert_eq!(
            items_of(&settled_lists(
                &dropping(),
                &[diamond.clone(), emerald.clone()]
            )),
            [
                dropped(7, "minecraft:emerald", 3),
                dropped(8, "minecraft:dirt", 1)
            ]
        );
        assert_eq!(
            items_of(&settled_lists(&dropping(), &[emerald, diamond])),
            [
                dropped(7, "minecraft:diamond", 1),
                dropped(8, "minecraft:dirt", 1)
            ]
        );
    }

    #[test]
    fn two_handlers_adding_the_same_broadcast_recipient_give_it_once() {
        let broadcast = EventPayload::BroadcastMessage(BroadcastMessage {
            message: "restarting".to_owned(),
            recipients: vec![SenderView::Console, SenderView::Player(listener("Alice"))],
        });
        let adding = |name: &str| {
            EventPatch::BroadcastMessage(BroadcastMessagePatch {
                recipients: ListDiff {
                    removed: Vec::new(),
                    replaced: Vec::new(),
                    added: vec![SenderView::Player(listener(name))],
                },
                ..BroadcastMessagePatch::default()
            })
        };
        let EventPayload::BroadcastMessage(settled) =
            settled_lists(&broadcast, &[adding("Bob"), adding("Bob"), adding("Alice")])
        else {
            panic!("expected a broadcast");
        };
        assert_eq!(
            settled.recipients,
            [
                SenderView::Console,
                SenderView::Player(listener("Alice")),
                SenderView::Player(listener("Bob"))
            ]
        );
    }

    #[test]
    fn two_handlers_adding_the_same_explosion_position_break_it_once() {
        let at = rspigot_domain::BlockPos::new(2, 63, 0);
        let far = rspigot_domain::BlockPos::new(5, 60, 5);
        let blast = EventPayload::BlockExplode(BlockExplode {
            block: at,
            block_list: vec![at],
            exploded_block_state: BlockSnapshot {
                pos: at,
                state: BlockState {
                    kind: "minecraft:air".to_owned(),
                    properties: Vec::new(),
                },
            },
            explosion_result: ExplosionResult::Destroy,
            explosion_yield: 1.0,
        });
        let adding = EventPatch::BlockExplode(BlockExplodePatch {
            block_list: ListDiff {
                removed: Vec::new(),
                replaced: Vec::new(),
                added: vec![far, at],
            },
            ..BlockExplodePatch::default()
        });
        let EventPayload::BlockExplode(settled) = settled_lists(&blast, &[adding.clone(), adding])
        else {
            panic!("expected a block explosion");
        };
        assert_eq!(settled.block_list, [at, far]);
    }

    #[test]
    fn a_diff_naming_a_missing_element_is_refused_and_the_others_still_apply() {
        let payload = chat(&["Alice", "Bob"]);
        let alice = PlayerId::offline("Alice");
        let at = Vec3::new(0.0, 64.0, 0.0);
        let asked = asked_in(&[
            (1, 1, EventKind::AsyncPlayerChat),
            (2, 1, EventKind::AsyncPlayerChat),
        ]);
        let mut decisions = Decisions::default();
        decisions.queue(
            1,
            Pending::new(alice, payload, asked, 5, 20, step(alice, at, at)),
        );
        let wrong = Verdict {
            handler: 1,
            cancel: false,
            patch: Some(recipients(&[0, 2], &[])),
        };
        let (_, warnings) = decisions.decide(1, 1, vec![wrong], 0);
        assert_eq!(
            warnings,
            ["patch of event 1 was refused: recipients has no element 2"]
        );
        let right = Verdict {
            handler: 1,
            cancel: false,
            patch: Some(recipients(&[1], &[])),
        };
        let (_, warnings) = decisions.decide(2, 1, vec![right], 0);
        assert!(warnings.is_empty());
        let ready = decisions.next_settled(0).expect("the chat settles");
        assert_eq!(
            ready.settlement,
            Settlement::Applied(Some(recipients(&[1], &[])))
        );
    }

    #[test]
    fn a_cancel_drops_every_list_diff() {
        let payload = chat(&["Alice", "Bob", "Carol"]);
        let rulings = [
            Verdict {
                handler: 1,
                cancel: false,
                patch: Some(recipients(&[1], &["Erin"])),
            },
            Verdict {
                handler: 1,
                cancel: true,
                patch: None,
            },
        ];
        for settlement in lists_settled(&payload, &rulings) {
            assert_eq!(settlement, Settlement::Cancelled(None));
        }
    }
}
