use rand::Rng;
use rspigot_blocks::BlockKind;
use rspigot_domain::entity::item::{THROWN_DROP_BELOW_EYES, thrown_velocity};
use rspigot_domain::entity::reward::{damage_value, max_damage, set_damage_value};
use rspigot_domain::item::{HOTBAR_START, ItemStack, OFFHAND_SLOT, same_item_same_components};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::{BlockPos, EntityId, Rotation, Vec3};

use crate::events::{Animation, ClientEvent, Direction, EntityStatus, EquipmentSlot, Hand};
use crate::plugins::decisions::Continuation;
use crate::plugins::events::{
    Action, EquipmentSlot as Slot, EventResult, Face, PlayerDropItem, PlayerDropItemPatch,
    PlayerInteract, PlayerInteractAtEntity, PlayerInteractPatch, PlayerItemBreak, PlayerItemDamage,
    PlayerItemDamagePatch, PlayerTakeLecternBook,
};
use crate::plugins::steps::{
    Aim, BookStep, ClickStep, DigStep, DropSource, DropStep, InteractStep, Launch, UseOnStep,
    UseStep, WearRequest, WearStep,
};
use crate::plugins::targets::{Punched, direction_of};
use crate::plugins::{Access, EventKind, EventPayload, LecternData, spec_of, stack_of};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const STANDING_EYES: f32 = 1.62;
const CROUCHING_EYES: f32 = 1.27;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UseMemo {
    pub(crate) pos: BlockPos,
    pub(crate) hand: Hand,
    pub(crate) item: Option<rspigot_domain::ItemId>,
    pub(crate) use_item: EventResult,
}

pub(crate) struct Click {
    pub(crate) action: Action,
    pub(crate) hand: Option<Hand>,
    pub(crate) stack: Option<ItemStack>,
    pub(crate) block: Option<(BlockPos, Direction)>,
    pub(crate) clicked_position: Option<Vec3>,
    pub(crate) use_interacted_block: EventResult,
}

const fn face_of(direction: Direction) -> Face {
    match direction {
        Direction::Down => Face::Down,
        Direction::Up => Face::Up,
        Direction::North => Face::North,
        Direction::South => Face::South,
        Direction::West => Face::West,
        Direction::East => Face::East,
    }
}

pub(crate) const fn slot_of(hand: Hand) -> Slot {
    match hand {
        Hand::Main => Slot::Hand,
        Hand::Off => Slot::OffHand,
    }
}

pub(crate) fn settled_results(initial: EventResult, patch: &PlayerInteractPatch) -> (bool, bool) {
    let block = patch.use_interacted_block.unwrap_or(initial);
    let item = patch.use_item_in_hand.unwrap_or(EventResult::Default);
    (block != EventResult::Deny, item != EventResult::Deny)
}

fn settled_drop(patch: PlayerDropItemPatch, dropped: ItemStack) -> Option<ItemStack> {
    match patch.item.as_ref().map(stack_of) {
        Some(Ok(stack)) => stack,
        Some(Err(_)) | None => Some(dropped),
    }
}

fn corner_offset(hit: Vec3, pos: BlockPos) -> Vec3 {
    hit - Vec3::new(f64::from(pos.x), f64::from(pos.y), f64::from(pos.z))
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn hand_slot_of(&self, id: PlayerId, hand: Hand) -> Option<usize> {
        let player = self.players.get(&id)?;
        Some(match hand {
            Hand::Main => HOTBAR_START + usize::from(player.inventory.selected_hotbar),
            Hand::Off => OFFHAND_SLOT,
        })
    }

    fn click_payload(&self, id: PlayerId, click: Click) -> Option<EventPayload> {
        let player = self.snapshot_of(id)?;
        let stack = click.stack.as_ref();
        let (clicked_block, block_face) = match click.block {
            Some((pos, face)) => (Some(pos), face_of(face)),
            None => (None, Face::South),
        };
        Some(EventPayload::PlayerInteract(PlayerInteract {
            action: click.action,
            block: clicked_block.is_some(),
            block_face,
            block_in_hand: stack.is_some_and(|stack| BlockKind::from_item(stack.item.0).is_some()),
            clicked_block,
            clicked_position: click.clicked_position,
            hand: click.hand.map(slot_of),
            has_item: stack.is_some(),
            item: stack.map(spec_of),
            material: stack
                .and_then(|stack| stack.item.kind())
                .map_or("minecraft:air", |kind| kind.name())
                .to_owned(),
            player,
            use_interacted_block: click.use_interacted_block,
            use_item_in_hand: EventResult::Default,
        }))
    }

    pub(crate) fn notice_click(&mut self, id: PlayerId, click: impl FnOnce(&Self) -> Click) {
        self.fire(
            id,
            Continuation::Notice(EventKind::PlayerInteract),
            move |server| server.click_payload(id, click(server)),
        );
    }

    pub(crate) fn click_block(&mut self, step: ClickStep) {
        let id = step.player;
        self.fire_deferred(id, Continuation::ClickBlock(Box::new(step)));
    }

    pub(crate) fn click_block_at_head(&mut self, step: ClickStep) -> Option<UseOnStep> {
        let id = step.player;
        let slot = self.hand_slot_of(id, step.hand)?;
        let player = self.players.get(&id)?;
        let game_mode = player.game_mode;
        let stack = player.inventory.get(slot).cloned();
        let Some(clicked) = self.world.chunks.block(step.pos) else {
            self.push(
                Audience::One(id),
                ClientEvent::BlockChangedAck(step.sequence),
            );
            return None;
        };
        if !self.within_reach(id, step.pos)
            || !self.world.dimension.height.contains(step.pos.y)
            || self.is_protected(id, step.pos)
        {
            self.refuse_block(id, step.pos, step.sequence);
            return None;
        }
        let block_denied =
            game_mode == GameMode::Spectator && !crate::containers::opens_menu(clicked);
        Some(UseOnStep {
            player: id,
            hand: step.hand,
            slot,
            pos: step.pos,
            face: step.face,
            cursor: step.cursor,
            sequence: step.sequence,
            stack,
            clicked,
            game_mode,
            block_denied,
        })
    }

    pub(crate) fn use_on_payload(&self, step: &UseOnStep) -> Option<EventPayload> {
        self.click_payload(
            step.player,
            Click {
                action: Action::RightClickBlock,
                hand: Some(step.hand),
                stack: step.stack.clone(),
                block: Some((step.pos, step.face)),
                clicked_position: Some(step.cursor),
                use_interacted_block: if step.block_denied {
                    EventResult::Deny
                } else {
                    EventResult::Allow
                },
            },
        )
    }

    pub(super) fn finish_use_item_on(&mut self, step: UseOnStep, patch: PlayerInteractPatch) {
        let initial = if step.block_denied {
            EventResult::Deny
        } else {
            EventResult::Allow
        };
        let (block_ok, item_ok) = settled_results(initial, &patch);
        let current = self.players.get(&step.player).map(|player| {
            (
                player.vitals.is_dead(),
                player.game_mode,
                player.inventory.get(step.slot).cloned(),
            )
        });
        let Some((dead, game_mode, stack)) = current else {
            return;
        };
        let unchanged = !dead
            && game_mode == step.game_mode
            && stack == step.stack
            && self.world.chunks.block(step.pos) == Some(step.clicked);
        if let Some(player) = self.players.get_mut(&step.player) {
            player.use_memo = Some(UseMemo {
                pos: step.pos,
                hand: step.hand,
                item: step.stack.as_ref().map(|stack| stack.item),
                use_item: patch.use_item_in_hand.unwrap_or(EventResult::Default),
            });
        }
        if !unchanged {
            self.refuse_click(step.player, step.pos, step.face, step.sequence);
            return;
        }
        self.use_item_on_now(&step, block_ok, item_ok);
    }

    pub(crate) fn refuse_click(
        &mut self,
        id: PlayerId,
        pos: BlockPos,
        face: Direction,
        sequence: i32,
    ) {
        let (dx, dy, dz) = face.offset();
        for at in [pos, pos.offset(dx, dy, dz)] {
            if let Some(state) = self.world.chunks.block(at) {
                self.push(
                    Audience::One(id),
                    ClientEvent::BlockUpdate { pos: at, state },
                );
            }
        }
        self.push(Audience::One(id), ClientEvent::BlockChangedAck(sequence));
        self.send_inventory(id);
    }

    pub(crate) fn use_item(&mut self, id: PlayerId, hand: Hand, sequence: i32, rotation: Rotation) {
        self.fire_deferred(
            id,
            Continuation::UseItem(UseStep {
                player: id,
                hand,
                sequence,
                rotation,
                aim: None,
            }),
        );
    }

    fn held_in(&self, id: PlayerId, hand: Hand) -> Option<ItemStack> {
        let slot = self.hand_slot_of(id, hand)?;
        self.players.get(&id)?.inventory.get(slot).cloned()
    }

    pub(crate) fn use_item_at_head(&mut self, step: UseStep) -> Option<UseStep> {
        let id = step.player;
        let Some(stack) = self.held_in(id, step.hand) else {
            self.push(
                Audience::One(id),
                ClientEvent::BlockChangedAck(step.sequence),
            );
            return None;
        };
        let hit = self.look_block(id, step.rotation);
        let memo = self
            .players
            .get_mut(&id)
            .and_then(|player| player.use_memo.take());
        if let (Some(hit), Some(memo)) = (hit, memo)
            && memo.pos == hit.pos
            && memo.hand == step.hand
            && memo.item == Some(stack.item)
        {
            self.finish_use_item(step, memo.use_item != EventResult::Deny);
            return None;
        }
        Some(UseStep {
            aim: hit.map(|hit| Aim {
                pos: hit.pos,
                face: direction_of(hit.face),
                clicked: corner_offset(hit.location, hit.pos),
            }),
            ..step
        })
    }

    pub(crate) fn use_payload(&self, step: &UseStep) -> Option<EventPayload> {
        let stack = self.held_in(step.player, step.hand);
        let click = match step.aim {
            Some(aim) => Click {
                action: Action::RightClickBlock,
                hand: Some(step.hand),
                stack,
                block: Some((aim.pos, aim.face)),
                clicked_position: Some(aim.clicked),
                use_interacted_block: EventResult::Deny,
            },
            None => Click {
                action: Action::RightClickAir,
                hand: Some(step.hand),
                stack,
                block: None,
                clicked_position: None,
                use_interacted_block: EventResult::Deny,
            },
        };
        self.click_payload(step.player, click)
    }

    pub(super) fn finish_use_item(&mut self, step: UseStep, item_ok: bool) {
        if self.players.contains_key(&step.player) {
            if item_ok
                && (self.use_mob_bucket(step.player, step.hand, step.rotation)
                    || self.use_steering_item(step.player, step.hand))
            {
            } else if item_ok {
                self.start_using_item(step.player, step.hand);
            } else {
                self.send_inventory(step.player);
            }
        }
        self.push(
            Audience::One(step.player),
            ClientEvent::BlockChangedAck(step.sequence),
        );
    }

    pub(crate) fn start_digging(
        &mut self,
        id: PlayerId,
        pos: BlockPos,
        face: Direction,
        sequence: i32,
    ) {
        let Some(game_mode) = self.players.get(&id).map(|player| player.game_mode) else {
            return;
        };
        let Some(clicked) = self.world.chunks.block(pos) else {
            self.push(Audience::One(id), ClientEvent::BlockChangedAck(sequence));
            return;
        };
        if !self.within_reach(id, pos) {
            self.refuse_block(id, pos, sequence);
            return;
        }
        let left_click = move |server: &Self| Click {
            action: Action::LeftClickBlock,
            hand: Some(Hand::Main),
            stack: server.held_in(id, Hand::Main),
            block: Some((pos, face)),
            clicked_position: None,
            use_interacted_block: EventResult::Allow,
        };
        if self.is_protected(id, pos) {
            self.notice_click(id, left_click);
            self.refuse_block(id, pos, sequence);
            return;
        }
        let step = DigStep {
            player: id,
            pos,
            sequence,
            clicked,
            game_mode,
        };
        self.fire(id, Continuation::Dig(step), move |server| {
            server.click_payload(id, left_click(server))
        });
    }

    pub(super) fn finish_dig(&mut self, step: DigStep, patch: PlayerInteractPatch) {
        let (block_ok, item_ok) = settled_results(EventResult::Allow, &patch);
        let Some(player) = self.players.get(&step.player) else {
            return;
        };
        let unchanged = player.game_mode == step.game_mode
            && self.world.chunks.block(step.pos) == Some(step.clicked);
        let survival = player.game_mode == GameMode::Survival;
        if !unchanged || !block_ok || !item_ok && survival {
            if block_ok && unchanged {
                self.ack_block(step.player, step.sequence);
            } else {
                self.refuse_block(step.player, step.pos, step.sequence);
            }
            return;
        }
        self.start_destroying(step.player, step.pos, step.sequence);
    }

    pub(crate) fn punch(&mut self, id: PlayerId, hand: Hand) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if player.vitals.is_dead() {
            return;
        }
        let kind = EventKind::PlayerInteract;
        if !self.plugins.listens(kind, Access::Observe)
            && !self.plugins.listens(kind, Access::Decide)
        {
            self.swing_arm(id, hand);
            return;
        }
        let game_mode = player.game_mode;
        let punched = match self.punch_target(id) {
            Punched::Nothing => Some((Action::LeftClickAir, None, None)),
            Punched::Block(hit) if game_mode == GameMode::Adventure => Some((
                Action::LeftClickBlock,
                Some((hit.pos, direction_of(hit.face))),
                Some(corner_offset(hit.location, hit.pos)),
            )),
            Punched::Block(_) | Punched::Entity => None,
        };
        if let Some((action, block, clicked_position)) = punched {
            self.notice_click(id, move |server| Click {
                action,
                hand: Some(Hand::Main),
                stack: server.held_in(id, Hand::Main),
                block,
                clicked_position,
                use_interacted_block: EventResult::Deny,
            });
        }
        self.swing_arm(id, hand);
    }

    pub(crate) fn ask_interact_entity(
        &mut self,
        id: PlayerId,
        target: EntityId,
        hand: Hand,
        location: Vec3,
    ) {
        let stack = self
            .hand_slot_of(id, hand)
            .and_then(|slot| self.players.get(&id)?.inventory.get(slot).cloned());
        let step = InteractStep {
            player: id,
            target,
            hand,
            stack,
        };
        self.fire(
            id,
            Continuation::InteractEntity(Box::new(step)),
            move |server| {
                Some(EventPayload::PlayerInteractAtEntity(
                    PlayerInteractAtEntity {
                        clicked_position: location,
                        hand: slot_of(hand),
                        player: server.snapshot_of(id)?,
                        right_clicked: server.entity_view(target)?,
                    },
                ))
            },
        );
    }

    pub(super) fn finish_interact_entity(&mut self, step: InteractStep, cancelled: bool) {
        let stack = self
            .hand_slot_of(step.player, step.hand)
            .and_then(|slot| self.players.get(&step.player)?.inventory.get(slot).cloned());
        let same_item =
            stack.as_ref().map(|stack| stack.item) == step.stack.as_ref().map(|stack| stack.item);
        if cancelled || !same_item || !self.may_interact_with(step.player, step.target) {
            self.refresh_hands(step.player);
            return;
        }
        self.interact_now(step.player, step.target, step.hand);
    }

    pub(crate) fn ask_drop(
        &mut self,
        id: PlayerId,
        stack: ItemStack,
        source: DropSource,
        thrower: bool,
        predicted: bool,
    ) {
        let Some(step) = self.drop_step(id, stack, source, thrower, predicted) else {
            return;
        };
        let dropped = step.stack.clone();
        self.fire(id, Continuation::Drop(Box::new(step)), move |server| {
            Some(EventPayload::PlayerDropItem(PlayerDropItem {
                item: spec_of(&dropped),
                player: server.snapshot_of(id)?,
            }))
        });
    }

    pub(crate) fn drop_step(
        &mut self,
        id: PlayerId,
        stack: ItemStack,
        source: DropSource,
        thrower: bool,
        predicted: bool,
    ) -> Option<DropStep> {
        let launch = self.launch_from(id)?;
        Some(DropStep {
            player: id,
            stack,
            source,
            launch,
            thrower,
            predicted,
        })
    }

    pub(crate) fn drop_payload_of(&self, step: &DropStep) -> Option<EventPayload> {
        Some(EventPayload::PlayerDropItem(PlayerDropItem {
            item: spec_of(&step.stack),
            player: self.snapshot_of(step.player)?,
        }))
    }

    fn launch_from(&mut self, id: PlayerId) -> Option<Launch> {
        let player = self.players.get(&id)?;
        let eyes = if player.sneaking {
            CROUCHING_EYES
        } else {
            STANDING_EYES
        };
        let position = Vec3::new(
            player.position.x,
            player.position.y + f64::from(eyes) - f64::from(THROWN_DROP_BELOW_EYES),
            player.position.z,
        );
        let rotation = player.rotation;
        let yaw = self.rng.random::<f32>() * 360.0;
        let rolls: [f32; 4] = std::array::from_fn(|_| self.rng.random::<f32>());
        let velocity = thrown_velocity(rotation.yaw, rotation.pitch, rolls);
        Some(Launch {
            position,
            velocity,
            yaw,
        })
    }

    pub(super) fn finish_drop(
        &mut self,
        step: DropStep,
        cancelled: bool,
        patch: PlayerDropItemPatch,
    ) {
        let id = step.player;
        let Some(player) = self.players.get_mut(&id) else {
            if !matches!(step.source, DropSource::Selected { .. })
                && let Some(thrown) = settled_drop(patch, step.stack)
            {
                self.throw(step.launch, thrown, step.thrower.then_some(id.0));
            }
            return;
        };
        if cancelled {
            match step.source {
                DropSource::Selected { .. } => self.send_inventory(id),
                DropSource::Creative => {}
                DropSource::Released => self.give_back(id, step.stack, step.thrower),
            }
            return;
        }
        if let DropSource::Selected { slot } = step.source {
            let held = player.inventory.get(slot).cloned();
            let still = held.as_ref().is_some_and(|held| {
                same_item_same_components(held, &step.stack) && held.count >= step.stack.count
            });
            if !still {
                self.send_inventory(id);
                return;
            }
            let before = Self::equipment_of(player);
            let mut left = held;
            rspigot_domain::item::split(&mut left, u32::from(step.stack.count.get()));
            player.inventory.set(slot, left.clone());
            player.menu.set_remote(slot, left);
            player.vitals.attack_ticker = 0;
            self.swing_for_drop(id, step.predicted);
            if let Some(thrown) = settled_drop(patch, step.stack) {
                self.throw(step.launch, thrown, step.thrower.then_some(id.0));
            }
            self.broadcast_equipment_changes(id, before);
            return;
        }
        self.swing_for_drop(id, step.predicted);
        if let Some(thrown) = settled_drop(patch, step.stack) {
            self.throw(step.launch, thrown, step.thrower.then_some(id.0));
        }
    }

    fn swing_for_drop(&mut self, id: PlayerId, predicted: bool) {
        let Some(entity) = self.players.get(&id).map(|player| player.entity_id) else {
            return;
        };
        let swing = ClientEvent::EntityAnimation {
            entity,
            animation: Animation::SwingMainArm,
        };
        if !predicted {
            self.push(Audience::One(id), swing.clone());
        }
        self.push(Audience::Trackers(entity), swing);
    }

    fn give_back(&mut self, id: PlayerId, stack: ItemStack, thrown: bool) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        let main = HOTBAR_START + usize::from(player.inventory.selected_hotbar);
        let held = player.inventory.get(main).cloned();
        match held {
            None if thrown => {
                player.inventory.set(main, Some(stack));
            }
            Some(mut held)
                if thrown
                    && stack.count.get() == 1
                    && same_item_same_components(&held, &stack)
                    && u32::from(held.count.get()) < rspigot_domain::item::max_stack(&held) =>
            {
                held.count = held.count.saturating_add(1);
                player.inventory.set(main, Some(held));
            }
            _ => {
                let mut rest = Some(stack);
                player.inventory.add(&mut rest, false);
            }
        }
        self.sync_inventory(id);
        self.broadcast_held_item(id);
    }

    pub(crate) fn wear(&mut self, id: PlayerId, hand: Hand, amount: i32) {
        let Some(slot) = self.hand_slot_of(id, hand) else {
            return;
        };
        let request = WearRequest {
            player: id,
            hand,
            slot,
            amount,
        };
        let kind = EventKind::PlayerItemDamage;
        if self.plugins.listens(kind, Access::Observe) || self.plugins.listens(kind, Access::Decide)
        {
            self.fire_deferred(id, Continuation::Wear(request));
        } else if let Some(step) = self.wear_at_head(&request) {
            self.finish_wear(step, false, PlayerItemDamagePatch::default());
        }
    }

    pub(crate) fn wear_at_head(&self, request: &WearRequest) -> Option<WearStep> {
        let player = self.players.get(&request.player)?;
        let stack = player.inventory.get(request.slot).cloned()?;
        stack.item.kind()?.max_damage()?;
        let damage = if player.abilities().instant_build || max_damage(&stack).is_none() {
            0
        } else {
            request.amount
        };
        Some(WearStep {
            player: request.player,
            hand: request.hand,
            slot: request.slot,
            stack,
            damage,
        })
    }

    pub(crate) fn wear_payload(&self, step: &WearStep) -> Option<EventPayload> {
        Some(EventPayload::PlayerItemDamage(PlayerItemDamage {
            damage: step.damage,
            item: spec_of(&step.stack),
            player: self.snapshot_of(step.player)?,
        }))
    }

    pub(crate) fn finish_wear(
        &mut self,
        step: WearStep,
        cancelled: bool,
        patch: PlayerItemDamagePatch,
    ) {
        let id = step.player;
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if player.inventory.get(step.slot) != Some(&step.stack) {
            return;
        }
        if cancelled {
            self.send_inventory(id);
            return;
        }
        let damage = patch.damage.unwrap_or(step.damage);
        if damage == 0 {
            if damage != step.damage {
                self.send_inventory(id);
            }
            return;
        }
        let before = Self::equipment_of(player);
        let mut stack = step.stack.clone();
        let worn = damage_value(&stack).saturating_add(damage);
        set_damage_value(&mut stack, worn);
        let broken = max_damage(&stack).is_some_and(|max| damage_value(&stack) >= max);
        let entity = player.entity_id;
        if broken {
            let broken_item = spec_of(&stack);
            self.fire(
                id,
                Continuation::Notice(EventKind::PlayerItemBreak),
                move |server| {
                    Some(EventPayload::PlayerItemBreak(PlayerItemBreak {
                        broken_item,
                        player: server.snapshot_of(id)?,
                    }))
                },
            );
        }
        if let Some(player) = self.players.get_mut(&id) {
            player
                .inventory
                .set(step.slot, if broken { None } else { Some(stack) });
        }
        if broken {
            let equipment = match step.hand {
                Hand::Main => EquipmentSlot::MainHand,
                Hand::Off => EquipmentSlot::OffHand,
            };
            let status = ClientEvent::EntityStatus {
                entity,
                status: EntityStatus::EquipmentBreak(equipment),
            };
            self.push(Audience::One(id), status.clone());
            self.push(Audience::Trackers(entity), status);
        }
        self.sync_inventory(id);
        self.broadcast_equipment_changes(id, before);
        if damage != step.damage {
            self.send_inventory(id);
        }
    }

    pub(crate) fn ask_take_book(
        &mut self,
        id: PlayerId,
        pos: BlockPos,
        window: i32,
        book: ItemStack,
    ) {
        let step = BookStep {
            player: id,
            pos,
            window,
            book,
        };
        self.fire(id, Continuation::TakeBook(Box::new(step)), move |server| {
            let book = server.lectern_book(pos).as_ref().map(spec_of);
            Some(EventPayload::PlayerTakeLecternBook(PlayerTakeLecternBook {
                book: book.clone(),
                lectern: LecternData {
                    pos,
                    book,
                    page: server.lectern_page(pos),
                },
                player: server.snapshot_of(id)?,
            }))
        });
    }

    pub(super) fn finish_take_book(&mut self, step: BookStep) {
        let open = self
            .players
            .get(&step.player)
            .and_then(|player| player.open_menu.as_ref())
            .is_some_and(|open| open.window == step.window && open.backing.pos == step.pos);
        if open && self.lectern_book(step.pos).as_ref() == Some(&step.book) {
            self.take_book_now(step.player, step.pos, step.book);
        }
    }
}
