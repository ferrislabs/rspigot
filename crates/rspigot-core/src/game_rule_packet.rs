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

impl<L: ClientLink> Server<L> {
    pub(crate) fn handle_set_game_rules(&mut self, id: PlayerId, entries: Vec<(String, String)>) {
        if !self
            .players
            .get(&id)
            .is_some_and(|player| player.permission_level() >= 2)
        {
            return;
        }
        for (key, value) in entries {
            match rule_of(&key) {
                Some(Rule::Flag(rule)) => {
                    if let Some(value) = flag(&value) {
                        self.set_game_rule(rule, value);
                    }
                }
                Some(Rule::Plain(slot)) => {
                    if let Some(value) = flag(&value) {
                        *slot(&mut self.config.rules) = value;
                    }
                }
                Some(Rule::Number(slot, min, max)) => {
                    if let Ok(value) = value.parse::<i32>()
                        && (min..=max).contains(&value)
                    {
                        *slot(&mut self.config.rules) = value;
                    }
                }
                None => {}
            }
        }
    }
}
