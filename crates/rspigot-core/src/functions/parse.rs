use std::borrow::Cow;

use rspigot_domain::Identifier;
use rspigot_nbt::{Compound, Tag};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Text(String),
    Variable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    Command(String),
    Macro(Vec<Segment>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    id: Identifier,
    entries: Vec<Entry>,
    has_macro: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error(
        "Unknown or invalid command '{command}' on line {line} (try removing the leading slash)"
    )]
    LeadingSlash { line: usize, command: String },
    #[error("Illegal macro variable name '{name}' on line {line}")]
    IllegalVariable { line: usize, name: String },
    #[error("Unterminated macro variable on line {line}")]
    Unterminated { line: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstantiationError {
    MissingArguments,
    MissingArgument(String),
}

impl Function {
    pub fn parse(id: Identifier, text: &str) -> Result<Self, ParseError> {
        let mut entries = Vec::new();
        let mut has_macro = false;
        let mut pending: Option<(usize, String)> = None;
        for (index, raw) in text.lines().enumerate() {
            let trimmed = raw.trim();
            if pending.is_none() && (trimmed.is_empty() || trimmed.starts_with('#')) {
                continue;
            }
            let (continues, body) = match trimmed.strip_suffix('\\') {
                Some(body) => (true, body),
                None => (false, trimmed),
            };
            match &mut pending {
                Some((_, joined)) => joined.push_str(body),
                None => pending = Some((index + 1, body.to_owned())),
            }
            if !continues && let Some((line, command)) = pending.take() {
                push_entry(&mut entries, &mut has_macro, line, command)?;
            }
        }
        if let Some((line, command)) = pending.take() {
            push_entry(&mut entries, &mut has_macro, line, command)?;
        }
        Ok(Self {
            id,
            entries,
            has_macro,
        })
    }

    pub fn id(&self) -> &Identifier {
        &self.id
    }

    pub fn is_macro(&self) -> bool {
        self.has_macro
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn commands<'a>(
        &'a self,
        arguments: Option<&Compound>,
    ) -> Result<Vec<Cow<'a, str>>, InstantiationError> {
        if self.has_macro && arguments.is_none() {
            return Err(InstantiationError::MissingArguments);
        }
        let mut commands = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            match entry {
                Entry::Command(command) => commands.push(Cow::Borrowed(command.as_str())),
                Entry::Macro(segments) => {
                    let mut line = String::new();
                    for segment in segments {
                        match segment {
                            Segment::Text(text) => line.push_str(text),
                            Segment::Variable(name) => {
                                let value = arguments
                                    .and_then(|arguments| arguments.get(name))
                                    .ok_or_else(|| {
                                        InstantiationError::MissingArgument(name.clone())
                                    })?;
                                push_value(&mut line, value);
                            }
                        }
                    }
                    commands.push(Cow::Owned(line));
                }
            }
        }
        Ok(commands)
    }
}

fn push_entry(
    entries: &mut Vec<Entry>,
    has_macro: &mut bool,
    line: usize,
    command: String,
) -> Result<(), ParseError> {
    if command.is_empty() {
        return Ok(());
    }
    if command.starts_with('/') {
        return Err(ParseError::LeadingSlash { line, command });
    }
    match command.strip_prefix('$') {
        Some(template) => {
            *has_macro = true;
            entries.push(Entry::Macro(segments(template, line)?));
        }
        None => entries.push(Entry::Command(command)),
    }
    Ok(())
}

fn segments(template: &str, line: usize) -> Result<Vec<Segment>, ParseError> {
    let mut segments = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find("$(") {
        if start > 0 {
            segments.push(Segment::Text(rest[..start].to_owned()));
        }
        let after = &rest[start + 2..];
        let end = after.find(')').ok_or(ParseError::Unterminated { line })?;
        let name = &after[..end];
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(ParseError::IllegalVariable {
                line,
                name: name.to_owned(),
            });
        }
        segments.push(Segment::Variable(name.to_owned()));
        rest = &after[end + 1..];
    }
    if !rest.is_empty() {
        segments.push(Segment::Text(rest.to_owned()));
    }
    Ok(segments)
}

fn push_value(out: &mut String, tag: &Tag) {
    match tag {
        Tag::String(text) => out.push_str(text),
        Tag::Byte(value) => out.push_str(&value.to_string()),
        Tag::Short(value) => out.push_str(&value.to_string()),
        Tag::Int(value) => out.push_str(&value.to_string()),
        Tag::Long(value) => out.push_str(&value.to_string()),
        Tag::Float(value) => out.push_str(&format!("{value:?}")),
        Tag::Double(value) => out.push_str(&format!("{value:?}")),
        other => push_snbt(out, other),
    }
}

fn push_snbt(out: &mut String, tag: &Tag) {
    match tag {
        Tag::Byte(value) => out.push_str(&format!("{value}b")),
        Tag::Short(value) => out.push_str(&format!("{value}s")),
        Tag::Int(value) => out.push_str(&value.to_string()),
        Tag::Long(value) => out.push_str(&format!("{value}L")),
        Tag::Float(value) => out.push_str(&format!("{value:?}f")),
        Tag::Double(value) => out.push_str(&format!("{value:?}d")),
        Tag::String(text) => {
            out.push('"');
            for c in text.chars() {
                if c == '"' || c == '\\' {
                    out.push('\\');
                }
                out.push(c);
            }
            out.push('"');
        }
        Tag::List(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                push_snbt(out, item);
            }
            out.push(']');
        }
        Tag::Compound(compound) => {
            out.push('{');
            for (index, (key, value)) in compound.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(key);
                out.push(':');
                push_snbt(out, value);
            }
            out.push('}');
        }
        Tag::ByteArray(values) => {
            push_array(out, "B", values.iter().map(|v| format!("{}b", *v as i8)))
        }
        Tag::IntArray(values) => push_array(out, "I", values.iter().map(ToString::to_string)),
        Tag::LongArray(values) => push_array(out, "L", values.iter().map(|v| format!("{v}L"))),
    }
}

fn push_array(out: &mut String, kind: &str, values: impl Iterator<Item = String>) {
    out.push('[');
    out.push_str(kind);
    out.push(';');
    for (index, value) in values.enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&value);
    }
    out.push(']');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> Identifier {
        Identifier::parse("test:f").unwrap()
    }

    fn lines(text: &str) -> Vec<String> {
        Function::parse(id(), text)
            .unwrap()
            .commands(None)
            .unwrap()
            .into_iter()
            .map(Cow::into_owned)
            .collect()
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let text = "# header\n\nsay one\n   \n  # indented comment\nsay two\r\n";
        assert_eq!(lines(text), ["say one", "say two"]);
    }

    #[test]
    fn a_trailing_backslash_joins_the_next_line() {
        let text = "say hello \\\n   world\\\n!\nsay next";
        assert_eq!(lines(text), ["say hello world!", "say next"]);
    }

    #[test]
    fn a_leading_slash_is_refused() {
        let error = Function::parse(id(), "say a\n/say b").unwrap_err();
        assert_eq!(
            error,
            ParseError::LeadingSlash {
                line: 2,
                command: "/say b".to_owned()
            }
        );
    }

    #[test]
    fn macros_substitute_arguments() {
        let function = Function::parse(id(), "say plain\n$say $(who) has $(count) items").unwrap();
        assert!(function.is_macro());
        let arguments = Compound::new().with("who", "Alex").with("count", 3);
        let commands = function.commands(Some(&arguments)).unwrap();
        assert_eq!(commands, ["say plain", "say Alex has 3 items"]);
    }

    #[test]
    fn macro_values_follow_their_tag_type() {
        let function = Function::parse(id(), "$say $(a) $(b) $(c) $(d)").unwrap();
        let arguments = Compound::new()
            .with("a", 1.5f64)
            .with("b", Tag::Byte(1))
            .with("c", Tag::Long(9))
            .with("d", Compound::new().with("k", "v"));
        assert_eq!(
            function.commands(Some(&arguments)).unwrap(),
            ["say 1.5 1 9 {k:\"v\"}"]
        );
    }

    #[test]
    fn a_missing_argument_names_the_variable() {
        let function = Function::parse(id(), "$say $(who) $(what)").unwrap();
        let arguments = Compound::new().with("who", "Alex");
        assert_eq!(
            function.commands(Some(&arguments)),
            Err(InstantiationError::MissingArgument("what".to_owned()))
        );
        assert_eq!(
            function.commands(None),
            Err(InstantiationError::MissingArguments)
        );
    }

    #[test]
    fn bad_macro_syntax_is_refused() {
        assert_eq!(
            Function::parse(id(), "$say $(who").unwrap_err(),
            ParseError::Unterminated { line: 1 }
        );
        assert_eq!(
            Function::parse(id(), "say a\n$say $(bad name)").unwrap_err(),
            ParseError::IllegalVariable {
                line: 2,
                name: "bad name".to_owned()
            }
        );
    }

    #[test]
    fn plain_functions_need_no_arguments() {
        let function = Function::parse(id(), "say a").unwrap();
        assert!(!function.is_macro());
        assert_eq!(function.len(), 1);
        assert_eq!(
            function.commands(Some(&Compound::new())).unwrap(),
            ["say a"]
        );
    }
}
