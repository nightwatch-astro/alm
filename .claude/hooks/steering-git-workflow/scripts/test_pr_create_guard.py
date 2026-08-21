#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path
from unittest.mock import patch

SCRIPT = Path(__file__).with_name("pr-create-guard.py")
CODEX_SCRIPT = (
    Path(__file__).resolve().parents[4]
    / ".codex"
    / "hooks"
    / "steering-git-workflow"
    / "scripts"
    / "pr-create-guard.py"
)
SPEC = importlib.util.spec_from_file_location("pr_create_guard", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)

BODY = "Tracks-Bead: work-1\nMerge-Bead: merge-1\n"


def merge_record(metadata: dict[str, object]) -> dict[str, object]:
    return {
        "id": "merge-1",
        "status": "open",
        "labels": ["pr:merge", "agent:integrator"],
        "metadata": {
            "branch": "fix/work-1",
            "repo": "owner/repo",
            "origin_actor": "actor-1",
            **metadata,
        },
    }


class MetadataIdsTests(unittest.TestCase):
    def test_json_encoded_string_decodes_to_ids(self) -> None:
        self.assertEqual(MODULE.metadata_ids('["work-1", "work-2"]'), {"work-1", "work-2"})

    def test_real_list_decodes_to_ids(self) -> None:
        self.assertEqual(MODULE.metadata_ids(["work-1"]), {"work-1"})

    def test_unparsable_string_is_not_a_list(self) -> None:
        self.assertIsNone(MODULE.metadata_ids("work-1"))

    def test_json_scalar_is_not_a_list(self) -> None:
        self.assertIsNone(MODULE.metadata_ids('"work-1"'))

    def test_non_string_elements_are_not_ids(self) -> None:
        self.assertIsNone(MODULE.metadata_ids("[1]"))

    def test_null_is_not_a_list(self) -> None:
        self.assertIsNone(MODULE.metadata_ids(None))


class ValidateMergeMetadataTests(unittest.TestCase):
    """The metadata shape `bd update --set-metadata` actually writes.

    Every --set-metadata value is stored as a string, so `tracks_beads` arrives as
    the JSON text ``["work-1"]``. Passing a real Python list here would exercise a
    shape no bd invocation produces.
    """

    def _validate(self, metadata: dict[str, object]) -> str | None:
        records = {"merge-1": merge_record(metadata), "work-1": {"id": "work-1", "status": "open"}}
        with patch.object(MODULE, "beads_workspace", return_value=True), patch.object(
            MODULE, "bead_record", side_effect=lambda cwd, bead_id: records.get(bead_id)
        ):
            return MODULE.validate(
                ["gh", "pr", "create", "--draft", "--body", BODY], Path("/repo")
            )

    def test_json_encoded_tracks_beads_string_is_accepted(self) -> None:
        self.assertIsNone(self._validate({"tracks_beads": '["work-1"]'}))

    def test_json_encoded_mismatch_is_still_rejected(self) -> None:
        self.assertIn("must match", self._validate({"tracks_beads": '["other-9"]'}) or "")

    def test_bare_id_string_is_rejected(self) -> None:
        self.assertIn("must match", self._validate({"tracks_beads": "work-1"}) or "")

    def test_json_encoded_closes_beads_mismatch_is_rejected(self) -> None:
        reason = self._validate(
            {"tracks_beads": '["work-1"]', "closes_beads": '["work-1"]'}
        )
        self.assertIn("must match", reason or "")


class DeployedCopiesTests(unittest.TestCase):
    def test_claude_and_codex_copies_are_identical(self) -> None:
        self.assertEqual(SCRIPT.read_bytes(), CODEX_SCRIPT.read_bytes())


if __name__ == "__main__":
    unittest.main()
