#!/usr/bin/env python3
from __future__ import annotations

import argparse

import _bootstrap  # noqa: F401
from github_harness.cli import run_mutation_cli
from github_harness.git import get_current_branch, get_repo_name_with_owner
from github_harness.github import build_closing_body, get_repo_view, run_gh, run_gh_json


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Create or reuse the open pull request for the current branch."
    )
    parser.add_argument("--title", required=True, help="Pull request title.")
    parser.add_argument("--description", required=True, help="Pull request body.")
    parser.add_argument("--closes", required=True, type=int, help="Issue number closed by this PR.")
    return parser


def find_open_pull_request(repo: str, branch: str) -> dict | None:
    payload = run_gh_json(
        [
            "pr",
            "list",
            "--repo",
            repo,
            "--head",
            branch,
            "--state",
            "open",
            "--json",
            "number,body,headRefName",
        ]
    )
    if not payload:
        return None
    return payload[0]


def main() -> None:
    args = build_parser().parse_args()

    repo = get_repo_name_with_owner()
    branch = get_current_branch()
    desired_body = build_closing_body(args.description, args.closes)
    existing_pr = find_open_pull_request(repo, branch)

    if existing_pr is not None:
        current_body = existing_pr.get("body") or ""
        updated_body = build_closing_body(current_body, args.closes)
        if updated_body.strip() != current_body.strip():
            run_gh(
                [
                    "pr",
                    "edit",
                    str(existing_pr["number"]),
                    "--repo",
                    repo,
                    "--body",
                    updated_body,
                ]
            )
        return

    repo_view = get_repo_view(repo)
    default_branch = ((repo_view.get("defaultBranchRef") or {}).get("name")) or "main"
    run_gh(
        [
            "pr",
            "create",
            "--repo",
            repo,
            "--head",
            branch,
            "--base",
            default_branch,
            "--title",
            args.title,
            "--body",
            desired_body,
        ]
    )


if __name__ == "__main__":
    run_mutation_cli(main)
