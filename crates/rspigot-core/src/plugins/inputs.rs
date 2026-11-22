use rspigot_domain::item::{HOTBAR_START, OFFHAND_SLOT};
use rspigot_domain::player::{GameMode, PlayerId};
use rspigot_domain::text::Component;

use crate::commands::{CommandContext, CommandSender};
use crate::events::{Animation, ClientEvent, Hand, InputState};
use crate::plugins::decisions::Continuation;
use crate::plugins::events::{
    EntityExhaustionReason, PlayerAnimation, PlayerAnimationType, PlayerGameModeChange,
    PlayerHideEntity, PlayerInput as Pressed, PlayerItemHeld, PlayerShowEntity,
    PlayerSwapHandItems, PlayerSwapHandItemsPatch, PlayerToggleFlight, PlayerToggleSneak,
    PlayerToggleSprint,
};
use crate::plugins::steps::{GameModeStep, HeldStep, InputStep, SwapStep, SwingStep, ToggleStep};
use crate::plugins::{EntityView, EventKind, EventPayload, Input, spec_of, stack_of};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const fn pressed(state: InputState) -> Input {
    Input {
        forward: state.forward,
        backward: state.backward,
        left: state.left,
        right: state.right,
        jump: state.jump,
        sneak: state.shift,
        sprint: state.sprint,
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn press(&mut self, id: PlayerId, state: InputState) {
        self.fire_deferred(id, Continuation::Input(InputStep { player: id, state }));
        self.fire_deferred(
            id,
            Continuation::Sneak(ToggleStep {
                player: id,
                value: state.shift,
            }),
        );
    }

    pub(crate) fn at_head(&mut self, then: Continuation) -> Option<Continuation> {
        match then {
            Continuation::Strike(step) => {
                self.strike_at_head(*step);
                None
            }
            Continuation::SetWeather(sky) => self.weather_at_head(sky),
            Continuation::Finish(step) => {
                self.finish_digging(step);
                None
            }
            Continuation::Abort(step) => {
                self.finish_aborting(step);
                None
            }
            Continuation::Login(key) => self.login_at_head(key).map(Continuation::Login),
            Continuation::Window(packet) => self.click_at_head(*packet),
            Continuation::CreativeSlot(packet) => self.creative_at_head(*packet),
            Continuation::Button(packet) => {
                self.handle_container_button(packet.player, packet.window, packet.button);
                None
            }
            Continuation::Close(packet) => {
                self.close_at_head(packet);
                None
            }
            Continuation::ClickBlock(step) => self
                .click_block_at_head(*step)
                .map(|step| Continuation::UseItemOn(Box::new(step))),
            Continuation::UseItem(step) => self.use_item_at_head(step).map(Continuation::UseItem),
            Continuation::DropSelected(step) => self
                .drop_selected_at_head(step)
                .map(|step| Continuation::Drop(Box::new(step))),
            Continuation::Wear(request) => self
                .wear_at_head(&request)
                .map(|step| Continuation::ItemDamage(Box::new(step))),
            Continuation::Input(step) => {
                let player = self.players.get(&step.player)?;
                (player.last_input != step.state).then_some(Continuation::Input(step))
            }
            Continuation::Sneak(step) => {
                let player = self.players.get(&step.player)?;
                (player.vitals.client_loaded() && player.sneaking != step.value)
                    .then_some(Continuation::Sneak(step))
            }
            Continuation::GameMode(step) => {
                let player = self.players.get(&step.player)?;
                (player.game_mode != step.mode).then_some(Continuation::GameMode(step))
            }
            Continuation::HeldSlot(step) => {
                let player = self.players.get(&step.player)?;
                if player.vitals.is_dead() {
                    return None;
                }
                Some(Continuation::HeldSlot(HeldStep {
                    previous: Some(player.inventory.selected_hotbar),
                    ..step
                }))
            }
            other => Some(other),
        }
    }

    pub(crate) fn head_payload(&self, then: &Continuation) -> Option<EventPayload> {
        Some(match then {
            Continuation::Login(key) => return self.login_payload(*key),
            Continuation::Weather(_) | Continuation::Thunder(_) => {
                return self.weather_payload(then);
            }
            Continuation::UseItemOn(step) => return self.use_on_payload(step),
            Continuation::UseItem(step) => return self.use_payload(step),
            Continuation::Drop(step) => return self.drop_payload_of(step),
            Continuation::ItemDamage(step) => return self.wear_payload(step),
            Continuation::Click(kind, step) => return self.window_click_payload(*kind, step),
            Continuation::Drag(step) => return self.drag_payload(step),
            Continuation::Creative(step) => return self.creative_payload(step),
            Continuation::Input(step) => EventPayload::PlayerInput(Pressed {
                input: pressed(step.state),
                player: self.snapshot_of(step.player)?,
            }),
            Continuation::Sneak(step) => EventPayload::PlayerToggleSneak(PlayerToggleSneak {
                player: self.snapshot_of(step.player)?,
                sneaking: step.value,
            }),
            Continuation::GameMode(step) => {
                EventPayload::PlayerGameModeChange(PlayerGameModeChange {
                    new_game_mode: step.mode,
                    player: self.snapshot_of(step.player)?,
                })
            }
            Continuation::HeldSlot(step) => EventPayload::PlayerItemHeld(PlayerItemHeld {
                new_slot: i32::from(step.slot),
                player: self.snapshot_of(step.player)?,
                previous_slot: i32::from(step.previous?),
            }),
            _ => return None,
        })
    }

    pub(super) fn finish_input(&mut self, step: InputStep) {
        let Some(player) = self.players.get_mut(&step.player) else {
            return;
        };
        let jumped = player.on_ground && step.state.jump && !player.last_input.jump;
        player.last_input = step.state;
        if jumped {
            self.award_custom(step.player, crate::stats::custom::JUMP, 1);
            let sprinting = self.players.get(&step.player).is_some_and(|p| p.sprinting);
            let (cost, reason) = if sprinting {
                (
                    self.config.hunger.jump(true),
                    EntityExhaustionReason::JumpSprint,
                )
            } else {
                (self.config.hunger.jump(false), EntityExhaustionReason::Jump)
            };
            if cost > 0.0 {
                self.exhaust(step.player, cost, reason);
            }
        }
    }

    pub(super) fn finish_sneak(&mut self, step: ToggleStep) {
        if let Some(player) = self.players.get_mut(&step.player)
            && player.sneaking != step.value
        {
            player.sneaking = step.value;
            self.broadcast_metadata(step.player);
        }
    }

    pub(crate) fn toggle_sprint(&mut self, id: PlayerId, sprinting: bool) {
        if !self
            .players
            .get(&id)
            .is_some_and(|player| player.vitals.client_loaded())
        {
            return;
        }
        let step = ToggleStep {
            player: id,
            value: sprinting,
        };
        self.fire(id, Continuation::Sprint(step), move |server| {
            server.snapshot_of(id).map(|player| {
                EventPayload::PlayerToggleSprint(PlayerToggleSprint { player, sprinting })
            })
        });
    }

    pub(super) fn finish_sprint(&mut self, step: ToggleStep) {
        if let Some(player) = self.players.get_mut(&step.player) {
            player.sprinting = step.value;
            self.broadcast_metadata(step.player);
        }
    }

    pub(crate) fn toggle_flight(&mut self, id: PlayerId, flying: bool) {
        let Some(player) = self.players.get_mut(&id) else {
            return;
        };
        if !player.abilities().may_fly {
            player.flying = false;
            return;
        }
        if player.flying == flying {
            return;
        }
        let step = ToggleStep {
            player: id,
            value: flying,
        };
        self.fire(id, Continuation::Flight(step), move |server| {
            server.snapshot_of(id).map(|player| {
                EventPayload::PlayerToggleFlight(PlayerToggleFlight { flying, player })
            })
        });
    }

    pub(super) fn finish_flight(&mut self, step: ToggleStep, cancelled: bool) {
        let Some(player) = self.players.get_mut(&step.player) else {
            return;
        };
        if !cancelled && player.abilities().may_fly && player.flying != step.value {
            player.flying = step.value;
            return;
        }
        self.send_abilities(step.player);
    }

    pub(crate) fn change_game_mode(
        &mut self,
        id: PlayerId,
        mode: GameMode,
        replies: Vec<(CommandSender, Component)>,
    ) {
        self.fire_deferred(
            id,
            Continuation::GameMode(GameModeStep {
                player: id,
                mode,
                replies,
            }),
        );
    }

    pub(super) fn finish_game_mode(&mut self, step: GameModeStep) {
        if self
            .players
            .get(&step.player)
            .is_none_or(|player| player.game_mode == step.mode)
        {
            return;
        }
        self.set_game_mode_now(step.player, step.mode);
        for (sender, reply) in step.replies {
            CommandContext::reply(self, sender, reply);
        }
    }

    pub(crate) fn notify_visibility(
        &mut self,
        kind: EventKind,
        viewer: PlayerId,
        target: PlayerId,
    ) {
        self.fire(viewer, Continuation::Notice(kind), move |server| {
            let player = server.snapshot_of(viewer)?;
            let shown = server.players.get(&target)?;
            let entity = EntityView {
                id: shown.entity_id,
                uuid: target.0,
                kind: rspigot_entities::EntityKind::Player.name().to_owned(),
                position: shown.position,
                player: server.snapshot_of(target),
                meta: crate::plugins::MetaSnapshot::default(),
                marks: Default::default(),
            };
            Some(if kind == EventKind::PlayerHideEntity {
                EventPayload::PlayerHideEntity(PlayerHideEntity { entity, player })
            } else {
                EventPayload::PlayerShowEntity(PlayerShowEntity { entity, player })
            })
        });
    }

    pub(crate) fn select_slot(&mut self, id: PlayerId, slot: u8) {
        self.fire_deferred(
            id,
            Continuation::HeldSlot(HeldStep {
                player: id,
                slot,
                previous: None,
            }),
        );
    }

    pub(super) fn finish_held_slot(&mut self, step: HeldStep, cancelled: bool) {
        let Some(player) = self.players.get_mut(&step.player) else {
            return;
        };
        let current = player.inventory.selected_hotbar;
        let outdated = step.previous.is_some_and(|previous| previous != current);
        if cancelled || outdated || player.vitals.is_dead() {
            self.push(Audience::One(step.player), ClientEvent::HeldSlot(current));
            return;
        }
        player.inventory.selected_hotbar = step.slot;
        if current != step.slot {
            self.stop_using_main_hand(step.player);
        }
        self.broadcast_held_item(step.player);
    }

    pub(crate) fn swing_arm(&mut self, id: PlayerId, hand: Hand) {
        if self
            .players
            .get(&id)
            .is_none_or(|player| player.vitals.is_dead())
        {
            return;
        }
        let animation_type = match hand {
            Hand::Main => PlayerAnimationType::ArmSwing,
            Hand::Off => PlayerAnimationType::OffArmSwing,
        };
        self.fire(
            id,
            Continuation::Swing(SwingStep { player: id, hand }),
            move |server| {
                server.snapshot_of(id).map(|player| {
                    EventPayload::PlayerAnimation(PlayerAnimation {
                        animation_type,
                        player,
                    })
                })
            },
        );
    }

    pub(super) fn finish_swing(&mut self, step: SwingStep) {
        let Some(player) = self.players.get_mut(&step.player) else {
            return;
        };
        if !player.vitals.is_dead() {
            player.vitals.attack_ticker = 0;
        }
        let entity = player.entity_id;
        let animation = match step.hand {
            Hand::Main => Animation::SwingMainArm,
            Hand::Off => Animation::SwingOffHand,
        };
        self.push(
            Audience::Trackers(entity),
            ClientEvent::EntityAnimation { entity, animation },
        );
    }

    pub(crate) fn swap_hands(&mut self, id: PlayerId) {
        let Some(player) = self.players.get(&id) else {
            return;
        };
        if player.vitals.is_dead()
            || !player.vitals.client_loaded()
            || player.game_mode == GameMode::Spectator
        {
            return;
        }
        let selected = player.inventory.selected_hotbar;
        let main = player
            .inventory
            .get(HOTBAR_START + usize::from(selected))
            .cloned();
        let off = player.inventory.get(OFFHAND_SLOT).cloned();
        let main_hand_item = off.as_ref().map(spec_of);
        let off_hand_item = main.as_ref().map(spec_of);
        let step = SwapStep {
            player: id,
            selected,
            main,
            off,
        };
        self.fire(id, Continuation::Swap(Box::new(step)), move |server| {
            server.snapshot_of(id).map(|player| {
                EventPayload::PlayerSwapHandItems(PlayerSwapHandItems {
                    main_hand_item,
                    off_hand_item,
                    player,
                })
            })
        });
    }

    pub(super) fn finish_swap(&mut self, step: SwapStep, patch: PlayerSwapHandItemsPatch) {
        let Some(player) = self.players.get_mut(&step.player) else {
            return;
        };
        let main_slot = HOTBAR_START + usize::from(step.selected);
        let unchanged = player.inventory.selected_hotbar == step.selected
            && player.inventory.get(main_slot) == step.main.as_ref()
            && player.inventory.get(OFFHAND_SLOT) == step.off.as_ref();
        if unchanged {
            let new_main = settled_hand(patch.main_hand_item, step.off.clone());
            let new_off = settled_hand(patch.off_hand_item, step.main);
            player.inventory.set(OFFHAND_SLOT, new_off);
            player.inventory.set(main_slot, new_main);
            player.using_item = None;
        }
        self.send_inventory(step.player);
        self.broadcast_held_item(step.player);
    }
}

fn settled_hand(
    patched: Option<Option<crate::plugins::ItemSpec>>,
    live: Option<rspigot_domain::item::ItemStack>,
) -> Option<rspigot_domain::item::ItemStack> {
    match patched {
        None => live,
        Some(spec) if spec == live.as_ref().map(spec_of) => live,
        Some(None) => None,
        Some(Some(spec)) => stack_of(&spec).unwrap_or(live),
    }
}
