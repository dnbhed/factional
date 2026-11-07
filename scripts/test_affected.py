"""What `affected.py` tests for a change (P-154): `python3 -m unittest discover scripts`."""

import unittest

from affected import EVERYTHING, plan

# A slice of the repository's packages: {name: (dir, workspace, path dependencies)}.
PACKAGES = {
    "factional-core": ("crates/core", "main", []),
    "factional-worldgen": ("crates/worldgen", "main", ["factional-core"]),
    "factional-content": ("crates/content", "main", ["factional-core", "factional-worldgen"]),
    "factional-session": ("crates/session", "main", ["factional-content"]),
    "factional-cli": ("crates/cli", "main", ["factional-session"]),
    "factional-editor": ("crates/editor", "main", ["factional-session"]),
    "factional-bevy": ("crates/bevy", "bevy", ["factional-content"]),
    "factional-bevy-inspector": ("crates/bevy/inspector", "bevy", ["factional-bevy"]),
    "factional-builder": (
        "crates/bevy/builder",
        "bevy",
        ["factional-bevy", "factional-session", "factional-worldgen"],
    ),
}


def tested(files, workspace):
    return plan(files, PACKAGES, workspace)[0]


class Affected(unittest.TestCase):
    def test_a_crate_tests_itself_and_every_crate_depending_on_it_in_both_workspaces(self):
        files = ["crates/session/src/world.rs"]
        self.assertEqual(
            tested(files, "main"), ["factional-cli", "factional-editor", "factional-session"]
        )
        self.assertEqual(tested(files, "bevy"), ["factional-builder"])

    def test_a_crate_far_down_tests_everything_above_it(self):
        self.assertEqual(
            tested(["crates/worldgen/src/map.rs"], "main"),
            [
                "factional-cli",
                "factional-content",
                "factional-editor",
                "factional-session",
                "factional-worldgen",
            ],
        )
        self.assertEqual(
            tested(["crates/worldgen/src/map.rs"], "bevy"),
            ["factional-bevy", "factional-bevy-inspector", "factional-builder"],
        )

    def test_a_crate_inside_another_crates_directory_is_its_own(self):
        self.assertEqual(tested(["crates/bevy/builder/src/wizard.rs"], "bevy"), ["factional-builder"])
        self.assertEqual(tested(["crates/bevy/builder/src/wizard.rs"], "main"), [])
        self.assertEqual(
            tested(["crates/bevy/src/lib.rs"], "bevy"),
            ["factional-bevy", "factional-bevy-inspector", "factional-builder"],
        )

    def test_prose_and_these_scripts_test_nothing(self):
        files = [
            "docs/PLAN.md",
            "docs/research/maps.md",
            "CLAUDE.md",
            "README.md",
            ".gitignore",
            "scripts/affected.py",
        ]
        self.assertEqual(tested(files, "main"), [])
        self.assertEqual(tested(files, "bevy"), [])

    def test_data_tests_only_the_crates_that_read_it(self):
        self.assertEqual(tested(["scenarios/riverhold.scenario"], "main"), ["factional-cli"])
        self.assertEqual(tested(["docs/examples/riverhold/README.md"], "main"), ["factional-content"])
        self.assertEqual(
            tested(["schema/factions.schema.json"], "main"), ["factional-cli", "factional-content"]
        )
        self.assertEqual(tested(["content/sample/builder.toml"], "main"), [])
        self.assertEqual(tested(["content/sample/builder.toml"], "bevy"), ["factional-builder"])

    def test_anything_else_tests_everything(self):
        for path in [
            "content/sample/factions.toml",
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            "clippy.toml",
            ".github/workflows/ci.yml",
        ]:
            self.assertEqual(tested([path], "main"), EVERYTHING, path)
            self.assertEqual(tested([path], "bevy"), EVERYTHING, path)

    def test_the_bevy_workspaces_configuration_tests_all_of_it_and_none_of_the_main_one(self):
        for path in ["crates/bevy/Cargo.lock", "crates/bevy/Cargo.toml", "crates/bevy/.cargo/config.toml"]:
            self.assertEqual(tested([path], "bevy"), EVERYTHING, path)
            self.assertEqual(tested([path], "main"), [], path)

    def test_each_file_is_said_with_why(self):
        _, why = plan(["docs/PLAN.md", "crates/cli/src/main.rs"], PACKAGES, "main")
        self.assertEqual(
            why, ["docs/PLAN.md: no cargo test reads it", "crates/cli/src/main.rs: in factional-cli"]
        )


if __name__ == "__main__":
    unittest.main()
