import importlib.util
import re
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
GENERATED = TOOLS.parent / "crates" / "rspigot-data" / "src" / "generated"


def load_generator():
    spec = importlib.util.spec_from_file_location("gen_data", TOOLS / "gen_data.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class EmitModTest(unittest.TestCase):
    def test_declares_every_generated_module(self):
        declared = re.findall(r"^pub mod (\w+);$", load_generator().emit_mod(), re.M)
        on_disk = sorted(path.stem for path in GENERATED.glob("*.rs") if path.stem != "mod")
        self.assertEqual(sorted(declared), on_disk)

    def test_keeps_hand_generated_modules(self):
        declared = load_generator().emit_mod()
        for name in ("advancements", "brewing", "recipe_book"):
            self.assertIn("pub mod %s;" % name, declared)


if __name__ == "__main__":
    unittest.main()
