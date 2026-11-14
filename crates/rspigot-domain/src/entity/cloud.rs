use rspigot_entities::EntityDimensions;
use rspigot_nbt::{Compound, Tag};
use uuid::Uuid;

use super::base::EntityBase;
use super::data::{ParticleSetting, SyncedValue};
use super::save::{read_uuid, uuid_tag};
use crate::EntityId;
use crate::effect::EffectInstance;
use crate::potion::{self, PotionContents};

pub const DATA_RADIUS: u8 = 8;
pub const DATA_WAITING: u8 = 9;
pub const DATA_PARTICLE: u8 = 10;
pub const KEYS: &[&str] = &[
    "Age",
    "Duration",
    "WaitTime",
    "ReapplicationDelay",
    "DurationOnUse",
    "RadiusOnUse",
    "RadiusPerTick",
    "Radius",
    "custom_particle",
    "Owner",
    "potion_contents",
    "potion_duration_scale",
];
pub const DEFAULT_RADIUS: f32 = 3.0;
pub const MAX_RADIUS: f32 = 32.0;
pub const MIN_RADIUS: f32 = 0.5;
pub const HEIGHT: f32 = 0.5;
pub const EYE_FACTOR: f32 = 0.85;
pub const APPLY_INTERVAL: i32 = 5;
pub const INSTANT_SCALE: f64 = 0.5;
pub const ENTITY_EFFECT: &str = "minecraft:entity_effect";
pub const DRAGON_BREATH: &str = "minecraft:dragon_breath";
pub const OPAQUE: i32 = 0xFF00_0000_u32 as i32;
const DEFAULT_WAIT: i32 = 20;

#[derive(Debug, Clone, PartialEq)]
pub struct CloudState {
    pub duration: i32,
    pub wait_time: i32,
    pub reapplication_delay: i32,
    pub duration_on_use: i32,
    pub radius_on_use: f32,
    pub radius_per_tick: f32,
    pub owner: Option<Uuid>,
    pub contents: Compound,
    pub custom_particle: Option<Tag>,
    pub potion_duration_scale: f32,
    pub victims: Vec<(EntityId, i32)>,
}

impl Default for CloudState {
    fn default() -> Self {
        Self {
            duration: -1,
            wait_time: DEFAULT_WAIT,
            reapplication_delay: DEFAULT_WAIT,
            duration_on_use: 0,
            radius_on_use: 0.0,
            radius_per_tick: 0.0,
            owner: None,
            contents: Compound::new(),
            custom_particle: None,
            potion_duration_scale: 1.0,
            victims: Vec::new(),
        }
    }
}

pub fn default_particle() -> ParticleSetting {
    ParticleSetting::Color {
        name: ENTITY_EFFECT.to_owned(),
        argb: -1,
    }
}

pub fn define(base: &mut EntityBase) -> CloudState {
    base.data
        .define(DATA_RADIUS, SyncedValue::Float(DEFAULT_RADIUS));
    base.data.define(DATA_WAITING, SyncedValue::Boolean(false));
    base.data
        .define(DATA_PARTICLE, SyncedValue::Particle(default_particle()));
    base.dimensions = dimensions(DEFAULT_RADIUS);
    CloudState::default()
}

fn dimensions(radius: f32) -> EntityDimensions {
    EntityDimensions {
        width: radius * 2.0,
        height: HEIGHT,
        eye_height: HEIGHT * EYE_FACTOR,
    }
}

pub fn radius(base: &EntityBase) -> f32 {
    match base.data.get(DATA_RADIUS) {
        Some(SyncedValue::Float(radius)) => *radius,
        _ => DEFAULT_RADIUS,
    }
}

pub fn set_radius(base: &mut EntityBase, radius: f32) {
    let radius = radius.clamp(0.0, MAX_RADIUS);
    base.data.set(DATA_RADIUS, SyncedValue::Float(radius));
    base.dimensions = dimensions(radius);
}

pub fn is_waiting(base: &EntityBase) -> bool {
    base.data.boolean(DATA_WAITING)
}

pub fn set_waiting(base: &mut EntityBase, waiting: bool) {
    base.data.set(DATA_WAITING, SyncedValue::Boolean(waiting));
}

pub fn particle(base: &EntityBase) -> ParticleSetting {
    match base.data.get(DATA_PARTICLE) {
        Some(SyncedValue::Particle(particle)) => particle.clone(),
        _ => default_particle(),
    }
}

pub fn parse_particle(tag: &Tag) -> Option<ParticleSetting> {
    let (name, compound) = match tag {
        Tag::String(name) => (name.as_str(), None),
        Tag::Compound(compound) => (compound.get_str("type")?, Some(compound)),
        _ => return None,
    };
    let name = if name.contains(':') {
        name.to_owned()
    } else {
        format!("minecraft:{name}")
    };
    let compound = compound.filter(|compound| compound.contains_key("power"));
    Some(match compound {
        Some(compound) => ParticleSetting::Power {
            power: compound.get_f64("power").unwrap_or(1.0) as f32,
            name,
        },
        None if name == DRAGON_BREATH => ParticleSetting::Power { name, power: 1.0 },
        None if name == ENTITY_EFFECT => ParticleSetting::Color { name, argb: -1 },
        None => ParticleSetting::Named(name),
    })
}

pub fn particle_tag(particle: &ParticleSetting) -> Tag {
    match particle {
        ParticleSetting::Named(name) => Tag::String(name.clone()),
        ParticleSetting::Power { name, power } => Tag::Compound(
            Compound::new()
                .with("type", name.as_str())
                .with("power", *power),
        ),
        ParticleSetting::Color { name, argb } => Tag::Compound(
            Compound::new()
                .with("type", name.as_str())
                .with("color", *argb),
        ),
    }
}

impl CloudState {
    pub fn effects(&self) -> Vec<EffectInstance> {
        PotionContents::of_compound(&self.contents).effects()
    }

    pub fn scaled_effects(&self) -> Vec<EffectInstance> {
        PotionContents::of_compound(&self.contents).scaled_effects(self.potion_duration_scale)
    }

    pub fn has_effects(&self) -> bool {
        !self.effects().is_empty()
    }

    pub fn color(&self) -> i32 {
        PotionContents::of_compound(&self.contents).color() | OPAQUE
    }

    pub fn add_effect(&mut self, effect: &EffectInstance) {
        let mut list: Vec<Tag> = self
            .contents
            .get_list("custom_effects")
            .map(<[Tag]>::to_vec)
            .unwrap_or_default();
        list.push(Tag::Compound(effect_to_nbt(effect)));
        self.contents.insert("custom_effects", Tag::List(list));
    }

    pub fn refresh_particle(&self, base: &mut EntityBase) {
        let particle = match &self.custom_particle {
            Some(tag) => parse_particle(tag).unwrap_or_else(default_particle),
            None => ParticleSetting::Color {
                name: ENTITY_EFFECT.to_owned(),
                argb: self.color(),
            },
        };
        base.data
            .set(DATA_PARTICLE, SyncedValue::Particle(particle));
    }
}

pub fn effect_to_nbt(effect: &EffectInstance) -> Compound {
    let duration = match effect.duration {
        crate::effect::EffectDuration::Ticks(ticks) => ticks as i32,
        crate::effect::EffectDuration::Infinite => -1,
    };
    Compound::new()
        .with("id", effect.effect.name().unwrap_or_default())
        .with("amplifier", effect.amplifier as i8)
        .with("duration", duration)
        .with("ambient", effect.ambient)
        .with("show_particles", effect.visible)
        .with("show_icon", effect.show_icon)
}

pub fn read(nbt: &Compound, base: &mut EntityBase, state: &mut CloudState) {
    base.tick_count = nbt.get_i32("Age").unwrap_or(0);
    state.duration = nbt.get_i32("Duration").unwrap_or(-1);
    state.wait_time = nbt.get_i32("WaitTime").unwrap_or(DEFAULT_WAIT);
    state.reapplication_delay = nbt.get_i32("ReapplicationDelay").unwrap_or(DEFAULT_WAIT);
    state.duration_on_use = nbt.get_i32("DurationOnUse").unwrap_or(0);
    state.radius_on_use = nbt.get_f64("RadiusOnUse").unwrap_or(0.0) as f32;
    state.radius_per_tick = nbt.get_f64("RadiusPerTick").unwrap_or(0.0) as f32;
    set_radius(
        base,
        nbt.get_f64("Radius")
            .map_or(DEFAULT_RADIUS, |value| value as f32),
    );
    state.owner = nbt.get("Owner").and_then(read_uuid);
    state.custom_particle = nbt.get("custom_particle").cloned();
    state.contents = nbt
        .get_compound("potion_contents")
        .cloned()
        .unwrap_or_default();
    state.potion_duration_scale = nbt
        .get_f64("potion_duration_scale")
        .map_or(1.0, |v| v as f32);
    state.refresh_particle(base);
}

pub fn write(base: &EntityBase, state: &CloudState, out: &mut Compound) {
    out.insert("Age", base.tick_count);
    out.insert("Duration", state.duration);
    out.insert("WaitTime", state.wait_time);
    out.insert("ReapplicationDelay", state.reapplication_delay);
    out.insert("DurationOnUse", state.duration_on_use);
    out.insert("RadiusOnUse", state.radius_on_use);
    out.insert("RadiusPerTick", state.radius_per_tick);
    out.insert("Radius", radius(base));
    if let Some(tag) = &state.custom_particle {
        out.insert("custom_particle", tag.clone());
    }
    if let Some(owner) = state.owner {
        out.insert("Owner", uuid_tag(owner));
    }
    if !state.contents.is_empty() {
        out.insert("potion_contents", state.contents.clone());
    }
    if state.potion_duration_scale != 1.0 {
        out.insert("potion_duration_scale", state.potion_duration_scale);
    }
}

pub fn harm_contents() -> Compound {
    Compound::new().with("potion", potion::HARMING)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effect::{EffectDuration, EffectId};
    use crate::entity::save::{load_entity, save_entity};
    use crate::entity::{Entity, EntityState};

    fn cloud(nbt: Compound) -> Entity {
        load_entity(
            &nbt.with("id", "minecraft:area_effect_cloud"),
            EntityId(1),
            Uuid::from_u128(1),
        )
        .unwrap()
    }

    fn state(entity: &Entity) -> &CloudState {
        let EntityState::Cloud(state) = &entity.state else {
            panic!("not a cloud");
        };
        state
    }

    #[test]
    fn a_fresh_cloud_lasts_forever_waits_twenty_ticks_and_has_a_three_block_radius() {
        let entity = cloud(Compound::new());
        let state = state(&entity);
        assert_eq!(state.duration, -1);
        assert_eq!(state.wait_time, 20);
        assert_eq!(state.reapplication_delay, 20);
        assert_eq!(radius(&entity.base), 3.0);
        assert!(!is_waiting(&entity.base));
        assert_eq!(entity.base.dimensions.width, 6.0);
        assert_eq!(entity.base.dimensions.height, 0.5);
        assert_eq!(
            entity.base.data.get(10),
            Some(&SyncedValue::Particle(ParticleSetting::Color {
                name: ENTITY_EFFECT.to_owned(),
                argb: potion::DEFAULT_COLOR | OPAQUE,
            }))
        );
        let mut base = EntityBase::new(
            EntityId(2),
            Uuid::from_u128(2),
            rspigot_entities::EntityKind::AreaEffectCloud,
        );
        define(&mut base);
        assert_eq!(
            base.data.get(10),
            Some(&SyncedValue::Particle(default_particle()))
        );
    }

    #[test]
    fn the_radius_is_clamped_and_resizes_the_box() {
        let mut entity = cloud(Compound::new());
        set_radius(&mut entity.base, 50.0);
        assert_eq!(radius(&entity.base), 32.0);
        assert_eq!(entity.base.dimensions.width, 64.0);
        set_radius(&mut entity.base, -4.0);
        assert_eq!(radius(&entity.base), 0.0);
    }

    #[test]
    fn the_dragon_breath_cloud_layout_round_trips() {
        let mut nbt = Compound::new()
            .with("Age", 30)
            .with("Duration", 600)
            .with("WaitTime", 20)
            .with("RadiusPerTick", 0.006_666_667f32)
            .with("Radius", 3.0f32)
            .with("potion_duration_scale", 0.25f32)
            .with("Owner", uuid_tag(Uuid::from_u128(9)))
            .with(
                "custom_particle",
                Compound::new()
                    .with("type", DRAGON_BREATH)
                    .with("power", 1.0f32),
            );
        nbt.insert(
            "potion_contents",
            Compound::new().with(
                "custom_effects",
                Tag::List(vec![Tag::Compound(
                    Compound::new()
                        .with("id", "minecraft:instant_damage")
                        .with("amplifier", 1i8)
                        .with("duration", 1)
                        .with("ambient", false)
                        .with("show_particles", true)
                        .with("show_icon", true),
                )]),
            ),
        );
        let entity = cloud(nbt);
        let state = state(&entity);
        assert_eq!(entity.base.tick_count, 30);
        assert_eq!(state.owner, Some(Uuid::from_u128(9)));
        assert_eq!(state.potion_duration_scale, 0.25);
        let effects = state.effects();
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0].amplifier, 1);
        assert_eq!(effects[0].duration, EffectDuration::Ticks(1));
        assert_eq!(
            particle(&entity.base),
            ParticleSetting::Power {
                name: DRAGON_BREATH.to_owned(),
                power: 1.0
            }
        );
        let saved = save_entity(&entity, None).unwrap();
        assert_eq!(saved.get_i32("Age"), Some(30));
        assert_eq!(saved.get_i32("Duration"), Some(600));
        assert_eq!(saved.get_f64("potion_duration_scale"), Some(0.25));
        let again = load_entity(&saved, EntityId(1), Uuid::from_u128(1)).unwrap();
        assert_eq!(save_entity(&again, None), Some(saved));
    }

    #[test]
    fn an_empty_cloud_writes_no_potion_keys() {
        let saved = save_entity(&cloud(Compound::new()), None).unwrap();
        assert!(!saved.contains_key("potion_contents"));
        assert!(!saved.contains_key("potion_duration_scale"));
        assert!(!saved.contains_key("custom_particle"));
        assert_eq!(saved.get_i32("Duration"), Some(-1));
    }

    #[test]
    fn added_effects_are_listed_and_tint_the_default_particle() {
        let mut entity = cloud(Compound::new());
        let EntityState::Cloud(state) = &mut entity.state else {
            panic!();
        };
        let harm = EffectId::from_name("minecraft:instant_damage").unwrap();
        state.add_effect(&EffectInstance::new(
            harm,
            EffectDuration::Ticks(1),
            1,
            true,
        ));
        assert!(state.has_effects());
        state.refresh_particle(&mut entity.base);
        let ParticleSetting::Color { argb, .. } = particle(&entity.base) else {
            panic!();
        };
        assert_eq!(argb >> 24 & 0xFF, 0xFF);
    }

    #[test]
    fn particles_parse_from_strings_and_compounds() {
        assert_eq!(
            parse_particle(&Tag::String("minecraft:flame".to_owned())),
            Some(ParticleSetting::Named("minecraft:flame".to_owned()))
        );
        assert_eq!(
            parse_particle(&Tag::String("dragon_breath".to_owned())),
            Some(ParticleSetting::Power {
                name: DRAGON_BREATH.to_owned(),
                power: 1.0
            })
        );
        assert_eq!(parse_particle(&Tag::Int(3)), None);
    }
}
