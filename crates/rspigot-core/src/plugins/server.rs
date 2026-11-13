use std::fmt::Display;

use rspigot_domain::effect::EffectInstance;
use rspigot_domain::player::PlayerId;
use rspigot_domain::{EntityId, Rotation, Vec3};

use crate::commands::CommandContext;
use crate::events::ClientEvent;
use crate::player::Player;
use crate::plugins::aims::settled_target;
use crate::plugins::burning::settled_duration;
use crate::plugins::clicks::Writes;
use crate::plugins::damage::settled_damage;
use crate::plugins::decisions::{
    Asked, Continuation, Flushed, Held, Lane, MoveStep, Pending, Ready, Settlement, Taken, Undo,
    Undone,
};
use crate::plugins::events::{EventResult, PlayerMove, PlayerMovePatch, PlayerTeleportCause};
use crate::plugins::furnaces::FurnaceAnswer;
use crate::plugins::knockback::settled_knockback;
use crate::plugins::objects::settled_results;
use crate::plugins::player::SPEED_SCALE;
use crate::plugins::prepare::settled_result;
use crate::plugins::{
    Access, Answer, EffectSpec, EventKind, EventPatch, EventPayload, FromPlugin, HandlerSpec,
    Location, Order, Phase, PlayerSnapshot, PlayerState, Question, ToPlugin, Verdict,
};
use crate::ports::ClientLink;
use crate::server::Server;

macro_rules! patch_of {
    ($patch:expr, $kind:ident) => {
        match $patch {
            Some(EventPatch::$kind(patch)) => patch,
            _ => Default::default(),
        }
    };
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn next_event_id(&mut self) -> u64 {
        self.plugins.next_event_id()
    }

    pub(crate) fn notify_plugins(
        &mut self,
        id: u64,
        payload: &EventPayload,
        cancelled: bool,
        phase: Phase,
    ) {
        let listeners =
            self.plugins
                .listeners(payload.kind(), Access::Observe, &self.config.plugins);
        self.send_notice(id, listeners, payload, cancelled, phase);
    }

    pub(crate) fn notify_others(&mut self, concerned: u64, payload: &EventPayload) {
        let mut listeners =
            self.plugins
                .listeners(payload.kind(), Access::Observe, &self.config.plugins);
        listeners.retain(|(connection, _)| *connection != concerned);
        if listeners.is_empty() {
            return;
        }
        let id = self.next_event_id();
        self.send_notice(id, listeners, payload, false, Phase::Settled);
    }

    fn send_notice(
        &self,
        id: u64,
        listeners: Vec<(u64, HandlerSpec)>,
        payload: &EventPayload,
        cancelled: bool,
        phase: Phase,
    ) {
        let (cancelled, heard) = audience(listeners, payload, cancelled, phase);
        for (connection, handlers) in heard {
            self.plugins.send(
                connection,
                ToPlugin::Event {
                    id,
                    handlers,
                    phase,
                    cancelled,
                    payload: payload.clone(),
                },
            );
        }
    }

    pub(crate) fn snapshot_of(&self, id: PlayerId) -> Option<PlayerSnapshot> {
        self.players.get(&id).map(|player| self.describe(player))
    }

    pub(crate) fn plugin_message(&mut self, connection: u64, message: FromPlugin) {
        if !self.plugins.is_current(connection) {
            return;
        }
        match message {
            FromPlugin::Decision { event, verdicts } => {
                self.take_decision(connection, event, verdicts);
            }
            FromPlugin::Order(order) => self.apply_order(connection, None, order),
            FromPlugin::Reaction { event, order } => {
                self.apply_order(connection, Some(event), order);
            }
            FromPlugin::Question { request, question } => {
                if let Some(answer) = self.answer_question(connection, request, question) {
                    self.plugins
                        .send(connection, ToPlugin::Answer { request, answer });
                }
            }
        }
    }

    fn apply_order(&mut self, connection: u64, origin: Option<u64>, order: Order) {
        match order {
            Order::Player { player, order } => self.apply_player_order(connection, player, order),
            Order::Inventory { player, order } => {
                self.apply_inventory_order(connection, player, order);
            }
            Order::Server(order) => self.apply_server_order(connection, origin, order),
            Order::World(order) => self.apply_world_order(connection, order),
            Order::Block { pos, order } => self.apply_block_order(connection, pos, order),
            Order::Entity { entity, order } => self.apply_entity_order(connection, entity, order),
            Order::CustomInventory { inventory, order } => {
                self.apply_custom_inventory_order(connection, inventory, order);
            }
        }
    }

    pub(super) fn plugin_name(&self, connection: u64) -> String {
        self.plugins.name(connection).unwrap_or_default().to_owned()
    }

    pub(super) fn warn_plugin(&self, connection: u64, order: &str, reason: impl Display) {
        self.plugins.send(
            connection,
            ToPlugin::Warning(format!("{order} was not applied: {reason}")),
        );
    }

    fn answer_question(
        &mut self,
        connection: u64,
        request: u64,
        question: Question,
    ) -> Option<Answer> {
        Some(match question {
            Question::Player(id) => self.state_of(id).map_or(Answer::NotFound, Answer::Player),
            Question::Players => Answer::Players(self.online_snapshots()),
            Question::PlayerNamed(name) => Answer::Players(
                CommandContext::find_player(self, &name)
                    .and_then(|id| self.snapshot_of(id))
                    .into_iter()
                    .collect(),
            ),
            Question::Inventory(id) => self
                .inventory_state(id)
                .map_or(Answer::NotFound, Answer::Inventory),
            Question::TargetBlock {
                player,
                max_distance,
            } => self.target_block(player, max_distance),
            Question::TargetEntity {
                player,
                max_distance,
            } => self.target_entity(player, max_distance),
            Question::Server => Answer::Server(self.server_state()),
            Question::Whitelist => Answer::Names(self.admin.whitelist.clone()),
            Question::Operators => Answer::Names(self.admin.operators.clone()),
            Question::Bans => Answer::Bans(self.ban_entries()),
            Question::IpBans => Answer::IpBans(self.ip_ban_entries()),
            Question::World => Answer::World(self.world_state()),
            Question::Block(pos) => self
                .block_state(pos)
                .map_or(Answer::NotFound, Answer::Block),
            Question::Entities { near, kind } => self
                .listed_entities(near, kind.as_deref())
                .map_or_else(Answer::Refused, Answer::Entities),
            Question::Entity(id) => self
                .entity_details(id)
                .map_or(Answer::NotFound, Answer::Entity),
            Question::Spawn { kind, position } => {
                return self
                    .spawn_for_plugin(connection, request, &kind, position)
                    .err()
                    .map(Answer::Refused);
            }
            Question::DropItem { position, item } => {
                return self
                    .drop_for_plugin(connection, request, position, &item)
                    .err()
                    .map(Answer::Refused);
            }
            Question::CustomInventory(inventory) => self
                .custom_inventory_state(connection, inventory)
                .map_or(Answer::NotFound, Answer::CustomInventory),
        })
    }

    pub(crate) fn decide_move(
        &mut self,
        id: PlayerId,
        from: Vec3,
        to: Vec3,
        rotation: Option<Rotation>,
        on_ground: bool,
    ) {
        let step = MoveStep {
            player: id,
            from,
            to,
            rotation,
            on_ground,
        };
        self.fire(id, Continuation::PlayerMove(step), |server| {
            server.move_payload(id, from, to, rotation)
        });
    }

    pub(crate) fn fire(
        &mut self,
        lane: impl Into<Lane>,
        then: Continuation,
        describe: impl FnOnce(&Self) -> Option<EventPayload>,
    ) {
        self.fire_as(lane.into(), None, then, describe);
    }

    pub(crate) fn fire_numbered(
        &mut self,
        lane: impl Into<Lane>,
        event: u64,
        then: Continuation,
        describe: impl FnOnce(&Self) -> Option<EventPayload>,
    ) {
        self.fire_as(lane.into(), Some(event), then, describe);
    }

    pub(crate) fn fire_deferred(&mut self, lane: impl Into<Lane>, then: Continuation) {
        let lane = lane.into();
        if self.decisions.busy(lane) {
            let event = self.next_event_id();
            self.decisions.defer(event, lane, then);
            return;
        }
        self.fire_head(lane, None, then);
    }

    pub(crate) fn fire_refused(
        &mut self,
        lane: impl Into<Lane>,
        then: Continuation,
        describe: impl FnOnce(&Self) -> Option<EventPayload>,
    ) {
        let lane = lane.into();
        let observed = self.plugins.listens(then.kind(), Access::Observe);
        if !observed || !self.decisions.busy(lane) {
            if observed && let Some(payload) = describe(self) {
                let event = self.next_event_id();
                self.notify_plugins(event, &payload, true, Phase::Settled);
            }
            self.run_refused(then);
            return;
        }
        let Some(payload) = describe(self) else {
            self.run_refused(then);
            return;
        };
        let event = self.next_event_id();
        let tick = self.current_tick();
        let until = tick.saturating_add(self.config.plugins.late_cancel_ticks);
        self.decisions.queue(
            event,
            Pending::new(lane, payload, Vec::new(), tick, until, then).refused(),
        );
    }

    fn fire_head(&mut self, lane: Lane, order: Option<u64>, then: Continuation) {
        let Some(then) = self.at_head(then) else {
            return;
        };
        let kind = then.kind();
        let listened = self.plugins.listens(kind, Access::Observe)
            || self.plugins.listens(kind, Access::Decide);
        let payload = if listened {
            self.head_payload(&then)
        } else {
            None
        };
        self.fire_as(lane, order, then, move |_| payload);
    }

    fn fire_as(
        &mut self,
        lane: Lane,
        order: Option<u64>,
        then: Continuation,
        describe: impl FnOnce(&Self) -> Option<EventPayload>,
    ) {
        let kind = then.kind();
        let observed = self.plugins.listens(kind, Access::Observe);
        let decided = self.plugins.listens(kind, Access::Decide);
        if !observed && !decided {
            self.run_now(then);
            return;
        }
        if then.may_skip()
            && decided
            && self
                .decisions
                .in_flight(lane, |queued| queued.same_as(&then))
        {
            return;
        }
        let deciding = self.deciding(&then);
        let busy = self.decisions.busy(lane);
        if !decided && !busy {
            if let Some(payload) = describe(self) {
                let event = order.unwrap_or_else(|| self.next_event_id());
                self.notify_plugins(event, &payload, false, Phase::Settled);
            }
            self.run_now(then);
            return;
        }
        let Some(payload) = describe(self) else {
            self.run_now(then);
            return;
        };
        let event = order.unwrap_or_else(|| self.next_event_id());
        let deciders = if decided {
            self.plugins
                .listeners(kind, Access::Decide, &self.config.plugins)
        } else {
            Vec::new()
        };
        let asked = if deciding {
            deciders
                .iter()
                .map(|(connection, spec)| Asked {
                    connection: *connection,
                    handler: spec.id,
                    kind: spec.event,
                })
                .collect()
        } else {
            Vec::new()
        };
        for (connection, handlers) in recipients(deciders, false, Phase::Decide) {
            self.plugins.send(
                connection,
                ToPlugin::Event {
                    id: event,
                    handlers,
                    phase: Phase::Decide,
                    cancelled: false,
                    payload: payload.clone(),
                },
            );
        }
        let arrival = self.current_tick();
        let settings = &self.config.plugins;
        let deadline = arrival
            .saturating_add(settings.decision_ticks)
            .saturating_add(1);
        let until = arrival.saturating_add(settings.late_cancel_ticks);
        let pending = Pending::new(lane, payload, asked, deadline, until, then);
        if deciding || busy {
            self.decisions.queue(event, pending);
        } else {
            self.settle(pending.settle(event));
        }
    }

    pub(crate) fn listened(&self, kind: EventKind) -> bool {
        self.plugins.listens(kind, Access::Observe) || self.plugins.listens(kind, Access::Decide)
    }

    fn run_refused(&mut self, then: Continuation) {
        let dropped = self.conclude(then, true, None, Vec::new());
        self.notify_undone(dropped, Phase::Settled);
    }

    fn run_now(&mut self, then: Continuation) {
        let dropped = self.conclude(then, false, None, Vec::new());
        self.notify_undone(dropped, Phase::Settled);
    }

    fn deciding(&self, then: &Continuation) -> bool {
        !matches!(then, Continuation::Notice(_))
            && self.plugins.listens(then.kind(), Access::Decide)
    }

    pub(crate) fn parks(&self, lane: Lane, then: &Continuation) -> bool {
        self.deciding(then) || self.decisions.busy(lane)
    }

    pub(crate) fn asks(&self, then: &Continuation) -> bool {
        self.deciding(then)
    }

    pub(crate) fn hold_or_apply(&mut self, id: PlayerId, held: Held) {
        if let Err(held) = self.decisions.hold(id, held) {
            self.apply_move(id, held.position, held.rotation, held.on_ground);
        }
    }

    pub(crate) fn tick_plugins(&mut self) {
        self.decisions.expire(self.current_tick());
        self.settle_events();
    }

    pub(crate) fn plugin_disconnected(&mut self, connection: u64) {
        let departed = self.plugins.disconnect(connection);
        self.forget_plugin_inventories(connection);
        if let Some(plugin) = departed {
            self.refresh_ping_hook();
            self.plugin_disabled(connection, plugin);
        }
        self.release_departed_plugins();
    }

    pub(crate) fn release_departed_plugins(&mut self) {
        let plugins = &self.plugins;
        self.decisions
            .release(|connection| plugins.is_current(connection));
        self.settle_events();
    }

    fn take_decision(&mut self, connection: u64, event: u64, verdicts: Vec<Verdict>) {
        let (taken, warnings) =
            self.decisions
                .decide(connection, event, verdicts, self.current_tick());
        for warning in warnings {
            self.plugins.send(connection, ToPlugin::Warning(warning));
        }
        match taken {
            Taken::Counted => self.settle_events(),
            Taken::Ignored => {}
            Taken::RolledBack(rollback) => {
                self.undo(rollback.lane, rollback.undo);
                self.notify_undone(rollback.undone, Phase::RolledBack);
                self.notify_undone(rollback.dropped, Phase::Settled);
            }
            Taken::TooLate(what) => {
                if self.plugins.issued(event) {
                    self.plugins.send(
                        connection,
                        ToPlugin::Warning(format!("{what} of event {event} arrived too late")),
                    );
                }
            }
        }
    }

    fn settle_events(&mut self) {
        let tick = self.current_tick();
        loop {
            if let Some(due) = self.decisions.next_due() {
                self.decisions.enter(due.lane);
                self.fire_head(due.lane, Some(due.event), due.then);
                self.decisions.leave();
                continue;
            }
            match self.decisions.next_settled(tick) {
                Some(ready) => self.settle(ready),
                None => break,
            }
        }
    }

    pub(crate) fn settle_lane(&mut self, lane: Lane, logins: bool) {
        let dropped = self.decisions.drop_pending(lane);
        self.notify_undone(dropped, Phase::Settled);
        while let Some(next) = self.decisions.flush(lane, logins) {
            self.settle_flushed(lane, next);
        }
    }

    pub(crate) fn settle_acting_on(&mut self, entities: &[EntityId]) {
        while let Some(lane) = self.decisions.lane_where(|lane, then| {
            entities
                .iter()
                .any(|id| lane == Lane::Entity(*id) || then.acts_on(*id))
        }) {
            let Some(next) = self.decisions.flush(lane, false) else {
                break;
            };
            self.settle_flushed(lane, next);
        }
    }

    fn settle_flushed(&mut self, lane: Lane, next: Flushed) {
        match next {
            Flushed::Ready(ready) => self.settle(*ready),
            Flushed::Due(due) => {
                self.decisions.enter(lane);
                self.fire_head(lane, Some(due.event), due.then);
                self.decisions.leave();
            }
        }
    }

    pub(crate) fn settle_every_lane(&mut self) {
        while let Some(lane) = self.decisions.first_lane() {
            self.settle_lane(lane, true);
        }
    }

    fn settle(&mut self, ready: Ready) {
        let Ready {
            event,
            lane,
            mut payload,
            settlement,
            then,
            held,
            mut record,
        } = ready;
        let (cancelled, patch) = match settlement {
            Settlement::Cancelled(written) => (true, written),
            Settlement::Applied(patch) => (false, patch),
        };
        if self.stale(&then) {
            self.decisions.remember(event, record);
            return;
        }
        if let Some(patch) = patch.clone() {
            patch.settle(&mut payload);
        }
        self.notify_plugins(event, &payload, cancelled, Phase::Settled);
        if !cancelled {
            record.applied = Some(payload);
        }
        self.decisions.remember(event, record);
        self.decisions.enter(lane);
        let dropped = self.conclude(then, cancelled, patch, held);
        self.decisions.leave();
        self.notify_undone(dropped, Phase::Settled);
    }

    fn conclude(
        &mut self,
        then: Continuation,
        cancelled: bool,
        patch: Option<EventPatch>,
        held: Vec<Held>,
    ) -> Undone {
        match then {
            Continuation::PlayerMove(step) if cancelled => {
                let dropped = self.decisions.drop_pending(step.player);
                self.send_back(step.player, step.from);
                dropped
            }
            Continuation::PlayerMove(step) => {
                self.finish_move(step, patch_of!(patch, PlayerMove), held)
            }
            Continuation::Notice(_) => Vec::new(),
            Continuation::PreLogin(reply) => {
                self.answer_pre_login(&reply, patch_of!(patch, AsyncPlayerPreLogin));
                Vec::new()
            }
            Continuation::Login(key) => {
                self.finish_login(key, patch_of!(patch, PlayerLogin));
                Vec::new()
            }
            Continuation::Join(id) => {
                self.announce_join(id, patch_of!(patch, PlayerJoin));
                Vec::new()
            }
            Continuation::Leave(step) => {
                self.announce_leave(step, patch_of!(patch, PlayerLeave));
                Vec::new()
            }
            Continuation::Kick(_) if cancelled => Vec::new(),
            Continuation::Kick(step) => {
                self.finish_kick(step, patch_of!(patch, PlayerKick));
                Vec::new()
            }
            Continuation::Settings(step) => {
                if let Some(settings) = step.settings {
                    self.apply_settings(step.player, settings);
                }
                Vec::new()
            }
            Continuation::Chat(..) | Continuation::Command(_) if cancelled => Vec::new(),
            Continuation::Chat(step, offered) => {
                self.finish_chat(step, offered, patch_of!(patch, AsyncPlayerChat));
                Vec::new()
            }
            Continuation::Command(step) => {
                self.finish_command(step, patch_of!(patch, PlayerCommandPreprocess));
                Vec::new()
            }
            Continuation::Teleport(step) => {
                self.finish_teleport(step, cancelled, patch_of!(patch, PlayerTeleport))
            }
            Continuation::Velocity(_) if cancelled => Vec::new(),
            Continuation::Velocity(step) => {
                self.finish_velocity(step, patch_of!(patch, PlayerVelocity));
                Vec::new()
            }
            Continuation::Respawn(step) => {
                self.finish_respawn(step, patch_of!(patch, PlayerRespawn));
                Vec::new()
            }
            Continuation::Death(step) => {
                self.finish_death(*step, patch_of!(patch, PlayerDeath));
                Vec::new()
            }
            Continuation::Input(step) => {
                self.finish_input(step);
                Vec::new()
            }
            Continuation::Sneak(_)
            | Continuation::Sprint(_)
            | Continuation::GameMode(_)
            | Continuation::Swing(_)
            | Continuation::Swap(_)
                if cancelled =>
            {
                Vec::new()
            }
            Continuation::Sneak(step) => {
                self.finish_sneak(step);
                Vec::new()
            }
            Continuation::Sprint(step) => {
                self.finish_sprint(step);
                Vec::new()
            }
            Continuation::Flight(step) => {
                self.finish_flight(step, cancelled);
                Vec::new()
            }
            Continuation::GameMode(step) => {
                self.finish_game_mode(step);
                Vec::new()
            }
            Continuation::HeldSlot(step) => {
                self.finish_held_slot(step, cancelled);
                Vec::new()
            }
            Continuation::Swing(step) => {
                self.finish_swing(step);
                Vec::new()
            }
            Continuation::Swap(step) => {
                self.finish_swap(*step, patch_of!(patch, PlayerSwapHandItems));
                Vec::new()
            }
            Continuation::UseItemOn(step) => {
                self.finish_use_item_on(*step, patch_of!(patch, PlayerInteract));
                Vec::new()
            }
            Continuation::UseItem(step) => {
                let patch = patch_of!(patch, PlayerInteract);
                let (_, item_ok) = settled_results(EventResult::Deny, &patch);
                self.finish_use_item(step, item_ok);
                Vec::new()
            }
            Continuation::Dig(step) => {
                self.finish_dig(step, patch_of!(patch, PlayerInteract));
                Vec::new()
            }
            Continuation::InteractEntity(step) => {
                self.finish_interact_entity(*step, cancelled);
                Vec::new()
            }
            Continuation::Drop(step) => {
                self.finish_drop(*step, cancelled, patch_of!(patch, PlayerDropItem));
                Vec::new()
            }
            Continuation::ItemDamage(step) => {
                self.finish_wear(*step, cancelled, patch_of!(patch, PlayerItemDamage));
                Vec::new()
            }
            Continuation::TakeBook(_) if cancelled => Vec::new(),
            Continuation::TakeBook(step) => {
                self.finish_take_book(*step);
                Vec::new()
            }
            Continuation::ExpCooldown(step) => {
                self.finish_cooldown(step, patch_of!(patch, PlayerExpCooldownChange));
                Vec::new()
            }
            Continuation::ExpChange(step) => {
                self.finish_exp_change(step, patch_of!(patch, PlayerExpChange));
                Vec::new()
            }
            Continuation::PickupArrow(_) | Continuation::Shear(_) if cancelled => Vec::new(),
            Continuation::PickupArrow(step) => {
                self.finish_arrow_pickup(step);
                Vec::new()
            }
            Continuation::Shear(step) => {
                self.finish_shear(*step);
                Vec::new()
            }
            Continuation::ClickBlock(_)
            | Continuation::DropSelected(_)
            | Continuation::Wear(_)
            | Continuation::SetWeather(_)
            | Continuation::Strike(_) => Vec::new(),
            Continuation::CommandSend(id, offered) => {
                self.finish_command_send(id, offered, patch_of!(patch, PlayerCommandSend));
                Vec::new()
            }
            Continuation::Damage(step) => {
                self.finish_damage(*step, cancelled, patch_of!(patch, BlockDamage));
                Vec::new()
            }
            Continuation::Break(step) => {
                self.finish_break(*step, cancelled, patch_of!(patch, BlockBreak));
                Vec::new()
            }
            Continuation::Finish(_) => Vec::new(),
            Continuation::Abort(step) => {
                self.finish_abort(&step);
                Vec::new()
            }
            Continuation::DropItems(step) => {
                self.finish_drop_items(*step, cancelled, patch_of!(patch, BlockDropItem));
                Vec::new()
            }
            Continuation::CanBuild(step) => {
                self.finish_can_build(*step, patch_of!(patch, BlockCanBuild));
                Vec::new()
            }
            Continuation::Place(step) => {
                let build = patch_of!(patch, BlockPlace).build;
                self.finish_place(*step, cancelled, build);
                Vec::new()
            }
            Continuation::MultiPlace(step) => {
                let build = patch_of!(patch, BlockMultiPlace).build;
                self.finish_place(*step, cancelled, build);
                Vec::new()
            }
            Continuation::Physics(step) => {
                if !cancelled {
                    self.finish_physics(step);
                }
                Vec::new()
            }
            Continuation::Cook(step) => {
                if !cancelled {
                    self.finish_cook(*step, patch_of!(patch, BlockCook));
                }
                Vec::new()
            }
            Continuation::CampfireStart(step) => {
                self.finish_campfire_start(*step, patch_of!(patch, CampfireStart));
                Vec::new()
            }
            Continuation::Exp(step) => {
                self.finish_block_exp(step, patch_of!(patch, BlockExp));
                Vec::new()
            }
            Continuation::Explode(step) => {
                self.finish_explosion(*step, cancelled, patch_of!(patch, BlockExplode));
                Vec::new()
            }
            Continuation::ChunkUnload(pos) => {
                self.finish_chunk_unload(pos, patch_of!(patch, ChunkUnload));
                Vec::new()
            }
            Continuation::TimeSkip(step) => {
                if !cancelled {
                    self.finish_time_skip(step, patch_of!(patch, TimeSkip));
                }
                Vec::new()
            }
            Continuation::Weather(step) => {
                self.finish_weather(step, cancelled);
                Vec::new()
            }
            Continuation::Thunder(to) => {
                self.finish_thunder(to, cancelled);
                Vec::new()
            }
            Continuation::Spawn(step) => {
                self.finish_spawn(*step, cancelled);
                Vec::new()
            }
            Continuation::EntityDrop(item) => {
                self.finish_entity_drop(*item, cancelled);
                Vec::new()
            }
            Continuation::Hurt(step) => {
                self.finish_hurt(*step, cancelled, settled_damage(patch));
                Vec::new()
            }
            Continuation::MobDeath(step) => {
                self.finish_mob_death(*step, patch_of!(patch, EntityDeath));
                Vec::new()
            }
            Continuation::Regain(step) => {
                self.finish_regain(step, cancelled, patch_of!(patch, EntityRegainHealth));
                Vec::new()
            }
            Continuation::MobRegain(step) => {
                self.finish_mob_regain(step, cancelled, patch_of!(patch, EntityRegainHealth));
                Vec::new()
            }
            Continuation::Exhaust(step) => {
                self.finish_exhaust(step, cancelled, patch_of!(patch, EntityExhaustion));
                Vec::new()
            }
            Continuation::Food(step) => {
                self.finish_food(step, cancelled, patch_of!(patch, FoodLevelChange));
                Vec::new()
            }
            Continuation::Potion(step) => {
                self.finish_potion(step, cancelled, patch_of!(patch, EntityPotionEffect));
                Vec::new()
            }
            Continuation::Combust(step) => {
                self.finish_combust(step, cancelled, settled_duration(patch));
                Vec::new()
            }
            Continuation::Knock(step) => {
                self.finish_knock(step, cancelled, settled_knockback(patch));
                Vec::new()
            }
            Continuation::ItemDespawn(id) => {
                self.finish_item_despawn(id, cancelled);
                Vec::new()
            }
            Continuation::PickupItem(step) => {
                self.finish_pickup(*step, cancelled);
                Vec::new()
            }
            Continuation::Gather(step) => {
                self.finish_gather(*step, cancelled);
                Vec::new()
            }
            Continuation::MobPickup(step) => {
                self.finish_mob_pickup(*step, cancelled);
                Vec::new()
            }
            Continuation::PiglinPickup(step) => {
                self.finish_piglin_pickup(*step, cancelled);
                Vec::new()
            }
            Continuation::Barter(step) => {
                self.finish_barter(*step, cancelled);
                Vec::new()
            }
            Continuation::ChangeBlock(step) => {
                self.finish_change(*step, cancelled);
                Vec::new()
            }
            Continuation::Form(step) => {
                if !cancelled {
                    self.finish_snow_trail(step);
                }
                Vec::new()
            }
            Continuation::Ignite(step) => {
                self.finish_ignite(step, cancelled);
                Vec::new()
            }
            Continuation::Regrow(sheep) => {
                if !cancelled {
                    self.regrow_wool(sheep);
                }
                Vec::new()
            }
            Continuation::EggBreak { mob, pos } => {
                if !cancelled {
                    self.break_turtle_egg(mob, pos);
                }
                Vec::new()
            }
            Continuation::Grow(step) => {
                self.finish_grow(step, cancelled);
                Vec::new()
            }
            Continuation::DoorOpen { mob, pos } => {
                if !cancelled {
                    self.finish_door_open(mob, pos);
                }
                Vec::new()
            }
            Continuation::EntityExplode(step) => {
                self.finish_entity_explosion(*step, cancelled, patch_of!(patch, EntityExplode));
                Vec::new()
            }
            Continuation::Splash(step) => {
                self.finish_splash(*step, cancelled, patch_of!(patch, PotionSplash));
                Vec::new()
            }
            Continuation::Prime(step) => {
                self.finish_prime(step, cancelled, patch_of!(patch, ExplosionPrime));
                Vec::new()
            }
            Continuation::Relocate(step) => {
                self.finish_relocate(step, cancelled, patch_of!(patch, EntityTeleport));
                Vec::new()
            }
            Continuation::Shoot(step) => {
                self.finish_shot(*step, cancelled, patch_of!(patch, EntityShootBow));
                Vec::new()
            }
            Continuation::ProjectileHit { arrow, target } => {
                self.finish_projectile_hit(arrow, target, cancelled);
                Vec::new()
            }
            Continuation::Aim(step) => {
                self.finish_aim(step, cancelled, settled_target(patch));
                Vec::new()
            }
            Continuation::Breed(step) => {
                if !cancelled {
                    self.finish_breed(*step, patch_of!(patch, EntityBreed));
                }
                Vec::new()
            }
            Continuation::Love(step) => {
                if !cancelled {
                    self.finish_love(*step, patch_of!(patch, EntityEnterLoveMode));
                }
                Vec::new()
            }
            Continuation::Tame(step) => {
                self.finish_tame(step, cancelled);
                Vec::new()
            }
            Continuation::PigAnger(step) => {
                self.finish_pig_zombie_anger(step, cancelled, patch_of!(patch, PigZombieAnger));
                Vec::new()
            }
            Continuation::Dye(step) => {
                self.finish_dye(*step, cancelled, patch_of!(patch, SheepDyeWool));
                Vec::new()
            }
            Continuation::Window(_) => Vec::new(),
            Continuation::Click(kind, step) => {
                self.finish_click(kind, *step, Writes::of(patch));
                Vec::new()
            }
            Continuation::Drag(step) => {
                self.finish_drag(*step, Writes::of(patch));
                Vec::new()
            }
            Continuation::Furnace(kind, step) => {
                let answer = match kind {
                    EventKind::FurnaceBurn => FurnaceAnswer::Burn(patch_of!(patch, FurnaceBurn)),
                    EventKind::FurnaceStartSmelt => {
                        FurnaceAnswer::Start(patch_of!(patch, FurnaceStartSmelt))
                    }
                    _ => FurnaceAnswer::Smelt(patch_of!(patch, FurnaceSmelt)),
                };
                self.finish_furnace(*step, cancelled, answer);
                Vec::new()
            }
            Continuation::Extract(step) => {
                self.finish_extract(step, patch_of!(patch, FurnaceExtract));
                Vec::new()
            }
            Continuation::Prepare(_, step) => {
                self.finish_prepare(*step, settled_result(patch));
                Vec::new()
            }
            Continuation::Open(step) => {
                self.finish_open(*step, cancelled);
                Vec::new()
            }
            Continuation::OpenPlugin(step) => {
                self.finish_plugin_open(step, cancelled);
                Vec::new()
            }
            Continuation::Close(_) | Continuation::CreativeSlot(_) | Continuation::Button(_) => {
                Vec::new()
            }
            Continuation::Creative(step) => {
                self.finish_creative(*step, Writes::of(patch));
                Vec::new()
            }
            Continuation::Ping(step) => {
                self.finish_ping(*step, patch_of!(patch, ServerListPing));
                Vec::new()
            }
            Continuation::ServerCommand(_) | Continuation::Broadcast(..) if cancelled => Vec::new(),
            Continuation::ServerCommand(line) => {
                self.finish_server_command(line, patch_of!(patch, ServerCommand));
                Vec::new()
            }
            Continuation::TabComplete(step) => {
                self.finish_tab_complete(*step, cancelled, patch_of!(patch, TabComplete));
                Vec::new()
            }
            Continuation::Broadcast(message, offered) => {
                self.finish_broadcast(&message, offered, patch_of!(patch, BroadcastMessage));
                Vec::new()
            }
        }
    }

    fn stale(&self, then: &Continuation) -> bool {
        match then {
            Continuation::ChunkUnload(pos) => !self.world.chunks.unload_due(*pos),
            _ => false,
        }
    }

    fn undo(&mut self, lane: Lane, undo: Undo) {
        match (lane, undo) {
            (Lane::Player(player), Undo::SendBack(position)) => self.send_back(player, position),
            (
                Lane::Login(_)
                | Lane::Block(_)
                | Lane::Chunk(_)
                | Lane::Entity(_)
                | Lane::World
                | Lane::Console
                | Lane::Broadcast
                | Lane::Ping,
                Undo::SendBack(_),
            ) => {}
        }
    }

    fn finish_move(&mut self, step: MoveStep, patch: PlayerMovePatch, held: Vec<Held>) -> Undone {
        if let Some(to) = patch.to.map(|to| to.position).filter(|to| *to != step.to) {
            let dropped = self.decisions.drop_pending(step.player);
            self.ask_teleport(
                step.player,
                to,
                None,
                PlayerTeleportCause::Plugin,
                Some(step),
            );
            return dropped;
        }
        self.apply_move(step.player, Some(step.to), step.rotation, step.on_ground);
        for packet in held {
            if self
                .players
                .get(&step.player)
                .is_none_or(|p| p.vitals.is_dead())
            {
                break;
            }
            self.apply_move(
                step.player,
                packet.position,
                packet.rotation,
                packet.on_ground,
            );
        }
        Vec::new()
    }

    pub(crate) fn notify_undone(&mut self, undone: Undone, phase: Phase) {
        for (event, payload) in undone {
            self.notify_plugins(event, &payload, true, phase);
        }
    }

    fn send_back(&mut self, id: PlayerId, position: Vec3) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if player.position != position {
            self.teleport_player(id, position);
            return;
        }
        let teleport = player.next_teleport_id();
        player.link.send(ClientEvent::Teleport {
            id: teleport,
            position,
            rotation: player.rotation,
        });
    }

    fn move_payload(
        &self,
        id: PlayerId,
        from: Vec3,
        to: Vec3,
        rotation: Option<Rotation>,
    ) -> Option<EventPayload> {
        self.snapshot_of(id).map(|mut player| {
            player.position = from;
            let from = Location {
                world: player.world.clone(),
                position: from,
                rotation: player.rotation,
            };
            let to = Location {
                position: to,
                rotation: rotation.unwrap_or(player.rotation),
                ..from.clone()
            };
            EventPayload::PlayerMove(PlayerMove {
                player,
                from,
                to: Some(to),
            })
        })
    }

    fn describe(&self, player: &Player<L>) -> PlayerSnapshot {
        PlayerSnapshot {
            id: player.id,
            name: player.profile.name.clone(),
            entity_id: player.entity_id.0,
            world: self.world.dimension.key.to_string(),
            position: player.position,
            rotation: player.rotation,
            game_mode: player.game_mode,
        }
    }

    fn state_of(&self, id: PlayerId) -> Option<PlayerState> {
        self.players.get(&id).map(|player| {
            let abilities = player.abilities();
            let vitals = &player.vitals;
            PlayerState {
                player: self.describe(player),
                health: vitals.health,
                max_health: crate::health::max_health(player),
                absorption: vitals.absorption,
                food: vitals.food.food,
                saturation: vitals.food.saturation,
                exhaustion: vitals.food.exhaustion,
                level: vitals.experience.level,
                exp: vitals.experience.progress,
                total_exp: vitals.experience.total,
                operator: player.operator,
                allow_flight: abilities.may_fly,
                flying: abilities.flying,
                fly_speed: abilities.fly_speed * SPEED_SCALE,
                walk_speed: abilities.walk_speed * SPEED_SCALE,
                sneaking: player.sneaking,
                sprinting: player.sprinting,
                on_ground: player.on_ground,
                fire_ticks: vitals.fire_ticks,
                dead: vitals.is_dead(),
                effects: player.effects.iter().filter_map(effect_spec).collect(),
                ping: player.latency,
                locale: player.settings.locale.clone(),
                address: player.address,
                hidden: self.visibility.hidden_from(id),
            }
        })
    }

    pub(crate) fn online_snapshots(&self) -> Vec<PlayerSnapshot> {
        let mut online: Vec<PlayerSnapshot> = self
            .players
            .values()
            .map(|player| self.describe(player))
            .collect();
        online.sort_by_key(|snapshot| snapshot.entity_id);
        online
    }
}

pub(crate) fn effect_spec(effect: &EffectInstance) -> Option<EffectSpec> {
    Some(EffectSpec {
        key: effect.effect.name()?.to_owned(),
        duration: effect.duration.wire_ticks(),
        amplifier: i32::from(effect.amplifier),
        ambient: effect.ambient,
        particles: effect.visible,
        icon: effect.show_icon,
    })
}

fn audience(
    listeners: Vec<(u64, HandlerSpec)>,
    payload: &EventPayload,
    cancelled: bool,
    phase: Phase,
) -> (bool, Vec<(u64, Vec<u32>)>) {
    let cancelled = cancelled || payload.cancelled();
    (cancelled, recipients(listeners, cancelled, phase))
}

fn recipients(
    listeners: Vec<(u64, HandlerSpec)>,
    cancelled: bool,
    phase: Phase,
) -> Vec<(u64, Vec<u32>)> {
    let skip_filtered = cancelled && phase == Phase::Settled;
    let mut grouped: Vec<(u64, Vec<u32>)> = Vec::new();
    for (connection, spec) in listeners {
        if skip_filtered && spec.ignore_cancelled {
            continue;
        }
        match grouped.iter_mut().find(|(known, _)| *known == connection) {
            Some((_, handlers)) => handlers.push(spec.id),
            None => grouped.push((connection, vec![spec.id])),
        }
    }
    grouped
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rspigot_domain::chunk::ChunkColumn;
    use rspigot_domain::player::GameMode;
    use rspigot_domain::text::Component;
    use rspigot_domain::world::Dimension;
    use rspigot_domain::{BiomeId, ChunkPos, Identifier};
    use rspigot_nbt::Compound;

    use super::*;
    use crate::config::{GameConfig, LevelData};
    use crate::events::Inbound;
    use crate::plugins::events::{Action, EventResult, Face, PlayerInteract, WorldSave};
    use crate::plugins::steps::WeatherStep;
    use crate::plugins::{EventKind, PluginIntro, Priority};
    use crate::ports::{ChunkGenerator, ChunkStore, EntityStore, PluginOutbox, StoreError};
    use crate::status::StatusBoard;

    struct Quiet;

    impl ClientLink for Quiet {
        fn send(&self, _: ClientEvent) {}

        fn disconnect(&self, _: Component) {}

        fn is_connected(&self) -> bool {
            true
        }
    }

    impl PluginOutbox for Quiet {
        fn send(&self, _: ToPlugin) {}
    }

    impl ChunkStore for Quiet {
        fn load(&self, _: &Identifier, _: ChunkPos) -> Result<Option<ChunkColumn>, StoreError> {
            Ok(None)
        }

        fn save(&self, _: &Identifier, _: &ChunkColumn) -> Result<(), StoreError> {
            Ok(())
        }

        fn flush(&self) -> Result<(), StoreError> {
            Ok(())
        }
    }

    impl EntityStore for Quiet {
        fn load_entities(
            &self,
            _: &Identifier,
            _: ChunkPos,
        ) -> Result<Option<Vec<Compound>>, StoreError> {
            Ok(None)
        }

        fn save_entities(
            &self,
            _: &Identifier,
            _: ChunkPos,
            _: &[Compound],
        ) -> Result<(), StoreError> {
            Ok(())
        }
    }

    impl ChunkGenerator for Quiet {
        fn generate(&self, dimension: &Dimension, pos: ChunkPos) -> ChunkColumn {
            ChunkColumn::empty(pos, dimension.height, BiomeId(0))
        }

        fn spawn_point(&self, _: &Dimension) -> Vec3 {
            Vec3::new(0.5, 64.0, 0.5)
        }
    }

    #[test]
    fn a_spawn_that_cannot_be_placed_forgets_its_initial_target() {
        let mut server = quiet_server();
        let zombie = rspigot_entities::EntityKind::Zombie;
        let first = crate::mob_actions::new_mob(server.new_entity_base(zombie));
        let uuid = first.base.uuid;
        assert!(server.place_entity(first).is_ok());
        let mut base = server.new_entity_base(zombie);
        base.uuid = uuid;
        let twin = crate::mob_actions::new_mob(base);
        let id = twin.base.id;
        server
            .entities
            .mobs
            .initial_targets
            .insert(id, rspigot_domain::EntityId(99));
        let step = crate::plugins::steps::SpawnStep {
            kind: EventKind::CreatureSpawn,
            entity: twin,
            loaded: false,
            birth: crate::plugins::steps::Birth::of(
                crate::plugins::events::CreatureSpawnReason::Custom,
            ),
        };
        server.finish_spawn(step, false);
        assert!(!server.entities.mobs.initial_targets.contains_key(&id));
    }

    fn quiet_server() -> Server<Quiet> {
        let config = GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 0,
            ..GameConfig::default()
        };
        Server::new(
            config,
            LevelData::default(),
            Arc::new(Quiet),
            Arc::new(Quiet),
            StatusBoard::new(1),
        )
    }

    fn observing(server: &mut Server<Quiet>, kind: EventKind) {
        server.handle(Inbound::PluginConnected {
            connection: 1,
            intro: PluginIntro {
                name: "watch".to_owned(),
                version: "1.0.0".to_owned(),
                handlers: vec![HandlerSpec {
                    id: 1,
                    event: kind,
                    access: Access::Observe,
                    priority: Priority::Normal,
                    ignore_cancelled: false,
                }],
            },
            outbox: Box::new(Quiet),
        });
    }

    fn busy_world(server: &mut Server<Quiet>) {
        let save = EventPayload::WorldSave(WorldSave {
            world: "minecraft:overworld".to_owned(),
        });
        let waiting = Asked {
            connection: 9,
            handler: 1,
            kind: EventKind::WorldSave,
        };
        server.decisions.queue(
            1,
            Pending::new(
                Lane::World,
                save,
                vec![waiting],
                u64::MAX,
                u64::MAX,
                Continuation::Notice(EventKind::WorldSave),
            ),
        );
        assert!(server.decisions.busy(Lane::World));
    }

    fn storm() -> Continuation {
        Continuation::Weather(WeatherStep {
            rain: Some(true),
            thunder: Some(true),
        })
    }

    #[test]
    fn a_refused_event_with_nothing_to_describe_still_runs_its_continuation() {
        let mut server = quiet_server();
        observing(&mut server, EventKind::WeatherChange);
        busy_world(&mut server);
        server.fire_refused(Lane::World, storm(), |_| None);
        assert!(!server.world.level.raining);
        assert!(server.world.level.thundering);
    }

    #[test]
    fn a_refused_event_nobody_listens_to_runs_at_once_on_a_busy_lane() {
        let mut server = quiet_server();
        busy_world(&mut server);
        server.fire_refused(Lane::World, storm(), |_| {
            panic!("an event nobody listens to is never described")
        });
        assert!(!server.world.level.raining);
        assert!(server.world.level.thundering);
    }

    fn listener(connection: u64, id: u32, ignore_cancelled: bool) -> (u64, HandlerSpec) {
        (
            connection,
            HandlerSpec {
                id,
                event: EventKind::PlayerMove,
                access: Access::Observe,
                priority: Priority::Normal,
                ignore_cancelled,
            },
        )
    }

    fn mixed() -> Vec<(u64, HandlerSpec)> {
        vec![
            listener(2, 4, false),
            listener(1, 1, true),
            listener(2, 5, true),
            listener(1, 2, false),
        ]
    }

    #[test]
    fn handlers_are_grouped_by_connection_in_the_order_they_were_listed() {
        assert_eq!(
            recipients(mixed(), false, Phase::Settled),
            [(2, vec![4, 5]), (1, vec![1, 2])]
        );
    }

    #[test]
    fn a_cancelled_event_skips_the_handlers_that_ignore_cancelled() {
        assert_eq!(
            recipients(mixed(), true, Phase::Settled),
            [(2, vec![4]), (1, vec![2])]
        );
    }

    #[test]
    fn a_rolled_back_event_still_reaches_the_handlers_that_ignore_cancelled() {
        assert_eq!(
            recipients(mixed(), true, Phase::RolledBack),
            [(2, vec![4, 5]), (1, vec![1, 2])]
        );
    }

    #[test]
    fn a_connection_whose_handlers_are_all_skipped_hears_nothing() {
        let only_filtered = vec![listener(1, 1, true), listener(3, 2, false)];
        assert_eq!(
            recipients(only_filtered, true, Phase::Settled),
            [(3, vec![2])]
        );
        assert!(recipients(Vec::new(), false, Phase::Settled).is_empty());
    }

    fn interaction(result: EventResult) -> EventPayload {
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
            player: PlayerSnapshot {
                id: PlayerId::offline("Alice"),
                name: "Alice".to_owned(),
                entity_id: 1,
                world: "minecraft:overworld".to_owned(),
                position: Vec3::new(0.0, 64.0, 0.0),
                rotation: Rotation::default(),
                game_mode: GameMode::Survival,
            },
            use_interacted_block: result,
            use_item_in_hand: EventResult::Default,
        })
    }

    #[test]
    fn a_denied_result_skips_the_handlers_that_ignore_cancelled() {
        assert_eq!(
            audience(
                mixed(),
                &interaction(EventResult::Deny),
                false,
                Phase::Settled
            ),
            (true, vec![(2, vec![4]), (1, vec![2])])
        );
        assert_eq!(
            audience(
                mixed(),
                &interaction(EventResult::Allow),
                false,
                Phase::Settled
            ),
            (false, vec![(2, vec![4, 5]), (1, vec![1, 2])])
        );
    }
}
