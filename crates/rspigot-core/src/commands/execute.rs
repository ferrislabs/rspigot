use rspigot_domain::chunk::HeightmapKind;
use rspigot_domain::text::{Component, NamedColor};
use rspigot_domain::{BlockPos, Identifier, Rotation, Vec3};
use rspigot_entities::EntityKind;
use rspigot_nbt::{NbtPath, Tag};

use super::args::{BlockPredicate, take};
use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, Target, execute, next_argument, parse_vec3,
    unknown_dimension,
};
use super::scoreboard_commands::{resolve_holders, resolve_single_holder};
use super::selector;
use super::source::{Anchor, DataTarget, Relation, Source, heightmap};

const MAX_COMPARED_BLOCKS: i64 = 32_768;

#[derive(Debug, Clone)]
enum Cond {
    Block(BlockPredicate, [String; 3]),
    Blocks {
        start: [String; 3],
        end: [String; 3],
        destination: [String; 3],
        masked: bool,
    },
    Biome(String, [String; 3]),
    Loaded([String; 3]),
    Dimension(Identifier),
    Entity(String),
    Predicate(Identifier),
    Function(Identifier),
    Data(DataSpec, NbtPath),
    Score {
        target: String,
        objective: String,
        test: ScoreTest,
    },
}

#[derive(Debug, Clone)]
enum ScoreTest {
    Compare {
        operator: String,
        source: String,
        source_objective: String,
    },
    Matches(selector::IntRange),
}

#[derive(Debug, Clone)]
enum DataSpec {
    Entity(String),
    Block([String; 3]),
    Storage(Identifier),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Numeric {
    Int,
    Float,
    Short,
    Long,
    Double,
    Byte,
}

#[derive(Debug, Clone)]
enum Sink {
    Score(String, String),
    BossBar(Identifier, bool),
    Data(DataSpec, NbtPath, Numeric, f64),
}

#[derive(Debug, Clone)]
enum Step {
    As(String),
    At(String),
    Positioned([String; 3]),
    PositionedAs(String),
    PositionedOver(HeightmapKind),
    Rotated([String; 2]),
    RotatedAs(String),
    FacingPos([String; 3]),
    FacingEntity(String, Anchor),
    Align([bool; 3]),
    Anchored(Anchor),
    In(Identifier),
    On(Relation),
    Summon(EntityKind),
    If(bool, Cond),
    Store(bool, Sink),
}

struct Chain {
    steps: Vec<Step>,
    command: Option<String>,
}

enum Problem {
    Unknown,
    Message(Component),
}

impl From<Component> for Problem {
    fn from(message: Component) -> Self {
        Self::Message(message)
    }
}

fn message(key: &str, args: &[&str]) -> Problem {
    Problem::Message(Component::translatable(
        key,
        args.iter().map(|arg| Component::text(*arg)).collect(),
    ))
}

fn triple(rest: &str) -> Result<([String; 3], &str), Problem> {
    let mut out: [String; 3] = Default::default();
    let mut cursor = rest;
    for slot in &mut out {
        let (token, after) = next_argument(cursor).ok_or(Problem::Unknown)?;
        *slot = token.to_owned();
        cursor = after;
    }
    Ok((out, cursor))
}

fn selector_token(rest: &str) -> Result<(String, &str), Problem> {
    let (token, after) = take(rest).ok_or(Problem::Unknown)?;
    selector::parse(token)?;
    Ok((token.to_owned(), after))
}

fn identifier(token: &str) -> Result<Identifier, Problem> {
    Identifier::parse(token).map_err(|_| message("argument.id.invalid", &[]))
}

fn parse_data_spec(rest: &str) -> Result<(DataSpec, &str), Problem> {
    let (kind, rest) = take(rest).ok_or(Problem::Unknown)?;
    parse_data_kind(kind, rest)
}

fn parse_data_kind<'a>(kind: &str, rest: &'a str) -> Result<(DataSpec, &'a str), Problem> {
    match kind {
        "entity" => {
            let (sel, after) = selector_token(rest)?;
            Ok((DataSpec::Entity(sel), after))
        }
        "block" => {
            let (pos, after) = triple(rest)?;
            Ok((DataSpec::Block(pos), after))
        }
        "storage" => {
            let (id, after) = take(rest).ok_or(Problem::Unknown)?;
            Ok((DataSpec::Storage(identifier(id)?), after))
        }
        _ => Err(Problem::Unknown),
    }
}

fn parse_path(token: &str) -> Result<NbtPath, Problem> {
    NbtPath::parse(token).map_err(|e| message(e.key, &[]))
}

fn objective_missing(token: &str) -> Component {
    Component::translatable(
        "arguments.objective.notFound",
        vec![Component::text(token.to_owned())],
    )
}

fn parse_condition(rest: &str) -> Result<(Cond, &str), Problem> {
    let (kind, rest) = take(rest).ok_or(Problem::Unknown)?;
    match kind {
        "block" => {
            let (pos, rest) = triple(rest)?;
            let (token, rest) = take(rest).ok_or(Problem::Unknown)?;
            Ok((Cond::Block(BlockPredicate::parse(token)?, pos), rest))
        }
        "biome" => {
            let (pos, rest) = triple(rest)?;
            let (token, rest) = take(rest).ok_or(Problem::Unknown)?;
            let bare = token.trim_start_matches('#');
            let id = if bare.contains(':') {
                bare.to_owned()
            } else {
                format!("minecraft:{bare}")
            };
            let known = if token.starts_with('#') {
                rspigot_data::tags::tag("minecraft:worldgen/biome", &id).is_some()
            } else {
                rspigot_data::registries::biome_id(&id).is_some()
            };
            if !known {
                return Err(message(
                    "argument.resource.not_found",
                    &[&id, "minecraft:worldgen/biome"],
                ));
            }
            let canonical = if token.starts_with('#') {
                format!("#{id}")
            } else {
                id
            };
            Ok((Cond::Biome(canonical, pos), rest))
        }
        "loaded" => {
            let (pos, rest) = triple(rest)?;
            Ok((Cond::Loaded(pos), rest))
        }
        "dimension" => {
            let (token, rest) = take(rest).ok_or(Problem::Unknown)?;
            Ok((Cond::Dimension(identifier(token)?), rest))
        }
        "score" => {
            let (target, rest) = take(rest).ok_or(Problem::Unknown)?;
            let (objective, rest) = take(rest).ok_or(Problem::Unknown)?;
            let (kind, rest) = take(rest).ok_or(Problem::Unknown)?;
            let (test, rest) = match kind {
                "matches" => {
                    let (range, rest) = take(rest).ok_or(Problem::Unknown)?;
                    (ScoreTest::Matches(selector::parse_int_range(range)?), rest)
                }
                "=" | "<" | "<=" | ">" | ">=" => {
                    let (source, rest) = take(rest).ok_or(Problem::Unknown)?;
                    let (source_objective, rest) = take(rest).ok_or(Problem::Unknown)?;
                    (
                        ScoreTest::Compare {
                            operator: kind.to_owned(),
                            source: source.to_owned(),
                            source_objective: source_objective.to_owned(),
                        },
                        rest,
                    )
                }
                _ => return Err(Problem::Unknown),
            };
            Ok((
                Cond::Score {
                    target: target.to_owned(),
                    objective: objective.to_owned(),
                    test,
                },
                rest,
            ))
        }
        "blocks" => {
            let (start, rest) = triple(rest)?;
            let (end, rest) = triple(rest)?;
            let (destination, rest) = triple(rest)?;
            let (mode, rest) = take(rest).ok_or(Problem::Unknown)?;
            let masked = match mode {
                "all" => false,
                "masked" => true,
                _ => return Err(Problem::Unknown),
            };
            Ok((
                Cond::Blocks {
                    start,
                    end,
                    destination,
                    masked,
                },
                rest,
            ))
        }
        "entity" => {
            let (sel, rest) = selector_token(rest)?;
            Ok((Cond::Entity(sel), rest))
        }
        "predicate" => {
            let (token, rest) = take(rest).ok_or(Problem::Unknown)?;
            Ok((Cond::Predicate(identifier(token)?), rest))
        }
        "function" => {
            let (token, rest) = take(rest).ok_or(Problem::Unknown)?;
            Ok((Cond::Function(identifier(token)?), rest))
        }
        "data" => {
            let (spec, rest) = parse_data_spec(rest)?;
            let (path, rest) = take(rest).ok_or(Problem::Unknown)?;
            Ok((Cond::Data(spec, parse_path(path)?), rest))
        }
        _ => Err(Problem::Unknown),
    }
}

fn parse_store(rest: &str) -> Result<(Sink, &str), Problem> {
    let (kind, rest) = take(rest).ok_or(Problem::Unknown)?;
    match kind {
        "score" => {
            let (targets, rest) = take(rest).ok_or(Problem::Unknown)?;
            let (objective, rest) = take(rest).ok_or(Problem::Unknown)?;
            Ok((Sink::Score(targets.to_owned(), objective.to_owned()), rest))
        }
        "bossbar" => {
            let (id, rest) = take(rest).ok_or(Problem::Unknown)?;
            let (field, rest) = take(rest).ok_or(Problem::Unknown)?;
            let maximum = match field {
                "value" => false,
                "max" => true,
                _ => return Err(Problem::Unknown),
            };
            Ok((Sink::BossBar(identifier(id)?, maximum), rest))
        }
        _ => {
            let (spec, rest) = parse_data_kind(kind, rest)?;
            let (path, rest) = take(rest).ok_or(Problem::Unknown)?;
            let path = parse_path(path)?;
            let (numeric, rest) = take(rest).ok_or(Problem::Unknown)?;
            let numeric = match numeric {
                "int" => Numeric::Int,
                "float" => Numeric::Float,
                "short" => Numeric::Short,
                "long" => Numeric::Long,
                "double" => Numeric::Double,
                "byte" => Numeric::Byte,
                _ => return Err(Problem::Unknown),
            };
            let (scale, rest) = take(rest).ok_or(Problem::Unknown)?;
            let scale = scale
                .parse::<f64>()
                .map_err(|_| message("parsing.double.invalid", &[scale]))?;
            Ok((Sink::Data(spec, path, numeric, scale), rest))
        }
    }
}

fn parse_chain(rest: &str) -> Result<Chain, Problem> {
    let mut steps = Vec::new();
    let mut cursor = rest;
    loop {
        let Some((word, after)) = next_argument(cursor) else {
            if matches!(steps.last(), Some(Step::If(..))) {
                return Ok(Chain {
                    steps,
                    command: None,
                });
            }
            return Err(Problem::Unknown);
        };
        cursor = after;
        match word {
            "run" => {
                let command = after.trim();
                if command.is_empty() {
                    return Err(Problem::Unknown);
                }
                return Ok(Chain {
                    steps,
                    command: Some(command.to_owned()),
                });
            }
            "as" => {
                let (sel, after) = selector_token(cursor)?;
                steps.push(Step::As(sel));
                cursor = after;
            }
            "at" => {
                let (sel, after) = selector_token(cursor)?;
                steps.push(Step::At(sel));
                cursor = after;
            }
            "positioned" => {
                let (first, after) = next_argument(cursor).ok_or(Problem::Unknown)?;
                match first {
                    "as" => {
                        let (sel, after) = selector_token(after)?;
                        steps.push(Step::PositionedAs(sel));
                        cursor = after;
                    }
                    "over" => {
                        let (name, after) = next_argument(after).ok_or(Problem::Unknown)?;
                        let kind = heightmap(name)
                            .ok_or_else(|| message("argument.enum.invalid", &[name]))?;
                        steps.push(Step::PositionedOver(kind));
                        cursor = after;
                    }
                    _ => {
                        let (pos, after) = triple(cursor)?;
                        steps.push(Step::Positioned(pos));
                        cursor = after;
                    }
                }
            }
            "rotated" => {
                let (first, after) = next_argument(cursor).ok_or(Problem::Unknown)?;
                if first == "as" {
                    let (sel, after) = selector_token(after)?;
                    steps.push(Step::RotatedAs(sel));
                    cursor = after;
                } else {
                    let (second, after) = next_argument(after).ok_or(Problem::Unknown)?;
                    steps.push(Step::Rotated([first.to_owned(), second.to_owned()]));
                    cursor = after;
                }
            }
            "facing" => {
                let (first, after) = next_argument(cursor).ok_or(Problem::Unknown)?;
                if first == "entity" {
                    let (sel, after) = selector_token(after)?;
                    let (anchor, after) = next_argument(after).ok_or(Problem::Unknown)?;
                    let anchor = Anchor::parse(anchor)
                        .ok_or_else(|| message("argument.anchor.invalid", &[anchor]))?;
                    steps.push(Step::FacingEntity(sel, anchor));
                    cursor = after;
                } else {
                    let (pos, after) = triple(cursor)?;
                    steps.push(Step::FacingPos(pos));
                    cursor = after;
                }
            }
            "align" => {
                let (axes, after) = next_argument(cursor).ok_or(Problem::Unknown)?;
                let mut flags = [false; 3];
                for axis in axes.chars() {
                    let slot = match axis {
                        'x' => 0,
                        'y' => 1,
                        'z' => 2,
                        _ => return Err(message("arguments.swizzle.invalid", &[])),
                    };
                    if flags[slot] {
                        return Err(message("arguments.swizzle.invalid", &[]));
                    }
                    flags[slot] = true;
                }
                steps.push(Step::Align(flags));
                cursor = after;
            }
            "anchored" => {
                let (anchor, after) = next_argument(cursor).ok_or(Problem::Unknown)?;
                let anchor = Anchor::parse(anchor)
                    .ok_or_else(|| message("argument.anchor.invalid", &[anchor]))?;
                steps.push(Step::Anchored(anchor));
                cursor = after;
            }
            "in" => {
                let (name, after) = next_argument(cursor).ok_or(Problem::Unknown)?;
                steps.push(Step::In(identifier(name)?));
                cursor = after;
            }
            "on" => {
                let (name, after) = next_argument(cursor).ok_or(Problem::Unknown)?;
                steps.push(Step::On(Relation::parse(name).ok_or(Problem::Unknown)?));
                cursor = after;
            }
            "summon" => {
                let (name, after) = next_argument(cursor).ok_or(Problem::Unknown)?;
                let id = if name.contains(':') {
                    name.to_owned()
                } else {
                    format!("minecraft:{name}")
                };
                let kind = EntityKind::from_name(&id).ok_or_else(|| {
                    message(
                        "argument.resource.not_found",
                        &[&id, "minecraft:entity_type"],
                    )
                })?;
                steps.push(Step::Summon(kind));
                cursor = after;
            }
            "if" | "unless" => {
                let (condition, after) = parse_condition(cursor)?;
                steps.push(Step::If(word == "if", condition));
                cursor = after;
            }
            "store" => {
                let (mode, after) = next_argument(cursor).ok_or(Problem::Unknown)?;
                let result = match mode {
                    "result" => true,
                    "success" => false,
                    _ => return Err(Problem::Unknown),
                };
                let (sink, after) = parse_store(after)?;
                steps.push(Step::Store(result, sink));
                cursor = after;
            }
            _ => return Err(Problem::Unknown),
        }
    }
}

#[derive(Clone)]
struct State {
    source: Source,
    stores: Vec<(bool, Sink)>,
}

#[derive(Default)]
struct Tally {
    successes: i32,
    total: i64,
}

fn floor(value: f64) -> i32 {
    value.floor() as i32
}

fn block_pos<C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    raw: &[String; 3],
) -> Result<BlockPos, Component> {
    let refs = [raw[0].as_str(), raw[1].as_str(), raw[2].as_str()];
    let position = parse_vec3(ctx, sender, refs)?;
    Ok(BlockPos::new(
        floor(position.x),
        floor(position.y),
        floor(position.z),
    ))
}

fn loaded_pos<C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    raw: &[String; 3],
) -> Result<BlockPos, Component> {
    let pos = block_pos(ctx, sender, raw)?;
    if ctx.is_position_loaded(pos) {
        Ok(pos)
    } else {
        Err(Component::translatable("argument.pos.unloaded", Vec::new()))
    }
}

fn wrap_degrees(value: f32) -> f32 {
    let wrapped = value % 360.0;
    if wrapped >= 180.0 {
        wrapped - 360.0
    } else if wrapped < -180.0 {
        wrapped + 360.0
    } else {
        wrapped
    }
}

pub(super) fn parse_angle(raw: &str, base: f32) -> Result<f32, Component> {
    let (relative, number) = match raw.strip_prefix('~') {
        Some(rest) => (true, rest),
        None => (false, raw),
    };
    let value = if number.is_empty() && relative {
        0.0
    } else {
        number.parse::<f32>().map_err(|_| {
            Component::translatable("parsing.float.invalid", vec![Component::text(number)])
        })?
    };
    Ok(if relative { base + value } else { value })
}

fn anchored_position<C: CommandContext>(ctx: &C, source: &Source) -> Vec3 {
    match (source.anchor, source.executor) {
        (Anchor::Eyes, Some(executor)) => Vec3::new(
            source.position.x,
            source.position.y + ctx.target_eye_height(executor),
            source.position.z,
        ),
        _ => source.position,
    }
}

pub(super) fn look_at(from: Vec3, to: Vec3) -> Rotation {
    let (dx, dy, dz) = (to.x - from.x, to.y - from.y, to.z - from.z);
    let horizontal = dx.hypot(dz);
    let pitch = wrap_degrees((-dy.atan2(horizontal).to_degrees()) as f32);
    let yaw = wrap_degrees((dz.atan2(dx).to_degrees() - 90.0) as f32);
    Rotation::new(yaw, pitch)
}

fn data_target<C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    spec: &DataSpec,
) -> Result<DataTarget, Component> {
    match spec {
        DataSpec::Entity(sel) => {
            let found = ctx.select_entities(sender, sel)?;
            match found.as_slice() {
                [single] => Ok(DataTarget::Entity(*single)),
                [] => Err(Component::translatable(
                    "argument.entity.notfound.entity",
                    Vec::new(),
                )),
                _ => Err(Component::translatable(
                    "argument.entity.toomany",
                    Vec::new(),
                )),
            }
        }
        DataSpec::Block(pos) => Ok(DataTarget::Block(block_pos(ctx, sender, pos)?)),
        DataSpec::Storage(id) => Ok(DataTarget::Storage(id.clone())),
    }
}

fn blocks_equal<C: CommandContext>(
    ctx: &C,
    masked: bool,
    from: BlockPos,
    to: BlockPos,
) -> Option<bool> {
    let source = ctx.block_state_at(from)?;
    if masked && source.is_air() {
        return Some(true);
    }
    let destination = ctx.block_state_at(to)?;
    if source != destination {
        return Some(false);
    }
    Some(ctx.block_entity_data(from) == ctx.block_entity_data(to))
}

fn test_blocks<C: CommandContext>(
    ctx: &C,
    sender: CommandSender,
    spec: (&[String; 3], &[String; 3], &[String; 3], bool),
) -> Result<i32, Component> {
    let (start, end, destination, masked) = spec;
    let start = loaded_pos(ctx, sender, start)?;
    let end = loaded_pos(ctx, sender, end)?;
    let destination = loaded_pos(ctx, sender, destination)?;
    let (min, max) = (
        BlockPos::new(start.x.min(end.x), start.y.min(end.y), start.z.min(end.z)),
        BlockPos::new(start.x.max(end.x), start.y.max(end.y), start.z.max(end.z)),
    );
    let (sx, sy, sz) = (
        i64::from(max.x - min.x + 1),
        i64::from(max.y - min.y + 1),
        i64::from(max.z - min.z + 1),
    );
    let volume = sx * sy * sz;
    if volume > MAX_COMPARED_BLOCKS {
        return Err(Component::translatable(
            "commands.execute.blocks.toobig",
            vec![
                Component::text(MAX_COMPARED_BLOCKS.to_string()),
                Component::text(volume.to_string()),
            ],
        ));
    }
    let offset = (
        destination.x - min.x,
        destination.y - min.y,
        destination.z - min.z,
    );
    let far = BlockPos::new(max.x + offset.0, max.y + offset.1, max.z + offset.2);
    if !ctx.is_position_loaded(far) {
        return Err(Component::translatable("argument.pos.unloaded", Vec::new()));
    }
    let mut count = 0;
    for y in min.y..=max.y {
        for z in min.z..=max.z {
            for x in min.x..=max.x {
                let from = BlockPos::new(x, y, z);
                let to = BlockPos::new(x + offset.0, y + offset.1, z + offset.2);
                match blocks_equal(ctx, masked, from, to) {
                    Some(true) => count += 1,
                    _ => return Ok(0),
                }
            }
        }
    }
    Ok(count)
}

fn matches_biome(wanted: &str, actual: &str) -> bool {
    match wanted.strip_prefix('#') {
        Some(tag) => rspigot_data::tags::tag("minecraft:worldgen/biome", tag).is_some_and(|ids| {
            rspigot_data::registries::biome_id(actual)
                .is_some_and(|id| ids.binary_search(&u32::from(id)).is_ok())
        }),
        None => wanted == actual,
    }
}

fn counted(condition: &Cond) -> bool {
    matches!(
        condition,
        Cond::Entity(_) | Cond::Blocks { .. } | Cond::Data(..)
    )
}

fn test<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    condition: &Cond,
) -> Result<i32, Component> {
    match condition {
        Cond::Block(predicate, pos) => {
            let pos = loaded_pos(ctx, sender, pos)?;
            let state = ctx.block_state_at(pos);
            let entity = predicate
                .needs_block_entity()
                .then(|| ctx.block_entity_data(pos))
                .flatten();
            Ok(i32::from(state.is_some_and(|state| {
                predicate.matches(state, entity.as_ref())
            })))
        }
        Cond::Blocks {
            start,
            end,
            destination,
            masked,
        } => test_blocks(ctx, sender, (start, end, destination, *masked)),
        Cond::Biome(wanted, pos) => {
            let pos = loaded_pos(ctx, sender, pos)?;
            Ok(i32::from(
                ctx.biome_name_at(pos)
                    .is_some_and(|actual| matches_biome(wanted, &actual)),
            ))
        }
        Cond::Loaded(pos) => {
            let pos = block_pos(ctx, sender, pos)?;
            Ok(i32::from(ctx.is_position_loaded(pos)))
        }
        Cond::Dimension(wanted) => Ok(i32::from(ctx.current_dimension().as_ref() == Some(wanted))),
        Cond::Entity(sel) => Ok(ctx.select_entities(sender, sel)?.len() as i32),
        Cond::Predicate(id) => match ctx.predicate_holds(sender, id) {
            Some(held) => Ok(i32::from(held)),
            None => Err(Component::translatable(
                "argument.resource.not_found",
                vec![
                    Component::text(id.to_string()),
                    Component::text("minecraft:predicate"),
                ],
            )),
        },
        Cond::Function(id) => super::function_commands::probe(ctx, sender, id),
        Cond::Data(spec, path) => {
            let target = data_target(ctx, sender, spec)?;
            let data = Tag::Compound(ctx.read_data(&target)?);
            Ok(path.count(&data) as i32)
        }
        Cond::Score {
            target,
            objective,
            test,
        } => test_score(ctx, sender, target, objective, test),
    }
}

fn test_score<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    target: &str,
    objective: &str,
    test: &ScoreTest,
) -> Result<i32, Component> {
    if ctx.scoreboard().objective(objective).is_none() {
        return Err(objective_missing(objective));
    }
    let holder = resolve_single_holder(ctx, sender, target)?;
    let value = ctx.scoreboard().score(&holder, objective).map(|s| s.value);
    match test {
        ScoreTest::Matches((min, max)) => Ok(i32::from(value.is_some_and(|value| {
            min.is_none_or(|min| value >= min) && max.is_none_or(|max| value <= max)
        }))),
        ScoreTest::Compare {
            operator,
            source,
            source_objective,
        } => {
            if ctx.scoreboard().objective(source_objective).is_none() {
                return Err(objective_missing(source_objective));
            }
            let source = resolve_single_holder(ctx, sender, source)?;
            let other = ctx
                .scoreboard()
                .score(&source, source_objective)
                .map(|s| s.value);
            let (Some(a), Some(b)) = (value, other) else {
                return Ok(0);
            };
            Ok(i32::from(match operator.as_str() {
                "=" => a == b,
                "<" => a < b,
                "<=" => a <= b,
                ">" => a > b,
                _ => a >= b,
            }))
        }
    }
}

fn scaled(numeric: Numeric, value: i32, scale: f64) -> Tag {
    let product = f64::from(value) * scale;
    match numeric {
        Numeric::Int => Tag::Int(product as i32),
        Numeric::Float => Tag::Float((f64::from(value) * scale) as f32),
        Numeric::Short => Tag::Short(product as i16),
        Numeric::Long => Tag::Long(product as i64),
        Numeric::Double => Tag::Double(product),
        Numeric::Byte => Tag::Byte(product as i8),
    }
}

fn store_value<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    sink: &Sink,
    value: i32,
) -> Result<(), Component> {
    match sink {
        Sink::Score(targets, objective) => {
            let Some(found) = ctx.scoreboard().objective(objective) else {
                return Err(objective_missing(objective));
            };
            if found.criteria.read_only() {
                return Err(Component::translatable(
                    "arguments.objective.readonly",
                    vec![Component::text(objective.clone())],
                ));
            }
            let holders = resolve_holders(ctx, sender, targets)?;
            let displays: Vec<Option<Component>> = holders
                .iter()
                .map(|holder| ctx.holder_display_name(holder))
                .collect();
            ctx.change_scoreboard(|board| {
                let mut messages = Vec::new();
                for (holder, display) in holders.iter().zip(displays) {
                    if let Ok(sent) = board.set_score(holder, objective, value, display) {
                        messages.extend(sent);
                    }
                }
                ((), messages)
            });
            Ok(())
        }
        Sink::BossBar(id, maximum) => {
            if ctx.set_boss_bar_value(id, *maximum, value) {
                Ok(())
            } else {
                Err(Component::translatable(
                    "commands.bossbar.unknown",
                    vec![Component::text(id.to_string())],
                ))
            }
        }
        Sink::Data(spec, path, numeric, scale) => {
            let target = data_target(ctx, sender, spec)?;
            let mut data = Tag::Compound(ctx.read_data(&target).unwrap_or_default());
            path.set(&mut data, &scaled(*numeric, value, *scale));
            let Tag::Compound(data) = data else {
                return Ok(());
            };
            ctx.write_data(&target, data)
        }
    }
}

fn apply_stores<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    state: &State,
    success: bool,
    result: i32,
) {
    for (is_result, sink) in &state.stores {
        let value = if *is_result {
            result
        } else {
            i32::from(success)
        };
        if let Err(error) = store_value(ctx, sender, sink, value) {
            ctx.reply(sender, error.color(NamedColor::Red));
        }
    }
}

fn enter<C: CommandContext>(
    ctx: &mut C,
    dimension: Option<Identifier>,
    swap: bool,
    run: impl FnOnce(&mut C),
) -> bool {
    match dimension {
        Some(dimension) if ctx.current_dimension().as_ref() != Some(&dimension) => {
            ctx.run_in_dimension(&dimension, swap, run)
        }
        _ => {
            run(ctx);
            true
        }
    }
}

struct Plan<'a> {
    chain: &'a Chain,
    sender: CommandSender,
    swap: bool,
}

fn finish<C: CommandContext>(
    ctx: &mut C,
    plan: &Plan<'_>,
    state: &State,
    command: &str,
    tally: &mut Tally,
) {
    let mut outcome = CommandOutcome::Failure;
    ctx.run_with_source(&state.source, |ctx| {
        ctx.take_result();
        outcome = execute(ctx, plan.sender, command);
    });
    let success = outcome == CommandOutcome::Success;
    let result = ctx.take_result().unwrap_or(i32::from(success));
    if success {
        tally.successes += 1;
        tally.total += i64::from(result);
    }
    apply_stores(ctx, plan.sender, state, success, result);
}

fn finish_condition<C: CommandContext>(
    ctx: &mut C,
    plan: &Plan<'_>,
    state: &State,
    positive: bool,
    condition: &Cond,
    tally: &mut Tally,
) {
    let sender = plan.sender;
    let mut counted_result = Ok(0);
    ctx.run_with_source(&state.source, |ctx| {
        counted_result = test(ctx, sender, condition);
    });
    let count = match counted_result {
        Ok(count) => count,
        Err(error) => {
            ctx.reply(sender, error.color(NamedColor::Red));
            apply_stores(ctx, sender, state, false, 0);
            return;
        }
    };
    let numeric = counted(condition);
    let passed = (count > 0) == positive;
    if passed {
        let (key, args, result) = if positive && numeric {
            (
                "commands.execute.conditional.pass_count",
                vec![Component::text(count.to_string())],
                count,
            )
        } else {
            ("commands.execute.conditional.pass", Vec::new(), 1)
        };
        ctx.reply(sender, Component::translatable(key, args));
        tally.successes += 1;
        tally.total += i64::from(result);
        apply_stores(ctx, sender, state, true, result);
    } else {
        let (key, args) = if !positive && numeric {
            (
                "commands.execute.conditional.fail_count",
                vec![Component::text(count.to_string())],
            )
        } else {
            ("commands.execute.conditional.fail", Vec::new())
        };
        ctx.reply(
            sender,
            Component::translatable(key, args).color(NamedColor::Red),
        );
        apply_stores(ctx, sender, state, false, 0);
    }
}

fn walk<C: CommandContext>(
    ctx: &mut C,
    plan: &Plan<'_>,
    index: usize,
    state: &State,
    tally: &mut Tally,
) {
    let sender = plan.sender;
    let Some(step) = plan.chain.steps.get(index) else {
        if let Some(command) = &plan.chain.command {
            finish(ctx, plan, state, command, tally);
        }
        return;
    };
    let last = index + 1 == plan.chain.steps.len() && plan.chain.command.is_none();
    if let (Step::If(positive, condition), true) = (step, last) {
        finish_condition(ctx, plan, state, *positive, condition, tally);
        return;
    }
    let mut next: Vec<(State, Option<Identifier>)> = Vec::new();
    let mut failure: Option<Component> = None;
    ctx.run_with_source(&state.source, |ctx| {
        match advance(ctx, sender, step, state) {
            Ok(found) => next = found,
            Err(error) => failure = Some(error),
        }
    });
    if let Some(error) = failure {
        ctx.reply(sender, error.color(NamedColor::Red));
        return;
    }
    let fork_limit = ctx
        .game_rule_text("max_command_forks")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(65_536);
    if next.len() > fork_limit {
        next.truncate(fork_limit);
        ctx.reply(
            sender,
            Component::translatable(
                "command.forkLimit",
                vec![Component::text(fork_limit.to_string())],
            )
            .color(NamedColor::Red),
        );
    }
    for (child, dimension) in next {
        let target = dimension.clone();
        let entered = enter(ctx, target, plan.swap, |ctx| {
            walk(ctx, plan, index + 1, &child, tally);
        });
        if !entered {
            let name = dimension.map(|d| d.to_string()).unwrap_or_default();
            unknown_dimension(ctx, sender, &name);
        }
    }
}

fn advance<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    step: &Step,
    state: &State,
) -> Result<Vec<(State, Option<Identifier>)>, Component> {
    let with = |source: Source| {
        (
            State {
                source,
                stores: state.stores.clone(),
            },
            None,
        )
    };
    let current = &state.source;
    match step {
        Step::As(sel) => Ok(ctx
            .select_entities(sender, sel)?
            .into_iter()
            .map(|target| {
                with(Source {
                    executor: Some(target),
                    ..current.clone()
                })
            })
            .collect()),
        Step::At(sel) => Ok(ctx
            .select_entities(sender, sel)?
            .into_iter()
            .filter_map(|target| {
                let position = ctx.target_position(target)?;
                let rotation = ctx.target_rotation(target).unwrap_or(current.rotation);
                let dimension = ctx.target_dimension(target);
                let source = Source {
                    position,
                    rotation,
                    dimension: dimension.clone(),
                    ..current.clone()
                };
                Some((with(source).0, dimension))
            })
            .collect()),
        Step::Positioned(raw) => {
            let refs = [raw[0].as_str(), raw[1].as_str(), raw[2].as_str()];
            let position = parse_vec3(ctx, sender, refs)?;
            Ok(vec![with(Source {
                position,
                ..current.clone()
            })])
        }
        Step::PositionedAs(sel) => Ok(ctx
            .select_entities(sender, sel)?
            .into_iter()
            .filter_map(|target| {
                let position = ctx.target_position(target)?;
                Some(with(Source {
                    position,
                    ..current.clone()
                }))
            })
            .collect()),
        Step::PositionedOver(kind) => {
            let (x, z) = (floor(current.position.x), floor(current.position.z));
            let y = ctx
                .surface_height(*kind, x, z)
                .ok_or_else(|| Component::translatable("argument.pos.unloaded", Vec::new()))?;
            Ok(vec![with(Source {
                position: Vec3::new(current.position.x, f64::from(y), current.position.z),
                ..current.clone()
            })])
        }
        Step::Rotated([yaw, pitch]) => {
            let rotation = Rotation::new(
                parse_angle(yaw, current.rotation.yaw)?,
                parse_angle(pitch, current.rotation.pitch)?,
            );
            Ok(vec![with(Source {
                rotation,
                ..current.clone()
            })])
        }
        Step::RotatedAs(sel) => Ok(ctx
            .select_entities(sender, sel)?
            .into_iter()
            .filter_map(|target| {
                let rotation = ctx.target_rotation(target)?;
                Some(with(Source {
                    rotation,
                    ..current.clone()
                }))
            })
            .collect()),
        Step::FacingPos(raw) => {
            let refs = [raw[0].as_str(), raw[1].as_str(), raw[2].as_str()];
            let to = parse_vec3(ctx, sender, refs)?;
            let rotation = look_at(anchored_position(ctx, current), to);
            Ok(vec![with(Source {
                rotation,
                ..current.clone()
            })])
        }
        Step::FacingEntity(sel, anchor) => Ok(ctx
            .select_entities(sender, sel)?
            .into_iter()
            .filter_map(|target| {
                let mut to = ctx.target_position(target)?;
                if *anchor == Anchor::Eyes {
                    to.y += ctx.target_eye_height(target);
                }
                let rotation = look_at(anchored_position(ctx, current), to);
                Some(with(Source {
                    rotation,
                    ..current.clone()
                }))
            })
            .collect()),
        Step::Align(axes) => {
            let p = current.position;
            let position = Vec3::new(
                if axes[0] { p.x.floor() } else { p.x },
                if axes[1] { p.y.floor() } else { p.y },
                if axes[2] { p.z.floor() } else { p.z },
            );
            Ok(vec![with(Source {
                position,
                ..current.clone()
            })])
        }
        Step::Anchored(anchor) => Ok(vec![with(Source {
            anchor: *anchor,
            ..current.clone()
        })]),
        Step::In(dimension) => Ok(vec![(
            State {
                source: Source {
                    dimension: Some(dimension.clone()),
                    ..current.clone()
                },
                stores: state.stores.clone(),
            },
            Some(dimension.clone()),
        )]),
        Step::On(relation) => Ok(current
            .executor
            .map(|executor| ctx.related_targets(executor, *relation))
            .unwrap_or_default()
            .into_iter()
            .map(|target| {
                with(Source {
                    executor: Some(target),
                    ..current.clone()
                })
            })
            .collect()),
        Step::Summon(kind) => {
            let spawned = ctx.summon(sender, *kind, current.position, None)?;
            Ok(spawned
                .map(|id| {
                    with(Source {
                        executor: Some(Target::Entity(id)),
                        ..current.clone()
                    })
                })
                .into_iter()
                .collect())
        }
        Step::If(positive, condition) => {
            let count = test(ctx, sender, condition)?;
            Ok(if (count > 0) == *positive {
                vec![with(current.clone())]
            } else {
                Vec::new()
            })
        }
        Step::Store(result, sink) => {
            let mut stores = state.stores.clone();
            stores.push((*result, sink.clone()));
            Ok(vec![(
                State {
                    source: current.clone(),
                    stores,
                },
                None,
            )])
        }
    }
}

fn swaps(command: Option<&str>) -> bool {
    let head = command
        .and_then(|c| c.trim_start_matches('/').split_whitespace().next())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let head = head.strip_prefix("minecraft:").unwrap_or(&head);
    !matches!(head, "tp" | "teleport")
}

fn check_objectives<C: CommandContext>(ctx: &C, chain: &Chain) -> Result<(), Component> {
    let known = |name: &str| ctx.scoreboard().objective(name).is_some();
    for step in &chain.steps {
        match step {
            Step::If(
                _,
                Cond::Score {
                    objective, test, ..
                },
            ) => {
                if !known(objective) {
                    return Err(objective_missing(objective));
                }
                if let ScoreTest::Compare {
                    source_objective, ..
                } = test
                    && !known(source_objective)
                {
                    return Err(objective_missing(source_objective));
                }
            }
            Step::Store(_, Sink::Score(_, objective)) => {
                match ctx.scoreboard().objective(objective) {
                    None => return Err(objective_missing(objective)),
                    Some(found) if found.criteria.read_only() => {
                        return Err(Component::translatable(
                            "arguments.objective.readonly",
                            vec![Component::text(objective.clone())],
                        ));
                    }
                    Some(_) => {}
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let chain = match parse_chain(rest) {
        Ok(chain) => chain,
        Err(Problem::Unknown) => {
            ctx.reply(
                sender,
                Component::translatable("command.unknown.command", Vec::new())
                    .color(NamedColor::Red),
            );
            ctx.reply(
                sender,
                Component::text("")
                    .color(NamedColor::Gray)
                    .append(Component::text(format!("execute {rest}")).color(NamedColor::Red))
                    .append(
                        Component::translatable("command.context.here", Vec::new())
                            .color(NamedColor::Red)
                            .italic(true),
                    ),
            );
            return CommandOutcome::Failure;
        }
        Err(Problem::Message(error)) => {
            ctx.reply(sender, error.color(NamedColor::Red));
            return CommandOutcome::Failure;
        }
    };
    if let Err(error) = check_objectives(ctx, &chain) {
        ctx.reply(sender, error.color(NamedColor::Red));
        return CommandOutcome::Failure;
    }
    let plan = Plan {
        swap: swaps(chain.command.as_deref()),
        chain: &chain,
        sender,
    };
    let state = State {
        source: ctx.command_source(sender),
        stores: Vec::new(),
    };
    let mut tally = Tally::default();
    walk(ctx, &plan, 0, &state, &mut tally);
    ctx.record_result(i32::try_from(tally.total).unwrap_or(i32::MAX));
    if tally.successes > 0 {
        CommandOutcome::Success
    } else {
        CommandOutcome::Failure
    }
}
