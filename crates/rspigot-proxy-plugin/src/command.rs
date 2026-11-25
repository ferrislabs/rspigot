use crate::wire::{ArgumentKind, CommandArgument, CommandSpec};

pub fn command(name: &str) -> CommandBuilder {
    CommandBuilder {
        spec: CommandSpec {
            name: name.to_owned(),
            aliases: Vec::new(),
            permission: None,
            arguments: Vec::new(),
        },
    }
}

#[derive(Debug, Clone)]
pub struct CommandBuilder {
    spec: CommandSpec,
}

impl CommandBuilder {
    #[must_use]
    pub fn alias(mut self, alias: &str) -> Self {
        self.spec.aliases.push(alias.to_owned());
        self
    }

    #[must_use]
    pub fn permission(mut self, node: &str) -> Self {
        self.spec.permission = Some(node.to_owned());
        self
    }

    #[must_use]
    pub fn argument(self, name: &str, kind: ArgumentKind) -> Self {
        self.push(name, kind, false, Vec::new())
    }

    #[must_use]
    pub fn optional(self, name: &str, kind: ArgumentKind) -> Self {
        self.push(name, kind, true, Vec::new())
    }

    #[must_use]
    pub fn choice(self, name: &str, choices: &[&str], optional: bool) -> Self {
        let choices = choices.iter().map(|choice| (*choice).to_owned()).collect();
        self.push(name, ArgumentKind::Choice, optional, choices)
    }

    fn push(
        mut self,
        name: &str,
        kind: ArgumentKind,
        optional: bool,
        choices: Vec<String>,
    ) -> Self {
        self.spec.arguments.push(CommandArgument {
            name: name.to_owned(),
            kind: kind as i32,
            optional,
            choices,
        });
        self
    }

    pub fn build(self) -> CommandSpec {
        self.spec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_builder_fills_the_wire_spec() {
        let spec = command("server")
            .alias("sv")
            .permission("proxy.command.server")
            .optional("target", ArgumentKind::Server)
            .build();
        assert_eq!(spec.name, "server");
        assert_eq!(spec.aliases, ["sv"]);
        assert_eq!(spec.permission.as_deref(), Some("proxy.command.server"));
        assert_eq!(spec.arguments[0].kind, ArgumentKind::Server as i32);
        assert!(spec.arguments[0].optional);
    }

    #[test]
    fn choices_are_kept() {
        let spec = command("mode")
            .choice("value", &["on", "off"], false)
            .build();
        assert_eq!(spec.arguments[0].choices, ["on", "off"]);
        assert!(!spec.arguments[0].optional);
    }
}
