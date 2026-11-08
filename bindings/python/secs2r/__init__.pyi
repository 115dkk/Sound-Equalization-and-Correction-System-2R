from os import PathLike
from typing import Any, Iterator, Optional, Union

__version__: str

class SecsError(Exception):
    code: str
    message: str
    detail: Optional[str]
    params: dict[str, str]
    retryable: bool

class _Events(Iterator[dict[str, Any]]):
    """Event iterator shared by Engine and Client, with a 1024-event queue.

    A full queue drops new non-terminal events; dropped_events() counts them.
    next() blocks until an event arrives or the events(timeout=...) wait expires.
    A finite timeout raises TimeoutError and closes the iterator; None waits
    indefinitely. close() from another thread ends a blocked next(); closed
    reports whether the iterator has closed. Waiting checks Python signals
    every 100 ms. Close a client iterator to release its socket and reader thread.

    The local API server has a separate 256-event queue. Falling behind it
    produces a lagged event with the missed count, then a WebSocket close
    with code 1013.
    """
    def __iter__(self) -> _Events: ...
    def __next__(self) -> dict[str, Any]: ...
    def dropped_events(self) -> int: ...
    def close(self) -> None: ...
    @property
    def closed(self) -> bool: ...

class Engine:
    def __init__(
        self,
        app_settings_path: Optional[Union[str, PathLike[str]]] = ...,
    ) -> None: ...
    def call(self, request: dict[str, Any]) -> dict[str, Any]: ...
    def wait_job(self, job_id: int, timeout: float) -> dict[str, Any]: ...
    def events(self, timeout: Optional[float] = ...) -> _Events: ...

class Client:
    def __init__(
        self,
        port: int = ...,
        token: Optional[str] = ...,
        token_path: Optional[Union[str, PathLike[str]]] = ...,
    ) -> None: ...
    def call(self, request: dict[str, Any]) -> dict[str, Any]: ...
    def status(self) -> dict[str, Any]: ...
    def events(self, timeout: Optional[float] = ...) -> _Events: ...
