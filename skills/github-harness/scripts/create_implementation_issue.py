#!/usr/bin/env python3
from __future__ import annotations

import argparse

import _bootstrap  # noqa: F401
from github_harness.authoring import create_managed_issue, parse_issue_number_list
from github_harness.cli import run_mutation_cli
from github_harness.git import get_repo_name_with_owner


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Create a managed implementation issue and set its project kind."
    )
    parser.add_argument("--title", required=True, help="Issue title in the form '<type>: <short title>'.")
    parser.add_argument("--description", required=True, help="Description section text.")
    parser.add_argument("--contract", required=True, help="Contract section text.")
    parser.add_argument("--acceptance", required=True, help="Acceptance section text.")
    parser.add_argument("--design", default="", help="Optional Design notes section text.")
    parser.add_argument("--parent", help="Optional parent epic issue number.")
    parser.add_argument("--blocked-by", dest="blocked_by", help="Comma-separated blocker issue numbers.")
    parser.add_argument("--blocking", help="Comma-separated issue numbers blocked by the new issue.")
    return parser


def main() -> None:
    args = build_parser().parse_args()

    repo = get_repo_name_with_owner()
    create_managed_issue(
        repo,
        kind="implementation",
        title=args.title,
        description=args.description,
        contract=args.contract,
        acceptance=args.acceptance,
        design=args.design,
        parent=parse_issue_number_list(args.parent, allow_many=False),
        blocked_by=parse_issue_number_list(args.blocked_by),
        blocking=parse_issue_number_list(args.blocking),
    )


if __name__ == "__main__":
    run_mutation_cli(main)
