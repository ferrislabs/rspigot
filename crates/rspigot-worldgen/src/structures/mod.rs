mod alias;
mod bbox;
mod blueprint;
mod data;
mod jigsaw;
mod library;
mod nbt;
mod pack;
mod place;
mod placement;
mod pool;
pub(crate) mod processors;
mod render;
mod sets;
mod shaped;
pub(crate) mod structure;
mod template;
mod world;

#[cfg(test)]
mod fortress_tests;
#[cfg(test)]
mod l2_tests;
#[cfg(test)]
mod library_tests;
#[cfg(test)]
mod processor_tests;
#[cfg(test)]
mod shaped_tests;
#[cfg(test)]
mod tests;

pub(crate) use bbox::Bbox;
pub(crate) use jigsaw::{Base, Ground, Heightmap};
pub(crate) use library::Library;
pub(crate) use nbt::{attach, chunk_entities, spawned_entities};
pub use pack::LoadIssue;
pub(crate) use place::place_start;
pub(crate) use placement::BiomeProbe;
pub(crate) use render::{jigsaw_blocks, start_blocks, template_blocks};
pub(crate) use world::{Engine, STEP_NAMES, Start};

pub(crate) fn install_roots(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    library::Library::discover(dir).0
}

pub fn install_datapacks(dir: &std::path::Path) -> Vec<LoadIssue> {
    let (packs, mut issues) = library::Library::discover(dir);
    if packs.is_empty() {
        return issues;
    }
    issues.extend(crate::worldgen_pack::install_from(dir));
    let (library, found) = library::Library::build(&packs);
    issues.extend(found);
    if let Err(reason) = library::Library::install(library) {
        issues.push(LoadIssue {
            path: dir.to_owned(),
            reason,
        });
    }
    issues
}
