use std::collections::HashMap;
use std::sync::OnceLock;

use rspigot_blocks::StateFlags;
use rspigot_domain::BlockStateId;
use serde_json::Value;

use super::super::prov::Pos;
use super::rotation::Rotation;
use crate::datapack::state_named;

const SOURCE: &str = include_str!("../../../../data/templates.json");

#[derive(Debug)]
pub(crate) struct Template {
    size: Pos,
    blocks: Vec<(Pos, BlockStateId)>,
}

fn library() -> &'static HashMap<String, Template> {
    static LIBRARY: OnceLock<HashMap<String, Template>> = OnceLock::new();
    LIBRARY.get_or_init(|| {
        let Ok(Value::Object(entries)) = serde_json::from_str::<Value>(SOURCE) else {
            return HashMap::new();
        };
        entries
            .iter()
            .filter_map(|(name, value)| Some((name.clone(), Template::parse(value)?)))
            .collect()
    })
}

fn triple(value: &Value) -> Option<Pos> {
    let items = value.as_array()?;
    let at = |index: usize| {
        items
            .get(index)?
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
    };
    Some((at(0)?, at(1)?, at(2)?))
}

fn palette_state(entry: &Value) -> Option<BlockStateId> {
    let items = entry.as_array()?;
    let name = items.first()?.as_str()?;
    let properties: Vec<(&str, &str)> = items[1..]
        .iter()
        .filter_map(|pair| {
            let pair = pair.as_array()?;
            Some((pair.first()?.as_str()?, pair.get(1)?.as_str()?))
        })
        .collect();
    state_named(name, &properties)
}

fn full_cube(state: BlockStateId) -> bool {
    state.flags().contains(StateFlags::FULL_COLLISION_CUBE)
}

impl Template {
    pub(crate) fn named(name: &str) -> Option<&'static Self> {
        library().get(name)
    }

    fn parse(value: &Value) -> Option<Self> {
        let size = triple(value.get("size")?)?;
        let palette: Vec<BlockStateId> = value
            .get("palette")?
            .as_array()?
            .iter()
            .map(palette_state)
            .collect::<Option<_>>()?;
        let mut full = Vec::new();
        let mut other = Vec::new();
        for block in value.get("blocks")?.as_array()? {
            let items = block.as_array()?;
            let pos = (
                i32::try_from(items.first()?.as_i64()?).ok()?,
                i32::try_from(items.get(1)?.as_i64()?).ok()?,
                i32::try_from(items.get(2)?.as_i64()?).ok()?,
            );
            let state = *palette.get(usize::try_from(items.get(3)?.as_i64()?).ok()?)?;
            if full_cube(state) {
                full.push((pos, state));
            } else {
                other.push((pos, state));
            }
        }
        let order = |entry: &(Pos, BlockStateId)| (entry.0.1, entry.0.0, entry.0.2);
        full.sort_by_key(order);
        other.sort_by_key(order);
        full.extend(other);
        Some(Self { size, blocks: full })
    }

    pub(crate) fn blocks(&self) -> &[(Pos, BlockStateId)] {
        &self.blocks
    }

    pub(crate) fn size(&self, rotation: Rotation) -> Pos {
        if rotation.swaps_axes() {
            (self.size.2, self.size.1, self.size.0)
        } else {
            self.size
        }
    }

    pub(crate) fn zero_position(&self, (x, y, z): Pos, rotation: Rotation) -> Pos {
        let (width, depth) = (self.size.0 - 1, self.size.2 - 1);
        match rotation {
            Rotation::None => (x, y, z),
            Rotation::Clockwise90 => (x + depth, y, z),
            Rotation::Clockwise180 => (x + width, y, z + depth),
            Rotation::Counterclockwise90 => (x, y, z + width),
        }
    }

    pub(crate) fn bounds(&self, (x, y, z): Pos, rotation: Rotation) -> (Pos, Pos) {
        let far = (self.size.0 - 1, self.size.1 - 1, self.size.2 - 1);
        let a = rotation.transform((0, 0, 0));
        let b = rotation.transform(far);
        (
            (x + a.0.min(b.0), y + a.1.min(b.1), z + a.2.min(b.2)),
            (x + a.0.max(b.0), y + a.1.max(b.1), z + a.2.max(b.2)),
        )
    }
}
