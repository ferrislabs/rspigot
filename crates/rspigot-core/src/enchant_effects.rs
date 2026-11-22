use rand::Rng;
use rspigot_data::enchantments::{
    self, ConditionalEffect, EffectRole, EnchantmentEffects, EntityEffect, Requirement,
    ValueEffect, ValueOp,
};
use rspigot_domain::enchant;
use rspigot_domain::item::ItemStack;

use crate::enchant_scene::{EffectScene, requirement_holds};
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
    pub slot: EquipSlot,
    pub level: i32,
    pub effects: &'static EnchantmentEffects,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Planned {
    pub slot: EquipSlot,
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
                            slot: worn.slot,
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

pub(crate) fn smash_damage_per_fallen_block(
    weapon: &[Worn<'_>],
    context: &LootContext<'_>,
    rng: &mut (impl Rng + ?Sized),
) -> f32 {
    process_value(
        &active_in(weapon),
        |e| e.smash_damage_per_fallen_block,
        0.0,
        context,
        rng,
    )
}

pub(crate) fn armor_effectiveness(
    weapon: &[Worn<'_>],
    context: &LootContext<'_>,
    rng: &mut (impl Rng + ?Sized),
) -> f32 {
    process_value(
        &active_in(weapon),
        |e| e.armor_effectiveness,
        1.0,
        context,
        rng,
    )
    .clamp(0.0, 1.0)
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

fn requirement_ok(
    requirement: Option<&Requirement>,
    level: i32,
    scene: &EffectScene<'_>,
    rng: &mut (impl Rng + ?Sized),
) -> bool {
    let scoped = EffectScene {
        loot: LootContext {
            enchantment_level: level,
            ..scene.loot
        },
        ..*scene
    };
    requirement.is_none_or(|requirement| requirement_holds(requirement, &scoped, rng))
}

pub(crate) fn post_attack_in(
    attacker: &[Worn<'_>],
    victim: &[Worn<'_>],
    scene: &EffectScene<'_>,
    rng: &mut (impl Rng + ?Sized),
) -> Vec<Planned> {
    let mut planned = Vec::new();
    for (role, worn) in [
        (EffectRole::Attacker, attacker),
        (EffectRole::Victim, victim),
    ] {
        for active in active_in(worn) {
            for entry in active.effects.post_attack {
                if entry.enchanted == role
                    && requirement_ok(entry.requirements.as_ref(), active.level, scene, rng)
                {
                    planned.push(Planned {
                        slot: active.slot,
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

pub(crate) fn conditional(
    worn: &[Worn<'_>],
    select: fn(&'static EnchantmentEffects) -> &'static [ConditionalEffect],
    affected: EffectRole,
    scene: &EffectScene<'_>,
    rng: &mut (impl Rng + ?Sized),
) -> Vec<Planned> {
    let mut planned = Vec::new();
    for active in active_in(worn) {
        for ConditionalEffect {
            effect,
            requirements,
        } in select(active.effects)
        {
            if requirement_ok(requirements.as_ref(), active.level, scene, rng) {
                planned.push(Planned {
                    slot: active.slot,
                    enchanted: EffectRole::Attacker,
                    affected,
                    effect,
                    level: active.level,
                });
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
    conditional(
        weapon,
        |e| e.projectile_spawned,
        EffectRole::DamagingEntity,
        &EffectScene::of(context),
        rng,
    )
}

fn active_on_item(stack: &ItemStack) -> Vec<Active> {
    enchant::levels(stack)
        .into_iter()
        .filter_map(|(name, level)| {
            Some(Active {
                slot: EquipSlot::Mainhand,
                level: i32::from(level),
                effects: enchantments::effects(&name)?,
            })
        })
        .collect()
}

fn process_on_item(
    stack: &ItemStack,
    select: fn(&'static EnchantmentEffects) -> &'static [ValueEffect],
    base: f32,
    rng: &mut (impl Rng + ?Sized),
) -> f32 {
    let context = LootContext {
        tool: Some(stack),
        ..LootContext::default()
    };
    process_value(&active_on_item(stack), select, base, &context, rng)
}

pub(crate) fn modify_item_damage(
    stack: &ItemStack,
    amount: i32,
    rng: &mut (impl Rng + ?Sized),
) -> i32 {
    if amount <= 0 {
        return amount;
    }
    (process_on_item(stack, |e| e.item_damage, amount as f32, rng) as i32).max(0)
}

pub(crate) fn modify_repair_from_xp(
    stack: &ItemStack,
    amount: i32,
    rng: &mut (impl Rng + ?Sized),
) -> i32 {
    (process_on_item(stack, |e| e.repair_with_xp, amount as f32, rng) as i32).max(0)
}

pub(crate) fn repairs_with_xp(stack: &ItemStack) -> bool {
    active_on_item(stack)
        .iter()
        .any(|active| !active.effects.repair_with_xp.is_empty())
}

pub(crate) fn modify_block_experience(
    tool: Option<&ItemStack>,
    amount: i32,
    rng: &mut (impl Rng + ?Sized),
) -> i32 {
    let Some(tool) = tool else {
        return amount;
    };
    (process_on_item(tool, |e| e.block_experience, amount as f32, rng) as i32).max(0)
}

pub(crate) fn modify_mob_experience(
    attacker: &[Worn<'_>],
    amount: i32,
    context: &LootContext<'_>,
    rng: &mut (impl Rng + ?Sized),
) -> i32 {
    (process_value(
        &active_in(attacker),
        |e| e.mob_experience,
        amount as f32,
        context,
        rng,
    ) as i32)
        .max(0)
}

pub(crate) fn fishing_luck_bonus(rod: &ItemStack, rng: &mut (impl Rng + ?Sized)) -> f32 {
    process_on_item(rod, |e| e.fishing_luck_bonus, 0.0, rng)
}

pub(crate) fn fishing_time_reduction(rod: &ItemStack, rng: &mut (impl Rng + ?Sized)) -> f32 {
    process_on_item(rod, |e| e.fishing_time_reduction, 0.0, rng)
}

pub(crate) fn trident_return_acceleration(
    trident: &ItemStack,
    rng: &mut (impl Rng + ?Sized),
) -> f32 {
    process_on_item(trident, |e| e.trident_return_acceleration, 0.0, rng)
}

pub(crate) fn damage_immune(
    equipment: &[Worn<'_>],
    context: &LootContext<'_>,
    rng: &mut (impl Rng + ?Sized),
) -> bool {
    active_in(equipment).iter().any(|active| {
        let scoped = LootContext {
            enchantment_level: active.level,
            ..*context
        };
        active.effects.damage_immunity.iter().any(|immunity| {
            immunity
                .requirements
                .as_ref()
                .is_none_or(|requirements| condition_holds(requirements, &scoped, rng))
        })
    })
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

    fn post_attack(
        attacker: &[Worn<'_>],
        victim: &[Worn<'_>],
        context: &LootContext<'_>,
        rng: &mut (impl Rng + ?Sized),
    ) -> Vec<Planned> {
        post_attack_in(attacker, victim, &EffectScene::of(context), rng)
    }

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

    fn worn(stack: &ItemStack, slot: EquipSlot) -> [Worn<'_>; 1] {
        [Worn { stack, slot }]
    }

    #[test]
    fn unbreaking_spares_tools_and_armor_at_the_datapack_odds() {
        let sword = stack(ItemKind::DiamondSword, &[("minecraft:unbreaking", 3)]);
        let helmet = stack(ItemKind::DiamondHelmet, &[("minecraft:unbreaking", 3)]);
        let plain = stack(ItemKind::DiamondSword, &[]);
        let mut random = rng();
        let mut worn_sword = 0;
        let mut worn_helmet = 0;
        for _ in 0..4_000 {
            worn_sword += modify_item_damage(&sword, 1, &mut random);
            worn_helmet += modify_item_damage(&helmet, 1, &mut random);
        }
        assert!((850..1150).contains(&worn_sword), "{worn_sword}");
        assert!((2600..3000).contains(&worn_helmet), "{worn_helmet}");
        assert_eq!(modify_item_damage(&plain, 5, &mut random), 5);
        assert_eq!(modify_item_damage(&sword, 0, &mut random), 0);
    }

    #[test]
    fn mending_doubles_the_durability_an_orb_repairs() {
        let pick = stack(ItemKind::DiamondPickaxe, &[("minecraft:mending", 1)]);
        let plain = stack(ItemKind::DiamondPickaxe, &[]);
        assert!(repairs_with_xp(&pick));
        assert!(!repairs_with_xp(&plain));
        assert_eq!(modify_repair_from_xp(&pick, 5, &mut rng()), 10);
        assert_eq!(modify_repair_from_xp(&plain, 5, &mut rng()), 5);
    }

    #[test]
    fn silk_touch_sets_the_block_experience_to_zero() {
        let silk = stack(ItemKind::DiamondPickaxe, &[("minecraft:silk_touch", 1)]);
        let plain = stack(ItemKind::DiamondPickaxe, &[]);
        assert_eq!(modify_block_experience(Some(&silk), 7, &mut rng()), 0);
        assert_eq!(modify_block_experience(Some(&plain), 7, &mut rng()), 7);
        assert_eq!(modify_block_experience(None, 7, &mut rng()), 7);
    }

    #[test]
    fn mob_experience_follows_the_component_of_any_enchantment_that_carries_it() {
        let mending = enchantments::effects("minecraft:mending").unwrap();
        let carrier: &'static EnchantmentEffects = Box::leak(Box::new(EnchantmentEffects {
            mob_experience: &[ValueEffect {
                op: ValueOp::Multiply(rspigot_data::loot::LevelBasedValue::Constant(2.0)),
                requirements: None,
            }],
            ..*mending
        }));
        let actives = [Active {
            slot: EquipSlot::Mainhand,
            level: 1,
            effects: carrier,
        }];
        let value = process_value(
            &actives,
            |e| e.mob_experience,
            10.0,
            &LootContext::default(),
            &mut rng(),
        );
        assert_eq!(value, 20.0);
        let sword = stack(ItemKind::DiamondSword, &[("minecraft:looting", 3)]);
        assert_eq!(
            modify_mob_experience(
                &worn(&sword, EquipSlot::Mainhand),
                10,
                &LootContext::default(),
                &mut rng()
            ),
            10
        );
    }

    #[test]
    fn luck_of_the_sea_and_lure_feed_the_fishing_hook() {
        let rod = stack(
            ItemKind::FishingRod,
            &[("minecraft:luck_of_the_sea", 3), ("minecraft:lure", 3)],
        );
        let plain = stack(ItemKind::FishingRod, &[]);
        assert_eq!(fishing_luck_bonus(&rod, &mut rng()), 3.0);
        assert_eq!(fishing_time_reduction(&rod, &mut rng()), 15.0);
        assert_eq!(fishing_luck_bonus(&plain, &mut rng()), 0.0);
        assert_eq!(fishing_time_reduction(&plain, &mut rng()), 0.0);
    }

    #[test]
    fn loyalty_accelerates_the_returning_trident_by_its_level() {
        let trident = stack(ItemKind::Trident, &[("minecraft:loyalty", 3)]);
        assert_eq!(trident_return_acceleration(&trident, &mut rng()), 3.0);
        let plain = stack(ItemKind::Trident, &[]);
        assert_eq!(trident_return_acceleration(&plain, &mut rng()), 0.0);
    }

    #[test]
    fn frost_walker_makes_the_wearer_immune_to_hot_floor_only() {
        let boots = stack(ItemKind::DiamondBoots, &[("minecraft:frost_walker", 2)]);
        let bare = stack(ItemKind::DiamondBoots, &[]);
        let this = LootEntity::of(EntityKind::Player);
        let facts = |kind| LootDamage {
            kind,
            source: None,
            direct: None,
            is_direct: false,
        };
        let hot = facts(DamageType::HotFloor);
        let burning = facts(DamageType::OnFire);
        let context = |damage| LootContext {
            this: Some(&this),
            damage: Some(damage),
            ..LootContext::default()
        };
        let feet = worn(&boots, EquipSlot::Feet);
        assert!(damage_immune(&feet, &context(&hot), &mut rng()));
        assert!(!damage_immune(&feet, &context(&burning), &mut rng()));
        assert!(!damage_immune(
            &worn(&bare, EquipSlot::Feet),
            &context(&hot),
            &mut rng()
        ));
    }

    fn standing() -> crate::enchant_scene::EntityFacts {
        crate::enchant_scene::EntityFacts {
            kind: Some("minecraft:player"),
            on_ground: true,
            player: Some(crate::enchant_scene::PlayerFacts {
                gamemode: "survival",
                food: 20,
            }),
            ..crate::enchant_scene::EntityFacts::default()
        }
    }

    fn planned_in(
        stack: &ItemStack,
        slot: EquipSlot,
        select: fn(&'static EnchantmentEffects) -> &'static [ConditionalEffect],
        scene: &EffectScene<'_>,
    ) -> Vec<Planned> {
        conditional(
            &worn(stack, slot),
            select,
            EffectRole::Attacker,
            scene,
            &mut rng(),
        )
    }

    #[test]
    fn channeling_summons_lightning_when_a_trident_hits_a_lightning_rod_in_a_thunderstorm() {
        use crate::enchant_scene::{EntityFacts, LocationFacts};
        let trident = stack(ItemKind::Trident, &[("minecraft:channeling", 1)]);
        let projectile = EntityFacts {
            kind: Some("minecraft:trident"),
            ..EntityFacts::default()
        };
        let rod = Some(u32::from(rspigot_blocks::BlockKind::LightningRod.id().0));
        let scene = |thunder: bool, block, sees_sky| EffectScene {
            loot: LootContext {
                weather: Some((thunder, thunder)),
                ..LootContext::default()
            },
            this: Some(&projectile),
            location: Some(LocationFacts { block, sees_sky }),
            active: true,
            ..EffectScene::default()
        };
        let hit = |scene: &EffectScene<'_>| {
            planned_in(&trident, EquipSlot::Mainhand, |e| e.hit_block, scene)
        };
        let planned = hit(&scene(true, rod, true));
        assert_eq!(planned.len(), 1);
        let EntityEffect::AllOf(parts) = planned[0].effect else {
            panic!("all of")
        };
        assert!(matches!(
            parts[0],
            EntityEffect::SummonEntity {
                entity: "minecraft:lightning_bolt",
                ..
            }
        ));
        assert!(hit(&scene(false, rod, true)).is_empty());
        assert!(hit(&scene(true, None, true)).is_empty());
        assert!(hit(&scene(true, rod, false)).is_empty());
    }

    #[test]
    fn channeling_strikes_a_struck_victim_in_a_thunderstorm_when_the_attacker_is_a_trident() {
        use crate::enchant_scene::EntityFacts;
        let trident = stack(ItemKind::Trident, &[("minecraft:channeling", 1)]);
        let victim = EntityFacts {
            sees_sky: true,
            ..EntityFacts::default()
        };
        let thrown = EntityFacts {
            kind: Some("minecraft:trident"),
            ..EntityFacts::default()
        };
        let melee = EntityFacts {
            kind: Some("minecraft:player"),
            ..EntityFacts::default()
        };
        let scene = |direct, thunder| EffectScene {
            loot: LootContext {
                weather: Some((thunder, thunder)),
                ..LootContext::default()
            },
            this: Some(&victim),
            direct_attacker: Some(direct),
            active: true,
            ..EffectScene::default()
        };
        let held = worn(&trident, EquipSlot::Mainhand);
        let planned = post_attack_in(&held, &[], &scene(&thrown, true), &mut rng());
        assert_eq!(planned.len(), 1);
        assert!(matches!(planned[0].effect, EntityEffect::AllOf(_)));
        assert!(post_attack_in(&held, &[], &scene(&thrown, false), &mut rng()).is_empty());
        assert!(post_attack_in(&held, &[], &scene(&melee, true), &mut rng()).is_empty());
    }

    #[test]
    fn wind_burst_explodes_for_a_falling_attacker_that_is_not_flying() {
        use crate::enchant_scene::EntityFacts;
        let mace = stack(ItemKind::Mace, &[("minecraft:wind_burst", 2)]);
        let falling = |flying, fall_distance| EntityFacts {
            flying,
            fall_distance,
            ..standing()
        };
        let victim = EntityFacts::default();
        let check = |attacker: &EntityFacts| {
            let scene = EffectScene {
                this: Some(&victim),
                attacker: Some(attacker),
                direct_attacker: Some(attacker),
                active: true,
                ..EffectScene::default()
            };
            post_attack_in(&worn(&mace, EquipSlot::Mainhand), &[], &scene, &mut rng())
        };
        let planned = check(&falling(false, 2.0));
        assert_eq!(planned.len(), 1);
        let EntityEffect::Explode {
            knockback_multiplier,
            ..
        } = planned[0].effect
        else {
            panic!("explode")
        };
        assert_eq!(knockback_multiplier.calculate(planned[0].level), 1.75);
        assert!(check(&falling(true, 2.0)).is_empty());
        assert!(check(&falling(false, 1.0)).is_empty());
    }

    #[test]
    fn lunge_pushes_and_tires_a_grounded_wielder_with_enough_food() {
        let spear = stack(ItemKind::DiamondSpear, &[("minecraft:lunge", 2)]);
        let scene_of = |facts: &crate::enchant_scene::EntityFacts| {
            let planned = planned_in(
                &spear,
                EquipSlot::Mainhand,
                |e| e.post_piercing_attack,
                &EffectScene {
                    this: Some(facts),
                    active: true,
                    ..EffectScene::default()
                },
            );
            planned.len()
        };
        assert_eq!(scene_of(&standing()), 1);
        let mut swimming = standing();
        swimming.in_water = true;
        assert_eq!(scene_of(&swimming), 0);
        let mut riding = standing();
        riding.has_vehicle = true;
        assert_eq!(scene_of(&riding), 0);
        let mut hungry = standing();
        hungry.player = Some(crate::enchant_scene::PlayerFacts {
            gamemode: "survival",
            food: 6,
        });
        assert_eq!(scene_of(&hungry), 0);
        let mut creative = hungry;
        creative.player = Some(crate::enchant_scene::PlayerFacts {
            gamemode: "creative",
            food: 0,
        });
        assert_eq!(scene_of(&creative), 1);
        let planned = planned_in(
            &spear,
            EquipSlot::Mainhand,
            |e| e.post_piercing_attack,
            &EffectScene {
                this: Some(&standing()),
                active: true,
                ..EffectScene::default()
            },
        );
        let EntityEffect::AllOf(parts) = planned[0].effect else {
            panic!("all of")
        };
        let magnitude = parts.iter().find_map(|part| match part {
            EntityEffect::ApplyImpulse { magnitude, .. } => Some(magnitude.calculate(2)),
            _ => None,
        });
        assert!((magnitude.unwrap() - 0.916).abs() < 1e-5);
    }

    #[test]
    fn frost_walker_freezes_only_for_a_grounded_wielder_without_a_vehicle() {
        let boots = stack(ItemKind::DiamondBoots, &[("minecraft:frost_walker", 1)]);
        let scene_of = |facts: &crate::enchant_scene::EntityFacts| {
            planned_in(
                &boots,
                EquipSlot::Feet,
                |e| e.location_changed,
                &EffectScene {
                    this: Some(facts),
                    active: true,
                    ..EffectScene::default()
                },
            )
        };
        let planned = scene_of(&standing());
        assert_eq!(planned.len(), 1);
        assert!(matches!(
            planned[0].effect,
            EntityEffect::ReplaceDisk {
                block: "minecraft:frosted_ice",
                ..
            }
        ));
        let mut airborne = standing();
        airborne.on_ground = false;
        assert!(scene_of(&airborne).is_empty());
        let mut riding = standing();
        riding.has_vehicle = true;
        assert!(scene_of(&riding).is_empty());
    }

    #[test]
    fn soul_speed_grants_its_boost_on_soul_blocks_and_ticks_particles_every_five_ticks() {
        let boots = stack(ItemKind::DiamondBoots, &[("minecraft:soul_speed", 2)]);
        let soul_sand = u32::from(rspigot_blocks::BlockKind::SoulSand.id().0);
        let stone = u32::from(rspigot_blocks::BlockKind::Stone.id().0);
        let on = |block, age, active| {
            let mut facts = standing();
            facts.movement_block = Some(block);
            facts.age = age;
            facts.horizontal_speed = 0.1;
            (facts, active)
        };
        let boosts = |block, active| {
            let (facts, active) = on(block, 10, active);
            planned_in(
                &boots,
                EquipSlot::Feet,
                |e| e.location_changed,
                &EffectScene {
                    this: Some(&facts),
                    active,
                    ..EffectScene::default()
                },
            )
            .iter()
            .filter(|plan| matches!(plan.effect, EntityEffect::AllOf(_)))
            .count()
        };
        assert_eq!(boosts(soul_sand, true), 1);
        assert_eq!(boosts(stone, true), 0);
        let particles = |age| {
            let (facts, _) = on(soul_sand, age, true);
            planned_in(
                &boots,
                EquipSlot::Feet,
                |e| e.tick,
                &EffectScene {
                    this: Some(&facts),
                    active: true,
                    ..EffectScene::default()
                },
            )
            .iter()
            .any(|plan| matches!(plan.effect, EntityEffect::SpawnParticles { .. }))
        };
        assert!(particles(10));
        assert!(!particles(12));
    }
}
