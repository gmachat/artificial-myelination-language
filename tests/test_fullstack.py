import json
import unittest

from aml import CompileError
from aml.fullstack import fullstack_project_files, parse_fullstack


SOURCE = """app inventory_dashboard
profile fullstack
data Product:
  name: Text
  price: Float
  active: Bool
resource products: Product:
  list
  create
  delete
page Dashboard at "/":
  resource products
"""


class FullstackTests(unittest.TestCase):
    def test_parses_data_resource_and_page(self):
        spec = parse_fullstack(SOURCE)
        self.assertEqual(spec.app, "inventory_dashboard")
        self.assertEqual(spec.models["Product"].fields[1], ("price", "Float"))
        self.assertEqual(spec.resources[0].actions, ["list", "create", "delete"])
        self.assertEqual(spec.page, "Dashboard")
        self.assertEqual(spec.storage, "json")

    def test_generates_ui_api_storage_and_dev_runner(self):
        files = fullstack_project_files(SOURCE)
        for path in (
            "package.json",
            "server.mjs",
            "vite.config.js",
            "scripts/dev.mjs",
            "data/store.json",
            "src/App.jsx",
            "src/main.jsx",
            "src/styles.css",
        ):
            self.assertIn(path, files)
        package = json.loads(files["package.json"])
        self.assertEqual(package["scripts"]["start"], "node server.mjs")
        self.assertIn("/api/health", files["server.mjs"])
        self.assertIn("randomUUID", files["server.mjs"])
        self.assertIn("mutationQueue", files["server.mjs"])
        self.assertEqual(json.loads(files["data/store.json"]), {"products": []})

    def test_generates_postgres_pool_migration_and_update(self):
        source = """app production
profile fullstack
storage postgres
data Game:
  starts_at: Time
  edge: Float
resource games: Game:
  list
  create
  update
  delete
page Dashboard at "/":
  resource games
"""
        files = fullstack_project_files(source)
        package = json.loads(files["package.json"])
        self.assertEqual(package["scripts"]["migrate"], "node scripts/migrate.mjs")
        self.assertIn("pg", package["dependencies"])
        self.assertNotIn("data/store.json", files)
        self.assertIn("migrations/001_initial.sql", files)
        self.assertIn('"starts_at" TIMESTAMPTZ NOT NULL', files["migrations/001_initial.sql"])
        self.assertIn('"updated_at" TIMESTAMPTZ', files["migrations/001_initial.sql"])
        self.assertIn("new Pool", files["server.mjs"])
        self.assertIn("DATABASE_URL is required", files["server.mjs"])
        self.assertIn("UPDATE ${identifier(resource.name)}", files["server.mjs"])
        self.assertIn("setEditingId", files["src/App.jsx"])

    def test_rejects_resource_with_unknown_model(self):
        with self.assertRaisesRegex(CompileError, "unknown data 'Missing'"):
            parse_fullstack("""app bad
profile fullstack
resource things: Missing:
  list
page Home at "/":
  resource things
""")

    def test_rejects_unsupported_persistent_type(self):
        with self.assertRaisesRegex(CompileError, "persistent field type"):
            parse_fullstack("""app bad
profile fullstack
data Thing:
  payload: Any
resource things: Thing:
  list
page Home at "/":
  resource things
""")


if __name__ == "__main__":
    unittest.main()

