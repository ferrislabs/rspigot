use rspigot_domain::chunk::ChunkColumn;
use rspigot_domain::{BlockPos, ChunkPos};
use rspigot_nbt::{Compound, Tag};

use crate::ports::ClientLink;
use crate::server::Server;

const STRUCTURES: &str = "structures";
const STARTS: &str = "starts";
const REFERENCES: &str = "References";
const CHILDREN: &str = "Children";
const BOUNDS: &str = "BB";

fn unpack(packed: i64) -> ChunkPos {
    ChunkPos::new(packed as i32, (packed >> 32) as i32)
}

fn piece_contains(piece: &Tag, pos: BlockPos) -> bool {
    let Some(Tag::IntArray(bounds)) = piece.as_compound().and_then(|piece| piece.get(BOUNDS))
    else {
        return false;
    };
    let [min_x, min_y, min_z, max_x, max_y, max_z] = bounds.as_slice() else {
        return false;
    };
    (*min_x..=*max_x).contains(&pos.x)
        && (*min_y..=*max_y).contains(&pos.y)
        && (*min_z..=*max_z).contains(&pos.z)
}

fn start_contains(start: &Compound, pos: BlockPos) -> bool {
    start
        .get_list(CHILDREN)
        .is_some_and(|pieces| pieces.iter().any(|piece| piece_contains(piece, pos)))
}

fn qualified(name: &str) -> String {
    if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    }
}

pub(crate) fn structures_containing<'a>(
    chunk_at: impl Fn(ChunkPos) -> Option<&'a ChunkColumn>,
    pos: BlockPos,
) -> Vec<String> {
    let Some(references) = chunk_at(pos.chunk())
        .and_then(|chunk| chunk.extra.get_compound(STRUCTURES))
        .and_then(|structures| structures.get_compound(REFERENCES))
    else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for (name, tag) in references.iter() {
        let Tag::LongArray(chunks) = tag else {
            continue;
        };
        let inside = chunks.iter().any(|packed| {
            chunk_at(unpack(*packed))
                .and_then(|chunk| chunk.extra.get_compound(STRUCTURES))
                .and_then(|structures| structures.get_compound(STARTS))
                .and_then(|starts| starts.get_compound(name))
                .is_some_and(|start| start_contains(start, pos))
        });
        if inside {
            found.push(qualified(name));
        }
    }
    found.sort_unstable();
    found
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn structures_at(&self, pos: BlockPos) -> Vec<String> {
        structures_containing(
            |chunk| self.world.chunks.get(chunk).map(|loaded| &**loaded),
            pos,
        )
    }
}

#[cfg(test)]
mod tests {
    use rspigot_domain::BiomeId;
    use rspigot_domain::world::WorldHeight;

    use super::*;

    fn chunk_with(pos: ChunkPos, extra: Compound) -> ChunkColumn {
        let mut chunk = ChunkColumn::empty(pos, WorldHeight::OVERWORLD, BiomeId(0));
        chunk.extra = extra;
        chunk
    }

    fn packed(pos: ChunkPos) -> i64 {
        i64::from(pos.x as u32) | (i64::from(pos.z) << 32)
    }

    #[test]
    fn a_structure_is_found_where_one_of_its_pieces_is() {
        let origin = ChunkPos::new(0, 0);
        let neighbour = ChunkPos::new(1, 0);
        let piece = Compound::new().with("BB", Tag::IntArray(vec![2, 40, 2, 20, 60, 12]));
        let start = Compound::new().with("Children", Tag::List(vec![Tag::Compound(piece)]));
        let owner = chunk_with(
            origin,
            Compound::new().with(
                STRUCTURES,
                Compound::new().with(
                    STARTS,
                    Compound::new().with("minecraft:fortress", Tag::Compound(start)),
                ),
            ),
        );
        let referenced = chunk_with(
            neighbour,
            Compound::new().with(
                STRUCTURES,
                Compound::new().with(
                    REFERENCES,
                    Compound::new()
                        .with("minecraft:fortress", Tag::LongArray(vec![packed(origin)])),
                ),
            ),
        );
        let at = |pos: ChunkPos| {
            if pos == origin {
                Some(&owner)
            } else if pos == neighbour {
                Some(&referenced)
            } else {
                None
            }
        };
        assert_eq!(
            structures_containing(at, BlockPos::new(18, 50, 5)),
            ["minecraft:fortress"]
        );
        assert!(structures_containing(at, BlockPos::new(18, 70, 5)).is_empty());
        assert!(structures_containing(at, BlockPos::new(18, 50, 30)).is_empty());
    }
}
