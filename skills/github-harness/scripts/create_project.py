#!/usr/bin/env python3
from __future__ import annotations

import argparse

import _bootstrap  # noqa: F401
from github_harness.cli import run_mutation_cli
from github_harness.git import get_repo_name_with_owner
from github_harness.github import bootstrap_repository_project


def build_parser() -> argparse.ArgumentParser:
    return argparse.ArgumentParser(
        description=(
            "Bootstrap or validate the repository project and ensure the required "
            "Status, kind, Priority, and Size fields."
        )
    )


def main() -> None:
    build_parser().parse_args()

    repo = get_repo_name_with_owner()
    bootstrap_repository_project(repo)


if __name__ == "__main__":
    run_mutation_cli(main)
