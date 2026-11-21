import argparse
import json
import os
import sys

DEFAULT_IN = "/Users/baptiste.parmantier/Development/rspigot/re/datagen/generated/reports/map_colors.json"
DEFAULT_OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..",
        "crates",
        "rspigot-data",
        "src",
        "generated",
        "map_colors.rs",
    )
)


def rows(values, width):
    lines = []
    for start in range(0, len(values), width):
        lines.append("    " + ", ".join(str(v) for v in values[start : start + width]) + ",")
    return lines


def render(data):
    by_id = {}
    for c in data["colors"]:
        by_id.setdefault(c["id"], c["rgb"])
    if sorted(by_id) != list(range(len(by_id))) or len(by_id) > 64:
        sys.exit("map colour ids are not dense")
    rgb = [by_id.get(i, 0) for i in range(64)]
    brightness = sorted(data["brightness"], key=lambda b: b["id"])
    if [b["id"] for b in brightness] != [0, 1, 2, 3]:
        sys.exit("unexpected brightness ids")
    states = data["states"]
    types = sorted(data["decoration_types"], key=lambda t: t["id"])
    if [t["id"] for t in types] != list(range(len(types))):
        sys.exit("decoration type ids are not dense")
    lines = [
        "use crate::map_colors::DecorationType;",
        "",
        "pub(crate) static BRIGHTNESS_MODIFIERS: [u32; 4] = [%s];"
        % ", ".join(str(b["modifier"]) for b in brightness),
        "",
        "pub(crate) static COLOR_RGB: [u32; 64] = [",
    ]
    lines += rows(rgb, 8)
    lines += ["];", "", "pub(crate) static STATE_COLOR: [u8; %d] = [" % len(states)]
    lines += rows(states, 32)
    lines += ["];", "", "pub(crate) static DECORATION_TYPES: [DecorationType; %d] = [" % len(types)]
    for t in types:
        lines += [
            "    DecorationType {",
            '        name: "%s",' % t["name"],
            '        asset: "%s",' % t["asset"],
            "        show_on_item_frame: %s," % str(t["show_on_item_frame"]).lower(),
            "        track_count: %s," % str(t["track_count"]).lower(),
            "    },",
        ]
    lines += ["];", ""]
    text = "\n".join(lines)
    assert "//" not in text
    return text


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--dump", default=DEFAULT_IN)
    parser.add_argument("--out", default=DEFAULT_OUT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    text = render(json.load(open(args.dump)))
    if args.check:
        if not os.path.exists(args.out) or open(args.out).read() != text:
            sys.exit("%s is stale" % args.out)
        return
    with open(args.out, "w", encoding="utf-8") as handle:
        handle.write(text)


main()
