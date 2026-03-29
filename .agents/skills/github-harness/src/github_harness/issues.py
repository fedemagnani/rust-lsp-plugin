from __future__ import annotations

import re
from collections.abc import Iterable
from typing import Any

from .errors import HarnessError

_SECTION_KEYS = {
    "description": "description",
    "contract": "contract",
    "acceptance": "acceptance",
    "design notes": "design",
}
_SECTION_ORDER = ("description", "contract", "acceptance", "design")
_SECTION_HEADINGS = {
    "description": "Description",
    "contract": "Contract",
    "acceptance": "Acceptance",
    "design": "Design notes",
}
_DEFAULT_READ_SELECTORS = {
    "description",
    "contract",
    "acceptance",
    "design",
    "status",
    "priority",
    "size",
    "kind",
    "parent",
    "children",
    "blocked_by",
    "blocking",
}
_ALL_READ_SELECTORS = _DEFAULT_READ_SELECTORS | {"comments"}
_PROJECT_FIELD_KEYS = ("status", "priority", "size", "kind")
_TOKEN_PATTERN = re.compile(r"[a-z0-9]+")
_SPACE_PATTERN = re.compile(r"\s+")
_TITLE_PATTERN = re.compile(r"^[^:\s][^:]*:\s+\S(?:.*\S)?$")
_UNSET = object()


def parse_issue_sections(body: str) -> dict[str, str]:
    sections: dict[str, str] = {}
    current_key: str | None = None
    current_lines: list[str] = []

    def flush() -> None:
        nonlocal current_lines
        if current_key is None:
            current_lines = []
            return
        value = "\n".join(current_lines).strip()
        if value:
            sections[current_key] = value
        current_lines = []

    for line in body.splitlines():
        heading_match = re.match(r"^##\s+(.+?)\s*$", line)
        if heading_match:
            flush()
            heading = heading_match.group(1).strip().lower()
            current_key = _SECTION_KEYS.get(heading)
            continue
        if current_key is not None:
            current_lines.append(line)

    flush()
    return sections


def validate_issue_title(title: str, *, expected_type: str | None = None) -> str:
    cleaned = str(title).strip()
    if not _TITLE_PATTERN.match(cleaned):
        raise HarnessError("Issue title must use the format '<type>: <short title>'.")

    if expected_type is not None:
        actual_type = cleaned.split(":", maxsplit=1)[0].strip().lower()
        if actual_type != expected_type.strip().lower():
            raise HarnessError(f"Issue title must start with '{expected_type}:'.")

    return cleaned


def render_issue_body(
    *,
    description: str,
    contract: str,
    acceptance: str,
    design: str = "",
) -> str:
    sections = {
        "description": _require_section_text("Description", description),
        "contract": _require_section_text("Contract", contract),
        "acceptance": _require_section_text("Acceptance", acceptance),
        "design": _clean_section_text(design),
    }

    blocks: list[str] = []
    for key in _SECTION_ORDER:
        heading = _SECTION_HEADINGS[key]
        value = sections[key]
        blocks.append(f"## {heading}\n{value}" if value else f"## {heading}")

    return "\n\n".join(blocks).rstrip() + "\n"


def rewrite_issue_body(
    body: str,
    *,
    description: object = _UNSET,
    contract: object = _UNSET,
    acceptance: object = _UNSET,
    design: object = _UNSET,
) -> str:
    current = parse_issue_sections(body)
    next_sections = {
        "description": current.get("description", ""),
        "contract": current.get("contract", ""),
        "acceptance": current.get("acceptance", ""),
        "design": current.get("design", ""),
    }

    overrides = {
        "description": description,
        "contract": contract,
        "acceptance": acceptance,
        "design": design,
    }
    for key, value in overrides.items():
        if value is not _UNSET:
            next_sections[key] = _clean_section_text(value)

    return render_issue_body(
        description=next_sections["description"],
        contract=next_sections["contract"],
        acceptance=next_sections["acceptance"],
        design=next_sections["design"],
    )


def select_issue_read_fields(raw_selectors: set[str], *, include_all: bool = False) -> set[str]:
    if include_all:
        return set(_ALL_READ_SELECTORS)
    return raw_selectors or set(_DEFAULT_READ_SELECTORS)


def _build_issue_ref(node: dict[str, Any] | None) -> dict[str, Any] | None:
    if not node:
        return None
    return {
        "number": node.get("number"),
        "title": node.get("title"),
        "url": node.get("url"),
        "state": node.get("state"),
    }


def _build_issue_refs(nodes: Iterable[dict[str, Any]]) -> list[dict[str, Any]]:
    return [ref for ref in (_build_issue_ref(node) for node in nodes) if ref is not None]


def extract_project_field_values(issue: dict[str, Any]) -> dict[str, str]:
    number = issue.get("number")
    values: dict[str, str] = {}

    for key in _PROJECT_FIELD_KEYS:
        seen: list[str] = []
        for item in (issue.get("projectItems") or {}).get("nodes") or []:
            for raw_value in _iter_project_field_values(item, key):
                name = str(raw_value.get("name") or "").strip()
                if name and name not in seen:
                    seen.append(name)

        if len(seen) > 1:
            choices = ", ".join(sorted(seen))
            raise HarnessError(f"Issue #{number} has conflicting project field values for {key}: {choices}")
        if seen:
            values[key] = seen[0]

    return values


def _iter_project_field_values(item: dict[str, Any], key: str) -> Iterable[dict[str, Any]]:
    if key == "kind":
        for alias in ("kind", "kindLegacy"):
            raw_value = item.get(alias)
            if isinstance(raw_value, dict):
                yield raw_value
        return

    raw_value = item.get(key)
    if isinstance(raw_value, dict):
        yield raw_value


def build_comment_entries(comments: Iterable[dict[str, Any]]) -> list[dict[str, Any]]:
    entries: list[dict[str, Any]] = []
    for comment in comments:
        body = str(comment.get("bodyText") or "").strip()
        if not body:
            continue
        entries.append(
            {
                "author": ((comment.get("author") or {}).get("login")),
                "comment": body,
                "created_at": comment.get("createdAt"),
                "updated_at": comment.get("updatedAt"),
                "url": comment.get("url"),
            }
        )
    return entries


def build_issue_payload(
    issue: dict[str, Any],
    *,
    selectors: set[str],
    comments: list[dict[str, Any]] | None = None,
) -> dict[str, Any]:
    selected = select_issue_read_fields(selectors)
    sections = parse_issue_sections(str(issue.get("body") or ""))
    project_fields = extract_project_field_values(issue)
    parent = _build_issue_ref(issue.get("parent") or None)
    children = _build_issue_refs((issue.get("subIssues") or {}).get("nodes") or [])
    blocked_by = _build_issue_refs((issue.get("blockedBy") or {}).get("nodes") or [])
    blocking = _build_issue_refs((issue.get("blocking") or {}).get("nodes") or [])

    payload: dict[str, Any] = {
        "issue": issue.get("number"),
        "title": issue.get("title"),
        "url": issue.get("url"),
        "state": issue.get("state"),
    }

    for key in ("description", "contract", "acceptance", "design"):
        if key in selected:
            payload[key] = sections.get(key)

    for key in _PROJECT_FIELD_KEYS:
        if key in selected:
            payload[key] = project_fields.get(key)

    if "parent" in selected:
        payload["parent"] = parent

    if "children" in selected:
        payload["children"] = children
        payload["children_count"] = len(children)

    if "blocked_by" in selected:
        payload["blocked_by"] = blocked_by
        payload["blocked_by_count"] = len(blocked_by)
        payload["blocked_by_open_count"] = sum(1 for node in blocked_by if node.get("state") == "OPEN")

    if "blocking" in selected:
        payload["blocking"] = blocking
        payload["blocking_count"] = len(blocking)

    if "comments" in selected:
        issue_comments = comments or []
        payload["comments"] = issue_comments
        payload["comments_count"] = len(issue_comments)

    return payload


def build_issue_graph_payload(issue: dict[str, Any]) -> dict[str, Any]:
    return build_issue_payload(
        issue,
        selectors={"parent", "children", "blocked_by", "blocking"},
    )


def build_issue_summary(issue: dict[str, Any]) -> dict[str, Any]:
    project_fields = extract_project_field_values(issue)
    blocked_by = _build_issue_refs((issue.get("blockedBy") or {}).get("nodes") or [])
    children = _build_issue_refs((issue.get("subIssues") or {}).get("nodes") or [])

    return {
        "number": issue.get("number"),
        "title": issue.get("title"),
        "url": issue.get("url"),
        "state": issue.get("state"),
        "parent": _build_issue_ref(issue.get("parent") or None),
        "status": project_fields.get("status"),
        "priority": project_fields.get("priority"),
        "size": project_fields.get("size"),
        "kind": project_fields.get("kind"),
        "blocked_by_open_count": sum(1 for node in blocked_by if node.get("state") == "OPEN"),
        "children_count": len(children),
    }


def matches_requested_kind(issue: dict[str, Any], requested_kind: str | None) -> bool:
    if requested_kind is None:
        return True
    value = extract_project_field_values(issue).get("kind")
    return value is not None and _normalize_name(value) == _normalize_name(requested_kind)


def build_epic_entries(issues: list[dict[str, Any]]) -> list[dict[str, Any]]:
    issue_by_number = {issue["number"]: issue for issue in issues if issue.get("number") is not None}
    epics: list[dict[str, Any]] = []

    for issue in issues:
        if issue.get("state") != "OPEN":
            continue
        if _normalize_name(extract_project_field_values(issue).get("kind", "")) != "epic":
            continue

        child_entries: list[dict[str, Any]] = []
        for child in (issue.get("subIssues") or {}).get("nodes") or []:
            full_child = issue_by_number.get(child.get("number"))
            child_entries.append(build_issue_summary(full_child or child))

        epics.append(
            {
                "number": issue.get("number"),
                "title": issue.get("title"),
                "url": issue.get("url"),
                "state": issue.get("state"),
                "status": extract_project_field_values(issue).get("status"),
                "kind": extract_project_field_values(issue).get("kind"),
                "children": child_entries,
                "children_count": len(child_entries),
                "children_open_count": sum(1 for child in child_entries if child.get("state") == "OPEN"),
                "children_closed_count": sum(1 for child in child_entries if child.get("state") == "CLOSED"),
            }
        )

    epics.sort(key=lambda item: item["number"])
    return epics


def rank_issue_matches(
    issues: list[dict[str, Any]],
    *,
    keywords: str,
    include_description: bool,
) -> list[dict[str, Any]]:
    query_tokens = set(_tokenize(keywords))
    normalized_keywords = _normalize_text(keywords)
    matches: list[dict[str, Any]] = []

    for issue in issues:
        title = str(issue.get("title") or "")
        title_text = _normalize_text(title)
        body = str(issue.get("body") or "")
        sections = parse_issue_sections(body)
        description = _normalize_text(sections.get("description") or body) if include_description else ""
        title_tokens = set(_tokenize(title))
        description_tokens = set(_tokenize(sections.get("description") or body)) if include_description else set()
        matched_tokens = query_tokens & title_tokens
        if include_description:
            matched_tokens |= query_tokens & description_tokens
        has_phrase_match = bool(
            normalized_keywords
            and (normalized_keywords in title_text or (include_description and normalized_keywords in description))
        )

        if len(query_tokens) > 1 and not has_phrase_match and len(matched_tokens) < 2:
            continue

        score = 0
        if normalized_keywords and normalized_keywords in title_text:
            score += 8
        if include_description and normalized_keywords and normalized_keywords in description:
            score += 4
        score += 4 * len(query_tokens & title_tokens)
        if include_description:
            score += 2 * len(query_tokens & description_tokens)

        if score <= 0:
            continue

        match = build_issue_summary(issue)
        match["score"] = score
        matches.append(match)

    matches.sort(
        key=lambda item: (
            -int(item["score"]),
            0 if item.get("state") == "OPEN" else 1,
            int(item["number"]),
        )
    )
    return matches[:20]


def validate_issue_graph(issues: list[dict[str, Any]]) -> dict[str, Any]:
    issue_by_number = {issue["number"]: issue for issue in issues if issue.get("number") is not None}
    hierarchy_edges = {
        issue["number"]: [node.get("number") for node in (issue.get("subIssues") or {}).get("nodes") or [] if node.get("number")]
        for issue in issues
        if issue.get("number") is not None
    }
    dependency_edges = {
        issue["number"]: [node.get("number") for node in (issue.get("blockedBy") or {}).get("nodes") or [] if node.get("number")]
        for issue in issues
        if issue.get("number") is not None
    }

    broken_references: list[dict[str, Any]] = []
    broken_keys: set[tuple[int, str, int | None, str]] = set()
    orphaned_children: list[dict[str, Any]] = []
    orphan_keys: set[tuple[int, str]] = set()

    def add_broken(issue_number: int, relationship: str, target: int | None, detail: str) -> None:
        key = (issue_number, relationship, target, detail)
        if key in broken_keys:
            return
        broken_keys.add(key)
        broken_references.append(
            {
                "issue": issue_number,
                "relationship": relationship,
                "target": target,
                "detail": detail,
            }
        )

    def add_orphan(issue_number: int, title: str | None, detail: str) -> None:
        key = (issue_number, detail)
        if key in orphan_keys:
            return
        orphan_keys.add(key)
        orphaned_children.append(
            {
                "issue": issue_number,
                "title": title,
                "detail": detail,
            }
        )

    for issue in issues:
        number = issue.get("number")
        if number is None:
            continue

        parent_number = ((issue.get("parent") or {}).get("number")) or None
        if parent_number is not None:
            parent_issue = issue_by_number.get(parent_number)
            if parent_issue is None:
                add_broken(number, "parent", parent_number, "Parent issue could not be resolved.")
            else:
                parent_children = {
                    node.get("number")
                    for node in (parent_issue.get("subIssues") or {}).get("nodes") or []
                    if node.get("number") is not None
                }
                if number not in parent_children:
                    add_broken(number, "parent", parent_number, "Parent issue does not list this child.")

        for child in (issue.get("subIssues") or {}).get("nodes") or []:
            child_number = child.get("number")
            if child_number is None:
                continue
            child_issue = issue_by_number.get(child_number)
            if child_issue is None:
                add_broken(number, "children", child_number, "Child issue could not be resolved.")
                continue
            child_parent = ((child_issue.get("parent") or {}).get("number")) or None
            if child_parent != number:
                add_broken(number, "children", child_number, "Child issue points to a different parent.")

        for blocker in (issue.get("blockedBy") or {}).get("nodes") or []:
            blocker_number = blocker.get("number")
            if blocker_number is None:
                continue
            blocker_issue = issue_by_number.get(blocker_number)
            if blocker_issue is None:
                add_broken(number, "blocked_by", blocker_number, "Blocking issue could not be resolved.")
                continue
            blocker_targets = {
                node.get("number")
                for node in (blocker_issue.get("blocking") or {}).get("nodes") or []
                if node.get("number") is not None
            }
            if number not in blocker_targets:
                add_broken(number, "blocked_by", blocker_number, "Blocking issue is missing the reciprocal edge.")

        for blocked in (issue.get("blocking") or {}).get("nodes") or []:
            blocked_number = blocked.get("number")
            if blocked_number is None:
                continue
            blocked_issue = issue_by_number.get(blocked_number)
            if blocked_issue is None:
                add_broken(number, "blocking", blocked_number, "Blocked issue could not be resolved.")
                continue
            blocked_by_numbers = {
                node.get("number")
                for node in (blocked_issue.get("blockedBy") or {}).get("nodes") or []
                if node.get("number") is not None
            }
            if number not in blocked_by_numbers:
                add_broken(number, "blocking", blocked_number, "Blocked issue is missing the reciprocal edge.")

        kind = _normalize_name(extract_project_field_values(issue).get("kind", ""))
        if kind == "implementation":
            if parent_number is None:
                add_orphan(number, issue.get("title"), "Implementation issue has no parent epic.")
            else:
                parent_issue = issue_by_number.get(parent_number)
                if parent_issue is None:
                    continue
                parent_kind = _normalize_name(extract_project_field_values(parent_issue).get("kind", ""))
                if parent_kind != "epic":
                    add_orphan(number, issue.get("title"), "Implementation issue parent is not an epic.")

    hierarchy_cycles = _find_cycles(hierarchy_edges)
    dependency_cycles = _find_cycles(dependency_edges)

    broken_references.sort(key=lambda item: (int(item["issue"]), item["relationship"], int(item.get("target") or 0)))
    orphaned_children.sort(key=lambda item: int(item["issue"]))

    return {
        "valid": not hierarchy_cycles and not dependency_cycles and not broken_references and not orphaned_children,
        "issues_count": len(issue_by_number),
        "open_issues_count": sum(1 for issue in issues if issue.get("state") == "OPEN"),
        "hierarchy_cycles": hierarchy_cycles,
        "dependency_cycles": dependency_cycles,
        "broken_references": broken_references,
        "orphaned_children": orphaned_children,
    }


def _find_cycles(edges: dict[int, list[int]]) -> list[list[int]]:
    visited: set[int] = set()
    active: set[int] = set()
    path: list[int] = []
    cycles: list[list[int]] = []
    cycle_keys: set[tuple[int, ...]] = set()

    def visit(node: int) -> None:
        visited.add(node)
        active.add(node)
        path.append(node)

        for neighbor in edges.get(node, []):
            if neighbor not in edges:
                continue
            if neighbor in active:
                start = path.index(neighbor)
                cycle = path[start:] + [neighbor]
                canonical = _canonicalize_cycle(cycle)
                if canonical not in cycle_keys:
                    cycle_keys.add(canonical)
                    cycles.append(list(canonical))
                continue
            if neighbor not in visited:
                visit(neighbor)

        active.remove(node)
        path.pop()

    for node in sorted(edges):
        if node not in visited:
            visit(node)

    cycles.sort()
    return cycles


def _canonicalize_cycle(cycle: list[int]) -> tuple[int, ...]:
    if len(cycle) <= 1:
        return tuple(cycle)
    base = cycle[:-1]
    if not base:
        return tuple(cycle)
    rotations = [tuple(base[index:] + base[:index]) for index in range(len(base))]
    canonical_base = min(rotations)
    return canonical_base + (canonical_base[0],)


def _normalize_name(value: str) -> str:
    return re.sub(r"[-_\s]+", "-", value.strip().lower())


def _normalize_text(value: str) -> str:
    return _SPACE_PATTERN.sub(" ", value.strip().lower())


def _tokenize(value: str) -> list[str]:
    return [token for token in _TOKEN_PATTERN.findall(value.lower()) if len(token) > 1]


def _clean_section_text(value: object) -> str:
    return str(value).strip()


def _require_section_text(name: str, value: object) -> str:
    cleaned = _clean_section_text(value)
    if not cleaned:
        raise HarnessError(f"{name} section cannot be empty.")
    return cleaned
