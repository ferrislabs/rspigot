use std::fmt::Write;
use std::path::{Path, PathBuf};

fn collect(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect(&path, base, out);
        } else {
            out.push(path.strip_prefix(base).unwrap().to_owned());
        }
    }
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let pack = manifest
        .join("../../datapacks/vanilla-extras")
        .canonicalize()
        .unwrap();
    println!("cargo:rerun-if-changed={}", pack.display());
    let mut files = Vec::new();
    collect(&pack, &pack, &mut files);
    let mut source = String::from(
        "const PACK_NAME: &str = \"vanilla-extras\";\nconst FILES: &[(&str, &[u8])] = &[\n",
    );
    for relative in &files {
        let name = relative.to_string_lossy().replace('\\', "/");
        writeln!(
            source,
            "    ({name:?}, include_bytes!({:?})),",
            pack.join(relative).to_string_lossy()
        )
        .unwrap();
    }
    source.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("bundled_pack.rs");
    std::fs::write(out, source).unwrap();
}
