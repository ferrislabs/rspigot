import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
GENERATOR = REPO / "tools" / "gen_plugin_events.py"
DUMP = Path(os.environ.get("RSPIGOT_API_DUMP", REPO.parent / "re" / "api-dump")) / "event-types.json"
PROTO = "crates/rspigot-plugin-protocol/proto/rspigot/plugin/v1"
INPUTS = [
    "tools/plugin-events/catalog.json",
    "tools/plugin-events/overrides.toml",
    f"{PROTO}/types.proto",
    f"{PROTO}/plugin.proto",
    "crates/rspigot-plugin/src/lib.rs",
]
OUTPUTS = [
    f"{PROTO}/events.proto",
    "crates/rspigot-core/src/plugins/events.rs",
    "crates/rspigot-plugins/src/convert/events.rs",
    "crates/rspigot-plugin/src/events.rs",
    "crates/rspigot-plugin-macros/src/events.rs",
    "crates/rspigot-plugin/EVENTS.md",
    "crates/rspigot-plugin/tests/every_event.rs",
]


def load_extract():
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location("extract", REPO / "tools" / "plugin-events" / "extract.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def generator(root, *arguments):
    return subprocess.run(
        [sys.executable, "-I", str(GENERATOR), "--root", str(root), *arguments],
        capture_output=True, text=True,
    )


class Generator(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.root = Path(self.scratch.name)
        for relative in INPUTS + OUTPUTS:
            target = self.root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(REPO / relative, target)

    def tearDown(self):
        self.scratch.cleanup()

    def catalog(self):
        return json.loads((self.root / INPUTS[0]).read_text())

    def write_catalog(self, catalog):
        (self.root / INPUTS[0]).write_text(json.dumps(catalog))

    def outputs(self):
        return {relative: (self.root / relative).read_bytes() for relative in OUTPUTS}

    def test_the_committed_files_are_fresh_and_two_runs_write_the_same_bytes(self):
        self.assertEqual(generator(self.root, "--check").returncode, 0)
        committed = self.outputs()
        self.assertEqual(generator(self.root).returncode, 0)
        first = self.outputs()
        self.assertEqual(generator(self.root).returncode, 0)
        self.assertEqual(self.outputs(), first)
        self.assertEqual(first, committed)

    def test_check_fails_after_a_manual_edit_of_any_generated_file(self):
        for relative in OUTPUTS:
            path = self.root / relative
            original = path.read_text()
            path.write_text(original + " ")
            failed = generator(self.root, "--check")
            self.assertEqual(failed.returncode, 1, relative)
            self.assertIn(relative, failed.stderr)
            path.write_text(original)
        self.assertEqual(generator(self.root, "--check").returncode, 0)

    def sdk_impl(self, event):
        text = (self.root / "crates/rspigot-plugin/src/events.rs").read_text()
        start = text.index(f"\nimpl {event} {{")
        return text[start : text.index("\n}\n", start)]

    def test_every_player_actor_event_has_player_accessor(self):
        for event in ("InventoryClick", "EnchantItem", "BlockBreak", "CraftItem", "FoodLevelChange", "PlayerDeath"):
            self.assertIn("pub fn player(&self) -> &Player {", self.sdk_impl(event), event)
        self.assertIn("pub fn player(&self) -> Option<&Player> {", self.sdk_impl("EntityEnterLoveMode"))
        text = (self.root / "crates/rspigot-plugin/src/events.rs").read_text()
        for name in ("who_clicked", "enchanter", "human_entity"):
            self.assertNotIn(f"pub fn {name}(", text)

    def test_no_event_exposes_two_player_accessors(self):
        text = (self.root / "crates/rspigot-plugin/src/events.rs").read_text()
        for body in re.split(r"\nimpl ", text)[1:]:
            self.assertLessEqual(len(re.findall(r"pub fn player\(", body.split("\n}\n")[0])), 1)

    def test_an_actor_override_must_name_a_player_getter(self):
        overrides = self.root / INPUTS[1]
        original = overrides.read_text()
        overrides.write_text(original.replace('[events.InventoryClick]\nactor = "whoClicked"', '[events.InventoryClick]\nactor = "slot"'))
        failed = generator(self.root, "--check")
        self.assertEqual(failed.returncode, 1)
        self.assertIn("InventoryClick: the actor slot is not a player getter", failed.stderr)

    def test_an_unknown_java_type_fails_the_generation(self):
        catalog = self.catalog()
        event = next(event for event in catalog["events"] if event["name"] == "PlayerJoin")
        event["getters"].append(
            {"name": "getMystery", "declared_in": event["java"], "type": "org.example.Mystery", "nullable": None, "key": "mystery"}
        )
        event["fields"]["mystery"] = 99
        self.write_catalog(catalog)
        failed = generator(self.root, "--check")
        self.assertEqual(failed.returncode, 1)
        self.assertIn("org.example.Mystery", failed.stderr)

    def test_an_event_without_a_state_fails_the_generation(self):
        overrides = self.root / INPUTS[1]
        stateless = [line for line in overrides.read_text().splitlines(keepends=True) if not line.startswith("PlayerJoin = ")]
        overrides.write_text("".join(stateless))
        failed = generator(self.root, "--check")
        self.assertEqual(failed.returncode, 1)
        self.assertIn("PlayerJoin must have exactly one state", failed.stderr)

    def test_a_length_limit_needs_an_existing_text_setter(self):
        overrides = self.root / INPUTS[1]
        original = overrides.read_text()
        for limit, message in (
            ("setNothing = 5", "the override limits setNothing matches nothing"),
            ("setMaxPlayers = 5", "the limit of setMaxPlayers needs a text setter and a positive length"),
        ):
            overrides.write_text(original.replace("limits = { setMotd = 32767 }", f"limits = {{ {limit} }}"))
            failed = generator(self.root, "--check")
            self.assertEqual(failed.returncode, 1, limit)
            self.assertIn(message, failed.stderr)
        overrides.write_text(original)
        self.assertEqual(generator(self.root, "--check").returncode, 0)

    def test_a_deprecated_or_retired_event_leaves_its_numbers_reserved(self):
        catalog = self.catalog()
        sheared = next(event for event in catalog["events"] if event["name"] == "SheepRegrowWool")
        sheared["deprecated"] = True
        catalog["retired"] = [{"java": "org.example.GoneEvent", "name": "Gone", "number": 900, "fields": {}}]
        self.write_catalog(catalog)
        overrides = self.root / INPUTS[1]
        stateless = [
            line
            for line in overrides.read_text().replace('"SheepRegrowWool", ', "").splitlines(keepends=True)
            if not line.startswith("SheepRegrowWool = ")
        ]
        overrides.write_text("".join(stateless))
        self.assertEqual(generator(self.root).returncode, 0)
        proto = (self.root / OUTPUTS[0]).read_text()
        number = sheared["number"]
        self.assertNotIn("SHEEP_REGROW_WOOL", proto)
        self.assertRegex(proto, rf"enum EventKind {{[^}}]* reserved {number}, 900; }}")
        for message in ("Event", "Patch"):
            body = re.search(rf"^message {message} {{\n(.*?)^}}", proto, re.S | re.M)[1]
            self.assertIn(f"reserved {number + 9}, 909;", body)

    def test_enum_numbers_come_from_the_catalog_and_a_removed_constant_is_reserved(self):
        catalog = self.catalog()
        action = catalog["types"]["org.bukkit.event.block.Action"]
        action["constants"].insert(2, "MIDDLE_CLICK")
        action["constants"].remove("PHYSICAL")
        action["numbers"]["MIDDLE_CLICK"] = 6
        self.write_catalog(catalog)
        self.assertEqual(generator(self.root).returncode, 0)
        proto = (self.root / OUTPUTS[0]).read_text()
        self.assertIn(
            "enum Action { ACTION_UNSPECIFIED = 0; ACTION_LEFT_CLICK_BLOCK = 1; ACTION_RIGHT_CLICK_BLOCK = 2; "
            "ACTION_MIDDLE_CLICK = 6; ACTION_LEFT_CLICK_AIR = 3; ACTION_RIGHT_CLICK_AIR = 4; reserved 5; }",
            proto,
        )
        del action["numbers"]["MIDDLE_CLICK"]
        self.write_catalog(catalog)
        failed = generator(self.root, "--check")
        self.assertEqual(failed.returncode, 1)
        self.assertIn("MIDDLE_CLICK", failed.stderr)

    def test_every_list_patch_travels_as_a_diff_against_the_snapshot(self):
        self.assertEqual(generator(self.root).returncode, 0)
        proto = (self.root / OUTPUTS[0]).read_text()
        self.assertIn(
            "message PlayerSnapshotListDiff { repeated uint32 removed = 1; repeated PlayerSnapshot added = 2; "
            "repeated PlayerSnapshotReplacement replaced = 3; }",
            proto,
        )
        self.assertIn("message ItemReplacement { uint32 index = 1; Item value = 2; }", proto)
        self.assertRegex(proto, r"message AsyncPlayerChatPatch {[^}]* PlayerSnapshotListDiff recipients = 1004; reserved 4; }")
        self.assertRegex(proto, r"message EntityDeathPatch {[^}]* ItemListDiff drops = 1003; reserved 3; }")
        self.assertRegex(proto, r"message ServerListPingPatch {[^}]* StringListDiff hidden_players = 1100; reserved 100; }")
        self.assertNotRegex(proto, r"(?m)^message \w+List {")
        core = (self.root / OUTPUTS[1]).read_text()
        self.assertIn("pub recipients: SetDiff<PlayerSnapshot>,", core)
        self.assertIn("pub commands: SetDiff<String>,", core)
        self.assertIn("pub block_list: SetDiff<BlockPos>,", core)
        self.assertIn("pub hidden_players: SetDiff<Uuid>,", core)
        self.assertIn("pub drops: ListDiff<ItemSpec>,", core)
        self.assertIn("pub completions: ListDiff<String>,", core)
        self.assertIn("recipients: self.recipients.then(later.recipients),", core)
        self.assertIn("lines: self.lines.then(later.lines),", core)
        self.assertIn('self.lines.slot_refusal("lines", event.lines.len())', core)
        sdk = (self.root / OUTPUTS[3]).read_text()
        self.assertIn("recipients_snapshot: Option<Vec<Player>>,", sdk)
        self.assertIn("diff::same_item,", sdk)
        self.assertIn("diff::same_entity,", sdk)
        self.assertIn("diff::slots(snapshot, value.clone())", sdk)

    def test_a_field_number_past_the_diff_offset_fails_the_generation(self):
        catalog = self.catalog()
        event = next(event for event in catalog["events"] if event["name"] == "PlayerJoin")
        event["getters"].append(
            {"name": "getMystery", "declared_in": event["java"], "type": "int", "nullable": None, "key": "mystery"}
        )
        event["fields"]["mystery"] = 1000
        self.write_catalog(catalog)
        failed = generator(self.root, "--check")
        self.assertEqual(failed.returncode, 1)
        self.assertIn("PlayerJoin.mystery", failed.stderr)

    def test_a_set_override_on_a_field_that_is_not_a_list_patch_fails_the_generation(self):
        overrides = self.root / INPUTS[1]
        overrides.write_text(overrides.read_text() + '\n[events.TabComplete]\nsets = ["getBuffer"]\n')
        failed = generator(self.root, "--check")
        self.assertEqual(failed.returncode, 1)
        self.assertIn("TabComplete: the set getBuffer is not a list patch", failed.stderr)

    def test_a_list_patch_without_its_getter_fails_the_generation(self):
        overrides = self.root / INPUTS[1]
        overrides.write_text(overrides.read_text() + '\n[events.TabComplete]\nomit = { getCompletions = "hidden" }\n')
        failed = generator(self.root, "--check")
        self.assertEqual(failed.returncode, 1)
        self.assertIn("TabComplete.completions", failed.stderr)

    @unittest.skipUnless(DUMP.exists(), "the API dump is not on this machine")
    def test_check_dump_passes_on_the_api_dump_and_fails_when_the_catalog_lacks_an_event(self):
        self.assertEqual(generator(self.root, "--check-dump", str(DUMP)).returncode, 0)
        catalog = self.catalog()
        catalog["events"] = [event for event in catalog["events"] if event["name"] != "EntityDamageByEntity"]
        self.write_catalog(catalog)
        failed = generator(self.root, "--check-dump", str(DUMP))
        self.assertEqual(failed.returncode, 1)
        self.assertIn("org.bukkit.event.entity.EntityDamageByEntityEvent", failed.stderr)


class Numbering(unittest.TestCase):
    def setUp(self):
        self.extract = load_extract()

    def test_a_constant_inserted_in_the_middle_leaves_the_others_their_numbers(self):
        previous = {"DOWN": 1, "UP": 2, "NORTH": 3}
        self.assertEqual(
            self.extract.number_constants(["DOWN", "SELF", "UP", "NORTH"], previous),
            {"DOWN": 1, "UP": 2, "NORTH": 3, "SELF": 4},
        )
        self.assertEqual(self.extract.number_constants(["UP", "NORTH"], previous), previous)
        self.assertEqual(self.extract.number_constants(["A", "B"], None), {"A": 1, "B": 2})

    def test_a_deprecated_or_removed_event_keeps_its_number_and_a_new_one_gets_a_fresh_number(self):
        previous = {"events": [
            {"java": "org.bukkit.event.player.PlayerJoinEvent", "number": 1, "fields": {"player": 1}},
            {"java": "org.example.AlphaEvent", "number": 6, "fields": {"alpha": 1}},
            {"java": "org.example.BravoEvent", "number": 7, "fields": {"bravo": 1}},
        ]}
        names = ["org.bukkit.event.player.PlayerJoinEvent", "org.example.AlphaEvent", "org.example.CharlieEvent"]
        numbers = self.extract.number_events(names, {"org.example.AlphaEvent"}, previous)
        self.assertEqual(numbers["org.example.AlphaEvent"], 6)
        self.assertEqual(numbers["org.bukkit.event.player.PlayerJoinEvent"], 1)
        self.assertEqual(numbers["org.example.CharlieEvent"], 8)
        retired = self.extract.retired_events(previous, names)
        self.assertEqual(retired, [{"java": "org.example.BravoEvent", "name": "Bravo", "number": 7, "fields": {"bravo": 1}}])
        charlie = {"java": "org.example.CharlieEvent", "number": 8, "fields": {}}
        later = {"events": previous["events"][:2] + [charlie], "retired": retired}
        names = names + ["org.example.DeltaEvent"]
        numbers = self.extract.number_events(names, {"org.example.AlphaEvent"}, later)
        self.assertEqual((numbers["org.example.CharlieEvent"], numbers["org.example.DeltaEvent"]), (8, 9))
        self.assertEqual(self.extract.retired_events(later, names), retired)
        back = self.extract.number_events(names + ["org.example.BravoEvent"], set(), later)
        self.assertEqual(back["org.example.BravoEvent"], 7)
        self.assertEqual(self.extract.retired_events(later, names + ["org.example.BravoEvent"]), [])


if __name__ == "__main__":
    unittest.main()
