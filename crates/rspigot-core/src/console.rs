use rspigot_domain::text::Component;

fn english(key: &str) -> Option<&'static str> {
    Some(match key {
        "commands.seed.success" => "Seed: %s",
        "command.unknown.command" => "Unknown or incomplete command, see below for error",
        "command.context.here" => "<--[HERE]",
        "multiplayer.player.joined" => "%s joined the game",
        "multiplayer.player.left" => "%s left the game",
        "argument.entity.notfound.player" => "No player was found",
        "commands.gamemode.success.self" => "Set own game mode to %s",
        "commands.gamemode.success.other" => "Set %s's game mode to %s",
        "gameMode.changed" => "Your game mode has been updated to %s",
        "commands.teleport.success.location.single" => "Teleported %s to %s, %s, %s",
        "commands.time.set" => "Set the time to %s",
        "commands.time.query" => "The time is %s",
        "commands.give.success.single" => "Gave %s %s to %s",
        "argument.item.id.invalid" => "Unknown item '%s'",
        "commands.clear.success.single" => "Removed %s item(s) from player %s",
        "commands.kick.success" => "Kicked %s: %s",
        "commands.op.failed" => "Nothing changed. The player is already an operator",
        "commands.deop.failed" => "Nothing changed. The player is not an operator",
        "commands.op.success" => "Made %s a server operator",
        "commands.deop.success" => "Made %s no longer a server operator",
        "commands.difficulty.query" => "The difficulty is %s",
        "commands.difficulty.success" => "The difficulty has been set to %s",
        "commands.weather.set.clear" => "Set the weather to clear",
        "commands.weather.set.rain" => "Set the weather to rain",
        "commands.weather.set.thunder" => "Set the weather to rain and thunder",
        "commands.setworldspawn.success" => "Set the world spawn point to %s, %s, %s [%s]",
        "commands.list.players" => "There are %s of a max of %s players online: %s",
        "options.difficulty.peaceful" => "Peaceful",
        "options.difficulty.easy" => "Easy",
        "options.difficulty.normal" => "Normal",
        "options.difficulty.hard" => "Hard",
        "gameMode.survival" => "Survival Mode",
        "gameMode.creative" => "Creative Mode",
        "gameMode.adventure" => "Adventure Mode",
        "gameMode.spectator" => "Spectator Mode",
        "multiplayer.disconnect.duplicate_login" => "You logged in from another location",
        "multiplayer.disconnect.server_full" => "The server is full!",
        "multiplayer.disconnect.server_shutdown" => "Server closed",
        "multiplayer.disconnect.invalid_player_movement" => "Invalid move player packet received",
        "multiplayer.disconnect.illegal_characters" => "Illegal characters in chat",
        "multiplayer.disconnect.transfers_disabled" => "This server does not accept transfers",
        "multiplayer.disconnect.invalid_player_data" => "Invalid player data",
        "multiplayer.disconnect.unverified_username" => "Failed to verify username!",
        "multiplayer.disconnect.banned" => "You are banned from this server",
        "multiplayer.disconnect.banned.reason" => "You are banned from this server.\nReason: %s",
        "multiplayer.disconnect.banned.expiration" => "\nYour ban will be removed on %s",
        "multiplayer.disconnect.banned.reason.default" => "Banned by an operator.",
        "multiplayer.disconnect.banned_ip.reason" => {
            "Your IP address is banned from this server.\nReason: %s"
        }
        "multiplayer.disconnect.banned_ip.expiration" => "\nYour ban will be removed on %s",
        "multiplayer.disconnect.ip_banned" => "You have been IP banned from this server",
        "commands.ban.success" => "Banned %s: %s",
        "commands.ban.failed" => "Nothing changed. The player is already banned",
        "commands.banip.success" => "Banned IP %s: %s",
        "commands.banip.info" => "This ban affects %s player(s): %s",
        "commands.banip.invalid" => "Invalid IP address or unknown player",
        "commands.banip.failed" => "Nothing changed. That IP is already banned",
        "commands.pardon.success" => "Unbanned %s",
        "commands.pardon.failed" => "Nothing changed. The player isn't banned",
        "commands.pardonip.success" => "Unbanned IP %s",
        "commands.pardonip.invalid" => "Invalid IP address",
        "commands.pardonip.failed" => "Nothing changed. That IP isn't banned",
        "commands.banlist.none" => "There are no bans",
        "commands.banlist.list" => "There are %s ban(s):",
        "commands.banlist.entry" => "%s was banned by %s: %s",
        "disconnect.timeout" => "Timed out",
        "commands.whitelist.enabled" => "Whitelist is now turned on",
        "commands.whitelist.disabled" => "Whitelist is now turned off",
        "commands.whitelist.alreadyOn" => "Whitelist is already turned on",
        "commands.whitelist.alreadyOff" => "Whitelist is already turned off",
        "commands.whitelist.add.success" => "Added %s to the whitelist",
        "commands.whitelist.add.failed" => "Player is already whitelisted",
        "commands.whitelist.remove.success" => "Removed %s from the whitelist",
        "commands.whitelist.remove.failed" => "Player is not whitelisted",
        "commands.whitelist.list" => "There are %s whitelisted player(s): %s",
        "commands.whitelist.none" => "There are no whitelisted players",
        "commands.whitelist.reloaded" => "Reloaded the whitelist",
        "commands.effect.give.success.single" => "Applied effect %s to %s",
        "commands.effect.give.success.multiple" => "Applied effect %s to %s targets",
        "commands.effect.give.failed" => {
            "Unable to apply this effect (target is either immune to effects, or has something stronger)"
        }
        "commands.effect.clear.everything.success.single" => "Removed every effect from %s",
        "commands.effect.clear.everything.success.multiple" => {
            "Removed every effect from %s targets"
        }
        "commands.effect.clear.everything.failed" => "Target has no effects to remove",
        "commands.effect.clear.specific.success.single" => "Removed effect %s from %s",
        "commands.effect.clear.specific.success.multiple" => "Removed effect %s from %s targets",
        "commands.effect.clear.specific.failed" => "Target doesn't have the requested effect",
        "commands.summon.failed" => "Unable to summon entity",
        "commands.summon.failed.peaceful" => "Monsters cannot be summoned in Peaceful difficulty",
        "commands.summon.failed.uuid" => "Unable to summon entity due to duplicate UUIDs",
        "commands.summon.invalidPosition" => "Invalid position for summon",
        "argument.resource.not_found" => "Can't find element '%s' of type '%s'",
        "argument.integer.big" => "Integer must not be more than %s: found %s",
        "argument.integer.low" => "Integer must not be less than %s: found %s",
        "argument.entity.notfound.entity" => "No entity was found",
        "argument.entity.selector.unknown" => "Unknown selector type '%s'",
        "argument.entity.options.unknown" => "Unknown option '%s'",
        "parsing.int.invalid" => "Invalid integer '%s'",
        "effect.minecraft.speed" => "Speed",
        "effect.minecraft.slowness" => "Slowness",
        "effect.minecraft.haste" => "Haste",
        "effect.minecraft.mining_fatigue" => "Mining Fatigue",
        "effect.minecraft.strength" => "Strength",
        "effect.minecraft.instant_health" => "Instant Health",
        "effect.minecraft.instant_damage" => "Instant Damage",
        "effect.minecraft.jump_boost" => "Jump Boost",
        "effect.minecraft.nausea" => "Nausea",
        "effect.minecraft.regeneration" => "Regeneration",
        "effect.minecraft.resistance" => "Resistance",
        "effect.minecraft.fire_resistance" => "Fire Resistance",
        "effect.minecraft.water_breathing" => "Water Breathing",
        "effect.minecraft.invisibility" => "Invisibility",
        "effect.minecraft.blindness" => "Blindness",
        "effect.minecraft.night_vision" => "Night Vision",
        "effect.minecraft.hunger" => "Hunger",
        "effect.minecraft.weakness" => "Weakness",
        "effect.minecraft.poison" => "Poison",
        "effect.minecraft.wither" => "Wither",
        "effect.minecraft.health_boost" => "Health Boost",
        "effect.minecraft.absorption" => "Absorption",
        "effect.minecraft.saturation" => "Saturation",
        "effect.minecraft.glowing" => "Glowing",
        "effect.minecraft.levitation" => "Levitation",
        "effect.minecraft.luck" => "Luck",
        "effect.minecraft.unluck" => "Bad Luck",
        "effect.minecraft.slow_falling" => "Slow Falling",
        "effect.minecraft.conduit_power" => "Conduit Power",
        "effect.minecraft.dolphins_grace" => "Dolphin's Grace",
        "effect.minecraft.bad_omen" => "Bad Omen",
        "effect.minecraft.hero_of_the_village" => "Hero of the Village",
        "effect.minecraft.darkness" => "Darkness",
        "effect.minecraft.trial_omen" => "Trial Omen",
        "effect.minecraft.raid_omen" => "Raid Omen",
        "effect.minecraft.wind_charged" => "Wind Charged",
        "effect.minecraft.weaving" => "Weaving",
        "effect.minecraft.oozing" => "Oozing",
        "effect.minecraft.infested" => "Infested",
        "effect.minecraft.breath_of_the_nautilus" => "Breath of the Nautilus",
        "parsing.bool.invalid" => "Invalid boolean: expected 'true' or 'false' but found '%s'",
        _ => return None,
    })
}

const TITLES: [(&str, &str); 21] = [
    ("container.chest", "Chest"),
    ("container.chestDouble", "Large Chest"),
    ("container.enderchest", "Ender Chest"),
    ("container.barrel", "Barrel"),
    ("container.hopper", "Item Hopper"),
    ("container.dispenser", "Dispenser"),
    ("container.dropper", "Dropper"),
    ("container.crafting", "Crafting"),
    ("container.grindstone_title", "Repair & Disenchant"),
    ("container.stonecutter", "Stonecutter"),
    ("container.cartography_table", "Cartography Table"),
    ("container.loom", "Loom"),
    ("container.beacon", "Beacon"),
    ("container.lectern", "Lectern"),
    ("container.upgrade", "Upgrade Gear"),
    ("container.repair", "Repair & Name"),
    ("container.furnace", "Furnace"),
    ("container.blast_furnace", "Blast Furnace"),
    ("container.smoker", "Smoker"),
    ("container.shulkerBox", "Shulker Box"),
    ("container.inventory", "Inventory"),
];

pub(crate) fn english_title(key: &str) -> String {
    TITLES
        .iter()
        .find(|(known, _)| *known == key)
        .map_or_else(|| key.to_owned(), |(_, text)| (*text).to_owned())
}

pub fn console_text(component: &Component) -> String {
    component.render(&english)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_message_reads_like_vanilla_console() {
        let joined =
            Component::translatable("multiplayer.player.joined", vec![Component::text("Probe")]);
        assert_eq!(console_text(&joined), "Probe joined the game");
    }
}
