from __future__ import annotations

import sys
from collections.abc import Callable

from .errors import HarnessError

SUCCESS_SENTINEL = "Action succeeded"


def _run_or_exit(main: Callable[[], None]) -> None:
    try:
        main()
    except HarnessError as exc:
        print(str(exc), file=sys.stderr)
        raise SystemExit(1) from exc


def run_cli(main: Callable[[], None]) -> None:
    _run_or_exit(main)


def run_mutation_cli(main: Callable[[], None]) -> None:
    _run_or_exit(main)
    print(SUCCESS_SENTINEL)
