use std::collections::BTreeMap;
use std::fmt::Write;

use crate::tree::{Handle, Payload, Tree, Verb, snake};

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
    out.push_str("use rspigot_plugin_protocol::v1::{self, from_plugin};\n\n");
    out.push_str("use crate::link::Link;\n\n");
    out.push_str("#[derive(Debug, Clone)]\n");
    let _ = writeln!(out, "pub struct {} {{", handle.name);
    let targeted = handle
        .verbs
        .first()
        .is_some_and(|verb| verb.target.is_some());
    if targeted {
        out.push_str("    pub(crate) target: String,\n");
    }
    out.push_str("    pub(crate) link: Link,\n}\n\n");
    let _ = writeln!(out, "impl {} {{", handle.name);
    if targeted {
        out.push_str(
            "    #[allow(dead_code)]\n    pub(crate) fn new(target: String, link: Link) -> Self {\n",
        );
        out.push_str("        Self { target, link }\n    }\n");
    } else {
        out.push_str("    #[allow(dead_code)]\n    pub(crate) fn new(link: Link) -> Self {\n");
        out.push_str("        Self { link }\n    }\n");
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
        let _ = writeln!(out, "            {target}: self.target.clone(),");
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
