use rspigot_domain::Identifier;
use rspigot_domain::text::Component;
use rspigot_nbt::{NbtPath, Tag};
use serde_json::Value;

use super::dispatch::{
    CommandContext, CommandOutcome, CommandSender, fail, next_argument, select_one, usage,
};
use super::source::DataTarget;

const MAX_DEPTH: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Integer,
    Float,
}

struct Environment {
    this: Option<String>,
}

fn text(value: impl ToString) -> Component {
    Component::text(value.to_string())
}

pub(super) fn tag_to_json(tag: &Tag) -> Value {
    match tag {
        Tag::Byte(v) => Value::from(*v),
        Tag::Short(v) => Value::from(*v),
        Tag::Int(v) => Value::from(*v),
        Tag::Long(v) => Value::from(*v),
        Tag::Float(v) => Value::from(*v),
        Tag::Double(v) => Value::from(*v),
        Tag::String(v) => Value::String(v.clone()),
        Tag::List(list) => Value::Array(list.iter().map(tag_to_json).collect()),
        Tag::Compound(compound) => Value::Object(
            compound
                .iter()
                .map(|(key, value)| (key.to_owned(), tag_to_json(value)))
                .collect(),
        ),
        _ => Value::Null,
    }
}

fn tag_number(tag: &Tag) -> Option<f64> {
    match tag {
        Tag::Byte(v) => Some(f64::from(*v)),
        Tag::Short(v) => Some(f64::from(*v)),
        Tag::Int(v) => Some(f64::from(*v)),
        Tag::Long(v) => Some(*v as f64),
        Tag::Float(v) => Some(f64::from(*v)),
        Tag::Double(v) => Some(*v),
        _ => None,
    }
}

fn parse_value(spec: &str) -> Option<Value> {
    serde_json::from_str::<Value>(spec).ok().or_else(|| {
        rspigot_nbt::parse_tag(spec)
            .ok()
            .map(|tag| tag_to_json(&tag))
    })
}

fn number_of(value: &Value) -> Option<f64> {
    value.as_f64()
}

struct Evaluator<'a, C: CommandContext> {
    ctx: &'a mut C,
    mode: Mode,
    environment: Environment,
}

impl<C: CommandContext> Evaluator<'_, C> {
    fn child(&mut self, value: Option<&Value>, depth: usize) -> Result<f64, String> {
        self.eval(value.ok_or("a provider is missing an input")?, depth + 1)
    }

    fn unit(&mut self) -> f64 {
        f64::from(self.ctx.random_between(0, 1 << 24)) / f64::from(1 << 24)
    }

    fn eval(&mut self, value: &Value, depth: usize) -> Result<f64, String> {
        if depth > MAX_DEPTH {
            return Err("number provider is nested too deeply".to_owned());
        }
        if let Some(number) = number_of(value) {
            return Ok(number);
        }
        let Some(object) = value.as_object() else {
            return Err(format!("expected a number provider, found {value}"));
        };
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .map(|k| k.strip_prefix("minecraft:").unwrap_or(k))
            .ok_or("a number provider needs a type")?;
        let field = |name: &str| object.get(name);
        let binary = |this: &mut Self| -> Result<(f64, f64), String> {
            if let Some(Value::Array(inputs)) = field("inputs")
                && let [a, b] = inputs.as_slice()
            {
                return Ok((this.eval(a, depth + 1)?, this.eval(b, depth + 1)?));
            }
            Ok((
                this.child(field("left"), depth)?,
                this.child(field("right"), depth)?,
            ))
        };
        let result = match kind {
            "constant" => number_of(field("value").ok_or("constant needs a value")?)
                .ok_or("constant needs a number")?,
            "uniform" => {
                let min = self.child(field("min"), depth)?;
                let max = self.child(field("max"), depth)?;
                if self.mode == Mode::Integer {
                    f64::from(self.ctx.random_between(min as i32, max as i32))
                } else {
                    min + (max - min) * self.unit()
                }
            }
            "binomial" => {
                let n = self.child(field("n"), depth)? as i32;
                let p = self.child(field("p"), depth)?;
                let mut hits = 0;
                for _ in 0..n.max(0) {
                    if self.unit() < p {
                        hits += 1;
                    }
                }
                f64::from(hits)
            }
            "add" | "sum" => match field("inputs") {
                Some(Value::Array(inputs)) => {
                    let mut total = 0.0;
                    for input in inputs {
                        total += self.eval(input, depth + 1)?;
                    }
                    total
                }
                _ => {
                    let (a, b) = binary(self)?;
                    a + b
                }
            },
            "avg" => {
                let Some(Value::Array(inputs)) = field("inputs") else {
                    return Err("avg needs inputs".to_owned());
                };
                let mut total = 0.0;
                for input in inputs {
                    total += self.eval(input, depth + 1)?;
                }
                total / inputs.len().max(1) as f64
            }
            "sub" => {
                let (a, b) = binary(self)?;
                a - b
            }
            "mul" => {
                let (a, b) = binary(self)?;
                a * b
            }
            "div" => {
                let (a, b) = binary(self)?;
                if self.mode == Mode::Integer {
                    (a / b).trunc()
                } else {
                    a / b
                }
            }
            "floor_div" => {
                let (a, b) = binary(self)?;
                (a / b).floor()
            }
            "mod" => {
                let (a, b) = binary(self)?;
                a % b
            }
            "floor_mod" => {
                let (a, b) = binary(self)?;
                a - b * (a / b).floor()
            }
            "min" => {
                let (a, b) = binary(self)?;
                a.min(b)
            }
            "max" => {
                let (a, b) = binary(self)?;
                a.max(b)
            }
            "pow" => {
                let base = self.child(field("base"), depth)?;
                let exponent = self.child(field("exponent"), depth)?;
                base.powf(exponent)
            }
            "abs" | "negate" | "sqrt" | "sin" | "cos" | "floor" | "ceil" | "round" | "truncate"
            | "length" | "from_int" | "from_float" => {
                let input = self.child(field("input"), depth)?;
                match kind {
                    "abs" => input.abs(),
                    "negate" => -input,
                    "sqrt" => input.sqrt(),
                    "sin" => input.sin(),
                    "cos" => input.cos(),
                    "floor" | "from_float" => input.floor(),
                    "ceil" => input.ceil(),
                    "round" => input.round(),
                    "truncate" => input.trunc(),
                    "length" => input.abs(),
                    _ => input,
                }
            }
            "weighted_list" => {
                let Some(Value::Array(entries)) = field("distribution") else {
                    return Err("weighted_list needs a distribution".to_owned());
                };
                let total: i64 = entries
                    .iter()
                    .map(|entry| entry.get("weight").and_then(Value::as_i64).unwrap_or(1))
                    .sum();
                if total <= 0 {
                    return Err("weighted_list has no weight".to_owned());
                }
                let mut roll = i64::from(self.ctx.random_between(0, (total - 1) as i32));
                let mut chosen = None;
                for entry in entries {
                    roll -= entry.get("weight").and_then(Value::as_i64).unwrap_or(1);
                    if roll < 0 {
                        chosen = entry.get("data");
                        break;
                    }
                }
                self.child(chosen, depth)?
            }
            "score" => {
                let target = field("target").ok_or("score needs a target")?;
                let holder = match target.get("type").and_then(Value::as_str) {
                    Some("minecraft:fixed" | "fixed") => target
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    Some("minecraft:context" | "context") => self.environment.this.clone(),
                    _ => None,
                }
                .ok_or("the score target cannot be resolved here")?;
                let objective = field("score")
                    .and_then(Value::as_str)
                    .ok_or("score needs an objective")?;
                let score = self
                    .ctx
                    .scoreboard()
                    .score(&holder, objective)
                    .map_or(0, |score| score.value);
                f64::from(score)
            }
            "storage" => {
                let storage = field("storage")
                    .and_then(Value::as_str)
                    .and_then(|id| Identifier::parse(id).ok())
                    .ok_or("storage needs an id")?;
                let path = field("path")
                    .and_then(Value::as_str)
                    .ok_or("storage needs a path")?;
                let path = NbtPath::parse(path).map_err(|_| "invalid storage path".to_owned())?;
                let data = self
                    .ctx
                    .read_data(&DataTarget::Storage(storage))
                    .map_err(|_| "the storage is unreadable".to_owned())?;
                let root = Tag::Compound(data);
                path.get(&root)
                    .first()
                    .and_then(|tag| tag_number(tag))
                    .ok_or("the storage path holds no number")?
            }
            other => return Err(format!("unsupported number provider `{other}`")),
        };
        Ok(result)
    }
}

fn split_scale(rest: &str, mode: Mode) -> (String, f64) {
    let rest = rest.trim();
    if mode == Mode::Float
        && parse_value(rest).is_none()
        && let Some((head, last)) = rest.rsplit_once(char::is_whitespace)
        && let Ok(scale) = last.parse::<f32>()
    {
        return (head.trim().to_owned(), f64::from(scale));
    }
    if mode == Mode::Float
        && parse_value(rest).is_some()
        && let Some((head, last)) = rest.rsplit_once(char::is_whitespace)
        && let Ok(scale) = last.parse::<f32>()
        && parse_value(head.trim()).is_some()
        && !head.trim().is_empty()
    {
        return (head.trim().to_owned(), f64::from(scale));
    }
    (rest.to_owned(), 1.0)
}

pub(super) fn run<C: CommandContext>(
    ctx: &mut C,
    sender: CommandSender,
    rest: &str,
) -> CommandOutcome {
    let help =
        "/compute (default|block <pos>|entity <target>) (integer|float) <provider> [<scale>]";
    let Some((context, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let (this, rest) = match context {
        "default" => (None, rest),
        "block" => {
            let mut tail = rest;
            for _ in 0..3 {
                let Some((_, after)) = next_argument(tail) else {
                    return fail(ctx, sender, "argument.pos3d.incomplete", Vec::new());
                };
                tail = after;
            }
            (None, tail)
        }
        "entity" => {
            let Some((selector, after)) = next_argument(rest) else {
                return usage(ctx, sender, help);
            };
            let target = match select_one(ctx, sender, selector) {
                Ok(target) => target,
                Err(outcome) => return outcome,
            };
            (ctx.score_holder_name(target), after)
        }
        _ => return usage(ctx, sender, help),
    };
    let Some((kind, rest)) = next_argument(rest) else {
        return usage(ctx, sender, help);
    };
    let mode = match kind {
        "integer" => Mode::Integer,
        "float" => Mode::Float,
        _ => return usage(ctx, sender, help),
    };
    let (spec, scale) = split_scale(rest, mode);
    if spec.is_empty() {
        return usage(ctx, sender, help);
    }
    let Some(value) = parse_value(&spec) else {
        return fail(ctx, sender, "argument.resource_or_id.invalid", Vec::new());
    };
    let mut evaluator = Evaluator {
        ctx,
        mode,
        environment: Environment { this },
    };
    let raw = match evaluator.eval(&value, 0) {
        Ok(raw) => raw,
        Err(reason) => {
            return fail(
                ctx,
                sender,
                "command.compute.result.unnamed.invalid",
                vec![text(reason)],
            );
        }
    };
    if !raw.is_finite() {
        return fail(
            ctx,
            sender,
            "command.compute.result.unnamed.invalid",
            vec![text(raw)],
        );
    }
    let result = if mode == Mode::Integer {
        raw as i32
    } else {
        (raw * scale).floor() as i32
    };
    let message = if mode == Mode::Float && f64::from(result) != raw {
        Component::translatable(
            "command.compute.result.unnamed.rounded",
            vec![text(raw), text(result)],
        )
    } else {
        Component::translatable("command.compute.result.unnamed.exact", vec![text(result)])
    };
    ctx.reply(sender, message);
    ctx.record_result(result);
    CommandOutcome::Success
}
