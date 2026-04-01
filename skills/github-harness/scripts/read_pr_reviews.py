#!/usr/bin/env python3
from __future__ import annotations

import argparse
from typing import Any

import _bootstrap  # noqa: F401
from github_harness.cli import run_cli
from github_harness.git import get_repo_name_with_owner
from github_harness.github import get_pull_request_reviews
from github_harness.json_output import emit_json


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Read review bodies and inline review comments.")
    parser.add_argument("--pr", required=True, type=int, help="Pull request number.")
    return parser


def build_review_entries(pull_request: dict[str, Any]) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    head_oid = pull_request.get("headRefOid")
    all_entries: list[dict[str, Any]] = []
    new_entries: list[dict[str, Any]] = []

    for review in (pull_request.get("reviews") or {}).get("nodes") or []:
        review_id = review.get("fullDatabaseId") or review.get("id")
        review_commit_oid = ((review.get("commit") or {}).get("oid")) or None

        body = review.get("bodyText") or ""
        if body.strip():
            entry = {
                "review_id": review_id,
                "author": ((review.get("author") or {}).get("login")),
                "state": review.get("state"),
                "comment": body,
                "submitted_at": review.get("submittedAt"),
                "commit_oid": review_commit_oid,
            }
            all_entries.append(entry)
            if head_oid and review_commit_oid == head_oid:
                new_entries.append(entry)

        for comment in (review.get("comments") or {}).get("nodes") or []:
            body = comment.get("bodyText") or ""
            if not body.strip():
                continue

            comment_commit_oid = ((comment.get("commit") or {}).get("oid")) or review_commit_oid
            entry = {
                "review_id": review_id,
                "review_comment_id": comment.get("fullDatabaseId") or comment.get("id"),
                "author": ((comment.get("author") or {}).get("login")) or ((review.get("author") or {}).get("login")),
                "state": review.get("state"),
                "comment": body,
                "path": comment.get("path"),
                "line": comment.get("line") or comment.get("originalLine"),
                "published_at": comment.get("publishedAt"),
                "outdated": comment.get("outdated"),
                "commit_oid": comment_commit_oid,
            }
            all_entries.append(entry)
            if head_oid and comment_commit_oid == head_oid:
                new_entries.append(entry)

    return all_entries, new_entries


def main() -> None:
    args = build_parser().parse_args()

    repo = get_repo_name_with_owner()
    pull_request = get_pull_request_reviews(repo, args.pr)
    reviews, reviews_new = build_review_entries(pull_request)

    emit_json(
        {
            "pr": pull_request.get("number"),
            "url": pull_request.get("url"),
            "title": pull_request.get("title"),
            "head_ref": pull_request.get("headRefName"),
            "head_oid": pull_request.get("headRefOid"),
            "reviews": reviews,
            "reviews_new": reviews_new,
            "reviews_new_count": len(reviews_new),
        }
    )


if __name__ == "__main__":
    run_cli(main)
