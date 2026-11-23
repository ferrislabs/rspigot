use rspigot_plugin_protocol::v1::{self, answer, from_plugin, question, recipe_def, server_order};

use crate::{Error, ItemStack, Material, PlayerRecipesHandle, ServerRecipesHandle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ingredient {
    Items(Vec<Material>),
    Tag(String),
    Exact(ItemStack),
}

impl Ingredient {
    pub fn item(material: Material) -> Self {
        Self::Items(vec![material])
    }

    pub fn items(materials: impl IntoIterator<Item = Material>) -> Self {
        Self::Items(materials.into_iter().collect())
    }

    pub fn tag(tag: impl Into<String>) -> Self {
        Self::Tag(tag.into())
    }

    pub fn exact(stack: ItemStack) -> Self {
        Self::Exact(stack)
    }

    fn into_wire(self) -> v1::RecipeIngredient {
        v1::RecipeIngredient {
            choice: Some(match self {
                Self::Items(materials) => v1::recipe_ingredient::Choice::Items(v1::ItemKeys {
                    keys: materials
                        .into_iter()
                        .map(|material| material.key().to_owned())
                        .collect(),
                }),
                Self::Tag(tag) => v1::recipe_ingredient::Choice::Tag(tag),
                Self::Exact(stack) => v1::recipe_ingredient::Choice::Exact(stack.into_wire()),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookingKind {
    Furnace,
    BlastFurnace,
    Smoker,
    Campfire,
}

impl CookingKind {
    const fn wire(self) -> v1::RecipeKind {
        match self {
            Self::Furnace => v1::RecipeKind::Furnace,
            Self::BlastFurnace => v1::RecipeKind::BlastFurnace,
            Self::Smoker => v1::RecipeKind::Smoker,
            Self::Campfire => v1::RecipeKind::Campfire,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeCategory {
    Building,
    Redstone,
    Equipment,
    Misc,
    Food,
}

impl RecipeCategory {
    const fn wire(self) -> v1::RecipeCategory {
        match self {
            Self::Building => v1::RecipeCategory::Building,
            Self::Redstone => v1::RecipeCategory::Redstone,
            Self::Equipment => v1::RecipeCategory::Equipment,
            Self::Misc => v1::RecipeCategory::Misc,
            Self::Food => v1::RecipeCategory::Food,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecipeKind {
    Shaped,
    Shapeless,
    Furnace,
    BlastFurnace,
    Smoker,
    Campfire,
    Stonecutting,
    Smithing,
    Special,
}

impl RecipeKind {
    fn from_wire(kind: i32) -> Result<Self, Error> {
        Ok(match v1::RecipeKind::try_from(kind) {
            Ok(v1::RecipeKind::Shaped) => Self::Shaped,
            Ok(v1::RecipeKind::Shapeless) => Self::Shapeless,
            Ok(v1::RecipeKind::Furnace) => Self::Furnace,
            Ok(v1::RecipeKind::BlastFurnace) => Self::BlastFurnace,
            Ok(v1::RecipeKind::Smoker) => Self::Smoker,
            Ok(v1::RecipeKind::Campfire) => Self::Campfire,
            Ok(v1::RecipeKind::Stonecutting) => Self::Stonecutting,
            Ok(v1::RecipeKind::Smithing) => Self::Smithing,
            Ok(v1::RecipeKind::Special) => Self::Special,
            Ok(v1::RecipeKind::Unspecified) | Err(_) => {
                return Err(Error::Malformed(format!("a recipe kind {kind}")));
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecipeInfo {
    pub id: String,
    pub kind: RecipeKind,
    pub plugin: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Body {
    Shaped {
        pattern: Vec<String>,
        keys: Vec<(char, Ingredient)>,
    },
    Shapeless(Vec<Ingredient>),
    Cooking {
        kind: CookingKind,
        ingredient: Ingredient,
        experience: f32,
        ticks: u32,
    },
    Stonecutting(Ingredient),
    SmithingTrim {
        template: Ingredient,
        base: Ingredient,
        addition: Ingredient,
        pattern: String,
    },
    Transmute {
        input: Ingredient,
        material: Ingredient,
    },
    Smithing {
        template: Ingredient,
        base: Ingredient,
        addition: Ingredient,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Recipe {
    pub id: String,
    result: ItemStack,
    body: Body,
    group: String,
    category: RecipeCategory,
    mirror: bool,
    override_vanilla: bool,
    auto_discover: bool,
}

impl Recipe {
    fn of(id: impl Into<String>, result: ItemStack, body: Body, category: RecipeCategory) -> Self {
        Self {
            id: id.into(),
            result,
            body,
            group: String::new(),
            category,
            mirror: true,
            override_vanilla: false,
            auto_discover: false,
        }
    }

    pub fn shaped<R, K>(id: impl Into<String>, result: ItemStack, pattern: R, keys: K) -> Self
    where
        R: IntoIterator,
        R::Item: Into<String>,
        K: IntoIterator<Item = (char, Ingredient)>,
    {
        Self::of(
            id,
            result,
            Body::Shaped {
                pattern: pattern.into_iter().map(Into::into).collect(),
                keys: keys.into_iter().collect(),
            },
            RecipeCategory::Misc,
        )
    }

    pub fn shapeless(
        id: impl Into<String>,
        result: ItemStack,
        ingredients: impl IntoIterator<Item = Ingredient>,
    ) -> Self {
        Self::of(
            id,
            result,
            Body::Shapeless(ingredients.into_iter().collect()),
            RecipeCategory::Misc,
        )
    }

    pub fn cooking(
        kind: CookingKind,
        id: impl Into<String>,
        ingredient: Ingredient,
        result: ItemStack,
        experience: f32,
        ticks: u32,
    ) -> Self {
        Self::of(
            id,
            result,
            Body::Cooking {
                kind,
                ingredient,
                experience,
                ticks,
            },
            RecipeCategory::Misc,
        )
    }

    pub fn stonecutting(id: impl Into<String>, ingredient: Ingredient, result: ItemStack) -> Self {
        Self::of(
            id,
            result,
            Body::Stonecutting(ingredient),
            RecipeCategory::Misc,
        )
    }

    pub fn smithing(
        id: impl Into<String>,
        template: Ingredient,
        base: Ingredient,
        addition: Ingredient,
        result: ItemStack,
    ) -> Self {
        Self::of(
            id,
            result,
            Body::Smithing {
                template,
                base,
                addition,
            },
            RecipeCategory::Misc,
        )
    }

    pub fn smithing_trim(
        id: impl Into<String>,
        template: Ingredient,
        base: Ingredient,
        addition: Ingredient,
        pattern: impl Into<String>,
        result: ItemStack,
    ) -> Self {
        Self::of(
            id,
            result,
            Body::SmithingTrim {
                template,
                base,
                addition,
                pattern: pattern.into(),
            },
            RecipeCategory::Misc,
        )
    }

    pub fn transmute(
        id: impl Into<String>,
        input: Ingredient,
        material: Ingredient,
        result: ItemStack,
    ) -> Self {
        Self::of(
            id,
            result,
            Body::Transmute { input, material },
            RecipeCategory::Misc,
        )
    }

    pub fn group(self, group: impl Into<String>) -> Self {
        Self {
            group: group.into(),
            ..self
        }
    }

    pub fn category(self, category: RecipeCategory) -> Self {
        Self { category, ..self }
    }

    pub fn mirror(self, mirror: bool) -> Self {
        Self { mirror, ..self }
    }

    pub fn override_vanilla(self, override_vanilla: bool) -> Self {
        Self {
            override_vanilla,
            ..self
        }
    }

    pub fn auto_discover(self, auto_discover: bool) -> Self {
        Self {
            auto_discover,
            ..self
        }
    }

    fn into_wire(self) -> v1::RecipeDef {
        let mirror = self.mirror;
        let body = match self.body {
            Body::Shaped { pattern, keys } => recipe_def::Body::Shaped(v1::ShapedRecipe {
                pattern,
                keys: keys
                    .into_iter()
                    .map(|(symbol, ingredient)| v1::RecipeKey {
                        symbol: symbol.to_string(),
                        ingredient: Some(ingredient.into_wire()),
                    })
                    .collect(),
                mirror,
            }),
            Body::Shapeless(ingredients) => recipe_def::Body::Shapeless(v1::ShapelessRecipe {
                ingredients: ingredients.into_iter().map(Ingredient::into_wire).collect(),
            }),
            Body::Cooking {
                kind,
                ingredient,
                experience,
                ticks,
            } => recipe_def::Body::Cooking(v1::CookingRecipe {
                kind: kind.wire() as i32,
                ingredient: Some(ingredient.into_wire()),
                experience,
                time: ticks,
            }),
            Body::Stonecutting(ingredient) => {
                recipe_def::Body::Stonecutting(ingredient.into_wire())
            }
            Body::SmithingTrim {
                template,
                base,
                addition,
                pattern,
            } => recipe_def::Body::SmithingTrim(v1::SmithingTrimRecipe {
                template: Some(template.into_wire()),
                base: Some(base.into_wire()),
                addition: Some(addition.into_wire()),
                pattern,
            }),
            Body::Transmute { input, material } => {
                recipe_def::Body::Transmute(v1::TransmuteRecipe {
                    input: Some(input.into_wire()),
                    material: Some(material.into_wire()),
                })
            }
            Body::Smithing {
                template,
                base,
                addition,
            } => recipe_def::Body::Smithing(v1::SmithingRecipe {
                template: Some(template.into_wire()),
                base: Some(base.into_wire()),
                addition: Some(addition.into_wire()),
            }),
        };
        v1::RecipeDef {
            id: self.id,
            result: Some(self.result.into_wire()),
            group: self.group,
            category: self.category.wire() as i32,
            override_vanilla: self.override_vanilla,
            auto_discover: self.auto_discover,
            body: Some(body),
        }
    }
}

impl ServerRecipesHandle {
    pub async fn add(&self, recipe: Recipe) -> Result<(), Error> {
        self.link
            .act(from_plugin::Body::Server(v1::ServerOrder {
                order: Some(server_order::Order::AddRecipe(recipe.into_wire())),
            }))
            .await
    }

    pub async fn get_recipes(&self) -> Result<Vec<RecipeInfo>, Error> {
        let list = self
            .link
            .ask(
                question::Question::Recipes(v1::Empty {}),
                "a list of recipes",
                |body| match body {
                    answer::Body::Recipes(list) => Some(list),
                    _ => None,
                },
            )
            .await?;
        list.recipes
            .into_iter()
            .map(|info| {
                Ok(RecipeInfo {
                    id: info.id,
                    kind: RecipeKind::from_wire(info.kind)?,
                    plugin: info.plugin,
                })
            })
            .collect()
    }
}

impl PlayerRecipesHandle {
    pub async fn get_discovered(&self) -> Result<Vec<String>, Error> {
        let list = self
            .link
            .ask(
                question::Question::DiscoveredRecipes(self.target.to_string()),
                "a list of recipes",
                |body| match body {
                    answer::Body::RecipeIds(list) => Some(list),
                    _ => None,
                },
            )
            .await?;
        Ok(list.ids)
    }
}

#[cfg(test)]
mod tests {
    use tokio::sync::mpsc;

    use super::*;
    use crate::link::Link;

    fn wand() -> Recipe {
        Recipe::shaped(
            "demo:wand",
            ItemStack::new(Material::DiamondSword, 1).name("Wand"),
            ["ABA", " B "],
            [
                ('A', Ingredient::item(Material::Stick)),
                ('B', Ingredient::tag("minecraft:planks")),
            ],
        )
        .group("wands")
        .category(RecipeCategory::Equipment)
        .mirror(false)
        .override_vanilla(true)
        .auto_discover(true)
    }

    #[test]
    fn a_shaped_recipe_becomes_its_wire_definition() {
        let wire = wand().into_wire();
        assert_eq!(wire.id, "demo:wand");
        assert_eq!(wire.group, "wands");
        assert_eq!(wire.category, v1::RecipeCategory::Equipment as i32);
        assert!(wire.override_vanilla && wire.auto_discover);
        assert_eq!(
            wire.result.as_ref().map(|item| item.key.as_str()),
            Some("minecraft:diamond_sword")
        );
        let Some(recipe_def::Body::Shaped(shaped)) = wire.body else {
            panic!("a shaped body");
        };
        assert_eq!(shaped.pattern, ["ABA", " B "]);
        assert!(!shaped.mirror);
        assert_eq!(shaped.keys.len(), 2);
        assert_eq!(shaped.keys[0].symbol, "A");
        assert_eq!(
            shaped.keys[1].ingredient,
            Some(v1::RecipeIngredient {
                choice: Some(v1::recipe_ingredient::Choice::Tag(
                    "minecraft:planks".to_owned()
                ))
            })
        );
    }

    #[test]
    fn the_other_kinds_carry_their_own_bodies() {
        let stick = || Ingredient::items([Material::Stick, Material::Bone]);
        let result = || ItemStack::new(Material::Diamond, 2);
        let cooking =
            Recipe::cooking(CookingKind::Smoker, "demo:c", stick(), result(), 1.5, 40).into_wire();
        let Some(recipe_def::Body::Cooking(cooking)) = cooking.body else {
            panic!("a cooking body");
        };
        assert_eq!(cooking.kind, v1::RecipeKind::Smoker as i32);
        assert_eq!((cooking.experience, cooking.time), (1.5, 40));
        assert!(matches!(
            Recipe::shapeless("demo:s", result(), [stick()])
                .into_wire()
                .body,
            Some(recipe_def::Body::Shapeless(_))
        ));
        assert!(matches!(
            Recipe::stonecutting("demo:k", stick(), result())
                .into_wire()
                .body,
            Some(recipe_def::Body::Stonecutting(_))
        ));
        assert!(matches!(
            Recipe::smithing("demo:m", stick(), stick(), stick(), result())
                .into_wire()
                .body,
            Some(recipe_def::Body::Smithing(_))
        ));
    }

    #[tokio::test]
    async fn adding_a_recipe_waits_for_the_server_and_reports_a_refusal() {
        let link = Link::default();
        let (outbound, mut receiver) = mpsc::unbounded_channel();
        let _attached = link.attach(outbound);
        let handle = ServerRecipesHandle::new(link.clone());
        let adding = tokio::spawn({
            let handle = handle.clone();
            async move { handle.add(wand()).await }
        });
        let message = receiver.recv().await.unwrap();
        let Some(from_plugin::Body::Server(v1::ServerOrder {
            order: Some(server_order::Order::AddRecipe(def)),
        })) = message.body
        else {
            panic!("an add order");
        };
        assert_eq!(def.id, "demo:wand");
        link.answer(v1::Answer {
            request: message.request.unwrap(),
            body: Some(answer::Body::Failed(v1::ActionError {
                kind: v1::ActionErrorKind::InvalidArgument as i32,
                reason: "the pattern is empty".to_owned(),
            })),
        });
        let refused = adding.await.unwrap();
        assert!(matches!(
            refused,
            Err(Error::Action(error)) if error.kind == crate::ActionErrorKind::InvalidArgument
        ));
    }

    #[test]
    fn removing_and_discovering_send_the_generated_orders() {
        let link = Link::default();
        let (outbound, mut receiver) = mpsc::unbounded_channel();
        let _attached = link.attach(outbound);
        ServerRecipesHandle::new(link.clone()).remove("demo:wand");
        PlayerRecipesHandle::new("abc".into(), link).discover("demo:wand");
        assert!(matches!(
            receiver.try_recv().unwrap().body,
            Some(from_plugin::Body::Server(v1::ServerOrder {
                order: Some(server_order::Order::RemoveRecipe(id))
            })) if id == "demo:wand"
        ));
        assert!(matches!(
            receiver.try_recv().unwrap().body,
            Some(from_plugin::Body::Player(v1::PlayerOrder {
                order: Some(v1::player_order::Order::DiscoverRecipe(id)), ..
            })) if id == "demo:wand"
        ));
    }
}
