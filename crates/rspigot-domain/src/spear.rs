use rspigot_items::ItemKind;

pub const CONTACT_COOLDOWN_TICKS: i32 = 10;
pub const LUNGE: &str = "minecraft:lunge";
pub const MOTION_TO_SPEED: f64 = 20.0;
pub const PLAYER_FACTOR: f64 = 1.0;
pub const MOB_FACTOR: f64 = 0.2;
pub const STAB_KNOCKBACK: f32 = 0.4;
pub const HITBOX_MARGIN: f64 = 0.125;
pub const MIN_REACH: f64 = 2.0;
pub const MAX_REACH: f64 = 4.5;
pub const MIN_CREATIVE_REACH: f64 = 2.0;
pub const MAX_CREATIVE_REACH: f64 = 6.5;
pub const MOB_REACH_FACTOR: f64 = 0.5;
pub const USE_SOUND: &str = "minecraft:item.spear.use";
pub const ATTACK_SOUND: &str = "minecraft:item.spear.attack";
pub const HIT_SOUND: &str = "minecraft:item.spear.hit";
pub const WOOD_USE_SOUND: &str = "minecraft:item.spear_wood.use";
pub const WOOD_ATTACK_SOUND: &str = "minecraft:item.spear_wood.attack";
pub const WOOD_HIT_SOUND: &str = "minecraft:item.spear_wood.hit";
pub const LUNGE_SOUNDS: [&str; 3] = [
    "minecraft:item.spear.lunge_1",
    "minecraft:item.spear.lunge_2",
    "minecraft:item.spear.lunge_3",
];
pub const LUNGE_IMPULSE: f64 = 0.458;
pub const LUNGE_EXHAUSTION: f32 = 4.0;
pub const LUNGE_MIN_FOOD: i32 = 7;
pub const SPEAR_MOBS_WINDOW: i32 = 20;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Condition {
    pub max_duration_ticks: i32,
    pub min_speed: f64,
    pub min_relative_speed: f64,
}

impl Condition {
    pub fn test(
        &self,
        ticks_used: i32,
        attacker_speed: f64,
        relative_speed: f64,
        factor: f64,
    ) -> bool {
        ticks_used <= self.max_duration_ticks
            && self.min_speed * factor <= attacker_speed
            && self.min_relative_speed * factor <= relative_speed
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spear {
    pub delay_ticks: i32,
    pub damage_multiplier: f32,
    pub damage: Condition,
    pub knockback: Condition,
    pub dismount: Condition,
    pub wooden: bool,
    pub attack_ticks: i32,
}

const fn speed(max_duration_ticks: i32, min_speed: f64) -> Condition {
    Condition {
        max_duration_ticks,
        min_speed,
        min_relative_speed: 0.0,
    }
}

const fn relative(max_duration_ticks: i32, min_relative_speed: f64) -> Condition {
    Condition {
        max_duration_ticks,
        min_speed: 0.0,
        min_relative_speed,
    }
}

const fn tier(
    delay_ticks: i32,
    damage_multiplier: f32,
    damage_ticks: i32,
    knockback_ticks: i32,
    dismount: Condition,
    wooden: bool,
    attack_ticks: i32,
) -> Spear {
    Spear {
        delay_ticks,
        damage_multiplier,
        damage: relative(damage_ticks, 4.6),
        knockback: speed(knockback_ticks, 5.1),
        dismount,
        wooden,
        attack_ticks,
    }
}

pub fn of(item: ItemKind) -> Option<Spear> {
    Some(match item {
        ItemKind::WoodenSpear => tier(15, 0.7, 300, 200, speed(100, 14.0), true, 13),
        ItemKind::StoneSpear => tier(14, 0.82, 275, 180, speed(90, 13.0), false, 15),
        ItemKind::CopperSpear => tier(13, 0.82, 250, 165, speed(80, 12.0), false, 17),
        ItemKind::IronSpear => tier(12, 0.95, 225, 135, speed(50, 11.0), false, 19),
        ItemKind::GoldenSpear => tier(14, 0.7, 275, 170, speed(70, 13.0), false, 19),
        ItemKind::DiamondSpear => tier(10, 1.075, 200, 130, speed(60, 10.0), false, 21),
        ItemKind::NetheriteSpear => tier(8, 1.2, 175, 110, speed(50, 9.0), false, 23),
        _ => return None,
    })
}

impl Spear {
    pub fn use_duration(&self) -> i32 {
        self.delay_ticks + self.damage.max_duration_ticks
    }

    pub fn use_sound(&self) -> &'static str {
        if self.wooden {
            WOOD_USE_SOUND
        } else {
            USE_SOUND
        }
    }

    pub fn attack_sound(&self) -> &'static str {
        if self.wooden {
            WOOD_ATTACK_SOUND
        } else {
            ATTACK_SOUND
        }
    }

    pub fn hit_sound(&self) -> &'static str {
        if self.wooden {
            WOOD_HIT_SOUND
        } else {
            HIT_SOUND
        }
    }

    pub fn charging(&self, ticks_used: i32) -> Option<i32> {
        (ticks_used >= self.delay_ticks).then_some(ticks_used - self.delay_ticks)
    }

    pub fn bonus_damage(&self, relative_speed: f64) -> f32 {
        (relative_speed * f64::from(self.damage_multiplier)).floor() as f32
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Contact {
    pub deals_damage: bool,
    pub knocks_back: bool,
    pub dismounts: bool,
}

impl Contact {
    pub fn any(self) -> bool {
        self.deals_damage || self.knocks_back || self.dismounts
    }
}

pub fn contact(
    spear: &Spear,
    ticks_charged: i32,
    attacker_speed: f64,
    relative_speed: f64,
    factor: f64,
) -> Contact {
    let test = |condition: &Condition| {
        condition.test(ticks_charged, attacker_speed, relative_speed, factor)
    };
    Contact {
        deals_damage: test(&spear.damage),
        knocks_back: test(&spear.knockback),
        dismounts: test(&spear.dismount),
    }
}

pub fn relative_speed(attacker_speed: f64, target_speed: f64) -> f64 {
    (attacker_speed - target_speed).max(0.0)
}

pub fn lunge_impulse(level: i32) -> f64 {
    LUNGE_IMPULSE * f64::from(level.max(0))
}

pub fn lunge_exhaustion(level: i32) -> f32 {
    LUNGE_EXHAUSTION * level.max(0) as f32
}

pub fn lunge_allowed(
    riding: bool,
    gliding: bool,
    in_water: bool,
    food: i32,
    creative: bool,
) -> bool {
    !riding && !gliding && !in_water && (creative || food >= LUNGE_MIN_FOOD)
}

pub fn reach(creative: bool) -> (f64, f64) {
    if creative {
        (MIN_CREATIVE_REACH, MAX_CREATIVE_REACH)
    } else {
        (MIN_REACH, MAX_REACH)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spear_has_a_tier_and_nothing_else_does() {
        for item in [
            ItemKind::WoodenSpear,
            ItemKind::StoneSpear,
            ItemKind::CopperSpear,
            ItemKind::IronSpear,
            ItemKind::GoldenSpear,
            ItemKind::DiamondSpear,
            ItemKind::NetheriteSpear,
        ] {
            assert!(of(item).is_some());
        }
        assert!(of(ItemKind::IronSword).is_none());
    }

    #[test]
    fn the_iron_spear_matches_its_components() {
        let iron = of(ItemKind::IronSpear).unwrap();
        assert_eq!(iron.delay_ticks, 12);
        assert_eq!(iron.use_duration(), 237);
        assert_eq!(iron.damage_multiplier, 0.95);
        assert_eq!(iron.dismount, speed(50, 11.0));
        assert_eq!(iron.knockback, speed(135, 5.1));
        assert_eq!(iron.damage, relative(225, 4.6));
        assert!(!iron.wooden);
        assert!(of(ItemKind::WoodenSpear).unwrap().wooden);
    }

    #[test]
    fn the_stab_animation_lasts_longer_for_better_tiers() {
        assert_eq!(of(ItemKind::WoodenSpear).unwrap().attack_ticks, 13);
        assert_eq!(of(ItemKind::IronSpear).unwrap().attack_ticks, 19);
        assert_eq!(of(ItemKind::NetheriteSpear).unwrap().attack_ticks, 23);
    }

    #[test]
    fn nothing_hits_before_the_delay() {
        let iron = of(ItemKind::IronSpear).unwrap();
        assert_eq!(iron.charging(11), None);
        assert_eq!(iron.charging(12), Some(0));
        assert_eq!(iron.charging(20), Some(8));
    }

    #[test]
    fn a_condition_needs_time_speed_and_relative_speed() {
        let condition = relative(10, 4.6);
        assert!(condition.test(10, 0.0, 4.6, 1.0));
        assert!(!condition.test(11, 0.0, 9.0, 1.0));
        assert!(!condition.test(5, 0.0, 4.5, 1.0));
        assert!(condition.test(5, 0.0, 0.92, MOB_FACTOR));
    }

    #[test]
    fn a_fast_charge_unlocks_every_effect_and_a_slow_one_none() {
        let iron = of(ItemKind::IronSpear).unwrap();
        let fast = contact(&iron, 10, 12.0, 12.0, PLAYER_FACTOR);
        assert!(fast.deals_damage && fast.knocks_back && fast.dismounts);
        let slow = contact(&iron, 10, 1.0, 1.0, PLAYER_FACTOR);
        assert!(!slow.any());
        let mid = contact(&iron, 10, 6.0, 6.0, PLAYER_FACTOR);
        assert!(mid.deals_damage && mid.knocks_back && !mid.dismounts);
    }

    #[test]
    fn the_bonus_damage_floors_the_scaled_relative_speed() {
        let iron = of(ItemKind::IronSpear).unwrap();
        assert_eq!(iron.bonus_damage(10.0), 9.0);
        assert_eq!(iron.bonus_damage(0.0), 0.0);
        assert_eq!(relative_speed(3.0, 5.0), 0.0);
        assert_eq!(relative_speed(8.0, 5.0), 3.0);
    }

    #[test]
    fn lunge_scales_with_its_level_and_needs_food() {
        assert!((lunge_impulse(2) - 0.916).abs() < 1e-9);
        assert_eq!(lunge_exhaustion(3), 12.0);
        assert!(lunge_allowed(false, false, false, 7, false));
        assert!(!lunge_allowed(false, false, false, 6, false));
        assert!(lunge_allowed(false, false, false, 0, true));
        assert!(!lunge_allowed(true, false, false, 20, false));
        assert!(!lunge_allowed(false, true, false, 20, false));
        assert!(!lunge_allowed(false, false, true, 20, false));
    }

    #[test]
    fn creative_reaches_further() {
        assert_eq!(reach(false), (2.0, 4.5));
        assert_eq!(reach(true), (2.0, 6.5));
    }
}
