use rspigot_domain::particle::{BLOCK_PARTICLES, Particle, ParticleData};
use rspigot_domain::player::PlayerId;
use rspigot_domain::text::Component;
use rspigot_domain::{Identifier, Vec3};
use rspigot_entities::SoundSource;
use rspigot_items::ItemKind;

use super::args::parse_block_state;
use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, Target, fail, looks_like_coordinate,
    next_argument, parse_vec3, usage,
};
use super::server_commands::{players, reply_err};

const SOURCES: [(&str, SoundSource); 11] = [
    ("master", SoundSource::Master),
    ("music", SoundSource::Music),
    ("record", SoundSource::Records),
    ("weather", SoundSource::Weather),
    ("block", SoundSource::Blocks),
    ("hostile", SoundSource::Hostile),
    ("neutral", SoundSource::Neutral),
    ("player", SoundSource::Players),
    ("ambient", SoundSource::Ambient),
    ("voice", SoundSource::Voice),
    ("ui", SoundSource::Ui),
];

const VIEW_DISTANCE: f64 = 32.0;
const FORCED_VIEW_DISTANCE: f64 = 512.0;

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

fn source_of(name: &str) -> Option<SoundSource> {
    SOURCES
        .iter()
        .find_map(|(label, source)| (*label == name).then_some(*source))
}

fn source_name(source: SoundSource) -> &'static str {
    SOURCES
        .iter()
        .find_map(|(label, candidate)| (*candidate == source).then_some(*label))
        .unwrap_or("master")
}

fn identifier<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    raw: &str,
) -> Result<Identifier, CommandOutcome> {
    let full = if raw.contains(':') {
        raw.to_owned()
    } else {
        format!("minecraft:{raw}")
    };
    Identifier::parse(&full).map_err(|_| fail(ctx, sender, "argument.id.invalid", Vec::new()))
}

fn float<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    raw: &str,
    min: f32,
    max: f32,
) -> Result<f32, CommandOutcome> {
    let Ok(value) = raw.parse::<f32>() else {
        return Err(fail(ctx, sender, "parsing.float.invalid", vec![text(raw)]));
    };
    if value < min {
        return Err(fail(
            ctx,
            sender,
            "argument.float.low",
            vec![text(min), text(value)],
        ));
    }
    if value > max {
        return Err(fail(
            ctx,
            sender,
            "argument.float.big",
            vec![text(max), text(value)],
        ));
    }
    Ok(value)
}

fn optional_position<'a, C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &'a str,
) -> Result<(Vec3, &'a str), CommandOutcome> {
    let Some((first, _)) = next_argument(rest).filter(|(token, _)| looks_like_coordinate(token))
    else {
        return Ok((ctx.sender_origin(sender), rest));
    };
    let incomplete = |ctx: &mut C| fail(ctx, sender, "argument.pos3d.incomplete", Vec::new());
    let (_, after_x) = next_argument(rest).ok_or_else(|| incomplete(ctx))?;
    let (y, after_y) = next_argument(after_x).ok_or_else(|| incomplete(ctx))?;
    let (z, after_z) = next_argument(after_y).ok_or_else(|| incomplete(ctx))?;
    let position = parse_vec3(ctx, sender, [first, y, z]).map_err(|e| reply_err(ctx, sender, e))?;
    Ok((position, after_z))
}

fn playsound<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/playsound <sound> <source> <targets> [pos] [volume] [pitch] [minVolume]";
    let Some((raw_sound, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((raw_source, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let sound = match identifier(ctx, sender, raw_sound) {
        Ok(sound) => sound,
        Err(outcome) => return outcome,
    };
    let Some(source) = source_of(raw_source) else {
        return fail(ctx, sender, "argument.enum.invalid", vec![text(raw_source)]);
    };
    let targets = match players(ctx, sender, selector) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let (position, rest) = match optional_position(ctx, sender, rest) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let mut cursor = rest;
    let mut numbers = [1.0f32, 1.0, 0.0];
    let limits = [(0.0, f32::MAX), (0.0, 2.0), (0.0, 1.0)];
    for (slot, (min, max)) in numbers.iter_mut().zip(limits) {
        let Some((raw, after)) = next_argument(cursor) else {
            break;
        };
        match float(ctx, sender, raw, min, max) {
            Ok(value) => *slot = value,
            Err(outcome) => return outcome,
        }
        cursor = after;
    }
    let [volume, pitch, min_volume] = numbers;
    let range = crate::sound::sound_range(volume);
    let mut heard = Vec::new();
    for player in targets {
        let Some(at) = ctx.player_position(player) else {
            continue;
        };
        if ctx.target_dimension(Target::Player(player)) != ctx.current_dimension() {
            continue;
        }
        let offset = Vec3::new(position.x - at.x, position.y - at.y, position.z - at.z);
        let distance_squared = offset.x * offset.x + offset.y * offset.y + offset.z * offset.z;
        let (spot, loudness) = if distance_squared > range * range {
            if min_volume <= 0.0 {
                continue;
            }
            let distance = distance_squared.sqrt();
            (
                Vec3::new(
                    at.x + offset.x / distance * 2.0,
                    at.y + offset.y / distance * 2.0,
                    at.z + offset.z / distance * 2.0,
                ),
                min_volume,
            )
        } else {
            (position, volume)
        };
        ctx.play_sound_to(player, &sound.to_string(), source, spot, loudness, pitch);
        heard.push(player);
    }
    if heard.is_empty() {
        return fail(ctx, sender, "commands.playsound.failed", Vec::new());
    }
    let (key, second) = match heard.as_slice() {
        [only] => (
            "commands.playsound.success.single",
            ctx.target_name(Target::Player(*only)),
        ),
        many => ("commands.playsound.success.multiple", text(many.len())),
    };
    ctx.record_result(i32::try_from(heard.len()).unwrap_or(i32::MAX));
    ctx.reply(
        sender,
        Component::translatable(key, vec![text(sound), second]),
    );
    CommandOutcome::Success
}

fn stopsound<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/stopsound <targets> [*|<source>] [<sound>]";
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let targets = match players(ctx, sender, selector) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let mut source = None;
    let mut sound = None;
    if let Some((raw_source, rest)) = next_argument(rest) {
        if raw_source != "*" {
            let Some(found) = source_of(raw_source) else {
                return fail(ctx, sender, "argument.enum.invalid", vec![text(raw_source)]);
            };
            source = Some(found);
        }
        if let Some((raw_sound, _)) = next_argument(rest) {
            match identifier(ctx, sender, raw_sound) {
                Ok(found) => sound = Some(found.to_string()),
                Err(outcome) => return outcome,
            }
        }
    }
    for player in &targets {
        ctx.stop_sound_for(*player, source, sound.clone());
    }
    let (key, arguments) = match (source, &sound) {
        (Some(source), Some(sound)) => (
            "commands.stopsound.success.source.sound",
            vec![text(sound), text(source_name(source))],
        ),
        (Some(source), None) => (
            "commands.stopsound.success.source.any",
            vec![text(source_name(source))],
        ),
        (None, Some(sound)) => (
            "commands.stopsound.success.sourceless.sound",
            vec![text(sound)],
        ),
        (None, None) => ("commands.stopsound.success.sourceless.any", Vec::new()),
    };
    ctx.record_result(i32::try_from(targets.len()).unwrap_or(i32::MAX));
    ctx.reply(sender, Component::translatable(key, arguments));
    CommandOutcome::Success
}

fn registered_particle(name: &str) -> Option<&'static str> {
    rspigot_data::registries::static_entries("minecraft:particle_type")?
        .iter()
        .copied()
        .find(|entry| *entry == name)
}

fn particle_options<'a, C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    kind: &'static str,
    rest: &'a str,
) -> Result<(ParticleData, &'a str), CommandOutcome> {
    if BLOCK_PARTICLES.contains(&kind) {
        let Some((raw, rest)) = next_argument(rest) else {
            return Err(usage(ctx, sender, "/particle <name> <block> ..."));
        };
        return match parse_block_state(raw) {
            Ok((state, _)) => Ok((ParticleData::Block(state), rest)),
            Err(error) => Err(reply_err(ctx, sender, error)),
        };
    }
    match kind {
        "minecraft:item" => {
            let Some((raw, rest)) = next_argument(rest) else {
                return Err(usage(ctx, sender, "/particle item <item> ..."));
            };
            let Some(item) = ItemKind::from_name(raw) else {
                return Err(fail(
                    ctx,
                    sender,
                    "argument.item.id.invalid",
                    vec![text(raw)],
                ));
            };
            Ok((ParticleData::Item(item.id()), rest))
        }
        "minecraft:dust" => {
            let mut values = [0.0f32; 4];
            let mut cursor = rest;
            for (index, slot) in values.iter_mut().enumerate() {
                let Some((raw, after)) = next_argument(cursor) else {
                    return Err(usage(ctx, sender, "/particle dust <r> <g> <b> <scale> ..."));
                };
                let (min, max) = if index == 3 { (0.01, 4.0) } else { (0.0, 1.0) };
                *slot = float(ctx, sender, raw, min, max)?;
                cursor = after;
            }
            let channel = |value: f32| (value * 255.0).round() as u32;
            let color = (channel(values[0]) << 16) | (channel(values[1]) << 8) | channel(values[2]);
            Ok((
                ParticleData::Dust {
                    color,
                    scale: values[3],
                },
                cursor,
            ))
        }
        _ if rspigot_domain::particle::PARTICLES_WITH_DATA.contains(&kind) => Err(fail(
            ctx,
            sender,
            "argument.particle.unknown",
            vec![text(kind)],
        )),
        _ => Ok((ParticleData::None, rest)),
    }
}

fn particle<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/particle <name> [pos] [delta] [speed] [count] [force|normal] [viewers]";
    let Some((raw_name, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let name = match identifier(ctx, sender, raw_name) {
        Ok(name) => name.to_string(),
        Err(outcome) => return outcome,
    };
    let Some(kind) = registered_particle(&name) else {
        return fail(ctx, sender, "argument.particle.unknown", vec![text(name)]);
    };
    let (data, rest) = match particle_options(ctx, sender, kind, rest) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let mut position = ctx.sender_origin(sender);
    let mut spread = Vec3::ZERO;
    let mut speed = 0.0f32;
    let mut count = 0i32;
    let mut force = false;
    let mut viewers = None;
    let mut cursor = rest;
    if next_argument(cursor).is_some() {
        match optional_position(ctx, sender, cursor) {
            Ok((found, after)) => {
                position = found;
                cursor = after;
            }
            Err(outcome) => return outcome,
        }
        let mut raw = [""; 3];
        for slot in &mut raw {
            let Some((token, after)) = next_argument(cursor) else {
                return fail(ctx, sender, "argument.pos3d.incomplete", Vec::new());
            };
            *slot = token;
            cursor = after;
        }
        spread = match parse_delta(ctx, sender, raw) {
            Ok(found) => found,
            Err(outcome) => return outcome,
        };
        let Some((raw_speed, after)) = next_argument(cursor) else {
            return usage(ctx, sender, help);
        };
        speed = match float(ctx, sender, raw_speed, 0.0, f32::MAX) {
            Ok(value) => value,
            Err(outcome) => return outcome,
        };
        let Some((raw_count, after)) = next_argument(after) else {
            return usage(ctx, sender, help);
        };
        count = match raw_count.parse::<i32>() {
            Ok(value) if value >= 0 => value,
            Ok(value) => {
                return fail(
                    ctx,
                    sender,
                    "argument.integer.low",
                    vec![text(0), text(value)],
                );
            }
            Err(_) => {
                return fail(ctx, sender, "parsing.int.invalid", vec![text(raw_count)]);
            }
        };
        cursor = after;
        if let Some((mode, after)) = next_argument(cursor) {
            match mode {
                "force" => force = true,
                "normal" => {}
                _ => return usage(ctx, sender, help),
            }
            cursor = after;
            if let Some((selector, _)) = next_argument(cursor) {
                viewers = Some(match players(ctx, sender, selector) {
                    Ok(found) => found,
                    Err(outcome) => return outcome,
                });
            }
        }
    }
    let audience = viewers.unwrap_or_else(|| ctx.player_ids());
    let reach = if force {
        FORCED_VIEW_DISTANCE
    } else {
        VIEW_DISTANCE
    };
    let shown = Particle {
        kind,
        data,
        position,
        count,
        spread,
        speed: f64::from(speed),
    };
    let mut seen = 0;
    for player in audience {
        if reaches(ctx, player, position, reach) && ctx.send_particle_to(player, shown, force) {
            seen += 1;
        }
    }
    if seen == 0 {
        return fail(ctx, sender, "commands.particle.failed", Vec::new());
    }
    ctx.record_result(seen);
    ctx.reply(
        sender,
        Component::translatable("commands.particle.success", vec![text(name)]),
    );
    CommandOutcome::Success
}

fn parse_delta<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    raw: [&str; 3],
) -> Result<Vec3, CommandOutcome> {
    let mut values = [0.0f64; 3];
    for (slot, token) in values.iter_mut().zip(raw) {
        *slot = token
            .parse::<f64>()
            .map_err(|_| fail(ctx, sender, "parsing.double.invalid", vec![text(token)]))?;
    }
    Ok(Vec3::new(values[0], values[1], values[2]))
}

fn reaches<C: CommandContext>(ctx: &C, player: PlayerId, at: Vec3, distance: f64) -> bool {
    if ctx.target_dimension(Target::Player(player)) != ctx.current_dimension() {
        return false;
    }
    ctx.player_position(player).is_some_and(|position| {
        let block = Vec3::new(position.x.floor(), position.y.floor(), position.z.floor());
        let dx = block.x + 0.5 - at.x;
        let dy = block.y + 0.5 - at.y;
        let dz = block.z + 0.5 - at.z;
        dx * dx + dy * dy + dz * dz < distance * distance
    })
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    label: &str,
    rest: &str,
) -> CommandOutcome {
    match label {
        "playsound" => playsound(ctx, sender, rest),
        "stopsound" => stopsound(ctx, sender, rest),
        _ => particle(ctx, sender, rest),
    }
}
