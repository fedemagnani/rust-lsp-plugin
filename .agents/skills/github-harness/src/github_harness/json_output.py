from __future__ import annotations

import json
import sys
from typing import Any

_SKIP = object()


def _prune(value: Any) -> Any:
    if value is None:
        return _SKIP
    if isinstance(value, str):
        return value if value.strip() else _SKIP
    if isinstance(value, dict):
        cleaned = {}
        for key, nested in value.items():
            pruned = _prune(nested)
            if pruned is not _SKIP:
                cleaned[key] = pruned
        return cleaned if cleaned else _SKIP
    if isinstance(value, (list, tuple, set)):
        cleaned = []
        for nested in value:
            pruned = _prune(nested)
            if pruned is not _SKIP:
                cleaned.append(pruned)
        return cleaned if cleaned else _SKIP
    return value


def _empty_collection(value: Any) -> list[Any] | None:
    if not isinstance(value, (list, tuple, set)):
        return None

    cleaned = []
    for nested in value:
        pruned = _prune(nested)
        if pruned is not _SKIP:
            cleaned.append(pruned)
    return cleaned


def prune_empty(value: Any, *, preserve_empty_keys: set[str] | None = None) -> Any:
    if isinstance(value, dict):
        preserved = set(preserve_empty_keys or ())
        cleaned = {}
        for key, nested in value.items():
            pruned = _prune(nested)
            if pruned is not _SKIP:
                cleaned[key] = pruned
                continue

            if key in preserved:
                empty_collection = _empty_collection(nested)
                if empty_collection is not None:
                    cleaned[key] = empty_collection

        return cleaned if cleaned else {}

    pruned = _prune(value)
    if pruned is _SKIP:
        if isinstance(value, dict):
            return {}
        if isinstance(value, list):
            return []
        return None
    return pruned


def emit_json(value: Any, *, preserve_empty_keys: set[str] | None = None) -> None:
    json.dump(prune_empty(value, preserve_empty_keys=preserve_empty_keys), sys.stdout, sort_keys=True)
    sys.stdout.write("\n")
