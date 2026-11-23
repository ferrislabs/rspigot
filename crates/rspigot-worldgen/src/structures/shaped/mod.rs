pub(crate) mod desert;
pub(crate) mod end_city;
pub(crate) mod fortress;
pub(crate) mod fossil;
pub(crate) mod frame;
pub(crate) mod hut;
pub(crate) mod igloo;
pub(crate) mod jungle;
pub(crate) mod mansion;
pub(crate) mod mineshaft;
pub(crate) mod monument;
pub(crate) mod ocean_ruin;
pub(crate) mod portal;
pub(crate) mod scatter;
pub(crate) mod shipwreck;
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
    Fortress(fortress::Piece),
    Stronghold(stronghold::Piece),
    Portal(portal::Piece),
    Treasure(treasure::Piece),
    Fossil(fossil::Piece),
    Igloo(igloo::Piece),
    Wreck(shipwreck::Piece),
    Ruin(ocean_ruin::Piece),
    Temple(scatter::Piece),
    Monument(monument::Piece),
    Mansion(mansion::Piece),
    EndCity(end_city::Piece),
}

impl Shaped {
    pub(crate) fn bbox(&self) -> &Bbox {
        match self {
            Self::Mine(piece) => &piece.bbox,
            Self::Fortress(piece) => &piece.bbox,
            Self::Stronghold(piece) => &piece.bbox,
            Self::Portal(piece) => &piece.bbox,
            Self::Treasure(piece) => &piece.bbox,
            Self::Fossil(piece) => &piece.bbox,
            Self::Igloo(piece) => &piece.bbox,
            Self::Wreck(piece) => &piece.bbox,
            Self::Ruin(piece) => &piece.bbox,
            Self::Temple(piece) => &piece.bbox,
            Self::Monument(piece) => &piece.bbox,
            Self::Mansion(piece) => &piece.bbox,
            Self::EndCity(piece) => &piece.bbox,
        }
    }

    pub(crate) fn place(&self, run: &mut Run, write: &Bbox) {
        match self {
            Self::Mine(piece) => piece.place(run, write),
            Self::Fortress(piece) => piece.place(run, write),
            Self::Stronghold(piece) => piece.place(run, write),
            Self::Portal(piece) => piece.place(run, write),
            Self::Treasure(piece) => piece.place(run, write),
            Self::Fossil(piece) => piece.place(run, write),
            Self::Igloo(piece) => piece.place(run, write),
            Self::Wreck(piece) => piece.place(run, write),
            Self::Ruin(piece) => piece.place(run, write),
            Self::Temple(piece) => piece.place(run, write),
            Self::Monument(piece) => piece.place(run, write),
            Self::Mansion(piece) => piece.place(run, write),
            Self::EndCity(piece) => piece.place(run, write),
        }
    }

    #[cfg(test)]
    pub(crate) fn class(&self) -> String {
        match self {
            Self::Mine(piece) => piece.class().to_owned(),
            Self::Fortress(piece) => piece.class().to_owned(),
            Self::Stronghold(piece) => piece.class().to_owned(),
            Self::Portal(_) => "RuinedPortalPiece".to_owned(),
            Self::Treasure(_) => "BuriedTreasurePiece".to_owned(),
            Self::Monument(_) => "MonumentBuilding".to_owned(),
            Self::Mansion(_) => "WoodlandMansionPiece".to_owned(),
            Self::EndCity(_) => "EndCityPiece".to_owned(),
            Self::Fossil(_) => "NetherFossilPiece".to_owned(),
            Self::Igloo(_) => "IglooPiece".to_owned(),
            Self::Wreck(_) => "ShipwreckPiece".to_owned(),
            Self::Ruin(_) => "OceanRuinPiece".to_owned(),
            Self::Temple(piece) => match piece.temple {
                scatter::Temple::Desert => "DesertPyramidPiece",
                scatter::Temple::Jungle => "JungleTemplePiece",
                scatter::Temple::SwampHut => "SwampHutPiece",
            }
            .to_owned(),
        }
    }

    #[cfg(test)]
    pub(crate) fn facing_name(&self) -> &'static str {
        use super::blueprint::Dir;
        let facing = match self {
            Self::Mine(piece) => piece.facing,
            Self::Fortress(piece) => Some(piece.facing),
            Self::Stronghold(piece) => Some(piece.facing),
            Self::Portal(_) => Some(Dir::North),
            Self::Treasure(_) | Self::Mansion(_) | Self::EndCity(_) => None,
            Self::Temple(piece) => Some(piece.facing),
            Self::Monument(piece) => Some(piece.facing),
            Self::Fossil(_) | Self::Igloo(_) | Self::Wreck(_) | Self::Ruin(_) => Some(Dir::North),
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
    pub(crate) fn extra_lines(&self) -> Vec<String> {
        match self {
            Self::Monument(piece) => piece.child_lines(),
            _ => self
                .template_line()
                .map(|line| format!("TPL {line}"))
                .into_iter()
                .collect(),
        }
    }

    #[cfg(test)]
    pub(crate) fn template_line(&self) -> Option<String> {
        match self {
            Self::Fossil(piece) => Some(piece.tpl.line()),
            Self::Igloo(piece) => Some(piece.tpl.line()),
            Self::Wreck(piece) => Some(piece.tpl.line()),
            Self::Ruin(piece) => Some(piece.tpl.line()),
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
            Self::Fortress(piece) => (
                piece.id(),
                piece.bbox,
                Some(piece.facing),
                piece.depth,
                piece.extra(),
            ),
            Self::Treasure(piece) => (piece.id(), piece.bbox, None, 0, Vec::new()),
            Self::Monument(piece) => (piece.id(), piece.bbox, Some(piece.facing), 0, Vec::new()),
            Self::EndCity(piece) => (
                piece.id(),
                piece.bbox,
                None,
                0,
                vec![
                    ("Template", Tag::from(piece.template())),
                    ("Rot", Tag::from(piece.tpl.rotation_name())),
                    ("OW", Tag::from(piece.overwrite)),
                ],
            ),
            Self::Mansion(piece) => (
                piece.id(),
                piece.bbox,
                None,
                0,
                vec![
                    ("Template", Tag::from(piece.template())),
                    ("Rot", Tag::from(piece.tpl.rotation_name())),
                    ("Mi", Tag::from(piece.mirror_name())),
                ],
            ),
            Self::Temple(piece) => {
                let (width, height, depth) = piece.temple.size();
                (
                    piece.id(),
                    piece.bbox,
                    Some(piece.facing),
                    0,
                    vec![
                        ("Width", Tag::from(width)),
                        ("Height", Tag::from(height)),
                        ("Depth", Tag::from(depth)),
                        ("HPos", Tag::from(-1)),
                    ],
                )
            }
            Self::Ruin(piece) => (
                piece.id(),
                piece.bbox,
                Some(super::blueprint::Dir::North),
                0,
                vec![
                    ("Template", Tag::from(piece.tpl.name)),
                    ("Rot", Tag::from(piece.tpl.rotation_name())),
                    ("Integrity", Tag::from(piece.integrity)),
                    (
                        "BiomeType",
                        Tag::from(if piece.warm { "warm" } else { "cold" }),
                    ),
                    ("IsLarge", Tag::from(piece.large)),
                ],
            ),
            Self::Wreck(piece) => (
                piece.id(),
                piece.bbox,
                Some(super::blueprint::Dir::North),
                0,
                template_tags(&piece.tpl),
            ),
            Self::Igloo(piece) => (
                piece.id(),
                piece.bbox,
                Some(super::blueprint::Dir::North),
                0,
                template_tags(&piece.tpl),
            ),
            Self::Fossil(piece) => (
                piece.id(),
                piece.bbox,
                Some(super::blueprint::Dir::North),
                0,
                template_tags(&piece.tpl),
            ),
            Self::Portal(piece) => {
                let mut tag = Compound::new()
                    .with("id", "minecraft:rupo")
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

fn template_tags(tpl: &tpl::Tpl) -> Vec<(&'static str, Tag)> {
    vec![
        ("Template", Tag::from(tpl.name)),
        ("Rot", Tag::from(tpl.rotation_name())),
    ]
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
    Igloo,
    Shipwreck { beached: bool },
    OceanRuin(ocean_ruin::Params),
    Temple(scatter::Temple),
    Monument,
    Fortress,
    Mansion,
    EndCity,
}

impl Other {
    pub(crate) fn parse(kind: &str, value: &Value) -> Option<Self> {
        Some(match kind {
            "buried_treasure" => Self::BuriedTreasure,
            "igloo" => Self::Igloo,
            "ocean_monument" => Self::Monument,
            "fortress" => Self::Fortress,
            "woodland_mansion" => Self::Mansion,
            "end_city" => Self::EndCity,
            "desert_pyramid" => Self::Temple(scatter::Temple::Desert),
            "jungle_temple" => Self::Temple(scatter::Temple::Jungle),
            "swamp_hut" => Self::Temple(scatter::Temple::SwampHut),
            "ocean_ruin" => {
                let number = |key: &str| {
                    value
                        .get(key)
                        .and_then(Value::as_f64)
                        .map_or(0.0, |n| n as f32)
                };
                Self::OceanRuin(ocean_ruin::Params {
                    warm: value.get("biome_temp").and_then(Value::as_str) == Some("warm"),
                    large: number("large_probability"),
                    cluster: number("cluster_probability"),
                })
            }
            "shipwreck" => Self::Shipwreck {
                beached: value
                    .get("is_beached")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
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
            Self::Shipwreck { beached } => {
                shipwreck::generate(beached, library, ground, valid, probe, seed, chunk)
                    .map(|piece| vec![Shaped::Wreck(piece)])
            }
            Self::OceanRuin(params) => {
                ocean_ruin::generate(params, library, ground, valid, probe, seed, chunk)
                    .map(|pieces| pieces.into_iter().map(Shaped::Ruin).collect())
            }
            Self::Temple(temple) => scatter::generate(temple, ground, valid, probe, seed, chunk)
                .map(|piece| vec![Shaped::Temple(piece)]),
            Self::Monument => monument::generate(ground, valid, probe, seed, chunk)
                .map(|piece| vec![Shaped::Monument(piece)]),
            Self::Fortress => fortress::generate(valid, probe, seed, chunk)
                .map(|pieces| pieces.into_iter().map(Shaped::Fortress).collect()),
            Self::Mansion => mansion::generate(library, ground, valid, probe, seed, chunk)
                .map(|pieces| pieces.into_iter().map(Shaped::Mansion).collect()),
            Self::EndCity => end_city::generate(library, ground, valid, probe, seed, chunk)
                .map(|pieces| pieces.into_iter().map(Shaped::EndCity).collect()),
            Self::Igloo => igloo::generate(library, ground, valid, probe, seed, chunk)
                .map(|pieces| pieces.into_iter().map(Shaped::Igloo).collect()),
            Self::NetherFossil(height) => {
                fossil::generate(height, library, ground, valid, probe, seed, chunk)
                    .map(|piece| vec![Shaped::Fossil(piece)])
            }
        }
    }
}
