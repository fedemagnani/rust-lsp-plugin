#!/usr/bin/env python3
from __future__ import annotations

import argparse

import _bootstrap  # noqa: F401
from github_harness.cli import run_mutation_cli
from github_harness.errors import HarnessError
from github_harness.git import get_repo_name_with_owner
from github_harness.github import get_issue_details, update_issue as update_issue_remote
from github_harness.issues import rewrite_issue_body, validate_issue_title


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Rewrite selected issue sections in place without disturbing untouched content."
    )
    parser.add_argument("--issue", required=True, type=int, help="Target issue number.")
    parser.add_argument("--title", help="Replacement issue title.")
    parser.add_argument("--description", help="Replacement Description section text.")
    parser.add_argument("--contract", help="Replacement Contract section text.")
    parser.add_argument("--acceptance", help="Replacement Acceptance section text.")
    parser.add_argument("--design", help="Replacement Design notes section text. Use an empty string to clear it.")
    return parser


def main() -> None:
    args = build_parser().parse_args()

    section_updates = {
        "description": args.description,
        "contract": args.contract,
        "acceptance": args.acceptance,
        "design": args.design,
    }
    if args.title is None and all(value is None for value in section_updates.values()):
        raise HarnessError("At least one title or section update must be provided.")

    repo = get_repo_name_with_owner()
    issue = get_issue_details(repo, args.issue)
    current_title = str(issue.get("title") or "").strip()
    current_body = str(issue.get("body") or "")

    desired_title = validate_issue_title(args.title) if args.title is not None else current_title

    active_updates = {key: value for key, value in section_updates.items() if value is not None}
    desired_body = rewrite_issue_body(current_body, **active_updates) if active_updates else current_body

    if desired_title == current_title and desired_body.strip() == current_body.strip():
        return

    update_issue_remote(issue["id"], title=desired_title, body=desired_body)


if __name__ == "__main__":
    run_mutation_cli(main)
