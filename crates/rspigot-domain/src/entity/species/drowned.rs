use rspigot_items::{EquipmentSlot, ItemKind};

use crate::entity::living::{LivingState, slot_index};
use crate::entity::spawn::set_item_slot;
use crate::item::ItemStack;
use crate::random::RandomSource;

pub const WATER_SEARCH_TRIES: i32 = 10;
pub const WATER_SEARCH_WIDTH: i32 = 20;
pub const WATER_SEARCH_OFFSET: i32 = 10;
pub const WATER_SEARCH_TOP: i32 = 2;
pub const WATER_SEARCH_HEIGHT: i32 = 8;
pub const BEACH_SEARCH: i32 = 8;
pub const BEACH_VERTICAL: i32 = 2;
pub const BEACH_DEPTH: i32 = 3;
pub const BEACH_RECALCULATE: i32 = 40;
pub const SWIM_UP_START: i32 = 2;
pub const SWIM_UP_HORIZONTAL: i32 = 4;
pub const SWIM_UP_VERTICAL: i32 = 8;
pub const CLOSE_TO_NEXT_SQR: f64 = 4.0;
pub const TRIDENT_CHANCE: f32 = 0.9;
pub const TRIDENT_OVER_ROD: i32 = 10;
pub const EQUIPMENT_ROLL: i32 = 16;
pub const SHELL_CHANCE: f32 = 0.03;
pub const SWIM_SPEED_PUSH: f32 = 0.01;
pub const SWIMMING_FLAG: u8 = 1 << 4;
pub const BUOYANCY: f64 = 0.002;
pub const SINK: f64 = -0.008;
pub const ATTACK_INTERVAL: i32 = 40;
pub const ATTACK_RADIUS: f32 = 10.0;
pub const THROW_SPEED: f32 = 1.6;
pub const AMBIENT_WATER_SOUND: &str = "minecraft:entity.drowned.ambient_water";
pub const AMBIENT_SOUND: &str = "minecraft:entity.drowned.ambient";
pub const SHOOT_SOUND: &str = "minecraft:entity.drowned.shoot";

pub fn populate_equipment(living: &mut LivingState, random: &mut impl RandomSource) {
    if random.next_float() <= TRIDENT_CHANCE {
        return;
    }
    let item = if random.next_int(EQUIPMENT_ROLL) < TRIDENT_OVER_ROD {
        ItemKind::Trident
    } else {
        ItemKind::FishingRod
    };
    set_item_slot(
        living,
        EquipmentSlot::Mainhand,
        Some(ItemStack::new(item.id(), std::num::NonZeroU8::MIN)),
    );
}

pub const NAUTILUS_JOCKEY_CHANCE: f32 = 0.5;
const FREQUENT_BIOMES_TAG: &str = "minecraft:more_frequent_drowned_spawns";
const BIOME_REGISTRY: &str = "minecraft:worldgen/biome";

pub fn is_frequent_drowned_biome(biome: crate::BiomeId) -> bool {
    rspigot_data::tags::tag(BIOME_REGISTRY, FREQUENT_BIOMES_TAG)
        .is_some_and(|ids| ids.binary_search(&u32::from(biome.0)).is_ok())
}

pub fn rolls_nautilus_jockey(
    living: &LivingState,
    baby: bool,
    frequent_biome: bool,
    random: &mut impl RandomSource,
) -> bool {
    living
        .equipment(EquipmentSlot::Mainhand)
        .is_some_and(|held| held.item == ItemKind::Trident.id())
        && random.next_float() < NAUTILUS_JOCKEY_CHANCE
        && !baby
        && !frequent_biome
}

pub fn roll_nautilus_shell(living: &mut LivingState, random: &mut impl RandomSource) {
    if living.equipment(EquipmentSlot::Offhand).is_some() || random.next_float() >= SHELL_CHANCE {
        return;
    }
    set_item_slot(
        living,
        EquipmentSlot::Offhand,
        Some(ItemStack::new(
            ItemKind::NautilusShell.id(),
            std::num::NonZeroU8::MIN,
        )),
    );
    if let Some(mob) = living.mob.as_mut() {
        mob.drop_chances[slot_index(EquipmentSlot::Offhand)] = 2.0;
    }
}

pub fn wants_to_swim(searching_for_land: bool, target_in_water: bool, has_target: bool) -> bool {
    searching_for_land || (has_target && target_in_water)
}

pub fn holds_trident(living: &LivingState) -> bool {
    living
        .equipment(EquipmentSlot::Mainhand)
        .is_some_and(|stack| stack.item == ItemKind::Trident.id())
}

#[cfg(test)]
mod tests {
    use rspigot_entities::EntityKind;
    use uuid::Uuid;

    use super::*;
    use crate::EntityId;
    use crate::entity::base::EntityBase;
    use crate::entity::living::{self, travel};
    use crate::entity::species::Species;
    use crate::random::LegacyRandom;
    use crate::{BlockStateId, Vec3};

    fn drowned() -> (EntityBase, LivingState) {
        let mut base = EntityBase::new(EntityId(1), Uuid::from_u128(1), EntityKind::Drowned);
        base.position = Vec3::new(0.5, 64.0, 0.5);
        let living = LivingState::new(&mut base, 0.0);
        (base, living)
    }

    fn armed(item: ItemKind) -> LivingState {
        let (_, mut living) = drowned();
        set_item_slot(
            &mut living,
            EquipmentSlot::Mainhand,
            Some(ItemStack::new(item.id(), std::num::NonZeroU8::MIN)),
        );
        living
    }

    #[test]
    fn a_trident_drowned_rides_a_zombie_nautilus_half_the_time_outside_frequent_biomes() {
        let living = armed(ItemKind::Trident);
        let mut low = Script(0.49);
        assert!(rolls_nautilus_jockey(&living, false, false, &mut low));
        let mut high = Script(0.5);
        assert!(!rolls_nautilus_jockey(&living, false, false, &mut high));
    }

    #[test]
    fn babies_other_weapons_and_frequent_drowned_biomes_never_ride() {
        let trident = armed(ItemKind::Trident);
        assert!(!rolls_nautilus_jockey(
            &trident,
            true,
            false,
            &mut Script(0.0)
        ));
        assert!(!rolls_nautilus_jockey(
            &trident,
            false,
            true,
            &mut Script(0.0)
        ));
        let rod = armed(ItemKind::FishingRod);
        assert!(!rolls_nautilus_jockey(&rod, false, false, &mut Script(0.0)));
        let (_, bare) = drowned();
        assert!(!rolls_nautilus_jockey(
            &bare,
            false,
            false,
            &mut Script(0.0)
        ));
    }

    #[test]
    fn the_frequent_drowned_biomes_are_river_and_the_like() {
        let river = rspigot_data::registries::biome_id("minecraft:river").unwrap();
        let desert = rspigot_data::registries::biome_id("minecraft:desert").unwrap();
        assert!(is_frequent_drowned_biome(crate::BiomeId(river)));
        assert!(!is_frequent_drowned_biome(crate::BiomeId(desert)));
    }

    struct Script(f32);

    impl RandomSource for Script {
        fn next_float(&mut self) -> f32 {
            self.0
        }
        fn next_int(&mut self, _: i32) -> i32 {
            0
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
    fn a_tenth_of_drowned_hold_a_trident_five_eighths_of_the_time_else_a_fishing_rod() {
        let (mut tridents, mut rods, mut empty) = (0, 0, 0);
        for seed in 0..20_000 {
            let (_, mut living) = drowned();
            let mut random = LegacyRandom::new(seed);
            populate_equipment(&mut living, &mut random);
            match living.equipment(EquipmentSlot::Mainhand).map(|s| s.item) {
                Some(item) if item == ItemKind::Trident.id() => tridents += 1,
                Some(item) if item == ItemKind::FishingRod.id() => rods += 1,
                Some(_) => panic!("unexpected item"),
                None => empty += 1,
            }
        }
        assert!(
            (1_700..2_600).contains(&(tridents + rods)),
            "{tridents} {rods}"
        );
        assert!(tridents > rods, "{tridents} {rods}");
        assert!((17_400..18_300).contains(&empty), "{empty}");
    }

    #[test]
    fn a_nautilus_shell_in_the_off_hand_is_rolled_at_three_percent_and_always_dropped() {
        let mut given = 0;
        for seed in 0..20_000 {
            let (_, mut living) = drowned();
            let mut random = LegacyRandom::new(seed);
            roll_nautilus_shell(&mut living, &mut random);
            if living.equipment(EquipmentSlot::Offhand).is_some() {
                given += 1;
                assert_eq!(
                    living.mob.as_ref().unwrap().drop_chances[slot_index(EquipmentSlot::Offhand)],
                    2.0
                );
            }
        }
        assert!((450..750).contains(&given), "{given}");
        let (_, mut living) = drowned();
        set_item_slot(
            &mut living,
            EquipmentSlot::Offhand,
            Some(ItemStack::new(
                ItemKind::Torch.id(),
                std::num::NonZeroU8::MIN,
            )),
        );
        roll_nautilus_shell(&mut living, &mut LegacyRandom::new(1));
        assert_eq!(
            living.equipment(EquipmentSlot::Offhand).unwrap().item,
            ItemKind::Torch.id()
        );
    }

    #[test]
    fn a_drowned_wants_to_swim_when_searching_for_land_or_chasing_a_target_in_water() {
        assert!(wants_to_swim(true, false, false));
        assert!(wants_to_swim(false, true, true));
        assert!(!wants_to_swim(false, true, false));
        assert!(!wants_to_swim(false, false, true));
    }

    struct Water;

    impl crate::entity::physics::BlockView for Water {
        fn block(&self, _: crate::BlockPos) -> Option<BlockStateId> {
            Some(rspigot_blocks::BlockKind::Water.default_state())
        }
    }

    #[test]
    fn a_submerged_drowned_that_wants_to_swim_pushes_by_a_hundredth_and_ignores_gravity() {
        let (mut base, mut living) = drowned();
        base.fluid.in_water = true;
        base.eye_in_water = true;
        if let Some(Species::Zombie(zombie)) = living.mob.as_mut().map(|mob| &mut mob.species) {
            zombie.searching_for_land = true;
        }
        base.velocity = Vec3::ZERO;
        travel(&mut base, &mut living, &Water, Vec3::new(0.0, 0.0, 1.0));
        assert!((base.velocity.z - 0.01 * 0.9).abs() < 1.0e-9);
        assert_eq!(base.velocity.y, 0.0);
        let (mut calm, mut living) = drowned();
        calm.fluid.in_water = true;
        calm.eye_in_water = true;
        calm.velocity = Vec3::ZERO;
        travel(&mut calm, &mut living, &Water, Vec3::new(0.0, 0.0, 1.0));
        assert!(calm.velocity.z != base.velocity.z);
        let _ = living::health(&calm);
    }
}
