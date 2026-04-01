from __future__ import annotations

import re
from collections.abc import Sequence


_MISSING_SCOPES_PATTERNS = [
    re.compile(r"one of the following scopes:\s*\[(?P<scopes>[^\]]+)\]", re.IGNORECASE),
    re.compile(r"required scopes?:\s*\[(?P<scopes>[^\]]+)\]", re.IGNORECASE),
]


def _parse_scopes(raw_scopes: str) -> list[str]:
    scopes: list[str] = []
    for chunk in raw_scopes.split(","):
        scope = chunk.strip().strip("'").strip('"')
        if scope and scope not in scopes:
            scopes.append(scope)
    return scopes


def _extract_required_scopes(message: str) -> list[str]:
    scopes: list[str] = []
    for pattern in _MISSING_SCOPES_PATTERNS:
        for match in pattern.finditer(message):
            for scope in _parse_scopes(match.group("scopes")):
                if scope not in scopes:
                    scopes.append(scope)
    return scopes


def _build_scope_refresh_command(scopes: list[str]) -> str:
    requested = list(scopes)
    if any(scope in {"read:project", "project"} for scope in requested):
        for scope in ("read:project", "project"):
            if scope not in requested:
                requested.append(scope)
    return ",".join(requested)


def classify_command_not_found(executable: str) -> str:
    if executable == "gh":
        return "GitHub CLI is not installed. Install `gh` from https://cli.github.com/ and retry."
    if executable == "git":
        return "Git is not installed or not on PATH. Install Git and retry."
    return f"Command not found: {executable}"


def classify_git_failure(command: Sequence[str], message: str) -> str:
    lower = message.lower()
    if len(command) >= 3 and command[1:3] == ["remote", "get-url"] and "no such remote" in lower:
        return (
            "Git remote `origin` is missing. Run this command inside the GitHub clone or add an origin "
            "with `git remote add origin git@github.com:<owner>/<repo>.git`, then retry."
        )
    return message


def classify_gh_failure(message: str) -> str:
    lower = message.lower()

    required_scopes = _extract_required_scopes(message)
    if required_scopes:
        scope_args = _build_scope_refresh_command(required_scopes)
        scope_list = ", ".join(required_scopes)
        return (
            f"GitHub authentication is missing required scopes ({scope_list}). "
            f"Run `gh auth refresh --scopes {scope_args}` for the active account, then retry. "
            "If you are using `GH_TOKEN`, regenerate it with the same scopes instead."
        )

    auth_markers = (
        "gh auth login",
        "authentication required",
        "not logged into any github hosts",
        "currently not logged in",
        "invalid authentication token",
        "http 401",
    )
    if any(marker in lower for marker in auth_markers):
        return (
            "GitHub CLI is not authenticated. Run `gh auth login --web` or `gh auth refresh`, "
            "then retry."
        )

    return message


def classify_subprocess_failure(command: Sequence[str], message: str) -> str:
    if not command:
        return message

    executable = command[0]
    if executable == "gh":
        return classify_gh_failure(message)
    if executable == "git":
        return classify_git_failure(command, message)
    return message


def unsupported_github_remote_message(remote: str) -> str:
    return (
        "Git remote `origin` must point to GitHub using ssh or https. "
        f"Current value: {remote}. Update it with "
        "`git remote set-url origin git@github.com:<owner>/<repo>.git` and retry."
    )
