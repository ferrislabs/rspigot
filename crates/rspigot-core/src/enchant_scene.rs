use rand::Rng;
use rspigot_data::enchantments::{EntityCheck, Requirement};
use rspigot_data::loot::EntityTarget;
use rspigot_data::tags;

use crate::loot::{LootContext, condition_holds};

const BLOCK_REGISTRY: &str = "minecraft:block";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlayerFacts {
    pub gamemode: &'static str,
    pub food: i32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct EntityFacts {
    pub kind: Option<&'static str>,
    pub on_ground: bool,
    pub flying: bool,
    pub fall_flying: bool,
    pub in_water: bool,
    pub has_vehicle: bool,
    pub movement_block: Option<u32>,
    pub age: i64,
    pub horizontal_speed: f32,
    pub fall_distance: f32,
    pub player: Option<PlayerFacts>,
    pub sees_sky: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LocationFacts {
    pub block: Option<u32>,
    pub sees_sky: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct EffectScene<'a> {
    pub loot: LootContext<'a>,
    pub this: Option<&'a EntityFacts>,
    pub attacker: Option<&'a EntityFacts>,
    pub direct_attacker: Option<&'a EntityFacts>,
    pub location: Option<LocationFacts>,
    pub active: bool,
}

impl<'a> EffectScene<'a> {
    pub(crate) fn of(loot: &LootContext<'a>) -> Self {
        Self {
            loot: *loot,
            ..Self::default()
        }
    }

    fn facts(&self, target: EntityTarget) -> Option<&'a EntityFacts> {
        match target {
            EntityTarget::This => self.this,
            EntityTarget::Attacker => self.attacker,
            EntityTarget::DirectAttacker => self.direct_attacker,
            _ => None,
        }
    }
}

fn check_holds(check: &EntityCheck, facts: &EntityFacts) -> bool {
    match *check {
        EntityCheck::OnGround(expected) => facts.on_ground == expected,
        EntityCheck::Flying(expected) => facts.flying == expected,
        EntityCheck::FallFlying(expected) => facts.fall_flying == expected,
        EntityCheck::InWater(expected) => facts.in_water == expected,
        EntityCheck::Vehicle(expected) => facts.has_vehicle == expected,
        EntityCheck::MovementAffectedBy(tag) => facts.movement_block.is_some_and(|block| {
            tags::tag(BLOCK_REGISTRY, tag).is_some_and(|ids| ids.binary_search(&block).is_ok())
        }),
        EntityCheck::PeriodicTick(period) => period > 0 && facts.age % i64::from(period) == 0,
        EntityCheck::HorizontalSpeedMin(min) => facts.horizontal_speed >= min,
        EntityCheck::FallDistanceMin(min) => facts.fall_distance >= min,
        EntityCheck::IsPlayer(expected) => facts.player.is_some() == expected,
        EntityCheck::Gamemode(modes) => facts
            .player
            .is_some_and(|player| modes.contains(&player.gamemode)),
        EntityCheck::FoodMin(min) => facts.player.is_some_and(|player| player.food >= min),
        EntityCheck::CanSeeSky(expected) => facts.sees_sky == expected,
        EntityCheck::EntityType(name) => facts.kind == Some(name),
    }
}

pub(crate) fn requirement_holds<R: Rng + ?Sized>(
    requirement: &Requirement,
    scene: &EffectScene<'_>,
    rng: &mut R,
) -> bool {
    match requirement {
        Requirement::Loot(condition) => condition_holds(condition, &scene.loot, rng),
        Requirement::Not(inner) => !requirement_holds(inner, scene, rng),
        Requirement::AnyOf(terms) => terms.iter().any(|term| requirement_holds(term, scene, rng)),
        Requirement::AllOf(terms) => terms.iter().all(|term| requirement_holds(term, scene, rng)),
        Requirement::ActiveCheck(expected) => scene.active == *expected,
        Requirement::Entity { target, check } => scene
            .facts(*target)
            .is_some_and(|facts| check_holds(check, facts)),
        Requirement::Location {
            block_tag,
            can_see_sky,
        } => scene.location.is_some_and(|location| {
            block_tag.is_none_or(|tag| {
                location.block.is_some_and(|block| {
                    tags::tag(BLOCK_REGISTRY, tag)
                        .is_some_and(|ids| ids.binary_search(&block).is_ok())
                })
            }) && can_see_sky.is_none_or(|expected| location.sees_sky == expected)
        }),
    }
}
