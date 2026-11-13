use crate::random::RandomSource;
use rspigot_blocks::{BlockKind, StateFlags};
use rspigot_domain::{BlockPos, Vec3, mth};
use rspigot_entities::EntityKind;

use crate::brain::AiContext;
use crate::random::MobRandom;
use crate::world::{AiWorld, MobView};
use crate::{BlockView, Swimming, block_path_type};

const RANDOM_POS_ATTEMPTS: usize = 10;
const SWIMMABLE_ATTEMPTS: i32 = 10;
pub(crate) const HALF_PI: f64 = 1.570_796_370_506_286_6;
const SQRT_OF_TWO: f32 = std::f32::consts::SQRT_2;
const GRASS_WALK_VALUE: f32 = 10.0;
const INFESTED_WALK_VALUE: f32 = 10.0;

pub fn light_magic_value(world: &impl AiWorld, pos: BlockPos) -> f32 {
    let level = world.raw_brightness(pos) as f32 / 15.0;
    let curve = level / (4.0 - 3.0 * level);
    curve + world.ambient_light() * (1.0 - curve)
}

pub fn mob_light_magic_value(world: &impl AiWorld, mob: &MobView) -> f32 {
    let block = mob.block_pos();
    if !world.chunk_loaded(block.chunk()) {
        return 0.0;
    }
    light_magic_value(world, mob.eye_position().block())
}

const HOGLIN_REPELLENT_DISTANCE: f64 = 8.0;
const CRIMSON_NYLIUM_WALK_VALUE: f32 = 10.0;

fn context_walk_target_value<W: AiWorld>(context: &AiContext<'_, W>, pos: BlockPos) -> f32 {
    if context.mob.kind != EntityKind::Hoglin {
        return walk_target_value(context.mob.kind, context.world, pos);
    }
    let near_repellent = context.brain.repellent.is_some_and(|repellent| {
        let (dx, dy, dz) = (
            f64::from(repellent.x - pos.x),
            f64::from(repellent.y - pos.y),
            f64::from(repellent.z - pos.z),
        );
        dx * dx + dy * dy + dz * dz < HOGLIN_REPELLENT_DISTANCE * HOGLIN_REPELLENT_DISTANCE
    });
    if near_repellent {
        -1.0
    } else if context
        .world
        .state(pos.offset(0, -1, 0))
        .is_some_and(|state| state.kind() == Some(BlockKind::CrimsonNylium))
    {
        CRIMSON_NYLIUM_WALK_VALUE
    } else {
        0.0
    }
}

pub fn walk_target_value(kind: EntityKind, world: &impl AiWorld, pos: BlockPos) -> f32 {
    if kind.extends("Animal") {
        let below = world.state(pos.offset(0, -1, 0));
        if below.is_some_and(|state| state.kind() == Some(BlockKind::GrassBlock)) {
            GRASS_WALK_VALUE
        } else {
            light_magic_value(world, pos) - 0.5
        }
    } else if kind == EntityKind::Silverfish
        && world
            .state(pos.offset(0, -1, 0))
            .is_some_and(rspigot_blocks::infested::is_compatible_host_block)
    {
        INFESTED_WALK_VALUE
    } else if kind == EntityKind::Giant {
        light_magic_value(world, pos) - 0.5
    } else if kind.extends("Monster") {
        -(light_magic_value(world, pos) - 0.5)
    } else {
        0.0
    }
}

pub fn is_stable_destination(view: &impl BlockView, pos: BlockPos) -> bool {
    view.state(pos.offset(0, -1, 0))
        .is_some_and(|state| state.flags().contains(StateFlags::SOLID_RENDER))
}

fn is_stable<W: AiWorld>(context: &AiContext<'_, W>, pos: BlockPos) -> bool {
    if let Some(swimming) = context.brain.profile.swimming {
        return match swimming {
            Swimming::WaterBound { .. } => !context
                .world
                .state(pos)
                .is_some_and(|state| state.flags().contains(StateFlags::SOLID_RENDER)),
            Swimming::Amphibious { .. } => context
                .world
                .state(pos.offset(0, -1, 0))
                .is_some_and(|state| !state.is_air()),
        };
    }
    if context.mob.kind == EntityKind::Bee {
        return context
            .world
            .state(pos.offset(0, -1, 0))
            .is_some_and(|state| !state.is_air());
    }
    is_stable_destination(context.world, pos)
}

pub fn is_solid(view: &impl BlockView, pos: BlockPos) -> bool {
    view.state(pos)
        .is_some_and(|state| state.flags().contains(StateFlags::LEGACY_SOLID))
}

fn is_water(view: &impl BlockView, pos: BlockPos) -> bool {
    view.state(pos)
        .is_some_and(|state| state.flags().contains(StateFlags::WATER))
}

fn is_outside_limits(view: &impl BlockView, pos: BlockPos) -> bool {
    pos.y < view.min_y() || pos.y > view.max_y()
}

fn has_malus<W: AiWorld>(context: &AiContext<'_, W>, pos: BlockPos) -> bool {
    context
        .brain
        .profile
        .malus(block_path_type(context.world, pos))
        != 0.0
}

pub fn random_direction(random: &mut MobRandom, horizontal: i32, vertical: i32) -> BlockPos {
    let x = random.next_int(2 * horizontal + 1) - horizontal;
    let y = random.next_int(2 * vertical + 1) - vertical;
    let z = random.next_int(2 * horizontal + 1) - horizontal;
    BlockPos::new(x, y, z)
}

fn toward_direction(
    mob: &MobView,
    random: &mut MobRandom,
    range: f64,
    direction: BlockPos,
) -> BlockPos {
    let mut dx = f64::from(direction.x);
    let mut dz = f64::from(direction.z);
    if mob.has_home() && range > 1.0 {
        let home = mob.home_pos;
        if mob.position.x > f64::from(home.x) {
            dx -= random.next_double() * range / 2.0;
        } else {
            dx += random.next_double() * range / 2.0;
        }
        if mob.position.z > f64::from(home.z) {
            dz -= random.next_double() * range / 2.0;
        } else {
            dz += random.next_double() * range / 2.0;
        }
    }
    Vec3::new(
        dx + mob.position.x,
        f64::from(direction.y) + mob.position.y,
        dz + mob.position.z,
    )
    .block()
}

fn generate<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    supplier: impl FnMut(&mut AiContext<'_, W>) -> Option<BlockPos>,
) -> Option<Vec3> {
    generate_scored(context, supplier, |context, pos| {
        f64::from(context_walk_target_value(context, pos))
    })
}

fn generate_scored<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    mut supplier: impl FnMut(&mut AiContext<'_, W>) -> Option<BlockPos>,
    scorer: impl Fn(&AiContext<'_, W>, BlockPos) -> f64,
) -> Option<Vec3> {
    let mut best = f64::NEG_INFINITY;
    let mut found = None;
    for _ in 0..RANDOM_POS_ATTEMPTS {
        let Some(pos) = supplier(context) else {
            continue;
        };
        let value = scorer(context, pos);
        if value > best {
            best = value;
            found = Some(pos);
        }
    }
    found.map(BlockPos::center)
}

fn default_toward<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    horizontal: i32,
    restricted: bool,
    direction: BlockPos,
) -> Option<BlockPos> {
    let pos = toward_direction(
        context.mob,
        &mut context.brain.random,
        f64::from(horizontal),
        direction,
    );
    let rejected = is_outside_limits(context.world, pos)
        || (restricted && !context.mob.is_within_home(pos))
        || !is_stable(context, pos)
        || has_malus(context, pos);
    (!rejected).then_some(pos)
}

pub fn default_pos<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    horizontal: i32,
    vertical: i32,
) -> Option<Vec3> {
    let restricted = context.mob.is_restricted(f64::from(horizontal));
    generate(context, |context| {
        let direction = random_direction(&mut context.brain.random, horizontal, vertical);
        default_toward(context, horizontal, restricted, direction)
    })
}

pub fn swimmable_pos<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    horizontal: i32,
    vertical: i32,
) -> Option<Vec3> {
    let mut position = default_pos(context, horizontal, vertical);
    let mut attempts = 0;
    while let Some(found) = position {
        if crate::swim::is_water_pathfindable(context.world.state(found.block())) {
            return Some(found);
        }
        let again = attempts < SWIMMABLE_ATTEMPTS;
        attempts += 1;
        if !again {
            return Some(found);
        }
        position = default_pos(context, horizontal, vertical);
    }
    None
}

pub fn direction_within_radians(
    random: &mut MobRandom,
    max_horizontal: f64,
    vertical: i32,
    direction: (f64, f64),
    max_radians: f64,
) -> Option<BlockPos> {
    direction_within_radians_offset(random, max_horizontal, vertical, 0, direction, max_radians)
}

pub fn direction_within_radians_offset(
    random: &mut MobRandom,
    max_horizontal: f64,
    vertical: i32,
    y_offset: i32,
    direction: (f64, f64),
    max_radians: f64,
) -> Option<BlockPos> {
    let center = mth::atan2(direction.1, direction.0) - HALF_PI;
    let yaw = center + f64::from(2.0 * random.next_float() - 1.0) * max_radians;
    let distance =
        mth::lerp(random.next_double().sqrt(), 0.0, max_horizontal) * f64::from(SQRT_OF_TWO);
    let x = -distance * yaw.sin();
    let z = distance * yaw.cos();
    if x.abs() > max_horizontal || z.abs() > max_horizontal {
        return None;
    }
    let y = random.next_int(2 * vertical + 1) - vertical + y_offset;
    Some(Vec3::new(x, f64::from(y), z).block())
}

pub fn pos_away<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    horizontal: i32,
    vertical: i32,
    away_from: Vec3,
) -> Option<Vec3> {
    let direction = context.mob.position - away_from;
    pos_in_direction(
        context,
        horizontal,
        vertical,
        (direction.x, direction.z),
        HALF_PI,
    )
}

pub fn pos_towards<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    horizontal: i32,
    vertical: i32,
    towards: Vec3,
    max_radians: f64,
) -> Option<Vec3> {
    let direction = towards - context.mob.position;
    pos_in_direction(
        context,
        horizontal,
        vertical,
        (direction.x, direction.z),
        max_radians,
    )
}

fn pos_in_direction<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    horizontal: i32,
    vertical: i32,
    direction: (f64, f64),
    max_radians: f64,
) -> Option<Vec3> {
    let restricted = context.mob.is_restricted(f64::from(horizontal));
    generate(context, |context| {
        let pos = direction_within_radians(
            &mut context.brain.random,
            f64::from(horizontal),
            vertical,
            direction,
            max_radians,
        )?;
        default_toward(context, horizontal, restricted, pos)
    })
}

pub fn move_up_out_of_solid(view: &impl BlockView, pos: BlockPos) -> BlockPos {
    if !is_solid(view, pos) {
        return pos;
    }
    let mut above = pos.offset(0, 1, 0);
    while above.y <= view.max_y() && is_solid(view, above) {
        above = above.offset(0, 1, 0);
    }
    above
}

pub fn land_pos_towards<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    horizontal: i32,
    vertical: i32,
    towards: Vec3,
) -> Option<Vec3> {
    let direction = towards - context.mob.position;
    land_pos_in_direction(context, horizontal, vertical, direction)
}

pub fn land_pos_away<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    horizontal: i32,
    vertical: i32,
    away_from: Vec3,
) -> Option<Vec3> {
    let direction = context.mob.position - away_from;
    land_pos_in_direction(context, horizontal, vertical, direction)
}

fn land_pos_in_direction<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    horizontal: i32,
    vertical: i32,
    direction: Vec3,
) -> Option<Vec3> {
    let restricted = context.mob.is_restricted(f64::from(horizontal));
    generate(context, |context| {
        let offset = direction_within_radians(
            &mut context.brain.random,
            f64::from(horizontal),
            vertical,
            (direction.x, direction.z),
            HALF_PI,
        )?;
        let pos = toward_direction(
            context.mob,
            &mut context.brain.random,
            f64::from(horizontal),
            offset,
        );
        if is_outside_limits(context.world, pos)
            || (restricted && !context.mob.is_within_home(pos))
            || !is_stable(context, pos)
        {
            return None;
        }
        let raised = move_up_out_of_solid(context.world, pos);
        (!is_water(context.world, raised) && !has_malus(context, raised)).then_some(raised)
    })
}

pub fn land_pos<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    horizontal: i32,
    vertical: i32,
) -> Option<Vec3> {
    land_pos_scored(context, horizontal, vertical, |context, pos| {
        f64::from(context_walk_target_value(context, pos))
    })
}

pub fn land_pos_scored<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    horizontal: i32,
    vertical: i32,
    scorer: impl Fn(&AiContext<'_, W>, BlockPos) -> f64,
) -> Option<Vec3> {
    let restricted = context.mob.is_restricted(f64::from(horizontal));
    generate_scored(
        context,
        |context| {
            let direction = random_direction(&mut context.brain.random, horizontal, vertical);
            let pos = toward_direction(
                context.mob,
                &mut context.brain.random,
                f64::from(horizontal),
                direction,
            );
            if is_outside_limits(context.world, pos)
                || (restricted && !context.mob.is_within_home(pos))
                || !is_stable(context, pos)
            {
                return None;
            }
            let raised = move_up_out_of_solid(context.world, pos);
            (!is_water(context.world, raised) && !has_malus(context, raised)).then_some(raised)
        },
        scorer,
    )
}

pub fn move_up_to_above_solid(
    view: &impl BlockView,
    pos: BlockPos,
    above_solid: i32,
    max_y: i32,
) -> BlockPos {
    if !is_solid(view, pos) {
        return pos;
    }
    let mut cursor = pos.offset(0, 1, 0);
    while cursor.y <= max_y && is_solid(view, cursor) {
        cursor = cursor.offset(0, 1, 0);
    }
    let start = cursor.y;
    loop {
        if cursor.y > max_y || above_solid <= cursor.y - start {
            return cursor;
        }
        cursor = cursor.offset(0, 1, 0);
        if is_solid(view, cursor) {
            return cursor.offset(0, -1, 0);
        }
    }
}

fn air_and_water_candidate<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    horizontal: i32,
    vertical: i32,
    flying_height: i32,
    direction: (f64, f64),
    max_radians: f64,
    restricted: bool,
) -> Option<BlockPos> {
    let offset = direction_within_radians_offset(
        &mut context.brain.random,
        f64::from(horizontal),
        vertical,
        flying_height,
        direction,
        max_radians,
    )?;
    let pos = toward_direction(
        context.mob,
        &mut context.brain.random,
        f64::from(horizontal),
        offset,
    );
    if is_outside_limits(context.world, pos) || (restricted && !context.mob.is_within_home(pos)) {
        return None;
    }
    let raised = move_up_out_of_solid(context.world, pos);
    (!has_malus(context, raised)).then_some(raised)
}

pub fn air_and_water_pos<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    horizontal: i32,
    vertical: i32,
    flying_height: i32,
    direction: (f64, f64),
    max_radians: f64,
) -> Option<Vec3> {
    let restricted = context.mob.is_restricted(f64::from(horizontal));
    generate(context, |context| {
        air_and_water_candidate(
            context,
            horizontal,
            vertical,
            flying_height,
            direction,
            max_radians,
            restricted,
        )
    })
}

pub fn air_pos_towards<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    horizontal: i32,
    vertical: i32,
    flying_height: i32,
    towards: Vec3,
    max_radians: f64,
) -> Option<Vec3> {
    let direction = towards - context.mob.position;
    let restricted = context.mob.is_restricted(f64::from(horizontal));
    generate(context, |context| {
        let pos = air_and_water_candidate(
            context,
            horizontal,
            vertical,
            flying_height,
            (direction.x, direction.z),
            max_radians,
            restricted,
        )?;
        (!is_water(context.world, pos)).then_some(pos)
    })
}

pub fn hover_pos<W: AiWorld>(
    context: &mut AiContext<'_, W>,
    horizontal: i32,
    vertical: i32,
    direction: (f64, f64),
    max_radians: f64,
    hover_max_height: i32,
    hover_min_height: i32,
) -> Option<Vec3> {
    let restricted = context.mob.is_restricted(f64::from(horizontal));
    generate(context, |context| {
        let offset = direction_within_radians(
            &mut context.brain.random,
            f64::from(horizontal),
            vertical,
            direction,
            max_radians,
        )?;
        let pos = toward_direction(
            context.mob,
            &mut context.brain.random,
            f64::from(horizontal),
            offset,
        );
        if is_outside_limits(context.world, pos)
            || (restricted && !context.mob.is_within_home(pos))
            || !is_stable(context, pos)
        {
            return None;
        }
        let height = context
            .brain
            .random
            .next_int(hover_max_height - hover_min_height + 1)
            + hover_min_height;
        let max_y = context.world.max_y();
        let raised = move_up_to_above_solid(context.world, pos, height, max_y);
        (!is_water(context.world, raised) && !has_malus(context, raised)).then_some(raised)
    })
}
