use rspigot_nbt::{Compound, Tag};

use super::nbt::{compound_mut, rename_key};

const OVERWORLD: &str = "minecraft:overworld";
const DEFAULT_WORLD_BORDER_SIZE: f64 = 59_999_968.0;

const GAME_RULE_NAMES: &[(&str, &str)] = &[
    (
        "allowEnteringNetherUsingPortals",
        "minecraft:allow_entering_nether_using_portals",
    ),
    (
        "announceAdvancements",
        "minecraft:show_advancement_messages",
    ),
    (
        "blockExplosionDropDecay",
        "minecraft:block_explosion_drop_decay",
    ),
    ("commandBlockOutput", "minecraft:command_block_output"),
    ("enableCommandBlocks", "minecraft:command_blocks_work"),
    ("commandBlocksEnabled", "minecraft:command_blocks_work"),
    (
        "commandModificationBlockLimit",
        "minecraft:max_block_modifications",
    ),
    (
        "disableElytraMovementCheck",
        "minecraft:elytra_movement_check",
    ),
    (
        "disablePlayerMovementCheck",
        "minecraft:player_movement_check",
    ),
    ("disableRaids", "minecraft:raids"),
    ("doDaylightCycle", "minecraft:advance_time"),
    ("doEntityDrops", "minecraft:entity_drops"),
    ("doImmediateRespawn", "minecraft:immediate_respawn"),
    ("doInsomnia", "minecraft:spawn_phantoms"),
    ("doLimitedCrafting", "minecraft:limited_crafting"),
    ("doMobLoot", "minecraft:mob_drops"),
    ("doMobSpawning", "minecraft:spawn_mobs"),
    ("doPatrolSpawning", "minecraft:spawn_patrols"),
    ("doTileDrops", "minecraft:block_drops"),
    ("doTraderSpawning", "minecraft:spawn_wandering_traders"),
    ("doVinesSpread", "minecraft:spread_vines"),
    ("doWardenSpawning", "minecraft:spawn_wardens"),
    ("doWeatherCycle", "minecraft:advance_weather"),
    ("drowningDamage", "minecraft:drowning_damage"),
    (
        "enderPearlsVanishOnDeath",
        "minecraft:ender_pearls_vanish_on_death",
    ),
    ("fallDamage", "minecraft:fall_damage"),
    ("fireDamage", "minecraft:fire_damage"),
    ("forgiveDeadPlayers", "minecraft:forgive_dead_players"),
    ("freezeDamage", "minecraft:freeze_damage"),
    ("globalSoundEvents", "minecraft:global_sound_events"),
    ("keepInventory", "minecraft:keep_inventory"),
    ("lavaSourceConversion", "minecraft:lava_source_conversion"),
    ("locatorBar", "minecraft:locator_bar"),
    ("logAdminCommands", "minecraft:log_admin_commands"),
    (
        "maxCommandChainLength",
        "minecraft:max_command_sequence_length",
    ),
    ("maxCommandForkCount", "minecraft:max_command_forks"),
    ("maxEntityCramming", "minecraft:max_entity_cramming"),
    ("minecartMaxSpeed", "minecraft:max_minecart_speed"),
    (
        "mobExplosionDropDecay",
        "minecraft:mob_explosion_drop_decay",
    ),
    ("mobGriefing", "minecraft:mob_griefing"),
    (
        "naturalRegeneration",
        "minecraft:natural_health_regeneration",
    ),
    (
        "playersNetherPortalCreativeDelay",
        "minecraft:players_nether_portal_creative_delay",
    ),
    (
        "playersNetherPortalDefaultDelay",
        "minecraft:players_nether_portal_default_delay",
    ),
    (
        "playersSleepingPercentage",
        "minecraft:players_sleeping_percentage",
    ),
    (
        "projectilesCanBreakBlocks",
        "minecraft:projectiles_can_break_blocks",
    ),
    ("pvp", "minecraft:pvp"),
    ("randomTickSpeed", "minecraft:random_tick_speed"),
    ("reducedDebugInfo", "minecraft:reduced_debug_info"),
    ("sendCommandFeedback", "minecraft:send_command_feedback"),
    ("showDeathMessages", "minecraft:show_death_messages"),
    (
        "snowAccumulationHeight",
        "minecraft:max_snow_accumulation_height",
    ),
    ("spawnMonsters", "minecraft:spawn_monsters"),
    ("spawnRadius", "minecraft:respawn_radius"),
    ("spawnerBlocksEnabled", "minecraft:spawner_blocks_work"),
    (
        "spectatorsGenerateChunks",
        "minecraft:spectators_generate_chunks",
    ),
    ("tntExplodes", "minecraft:tnt_explodes"),
    (
        "tntExplosionDropDecay",
        "minecraft:tnt_explosion_drop_decay",
    ),
    ("universalAnger", "minecraft:universal_anger"),
    ("waterSourceConversion", "minecraft:water_source_conversion"),
];

const INVERTED_GAME_RULES: &[&str] = &[
    "disableElytraMovementCheck",
    "disablePlayerMovementCheck",
    "disableRaids",
];

const DROPPED_GAME_RULES: &[&str] = &[
    "spawnChunkRadius",
    "entitiesWithPassengersCanUsePortals",
    "gameLoopFunction",
];

const BORDER_KEYS: [&str; 9] = [
    "BorderCenterX",
    "BorderCenterZ",
    "BorderSize",
    "BorderSizeLerpTime",
    "BorderSizeLerpTarget",
    "BorderSafeZone",
    "BorderDamagePerBlock",
    "BorderWarningBlocks",
    "BorderWarningTime",
];

const SPAWN_KEYS: [&str; 4] = ["SpawnX", "SpawnY", "SpawnZ", "SpawnAngle"];

const WEATHER_KEYS: [&str; 5] = [
    "clearWeatherTime",
    "rainTime",
    "thunderTime",
    "raining",
    "thundering",
];

const TRADER_KEYS: [&str; 3] = [
    "WanderingTraderSpawnDelay",
    "WanderingTraderSpawnChance",
    "WanderingTraderId",
];

const DEFAULT_FIRE_SPREAD_RADIUS: i32 = 128;

fn data_mut(root: &mut Compound) -> Option<&mut Compound> {
    compound_mut(root, "Data")
}

fn parse_flag(tag: &Tag) -> bool {
    match tag {
        Tag::String(text) => text.eq_ignore_ascii_case("true"),
        other => other.as_i64().is_some_and(|value| value != 0),
    }
}

fn convert_rule_value(tag: &Tag, inverted: bool) -> Tag {
    let text = match tag {
        Tag::String(text) => text.trim(),
        _ => "",
    };
    if inverted {
        return Tag::Byte(i8::from(!parse_flag(tag)));
    }
    if text.eq_ignore_ascii_case("true") || text.eq_ignore_ascii_case("false") {
        return Tag::Byte(i8::from(text.eq_ignore_ascii_case("true")));
    }
    if let Ok(number) = text.parse::<i64>() {
        return Tag::Int(number.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32);
    }
    tag.clone()
}

pub fn game_rule_registry_4658(root: &mut Compound) {
    let Some(data) = data_mut(root) else {
        return;
    };
    let Some(Tag::Compound(mut old)) = data.remove("GameRules") else {
        return;
    };
    let fire_tick = old.get("doFireTick").is_none_or(parse_flag);
    let away_from_player = old
        .get("allowFireTicksAwayFromPlayer")
        .is_some_and(parse_flag);
    let radius = match (fire_tick, away_from_player) {
        (false, _) => 0,
        (true, true) => -1,
        (true, false) => DEFAULT_FIRE_SPREAD_RADIUS,
    };
    let mut rules = Compound::new();
    if radius != DEFAULT_FIRE_SPREAD_RADIUS {
        rules.insert("minecraft:fire_spread_radius_around_player", radius);
    }
    for name in DROPPED_GAME_RULES
        .iter()
        .chain(&["doFireTick", "allowFireTicksAwayFromPlayer"])
    {
        old.remove(name);
    }
    for (key, value) in old {
        match GAME_RULE_NAMES
            .iter()
            .find(|(old_name, _)| *old_name == key)
        {
            Some((old_name, new_name)) => {
                let inverted = INVERTED_GAME_RULES.contains(old_name);
                rules.insert(*new_name, convert_rule_value(&value, inverted));
            }
            None => {
                rules.insert(key, value);
            }
        }
    }
    data.insert("game_rules", rules);
}

pub fn legacy_dragon_fight_3459(root: &mut Compound) {
    let Some(data) = data_mut(root) else {
        return;
    };
    if data.contains_key("DragonFight") {
        return;
    }
    let Some(mut fight) = data
        .get_compound("DimensionData")
        .and_then(|dimensions| dimensions.get_compound("1"))
        .and_then(|end| end.get_compound("DragonFight"))
        .cloned()
    else {
        return;
    };
    if let Some(Tag::Compound(location)) = fight.get("ExitPortalLocation")
        && let (Some(x), Some(y), Some(z)) = (
            location.get_i32("X"),
            location.get_i32("Y"),
            location.get_i32("Z"),
        )
    {
        fight.insert("ExitPortalLocation", Tag::IntArray(vec![x, y, z]));
    }
    data.insert("DragonFight", fight);
}

pub fn legacy_world_border_4544(root: &mut Compound) {
    let Some(data) = data_mut(root) else {
        return;
    };
    if !BORDER_KEYS.iter().any(|key| data.contains_key(key)) {
        return;
    }
    let double = |data: &Compound, key: &str, default: f64| data.get_f64(key).unwrap_or(default);
    let border = Compound::new()
        .with("center_x", double(data, "BorderCenterX", 0.0))
        .with("center_z", double(data, "BorderCenterZ", 0.0))
        .with(
            "size",
            double(data, "BorderSize", DEFAULT_WORLD_BORDER_SIZE),
        )
        .with("lerp_time", data.get_i64("BorderSizeLerpTime").unwrap_or(0))
        .with("lerp_target", double(data, "BorderSizeLerpTarget", 0.0))
        .with("safe_zone", double(data, "BorderSafeZone", 5.0))
        .with(
            "damage_per_block",
            double(data, "BorderDamagePerBlock", 0.2),
        )
        .with(
            "warning_blocks",
            data.get_i32("BorderWarningBlocks").unwrap_or(5),
        )
        .with(
            "warning_time",
            data.get_i32("BorderWarningTime").unwrap_or(15),
        );
    for key in BORDER_KEYS {
        data.remove(key);
    }
    data.insert("world_border", border);
}

pub fn world_spawn_4548(root: &mut Compound) {
    let Some(data) = data_mut(root) else {
        return;
    };
    if !SPAWN_KEYS.iter().any(|key| data.contains_key(key)) {
        return;
    }
    let coordinate = |data: &Compound, key: &str| data.get_i32(key).unwrap_or(0);
    let pos = vec![
        coordinate(data, "SpawnX"),
        coordinate(data, "SpawnY"),
        coordinate(data, "SpawnZ"),
    ];
    let yaw = data.get_f64("SpawnAngle").unwrap_or(0.0) as f32;
    for key in SPAWN_KEYS {
        data.remove(key);
    }
    data.insert(
        "spawn",
        Compound::new()
            .with("dimension", OVERWORLD)
            .with("pos", Tag::IntArray(pos))
            .with("yaw", yaw)
            .with("pitch", 0.0f32),
    );
}

pub fn border_warning_time_4657(root: &mut Compound) {
    if let Some(border) = data_mut(root).and_then(|data| compound_mut(data, "world_border")) {
        scale_warning_time(border);
    }
}

fn scale_warning_time(border: &mut Compound) {
    let seconds = border.get_i32("warning_time").unwrap_or(15);
    border.insert("warning_time", seconds.saturating_mul(20));
}

pub fn saved_border_warning_time_4657(root: &mut Compound) {
    if let Some(data) = compound_mut(root, "data")
        && data.contains_key("warning_time")
        && data.contains_key("center_x")
    {
        scale_warning_time(data);
    }
}

pub fn day_time_to_clock_4766(root: &mut Compound) {
    let Some(data) = data_mut(root) else {
        return;
    };
    let Some(ticks) = data.remove("DayTime").and_then(|time| time.as_i64()) else {
        return;
    };
    let clock = Compound::new()
        .with("total_ticks", ticks)
        .with("paused", false);
    data.insert("world_clocks", Compound::new().with(OVERWORLD, clock));
}

pub fn difficulty_settings_4771(root: &mut Compound) {
    let Some(data) = data_mut(root) else {
        return;
    };
    let Some(difficulty) = data.remove("Difficulty") else {
        return;
    };
    let difficulty = match difficulty.as_i64().unwrap_or(2) {
        0 => "peaceful",
        1 => "easy",
        3 => "hard",
        _ => "normal",
    };
    let hardcore = data
        .remove("hardcore")
        .is_some_and(|value| parse_flag(&value));
    let locked = data
        .remove("DifficultyLocked")
        .is_some_and(|value| parse_flag(&value));
    data.insert(
        "difficulty_settings",
        Compound::new()
            .with("difficulty", difficulty)
            .with("hardcore", hardcore)
            .with("locked", locked),
    );
}

fn fix_dragon_fight(fight: &mut Compound) {
    for (from, to) in [
        ("NeedsStateScanning", "needs_state_scanning"),
        ("DragonKilled", "dragon_killed"),
        ("PreviouslyKilled", "previously_killed"),
        ("Dragon", "dragon_uuid"),
        ("ExitPortalLocation", "exit_portal_location"),
        ("Gateways", "gateways"),
    ] {
        rename_key(fight, from, to);
    }
    if let Some(respawning) = fight.remove("IsRespawning")
        && parse_flag(&respawning)
    {
        fight.insert("respawn_stage", "start");
        fight.insert("respawn_time", 0);
    }
}

fn fix_scheduled_event(event: &mut Compound) {
    rename_key(event, "Name", "id");
    rename_key(event, "TriggerTime", "trigger_time");
    if let Some(mut callback) = event.remove("Callback") {
        if let Some(callback) = callback.as_compound_mut() {
            rename_key(callback, "Type", "type");
            rename_key(callback, "Name", "id");
        }
        event.insert("callback", callback);
    }
}

pub fn split_level_data_4771(root: &mut Compound) {
    let Some(data) = data_mut(root) else {
        return;
    };
    if let Some(Tag::Compound(mut fight)) = data.remove("DragonFight") {
        fix_dragon_fight(&mut fight);
        data.insert("dragon_fight", fight);
    }
    if TRADER_KEYS.iter().any(|key| data.contains_key(key)) {
        let trader = Compound::new()
            .with(
                "spawn_delay",
                data.get_i32("WanderingTraderSpawnDelay").unwrap_or(24_000),
            )
            .with(
                "spawn_chance",
                data.get_i32("WanderingTraderSpawnChance").unwrap_or(25),
            );
        data.insert("wandering_trader_migration_data", trader);
        for key in TRADER_KEYS {
            data.remove(key);
        }
    }
    if WEATHER_KEYS.iter().any(|key| data.contains_key(key)) {
        let int = |data: &Compound, key: &str| data.get_i32(key).unwrap_or(0);
        let flag = |data: &Compound, key: &str| data.get_bool(key).unwrap_or(false);
        let weather = Compound::new()
            .with("clear_weather_time", int(data, "clearWeatherTime"))
            .with("rain_time", int(data, "rainTime"))
            .with("thunder_time", int(data, "thunderTime"))
            .with("raining", flag(data, "raining"))
            .with("thundering", flag(data, "thundering"));
        for key in WEATHER_KEYS {
            data.remove(key);
        }
        data.insert("weather_data", weather);
    }
    if let Some(Tag::List(mut events)) = data.remove("ScheduledEvents") {
        for event in &mut events {
            if let Some(event) = event.as_compound_mut() {
                fix_scheduled_event(event);
            }
        }
        data.insert(
            "scheduled_events",
            Compound::new().with("events", Tag::List(events)),
        );
    }
    if let Some(Tag::Compound(mut settings)) = data.remove("WorldGenSettings") {
        rename_key(&mut settings, "generate_features", "generate_structures");
        data.insert("world_gen_settings", settings);
    }
}

pub fn forced_chunks_to_tickets_4297(root: &mut Compound) {
    let Some(data) = compound_mut(root, "data") else {
        return;
    };
    let Some(Tag::LongArray(forced)) = data.remove("Forced") else {
        return;
    };
    let tickets = forced
        .into_iter()
        .map(|chunk| {
            Tag::Compound(
                Compound::new()
                    .with("type", "minecraft:forced")
                    .with("level", 31)
                    .with("ticks_left", 0i64)
                    .with("chunk_pos", chunk),
            )
        })
        .collect();
    data.insert("tickets", Tag::List(tickets));
}

pub fn ticket_chunk_pos_4309(root: &mut Compound) {
    let Some(tickets) =
        compound_mut(root, "data").and_then(|data| super::nbt::list_mut(data, "tickets"))
    else {
        return;
    };
    for ticket in tickets {
        let Some(ticket) = ticket.as_compound_mut() else {
            continue;
        };
        if let Some(Tag::Long(packed)) = ticket.get("chunk_pos") {
            let x = (*packed & 0xFFFF_FFFF) as u32 as i32;
            let z = ((*packed >> 32) & 0xFFFF_FFFF) as u32 as i32;
            ticket.insert("chunk_pos", Tag::IntArray(vec![x, z]));
        }
    }
}

pub fn remove_unpopulated_poi_types_3097(root: &mut Compound) {
    let Some(sections) = compound_mut(root, "Sections") else {
        return;
    };
    let keys: Vec<String> = sections.iter().map(|(key, _)| key.to_owned()).collect();
    for key in keys {
        let Some(records) =
            compound_mut(sections, &key).and_then(|s| super::nbt::list_mut(s, "Records"))
        else {
            continue;
        };
        records.retain(|record| {
            !matches!(
                record
                    .as_compound()
                    .and_then(|record| record.get_str("type")),
                Some("minecraft:unemployed" | "minecraft:nitwit")
            )
        });
    }
}

pub fn player_respawn_4548(root: &mut Compound) {
    let Some(respawn) = compound_mut(root, "respawn") else {
        return;
    };
    if !respawn.contains_key("dimension") {
        respawn.insert("dimension", OVERWORLD);
    }
    if let Some(angle) = respawn.remove("angle") {
        respawn.insert("yaw", angle.as_f64().unwrap_or(0.0) as f32);
    }
    respawn.insert("pitch", 0.0f32);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(data: Compound) -> Compound {
        Compound::new().with("Data", data)
    }

    fn data(root: &Compound) -> &Compound {
        root.get_compound("Data").unwrap()
    }

    #[test]
    fn game_rules_get_registry_names_and_typed_values() {
        let mut root = level(
            Compound::new().with(
                "GameRules",
                Compound::new()
                    .with("doDaylightCycle", "false")
                    .with("randomTickSpeed", "5")
                    .with("disableRaids", "true")
                    .with("keepInventory", "true")
                    .with("doFireTick", "true")
                    .with("spawnChunkRadius", "2"),
            ),
        );
        game_rule_registry_4658(&mut root);
        let rules = data(&root).get_compound("game_rules").unwrap();
        assert_eq!(rules.get("minecraft:advance_time"), Some(&Tag::Byte(0)));
        assert_eq!(rules.get("minecraft:random_tick_speed"), Some(&Tag::Int(5)));
        assert_eq!(rules.get("minecraft:raids"), Some(&Tag::Byte(0)));
        assert_eq!(rules.get("minecraft:keep_inventory"), Some(&Tag::Byte(1)));
        assert!(!rules.contains_key("minecraft:fire_spread_radius_around_player"));
        assert!(!rules.contains_key("spawnChunkRadius") && !rules.contains_key("doFireTick"));
        assert!(!data(&root).contains_key("GameRules"));
    }

    #[test]
    fn fire_ticks_become_a_spread_radius() {
        let mut root =
            level(Compound::new().with("GameRules", Compound::new().with("doFireTick", "false")));
        game_rule_registry_4658(&mut root);
        assert_eq!(
            data(&root)
                .get_compound("game_rules")
                .unwrap()
                .get_i32("minecraft:fire_spread_radius_around_player"),
            Some(0)
        );
    }

    #[test]
    fn the_spawn_and_clock_fields_are_regrouped() {
        let mut root = level(
            Compound::new()
                .with("SpawnX", 1)
                .with("SpawnY", 2)
                .with("SpawnZ", 3)
                .with("SpawnAngle", 90.0f32)
                .with("DayTime", 6000i64)
                .with("Time", 7000i64),
        );
        world_spawn_4548(&mut root);
        day_time_to_clock_4766(&mut root);
        let spawn = data(&root).get_compound("spawn").unwrap();
        assert_eq!(spawn.get_int_array("pos"), Some(&[1, 2, 3][..]));
        assert_eq!(spawn.get_f64("yaw"), Some(90.0));
        assert_eq!(spawn.get_str("dimension"), Some("minecraft:overworld"));
        assert!(!data(&root).contains_key("SpawnX") && !data(&root).contains_key("DayTime"));
        let clock = data(&root)
            .get_compound("world_clocks")
            .unwrap()
            .get_compound("minecraft:overworld")
            .unwrap();
        assert_eq!(clock.get_i64("total_ticks"), Some(6000));
        assert_eq!(data(&root).get_i64("Time"), Some(7000));
    }

    #[test]
    fn the_world_border_is_grouped_and_its_warning_time_is_in_ticks() {
        let mut root = level(
            Compound::new()
                .with("BorderCenterX", 10.0)
                .with("BorderSize", 100.0)
                .with("BorderWarningTime", 15),
        );
        legacy_world_border_4544(&mut root);
        border_warning_time_4657(&mut root);
        let border = data(&root).get_compound("world_border").unwrap();
        assert_eq!(border.get_f64("center_x"), Some(10.0));
        assert_eq!(border.get_f64("size"), Some(100.0));
        assert_eq!(border.get_f64("safe_zone"), Some(5.0));
        assert_eq!(border.get_i32("warning_time"), Some(300));
        assert!(!data(&root).contains_key("BorderSize"));
    }

    #[test]
    fn a_saved_world_border_scales_its_warning_time_too() {
        let mut root = Compound::new().with(
            "data",
            Compound::new()
                .with("center_x", 0.0)
                .with("warning_time", 15),
        );
        saved_border_warning_time_4657(&mut root);
        assert_eq!(
            root.get_compound("data").unwrap().get_i32("warning_time"),
            Some(300)
        );
        let mut other = Compound::new().with("data", Compound::new().with("warning_time", 1));
        saved_border_warning_time_4657(&mut other);
        assert_eq!(
            other.get_compound("data").unwrap().get_i32("warning_time"),
            Some(1)
        );
    }

    #[test]
    fn the_level_is_split_into_the_26_saved_data_compounds() {
        let mut root = level(
            Compound::new()
                .with("Difficulty", 3i8)
                .with("hardcore", 1i8)
                .with("DifficultyLocked", 0i8)
                .with("raining", 1i8)
                .with("rainTime", 500)
                .with("clearWeatherTime", 10)
                .with("WanderingTraderSpawnDelay", 12_000)
                .with(
                    "ScheduledEvents",
                    Tag::List(vec![Tag::Compound(
                        Compound::new().with("Name", "ev").with("TriggerTime", 5i64),
                    )]),
                )
                .with(
                    "DragonFight",
                    Compound::new()
                        .with("DragonKilled", 1i8)
                        .with("IsRespawning", 1i8),
                )
                .with(
                    "WorldGenSettings",
                    Compound::new()
                        .with("seed", 7i64)
                        .with("generate_features", 1i8),
                ),
        );
        difficulty_settings_4771(&mut root);
        split_level_data_4771(&mut root);
        let data = data(&root);
        let difficulty = data.get_compound("difficulty_settings").unwrap();
        assert_eq!(difficulty.get_str("difficulty"), Some("hard"));
        assert_eq!(difficulty.get_bool("hardcore"), Some(true));
        let weather = data.get_compound("weather_data").unwrap();
        assert_eq!(weather.get_bool("raining"), Some(true));
        assert_eq!(weather.get_i32("rain_time"), Some(500));
        assert_eq!(weather.get_i32("clear_weather_time"), Some(10));
        let trader = data
            .get_compound("wandering_trader_migration_data")
            .unwrap();
        assert_eq!(trader.get_i32("spawn_delay"), Some(12_000));
        assert_eq!(trader.get_i32("spawn_chance"), Some(25));
        let event = data
            .get_compound("scheduled_events")
            .unwrap()
            .get_list("events")
            .unwrap()[0]
            .as_compound()
            .unwrap();
        assert_eq!(event.get_str("id"), Some("ev"));
        assert_eq!(event.get_i64("trigger_time"), Some(5));
        let fight = data.get_compound("dragon_fight").unwrap();
        assert_eq!(fight.get_bool("dragon_killed"), Some(true));
        assert_eq!(fight.get_str("respawn_stage"), Some("start"));
        let settings = data.get_compound("world_gen_settings").unwrap();
        assert_eq!(settings.get_i64("seed"), Some(7));
        assert!(settings.contains_key("generate_structures"));
        for key in [
            "Difficulty",
            "raining",
            "ScheduledEvents",
            "DragonFight",
            "WorldGenSettings",
        ] {
            assert!(!data.contains_key(key), "{key}");
        }
    }

    #[test]
    fn the_dragon_fight_is_lifted_out_of_the_dimension_data() {
        let mut root = level(Compound::new().with(
            "DimensionData",
            Compound::new().with(
                "1",
                Compound::new().with(
                    "DragonFight",
                    Compound::new().with(
                        "ExitPortalLocation",
                        Compound::new().with("X", 1).with("Y", 2).with("Z", 3),
                    ),
                ),
            ),
        ));
        legacy_dragon_fight_3459(&mut root);
        let location = data(&root)
            .get_compound("DragonFight")
            .unwrap()
            .get_int_array("ExitPortalLocation")
            .unwrap();
        assert_eq!(location, [1, 2, 3]);
    }

    #[test]
    fn forced_chunks_become_tickets_with_unpacked_positions() {
        let packed = (7i64 << 32) | (0xFFFF_FFFDi64);
        let mut root = Compound::new().with(
            "data",
            Compound::new().with("Forced", Tag::LongArray(vec![packed])),
        );
        forced_chunks_to_tickets_4297(&mut root);
        ticket_chunk_pos_4309(&mut root);
        let data = root.get_compound("data").unwrap();
        assert!(!data.contains_key("Forced"));
        let ticket = data.get_list("tickets").unwrap()[0].as_compound().unwrap();
        assert_eq!(ticket.get_str("type"), Some("minecraft:forced"));
        assert_eq!(ticket.get_i32("level"), Some(31));
        assert_eq!(ticket.get_int_array("chunk_pos"), Some(&[-3, 7][..]));
    }

    #[test]
    fn unemployed_and_nitwit_poi_records_are_dropped() {
        let record = |kind: &str| Tag::Compound(Compound::new().with("type", kind));
        let mut root = Compound::new().with(
            "Sections",
            Compound::new().with(
                "0",
                Compound::new().with(
                    "Records",
                    Tag::List(vec![
                        record("minecraft:unemployed"),
                        record("minecraft:nitwit"),
                        record("minecraft:home"),
                    ]),
                ),
            ),
        );
        remove_unpopulated_poi_types_3097(&mut root);
        let records = root
            .get_compound("Sections")
            .unwrap()
            .get_compound("0")
            .unwrap()
            .get_list("Records")
            .unwrap();
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn the_player_respawn_point_gets_a_dimension_and_a_pitch() {
        let mut player = Compound::new().with(
            "respawn",
            Compound::new()
                .with("pos", Tag::IntArray(vec![0, 64, 0]))
                .with("angle", 45.0f32),
        );
        player_respawn_4548(&mut player);
        let respawn = player.get_compound("respawn").unwrap();
        assert_eq!(respawn.get_str("dimension"), Some("minecraft:overworld"));
        assert_eq!(respawn.get_f64("yaw"), Some(45.0));
        assert_eq!(respawn.get_f64("pitch"), Some(0.0));
        assert!(!respawn.contains_key("angle"));
    }
}
