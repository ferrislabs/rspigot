use rspigot_domain::player::PlayerId;

use crate::plugins::GameRule;
use crate::ports::ClientLink;
use crate::server::Server;

enum Rule {
    Flag(GameRule),
    Plain(fn(&mut crate::config::GameRules) -> &mut bool),
    Number(fn(&mut crate::config::GameRules) -> &mut i32, i32, i32),
}

fn rule_of(key: &str) -> Option<Rule> {
    let name = key.strip_prefix("minecraft:")?;
    Some(match name {
        "advance_time" => Rule::Flag(GameRule::DaylightCycle),
        "natural_health_regeneration" => Rule::Flag(GameRule::NaturalRegeneration),
        "keep_inventory" => Rule::Flag(GameRule::KeepInventory),
        "show_death_messages" => Rule::Flag(GameRule::ShowDeathMessages),
        "immediate_respawn" => Rule::Flag(GameRule::ImmediateRespawn),
        "fall_damage" => Rule::Flag(GameRule::FallDamage),
        "mob_drops" => Rule::Flag(GameRule::MobDrops),
        "mob_griefing" => Rule::Flag(GameRule::MobGriefing),
        "mob_explosion_drop_decay" => Rule::Flag(GameRule::MobExplosionDropDecay),
        "block_drops" => Rule::Flag(GameRule::BlockDrops),
        "universal_anger" => Rule::Flag(GameRule::UniversalAnger),
        "tnt_explodes" => Rule::Plain(|rules| &mut rules.tnt_explodes),
        "forgive_dead_players" => Rule::Plain(|rules| &mut rules.forgive_dead_players),
        "spawn_patrols" => Rule::Plain(|rules| &mut rules.spawn_patrols),
        "spawn_phantoms" => Rule::Plain(|rules| &mut rules.spawn_phantoms),
        "spawn_wandering_traders" => Rule::Plain(|rules| &mut rules.spawn_wandering_traders),
        "raids" => Rule::Plain(|rules| &mut rules.raids),
        "players_sleeping_percentage" => {
            Rule::Number(|rules| &mut rules.players_sleeping_percentage, 0, i32::MAX)
        }
        "random_tick_speed" => Rule::Number(|rules| &mut rules.random_tick_speed, 0, i32::MAX),
        "fire_spread_radius_around_player" => {
            Rule::Number(|rules| &mut rules.fire_spread_radius, -1, i32::MAX)
        }
        "max_snow_accumulation_height" => {
            Rule::Number(|rules| &mut rules.max_snow_accumulation_height, 0, 8)
        }
        "max_command_sequence_length" => {
            Rule::Number(|rules| &mut rules.max_command_sequence_length, 1, i32::MAX)
        }
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

pub(crate) const RULE_NAMES: [&str; 22] = [
    "advance_time",
    "natural_health_regeneration",
    "keep_inventory",
    "show_death_messages",
    "immediate_respawn",
    "fall_damage",
    "mob_drops",
    "mob_griefing",
    "mob_explosion_drop_decay",
    "block_drops",
    "universal_anger",
    "tnt_explodes",
    "forgive_dead_players",
    "spawn_patrols",
    "spawn_phantoms",
    "spawn_wandering_traders",
    "raids",
    "players_sleeping_percentage",
    "random_tick_speed",
    "fire_spread_radius_around_player",
    "max_snow_accumulation_height",
    "max_command_sequence_length",
];

impl<L: ClientLink> Server<L> {
    pub(crate) fn game_rule_text(&self, name: &str) -> Option<String> {
        let key = format!("minecraft:{name}");
        let mut rules = self.config.rules;
        let mut daylight = self.world.level.daylight_cycle;
        Some(match rule_of(&key)? {
            Rule::Flag(rule) => {
                crate::plugins::world::rule_flag(&mut rules, &mut daylight, rule).to_string()
            }
            Rule::Plain(slot) => slot(&mut rules).to_string(),
            Rule::Number(slot, _, _) => slot(&mut rules).to_string(),
        })
    }

    pub(crate) fn apply_game_rule(&mut self, name: &str, value: &str) -> Result<(), &'static str> {
        let key = format!("minecraft:{name}");
        match rule_of(&key).ok_or("argument.rule.invalid")? {
            Rule::Flag(rule) => {
                let value = flag(value).ok_or("parsing.bool.invalid")?;
                self.set_game_rule(rule, value);
            }
            Rule::Plain(slot) => {
                *slot(&mut self.config.rules) = flag(value).ok_or("parsing.bool.invalid")?;
            }
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
