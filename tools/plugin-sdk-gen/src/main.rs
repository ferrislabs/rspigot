use std::env;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use plugin_sdk_gen::{Error, describe, matches, render, write};

fn main() -> ExitCode {
    let mut check = false;
    for argument in env::args().skip(1) {
        match argument.as_str() {
            "--check" => check = true,
            other => {
                eprintln!("unknown argument `{other}`, the only option is `--check`");
                return ExitCode::FAILURE;
            }
        }
    }
    match run(check) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => {
            eprintln!(
                "the generated SDK is out of date, run `cargo run --manifest-path tools/plugin-sdk-gen/Cargo.toml`"
            );
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("generation failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(check: bool) -> Result<bool, Error> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates");
    let protocol = root.join("rspigot-plugin-protocol");
    let output = root.join("rspigot-plugin/src/generated");

    let scratch = env::temp_dir().join(format!("plugin-sdk-gen-{}", std::process::id()));
    fs::create_dir_all(&scratch)?;
    let fresh = describe(&protocol, &scratch).and_then(|set| render(&set));
    let _ = fs::remove_dir_all(&scratch);
    let fresh = fresh?;

    if check {
        matches(&output, &fresh)
    } else {
        write(&output, &fresh)?;
        Ok(true)
    }
}
