#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import _bootstrap  # noqa: F401
from github_harness.cli import run_cli
from github_harness.errors import HarnessError
from github_harness.git import get_repo_name_with_owner
from github_harness.json_output import emit_json

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[1]
VALIDATION_CATEGORIES = (
    "bootstrap",
    "issue_authoring",
    "issue_read",
    "query",
    "relationships",
    "assessment",
    "blocked_new",
    "pull_request",
    "idempotency",
)


@dataclass(frozen=True)
class ValidationNames:
    label: str
    slug: str
    prefix: str
    epic_title: str
    implementation_title: str
    blocker_title: str
    branch: str
    pr_title: str
    commit_message: str


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Exercise the github-harness workflow against live GitHub state and emit a report."
    )
    parser.add_argument(
        "--label",
        help="Optional validation label. Defaults to the current UTC timestamp.",
    )
    parser.add_argument(
        "--keep-branch",
        action="store_true",
        help="Leave the validation branch checked out instead of restoring the original ref.",
    )
    return parser


def build_validation_names(label: str | None) -> ValidationNames:
    normalized = (label or datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")).strip()
    if not normalized:
        raise HarnessError("Validation label cannot be empty.")

    slug = "".join(ch.lower() if ch.isalnum() else "-" for ch in normalized).strip("-")
    if not slug:
        raise HarnessError("Validation label must contain at least one alphanumeric character.")

    prefix = f"validation {normalized}"
    return ValidationNames(
        label=normalized,
        slug=slug,
        prefix=prefix,
        epic_title=f"epic: {prefix} live workflow",
        implementation_title=f"feat: {prefix} implementation path",
        blocker_title=f"feat: {prefix} blocker path",
        branch=f"validation/{slug}",
        pr_title=f"test: {prefix} live workflow",
        commit_message=f"test(github-harness): validate live workflow {normalized}",
    )


def run_command(command: list[str]) -> str:
    completed = subprocess.run(
        command,
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        message = completed.stderr.strip() or completed.stdout.strip() or "Command failed."
        raise HarnessError(message)
    return completed.stdout


def run_json_command(command: list[str]) -> Any:
    output = run_command(command)
    if not output.strip():
        return None
    try:
        return json.loads(output)
    except json.JSONDecodeError as exc:
        raise HarnessError("Command returned invalid JSON.") from exc


def run_script(script_name: str, *args: str, expect_json: bool = False) -> Any:
    command = [sys.executable, str(SCRIPT_DIR / script_name), *args]
    if expect_json:
        return run_json_command(command)
    run_command(command)
    return None


def run_gh_json(*args: str) -> Any:
    return run_json_command(["gh", *args])


def ensure_clean_worktree() -> None:
    status = run_command(["git", "status", "--short"]).strip()
    if status:
        raise HarnessError("Validation requires a clean git worktree.")


def current_checkout_ref() -> tuple[str, bool]:
    branch = run_command(["git", "branch", "--show-current"]).strip()
    if branch:
        return branch, True
    return run_command(["git", "rev-parse", "HEAD"]).strip(), False


def restore_checkout(ref: str) -> None:
    run_command(["git", "checkout", ref])


def find_exact_match(items: list[dict[str, Any]], title: str) -> dict[str, Any]:
    matches = [item for item in items if item.get("title") == title]
    if not matches:
        raise HarnessError(f"Could not find validation artifact titled '{title}'.")
    if len(matches) > 1:
        raise HarnessError(f"Found multiple validation artifacts titled '{title}'.")
    return matches[0]


def list_open_pull_requests(repo: str, branch: str) -> list[dict[str, Any]]:
    payload = run_gh_json(
        "pr",
        "list",
        "--repo",
        repo,
        "--head",
        branch,
        "--state",
        "open",
        "--json",
        "number,title,url,body,headRefName",
    )
    if not isinstance(payload, list):
        raise HarnessError("GitHub CLI did not return the expected pull request list.")
    return payload


def filter_validation_graph_findings(report: dict[str, Any], issue_numbers: set[int]) -> dict[str, Any]:
    return {
        "hierarchy_cycles": [
            cycle for cycle in report.get("hierarchy_cycles") or [] if any(node in issue_numbers for node in cycle)
        ],
        "dependency_cycles": [
            cycle for cycle in report.get("dependency_cycles") or [] if any(node in issue_numbers for node in cycle)
        ],
        "broken_references": [
            item
            for item in report.get("broken_references") or []
            if int(item.get("issue") or 0) in issue_numbers or int(item.get("target") or 0) in issue_numbers
        ],
        "orphaned_children": [
            item for item in report.get("orphaned_children") or [] if int(item.get("issue") or 0) in issue_numbers
        ],
    }


def compute_scores(steps: list[dict[str, Any]]) -> dict[str, int]:
    relevant = [step for step in steps if step.get("status") != "skipped"]
    passed = [step for step in relevant if step.get("status") == "passed"]
    effectiveness = int(round(100 * len(passed) / len(relevant))) if relevant else 0

    covered = {
        str(step.get("category"))
        for step in steps
        if step.get("status") == "passed" and str(step.get("category") or "")
    }
    feature_completeness = int(round(100 * len(covered & set(VALIDATION_CATEGORIES)) / len(VALIDATION_CATEGORIES)))

    isolation_steps = [step for step in steps if step.get("category") == "isolation"]
    isolation = 100 if isolation_steps and all(step.get("status") == "passed" for step in isolation_steps) else 0

    return {
        "effectiveness_soundness": effectiveness,
        "feature_completeness": feature_completeness,
        "isolation": isolation,
    }


def append_step(report: dict[str, Any], *, name: str, category: str, status: str, evidence: dict[str, Any]) -> None:
    report["steps"].append(
        {
            "name": name,
            "category": category,
            "status": status,
            "evidence": evidence,
        }
    )


def main() -> None:
    args = build_parser().parse_args()

    os.chdir(REPO_ROOT)

    repo = get_repo_name_with_owner()
    names = build_validation_names(args.label)
    report: dict[str, Any] = {
        "repository": repo,
        "label": names.label,
        "prefix": names.prefix,
        "branch": names.branch,
        "steps": [],
        "artifacts": {},
    }
    tracked_issues: set[int] = set()
    original_ref: str | None = None
    restore_needed = False

    try:
        ensure_clean_worktree()

        try:
            run_script("create_project.py")
            run_script("create_project.py")
            append_step(
                report,
                name="bootstrap repository project",
                category="bootstrap",
                status="passed",
                evidence={"rerun": "second bootstrap succeeded without error"},
            )
        except HarnessError as exc:
            append_step(
                report,
                name="bootstrap repository project",
                category="bootstrap",
                status="failed",
                evidence={"error": str(exc)},
            )

        epic: dict[str, Any] | None = None
        try:
            run_script(
                "create_epic_issue.py",
                "--title",
                names.epic_title,
                "--description",
                f"{names.prefix} validates the live github-harness flow on this repository.",
                "--contract",
                "The validation epic must track the live workflow artifacts needed to judge the harness end to end.",
                "--acceptance",
                "1. The live validation flow creates and reads managed issues. 2. It verifies relationships and pull-request creation.",
                "--design",
                "Validation artifacts must remain clearly prefixed and isolated.",
            )
            epic_payload = run_script("list_epics.py", expect_json=True)
            epic = find_exact_match(list(epic_payload.get("epics") or []), names.epic_title)
            report["artifacts"]["epic"] = epic
            tracked_issues.add(int(epic["number"]))
            append_step(
                report,
                name="create validation epic",
                category="issue_authoring",
                status="passed",
                evidence={"issue": epic["number"], "title": epic["title"]},
            )
        except HarnessError as exc:
            append_step(
                report,
                name="create validation epic",
                category="issue_authoring",
                status="failed",
                evidence={"error": str(exc)},
            )
            epic = None

        implementation: dict[str, Any] | None = None
        blocker: dict[str, Any] | None = None
        if epic is not None:
            try:
                run_script(
                    "create_implementation_issue.py",
                    "--title",
                    names.implementation_title,
                    "--description",
                    f"{names.prefix} implementation issue used to validate live command execution.",
                    "--contract",
                    "The implementation issue must exercise reads, dependency updates, and pull-request creation.",
                    "--acceptance",
                    "1. The issue can be read with --all. 2. It can be linked to a blocker. 3. It can be targeted by create_pr.py.",
                    "--design",
                    "Keep the validation implementation issue narrow and repo-scoped.",
                    "--parent",
                    str(epic["number"]),
                )
                run_script(
                    "create_implementation_issue.py",
                    "--title",
                    names.blocker_title,
                    "--description",
                    f"{names.prefix} blocker issue used to validate blocked-new handling and dependency updates.",
                    "--contract",
                    "The blocker issue must remain linked to the validation implementation issue.",
                    "--acceptance",
                    "1. The blocker can be discovered through issue graph reads. 2. The blocked-new stash message can reference it.",
                    "--design",
                    "Use the blocker only for validation graph and stash checks.",
                    "--parent",
                    str(epic["number"]),
                )
                open_issues = run_script(
                    "list_open_issues.py",
                    "--parent",
                    str(epic["number"]),
                    "--kind",
                    "implementation",
                    expect_json=True,
                )
                issue_entries = list(open_issues.get("issues") or [])
                implementation = find_exact_match(issue_entries, names.implementation_title)
                blocker = find_exact_match(issue_entries, names.blocker_title)
                report["artifacts"]["implementation_issue"] = implementation
                report["artifacts"]["blocker_issue"] = blocker
                tracked_issues.update({int(implementation["number"]), int(blocker["number"])})
                append_step(
                    report,
                    name="create validation implementation issues",
                    category="issue_authoring",
                    status="passed",
                    evidence={
                        "implementation_issue": implementation["number"],
                        "blocker_issue": blocker["number"],
                    },
                )
            except HarnessError as exc:
                append_step(
                    report,
                    name="create validation implementation issues",
                    category="issue_authoring",
                    status="failed",
                    evidence={"error": str(exc)},
                )

        if implementation is not None:
            try:
                issue_payload = run_script(
                    "read_issue.py",
                    "--issue",
                    str(implementation["number"]),
                    "--all",
                    expect_json=True,
                )
                has_expected_fields = {"description", "contract", "acceptance", "design", "comments"}.issubset(
                    set(issue_payload.keys())
                )
                if not has_expected_fields:
                    raise HarnessError("read_issue.py --all did not return the expected full payload.")
                append_step(
                    report,
                    name="read issue with all selectors",
                    category="issue_read",
                    status="passed",
                    evidence={"issue": implementation["number"], "keys": sorted(issue_payload.keys())},
                )
            except HarnessError as exc:
                append_step(
                    report,
                    name="read issue with all selectors",
                    category="issue_read",
                    status="failed",
                    evidence={"error": str(exc)},
                )
        else:
            append_step(
                report,
                name="read issue with all selectors",
                category="issue_read",
                status="skipped",
                evidence={"reason": "validation implementation issue was not created"},
            )

        if epic is not None and implementation is not None and blocker is not None:
            try:
                run_script(
                    "update_dependencies.py",
                    "--issue",
                    str(implementation["number"]),
                    "--blocked-by",
                    str(blocker["number"]),
                )
                graph_payload = run_script(
                    "get_issue_graph.py",
                    "--issue",
                    str(implementation["number"]),
                    expect_json=True,
                )
                blocked_by_numbers = {int(node["number"]) for node in graph_payload.get("blocked_by") or []}
                parent_number = int((graph_payload.get("parent") or {}).get("number") or 0)
                if blocked_by_numbers != {int(blocker["number"])} or parent_number != int(epic["number"]):
                    raise HarnessError("Issue graph does not reflect the expected validation relationships.")
                append_step(
                    report,
                    name="update and read issue relationships",
                    category="relationships",
                    status="passed",
                    evidence={
                        "implementation_issue": implementation["number"],
                        "parent": parent_number,
                        "blocked_by": sorted(blocked_by_numbers),
                    },
                )
            except HarnessError as exc:
                append_step(
                    report,
                    name="update and read issue relationships",
                    category="relationships",
                    status="failed",
                    evidence={"error": str(exc)},
                )
        else:
            append_step(
                report,
                name="update and read issue relationships",
                category="relationships",
                status="skipped",
                evidence={"reason": "relationship artifacts were not created"},
            )

        if epic is not None and implementation is not None and blocker is not None:
            try:
                list_payload = run_script(
                    "list_open_issues.py",
                    "--parent",
                    str(epic["number"]),
                    "--kind",
                    "implementation",
                    expect_json=True,
                )
                search_payload = run_script(
                    "search_issues.py",
                    "--keywords",
                    names.implementation_title.split(": ", maxsplit=1)[1],
                    "--description",
                    expect_json=True,
                )
                validation_graph = run_script("validate_graph.py", expect_json=True)
                filtered_graph = filter_validation_graph_findings(validation_graph, tracked_issues)
                if any(filtered_graph.values()):
                    raise HarnessError("Validation issue graph contains tracked broken references or cycles.")
                active_design = (
                    f"Active validation implementation issues: #{implementation['number']} and #{blocker['number']}"
                )
                run_script(
                    "update_issue.py",
                    "--issue",
                    str(epic["number"]),
                    "--design",
                    active_design,
                )
                epic_readback = run_script(
                    "read_issue.py",
                    "--issue",
                    str(epic["number"]),
                    "--all",
                    expect_json=True,
                )
                exact_match = find_exact_match(list(search_payload.get("matches") or []), names.implementation_title)
                if exact_match.get("number") != implementation["number"]:
                    raise HarnessError("Duplicate-check search did not return the expected implementation issue.")
                if active_design not in str(epic_readback.get("design") or ""):
                    raise HarnessError("Epic reassessment update did not persist the active issue summary.")
                append_step(
                    report,
                    name="run query commands and epic reassessment checks",
                    category="query",
                    status="passed",
                    evidence={
                        "open_implementation_issues": list_payload.get("issues_count"),
                        "search_matches": search_payload.get("matches_count"),
                        "tracked_graph_findings": filtered_graph,
                    },
                )
                append_step(
                    report,
                    name="update epic with active implementation issues",
                    category="assessment",
                    status="passed",
                    evidence={"epic": epic["number"], "design": active_design},
                )
            except HarnessError as exc:
                append_step(
                    report,
                    name="run query commands and epic reassessment checks",
                    category="query",
                    status="failed",
                    evidence={"error": str(exc)},
                )
        else:
            append_step(
                report,
                name="run query commands and epic reassessment checks",
                category="query",
                status="skipped",
                evidence={"reason": "validation issue graph was not created"},
            )

        if implementation is not None and blocker is not None:
            stash_path = REPO_ROOT / f".validation-stash-{names.slug}.txt"
            stash_message = (
                f"blocked-new: issue #{implementation['number']} blocked by #{blocker['number']}"
            )
            try:
                stash_path.write_text(f"{names.prefix}\n", encoding="utf-8")
                run_command(
                    [
                        "git",
                        "stash",
                        "push",
                        "--include-untracked",
                        "--message",
                        stash_message,
                        "--",
                        stash_path.name,
                    ]
                )
                stash_entries = run_command(["git", "stash", "list", "--format=%gd %s"]).splitlines()
                matching_entry = next((line for line in stash_entries if stash_message in line), "")
                if not matching_entry:
                    raise HarnessError("blocked-new stash validation did not create the expected stash entry.")
                stash_ref = matching_entry.split(" ", maxsplit=1)[0]
                run_command(["git", "stash", "drop", stash_ref])
                report["artifacts"]["blocked_new_stash_message"] = stash_message
                append_step(
                    report,
                    name="validate blocked-new stash handling",
                    category="blocked_new",
                    status="passed",
                    evidence={"stash_ref": stash_ref, "message": stash_message},
                )
            except HarnessError as exc:
                append_step(
                    report,
                    name="validate blocked-new stash handling",
                    category="blocked_new",
                    status="failed",
                    evidence={"error": str(exc)},
                )
            finally:
                if stash_path.exists():
                    stash_path.unlink()
        else:
            append_step(
                report,
                name="validate blocked-new stash handling",
                category="blocked_new",
                status="skipped",
                evidence={"reason": "blocked-new issue references were not created"},
            )

        if implementation is not None:
            try:
                ensure_clean_worktree()
                original_ref, _ = current_checkout_ref()
                restore_needed = True
                run_command(["git", "checkout", "-B", names.branch])
                run_command(["git", "commit", "--allow-empty", "-m", names.commit_message])
                run_command(["git", "push", "-u", "origin", names.branch])
                run_script(
                    "create_pr.py",
                    "--title",
                    names.pr_title,
                    "--description",
                    f"Live github-harness validation for {names.prefix}.",
                    "--closes",
                    str(implementation["number"]),
                )
                run_script(
                    "create_pr.py",
                    "--title",
                    names.pr_title,
                    "--description",
                    f"Live github-harness validation for {names.prefix}.",
                    "--closes",
                    str(implementation["number"]),
                )
                pull_requests = list_open_pull_requests(repo, names.branch)
                if len(pull_requests) != 1:
                    raise HarnessError("Validation branch does not have exactly one open pull request.")
                report["artifacts"]["pull_request"] = pull_requests[0]
                append_step(
                    report,
                    name="create or reuse pull request",
                    category="pull_request",
                    status="passed",
                    evidence={"branch": names.branch, "pull_request": pull_requests[0]["number"]},
                )
                append_step(
                    report,
                    name="rerun pull request creation idempotently",
                    category="idempotency",
                    status="passed",
                    evidence={"branch": names.branch, "open_pull_requests": len(pull_requests)},
                )
            except HarnessError as exc:
                append_step(
                    report,
                    name="create or reuse pull request",
                    category="pull_request",
                    status="failed",
                    evidence={"error": str(exc)},
                )
        else:
            append_step(
                report,
                name="create or reuse pull request",
                category="pull_request",
                status="skipped",
                evidence={"reason": "validation implementation issue was not created"},
            )

        artifact_titles = [
            str((report.get("artifacts") or {}).get("epic", {}).get("title") or ""),
            str((report.get("artifacts") or {}).get("implementation_issue", {}).get("title") or ""),
            str((report.get("artifacts") or {}).get("blocker_issue", {}).get("title") or ""),
        ]
        isolated = (
            all(artifact_titles)
            and all(title.startswith(("epic: validation ", "feat: validation ")) for title in artifact_titles)
            and names.slug in names.branch
        )
        append_step(
            report,
            name="verify validation artifact isolation",
            category="isolation",
            status="passed" if isolated else "failed",
            evidence={
                "branch": names.branch,
                "tracked_issues": sorted(tracked_issues),
            },
        )
    finally:
        if restore_needed and not args.keep_branch and original_ref is not None:
            try:
                restore_checkout(original_ref)
            except HarnessError as exc:
                append_step(
                    report,
                    name="restore original checkout",
                    category="isolation",
                    status="failed",
                    evidence={"error": str(exc), "target": original_ref},
                )

    report["scores"] = compute_scores(report["steps"])
    emit_json(report)


if __name__ == "__main__":
    run_cli(main)
