import argparse
import glob
import json
import os
import subprocess
import sys

RE_ROOT = "/Users/baptiste.parmantier/Development/rspigot/re"
DEFAULT_ADVANCEMENTS = RE_ROOT + "/server/data/minecraft/advancement"
DEFAULT_REGISTRIES = RE_ROOT + "/datagen/generated/reports/registries.json"
DEFAULT_OUT = os.path.normpath(
    os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..",
        "crates",
        "rspigot-data",
        "src",
        "generated",
        "advancements.rs",
    )
)
FRAMES = {"task": 0, "challenge": 1, "goal": 2}


class Position:
    def __init__(self, node, parent, previous, index, x):
        self.node = node
        self.parent = parent
        self.previous = previous
        self.index = index
        self.children = []
        self.ancestor = self
        self.thread = None
        self.x = x
        self.y = -1.0
        self.mod = 0.0
        self.change = 0.0
        self.shift = 0.0
        last = None
        for child in node["children"]:
            last = self.add_child(child, last)

    def add_child(self, node, previous):
        if node["display"] is not None:
            previous = Position(node, self, previous, len(self.children) + 1, self.x + 1)
            self.children.append(previous)
        else:
            for child in node["children"]:
                previous = self.add_child(child, previous)
        return previous

    def first_walk(self):
        if not self.children:
            self.y = self.previous.y + 1.0 if self.previous is not None else 0.0
            return
        default = None
        for child in self.children:
            child.first_walk()
            default = child.apportion(child if default is None else default)
        self.execute_shifts()
        midpoint = (self.children[0].y + self.children[-1].y) / 2.0
        if self.previous is not None:
            self.y = self.previous.y + 1.0
            self.mod = self.y - midpoint
        else:
            self.y = midpoint

    def second_walk(self, mod_sum, depth, low):
        self.y += mod_sum
        self.x = depth
        if self.y < low:
            low = self.y
        for child in self.children:
            low = child.second_walk(mod_sum + self.mod, depth + 1, low)
        return low

    def third_walk(self, offset):
        self.y += offset
        for child in self.children:
            child.third_walk(offset)

    def execute_shifts(self):
        shift = 0.0
        change = 0.0
        for child in reversed(self.children):
            child.y += shift
            child.mod += shift
            change += child.change
            shift += child.shift + change

    def previous_or_thread(self):
        if self.thread is not None:
            return self.thread
        return self.children[0] if self.children else None

    def next_or_thread(self):
        if self.thread is not None:
            return self.thread
        return self.children[-1] if self.children else None

    def apportion(self, default):
        if self.previous is None:
            return default
        vir = vor = self
        vil = self.previous
        vol = self.parent.children[0]
        sir = sor = self.mod
        sil = vil.mod
        sol = vol.mod
        while vil.next_or_thread() is not None and vir.previous_or_thread() is not None:
            vil = vil.next_or_thread()
            vir = vir.previous_or_thread()
            vol = vol.previous_or_thread()
            vor = vor.next_or_thread()
            vor.ancestor = self
            shift = vil.y + sil - (vir.y + sir) + 1.0
            if shift > 0.0:
                vil.get_ancestor(self, default).move_subtree(self, shift)
                sir += shift
                sor += shift
            sil += vil.mod
            sir += vir.mod
            sol += vol.mod
            sor += vor.mod
        if vil.next_or_thread() is not None and vor.next_or_thread() is None:
            vor.thread = vil.next_or_thread()
            vor.mod += sil - sor
        else:
            if vir.previous_or_thread() is not None and vol.previous_or_thread() is None:
                vol.thread = vir.previous_or_thread()
                vol.mod += sir - sol
            default = self
        return default

    def move_subtree(self, right, shift):
        subtrees = float(right.index - self.index)
        if subtrees != 0.0:
            right.change -= shift / subtrees
            self.change += shift / subtrees
        right.shift += shift
        right.y += shift
        right.mod += shift

    def get_ancestor(self, other, default):
        if self.ancestor is not None and self.ancestor in other.parent.children:
            return self.ancestor
        return default

    def finalize(self):
        self.node["x"] = float(self.x)
        self.node["y"] = float(self.y)
        for child in self.children:
            child.finalize()


def layout(root):
    position = Position(root, None, None, 1, 0)
    position.first_walk()
    low = position.second_walk(0.0, 0, position.y)
    if low < 0.0:
        position.third_walk(-low)
    position.finalize()


def load(directory, items):
    nodes = {}
    for path in sorted(glob.glob(os.path.join(directory, "**", "*.json"), recursive=True)):
        name = os.path.relpath(path, directory)[: -len(".json")]
        data = json.load(open(path))
        advancement_id = "minecraft:" + name
        display = data.get("display")
        if display is not None:
            for key in ("title", "description"):
                value = display[key]
                if not (isinstance(value, dict) and set(value) == {"translate"}):
                    sys.exit("unsupported %s in %s" % (key, path))
        criteria = []
        for criterion, body in data["criteria"].items():
            criteria.append((criterion, trigger_of(body, items, path)))
        requirements = data.get("requirements") or [[name] for name, _ in criteria]
        rewards = data.get("rewards", {})
        nodes[advancement_id] = {
            "id": advancement_id,
            "parent": data.get("parent"),
            "display": display,
            "criteria": criteria,
            "requirements": requirements,
            "recipes": rewards.get("recipes", []),
            "experience": rewards.get("experience", 0),
            "telemetry": data.get("sends_telemetry_event", False),
            "children": [],
            "x": 0.0,
            "y": 0.0,
        }
    for node in nodes.values():
        if node["parent"] is not None:
            nodes[node["parent"]]["children"].append(node)
    for node in nodes.values():
        node["children"].sort(key=lambda child: child["id"])
        if node["parent"] is None and node["display"] is not None:
            layout(node)
    return [nodes[key] for key in sorted(nodes)]


def item_match(value, items, path):
    if value.startswith("#"):
        return "ItemMatch::Tag(%s)" % quote(value[1:])
    if value not in items:
        sys.exit("unknown item %s in %s" % (value, path))
    return "ItemMatch::Item(%d)" % items[value]


def quote(text):
    return json.dumps(text)


def trigger_of(body, items, path):
    trigger = body["trigger"]
    conditions = body.get("conditions")
    if trigger == "minecraft:impossible":
        return "Trigger::Impossible"
    if trigger == "minecraft:tick":
        return "Trigger::Tick"
    if trigger == "minecraft:slept_in_bed" and conditions is None:
        return "Trigger::SleptInBed"
    if trigger == "minecraft:entity_killed_player" and conditions is None:
        return "Trigger::EntityKilledPlayer"
    if trigger == "minecraft:recipe_unlocked":
        return "Trigger::RecipeUnlocked(%s)" % quote(conditions["recipes"])
    if trigger == "minecraft:inventory_changed":
        conditions = conditions or {}
        extra = set(conditions) - {"items", "slots"}
        slots = conditions.get("slots", {})
        occupied = slots.get("occupied", {})
        if not extra and set(slots) <= {"occupied"} and set(occupied) <= {"min"}:
            matches = [
                item_match(entry["items"], items, path)
                for entry in conditions.get("items", [])
                if set(entry) == {"items"} and isinstance(entry["items"], str)
            ]
            if len(matches) == len(conditions.get("items", [])):
                return "Trigger::InventoryChanged { items: &[%s], occupied: %d }" % (
                    ", ".join(matches),
                    occupied.get("min", 0),
                )
    if trigger == "minecraft:consume_item":
        if conditions is None:
            return "Trigger::ConsumeItem(None)"
        item = conditions.get("item", {})
        if set(conditions) == {"item"} and set(item) == {"items"} and isinstance(item["items"], str):
            return "Trigger::ConsumeItem(Some(%s))" % item_match(item["items"], items, path)
    if trigger == "minecraft:player_killed_entity":
        if conditions is None:
            return "Trigger::KilledEntity(None)"
        entity = conditions.get("entity", {})
        predicate = entity.get("predicate", {}) if isinstance(entity, dict) else {}
        target = predicate.get("minecraft:entity_type")
        if (
            set(conditions) == {"entity"}
            and set(entity) <= {"type", "entity", "predicate"}
            and set(predicate) == {"minecraft:entity_type"}
            and isinstance(target, str)
            and not target.startswith("#")
        ):
            return "Trigger::KilledEntity(Some(%s))" % quote(target)
    return "Trigger::Other(%s)" % quote(trigger)


def render(nodes):
    lines = [
        "use crate::advancements::{Advancement, Criterion, Display, ItemMatch, Rewards, Trigger};",
        "",
        "pub(crate) static ADVANCEMENTS: [Advancement; %d] = [" % len(nodes),
    ]
    for node in nodes:
        lines.append("    Advancement {")
        lines.append("        id: %s," % quote(node["id"]))
        parent = node["parent"]
        lines.append("        parent: %s," % ("None" if parent is None else "Some(%s)" % quote(parent)))
        display = node["display"]
        if display is None:
            lines.append("        display: None,")
        else:
            icon = display["icon"]["id"]
            lines.append("        display: Some(Display {")
            lines.append("            icon: %d," % ITEMS[icon])
            lines.append("            title: %s," % quote(display["title"]["translate"]))
            lines.append("            description: %s," % quote(display["description"]["translate"]))
            lines.append("            frame: %d," % FRAMES[display.get("frame", "task")])
            background = display.get("background")
            lines.append("            background: %s," % ("None" if background is None else "Some(%s)" % quote(background)))
            lines.append("            show_toast: %s," % str(display.get("show_toast", True)).lower())
            lines.append("            announce_to_chat: %s," % str(display.get("announce_to_chat", True)).lower())
            lines.append("            hidden: %s," % str(display.get("hidden", False)).lower())
            lines.append("            x: %r," % node["x"])
            lines.append("            y: %r," % node["y"])
            lines.append("        }),")
        lines.append("        criteria: &[")
        for name, trigger in node["criteria"]:
            lines.append("            Criterion {")
            lines.append("                name: %s," % quote(name))
            lines.append("                trigger: %s," % trigger)
            lines.append("            },")
        lines.append("        ],")
        groups = ", ".join("&[%s]" % ", ".join(quote(name) for name in group) for group in node["requirements"])
        lines.append("        requirements: &[%s]," % groups)
        recipes = ", ".join(quote(name) for name in node["recipes"])
        lines.append("        rewards: Rewards {")
        lines.append("            recipes: &[%s]," % recipes)
        lines.append("            experience: %d," % node["experience"])
        lines.append("        },")
        lines.append("        telemetry: %s," % str(node["telemetry"]).lower())
        lines.append("    },")
    lines.append("];")
    return "\n".join(lines) + "\n"


ITEMS = {}


def formatted(text):
    result = subprocess.run(
        ["rustfmt", "--edition", "2024"], input=text, capture_output=True, text=True, check=True
    )
    return result.stdout


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--advancements", default=DEFAULT_ADVANCEMENTS)
    parser.add_argument("--registries", default=DEFAULT_REGISTRIES)
    parser.add_argument("--out", default=DEFAULT_OUT)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    registries = json.load(open(args.registries))["minecraft:item"]["entries"]
    ITEMS.update({name: entry["protocol_id"] for name, entry in registries.items()})
    text = formatted(render(load(args.advancements, ITEMS)))
    if args.check:
        if not os.path.exists(args.out) or open(args.out).read() != text:
            sys.exit("%s is stale" % args.out)
        return
    with open(args.out, "w") as out:
        out.write(text)


main()
