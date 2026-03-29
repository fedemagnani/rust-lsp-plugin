from __future__ import annotations

import re

from .errors import HarnessError
from .process import run
from .troubleshooting import unsupported_github_remote_message

_GITHUB_REMOTE_PATTERN = re.compile(
    r"""
    ^
    (?:
        git@github\.com:
      | https://github\.com/
      | ssh://git@github\.com/
    )
    (?P<owner>[^/]+)/(?P<repo>[^/]+?)(?:\.git)?$
    """,
    re.VERBOSE,
)


def get_current_branch() -> str:
    branch = run(["git", "branch", "--show-current"]).strip()
    if not branch:
        raise HarnessError("Current HEAD is detached; switch to a branch before creating a pull request.")
    return branch


def get_repo_name_with_owner() -> str:
    remote = run(["git", "remote", "get-url", "origin"]).strip()
    match = _GITHUB_REMOTE_PATTERN.match(remote)
    if not match:
        raise HarnessError(unsupported_github_remote_message(remote))
    owner = match.group("owner")
    repo = match.group("repo")
    return f"{owner}/{repo}"
