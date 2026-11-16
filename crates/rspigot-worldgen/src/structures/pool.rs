use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;

use super::bbox::{Bbox, Pos};
use super::blueprint::{Blueprint, Dir, Jigsaw};
use super::data::{short, structure_data};
use crate::legacy::JavaRandom;
use crate::overworld::features::structure::Rotation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Projection {
    Rigid,
    TerrainMatching,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Processors {
    Named(String),
    Inline(Value),
}

#[derive(Debug)]
pub(crate) enum Element {
    Empty,
    Single {
        template: String,
        blueprint: Option<&'static Blueprint>,
        processors: Processors,
        projection: Projection,
        legacy: bool,
    },
    Feature {
        feature: String,
        projection: Projection,
    },
    List {
        elements: Vec<Element>,
        projection: Projection,
    },
}

#[derive(Debug)]
pub(crate) struct Pool {
    pub(crate) name: String,
    pub(crate) elements: Vec<(Element, i32)>,
    pub(crate) fallback: String,
    expanded: Vec<usize>,
    max_size: OnceLock<i32>,
}

pub(crate) fn shuffle<T>(items: &mut [T], random: &mut JavaRandom) {
    for j in (2..=items.len()).rev() {
        let k = random.next_int(j as i32) as usize;
        items.swap(j - 1, k);
    }
}

fn projection(value: &Value) -> Projection {
    match value.get("projection").and_then(Value::as_str) {
        Some("terrain_matching") => Projection::TerrainMatching,
        _ => Projection::Rigid,
    }
}

impl Element {
    fn parse(value: &Value) -> Self {
        match short(value.get("element_type").and_then(Value::as_str).unwrap_or("")) {
            "single_pool_element" | "legacy_single_pool_element" => {
                let template = value
                    .get("location")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                let processors = match value.get("processors") {
                    Some(Value::String(name)) => Processors::Named(name.clone()),
                    Some(inline) => Processors::Inline(inline.clone()),
                    None => Processors::Named("minecraft:empty".to_owned()),
                };
                Self::Single {
                    blueprint: Blueprint::named(&template),
                    template,
                    processors,
                    projection: projection(value),
                    legacy: short(value.get("element_type").and_then(Value::as_str).unwrap_or(""))
                        == "legacy_single_pool_element",
                }
            }
            "feature_pool_element" => Self::Feature {
                feature: value
                    .get("feature")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                projection: projection(value),
            },
            "list_pool_element" => Self::List {
                elements: value
                    .get("elements")
                    .and_then(Value::as_array)
                    .map(|items| items.iter().map(Self::parse).collect())
                    .unwrap_or_default(),
                projection: projection(value),
            },
            _ => Self::Empty,
        }
    }

    pub(crate) fn projection(&self) -> Projection {
        match self {
            Self::Single { projection, .. }
            | Self::Feature { projection, .. }
            | Self::List { projection, .. } => *projection,
            Self::Empty => Projection::Rigid,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    pub(crate) fn size(&self, rotation: Rotation) -> Pos {
        match self {
            Self::Single { blueprint, .. } => blueprint.map_or((0, 0, 0), |b| b.size(rotation)),
            Self::List { elements, .. } => elements.iter().fold((0, 0, 0), |acc, element| {
                let size = element.size(rotation);
                (acc.0.max(size.0), acc.1.max(size.1), acc.2.max(size.2))
            }),
            Self::Feature { .. } | Self::Empty => (0, 0, 0),
        }
    }

    pub(crate) fn bounds(&self, pos: Pos, rotation: Rotation) -> Bbox {
        match self {
            Self::Single { blueprint, .. } => match blueprint {
                Some(blueprint) => blueprint.bounds(pos, rotation),
                None => Bbox::from_corners(pos, pos),
            },
            Self::Feature { .. } | Self::Empty => Bbox::from_corners(pos, pos),
            Self::List { elements, .. } => {
                let mut iter = elements
                    .iter()
                    .filter(|element| !element.is_empty())
                    .map(|element| element.bounds(pos, rotation));
                let mut total = iter.next().unwrap_or(Bbox::from_corners(pos, pos));
                for next in iter {
                    total.encapsulate(&next);
                }
                total
            }
        }
    }

    pub(crate) fn ground_level_delta(&self) -> i32 {
        1
    }

    pub(crate) fn jigsaws(&self, pos: Pos, rotation: Rotation) -> Vec<Jigsaw> {
        match self {
            Self::Single { blueprint, .. } => {
                blueprint.map_or_else(Vec::new, |blueprint| blueprint.jigsaws(pos, rotation))
            }
            Self::Feature { .. } => vec![Jigsaw {
                pos,
                front: Dir::Down,
                top: Dir::South,
                rollable: true,
                name: "minecraft:bottom".to_owned(),
                target: "minecraft:empty".to_owned(),
                pool: "minecraft:empty".to_owned(),
                final_state: "minecraft:air".to_owned(),
                selection_priority: 0,
                placement_priority: 0,
            }],
            Self::List { elements, .. } => elements
                .first()
                .map_or_else(Vec::new, |first| first.jigsaws(pos, rotation)),
            Self::Empty => Vec::new(),
        }
    }

    pub(crate) fn shuffled_jigsaws(
        &self,
        pos: Pos,
        rotation: Rotation,
        random: &mut JavaRandom,
    ) -> Vec<Jigsaw> {
        let mut list = self.jigsaws(pos, rotation);
        if matches!(self, Self::Single { .. } | Self::List { .. }) {
            shuffle(&mut list, random);
            list.sort_by_key(|jigsaw| std::cmp::Reverse(jigsaw.selection_priority));
        }
        list
    }
}

impl Pool {
    fn parse(name: &str, value: &Value) -> Self {
        let elements: Vec<(Element, i32)> = value
            .get("elements")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| {
                        let weight = i32::try_from(item.get("weight")?.as_i64()?).ok()?;
                        Some((Element::parse(item.get("element")?), weight))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut expanded = Vec::new();
        for (index, (_, weight)) in elements.iter().enumerate() {
            for _ in 0..*weight {
                expanded.push(index);
            }
        }
        Self {
            name: name.to_owned(),
            fallback: value
                .get("fallback")
                .and_then(Value::as_str)
                .unwrap_or("minecraft:empty")
                .to_owned(),
            elements,
            expanded,
            max_size: OnceLock::new(),
        }
    }

    pub(crate) fn size(&self) -> usize {
        self.expanded.len()
    }

    pub(crate) fn random_element(&self, random: &mut JavaRandom) -> Option<&Element> {
        if self.expanded.is_empty() {
            return None;
        }
        let index = random.next_int(self.expanded.len() as i32) as usize;
        Some(&self.elements[self.expanded[index]].0)
    }

    pub(crate) fn shuffled_elements(&self, random: &mut JavaRandom) -> Vec<&Element> {
        let mut list: Vec<usize> = self.expanded.clone();
        shuffle(&mut list, random);
        list.into_iter().map(|index| &self.elements[index].0).collect()
    }

    pub(crate) fn max_size(&self) -> i32 {
        *self.max_size.get_or_init(|| {
            self.expanded
                .iter()
                .map(|index| &self.elements[*index].0)
                .filter(|element| !element.is_empty())
                .map(|element| element.size(Rotation::None).1)
                .max()
                .unwrap_or(0)
        })
    }
}

#[derive(Debug)]
pub(crate) struct Pools {
    pools: HashMap<String, Pool>,
    empty: Pool,
}

impl Pools {
    pub(crate) fn load() -> Self {
        let mut pools = HashMap::new();
        if let Some(Value::Object(map)) = structure_data().get("pools") {
            for (name, value) in map {
                pools.insert(name.clone(), Pool::parse(name, value));
            }
        }
        Self {
            pools,
            empty: Pool::parse("minecraft:empty", &Value::Null),
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Pool> {
        if name == "minecraft:empty" {
            return Some(&self.empty);
        }
        self.pools.get(name)
    }
}
