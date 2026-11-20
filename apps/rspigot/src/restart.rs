use std::path::{Path, PathBuf};

pub const RESTART_EXIT_CODE: i32 = 3;

#[derive(Debug, PartialEq, Eq)]
pub enum Plan {
    Script(PathBuf),
    Exit(i32),
}

pub fn plan(root: &Path, script: &str) -> Plan {
    let script = script.trim();
    if script.is_empty() {
        return Plan::Exit(RESTART_EXIT_CODE);
    }
    let path = root.join(script);
    if path.is_file() {
        Plan::Script(path)
    } else {
        Plan::Exit(RESTART_EXIT_CODE)
    }
}

pub fn perform(root: &Path, script: &str) {
    match plan(root, script) {
        Plan::Script(path) => {
            tracing::info!("Attempting to restart with {}", path.display());
            match std::process::Command::new("sh")
                .arg(&path)
                .current_dir(root)
                .spawn()
            {
                Ok(_) => std::process::exit(0),
                Err(error) => {
                    tracing::warn!("Could not start the restart script: {error}");
                    std::process::exit(RESTART_EXIT_CODE);
                }
            }
        }
        Plan::Exit(code) => {
            tracing::warn!(
                "No restart script found at settings.restart-script in spigot.yml, exiting with code {code}"
            );
            std::process::exit(code);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_or_empty_script_exits_with_the_restart_code() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(plan(dir.path(), ""), Plan::Exit(RESTART_EXIT_CODE));
        assert_eq!(
            plan(dir.path(), "./absent.sh"),
            Plan::Exit(RESTART_EXIT_CODE)
        );
    }

    #[test]
    fn an_existing_script_is_run_from_the_server_folder() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("start.sh"), "#!/bin/sh\n").unwrap();
        assert_eq!(
            plan(dir.path(), "./start.sh"),
            Plan::Script(dir.path().join("./start.sh"))
        );
    }
}
