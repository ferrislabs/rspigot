use crate::world::Difficulty;

pub const MAX_FOOD: i32 = 20;
pub const START_SATURATION: f32 = 5.0;
pub const MAX_SATURATION: f32 = 20.0;
const EXHAUSTION_DROP: f32 = 4.0;
const MAX_EXHAUSTION: f32 = 40.0;
const HEAL_LEVEL: i32 = 18;
const SATURATED_REGEN_TICKS: i32 = 10;
const UNSATURATED_REGEN_TICKS: i32 = 80;
const STARVATION_TICKS: i32 = 80;
const SATURATED_HEAL_CAP: f32 = 6.0;
const EXHAUSTION_HEAL: f32 = 6.0;
const STARVE_HEALTH_FLOOR_EASY: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FoodData {
    pub food: i32,
    pub saturation: f32,
    pub exhaustion: f32,
    tick_timer: i32,
}

impl Default for FoodData {
    fn default() -> Self {
        Self {
            food: MAX_FOOD,
            saturation: START_SATURATION,
            exhaustion: 0.0,
            tick_timer: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FoodEffect {
    None,
    Heal { amount: f32, exhaustion: f32 },
    Starve,
}

pub fn saturation_by_modifier(nutrition: i32, modifier: f32) -> f32 {
    nutrition as f32 * modifier * 2.0
}

impl FoodData {
    pub const fn new(food: i32, saturation: f32) -> Self {
        Self {
            food,
            saturation,
            exhaustion: 0.0,
            tick_timer: 0,
        }
    }

    pub const fn restore(food: i32, saturation: f32, exhaustion: f32, tick_timer: i32) -> Self {
        Self {
            food,
            saturation,
            exhaustion,
            tick_timer,
        }
    }

    pub const fn tick_timer(&self) -> i32 {
        self.tick_timer
    }

    pub fn add_exhaustion(&mut self, amount: f32) {
        self.exhaustion = (self.exhaustion + amount).min(MAX_EXHAUSTION);
    }

    pub fn add(&mut self, nutrition: i32, saturation: f32) {
        self.food = (nutrition + self.food).clamp(0, MAX_FOOD);
        self.saturation =
            crate::damage::java_clamp(saturation + self.saturation, 0.0, self.food as f32);
    }

    pub fn eat(&mut self, nutrition: i32, modifier: f32) {
        self.add(nutrition, saturation_by_modifier(nutrition, modifier));
    }

    pub fn needs_food(&self) -> bool {
        self.food < MAX_FOOD
    }

    pub fn tick(
        &mut self,
        difficulty: Difficulty,
        natural_regeneration: bool,
        health: f32,
        max_health: f32,
    ) -> FoodEffect {
        if let Some(level) = self.tick_exhaustion(difficulty) {
            self.food = level;
        }
        self.tick_regeneration(difficulty, natural_regeneration, health, max_health)
    }

    pub fn tick_exhaustion(&mut self, difficulty: Difficulty) -> Option<i32> {
        if self.exhaustion <= EXHAUSTION_DROP {
            return None;
        }
        self.exhaustion -= EXHAUSTION_DROP;
        if self.saturation > 0.0 {
            self.saturation = (self.saturation - 1.0).max(0.0);
            None
        } else if difficulty != Difficulty::Peaceful {
            Some((self.food - 1).max(0))
        } else {
            None
        }
    }

    pub fn tick_regeneration(
        &mut self,
        difficulty: Difficulty,
        natural_regeneration: bool,
        health: f32,
        max_health: f32,
    ) -> FoodEffect {
        self.tick_regeneration_costing(
            difficulty,
            natural_regeneration,
            health,
            max_health,
            EXHAUSTION_HEAL,
        )
    }

    pub fn tick_regeneration_costing(
        &mut self,
        difficulty: Difficulty,
        natural_regeneration: bool,
        health: f32,
        max_health: f32,
        regen_exhaustion: f32,
    ) -> FoodEffect {
        let hurt = health > 0.0 && health < max_health;
        if natural_regeneration && self.saturation > 0.0 && hurt && self.food >= MAX_FOOD {
            self.tick_timer += 1;
            if self.tick_timer >= SATURATED_REGEN_TICKS {
                let spent = self.saturation.min(SATURATED_HEAL_CAP);
                self.tick_timer = 0;
                return FoodEffect::Heal {
                    amount: spent / SATURATED_HEAL_CAP,
                    exhaustion: spent,
                };
            }
        } else if natural_regeneration && self.food >= HEAL_LEVEL && hurt {
            self.tick_timer += 1;
            if self.tick_timer >= UNSATURATED_REGEN_TICKS {
                self.tick_timer = 0;
                return FoodEffect::Heal {
                    amount: 1.0,
                    exhaustion: regen_exhaustion,
                };
            }
        } else if self.food <= 0 {
            self.tick_timer += 1;
            if self.tick_timer >= STARVATION_TICKS {
                self.tick_timer = 0;
                let starves = health > STARVE_HEALTH_FLOOR_EASY
                    || difficulty == Difficulty::Hard
                    || (health > 1.0 && difficulty == Difficulty::Normal);
                if starves {
                    return FoodEffect::Starve;
                }
            }
        } else {
            self.tick_timer = 0;
        }
        FoodEffect::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hunger_proposes_the_lower_level_without_writing_it() {
        let mut food = FoodData::restore(12, 0.0, 4.5, 0);
        assert_eq!(food.tick_exhaustion(Difficulty::Normal), Some(11));
        assert_eq!(food.food, 12);
        assert!((food.exhaustion - 0.5).abs() < 1e-6);
        let mut fed = FoodData::restore(12, 2.0, 4.5, 0);
        assert_eq!(fed.tick_exhaustion(Difficulty::Normal), None);
        assert_eq!(fed.saturation, 1.0);
        let mut calm = FoodData::restore(12, 0.0, 4.5, 0);
        assert_eq!(calm.tick_exhaustion(Difficulty::Peaceful), None);
        let mut starving = FoodData::restore(0, 0.0, 4.5, 0);
        assert_eq!(starving.tick_exhaustion(Difficulty::Hard), Some(0));
    }

    fn run(
        food: &mut FoodData,
        difficulty: Difficulty,
        health: f32,
        ticks: usize,
    ) -> Vec<FoodEffect> {
        (0..ticks)
            .map(|_| food.tick(difficulty, true, health, 20.0))
            .filter(|effect| *effect != FoodEffect::None)
            .collect()
    }

    #[test]
    fn full_and_saturated_heals_every_ten_ticks() {
        let mut food = FoodData::default();
        let effects = run(&mut food, Difficulty::Normal, 10.0, 20);
        assert_eq!(
            effects,
            vec![
                FoodEffect::Heal {
                    amount: 5.0 / 6.0,
                    exhaustion: 5.0
                };
                2
            ]
        );
    }

    #[test]
    fn eighteen_food_heals_every_eighty_ticks() {
        let mut food = FoodData {
            food: 18,
            saturation: 0.0,
            ..FoodData::default()
        };
        let effects = run(&mut food, Difficulty::Easy, 10.0, 160);
        assert_eq!(
            effects,
            vec![
                FoodEffect::Heal {
                    amount: 1.0,
                    exhaustion: 6.0
                };
                2
            ]
        );
    }

    #[test]
    fn unsaturated_regeneration_costs_the_configured_exhaustion() {
        let mut food = FoodData {
            food: 20,
            saturation: 0.0,
            ..FoodData::default()
        };
        let mut effect = FoodEffect::None;
        for _ in 0..UNSATURATED_REGEN_TICKS {
            effect = food.tick_regeneration_costing(Difficulty::Normal, true, 10.0, 20.0, 3.0);
        }
        assert_eq!(
            effect,
            FoodEffect::Heal {
                amount: 1.0,
                exhaustion: 3.0
            }
        );
    }

    #[test]
    fn natural_regeneration_rule_disables_healing() {
        let mut food = FoodData::default();
        for _ in 0..200 {
            assert_eq!(
                food.tick(Difficulty::Easy, false, 10.0, 20.0),
                FoodEffect::None
            );
        }
    }

    #[test]
    fn starvation_depends_on_difficulty() {
        let starving = FoodData {
            food: 0,
            saturation: 0.0,
            ..FoodData::default()
        };
        let mut hard = starving;
        assert_eq!(run(&mut hard, Difficulty::Hard, 1.0, 160).len(), 2);
        let mut normal = starving;
        assert_eq!(run(&mut normal, Difficulty::Normal, 2.0, 80).len(), 1);
        let mut normal_low = starving;
        assert!(run(&mut normal_low, Difficulty::Normal, 1.0, 400).is_empty());
        let mut easy = starving;
        assert!(run(&mut easy, Difficulty::Easy, 10.0, 400).is_empty());
        let mut easy_high = starving;
        assert_eq!(run(&mut easy_high, Difficulty::Easy, 11.0, 80).len(), 1);
        let mut peaceful = starving;
        assert!(run(&mut peaceful, Difficulty::Peaceful, 10.0, 400).is_empty());
    }

    #[test]
    fn exhaustion_drains_saturation_then_food() {
        let mut food = FoodData {
            food: 10,
            saturation: 1.0,
            exhaustion: 0.0,
            tick_timer: 0,
        };
        food.add_exhaustion(4.5);
        food.tick(Difficulty::Normal, true, 20.0, 20.0);
        assert_eq!((food.food, food.saturation), (10, 0.0));
        assert!((food.exhaustion - 0.5).abs() < 1e-6);
        food.add_exhaustion(4.0);
        food.tick(Difficulty::Normal, true, 20.0, 20.0);
        assert_eq!(food.food, 9);
        food.add_exhaustion(100.0);
        assert_eq!(food.exhaustion, 40.0);
        let mut peaceful = FoodData {
            food: 10,
            saturation: 0.0,
            exhaustion: 5.0,
            tick_timer: 0,
        };
        peaceful.tick(Difficulty::Peaceful, true, 20.0, 20.0);
        assert_eq!(peaceful.food, 10);
    }

    #[test]
    fn eating_caps_food_and_saturation() {
        let mut food = FoodData {
            food: 18,
            saturation: 0.0,
            ..FoodData::default()
        };
        food.eat(4, 1.0);
        assert_eq!(food.food, 20);
        assert_eq!(food.saturation, 8.0);
        food.eat(20, 1.0);
        assert_eq!(food.saturation, 20.0);
    }
}
