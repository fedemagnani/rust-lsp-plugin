from __future__ import annotations

import contextlib
import io
import sys
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "src"

if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

SCRIPTS = ROOT / "scripts"
if str(SCRIPTS) not in sys.path:
    sys.path.insert(0, str(SCRIPTS))

from github_harness.github import build_closing_body, build_graphql_args, normalize_status_label
from github_harness.json_output import prune_empty
from github_harness.cli import SUCCESS_SENTINEL, run_mutation_cli
from github_harness.errors import HarnessError
from github_harness.troubleshooting import classify_gh_failure, unsupported_github_remote_message
from read_pr_reviews import build_review_entries


class JsonOutputTests(unittest.TestCase):
    def test_prune_empty_removes_null_empty_and_blank_values(self) -> None:
        payload = {
            "keep_false": False,
            "keep_zero": 0,
            "drop_none": None,
            "drop_blank": "   ",
            "drop_empty_list": [],
            "nested": {
                "keep": "value",
                "drop": "",
            },
            "items": [
                {"comment": "value", "empty": ""},
                {},
            ],
        }

        self.assertEqual(
            prune_empty(payload),
            {
                "keep_false": False,
                "keep_zero": 0,
                "nested": {"keep": "value"},
                "items": [{"comment": "value"}],
            },
        )

    def test_prune_empty_preserves_selected_top_level_empty_lists(self) -> None:
        payload = {
            "comments": [],
            "children": [{}],
            "blocked_by": [],
            "drop_empty_list": [],
            "nested": {
                "drop_empty_list": [],
            },
        }

        self.assertEqual(
            prune_empty(payload, preserve_empty_keys={"comments", "children", "blocked_by"}),
            {
                "comments": [],
                "children": [],
                "blocked_by": [],
            },
        )


class GithubHelperTests(unittest.TestCase):
    def test_build_closing_body_appends_reference_once(self) -> None:
        self.assertEqual(
            build_closing_body("Ship the execution flow.", 36),
            "Ship the execution flow.\n\nCloses #36",
        )
        self.assertEqual(
            build_closing_body("Ship the execution flow.\n\nCloses #36", 36),
            "Ship the execution flow.\n\nCloses #36",
        )

    def test_normalize_status_label_normalizes_spaces_and_case(self) -> None:
        self.assertEqual(normalize_status_label("In Progress"), "in-progress")
        self.assertEqual(normalize_status_label("in_progress"), "in-progress")

    def test_build_graphql_args_uses_raw_flags_for_strings_and_typed_flags_for_bool_and_int(self) -> None:
        self.assertEqual(
            build_graphql_args(
                "query Test { viewer { login } }",
                owner="12345",
                repo="false",
                number=36,
                replaceParent=True,
            ),
            [
                "api",
                "graphql",
                "-f",
                "query=query Test { viewer { login } }",
                "-f",
                "owner=12345",
                "-f",
                "repo=false",
                "-F",
                "number=36",
                "-F",
                "replaceParent=true",
            ],
        )


class TroubleshootingTests(unittest.TestCase):
    def test_classify_gh_failure_recommends_scope_refresh(self) -> None:
        message = (
            "Your token has not been granted the required scopes to execute this query. "
            "The 'id' field requires one of the following scopes: ['read:project']."
        )
        classified = classify_gh_failure(message)
        self.assertIn("gh auth refresh --scopes read:project,project", classified)
        self.assertIn("read:project", classified)

    def test_unsupported_remote_message_recommends_resetting_origin(self) -> None:
        message = unsupported_github_remote_message("git@gitlab.com:team/repo.git")
        self.assertIn("git remote set-url origin", message)
        self.assertIn("GitHub", message)


class CliTests(unittest.TestCase):
    def test_run_mutation_cli_prints_exact_success_sentinel(self) -> None:
        stdout = io.StringIO()
        stderr = io.StringIO()

        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            run_mutation_cli(lambda: None)

        self.assertEqual(stdout.getvalue(), f"{SUCCESS_SENTINEL}\n")
        self.assertEqual(stderr.getvalue(), "")

    def test_run_mutation_cli_exits_non_zero_and_prints_error(self) -> None:
        stdout = io.StringIO()
        stderr = io.StringIO()

        with (
            contextlib.redirect_stdout(stdout),
            contextlib.redirect_stderr(stderr),
            self.assertRaises(SystemExit) as exc_info,
        ):
            run_mutation_cli(lambda: (_ for _ in ()).throw(HarnessError("boom")))

        self.assertEqual(exc_info.exception.code, 1)
        self.assertEqual(stdout.getvalue(), "")
        self.assertEqual(stderr.getvalue(), "boom\n")

    def test_state_changing_scripts_use_mutation_wrapper(self) -> None:
        expected = {
            "add_comment.py",
            "create_epic_issue.py",
            "create_implementation_issue.py",
            "create_pr.py",
            "create_project.py",
            "update_dependencies.py",
            "update_issue.py",
        }

        for script_name in expected:
            script_path = SCRIPTS / script_name
            script = script_path.read_text(encoding="utf-8")
            self.assertIn("from github_harness.cli import run_mutation_cli", script, script_name)
            self.assertIn("run_mutation_cli(main)", script, script_name)


class ReadPrReviewsTests(unittest.TestCase):
    def test_build_review_entries_flattens_review_bodies_and_comments(self) -> None:
        payload = {
            "headRefOid": "head123",
            "reviews": {
                "nodes": [
                    {
                        "id": "R_1",
                        "fullDatabaseId": 101,
                        "state": "COMMENTED",
                        "bodyText": "Please tighten the docs.",
                        "submittedAt": "2026-03-15T12:00:00Z",
                        "author": {"login": "fedemagnani"},
                        "commit": {"oid": "head123"},
                        "comments": {
                            "nodes": [
                                {
                                    "id": "PRRC_1",
                                    "fullDatabaseId": 201,
                                    "bodyText": "This note needs a clearer acceptance section.",
                                    "path": "github-harness/references/IMPLEMENTATION_WORKFLOWS.md",
                                    "line": 12,
                                    "originalLine": 12,
                                    "outdated": False,
                                    "publishedAt": "2026-03-15T12:01:00Z",
                                    "author": {"login": "fedemagnani"},
                                    "commit": {"oid": "head123"},
                                }
                            ]
                        },
                    }
                ]
            },
        }

        reviews, reviews_new = build_review_entries(payload)

        self.assertEqual(len(reviews), 2)
        self.assertEqual(len(reviews_new), 2)
        self.assertEqual(reviews[0]["review_id"], 101)
        self.assertEqual(reviews[1]["review_comment_id"], 201)


if __name__ == "__main__":
    unittest.main()
