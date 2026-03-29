#!/usr/bin/env python3
from __future__ import annotations

import argparse

import _bootstrap  # noqa: F401
from github_harness.cli import run_cli
from github_harness.git import get_repo_name_with_owner
from github_harness.github import list_repository_issues
from github_harness.issues import rank_issue_matches
from github_harness.json_output import emit_json


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Search likely duplicate or adjacent issues.")
    parser.add_argument("--keywords", required=True, help="Keywords to search for.")
    parser.add_argument(
        "--description",
        action="store_true",
        help="Include issue descriptions in addition to titles when ranking matches.",
    )
    return parser


def main() -> None:
    args = build_parser().parse_args()

    repo = get_repo_name_with_owner()
    issues = list_repository_issues(repo)
    matches = rank_issue_matches(
        issues,
        keywords=args.keywords,
        include_description=args.description,
    )
    emit_json(
        {
            "keywords": args.keywords,
            "description": args.description,
            "matches": matches,
            "matches_count": len(matches),
        }
    )


if __name__ == "__main__":
    run_cli(main)
