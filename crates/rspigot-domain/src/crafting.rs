use std::num::NonZeroU8;
use std::sync::OnceLock;

use rspigot_items::{Craft, FireworkShapes, ItemKind, ItemSet, Product, ProductComponent};
use rspigot_nbt::{Compound, Tag};

use crate::dye::{DYE_COLORS, of_dye_item};
use crate::item::{ItemStack, Slot, count, same_item_same_components, with_count};
use crate::plugin_recipes::{self, Body, PluginRecipe};

const SUSPICIOUS_STEW_EFFECTS: &str = "minecraft:suspicious_stew_effects";
const WRITTEN_BOOK_CONTENT: &str = "minecraft:written_book_content";
const MAX_CLONABLE_GENERATION: i32 = 1;
const MAX_DAMAGE: &str = "minecraft:max_damage";
const DAMAGE: &str = "minecraft:damage";
const ENCHANTMENTS: &str = "minecraft:enchantments";
const REPAIR_BONUS_PERCENT: i32 = 5;
const BANNER_PATTERNS: &str = "minecraft:banner_patterns";
const BASE_COLOR: &str = "minecraft:base_color";
const POT_DECORATIONS: &str = "minecraft:pot_decorations";
const POTION_CONTENTS: &str = "minecraft:potion_contents";
const MAX_BANNER_LAYERS: usize = 6;
const BANNER_SUFFIX: &str = "_banner";
const BRICK: &str = "minecraft:brick";
const DYED_COLOR: &str = "minecraft:dyed_color";
const FIREWORK_EXPLOSION: &str = "minecraft:firework_explosion";
const FIREWORKS: &str = "minecraft:fireworks";
const SMALL_BALL: &str = "small_ball";
const MAX_FIREWORK_FUEL: usize = 3;
const DEFAULT_FLIGHT_DURATION: i32 = 1;

struct Input<'a> {
    width: usize,
    height: usize,
    cells: Vec<Option<&'a ItemStack>>,
    filled: usize,
}

impl<'a> Input<'a> {
    fn of(width: usize, height: usize, slots: &'a [Slot]) -> Option<Self> {
        let occupied = |x: usize, y: usize| slots.get(y * width + x).is_some_and(Option::is_some);
        let columns: Vec<usize> = (0..width)
            .filter(|&x| (0..height).any(|y| occupied(x, y)))
            .collect();
        let rows: Vec<usize> = (0..height)
            .filter(|&y| (0..width).any(|x| occupied(x, y)))
            .collect();
        let (left, right) = (*columns.first()?, *columns.last()?);
        let (top, bottom) = (*rows.first()?, *rows.last()?);
        let cells: Vec<Option<&ItemStack>> = (top..=bottom)
            .flat_map(|y| (left..=right).map(move |x| (x, y)))
            .map(|(x, y)| slots[y * width + x].as_ref())
            .collect();
        let filled = cells.iter().flatten().count();
        Some(Self {
            width: right - left + 1,
            height: bottom - top + 1,
            cells,
            filled,
        })
    }

    fn stacks(&self) -> Vec<&'a ItemStack> {
        self.cells.iter().flatten().copied().collect()
    }
}

fn recipes() -> &'static [(ItemKind, &'static Craft)] {
    static INDEX: OnceLock<Vec<(ItemKind, &'static Craft)>> = OnceLock::new();
    INDEX.get_or_init(|| {
        ItemKind::ALL
            .into_iter()
            .flat_map(|kind| kind.crafts().iter().map(move |craft| (kind, craft)))
            .filter(|(_, craft)| craft.uses_crafting_grid())
            .collect()
    })
}

enum Found {
    Vanilla(ItemKind, &'static Craft),
    Plugin(&'static str, ItemStack),
}

impl Found {
    fn craft(&self) -> Option<&'static Craft> {
        match self {
            Self::Vanilla(_, craft) => Some(craft),
            Self::Plugin(..) => None,
        }
    }
}

fn plugin_matches(recipe: &PluginRecipe, input: &Input) -> bool {
    match &recipe.body {
        Body::Shaped {
            width,
            height,
            mirror,
            cells,
        } => shaped_matches(*width, *height, !*mirror, cells, input),
        Body::Shapeless { ingredients } => shapeless_matches(ingredients, input),
        _ => false,
    }
}

fn find_plugin(overriding: bool, input: &Input) -> Option<Found> {
    plugin_recipes::with(|recipes| {
        recipes
            .precedence(overriding)
            .filter(|recipe| recipe.uses_crafting_grid())
            .find(|recipe| plugin_matches(recipe, input))
            .map(|recipe| Found::Plugin(recipe.id, recipe.result.clone()))
    })
}

fn find(input: &Input) -> Option<Found> {
    find_plugin(true, input)
        .or_else(|| {
            recipes()
                .iter()
                .find(|(owner, craft)| matches(*owner, craft, input))
                .map(|(owner, craft)| Found::Vanilla(*owner, craft))
        })
        .or_else(|| find_plugin(false, input))
}

fn matches(owner: ItemKind, craft: &Craft, input: &Input) -> bool {
    match *craft {
        Craft::Shaped {
            width,
            height,
            symmetrical,
            cells,
            ..
        } => shaped_matches(
            usize::from(width),
            usize::from(height),
            symmetrical,
            cells,
            input,
        ),
        Craft::Shapeless { ingredients, .. } => shapeless_matches(ingredients, input),
        Craft::Dye { dye, .. } => dyeable(owner, dye, input).is_some(),
        Craft::FireworkStar {
            dye,
            fuel,
            trail,
            twinkle,
            shapes,
            ..
        } => star_parts(
            &StarIngredients {
                dye,
                fuel,
                trail,
                twinkle,
                shapes,
            },
            input,
        )
        .is_some(),
        Craft::FireworkStarFade { dye, .. } => fadeable(owner, dye, input).is_some(),
        Craft::FireworkRocket {
            shell, star, fuel, ..
        } => rocket_parts(shell, star, fuel, input).is_some(),
        Craft::RepairItem { .. } => repairable(owner, input).is_some(),
        Craft::BannerDuplicate { .. } => duplicable(owner, input).is_some(),
        Craft::ShieldDecoration { banner, .. } => decorable(owner, banner, input).is_some(),
        Craft::DecoratedPot {
            back,
            front,
            left,
            right,
            ..
        } => pot_sides(back, front, left, right, input).is_some(),
        Craft::Imbue {
            source, material, ..
        } => imbuable(source, material, input).is_some(),
        Craft::BookCloning {
            source, material, ..
        } => input.filled >= 2 && cloning(source, material, input).is_some(),
        Craft::Transmute {
            input: base,
            material,
            min_material,
            max_material,
            add_material_count,
            count,
            ..
        } => transmute_matches(
            owner,
            base,
            material,
            usize::from(min_material)..=usize::from(max_material),
            add_material_count,
            usize::from(count),
            input,
        ),
        _ => false,
    }
}

fn shaped_matches(
    width: usize,
    height: usize,
    symmetrical: bool,
    cells: &[Option<ItemSet>],
    input: &Input,
) -> bool {
    if input.filled != cells.iter().flatten().count()
        || input.width != width
        || input.height != height
    {
        return false;
    }
    let lays_over = |mirrored: bool| {
        (0..height).all(|y| {
            (0..width).all(|x| {
                let column = if mirrored { width - 1 - x } else { x };
                match (cells[y * width + column], input.cells[y * width + x]) {
                    (None, None) => true,
                    (Some(set), Some(stack)) => set.contains(stack.item),
                    _ => false,
                }
            })
        })
    };
    (!symmetrical && lays_over(true)) || lays_over(false)
}

fn shapeless_matches(ingredients: &[ItemSet], input: &Input) -> bool {
    if input.filled != ingredients.len() {
        return false;
    }
    let stacks = input.stacks();
    let mut claimed: Vec<Option<usize>> = vec![None; stacks.len()];
    (0..ingredients.len()).all(|ingredient| {
        let mut seen = vec![false; stacks.len()];
        claim(ingredient, ingredients, &stacks, &mut claimed, &mut seen)
    })
}

fn claim(
    ingredient: usize,
    ingredients: &[ItemSet],
    stacks: &[&ItemStack],
    claimed: &mut [Option<usize>],
    seen: &mut [bool],
) -> bool {
    for (slot, stack) in stacks.iter().enumerate() {
        if seen[slot] || !ingredients[ingredient].contains(stack.item) {
            continue;
        }
        seen[slot] = true;
        let free = match claimed[slot] {
            None => true,
            Some(other) => claim(other, ingredients, stacks, claimed, seen),
        };
        if free {
            claimed[slot] = Some(ingredient);
            return true;
        }
    }
    false
}

struct Transmuted<'a> {
    base: &'a ItemStack,
    materials: usize,
}

fn transmuted<'a>(
    base: ItemSet,
    material: ItemSet,
    max_material: usize,
    input: &Input<'a>,
) -> Option<Transmuted<'a>> {
    let mut found = None;
    let mut materials = 0;
    for stack in input.stacks() {
        if base.contains(stack.item) {
            if found.is_some() {
                return None;
            }
            found = Some(stack);
        } else if material.contains(stack.item) {
            materials += 1;
            if materials > max_material {
                return None;
            }
        } else {
            return None;
        }
    }
    found.map(|base| Transmuted { base, materials })
}

fn transmute_matches(
    owner: ItemKind,
    base: ItemSet,
    material: ItemSet,
    range: std::ops::RangeInclusive<usize>,
    add_material_count: bool,
    count: usize,
    input: &Input,
) -> bool {
    if input.filled < range.start() + 1 || input.filled > range.end() + 1 {
        return false;
    }
    let Some(found) = transmuted(base, material, *range.end(), input) else {
        return false;
    };
    if !range.contains(&found.materials) {
        return false;
    }
    let size = if add_material_count {
        found.materials + count
    } else {
        count
    };
    size != 1 || found.base.item != owner.id()
}

fn durability(stack: &ItemStack) -> Option<i32> {
    stack.components.get_i32(MAX_DAMAGE).or_else(|| {
        let max = stack.item.kind()?.max_damage()?;
        i32::try_from(max).ok()
    })
}

fn damage_of(stack: &ItemStack, max: i32) -> i32 {
    stack.components.get_i32(DAMAGE).unwrap_or(0).clamp(0, max)
}

fn repairable<'a>(owner: ItemKind, input: &Input<'a>) -> Option<(&'a ItemStack, &'a ItemStack)> {
    if input.filled != 2 {
        return None;
    }
    let stacks = input.stacks();
    let (first, second) = (stacks[0], stacks[1]);
    let combinable = first.item == owner.id()
        && second.item == owner.id()
        && first.count.get() == 1
        && second.count.get() == 1
        && durability(first).is_some()
        && durability(second).is_some();
    combinable.then_some((first, second))
}

fn enchantment_levels(stack: &ItemStack) -> Vec<(String, i32)> {
    let Some(enchantments) = stack.components.get_compound(ENCHANTMENTS) else {
        return Vec::new();
    };
    let levels = enchantments.get_compound("levels").unwrap_or(enchantments);
    levels
        .iter()
        .filter_map(|(name, level)| Some((name.to_owned(), i32::try_from(level.as_i64()?).ok()?)))
        .collect()
}

fn is_curse(enchantment: &str) -> bool {
    rspigot_data::registries::synced_index("minecraft:enchantment", enchantment)
        .zip(rspigot_data::tags::tag(
            "minecraft:enchantment",
            "minecraft:curse",
        ))
        .is_some_and(|(id, curses)| curses.contains(&id))
}

fn repaired(owner: ItemKind, first: &ItemStack, second: &ItemStack) -> Slot {
    let default = owner.max_damage().and_then(|max| i32::try_from(max).ok());
    let first_max = durability(first)?;
    let second_max = durability(second)?;
    let max = first_max.max(second_max);
    let left =
        (first_max - damage_of(first, first_max)) + (second_max - damage_of(second, second_max));
    let total = left + max * REPAIR_BONUS_PERCENT / 100;
    let mut components = Compound::new();
    if default != Some(max) {
        components.insert(MAX_DAMAGE, max);
    }
    let damage = (max - total).clamp(0, max);
    if damage > 0 {
        components.insert(DAMAGE, damage);
    }
    let mut curses = Compound::new();
    for (name, level) in enchantment_levels(first) {
        if is_curse(&name) {
            let other = enchantment_levels(second)
                .into_iter()
                .find_map(|(other, level)| (other == name).then_some(level))
                .unwrap_or(0);
            curses.insert(name, level.max(other));
        }
    }
    for (name, level) in enchantment_levels(second) {
        if is_curse(&name) && !curses.contains_key(&name) {
            curses.insert(name, level);
        }
    }
    if !curses.is_empty() {
        components.insert(ENCHANTMENTS, curses);
    }
    Some(ItemStack {
        item: owner.id(),
        count: NonZeroU8::MIN,
        components,
    })
}

fn dyeable<'a>(
    owner: ItemKind,
    dye: ItemSet,
    input: &Input<'a>,
) -> Option<(&'a ItemStack, Vec<usize>)> {
    if input.filled < 2 {
        return None;
    }
    let mut target = None;
    let mut colors = Vec::new();
    for stack in input.stacks() {
        if stack.item == owner.id() {
            if target.replace(stack).is_some() {
                return None;
            }
        } else {
            let color = dye
                .contains(stack.item)
                .then(|| of_dye_item(stack))
                .flatten()?;
            colors.push(color);
        }
    }
    let target = target?;
    (!colors.is_empty()).then_some((target, colors))
}

fn blended(current: Option<i32>, colors: &[usize]) -> i32 {
    let mut sums = [0i32; 3];
    let mut brightest = 0i32;
    let mut count = 0i32;
    let rgb = current
        .into_iter()
        .chain(colors.iter().map(|&color| DYE_COLORS[color].texture));
    for color in rgb {
        let channels = [(color >> 16) & 0xFF, (color >> 8) & 0xFF, color & 0xFF];
        brightest += channels.into_iter().max().unwrap_or(0);
        for (sum, channel) in sums.iter_mut().zip(channels) {
            *sum += channel;
        }
        count += 1;
    }
    let average = sums.map(|sum| sum / count);
    let average_max = brightest as f32 / count as f32;
    let max_average = average.into_iter().max().unwrap_or(0) as f32;
    let [red, green, blue] =
        average.map(|channel| (channel as f32 * average_max / max_average) as i32);
    ((red & 0xFF) << 16) | ((green & 0xFF) << 8) | (blue & 0xFF)
}

struct StarIngredients {
    dye: ItemSet,
    fuel: ItemSet,
    trail: ItemSet,
    twinkle: ItemSet,
    shapes: FireworkShapes,
}

struct Star {
    shape: &'static str,
    colors: Vec<i32>,
    trail: bool,
    twinkle: bool,
}

fn shape_of(shapes: &FireworkShapes, stack: &ItemStack) -> Option<&'static str> {
    [
        ("burst", shapes.burst),
        ("creeper", shapes.creeper),
        ("large_ball", shapes.large_ball),
        ("star", shapes.star),
    ]
    .into_iter()
    .find_map(|(name, set)| set.contains(stack.item).then_some(name))
}

fn star_parts(ingredients: &StarIngredients, input: &Input) -> Option<Star> {
    if input.filled < 2 {
        return None;
    }
    let mut star = Star {
        shape: SMALL_BALL,
        colors: Vec::new(),
        trail: false,
        twinkle: false,
    };
    let (mut shaped, mut fueled) = (false, false);
    for stack in input.stacks() {
        let item = stack.item;
        if ingredients.twinkle.contains(item) {
            if std::mem::replace(&mut star.twinkle, true) {
                return None;
            }
        } else if ingredients.trail.contains(item) {
            if std::mem::replace(&mut star.trail, true) {
                return None;
            }
        } else if ingredients.fuel.contains(item) {
            if std::mem::replace(&mut fueled, true) {
                return None;
            }
        } else if ingredients.dye.contains(item) && of_dye_item(stack).is_some() {
            star.colors.push(DYE_COLORS[of_dye_item(stack)?].firework);
        } else {
            let shape = shape_of(&ingredients.shapes, stack)?;
            if std::mem::replace(&mut shaped, true) {
                return None;
            }
            star.shape = shape;
        }
    }
    (fueled && !star.colors.is_empty()).then_some(star)
}

fn color_list(colors: &[i32]) -> Tag {
    Tag::List(colors.iter().map(|&color| Tag::Int(color)).collect())
}

fn explosion(star: &Star) -> Compound {
    let mut explosion = Compound::new().with("shape", star.shape);
    if !star.colors.is_empty() {
        explosion.insert("colors", color_list(&star.colors));
    }
    if star.trail {
        explosion.insert("has_trail", true);
    }
    if star.twinkle {
        explosion.insert("has_twinkle", true);
    }
    explosion
}

fn fadeable<'a>(
    owner: ItemKind,
    dye: ItemSet,
    input: &Input<'a>,
) -> Option<(&'a ItemStack, Vec<i32>)> {
    if input.filled < 2 {
        return None;
    }
    let mut target = None;
    let mut fade = Vec::new();
    for stack in input.stacks() {
        if let Some(color) = dye
            .contains(stack.item)
            .then(|| of_dye_item(stack))
            .flatten()
        {
            fade.push(DYE_COLORS[color].firework);
        } else if stack.item == owner.id() {
            if target.replace(stack).is_some() {
                return None;
            }
        } else {
            return None;
        }
    }
    let target = target?;
    (!fade.is_empty()).then_some((target, fade))
}

fn rocket_parts<'a>(
    shell: ItemSet,
    star: ItemSet,
    fuel: ItemSet,
    input: &Input<'a>,
) -> Option<(usize, Vec<&'a ItemStack>)> {
    if input.filled < 2 {
        return None;
    }
    let mut shelled = false;
    let mut flight = 0;
    let mut stars = Vec::new();
    for stack in input.stacks() {
        if shell.contains(stack.item) {
            if std::mem::replace(&mut shelled, true) {
                return None;
            }
        } else if fuel.contains(stack.item) {
            flight += 1;
            if flight > MAX_FIREWORK_FUEL {
                return None;
            }
        } else if star.contains(stack.item) {
            stars.push(stack);
        } else {
            return None;
        }
    }
    (shelled && flight >= 1).then_some((flight, stars))
}

fn layers(stack: &ItemStack) -> usize {
    stack
        .components
        .get(BANNER_PATTERNS)
        .and_then(Tag::as_list)
        .map_or(0, <[Tag]>::len)
}

fn duplicable<'a>(owner: ItemKind, input: &Input<'a>) -> Option<&'a ItemStack> {
    if input.filled != 2 {
        return None;
    }
    let mut patterned = None;
    let mut blank = false;
    for stack in input.stacks() {
        if stack.item != owner.id() {
            return None;
        }
        match layers(stack) {
            0 if blank => return None,
            0 => blank = true,
            1..=MAX_BANNER_LAYERS if patterned.is_none() => patterned = Some(stack),
            _ => return None,
        }
    }
    patterned.filter(|_| blank)
}

fn banner_color(stack: &ItemStack) -> Option<&'static str> {
    let name = stack.item.kind()?.name();
    name.strip_prefix("minecraft:")?.strip_suffix(BANNER_SUFFIX)
}

fn decorable<'a>(
    owner: ItemKind,
    banner: ItemSet,
    input: &Input<'a>,
) -> Option<(&'a ItemStack, &'a ItemStack)> {
    if input.filled != 2 {
        return None;
    }
    let mut found_banner = None;
    let mut found_shield = None;
    for stack in input.stacks() {
        if banner.contains(stack.item) && banner_color(stack).is_some() {
            if found_banner.replace(stack).is_some() {
                return None;
            }
        } else if stack.item == owner.id() {
            if layers(stack) > 0 || found_shield.replace(stack).is_some() {
                return None;
            }
        } else {
            return None;
        }
    }
    Some((found_shield?, found_banner?))
}

fn decorated(owner: ItemKind, shield: &ItemStack, banner: &ItemStack) -> Slot {
    let mut components = shield.components.clone();
    match banner.components.get(BANNER_PATTERNS) {
        Some(patterns) if layers(banner) > 0 => {
            components.insert(BANNER_PATTERNS, patterns.clone());
        }
        _ => {
            components.remove(BANNER_PATTERNS);
        }
    }
    components.insert(BASE_COLOR, banner_color(banner)?);
    Some(ItemStack {
        item: owner.id(),
        count: NonZeroU8::MIN,
        components,
    })
}

fn pot_sides<'a>(
    back: ItemSet,
    front: ItemSet,
    left: ItemSet,
    right: ItemSet,
    input: &Input<'a>,
) -> Option<[&'a ItemStack; 4]> {
    if input.filled != 4 || input.width != 3 || input.height != 3 {
        return None;
    }
    let side =
        |set: ItemSet, index: usize| input.cells[index].filter(|stack| set.contains(stack.item));
    Some([
        side(back, 1)?,
        side(left, 3)?,
        side(right, 5)?,
        side(front, 7)?,
    ])
}

fn pot_decorations(sides: [&ItemStack; 4]) -> Option<Compound> {
    let mut decorations = Compound::new();
    let mut plain_bricks = true;
    for (key, stack) in ["back", "left", "right", "front"].into_iter().zip(sides) {
        let name = stack.item.kind()?.name();
        let mut template = Compound::new().with("id", name);
        if !stack.components.is_empty() {
            template.insert("components", stack.components.clone());
        }
        plain_bricks &= name == BRICK && stack.components.is_empty();
        decorations.insert(key, template);
    }
    (!plain_bricks).then_some(decorations)
}

fn imbuable<'a>(source: ItemSet, material: ItemSet, input: &Input<'a>) -> Option<&'a ItemStack> {
    if input.filled != 9 || input.width != 3 || input.height != 3 {
        return None;
    }
    let potion = input.cells[4].filter(|stack| source.contains(stack.item))?;
    let others = input
        .cells
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != 4)
        .all(|(_, cell)| cell.is_some_and(|stack| material.contains(stack.item)));
    others.then_some(potion)
}

struct Cloning<'a> {
    source: &'a ItemStack,
    generation: i32,
    copies: usize,
}

fn cloning<'a>(source: ItemSet, material: ItemSet, input: &Input<'a>) -> Option<Cloning<'a>> {
    let mut found = None;
    let mut copies = 0;
    for stack in input.stacks() {
        if source.contains(stack.item) {
            let content = stack.components.get_compound(WRITTEN_BOOK_CONTENT)?;
            let generation = content.get_i32("generation").unwrap_or(0);
            if !(0..=MAX_CLONABLE_GENERATION).contains(&generation) || found.is_some() {
                return None;
            }
            found = Some((stack, generation));
        } else if material.contains(stack.item) {
            copies += 1;
        } else {
            return None;
        }
    }
    let (source, generation) = found?;
    (copies > 0).then_some(Cloning {
        source,
        generation,
        copies,
    })
}

fn product_stack(owner: ItemKind, product: &Product) -> Slot {
    let mut components = Compound::new();
    for component in product.components {
        match component {
            ProductComponent::SuspiciousStewEffects(effects) => {
                let list = effects
                    .iter()
                    .map(|effect| {
                        Tag::Compound(
                            Compound::new()
                                .with("id", effect.effect)
                                .with("duration", effect.duration),
                        )
                    })
                    .collect();
                components.insert(SUSPICIOUS_STEW_EFFECTS, Tag::List(list));
            }
        }
    }
    Some(ItemStack {
        item: owner.id(),
        count: NonZeroU8::new(product.count)?,
        components,
    })
}

fn assemble(owner: ItemKind, craft: &Craft, input: &Input) -> Slot {
    match *craft {
        Craft::Shaped { product, .. } | Craft::Shapeless { product, .. } => {
            product_stack(owner, &product)
        }
        Craft::Transmute {
            input: base,
            material,
            max_material,
            add_material_count,
            count,
            ..
        } => {
            let found = transmuted(base, material, usize::from(max_material), input)?;
            let extra = if add_material_count {
                u8::try_from(found.materials).ok()?
            } else {
                0
            };
            Some(ItemStack {
                item: owner.id(),
                count: NonZeroU8::new(count.checked_add(extra)?)?,
                components: found.base.components.clone(),
            })
        }
        Craft::Dye { dye, .. } => {
            let (target, colors) = dyeable(owner, dye, input)?;
            let current = target.components.get_i32(DYED_COLOR);
            let mut components = target.components.clone();
            components.insert(DYED_COLOR, blended(current, &colors));
            Some(ItemStack {
                item: owner.id(),
                count: NonZeroU8::MIN,
                components,
            })
        }
        Craft::FireworkStar {
            dye,
            fuel,
            trail,
            twinkle,
            shapes,
            ..
        } => {
            let star = star_parts(
                &StarIngredients {
                    dye,
                    fuel,
                    trail,
                    twinkle,
                    shapes,
                },
                input,
            )?;
            let mut components = Compound::new();
            components.insert(FIREWORK_EXPLOSION, explosion(&star));
            Some(ItemStack {
                item: owner.id(),
                count: NonZeroU8::MIN,
                components,
            })
        }
        Craft::FireworkStarFade { dye, .. } => {
            let (target, fade) = fadeable(owner, dye, input)?;
            let mut components = target.components.clone();
            let mut faded = target
                .components
                .get_compound(FIREWORK_EXPLOSION)
                .cloned()
                .unwrap_or_else(|| Compound::new().with("shape", SMALL_BALL));
            faded.insert("fade_colors", color_list(&fade));
            components.insert(FIREWORK_EXPLOSION, faded);
            Some(ItemStack {
                item: owner.id(),
                count: NonZeroU8::MIN,
                components,
            })
        }
        Craft::FireworkRocket {
            shell,
            star,
            fuel,
            count,
            ..
        } => {
            let (flight, stars) = rocket_parts(shell, star, fuel, input)?;
            let explosions: Vec<Tag> = stars
                .iter()
                .filter_map(|star| star.components.get_compound(FIREWORK_EXPLOSION))
                .map(|explosion| Tag::Compound(explosion.clone()))
                .collect();
            let flight = i32::try_from(flight).ok()?;
            let mut components = Compound::new();
            if flight != DEFAULT_FLIGHT_DURATION || !explosions.is_empty() {
                let mut fireworks =
                    Compound::new().with("flight_duration", i8::try_from(flight).ok()?);
                if !explosions.is_empty() {
                    fireworks.insert("explosions", Tag::List(explosions));
                }
                components.insert(FIREWORKS, fireworks);
            }
            Some(ItemStack {
                item: owner.id(),
                count: NonZeroU8::new(count)?,
                components,
            })
        }
        Craft::RepairItem { .. } => {
            let (first, second) = repairable(owner, input)?;
            repaired(owner, first, second)
        }
        Craft::BannerDuplicate { .. } => {
            let patterned = duplicable(owner, input)?;
            Some(ItemStack {
                item: owner.id(),
                count: NonZeroU8::MIN,
                components: patterned.components.clone(),
            })
        }
        Craft::ShieldDecoration { banner, .. } => {
            let (shield, banner) = decorable(owner, banner, input)?;
            decorated(owner, shield, banner)
        }
        Craft::DecoratedPot {
            back,
            front,
            left,
            right,
            ..
        } => {
            let mut components = Compound::new();
            if let Some(decorations) = pot_decorations(pot_sides(back, front, left, right, input)?)
            {
                components.insert(POT_DECORATIONS, decorations);
            }
            Some(ItemStack {
                item: owner.id(),
                count: NonZeroU8::MIN,
                components,
            })
        }
        Craft::Imbue {
            source,
            material,
            count,
            ..
        } => {
            let potion = imbuable(source, material, input)?;
            let mut components = Compound::new();
            if let Some(contents) = potion.components.get(POTION_CONTENTS) {
                components.insert(POTION_CONTENTS, contents.clone());
            }
            Some(ItemStack {
                item: owner.id(),
                count: NonZeroU8::new(count)?,
                components,
            })
        }
        Craft::BookCloning {
            source, material, ..
        } => {
            let cloned = cloning(source, material, input)?;
            let mut components = cloned.source.components.clone();
            if let Some(Tag::Compound(content)) = components.get_mut(WRITTEN_BOOK_CONTENT) {
                content.insert("generation", cloned.generation + 1);
            }
            Some(ItemStack {
                item: owner.id(),
                count: NonZeroU8::new(u8::try_from(cloned.copies).ok()?)?,
                components,
            })
        }
        _ => None,
    }
}

fn remainder_of(stack: &ItemStack) -> Slot {
    let item = stack.item.kind()?.crafting_remainder()?;
    Some(ItemStack::new(item, NonZeroU8::MIN))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recipe {
    pub id: &'static str,
    pub repair: bool,
}

pub fn recipe(width: usize, height: usize, slots: &[Slot]) -> Option<Recipe> {
    let input = Input::of(width, height, slots)?;
    Some(match find(&input)? {
        Found::Vanilla(_, craft) => Recipe {
            id: craft.id(),
            repair: matches!(craft, Craft::RepairItem { .. }),
        },
        Found::Plugin(id, _) => Recipe { id, repair: false },
    })
}

pub fn result(width: usize, height: usize, slots: &[Slot]) -> Slot {
    let input = Input::of(width, height, slots)?;
    match find(&input)? {
        Found::Vanilla(owner, craft) => assemble(owner, craft, &input),
        Found::Plugin(_, result) => Some(result),
    }
}

fn remainders(craft: Option<&Craft>, slots: &[Slot]) -> Vec<Slot> {
    let mut remainders = vec![None; slots.len()];
    for (index, slot) in slots.iter().enumerate() {
        let Some(stack) = slot else {
            continue;
        };
        if let Some(remainder) = remainder_of(stack) {
            remainders[index] = Some(remainder);
            continue;
        }
        match craft {
            Some(Craft::BookCloning { .. })
                if stack.components.contains_key(WRITTEN_BOOK_CONTENT) =>
            {
                remainders[index] = with_count(stack, 1);
                break;
            }
            Some(Craft::BannerDuplicate { .. }) if layers(stack) > 0 => {
                remainders[index] = with_count(stack, 1);
            }
            _ => {}
        }
    }
    remainders
}

pub fn take_result(width: usize, height: usize, slots: &mut [Slot]) -> Vec<ItemStack> {
    let matched = Input::of(width, height, slots).and_then(|input| find(&input));
    let mut leftovers = Vec::new();
    let remainders = remainders(matched.as_ref().and_then(Found::craft), slots);
    for (slot, remainder) in slots.iter_mut().zip(remainders) {
        let Some(stack) = slot.take() else {
            continue;
        };
        *slot = with_count(&stack, u32::from(stack.count.get()) - 1);
        let Some(remainder) = remainder else {
            continue;
        };
        match slot {
            None => *slot = Some(remainder),
            Some(existing) if same_item_same_components(existing, &remainder) => {
                let total = count(slot) + count(&Some(remainder.clone()));
                *slot = with_count(&remainder, total);
            }
            Some(_) => leftovers.push(remainder),
        }
    }
    leftovers
}

#[cfg(test)]
mod tests {
    use rspigot_items::ItemId;

    use super::*;

    #[test]
    fn taking_a_result_from_a_grid_without_a_recipe_uses_one_of_each_stack() {
        let mut slots = grid(
            2,
            2,
            &[
                (0, ItemKind::Dirt, 2),
                (1, ItemKind::Stone, 1),
                (3, ItemKind::LavaBucket, 1),
            ],
        );
        assert!(take_result(2, 2, &mut slots).is_empty());
        assert_eq!(slots[0], stack(ItemKind::Dirt, 1));
        assert_eq!(slots[1], None);
        assert_eq!(slots[3], stack(ItemKind::Bucket, 1));
    }

    #[test]
    fn the_recipe_of_a_grid_carries_its_datapack_id() {
        let planks = grid(2, 2, &[(0, ItemKind::OakLog, 1)]);
        assert_eq!(
            recipe(2, 2, &planks),
            Some(Recipe {
                id: "minecraft:oak_planks",
                repair: false
            })
        );
        let mut swords = vec![None; 4];
        swords[0] = Some(ItemStack::new(ItemKind::IronSword.id(), NonZeroU8::MIN));
        swords[3] = Some(ItemStack::new(ItemKind::IronSword.id(), NonZeroU8::MIN));
        assert_eq!(
            recipe(2, 2, &swords),
            Some(Recipe {
                id: "minecraft:repair_item",
                repair: true
            })
        );
        assert_eq!(recipe(2, 2, &[None, None, None, None]), None);
    }

    fn stack(kind: ItemKind, amount: u8) -> Slot {
        Some(ItemStack::new(kind.id(), NonZeroU8::new(amount).unwrap()))
    }

    fn grid(width: usize, height: usize, filled: &[(usize, ItemKind, u8)]) -> Vec<Slot> {
        let mut slots = vec![None; width * height];
        for &(index, kind, amount) in filled {
            slots[index] = stack(kind, amount);
        }
        slots
    }

    fn crafted(width: usize, height: usize, filled: &[(usize, ItemKind, u8)]) -> Slot {
        result(width, height, &grid(width, height, filled))
    }

    #[test]
    fn an_empty_grid_crafts_nothing() {
        assert_eq!(crafted(3, 3, &[]), None);
        assert_eq!(crafted(2, 2, &[]), None);
    }

    #[test]
    fn a_log_makes_four_planks_wherever_it_sits() {
        for index in 0..4 {
            assert_eq!(
                crafted(2, 2, &[(index, ItemKind::OakLog, 1)]),
                stack(ItemKind::OakPlanks, 4)
            );
        }
        assert_eq!(
            crafted(3, 3, &[(8, ItemKind::BirchLog, 5)]),
            stack(ItemKind::BirchPlanks, 4)
        );
    }

    #[test]
    fn shaped_recipes_ignore_where_the_pattern_sits() {
        let planks = |indices: &[usize]| -> Vec<(usize, ItemKind, u8)> {
            indices
                .iter()
                .map(|&index| (index, ItemKind::OakPlanks, 1))
                .collect()
        };
        assert_eq!(crafted(2, 2, &planks(&[0, 2])), stack(ItemKind::Stick, 4));
        assert_eq!(crafted(2, 2, &planks(&[1, 3])), stack(ItemKind::Stick, 4));
        assert_eq!(crafted(3, 3, &planks(&[4, 7])), stack(ItemKind::Stick, 4));
        assert_eq!(crafted(3, 3, &planks(&[1, 4])), stack(ItemKind::Stick, 4));
        assert_eq!(crafted(3, 3, &planks(&[0, 2])), None);
        assert_eq!(crafted(2, 2, &planks(&[0, 3])), None);
    }

    #[test]
    fn a_four_by_four_square_of_planks_makes_a_crafting_table_in_the_small_grid() {
        let planks: Vec<_> = (0..4).map(|i| (i, ItemKind::OakPlanks, 1)).collect();
        assert_eq!(crafted(2, 2, &planks), stack(ItemKind::CraftingTable, 1));
        assert_eq!(crafted(2, 2, &planks[..3]), None);
    }

    #[test]
    fn the_whole_recipe_must_be_present_and_nothing_else() {
        let pickaxe = [
            (0, ItemKind::IronIngot, 1),
            (1, ItemKind::IronIngot, 1),
            (2, ItemKind::IronIngot, 1),
            (4, ItemKind::Stick, 1),
            (7, ItemKind::Stick, 1),
        ];
        assert_eq!(crafted(3, 3, &pickaxe), stack(ItemKind::IronPickaxe, 1));
        assert_eq!(crafted(3, 3, &pickaxe[..4]), None);
        let mut extra = pickaxe.to_vec();
        extra.push((8, ItemKind::Dirt, 1));
        assert_eq!(crafted(3, 3, &extra), None);
        let mut wrong = pickaxe.to_vec();
        wrong[0].1 = ItemKind::GoldIngot;
        assert_eq!(crafted(3, 3, &wrong), None);
    }

    #[test]
    fn a_tag_ingredient_takes_any_member() {
        for (material, pickaxe) in [
            (ItemKind::IronIngot, ItemKind::IronPickaxe),
            (ItemKind::Diamond, ItemKind::DiamondPickaxe),
            (ItemKind::GoldIngot, ItemKind::GoldenPickaxe),
            (ItemKind::CopperIngot, ItemKind::CopperPickaxe),
        ] {
            let slots = [
                (0, material, 1),
                (1, material, 1),
                (2, material, 1),
                (4, ItemKind::Stick, 1),
                (7, ItemKind::Stick, 1),
            ];
            assert_eq!(crafted(3, 3, &slots), stack(pickaxe, 1), "{material:?}");
        }
        let mixed = [
            (0, ItemKind::OakPlanks, 1),
            (1, ItemKind::SprucePlanks, 1),
            (2, ItemKind::OakPlanks, 1),
            (3, ItemKind::BirchPlanks, 1),
        ];
        assert_eq!(crafted(2, 2, &mixed), stack(ItemKind::CraftingTable, 1));
    }

    #[test]
    fn a_pattern_that_is_not_symmetric_also_matches_mirrored() {
        let hoe = |a: usize, b: usize, c: usize, d: usize| {
            [
                (a, ItemKind::OakPlanks, 1),
                (b, ItemKind::OakPlanks, 1),
                (c, ItemKind::Stick, 1),
                (d, ItemKind::Stick, 1),
            ]
        };
        assert_eq!(
            crafted(3, 3, &hoe(0, 1, 4, 7)),
            stack(ItemKind::WoodenHoe, 1)
        );
        assert_eq!(
            crafted(3, 3, &hoe(1, 2, 4, 7)),
            stack(ItemKind::WoodenHoe, 1)
        );
        assert_eq!(
            crafted(3, 3, &hoe(1, 2, 5, 8)),
            stack(ItemKind::WoodenHoe, 1)
        );
        assert_eq!(
            crafted(3, 3, &hoe(0, 1, 3, 6)),
            stack(ItemKind::WoodenHoe, 1)
        );
        assert_eq!(crafted(3, 3, &hoe(0, 2, 4, 7)), None);
    }

    #[test]
    fn shapeless_recipes_ignore_order_but_not_content() {
        let dye = |a: ItemKind, b: ItemKind| crafted(3, 3, &[(1, a, 1), (8, b, 1)]);
        assert_eq!(
            dye(ItemKind::RedDye, ItemKind::YellowDye),
            stack(ItemKind::OrangeDye, 2)
        );
        assert_eq!(
            dye(ItemKind::YellowDye, ItemKind::RedDye),
            stack(ItemKind::OrangeDye, 2)
        );
        assert_eq!(dye(ItemKind::RedDye, ItemKind::RedDye), None);
        assert_eq!(
            crafted(
                3,
                3,
                &[
                    (0, ItemKind::RedDye, 1),
                    (1, ItemKind::YellowDye, 1),
                    (2, ItemKind::BoneMeal, 1)
                ]
            ),
            None
        );
    }

    #[test]
    fn shapeless_matching_needs_every_ingredient_to_get_its_own_stack() {
        let flower_and_sugar =
            |first: ItemKind, second: ItemKind| crafted(3, 3, &[(0, first, 1), (1, second, 1)]);
        assert_eq!(flower_and_sugar(ItemKind::Sugar, ItemKind::Sugar), None);
        assert_eq!(
            crafted(
                3,
                3,
                &[
                    (0, ItemKind::MilkBucket, 1),
                    (1, ItemKind::Sugar, 1),
                    (2, ItemKind::Egg, 1),
                    (3, ItemKind::Wheat, 1)
                ]
            ),
            None
        );
    }

    #[test]
    fn a_stack_that_fits_two_ingredients_is_given_to_the_one_that_needs_it() {
        static EITHER: [ItemId; 2] = [ItemId(1), ItemId(2)];
        static FIRST: [ItemId; 1] = [ItemId(1)];
        let ingredients = [ItemSet::Direct(&EITHER), ItemSet::Direct(&FIRST)];
        let stone = ItemStack::new(ItemId(1), NonZeroU8::MIN);
        let granite = ItemStack::new(ItemId(2), NonZeroU8::MIN);
        for slots in [
            vec![Some(stone.clone()), Some(granite.clone())],
            vec![Some(granite.clone()), Some(stone.clone())],
        ] {
            let input = Input::of(2, 1, &slots).unwrap();
            assert!(shapeless_matches(&ingredients, &input));
        }
        let slots = vec![Some(granite.clone()), Some(granite)];
        let input = Input::of(2, 1, &slots).unwrap();
        assert!(!shapeless_matches(&ingredients, &input));
    }

    #[test]
    fn a_wider_pattern_never_reads_past_a_narrower_grid() {
        let block: Vec<_> = (0..4).map(|i| (i, ItemKind::Leather, 1)).collect();
        assert_eq!(crafted(2, 2, &block), None);
        let boots = [
            (0, ItemKind::Leather, 1),
            (2, ItemKind::Leather, 1),
            (3, ItemKind::Leather, 1),
            (5, ItemKind::Leather, 1),
        ];
        assert_eq!(crafted(3, 2, &boots), stack(ItemKind::LeatherBoots, 1));
        let lopsided = [
            (0, ItemKind::OakPlanks, 1),
            (1, ItemKind::OakPlanks, 1),
            (2, ItemKind::OakPlanks, 1),
            (3, ItemKind::OakPlanks, 1),
        ];
        assert_eq!(crafted(3, 2, &lopsided), None);
    }

    fn with(kind: ItemKind, amount: u8, key: &str, value: impl Into<Tag>) -> Slot {
        let mut stack = stack(kind, amount).unwrap();
        stack.components.insert(key, value);
        Some(stack)
    }

    fn laid(slots: Vec<Slot>) -> Vec<Slot> {
        let mut grid = vec![None; 9];
        for (index, slot) in slots.into_iter().enumerate() {
            grid[index] = slot;
        }
        grid
    }

    fn colors_of(stack: &ItemStack, key: &str, list: &str) -> Vec<i32> {
        stack
            .components
            .get_compound(key)
            .and_then(|compound| compound.get_list(list))
            .map(|tags| {
                tags.iter()
                    .map(|tag| i32::try_from(tag.as_i64().unwrap()).unwrap())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn a_dye_colours_leather_armor_with_the_dye_colour() {
        let boots = result(
            3,
            3,
            &laid(vec![
                stack(ItemKind::LeatherBoots, 1),
                stack(ItemKind::RedDye, 1),
            ]),
        )
        .unwrap();
        assert_eq!(boots.item, ItemKind::LeatherBoots.id());
        assert_eq!(
            boots.components.get_i32("minecraft:dyed_color"),
            Some(0xB02E26)
        );
    }

    #[test]
    fn dyes_blend_with_each_other_and_with_the_colour_already_there() {
        let both = result(
            3,
            3,
            &laid(vec![
                stack(ItemKind::LeatherBoots, 1),
                stack(ItemKind::RedDye, 1),
                stack(ItemKind::WhiteDye, 1),
            ]),
        )
        .unwrap();
        assert_eq!(
            both.components.get_i32("minecraft:dyed_color"),
            Some(0xD79894)
        );
        let again = result(
            3,
            3,
            &laid(vec![
                with(ItemKind::LeatherBoots, 1, "minecraft:dyed_color", 0xB02E26),
                stack(ItemKind::WhiteDye, 1),
            ]),
        )
        .unwrap();
        assert_eq!(
            again.components.get_i32("minecraft:dyed_color"),
            Some(0xD79894)
        );
    }

    #[test]
    fn a_stack_of_dye_counts_once_and_the_rest_of_the_item_is_kept() {
        let boots = result(
            3,
            3,
            &laid(vec![
                with(ItemKind::LeatherBoots, 1, "minecraft:damage", 12),
                stack(ItemKind::RedDye, 5),
            ]),
        )
        .unwrap();
        assert_eq!(
            boots.components.get_i32("minecraft:dyed_color"),
            Some(0xB02E26)
        );
        assert_eq!(boots.components.get_i32("minecraft:damage"), Some(12));
    }

    #[test]
    fn dyeing_needs_one_garment_some_dye_and_nothing_else() {
        let dyed = |extra: Vec<Slot>| result(3, 3, &laid(extra));
        assert_eq!(dyed(vec![stack(ItemKind::LeatherBoots, 1)]), None);
        assert_eq!(dyed(vec![stack(ItemKind::RedDye, 1)]), None);
        assert_eq!(
            dyed(vec![
                stack(ItemKind::LeatherBoots, 1),
                stack(ItemKind::LeatherBoots, 1),
                stack(ItemKind::RedDye, 1)
            ]),
            None
        );
        assert_eq!(
            dyed(vec![
                stack(ItemKind::LeatherBoots, 1),
                stack(ItemKind::RedDye, 1),
                stack(ItemKind::Stick, 1)
            ]),
            None
        );
        assert_eq!(
            dyed(vec![
                stack(ItemKind::IronBoots, 1),
                stack(ItemKind::RedDye, 1)
            ]),
            None
        );
    }

    #[test]
    fn a_firework_star_takes_its_colours_shape_and_effects() {
        let star = result(
            3,
            3,
            &laid(vec![
                stack(ItemKind::Gunpowder, 1),
                stack(ItemKind::RedDye, 1),
                stack(ItemKind::BlueDye, 1),
                stack(ItemKind::Feather, 1),
                stack(ItemKind::Diamond, 1),
                stack(ItemKind::GlowstoneDust, 1),
            ]),
        )
        .unwrap();
        assert_eq!(star.item, ItemKind::FireworkStar.id());
        let explosion = star
            .components
            .get_compound("minecraft:firework_explosion")
            .unwrap();
        assert_eq!(explosion.get_str("shape"), Some("burst"));
        assert_eq!(
            colors_of(&star, "minecraft:firework_explosion", "colors"),
            vec![0xB3312C, 0x253192]
        );
        assert_eq!(explosion.get_bool("has_trail"), Some(true));
        assert_eq!(explosion.get_bool("has_twinkle"), Some(true));
    }

    #[test]
    fn a_plain_firework_star_is_a_small_ball_without_effects() {
        let star = result(
            3,
            3,
            &laid(vec![
                stack(ItemKind::Gunpowder, 1),
                stack(ItemKind::LimeDye, 1),
            ]),
        )
        .unwrap();
        let explosion = star
            .components
            .get_compound("minecraft:firework_explosion")
            .unwrap();
        assert_eq!(explosion.get_str("shape"), Some("small_ball"));
        assert_eq!(explosion.get_bool("has_trail"), None);
        assert_eq!(explosion.get_bool("has_twinkle"), None);
        assert!(explosion.get_list("fade_colors").is_none());
    }

    #[test]
    fn a_firework_star_needs_fuel_a_dye_and_at_most_one_of_the_rest() {
        let make = |items: Vec<Slot>| result(3, 3, &laid(items));
        let dye = || stack(ItemKind::RedDye, 1);
        let gunpowder = || stack(ItemKind::Gunpowder, 1);
        assert_eq!(make(vec![dye(), stack(ItemKind::Feather, 1)]), None);
        assert_eq!(make(vec![gunpowder(), stack(ItemKind::Feather, 1)]), None);
        assert_eq!(make(vec![gunpowder(), gunpowder(), dye()]), None);
        assert_eq!(
            make(vec![
                gunpowder(),
                dye(),
                stack(ItemKind::GlowstoneDust, 1),
                stack(ItemKind::GlowstoneDust, 1)
            ]),
            None
        );
        assert_eq!(
            make(vec![
                gunpowder(),
                dye(),
                stack(ItemKind::Feather, 1),
                stack(ItemKind::GoldNugget, 1)
            ]),
            None
        );
        assert_eq!(
            make(vec![
                gunpowder(),
                dye(),
                stack(ItemKind::Diamond, 1),
                stack(ItemKind::Diamond, 1)
            ]),
            None
        );
        assert_eq!(
            make(vec![gunpowder(), dye(), stack(ItemKind::Stick, 1)]),
            None
        );
        assert!(make(vec![gunpowder(), dye(), dye(), dye()]).is_some());
        for (shape, name) in [
            (ItemKind::FireCharge, "large_ball"),
            (ItemKind::GoldNugget, "star"),
            (ItemKind::CreeperHead, "creeper"),
        ] {
            let star = make(vec![gunpowder(), dye(), stack(shape, 1)]).unwrap();
            let explosion = star
                .components
                .get_compound("minecraft:firework_explosion")
                .unwrap();
            assert_eq!(explosion.get_str("shape"), Some(name));
        }
    }

    #[test]
    fn fading_a_star_replaces_its_fade_colours_and_keeps_the_rest() {
        let explosion = Compound::new()
            .with("shape", "star")
            .with("colors", Tag::List(vec![Tag::Int(0xB3312C)]))
            .with("fade_colors", Tag::List(vec![Tag::Int(1)]))
            .with("has_trail", true);
        let faded = result(
            3,
            3,
            &laid(vec![
                with(
                    ItemKind::FireworkStar,
                    1,
                    "minecraft:firework_explosion",
                    explosion,
                ),
                stack(ItemKind::BlueDye, 1),
                stack(ItemKind::GreenDye, 1),
            ]),
        )
        .unwrap();
        assert_eq!(
            colors_of(&faded, "minecraft:firework_explosion", "fade_colors"),
            vec![0x253192, 0x3B511A]
        );
        assert_eq!(
            colors_of(&faded, "minecraft:firework_explosion", "colors"),
            vec![0xB3312C]
        );
        let compound = faded
            .components
            .get_compound("minecraft:firework_explosion")
            .unwrap();
        assert_eq!(compound.get_str("shape"), Some("star"));
        assert_eq!(compound.get_bool("has_trail"), Some(true));
        assert_eq!(
            result(3, 3, &laid(vec![stack(ItemKind::FireworkStar, 1)])),
            None
        );
        assert_eq!(
            result(
                3,
                3,
                &laid(vec![
                    stack(ItemKind::FireworkStar, 1),
                    stack(ItemKind::Stick, 1)
                ])
            ),
            None
        );
    }

    #[test]
    fn a_rocket_flies_as_long_as_it_has_gunpowder_and_carries_the_stars() {
        let star = with(
            ItemKind::FireworkStar,
            1,
            "minecraft:firework_explosion",
            Compound::new().with("shape", "creeper"),
        );
        let rocket = result(
            3,
            3,
            &laid(vec![
                stack(ItemKind::Paper, 1),
                stack(ItemKind::Gunpowder, 1),
                stack(ItemKind::Gunpowder, 1),
                star.clone(),
                star,
            ]),
        )
        .unwrap();
        assert_eq!(rocket.item, ItemKind::FireworkRocket.id());
        assert_eq!(rocket.count.get(), 3);
        let fireworks = rocket
            .components
            .get_compound("minecraft:fireworks")
            .unwrap();
        assert_eq!(fireworks.get_i32("flight_duration"), Some(2));
        assert_eq!(fireworks.get_list("explosions").map(<[Tag]>::len), Some(2));
    }

    #[test]
    fn a_rocket_with_one_gunpowder_and_no_star_is_the_default_rocket() {
        let rocket = result(
            3,
            3,
            &laid(vec![
                stack(ItemKind::Paper, 1),
                stack(ItemKind::Gunpowder, 1),
            ]),
        )
        .unwrap();
        assert_eq!(rocket.item, ItemKind::FireworkRocket.id());
        assert_eq!(rocket.count.get(), 3);
        assert!(rocket.components.is_empty());
    }

    #[test]
    fn a_rocket_takes_one_paper_and_one_to_three_gunpowder() {
        let paper = || stack(ItemKind::Paper, 1);
        let powder = || stack(ItemKind::Gunpowder, 1);
        let make = |items: Vec<Slot>| result(3, 3, &laid(items));
        assert!(make(vec![paper(), powder(), powder(), powder()]).is_some());
        assert_eq!(
            make(vec![paper(), powder(), powder(), powder(), powder()]),
            None
        );
        assert_eq!(make(vec![paper(), paper(), powder()]), None);
        assert_eq!(make(vec![powder(), stack(ItemKind::FireworkStar, 1)]), None);
        assert_eq!(make(vec![paper(), stack(ItemKind::FireworkStar, 1)]), None);
        assert_eq!(
            make(vec![paper(), powder(), stack(ItemKind::Stick, 1)]),
            None
        );
    }

    fn pickaxe(damage: i32) -> Slot {
        if damage == 0 {
            stack(ItemKind::IronPickaxe, 1)
        } else {
            with(ItemKind::IronPickaxe, 1, "minecraft:damage", damage)
        }
    }

    #[test]
    fn two_damaged_tools_make_one_with_the_durability_left_plus_a_bonus() {
        let tool = result(3, 3, &laid(vec![pickaxe(200), pickaxe(200)])).unwrap();
        assert_eq!(tool.item, ItemKind::IronPickaxe.id());
        assert_eq!(tool.count.get(), 1);
        assert_eq!(tool.components.get_i32("minecraft:damage"), Some(138));
        let whole = result(3, 3, &laid(vec![pickaxe(100), pickaxe(0)])).unwrap();
        assert_eq!(whole.components.get_i32("minecraft:damage"), None);
        let some = result(3, 3, &laid(vec![pickaxe(249), pickaxe(249)])).unwrap();
        assert_eq!(
            some.components.get_i32("minecraft:damage"),
            Some(250 - 2 - 12)
        );
    }

    #[test]
    fn repair_wants_two_single_copies_of_the_same_damageable_item() {
        let make = |items: Vec<Slot>| result(3, 3, &laid(items));
        assert_eq!(make(vec![pickaxe(5)]), None);
        assert_eq!(make(vec![pickaxe(5), pickaxe(5), pickaxe(5)]), None);
        assert_eq!(make(vec![pickaxe(5), stack(ItemKind::IronAxe, 1)]), None);
        assert_eq!(make(vec![pickaxe(5), stack(ItemKind::Stick, 1)]), None);
        assert_eq!(
            make(vec![stack(ItemKind::IronPickaxe, 2), pickaxe(5)]),
            None
        );
        assert_eq!(
            make(vec![stack(ItemKind::Stick, 1), stack(ItemKind::Stick, 1)]),
            None
        );
    }

    #[test]
    fn repair_keeps_only_the_curses_at_their_highest_level() {
        let curses = |name: &str, level: i32| {
            Compound::new()
                .with(name, level)
                .with("minecraft:sharpness", 5)
        };
        let mut first = pickaxe(10).unwrap();
        first.components.insert(
            "minecraft:enchantments",
            curses("minecraft:vanishing_curse", 1),
        );
        first.components.insert("minecraft:custom_name", "named");
        let mut second = pickaxe(10).unwrap();
        second.components.insert(
            "minecraft:enchantments",
            Compound::new()
                .with("minecraft:binding_curse", 1)
                .with("minecraft:vanishing_curse", 3)
                .with("minecraft:efficiency", 4),
        );
        let tool = result(3, 3, &laid(vec![Some(first), Some(second)])).unwrap();
        let kept = tool
            .components
            .get_compound("minecraft:enchantments")
            .unwrap();
        assert_eq!(kept.len(), 2);
        assert_eq!(kept.get_i32("minecraft:vanishing_curse"), Some(3));
        assert_eq!(kept.get_i32("minecraft:binding_curse"), Some(1));
        assert!(tool.components.get("minecraft:custom_name").is_none());
    }

    #[test]
    fn repair_without_curses_leaves_no_enchantments() {
        let mut first = pickaxe(10).unwrap();
        first.components.insert(
            "minecraft:enchantments",
            Compound::new().with("minecraft:sharpness", 5),
        );
        let tool = result(3, 3, &laid(vec![Some(first), pickaxe(10)])).unwrap();
        assert!(tool.components.get("minecraft:enchantments").is_none());
    }

    #[test]
    fn repair_follows_a_max_damage_override() {
        let big = |damage: i32| {
            let mut tool = pickaxe(damage).unwrap();
            tool.components.insert("minecraft:max_damage", 1000);
            Some(tool)
        };
        let tool = result(3, 3, &laid(vec![big(500), big(500)])).unwrap();
        assert_eq!(tool.components.get_i32("minecraft:max_damage"), Some(1000));
        assert_eq!(tool.components.get_i32("minecraft:damage"), None);
        let worn = result(3, 3, &laid(vec![big(900), big(900)])).unwrap();
        assert_eq!(worn.components.get_i32("minecraft:damage"), Some(750));
        let plain = result(3, 3, &laid(vec![pickaxe(0), pickaxe(0)])).unwrap();
        assert!(plain.components.get_i32("minecraft:max_damage").is_none());
    }

    fn book(generation: i32) -> Slot {
        with(
            ItemKind::WrittenBook,
            1,
            "minecraft:written_book_content",
            Compound::new()
                .with("title", "t")
                .with("author", "a")
                .with("generation", generation),
        )
    }

    fn generation_of(stack: &ItemStack) -> Option<i32> {
        stack
            .components
            .get_compound("minecraft:written_book_content")
            .map(|content| content.get_i32("generation").unwrap_or(0))
    }

    #[test]
    fn writable_books_turn_a_written_book_into_copies_one_generation_older() {
        let copies = result(
            3,
            3,
            &laid(vec![
                book(0),
                stack(ItemKind::WritableBook, 1),
                stack(ItemKind::WritableBook, 1),
            ]),
        )
        .unwrap();
        assert_eq!(copies.item, ItemKind::WrittenBook.id());
        assert_eq!(copies.count.get(), 2);
        assert_eq!(generation_of(&copies), Some(1));
        let again = result(3, 3, &laid(vec![book(1), stack(ItemKind::WritableBook, 1)])).unwrap();
        assert_eq!(generation_of(&again), Some(2));
        let plain = with(
            ItemKind::WrittenBook,
            1,
            "minecraft:written_book_content",
            Compound::new().with("title", "t").with("author", "a"),
        );
        let copy = result(3, 3, &laid(vec![plain, stack(ItemKind::WritableBook, 1)])).unwrap();
        assert_eq!(generation_of(&copy), Some(1));
    }

    #[test]
    fn a_copy_of_a_copy_cannot_be_copied_again() {
        let make = |items: Vec<Slot>| result(3, 3, &laid(items));
        let writable = || stack(ItemKind::WritableBook, 1);
        assert_eq!(make(vec![book(2), writable()]), None);
        assert_eq!(make(vec![book(3), writable()]), None);
        assert_eq!(make(vec![book(0), book(0), writable()]), None);
        assert_eq!(make(vec![book(0)]), None);
        assert_eq!(make(vec![writable(), writable()]), None);
        assert_eq!(
            make(vec![book(0), writable(), stack(ItemKind::Stick, 1)]),
            None
        );
        assert_eq!(
            make(vec![stack(ItemKind::WrittenBook, 1), writable()]),
            None
        );
    }

    #[test]
    fn cloning_a_book_keeps_the_original_and_uses_the_writable_books() {
        let mut slots = laid(vec![
            stack(ItemKind::WritableBook, 1),
            book(0),
            stack(ItemKind::WritableBook, 1),
        ]);
        assert!(take_result(3, 3, &mut slots).is_empty());
        assert_eq!(slots[0], None);
        assert_eq!(slots[1], book(0));
        assert_eq!(slots[2], None);
    }

    fn banner(kind: ItemKind, layers: usize) -> Slot {
        if layers == 0 {
            return stack(kind, 1);
        }
        let layer = Tag::Compound(
            Compound::new()
                .with("pattern", "minecraft:stripe_bottom")
                .with("color", "red"),
        );
        with(
            kind,
            1,
            "minecraft:banner_patterns",
            Tag::List(vec![layer; layers]),
        )
    }

    #[test]
    fn a_patterned_banner_is_copied_onto_a_blank_one_of_the_same_colour() {
        let copy = result(
            3,
            3,
            &laid(vec![
                banner(ItemKind::WhiteBanner, 0),
                banner(ItemKind::WhiteBanner, 3),
            ]),
        )
        .unwrap();
        assert_eq!(copy.item, ItemKind::WhiteBanner.id());
        assert_eq!(copy.count.get(), 1);
        assert_eq!(layers(&copy), 3);
    }

    #[test]
    fn banner_copying_needs_exactly_one_blank_and_one_patterned_banner() {
        let make = |items: Vec<Slot>| result(3, 3, &laid(items));
        let white = |layers| banner(ItemKind::WhiteBanner, layers);
        assert_eq!(make(vec![white(1), white(2)]), None);
        assert_eq!(make(vec![white(0), white(0)]), None);
        assert_eq!(make(vec![white(0), white(7)]), None);
        assert!(make(vec![white(0), white(6)]).is_some());
        assert_eq!(make(vec![white(0), banner(ItemKind::RedBanner, 2)]), None);
        assert_eq!(make(vec![white(2)]), None);
        assert_eq!(make(vec![white(0), white(2), white(0)]), None);
        assert_eq!(make(vec![white(2), stack(ItemKind::Stick, 1)]), None);
    }

    #[test]
    fn copying_a_banner_leaves_the_patterned_one_behind() {
        let mut slots = laid(vec![
            banner(ItemKind::WhiteBanner, 0),
            banner(ItemKind::WhiteBanner, 3),
        ]);
        assert!(take_result(3, 3, &mut slots).is_empty());
        assert_eq!(slots[0], None);
        assert_eq!(slots[1], banner(ItemKind::WhiteBanner, 3));
    }

    #[test]
    fn a_banner_decorates_a_shield_with_its_colour_and_patterns() {
        let shield = result(
            3,
            3,
            &laid(vec![
                with(ItemKind::Shield, 1, "minecraft:damage", 20),
                banner(ItemKind::RedBanner, 2),
            ]),
        )
        .unwrap();
        assert_eq!(shield.item, ItemKind::Shield.id());
        assert_eq!(
            shield.components.get_str("minecraft:base_color"),
            Some("red")
        );
        assert_eq!(layers(&shield), 2);
        assert_eq!(shield.components.get_i32("minecraft:damage"), Some(20));
        let plain = result(
            3,
            3,
            &laid(vec![
                stack(ItemKind::Shield, 1),
                banner(ItemKind::BlueBanner, 0),
            ]),
        )
        .unwrap();
        assert_eq!(
            plain.components.get_str("minecraft:base_color"),
            Some("blue")
        );
        assert!(plain.components.get("minecraft:banner_patterns").is_none());
    }

    #[test]
    fn a_shield_is_decorated_only_when_it_has_no_patterns_and_one_banner_is_given() {
        let make = |items: Vec<Slot>| result(3, 3, &laid(items));
        let shield = || stack(ItemKind::Shield, 1);
        let red = |layers| banner(ItemKind::RedBanner, layers);
        assert_eq!(make(vec![shield()]), None);
        assert_eq!(make(vec![red(1)]), None);
        assert_eq!(make(vec![shield(), red(0), red(0)]), None);
        assert_eq!(make(vec![shield(), shield(), red(0)]), None);
        assert_eq!(make(vec![shield(), stack(ItemKind::Stick, 1)]), None);
        let patterned = with(
            ItemKind::Shield,
            1,
            "minecraft:banner_patterns",
            Tag::List(vec![Tag::Compound(Compound::new())]),
        );
        assert_eq!(make(vec![patterned, red(0)]), None);
        let based = with(ItemKind::Shield, 1, "minecraft:base_color", "green");
        let redone = make(vec![based, red(0)]).unwrap();
        assert_eq!(
            redone.components.get_str("minecraft:base_color"),
            Some("red")
        );
    }

    fn pot(back: ItemKind, left: ItemKind, right: ItemKind, front: ItemKind) -> Vec<Slot> {
        let mut grid = vec![None; 9];
        grid[1] = stack(back, 1);
        grid[3] = stack(left, 1);
        grid[5] = stack(right, 1);
        grid[7] = stack(front, 1);
        grid
    }

    #[test]
    fn sherds_around_the_empty_centre_make_a_decorated_pot() {
        let decorated = result(
            3,
            3,
            &pot(
                ItemKind::Brick,
                ItemKind::ArcherPotterySherd,
                ItemKind::AnglerPotterySherd,
                ItemKind::BladePotterySherd,
            ),
        )
        .unwrap();
        assert_eq!(decorated.item, ItemKind::DecoratedPot.id());
        let sides = decorated
            .components
            .get_compound("minecraft:pot_decorations")
            .unwrap();
        let id = |side: &str| sides.get_compound(side).and_then(|s| s.get_str("id"));
        assert_eq!(id("back"), Some("minecraft:brick"));
        assert_eq!(id("left"), Some("minecraft:archer_pottery_sherd"));
        assert_eq!(id("right"), Some("minecraft:angler_pottery_sherd"));
        assert_eq!(id("front"), Some("minecraft:blade_pottery_sherd"));
    }

    #[test]
    fn a_pot_of_four_bricks_is_the_plain_pot() {
        let plain = result(
            3,
            3,
            &pot(
                ItemKind::Brick,
                ItemKind::Brick,
                ItemKind::Brick,
                ItemKind::Brick,
            ),
        );
        assert!(plain.unwrap().components.is_empty());
    }

    #[test]
    fn the_pot_pattern_needs_the_plus_shape_and_pottery_items() {
        let mut shifted = pot(
            ItemKind::Brick,
            ItemKind::Brick,
            ItemKind::Brick,
            ItemKind::Brick,
        );
        shifted.swap(1, 0);
        assert_eq!(result(3, 3, &shifted), None);
        let mut corner = pot(
            ItemKind::Brick,
            ItemKind::Brick,
            ItemKind::Brick,
            ItemKind::Brick,
        );
        corner[0] = stack(ItemKind::Brick, 1);
        assert_eq!(result(3, 3, &corner), None);
        assert_eq!(
            result(
                3,
                3,
                &pot(
                    ItemKind::Brick,
                    ItemKind::Brick,
                    ItemKind::Brick,
                    ItemKind::Stick
                )
            ),
            None
        );
    }

    fn lingering() -> Slot {
        with(
            ItemKind::LingeringPotion,
            1,
            "minecraft:potion_contents",
            Compound::new().with("potion", "minecraft:poison"),
        )
    }

    #[test]
    fn a_lingering_potion_ringed_by_arrows_makes_tipped_arrows() {
        let mut grid = vec![stack(ItemKind::Arrow, 1); 9];
        grid[4] = lingering();
        let arrows = result(3, 3, &grid).unwrap();
        assert_eq!(arrows.item, ItemKind::TippedArrow.id());
        assert_eq!(arrows.count.get(), 8);
        assert_eq!(
            arrows
                .components
                .get_compound("minecraft:potion_contents")
                .and_then(|contents| contents.get_str("potion")),
            Some("minecraft:poison")
        );
    }

    #[test]
    fn the_tipped_arrow_recipe_needs_the_potion_in_the_middle_and_eight_arrows() {
        let mut grid = vec![stack(ItemKind::Arrow, 1); 9];
        grid[4] = lingering();
        grid[0] = None;
        assert_eq!(result(3, 3, &grid), None);
        let mut off = vec![stack(ItemKind::Arrow, 1); 9];
        off[0] = lingering();
        assert_eq!(result(3, 3, &off), None);
        let mut plain = vec![stack(ItemKind::Arrow, 1); 9];
        plain[4] = stack(ItemKind::Potion, 1);
        assert_eq!(result(3, 3, &plain), None);
    }

    #[test]
    fn the_potion_is_used_up_and_leaves_no_bottle() {
        let mut grid = vec![stack(ItemKind::Arrow, 1); 9];
        grid[4] = lingering();
        assert!(take_result(3, 3, &mut grid).is_empty());
        assert!(grid.iter().all(Option::is_none));
    }

    #[test]
    fn the_blend_of_nothing_in_particular_stays_in_range() {
        assert_eq!(blended(None, &[0]), 0xF9FFFE);
        assert_eq!(blended(None, &[15]), 0x1D1D21);
        assert_eq!(blended(Some(0), &[15]), 0x0E0E10);
    }

    #[test]
    fn the_stack_sizes_in_the_grid_do_not_matter() {
        assert_eq!(
            crafted(2, 2, &[(0, ItemKind::OakLog, 64)]),
            stack(ItemKind::OakPlanks, 4)
        );
    }

    #[test]
    fn a_transmute_recipe_keeps_the_components_of_its_input() {
        let mut shulker = ItemStack::new(ItemKind::ShulkerBox.id(), NonZeroU8::MIN);
        shulker.components.insert("minecraft:custom_name", "kept");
        let mut slots = vec![None; 9];
        slots[0] = Some(shulker);
        slots[1] = stack(ItemKind::BlackDye, 1);
        let dyed = result(3, 3, &slots).unwrap();
        assert_eq!(dyed.item, ItemKind::BlackShulkerBox.id());
        assert_eq!(dyed.count.get(), 1);
        assert_eq!(
            dyed.components.get_str("minecraft:custom_name"),
            Some("kept")
        );
    }

    #[test]
    fn a_transmute_into_the_item_it_already_is_does_nothing() {
        assert_eq!(
            crafted(
                3,
                3,
                &[
                    (0, ItemKind::BlackShulkerBox, 1),
                    (1, ItemKind::BlackDye, 1)
                ]
            ),
            None
        );
        assert!(
            crafted(
                3,
                3,
                &[
                    (0, ItemKind::BlackShulkerBox, 1),
                    (1, ItemKind::WhiteDye, 1)
                ]
            )
            .is_some()
        );
    }

    #[test]
    fn a_transmute_needs_one_input_and_only_materials_besides() {
        assert_eq!(
            crafted(
                3,
                3,
                &[
                    (0, ItemKind::ShulkerBox, 1),
                    (1, ItemKind::BlackDye, 1),
                    (2, ItemKind::BlackDye, 1)
                ]
            ),
            None
        );
        assert_eq!(
            crafted(
                3,
                3,
                &[
                    (0, ItemKind::ShulkerBox, 1),
                    (1, ItemKind::WhiteShulkerBox, 1),
                    (2, ItemKind::BlackDye, 1)
                ]
            ),
            None
        );
        assert_eq!(crafted(3, 3, &[(1, ItemKind::BlackDye, 1)]), None);
    }

    #[test]
    fn cloning_a_map_counts_every_empty_map_as_a_copy() {
        let mut filled = ItemStack::new(ItemKind::FilledMap.id(), NonZeroU8::MIN);
        filled.components.insert("minecraft:map_id", 7);
        let mut slots = vec![None; 9];
        slots[0] = Some(filled);
        for slot in &mut slots[1..4] {
            *slot = stack(ItemKind::Map, 1);
        }
        let copies = result(3, 3, &slots).unwrap();
        assert_eq!(copies.item, ItemKind::FilledMap.id());
        assert_eq!(copies.count.get(), 4);
        assert_eq!(copies.components.get_i32("minecraft:map_id"), Some(7));
        for slot in &mut slots[4..] {
            *slot = stack(ItemKind::Map, 1);
        }
        assert_eq!(result(3, 3, &slots).unwrap().count.get(), 9);
    }

    #[test]
    fn cloning_keeps_the_map_kind() {
        let slots = grid(
            3,
            3,
            &[(0, ItemKind::DesertVillageMap, 1), (1, ItemKind::Map, 1)],
        );
        let copy = result(3, 3, &slots).unwrap();
        assert_eq!(copy.item, ItemKind::DesertVillageMap.id());
        assert_eq!(copy.count.get(), 2);
    }

    #[test]
    fn a_suspicious_stew_carries_its_effect() {
        let stew = crafted(
            3,
            3,
            &[
                (0, ItemKind::BrownMushroom, 1),
                (1, ItemKind::RedMushroom, 1),
                (2, ItemKind::Bowl, 1),
                (3, ItemKind::Dandelion, 1),
            ],
        )
        .unwrap();
        assert_eq!(stew.item, ItemKind::SuspiciousStew.id());
        let effects = stew
            .components
            .get_list("minecraft:suspicious_stew_effects")
            .unwrap();
        assert_eq!(effects.len(), 1);
        let effect = effects[0].as_compound().unwrap();
        assert_eq!(effect.get_str("id"), Some("minecraft:saturation"));
        assert_eq!(effect.get_i32("duration"), Some(7));
    }

    #[test]
    fn taking_the_result_uses_one_of_each_ingredient() {
        let mut slots = grid(
            3,
            3,
            &[(0, ItemKind::OakPlanks, 5), (3, ItemKind::OakPlanks, 1)],
        );
        assert!(take_result(3, 3, &mut slots).is_empty());
        assert_eq!(slots[0], stack(ItemKind::OakPlanks, 4));
        assert_eq!(slots[3], None);
    }

    #[test]
    fn taking_the_result_leaves_the_crafting_remainder() {
        let mut slots = grid(
            3,
            3,
            &[
                (0, ItemKind::MilkBucket, 1),
                (1, ItemKind::MilkBucket, 1),
                (2, ItemKind::MilkBucket, 1),
                (3, ItemKind::Sugar, 1),
                (4, ItemKind::Egg, 1),
                (5, ItemKind::Sugar, 1),
                (6, ItemKind::Wheat, 1),
                (7, ItemKind::Wheat, 1),
                (8, ItemKind::Wheat, 1),
            ],
        );
        assert_eq!(result(3, 3, &slots), stack(ItemKind::Cake, 1));
        assert!(take_result(3, 3, &mut slots).is_empty());
        assert!(
            slots[..3]
                .iter()
                .all(|slot| *slot == stack(ItemKind::Bucket, 1))
        );
        assert!(slots[3..].iter().all(Option::is_none));
    }

    #[test]
    fn a_remainder_that_cannot_take_the_emptied_slot_is_handed_over() {
        let mut slots = grid(2, 2, &[(0, ItemKind::HoneyBottle, 4)]);
        assert_eq!(result(2, 2, &slots), stack(ItemKind::Sugar, 3));
        assert_eq!(
            take_result(2, 2, &mut slots),
            vec![ItemStack::new(ItemKind::GlassBottle.id(), NonZeroU8::MIN)]
        );
        assert_eq!(slots[0], stack(ItemKind::HoneyBottle, 3));
    }

    fn installed(recipes: Vec<crate::plugin_recipes::PluginRecipe>) {
        let mut set = crate::plugin_recipes::PluginRecipes::default();
        for recipe in recipes {
            set.add(recipe);
        }
        crate::plugin_recipes::activate(&std::sync::Arc::new(set));
    }

    fn custom(
        id: &str,
        body: crate::plugin_recipes::Body,
        result: ItemKind,
        amount: u8,
        override_vanilla: bool,
    ) -> crate::plugin_recipes::PluginRecipe {
        let (id, display_id) = crate::plugin_recipes::intern_id(id);
        crate::plugin_recipes::PluginRecipe {
            id,
            display_id,
            owner: "test".to_owned(),
            body,
            result: ItemStack::new(result.id(), NonZeroU8::new(amount).unwrap()),
            group: String::new(),
            category: crate::plugin_recipes::Category::Misc,
            override_vanilla,
            auto_discover: false,
        }
    }

    fn choice(items: &[ItemKind]) -> ItemSet {
        let ids: Vec<_> = items.iter().map(|item| item.id()).collect();
        crate::plugin_recipes::intern_items(&ids)
    }

    fn bridge(mirror: bool) -> crate::plugin_recipes::Body {
        let stick = choice(&[ItemKind::Stick]);
        let planks = choice(&[ItemKind::OakPlanks, ItemKind::SprucePlanks]);
        crate::plugin_recipes::Body::Shaped {
            width: 2,
            height: 2,
            mirror,
            cells: vec![Some(stick), Some(planks), None, Some(planks)],
        }
    }

    #[test]
    fn a_plugin_shaped_recipe_with_a_choice_crafts_in_both_grid_sizes() {
        installed(vec![custom(
            "t:bridge",
            bridge(true),
            ItemKind::Diamond,
            3,
            false,
        )]);
        let filled = [
            (0, ItemKind::Stick, 1),
            (1, ItemKind::SprucePlanks, 1),
            (3, ItemKind::OakPlanks, 1),
        ];
        assert_eq!(crafted(2, 2, &filled), stack(ItemKind::Diamond, 3));
        let shifted = [
            (4, ItemKind::Stick, 1),
            (5, ItemKind::OakPlanks, 1),
            (8, ItemKind::OakPlanks, 1),
        ];
        assert_eq!(crafted(3, 3, &shifted), stack(ItemKind::Diamond, 3));
        let wrong = [
            (0, ItemKind::Stick, 1),
            (1, ItemKind::Dirt, 1),
            (3, ItemKind::OakPlanks, 1),
        ];
        assert_eq!(crafted(2, 2, &wrong), None);
        installed(Vec::new());
        assert_eq!(crafted(2, 2, &filled), None);
    }

    #[test]
    fn a_plugin_shaped_recipe_matches_its_mirror_only_when_allowed() {
        let mirrored = [
            (1, ItemKind::Stick, 1),
            (0, ItemKind::OakPlanks, 1),
            (2, ItemKind::OakPlanks, 1),
        ];
        installed(vec![custom(
            "t:bridge",
            bridge(true),
            ItemKind::Diamond,
            1,
            false,
        )]);
        assert_eq!(crafted(2, 2, &mirrored), stack(ItemKind::Diamond, 1));
        installed(vec![custom(
            "t:bridge",
            bridge(false),
            ItemKind::Diamond,
            1,
            false,
        )]);
        assert_eq!(crafted(2, 2, &mirrored), None);
        installed(Vec::new());
    }

    #[test]
    fn a_plugin_shapeless_recipe_ignores_the_positions() {
        let body = crate::plugin_recipes::Body::Shapeless {
            ingredients: vec![
                choice(&[ItemKind::Stick]),
                choice(&[ItemKind::Dirt, ItemKind::Stone]),
            ],
        };
        installed(vec![custom("t:pair", body, ItemKind::Emerald, 2, false)]);
        assert_eq!(
            crafted(3, 3, &[(8, ItemKind::Stone, 1), (2, ItemKind::Stick, 1)]),
            stack(ItemKind::Emerald, 2)
        );
        assert_eq!(crafted(2, 2, &[(0, ItemKind::Stick, 1)]), None);
        installed(Vec::new());
    }

    #[test]
    fn a_vanilla_recipe_wins_over_a_plugin_one_unless_it_overrides() {
        let body = || crate::plugin_recipes::Body::Shapeless {
            ingredients: vec![choice(&[ItemKind::OakLog])],
        };
        let log = [(0, ItemKind::OakLog, 1)];
        installed(vec![custom("t:logs", body(), ItemKind::Diamond, 1, false)]);
        assert_eq!(crafted(2, 2, &log), stack(ItemKind::OakPlanks, 4));
        assert_eq!(
            recipe(2, 2, &grid(2, 2, &log)).unwrap().id,
            "minecraft:oak_planks"
        );
        installed(vec![custom("t:logs", body(), ItemKind::Diamond, 1, true)]);
        assert_eq!(crafted(2, 2, &log), stack(ItemKind::Diamond, 1));
        let found = recipe(2, 2, &grid(2, 2, &log)).unwrap();
        assert_eq!((found.id, found.repair), ("t:logs", false));
        installed(Vec::new());
        assert_eq!(crafted(2, 2, &log), stack(ItemKind::OakPlanks, 4));
    }

    #[test]
    fn a_plugin_recipe_keeps_the_crafting_remainder_of_its_ingredients() {
        let body = crate::plugin_recipes::Body::Shapeless {
            ingredients: vec![choice(&[ItemKind::MilkBucket]), choice(&[ItemKind::Stick])],
        };
        installed(vec![custom("t:milk", body, ItemKind::Diamond, 1, false)]);
        let mut slots = grid(
            2,
            2,
            &[(0, ItemKind::MilkBucket, 1), (1, ItemKind::Stick, 2)],
        );
        assert_eq!(result(2, 2, &slots), stack(ItemKind::Diamond, 1));
        assert!(take_result(2, 2, &mut slots).is_empty());
        assert_eq!(slots[0], stack(ItemKind::Bucket, 1));
        assert_eq!(slots[1], stack(ItemKind::Stick, 1));
        installed(Vec::new());
    }
}
