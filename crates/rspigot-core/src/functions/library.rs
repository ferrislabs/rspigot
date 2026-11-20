use std::collections::BTreeMap;
use std::sync::Arc;

use rspigot_domain::Identifier;
use rustc_hash::{FxHashMap, FxHashSet};

use super::parse::Function;
use super::tags::{Reference, TagFile};
use crate::ports::PackContents;

#[derive(Debug, Clone, Default)]
pub struct FunctionLibrary {
    functions: FxHashMap<Identifier, Arc<Function>>,
    tags: FxHashMap<Identifier, Vec<Arc<Function>>>,
}

impl FunctionLibrary {
    pub fn build<'a>(packs: impl IntoIterator<Item = &'a PackContents>) -> Self {
        let mut functions: FxHashMap<Identifier, Arc<Function>> = FxHashMap::default();
        let mut definitions: BTreeMap<Identifier, Vec<super::tags::TagEntry>> = BTreeMap::new();
        for pack in packs {
            for file in &pack.functions {
                match Function::parse(file.id.clone(), &file.text) {
                    Ok(function) => {
                        functions.insert(file.id.clone(), Arc::new(function));
                    }
                    Err(error) => {
                        tracing::warn!(
                            "Failed to load function {} from pack {}: {error}",
                            file.id,
                            pack.name
                        );
                    }
                }
            }
            for file in &pack.function_tags {
                match TagFile::parse(&file.text) {
                    Ok(tag) => {
                        let entries = definitions.entry(file.id.clone()).or_default();
                        if tag.replace {
                            entries.clear();
                        }
                        entries.extend(tag.entries);
                    }
                    Err(error) => {
                        tracing::warn!(
                            "Failed to load function tag {} from pack {}: {error}",
                            file.id,
                            pack.name
                        );
                    }
                }
            }
        }
        let mut tags = FxHashMap::default();
        for id in definitions.keys() {
            let mut stack = vec![id.clone()];
            match resolve(id, &definitions, &functions, &mut stack) {
                Ok(members) => {
                    tags.insert(id.clone(), members);
                }
                Err(missing) => {
                    tracing::warn!(
                        "Couldn't load function tag {id} as it is missing following references: {missing}"
                    );
                }
            }
        }
        Self { functions, tags }
    }

    pub fn function(&self, id: &Identifier) -> Option<&Arc<Function>> {
        self.functions.get(id)
    }

    pub fn tag(&self, id: &Identifier) -> Option<&[Arc<Function>]> {
        self.tags.get(id).map(Vec::as_slice)
    }

    pub fn function_ids(&self) -> Vec<&Identifier> {
        let mut ids: Vec<&Identifier> = self.functions.keys().collect();
        ids.sort();
        ids
    }

    pub fn tag_ids(&self) -> Vec<&Identifier> {
        let mut ids: Vec<&Identifier> = self.tags.keys().collect();
        ids.sort();
        ids
    }

    pub fn len(&self) -> usize {
        self.functions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }
}

fn resolve(
    id: &Identifier,
    definitions: &BTreeMap<Identifier, Vec<super::tags::TagEntry>>,
    functions: &FxHashMap<Identifier, Arc<Function>>,
    stack: &mut Vec<Identifier>,
) -> Result<Vec<Arc<Function>>, String> {
    let mut members: Vec<Arc<Function>> = Vec::new();
    let mut seen: FxHashSet<Identifier> = FxHashSet::default();
    let mut missing = Vec::new();
    for entry in definitions.get(id).map_or(&[][..], Vec::as_slice) {
        match &entry.reference {
            Reference::Function(function) => match functions.get(function) {
                Some(found) => {
                    if seen.insert(function.clone()) {
                        members.push(Arc::clone(found));
                    }
                }
                None if entry.required => missing.push(function.to_string()),
                None => {}
            },
            Reference::Tag(nested) => {
                let resolved = if stack.contains(nested) || !definitions.contains_key(nested) {
                    Err(format!("#{nested}"))
                } else {
                    stack.push(nested.clone());
                    let resolved = resolve(nested, definitions, functions, stack);
                    stack.pop();
                    resolved.map_err(|_| format!("#{nested}"))
                };
                match resolved {
                    Ok(found) => {
                        for function in found {
                            if seen.insert(function.id().clone()) {
                                members.push(function);
                            }
                        }
                    }
                    Err(name) if entry.required => missing.push(name),
                    Err(_) => {}
                }
            }
        }
    }
    if missing.is_empty() {
        Ok(members)
    } else {
        Err(missing.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{PackFile, PackMeta};

    fn id(text: &str) -> Identifier {
        Identifier::parse(text).unwrap()
    }

    fn file(name: &str, text: &str) -> PackFile {
        PackFile {
            id: id(name),
            text: text.to_owned(),
        }
    }

    fn pack(name: &str, functions: &[(&str, &str)], tags: &[(&str, &str)]) -> PackContents {
        PackContents {
            name: name.to_owned(),
            meta: PackMeta::default(),
            functions: functions.iter().map(|(n, t)| file(n, t)).collect(),
            function_tags: tags.iter().map(|(n, t)| file(n, t)).collect(),
            ..PackContents::default()
        }
    }

    fn members(library: &FunctionLibrary, tag: &str) -> Vec<String> {
        library
            .tag(&id(tag))
            .map(|found| found.iter().map(|f| f.id().to_string()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn a_later_pack_overrides_a_function_by_id() {
        let first = pack("a", &[("t:f", "say old")], &[]);
        let second = pack("b", &[("t:f", "say new")], &[]);
        let library = FunctionLibrary::build([&first, &second]);
        let function = library.function(&id("t:f")).unwrap();
        assert_eq!(function.commands(None).unwrap(), ["say new"]);
        assert_eq!(library.len(), 1);
    }

    #[test]
    fn a_broken_function_is_skipped() {
        let broken = pack("a", &[("t:bad", "/say x"), ("t:ok", "say y")], &[]);
        let library = FunctionLibrary::build([&broken]);
        assert!(library.function(&id("t:bad")).is_none());
        assert!(library.function(&id("t:ok")).is_some());
    }

    #[test]
    fn tags_merge_across_packs_and_replace_drops_earlier_values() {
        let first = pack(
            "a",
            &[("t:a", ""), ("t:b", ""), ("t:c", "")],
            &[("t:group", r#"{"values":["t:a","t:b"]}"#)],
        );
        let appended = pack("b", &[], &[("t:group", r#"{"values":["t:c","t:a"]}"#)]);
        let library = FunctionLibrary::build([&first, &appended]);
        assert_eq!(members(&library, "t:group"), ["t:a", "t:b", "t:c"]);
        let replaced = pack(
            "c",
            &[],
            &[("t:group", r#"{"replace":true,"values":["t:c"]}"#)],
        );
        let library = FunctionLibrary::build([&first, &appended, &replaced]);
        assert_eq!(members(&library, "t:group"), ["t:c"]);
    }

    #[test]
    fn nested_tags_flatten_in_order_without_duplicates() {
        let source = pack(
            "a",
            &[("t:a", ""), ("t:b", ""), ("t:c", "")],
            &[
                ("t:inner", r#"{"values":["t:b","t:c"]}"#),
                ("t:outer", r##"{"values":["t:a","#t:inner","t:b"]}"##),
            ],
        );
        let library = FunctionLibrary::build([&source]);
        assert_eq!(members(&library, "t:outer"), ["t:a", "t:b", "t:c"]);
    }

    #[test]
    fn optional_entries_may_be_missing_and_required_ones_may_not() {
        let source = pack(
            "a",
            &[("t:a", "")],
            &[
                (
                    "t:soft",
                    r##"{"values":["t:a",{"id":"t:nope","required":false},{"id":"#t:nope_tag","required":false}]}"##,
                ),
                ("t:hard", r#"{"values":["t:a","t:nope"]}"#),
                ("t:chain", r##"{"values":["#t:hard"]}"##),
            ],
        );
        let library = FunctionLibrary::build([&source]);
        assert_eq!(members(&library, "t:soft"), ["t:a"]);
        assert!(library.tag(&id("t:hard")).is_none());
        assert!(library.tag(&id("t:chain")).is_none());
    }

    #[test]
    fn tag_cycles_fail_instead_of_looping() {
        let source = pack(
            "a",
            &[("t:a", "")],
            &[
                ("t:one", r##"{"values":["t:a","#t:two"]}"##),
                ("t:two", r##"{"values":["#t:one"]}"##),
            ],
        );
        let library = FunctionLibrary::build([&source]);
        assert!(library.tag(&id("t:one")).is_none());
        assert!(library.tag(&id("t:two")).is_none());
    }
}
