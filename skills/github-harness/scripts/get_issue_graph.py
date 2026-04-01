#!/usr/bin/env python3
from __future__ import annotations

import argparse

import _bootstrap  # noqa: F401
from github_harness.cli import run_cli
from github_harness.git import get_repo_name_with_owner
from github_harness.github import get_issue_details
from github_harness.issues import build_issue_graph_payload
from github_harness.json_output import emit_json


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Read blocker and hierarchy relationships for one issue.")
    parser.add_argument("--issue", required=True, type=int, help="Target issue number.")
    return parser


def main() -> None:
    args = build_parser().parse_args()

    repo = get_repo_name_with_owner()
    issue = get_issue_details(repo, args.issue)
    emit_json(build_issue_graph_payload(issue))


if __name__ == "__main__":
    run_cli(main)
