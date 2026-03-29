from __future__ import annotations

import argparse
import sys
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"
SCRIPTS = ROOT / "scripts"

if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

from github_harness.issues import (  # noqa: E402
    build_epic_entries,
    build_issue_payload,
    parse_issue_sections,
    rank_issue_matches,
    select_issue_read_fields,
    validate_issue_graph,
)
import read_issue  # noqa: E402


def make_issue(
    number: int,
    *,
    title: str,
    state: str = "OPEN",
    body: str = "",
    kind: str | None = None,
    kind_field_name: str = "kind",
    status: str | None = None,
    parent: dict | None = None,
    children: list[dict] | None = None,
    blocked_by: list[dict] | None = None,
    blocking: list[dict] | None = None,
) -> dict:
    project_item = {
        "status": {"name": status} if status else None,
        "priority": None,
        "size": None,
        "kind": {"name": kind} if kind and kind_field_name == "kind" else None,
        "kindLegacy": {"name": kind} if kind and kind_field_name == "Kind" else None,
    }
    return {
        "number": number,
        "title": title,
        "url": f"https://example.test/issues/{number}",
        "state": state,
        "body": body,
        "parent": parent,
        "subIssues": {"nodes": children or []},
        "blockedBy": {"nodes": blocked_by or []},
        "blocking": {"nodes": blocking or []},
        "projectItems": {"nodes": [project_item]},
    }


class IssueParsingTests(unittest.TestCase):
    def test_parse_issue_sections_extracts_expected_blocks(self) -> None:
        body = """## Description
Current state.

## Contract
Do the thing.

## Acceptance
1. Works.

## Design notes
Keep it small.
"""

        self.assertEqual(
            parse_issue_sections(body),
            {
                "description": "Current state.",
                "contract": "Do the thing.",
                "acceptance": "1. Works.",
                "design": "Keep it small.",
            },
        )

    def test_build_issue_payload_defaults_to_sections_fields_and_relationships(self) -> None:
        issue = make_issue(
            37,
            title="feat: implement reads",
            body="""## Description
Current state.

## Contract
Read the issue.

## Acceptance
1. Output JSON.

## Design notes
Preserve relationships.
""",
            status="Backlog",
            kind="implementation",
            parent={"number": 38, "title": "epic: reads", "url": "https://example.test/issues/38", "state": "OPEN"},
            children=[{"number": 40, "title": "feat: child", "url": "https://example.test/issues/40", "state": "OPEN"}],
            blocked_by=[{"number": 41, "title": "feat: blocker", "url": "https://example.test/issues/41", "state": "OPEN"}],
        )

        payload = build_issue_payload(issue, selectors=set())

        self.assertEqual(payload["issue"], 37)
        self.assertEqual(payload["description"], "Current state.")
        self.assertEqual(payload["status"], "Backlog")
        self.assertEqual(payload["kind"], "implementation")
        self.assertEqual(payload["children_count"], 1)
        self.assertEqual(payload["blocked_by_open_count"], 1)
        self.assertNotIn("comments", payload)

    def test_build_issue_payload_reads_kind_from_legacy_kind_field(self) -> None:
        issue = make_issue(
            38,
            title="feat: implement reads",
            kind="implementation",
            kind_field_name="Kind",
        )

        payload = build_issue_payload(issue, selectors={"kind"})

        self.assertEqual(payload["kind"], "implementation")

    def test_select_issue_read_fields_expands_all_to_include_comments(self) -> None:
        self.assertEqual(
            select_issue_read_fields(set(), include_all=True),
            {
                "description",
                "contract",
                "acceptance",
                "design",
                "comments",
                "status",
                "priority",
                "size",
                "kind",
                "parent",
                "children",
                "blocked_by",
                "blocking",
            },
        )

    def test_build_issue_payload_includes_requested_empty_collections_before_json_pruning(self) -> None:
        issue = make_issue(
            39,
            title="feat: empty relationships",
            body="""## Description
Current state.

## Contract
Read the issue.

## Acceptance
1. Output JSON.
""",
            status="Backlog",
            kind="implementation",
        )

        payload = build_issue_payload(
            issue,
            selectors={"comments", "children", "blocked_by", "blocking"},
            comments=[],
        )

        self.assertEqual(payload["comments"], [])
        self.assertEqual(payload["children"], [])
        self.assertEqual(payload["blocked_by"], [])
        self.assertEqual(payload["blocking"], [])
        self.assertEqual(payload["comments_count"], 0)
        self.assertEqual(payload["children_count"], 0)
        self.assertEqual(payload["blocked_by_count"], 0)
        self.assertEqual(payload["blocking_count"], 0)


class SearchTests(unittest.TestCase):
    def test_rank_issue_matches_can_include_description_text(self) -> None:
        issues = [
            make_issue(10, title="feat: bootstrap github harness"),
            make_issue(
                11,
                title="docs: operator notes",
                body="""## Description
Document the bootstrap workflow for the project.
""",
            ),
        ]

        title_only = rank_issue_matches(issues, keywords="bootstrap", include_description=False)
        with_description = rank_issue_matches(issues, keywords="bootstrap", include_description=True)

        self.assertEqual([item["number"] for item in title_only], [10])
        self.assertEqual({item["number"] for item in with_description}, {10, 11})


class EpicListingTests(unittest.TestCase):
    def test_build_epic_entries_includes_child_progress_counts(self) -> None:
        epic = make_issue(
            50,
            title="epic: issue reads",
            kind="epic",
            status="Backlog",
            children=[
                {"number": 51, "title": "feat: read issue", "url": "https://example.test/issues/51", "state": "OPEN"},
                {"number": 52, "title": "feat: read comments", "url": "https://example.test/issues/52", "state": "CLOSED"},
            ],
        )
        child_open = make_issue(
            51,
            title="feat: read issue",
            kind="implementation",
            status="In Progress",
            parent={"number": 50, "title": "epic: issue reads", "url": "https://example.test/issues/50", "state": "OPEN"},
        )
        child_closed = make_issue(
            52,
            title="feat: read comments",
            state="CLOSED",
            kind="implementation",
            status="In Progress",
            parent={"number": 50, "title": "epic: issue reads", "url": "https://example.test/issues/50", "state": "OPEN"},
        )

        epics = build_epic_entries([epic, child_open, child_closed])

        self.assertEqual(len(epics), 1)
        self.assertEqual(epics[0]["children_count"], 2)
        self.assertEqual(epics[0]["children_open_count"], 1)
        self.assertEqual(epics[0]["children_closed_count"], 1)


class GraphValidationTests(unittest.TestCase):
    def test_validate_issue_graph_reports_cycles_breaks_and_orphans(self) -> None:
        issue_60 = make_issue(
            60,
            title="epic: graph",
            kind="epic",
            children=[{"number": 61, "title": "feat: child", "url": "https://example.test/issues/61", "state": "OPEN"}],
            blocked_by=[{"number": 61, "title": "feat: child", "url": "https://example.test/issues/61", "state": "OPEN"}],
            blocking=[{"number": 61, "title": "feat: child", "url": "https://example.test/issues/61", "state": "OPEN"}],
        )
        issue_61 = make_issue(
            61,
            title="feat: child",
            kind="implementation",
            parent={"number": 60, "title": "epic: graph", "url": "https://example.test/issues/60", "state": "OPEN"},
            children=[{"number": 60, "title": "epic: graph", "url": "https://example.test/issues/60", "state": "OPEN"}],
            blocked_by=[{"number": 60, "title": "epic: graph", "url": "https://example.test/issues/60", "state": "OPEN"}],
            blocking=[{"number": 60, "title": "epic: graph", "url": "https://example.test/issues/60", "state": "OPEN"}],
        )
        issue_62 = make_issue(62, title="feat: orphan", kind="implementation")
        issue_63 = make_issue(
            63,
            title="feat: broken parent",
            kind="implementation",
            parent={"number": 999, "title": "epic: missing", "url": "https://example.test/issues/999", "state": "OPEN"},
        )

        result = validate_issue_graph([issue_60, issue_61, issue_62, issue_63])

        self.assertFalse(result["valid"])
        self.assertEqual(result["hierarchy_cycles"], [[60, 61, 60]])
        self.assertEqual(result["dependency_cycles"], [[60, 61, 60]])
        self.assertEqual(
            [item["issue"] for item in result["orphaned_children"]],
            [62],
        )
        self.assertIn(63, [item["issue"] for item in result["broken_references"]])


class ReadIssueScriptTests(unittest.TestCase):
    def test_main_with_all_fetches_comments_and_uses_effective_selectors(self) -> None:
        parser = mock.Mock()
        parser.parse_args.return_value = argparse.Namespace(
            issue=37,
            all=True,
            description=False,
            contract=False,
            acceptance=False,
            design=False,
            comments=False,
            status=False,
            priority=False,
            size=False,
            kind=False,
            parent=False,
            children=False,
            blocked_by=False,
            blocking=False,
        )

        with (
            mock.patch.object(read_issue, "build_parser", return_value=parser),
            mock.patch.object(read_issue, "get_repo_name_with_owner", return_value="fedemagnani/harness-eng-skills"),
            mock.patch.object(read_issue, "get_issue_details", return_value={"number": 37}),
            mock.patch.object(read_issue, "get_issue_comments", return_value={"comments": [{"bodyText": "note"}]}) as get_issue_comments,
            mock.patch.object(read_issue, "build_comment_entries", return_value=[{"comment": "note"}]),
            mock.patch.object(read_issue, "build_issue_payload", return_value={"issue": 37}) as build_issue_payload,
            mock.patch.object(read_issue, "emit_json") as emit_json,
        ):
            read_issue.main()

        get_issue_comments.assert_called_once_with("fedemagnani/harness-eng-skills", 37)
        build_issue_payload.assert_called_once()
        self.assertIn("comments", build_issue_payload.call_args.kwargs["selectors"])
        self.assertEqual(
            emit_json.call_args.kwargs["preserve_empty_keys"],
            {"comments", "children", "blocked_by", "blocking"},
        )

    def test_main_uses_effective_selectors_for_comment_fetch(self) -> None:
        parser = mock.Mock()
        parser.parse_args.return_value = argparse.Namespace(
            issue=37,
            all=False,
            description=False,
            contract=False,
            acceptance=False,
            design=False,
            comments=False,
            status=False,
            priority=False,
            size=False,
            kind=False,
            parent=False,
            children=False,
            blocked_by=False,
            blocking=False,
        )

        with (
            mock.patch.object(read_issue, "build_parser", return_value=parser),
            mock.patch.object(read_issue, "get_repo_name_with_owner", return_value="fedemagnani/harness-eng-skills"),
            mock.patch.object(read_issue, "get_issue_details", return_value={"number": 37}),
            mock.patch.object(read_issue, "select_issue_read_fields", return_value={"comments"}),
            mock.patch.object(read_issue, "get_issue_comments", return_value={"comments": [{"bodyText": "note"}]}) as get_issue_comments,
            mock.patch.object(read_issue, "build_comment_entries", return_value=[{"comment": "note"}]),
            mock.patch.object(read_issue, "build_issue_payload", return_value={"issue": 37}) as build_issue_payload,
            mock.patch.object(read_issue, "emit_json") as emit_json,
        ):
            read_issue.main()

        get_issue_comments.assert_called_once_with("fedemagnani/harness-eng-skills", 37)
        build_issue_payload.assert_called_once()
        self.assertEqual(emit_json.call_args.kwargs["preserve_empty_keys"], {"comments"})

    def test_main_preserves_requested_empty_relationship_collections(self) -> None:
        parser = mock.Mock()
        parser.parse_args.return_value = argparse.Namespace(
            issue=37,
            all=False,
            description=False,
            contract=False,
            acceptance=False,
            design=False,
            comments=False,
            status=False,
            priority=False,
            size=False,
            kind=False,
            parent=False,
            children=False,
            blocked_by=False,
            blocking=False,
        )

        with (
            mock.patch.object(read_issue, "build_parser", return_value=parser),
            mock.patch.object(read_issue, "get_repo_name_with_owner", return_value="fedemagnani/harness-eng-skills"),
            mock.patch.object(read_issue, "get_issue_details", return_value={"number": 37}),
            mock.patch.object(read_issue, "select_issue_read_fields", return_value={"children", "blocked_by", "blocking"}),
            mock.patch.object(read_issue, "build_issue_payload", return_value={"issue": 37}) as build_issue_payload,
            mock.patch.object(read_issue, "emit_json") as emit_json,
        ):
            read_issue.main()

        build_issue_payload.assert_called_once()
        self.assertEqual(emit_json.call_args.kwargs["preserve_empty_keys"], {"children", "blocked_by", "blocking"})


if __name__ == "__main__":
    unittest.main()
