use std::collections::BTreeMap;
use std::fmt::Write;

use crate::tree::{Handle, Payload, Tree, Verb, snake};

const SNAPSHOT_VALUES: &[(&str, &str)] = &[("player.game_mode", "GameMode")];

fn snapshot_value(handle: &Handle) -> Option<&'static str> {
    SNAPSHOT_VALUES
        .iter()
        .find(|(path, _)| *path == handle.path)
        .map(|(_, kind)| *kind)
}

pub fn files(tree: &Tree) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let mut index = String::new();
    for handle in tree.handles() {
        let module = module(handle);
        let _ = writeln!(index, "mod {module};");
        let _ = writeln!(index, "pub use {module}::{};", handle.name);
        files.insert(format!("{module}.rs"), handle_file(handle));
    }
    files.insert("mod.rs".to_owned(), index);
    files
}

fn module(handle: &Handle) -> String {
    handle.path.replace('.', "_")
}

fn handle_file(handle: &Handle) -> String {
    let mut out = String::new();
    let value = snapshot_value(handle);
    let targeted = handle
        .verbs
        .first()
        .is_some_and(|verb| verb.target.is_some());
    out.push_str("use rspigot_plugin_protocol::v1::{self, from_plugin};\n\n");
    if targeted {
        out.push_str("use std::sync::Arc;\n\n");
    }
    out.push_str("use crate::link::Link;\n");
    if let Some(kind) = value {
        let _ = writeln!(out, "use crate::{kind};");
    }
    out.push_str("\n#[derive(Debug, Clone)]\n");
    let _ = writeln!(out, "pub struct {} {{", handle.name);
    if targeted {
        out.push_str("    pub(crate) target: Arc<str>,\n");
    }
    out.push_str("    pub(crate) link: Link,\n");
    if let Some(kind) = value {
        let _ = writeln!(out, "    pub(crate) value: {kind},");
    }
    out.push_str("}\n\n");
    let _ = writeln!(out, "impl {} {{", handle.name);
    let mut parameters = Vec::new();
    let mut fields = Vec::new();
    if targeted {
        parameters.push("target: Arc<str>".to_owned());
        fields.push("target");
    }
    parameters.push("link: Link".to_owned());
    fields.push("link");
    if let Some(kind) = value {
        parameters.push(format!("value: {kind}"));
        fields.push("value");
    }
    let _ = writeln!(
        out,
        "    #[allow(dead_code)]\n    pub(crate) fn new({}) -> Self {{",
        parameters.join(", ")
    );
    let _ = writeln!(out, "        Self {{ {} }}\n    }}", fields.join(", "));
    if let Some(kind) = value {
        let _ = writeln!(
            out,
            "\n    pub const fn value(&self) -> {kind} {{\n        self.value\n    }}"
        );
    }
    for verb in &handle.verbs {
        out.push('\n');
        out.push_str(&method(verb));
    }
    out.push_str("}\n");
    out
}

fn method(verb: &Verb) -> String {
    let (parameter, value) = match &verb.payload {
        Payload::Nothing => (String::new(), "v1::Empty {}".to_owned()),
        Payload::Message(name) => (
            format!(", value: impl Into<v1::{name}>"),
            "value.into()".to_owned(),
        ),
        Payload::Text => (
            ", value: impl Into<String>".to_owned(),
            "value.into()".to_owned(),
        ),
        Payload::Scalar(kind) if matches!(*kind, "u32" | "u64") => (
            format!(", value: impl Into<{kind}>"),
            "value.into()".to_owned(),
        ),
        Payload::Scalar(kind) => (format!(", value: {kind}"), "value".to_owned()),
        Payload::Enum(name) => (
            format!(", value: impl Into<v1::{name}>"),
            format!("Into::<v1::{name}>::into(value) as i32"),
        ),
    };
    let module = snake(&verb.order);
    let mut out = String::new();
    let _ = writeln!(out, "    pub fn {}(&self{parameter}) {{", verb.name);
    let _ = writeln!(
        out,
        "        self.link.order(from_plugin::Body::{}(v1::{} {{",
        verb.body, verb.order
    );
    if let Some(target) = &verb.target {
        let _ = writeln!(out, "            {target}: self.target.to_string(),");
    }
    let _ = writeln!(
        out,
        "            {}: Some(v1::{module}::{}::{}({value})),",
        verb.oneof.to_lowercase(),
        verb.oneof,
        verb.variant
    );
    out.push_str("        }));\n    }\n");
    out
}
