use rspigot_data::brewing::{self, Mix};
use rspigot_items::ItemKind;

use crate::item::{ItemStack, Slot, with_count};
use crate::potion::{self, PotionContents};

pub const BOTTLES: usize = 3;
pub const INGREDIENT: usize = 3;
pub const FUEL: usize = 4;
pub const SLOTS: usize = 5;
pub const DEFAULT_BREW_TIME: i32 = 400;
pub const FUEL_USES: i32 = 20;
pub const BREW_EVENT: i32 = 1035;
const DEFAULT_SPEED: f32 = 1.0;

#[derive(Debug, Clone, PartialEq)]
pub struct BrewingStand {
    pub items: [Slot; SLOTS],
    pub brew_time: i32,
    pub total_brew_time: i32,
    pub fuel: i32,
    pub total_fuel: i32,
    pub speed: f32,
    pub ingredient: Option<ItemKind>,
}

impl Default for BrewingStand {
    fn default() -> Self {
        Self {
            items: [None, None, None, None, None],
            brew_time: 0,
            total_brew_time: 0,
            fuel: 0,
            total_fuel: 0,
            speed: DEFAULT_SPEED,
            ingredient: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Ask {
    Fuel { fuel: ItemStack, power: i32 },
    Start { source: ItemStack, total: i32 },
    Brew { results: Vec<Slot>, fuel: i32 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum FuelAnswer {
    Cancelled,
    Use { power: i32, consuming: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BrewAnswer {
    Cancelled,
    Results(Vec<Slot>),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Answers {
    pub fuel: Option<FuelAnswer>,
    pub start: Option<i32>,
    pub brew: Option<BrewAnswer>,
}

impl Answers {
    pub fn settle_default(&mut self, ask: &Ask) {
        match ask {
            Ask::Fuel { power, .. } => {
                self.fuel = Some(FuelAnswer::Use {
                    power: *power,
                    consuming: true,
                });
            }
            Ask::Start { total, .. } => self.start = Some(*total),
            Ask::Brew { results, .. } => self.brew = Some(BrewAnswer::Results(results.clone())),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Progress {
    Done(Tick),
    Ask(Ask),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tick {
    pub bottles: [bool; BOTTLES],
    pub brewed: bool,
}

fn name_of(stack: &ItemStack) -> Option<&'static str> {
    stack.item.kind().map(ItemKind::name)
}

pub fn is_reagent(stack: &ItemStack) -> bool {
    name_of(stack).is_some_and(brewing::is_reagent)
}

pub fn is_container(stack: &ItemStack) -> bool {
    matches!(
        stack.item.kind(),
        Some(
            ItemKind::Potion
                | ItemKind::SplashPotion
                | ItemKind::LingeringPotion
                | ItemKind::GlassBottle
        )
    )
}

pub fn is_fuel(stack: &ItemStack) -> bool {
    stack.item.kind() == Some(ItemKind::BlazePowder)
}

fn mix_for(bottle: &ItemStack, ingredient: &ItemStack) -> Option<&'static Mix> {
    let item = name_of(bottle)?;
    let potion = PotionContents::of(bottle);
    let potion = potion
        .is_potion_without_custom_effects()
        .then_some(potion.potion)
        .flatten()?;
    brewing::find(item, potion, name_of(ingredient)?)
}

pub fn mixed(bottle: &ItemStack, ingredient: &ItemStack) -> Slot {
    let Some(mix) = mix_for(bottle, ingredient) else {
        return Some(bottle.clone());
    };
    let kind = ItemKind::from_name(mix.output)?;
    Some(potion::stack(kind, mix.output_potion))
}

fn brewable(items: &[Slot; SLOTS]) -> bool {
    let Some(ingredient) = items[INGREDIENT].as_ref() else {
        return false;
    };
    is_reagent(ingredient)
        && items[..BOTTLES]
            .iter()
            .flatten()
            .any(|bottle| mix_for(bottle, ingredient).is_some())
}

fn bottles(items: &[Slot; SLOTS]) -> [bool; BOTTLES] {
    std::array::from_fn(|index| items[index].is_some())
}

impl BrewingStand {
    pub fn data(&self) -> [i16; 4] {
        let short = |value: i32| i16::try_from(value).unwrap_or(i16::MAX);
        [
            short(self.brew_time),
            short(self.fuel),
            short(self.total_brew_time),
            short(self.total_fuel),
        ]
    }

    pub fn step(&mut self, answers: &Answers) -> Progress {
        if self.fuel <= 0
            && let Some(fuel) = self.items[FUEL].clone().filter(is_fuel)
        {
            match &answers.fuel {
                None => {
                    return Progress::Ask(Ask::Fuel {
                        fuel,
                        power: FUEL_USES,
                    });
                }
                Some(FuelAnswer::Cancelled) => {
                    return Progress::Done(self.settled(false));
                }
                Some(FuelAnswer::Use { power, consuming }) => {
                    self.fuel = *power;
                    self.total_fuel = *power;
                    self.speed = DEFAULT_SPEED;
                    if *power > 0 && *consuming {
                        self.items[FUEL] =
                            with_count(&fuel, u32::from(fuel.count.get()).saturating_sub(1));
                    }
                }
            }
        }
        let brewable = brewable(&self.items);
        let ingredient = self.items[INGREDIENT].clone();
        let mut brewed = false;
        if self.brew_time > 0 {
            self.brew_time -= 1;
            if self.brew_time <= 0 && brewable {
                match self.brew(&answers.brew) {
                    Ok(done) => brewed = done,
                    Err(ask) => return Progress::Ask(ask),
                }
            } else if !brewable
                || ingredient.as_ref().and_then(|stack| stack.item.kind()) != self.ingredient
            {
                self.brew_time = 0;
            }
        } else if brewable && self.fuel > 0 {
            let speed = if self.speed > 0.0 {
                self.speed
            } else {
                DEFAULT_SPEED
            };
            let Some(source) = ingredient else {
                return Progress::Done(self.settled(false));
            };
            let total = (DEFAULT_BREW_TIME as f32 / speed).ceil() as i32;
            let Some(answer) = answers.start else {
                return Progress::Ask(Ask::Start { source, total });
            };
            self.fuel -= 1;
            self.brew_time = answer;
            self.total_brew_time = answer;
            self.ingredient = source.item.kind();
        }
        Progress::Done(self.settled(brewed))
    }

    fn settled(&self, brewed: bool) -> Tick {
        Tick {
            bottles: bottles(&self.items),
            brewed,
        }
    }

    fn brew(&mut self, answer: &Option<BrewAnswer>) -> Result<bool, Ask> {
        let Some(ingredient) = self.items[INGREDIENT].clone() else {
            return Ok(false);
        };
        let results = match answer {
            None => {
                let results: Vec<Slot> = self.items[..BOTTLES]
                    .iter()
                    .map(|bottle| {
                        bottle
                            .as_ref()
                            .and_then(|bottle| mixed(bottle, &ingredient))
                    })
                    .collect();
                return Err(Ask::Brew {
                    results,
                    fuel: self.fuel,
                });
            }
            Some(BrewAnswer::Cancelled) => return Ok(false),
            Some(BrewAnswer::Results(results)) => results,
        };
        for (index, slot) in self.items[..BOTTLES].iter_mut().enumerate() {
            *slot = results.get(index).cloned().flatten();
        }
        self.items[INGREDIENT] = with_count(&ingredient, u32::from(ingredient.count.get()) - 1);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enchant::tests::stack as single;

    fn stack(name: &str, count: u32) -> ItemStack {
        with_count(&single(name), count).unwrap()
    }

    fn water() -> ItemStack {
        potion::stack(ItemKind::Potion, "minecraft:water")
    }

    fn stand() -> BrewingStand {
        let mut stand = BrewingStand::default();
        stand.items[0] = Some(water());
        stand.items[INGREDIENT] = Some(stack("minecraft:nether_wart", 2));
        stand.items[FUEL] = Some(stack("minecraft:blaze_powder", 2));
        stand
    }

    fn run(stand: &mut BrewingStand, answers: &mut Answers) -> Tick {
        loop {
            let mut next = stand.clone();
            match next.step(answers) {
                Progress::Done(tick) => {
                    *stand = next;
                    return tick;
                }
                Progress::Ask(ask) => answers.settle_default(&ask),
            }
        }
    }

    #[test]
    fn nether_wart_makes_water_awkward() {
        let mixed = mixed(&water(), &stack("minecraft:nether_wart", 1)).unwrap();
        assert!(PotionContents::of(&mixed).is("minecraft:awkward"));
    }

    #[test]
    fn a_bottle_with_no_recipe_comes_out_unchanged() {
        let bottle = potion::stack(ItemKind::Potion, "minecraft:awkward");
        assert_eq!(
            mixed(&bottle, &stack("minecraft:stick", 1)),
            Some(bottle.clone())
        );
    }

    #[test]
    fn the_first_tick_burns_one_fuel_and_starts_four_hundred_ticks() {
        let mut stand = stand();
        run(&mut stand, &mut Answers::default());
        assert_eq!(stand.fuel, 19);
        assert_eq!(stand.brew_time, 400);
        assert_eq!(stand.total_brew_time, 400);
        assert_eq!(stand.items[FUEL].as_ref().unwrap().count.get(), 1);
    }

    #[test]
    fn the_brew_completes_after_four_hundred_more_ticks() {
        let mut stand = stand();
        run(&mut stand, &mut Answers::default());
        let mut last = None;
        for _ in 0..400 {
            last = Some(run(&mut stand, &mut Answers::default()));
        }
        assert!(last.unwrap().brewed);
        assert!(PotionContents::of(stand.items[0].as_ref().unwrap()).is("minecraft:awkward"));
        assert_eq!(stand.items[INGREDIENT].as_ref().unwrap().count.get(), 1);
        assert_eq!(stand.brew_time, 0);
    }

    #[test]
    fn removing_the_ingredient_stops_the_brew() {
        let mut stand = stand();
        run(&mut stand, &mut Answers::default());
        stand.items[INGREDIENT] = None;
        run(&mut stand, &mut Answers::default());
        assert_eq!(stand.brew_time, 0);
    }

    #[test]
    fn without_a_recipe_nothing_starts() {
        let mut stand = stand();
        stand.items[INGREDIENT] = Some(stack("minecraft:stick", 1));
        run(&mut stand, &mut Answers::default());
        assert_eq!(stand.brew_time, 0);
        assert_eq!(stand.fuel, 20);
    }

    #[test]
    fn a_cancelled_fuel_ask_keeps_the_powder_and_the_stand_cold() {
        let mut stand = stand();
        let mut answers = Answers {
            fuel: Some(FuelAnswer::Cancelled),
            ..Answers::default()
        };
        run(&mut stand, &mut answers);
        assert_eq!(stand.fuel, 0);
        assert_eq!(stand.items[FUEL].as_ref().unwrap().count.get(), 2);
    }

    #[test]
    fn a_cancelled_brew_keeps_every_bottle_and_the_ingredient() {
        let mut stand = stand();
        run(&mut stand, &mut Answers::default());
        stand.brew_time = 1;
        let mut answers = Answers {
            brew: Some(BrewAnswer::Cancelled),
            ..Answers::default()
        };
        let tick = run(&mut stand, &mut answers);
        assert!(!tick.brewed);
        assert!(PotionContents::of(stand.items[0].as_ref().unwrap()).is("minecraft:water"));
        assert_eq!(stand.items[INGREDIENT].as_ref().unwrap().count.get(), 2);
    }

    #[test]
    fn a_non_consuming_fuel_answer_keeps_the_powder() {
        let mut stand = stand();
        let mut answers = Answers {
            fuel: Some(FuelAnswer::Use {
                power: 5,
                consuming: false,
            }),
            ..Answers::default()
        };
        run(&mut stand, &mut answers);
        assert_eq!(stand.fuel, 4);
        assert_eq!(stand.items[FUEL].as_ref().unwrap().count.get(), 2);
    }

    #[test]
    fn only_blaze_powder_is_fuel_and_only_bottles_are_containers() {
        assert!(is_fuel(&stack("minecraft:blaze_powder", 1)));
        assert!(!is_fuel(&stack("minecraft:coal", 1)));
        assert!(is_container(&stack("minecraft:glass_bottle", 1)));
        assert!(is_container(&water()));
        assert!(!is_container(&stack("minecraft:stick", 1)));
        assert!(is_reagent(&stack("minecraft:nether_wart", 1)));
    }

    #[test]
    fn the_stand_publishes_four_data_slots() {
        let mut stand = stand();
        run(&mut stand, &mut Answers::default());
        assert_eq!(stand.data(), [400, 19, 400, 20]);
    }
}
