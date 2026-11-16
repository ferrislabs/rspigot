use rspigot_domain::Vec3;
use rspigot_domain::text::Component;

use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, Target, fail, next_argument, parse_time,
    parse_vec3, select, select_one, usage,
};
use super::execute::{look_at, parse_angle};
use super::server_commands::{players, reply_err};
use super::source::Anchor;
use crate::events::{Hand, SwingStyle};

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

fn ride<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/ride <target> mount <vehicle> | /ride <target> dismount";
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let rider = match select_one(ctx, sender, selector) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let rider_name = ctx.target_name(rider);
    match action {
        "dismount" => {
            let Some(vehicle) = ctx.vehicle_of(rider) else {
                return fail(ctx, sender, "commands.ride.not_riding", vec![rider_name]);
            };
            let vehicle_name = ctx.target_name(vehicle);
            ctx.stop_riding_target(rider);
            ctx.record_result(1);
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.ride.dismount.success",
                    vec![rider_name, vehicle_name],
                ),
            );
            CommandOutcome::Success
        }
        "mount" => {
            let Some((vehicle_selector, _)) = next_argument(rest) else {
                return usage(ctx, sender, help);
            };
            let vehicle = match select_one(ctx, sender, vehicle_selector) {
                Ok(found) => found,
                Err(outcome) => return outcome,
            };
            let vehicle_name = ctx.target_name(vehicle);
            if let Some(current) = ctx.vehicle_of(rider) {
                let current_name = ctx.target_name(current);
                return fail(
                    ctx,
                    sender,
                    "commands.ride.already_riding",
                    vec![rider_name, current_name],
                );
            }
            if matches!(vehicle, Target::Player(_)) {
                return fail(
                    ctx,
                    sender,
                    "commands.ride.mount.failure.cant_ride_players",
                    Vec::new(),
                );
            }
            if ctx.would_loop(rider, vehicle) {
                return fail(ctx, sender, "commands.ride.mount.failure.loop", Vec::new());
            }
            if ctx.target_dimension(rider) != ctx.target_dimension(vehicle) {
                return fail(
                    ctx,
                    sender,
                    "commands.ride.mount.failure.wrong_dimension",
                    Vec::new(),
                );
            }
            if !ctx.start_riding_target(rider, vehicle) {
                return fail(
                    ctx,
                    sender,
                    "commands.ride.mount.failure.generic",
                    vec![rider_name, vehicle_name],
                );
            }
            ctx.record_result(1);
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.ride.mount.success",
                    vec![rider_name, vehicle_name],
                ),
            );
            CommandOutcome::Success
        }
        _ => usage(ctx, sender, help),
    }
}

fn rotate<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/rotate <target> <rotation> | facing <location> | facing entity <entity> [anchor]";
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((first, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let target = match select_one(ctx, sender, selector) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let rotation = if first == "facing" {
        let Some((second, after)) = next_argument(rest) else {
            return usage(ctx, sender, help);
        };
        let Some(position) = ctx.target_position(target) else {
            return fail(ctx, sender, "argument.entity.notfound.entity", Vec::new());
        };
        let anchor = ctx.command_source(sender).anchor;
        let from = anchored(ctx, target, position, anchor);
        let looked = if second == "entity" {
            let Some((entity_selector, after)) = next_argument(after) else {
                return usage(ctx, sender, help);
            };
            let facing = match select_one(ctx, sender, entity_selector) {
                Ok(found) => found,
                Err(outcome) => return outcome,
            };
            let facing_anchor = match next_argument(after) {
                None => Anchor::Feet,
                Some((raw, _)) => match Anchor::parse(raw) {
                    Some(found) => found,
                    None => {
                        return fail(ctx, sender, "argument.anchor.invalid", vec![text(raw)]);
                    }
                },
            };
            let Some(at) = ctx.target_position(facing) else {
                return fail(ctx, sender, "argument.entity.notfound.entity", Vec::new());
            };
            anchored(ctx, facing, at, facing_anchor)
        } else {
            let Some((y, after)) = next_argument(after) else {
                return fail(ctx, sender, "argument.pos3d.incomplete", Vec::new());
            };
            let Some((z, _)) = next_argument(after) else {
                return fail(ctx, sender, "argument.pos3d.incomplete", Vec::new());
            };
            match parse_vec3(ctx, sender, [second, y, z]) {
                Ok(found) => found,
                Err(error) => return reply_err(ctx, sender, error),
            }
        };
        look_at(from, looked)
    } else {
        let Some((pitch, _)) = next_argument(rest) else {
            return usage(ctx, sender, help);
        };
        let base = ctx.sender_rotation(sender);
        let yaw = match parse_angle(first, base.yaw) {
            Ok(value) => value,
            Err(error) => return reply_err(ctx, sender, error),
        };
        let pitch = match parse_angle(pitch, base.pitch) {
            Ok(value) => value,
            Err(error) => return reply_err(ctx, sender, error),
        };
        rspigot_domain::Rotation::new(yaw, pitch)
    };
    let who = ctx.target_name(target);
    ctx.rotate_to(target, rotation.yaw, rotation.pitch);
    ctx.record_result(1);
    ctx.reply(
        sender,
        Component::translatable("commands.rotate.success", vec![who]),
    );
    CommandOutcome::Success
}

fn anchored<C: CommandContext>(ctx: &C, target: Target, position: Vec3, anchor: Anchor) -> Vec3 {
    match anchor {
        Anchor::Feet => position,
        Anchor::Eyes => Vec3::new(
            position.x,
            position.y + ctx.target_eye_height(target),
            position.z,
        ),
    }
}

fn spectate<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/spectate [<target> [<player>]]";
    let target = match next_argument(rest) {
        None => None,
        Some((selector, _)) => match select_one(ctx, sender, selector) {
            Ok(found) => Some(found),
            Err(outcome) => return outcome,
        },
    };
    let viewer = match next_argument(rest).and_then(|(_, after)| next_argument(after)) {
        Some((selector, _)) => match players(ctx, sender, selector) {
            Ok(found) if found.len() == 1 => found[0],
            Ok(_) => return fail(ctx, sender, "argument.player.toomany", Vec::new()),
            Err(outcome) => return outcome,
        },
        None => match sender {
            CommandSender::Player(id) => id,
            CommandSender::Console => {
                return fail(ctx, sender, "permissions.requires.player", Vec::new());
            }
        },
    };
    if next_argument(rest)
        .and_then(|(_, after)| next_argument(after))
        .and_then(|(_, after)| next_argument(after))
        .is_some()
    {
        return usage(ctx, sender, help);
    }
    let viewer_name = ctx.target_name(Target::Player(viewer));
    if target == Some(Target::Player(viewer)) {
        return fail(ctx, sender, "commands.spectate.self", Vec::new());
    }
    if !ctx.is_spectator(viewer) {
        return fail(
            ctx,
            sender,
            "commands.spectate.not_spectator",
            vec![viewer_name],
        );
    }
    if let Some(found) = target
        && !ctx.can_be_camera(found)
    {
        let name = ctx.target_name(found);
        return fail(ctx, sender, "commands.spectate.cannot_spectate", vec![name]);
    }
    ctx.set_camera_of(viewer, target);
    ctx.record_result(1);
    let message = match target {
        Some(found) => Component::translatable(
            "commands.spectate.success.started",
            vec![ctx.target_name(found)],
        ),
        None => Component::translatable("commands.spectate.success.stopped", Vec::new()),
    };
    ctx.reply(sender, message);
    CommandOutcome::Success
}

fn swing<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/swing <targets> mainhand|offhand [none|whack|stab [<duration>]]";
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((raw_hand, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let targets = match select(ctx, sender, Some(selector)) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let hand = match raw_hand {
        "mainhand" => Hand::Main,
        "offhand" => Hand::Off,
        _ => return usage(ctx, sender, help),
    };
    let mut style = SwingStyle::Whack;
    let mut duration = 6;
    if let Some((raw_style, after)) = next_argument(rest) {
        style = match raw_style {
            "none" => SwingStyle::None,
            "whack" => SwingStyle::Whack,
            "stab" => SwingStyle::Stab,
            _ => return fail(ctx, sender, "argument.enum.invalid", vec![text(raw_style)]),
        };
        if let Some((raw_duration, _)) = next_argument(after) {
            duration = match parse_time(raw_duration) {
                Some(ticks) if ticks >= 1 => i32::try_from(ticks).unwrap_or(i32::MAX),
                Some(ticks) => {
                    return fail(
                        ctx,
                        sender,
                        "argument.time.invalid_tick_count",
                        vec![text(ticks)],
                    );
                }
                None => return fail(ctx, sender, "argument.time.invalid_unit", Vec::new()),
            };
        }
    }
    let swung: Vec<Target> = targets
        .iter()
        .copied()
        .filter(|target| ctx.swing_target(*target, hand, style, duration))
        .collect();
    if swung.is_empty() {
        return fail(ctx, sender, "commands.swing.failed.notliving", Vec::new());
    }
    ctx.record_result(i32::try_from(swung.len()).unwrap_or(i32::MAX));
    let message = match swung.as_slice() {
        [only] => Component::translatable(
            "commands.swing.success.single",
            vec![ctx.target_name(*only)],
        ),
        many => Component::translatable("commands.swing.success.multiple", vec![text(many.len())]),
    };
    ctx.reply(sender, message);
    CommandOutcome::Success
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    label: &str,
    rest: &str,
) -> CommandOutcome {
    match label {
        "ride" => ride(ctx, sender, rest),
        "rotate" => rotate(ctx, sender, rest),
        "spectate" => spectate(ctx, sender, rest),
        _ => swing(ctx, sender, rest),
    }
}
