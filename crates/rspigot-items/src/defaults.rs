use crate::{ConsumeEffect, ConsumeMobEffect, ItemKind};

const TOTEM_EFFECTS: &[ConsumeMobEffect] = &[
    ConsumeMobEffect {
        effect: "minecraft:regeneration",
        amplifier: 1,
        duration: 900,
        show_icon: true,
    },
    ConsumeMobEffect {
        effect: "minecraft:absorption",
        amplifier: 1,
        duration: 100,
        show_icon: true,
    },
    ConsumeMobEffect {
        effect: "minecraft:fire_resistance",
        amplifier: 0,
        duration: 800,
        show_icon: true,
    },
];

const TOTEM_DEATH_EFFECTS: &[ConsumeEffect] = &[
    ConsumeEffect::ClearAllEffects,
    ConsumeEffect::ApplyEffects {
        effects: TOTEM_EFFECTS,
        probability: 1.0,
    },
];

const GENERIC_EQUIP_SOUND: &str = "minecraft:item.armor.equip_generic";

impl ItemKind {
    pub const fn use_cooldown_seconds(self) -> Option<f32> {
        match self {
            Self::EnderPearl | Self::ChorusFruit => Some(1.0),
            Self::WindCharge => Some(0.5),
            _ => None,
        }
    }

    pub const fn is_glider(self) -> bool {
        matches!(self, Self::Elytra)
    }

    pub const fn death_effects(self) -> Option<&'static [ConsumeEffect]> {
        match self {
            Self::TotemOfUndying => Some(TOTEM_DEATH_EFFECTS),
            _ => None,
        }
    }

    pub const fn equippable_swappable(self) -> bool {
        !matches!(
            self,
            Self::CarvedPumpkin
                | Self::CreeperHead
                | Self::DragonHead
                | Self::PiglinHead
                | Self::PlayerHead
                | Self::SkeletonSkull
                | Self::WitherSkeletonSkull
                | Self::ZombieHead
                | Self::Shield
        )
    }

    pub fn equip_sound(self) -> &'static str {
        let name = self.name().trim_start_matches("minecraft:");
        if self == Self::Elytra {
            return "minecraft:item.armor.equip_elytra";
        }
        if self == Self::TurtleHelmet {
            return "minecraft:item.armor.equip_turtle";
        }
        let armor = ["_helmet", "_chestplate", "_leggings", "_boots"]
            .iter()
            .any(|suffix| name.ends_with(suffix));
        if !armor {
            return GENERIC_EQUIP_SOUND;
        }
        match name.split('_').next() {
            Some("chainmail") => "minecraft:item.armor.equip_chain",
            Some("copper") => "minecraft:item.armor.equip_copper",
            Some("diamond") => "minecraft:item.armor.equip_diamond",
            Some("golden") => "minecraft:item.armor.equip_gold",
            Some("iron") => "minecraft:item.armor.equip_iron",
            Some("leather") => "minecraft:item.armor.equip_leather",
            Some("netherite") => "minecraft:item.armor.equip_netherite",
            _ => GENERIC_EQUIP_SOUND,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn use_cooldowns_come_from_the_item_reports() {
        assert_eq!(ItemKind::EnderPearl.use_cooldown_seconds(), Some(1.0));
        assert_eq!(ItemKind::ChorusFruit.use_cooldown_seconds(), Some(1.0));
        assert_eq!(ItemKind::WindCharge.use_cooldown_seconds(), Some(0.5));
        assert_eq!(ItemKind::Snowball.use_cooldown_seconds(), None);
    }

    #[test]
    fn only_the_elytra_glides_and_only_the_totem_protects() {
        assert!(ItemKind::Elytra.is_glider());
        assert!(!ItemKind::DiamondChestplate.is_glider());
        assert!(ItemKind::TotemOfUndying.death_effects().is_some());
        assert!(ItemKind::Apple.death_effects().is_none());
    }

    #[test]
    fn heads_pumpkins_and_shields_are_not_swappable() {
        assert!(!ItemKind::CarvedPumpkin.equippable_swappable());
        assert!(!ItemKind::ZombieHead.equippable_swappable());
        assert!(!ItemKind::Shield.equippable_swappable());
        assert!(ItemKind::DiamondHelmet.equippable_swappable());
        assert!(ItemKind::Elytra.equippable_swappable());
    }

    #[test]
    fn equip_sounds_follow_the_armor_material() {
        assert_eq!(
            ItemKind::GoldenBoots.equip_sound(),
            "minecraft:item.armor.equip_gold"
        );
        assert_eq!(
            ItemKind::NetheriteHelmet.equip_sound(),
            "minecraft:item.armor.equip_netherite"
        );
        assert_eq!(
            ItemKind::TurtleHelmet.equip_sound(),
            "minecraft:item.armor.equip_turtle"
        );
        assert_eq!(
            ItemKind::CarvedPumpkin.equip_sound(),
            "minecraft:item.armor.equip_generic"
        );
    }
}
