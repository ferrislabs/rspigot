use rspigot_entities::EntityKind;

use rspigot_items::EquipmentSlot;

use super::Entity;
use super::living::{EQUIPMENT_SLOTS, LivingState, slot_index};
use crate::item::ItemStack;
use crate::random::RandomSource;

const MONSTER_REWARD: i32 = 5;
const BABY_ZOMBIE_FACTOR: f64 = 2.5;
const MAX_CUBE_SIZE: i32 = 127;
const PRESERVED_CHANCE: f32 = 1.0;

fn xp_reward(entity: &Entity) -> i32 {
    let kind = entity.base.kind;
    match kind {
        EntityKind::Blaze
        | EntityKind::Guardian
        | EntityKind::ElderGuardian
        | EntityKind::Breeze
        | EntityKind::Evoker => 10,
        EntityKind::Wither => 50,
        EntityKind::PiglinBrute | EntityKind::Ravager => 20,
        EntityKind::Vex | EntityKind::Endermite => 3,
        EntityKind::Creaking => 0,
        EntityKind::Phantom | EntityKind::Ghast | EntityKind::Shulker => MONSTER_REWARD,
        EntityKind::Slime | EntityKind::MagmaCube => cube_size(entity).clamp(1, MAX_CUBE_SIZE),
        EntityKind::Hoglin => {
            if entity.is_baby() {
                3
            } else {
                MONSTER_REWARD
            }
        }
        _ if kind.extends("Monster") => MONSTER_REWARD,
        _ => 0,
    }
}

pub fn cube_size(entity: &Entity) -> i32 {
    super::species::cube::size(&entity.base)
}

fn animal_like(kind: EntityKind) -> bool {
    kind.extends("Animal") || kind.extends("WaterAnimal") || kind.extends("AgeableWaterCreature")
}

pub fn base_experience_reward(entity: &Entity, random: &mut impl RandomSource) -> i32 {
    let kind = entity.base.kind;
    match kind {
        EntityKind::Hoglin | EntityKind::Piglin => return xp_reward(entity),
        EntityKind::SulfurCube => {
            return if entity.is_baby() {
                0
            } else {
                1 + random.next_int(2)
            };
        }
        _ if animal_like(kind) => return 1 + random.next_int(3),
        _ => {}
    }
    let mut reward = xp_reward(entity);
    if kind.extends("Zombie") && entity.is_baby() {
        reward = (f64::from(reward) * BABY_ZOMBIE_FACTOR) as i32;
    }
    let Some(living) = entity.living() else {
        return reward;
    };
    let Some(mob) = &living.mob else {
        return reward;
    };
    if reward > 0 {
        for slot in EQUIPMENT_SLOTS {
            if slot == rspigot_items::EquipmentSlot::Saddle {
                continue;
            }
            if living.equipment(slot).is_some()
                && mob.drop_chances[slot_index(slot)] <= PRESERVED_CHANCE
            {
                reward += 1 + random.next_int(3);
            }
        }
    }
    reward
}

pub const DAMAGE_COMPONENT: &str = "minecraft:damage";

pub fn max_damage(stack: &ItemStack) -> Option<i32> {
    if stack.components.contains_key("minecraft:unbreakable") {
        return None;
    }
    stack
        .item
        .kind()
        .and_then(|kind| kind.max_damage())
        .and_then(|max| i32::try_from(max).ok())
}

pub fn damage_value(stack: &ItemStack) -> i32 {
    stack.components.get_i32(DAMAGE_COMPONENT).unwrap_or(0)
}

pub fn set_damage_value(stack: &mut ItemStack, value: i32) {
    let max = max_damage(stack).unwrap_or(0);
    let value = value.clamp(0, max);
    if value == 0 {
        stack.components.remove(DAMAGE_COMPONENT);
    } else {
        stack.components.insert(DAMAGE_COMPONENT, value);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EquipmentDrop {
    pub slot: EquipmentSlot,
    pub original: ItemStack,
    pub dropped: ItemStack,
}

pub fn equipment_drops(
    living: &mut LivingState,
    killed_by_player: bool,
    random: &mut impl RandomSource,
) -> Vec<EquipmentDrop> {
    let drops = planned_equipment_drops(living, killed_by_player, random);
    for drop in &drops {
        living.equipment[slot_index(drop.slot)] = None;
    }
    drops
}

pub fn planned_equipment_drops(
    living: &LivingState,
    killed_by_player: bool,
    random: &mut impl RandomSource,
) -> Vec<EquipmentDrop> {
    let Some(chances) = living.mob.as_ref().map(|mob| mob.drop_chances) else {
        return Vec::new();
    };
    let mut drops = Vec::new();
    for slot in EQUIPMENT_SLOTS {
        let chance = chances[slot_index(slot)];
        if chance == 0.0 {
            continue;
        }
        let preserved = chance > PRESERVED_CHANCE;
        if living.equipment(slot).is_none() {
            continue;
        }
        if !killed_by_player && !preserved {
            continue;
        }
        if random.next_float() >= chance {
            continue;
        }
        let Some(original) = living.equipment[slot_index(slot)].clone() else {
            continue;
        };
        let mut dropped = original.clone();
        if !preserved && let Some(max) = max_damage(&dropped) {
            let inner = random.next_int((max - 3).max(1));
            let value = max - random.next_int(1 + inner);
            set_damage_value(&mut dropped, value);
        }
        drops.push(EquipmentDrop {
            slot,
            original,
            dropped,
        });
    }
    drops
}

pub fn should_drop_experience(entity: &Entity) -> bool {
    let kind = entity.base.kind;
    if kind == EntityKind::Tadpole {
        return false;
    }
    kind.extends("Monster")
        || matches!(kind, EntityKind::Hoglin | EntityKind::ArmorStand)
        || !entity.is_baby()
}

pub fn should_drop_loot(entity: &Entity, mob_drops: bool) -> bool {
    mob_drops && (entity.base.kind.extends("Monster") || !entity.is_baby())
}

#[cfg(test)]
mod tests {
    use rspigot_nbt::Compound;
    use uuid::Uuid;

    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    use super::*;
    use crate::EntityId;

    fn mob(kind: EntityKind, extra: Compound) -> Entity {
        let nbt = extra.with("id", kind.name());
        super::super::save::load_entity(&nbt, EntityId(1), Uuid::from_u128(1)).unwrap()
    }

    struct Scripted<F, I> {
        float: F,
        int: I,
    }

    impl<F: FnMut() -> f32, I: FnMut(i32) -> i32> RandomSource for Scripted<F, I> {
        fn next_float(&mut self) -> f32 {
            (self.float)()
        }

        fn next_int(&mut self, bound: i32) -> i32 {
            (self.int)(bound)
        }

        fn next_double(&mut self) -> f64 {
            f64::from((self.float)())
        }

        fn next_boolean(&mut self) -> bool {
            (self.int)(2) != 0
        }

        fn next_gaussian(&mut self) -> f64 {
            0.0
        }
    }

    fn fixed(value: i32) -> impl RandomSource {
        Scripted {
            float: || 0.0,
            int: move |bound: i32| value.min(bound - 1),
        }
    }

    #[test]
    fn animals_give_one_to_three_and_monsters_five() {
        let cow = mob(EntityKind::Cow, Compound::new());
        assert_eq!(base_experience_reward(&cow, &mut fixed(0)), 1);
        assert_eq!(base_experience_reward(&cow, &mut fixed(2)), 3);
        let zombie = mob(EntityKind::Zombie, Compound::new());
        assert_eq!(base_experience_reward(&zombie, &mut fixed(2)), 5);
        let blaze = mob(EntityKind::Blaze, Compound::new());
        assert_eq!(base_experience_reward(&blaze, &mut fixed(0)), 10);
        let villager = mob(EntityKind::Villager, Compound::new());
        assert_eq!(base_experience_reward(&villager, &mut fixed(0)), 0);
    }

    #[test]
    fn worn_equipment_adds_one_to_three_each_unless_guaranteed() {
        let mut zombie = mob(EntityKind::Zombie, Compound::new());
        let helmet = ItemStack::new(
            rspigot_items::ItemKind::IronHelmet.id(),
            std::num::NonZeroU8::MIN,
        );
        let sword = ItemStack::new(
            rspigot_items::ItemKind::IronSword.id(),
            std::num::NonZeroU8::MIN,
        );
        if let Some(living) = zombie.living_mut() {
            living.equipment[slot_index(rspigot_items::EquipmentSlot::Head)] = Some(helmet);
            living.equipment[slot_index(rspigot_items::EquipmentSlot::Mainhand)] = Some(sword);
            if let Some(state) = &mut living.mob {
                state.drop_chances[slot_index(rspigot_items::EquipmentSlot::Mainhand)] = 2.0;
            }
        }
        assert_eq!(base_experience_reward(&zombie, &mut fixed(2)), 5 + 3);
    }

    fn geared_zombie() -> Entity {
        let mut zombie = mob(EntityKind::Zombie, Compound::new());
        if let Some(living) = zombie.living_mut() {
            living.equipment[slot_index(EquipmentSlot::Head)] = Some(ItemStack::new(
                rspigot_items::ItemKind::IronHelmet.id(),
                std::num::NonZeroU8::MIN,
            ));
        }
        zombie
    }

    #[test]
    fn equipment_drops_at_eight_and_a_half_percent_when_killed_by_a_player() {
        let mut rng = StdRng::seed_from_u64(11);
        let mut dropped = 0;
        for _ in 0..20_000 {
            let mut zombie = geared_zombie();
            let living = zombie.living_mut().unwrap();
            let mut values = Vec::new();
            let drops = equipment_drops(
                living,
                true,
                &mut Scripted {
                    float: || rng.random::<f32>(),
                    int: |bound: i32| {
                        values.push(bound);
                        bound - 1
                    },
                },
            );
            if let [drop] = drops.as_slice() {
                dropped += 1;
                assert_eq!(drop.slot, EquipmentSlot::Head);
                assert_eq!(values, vec![162, 162]);
                assert_eq!(damage_value(&drop.dropped), 165 - 161);
                assert_eq!(damage_value(&drop.original), 0);
                assert!(living.equipment(EquipmentSlot::Head).is_none());
            }
        }
        assert!((1_550..=1_850).contains(&dropped), "{dropped}");
        let mut zombie = geared_zombie();
        let living = zombie.living_mut().unwrap();
        assert!(equipment_drops(living, false, &mut fixed(0)).is_empty());
    }

    #[test]
    fn guaranteed_drops_ignore_the_killer_and_keep_durability() {
        let mut zombie = geared_zombie();
        let living = zombie.living_mut().unwrap();
        living.mob.as_mut().unwrap().drop_chances[slot_index(EquipmentSlot::Head)] = 2.0;
        let mut random = Scripted {
            float: || 0.99,
            int: |_: i32| -> i32 { panic!() },
        };
        let drops = equipment_drops(living, false, &mut random);
        assert_eq!(drops.len(), 1);
        assert_eq!(damage_value(&drops[0].dropped), 0);
    }

    #[test]
    fn babies_change_rewards_and_drops() {
        let baby_zombie = mob(EntityKind::Zombie, Compound::new().with("IsBaby", true));
        assert_eq!(base_experience_reward(&baby_zombie, &mut fixed(0)), 12);
        assert!(should_drop_experience(&baby_zombie));
        assert!(should_drop_loot(&baby_zombie, true));
        let calf = mob(EntityKind::Cow, Compound::new().with("Age", -24000));
        assert!(!should_drop_experience(&calf));
        assert!(!should_drop_loot(&calf, true));
        assert!(!should_drop_loot(
            &mob(EntityKind::Cow, Compound::new()),
            false
        ));
        let big_slime = mob(EntityKind::Slime, Compound::new().with("Size", 3));
        assert_eq!(base_experience_reward(&big_slime, &mut fixed(0)), 4);
        assert!(!should_drop_experience(&mob(
            EntityKind::Tadpole,
            Compound::new()
        )));
    }
}
