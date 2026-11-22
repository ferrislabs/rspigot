use std::sync::OnceLock;

use rspigot_domain::text::{Component, NamedColor};

use super::dispatch::CommandContext;
use super::json_text;
use super::tree::{ArgumentParser, CommandNodeKind, CommandTree, build_tree};

const COLORS: [&str; 17] = [
    "black",
    "dark_blue",
    "dark_green",
    "dark_aqua",
    "dark_red",
    "dark_purple",
    "gold",
    "gray",
    "dark_gray",
    "blue",
    "green",
    "aqua",
    "red",
    "light_purple",
    "yellow",
    "white",
    "reset",
];

const OPERATIONS: [&str; 9] = ["=", "+=", "-=", "*=", "/=", "%=", "<", ">", "><"];
const HEIGHTMAPS: [&str; 4] = [
    "world_surface",
    "motion_blocking",
    "motion_blocking_no_leaves",
    "ocean_floor",
];

fn error(key: &str, token: &str) -> Component {
    Component::translatable(key, vec![Component::text(token)]).color(NamedColor::Red)
}

fn plain_error(key: &str) -> Component {
    Component::translatable(key, Vec::new()).color(NamedColor::Red)
}

fn one_of(options: &[&str], token: &str, key: &str) -> Result<(), Component> {
    if options.contains(&token) {
        Ok(())
    } else {
        Err(error(key, token))
    }
}

fn valid_identifier(token: &str, tags: bool) -> bool {
    let body = if tags {
        token.strip_prefix('#').unwrap_or(token)
    } else {
        token
    };
    let (namespace, path) = body.split_once(':').unwrap_or(("minecraft", body));
    let namespace_ok = !namespace.is_empty()
        && namespace
            .chars()
            .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_' | '-' | '.'));
    let path_ok = !path.is_empty()
        && path
            .chars()
            .all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_' | '-' | '.' | '/'));
    namespace_ok && path_ok
}

fn identifier(token: &str, tags: bool) -> Result<(), Component> {
    if valid_identifier(token, tags) {
        Ok(())
    } else {
        Err(plain_error("argument.id.invalid"))
    }
}

fn hex_color(token: &str) -> Result<(), Component> {
    let digits = token.strip_prefix('#').unwrap_or(token);
    if digits.len() == 6 && digits.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(error("argument.hexcolor.invalid", token))
    }
}

fn swizzle(token: &str) -> Result<(), Component> {
    let mut seen = [false; 3];
    for c in token.chars() {
        let index = match c {
            'x' => 0,
            'y' => 1,
            'z' => 2,
            _ => return Err(plain_error("arguments.swizzle.invalid")),
        };
        if seen[index] {
            return Err(plain_error("arguments.swizzle.invalid"));
        }
        seen[index] = true;
    }
    if token.is_empty() {
        return Err(plain_error("arguments.swizzle.invalid"));
    }
    Ok(())
}

fn display_slot(token: &str) -> Result<(), Component> {
    let valid = match token.strip_prefix("sidebar.team.") {
        Some(color) => COLORS[..16].contains(&color),
        None => matches!(token, "list" | "sidebar" | "below_name"),
    };
    if valid {
        Ok(())
    } else {
        Err(error("argument.scoreboardDisplaySlot.invalid", token))
    }
}

fn whole(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

fn int_range(token: &str) -> Result<(), Component> {
    if token.is_empty() || token == ".." {
        return Err(plain_error("argument.range.empty"));
    }
    let ranged = token.contains("..");
    let (min, max) = token.split_once("..").unwrap_or((token, token));
    let mut values = Vec::new();
    for part in [min, max] {
        if part.is_empty() {
            continue;
        }
        if whole(part) {
            match part.parse::<i64>() {
                Ok(value) => values.push(value),
                Err(_) => return Err(error("parsing.int.invalid", part)),
            }
        } else if part.parse::<f64>().is_ok() {
            return Err(plain_error("argument.range.ints"));
        } else {
            return Err(error("parsing.int.invalid", part));
        }
    }
    if let [min, max] = values.as_slice()
        && ranged
        && min > max
    {
        return Err(plain_error("argument.range.swapped"));
    }
    Ok(())
}

fn coordinate(token: &str, integer: bool, key: &str) -> Result<(), Component> {
    let relative = token.starts_with('~');
    let number = token.strip_prefix('~').unwrap_or(token);
    if relative && number.is_empty() {
        return Ok(());
    }
    let valid = if integer {
        number.parse::<i32>().is_ok()
    } else {
        number.parse::<f64>().is_ok_and(f64::is_finite)
    };
    if valid { Ok(()) } else { Err(plain_error(key)) }
}

fn arity(parser: &str) -> usize {
    match parser {
        "minecraft:rotation" | "minecraft:vec2" | "minecraft:column_pos" => 2,
        _ => 1,
    }
}

fn balanced(token: &str) -> bool {
    let count = |open: char, close: char| {
        token.chars().filter(|c| *c == open).count()
            == token.chars().filter(|c| *c == close).count()
    };
    count('{', '}') && count('[', ']') && !token.contains(['"', '\''])
}

fn tail(parser: &str) -> bool {
    matches!(
        parser,
        "minecraft:nbt_tag"
            | "minecraft:component"
            | "minecraft:style"
            | "minecraft:particle"
            | "minecraft:block_predicate"
            | "minecraft:slot_source"
    )
}

fn validate_tail(parser: &str, text: &str) -> Result<(), Component> {
    match parser {
        "minecraft:nbt_tag" => rspigot_nbt::parse_tag(text)
            .map(|_| ())
            .map_err(|_| plain_error("argument.nbt.expected.value")),
        "minecraft:component" => json_text::parse(text).map(|_| ()),
        "minecraft:style" => json_text::parse_style(text).map(|_| ()),
        "minecraft:particle" => {
            let id = text
                .split(|c: char| c == '{' || c.is_whitespace())
                .next()
                .unwrap_or_default();
            let full = if id.contains(':') {
                id.to_owned()
            } else {
                format!("minecraft:{id}")
            };
            if rspigot_data::registries::static_index("minecraft:particle_type", &full).is_some() {
                Ok(())
            } else {
                Err(error("particle.notFound", id))
            }
        }
        "minecraft:block_predicate" => {
            let id = text
                .split(|c: char| c == '[' || c == '{' || c.is_whitespace())
                .next()
                .unwrap_or_default();
            if id.starts_with('#') {
                return Ok(());
            }
            let full = if id.contains(':') {
                id.to_owned()
            } else {
                format!("minecraft:{id}")
            };
            if rspigot_data::registries::static_index("minecraft:block", &full).is_some() {
                Ok(())
            } else {
                Err(error("argument.block.id.invalid", id))
            }
        }
        _ => Ok(()),
    }
}

fn validate_with<C: CommandContext>(ctx: &C, parser: &str, token: &str) -> Result<(), Component> {
    match parser {
        "minecraft:objective" if ctx.scoreboard().objective(token).is_none() => {
            Err(error("arguments.objective.notFound", token))
        }
        "minecraft:team" if ctx.scoreboard().team(token).is_none() => {
            Err(error("team.notFound", token))
        }
        "minecraft:objective_criteria" => {
            if crate::scoreboard::parse_criteria(token).is_some() {
                Ok(())
            } else {
                Err(error("argument.criteria.invalid", token))
            }
        }
        "minecraft:item_slot" => {
            if super::slots::slot_ids(token).is_some() {
                Ok(())
            } else {
                Err(error("slot.unknown", token))
            }
        }
        "minecraft:nbt_path" if balanced(token) => rspigot_nbt::NbtPath::parse(token)
            .map(|_| ())
            .map_err(|_| plain_error("arguments.nbtpath.node.invalid")),
        _ => Ok(()),
    }
}

pub(super) fn validate(parser: &str, tokens: &[&str]) -> Result<(), Component> {
    let first = tokens.first().copied().unwrap_or_default();
    match parser {
        "minecraft:team_color" => one_of(&COLORS, first, "argument.color.invalid"),
        "minecraft:hex_color" => hex_color(first),
        "minecraft:template_rotation" => one_of(
            &["none", "clockwise_90", "180", "counterclockwise_90"],
            first,
            "argument.enum.invalid",
        ),
        "minecraft:template_mirror" => one_of(
            &["none", "left_right", "front_back"],
            first,
            "argument.enum.invalid",
        ),
        "minecraft:swing_animation" => one_of(
            &["none", "whack", "stab"],
            first,
            "argument.swing_animation.invalid",
        ),
        "minecraft:entity_anchor" => one_of(&["feet", "eyes"], first, "argument.anchor.invalid"),
        "minecraft:heightmap" => one_of(&HEIGHTMAPS, first, "argument.enum.invalid"),
        "minecraft:swizzle" => swizzle(first),
        "minecraft:operation" => one_of(&OPERATIONS, first, "arguments.operation.invalid"),
        "minecraft:scoreboard_slot" => display_slot(first),
        "minecraft:int_range" => int_range(first),
        "minecraft:rotation" | "minecraft:vec2" => {
            if tokens.len() < 2 {
                return Err(plain_error(if parser == "minecraft:rotation" {
                    "argument.rotation.incomplete"
                } else {
                    "argument.pos2d.incomplete"
                }));
            }
            tokens
                .iter()
                .take(2)
                .try_for_each(|t| coordinate(t, false, "argument.pos.missing.double"))
        }
        "minecraft:column_pos" => {
            if tokens.len() < 2 {
                return Err(plain_error("argument.pos2d.incomplete"));
            }
            tokens
                .iter()
                .take(2)
                .try_for_each(|t| coordinate(t, true, "argument.pos.missing.int"))
        }
        "minecraft:function" => identifier(first, true),
        "minecraft:loot_table"
        | "minecraft:loot_predicate"
        | "minecraft:loot_modifier"
        | "minecraft:feature" => identifier(first, false),
        _ => Ok(()),
    }
}

fn tree() -> &'static CommandTree {
    static TREE: OnceLock<CommandTree> = OnceLock::new();
    TREE.get_or_init(|| build_tree(4))
}

fn is_free_form(token: &str) -> bool {
    token.contains(['{', '[', '"', '\''])
}

fn walk<C: CommandContext>(ctx: &C, tree: &CommandTree, tokens: &[&str]) -> Option<Component> {
    let mut node = tree.root;
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        let current = &tree.nodes[tree.nodes[node].redirect.unwrap_or(node)];
        let literal = current.children.iter().copied().find(|child| {
            matches!(&tree.nodes[*child].kind, CommandNodeKind::Literal(name) if name == token)
        });
        if let Some(child) = literal {
            node = child;
            index += 1;
            continue;
        }
        let arguments: Vec<usize> = current
            .children
            .iter()
            .copied()
            .filter(|child| matches!(tree.nodes[*child].kind, CommandNodeKind::Argument { .. }))
            .collect();
        let [only] = arguments.as_slice() else {
            return None;
        };
        let CommandNodeKind::Argument { parser, .. } = &tree.nodes[*only].kind else {
            return None;
        };
        if let ArgumentParser::Named(name) = parser
            && tail(name)
        {
            return validate_tail(name, &tokens[index..].join(" ")).err();
        }
        if is_free_form(token) && !matches!(parser, ArgumentParser::Named("minecraft:nbt_path")) {
            return None;
        }
        match parser {
            ArgumentParser::Named(name) => {
                let take = arity(name).min(tokens.len() - index);
                let slice = &tokens[index..index + take];
                if *name != "minecraft:nbt_path" && slice.iter().any(|t| is_free_form(t)) {
                    return None;
                }
                if let Err(error) = validate(name, slice) {
                    return Some(error);
                }
                if let Err(error) = validate_with(ctx, name, slice[0]) {
                    return Some(error);
                }
                if *name == "minecraft:nbt_path" && !balanced(slice[0]) {
                    return None;
                }
                index += take;
            }
            ArgumentParser::Entity { .. }
            | ArgumentParser::Word
            | ArgumentParser::Bool
            | ArgumentParser::Integer { .. }
            | ArgumentParser::Float { .. }
            | ArgumentParser::Double { .. }
            | ArgumentParser::ResourceLocation
            | ArgumentParser::GameMode
            | ArgumentParser::Dimension
            | ArgumentParser::Time { .. }
            | ArgumentParser::GameProfile
            | ArgumentParser::ScoreHolder { .. } => index += 1,
            _ => return None,
        }
        node = *only;
    }
    None
}

pub(super) fn check_line<C: CommandContext>(ctx: &C, line: &str) -> Option<Component> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let label = tokens.first()?.to_ascii_lowercase();
    let stripped = label.strip_prefix("minecraft:").unwrap_or(&label);
    let mut rebuilt = vec![stripped];
    rebuilt.extend(&tokens[1..]);
    walk(ctx, tree(), &rebuilt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_values_validate_with_the_vanilla_keys() {
        for (parser, token, key) in [
            ("minecraft:team_color", "nope", "argument.color.invalid"),
            ("minecraft:hex_color", "zzzzzz", "argument.hexcolor.invalid"),
            ("minecraft:entity_anchor", "head", "argument.anchor.invalid"),
            ("minecraft:heightmap", "sky", "argument.enum.invalid"),
            ("minecraft:operation", "?=", "arguments.operation.invalid"),
            (
                "minecraft:scoreboard_slot",
                "middle",
                "argument.scoreboardDisplaySlot.invalid",
            ),
            ("minecraft:int_range", "5..1", "argument.range.swapped"),
            ("minecraft:int_range", "1.5", "argument.range.ints"),
            (
                "minecraft:swing_animation",
                "slap",
                "argument.swing_animation.invalid",
            ),
            ("minecraft:function", "T:Bad", "argument.id.invalid"),
            ("minecraft:swizzle", "xx", "arguments.swizzle.invalid"),
            ("minecraft:template_mirror", "up", "argument.enum.invalid"),
        ] {
            let found = validate(parser, &[token]).expect_err(token);
            assert!(format!("{found:?}").contains(key), "{token}: {found:?}");
        }
        for (parser, token) in [
            ("minecraft:team_color", "red"),
            ("minecraft:hex_color", "ff00aa"),
            ("minecraft:entity_anchor", "eyes"),
            ("minecraft:operation", "><"),
            ("minecraft:scoreboard_slot", "sidebar.team.red"),
            ("minecraft:int_range", "1..5"),
            ("minecraft:function", "#t:tag"),
        ] {
            assert!(validate(parser, &[token]).is_ok(), "{token}");
        }
        assert!(validate("minecraft:rotation", &["1"]).is_err());
        assert!(validate("minecraft:rotation", &["~", "~5"]).is_ok());
    }

    #[test]
    fn tail_values_validate_like_the_executors_parse_them() {
        assert!(validate_tail("minecraft:nbt_tag", "{a:1b}").is_ok());
        assert!(validate_tail("minecraft:nbt_tag", "{a:").is_err());
        assert!(validate_tail("minecraft:component", "{\"text\":\"x\"}").is_ok());
        assert!(validate_tail("minecraft:component", "{\"text\":").is_err());
        assert!(validate_tail("minecraft:particle", "minecraft:flame 0 0 0 0 0 0 0 1").is_ok());
        assert!(validate_tail("minecraft:particle", "nonsense").is_err());
        assert!(validate_tail("minecraft:block_predicate", "minecraft:stone[x=y]").is_ok());
        assert!(validate_tail("minecraft:block_predicate", "minecraft:nope").is_err());
        assert!(validate_tail("minecraft:block_predicate", "#minecraft:logs").is_ok());
    }
}
