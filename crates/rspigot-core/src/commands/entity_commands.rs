use rspigot_domain::Vec3;
use rspigot_domain::damage::DamageType;
use rspigot_domain::effect::{AttributeModifier, AttributeOperation};
use rspigot_domain::enchant;
use rspigot_domain::text::Component;

use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, Target, fail, looks_like_coordinate,
    next_argument, parse_vec3, select, select_one, usage,
};
use super::server_commands::reply_err;

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

fn namespaced(raw: &str) -> String {
    if raw.contains(':') {
        raw.to_owned()
    } else {
        format!("minecraft:{raw}")
    }
}

fn registered(registry: &str, name: &str) -> Option<&'static str> {
    rspigot_data::registries::static_entries(registry)?
        .iter()
        .copied()
        .find(|entry| *entry == name)
}

fn attribute_label(attribute: &str) -> Component {
    let path = attribute.strip_prefix("minecraft:").unwrap_or(attribute);
    Component::translatable(format!("attribute.name.{path}"), Vec::new())
}

fn identifier_label(id: &str) -> Component {
    text(id)
}

fn scale_of<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    raw: Option<&str>,
) -> Result<f64, CommandOutcome> {
    match raw {
        None => Ok(1.0),
        Some(raw) => raw
            .parse::<f64>()
            .map_err(|_| fail(ctx, sender, "parsing.double.invalid", vec![text(raw)])),
    }
}

fn scaled(value: f64, scale: f64) -> i32 {
    (value * scale) as i32
}

fn operation_of(name: &str) -> Option<AttributeOperation> {
    match name {
        "add_value" => Some(AttributeOperation::AddValue),
        "add_multiplied_base" => Some(AttributeOperation::AddMultipliedBase),
        "add_multiplied_total" => Some(AttributeOperation::AddMultipliedTotal),
        _ => None,
    }
}

fn attribute<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help = "/attribute <target> <attribute> get|base|modifier ...";
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((raw_attribute, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let target = match select_one(ctx, sender, selector) {
        Ok(target) => target,
        Err(outcome) => return outcome,
    };
    let name = namespaced(raw_attribute);
    let Some(attribute) = registered("minecraft:attribute", &name) else {
        return fail(
            ctx,
            sender,
            "argument.resource.not_found",
            vec![text(&name), text("minecraft:attribute")],
        );
    };
    let who = ctx.target_name(target);
    if !ctx.is_living(target) {
        return fail(ctx, sender, "commands.attribute.failed.entity", vec![who]);
    }
    let Some(state) = ctx.attribute_state(target, attribute) else {
        return fail(
            ctx,
            sender,
            "commands.attribute.failed.no_attribute",
            vec![who, attribute_label(attribute)],
        );
    };
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    match action {
        "get" => {
            let scale = match scale_of(ctx, sender, next_argument(rest).map(|(raw, _)| raw)) {
                Ok(scale) => scale,
                Err(outcome) => return outcome,
            };
            let value = rspigot_domain::attribute::value(&state);
            ctx.record_result(scaled(value, scale));
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.attribute.value.get.success",
                    vec![attribute_label(attribute), who, text(value)],
                ),
            );
            CommandOutcome::Success
        }
        "base" => attribute_base(ctx, sender, target, attribute, state.base, rest),
        "modifier" => attribute_modifier(ctx, sender, target, attribute, &state.modifiers, rest),
        _ => usage(ctx, sender, help),
    }
}

fn attribute_base<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    target: Target,
    attribute: &'static str,
    base: f64,
    rest: &str,
) -> CommandOutcome {
    let help = "/attribute <target> <attribute> base get|set|reset";
    let who = ctx.target_name(target);
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    match action {
        "get" => {
            let scale = match scale_of(ctx, sender, next_argument(rest).map(|(raw, _)| raw)) {
                Ok(scale) => scale,
                Err(outcome) => return outcome,
            };
            ctx.record_result(scaled(base, scale));
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.attribute.base_value.get.success",
                    vec![attribute_label(attribute), who, text(base)],
                ),
            );
            CommandOutcome::Success
        }
        "set" => {
            let Some((raw, _)) = next_argument(rest) else {
                return usage(ctx, sender, help);
            };
            let Ok(value) = raw.parse::<f64>() else {
                return fail(ctx, sender, "parsing.double.invalid", vec![text(raw)]);
            };
            ctx.set_attribute_base(target, attribute, value);
            ctx.record_result(1);
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.attribute.base_value.set.success",
                    vec![attribute_label(attribute), who, text(value)],
                ),
            );
            CommandOutcome::Success
        }
        "reset" => {
            let default = ctx
                .default_attribute_base(target, attribute)
                .unwrap_or(base);
            ctx.set_attribute_base(target, attribute, default);
            ctx.record_result(1);
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.attribute.base_value.reset.success",
                    vec![attribute_label(attribute), who, text(default)],
                ),
            );
            CommandOutcome::Success
        }
        _ => usage(ctx, sender, help),
    }
}

fn attribute_modifier<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    target: Target,
    attribute: &'static str,
    modifiers: &[AttributeModifier],
    rest: &str,
) -> CommandOutcome {
    let help = "/attribute <target> <attribute> modifier add|remove|value ...";
    let who = ctx.target_name(target);
    let Some((action, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let identifier = |ctx: &mut C, raw: &str| -> Result<String, CommandOutcome> {
        let full = namespaced(raw);
        rspigot_domain::Identifier::parse(&full)
            .map(|id| id.to_string())
            .map_err(|_| fail(ctx, sender, "argument.id.invalid", Vec::new()))
    };
    match action {
        "add" => {
            let Some((raw_id, rest)) = next_argument(rest) else {
                return usage(ctx, sender, help);
            };
            let Some((raw_value, rest)) = next_argument(rest) else {
                return usage(ctx, sender, help);
            };
            let Some((raw_operation, _)) = next_argument(rest) else {
                return usage(ctx, sender, help);
            };
            let id = match identifier(ctx, raw_id) {
                Ok(id) => id,
                Err(outcome) => return outcome,
            };
            let Ok(amount) = raw_value.parse::<f64>() else {
                return fail(ctx, sender, "parsing.double.invalid", vec![text(raw_value)]);
            };
            let Some(operation) = operation_of(raw_operation) else {
                return fail(
                    ctx,
                    sender,
                    "argument.enum.invalid",
                    vec![text(raw_operation)],
                );
            };
            let modifier = AttributeModifier {
                id: id.clone().into(),
                amount,
                operation,
            };
            if !ctx.add_attribute_modifier(target, attribute, modifier) {
                return fail(
                    ctx,
                    sender,
                    "commands.attribute.failed.modifier_already_present",
                    vec![identifier_label(&id), attribute_label(attribute), who],
                );
            }
            ctx.record_result(1);
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.attribute.modifier.add.success",
                    vec![identifier_label(&id), attribute_label(attribute), who],
                ),
            );
            CommandOutcome::Success
        }
        "remove" => {
            let Some((raw_id, _)) = next_argument(rest) else {
                return usage(ctx, sender, help);
            };
            let id = match identifier(ctx, raw_id) {
                Ok(id) => id,
                Err(outcome) => return outcome,
            };
            if !ctx.remove_attribute_modifier(target, attribute, &id) {
                return fail(
                    ctx,
                    sender,
                    "commands.attribute.failed.no_modifier",
                    vec![attribute_label(attribute), who, identifier_label(&id)],
                );
            }
            ctx.record_result(1);
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.attribute.modifier.remove.success",
                    vec![identifier_label(&id), attribute_label(attribute), who],
                ),
            );
            CommandOutcome::Success
        }
        "value" => {
            let Some(("get", rest)) = next_argument(rest) else {
                return usage(ctx, sender, help);
            };
            let Some((raw_id, rest)) = next_argument(rest) else {
                return usage(ctx, sender, help);
            };
            let id = match identifier(ctx, raw_id) {
                Ok(id) => id,
                Err(outcome) => return outcome,
            };
            let scale = match scale_of(ctx, sender, next_argument(rest).map(|(raw, _)| raw)) {
                Ok(scale) => scale,
                Err(outcome) => return outcome,
            };
            let Some(modifier) = modifiers.iter().find(|m| m.id == id.as_str()) else {
                return fail(
                    ctx,
                    sender,
                    "commands.attribute.failed.no_modifier",
                    vec![attribute_label(attribute), who, identifier_label(&id)],
                );
            };
            ctx.record_result(scaled(modifier.amount, scale));
            ctx.reply(
                sender,
                Component::translatable(
                    "commands.attribute.modifier.value.get.success",
                    vec![
                        identifier_label(&id),
                        attribute_label(attribute),
                        who,
                        text(modifier.amount),
                    ],
                ),
            );
            CommandOutcome::Success
        }
        _ => usage(ctx, sender, help),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamageSpec {
    pub kind: DamageType,
    pub position: Option<Vec3>,
    pub direct: Option<Target>,
    pub cause: Option<Target>,
}

fn damage<C: CommandContext>(ctx: &mut C, sender: CommandSender, rest: &str) -> CommandOutcome {
    let help =
        "/damage <target> <amount> [<damageType>] [at <location>|by <entity> [from <cause>]]";
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((raw_amount, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let target = match select_one(ctx, sender, selector) {
        Ok(target) => target,
        Err(outcome) => return outcome,
    };
    let Ok(amount) = raw_amount.parse::<f32>() else {
        return fail(ctx, sender, "parsing.float.invalid", vec![text(raw_amount)]);
    };
    if amount < 0.0 {
        return fail(
            ctx,
            sender,
            "argument.float.low",
            vec![text(0), text(amount)],
        );
    }
    let mut spec = DamageSpec {
        kind: DamageType::Generic,
        position: None,
        direct: None,
        cause: None,
    };
    if let Some((raw_type, rest)) = next_argument(rest) {
        let name = namespaced(raw_type);
        if registered_damage_type(&name).is_none() {
            return fail(
                ctx,
                sender,
                "argument.resource.not_found",
                vec![text(&name), text("minecraft:damage_type")],
            );
        }
        spec.kind = DamageType::from_name(&name).unwrap_or(DamageType::Generic);
        match next_argument(rest) {
            None => {}
            Some(("at", rest)) => {
                let mut raw = [""; 3];
                let mut cursor = rest;
                for slot in &mut raw {
                    let Some((token, after)) = next_argument(cursor) else {
                        return fail(ctx, sender, "argument.pos3d.incomplete", Vec::new());
                    };
                    if !looks_like_coordinate(token) {
                        return usage(ctx, sender, help);
                    }
                    *slot = token;
                    cursor = after;
                }
                match parse_vec3(ctx, sender, raw) {
                    Ok(position) => spec.position = Some(position),
                    Err(error) => return reply_err(ctx, sender, error),
                }
            }
            Some(("by", rest)) => {
                let Some((direct, rest)) = next_argument(rest) else {
                    return usage(ctx, sender, help);
                };
                match select_one(ctx, sender, direct) {
                    Ok(found) => spec.direct = Some(found),
                    Err(outcome) => return outcome,
                }
                match next_argument(rest) {
                    None => spec.cause = spec.direct,
                    Some(("from", rest)) => {
                        let Some((cause, _)) = next_argument(rest) else {
                            return usage(ctx, sender, help);
                        };
                        match select_one(ctx, sender, cause) {
                            Ok(found) => spec.cause = Some(found),
                            Err(outcome) => return outcome,
                        }
                    }
                    Some(_) => return usage(ctx, sender, help),
                }
            }
            Some(_) => return usage(ctx, sender, help),
        }
    }
    let who = ctx.target_name(target);
    if !ctx.damage_target(target, spec, amount) {
        return fail(ctx, sender, "commands.damage.invulnerable", Vec::new());
    }
    ctx.record_result(1);
    ctx.reply(
        sender,
        Component::translatable("commands.damage.success", vec![text(amount), who]),
    );
    CommandOutcome::Success
}

fn registered_damage_type(name: &str) -> Option<&'static str> {
    rspigot_data::registries::synced_entries("minecraft:damage_type")?
        .iter()
        .copied()
        .find(|entry| *entry == name)
}

fn enchantment_label(name: &str, level: u8, maximum: u8) -> Component {
    let (space, key) = name.split_once(':').unwrap_or(("minecraft", name));
    let base = Component::translatable(format!("enchantment.{space}.{key}"), Vec::new());
    if level == 1 && maximum == 1 {
        base
    } else {
        base.append(text(" ")).append(Component::translatable(
            format!("enchantment.level.{level}"),
            Vec::new(),
        ))
    }
}

fn enchant_command<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help = "/enchant <targets> <enchantment> [level]";
    let Some((selector, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let Some((raw_enchantment, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let targets = match select(ctx, sender, Some(selector)) {
        Ok(found) => found,
        Err(outcome) => return outcome,
    };
    let name = namespaced(raw_enchantment);
    let Some(definition) = enchant::definition(&name) else {
        return fail(
            ctx,
            sender,
            "argument.enchantment.invalid",
            vec![text(&name)],
        );
    };
    let level = match next_argument(rest) {
        None => 1,
        Some((raw, _)) => match raw.parse::<i32>() {
            Ok(value) if value >= 0 => value,
            Ok(value) => {
                return fail(
                    ctx,
                    sender,
                    "argument.integer.low",
                    vec![text(0), text(value)],
                );
            }
            Err(_) => return fail(ctx, sender, "parsing.int.invalid", vec![text(raw)]),
        },
    };
    if level > i32::from(definition.max_level) {
        return fail(
            ctx,
            sender,
            "commands.enchant.failed.level",
            vec![text(level), text(definition.max_level)],
        );
    }
    let single = targets.len() == 1;
    let mut changed = Vec::new();
    for target in &targets {
        let who = ctx.target_name(*target);
        if !ctx.is_living(*target) {
            if single {
                return fail(ctx, sender, "commands.enchant.failed.entity", vec![who]);
            }
            continue;
        }
        let Some(mut stack) = ctx.main_hand(*target) else {
            if single {
                return fail(ctx, sender, "commands.enchant.failed.itemless", vec![who]);
            }
            continue;
        };
        let held = enchant::levels(&stack);
        let compatible = definition.supports(stack.item.0)
            && held
                .iter()
                .filter(|(other, _)| *other != definition.name)
                .all(|(other, _)| {
                    enchant::definition(other).is_none_or(|held| definition.compatible_with(held))
                });
        if !compatible {
            if single {
                let item = stack
                    .item
                    .kind()
                    .map_or_else(|| text("item"), |kind| text(kind.name()));
                return fail(
                    ctx,
                    sender,
                    "commands.enchant.failed.incompatible",
                    vec![item],
                );
            }
            continue;
        }
        let mut levels: Vec<(String, u8)> = held
            .into_iter()
            .filter(|(other, _)| *other != definition.name)
            .collect();
        levels.push((
            definition.name.to_owned(),
            u8::try_from(level).unwrap_or(u8::MAX),
        ));
        enchant::set_levels(&mut stack, &levels);
        ctx.set_main_hand(*target, stack);
        changed.push(*target);
    }
    if changed.is_empty() {
        return fail(ctx, sender, "commands.enchant.failed", Vec::new());
    }
    let label = enchantment_label(
        definition.name,
        u8::try_from(level).unwrap_or(u8::MAX),
        definition.max_level,
    );
    let message = match changed.as_slice() {
        [only] => Component::translatable(
            "commands.enchant.success.single",
            vec![label, ctx.target_name(*only)],
        ),
        many => Component::translatable(
            "commands.enchant.success.multiple",
            vec![label, text(many.len())],
        ),
    };
    ctx.record_result(i32::try_from(changed.len()).unwrap_or(i32::MAX));
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
        "attribute" => attribute(ctx, sender, rest),
        "damage" => damage(ctx, sender, rest),
        _ => enchant_command(ctx, sender, rest),
    }
}
