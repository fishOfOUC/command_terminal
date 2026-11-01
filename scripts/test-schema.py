"""Validate the real CLI contract with JSON Schema Draft 2020-12.

Requires Python 3 and jsonschema. Build the CLI first; optionally pass its path.
All runtime requests use temporary directories, never the user's Runbook data.
"""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from jsonschema import Draft202012Validator


ROOT = Path(__file__).resolve().parents[1]
EXE = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else ROOT / "target/debug" / (
    "runbook.exe" if sys.platform == "win32" else "runbook"
)


class SchemaContract(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="runbook-schema-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.database = self.root / "data"
        code, response = self.cli(["schema"])
        self.assertEqual(code, 0, response)
        self.full = response["data"]
        self.validator = Draft202012Validator(self.full["requestSchema"])

    def cli(self, args, payload=None, database=None):
        process = subprocess.run(
            [str(EXE), "--data-dir", str(database or self.database), *args],
            input=None if payload is None else json.dumps(payload),
            text=True, encoding="utf-8", capture_output=True, timeout=15,
        )
        self.assertEqual(process.stderr, "")
        return process.returncode, json.loads(process.stdout)

    def api(self, payload):
        return self.cli(["api", "--input", "-"], payload)

    def create_project(self):
        payload = {"op": "project_create", "input": {
            "id": "review", "name": "Schema test", "path": str(self.root),
        }}
        self.assertTrue(self.validator.is_valid(payload))
        code, response = self.api(payload)
        self.assertEqual(code, 0, response)
        return response["data"]

    def command(self, **fields):
        return {"op": "command_create", "project": "review", "input": {
            "name": "Example", "base": "echo example", **fields,
        }}

    def assert_refs_resolve(self, schema):
        def walk(value):
            if isinstance(value, dict):
                if "$ref" in value:
                    ref = value["$ref"]
                    self.assertTrue(ref.startswith("#/"), ref)
                    target = schema
                    for part in ref[2:].split("/"):
                        target = target[part.replace("~1", "/").replace("~0", "~")]
                for child in value.values():
                    walk(child)
            elif isinstance(value, list):
                for child in value:
                    walk(child)
        Draft202012Validator.check_schema(schema)
        walk(schema)

    def test_standalone_schemas_and_all_scoped_operations(self):
        for key in ("requestSchema", "projectSchema", "commandSchema"):
            self.assert_refs_resolve(self.full[key])
        _, response = self.cli(["schema", "--list"])
        for operation in response["data"]["operations"]:
            with self.subTest(operation=operation):
                code, response = self.cli(["schema", "--op", operation])
                self.assertEqual(code, 0, response)
                schema = response["data"]["requestSchema"]
                self.assert_refs_resolve(schema)
                self.assertEqual(schema["properties"]["op"]["const"], operation)
                # Exercise every scoped schema, including its transitive refs.
                sample = {"op": operation, "id": "example", "project": "review",
                          "input": {"name": "Example", "base": "echo", "path": str(self.root)},
                          "patch": {}}
                sample = {k: v for k, v in sample.items() if k in schema["properties"]}
                if operation == "project_create":
                    del sample["input"]["base"]
                if operation == "command_create":
                    del sample["input"]["path"]
                self.assertTrue(Draft202012Validator(schema).is_valid(sample), sample)
                self.assertTrue(self.validator.is_valid(sample), sample)
        _, scoped = self.cli(["schema", "--op", "command_create"])
        definitions = scoped["data"]["requestSchema"]["$defs"]
        self.assertEqual(set(definitions), {"CommandCreate", "Parameter"})
        self.assertFalse(self.database.exists())

    def test_previous_schema_runtime_mismatches(self):
        self.create_project()
        cases = [
            {"op": "project_create", "input": {"id": "bad id", "name": "x", "path": str(self.root)}},
            {"op": "project_create", "input": {"name": "", "path": str(self.root)}},
            self.command(params=[{"id": "b", "label": "Bool", "type": "boolean", "flag": "--bool"}]),
            self.command(params=[{"id": "t", "label": "Text", "type": "text", "value": {}}]),
        ]
        for payload in cases:
            with self.subTest(payload=payload):
                self.assertFalse(self.validator.is_valid(payload), payload)
                code, response = self.api(payload)
                self.assertEqual(code, 2, response)
                self.assertEqual(response["error"]["code"], "VALIDATION_ERROR")

    def test_parameter_constraints_and_valid_values(self):
        self.create_project()
        for kind, value in [("text", ""), ("text", 3), ("number", "3.5"),
                            ("number", 0), ("boolean", False), ("boolean", True)]:
            with self.subTest(kind=kind, value=value):
                payload = self.command(params=[{
                    "id": "arg", "label": "Arg", "type": kind,
                    "flag": "--arg", "value": value,
                }])
                self.assertTrue(self.validator.is_valid(payload), payload)
                code, response = self.api(payload)
                self.assertEqual(code, 0, response)
                Draft202012Validator(self.full["commandSchema"]).validate(response["data"])
        for parameter in [
            {"type": "boolean", "value": False, "flag": ""},
            {"type": "boolean", "value": "false", "flag": "--arg"},
            {"type": "text", "value": False},
            {"type": "number", "value": None},
            {"type": "text", "value": []},
            {"type": "text", "value": "ok", "flag": "--bad flag"},
            {"type": "text", "value": "ok", "flag": "--arg\n"},
        ]:
            payload = self.command(params=[{"id": "arg", "label": "Arg", **parameter}])
            with self.subTest(parameter=parameter):
                self.assertFalse(self.validator.is_valid(payload))
                self.assertEqual(self.api(payload)[0], 2)

    def test_lengths_unicode_null_patches_and_normalization(self):
        self.create_project()
        for name in ("x" * 61, " \n\t\u3000", "bad\0name"):
            payload = self.command(name=name)
            with self.subTest(name=name):
                self.assertFalse(self.validator.is_valid(payload))
                self.assertEqual(self.api(payload)[0], 2)
        for name in ("中" * 60, " \n" + "x" * 60 + "\u3000", "valid\nname"):
            payload = self.command(name=name)
            with self.subTest(name=name):
                self.assertTrue(self.validator.is_valid(payload))
                self.assertEqual(self.api(payload)[0], 0)
        for field, value in [("description", "x" * 1001), ("description", "\0"),
                             ("base", "x" * 16385), ("params", [{}] * 65)]:
            self.assertFalse(self.validator.is_valid(self.command(**{field: value})))
        for ident in ("bad\n", "", "x" * 101):
            self.assertFalse(self.validator.is_valid(self.command(id=ident)))
        patch = {"op": "project_update", "id": "review", "patch": {
            "name": None, "description": None, "starred": None,
        }}
        self.assertTrue(self.validator.is_valid(patch))
        code, response = self.api(patch)
        self.assertEqual(code, 0, response)
        Draft202012Validator(self.full["projectSchema"]).validate(response["data"])
        command_patch = {"op": "command_update", "project": "review", "id": "id",
                         "patch": {"name": None, "base": None, "description": None, "params": None}}
        self.assertTrue(self.validator.is_valid(command_patch))

    def test_scoped_validation_agrees_with_full_contract(self):
        _, response = self.cli(["schema", "--op", "command_create"])
        scoped = Draft202012Validator(response["data"]["requestSchema"])
        valid = self.command(params=[{"id": "arg", "label": "Arg", "type": "boolean",
                                      "value": False, "flag": "--arg"}])
        cases = [valid, self.command(name=""), self.command(id="bad id")]
        missing = copy.deepcopy(valid)
        del missing["input"]["params"][0]["value"]
        cases.append(missing)
        for payload in cases:
            self.assertEqual(scoped.is_valid(payload), self.validator.is_valid(payload))
        # The schema query itself describes the mutual exclusion of its filters.
        self.assertFalse(self.validator.is_valid({"op": "schema", "list": True, "operation": "info"}))


if __name__ == "__main__":
    unittest.main(argv=[sys.argv[0]], verbosity=2)
