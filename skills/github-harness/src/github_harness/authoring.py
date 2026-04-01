from __future__ import annotations

from .errors import HarnessError
from .github import (
    add_blocked_by,
    add_issue_to_project,
    add_sub_issue,
    create_issue,
    get_issue_node,
    get_repository_kind_field_context,
    update_project_item_single_select_value,
)
from .issues import render_issue_body, validate_issue_title


def parse_issue_number_list(raw: str | None, *, allow_many: bool = True) -> list[int]:
    if raw is None:
        return []

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


def create_managed_issue(
    repo_name_with_owner: str,
    *,
    kind: str,
    title: str,
    description: str,
    contract: str,
    acceptance: str,
    design: str = "",
    parent: list[int] | None = None,
    blocked_by: list[int] | None = None,
    blocking: list[int] | None = None,
    expected_title_type: str | None = None,
) -> dict:
    validated_title = validate_issue_title(title, expected_type=expected_title_type)
    body = render_issue_body(
        description=description,
        contract=contract,
        acceptance=acceptance,
        design=design,
    )
    context = get_repository_kind_field_context(repo_name_with_owner, kind)

    issue = create_issue(context["repository_id"], validated_title, body)
    item_id = add_issue_to_project(context["project_id"], issue["id"])
    update_project_item_single_select_value(
        context["project_id"],
        item_id,
        context["kind_field_id"],
        context["kind_option_id"],
    )
    link_issue_relationships(
        repo_name_with_owner,
        issue_id=issue["id"],
        parent=parent or [],
        blocked_by=blocked_by or [],
        blocking=blocking or [],
    )
    return issue


def link_issue_relationships(
    repo_name_with_owner: str,
    *,
    issue_id: str,
    parent: list[int],
    blocked_by: list[int],
    blocking: list[int],
) -> None:
    if parent:
        parent_node = get_issue_node(repo_name_with_owner, parent[0])
        add_sub_issue(parent_node["id"], issue_id, replace_parent=True)

    for number in blocked_by:
        blocker = get_issue_node(repo_name_with_owner, number)
        add_blocked_by(issue_id, blocker["id"])

    for number in blocking:
        blocked_issue = get_issue_node(repo_name_with_owner, number)
        add_blocked_by(blocked_issue["id"], issue_id)
