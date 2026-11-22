use std::collections::BTreeMap;

use rspigot_data::loot::Function;
use serde_json::Value;

use super::DataError;
use crate::ports::PackItemModifier;

pub type ItemModifiers = BTreeMap<String, &'static [Function]>;

pub fn parse_functions(value: &Value) -> Result<&'static [Function], DataError> {
    super::functions::function_list(value)
}

pub fn collect(files: &[PackItemModifier], pack: &str, into: &mut ItemModifiers) {
    for file in files {
        let parsed = super::parse_json(&file.text).and_then(|json| parse_functions(&json));
        match parsed {
            Ok(functions) => {
                into.insert(file.id.to_string(), functions);
            }
            Err(error) => tracing::warn!(
                "Failed to load item modifier {} from pack {pack}: {error}",
                file.id
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use rspigot_data::loot::FunctionKind;

    use super::*;
    use crate::pack_data::PackData;
    use crate::ports::{PackContents, PackFile};

    fn file(id: &str, text: &str) -> PackFile {
        PackFile {
            id: rspigot_domain::Identifier::parse(id).unwrap(),
            text: text.to_owned(),
        }
    }

    #[test]
    fn a_file_is_one_function_or_an_array_of_them() {
        let single = serde_json::json!({
            "function": "minecraft:set_count",
            "count": 3
        });
        assert!(matches!(
            parse_functions(&single).unwrap()[0].kind,
            FunctionKind::SetCount { .. }
        ));
        let many = serde_json::json!([single, single]);
        assert_eq!(parse_functions(&many).unwrap().len(), 2);
    }

    #[test]
    fn modifiers_are_kept_by_id_and_bad_files_are_skipped() {
        let pack = PackContents {
            name: "p".to_owned(),
            item_modifiers: vec![
                file(
                    "t:triple",
                    r#"{"function":"minecraft:set_count","count":3}"#,
                ),
                file("t:broken", "{nope"),
            ],
            ..PackContents::default()
        };
        let data = PackData::build(&[&pack]);
        assert_eq!(data.item_modifier_ids().collect::<Vec<_>>(), ["t:triple"]);
        assert_eq!(data.item_modifier("t:triple").unwrap().len(), 1);
        assert!(data.item_modifier("t:broken").is_none());
    }
}
