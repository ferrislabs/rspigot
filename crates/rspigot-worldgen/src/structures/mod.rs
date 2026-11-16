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
mod processors;
mod sets;
mod shaped;
pub(crate) mod structure;
mod template;
mod world;

#[cfg(test)]
mod library_tests;
#[cfg(test)]
mod tests;

pub(crate) use bbox::Bbox;
pub(crate) use jigsaw::{Base, Ground, Heightmap};
pub(crate) use library::Library;
pub(crate) use nbt::{attach, chunk_entities, spawned_entities};
pub use pack::LoadIssue;
pub(crate) use place::place_start;
pub(crate) use placement::BiomeProbe;
pub(crate) use world::{Engine, Start};

pub fn install_datapacks(dir: &std::path::Path) -> Vec<LoadIssue> {
    let (packs, mut issues) = library::Library::discover(dir);
    if packs.is_empty() {
        return issues;
    }
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
