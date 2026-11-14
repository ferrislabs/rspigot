use crate::generated::sulfur_cube::ARCHETYPES;
use crate::tags;

const ITEM_REGISTRY: &str = "minecraft:item";
const SWALLOWABLE: &str = "minecraft:sulfur_cube_swallowable";
const FOOD: &str = "minecraft:sulfur_cube_food";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    AddValue,
    AddMultipliedBase,
    AddMultipliedTotal,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttributeEntry {
    pub attribute: &'static str,
    pub id: &'static str,
    pub amount: f64,
    pub operation: Operation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Explosion {
    pub power: i32,
    pub causes_fire: bool,
    pub fuse: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContactDamage {
    pub damage_type: &'static str,
    pub amount: f32,
    pub attribute_to_source: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Knockback {
    pub horizontal_power: f32,
    pub vertical_power: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sounds {
    pub hit_sound: &'static str,
    pub push_sound: &'static str,
    pub push_sound_impulse_threshold: f32,
    pub push_sound_cooldown: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Archetype {
    pub name: &'static str,
    pub items: &'static str,
    pub attribute_modifiers: &'static [AttributeEntry],
    pub buoyant: bool,
    pub explosion: Option<Explosion>,
    pub contact_damage: Option<ContactDamage>,
    pub knockback_modifiers: Knockback,
    pub sound_settings: Sounds,
}

pub const DEFAULT_KNOCKBACK: Knockback = Knockback {
    horizontal_power: 0.33,
    vertical_power: 0.06,
};

pub const DEFAULT_SOUNDS: Sounds = Sounds {
    hit_sound: "minecraft:entity.sulfur_cube.regular.hit",
    push_sound: "minecraft:entity.sulfur_cube.regular.push",
    push_sound_impulse_threshold: 0.2,
    push_sound_cooldown: 0.5,
};

pub fn archetypes() -> &'static [Archetype] {
    &ARCHETYPES
}

pub fn archetype(name: &str) -> Option<&'static Archetype> {
    ARCHETYPES.iter().find(|archetype| archetype.name == name)
}

impl Archetype {
    pub fn matches(&self, item: u16) -> bool {
        tags::tag(ITEM_REGISTRY, self.items)
            .is_some_and(|ids| ids.binary_search(&u32::from(item)).is_ok())
    }
}

pub fn matching(item: u16) -> impl Iterator<Item = &'static Archetype> {
    ARCHETYPES
        .iter()
        .filter(move |archetype| archetype.matches(item))
}

pub fn is_swallowable(item: u16) -> bool {
    in_item_tag(SWALLOWABLE, item)
}

pub fn is_food(item: u16) -> bool {
    in_item_tag(FOOD, item)
}

fn in_item_tag(name: &str, item: u16) -> bool {
    tags::tag(ITEM_REGISTRY, name).is_some_and(|ids| ids.binary_search(&u32::from(item)).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries;

    fn item(name: &str) -> u16 {
        registries::static_index(ITEM_REGISTRY, name).unwrap() as u16
    }

    #[test]
    fn the_twelve_vanilla_archetypes_are_loaded_by_name() {
        let names: Vec<&str> = archetypes().iter().map(|a| a.name).collect();
        assert_eq!(
            names,
            [
                "minecraft:bouncy",
                "minecraft:explosive",
                "minecraft:fast_flat",
                "minecraft:fast_sliding",
                "minecraft:high_resistance",
                "minecraft:hot",
                "minecraft:light",
                "minecraft:regular",
                "minecraft:slow_bouncy",
                "minecraft:slow_flat",
                "minecraft:slow_sliding",
                "minecraft:sticky",
            ]
        );
    }

    #[test]
    fn an_item_matches_exactly_one_archetype_and_every_swallowable_item_matches_one() {
        for id in 0..registries::static_entries(ITEM_REGISTRY).unwrap().len() as u16 {
            let count = matching(id).count();
            assert!(count <= 1, "{id}");
            assert_eq!(is_swallowable(id), count == 1, "{id}");
        }
    }

    #[test]
    fn blocks_select_their_archetype() {
        let name = |block: &str| matching(item(block)).next().map(|a| a.name);
        assert_eq!(name("minecraft:tnt"), Some("minecraft:explosive"));
        assert_eq!(name("minecraft:magma_block"), Some("minecraft:hot"));
        assert_eq!(name("minecraft:oak_planks"), Some("minecraft:bouncy"));
        assert_eq!(name("minecraft:white_wool"), Some("minecraft:light"));
        assert_eq!(name("minecraft:honeycomb_block"), Some("minecraft:sticky"));
        assert_eq!(name("minecraft:blue_ice"), Some("minecraft:fast_sliding"));
        assert_eq!(
            name("minecraft:soul_sand"),
            Some("minecraft:high_resistance")
        );
        assert_eq!(name("minecraft:iron_block"), Some("minecraft:slow_flat"));
        assert_eq!(name("minecraft:stone"), Some("minecraft:slow_bouncy"));
        assert_eq!(name("minecraft:dirt"), Some("minecraft:regular"));
        assert_eq!(name("minecraft:sponge"), Some("minecraft:fast_flat"));
        assert_eq!(name("minecraft:mycelium"), Some("minecraft:slow_sliding"));
        assert_eq!(name("minecraft:diamond"), None);
    }

    #[test]
    fn the_explosive_archetype_carries_the_tnt_fuse_and_power() {
        let explosive = archetype("minecraft:explosive").unwrap();
        assert_eq!(
            explosive.explosion,
            Some(Explosion {
                power: 3,
                causes_fire: false,
                fuse: 120
            })
        );
        assert!(explosive.buoyant);
        assert_eq!(explosive.contact_damage, None);
    }

    #[test]
    fn the_hot_archetype_burns_without_attributing_the_damage_to_the_cube() {
        let hot = archetype("minecraft:hot").unwrap();
        assert_eq!(
            hot.contact_damage,
            Some(ContactDamage {
                damage_type: "minecraft:sulfur_cube_hot",
                amount: 1.0,
                attribute_to_source: false,
            })
        );
        assert_eq!(hot.explosion, None);
    }

    #[test]
    fn archetypes_adjust_the_physics_attributes_and_the_knockback() {
        let bouncy = archetype("minecraft:bouncy").unwrap();
        let entry = |attribute: &str| {
            bouncy
                .attribute_modifiers
                .iter()
                .find(|entry| entry.attribute == attribute)
                .copied()
                .unwrap()
        };
        assert_eq!(entry("minecraft:knockback_resistance").amount, -2.0);
        assert_eq!(
            entry("minecraft:bounciness").id,
            "minecraft:bouncy_add_bounciness"
        );
        assert_eq!(entry("minecraft:bounciness").operation, Operation::AddValue);
        assert_eq!(
            entry("minecraft:friction_modifier").operation,
            Operation::AddMultipliedTotal
        );
        assert_eq!(bouncy.knockback_modifiers.vertical_power, 0.105);
        assert_eq!(bouncy.sound_settings.push_sound_cooldown, 0.7);
        assert!(!archetype("minecraft:sticky").unwrap().buoyant);
    }

    #[test]
    fn only_slime_balls_feed_a_baby_cube() {
        assert!(is_food(item("minecraft:slime_ball")));
        assert!(!is_food(item("minecraft:tnt")));
    }

    #[test]
    fn the_defaults_are_the_regular_sounds_and_a_weak_knockback() {
        assert_eq!(DEFAULT_KNOCKBACK.horizontal_power, 0.33);
        assert_eq!(DEFAULT_SOUNDS.push_sound_cooldown, 0.5);
        assert_eq!(
            DEFAULT_SOUNDS.hit_sound,
            archetype("minecraft:regular")
                .unwrap()
                .sound_settings
                .hit_sound
        );
    }
}
