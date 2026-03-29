#!/usr/bin/env python3
from __future__ import annotations

import argparse

import _bootstrap  # noqa: F401
from github_harness.cli import run_cli
from github_harness.git import get_repo_name_with_owner
from github_harness.github import list_repository_issues
from github_harness.issues import build_issue_summary, matches_requested_kind
from github_harness.json_output import emit_json


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="List open issues with optional parent and kind filters.")
    parser.add_argument("--parent", type=int, help="Filter to issues whose parent matches this issue number.")
    parser.add_argument("--kind", help="Filter to issues with this kind project value.")
    return parser


def main() -> None:
    args = build_parser().parse_args()

    repo = get_repo_name_with_owner()
    issues = list_repository_issues(repo)
    open_issues = []
    for issue in issues:
        if issue.get("state") != "OPEN":
            continue
        parent_number = ((issue.get("parent") or {}).get("number")) or None
        if args.parent is not None and parent_number != args.parent:
            continue
        if not matches_requested_kind(issue, args.kind):
            continue
        open_issues.append(build_issue_summary(issue))

    open_issues.sort(key=lambda item: item["number"])
    emit_json(
        {
            "parent": args.parent,
            "kind": args.kind,
            "issues": open_issues,
            "issues_count": len(open_issues),
        }
    )


if __name__ == "__main__":
    run_cli(main)
