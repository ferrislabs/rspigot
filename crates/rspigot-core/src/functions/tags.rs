use rspigot_domain::Identifier;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reference {
    Function(Identifier),
    Tag(Identifier),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagEntry {
    pub reference: Reference,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TagFile {
    pub replace: bool,
    pub entries: Vec<TagEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum TagError {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("tag file is not an object")]
    NotAnObject,
    #[error("invalid tag entry: {0}")]
    Entry(String),
}

fn reference(text: &str) -> Result<Reference, TagError> {
    let invalid = || TagError::Entry(text.to_owned());
    match text.strip_prefix('#') {
        Some(tag) => Identifier::parse(tag)
            .map(Reference::Tag)
            .map_err(|_| invalid()),
        None => Identifier::parse(text)
            .map(Reference::Function)
            .map_err(|_| invalid()),
    }
}

impl TagFile {
    pub fn parse(text: &str) -> Result<Self, TagError> {
        let root: serde_json::Value = serde_json::from_str(text)?;
        let object = root.as_object().ok_or(TagError::NotAnObject)?;
        let replace = object
            .get("replace")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let mut entries = Vec::new();
        for value in object
            .get("values")
            .and_then(serde_json::Value::as_array)
            .map_or(&[][..], Vec::as_slice)
        {
            let entry = match value {
                serde_json::Value::String(text) => TagEntry {
                    reference: reference(text)?,
                    required: true,
                },
                serde_json::Value::Object(object) => {
                    let id = object
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| TagError::Entry(value.to_string()))?;
                    TagEntry {
                        reference: reference(id)?,
                        required: object
                            .get("required")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(true),
                    }
                }
                other => return Err(TagError::Entry(other.to_string())),
            };
            entries.push(entry);
        }
        Ok(Self { replace, entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(text: &str) -> Identifier {
        Identifier::parse(text).unwrap()
    }

    #[test]
    fn values_replace_and_optional_entries_parse() {
        let file = TagFile::parse(
            r##"{"replace":true,"values":["a:one","#a:group",{"id":"a:two","required":false},{"id":"a:three"}]}"##,
        )
        .unwrap();
        assert!(file.replace);
        assert_eq!(
            file.entries,
            [
                TagEntry {
                    reference: Reference::Function(id("a:one")),
                    required: true
                },
                TagEntry {
                    reference: Reference::Tag(id("a:group")),
                    required: true
                },
                TagEntry {
                    reference: Reference::Function(id("a:two")),
                    required: false
                },
                TagEntry {
                    reference: Reference::Function(id("a:three")),
                    required: true
                },
            ]
        );
    }

    #[test]
    fn a_missing_values_list_is_empty() {
        let file = TagFile::parse("{}").unwrap();
        assert!(!file.replace);
        assert!(file.entries.is_empty());
    }

    #[test]
    fn broken_files_are_errors() {
        assert!(TagFile::parse("[]").is_err());
        assert!(TagFile::parse("{").is_err());
        assert!(TagFile::parse(r#"{"values":[7]}"#).is_err());
        assert!(TagFile::parse(r#"{"values":["Bad Id"]}"#).is_err());
    }
}
