from os import PathLike
from typing import Any, Iterator, Optional, Union

__version__: str

class SecsError(Exception):
    code: str
    message: str
    detail: Optional[str]

class Engine:
    def __init__(
        self,
        app_settings_path: Optional[Union[str, PathLike[str]]] = ...,
    ) -> None: ...
    def call(self, request: dict[str, Any]) -> dict[str, Any]: ...
    def wait_job(self, job_id: int, timeout: float) -> dict[str, Any]: ...
    def events(self) -> Iterator[dict[str, Any]]: ...

class Client:
    def __init__(
        self,
        port: int = ...,
        token: Optional[str] = ...,
        token_path: Optional[Union[str, PathLike[str]]] = ...,
    ) -> None: ...
    def call(self, request: dict[str, Any]) -> dict[str, Any]: ...
    def status(self) -> dict[str, Any]: ...
    def events(self) -> Iterator[dict[str, Any]]: ...
