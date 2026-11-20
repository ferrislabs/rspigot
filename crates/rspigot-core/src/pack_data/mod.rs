mod evaluate;
mod json;
mod table;
mod tags;

use std::sync::Arc;

use rspigot_data::loot::LootOverlay;
use rspigot_data::tags::TagOverlay;

use crate::ports::PackContents;

pub use json::{DataError, condition, predicate_file};
pub use tags::SUPPORTED_REGISTRIES;

#[derive(Debug, Clone, Default)]
pub struct PackData {
    loot: Arc<LootOverlay>,
    tags: Arc<TagOverlay>,
}

fn parse_json(text: &str) -> Result<serde_json::Value, DataError> {
    serde_json::from_str(text).map_err(|error| DataError::Invalid(error.to_string()))
}

impl PackData {
    pub fn build(packs: &[&PackContents]) -> Self {
        let mut loot = LootOverlay::default();
        for pack in packs {
            for file in &pack.predicates {
                let parsed = parse_json(&file.text).and_then(|json| predicate_file(&json));
                match parsed {
                    Ok(condition) => {
                        loot.insert_predicate(file.id.as_str(), json::leak(condition));
                    }
                    Err(error) => tracing::warn!(
                        "Failed to load predicate {} from pack {}: {error}",
                        file.id,
                        pack.name
                    ),
                }
            }
            for file in &pack.loot_tables {
                match parse_json(&file.text).and_then(|json| table::parse(&json)) {
                    Ok(outcome) => {
                        for reason in &outcome.skipped {
                            tracing::warn!(
                                "Skipped part of loot table {} from pack {}: {reason}",
                                file.id,
                                pack.name
                            );
                        }
                        loot.insert_table(file.id.as_str(), json::leak(outcome.table));
                    }
                    Err(error) => tracing::warn!(
                        "Failed to load loot table {} from pack {}: {error}",
                        file.id,
                        pack.name
                    ),
                }
            }
        }
        Self {
            loot: Arc::new(loot),
            tags: Arc::new(tags::merge(packs)),
        }
    }

    pub fn activate(&self) {
        rspigot_data::loot::activate(&self.loot);
        rspigot_data::tags::activate(&self.tags);
    }

    pub fn publish(&self) {
        rspigot_data::tags::publish((!self.tags.is_empty()).then(|| Arc::clone(&self.tags)));
    }

    pub fn loot(&self) -> &LootOverlay {
        &self.loot
    }

    pub fn tags(&self) -> &TagOverlay {
        &self.tags
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use rspigot_data::loot::{Condition, IntRange, NumberProvider};

    use super::*;
    use crate::ports::{PackFile, PackMeta};

    #[derive(Default, Clone)]
    struct Warnings(Arc<Mutex<Vec<String>>>);

    impl tracing::Subscriber for Warnings {
        fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
            *metadata.level() <= tracing::Level::WARN
        }
        fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            tracing::span::Id::from_u64(1)
        }
        fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
        fn event(&self, event: &tracing::Event<'_>) {
            struct Message(String);
            impl tracing::field::Visit for Message {
                fn record_debug(
                    &mut self,
                    field: &tracing::field::Field,
                    value: &dyn std::fmt::Debug,
                ) {
                    if field.name() == "message" {
                        self.0 = format!("{value:?}");
                    }
                }
            }
            let mut message = Message(String::new());
            event.record(&mut message);
            self.0.lock().unwrap().push(message.0);
        }
        fn enter(&self, _: &tracing::span::Id) {}
        fn exit(&self, _: &tracing::span::Id) {}
    }

    fn pack(predicates: &[(&str, &str)]) -> PackContents {
        PackContents {
            name: "p".to_owned(),
            meta: PackMeta::default(),
            predicates: predicates
                .iter()
                .map(|(id, text)| PackFile {
                    id: rspigot_domain::Identifier::parse(id).unwrap(),
                    text: (*text).to_owned(),
                })
                .collect(),
            ..PackContents::default()
        }
    }

    fn parse(text: &str) -> Condition {
        predicate_file(&serde_json::from_str(text).unwrap()).unwrap()
    }

    #[test]
    fn malformed_and_unsupported_predicates_log_a_warning_and_are_skipped() {
        let warnings = Warnings::default();
        let data = tracing::subscriber::with_default(warnings.clone(), || {
            PackData::build(&[&pack(&[
                ("t:ok", r#"{"condition":"minecraft:killed_by_player"}"#),
                ("t:bad_json", "{nope"),
                ("t:no_kind", "{}"),
                (
                    "t:far",
                    r#"{"condition":"minecraft:enchantment_active_check"}"#,
                ),
                (
                    "t:deep",
                    r#"{"condition":"minecraft:entity_properties","entity":"this","predicate":{"nbt":"{}"}}"#,
                ),
            ])])
        });
        let names: Vec<&str> = data.loot().predicate_names().collect();
        assert_eq!(names, ["t:ok"]);
        let logged = warnings.0.lock().unwrap().join("\n");
        for id in ["t:bad_json", "t:no_kind", "t:far", "t:deep"] {
            assert!(logged.contains(id), "{id} missing from {logged}");
        }
        assert!(logged.contains("enchantment_active_check"), "{logged}");
        assert!(logged.contains("nbt"), "{logged}");
    }

    #[test]
    fn conditions_parse_into_the_loot_model() {
        assert!(matches!(
            parse(r#"{"condition":"minecraft:weather_check","raining":true}"#),
            Condition::WeatherCheck {
                raining: Some(true),
                thundering: None
            }
        ));
        assert!(matches!(
            parse(
                r#"{"condition":"minecraft:time_check","value":{"min":0,"max":6000},"period":24000}"#
            ),
            Condition::TimeCheck {
                period: Some(24000),
                min: Some(0.0),
                max: Some(6000.0)
            }
        ));
        assert!(matches!(
            parse(
                r#"{"condition":"minecraft:value_check","value":{"type":"minecraft:uniform","min":1,"max":2},"range":3}"#
            ),
            Condition::ValueCheck {
                value: NumberProvider::Uniform { .. },
                min: Some(NumberProvider::Constant(3.0)),
                max: Some(NumberProvider::Constant(3.0))
            }
        ));
        let Condition::EntityScores { scores, .. } = parse(
            r#"{"condition":"minecraft:entity_scores","entity":"this","scores":{"n":{"min":2}}}"#,
        ) else {
            panic!()
        };
        assert_eq!(
            scores[0].1,
            IntRange {
                min: Some(2),
                max: None
            }
        );
        let Condition::AllOf(terms) = parse(
            r#"[{"condition":"minecraft:killed_by_player"},{"condition":"minecraft:survives_explosion"}]"#,
        ) else {
            panic!()
        };
        assert_eq!(terms.len(), 2);
    }
}
