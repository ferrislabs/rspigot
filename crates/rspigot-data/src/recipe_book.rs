use crate::generated::recipe_book::RECIPES;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BookRecipe {
    pub id: &'static str,
    pub category: u8,
    pub group: &'static str,
}

pub fn all() -> &'static [BookRecipe] {
    &RECIPES
}

pub fn find(id: &str) -> Option<(usize, &'static BookRecipe)> {
    RECIPES
        .binary_search_by(|recipe| recipe.id.cmp(id))
        .ok()
        .map(|index| (index, &RECIPES[index]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipes_are_sorted_so_that_a_search_finds_them() {
        assert!(all().windows(2).all(|pair| pair[0].id < pair[1].id));
        let (index, recipe) = find("minecraft:acacia_planks").expect("a recipe");
        assert_eq!(all()[index], *recipe);
        assert_eq!(recipe.group, "planks");
        assert_eq!(recipe.category, 0);
        assert!(find("minecraft:nope").is_none());
    }
}
