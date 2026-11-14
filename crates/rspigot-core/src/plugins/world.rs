use std::net::IpAddr;

use rspigot_domain::damage::DamageType;

use crate::commands::{self, CommandContext, CommandSender};
use crate::config::GameRules;
use crate::events::{ClientEvent, GameEvent, SoundSource};
use crate::explosion::BlockInteraction;
use crate::health::DamageSource;
use crate::plugins::events::TimeSkipReason;
use crate::plugins::resolve::{self, expiry_of, finite_not_negative, name_of};
use crate::plugins::{GameRule, ServerOrder, ServerState, WorldOrder, WorldState};
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const PARTICLE_RANGE: f64 = 32.0;
const DAY_TICKS: i64 = 24_000;

impl<L: ClientLink> Server<L> {
    pub(super) fn apply_server_order(
        &mut self,
        connection: u64,
        origin: Option<u64>,
        order: ServerOrder,
    ) {
        let name = order.name();
        if let Err(reason) = self.server_order(connection, origin, order) {
            self.warn_plugin(connection, name, reason);
        }
    }

    fn server_order(
        &mut self,
        connection: u64,
        origin: Option<u64>,
        order: ServerOrder,
    ) -> Result<(), String> {
        match order {
            ServerOrder::Broadcast(message) => self.broadcast_order(origin, message),
            ServerOrder::DispatchCommand(line) => {
                commands::execute(self, CommandSender::Console, &line);
            }
            ServerOrder::Shutdown => CommandContext::stop(self),
            ServerOrder::Save => {
                CommandContext::save_all(self);
            }
            ServerOrder::SetWhitelistEnabled(enabled) => {
                CommandContext::set_whitelist_enabled(self, enabled);
            }
            ServerOrder::WhitelistAdd(name) => {
                CommandContext::whitelist_add(self, name_of(&name)?);
            }
            ServerOrder::WhitelistRemove(name) => {
                CommandContext::whitelist_remove(self, name_of(&name)?);
            }
            ServerOrder::OperatorAdd(name) => {
                CommandContext::set_operator(self, name_of(&name)?, true);
            }
            ServerOrder::OperatorRemove(name) => {
                CommandContext::set_operator(self, name_of(&name)?, false);
            }
            ServerOrder::BanAdd {
                name,
                reason,
                expires,
            } => {
                let target = CommandContext::profile_entry(self, name_of(&name)?);
                let expires = expiry_of(expires)?;
                let source = self.plugin_name(connection);
                CommandContext::ban_profile(self, target, &source, reason, expires);
            }
            ServerOrder::BanRemove(name) => {
                let target = CommandContext::profile_entry(self, name_of(&name)?);
                CommandContext::pardon_profile(self, &target);
            }
            ServerOrder::IpBanAdd {
                address,
                reason,
                expires,
            } => {
                let address = address_of(&address)?;
                let expires = expiry_of(expires)?;
                let source = self.plugin_name(connection);
                CommandContext::ban_address(self, address, &source, reason, expires);
            }
            ServerOrder::IpBanRemove(address) => {
                CommandContext::pardon_address(self, address_of(&address)?);
            }
            ServerOrder::CreateInventory {
                id,
                rows,
                title,
                locked,
            } => self.create_plugin_inventory(connection, id, rows, &title, locked)?,
        }
        Ok(())
    }

    pub(super) fn apply_world_order(&mut self, connection: u64, order: WorldOrder) {
        let name = order.name();
        if let Err(reason) = self.world_order(order) {
            self.warn_plugin(connection, name, reason);
        }
    }

    fn world_order(&mut self, order: WorldOrder) -> Result<(), String> {
        match order {
            WorldOrder::SetTime(time) => {
                let full_time = self.world.level.day_time;
                let margin = time.wrapping_sub(full_time).rem_euclid(DAY_TICKS);
                self.change_day_time(full_time.wrapping_add(margin), Some(TimeSkipReason::Custom));
            }
            WorldOrder::SetWeather(weather) => CommandContext::set_weather(self, weather),
            WorldOrder::SetDifficulty(difficulty) => {
                CommandContext::set_difficulty(self, difficulty);
            }
            WorldOrder::SetSpawn(pos) => CommandContext::set_world_spawn(self, pos),
            WorldOrder::SetPvp(pvp) => self.config.pvp = pvp,
            WorldOrder::SetGameRule(rule, value) => self.set_game_rule(rule, value),
            WorldOrder::Save => {
                CommandContext::save_all(self);
            }
            WorldOrder::PlaySound { sound, position } => {
                let key = resolve::sound_of(&sound)?;
                self.play_sound(
                    None,
                    position,
                    key,
                    SoundSource::Master,
                    sound.volume,
                    sound.pitch,
                );
            }
            WorldOrder::SpawnParticle(spec) => {
                let particle = resolve::particle_of(&spec)?;
                self.push(
                    Audience::Nearby {
                        position: spec.position,
                        range: PARTICLE_RANGE,
                        except: None,
                    },
                    ClientEvent::Particle(particle),
                );
            }
            WorldOrder::Explode {
                position,
                power,
                breaks_blocks,
            } => {
                let position = resolve::position_of(position)?;
                finite_not_negative("power", f64::from(power))?;
                let interaction = if breaks_blocks {
                    BlockInteraction::Destroy
                } else {
                    BlockInteraction::Keep
                };
                let damage = DamageSource::new(DamageType::Explosion);
                self.explode(None, &damage, position, power, interaction, false, None);
            }
        }
        Ok(())
    }

    fn set_game_rule(&mut self, rule: GameRule, value: bool) {
        *rule_flag(
            &mut self.config.rules,
            &mut self.world.level.daylight_cycle,
            rule,
        ) = value;
        match rule {
            GameRule::DaylightCycle => {
                let event = self.time_event();
                self.push(Audience::Dimension(self.world.dimension.key.clone()), event);
            }
            GameRule::ImmediateRespawn => self.push(
                Audience::Dimension(self.world.dimension.key.clone()),
                ClientEvent::GameEvent(GameEvent::ImmediateRespawn(value)),
            ),
            _ => {}
        }
    }

    pub(super) fn server_state(&self) -> ServerState {
        ServerState {
            max_players: self.config.max_players,
            online: u32::try_from(self.players.len()).unwrap_or(u32::MAX),
            tps: self.tps(),
            whitelist_enabled: self.admin.whitelist_enabled,
        }
    }

    pub(super) fn world_state(&self) -> WorldState {
        let level = &self.world.level;
        let (mut rules, mut daylight) = (self.config.rules, level.daylight_cycle);
        WorldState {
            name: level.name.clone(),
            uuid: level.uuid,
            seed: level.seed,
            day_time: level.day_time,
            game_time: level.game_time,
            raining: level.raining,
            thundering: level.thundering,
            difficulty: self.world.difficulty,
            spawn: self.world.spawn.block(),
            pvp: self.config.pvp,
            rules: GameRule::ALL
                .iter()
                .map(|&rule| (rule, *rule_flag(&mut rules, &mut daylight, rule)))
                .collect(),
        }
    }
}

fn address_of(raw: &str) -> Result<IpAddr, String> {
    raw.parse()
        .map_err(|_| format!("{raw} is not an IP address"))
}

const fn rule_flag<'a>(
    rules: &'a mut GameRules,
    daylight: &'a mut bool,
    rule: GameRule,
) -> &'a mut bool {
    match rule {
        GameRule::DaylightCycle => daylight,
        GameRule::NaturalRegeneration => &mut rules.natural_regeneration,
        GameRule::KeepInventory => &mut rules.keep_inventory,
        GameRule::ShowDeathMessages => &mut rules.show_death_messages,
        GameRule::ImmediateRespawn => &mut rules.immediate_respawn,
        GameRule::FallDamage => &mut rules.fall_damage,
        GameRule::MobDrops => &mut rules.mob_drops,
        GameRule::MobGriefing => &mut rules.mob_griefing,
        GameRule::MobExplosionDropDecay => &mut rules.mob_explosion_drop_decay,
        GameRule::BlockDrops => &mut rules.block_drops,
        GameRule::UniversalAnger => &mut rules.universal_anger,
    }
}
