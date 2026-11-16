use std::io;
use std::path::Path;

include!(concat!(env!("OUT_DIR"), "/bundled_pack.rs"));

pub fn install(datapacks: &Path) -> io::Result<bool> {
    let root = datapacks.join(PACK_NAME);
    if root.exists() {
        return Ok(false);
    }
    for (relative, bytes) in FILES {
        let target = root.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, bytes)?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_the_pack_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let datapacks = dir.path().join("datapacks");
        assert!(install(&datapacks).unwrap());
        assert!(datapacks.join("vanilla-extras/pack.mcmeta").is_file());
        assert!(
            datapacks
                .join("vanilla-extras/data/minecraft/worldgen/processor_list/bridge.json")
                .is_file()
        );
    }

    #[test]
    fn never_overwrites_an_existing_pack() {
        let dir = tempfile::tempdir().unwrap();
        let datapacks = dir.path().join("datapacks");
        let pack = datapacks.join("vanilla-extras");
        std::fs::create_dir_all(&pack).unwrap();
        std::fs::write(pack.join("pack.mcmeta"), "mine").unwrap();
        assert!(!install(&datapacks).unwrap());
        assert_eq!(
            std::fs::read_to_string(pack.join("pack.mcmeta")).unwrap(),
            "mine"
        );
        assert!(!pack.join("data").exists());
    }
}
