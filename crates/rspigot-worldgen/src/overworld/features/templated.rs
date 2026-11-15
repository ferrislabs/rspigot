use serde_json::Value;

use super::place::Run;
use super::prov::Pos;
use super::structure::{Processor, Rotation, Settings, Template, place_in_world};

#[derive(Debug, Clone)]
struct Entry {
    template: &'static Template,
    rotations: Vec<Rotation>,
    weight: i32,
}

#[derive(Debug, Clone)]
pub(crate) struct Templated {
    entries: Vec<Entry>,
    processors: Vec<Processor>,
}

fn entry(value: &Value) -> Option<Entry> {
    let data = value.get("data")?;
    let rotations = match data.get("rotations").and_then(Value::as_array) {
        Some(names) => names
            .iter()
            .map(|name| Rotation::parse(name.as_str()?))
            .collect::<Option<Vec<_>>>()?,
        None => Rotation::ALL.to_vec(),
    };
    Some(Entry {
        template: Template::named(data.get("id")?.as_str()?)?,
        rotations,
        weight: i32::try_from(value.get("weight")?.as_i64()?).ok()?,
    })
}

impl Templated {
    pub(crate) fn compile(value: &Value) -> Option<Self> {
        let entries: Vec<Entry> = value
            .get("templates")?
            .as_array()?
            .iter()
            .map(entry)
            .collect::<Option<_>>()?;
        if entries.is_empty()
            || entries
                .iter()
                .any(|e| e.weight <= 0 || e.rotations.is_empty())
        {
            return None;
        }
        let processors = match value.get("processors") {
            Some(list) => Processor::compile_list(list)?,
            None => Vec::new(),
        };
        Some(Self {
            entries,
            processors,
        })
    }

    pub(crate) fn reach(&self) -> i32 {
        self.entries
            .iter()
            .map(|entry| {
                let size = entry.template.size(Rotation::None);
                size.0.max(size.2)
            })
            .max()
            .unwrap_or(1)
    }

    fn pick(&self, run: &mut Run) -> &Entry {
        let total: i32 = self.entries.iter().map(|entry| entry.weight).sum();
        let mut roll = run.rng.below(total);
        for entry in &self.entries {
            roll -= entry.weight;
            if roll < 0 {
                return entry;
            }
        }
        &self.entries[0]
    }

    pub(crate) fn place(&self, run: &mut Run, origin: Pos) -> bool {
        let entry = self.pick(run);
        let rotation = entry.rotations[run.rng.below(entry.rotations.len() as i32) as usize];
        let size = entry.template.size(Rotation::None);
        let (west_x, west_z) = rotation.vector((-1, 0));
        let (north_x, north_z) = rotation.vector((0, -1));
        let (half_x, half_z) = (size.0 / 2, size.2 / 2);
        let at = (
            origin.0 + west_x * half_x + north_x * half_z,
            origin.1,
            origin.2 + west_z * half_x + north_z * half_z,
        );
        let settings = Settings {
            rotation,
            bounds: None,
            processors: &self.processors,
        };
        place_in_world(entry.template, run, at, &settings)
    }
}

#[cfg(test)]
mod tests;
