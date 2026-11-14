use std::num::NonZeroU8;

use rspigot_domain::item::{HOTBAR_START, ItemStack};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::text::Component;
use rspigot_domain::{BlockPos, BlockStateId, ItemId, Rotation, Vec3};

use rspigot_blocks::{BlockKind, Face, PlaceContext, StateFlags, UseContext};

use crate::block_tools::ToolUse;
use crate::chests;
use crate::events::{
    ClientEvent, DigAction, Direction, EquipmentSlot, Hand, MovementAction, PlayerInput,
};
use crate::loot::{self, LootContext};
use crate::plugins::events::PlayerRespawnReason;
use crate::plugins::steps::{AbortStep, BuildStep, ClickStep, FinishStep, UseItem, UseOnStep};
use crate::plugins::{Continuation, Held, SuggestStep};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const OFFHAND_SLOT: usize = 45;
const MAX_REACH_SQUARED: f64 = 64.0;
const MAX_MOVE_SQUARED: f64 = 100.0 * 100.0;

impl<L: ClientLink> Server<L> {
    pub(crate) fn handle_input(&mut self, id: PlayerId, input: PlayerInput) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if player.vitals.dying && acts(&input) {
            self.refuse_while_dying(id, &input);
            return;
        }
        match input {
            PlayerInput::Move {
                position,
                rotation,
                on_ground,
            } => self.handle_move(id, position, rotation, on_ground),
            PlayerInput::TeleportConfirm(teleport) => {
                if let Some(player) = self.players.get_mut(&id)
                    && player.pending_teleport == Some(teleport)
                {
                    player.pending_teleport = None;
                }
            }
            PlayerInput::Chat(message) => self.handle_chat(id, &message),
            PlayerInput::Command(command) => self.run_player_command(id, &command),
            PlayerInput::Dig {
                action,
                pos,
                face,
                sequence,
            } => self.handle_dig(id, action, pos, face, sequence),
            PlayerInput::UseItemOn {
                hand,
                pos,
                face,
                cursor,
                sequence,
                ..
            } => self.handle_use_item_on(id, hand, pos, face, cursor, sequence),
            PlayerInput::UseItem {
                hand,
                sequence,
                rotation,
            } => self.use_item(id, hand, sequence, rotation),
            PlayerInput::HeldSlot(slot) => {
                if slot < 9 {
                    self.select_slot(id, slot);
                }
            }
            PlayerInput::CreativeSlot { slot, stack } => self.handle_creative_slot(id, slot, stack),
            PlayerInput::Swing(hand) => self.punch(id, hand),
            PlayerInput::Movement(action) => match action {
                MovementAction::StartSprinting => self.toggle_sprint(id, true),
                MovementAction::StopSprinting => self.toggle_sprint(id, false),
                _ => {}
            },
            PlayerInput::Input(state) => self.press(id, state),
            PlayerInput::Settings(settings) => self.change_settings(id, settings),
            PlayerInput::ChunkBatchReceived { chunks_per_tick } => {
                if let Some(player) = self.players.get_mut(&id) {
                    player.streamer.on_batch_received(chunks_per_tick);
                }
            }
            PlayerInput::Loaded => {
                if let Some(player) = self.players.get_mut(&id) {
                    player.loaded = true;
                    player.vitals.load_timeout = 0;
                }
            }
            PlayerInput::Abilities { flying } => self.toggle_flight(id, flying),
            PlayerInput::ChangeGameMode(mode) => {
                if self.players.get(&id).is_some_and(|p| p.operator) {
                    self.change_game_mode(id, mode, Vec::new());
                }
            }
            PlayerInput::PickBlock { pos, .. } => self.handle_pick_block(id, pos),
            PlayerInput::CommandSuggestion { id: request, text } => {
                self.handle_suggestion(id, request, &text)
            }
            PlayerInput::Respawn => self.respawn(id, PlayerRespawnReason::Death),
            PlayerInput::ClientTickEnd => {
                if let Some(player) = self.players.get_mut(&id) {
                    if !player.moved_this_tick {
                        player.known_movement = Vec3::ZERO;
                    }
                    player.moved_this_tick = false;
                }
            }
            PlayerInput::Attack(target) => self.handle_attack(id, target),
            PlayerInput::Interact {
                target,
                hand,
                sneaking,
                location,
            } => self.handle_interact(id, target, hand, sneaking, location),
            PlayerInput::ContainerClick(click) => self.handle_container_click(id, click),
            PlayerInput::ContainerClose(window) => self.handle_container_close(id, window),
            PlayerInput::ContainerButtonClick { window, button } => {
                self.press_button(id, window, button);
            }
            PlayerInput::SetBeacon { primary, secondary } => {
                self.handle_set_beacon(id, primary, secondary);
            }
            PlayerInput::Latency(millis) => {
                if let Some(player) = self.players.get_mut(&id) {
                    player.latency = player.latency.wrapping_mul(3).wrapping_add(millis) / 4;
                }
            }
        }
    }

    fn refuse_while_dying(&mut self, id: PlayerId, input: &PlayerInput) {
        match input {
            PlayerInput::Dig { pos, sequence, .. }
            | PlayerInput::UseItemOn { pos, sequence, .. } => {
                self.refuse_block(id, *pos, *sequence);
            }
            PlayerInput::UseItem { sequence, .. } => {
                self.push(Audience::One(id), ClientEvent::BlockChangedAck(*sequence));
            }
            PlayerInput::ContainerClick(_) | PlayerInput::CreativeSlot { .. } => {
                self.send_inventory(id);
            }
            _ => {}
        }
    }

    fn handle_move(
        &mut self,
        id: PlayerId,
        position: Option<Vec3>,
        rotation: Option<Rotation>,
        on_ground: bool,
    ) {
        let reference = self.decisions.reference(id);
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if player.pending_teleport.is_some() || player.vitals.is_dead() {
            return;
        }
        let from = reference.unwrap_or(player.position);
        if let Some(target) = position {
            if !target.is_finite()
                || target.x.abs() > 3.0e7
                || target.z.abs() > 3.0e7
                || target.y.abs() > 2.0e7
            {
                self.disconnect_player(
                    id,
                    Component::translatable(
                        "multiplayer.disconnect.invalid_player_movement",
                        Vec::new(),
                    ),
                );
                return;
            }
            if from.distance_squared(target) > MAX_MOVE_SQUARED {
                tracing::warn!(
                    "{} moved too quickly! {:.2},{:.2},{:.2}",
                    player.profile.name,
                    target.x - from.x,
                    target.y - from.y,
                    target.z - from.z
                );
                let teleport = player.next_teleport_id();
                player.link.send(ClientEvent::Teleport {
                    id: teleport,
                    position: from,
                    rotation: player.rotation,
                });
                return;
            }
        }
        match position {
            Some(to) if to != from => self.decide_move(id, from, to, rotation, on_ground),
            _ => self.hold_or_apply(
                id,
                Held {
                    position,
                    rotation,
                    on_ground,
                },
            ),
        }
    }

    pub(crate) fn apply_move(
        &mut self,
        id: PlayerId,
        position: Option<Vec3>,
        rotation: Option<Rotation>,
        on_ground: bool,
    ) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let from = player.position;
        if let Some(target) = position {
            player.known_movement = target - from;
            player.moved_this_tick = true;
            player.position = target;
        }
        if let Some(rotation) = rotation {
            player.rotation = Rotation::new(
                rotation.yaw.rem_euclid(360.0),
                rotation.pitch.clamp(-90.0, 90.0),
            );
        }
        player.on_ground = on_ground;
        let chunk = player.position.chunk();
        if chunk != player.streamer.center() {
            player
                .streamer
                .move_to(chunk, &mut self.world.chunks, &self.workers, &player.link);
        }
        let entity = player.entity_id;
        let to = player.position;
        let yaw = player.rotation.yaw;
        self.check_fall(id, to.y - from.y, on_ground);
        if position.is_some() || rotation.is_some() {
            self.push(
                Audience::Trackers(entity),
                ClientEvent::EntityMoved {
                    entity,
                    from,
                    to,
                    rotation: rotation.map(|_| Rotation::new(yaw, to_pitch(rotation))),
                    on_ground,
                },
            );
        }
        if rotation.is_some() {
            self.push(
                Audience::Trackers(entity),
                ClientEvent::EntityHeadRotation { entity, yaw },
            );
        }
        self.update_player_view(id);
    }

    fn handle_chat(&mut self, id: PlayerId, message: &str) {
        if !self.players.contains_key(&id) {
            return;
        }
        if message.chars().count() > 256 || message.chars().any(|c| c == '§' || c.is_control()) {
            self.disconnect_player(
                id,
                Component::translatable("multiplayer.disconnect.illegal_characters", Vec::new()),
            );
            return;
        }
        self.broadcast_chat(id, message);
    }

    pub(crate) fn within_reach(&self, id: PlayerId, pos: BlockPos) -> bool {
        self.players.get(&id).is_some_and(|p| {
            p.eye_position()
                .distance_squared(pos.center() + Vec3::new(0.0, 0.5, 0.0))
                <= MAX_REACH_SQUARED
        })
    }

    pub(crate) fn is_protected(&self, id: PlayerId, pos: BlockPos) -> bool {
        let radius = self.config.spawn_protection as i32;
        if radius == 0
            || self.players.get(&id).is_some_and(|p| p.operator)
            || self.players.len() <= 1 && self.admin.operators.is_empty()
        {
            return false;
        }
        let spawn = self.world.spawn.block();
        (pos.x - spawn.x).abs() <= radius && (pos.z - spawn.z).abs() <= radius
    }

    pub(crate) fn refuse_block(&mut self, id: PlayerId, pos: BlockPos, sequence: i32) {
        if let Some(state) = self.world.chunks.block(pos) {
            self.push(Audience::One(id), ClientEvent::BlockUpdate { pos, state });
        }
        self.ack_block(id, sequence);
    }

    fn handle_dig(
        &mut self,
        id: PlayerId,
        action: DigAction,
        pos: BlockPos,
        face: Direction,
        sequence: i32,
    ) {
        if !self.players.contains_key(&id) {
            return;
        }
        match action {
            DigAction::Start => self.start_digging(id, pos, face, sequence),
            DigAction::Finish => self.fire_deferred(
                id,
                Continuation::Finish(FinishStep {
                    player: id,
                    pos,
                    sequence,
                }),
            ),
            DigAction::Cancel => self.fire_deferred(
                id,
                Continuation::Abort(AbortStep {
                    player: id,
                    pos,
                    sequence,
                }),
            ),
            DigAction::SwapHands => self.swap_hands(id),
            DigAction::DropItem => self.handle_drop_action(id, false),
            DigAction::DropStack => self.handle_drop_action(id, true),
            DigAction::ReleaseUseItem => self.release_use_item(id),
            DigAction::Stab => self.send_inventory(id),
        }
    }

    pub(crate) fn break_block_naturally(&mut self, pos: BlockPos) -> bool {
        let Some(state) = self.world.chunks.block(pos).filter(|state| !state.is_air()) else {
            return false;
        };
        self.pop_block_loot(pos, &LootContext::block(state, None, None));
        self.spawn_after_break(pos, state, None);
        self.set_block(pos, BlockStateId::AIR)
    }

    fn pop_block_loot(&mut self, pos: BlockPos, context: &LootContext<'_>) {
        if !self.config.rules.block_drops {
            return;
        }
        for stack in loot::block_drops(context, &mut self.rng) {
            self.spawn_item(pos, stack);
        }
    }

    pub(crate) fn set_block(&mut self, pos: BlockPos, state: BlockStateId) -> bool {
        let before = self.world.chunks.block(pos);
        let lost = before
            .map(|old| self.contents_lost_by(pos, old, state))
            .unwrap_or_default();
        let book = before.and_then(|old| self.lectern_book_lost_by(pos, old, state));
        let owed = match before {
            Some(old) => self.furnace_experience_lost_by(pos, old, state),
            None => Vec::new(),
        };
        match self.world.chunks.set_block(pos, state) {
            Some(old) if old != state => {
                self.drop_contents(pos, lost);
                self.pop_furnace_experience(pos, owed);
                if let Some((at, book)) = book {
                    self.pop_book(at, book);
                }
                self.mob_paths_block_changed(pos, old, state);
                self.on_place(pos);
                self.ask_physics(pos, state);
                self.push(
                    Audience::ChunkViewers(pos.chunk()),
                    ClientEvent::BlockUpdate { pos, state },
                );
                self.fix_chest_pairs(pos, old, state);
                self.ensure_container_entity(pos, state);
                true
            }
            _ => false,
        }
    }

    fn handle_use_item_on(
        &mut self,
        id: PlayerId,
        hand: Hand,
        pos: BlockPos,
        face: Direction,
        cursor: Vec3,
        sequence: i32,
    ) {
        self.click_block(ClickStep {
            player: id,
            hand,
            pos,
            face,
            cursor,
            sequence,
        });
    }

    pub(crate) fn use_item_on_now(&mut self, step: &UseOnStep, block_ok: bool, item_ok: bool) {
        let (id, hand, slot, pos, face, cursor, sequence) = (
            step.player,
            step.hand,
            step.slot,
            step.pos,
            step.face,
            step.cursor,
            step.sequence,
        );
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let sneaking = player.sneaking;
        let game_mode = player.game_mode;
        let rotation = player.rotation;
        let stack = player.inventory.get(slot).cloned();
        let clicked = step.clicked;
        if block_ok {
            if !sneaking && self.use_block(pos, clicked, sneaking) {
                self.push(Audience::One(id), ClientEvent::BlockChangedAck(sequence));
                return;
            }
            let holding = stack.is_some()
                || self
                    .players
                    .get(&id)
                    .is_some_and(|player| player.inventory.get(OFFHAND_SLOT).is_some());
            if !(sneaking && holding) && self.place_book_on_lectern(id, slot, pos, clicked) {
                self.push(Audience::One(id), ClientEvent::BlockChangedAck(sequence));
                return;
            }
            if !(sneaking && holding)
                && self.place_food_on_campfire(id, hand, slot, pos, clicked, sequence)
            {
                return;
            }
            if !(sneaking && holding) && self.open_block_menu(id, pos, clicked) {
                self.push(Audience::One(id), ClientEvent::BlockChangedAck(sequence));
                return;
            }
        } else {
            self.resync_denied_block(id, pos, face, clicked, stack.as_ref());
        }
        if !item_ok {
            self.refuse_click(id, pos, face, sequence);
            return;
        }
        let usage = ToolUse {
            hand,
            pos,
            face,
            clicked,
            sneaking,
            game_mode,
        };
        if let Some(held) = &stack
            && self.use_tool_on_block(id, held, &usage, slot, sequence)
        {
            return;
        }
        if let Some(held) = &stack
            && !matches!(game_mode, GameMode::Adventure | GameMode::Spectator)
            && self.spawn_from_egg_on_block(id, slot, held, pos, face, clicked)
        {
            self.push(Audience::One(id), ClientEvent::BlockChangedAck(sequence));
            return;
        }
        let kind = stack.as_ref().and_then(|s| BlockKind::from_item(s.item.0));
        let (Some(stack), Some(kind)) = (stack, kind) else {
            self.push(Audience::One(id), ClientEvent::BlockChangedAck(sequence));
            return;
        };
        if game_mode == GameMode::Spectator || game_mode == GameMode::Adventure {
            self.refuse_block(id, pos, sequence);
            return;
        }
        let (dx, dy, dz) = face.offset();
        let target = if is_replaceable(clicked) {
            pos
        } else {
            pos.offset(dx, dy, dz)
        };
        let target_ok = self.world.chunks.block(target).is_some_and(is_replaceable)
            && self.world.dimension.height.contains(target.y);
        let Some(old) = self.world.chunks.block(target).filter(|_| target_ok) else {
            self.refuse_block(id, target, sequence);
            return;
        };
        if !self.within_reach(id, target) || self.is_protected(id, target) {
            self.refuse_block(id, target, sequence);
            return;
        }
        let context = PlaceContext {
            default_state: kind.default_state(),
            wall_variant: kind.wall_variant().map(BlockKind::default_state),
            face: block_face(face),
            yaw: rotation.yaw,
            pitch: rotation.pitch,
            cursor_y: cursor.y as f32,
        };
        let Some(placement) = kind.place(&context) else {
            self.refuse_block(id, target, sequence);
            return;
        };
        let Some(state) = self.placement_state(target, placement.state, rotation.pitch, sneaking)
        else {
            self.refuse_block(id, target, sequence);
            return;
        };
        let chunks = &self.world.chunks;
        let state = chests::placed_state(state, target, face, sneaking, |at| chunks.block(at));
        let linked = placement
            .linked
            .map(|linked| (target.offset(0, i32::from(linked.dy), 0), linked.state));
        let buildable = !self.intersects_player(target);
        self.ask_can_build(BuildStep {
            using: UseItem {
                player: id,
                hand,
                slot,
                stack,
                against: pos,
                sequence,
                game_mode,
            },
            target,
            old,
            state,
            linked,
            buildable,
        });
    }

    fn resync_denied_block(
        &mut self,
        id: PlayerId,
        pos: BlockPos,
        face: Direction,
        clicked: BlockStateId,
        stack: Option<&ItemStack>,
    ) {
        let (dx, dy, dz) = face.offset();
        let halves: Vec<BlockPos> = match clicked.property("half") {
            Some("lower") if is_door(clicked) => vec![pos.offset(0, 1, 0)],
            Some("upper") if is_door(clicked) => vec![pos.offset(0, -1, 0)],
            _ if stack.is_some_and(places_two_blocks) => {
                vec![pos.offset(dx, dy + 1, dz), pos.offset(0, 1, 0)]
            }
            _ => Vec::new(),
        };
        for at in std::iter::once(pos).chain(halves) {
            if let Some(state) = self.world.chunks.block(at) {
                self.push(
                    Audience::One(id),
                    ClientEvent::BlockUpdate { pos: at, state },
                );
            }
        }
        self.send_inventory(id);
    }

    fn use_block(&mut self, pos: BlockPos, state: BlockStateId, sneaking: bool) -> bool {
        let Some(kind) = state.kind() else {
            return false;
        };
        let Some(result) = kind.use_block(&UseContext { state, sneaking }) else {
            return false;
        };
        self.set_block(pos, result.state);
        if let Some(linked) = result.linked {
            let other = pos.offset(0, i32::from(linked.dy), 0);
            if let Some(other_state) = self.world.chunks.block(other)
                && other_state.kind() == Some(kind)
                && let Some(changed) = other_state.with_property(linked.property, linked.value)
            {
                self.set_block(other, changed);
            }
        }
        true
    }

    pub(crate) fn intersects_player(&self, pos: BlockPos) -> bool {
        self.players.values().any(|p| {
            p.game_mode != GameMode::Spectator && {
                let feet = p.position.block();
                pos.x == feet.x && pos.z == feet.z && (pos.y == feet.y || pos.y == feet.y + 1)
            }
        })
    }

    pub(crate) fn consume_one(&mut self, id: PlayerId, slot: usize, stack: ItemStack) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let remaining = stack.count.get() - 1;
        let new = NonZeroU8::new(remaining).map(|count| ItemStack { count, ..stack });
        player.inventory.set(slot, new);
        self.sync_inventory(id);
        self.broadcast_held_item(id);
    }

    fn handle_pick_block(&mut self, id: PlayerId, pos: BlockPos) {
        let Some(state) = self.world.chunks.block(pos) else {
            return;
        };
        let Some(item) = state.kind().and_then(BlockKind::item) else {
            return;
        };
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if player.game_mode != GameMode::Creative {
            return;
        }
        let slot = HOTBAR_START + usize::from(player.inventory.selected_hotbar);
        player
            .inventory
            .set(slot, Some(ItemStack::new(ItemId(item), NonZeroU8::MIN)));
        self.sync_inventory(id);
        self.broadcast_held_item(id);
    }

    fn handle_suggestion(&mut self, id: PlayerId, request: i32, text: &str) {
        let input = text.trim_start_matches('/');
        let offset = (text.len() - input.len()) as i32;
        let (start, prefix, candidates): (i32, &str, Vec<String>) = match input.rsplit_once(' ') {
            None => {
                let operator = self.players.get(&id).is_some_and(|p| p.operator);
                let tree = self.command_tree(operator);
                let names = tree.nodes[tree.root]
                    .children
                    .iter()
                    .filter_map(|c| match &tree.nodes[*c].kind {
                        crate::commands::CommandNodeKind::Literal(name) => Some(name.clone()),
                        _ => None,
                    })
                    .collect();
                (offset, input, names)
            }
            Some((head, last)) => (
                offset + head.len() as i32 + 1,
                last,
                self.players
                    .values()
                    .map(|p| p.profile.name.clone())
                    .collect(),
            ),
        };
        let lower = prefix.to_ascii_lowercase();
        let mut matches: Vec<String> = candidates
            .into_iter()
            .filter(|c| c.to_ascii_lowercase().starts_with(&lower))
            .collect();
        matches.sort();
        let step = SuggestStep {
            player: id,
            request,
            start,
            length: prefix.len() as i32,
            matches,
        };
        self.ask_tab_complete(text, step);
    }

    pub(crate) fn send_inventory(&mut self, id: PlayerId) {
        if let Some(player) = self.players.get_mut(&id) {
            let (slots, carried) = player.menu.sync_all(&player.inventory);
            let state_id = player.next_inventory_state();
            player.link.send(ClientEvent::InventoryContent {
                state_id,
                slots,
                carried,
            });
        }
    }

    pub(crate) fn broadcast_held_item(&mut self, id: PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let entity = player.entity_id;
        let main = player.inventory.held().cloned();
        let off = player.inventory.get(OFFHAND_SLOT).cloned();
        self.push(
            Audience::Trackers(entity),
            ClientEvent::EntityEquipment {
                entity,
                slots: vec![
                    (EquipmentSlot::MainHand, main),
                    (EquipmentSlot::OffHand, off),
                ],
            },
        );
    }

    pub(crate) fn broadcast_metadata(&mut self, id: PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        let entity = player.entity_id;
        let entries = Self::metadata(player);
        self.push(
            Audience::Trackers(entity),
            ClientEvent::EntityMetadata { entity, entries },
        );
    }

    pub(crate) fn set_game_mode_now(&mut self, id: PlayerId, mode: GameMode) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if player.game_mode == mode {
            return;
        }
        player.previous_game_mode = Some(player.game_mode);
        player.game_mode = mode;
        player.allow_flight = None;
        if !mode.can_fly() {
            player.flying = false;
        }
        player.link.send(ClientEvent::GameEvent(
            crate::events::GameEvent::ChangeGameMode(mode),
        ));
        player.link.send(ClientEvent::Abilities(player.abilities()));
        self.push(
            Audience::All,
            ClientEvent::PlayerInfoGameMode {
                player: id,
                game_mode: mode,
            },
        );
    }
}

const fn acts(input: &PlayerInput) -> bool {
    matches!(
        input,
        PlayerInput::Dig { .. }
            | PlayerInput::UseItemOn { .. }
            | PlayerInput::UseItem { .. }
            | PlayerInput::CreativeSlot { .. }
            | PlayerInput::PickBlock { .. }
            | PlayerInput::Attack(_)
            | PlayerInput::Interact { .. }
            | PlayerInput::ContainerClick(_)
            | PlayerInput::ContainerClose(_)
            | PlayerInput::ContainerButtonClick { .. }
            | PlayerInput::SetBeacon { .. }
    )
}

fn to_pitch(rotation: Option<Rotation>) -> f32 {
    rotation.map_or(0.0, |r| r.pitch.clamp(-90.0, 90.0))
}

fn is_door(state: BlockStateId) -> bool {
    state
        .kind()
        .is_some_and(|kind| kind.name().ends_with("_door"))
}

fn places_two_blocks(stack: &ItemStack) -> bool {
    BlockKind::from_item(stack.item.0).is_some_and(|kind| {
        let context = PlaceContext {
            default_state: kind.default_state(),
            wall_variant: None,
            face: Face::Up,
            yaw: 0.0,
            pitch: 0.0,
            cursor_y: 0.0,
        };
        kind.place(&context)
            .is_some_and(|placement| placement.linked.is_some())
    })
}

pub(crate) fn is_replaceable(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::REPLACEABLE)
}

const fn block_face(face: Direction) -> Face {
    match face {
        Direction::Down => Face::Down,
        Direction::Up => Face::Up,
        Direction::North => Face::North,
        Direction::South => Face::South,
        Direction::West => Face::West,
        Direction::East => Face::East,
    }
}
