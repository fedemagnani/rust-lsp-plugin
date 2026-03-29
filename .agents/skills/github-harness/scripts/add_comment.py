#!/usr/bin/env python3
from __future__ import annotations

import argparse

import _bootstrap  # noqa: F401
from github_harness.cli import run_mutation_cli
from github_harness.errors import HarnessError
from github_harness.git import get_repo_name_with_owner
from github_harness.github import add_issue_comment, get_issue_node


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Add a follow-up comment to one issue.")
    parser.add_argument("--issue", required=True, type=int, help="Target issue number.")
    parser.add_argument("comment", help="Comment body text.")
    return parser


def main() -> None:
    args = build_parser().parse_args()

    comment = args.comment.strip()
    if not comment:
        raise HarnessError("Comment text cannot be empty.")

    repo = get_repo_name_with_owner()
    issue = get_issue_node(repo, args.issue)
    add_issue_comment(issue["id"], comment)


if __name__ == "__main__":
    run_mutation_cli(main)
