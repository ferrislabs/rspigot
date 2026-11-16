pub(crate) mod frame;
pub(crate) mod mineshaft;
pub(crate) mod portal;
pub(crate) mod stronghold;

use rspigot_nbt::{Compound, Tag};

use super::bbox::Bbox;
use crate::overworld::features::place::Run;

#[derive(Debug)]
pub(crate) enum Shaped {
    Mine(mineshaft::Piece),
    Stronghold(stronghold::Piece),
    Portal(portal::Piece),
}

impl Shaped {
    pub(crate) fn bbox(&self) -> &Bbox {
        match self {
            Self::Mine(piece) => &piece.bbox,
            Self::Stronghold(piece) => &piece.bbox,
            Self::Portal(piece) => &piece.bbox,
        }
    }

    pub(crate) fn place(&self, run: &mut Run, write: &Bbox) {
        match self {
            Self::Mine(piece) => piece.place(run, write),
            Self::Stronghold(piece) => piece.place(run, write),
            Self::Portal(piece) => piece.place(run, write),
        }
    }

    pub(crate) fn tag(&self) -> Compound {
        let (id, bbox, facing, depth, extra) = match self {
            Self::Mine(piece) => (
                piece.id(),
                piece.bbox,
                piece.facing,
                piece.depth,
                piece.extra(),
            ),
            Self::Stronghold(piece) => (
                piece.id(),
                piece.bbox,
                Some(piece.facing),
                piece.depth,
                piece.extra(),
            ),
            Self::Portal(piece) => {
                let mut tag = Compound::new()
                    .with("id", "minecraft:rupt")
                    .with("GD", 0)
                    .with(
                        "BB",
                        Tag::IntArray(vec![
                            piece.bbox.min.0,
                            piece.bbox.min.1,
                            piece.bbox.min.2,
                            piece.bbox.max.0,
                            piece.bbox.max.1,
                            piece.bbox.max.2,
                        ]),
                    );
                for (key, value) in piece.tag_extra() {
                    tag.insert(key, value);
                }
                return tag;
            }
        };
        let mut tag = Compound::new()
            .with("id", format!("minecraft:{}", id.to_lowercase()))
            .with("GD", depth)
            .with(
                "BB",
                Tag::IntArray(vec![
                    bbox.min.0, bbox.min.1, bbox.min.2, bbox.max.0, bbox.max.1, bbox.max.2,
                ]),
            )
            .with(
                "O",
                facing.map_or(-1, |dir| match dir {
                    super::blueprint::Dir::South => 0,
                    super::blueprint::Dir::West => 1,
                    super::blueprint::Dir::North => 2,
                    _ => 3,
                }),
            );
        for (key, value) in extra {
            tag.insert(key, value);
        }
        tag
    }
}
