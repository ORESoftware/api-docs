import json
import stat
import tempfile
import unittest
from pathlib import Path

from ridl.contract import check_instance
from ridl.freeze import freeze, write_frozen
from ridl.model import load_route_map
from ridl.emit import json_schema


ROOT = Path(__file__).resolve().parents[1]


class FreezeTests(unittest.TestCase):
    def test_write_frozen_is_not_writable(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "out.txt"
            write_frozen(path, "hello\n")
            mode = path.stat().st_mode
            self.assertFalse(mode & stat.S_IWUSR)
            self.assertEqual(path.read_text(encoding="utf-8"), "hello\n")
            write_frozen(path, "hello\n")
            write_frozen(path, "world\n")
            self.assertEqual(path.read_text(encoding="utf-8"), "world\n")
            self.assertFalse(path.stat().st_mode & stat.S_IWUSR)
            freeze(path)


class JsonSchemaContractTests(unittest.TestCase):
    def test_emitted_schema_rejects_missing_and_extra_fields(self) -> None:
        demo = ROOT / "examples" / "demo.route-map.json"
        if not demo.is_file():
            self.skipTest("demo route map not present")
        rmap = load_route_map(demo)
        emitted = {item.path: item.text for item in json_schema.emit(rmap)}
        index = json.loads(emitted["json-schema/index.json"])
        self.assertEqual(
            index["$schema"], "https://json-schema.org/draft/2020-12/schema"
        )
        record = next(
            (
                {"$schema": index["$schema"], **defn}
                for defn in index["$defs"].values()
                if defn.get("type") == "object" and defn.get("additionalProperties") is False
            ),
            None,
        )
        self.assertIsNotNone(record)
        extra = {"__not_a_field__": 1}
        self.assertTrue(check_instance(record, extra))
        self.assertIsInstance(check_instance(record, {}), list)


if __name__ == "__main__":
    unittest.main()
