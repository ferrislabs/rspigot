import argparse
import glob
import json
import os
import sys

RE_ROOT = "/Users/baptiste.parmantier/Development/rspigot/re"
DEFAULT_RECIPES = RE_ROOT + "/server/data/minecraft/recipe/brewing"
DEFAULT_OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..",
        "crates",
        "rspigot-data",
        "src",
        "generated",
        "brewing.rs",
    )
)


def load(directory):
    recipes = []
    for path in sorted(glob.glob(os.path.join(directory, "*.json"))):
        data = json.load(open(path))
        if data["type"] != "minecraft:brewing":
            sys.exit("unexpected recipe type in %s" % path)
        source, output = data["input"], data["output"]
        potion = source["potion_contents"]["potions"]
        if not isinstance(potion, str) or set(source) - {"item", "potion_contents"}:
            sys.exit("unsupported input in %s" % path)
        components = output.get("components", {})
        if set(components) - {"minecraft:potion_contents"}:
            sys.exit("unsupported output in %s" % path)
        recipes.append(
            (
                source["item"],
                potion,
                data["reagent"]["item"],
                output["id"],
                components["minecraft:potion_contents"]["potion"],
            )
        )
    return recipes


def render(recipes):
    lines = [
        "use crate::brewing::Mix;",
        "",
        "pub(crate) static MIXES: [Mix; %d] = [" % len(recipes),
    ]
    for source, potion, reagent, output, result in recipes:
        lines.append("    Mix {")
        lines.append('        input: "%s",' % source)
        lines.append('        input_potion: "%s",' % potion)
        lines.append('        reagent: "%s",' % reagent)
        lines.append('        output: "%s",' % output)
        lines.append('        output_potion: "%s",' % result)
        lines.append("    },")
    lines.append("];")
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--recipes", default=DEFAULT_RECIPES)
    parser.add_argument("--out", default=DEFAULT_OUT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    text = render(load(args.recipes))
    if args.check:
        if not os.path.exists(args.out) or open(args.out).read() != text:
            sys.exit("%s is stale" % args.out)
        return
    with open(args.out, "w") as out:
        out.write(text)


main()
