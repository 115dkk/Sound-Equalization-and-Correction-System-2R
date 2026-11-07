"""Shared bounded HTTP and download helpers for trusted CI scripts."""
from __future__ import annotations

import hashlib
import json
import os
import shutil
import time
import urllib.request
from pathlib import Path
from typing import Callable


class FetchError(RuntimeError):
    """A bounded download failed validation or exhausted its retries."""


def strict_json(raw):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError("Duplicate JSON key")
            result[key] = value
        return result

    def invalid_constant(value):
        raise ValueError("Non-finite JSON constant")

    return json.loads(raw, object_pairs_hook=pairs, parse_constant=invalid_constant)


def header(headers, name):
    return headers.get(name) if headers is not None else None


def retry_delay(error, attempt, headers=None):
    headers = error.headers if headers is None else headers
    reset = header(headers, "x-ratelimit-reset")
    if reset is not None:
        try:
            remaining = float(reset) - time.time()
        except (TypeError, ValueError):
            remaining = 0
        if remaining > 120:
            raise error
        if remaining > 0 and header(headers, "x-ratelimit-remaining") == "0":
            return remaining + 1
    retry_after = header(headers, "Retry-After")
    if retry_after is not None:
        try:
            return min(60.0, max(0.0, float(retry_after)))
        except (TypeError, ValueError):
            pass
    return float(2 ** attempt)


def _sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def fetch_file(
    url: str,
    expected_sha256: str,
    target: Path,
    opener: Callable[..., object] = urllib.request.urlopen,
    attempts: int = 4,
    *,
    sleeper: Callable[[float], None] = time.sleep,
    error_type: type[Exception] = FetchError,
    timeout: float | None = None,
    remove_invalid_target: bool = False,
    temporary: Path | None = None,
    retry_errors: tuple[type[BaseException], ...] = (Exception,),
    checksum_message: str = "checksum mismatch: expected {expected} got {actual}",
    failure_message: str = "download failed after {attempts} attempts: {error}",
) -> bool:
    """Fetch a pinned file atomically and return whether it changed."""
    if attempts < 1:
        raise ValueError("attempts must be positive")
    target = Path(target)
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.is_file() and _sha256(target) == expected_sha256:
        return False
    if remove_invalid_target and target.exists():
        target.unlink()
    temporary = temporary or target.with_name(f".{target.name}.part")
    temporary.unlink(missing_ok=True)
    request = urllib.request.Request(url, headers={"User-Agent": "secs2r-ci"})
    last_error: BaseException | None = None
    for attempt in range(attempts):
        try:
            response = opener(request) if timeout is None else opener(request, timeout=timeout)
            with response, temporary.open("wb") as output:
                shutil.copyfileobj(response, output)
            actual = _sha256(temporary)
            if actual != expected_sha256:
                raise error_type(
                    checksum_message.format(expected=expected_sha256, actual=actual)
                )
            os.replace(temporary, target)
            return True
        except error_type:
            raise
        except retry_errors as error:
            last_error = error
            if attempt + 1 < attempts:
                sleeper(2 ** (attempt + 1))
        finally:
            temporary.unlink(missing_ok=True)
    raise error_type(
        failure_message.format(attempts=attempts, error=last_error)
    ) from last_error
