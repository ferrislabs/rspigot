use rspigot_entities::EntityKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackingRanges {
    pub players: i32,
    pub animals: i32,
    pub monsters: i32,
    pub misc: i32,
    pub display: i32,
    pub other: i32,
    pub monster_activation: i32,
}

impl Default for TrackingRanges {
    fn default() -> Self {
        Self {
            players: 48,
            animals: 48,
            monsters: 48,
            misc: 32,
            display: 128,
            other: 64,
            monster_activation: 32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationType {
    Monster,
    Raider,
    Animal,
    Misc,
}

pub fn activation_type(kind: EntityKind) -> ActivationType {
    if kind.extends("Raider") {
        ActivationType::Raider
    } else if kind.extends("Monster") || kind.extends("Slime") {
        ActivationType::Monster
    } else if kind.extends("PathfinderMob") || kind.extends("AmbientCreature") {
        ActivationType::Animal
    } else {
        ActivationType::Misc
    }
}

pub fn tracking_range(kind: EntityKind, ranges: &TrackingRanges) -> i32 {
    let default = i32::from(kind.tracking_range()) * 16;
    if default == 0 {
        return 0;
    }
    if kind == EntityKind::Player {
        return ranges.players;
    }
    match activation_type(kind) {
        ActivationType::Monster | ActivationType::Raider => return ranges.monsters,
        _ if kind.extends("Ghast") => {
            return ranges.monsters.max(ranges.monster_activation);
        }
        ActivationType::Animal => return ranges.animals,
        ActivationType::Misc => {}
    }
    if kind.extends("ItemFrame")
        || kind.extends("Painting")
        || kind.extends("ItemEntity")
        || kind.extends("ExperienceOrb")
    {
        ranges.misc
    } else if kind.extends("Display") {
        ranges.display
    } else {
        ranges.other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spigot_groups_follow_the_class_hierarchy() {
        let ranges = TrackingRanges::default();
        assert_eq!(tracking_range(EntityKind::Cow, &ranges), 48);
        assert_eq!(tracking_range(EntityKind::Zombie, &ranges), 48);
        assert_eq!(tracking_range(EntityKind::Pillager, &ranges), 48);
        assert_eq!(tracking_range(EntityKind::Item, &ranges), 32);
        assert_eq!(tracking_range(EntityKind::FallingBlock, &ranges), 64);
        assert_eq!(tracking_range(EntityKind::BlockDisplay, &ranges), 128);
        assert_eq!(tracking_range(EntityKind::Player, &ranges), 48);
        assert_eq!(tracking_range(EntityKind::Bat, &ranges), 48);
        let wide = TrackingRanges {
            monster_activation: 200,
            ..ranges
        };
        assert_eq!(tracking_range(EntityKind::Ghast, &wide), 200);
    }
}
