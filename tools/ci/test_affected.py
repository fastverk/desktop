"""A daemon change must test the app that packages its sibling sources."""
import unittest
from affected import affected

class AffectedTests(unittest.TestCase):
    def test_daemon_change_includes_app(self):
        self.assertEqual(set(affected(["fvkit", "fastverk-app"], ["fvkit/proto/fastverk/workspace/v1/workspace.proto"])), {"fvkit", "fastverk-app"})

    def test_app_change_does_not_rebuild_daemon_module(self):
        self.assertEqual(affected(["fvkit", "fastverk-app"], ["fastverk-app/app/dashboard/Sources/App.swift"]), ["fastverk-app"])

    def test_readme_change_has_no_module_jobs(self):
        self.assertEqual(affected(["fvkit", "fastverk-app"], ["README.md"]), [])

if __name__ == "__main__":
    unittest.main()
