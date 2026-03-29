#!/usr/bin/env python3
from __future__ import annotations

import argparse

import _bootstrap  # noqa: F401
from github_harness.cli import run_mutation_cli
from github_harness.errors import HarnessError
from github_harness.git import get_repo_name_with_owner
from github_harness.github import (
    add_blocked_by,
    add_sub_issue,
    get_issue_node,
    get_issue_relationships,
    remove_blocked_by,
    remove_sub_issue,
)


def parse_issue_numbers(raw: str | None, *, allow_many: bool = True) -> list[int] | None:
    if raw is None:
        return None

    text = raw.strip()
    if not text or text.lower() in {"none", "null"}:
        return []

    numbers: list[int] = []
    for chunk in text.split(","):
        part = chunk.strip()
        if not part:
            continue
        if not part.isdigit():
            raise HarnessError(f"Invalid issue number: {part}")
        number = int(part)
        if number not in numbers:
            numbers.append(number)

    if not allow_many and len(numbers) > 1:
        raise HarnessError("Only one parent issue can be assigned at a time.")

    return numbers


def ensure_no_self_reference(target_issue: int, numbers: list[int] | None, relationship: str) -> None:
    if numbers is None:
        return
    if target_issue in numbers:
        raise HarnessError(f"Issue #{target_issue} cannot reference itself as {relationship}.")


def sync_blocked_by(repo: str, issue_id: str, current_numbers: set[int], desired_numbers: list[int]) -> None:
    desired_set = set(desired_numbers)

    for number in sorted(current_numbers - desired_set):
        blocker = get_issue_node(repo, number)
        remove_blocked_by(issue_id, blocker["id"])

    for number in desired_numbers:
        if number not in current_numbers:
            blocker = get_issue_node(repo, number)
            add_blocked_by(issue_id, blocker["id"])


def sync_blocking(repo: str, target_issue_id: str, current_numbers: set[int], desired_numbers: list[int]) -> None:
    desired_set = set(desired_numbers)

    for number in sorted(current_numbers - desired_set):
        blocked_issue = get_issue_node(repo, number)
        remove_blocked_by(blocked_issue["id"], target_issue_id)

    for number in desired_numbers:
        if number not in current_numbers:
            blocked_issue = get_issue_node(repo, number)
            add_blocked_by(blocked_issue["id"], target_issue_id)


def sync_parent(repo: str, target_issue_id: str, current_parent: int | None, desired_numbers: list[int]) -> None:
    desired_parent = desired_numbers[0] if desired_numbers else None

    if current_parent == desired_parent:
        return

    if desired_parent is None:
        if current_parent is None:
            return
        current_parent_node = get_issue_node(repo, current_parent)
        remove_sub_issue(current_parent_node["id"], target_issue_id)
        return

    parent_node = get_issue_node(repo, desired_parent)
    add_sub_issue(parent_node["id"], target_issue_id, replace_parent=True)


def sync_children(repo: str, issue_id: str, current_numbers: set[int], desired_numbers: list[int]) -> None:
    desired_set = set(desired_numbers)

    for number in sorted(current_numbers - desired_set):
        child_node = get_issue_node(repo, number)
        remove_sub_issue(issue_id, child_node["id"])

    for number in desired_numbers:
        if number not in current_numbers:
            child_node = get_issue_node(repo, number)
            add_sub_issue(issue_id, child_node["id"], replace_parent=True)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Synchronize canonical blocker and hierarchy relationships for one issue."
    )
    parser.add_argument("--issue", required=True, type=int, help="Target issue number.")
    parser.add_argument("--blocked-by", dest="blocked_by", help="Comma-separated blocker issue numbers.")
    parser.add_argument("--blocking", help="Comma-separated issue numbers blocked by the target issue.")
    parser.add_argument("--parent", help="Parent issue number, or 'none' to clear.")
    parser.add_argument("--children", help="Comma-separated sub-issue numbers, or 'none' to clear.")
    return parser


def main() -> None:
    args = build_parser().parse_args()

    blocked_by = parse_issue_numbers(args.blocked_by)
    blocking = parse_issue_numbers(args.blocking)
    parent = parse_issue_numbers(args.parent, allow_many=False)
    children = parse_issue_numbers(args.children)

    if all(selection is None for selection in (blocked_by, blocking, parent, children)):
        raise HarnessError("At least one relationship selector must be provided.")

    ensure_no_self_reference(args.issue, blocked_by, "blocked-by")
    ensure_no_self_reference(args.issue, blocking, "blocking")
    ensure_no_self_reference(args.issue, parent, "parent")
    ensure_no_self_reference(args.issue, children, "children")

    repo = get_repo_name_with_owner()
    issue = get_issue_relationships(repo, args.issue)
    issue_id = issue["id"]

    if blocked_by is not None:
        current = {node["number"] for node in issue["blockedBy"]["nodes"]}
        sync_blocked_by(repo, issue_id, current, blocked_by)

    if blocking is not None:
        current = {node["number"] for node in issue["blocking"]["nodes"]}
        sync_blocking(repo, issue_id, current, blocking)

    if parent is not None:
        current_parent = (issue.get("parent") or {}).get("number")
        sync_parent(repo, issue_id, current_parent, parent)

    if children is not None:
        current = {node["number"] for node in issue["subIssues"]["nodes"]}
        sync_children(repo, issue_id, current, children)


if __name__ == "__main__":
    run_mutation_cli(main)
