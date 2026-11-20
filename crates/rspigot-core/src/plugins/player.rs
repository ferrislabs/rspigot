use std::ops::RangeInclusive;

use rand::Rng;
use rspigot_domain::attribute::MOVEMENT_SPEED;
use rspigot_domain::damage::DamageType;
use rspigot_domain::food::{MAX_FOOD, MAX_SATURATION};
use rspigot_domain::player::PlayerId;

use crate::admin::ListEntry;
use crate::commands::{self, CommandContext, CommandSender};
use crate::events::{ClientEvent, SoundSource};
use crate::health::{self, DamageSource};
use crate::player::Player;
use crate::plugins::events::{EntityPotionEffectCause, PlayerRespawnReason};
use crate::plugins::resolve::{self, finite_not_negative, not_negative, within};
use crate::plugins::{ActionError, EventKind, PlayerOrder, TitleSpec};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const MAX_EXHAUSTION: f32 = 40.0;
const SPEED: RangeInclusive<f32> = -1.0..=1.0;
pub(super) const SPEED_SCALE: f32 = 2.0;

impl<L: ClientLink> Server<L> {
    pub(super) fn apply_player_order(
        &mut self,
        connection: u64,
        player: PlayerId,
        order: PlayerOrder,
    ) {
        let name = order.name();
        if !self.players.contains_key(&player) {
            self.warn_plugin(connection, name, "the player is not online");
            return;
        }
        if let Err(reason) = self.player_order(connection, player, order) {
            self.warn_plugin(connection, name, reason);
        }
    }

    fn player_order(
        &mut self,
        connection: u64,
        id: PlayerId,
        order: PlayerOrder,
    ) -> Result<(), String> {
        match order {
            PlayerOrder::SendMessage(message) => {
                self.push(
                    Audience::One(id),
                    ClientEvent::system(resolve::text(&message)),
                );
            }
            PlayerOrder::SendActionBar(message) => self.push(
                Audience::One(id),
                ClientEvent::SystemChat {
                    message: resolve::text(&message),
                    overlay: true,
                },
            ),
            PlayerOrder::SendTitle(title) => self.send_title(id, &title)?,
            PlayerOrder::ShowDialog(spec) => {
                let dialog = dialog_of(&spec)?;
                self.push(Audience::One(id), ClientEvent::ShowDialog(Box::new(dialog)));
            }
            PlayerOrder::SendRichMessage(parts) => {
                let message = rich_message_of(&parts)?;
                self.push(
                    Audience::One(id),
                    ClientEvent::SystemChat {
                        message,
                        overlay: false,
                    },
                );
            }
            PlayerOrder::ClearDialog => self.push(Audience::One(id), ClientEvent::ClearDialog),
            PlayerOrder::ResetTitle => {
                self.push(Audience::One(id), ClientEvent::ClearTitles { reset: true });
            }
            PlayerOrder::SetTabList { header, footer } => self.push(
                Audience::One(id),
                ClientEvent::TabList {
                    header: resolve::text(&header),
                    footer: resolve::text(&footer),
                },
            ),
            PlayerOrder::Teleport(_)
            | PlayerOrder::TeleportFacing(..)
            | PlayerOrder::TeleportTo { .. } => {
                self.begin_teleport(id, order, None)
                    .map_err(|error| error.reason)?;
            }
            PlayerOrder::SetGameMode(mode) => self.change_game_mode(id, mode, Vec::new()),
            PlayerOrder::Kick(reason) => CommandContext::kick(self, id, resolve::text(&reason)),
            PlayerOrder::SetHealth(health) => {
                let max = self.players.get(&id).map_or(0.0, health::max_health);
                within("health", health, 0.0..=max)?;
                if health == 0.0 {
                    self.kill(id);
                } else {
                    self.update(id, |player| player.vitals.health = health);
                }
            }
            PlayerOrder::SetAbsorption(amount) => {
                finite_not_negative("absorption", f64::from(amount))?;
                self.update(id, |player| player.vitals.absorption = amount);
                self.clamp_vitals(id);
            }
            PlayerOrder::Damage(amount) => {
                let amount = resolve::damage_of(amount)?;
                self.hurt(id, &DamageSource::new(DamageType::Generic), amount);
            }
            PlayerOrder::Kill => {
                self.kill(id);
            }
            PlayerOrder::SetFoodLevel(food) => {
                within("food level", food, 0..=MAX_FOOD)?;
                self.update(id, |player| player.vitals.food.food = food);
            }
            PlayerOrder::SetSaturation(saturation) => {
                within("saturation", saturation, 0.0..=MAX_SATURATION)?;
                self.update(id, |player| player.vitals.food.saturation = saturation);
            }
            PlayerOrder::SetExhaustion(exhaustion) => {
                within("exhaustion", exhaustion, 0.0..=MAX_EXHAUSTION)?;
                self.update(id, |player| player.vitals.food.exhaustion = exhaustion);
            }
            PlayerOrder::GiveExp(points) => self.give_experience(id, points),
            PlayerOrder::GiveLevels(levels) => self.update(id, |player| {
                let tick_count = player.vitals.tick_count as i32;
                player.vitals.experience.give_levels(levels, tick_count);
            }),
            PlayerOrder::SetLevel(level) => {
                not_negative("level", level)?;
                self.update(id, |player| {
                    player.vitals.experience.level = level;
                    player.vitals.experience.last_sent = -1;
                });
            }
            PlayerOrder::SetExp(progress) => {
                within("experience progress", progress, 0.0..=1.0)?;
                self.update(id, |player| {
                    player.vitals.experience.progress = progress;
                    player.vitals.experience.last_sent = -1;
                });
            }
            PlayerOrder::AddEffect(spec) => {
                let effect = resolve::effect_of(&spec)?;
                self.apply_effect(id, effect, Some(EntityPotionEffectCause::Plugin));
            }
            PlayerOrder::RemoveEffect(key) => {
                let effect = resolve::effect_id(&key)?;
                self.remove_effects(id, Some(effect), EntityPotionEffectCause::Plugin);
            }
            PlayerOrder::ClearEffects => {
                self.remove_effects(id, None, EntityPotionEffectCause::Plugin);
            }
            PlayerOrder::SetFireTicks(ticks) => {
                self.update(id, |player| player.vitals.fire_ticks = ticks);
                self.broadcast_metadata(id);
            }
            PlayerOrder::SetVelocity(velocity) => {
                let velocity = resolve::velocity_of(velocity)?;
                self.update(id, |player| {
                    player.vitals.velocity = velocity;
                    player.vitals.sync_velocity = true;
                });
            }
            PlayerOrder::SetAllowFlight(allowed) => {
                self.update(id, |player| {
                    player.allow_flight = Some(allowed);
                    if !allowed {
                        player.flying = false;
                    }
                });
                self.send_abilities(id);
            }
            PlayerOrder::SetFlying(flying) => {
                let may_fly = self
                    .players
                    .get(&id)
                    .is_some_and(|player| player.abilities().may_fly);
                if flying && !may_fly {
                    return Err("the player is not allowed to fly".to_owned());
                }
                self.update(id, |player| player.flying = flying);
                self.send_abilities(id);
            }
            PlayerOrder::SetFlySpeed(speed) => {
                within("fly speed", speed, SPEED)?;
                self.update(id, |player| player.fly_speed = speed / SPEED_SCALE);
                self.send_abilities(id);
            }
            PlayerOrder::SetWalkSpeed(speed) => {
                within("walk speed", speed, SPEED)?;
                self.update(id, |player| player.walk_speed = speed / SPEED_SCALE);
                self.send_abilities(id);
                self.sync_attributes(id, &[MOVEMENT_SPEED]);
            }
            PlayerOrder::SetOp(operator) => {
                if let Some(name) = self.players.get(&id).map(|p| p.profile.name.clone()) {
                    CommandContext::set_operator(self, &name, operator);
                }
            }
            PlayerOrder::Respawn => {
                if !self.players.get(&id).is_some_and(|p| p.vitals.is_dead()) {
                    return Err("the player is not dead".to_owned());
                }
                self.in_player_dimension(id, |server| {
                    server.respawn(id, PlayerRespawnReason::Plugin);
                });
            }
            PlayerOrder::PerformCommand(line) => {
                commands::execute(self, CommandSender::Player(id), &line);
            }
            PlayerOrder::Chat(text) => match text.strip_prefix('/') {
                Some(command) => self.run_player_command(id, command),
                None => self.broadcast_chat(id, &text, None),
            },
            PlayerOrder::PlaySound(spec) => {
                let sound = resolve::sound_of(&spec)?;
                if let Some(position) = self.players.get(&id).map(|p| p.position) {
                    let seed = self.rng.random::<i64>();
                    self.push(
                        Audience::One(id),
                        ClientEvent::Sound {
                            sound,
                            source: SoundSource::Master,
                            position,
                            volume: spec.volume,
                            pitch: spec.pitch,
                            seed,
                        },
                    );
                }
            }
            PlayerOrder::SpawnParticle(spec) => {
                let particle = resolve::particle_of(&spec)?;
                self.push(Audience::One(id), ClientEvent::Particle(particle));
            }
            PlayerOrder::HidePlayer(target) => self.hide_player(connection, id, target)?,
            PlayerOrder::ShowPlayer(target) => self.show_player(connection, id, target)?,
            PlayerOrder::OpenInventory(inventory) => {
                self.begin_plugin_open(connection, id, inventory, None)
                    .map_err(|error| error.reason)?;
            }
            PlayerOrder::SetScoreboard(board) => self.set_player_board(id, board)?,
            PlayerOrder::CloseInventory => {
                self.close_window_for_plugin(id);
            }
            PlayerOrder::UpdateInventory => self.send_inventory(id),
            PlayerOrder::Ban { reason, expires } => {
                let expires = resolve::expiry_of(expires)?;
                if let Some(target) = self.players.get(&id).map(|p| ListEntry::of(&p.profile)) {
                    let source = self.plugin_name(connection);
                    CommandContext::ban_profile(self, target, &source, reason, expires);
                }
            }
            PlayerOrder::Transfer { host, port } => {
                self.transfer_for_plugin(id, &host, port)
                    .map_err(|error| error.reason)?;
            }
        }
        Ok(())
    }

    pub(super) fn transfer_for_plugin(
        &mut self,
        id: PlayerId,
        host: &str,
        port: u32,
    ) -> Result<(), ActionError> {
        if !self.players.contains_key(&id) {
            return Err(ActionError::player_gone());
        }
        if host.trim().is_empty() {
            return Err(ActionError::from("the host is empty".to_owned()));
        }
        let port = u16::try_from(port)
            .ok()
            .filter(|port| *port != 0)
            .ok_or_else(|| ActionError::from(format!("{port} is not a port")))?;
        CommandContext::transfer_player(self, id, host, i32::from(port));
        Ok(())
    }

    fn send_title(&mut self, id: PlayerId, title: &TitleSpec) -> Result<(), String> {
        not_negative("fade in", title.fade_in)?;
        not_negative("stay", title.stay)?;
        not_negative("fade out", title.fade_out)?;
        self.push(
            Audience::One(id),
            ClientEvent::TitleTimes {
                fade_in: title.fade_in,
                stay: title.stay,
                fade_out: title.fade_out,
            },
        );
        if let Some(subtitle) = &title.subtitle {
            self.push(
                Audience::One(id),
                ClientEvent::SubtitleText(resolve::text(subtitle)),
            );
        }
        self.push(
            Audience::One(id),
            ClientEvent::TitleText(resolve::text(&title.title)),
        );
        Ok(())
    }

    fn hide_player(
        &mut self,
        connection: u64,
        viewer: PlayerId,
        target: PlayerId,
    ) -> Result<(), String> {
        if viewer == target {
            return Err("a player cannot be hidden from themselves".to_owned());
        }
        let entity = self
            .players
            .get(&target)
            .map(|player| player.entity_id)
            .ok_or("the player to hide is not online")?;
        let plugin = self
            .plugins
            .name(connection)
            .ok_or("the plugin is no longer connected")?;
        if !self.visibility.hide(plugin, viewer, target) {
            return Ok(());
        }
        if let Some(player) = self.players.get_mut(&viewer) {
            if player.tracked.remove(&entity) {
                player.link.send(ClientEvent::RemoveEntities(vec![entity]));
            }
            player
                .link
                .send(ClientEvent::PlayerInfoRemove(vec![target]));
        }
        self.notify_visibility(EventKind::PlayerHideEntity, viewer, target);
        Ok(())
    }

    fn show_player(
        &mut self,
        connection: u64,
        viewer: PlayerId,
        target: PlayerId,
    ) -> Result<(), String> {
        let plugin = self
            .plugins
            .name(connection)
            .ok_or("the plugin is no longer connected")?;
        if !self.visibility.show(plugin, viewer, target) {
            return Ok(());
        }
        if let (Some(watcher), Some(shown)) = (self.players.get(&viewer), self.players.get(&target))
        {
            watcher
                .link
                .send(ClientEvent::PlayerInfo(vec![Self::list_entry(shown)]));
        }
        self.notify_visibility(EventKind::PlayerShowEntity, viewer, target);
        Ok(())
    }

    pub(super) fn send_abilities(&mut self, id: PlayerId) {
        if let Some(abilities) = self.players.get(&id).map(Player::abilities) {
            self.push(Audience::One(id), ClientEvent::Abilities(abilities));
        }
    }

    fn update(&mut self, id: PlayerId, change: impl FnOnce(&mut Player<L>)) {
        if let Some(player) = self.players.get_mut(&id) {
            change(player);
        }
    }
}

fn dialog_of(spec: &crate::plugins::DialogSpec) -> Result<rspigot_domain::dialog::Dialog, String> {
    use rspigot_domain::Identifier;
    use rspigot_domain::dialog::{Dialog, DialogButton, DialogInput};

    let inputs = spec
        .inputs
        .iter()
        .map(|input| DialogInput {
            key: input.key.clone(),
            label: resolve::text(&input.label),
            initial: input.initial.clone(),
            max_length: if input.max_length == 0 {
                rspigot_domain::dialog::DEFAULT_INPUT_LENGTH
            } else {
                input.max_length
            },
            multiline: input.multiline,
        })
        .collect();
    let buttons = spec
        .buttons
        .iter()
        .map(|button| {
            Identifier::parse(&button.id)
                .map(|id| DialogButton {
                    label: resolve::text(&button.label),
                    id,
                })
                .map_err(|_| format!("`{}` is not a valid button id", button.id))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Dialog::new(
        resolve::text(&spec.title),
        spec.body.iter().map(|line| resolve::text(line)).collect(),
        inputs,
        buttons,
        spec.can_close_with_escape,
    )
    .map_err(|error| error.to_string())
}

const MAX_CLICK_VALUE: usize = 256;
const MAX_RICH_PARTS: usize = 64;

fn rich_message_of(
    parts: &[crate::plugins::RichPartSpec],
) -> Result<rspigot_domain::text::Component, String> {
    use rspigot_domain::Identifier;
    use rspigot_domain::text::{ClickEvent, Component};

    use crate::plugins::ClickSpec;

    if parts.len() > MAX_RICH_PARTS {
        return Err(format!("a message holds at most {MAX_RICH_PARTS} parts"));
    }
    let too_long = |value: &str| {
        (value.chars().count() > MAX_CLICK_VALUE)
            .then(|| format!("a click value holds at most {MAX_CLICK_VALUE} characters"))
    };
    let mut extra = Vec::with_capacity(parts.len());
    for part in parts {
        let mut wrapper = Component::text("");
        wrapper.extra.push(resolve::text(&part.text));
        wrapper.style.hover = part
            .hover
            .as_deref()
            .map(|hover| Box::new(resolve::text(hover)));
        wrapper.style.click = match &part.click {
            None => None,
            Some(ClickSpec::RunCommand(command)) => {
                too_long(command).map_or(Ok(()), Err)?;
                Some(Box::new(ClickEvent::RunCommand(command.clone())))
            }
            Some(ClickSpec::SuggestCommand(command)) => {
                too_long(command).map_or(Ok(()), Err)?;
                Some(Box::new(ClickEvent::SuggestCommand(command.clone())))
            }
            Some(ClickSpec::OpenUrl(url)) => {
                too_long(url).map_or(Ok(()), Err)?;
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return Err(format!("`{url}` is not an http or https address"));
                }
                Some(Box::new(ClickEvent::OpenUrl(url.clone())))
            }
            Some(ClickSpec::CopyToClipboard(text)) => {
                too_long(text).map_or(Ok(()), Err)?;
                Some(Box::new(ClickEvent::CopyToClipboard(text.clone())))
            }
            Some(ClickSpec::Custom(id)) => {
                let id =
                    Identifier::parse(id).map_err(|_| format!("`{id}` is not a valid click id"))?;
                Some(Box::new(ClickEvent::Custom(id.to_string())))
            }
        };
        extra.push(wrapper);
    }
    let mut root = Component::text("");
    root.extra = extra;
    Ok(root)
}
