use std::path::{Path, PathBuf};

use rspigot_domain::Identifier;

use crate::error::AnvilError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DimensionLayout {
    #[default]
    Spigot,
    Vanilla,
    LegacySpigot,
    LegacyVanilla,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Known {
    Overworld,
    Nether,
    End,
    Custom,
}

impl DimensionLayout {
    pub fn region_dir(
        self,
        world_root: &Path,
        dimension: &Identifier,
    ) -> Result<PathBuf, AnvilError> {
        Ok(self.dimension_dir(world_root, dimension)?.join("region"))
    }

    pub fn entities_dir(
        self,
        world_root: &Path,
        dimension: &Identifier,
    ) -> Result<PathBuf, AnvilError> {
        Ok(self.dimension_dir(world_root, dimension)?.join("entities"))
    }

    pub fn poi_dir(self, world_root: &Path, dimension: &Identifier) -> Result<PathBuf, AnvilError> {
        Ok(self.dimension_dir(world_root, dimension)?.join("poi"))
    }

    pub fn dimension_dir(
        self,
        world_root: &Path,
        dimension: &Identifier,
    ) -> Result<PathBuf, AnvilError> {
        let namespace = dimension.namespace();
        let path = dimension.path();
        check_component(namespace)?;
        path.split('/').try_for_each(check_component)?;
        let known = match (namespace, path) {
            ("minecraft", "overworld") => Known::Overworld,
            ("minecraft", "the_nether") => Known::Nether,
            ("minecraft", "the_end") => Known::End,
            _ => Known::Custom,
        };
        let nested = |base: &Path| base.join("dimensions").join(namespace).join(path);
        let folder = match self {
            Self::Vanilla => nested(world_root),
            Self::LegacyVanilla => match known {
                Known::Overworld => world_root.to_path_buf(),
                Known::Nether => world_root.join("DIM-1"),
                Known::End => world_root.join("DIM1"),
                Known::Custom => nested(world_root),
            },
            Self::Spigot => {
                let world = spigot_world(world_root, known, namespace, path)?;
                nested(&world)
            }
            Self::LegacySpigot => {
                let world = spigot_world(world_root, known, namespace, path)?;
                match known {
                    Known::Overworld => world,
                    Known::Nether => world.join("DIM-1"),
                    Known::End => world.join("DIM1"),
                    Known::Custom => nested(&world),
                }
            }
        };
        Ok(folder)
    }
}

fn spigot_world(
    world_root: &Path,
    known: Known,
    namespace: &str,
    path: &str,
) -> Result<PathBuf, AnvilError> {
    if known == Known::Overworld {
        return Ok(world_root.to_path_buf());
    }
    let name = world_root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            AnvilError::InvalidPath(format!(
                "{} has no folder name to derive the dimension folders from",
                world_root.display()
            ))
        })?;
    let suffix = match known {
        Known::Nether => "nether".to_owned(),
        Known::End => "the_end".to_owned(),
        _ => format!("{namespace}_{path}"),
    };
    let container = world_root.parent().unwrap_or_else(|| Path::new(""));
    Ok(container.join(format!("{name}_{suffix}")))
}

fn check_component(component: &str) -> Result<(), AnvilError> {
    if component.is_empty() || component == "." || component == ".." {
        return Err(AnvilError::InvalidPath(format!(
            "dimension component `{component}` is not a safe folder name"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> Identifier {
        Identifier::parse(name).unwrap()
    }

    fn dir(layout: DimensionLayout, dimension: &str) -> PathBuf {
        layout
            .region_dir(Path::new("/srv/world"), &id(dimension))
            .unwrap()
    }

    #[test]
    fn spigot_uses_sibling_world_folders_with_the_26_dimension_path() {
        let layout = DimensionLayout::Spigot;
        assert_eq!(
            dir(layout, "minecraft:overworld"),
            Path::new("/srv/world/dimensions/minecraft/overworld/region")
        );
        assert_eq!(
            dir(layout, "minecraft:the_nether"),
            Path::new("/srv/world_nether/dimensions/minecraft/the_nether/region")
        );
        assert_eq!(
            dir(layout, "minecraft:the_end"),
            Path::new("/srv/world_the_end/dimensions/minecraft/the_end/region")
        );
        assert_eq!(
            dir(layout, "mypack:deep/dark"),
            Path::new("/srv/world_mypack_deep/dark/dimensions/mypack/deep/dark/region")
        );
    }

    #[test]
    fn entities_sit_next_to_the_region_folder() {
        let layout = DimensionLayout::Spigot;
        assert_eq!(
            layout
                .entities_dir(Path::new("/srv/world"), &id("minecraft:overworld"))
                .unwrap(),
            Path::new("/srv/world/dimensions/minecraft/overworld/entities")
        );
        assert_eq!(
            layout
                .entities_dir(Path::new("/srv/world"), &id("minecraft:the_nether"))
                .unwrap(),
            Path::new("/srv/world_nether/dimensions/minecraft/the_nether/entities")
        );
        assert_eq!(
            DimensionLayout::LegacyVanilla
                .entities_dir(Path::new("/srv/world"), &id("minecraft:overworld"))
                .unwrap(),
            Path::new("/srv/world/entities")
        );
    }

    #[test]
    fn poi_sits_next_to_the_region_folder_in_every_dimension() {
        let root = Path::new("/srv/world");
        let poi = |layout: DimensionLayout, dimension: &str| {
            layout.poi_dir(root, &id(dimension)).unwrap()
        };
        assert_eq!(
            poi(DimensionLayout::Spigot, "minecraft:overworld"),
            Path::new("/srv/world/dimensions/minecraft/overworld/poi")
        );
        assert_eq!(
            poi(DimensionLayout::Spigot, "minecraft:the_nether"),
            Path::new("/srv/world_nether/dimensions/minecraft/the_nether/poi")
        );
        assert_eq!(
            poi(DimensionLayout::Spigot, "minecraft:the_end"),
            Path::new("/srv/world_the_end/dimensions/minecraft/the_end/poi")
        );
        assert_eq!(
            poi(DimensionLayout::Vanilla, "minecraft:the_nether"),
            Path::new("/srv/world/dimensions/minecraft/the_nether/poi")
        );
        assert_eq!(
            poi(DimensionLayout::LegacyVanilla, "minecraft:overworld"),
            Path::new("/srv/world/poi")
        );
        assert_eq!(
            poi(DimensionLayout::LegacyVanilla, "minecraft:the_nether"),
            Path::new("/srv/world/DIM-1/poi")
        );
        assert_eq!(
            poi(DimensionLayout::LegacyVanilla, "minecraft:the_end"),
            Path::new("/srv/world/DIM1/poi")
        );
        assert_eq!(
            poi(DimensionLayout::LegacySpigot, "minecraft:the_nether"),
            Path::new("/srv/world_nether/DIM-1/poi")
        );
        assert_eq!(
            poi(DimensionLayout::LegacySpigot, "minecraft:the_end"),
            Path::new("/srv/world_the_end/DIM1/poi")
        );
    }

    #[test]
    fn vanilla_keeps_every_dimension_under_the_world() {
        let layout = DimensionLayout::Vanilla;
        assert_eq!(
            dir(layout, "minecraft:overworld"),
            Path::new("/srv/world/dimensions/minecraft/overworld/region")
        );
        assert_eq!(
            dir(layout, "minecraft:the_nether"),
            Path::new("/srv/world/dimensions/minecraft/the_nether/region")
        );
        assert_eq!(
            dir(layout, "minecraft:the_end"),
            Path::new("/srv/world/dimensions/minecraft/the_end/region")
        );
    }

    #[test]
    fn legacy_layouts_use_dim_folders() {
        assert_eq!(
            dir(DimensionLayout::LegacyVanilla, "minecraft:overworld"),
            Path::new("/srv/world/region")
        );
        assert_eq!(
            dir(DimensionLayout::LegacyVanilla, "minecraft:the_nether"),
            Path::new("/srv/world/DIM-1/region")
        );
        assert_eq!(
            dir(DimensionLayout::LegacyVanilla, "minecraft:the_end"),
            Path::new("/srv/world/DIM1/region")
        );
        assert_eq!(
            dir(DimensionLayout::LegacySpigot, "minecraft:overworld"),
            Path::new("/srv/world/region")
        );
        assert_eq!(
            dir(DimensionLayout::LegacySpigot, "minecraft:the_nether"),
            Path::new("/srv/world_nether/DIM-1/region")
        );
        assert_eq!(
            dir(DimensionLayout::LegacySpigot, "minecraft:the_end"),
            Path::new("/srv/world_the_end/DIM1/region")
        );
    }

    #[test]
    fn relative_world_root_has_an_empty_container() {
        let nether = DimensionLayout::Spigot
            .region_dir(Path::new("world"), &id("minecraft:the_nether"))
            .unwrap();
        assert_eq!(
            nether,
            Path::new("world_nether/dimensions/minecraft/the_nether/region")
        );
    }

    #[test]
    fn spigot_needs_a_named_world_root() {
        assert!(
            DimensionLayout::Spigot
                .region_dir(Path::new("/"), &id("minecraft:the_nether"))
                .is_err()
        );
        assert!(
            DimensionLayout::Vanilla
                .region_dir(Path::new("/"), &id("minecraft:the_nether"))
                .is_ok()
        );
    }

    #[test]
    fn path_traversal_in_custom_dimensions_is_rejected() {
        for dimension in ["evil:..", "evil:a/../b", "evil:a//b", "evil:."] {
            let parsed = Identifier::parse(dimension).unwrap();
            for layout in [
                DimensionLayout::Spigot,
                DimensionLayout::Vanilla,
                DimensionLayout::LegacySpigot,
                DimensionLayout::LegacyVanilla,
            ] {
                assert!(
                    layout.region_dir(Path::new("/srv/world"), &parsed).is_err(),
                    "{dimension} {layout:?}"
                );
            }
        }
        let namespace = Identifier::parse("..:x");
        if let Ok(namespace) = namespace {
            assert!(
                DimensionLayout::Vanilla
                    .region_dir(Path::new("/srv/world"), &namespace)
                    .is_err()
            );
        }
    }
}
