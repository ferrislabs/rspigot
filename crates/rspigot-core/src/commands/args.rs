use rspigot_blocks::{BlockKind, StateId};
use rspigot_domain::text::Component;
use rspigot_nbt::{Compound, Tag};

use super::selector::split_argument;

#[derive(Debug, Clone, PartialEq)]
pub enum BlockMatcher {
    Kind(BlockKind),
    Tag(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BlockPredicate {
    pub matcher: BlockMatcher,
    pub properties: Vec<(String, String)>,
    pub nbt: Option<Compound>,
}

fn translated(key: &str, args: &[&str]) -> Component {
    Component::translatable(key, args.iter().map(|a| Component::text(*a)).collect())
}

fn namespaced(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

fn split_properties(body: &str) -> Vec<(String, String)> {
    body.split(',')
        .filter(|entry| !entry.trim().is_empty())
        .map(|entry| match entry.split_once('=') {
            Some((key, value)) => (key.trim().to_owned(), value.trim().to_owned()),
            None => (entry.trim().to_owned(), String::new()),
        })
        .collect()
}

impl BlockPredicate {
    pub fn parse(text: &str) -> Result<Self, Component> {
        let (head, nbt) = match text.find('{') {
            Some(at) => {
                let (compound, used) = rspigot_nbt::parse_compound(&text[at..])
                    .map_err(|e| Component::translatable(e.key, Vec::new()))?;
                if at + used != text.len() {
                    return Err(Component::translatable("argument.nbt.trailing", Vec::new()));
                }
                (&text[..at], Some(compound))
            }
            None => (text, None),
        };
        let (name, properties) = match head.find('[') {
            Some(at) => {
                let body = head[at + 1..].strip_suffix(']').ok_or_else(|| {
                    Component::translatable("argument.block.property.unclosed", Vec::new())
                })?;
                (&head[..at], split_properties(body))
            }
            None => (head, Vec::new()),
        };
        let matcher = match name.strip_prefix('#') {
            Some(tag) => {
                let tag = namespaced(tag);
                if rspigot_data::tags::tag("minecraft:block", &tag).is_none() {
                    return Err(translated("arguments.block.tag.unknown", &[&tag]));
                }
                BlockMatcher::Tag(tag)
            }
            None => {
                let kind = BlockKind::from_name(&namespaced(name))
                    .ok_or_else(|| translated("argument.block.id.invalid", &[&namespaced(name)]))?;
                let reference = kind.default_state();
                for (key, value) in &properties {
                    if reference.property(key).is_none() {
                        return Err(translated(
                            "argument.block.property.unknown",
                            &[&namespaced(name), key],
                        ));
                    }
                    if reference.with_property(key, value).is_none() {
                        return Err(translated(
                            "argument.block.property.invalid",
                            &[&namespaced(name), value, key],
                        ));
                    }
                }
                BlockMatcher::Kind(kind)
            }
        };
        Ok(Self {
            matcher,
            properties,
            nbt,
        })
    }

    pub fn needs_block_entity(&self) -> bool {
        self.nbt.is_some()
    }

    pub fn matches(&self, state: StateId, entity: Option<&Compound>) -> bool {
        let kind_matches = match &self.matcher {
            BlockMatcher::Kind(kind) => state.kind() == Some(*kind),
            BlockMatcher::Tag(tag) => state.kind().is_some_and(|kind| {
                rspigot_data::tags::tag("minecraft:block", tag)
                    .is_some_and(|ids| ids.binary_search(&u32::from(kind.id().0)).is_ok())
            }),
        };
        if !kind_matches {
            return false;
        }
        if !self
            .properties
            .iter()
            .all(|(key, value)| state.property(key) == Some(value.as_str()))
        {
            return false;
        }
        match &self.nbt {
            None => true,
            Some(expected) => entity.is_some_and(|actual| {
                Tag::Compound(expected.clone()).contains(&Tag::Compound(actual.clone()))
            }),
        }
    }
}

pub fn take(input: &str) -> Option<(&str, &str)> {
    let (token, rest) = split_argument(input);
    (!token.is_empty()).then_some((token, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicates_parse_kinds_tags_properties_and_nbt() {
        let stone = BlockPredicate::parse("stone").expect("stone");
        assert!(stone.matches(BlockKind::from_name("stone").unwrap().default_state(), None));
        assert!(!stone.matches(StateId::AIR, None));
        let door = BlockPredicate::parse("minecraft:oak_door[half=upper]").expect("door");
        let state = BlockKind::from_name("oak_door")
            .unwrap()
            .default_state()
            .with_property("half", "upper")
            .unwrap();
        assert!(door.matches(state, None));
        assert!(!door.matches(state.with_property("half", "lower").unwrap(), None));
        let logs = BlockPredicate::parse("#minecraft:logs").expect("logs");
        assert!(logs.matches(
            BlockKind::from_name("oak_log").unwrap().default_state(),
            None
        ));
        let chest = BlockPredicate::parse("chest{CustomName:'x'}").expect("chest");
        assert!(chest.needs_block_entity());
        let actual = rspigot_nbt::parse_compound("{CustomName:'x',Lock:''}")
            .unwrap()
            .0;
        let state = BlockKind::from_name("chest").unwrap().default_state();
        assert!(chest.matches(state, Some(&actual)));
        assert!(!chest.matches(state, None));
    }

    #[test]
    fn bad_predicates_fail_with_vanilla_keys() {
        let key = |text: &str| match BlockPredicate::parse(text).unwrap_err().content {
            rspigot_domain::text::Content::Translatable { key, .. } => key,
            other => panic!("{other:?}"),
        };
        assert_eq!(key("nope"), "argument.block.id.invalid");
        assert_eq!(key("stone[x=y]"), "argument.block.property.unknown");
        assert_eq!(
            key("oak_door[half=side]"),
            "argument.block.property.invalid"
        );
        assert_eq!(key("#minecraft:nope"), "arguments.block.tag.unknown");
        assert_eq!(key("stone[half=upper"), "argument.block.property.unclosed");
    }
}
