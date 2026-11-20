use rspigot_domain::player::PlayerId;

use crate::config::{GameRules, RuleValue};
use crate::events::{ClientEvent, EntityStatus, GameEvent};
use crate::plugins::GameRule;
use crate::ports::ClientLink;
use crate::server::{Audience, Server};

const REDUCED_DEBUG_ON: i8 = 22;
const REDUCED_DEBUG_OFF: i8 = 23;

enum Rule {
    Flag(GameRule),
    Plain(fn(&mut GameRules) -> &mut bool),
    Mobs,
    Monsters,
    Pvp,
    Number(fn(&mut GameRules) -> &mut i32, i32, i32),
}

fn rule_of(name: &str) -> Option<Rule> {
    Some(match name {
        "advance_time" => Rule::Flag(GameRule::DaylightCycle),
        "advance_weather" => Rule::Plain(|rules| &mut rules.advance_weather),
        "allow_entering_nether_using_portals" => {
            Rule::Plain(|rules| &mut rules.allow_entering_nether_using_portals)
        }
        "block_drops" => Rule::Flag(GameRule::BlockDrops),
        "block_explosion_drop_decay" => Rule::Plain(|rules| &mut rules.block_explosion_drop_decay),
        "command_block_output" => Rule::Plain(|rules| &mut rules.command_block_output),
        "command_blocks_work" => Rule::Plain(|rules| &mut rules.command_blocks_work),
        "drowning_damage" => Rule::Plain(|rules| &mut rules.drowning_damage),
        "elytra_movement_check" => Rule::Plain(|rules| &mut rules.elytra_movement_check),
        "ender_pearls_vanish_on_death" => {
            Rule::Plain(|rules| &mut rules.ender_pearls_vanish_on_death)
        }
        "entity_drops" => Rule::Plain(|rules| &mut rules.entity_drops),
        "fall_damage" => Rule::Flag(GameRule::FallDamage),
        "fire_damage" => Rule::Plain(|rules| &mut rules.fire_damage),
        "fire_spread_radius_around_player" => {
            Rule::Number(|rules| &mut rules.fire_spread_radius, -1, i32::MAX)
        }
        "forgive_dead_players" => Rule::Plain(|rules| &mut rules.forgive_dead_players),
        "freeze_damage" => Rule::Plain(|rules| &mut rules.freeze_damage),
        "global_sound_events" => Rule::Plain(|rules| &mut rules.global_sound_events),
        "immediate_respawn" => Rule::Flag(GameRule::ImmediateRespawn),
        "keep_inventory" => Rule::Flag(GameRule::KeepInventory),
        "lava_source_conversion" => Rule::Plain(|rules| &mut rules.lava_source_conversion),
        "limited_crafting" => Rule::Plain(|rules| &mut rules.limited_crafting),
        "locator_bar" => Rule::Plain(|rules| &mut rules.locator_bar),
        "log_admin_commands" => Rule::Plain(|rules| &mut rules.log_admin_commands),
        "max_block_modifications" => {
            Rule::Number(|rules| &mut rules.max_block_modifications, 1, i32::MAX)
        }
        "max_command_forks" => Rule::Number(|rules| &mut rules.max_command_forks, 0, i32::MAX),
        "max_command_sequence_length" => {
            Rule::Number(|rules| &mut rules.max_command_sequence_length, 0, i32::MAX)
        }
        "max_entity_cramming" => Rule::Number(|rules| &mut rules.max_entity_cramming, 0, i32::MAX),
        "max_minecart_speed" => Rule::Number(|rules| &mut rules.max_minecart_speed, 1, 1000),
        "max_snow_accumulation_height" => {
            Rule::Number(|rules| &mut rules.max_snow_accumulation_height, 0, 8)
        }
        "mob_drops" => Rule::Flag(GameRule::MobDrops),
        "mob_explosion_drop_decay" => Rule::Flag(GameRule::MobExplosionDropDecay),
        "mob_griefing" => Rule::Flag(GameRule::MobGriefing),
        "natural_health_regeneration" => Rule::Flag(GameRule::NaturalRegeneration),
        "player_movement_check" => Rule::Plain(|rules| &mut rules.player_movement_check),
        "players_nether_portal_creative_delay" => Rule::Number(
            |rules| &mut rules.players_nether_portal_creative_delay,
            0,
            i32::MAX,
        ),
        "players_nether_portal_default_delay" => Rule::Number(
            |rules| &mut rules.players_nether_portal_default_delay,
            0,
            i32::MAX,
        ),
        "players_sleeping_percentage" => {
            Rule::Number(|rules| &mut rules.players_sleeping_percentage, 0, i32::MAX)
        }
        "projectiles_can_break_blocks" => {
            Rule::Plain(|rules| &mut rules.projectiles_can_break_blocks)
        }
        "pvp" => Rule::Pvp,
        "raids" => Rule::Plain(|rules| &mut rules.raids),
        "random_tick_speed" => Rule::Number(|rules| &mut rules.random_tick_speed, 0, i32::MAX),
        "reduced_debug_info" => Rule::Plain(|rules| &mut rules.reduced_debug_info),
        "respawn_radius" => Rule::Number(|rules| &mut rules.respawn_radius, 0, i32::MAX),
        "send_command_feedback" => Rule::Plain(|rules| &mut rules.send_command_feedback),
        "show_advancement_messages" => Rule::Plain(|rules| &mut rules.show_advancement_messages),
        "show_death_messages" => Rule::Flag(GameRule::ShowDeathMessages),
        "spawn_mobs" => Rule::Mobs,
        "spawn_monsters" => Rule::Monsters,
        "spawn_patrols" => Rule::Plain(|rules| &mut rules.spawn_patrols),
        "spawn_phantoms" => Rule::Plain(|rules| &mut rules.spawn_phantoms),
        "spawn_wandering_traders" => Rule::Plain(|rules| &mut rules.spawn_wandering_traders),
        "spawn_wardens" => Rule::Plain(|rules| &mut rules.spawn_wardens),
        "spawner_blocks_work" => Rule::Plain(|rules| &mut rules.spawner_blocks_work),
        "spectators_generate_chunks" => Rule::Plain(|rules| &mut rules.spectators_generate_chunks),
        "spread_vines" => Rule::Plain(|rules| &mut rules.spread_vines),
        "tnt_explodes" => Rule::Plain(|rules| &mut rules.tnt_explodes),
        "tnt_explosion_drop_decay" => Rule::Plain(|rules| &mut rules.tnt_explosion_drop_decay),
        "universal_anger" => Rule::Flag(GameRule::UniversalAnger),
        "water_source_conversion" => Rule::Plain(|rules| &mut rules.water_source_conversion),
        _ => return None,
    })
}

fn flag(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

pub(crate) const RULE_NAMES: [&str; 59] = [
    "advance_time",
    "advance_weather",
    "allow_entering_nether_using_portals",
    "block_drops",
    "block_explosion_drop_decay",
    "command_block_output",
    "command_blocks_work",
    "drowning_damage",
    "elytra_movement_check",
    "ender_pearls_vanish_on_death",
    "entity_drops",
    "fall_damage",
    "fire_damage",
    "fire_spread_radius_around_player",
    "forgive_dead_players",
    "freeze_damage",
    "global_sound_events",
    "immediate_respawn",
    "keep_inventory",
    "lava_source_conversion",
    "limited_crafting",
    "locator_bar",
    "log_admin_commands",
    "max_block_modifications",
    "max_command_forks",
    "max_command_sequence_length",
    "max_entity_cramming",
    "max_minecart_speed",
    "max_snow_accumulation_height",
    "mob_drops",
    "mob_explosion_drop_decay",
    "mob_griefing",
    "natural_health_regeneration",
    "player_movement_check",
    "players_nether_portal_creative_delay",
    "players_nether_portal_default_delay",
    "players_sleeping_percentage",
    "projectiles_can_break_blocks",
    "pvp",
    "raids",
    "random_tick_speed",
    "reduced_debug_info",
    "respawn_radius",
    "send_command_feedback",
    "show_advancement_messages",
    "show_death_messages",
    "spawn_mobs",
    "spawn_monsters",
    "spawn_patrols",
    "spawn_phantoms",
    "spawn_wandering_traders",
    "spawn_wardens",
    "spawner_blocks_work",
    "spectators_generate_chunks",
    "spread_vines",
    "tnt_explodes",
    "tnt_explosion_drop_decay",
    "universal_anger",
    "water_source_conversion",
];

pub(crate) fn integer_bounds(name: &str) -> Option<(i32, i32)> {
    match rule_of(name)? {
        Rule::Number(_, min, max) => Some((min, max)),
        _ => None,
    }
}

impl GameRules {
    pub fn values(&self) -> Vec<(&'static str, RuleValue)> {
        let mut copy = *self;
        let mut daylight = true;
        RULE_NAMES
            .iter()
            .filter_map(|&name| {
                let value = match rule_of(name)? {
                    Rule::Flag(GameRule::DaylightCycle) => return None,
                    Rule::Flag(rule) => RuleValue::Flag(*crate::plugins::world::rule_flag(
                        &mut copy,
                        &mut daylight,
                        rule,
                    )),
                    Rule::Plain(slot) => RuleValue::Flag(*slot(&mut copy)),
                    Rule::Number(slot, _, _) => RuleValue::Number(*slot(&mut copy)),
                    Rule::Mobs | Rule::Monsters | Rule::Pvp => return None,
                };
                Some((name, value))
            })
            .collect()
    }

    pub fn assign(&mut self, name: &str, value: RuleValue) -> bool {
        let mut daylight = true;
        match (rule_of(name), value) {
            (Some(Rule::Flag(GameRule::DaylightCycle)), _) => false,
            (Some(Rule::Flag(rule)), RuleValue::Flag(value)) => {
                *crate::plugins::world::rule_flag(self, &mut daylight, rule) = value;
                true
            }
            (Some(Rule::Plain(slot)), RuleValue::Flag(value)) => {
                *slot(self) = value;
                true
            }
            (Some(Rule::Number(slot, min, max)), RuleValue::Number(value))
                if (min..=max).contains(&value) =>
            {
                *slot(self) = value;
                true
            }
            _ => false,
        }
    }
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn game_rule_text(&self, name: &str) -> Option<String> {
        let mut rules = self.config.rules;
        let mut daylight = self.world.level.daylight_cycle;
        Some(match rule_of(name)? {
            Rule::Flag(rule) => {
                crate::plugins::world::rule_flag(&mut rules, &mut daylight, rule).to_string()
            }
            Rule::Plain(slot) => slot(&mut rules).to_string(),
            Rule::Mobs => self.world.level.spawn_mobs.to_string(),
            Rule::Monsters => self.world.level.spawn_monsters.to_string(),
            Rule::Pvp => self.config.pvp.to_string(),
            Rule::Number(slot, _, _) => slot(&mut rules).to_string(),
        })
    }

    pub(crate) fn apply_game_rule(&mut self, name: &str, value: &str) -> Result<(), &'static str> {
        match rule_of(name).ok_or("argument.rule.invalid")? {
            Rule::Flag(rule) => {
                let value = flag(value).ok_or("parsing.bool.invalid")?;
                self.set_game_rule(rule, value);
            }
            Rule::Plain(slot) => {
                let value = flag(value).ok_or("parsing.bool.invalid")?;
                *slot(&mut self.config.rules) = value;
                self.announce_rule(name, value);
            }
            Rule::Mobs => {
                self.world.level.spawn_mobs = flag(value).ok_or("parsing.bool.invalid")?;
            }
            Rule::Monsters => {
                self.world.level.spawn_monsters = flag(value).ok_or("parsing.bool.invalid")?;
            }
            Rule::Pvp => self.config.pvp = flag(value).ok_or("parsing.bool.invalid")?,
            Rule::Number(slot, min, max) => {
                let value = value.parse::<i32>().map_err(|_| "parsing.int.invalid")?;
                if value < min {
                    return Err("argument.integer.low");
                }
                if value > max {
                    return Err("argument.integer.big");
                }
                *slot(&mut self.config.rules) = value;
            }
        }
        Ok(())
    }

    fn announce_rule(&mut self, name: &str, value: bool) {
        match name {
            "limited_crafting" => self.push(
                Audience::All,
                ClientEvent::GameEvent(GameEvent::LimitedCrafting(value)),
            ),
            "reduced_debug_info" => {
                let status = if value {
                    REDUCED_DEBUG_ON
                } else {
                    REDUCED_DEBUG_OFF
                };
                let entities: Vec<_> = self
                    .players
                    .values()
                    .map(|player| (player.id, player.entity_id))
                    .collect();
                for (id, entity) in entities {
                    self.push(
                        Audience::One(id),
                        ClientEvent::EntityStatus {
                            entity,
                            status: EntityStatus::Event(status),
                        },
                    );
                }
            }
            _ => {}
        }
    }

    pub(crate) fn handle_set_game_rules(&mut self, id: PlayerId, entries: Vec<(String, String)>) {
        if !self
            .players
            .get(&id)
            .is_some_and(|player| player.permission_level() >= 2)
        {
            return;
        }
        for (key, value) in entries {
            if let Some(name) = key.strip_prefix("minecraft:") {
                let _ = self.apply_game_rule(name, &value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use crate::config::{GameConfig, GameRules, LevelData};
    use crate::explosion::BlockInteraction;
    use crate::server::Server;
    use crate::status::StatusBoard;
    use crate::world_state::tests::Quiet;

    fn server(level: LevelData) -> Server<Quiet> {
        let config = GameConfig {
            chunk_workers: 1,
            spawn_chunk_radius: 1,
            ..GameConfig::default()
        };
        let mut server = Server::new(
            config,
            level,
            Arc::new(Quiet),
            Arc::new(Quiet),
            StatusBoard::new(1),
        );
        server.wait_for_spawn(Duration::from_secs(10));
        server
    }

    #[test]
    fn the_stored_rules_replace_the_configured_ones_and_come_back_in_the_snapshot() {
        let stored = GameRules {
            respawn_radius: 2,
            spread_vines: false,
            ..GameRules::default()
        };
        let server = server(LevelData {
            rules: Some(stored),
            pvp: Some(false),
            ..LevelData::default()
        });
        assert_eq!(server.config.rules, stored);
        assert!(!server.config.pvp);
        let snapshot = server.level_snapshot();
        assert_eq!(snapshot.rules, Some(stored));
        assert_eq!(snapshot.pvp, Some(false));
    }

    #[test]
    fn a_level_without_stored_rules_keeps_the_configured_ones() {
        let server = server(LevelData::default());
        assert_eq!(server.config.rules, GameRules::default());
        assert!(server.config.pvp);
    }

    #[test]
    fn the_explosion_decay_rules_choose_the_block_interaction() {
        let mut server = server(LevelData::default());
        assert_eq!(server.tnt_block_interaction(), BlockInteraction::Destroy);
        assert_eq!(
            server.block_block_interaction(),
            BlockInteraction::DestroyWithDecay
        );
        assert_eq!(
            server.mob_block_interaction(),
            BlockInteraction::DestroyWithDecay
        );
        server.config.rules.tnt_explosion_drop_decay = true;
        server.config.rules.block_explosion_drop_decay = false;
        server.config.rules.mob_explosion_drop_decay = false;
        assert_eq!(
            server.tnt_block_interaction(),
            BlockInteraction::DestroyWithDecay
        );
        assert_eq!(server.block_block_interaction(), BlockInteraction::Destroy);
        assert_eq!(server.mob_block_interaction(), BlockInteraction::Destroy);
    }

    #[test]
    fn rules_values_and_assign_round_trip_every_persisted_rule() {
        let mut rules = GameRules::default();
        let values = rules.values();
        assert_eq!(values.len(), 59 - 4);
        for (name, value) in &values {
            assert!(rules.assign(name, *value), "{name}");
        }
        assert_eq!(rules, GameRules::default());
        assert!(!rules.assign("respawn_radius", crate::config::RuleValue::Number(-1)));
        assert!(!rules.assign("advance_time", crate::config::RuleValue::Flag(false)));
    }
}
