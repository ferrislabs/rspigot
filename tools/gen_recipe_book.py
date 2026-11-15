import argparse
import glob
import json
import os
import sys

RE_ROOT = "/Users/baptiste.parmantier/Development/rspigot/re"
DEFAULT_RECIPES = RE_ROOT + "/server/data/minecraft/recipe"
DEFAULT_REGISTRIES = RE_ROOT + "/datagen/generated/reports/registries.json"
DEFAULT_OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..",
        "crates",
        "rspigot-data",
        "src",
        "generated",
        "recipe_book.rs",
    )
)

CRAFTING = {
    "building": "crafting_building_blocks",
    "redstone": "crafting_redstone",
    "equipment": "crafting_equipment",
    "misc": "crafting_misc",
}
SMELTING = {"food": "furnace_food", "blocks": "furnace_blocks", "misc": "furnace_misc"}
BLASTING = {"blocks": "blast_furnace_blocks", "misc": "blast_furnace_misc"}
SMOKING = {"food": "smoker_food"}
TYPES = {
    "minecraft:crafting_shaped": CRAFTING,
    "minecraft:crafting_shapeless": CRAFTING,
    "minecraft:smelting": SMELTING,
    "minecraft:blasting": BLASTING,
    "minecraft:smoking": SMOKING,
}


def load(directory, categories):
    recipes = []
    for path in sorted(glob.glob(os.path.join(directory, "*.json"))):
        data = json.load(open(path))
        table = TYPES.get(data["type"])
        if table is None:
            continue
        category = data.get("category", "misc" if "misc" in table else next(iter(table)))
        if category not in table:
            sys.exit("unexpected category %s in %s" % (category, path))
        name = os.path.basename(path)[: -len(".json")]
        recipes.append(("minecraft:" + name, categories[table[category]], data.get("group", "")))
    recipes.sort()
    return recipes


def render(recipes):
    lines = [
        "use crate::recipe_book::BookRecipe;",
        "",
        "pub(crate) static RECIPES: [BookRecipe; %d] = [" % len(recipes),
    ]
    for recipe_id, category, group in recipes:
        lines.append("    BookRecipe {")
        lines.append('        id: "%s",' % recipe_id)
        lines.append("        category: %d," % category)
        lines.append('        group: "%s",' % group)
        lines.append("    },")
    lines.append("];")
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--recipes", default=DEFAULT_RECIPES)
    parser.add_argument("--registries", default=DEFAULT_REGISTRIES)
    parser.add_argument("--out", default=DEFAULT_OUT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    registries = json.load(open(args.registries))["minecraft:recipe_book_category"]["entries"]
    categories = {name.split(":")[1]: entry["protocol_id"] for name, entry in registries.items()}
    text = render(load(args.recipes, categories))
    if args.check:
        if not os.path.exists(args.out) or open(args.out).read() != text:
            sys.exit("%s is stale" % args.out)
        return
    with open(args.out, "w") as out:
        out.write(text)


main()
