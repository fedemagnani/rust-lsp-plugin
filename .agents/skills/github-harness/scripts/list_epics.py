#!/usr/bin/env python3
from __future__ import annotations

import argparse

import _bootstrap  # noqa: F401
from github_harness.cli import run_cli
from github_harness.git import get_repo_name_with_owner
from github_harness.github import list_repository_issues
from github_harness.issues import build_epic_entries
from github_harness.json_output import emit_json


def build_parser() -> argparse.ArgumentParser:
    return argparse.ArgumentParser(description="List open epic issues with child progress context.")


def main() -> None:
    build_parser().parse_args()

    repo = get_repo_name_with_owner()
    issues = list_repository_issues(repo)
    epics = build_epic_entries(issues)
    emit_json({"epics": epics, "epics_count": len(epics)})


if __name__ == "__main__":
    run_cli(main)
