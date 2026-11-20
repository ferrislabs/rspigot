use rspigot_domain::player::PlayerId;
use rspigot_domain::{EntityId, Vec3};

use crate::commands::{CommandContext, CommandSender, Target};
use crate::plugins::{
    ArgumentKind, ArgumentValue, CommandArgument, CommandSpec, EntityMarks, EntityView,
    MetaSnapshot,
};
use crate::ports::ClientLink;
use crate::server::Server;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Unfit {
    Mismatch,
    Incomplete,
    NoPlayer,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Token<'a> {
    pub(super) start: usize,
    pub(super) text: &'a str,
}

pub(super) fn tokens(line: &str) -> Vec<Token<'_>> {
    let mut found = Vec::new();
    let mut start = None;
    for (index, character) in line.char_indices() {
        match (character.is_whitespace(), start) {
            (true, Some(from)) => {
                found.push(Token {
                    start: from,
                    text: &line[from..index],
                });
                start = None;
            }
            (false, None) => start = Some(index),
            _ => {}
        }
    }
    if let Some(from) = start {
        found.push(Token {
            start: from,
            text: &line[from..],
        });
    }
    found
}

fn selector_end(tokens: &[Token<'_>], from: usize) -> usize {
    let first = tokens[from].text;
    if !first.starts_with('@') || !first.contains('[') {
        return from + 1;
    }
    let mut depth = 0_i32;
    for (offset, token) in tokens[from..].iter().enumerate() {
        for character in token.text.chars() {
            match character {
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            }
        }
        if depth <= 0 {
            return from + offset + 1;
        }
    }
    tokens.len()
}

pub(super) fn coordinate(word: &str, origin: f64, centered: bool) -> Option<f64> {
    let (relative, number) = match word.strip_prefix('~') {
        Some(rest) => (true, rest),
        None => (false, word),
    };
    let value = if number.is_empty() {
        0.0
    } else {
        number
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())?
    };
    Some(if relative {
        origin + value
    } else if centered && !number.contains('.') {
        value + 0.5
    } else {
        value
    })
}

pub(super) fn argument_at<'a>(
    spec: &'a CommandSpec,
    typed: &[&str],
) -> Option<&'a CommandArgument> {
    let position = typed.len();
    let mut cursor = 0;
    for argument in &spec.arguments {
        if argument.kind == ArgumentKind::GreedyText {
            return (position >= cursor).then_some(argument);
        }
        let width = argument.kind.width();
        if position < cursor + width {
            return Some(argument);
        }
        if argument.kind == ArgumentKind::Literal
            && !typed[cursor].eq_ignore_ascii_case(&argument.name)
        {
            return None;
        }
        cursor += width;
    }
    None
}

impl<L: ClientLink> Server<L> {
    pub(super) fn read_arguments(
        &self,
        sender: PlayerId,
        spec: &CommandSpec,
        line: &str,
    ) -> Result<(Vec<ArgumentValue>, usize), Unfit> {
        let tokens = tokens(line);
        let mut cursor = 0;
        let mut values = Vec::new();
        let mut literals = 0;
        for argument in &spec.arguments {
            if cursor >= tokens.len() {
                if argument.optional {
                    break;
                }
                return Err(Unfit::Incomplete);
            }
            let word = tokens[cursor].text;
            match argument.kind {
                ArgumentKind::Literal => {
                    if !word.eq_ignore_ascii_case(&argument.name) {
                        return Err(Unfit::Mismatch);
                    }
                    literals += 1;
                    cursor += 1;
                }
                ArgumentKind::GreedyText => {
                    values.push(ArgumentValue::Text(
                        line[tokens[cursor].start..].trim_end().to_owned(),
                    ));
                    cursor = tokens.len();
                }
                ArgumentKind::Player => {
                    values.push(
                        self.players
                            .values()
                            .find(|player| player.name().eq_ignore_ascii_case(word))
                            .and_then(|player| self.snapshot_of(player.id))
                            .map(ArgumentValue::Player)
                            .ok_or(Unfit::NoPlayer)?,
                    );
                    cursor += 1;
                }
                ArgumentKind::Item => {
                    values.push(ArgumentValue::Item(item_key(word)));
                    cursor += 1;
                }
                ArgumentKind::Integer => {
                    values.push(ArgumentValue::Integer(
                        word.parse().map_err(|_| Unfit::Incomplete)?,
                    ));
                    cursor += 1;
                }
                ArgumentKind::Text => {
                    values.push(ArgumentValue::Text(word.to_owned()));
                    cursor += 1;
                }
                ArgumentKind::Boolean => {
                    values.push(ArgumentValue::Flag(
                        word.parse().map_err(|_| Unfit::Incomplete)?,
                    ));
                    cursor += 1;
                }
                ArgumentKind::Double | ArgumentKind::Float => {
                    values.push(ArgumentValue::Decimal(
                        word.parse::<f64>()
                            .ok()
                            .filter(|value| value.is_finite())
                            .ok_or(Unfit::Incomplete)?,
                    ));
                    cursor += 1;
                }
                ArgumentKind::Choice => {
                    let chosen = argument
                        .choices
                        .iter()
                        .find(|choice| choice.eq_ignore_ascii_case(word))
                        .ok_or(Unfit::Incomplete)?;
                    values.push(ArgumentValue::Text(chosen.clone()));
                    cursor += 1;
                }
                ArgumentKind::Location => {
                    let [x, y, z] = tokens
                        .get(cursor..cursor + 3)
                        .and_then(|words| <&[Token<'_>; 3]>::try_from(words).ok())
                        .ok_or(Unfit::Incomplete)?;
                    let origin = self
                        .players
                        .get(&sender)
                        .map_or(Vec3::ZERO, |player| player.position);
                    let spot = Vec3::new(
                        coordinate(x.text, origin.x, true).ok_or(Unfit::Incomplete)?,
                        coordinate(y.text, origin.y, false).ok_or(Unfit::Incomplete)?,
                        coordinate(z.text, origin.z, true).ok_or(Unfit::Incomplete)?,
                    );
                    values.push(ArgumentValue::Location(spot));
                    cursor += 3;
                }
                ArgumentKind::Entities => {
                    let end = selector_end(&tokens, cursor);
                    let last = tokens[end - 1];
                    let text = &line[tokens[cursor].start..last.start + last.text.len()];
                    let found = self
                        .select_entities(CommandSender::Player(sender), text)
                        .map_err(|_| Unfit::Incomplete)?;
                    let views: Vec<EntityView> = found
                        .into_iter()
                        .filter_map(|target| self.target_view(target))
                        .collect();
                    if views.is_empty() {
                        return Err(Unfit::NoPlayer);
                    }
                    values.push(ArgumentValue::Entities(views));
                    cursor = end;
                }
            }
        }
        if cursor < tokens.len() {
            return Err(Unfit::Incomplete);
        }
        Ok((values, literals))
    }

    fn target_view(&self, target: Target) -> Option<EntityView> {
        match target {
            Target::Player(id) => {
                let snapshot = self.snapshot_of(id)?;
                Some(EntityView {
                    id: EntityId(snapshot.entity_id),
                    uuid: id.0,
                    kind: "minecraft:player".to_owned(),
                    position: snapshot.position,
                    player: Some(snapshot),
                    meta: MetaSnapshot::default(),
                    marks: EntityMarks::default(),
                })
            }
            Target::Entity(id) => self.entity_view(id),
        }
    }
}

fn item_key(word: &str) -> String {
    let word = word.to_ascii_lowercase();
    if word.contains(':') {
        word
    } else {
        format!("minecraft:{word}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(arguments: Vec<(&str, ArgumentKind)>) -> CommandSpec {
        CommandSpec {
            id: 1,
            name: "shop".to_owned(),
            arguments: arguments
                .into_iter()
                .map(|(name, kind)| CommandArgument {
                    name: name.to_owned(),
                    kind,
                    ..CommandArgument::default()
                })
                .collect(),
            ..CommandSpec::default()
        }
    }

    #[test]
    fn tokens_keep_where_they_start() {
        let found = tokens("a  bc d");
        let spots: Vec<(usize, &str)> = found.iter().map(|t| (t.start, t.text)).collect();
        assert_eq!(spots, [(0, "a"), (3, "bc"), (6, "d")]);
    }

    #[test]
    fn a_bracketed_selector_spans_its_spaces() {
        let found = tokens("@e[type=cow, limit=2] next");
        assert_eq!(selector_end(&found, 0), 2);
        assert_eq!(selector_end(&found, 2), 3);
    }

    #[test]
    fn coordinates_are_relative_centred_or_exact() {
        assert_eq!(coordinate("~", 10.0, true), Some(10.0));
        assert_eq!(coordinate("~-2.5", 10.0, true), Some(7.5));
        assert_eq!(coordinate("3", 10.0, true), Some(3.5));
        assert_eq!(coordinate("3", 10.0, false), Some(3.0));
        assert_eq!(coordinate("3.0", 10.0, true), Some(3.0));
        assert_eq!(coordinate("x", 10.0, true), None);
        assert_eq!(coordinate("inf", 10.0, true), None);
    }

    #[test]
    fn the_argument_under_the_cursor_skips_literals_and_wide_arguments() {
        let shop = spec(vec![
            ("buy", ArgumentKind::Literal),
            ("where", ArgumentKind::Location),
            ("note", ArgumentKind::GreedyText),
        ]);
        let name = |typed: &[&str]| argument_at(&shop, typed).map(|a| a.name.clone());
        assert_eq!(name(&[]), Some("buy".to_owned()));
        assert_eq!(name(&["buy"]), Some("where".to_owned()));
        assert_eq!(name(&["buy", "1", "2"]), Some("where".to_owned()));
        assert_eq!(name(&["buy", "1", "2", "3"]), Some("note".to_owned()));
        assert_eq!(
            name(&["buy", "1", "2", "3", "a", "b"]),
            Some("note".to_owned())
        );
        assert_eq!(name(&["sell"]), None);
    }
}
