use super::tree::{ArgumentParser, CommandNodeKind, CommandTree};

pub(crate) struct SignableArgument {
    pub(crate) name: String,
    pub(crate) value: String,
}

fn split_token(input: &str) -> (&str, &str) {
    match input.find(' ') {
        Some(at) => (&input[..at], &input[at..]),
        None => (input, ""),
    }
}

fn read_balanced(input: &str) -> (&str, &str) {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (at, ch) in input.char_indices() {
        if let Some(open) = quote {
            match (escaped, ch) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, c) if c == open => quote = None,
                _ => {}
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '[' | '{' => depth += 1,
            ']' | '}' => depth = depth.saturating_sub(1),
            ' ' if depth == 0 => return (&input[..at], &input[at..]),
            _ => {}
        }
    }
    (input, "")
}

fn tokens_for(parser: &ArgumentParser) -> usize {
    match parser {
        ArgumentParser::Vec3 | ArgumentParser::BlockPos => 3,
        _ => 1,
    }
}

fn consume<'a>(parser: &ArgumentParser, input: &'a str) -> &'a str {
    let mut rest = input;
    for step in 0..tokens_for(parser) {
        if step > 0 {
            rest = rest.trim_start_matches(' ');
        }
        rest = match parser {
            ArgumentParser::Word | ArgumentParser::Bool => split_token(rest).1,
            _ => read_balanced(rest).1,
        };
    }
    rest
}

pub(crate) fn signable_arguments(tree: &CommandTree, command: &str) -> Vec<SignableArgument> {
    let mut found = Vec::new();
    let mut node = tree.root;
    let mut rest = command;
    loop {
        let current = &tree.nodes[node];
        let children = match current.redirect {
            Some(target) if current.children.is_empty() => &tree.nodes[target].children,
            _ => &current.children,
        };
        let input = rest.trim_start_matches(' ');
        if input.is_empty() {
            return found;
        }
        let (token, after) = split_token(input);
        let literal = children.iter().copied().find(|child| {
            matches!(&tree.nodes[*child].kind, CommandNodeKind::Literal(name) if name == token)
        });
        if let Some(child) = literal {
            node = child;
            rest = after;
            continue;
        }
        let argument = children
            .iter()
            .copied()
            .find_map(|child| match &tree.nodes[child].kind {
                CommandNodeKind::Argument { name, parser, .. } => Some((child, name, parser)),
                _ => None,
            });
        let Some((child, name, parser)) = argument else {
            return found;
        };
        if matches!(parser, ArgumentParser::Message) {
            found.push(SignableArgument {
                name: name.clone(),
                value: input.to_owned(),
            });
            return found;
        }
        if matches!(parser, ArgumentParser::GreedyString) {
            return found;
        }
        node = child;
        rest = consume(parser, input);
    }
}

#[cfg(test)]
mod tests {
    use super::super::tree::build_tree;
    use super::*;

    fn names(command: &str) -> Vec<(String, String)> {
        signable_arguments(&build_tree(4), command)
            .into_iter()
            .map(|argument| (argument.name, argument.value))
            .collect()
    }

    #[test]
    fn a_whisper_has_one_signable_argument_after_the_target() {
        assert_eq!(
            names("msg Alice hello there"),
            vec![("message".to_owned(), "hello there".to_owned())]
        );
        assert_eq!(
            names("w Alice  spaced"),
            vec![("message".to_owned(), " spaced".to_owned())]
        );
    }

    #[test]
    fn say_and_me_take_the_whole_remainder() {
        assert_eq!(
            names("say hi all"),
            vec![("message".to_owned(), "hi all".to_owned())]
        );
        assert_eq!(
            names("me waves"),
            vec![("action".to_owned(), "waves".to_owned())]
        );
    }

    #[test]
    fn a_selector_with_spaces_inside_brackets_is_one_token() {
        assert_eq!(
            names("msg @a[name=\"a b\"] yo"),
            vec![("message".to_owned(), "yo".to_owned())]
        );
    }

    #[test]
    fn a_command_without_message_arguments_has_none() {
        assert!(names("time set day").is_empty());
        assert!(names("msg Alice").is_empty());
        assert!(names("nothing here").is_empty());
    }

    #[test]
    fn a_redirected_execute_reaches_the_message_after_run() {
        assert_eq!(
            names("execute as @a run say hello"),
            vec![("message".to_owned(), "hello".to_owned())]
        );
    }
}
