const REGISTRY: &str = "minecraft:damage_type";
const FALL_POWER_EPSILON: f64 = 1.0e-6;
const ARMOR_TOUGHNESS_BASE: f32 = 2.0;
const ARMOR_TOUGHNESS_DIVIDER: f32 = 4.0;
const MIN_ARMOR_RATIO: f32 = 0.2;
const MAX_ARMOR: f32 = 20.0;
const ARMOR_PROTECTION_DIVIDER: f32 = 25.0;
const RESISTANCE_PER_LEVEL: i32 = 5;
const RESISTANCE_DIVIDER: i32 = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DamageType {
    Fall,
    OutOfWorld,
    FallingAnvil,
    FallingBlock,
    FallingStalactite,
    Stalagmite,
    PlayerAttack,
    MobAttack,
    MobAttackNoAggro,
    GenericKill,
    Generic,
    Starve,
    Drown,
    Magic,
    IndirectMagic,
    Wither,
    OnFire,
    InFire,
    Lava,
    Arrow,
    Thrown,
    MobProjectile,
    Fireball,
    UnattributedFireball,
    WitherSkull,
    Explosion,
    PlayerExplosion,
    WindCharge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DamageTag {
    AlwaysMostSignificantFall,
    BypassesArmor,
    BypassesCooldown,
    BypassesEffects,
    BypassesInvulnerability,
    BypassesResistance,
    DamagesHelmet,
    IsFall,
    IsFire,
    NoAnger,
    NoImpact,
    NoKnockback,
}

impl DamageTag {
    pub const fn name(self) -> &'static str {
        match self {
            Self::AlwaysMostSignificantFall => "minecraft:always_most_significant_fall",
            Self::BypassesArmor => "minecraft:bypasses_armor",
            Self::BypassesCooldown => "minecraft:bypasses_cooldown",
            Self::BypassesEffects => "minecraft:bypasses_effects",
            Self::BypassesInvulnerability => "minecraft:bypasses_invulnerability",
            Self::BypassesResistance => "minecraft:bypasses_resistance",
            Self::DamagesHelmet => "minecraft:damages_helmet",
            Self::IsFall => "minecraft:is_fall",
            Self::IsFire => "minecraft:is_fire",
            Self::NoAnger => "minecraft:no_anger",
            Self::NoImpact => "minecraft:no_impact",
            Self::NoKnockback => "minecraft:no_knockback",
        }
    }
}

impl DamageType {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Fall => "minecraft:fall",
            Self::OutOfWorld => "minecraft:out_of_world",
            Self::FallingAnvil => "minecraft:falling_anvil",
            Self::FallingBlock => "minecraft:falling_block",
            Self::FallingStalactite => "minecraft:falling_stalactite",
            Self::Stalagmite => "minecraft:stalagmite",
            Self::PlayerAttack => "minecraft:player_attack",
            Self::MobAttack => "minecraft:mob_attack",
            Self::MobAttackNoAggro => "minecraft:mob_attack_no_aggro",
            Self::GenericKill => "minecraft:generic_kill",
            Self::Generic => "minecraft:generic",
            Self::Starve => "minecraft:starve",
            Self::Drown => "minecraft:drown",
            Self::Magic => "minecraft:magic",
            Self::IndirectMagic => "minecraft:indirect_magic",
            Self::Wither => "minecraft:wither",
            Self::OnFire => "minecraft:on_fire",
            Self::InFire => "minecraft:in_fire",
            Self::Lava => "minecraft:lava",
            Self::Arrow => "minecraft:arrow",
            Self::Thrown => "minecraft:thrown",
            Self::MobProjectile => "minecraft:mob_projectile",
            Self::Fireball => "minecraft:fireball",
            Self::UnattributedFireball => "minecraft:unattributed_fireball",
            Self::WitherSkull => "minecraft:wither_skull",
            Self::Explosion => "minecraft:explosion",
            Self::PlayerExplosion => "minecraft:player_explosion",
            Self::WindCharge => "minecraft:wind_charge",
        }
    }

    pub const fn message_id(self) -> &'static str {
        match self {
            Self::Fall => "fall",
            Self::OutOfWorld => "outOfWorld",
            Self::FallingAnvil => "anvil",
            Self::FallingBlock => "fallingBlock",
            Self::FallingStalactite => "fallingStalactite",
            Self::Stalagmite => "stalagmite",
            Self::PlayerAttack => "player",
            Self::MobAttack | Self::MobAttackNoAggro => "mob",
            Self::GenericKill => "genericKill",
            Self::Generic => "generic",
            Self::Starve => "starve",
            Self::Drown => "drown",
            Self::Magic => "magic",
            Self::IndirectMagic => "indirectMagic",
            Self::Wither => "wither",
            Self::OnFire => "onFire",
            Self::InFire => "inFire",
            Self::Lava => "lava",
            Self::Arrow => "arrow",
            Self::Thrown => "thrown",
            Self::MobProjectile => "mob",
            Self::Fireball => "fireball",
            Self::UnattributedFireball => "onFire",
            Self::WitherSkull => "witherSkull",
            Self::Explosion => "explosion",
            Self::PlayerExplosion => "explosion.player",
            Self::WindCharge => "mob",
        }
    }

    pub const fn exhaustion(self) -> f32 {
        match self {
            Self::FallingAnvil | Self::FallingBlock | Self::FallingStalactite => 0.1,
            Self::PlayerAttack | Self::MobAttack | Self::MobAttackNoAggro => 0.1,
            Self::InFire
            | Self::Lava
            | Self::Arrow
            | Self::Thrown
            | Self::MobProjectile
            | Self::Fireball
            | Self::UnattributedFireball
            | Self::WitherSkull
            | Self::Explosion
            | Self::PlayerExplosion
            | Self::WindCharge => 0.1,
            _ => 0.0,
        }
    }

    pub const fn scales_always(self) -> bool {
        matches!(self, Self::Explosion | Self::PlayerExplosion)
    }

    pub const fn fall_variants(self) -> bool {
        matches!(self, Self::Fall)
    }

    pub fn registry_id(self) -> Option<i32> {
        rspigot_data::registries::synced_index(REGISTRY, self.name())
            .and_then(|id| i32::try_from(id).ok())
    }

    pub fn is(self, tag: DamageTag) -> bool {
        self.in_tag(tag.name())
    }

    pub fn in_tag(self, tag: &str) -> bool {
        let Some(id) = rspigot_data::registries::synced_index(REGISTRY, self.name()) else {
            return false;
        };
        rspigot_data::tags::tag(REGISTRY, tag)
            .is_some_and(|members| members.binary_search(&id).is_ok())
    }
}

pub fn scaled_for_difficulty(amount: f32, difficulty: crate::world::Difficulty) -> f32 {
    use crate::world::Difficulty;
    match difficulty {
        Difficulty::Peaceful => 0.0,
        Difficulty::Easy => (amount / 2.0 + 1.0).min(amount),
        Difficulty::Normal => amount,
        Difficulty::Hard => amount * 3.0 / 2.0,
    }
}

pub fn java_clamp(value: f32, min: f32, max: f32) -> f32 {
    if value < min { min } else { value.min(max) }
}

pub fn fall_damage(distance: f64, multiplier: f32, safe_fall: f64, damage_multiplier: f64) -> i32 {
    let power = distance + FALL_POWER_EPSILON - safe_fall;
    (power * f64::from(multiplier) * damage_multiplier).floor() as i32
}

pub fn after_armor(damage: f32, armor: f32, toughness: f32) -> f32 {
    let toughness = ARMOR_TOUGHNESS_BASE + toughness / ARMOR_TOUGHNESS_DIVIDER;
    let effective = java_clamp(
        armor - damage / toughness,
        armor * MIN_ARMOR_RATIO,
        MAX_ARMOR,
    );
    damage * (1.0 - effective / ARMOR_PROTECTION_DIVIDER)
}

pub fn after_resistance(damage: f32, amplifier: u8) -> f32 {
    let reduction = (i32::from(amplifier) + 1) * RESISTANCE_PER_LEVEL;
    let kept = (RESISTANCE_DIVIDER - reduction) as f32;
    (damage * kept / RESISTANCE_DIVIDER as f32).max(0.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Absorbed {
    pub damage: f32,
    pub absorption: f32,
}

pub fn absorb(damage: f32, absorption: f32) -> Absorbed {
    let remaining = (damage - absorption).max(0.0);
    Absorbed {
        damage: remaining,
        absorption: absorption - (damage - remaining),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ids_follow_the_synced_damage_type_order() {
        assert_eq!(DamageType::Fall.registry_id(), Some(10));
        assert_eq!(DamageType::FallingAnvil.registry_id(), Some(11));
        assert_eq!(DamageType::GenericKill.registry_id(), Some(19));
        assert_eq!(DamageType::OutOfWorld.registry_id(), Some(32));
        assert_eq!(DamageType::PlayerAttack.registry_id(), Some(34));
        assert_eq!(DamageType::MobAttack.registry_id(), Some(28));
        assert_eq!(DamageType::Wither.registry_id(), Some(49));
    }

    #[test]
    fn the_no_aggro_mob_attack_is_registered_with_the_mob_message_and_the_no_anger_tag() {
        let kind = DamageType::MobAttackNoAggro;
        assert!(kind.registry_id().is_some());
        assert_eq!(kind.name(), "minecraft:mob_attack_no_aggro");
        assert_eq!(kind.message_id(), "mob");
        assert_eq!(kind.exhaustion(), 0.1);
        assert!(kind.is(DamageTag::NoAnger));
        assert!(!kind.is(DamageTag::NoKnockback));
    }

    #[test]
    fn thrown_damage_is_registered_and_costs_a_tenth_of_exhaustion() {
        assert!(DamageType::Thrown.registry_id().is_some());
        assert_eq!(DamageType::Thrown.name(), "minecraft:thrown");
        assert_eq!(DamageType::Thrown.message_id(), "thrown");
        assert_eq!(DamageType::Thrown.exhaustion(), 0.1);
    }

    #[test]
    fn the_mob_projectile_damage_type_is_registered_as_a_projectile_with_the_mob_message() {
        let kind = DamageType::MobProjectile;
        assert!(kind.registry_id().is_some());
        assert_eq!(kind.name(), "minecraft:mob_projectile");
        assert_eq!(kind.message_id(), "mob");
        assert_eq!(kind.exhaustion(), 0.1);
        assert!(kind.in_tag("minecraft:is_projectile"));
        assert!(!kind.is(DamageTag::BypassesArmor));
        assert!(!kind.is(DamageTag::IsFire));
    }

    #[test]
    fn fireball_damage_types_are_registered_with_the_datapack_messages_and_tags() {
        let all = [
            (DamageType::Fireball, "minecraft:fireball", "fireball"),
            (
                DamageType::UnattributedFireball,
                "minecraft:unattributed_fireball",
                "onFire",
            ),
            (
                DamageType::WitherSkull,
                "minecraft:wither_skull",
                "witherSkull",
            ),
        ];
        for (kind, name, message) in all {
            assert!(kind.registry_id().is_some(), "{name}");
            assert_eq!(kind.name(), name);
            assert_eq!(kind.message_id(), message);
            assert_eq!(kind.exhaustion(), 0.1);
            assert!(kind.in_tag("minecraft:is_projectile"), "{name}");
        }
        assert!(DamageType::Fireball.is(DamageTag::IsFire));
        assert!(DamageType::UnattributedFireball.is(DamageTag::IsFire));
        assert!(!DamageType::WitherSkull.is(DamageTag::IsFire));
        assert!(!DamageType::Fireball.is(DamageTag::BypassesArmor));
    }

    #[test]
    fn indirect_magic_is_registered_and_witches_resist_it_with_magic() {
        assert!(DamageType::IndirectMagic.registry_id().is_some());
        assert_eq!(DamageType::IndirectMagic.name(), "minecraft:indirect_magic");
        assert_eq!(DamageType::IndirectMagic.message_id(), "indirectMagic");
        assert_eq!(DamageType::IndirectMagic.exhaustion(), 0.0);
        assert!(DamageType::IndirectMagic.is(DamageTag::BypassesArmor));
        for kind in [DamageType::Magic, DamageType::IndirectMagic] {
            assert!(kind.in_tag("minecraft:witch_resistant_to"));
        }
        for kind in [
            DamageType::MobAttack,
            DamageType::Arrow,
            DamageType::Explosion,
        ] {
            assert!(!kind.in_tag("minecraft:witch_resistant_to"));
        }
    }

    #[test]
    fn tags_come_from_the_datapack() {
        assert!(DamageType::OutOfWorld.is(DamageTag::BypassesInvulnerability));
        assert!(DamageType::GenericKill.is(DamageTag::BypassesInvulnerability));
        assert!(!DamageType::Fall.is(DamageTag::BypassesInvulnerability));
        assert!(DamageType::Fall.is(DamageTag::IsFall));
        assert!(DamageType::Stalagmite.is(DamageTag::IsFall));
        assert!(DamageType::Fall.is(DamageTag::NoKnockback));
        assert!(DamageType::Magic.is(DamageTag::NoKnockback));
        assert!(!DamageType::PlayerAttack.is(DamageTag::NoKnockback));
        assert!(!DamageType::FallingAnvil.is(DamageTag::NoKnockback));
        assert!(DamageType::FallingAnvil.is(DamageTag::DamagesHelmet));
        assert!(DamageType::Starve.is(DamageTag::BypassesEffects));
        assert!(DamageType::OutOfWorld.is(DamageTag::AlwaysMostSignificantFall));
        assert!(!DamageType::Magic.is(DamageTag::BypassesCooldown));
        assert!(!DamageType::MobAttack.is(DamageTag::NoKnockback));
        assert!(!DamageType::MobAttack.is(DamageTag::NoAnger));
        assert!(DamageType::Fall.in_tag("minecraft:no_knockback"));
        assert!(!DamageType::PlayerAttack.in_tag("minecraft:is_projectile"));
        assert!(DamageType::Thrown.in_tag("minecraft:is_projectile"));
        assert!(DamageType::Arrow.in_tag("minecraft:is_projectile"));
        assert!(!DamageType::PlayerAttack.in_tag("minecraft:nope"));
    }

    #[test]
    fn difficulty_scales_mob_damage_against_players() {
        use crate::world::Difficulty;
        assert_eq!(scaled_for_difficulty(3.0, Difficulty::Peaceful), 0.0);
        assert_eq!(scaled_for_difficulty(3.0, Difficulty::Easy), 2.5);
        assert_eq!(scaled_for_difficulty(1.0, Difficulty::Easy), 1.0);
        assert_eq!(scaled_for_difficulty(3.0, Difficulty::Normal), 3.0);
        assert_eq!(scaled_for_difficulty(3.0, Difficulty::Hard), 4.5);
    }

    #[test]
    fn fall_damage_subtracts_the_safe_distance() {
        assert_eq!(fall_damage(20.0, 1.0, 3.0, 1.0), 17);
        assert_eq!(fall_damage(3.0, 1.0, 3.0, 1.0), 0);
        assert_eq!(fall_damage(3.5, 1.0, 3.0, 1.0), 0);
        assert_eq!(fall_damage(4.0, 1.0, 3.0, 1.0), 1);
        assert_eq!(fall_damage(1.0, 1.0, 3.0, 1.0), -2);
    }

    #[test]
    fn jump_boost_raises_the_safe_distance_and_hay_cuts_damage() {
        assert_eq!(fall_damage(20.0, 1.0, 5.0, 1.0), 15);
        assert_eq!(fall_damage(20.0, 0.2, 3.0, 1.0), 3);
        assert_eq!(fall_damage(30.0, 0.0, 3.0, 1.0), 0);
        assert_eq!(fall_damage(10.0 * 0.5, 1.0, 3.0, 1.0), 2);
        assert_eq!(fall_damage(4.0 + 2.5, 2.0, 3.0, 1.0), 7);
    }

    #[test]
    fn resistance_removes_a_fifth_per_level() {
        assert_eq!(after_resistance(10.0, 0), 8.0);
        assert_eq!(after_resistance(10.0, 1), 6.0);
        assert_eq!(after_resistance(10.0, 3), 2.0);
        assert_eq!(after_resistance(10.0, 4), 0.0);
        assert_eq!(after_resistance(10.0, 200), 0.0);
    }

    #[test]
    fn absorption_soaks_damage_first() {
        assert_eq!(
            absorb(3.0, 4.0),
            Absorbed {
                damage: 0.0,
                absorption: 1.0
            }
        );
        assert_eq!(
            absorb(6.0, 4.0),
            Absorbed {
                damage: 2.0,
                absorption: 0.0
            }
        );
        assert_eq!(
            absorb(5.0, 0.0),
            Absorbed {
                damage: 5.0,
                absorption: 0.0
            }
        );
    }

    #[test]
    fn armor_follows_combat_rules() {
        assert_eq!(after_armor(10.0, 0.0, 0.0), 10.0);
        assert!((after_armor(10.0, 20.0, 0.0) - 10.0 * (1.0 - 15.0 / 25.0)).abs() < 1e-6);
        assert!((after_armor(100.0, 20.0, 0.0) - 100.0 * (1.0 - 4.0 / 25.0)).abs() < 1e-4);
        assert!((after_armor(4.0, 2.0, 0.0) - 4.0 * (1.0 - 0.4 / 25.0)).abs() < 1e-6);
    }
}
