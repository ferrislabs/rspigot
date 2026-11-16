use rand::Rng;
use rspigot_data::enchantments::{
    self, ConditionalEffect, EffectRole, EnchantmentEffects, EntityEffect, ValueEffect, ValueOp,
};
use rspigot_domain::enchant;
use rspigot_domain::item::ItemStack;

use crate::loot::{LootContext, condition_holds};

const MAX_PROTECTION: f32 = 20.0;
const PROTECTION_DIVISOR: f32 = 25.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EquipSlot {
    Mainhand,
    Offhand,
    Head,
    Chest,
    Legs,
    Feet,
}

impl EquipSlot {
    fn is_armor(self) -> bool {
        matches!(self, Self::Head | Self::Chest | Self::Legs | Self::Feet)
    }

    fn belongs_to(self, group: &str) -> bool {
        match group {
            "any" => true,
            "hand" => matches!(self, Self::Mainhand | Self::Offhand),
            "armor" => self.is_armor(),
            "mainhand" => self == Self::Mainhand,
            "offhand" => self == Self::Offhand,
            "head" => self == Self::Head,
            "chest" => self == Self::Chest,
            "legs" => self == Self::Legs,
            "feet" => self == Self::Feet,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Worn<'a> {
    pub stack: &'a ItemStack,
    pub slot: EquipSlot,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Active {
    pub level: i32,
    pub effects: &'static EnchantmentEffects,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Planned {
    pub enchanted: EffectRole,
    pub affected: EffectRole,
    pub effect: &'static EntityEffect,
    pub level: i32,
}

pub(crate) fn active_in(worn: &[Worn<'_>]) -> Vec<Active> {
    worn.iter()
        .flat_map(|worn| {
            enchant::levels(worn.stack)
                .into_iter()
                .filter_map(move |(name, level)| {
                    let effects = enchantments::effects(&name)?;
                    effects
                        .slots
                        .iter()
                        .any(|group| worn.slot.belongs_to(group))
                        .then_some(Active {
                            level: i32::from(level),
                            effects,
                        })
                })
        })
        .collect()
}

fn apply(op: ValueOp, level: i32, value: f32, rng: &mut (impl Rng + ?Sized)) -> f32 {
    match op {
        ValueOp::Add(amount) => value + amount.calculate(level),
        ValueOp::Multiply(factor) => value * factor.calculate(level),
        ValueOp::Set(amount) => amount.calculate(level),
        ValueOp::RemoveBinomial(chance) => {
            let chance = chance.calculate(level);
            let removed = (0..value as i32)
                .filter(|_| rng.random::<f32>() < chance)
                .count();
            value - removed as f32
        }
    }
}

pub(crate) fn process_value(
    actives: &[Active],
    select: fn(&'static EnchantmentEffects) -> &'static [ValueEffect],
    base: f32,
    context: &LootContext<'_>,
    rng: &mut (impl Rng + ?Sized),
) -> f32 {
    let mut value = base;
    for active in actives {
        let scoped = LootContext {
            enchantment_level: active.level,
            ..*context
        };
        for effect in select(active.effects) {
            if effect
                .requirements
                .as_ref()
                .is_none_or(|requirements| condition_holds(requirements, &scoped, rng))
            {
                value = apply(effect.op, active.level, value, rng);
            }
        }
    }
    value
}

pub(crate) fn modify_damage(
    weapon: &[Worn<'_>],
    base: f32,
    context: &LootContext<'_>,
    rng: &mut (impl Rng + ?Sized),
) -> f32 {
    process_value(&active_in(weapon), |e| e.damage, base, context, rng)
}

pub(crate) fn modify_knockback(
    weapon: &[Worn<'_>],
    base: f32,
    context: &LootContext<'_>,
    rng: &mut (impl Rng + ?Sized),
) -> f32 {
    process_value(&active_in(weapon), |e| e.knockback, base, context, rng)
}

pub(crate) fn damage_protection(
    equipment: &[Worn<'_>],
    context: &LootContext<'_>,
    rng: &mut (impl Rng + ?Sized),
) -> f32 {
    process_value(
        &active_in(equipment),
        |e| e.damage_protection,
        0.0,
        context,
        rng,
    )
}

pub(crate) fn after_magic_absorb(damage: f32, protection: f32) -> f32 {
    damage * (1.0 - protection.clamp(0.0, MAX_PROTECTION) / PROTECTION_DIVISOR)
}

pub(crate) fn modify_ammo_use(
    bow: &[Worn<'_>],
    ammo: &ItemStack,
    base: f32,
    context: &LootContext<'_>,
    rng: &mut (impl Rng + ?Sized),
) -> f32 {
    let scoped = LootContext {
        tool: Some(ammo),
        ..*context
    };
    process_value(&active_in(bow), |e| e.ammo_use, base, &scoped, rng)
}

pub(crate) fn equipment_drop_bonus(
    weapon: &[Worn<'_>],
    base: f32,
    context: &LootContext<'_>,
    rng: &mut (impl Rng + ?Sized),
) -> f32 {
    process_value(
        &active_in(weapon),
        |e| e.equipment_drops,
        base,
        context,
        rng,
    )
}

pub(crate) fn post_attack(
    attacker: &[Worn<'_>],
    victim: &[Worn<'_>],
    context: &LootContext<'_>,
    rng: &mut (impl Rng + ?Sized),
) -> Vec<Planned> {
    let mut planned = Vec::new();
    for (role, worn) in [
        (EffectRole::Attacker, attacker),
        (EffectRole::Victim, victim),
    ] {
        for active in active_in(worn) {
            let scoped = LootContext {
                enchantment_level: active.level,
                ..*context
            };
            for entry in active.effects.post_attack {
                if entry.enchanted == role
                    && entry
                        .requirements
                        .as_ref()
                        .is_none_or(|requirements| condition_holds(requirements, &scoped, rng))
                {
                    planned.push(Planned {
                        enchanted: entry.enchanted,
                        affected: entry.affected,
                        effect: &entry.effect,
                        level: active.level,
                    });
                }
            }
        }
    }
    planned
}

pub(crate) fn projectile_spawned(
    weapon: &[Worn<'_>],
    context: &LootContext<'_>,
    rng: &mut (impl Rng + ?Sized),
) -> Vec<Planned> {
    let mut planned = Vec::new();
    for active in active_in(weapon) {
        let scoped = LootContext {
            enchantment_level: active.level,
            ..*context
        };
        for ConditionalEffect {
            effect,
            requirements,
        } in active.effects.projectile_spawned
        {
            if requirements
                .as_ref()
                .is_none_or(|requirements| condition_holds(requirements, &scoped, rng))
            {
                planned.push(Planned {
                    enchanted: EffectRole::Attacker,
                    affected: EffectRole::DamagingEntity,
                    effect,
                    level: active.level,
                });
            }
        }
    }
    planned
}

pub(crate) fn attribute_bonus(worn: &[Worn<'_>], attribute: &str) -> f64 {
    active_in(worn)
        .iter()
        .flat_map(|active| {
            active
                .effects
                .attributes
                .iter()
                .filter(|entry| entry.attribute == attribute)
                .map(|entry| f64::from(entry.amount.calculate(active.level)))
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU8;

    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use rspigot_data::enchantments::{EffectRole, EntityEffect};
    use rspigot_domain::ItemId;
    use rspigot_domain::damage::DamageType;
    use rspigot_entities::EntityKind;
    use rspigot_items::ItemKind;

    use super::*;
    use crate::loot::{LootDamage, LootEntity};

    fn stack(item: ItemKind, enchantments: &[(&str, u8)]) -> ItemStack {
        let mut stack = ItemStack::new(ItemId(item.id().0), NonZeroU8::MIN);
        let levels: Vec<(String, u8)> = enchantments
            .iter()
            .map(|(name, level)| ((*name).to_owned(), *level))
            .collect();
        enchant::set_levels(&mut stack, &levels);
        stack
    }

    fn rng() -> StdRng {
        StdRng::seed_from_u64(3)
    }

    fn melee(kind: DamageType) -> LootDamage {
        LootDamage {
            kind,
            source: Some(LootEntity::of(EntityKind::Player)),
            direct: Some(LootEntity::of(EntityKind::Player)),
            is_direct: true,
        }
    }

    fn damage_with(weapon: &ItemStack, victim: EntityKind, base: f32) -> f32 {
        let this = LootEntity::of(victim);
        let damage = melee(DamageType::PlayerAttack);
        let context = LootContext {
            this: Some(&this),
            damage: Some(&damage),
            ..LootContext::default()
        };
        let worn = [Worn {
            stack: weapon,
            slot: EquipSlot::Mainhand,
        }];
        modify_damage(&worn, base, &context, &mut rng())
    }

    #[test]
    fn sharpness_adds_one_plus_half_a_point_per_extra_level() {
        let sword = stack(ItemKind::DiamondSword, &[("minecraft:sharpness", 3)]);
        assert_eq!(damage_with(&sword, EntityKind::Cow, 7.0), 9.0);
        let sword = stack(ItemKind::DiamondSword, &[("minecraft:sharpness", 1)]);
        assert_eq!(damage_with(&sword, EntityKind::Cow, 7.0), 8.0);
    }

    #[test]
    fn smite_hits_the_undead_and_bane_the_arthropods() {
        let smite = stack(ItemKind::DiamondSword, &[("minecraft:smite", 2)]);
        assert_eq!(damage_with(&smite, EntityKind::Zombie, 7.0), 12.0);
        assert_eq!(damage_with(&smite, EntityKind::Spider, 7.0), 7.0);
        assert_eq!(damage_with(&smite, EntityKind::Cow, 7.0), 7.0);
        let bane = stack(
            ItemKind::DiamondSword,
            &[("minecraft:bane_of_arthropods", 1)],
        );
        assert_eq!(damage_with(&bane, EntityKind::Spider, 7.0), 9.5);
        assert_eq!(damage_with(&bane, EntityKind::Zombie, 7.0), 7.0);
    }

    #[test]
    fn impaling_only_counts_against_aquatic_mobs() {
        let trident = stack(ItemKind::Trident, &[("minecraft:impaling", 2)]);
        assert_eq!(damage_with(&trident, EntityKind::Squid, 8.0), 13.0);
        assert_eq!(damage_with(&trident, EntityKind::Cow, 8.0), 8.0);
    }

    #[test]
    fn power_needs_an_arrow_as_the_direct_attacker() {
        let bow = stack(ItemKind::Bow, &[("minecraft:power", 4)]);
        let this = LootEntity::of(EntityKind::Cow);
        let arrow = LootDamage {
            kind: DamageType::Arrow,
            source: Some(LootEntity::of(EntityKind::Player)),
            direct: Some(LootEntity::of(EntityKind::Arrow)),
            is_direct: false,
        };
        let context = LootContext {
            this: Some(&this),
            damage: Some(&arrow),
            ..LootContext::default()
        };
        let worn = [Worn {
            stack: &bow,
            slot: EquipSlot::Mainhand,
        }];
        assert_eq!(modify_damage(&worn, 6.0, &context, &mut rng()), 8.5);
        let melee = melee(DamageType::PlayerAttack);
        let context = LootContext {
            this: Some(&this),
            damage: Some(&melee),
            ..LootContext::default()
        };
        assert_eq!(modify_damage(&worn, 6.0, &context, &mut rng()), 6.0);
    }

    #[test]
    fn knockback_adds_a_point_per_level_and_punch_needs_an_arrow() {
        let sword = stack(ItemKind::DiamondSword, &[("minecraft:knockback", 2)]);
        let damage = melee(DamageType::PlayerAttack);
        let this = LootEntity::of(EntityKind::Cow);
        let context = LootContext {
            this: Some(&this),
            damage: Some(&damage),
            ..LootContext::default()
        };
        let worn = [Worn {
            stack: &sword,
            slot: EquipSlot::Mainhand,
        }];
        assert_eq!(modify_knockback(&worn, 0.0, &context, &mut rng()), 2.0);
        let bow = stack(ItemKind::Bow, &[("minecraft:punch", 2)]);
        let worn = [Worn {
            stack: &bow,
            slot: EquipSlot::Mainhand,
        }];
        assert_eq!(modify_knockback(&worn, 0.0, &context, &mut rng()), 0.0);
    }

    fn armor_set<'a>(pieces: &'a [ItemStack; 4]) -> Vec<Worn<'a>> {
        [
            EquipSlot::Head,
            EquipSlot::Chest,
            EquipSlot::Legs,
            EquipSlot::Feet,
        ]
        .into_iter()
        .zip(pieces)
        .map(|(slot, stack)| Worn { stack, slot })
        .collect()
    }

    fn protection_against(kind: DamageType, pieces: &[ItemStack; 4]) -> f32 {
        let this = LootEntity::of(EntityKind::Player);
        let damage = LootDamage {
            kind,
            source: None,
            direct: None,
            is_direct: false,
        };
        let context = LootContext {
            this: Some(&this),
            damage: Some(&damage),
            ..LootContext::default()
        };
        damage_protection(&armor_set(pieces), &context, &mut rng())
    }

    fn full_set(enchantment: &str, level: u8) -> [ItemStack; 4] {
        [
            ItemKind::DiamondHelmet,
            ItemKind::DiamondChestplate,
            ItemKind::DiamondLeggings,
            ItemKind::DiamondBoots,
        ]
        .map(|item| stack(item, &[(enchantment, level)]))
    }

    #[test]
    fn the_protection_family_sums_over_the_armor_pieces_by_damage_type() {
        let protection = full_set("minecraft:protection", 4);
        assert_eq!(protection_against(DamageType::Generic, &protection), 16.0);
        assert_eq!(protection_against(DamageType::OutOfWorld, &protection), 0.0);
        let blast = full_set("minecraft:blast_protection", 4);
        assert_eq!(
            protection_against(DamageType::PlayerExplosion, &blast),
            32.0
        );
        assert_eq!(protection_against(DamageType::Generic, &blast), 0.0);
        let fire = full_set("minecraft:fire_protection", 2);
        assert_eq!(protection_against(DamageType::OnFire, &fire), 16.0);
        let feather = full_set("minecraft:feather_falling", 4);
        assert_eq!(protection_against(DamageType::Fall, &feather), 48.0);
        let projectile = full_set("minecraft:projectile_protection", 1);
        assert_eq!(protection_against(DamageType::Arrow, &projectile), 8.0);
    }

    #[test]
    fn armor_enchantments_do_not_count_in_the_hand() {
        let boots = stack(ItemKind::DiamondBoots, &[("minecraft:protection", 4)]);
        let this = LootEntity::of(EntityKind::Player);
        let damage = LootDamage {
            kind: DamageType::Generic,
            source: None,
            direct: None,
            is_direct: false,
        };
        let context = LootContext {
            this: Some(&this),
            damage: Some(&damage),
            ..LootContext::default()
        };
        let held = [Worn {
            stack: &boots,
            slot: EquipSlot::Mainhand,
        }];
        assert_eq!(damage_protection(&held, &context, &mut rng()), 0.0);
    }

    #[test]
    fn magic_absorption_caps_at_twenty_points() {
        assert_eq!(after_magic_absorb(10.0, 0.0), 10.0);
        assert!((after_magic_absorb(10.0, 10.0) - 6.0).abs() < 1e-6);
        assert!((after_magic_absorb(10.0, 48.0) - 2.0).abs() < 1e-6);
        assert_eq!(after_magic_absorb(10.0, -5.0), 10.0);
    }

    #[test]
    fn infinity_removes_the_cost_of_plain_arrows_only() {
        let bow = stack(ItemKind::Bow, &[("minecraft:infinity", 1)]);
        let worn = [Worn {
            stack: &bow,
            slot: EquipSlot::Mainhand,
        }];
        let context = LootContext::default();
        let arrow = stack(ItemKind::Arrow, &[]);
        let spectral = stack(ItemKind::SpectralArrow, &[]);
        assert_eq!(
            modify_ammo_use(&worn, &arrow, 1.0, &context, &mut rng()),
            0.0
        );
        assert_eq!(
            modify_ammo_use(&worn, &spectral, 1.0, &context, &mut rng()),
            1.0
        );
    }

    #[test]
    fn fire_aspect_ignites_the_victim_on_direct_hits_only() {
        let sword = stack(ItemKind::DiamondSword, &[("minecraft:fire_aspect", 2)]);
        let attacker = [Worn {
            stack: &sword,
            slot: EquipSlot::Mainhand,
        }];
        let this = LootEntity::of(EntityKind::Cow);
        let direct = melee(DamageType::PlayerAttack);
        let context = LootContext {
            this: Some(&this),
            damage: Some(&direct),
            ..LootContext::default()
        };
        let planned = post_attack(&attacker, &[], &context, &mut rng());
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].affected, EffectRole::Victim);
        let EntityEffect::Ignite(seconds) = planned[0].effect else {
            panic!("ignite")
        };
        assert_eq!(seconds.calculate(planned[0].level), 8.0);
        let indirect = LootDamage {
            is_direct: false,
            ..direct.clone()
        };
        let context = LootContext {
            damage: Some(&indirect),
            ..context
        };
        assert!(post_attack(&attacker, &[], &context, &mut rng()).is_empty());
    }

    #[test]
    fn thorns_trigger_from_the_victims_armor_with_a_level_scaled_chance() {
        let chest = stack(ItemKind::DiamondChestplate, &[("minecraft:thorns", 7)]);
        let victim = [Worn {
            stack: &chest,
            slot: EquipSlot::Chest,
        }];
        let this = LootEntity::of(EntityKind::Player);
        let damage = melee(DamageType::MobAttack);
        let context = LootContext {
            this: Some(&this),
            damage: Some(&damage),
            ..LootContext::default()
        };
        let planned = post_attack(&[], &victim, &context, &mut rng());
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].enchanted, EffectRole::Victim);
        assert_eq!(planned[0].affected, EffectRole::Attacker);
        let low = stack(ItemKind::DiamondChestplate, &[("minecraft:thorns", 1)]);
        let victim = [Worn {
            stack: &low,
            slot: EquipSlot::Chest,
        }];
        let mut thorned = 0;
        let mut random = rng();
        for _ in 0..2_000 {
            thorned += post_attack(&[], &victim, &context, &mut random).len();
        }
        assert!((200..400).contains(&thorned), "{thorned}");
    }

    #[test]
    fn bane_of_arthropods_slows_arthropod_victims() {
        let sword = stack(
            ItemKind::DiamondSword,
            &[("minecraft:bane_of_arthropods", 2)],
        );
        let attacker = [Worn {
            stack: &sword,
            slot: EquipSlot::Mainhand,
        }];
        let direct = melee(DamageType::PlayerAttack);
        let spider = LootEntity::of(EntityKind::Spider);
        let context = LootContext {
            this: Some(&spider),
            damage: Some(&direct),
            ..LootContext::default()
        };
        let planned = post_attack(&attacker, &[], &context, &mut rng());
        assert!(matches!(
            planned[0].effect,
            EntityEffect::ApplyMobEffect {
                effect: "minecraft:slowness",
                ..
            }
        ));
        let cow = LootEntity::of(EntityKind::Cow);
        let context = LootContext {
            this: Some(&cow),
            ..context
        };
        assert!(post_attack(&attacker, &[], &context, &mut rng()).is_empty());
    }

    #[test]
    fn flame_ignites_the_projectile_it_spawns() {
        let bow = stack(ItemKind::Bow, &[("minecraft:flame", 1)]);
        let worn = [Worn {
            stack: &bow,
            slot: EquipSlot::Mainhand,
        }];
        let planned = projectile_spawned(&worn, &LootContext::default(), &mut rng());
        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].affected, EffectRole::DamagingEntity);
    }

    #[test]
    fn sweeping_edge_raises_the_sweep_damage_ratio() {
        let sword = stack(ItemKind::DiamondSword, &[("minecraft:sweeping_edge", 3)]);
        let worn = [Worn {
            stack: &sword,
            slot: EquipSlot::Mainhand,
        }];
        assert!((attribute_bonus(&worn, "minecraft:sweeping_damage_ratio") - 0.75).abs() < 1e-6);
        assert_eq!(attribute_bonus(&worn, "minecraft:attack_damage"), 0.0);
    }

    #[test]
    fn looting_adds_to_the_equipment_drop_chance_for_player_attackers() {
        let sword = stack(ItemKind::DiamondSword, &[("minecraft:looting", 3)]);
        let worn = [Worn {
            stack: &sword,
            slot: EquipSlot::Mainhand,
        }];
        let damage = melee(DamageType::PlayerAttack);
        let this = LootEntity::of(EntityKind::Zombie);
        let context = LootContext {
            this: Some(&this),
            damage: Some(&damage),
            ..LootContext::default()
        };
        let chance = equipment_drop_bonus(&worn, 0.085, &context, &mut rng());
        assert!((chance - 0.115).abs() < 1e-6, "{chance}");
    }
}
