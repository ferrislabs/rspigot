pub(crate) mod fossil;
pub(crate) mod frame;
pub(crate) mod mineshaft;
pub(crate) mod portal;
pub(crate) mod start;
pub(crate) mod stronghold;
pub(crate) mod tpl;
pub(crate) mod treasure;

use rspigot_nbt::{Compound, Tag};
use serde_json::Value;

use super::bbox::Bbox;
use super::jigsaw::Ground;
use super::library::Library;
use super::placement::BiomeProbe;
use crate::overworld::features::place::Run;

#[derive(Debug)]
pub(crate) enum Shaped {
    Mine(mineshaft::Piece),
    Stronghold(stronghold::Piece),
    Portal(portal::Piece),
    Treasure(treasure::Piece),
    Fossil(fossil::Piece),
}

impl Shaped {
    pub(crate) fn bbox(&self) -> &Bbox {
        match self {
            Self::Mine(piece) => &piece.bbox,
            Self::Stronghold(piece) => &piece.bbox,
            Self::Portal(piece) => &piece.bbox,
            Self::Treasure(piece) => &piece.bbox,
            Self::Fossil(piece) => &piece.bbox,
        }
    }

    pub(crate) fn place(&self, run: &mut Run, write: &Bbox) {
        match self {
            Self::Mine(piece) => piece.place(run, write),
            Self::Stronghold(piece) => piece.place(run, write),
            Self::Portal(piece) => piece.place(run, write),
            Self::Treasure(piece) => piece.place(run, write),
            Self::Fossil(piece) => piece.place(run, write),
        }
    }

    #[cfg(test)]
    pub(crate) fn class(&self) -> String {
        match self {
            Self::Mine(piece) => piece.class().to_owned(),
            Self::Stronghold(piece) => piece.class().to_owned(),
            Self::Portal(_) => "RuinedPortalPiece".to_owned(),
            Self::Treasure(_) => "BuriedTreasurePiece".to_owned(),
            Self::Fossil(_) => "NetherFossilPiece".to_owned(),
        }
    }

    #[cfg(test)]
    pub(crate) fn facing_name(&self) -> &'static str {
        use super::blueprint::Dir;
        let facing = match self {
            Self::Mine(piece) => piece.facing,
            Self::Stronghold(piece) => Some(piece.facing),
            Self::Portal(_) => Some(Dir::North),
            Self::Treasure(_) => None,
            Self::Fossil(_) => Some(Dir::North),
        };
        match facing {
            None => "null",
            Some(Dir::North) => "north",
            Some(Dir::South) => "south",
            Some(Dir::West) => "west",
            Some(Dir::East) => "east",
            Some(_) => "?",
        }
    }

    #[cfg(test)]
    pub(crate) fn template_line(&self) -> Option<String> {
        match self {
            Self::Fossil(piece) => Some(piece.tpl.line()),
            _ => None,
        }
    }

    pub(crate) fn tag(&self) -> Compound {
        let (id, bbox, facing, depth, extra) = match self {
            Self::Mine(piece) => (
                piece.id(),
                piece.bbox,
                piece.facing,
                piece.depth,
                tags(piece.extra()),
            ),
            Self::Stronghold(piece) => (
                piece.id(),
                piece.bbox,
                Some(piece.facing),
                piece.depth,
                tags(piece.extra()),
            ),
            Self::Treasure(piece) => (piece.id(), piece.bbox, None, 0, Vec::new()),
            Self::Fossil(piece) => (
                piece.id(),
                piece.bbox,
                Some(super::blueprint::Dir::North),
                0,
                vec![
                    ("Template", Tag::from(piece.tpl.name)),
                    ("Rot", Tag::from(piece.tpl.rotation_name())),
                ],
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

fn tags<T: Into<Tag>>(items: Vec<(&'static str, T)>) -> Vec<(&'static str, Tag)> {
    items
        .into_iter()
        .map(|(key, value)| (key, value.into()))
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Other {
    BuriedTreasure,
    NetherFossil(fossil::Height),
}

impl Other {
    pub(crate) fn parse(kind: &str, value: &Value) -> Option<Self> {
        Some(match kind {
            "buried_treasure" => Self::BuriedTreasure,
            "nether_fossil" => {
                let height = value.get("height")?;
                Self::NetherFossil(fossil::Height {
                    min: super::structure::anchor(height.get("min_inclusive")),
                    max: super::structure::anchor(height.get("max_inclusive")),
                })
            }
            _ => return None,
        })
    }

    pub(crate) fn generate<G: Ground>(
        self,
        library: &'static Library,
        ground: &G,
        valid: &[u8],
        probe: &impl BiomeProbe,
        seed: i64,
        chunk: (i32, i32),
    ) -> Option<Vec<Shaped>> {
        match self {
            Self::BuriedTreasure => treasure::generate(ground, valid, probe, chunk)
                .map(|piece| vec![Shaped::Treasure(piece)]),
            Self::NetherFossil(height) => {
                fossil::generate(height, library, ground, valid, probe, seed, chunk)
                    .map(|piece| vec![Shaped::Fossil(piece)])
            }
        }
    }
}
