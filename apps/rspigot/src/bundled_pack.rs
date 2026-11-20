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

    #[test]
    fn a_fresh_world_generates_the_extras_structures_without_any_copy() {
        use rspigot_core::ports::ChunkGenerator;
        use rspigot_domain::Vec3;
        use rspigot_domain::world::Dimension;

        let dir = tempfile::tempdir().unwrap();
        let datapacks = dir.path().join("world/datapacks");
        assert!(install(&datapacks).unwrap());
        let issues = rspigot_worldgen::install_datapacks(&datapacks);
        assert!(
            issues
                .iter()
                .all(|issue| issue.reason.contains("the missing template")),
            "{issues:?}"
        );
        let generator = rspigot_worldgen::WorldGenerator::from_level_type("default", 1, "");
        let from = Vec3::new(0.0, 64.0, 0.0);
        let overworld = Dimension::overworld();
        for wanted in [
            "minecraft:ancient_city",
            "minecraft:trial_chambers",
            "minecraft:trail_ruins",
            "minecraft:abandoned_camp_forest",
        ] {
            let found = generator.nearest_structure(&overworld, from, wanted);
            assert!(found.is_some(), "{wanted} was not generated");
        }
        let found =
            generator.nearest_structure(&Dimension::nether(), from, "minecraft:bastion_remnant");
        assert!(found.is_some(), "bastion_remnant was not generated");
    }

    #[test]
    fn the_readme_does_not_ask_the_user_to_copy_the_pack() {
        let readme =
            std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../README.md"))
                .unwrap();
        assert!(!readme.contains("copy that folder"));
        assert!(readme.contains("embedded in the server binary"));
        assert!(readme.contains("nothing has to be copied"));
    }
}
