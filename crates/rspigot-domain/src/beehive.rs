use rspigot_nbt::{Compound, Tag};

use crate::BlockPos;
use crate::random::RandomSource;

pub const ENTITY: &str = "beehive";
pub const MAX_BEES: i32 = 3;
pub const MIN_TICKS_WITH_NECTAR: i32 = 2400;
pub const MIN_TICKS_WITHOUT_NECTAR: i32 = 600;
pub const MAX_HONEY_LEVEL: i32 = 5;
pub const DOUBLE_HONEY_ODDS: i32 = 100;
pub const WORK_SOUND_CHANCE: f64 = 0.005;
pub const SEDATED_STAY_OUT_TICKS: i32 = 400;
pub const ANGRY_PLAYER_DISTANCE_SQR: f64 = 16.0;
pub const FLOWER_KEEP_ODDS: f32 = 0.9;
pub const EXIT_OFFSET: f64 = 0.55;
pub const ANGER_RADIUS: (f64, f64, f64) = (8.0, 6.0, 8.0);
pub const ENTER_SOUND: &str = "minecraft:block.beehive.enter";
pub const EXIT_SOUND: &str = "minecraft:block.beehive.exit";
pub const WORK_SOUND: &str = "minecraft:block.beehive.work";
pub const SHEAR_SOUND: &str = "minecraft:block.beehive.shear";
pub const BOTTLE_FILL_SOUND: &str = "minecraft:item.bottle.fill";
pub const HONEYCOMB_LOOT: &str = "minecraft:harvest/beehive";
pub const MAX_ENTITIES_KEY: &str = "Bukkit.MaxEntities";
pub const IGNORED_BEE_TAGS: [&str; 25] = [
    "Air",
    "drop_chances",
    "equipment",
    "Brain",
    "CanPickUpLoot",
    "DeathTime",
    "fall_distance",
    "FallFlying",
    "Fire",
    "HurtTime",
    "LeftHanded",
    "Motion",
    "NoGravity",
    "OnGround",
    "PortalCooldown",
    "Pos",
    "Rotation",
    "sleeping_pos",
    "CannotEnterHiveTicks",
    "TicksSincePollination",
    "CropsGrownSincePollination",
    "hive_pos",
    "Passengers",
    "leash",
    "UUID",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseStatus {
    HoneyDelivered,
    BeeReleased,
    Emergency,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Occupant {
    pub entity_data: Compound,
    pub ticks_in_hive: i32,
    pub min_ticks_in_hive: i32,
}

impl Occupant {
    pub fn of(entity: &Compound) -> Self {
        let mut data = entity.clone();
        for key in IGNORED_BEE_TAGS {
            data.remove(key);
        }
        let nectar = data.get_bool("HasNectar").unwrap_or(false);
        Self {
            entity_data: data,
            ticks_in_hive: 0,
            min_ticks_in_hive: if nectar {
                MIN_TICKS_WITH_NECTAR
            } else {
                MIN_TICKS_WITHOUT_NECTAR
            },
        }
    }

    pub fn create(ticks_in_hive: i32) -> Self {
        Self {
            entity_data: Compound::new().with("id", "minecraft:bee"),
            ticks_in_hive,
            min_ticks_in_hive: MIN_TICKS_WITHOUT_NECTAR,
        }
    }

    pub fn has_nectar(&self) -> bool {
        self.entity_data.get_bool("HasNectar").unwrap_or(false)
    }

    pub fn entity_tag(&self) -> Compound {
        let mut tag = self.entity_data.clone();
        for key in IGNORED_BEE_TAGS {
            tag.remove(key);
        }
        tag
    }

    pub fn tick(&mut self) -> bool {
        let due = self.min_ticks_in_hive < self.ticks_in_hive;
        self.ticks_in_hive += 1;
        due
    }

    pub fn status(&self) -> ReleaseStatus {
        if self.has_nectar() {
            ReleaseStatus::HoneyDelivered
        } else {
            ReleaseStatus::BeeReleased
        }
    }

    fn read(tag: &Compound) -> Option<Self> {
        let entity_data = tag.get_compound("entity_data")?.clone();
        Some(Self {
            entity_data,
            ticks_in_hive: tag.get_i32("ticks_in_hive").unwrap_or(0),
            min_ticks_in_hive: tag.get_i32("min_ticks_in_hive").unwrap_or(0),
        })
    }

    fn write(&self) -> Compound {
        Compound::new()
            .with("entity_data", self.entity_data.clone())
            .with("ticks_in_hive", self.ticks_in_hive)
            .with("min_ticks_in_hive", self.min_ticks_in_hive)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Beehive {
    pub bees: Vec<Occupant>,
    pub flower_pos: Option<BlockPos>,
    pub max_bees: i32,
}

impl Default for Beehive {
    fn default() -> Self {
        Self {
            bees: Vec::new(),
            flower_pos: None,
            max_bees: MAX_BEES,
        }
    }
}

impl Beehive {
    pub fn is_empty(&self) -> bool {
        self.bees.is_empty()
    }

    pub fn is_full(&self) -> bool {
        self.bees.len() as i32 == self.max_bees
    }

    pub fn has_space(&self) -> bool {
        (self.bees.len() as i32) < self.max_bees
    }

    pub fn occupant_count(&self) -> usize {
        self.bees.len()
    }

    pub fn store(&mut self, occupant: Occupant) {
        self.bees.push(occupant);
    }

    pub fn adopt_flower(&mut self, bee_flower: Option<BlockPos>, random: &mut impl RandomSource) {
        if let Some(flower) = bee_flower
            && (self.flower_pos.is_none() || random.next_boolean())
        {
            self.flower_pos = Some(flower);
        }
    }

    pub fn tick_occupants(&mut self) -> Vec<usize> {
        let mut due = Vec::new();
        for (index, occupant) in self.bees.iter_mut().enumerate() {
            if occupant.tick() {
                due.push(index);
            }
        }
        due
    }

    pub fn defer(&mut self, index: usize) {
        if let Some(occupant) = self.bees.get_mut(index) {
            occupant.ticks_in_hive = occupant.min_ticks_in_hive / 2;
        }
    }

    pub fn remove_all(&mut self, indices: &[usize]) {
        let mut index = 0;
        self.bees.retain(|_| {
            let keep = !indices.contains(&index);
            index += 1;
            keep
        });
    }

    pub fn read(data: &Compound) -> Self {
        let bees = data
            .get_list("bees")
            .map(|list| {
                list.iter()
                    .filter_map(|tag| tag.as_compound().and_then(Occupant::read))
                    .collect()
            })
            .unwrap_or_default();
        let flower_pos = data
            .get_int_array("flower_pos")
            .filter(|values| values.len() == 3)
            .map(|values| BlockPos::new(values[0], values[1], values[2]));
        Self {
            bees,
            flower_pos,
            max_bees: data.get_i32(MAX_ENTITIES_KEY).unwrap_or(MAX_BEES),
        }
    }

    pub fn write(&self, data: &mut Compound) {
        data.insert(
            "bees",
            Tag::List(
                self.bees
                    .iter()
                    .map(|occupant| Tag::Compound(occupant.write()))
                    .collect(),
            ),
        );
        match self.flower_pos {
            Some(pos) => {
                data.insert("flower_pos", Tag::IntArray(vec![pos.x, pos.y, pos.z]));
            }
            None => {
                data.remove("flower_pos");
            }
        }
        data.insert(MAX_ENTITIES_KEY, self.max_bees);
    }
}

pub fn honey_after_delivery(honey: i32, random: &mut impl RandomSource) -> Option<i32> {
    if honey >= MAX_HONEY_LEVEL {
        return None;
    }
    let mut step = if random.next_int(DOUBLE_HONEY_ODDS) == 0 {
        2
    } else {
        1
    };
    if honey + step > MAX_HONEY_LEVEL {
        step -= 1;
    }
    Some(honey + step)
}

pub fn exit_offset(blocked: bool, width: f32) -> f64 {
    if blocked {
        0.0
    } else {
        EXIT_OFFSET + f64::from(width / 2.0)
    }
}

pub fn age_after_stay(age: i32, locked: bool, ticks_in_hive: i32) -> i32 {
    if locked {
        age
    } else if age < 0 {
        0.min(age + ticks_in_hive)
    } else if age > 0 {
        0.max(age - ticks_in_hive)
    } else {
        age
    }
}

pub fn love_after_stay(love: i32, ticks_in_hive: i32) -> i32 {
    0.max(love - ticks_in_hive)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::random::LegacyRandom;

    fn bee_tag(nectar: bool) -> Compound {
        Compound::new()
            .with("id", "minecraft:bee")
            .with("HasNectar", nectar)
            .with(
                "Pos",
                Tag::List(vec![1.0f64.into(), 2.0f64.into(), 3.0f64.into()]),
            )
            .with("hive_pos", Tag::IntArray(vec![1, 2, 3]))
            .with("Health", 10.0f32)
            .with("UUID", Tag::IntArray(vec![1, 2, 3, 4]))
    }

    #[test]
    fn an_occupant_drops_the_ignored_tags_and_keeps_the_rest() {
        let occupant = Occupant::of(&bee_tag(false));
        assert!(!occupant.entity_data.contains_key("Pos"));
        assert!(!occupant.entity_data.contains_key("hive_pos"));
        assert!(!occupant.entity_data.contains_key("UUID"));
        assert!(occupant.entity_data.contains_key("Health"));
        assert_eq!(occupant.entity_data.get_str("id"), Some("minecraft:bee"));
        assert_eq!(occupant.ticks_in_hive, 0);
    }

    #[test]
    fn the_minimum_stay_is_two_thousand_four_hundred_ticks_with_nectar_and_six_hundred_without() {
        assert_eq!(Occupant::of(&bee_tag(true)).min_ticks_in_hive, 2400);
        assert_eq!(Occupant::of(&bee_tag(false)).min_ticks_in_hive, 600);
        assert_eq!(
            Occupant::of(&bee_tag(true)).status(),
            ReleaseStatus::HoneyDelivered
        );
        assert_eq!(
            Occupant::of(&bee_tag(false)).status(),
            ReleaseStatus::BeeReleased
        );
    }

    #[test]
    fn an_occupant_is_due_once_the_ticks_exceed_the_minimum() {
        let mut occupant = Occupant::create(0);
        let mut due_at = None;
        for tick in 0..700 {
            if occupant.tick() && due_at.is_none() {
                due_at = Some(tick);
            }
        }
        assert_eq!(due_at, Some(601));
    }

    #[test]
    fn a_deferred_release_restarts_at_half_the_minimum() {
        let mut hive = Beehive::default();
        hive.store(Occupant::of(&bee_tag(true)));
        hive.bees[0].ticks_in_hive = 2401;
        assert_eq!(hive.tick_occupants(), vec![0]);
        hive.defer(0);
        assert_eq!(hive.bees[0].ticks_in_hive, 1200);
    }

    #[test]
    fn the_hive_holds_three_bees_by_default() {
        let mut hive = Beehive::default();
        assert!(hive.is_empty() && hive.has_space() && !hive.is_full());
        for _ in 0..3 {
            hive.store(Occupant::create(0));
        }
        assert!(hive.is_full() && !hive.has_space());
        assert_eq!(hive.occupant_count(), 3);
    }

    #[test]
    fn removing_the_released_bees_keeps_the_others_in_order() {
        let mut hive = Beehive::default();
        for ticks in [1, 2, 3] {
            hive.store(Occupant::create(ticks));
        }
        hive.remove_all(&[0, 2]);
        assert_eq!(hive.bees.len(), 1);
        assert_eq!(hive.bees[0].ticks_in_hive, 2);
    }

    #[test]
    fn the_vanilla_layout_round_trips() {
        let mut hive = Beehive::default();
        hive.store(Occupant::of(&bee_tag(true)));
        hive.flower_pos = Some(BlockPos::new(4, 5, 6));
        hive.max_bees = 5;
        let mut data = Compound::new();
        hive.write(&mut data);
        let bees = data.get_list("bees").unwrap();
        let first = bees[0].as_compound().unwrap();
        assert!(first.get_compound("entity_data").is_some());
        assert_eq!(first.get_i32("ticks_in_hive"), Some(0));
        assert_eq!(first.get_i32("min_ticks_in_hive"), Some(2400));
        assert_eq!(data.get("flower_pos"), Some(&Tag::IntArray(vec![4, 5, 6])));
        assert_eq!(data.get_i32("Bukkit.MaxEntities"), Some(5));
        assert_eq!(Beehive::read(&data), hive);
    }

    #[test]
    fn a_missing_layout_reads_as_an_empty_hive_with_three_places() {
        let hive = Beehive::read(&Compound::new());
        assert_eq!(hive, Beehive::default());
    }

    #[test]
    fn a_returning_bee_replaces_the_flower_always_when_none_and_half_the_time_otherwise() {
        let mut random = LegacyRandom::new(1);
        let mut hive = Beehive::default();
        hive.adopt_flower(Some(BlockPos::new(1, 1, 1)), &mut random);
        assert_eq!(hive.flower_pos, Some(BlockPos::new(1, 1, 1)));
        hive.adopt_flower(None, &mut random);
        assert_eq!(hive.flower_pos, Some(BlockPos::new(1, 1, 1)));
        let mut changed = 0;
        for _ in 0..200 {
            hive.flower_pos = Some(BlockPos::new(1, 1, 1));
            hive.adopt_flower(Some(BlockPos::new(2, 2, 2)), &mut random);
            if hive.flower_pos == Some(BlockPos::new(2, 2, 2)) {
                changed += 1;
            }
        }
        assert!((60..140).contains(&changed), "{changed}");
    }

    #[test]
    fn delivered_honey_adds_one_level_or_rarely_two_and_never_passes_five() {
        let mut random = LegacyRandom::new(9);
        assert_eq!(honey_after_delivery(5, &mut random), None);
        let mut twos = 0;
        for _ in 0..2000 {
            let level = honey_after_delivery(0, &mut random).unwrap();
            assert!(level == 1 || level == 2);
            if level == 2 {
                twos += 1;
            }
            assert!(honey_after_delivery(4, &mut random).unwrap() <= 5);
        }
        assert!((5..60).contains(&twos), "{twos}");
    }

    #[test]
    fn the_exit_offset_is_zero_when_blocked() {
        assert_eq!(exit_offset(true, 0.55), 0.0);
        assert!((exit_offset(false, 0.55) - (0.55 + f64::from(0.275f32))).abs() < 1e-9);
    }

    #[test]
    fn a_stay_ages_babies_and_adults_toward_zero_and_spares_locked_ages() {
        assert_eq!(age_after_stay(-1000, false, 600), -400);
        assert_eq!(age_after_stay(-100, false, 600), 0);
        assert_eq!(age_after_stay(5000, false, 600), 4400);
        assert_eq!(age_after_stay(300, false, 600), 0);
        assert_eq!(age_after_stay(-1000, true, 600), -1000);
        assert_eq!(love_after_stay(500, 600), 0);
        assert_eq!(love_after_stay(900, 600), 300);
    }
}
