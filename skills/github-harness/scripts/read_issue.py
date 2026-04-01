#!/usr/bin/env python3
from __future__ import annotations

import argparse

import _bootstrap  # noqa: F401
from github_harness.cli import run_cli
from github_harness.git import get_repo_name_with_owner
from github_harness.github import get_issue_comments, get_issue_details
from github_harness.issues import build_comment_entries, build_issue_payload, select_issue_read_fields
from github_harness.json_output import emit_json

_COLLECTION_OUTPUT_KEYS = {"comments", "children", "blocked_by", "blocking"}


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Read selected issue sections, project fields, and relationships."
    )
    parser.add_argument("--issue", required=True, type=int, help="Target issue number.")
    parser.add_argument("--all", action="store_true", help="Include sections, fields, relationships, and comments.")
    parser.add_argument("--description", action="store_true", help="Include the Description section.")
    parser.add_argument("--contract", action="store_true", help="Include the Contract section.")
    parser.add_argument("--acceptance", action="store_true", help="Include the Acceptance section.")
    parser.add_argument("--design", action="store_true", help="Include the Design notes section.")
    parser.add_argument("--comments", action="store_true", help="Include issue comments.")
    parser.add_argument("--status", action="store_true", help="Include the Status project field.")
    parser.add_argument("--priority", action="store_true", help="Include the Priority project field.")
    parser.add_argument("--size", action="store_true", help="Include the Size project field.")
    parser.add_argument("--kind", action="store_true", help="Include the kind project field.")
    parser.add_argument("--parent", action="store_true", help="Include the parent issue.")
    parser.add_argument("--children", action="store_true", help="Include sub-issues.")
    parser.add_argument("--blocked-by", dest="blocked_by", action="store_true", help="Include blockers.")
    parser.add_argument("--blocking", action="store_true", help="Include issues blocked by the target.")
    return parser


def main() -> None:
    args = build_parser().parse_args()

    selectors = {
        key
        for key, enabled in {
            "description": args.description,
            "contract": args.contract,
            "acceptance": args.acceptance,
            "design": args.design,
            "comments": args.comments,
            "status": args.status,
            "priority": args.priority,
            "size": args.size,
            "kind": args.kind,
            "parent": args.parent,
            "children": args.children,
            "blocked_by": args.blocked_by,
            "blocking": args.blocking,
        }.items()
        if enabled
    }

    repo = get_repo_name_with_owner()
    effective_selectors = select_issue_read_fields(selectors, include_all=args.all)
    issue = get_issue_details(repo, args.issue)
    comments = None
    if "comments" in effective_selectors:
        comment_payload = get_issue_comments(repo, args.issue)
        comments = build_comment_entries(comment_payload.get("comments") or [])

    emit_json(
        build_issue_payload(issue, selectors=effective_selectors, comments=comments),
        preserve_empty_keys=effective_selectors & _COLLECTION_OUTPUT_KEYS,
    )


if __name__ == "__main__":
    run_cli(main)
