from __future__ import annotations

import subprocess
from collections.abc import Sequence

from .errors import HarnessError
from .troubleshooting import classify_command_not_found, classify_subprocess_failure


def run(command: Sequence[str]) -> str:
    try:
        completed = subprocess.run(
            list(command),
            check=False,
            capture_output=True,
            text=True,
        )
    except FileNotFoundError as exc:
        raise HarnessError(classify_command_not_found(command[0])) from exc

    if completed.returncode != 0:
        message = completed.stderr.strip() or completed.stdout.strip() or "Command failed."
        raise HarnessError(classify_subprocess_failure(command, message))

    return completed.stdout
