"""Unit tests for publish_crates.py (python -m unittest, no dependencies)."""
import unittest

import publish_crates as p


def package(name, deps=(), publish=None, version="0.1.0"):
    return {
        "id": f"path+file:///w/{name}#{version}",
        "name": name,
        "version": version,
        "publish": publish,
        "dependencies": [{"name": d, "kind": k} for d, k in deps],
    }


def metadata(*packages, members=None):
    return {
        "packages": list(packages),
        "workspace_members": members or [pkg["id"] for pkg in packages],
    }


class PublishableTest(unittest.TestCase):
    def test_keeps_crates_io_and_unrestricted_members(self):
        found = p.publishable(metadata(
            package("a"),
            package("b", publish=["crates-io"]),
            package("app", publish=[]),
            package("private", publish=["other"]),
        ))
        self.assertEqual(sorted(found), ["a", "b"])

    def test_ignores_non_members(self):
        outside = package("dep")
        found = p.publishable(metadata(package("a"), outside, members=[package("a")["id"]]))
        self.assertEqual(sorted(found), ["a"])


class OrderTest(unittest.TestCase):
    def test_dependencies_come_first_and_ties_go_by_name(self):
        packages = p.publishable(metadata(
            package("types"),
            package("dsp", [("types", None)]),
            package("engine", [("dsp", None), ("types", None)]),
            package("graph", [("dsp", None), ("types", "build")]),
            package("api", [("engine", None), ("graph", None)]),
        ))
        self.assertEqual(p.publish_order(packages), ["types", "dsp", "engine", "graph", "api"])

    def test_dev_dependencies_do_not_order(self):
        packages = p.publishable(metadata(
            package("a", [("b", "dev")]),
            package("b", [("a", None)]),
        ))
        self.assertEqual(p.publish_order(packages), ["a", "b"])

    def test_cycle_is_an_error(self):
        packages = p.publishable(metadata(
            package("a", [("b", None)]),
            package("b", [("a", None)]),
        ))
        with self.assertRaises(SystemExit):
            p.publish_order(packages)


class VersionTest(unittest.TestCase):
    def test_every_crate_carries_the_release_version(self):
        packages = p.publishable(metadata(package("a"), package("b", version="0.1.1")))
        with self.assertRaises(SystemExit):
            p.check_versions(packages, "0.1.0")
        p.check_versions(p.publishable(metadata(package("a"))), "0.1.0")


if __name__ == "__main__":
    unittest.main()
