use rspigot_core::plugins::{
    CookingKind, IngredientSpec, RecipeKind, RecipeShape, RecipeSpec, RecipeView,
};
use rspigot_domain::plugin_recipes::Category;
use rspigot_plugin_protocol::v1::{self, recipe_def, recipe_ingredient};

use super::Malformed;
use super::shared::{item, required};

fn ingredient(wire: v1::RecipeIngredient) -> Result<IngredientSpec, Malformed> {
    Ok(match required(wire.choice)? {
        recipe_ingredient::Choice::Items(items) => IngredientSpec::Items(items.keys),
        recipe_ingredient::Choice::Tag(tag) => IngredientSpec::Tag(tag),
    })
}

fn required_ingredient(wire: Option<v1::RecipeIngredient>) -> Result<IngredientSpec, Malformed> {
    ingredient(required(wire)?)
}

fn category(raw: i32) -> Result<Category, Malformed> {
    match v1::RecipeCategory::try_from(raw) {
        Ok(v1::RecipeCategory::Building) => Ok(Category::Building),
        Ok(v1::RecipeCategory::Redstone) => Ok(Category::Redstone),
        Ok(v1::RecipeCategory::Equipment) => Ok(Category::Equipment),
        Ok(v1::RecipeCategory::Misc | v1::RecipeCategory::Unspecified) => Ok(Category::Misc),
        Ok(v1::RecipeCategory::Food) => Ok(Category::Food),
        Err(_) => Err(Malformed::Enum {
            field: "category",
            value: raw,
        }),
    }
}

fn cooking_kind(raw: i32) -> Result<CookingKind, Malformed> {
    match v1::RecipeKind::try_from(raw) {
        Ok(v1::RecipeKind::Furnace) => Ok(CookingKind::Smelting),
        Ok(v1::RecipeKind::BlastFurnace) => Ok(CookingKind::Blasting),
        Ok(v1::RecipeKind::Smoker) => Ok(CookingKind::Smoking),
        Ok(v1::RecipeKind::Campfire) => Ok(CookingKind::Campfire),
        _ => Err(Malformed::Enum {
            field: "cooking kind",
            value: raw,
        }),
    }
}

pub(super) fn spec(def: v1::RecipeDef) -> Result<RecipeSpec, Malformed> {
    let shape = match required(def.body)? {
        recipe_def::Body::Shaped(shaped) => RecipeShape::Shaped {
            pattern: shaped.pattern,
            keys: shaped
                .keys
                .into_iter()
                .map(|key| {
                    let mut symbols = key.symbol.chars();
                    let symbol = symbols.next().filter(|_| symbols.next().is_none()).ok_or(
                        Malformed::OutOfRange {
                            field: "recipe key length",
                            value: i64::try_from(key.symbol.chars().count()).unwrap_or(i64::MAX),
                        },
                    )?;
                    Ok((symbol, required_ingredient(key.ingredient)?))
                })
                .collect::<Result<_, Malformed>>()?,
            mirror: shaped.mirror,
        },
        recipe_def::Body::Shapeless(shapeless) => RecipeShape::Shapeless(
            shapeless
                .ingredients
                .into_iter()
                .map(ingredient)
                .collect::<Result<_, _>>()?,
        ),
        recipe_def::Body::Cooking(cooking) => RecipeShape::Cooking {
            kind: cooking_kind(cooking.kind)?,
            ingredient: required_ingredient(cooking.ingredient)?,
            experience: cooking.experience,
            time: cooking.time,
        },
        recipe_def::Body::Stonecutting(choice) => RecipeShape::Stonecutting(ingredient(choice)?),
        recipe_def::Body::Smithing(smithing) => RecipeShape::Smithing {
            template: required_ingredient(smithing.template)?,
            base: required_ingredient(smithing.base)?,
            addition: required_ingredient(smithing.addition)?,
        },
    };
    Ok(RecipeSpec {
        id: def.id,
        shape,
        result: item(required(def.result)?),
        group: def.group,
        category: category(def.category)?,
        override_vanilla: def.override_vanilla,
        auto_discover: def.auto_discover,
    })
}

const fn wire_kind(kind: RecipeKind) -> v1::RecipeKind {
    match kind {
        RecipeKind::Shaped => v1::RecipeKind::Shaped,
        RecipeKind::Shapeless => v1::RecipeKind::Shapeless,
        RecipeKind::Furnace => v1::RecipeKind::Furnace,
        RecipeKind::BlastFurnace => v1::RecipeKind::BlastFurnace,
        RecipeKind::Smoker => v1::RecipeKind::Smoker,
        RecipeKind::Campfire => v1::RecipeKind::Campfire,
        RecipeKind::Stonecutting => v1::RecipeKind::Stonecutting,
        RecipeKind::Smithing => v1::RecipeKind::Smithing,
        RecipeKind::Special => v1::RecipeKind::Special,
    }
}

pub(super) fn wire_list(views: Vec<RecipeView>) -> v1::RecipeList {
    v1::RecipeList {
        recipes: views
            .into_iter()
            .map(|view| v1::RecipeInfo {
                id: view.id,
                kind: wire_kind(view.kind) as i32,
                plugin: view.plugin,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(name: &str) -> v1::RecipeIngredient {
        v1::RecipeIngredient {
            choice: Some(recipe_ingredient::Choice::Tag(name.to_owned())),
        }
    }

    fn def(body: recipe_def::Body) -> v1::RecipeDef {
        v1::RecipeDef {
            id: "t:r".to_owned(),
            result: Some(v1::Item {
                key: "minecraft:stick".to_owned(),
                count: 2,
                ..Default::default()
            }),
            group: "g".to_owned(),
            category: v1::RecipeCategory::Food as i32,
            override_vanilla: true,
            auto_discover: true,
            body: Some(body),
        }
    }

    #[test]
    fn a_shaped_definition_becomes_a_spec() {
        let wire = def(recipe_def::Body::Shaped(v1::ShapedRecipe {
            pattern: vec!["A".to_owned()],
            keys: vec![v1::RecipeKey {
                symbol: "A".to_owned(),
                ingredient: Some(tag("minecraft:planks")),
            }],
            mirror: false,
        }));
        let spec = spec(wire).unwrap();
        assert_eq!(spec.id, "t:r");
        assert_eq!(spec.result.count, 2);
        assert_eq!(spec.category, Category::Food);
        assert!(spec.override_vanilla && spec.auto_discover);
        assert_eq!(
            spec.shape,
            RecipeShape::Shaped {
                pattern: vec!["A".to_owned()],
                keys: vec![('A', IngredientSpec::Tag("minecraft:planks".to_owned()))],
                mirror: false,
            }
        );
    }

    #[test]
    fn the_other_bodies_and_their_kinds_convert() {
        let cooking = def(recipe_def::Body::Cooking(v1::CookingRecipe {
            kind: v1::RecipeKind::Campfire as i32,
            ingredient: Some(tag("minecraft:logs")),
            experience: 0.5,
            time: 100,
        }));
        assert!(matches!(
            spec(cooking).unwrap().shape,
            RecipeShape::Cooking {
                kind: CookingKind::Campfire,
                time: 100,
                ..
            }
        ));
        let wrong = def(recipe_def::Body::Cooking(v1::CookingRecipe {
            kind: v1::RecipeKind::Shaped as i32,
            ingredient: Some(tag("minecraft:logs")),
            experience: 0.0,
            time: 1,
        }));
        assert!(spec(wrong).is_err());
        let smithing = def(recipe_def::Body::Smithing(v1::SmithingRecipe {
            template: Some(tag("a:b")),
            base: Some(tag("a:c")),
            addition: None,
        }));
        assert!(spec(smithing).is_err());
        let long = def(recipe_def::Body::Shaped(v1::ShapedRecipe {
            pattern: vec!["AB".to_owned()],
            keys: vec![v1::RecipeKey {
                symbol: "AB".to_owned(),
                ingredient: Some(tag("a:b")),
            }],
            mirror: true,
        }));
        assert!(spec(long).is_err());
    }

    #[test]
    fn a_listing_carries_the_kind_and_the_owner_flag() {
        let list = wire_list(vec![RecipeView {
            id: "t:r".to_owned(),
            kind: RecipeKind::Smoker,
            plugin: true,
        }]);
        assert_eq!(list.recipes[0].kind, v1::RecipeKind::Smoker as i32);
        assert!(list.recipes[0].plugin);
    }
}
