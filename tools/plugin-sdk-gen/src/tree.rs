use crate::Error;
use crate::descriptor::{DescriptorProto, FieldDescriptorProto, FileDescriptorSet};

const WIRE_PACKAGE: &str = "rspigot.plugin.v1";
const ENVELOPE: &str = "FromPlugin";
const EMPTY: &str = "Empty";

#[derive(Debug, PartialEq, Eq)]
pub enum Payload {
    Nothing,
    Message(String),
    Text,
    Scalar(&'static str),
    Enum(String),
}

#[derive(Debug)]
pub struct Verb {
    pub name: String,
    pub variant: String,
    pub order: String,
    pub body: String,
    pub oneof: String,
    pub target: Option<String>,
    pub payload: Payload,
}

#[derive(Debug)]
pub struct Handle {
    pub path: String,
    pub name: String,
    pub verbs: Vec<Verb>,
}

#[derive(Debug)]
pub struct Tree {
    handles: Vec<Handle>,
}

impl Tree {
    pub fn handles(&self) -> &[Handle] {
        &self.handles
    }

    pub fn build(set: &FileDescriptorSet) -> Result<Self, Error> {
        let messages: Vec<&DescriptorProto> = set
            .file
            .iter()
            .filter(|file| file.package.as_deref() == Some(WIRE_PACKAGE))
            .flat_map(|file| file.message_type.iter())
            .collect();
        let envelope = messages
            .iter()
            .find(|message| message.name.as_deref() == Some(ENVELOPE));

        let mut handles: Vec<Handle> = Vec::new();
        for message in &messages {
            for field in &message.field {
                let Some(path) = field.options.as_ref().and_then(|o| o.path.as_deref()) else {
                    continue;
                };
                let verb = verb(message, field, path, envelope.copied())?;
                let domain = domain(path)?;
                match handles.iter_mut().find(|handle| handle.path == domain) {
                    Some(handle) => {
                        if handle.verbs.iter().any(|known| known.name == verb.name) {
                            return Err(Error::Path {
                                path: path.to_owned(),
                                reason: format!("the verb `{}` is already taken", verb.name),
                            });
                        }
                        if handle.verbs[0].target.is_some() != verb.target.is_some() {
                            return Err(Error::Path {
                                path: path.to_owned(),
                                reason: "a domain cannot mix orders with and without a target"
                                    .to_owned(),
                            });
                        }
                        handle.verbs.push(verb);
                    }
                    None => handles.push(Handle {
                        path: domain.to_owned(),
                        name: format!("{}Handle", pascal(&domain.replace('.', "_"))),
                        verbs: vec![verb],
                    }),
                }
            }
        }
        handles.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Self { handles })
    }
}

fn domain(path: &str) -> Result<&str, Error> {
    let invalid = |reason: &str| Error::Path {
        path: path.to_owned(),
        reason: reason.to_owned(),
    };
    let segments: Vec<&str> = path.split('.').collect();
    if segments.len() < 3 || segments.iter().any(|segment| segment.is_empty()) {
        return Err(invalid("expected `<target>.<domain>.<verb>`"));
    }
    path.rsplit_once('.')
        .map(|(domain, _)| domain)
        .ok_or_else(|| invalid("expected `<target>.<domain>.<verb>`"))
}

fn verb(
    order: &DescriptorProto,
    field: &FieldDescriptorProto,
    path: &str,
    envelope: Option<&DescriptorProto>,
) -> Result<Verb, Error> {
    let invalid = |reason: String| Error::Path {
        path: path.to_owned(),
        reason,
    };
    let order_name = order.name.clone().unwrap_or_default();
    let field_name = field.name.clone().unwrap_or_default();
    let oneof = field
        .oneof_index
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| order.oneof_decl.get(index))
        .and_then(|oneof| oneof.name.clone())
        .ok_or_else(|| invalid("the field is not part of a oneof".to_owned()))?;
    let target = match order
        .field
        .iter()
        .find(|candidate| candidate.number == Some(1) && candidate.oneof_index.is_none())
    {
        Some(candidate) if candidate.r#type == Some(9) => candidate.name.clone(),
        Some(_) => {
            return Err(invalid(format!(
                "the target in field 1 of `{order_name}` is not a string"
            )));
        }
        None => None,
    };
    let body = envelope
        .and_then(|envelope| {
            envelope.field.iter().find(|candidate| {
                candidate
                    .type_name
                    .as_deref()
                    .is_some_and(|name| name == format!(".{WIRE_PACKAGE}.{order_name}"))
            })
        })
        .and_then(|candidate| candidate.name.as_deref())
        .map(pascal)
        .ok_or_else(|| invalid(format!("`{ENVELOPE}` does not carry `{order_name}`")))?;
    let verb_name = path.rsplit('.').next().unwrap_or(path).to_owned();
    Ok(Verb {
        name: verb_name,
        variant: pascal(&field_name),
        order: order_name,
        body,
        oneof: pascal(&oneof),
        target,
        payload: payload(field).map_err(invalid)?,
    })
}

fn payload(field: &FieldDescriptorProto) -> Result<Payload, String> {
    match field.r#type {
        Some(14) => {
            let name = field.type_name.as_deref().unwrap_or_default();
            Ok(Payload::Enum(
                name.rsplit('.').next().unwrap_or(name).to_owned(),
            ))
        }
        Some(11) => {
            let name = field.type_name.as_deref().unwrap_or_default();
            let short = name.rsplit('.').next().unwrap_or(name);
            if short == EMPTY {
                Ok(Payload::Nothing)
            } else {
                Ok(Payload::Message(short.to_owned()))
            }
        }
        Some(9) => Ok(Payload::Text),
        Some(8) => Ok(Payload::Scalar("bool")),
        Some(5) => Ok(Payload::Scalar("i32")),
        Some(13) => Ok(Payload::Scalar("u32")),
        Some(3) => Ok(Payload::Scalar("i64")),
        Some(4) => Ok(Payload::Scalar("u64")),
        Some(2) => Ok(Payload::Scalar("f32")),
        Some(1) => Ok(Payload::Scalar("f64")),
        other => Err(format!("unsupported field type {other:?}")),
    }
}

pub fn pascal(snake: &str) -> String {
    snake
        .split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect::<String>())
                .unwrap_or_default()
        })
        .collect()
}

pub fn snake(pascal: &str) -> String {
    let mut out = String::new();
    for (index, character) in pascal.chars().enumerate() {
        if character.is_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.extend(character.to_lowercase());
        } else {
            out.push(character);
        }
    }
    out
}
