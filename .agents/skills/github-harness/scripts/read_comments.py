#!/usr/bin/env python3
from __future__ import annotations

import argparse

import _bootstrap  # noqa: F401
from github_harness.cli import run_cli
from github_harness.git import get_repo_name_with_owner
from github_harness.github import get_issue_comments
from github_harness.issues import build_comment_entries
from github_harness.json_output import emit_json


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Read issue comments without the rest of the issue body.")
    parser.add_argument("--issue", required=True, type=int, help="Target issue number.")
    return parser


def main() -> None:
    args = build_parser().parse_args()

    repo = get_repo_name_with_owner()
    payload = get_issue_comments(repo, args.issue)
    comments = build_comment_entries(payload.get("comments") or [])
    emit_json(
        {
            "issue": payload.get("number"),
            "title": payload.get("title"),
            "url": payload.get("url"),
            "state": payload.get("state"),
            "comments": comments,
            "comments_count": len(comments),
        }
    )


if __name__ == "__main__":
    run_cli(main)
