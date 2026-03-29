#!/usr/bin/env python3
from __future__ import annotations

import argparse

import _bootstrap  # noqa: F401
from github_harness.authoring import create_managed_issue
from github_harness.cli import run_mutation_cli
from github_harness.git import get_repo_name_with_owner


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Create a managed epic issue and set its project kind.")
    parser.add_argument("--title", required=True, help="Issue title in the form 'epic: <concept>'.")
    parser.add_argument("--description", required=True, help="Description section text.")
    parser.add_argument("--contract", required=True, help="Contract section text.")
    parser.add_argument("--acceptance", required=True, help="Acceptance section text.")
    parser.add_argument("--design", default="", help="Optional Design notes section text.")
    return parser


def main() -> None:
    args = build_parser().parse_args()
    repo = get_repo_name_with_owner()
    create_managed_issue(
        repo,
        kind="epic",
        title=args.title,
        description=args.description,
        contract=args.contract,
        acceptance=args.acceptance,
        design=args.design,
        expected_title_type="epic",
    )


if __name__ == "__main__":
    run_mutation_cli(main)
