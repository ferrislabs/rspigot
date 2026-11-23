use rspigot_core::ports::{PlaceFailure, PlaceRequest, PlacedBlock};
use rspigot_domain::BlockPos;
use rspigot_domain::BlockStateId;
use rspigot_domain::world::Dimension;

use crate::structures::{Library, jigsaw_blocks, template_blocks};

pub(crate) fn placed(
    dimension: &Dimension,
    blocks: Vec<((i32, i32, i32), BlockStateId)>,
) -> Result<Vec<PlacedBlock>, PlaceFailure> {
    let height = dimension.height;
    let out: Vec<PlacedBlock> = blocks
        .into_iter()
        .filter(|((_, y, _), _)| height.contains(*y))
        .map(|((x, y, z), state)| PlacedBlock {
            pos: BlockPos::new(x, y, z),
            state,
        })
        .collect();
    if out.is_empty() {
        Err(PlaceFailure::Failed)
    } else {
        Ok(out)
    }
}

pub(crate) fn standalone(
    dimension: &Dimension,
    request: &PlaceRequest,
) -> Result<Vec<PlacedBlock>, PlaceFailure> {
    let library = Library::current();
    match request {
        PlaceRequest::Template {
            id,
            pos,
            rotation,
            mirror,
            integrity,
            seed,
        } => {
            let blueprint = library.blueprint(id).ok_or(PlaceFailure::Unknown)?;
            placed(
                dimension,
                template_blocks(
                    blueprint,
                    (pos.x, pos.y, pos.z),
                    *rotation,
                    *mirror,
                    *integrity,
                    *seed,
                ),
            )
        }
        PlaceRequest::Jigsaw {
            pool,
            target,
            max_depth,
            pos,
        } => placed(
            dimension,
            jigsaw_blocks(
                library,
                pool,
                target,
                *max_depth,
                (pos.x, pos.y, pos.z),
                (dimension.height.min_y, dimension.height.max_y()),
            )?,
        ),
        PlaceRequest::Grow { .. } => crate::growth::grow(dimension, request),
        PlaceRequest::Structure { .. } | PlaceRequest::Feature { .. } => Err(PlaceFailure::Failed),
    }
}
