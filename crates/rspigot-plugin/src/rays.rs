use rspigot_plugin_protocol::v1::{self, answer, lookup_answer, lookup_question, question};

use crate::link::Link;
use crate::target::{BlockTarget, Target};
use crate::{Error, Position, Vector};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FluidCollisionMode {
    #[default]
    Never,
    SourceOnly,
    Always,
}

impl FluidCollisionMode {
    const fn wire(self) -> v1::FluidMode {
        match self {
            Self::Never => v1::FluidMode::Never,
            Self::SourceOnly => v1::FluidMode::SourceOnly,
            Self::Always => v1::FluidMode::Always,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayTraceOptions {
    pub max_distance: f64,
    pub blocks: bool,
    pub entities: bool,
    pub fluid: FluidCollisionMode,
    pub ignore_passable: bool,
    pub ray_size: f64,
    pub ignore_entity: Option<i32>,
}

impl Default for RayTraceOptions {
    fn default() -> Self {
        Self {
            max_distance: 64.0,
            blocks: true,
            entities: true,
            fluid: FluidCollisionMode::Never,
            ignore_passable: false,
            ray_size: 0.0,
            ignore_entity: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RayHit {
    pub block: Option<BlockTarget>,
    pub entity: Option<Target>,
    pub position: Position,
    pub distance: f64,
}

pub(crate) async fn trace(
    link: &Link,
    origin: Position,
    direction: Vector,
    options: &RayTraceOptions,
) -> Result<RayHit, Error> {
    let query = lookup_question::Query::RayTrace(v1::RayTrace {
        origin: Some(origin.to_wire()),
        direction: Some(direction.to_wire()),
        max_distance: options.max_distance,
        blocks: options.blocks,
        entities: options.entities,
        fluid: options.fluid.wire() as i32,
        ignore_passable: options.ignore_passable,
        ray_size: options.ray_size,
        ignore_entity: options.ignore_entity,
    });
    let reply = link
        .ask(
            question::Question::Lookup(v1::LookupQuestion { query: Some(query) }),
            "a ray hit",
            |body| match body {
                answer::Body::Lookup(v1::LookupAnswer {
                    answer: Some(lookup_answer::Answer::RayHit(hit)),
                }) => Some(hit),
                _ => None,
            },
        )
        .await?;
    let position = Position::from_wire(reply.position, "position")?;
    let hit = reply.hit.and_then(|target| target.hit);
    let (block, entity) = match hit {
        Some(v1::target::Hit::Block(block)) => (
            BlockTarget::from_wire(Some(v1::target::Hit::Block(block)), link)?,
            None,
        ),
        other => (None, Target::from_wire(other, link)?),
    };
    Ok(RayHit {
        block,
        entity,
        position,
        distance: reply.distance,
    })
}
