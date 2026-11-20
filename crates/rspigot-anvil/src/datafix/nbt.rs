use rspigot_nbt::{Compound, Tag};

pub fn ensure_namespaced(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

pub fn walk_compounds<F: FnMut(&mut Compound)>(root: &mut Compound, visit: &mut F) {
    for value in root.values_mut() {
        walk_tag(value, visit);
    }
    visit(root);
}

pub fn walk_tag<F: FnMut(&mut Compound)>(tag: &mut Tag, visit: &mut F) {
    match tag {
        Tag::Compound(compound) => walk_compounds(compound, visit),
        Tag::List(items) => {
            for item in items {
                walk_tag(item, visit);
            }
        }
        _ => {}
    }
}

pub fn compound_mut<'a>(root: &'a mut Compound, key: &str) -> Option<&'a mut Compound> {
    root.get_mut(key).and_then(Tag::as_compound_mut)
}

pub fn list_mut<'a>(root: &'a mut Compound, key: &str) -> Option<&'a mut Vec<Tag>> {
    root.get_mut(key).and_then(Tag::as_list_mut)
}

pub fn rename_key(root: &mut Compound, from: &str, to: &str) {
    if let Some(value) = root.remove(from) {
        root.insert(to, value);
    }
}

pub fn rename_string<F: Fn(&str) -> Option<String>>(root: &mut Compound, key: &str, rename: &F) {
    if let Some(Tag::String(text)) = root.get_mut(key)
        && let Some(renamed) = rename(text)
    {
        *text = renamed;
    }
}

pub fn lookup(table: &[(&str, &str)], name: &str) -> Option<String> {
    table
        .iter()
        .find(|(from, _)| *from == name)
        .map(|(_, to)| (*to).to_owned())
}

pub fn is_item_stack(compound: &Compound) -> bool {
    let Some(id) = compound.get_str("id") else {
        return false;
    };
    if id == "minecraft:experience_orb" {
        return false;
    }
    matches!(
        compound.get("Count"),
        Some(Tag::Byte(_) | Tag::Short(_) | Tag::Int(_) | Tag::Long(_))
    ) || matches!(compound.get("count"), Some(Tag::Int(_) | Tag::Byte(_)))
}

pub fn walk_item_stacks<F: FnMut(&mut Compound)>(root: &mut Compound, visit: &mut F) {
    walk_compounds(root, &mut |compound| {
        if is_item_stack(compound) {
            visit(compound);
        }
    });
}

pub fn is_empty_stack(compound: &Compound) -> bool {
    !compound.contains_key("id")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_get_the_default_namespace() {
        assert_eq!(ensure_namespaced("stone"), "minecraft:stone");
        assert_eq!(ensure_namespaced("mod:stone"), "mod:stone");
    }

    #[test]
    fn item_stacks_are_found_at_any_depth() {
        let mut root = Compound::new().with(
            "Items",
            Tag::List(vec![Tag::Compound(
                Compound::new()
                    .with("id", "minecraft:chest")
                    .with("Count", 1i8)
                    .with(
                        "tag",
                        Compound::new().with(
                            "Items",
                            Tag::List(vec![Tag::Compound(
                                Compound::new()
                                    .with("id", "minecraft:stick")
                                    .with("count", 2),
                            )]),
                        ),
                    ),
            )]),
        );
        let mut seen = Vec::new();
        walk_item_stacks(&mut root, &mut |stack| {
            seen.push(stack.get_str("id").unwrap().to_owned());
        });
        assert_eq!(seen, ["minecraft:stick", "minecraft:chest"]);
    }

    #[test]
    fn experience_orbs_and_plain_entities_are_not_stacks() {
        assert!(!is_item_stack(
            &Compound::new()
                .with("id", "minecraft:experience_orb")
                .with("Count", 3)
        ));
        assert!(!is_item_stack(&Compound::new().with("id", "minecraft:cow")));
    }

    #[test]
    fn renaming_a_key_keeps_the_value() {
        let mut root = Compound::new().with("a", 1);
        rename_key(&mut root, "a", "b");
        assert_eq!(root.get_i32("b"), Some(1));
        assert!(!root.contains_key("a"));
    }
}
