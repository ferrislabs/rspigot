use std::ops::RangeInclusive;

use rand::Rng;
use rspigot_domain::Identifier;
use rspigot_domain::attribute::MOVEMENT_SPEED;
use rspigot_domain::damage::DamageType;
use rspigot_domain::food::{MAX_FOOD, MAX_SATURATION};
use rspigot_domain::player::PlayerId;

use crate::admin::ListEntry;
use crate::commands::{self, CommandContext, CommandSender};
use crate::events::{ClientEvent, SoundSource};
use crate::health::{self, DamageSource};
use crate::player::Player;
use crate::plugins::events::{EntityPotionEffectCause, PlayerRespawnReason, PlayerTeleportCause};
use crate::plugins::resolve::{self, finite_not_negative, not_negative, within};
use crate::plugins::{EventKind, PlayerOrder, TitleSpec};
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
        if !self.players.contains_key(&player) {
            return;
        }
        let name = order.name();
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
            PlayerOrder::Teleport(position) => {
                let position = resolve::position_of(position)?;
                self.ask_teleport(id, position, None, PlayerTeleportCause::Plugin, None);
            }
            PlayerOrder::TeleportFacing(position, rotation) => {
                let position = resolve::position_of(position)?;
                let rotation = resolve::rotation_of(rotation)?;
                self.ask_teleport(
                    id,
                    position,
                    Some(rotation),
                    PlayerTeleportCause::Plugin,
                    None,
                );
            }
            PlayerOrder::TeleportTo {
                world,
                position,
                rotation,
            } => {
                let position = resolve::position_of(position)?;
                let dimension = if world.is_empty() {
                    None
                } else {
                    let key = Identifier::parse(&world)
                        .map_err(|_| format!("`{world}` is not a world key"))?;
                    if !self.ensure_dimension(&key) {
                        return Err(format!("unknown world `{world}`"));
                    }
                    Some(key)
                };
                self.ask_teleport_in(
                    id,
                    dimension,
                    position,
                    rotation,
                    PlayerTeleportCause::Plugin,
                    None,
                );
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
                self.respawn(id, PlayerRespawnReason::Plugin);
            }
            PlayerOrder::PerformCommand(line) => {
                commands::execute(self, CommandSender::Player(id), &line);
            }
            PlayerOrder::Chat(text) => match text.strip_prefix('/') {
                Some(command) => self.run_player_command(id, command),
                None => self.broadcast_chat(id, &text),
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
            PlayerOrder::HidePlayer(target) => self.hide_player(connection, id, target),
            PlayerOrder::ShowPlayer(target) => self.show_player(connection, id, target),
            PlayerOrder::OpenInventory(inventory) => {
                self.open_plugin_menu(connection, id, inventory);
            }
            PlayerOrder::CloseInventory => {
                self.close_open_menu(id, true);
            }
            PlayerOrder::UpdateInventory => self.send_inventory(id),
            PlayerOrder::Ban { reason, expires } => {
                let expires = resolve::expiry_of(expires)?;
                if let Some(target) = self.players.get(&id).map(|p| ListEntry::of(&p.profile)) {
                    let source = self.plugin_name(connection);
                    CommandContext::ban_profile(self, target, &source, reason, expires);
                }
            }
        }
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

    fn hide_player(&mut self, connection: u64, viewer: PlayerId, target: PlayerId) {
        if viewer == target {
            return;
        }
        let Some(entity) = self.players.get(&target).map(|player| player.entity_id) else {
            return;
        };
        let Some(plugin) = self.plugins.name(connection) else {
            return;
        };
        if !self.visibility.hide(plugin, viewer, target) {
            return;
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
    }

    fn show_player(&mut self, connection: u64, viewer: PlayerId, target: PlayerId) {
        let Some(plugin) = self.plugins.name(connection) else {
            return;
        };
        if !self.visibility.show(plugin, viewer, target) {
            return;
        }
        if let (Some(watcher), Some(shown)) = (self.players.get(&viewer), self.players.get(&target))
        {
            watcher
                .link
                .send(ClientEvent::PlayerInfo(vec![Self::list_entry(shown)]));
        }
        self.notify_visibility(EventKind::PlayerShowEntity, viewer, target);
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
