use rspigot_nbt::{Compound, Tag};

use crate::random::RandomSource;

pub const ENTITY: &str = "mob_spawner";
pub const BLOCK_EVENT: u8 = 1;
pub const SPAWN_EFFECT_EVENT: i32 = 2004;
pub const DEFAULT_DELAY: i32 = 20;
pub const DEFAULT_MIN_DELAY: i32 = 200;
pub const DEFAULT_MAX_DELAY: i32 = 800;
pub const DEFAULT_COUNT: i32 = 4;
pub const DEFAULT_MAX_NEARBY: i32 = 6;
pub const DEFAULT_PLAYER_RANGE: i32 = 16;
pub const DEFAULT_SPAWN_RANGE: i32 = 4;
const FULL_LIGHT: (i32, i32) = (0, 15);

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpawnData {
    pub entity: Compound,
    pub rules: Option<Compound>,
    pub equipment: Option<Compound>,
}

impl SpawnData {
    pub fn of_entity(id: &str) -> Self {
        Self {
            entity: Compound::new().with("id", id),
            ..Self::default()
        }
    }

    pub fn entity_id(&self) -> Option<&str> {
        self.entity.get_str("id")
    }

    pub fn only_names_the_entity(&self) -> bool {
        self.entity.len() == 1 && self.entity_id().is_some()
    }

    pub fn light_limits(&self) -> Option<LightLimits> {
        let rules = self.rules.as_ref()?;
        Some(LightLimits {
            block: range_of(rules.get("block_light_limit")),
            sky: range_of(rules.get("sky_light_limit")),
        })
    }

    fn read(tag: &Compound) -> Self {
        let Some(entity) = tag.get_compound("entity") else {
            return Self {
                entity: tag.clone(),
                ..Self::default()
            };
        };
        Self {
            entity: entity.clone(),
            rules: tag.get_compound("custom_spawn_rules").cloned(),
            equipment: tag.get_compound("equipment").cloned(),
        }
    }

    fn write(&self) -> Compound {
        let mut tag = Compound::new().with("entity", self.entity.clone());
        if let Some(rules) = &self.rules {
            tag.insert("custom_spawn_rules", rules.clone());
        }
        if let Some(equipment) = &self.equipment {
            tag.insert("equipment", equipment.clone());
        }
        tag
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightLimits {
    pub block: (i32, i32),
    pub sky: (i32, i32),
}

impl LightLimits {
    pub const fn admits(self, block_light: i32, sky_light: i32) -> bool {
        block_light >= self.block.0
            && block_light <= self.block.1
            && sky_light >= self.sky.0
            && sky_light <= self.sky.1
    }
}

fn range_of(tag: Option<&Tag>) -> (i32, i32) {
    match tag {
        Some(Tag::List(items)) => match items.as_slice() {
            [min, max] => (
                min.as_i64().map_or(FULL_LIGHT.0, clamp_light),
                max.as_i64().map_or(FULL_LIGHT.1, clamp_light),
            ),
            _ => FULL_LIGHT,
        },
        Some(Tag::Compound(range)) => (
            range
                .get_i64("min_inclusive")
                .map_or(FULL_LIGHT.0, clamp_light),
            range
                .get_i64("max_inclusive")
                .map_or(FULL_LIGHT.1, clamp_light),
        ),
        _ => FULL_LIGHT,
    }
}

fn clamp_light(value: i64) -> i32 {
    i32::try_from(value.clamp(0, 15)).unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spawner {
    pub delay: i32,
    pub min_delay: i32,
    pub max_delay: i32,
    pub count: i32,
    pub max_nearby: i32,
    pub player_range: i32,
    pub range: i32,
    pub next: Option<SpawnData>,
    pub potentials: Vec<(SpawnData, i32)>,
}

impl Default for Spawner {
    fn default() -> Self {
        Self {
            delay: DEFAULT_DELAY,
            min_delay: DEFAULT_MIN_DELAY,
            max_delay: DEFAULT_MAX_DELAY,
            count: DEFAULT_COUNT,
            max_nearby: DEFAULT_MAX_NEARBY,
            player_range: DEFAULT_PLAYER_RANGE,
            range: DEFAULT_SPAWN_RANGE,
            next: None,
            potentials: Vec::new(),
        }
    }
}

impl Spawner {
    pub fn read(tag: &Compound) -> Self {
        let int = |key: &str, default: i32| tag.get_i32(key).unwrap_or(default);
        let next = tag.get_compound("SpawnData").map(SpawnData::read);
        let potentials = tag
            .get_list("SpawnPotentials")
            .unwrap_or_default()
            .iter()
            .filter_map(Tag::as_compound)
            .filter_map(|entry| {
                let data = entry.get_compound("data")?;
                Some((SpawnData::read(data), entry.get_i32("weight").unwrap_or(1)))
            })
            .collect();
        Self {
            delay: int("Delay", DEFAULT_DELAY),
            min_delay: int("MinSpawnDelay", DEFAULT_MIN_DELAY),
            max_delay: int("MaxSpawnDelay", DEFAULT_MAX_DELAY),
            count: int("SpawnCount", DEFAULT_COUNT),
            max_nearby: int("MaxNearbyEntities", DEFAULT_MAX_NEARBY),
            player_range: int("RequiredPlayerRange", DEFAULT_PLAYER_RANGE),
            range: int("SpawnRange", DEFAULT_SPAWN_RANGE),
            next,
            potentials,
        }
    }

    pub fn write(&self, tag: &mut Compound) {
        let short = |value: i32| Tag::Short(i16::try_from(value).unwrap_or(i16::MAX));
        tag.insert("Delay", short(self.delay));
        tag.insert("MinSpawnDelay", short(self.min_delay));
        tag.insert("MaxSpawnDelay", short(self.max_delay));
        tag.insert("SpawnCount", short(self.count));
        tag.insert("MaxNearbyEntities", short(self.max_nearby));
        tag.insert("RequiredPlayerRange", short(self.player_range));
        tag.insert("SpawnRange", short(self.range));
        match &self.next {
            Some(next) => tag.insert("SpawnData", next.write()),
            None => tag.remove("SpawnData"),
        };
        let potentials = self
            .potentials
            .iter()
            .map(|(data, weight)| {
                Tag::Compound(
                    Compound::new()
                        .with("data", data.write())
                        .with("weight", *weight),
                )
            })
            .collect();
        tag.insert("SpawnPotentials", Tag::List(potentials));
    }

    pub fn next_data(&mut self, random: &mut impl RandomSource) -> &SpawnData {
        if self.next.is_none() {
            let picked = self.pick(random).unwrap_or_default();
            self.next = Some(picked);
        }
        self.next.get_or_insert_with(SpawnData::default)
    }

    pub fn set_entity(&mut self, entity: &str) {
        self.next = Some(SpawnData::of_entity(entity));
        self.potentials.clear();
    }

    pub fn reset_delay(&mut self, random: &mut impl RandomSource) {
        self.delay = if self.min_delay < self.max_delay {
            self.min_delay + random.next_int(self.max_delay - self.min_delay)
        } else {
            self.min_delay
        };
        if let Some(picked) = self.pick(random) {
            self.next = Some(picked);
        }
    }

    pub fn spawned_id(&self) -> Option<&str> {
        self.next.as_ref().and_then(SpawnData::entity_id)
    }

    fn pick(&self, random: &mut impl RandomSource) -> Option<SpawnData> {
        let total: i32 = self.potentials.iter().map(|(_, weight)| *weight).sum();
        if total <= 0 {
            return None;
        }
        let mut roll = random.next_int(total);
        for (data, weight) in &self.potentials {
            roll -= weight;
            if roll < 0 {
                return Some(data.clone());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(i32);

    impl RandomSource for Fixed {
        fn next_float(&mut self) -> f32 {
            0.0
        }
        fn next_int(&mut self, bound: i32) -> i32 {
            self.0.min(bound - 1)
        }
        fn next_double(&mut self) -> f64 {
            0.0
        }
        fn next_boolean(&mut self) -> bool {
            false
        }
        fn next_gaussian(&mut self) -> f64 {
            0.0
        }
    }

    #[test]
    fn an_empty_tag_reads_as_the_vanilla_defaults() {
        assert_eq!(Spawner::read(&Compound::new()), Spawner::default());
    }

    #[test]
    fn a_spawner_survives_a_write_and_a_read() {
        let mut spawner = Spawner {
            delay: 7,
            count: 9,
            ..Spawner::default()
        };
        spawner.next = Some(SpawnData::of_entity("minecraft:zombie"));
        spawner.potentials = vec![(SpawnData::of_entity("minecraft:husk"), 3)];
        let mut tag = Compound::new();
        spawner.write(&mut tag);
        assert_eq!(Spawner::read(&tag), spawner);
    }

    #[test]
    fn the_delay_is_drawn_between_the_bounds_and_picks_the_next_mob() {
        let mut spawner = Spawner {
            min_delay: 100,
            max_delay: 300,
            potentials: vec![
                (SpawnData::of_entity("minecraft:cow"), 1),
                (SpawnData::of_entity("minecraft:pig"), 1),
            ],
            ..Spawner::default()
        };
        spawner.reset_delay(&mut Fixed(150));
        assert_eq!(spawner.delay, 100 + 150);
        assert_eq!(spawner.spawned_id(), Some("minecraft:pig"));
    }

    #[test]
    fn setting_the_entity_replaces_the_potentials() {
        let mut spawner = Spawner {
            potentials: vec![(SpawnData::of_entity("minecraft:cow"), 1)],
            ..Spawner::default()
        };
        spawner.set_entity("minecraft:bat");
        assert!(spawner.potentials.is_empty());
        assert_eq!(spawner.spawned_id(), Some("minecraft:bat"));
    }

    #[test]
    fn light_limits_read_both_range_encodings() {
        let rules = Compound::new()
            .with(
                "block_light_limit",
                Tag::List(vec![Tag::Int(0), Tag::Int(7)]),
            )
            .with(
                "sky_light_limit",
                Compound::new()
                    .with("min_inclusive", 4)
                    .with("max_inclusive", 9),
            );
        let data = SpawnData {
            rules: Some(rules),
            ..SpawnData::of_entity("minecraft:zombie")
        };
        let limits = data.light_limits().expect("custom rules");
        assert!(limits.admits(7, 4));
        assert!(!limits.admits(8, 4));
        assert!(!limits.admits(0, 10));
    }
}
