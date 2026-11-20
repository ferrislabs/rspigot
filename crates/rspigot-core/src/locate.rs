use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, TryRecvError};
use rspigot_domain::text::{ClickEvent, Color, Component, NamedColor};
use rspigot_domain::{BlockPos, Vec3};

use crate::chunks::{QueryAnswer, WorldQuery};
use crate::commands::{CommandContext, CommandSender};
use crate::ports::{ClientLink, PlaceFailure};
use crate::server::Server;
use crate::structure_ids::{STRUCTURES, TAGS};
use crate::world_state::DimensionKey;

const QUERY_TIMEOUT: Duration = Duration::from_secs(120);
const TAG_DEPTH: usize = 8;
const BIOME_REGISTRY: &str = "minecraft:worldgen/biome";
const POI_REGISTRY: &str = "minecraft:point_of_interest_type";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceKind {
    Structure,
    Jigsaw,
    Template,
    Feature,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
    Structure,
    Biome,
    Place(PlaceKind, BlockPos),
}

pub struct PendingQuery {
    sender: CommandSender,
    dimension: DimensionKey,
    origin: Vec3,
    kind: QueryKind,
    label: String,
    started: Instant,
    answer: Receiver<QueryAnswer>,
}

pub fn normalised(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

pub fn structure_ids() -> impl Iterator<Item = &'static str> {
    STRUCTURES.iter().copied()
}

pub fn structure_tag_names() -> impl Iterator<Item = &'static str> {
    TAGS.iter().map(|(name, _)| *name)
}

fn expand_structure_tag(name: &str, depth: usize, out: &mut Vec<String>) {
    if depth > TAG_DEPTH {
        return;
    }
    let Some((_, values)) = TAGS.iter().find(|(tag, _)| *tag == name) else {
        return;
    };
    for value in *values {
        match value.strip_prefix('#') {
            Some(inner) => expand_structure_tag(inner, depth + 1, out),
            None => out.push((*value).to_owned()),
        }
    }
}

pub fn resolve_structures(argument: &str) -> Result<Vec<String>, Component> {
    match argument.strip_prefix('#') {
        Some(tag) => {
            let mut found = Vec::new();
            expand_structure_tag(&normalised(tag), 0, &mut found);
            if found.is_empty() {
                return Err(Component::translatable(
                    "commands.locate.structure.invalid",
                    vec![Component::text(argument)],
                ));
            }
            found.sort();
            found.dedup();
            Ok(found)
        }
        None => {
            let id = normalised(argument);
            if STRUCTURES.contains(&id.as_str()) {
                Ok(vec![id])
            } else {
                Err(Component::translatable(
                    "argument.resource.not_found",
                    vec![
                        Component::text(id),
                        Component::text("minecraft:worldgen/structure"),
                    ],
                ))
            }
        }
    }
}

fn registry_members(registry: &str, tag: &str) -> Vec<String> {
    let entries = rspigot_data::registries::synced_entries(registry)
        .or_else(|| rspigot_data::registries::static_entries(registry))
        .unwrap_or(&[]);
    rspigot_data::tags::tag(registry, &normalised(tag))
        .unwrap_or(&[])
        .iter()
        .filter_map(|id| entries.get(usize::try_from(*id).ok()?))
        .map(|name| (*name).to_owned())
        .collect()
}

fn resolve_registry(
    registry: &str,
    argument: &str,
    exists: impl Fn(&str) -> bool,
) -> Result<Vec<String>, Component> {
    let not_found = |id: &str| {
        Component::translatable(
            "argument.resource_tag.not_found",
            vec![Component::text(id), Component::text(registry)],
        )
    };
    match argument.strip_prefix('#') {
        Some(tag) => {
            let members = registry_members(registry, tag);
            if members.is_empty() {
                Err(not_found(&normalised(tag)))
            } else {
                Ok(members)
            }
        }
        None => {
            let id = normalised(argument);
            if exists(&id) {
                Ok(vec![id])
            } else {
                Err(Component::translatable(
                    "argument.resource.not_found",
                    vec![Component::text(id), Component::text(registry)],
                ))
            }
        }
    }
}

pub fn resolve_biomes(argument: &str) -> Result<Vec<String>, Component> {
    resolve_registry(BIOME_REGISTRY, argument, |id| {
        rspigot_data::registries::synced_index(BIOME_REGISTRY, id).is_some()
    })
}

pub fn resolve_poi_types(argument: &str) -> Result<Vec<String>, Component> {
    resolve_registry(POI_REGISTRY, argument, |id| {
        rspigot_data::registries::static_index(POI_REGISTRY, id).is_some()
    })
}

pub fn coordinates(x: i32, y: Option<i32>, z: i32) -> Component {
    let height = y.map_or_else(|| "~".to_owned(), |y| y.to_string());
    let inner = Component::translatable(
        "chat.coordinates",
        vec![
            Component::text(x.to_string()),
            Component::text(height.clone()),
            Component::text(z.to_string()),
        ],
    );
    let mut wrapped = Component::translatable("chat.square_brackets", vec![inner]);
    wrapped.style.color = Some(Color::Named(NamedColor::Green));
    wrapped.style.click = Some(Box::new(ClickEvent::RunCommand(format!(
        "/tp @s {x} {height} {z}"
    ))));
    wrapped.style.hover = Some(Box::new(Component::translatable(
        "chat.coordinates.tooltip",
        Vec::new(),
    )));
    wrapped
}

pub fn flat_distance(from: Vec3, x: i32, z: i32) -> i32 {
    let (fx, fz) = (from.x.floor() as i32, from.z.floor() as i32);
    let (dx, dz) = (i64::from(x - fx), i64::from(z - fz));
    ((dx * dx + dz * dz) as f32).sqrt().floor() as i32
}

pub fn solid_distance(from: Vec3, to: BlockPos) -> i32 {
    let (fx, fy, fz) = (
        from.x.floor() as i32,
        from.y.floor() as i32,
        from.z.floor() as i32,
    );
    let (dx, dy, dz) = (
        i64::from(to.x - fx),
        i64::from(to.y - fy),
        i64::from(to.z - fz),
    );
    ((dx * dx + dy * dy + dz * dz) as f32).sqrt().floor() as i32
}

pub fn located(key: &str, name: &str, pos: BlockPos, include_y: bool, distance: i32) -> Component {
    Component::translatable(
        key,
        vec![
            Component::text(name),
            coordinates(pos.x, include_y.then_some(pos.y), pos.z),
            Component::text(distance.to_string()),
        ],
    )
}

pub fn locate_poi<C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    types: &[String],
) -> Option<(String, BlockPos)> {
    let origin = ctx.sender_origin(sender);
    ctx.nearest_poi(origin, types)
}

impl<L: ClientLink> Server<L> {
    pub(crate) fn begin_world_query(
        &mut self,
        sender: CommandSender,
        query: WorldQuery,
        kind: QueryKind,
        label: String,
    ) {
        let origin = self.sender_origin(sender);
        let answer = self
            .workers
            .request_query(&self.world.dimension, origin, query);
        self.pending_queries.push(PendingQuery {
            sender,
            dimension: self.world.dimension.key.clone(),
            origin,
            kind,
            label,
            started: Instant::now(),
            answer,
        });
    }

    pub(crate) fn tick_pending_queries(&mut self) {
        if self.pending_queries.is_empty() {
            return;
        }
        let mut ready = Vec::new();
        let dimension = self.world.dimension.key.clone();
        let mut waiting = Vec::new();
        for pending in std::mem::take(&mut self.pending_queries) {
            if pending.dimension != dimension {
                waiting.push(pending);
                continue;
            }
            match pending.answer.try_recv() {
                Err(TryRecvError::Empty) => waiting.push(pending),
                Err(TryRecvError::Disconnected) => {}
                Ok(answer) => ready.push((pending, answer)),
            }
        }
        self.pending_queries = waiting;
        for (pending, answer) in ready {
            self.deliver_query(&pending, answer);
        }
    }

    pub(crate) fn expire_pending_queries(&mut self) {
        self.pending_queries
            .retain(|pending| pending.started.elapsed() < QUERY_TIMEOUT);
    }

    pub(crate) fn forget_pending_queries(&mut self, id: rspigot_domain::player::PlayerId) {
        self.pending_queries
            .retain(|pending| pending.sender != CommandSender::Player(id));
    }

    fn deliver_query(&mut self, pending: &PendingQuery, answer: QueryAnswer) {
        if let CommandSender::Player(id) = pending.sender
            && !self.players.contains_key(&id)
        {
            return;
        }
        let name = pending.label.as_str();
        let message = match (pending.kind, answer) {
            (QueryKind::Structure, QueryAnswer::Located(Some((found, at)))) => {
                let pos = BlockPos::new(
                    at.x.floor() as i32,
                    at.y.floor() as i32,
                    at.z.floor() as i32,
                );
                let shown = if name.starts_with('#') {
                    found.as_str()
                } else {
                    name
                };
                located(
                    "commands.locate.structure.success",
                    shown,
                    pos,
                    false,
                    flat_distance(pending.origin, pos.x, pos.z),
                )
            }
            (QueryKind::Structure, QueryAnswer::Located(None)) => Component::translatable(
                "commands.locate.structure.not_found",
                vec![Component::text(name)],
            ),
            (QueryKind::Biome, QueryAnswer::Located(Some((found, at)))) => {
                let pos = BlockPos::new(
                    at.x.floor() as i32,
                    at.y.floor() as i32,
                    at.z.floor() as i32,
                );
                let shown = if name.starts_with('#') {
                    found.as_str()
                } else {
                    name
                };
                located(
                    "commands.locate.biome.success",
                    shown,
                    pos,
                    true,
                    solid_distance(pending.origin, pos),
                )
            }
            (QueryKind::Biome, QueryAnswer::Located(None)) => Component::translatable(
                "commands.locate.biome.not_found",
                vec![Component::text(name)],
            ),
            (QueryKind::Place(kind, at), QueryAnswer::Placed(Ok(blocks))) => {
                for block in &blocks {
                    self.set_block(block.pos, block.state);
                }
                place_success(kind, name, at)
            }
            (QueryKind::Place(kind, _), QueryAnswer::Placed(Err(PlaceFailure::Unknown))) => {
                place_invalid(kind, name)
            }
            (QueryKind::Place(kind, _), QueryAnswer::Placed(Err(PlaceFailure::Failed))) => {
                place_failed(kind)
            }
            _ => return,
        };
        self.reply(pending.sender, message);
    }
}

fn place_success(kind: PlaceKind, name: &str, at: BlockPos) -> Component {
    let coordinates = |first: Option<&str>| {
        let mut args: Vec<Component> = first.map(Component::text).into_iter().collect();
        args.extend([
            Component::text(at.x.to_string()),
            Component::text(at.y.to_string()),
            Component::text(at.z.to_string()),
        ]);
        args
    };
    match kind {
        PlaceKind::Structure => {
            Component::translatable("commands.place.structure.success", coordinates(Some(name)))
        }
        PlaceKind::Template => {
            Component::translatable("commands.place.template.success", coordinates(Some(name)))
        }
        PlaceKind::Feature => {
            Component::translatable("commands.place.feature.success", coordinates(Some(name)))
        }
        PlaceKind::Jigsaw => {
            Component::translatable("commands.place.jigsaw.success", coordinates(None))
        }
    }
}

fn place_invalid(kind: PlaceKind, name: &str) -> Component {
    let key = match kind {
        PlaceKind::Structure => "commands.place.structure.invalid",
        PlaceKind::Template => "commands.place.template.invalid",
        PlaceKind::Feature => "commands.place.feature.invalid",
        PlaceKind::Jigsaw => "commands.place.jigsaw.invalid",
    };
    Component::translatable(key, vec![Component::text(name)])
}

fn place_failed(kind: PlaceKind) -> Component {
    let key = match kind {
        PlaceKind::Structure => "commands.place.structure.failed",
        PlaceKind::Template => "commands.place.template.failed",
        PlaceKind::Feature => "commands.place.feature.failed",
        PlaceKind::Jigsaw => "commands.place.jigsaw.failed",
    };
    Component::translatable(key, Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_structure_tag_expands_to_its_members() {
        let village = resolve_structures("#minecraft:village").unwrap();
        assert!(village.contains(&"minecraft:village_plains".to_owned()));
        assert_eq!(village.len(), 5);
        assert_eq!(
            resolve_structures("stronghold").unwrap(),
            ["minecraft:stronghold"]
        );
        assert!(resolve_structures("nope").is_err());
        assert!(resolve_structures("#minecraft:nope").is_err());
    }

    #[test]
    fn coordinates_are_a_clickable_teleport() {
        let component = coordinates(12, None, -5);
        assert_eq!(
            component.style.click.as_deref(),
            Some(&ClickEvent::RunCommand("/tp @s 12 ~ -5".to_owned()))
        );
        assert!(component.style.hover.is_some());
        assert_eq!(
            coordinates(1, Some(70), 2).style.click.as_deref(),
            Some(&ClickEvent::RunCommand("/tp @s 1 70 2".to_owned()))
        );
    }

    #[test]
    fn distances_floor_the_euclidean_length() {
        let from = Vec3::new(0.5, 64.0, 0.5);
        assert_eq!(flat_distance(from, 3, 4), 5);
        assert_eq!(solid_distance(from, BlockPos::new(0, 64, 10)), 10);
    }
}
