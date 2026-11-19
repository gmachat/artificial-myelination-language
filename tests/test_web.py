import json
import unittest

from aml.web import (
    REACT_VERSION,
    VITE_REACT_VERSION,
    VITE_VERSION,
    web_project_files,
)


WEB_APP = """app aml_counter
profile web
component Counter(title: Text):
  state count: Int = 0
  view:
    column:
      heading title
      text "Count: {count}"
      button "Add":
        on click:
          set count = count + 1
"""


class WebProjectTests(unittest.TestCase):
    def test_generates_complete_vite_react_project(self):
        files = web_project_files(WEB_APP)
        self.assertEqual(
            set(files),
            {
                ".gitignore",
                "README.md",
                "index.html",
                "package.json",
                "vite.config.js",
                "src/App.jsx",
                "src/ErrorBoundary.jsx",
                "src/main.jsx",
                "src/styles.css",
            },
        )
        package = json.loads(files["package.json"])
        self.assertEqual(package["scripts"]["build"], "vite build")
        self.assertEqual(package["dependencies"]["react"], f"^{REACT_VERSION}")
        self.assertEqual(package["dependencies"]["react-dom"], f"^{REACT_VERSION}")
        self.assertEqual(package["devDependencies"]["vite"], f"^{VITE_VERSION}")
        self.assertEqual(
            package["devDependencies"]["@vitejs/plugin-react"],
            f"^{VITE_REACT_VERSION}",
        )

    def test_generates_safe_entrypoint_and_sample_props(self):
        files = web_project_files(WEB_APP)
        self.assertIn("<ErrorBoundary>", files["src/main.jsx"])
        self.assertIn('<App title={"AML title"} />', files["src/main.jsx"])
        self.assertIn('createRoot(root).render(', files["src/main.jsx"])
        self.assertIn('meta name="viewport"', files["index.html"])


if __name__ == "__main__":
    unittest.main()

