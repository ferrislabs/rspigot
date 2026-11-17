pub mod descriptor;
pub mod emit;
pub mod tree;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use prost::Message;

use crate::descriptor::FileDescriptorSet;
use crate::tree::Tree;

const PROTO_FILE: &str = "rspigot/plugin/v1/plugin.proto";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Compile(String),
    #[error("{0}")]
    Decode(#[from] prost::DecodeError),
    #[error("path `{path}`: {reason}")]
    Path { path: String, reason: String },
    #[error("{0}")]
    Unsupported(String),
}

pub fn describe(protocol_dir: &Path, scratch: &Path) -> Result<FileDescriptorSet, Error> {
    let proto_root = protocol_dir.join("proto");
    let descriptor_path = scratch.join("descriptor.bin");
    tonic_prost_build::configure()
        .build_server(false)
        .build_client(false)
        .build_transport(false)
        .emit_rerun_if_changed(false)
        .out_dir(scratch)
        .file_descriptor_set_path(&descriptor_path)
        .compile_protos(&[proto_root.join(PROTO_FILE)], &[proto_root])
        .map_err(|error| Error::Compile(error.to_string()))?;
    Ok(FileDescriptorSet::decode(
        fs::read(&descriptor_path)?.as_slice(),
    )?)
}

pub fn render(set: &FileDescriptorSet) -> Result<BTreeMap<String, String>, Error> {
    Ok(emit::files(&Tree::build(set)?))
}

pub fn matches(directory: &Path, fresh: &BTreeMap<String, String>) -> Result<bool, Error> {
    let mut found = BTreeMap::new();
    if directory.is_dir() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            found.insert(
                entry.file_name().to_string_lossy().into_owned(),
                fs::read_to_string(entry.path())?,
            );
        }
    }
    Ok(&found == fresh)
}

pub fn write(directory: &Path, fresh: &BTreeMap<String, String>) -> Result<(), Error> {
    if directory.is_dir() {
        fs::remove_dir_all(directory)?;
    }
    fs::create_dir_all(directory)?;
    for (name, content) in fresh {
        fs::write(directory.join(name), content)?;
    }
    Ok(())
}
