# P118 (Daybreak): the `secs2r` Python package (ADR 0019)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\pypi`
(branch `claude/pypi`, based on `claude/public-api`). Do not commit, do not
push, do not upload anything to PyPI. Work only in this worktree.

Read first: `CLAUDE.md`, `docs/adr/0019-python-package.md`,
`docs/adr/0016-public-rust-api-and-local-api.md`,
`docs/adr/0002-unsafe-isolation-and-policy-gates.md`, `unsafe-budget.toml`,
`crates/secs-policy/` (the gates), `crates/secs-api/src/lib.rs` (`Engine`,
`EngineBuilder`, `Client`, `Events`, `Secs`, `call_json`, `wait_job`,
`subscribe`, `status`), `.github/workflows/ci.yml`,
`.github/workflows/release.yml` (the `crates` job) and
`.github/scripts/release_gate.py` (how the API version is decided).

## First, settle one fact

Check whether the current PyO3 release compiles a `#[pymodule]` with
`#[pyclass]`/`#[pymethods]`/`#[pyfunction]` in a crate whose root has
`#![forbid(unsafe_code)]`. If it does, keep `forbid`, drop the exception
from ADR 0019 (edit the ADR's unsafe bullet and the matching result line)
and skip the policy-gate change below. If it does not, keep the ADR as it is
and quote the compiler error in the report.

## Build (fixed)

- Crate `bindings/python` named `secs-py`, `crate-type = ["cdylib"]`,
  `publish = false`, workspace member, `[lints] workspace = true` (adjust
  only what PyO3 forces, and say what). Depends on
  `sound-equalization-and-correction-system-rust-api` with features
  `engine` and `client`, `pyo3` with `abi3-py39` and `extension-module`,
  `serde_json`. No OS dependencies, no GPU.
- `bindings/python/pyproject.toml` for maturin: name `secs2r`, version
  taken from the crate (which follows the API version, 0.1.0), license
  `GPL-3.0-only`, `requires-python = ">=3.9"`, a README (`bindings/python/README.md`,
  English, short: what it is, install, the two examples below, licence),
  project URLs, classifiers. The Python package `secs2r` holds
  `__init__.py` (re-exports), `_secs2r` (the extension), `__init__.pyi`,
  `py.typed`.
- API:

  ```python
  class SecsError(Exception):
      code: str; message: str; detail: str | None
  class Engine:
      def __init__(self, app_settings_path: str | os.PathLike | None = None) -> None: ...
      def call(self, request: dict) -> dict: ...            # Request JSON → Response JSON
      def wait_job(self, job_id: int, timeout: float) -> dict: ...  # terminal JobEvent
      def events(self) -> Iterator[dict]: ...               # JobEvent stream
  class Client:
      def __init__(self, port: int = 4712, token: str | None = None,
                   token_path: str | os.PathLike | None = None) -> None: ...
      def call(self, request: dict) -> dict: ...
      def status(self) -> dict: ...
      def events(self) -> Iterator[dict]: ...
  ```

  Requests and responses cross as Python dicts through `serde_json` (via
  `json` round trip or `pythonize`-free manual conversion; pick one and say
  why). Release the GIL (`py.allow_threads`) around every engine and client
  call that can block. Map the Rust error type to `SecsError` with the IPC
  error's code and detail. `Client` with neither `token` nor `token_path`
  reads `Client::default_token_path()`.
- Policy gate (only if `forbid` does not work): `unsafe-budget.toml` lists
  `secs-py = 0`, and `secs-policy` accepts `#![deny(unsafe_code)]` with
  budget 0 for `secs-py` alone; add a test that another crate doing the
  same still fails.
- CI (`ci.yml`): a `python-wheels` job on ubuntu, windows and macos that
  builds the wheel with `PyO3/maturin-action` (pin the action by commit SHA
  like the other actions in the file), installs it into a fresh venv, and
  runs `bindings/python/tests/` with pytest. It runs on PRs when
  `bindings/python/**`, `crates/**`, `Cargo.lock` or the workflow changes
  (use the same change-detection style the file already uses), and uploads
  the wheels as artifacts named `secs2r-wheels-<os>`. macOS builds
  `universal2`; Linux builds manylinux x86_64. Add the job to whatever list
  of required checks the repo keeps in files (say where; do not touch GitHub
  settings).
- Release (`release.yml`): a `pypi` job after the `crates` job that builds
  the same wheels plus an sdist, then publishes with
  `pypa/gh-action-pypi-publish` (pinned by SHA) in environment `pypi` with
  `permissions: id-token: write`, `skip-existing: true`. It runs only when
  the release gate decided an API version (same condition the crates job
  uses). No token in the repository.
- Python tests (`bindings/python/tests/`): import, version equals the
  crate's, `Engine().call({"Health": null})` or the cheapest real request
  the service answers without a project (find it), an error maps to
  `SecsError` with a code, `Client` against a closed port raises
  `SecsError` quickly, and the oracle project round trip (open, calibrate,
  wait, results) if an oracle project path is available to tests the way
  `secs-api`'s own tests find one.
- Records: `features.toml` entry `api.python` (implemented, the pytest file
  and a Rust test if any), `docs/THIRD_PARTY.md` if PyO3 needs listing per
  the file's rules, `CHANGELOG.md` entry under the next version.

## Files

You may edit: `bindings/python/**` (new), workspace `Cargo.toml`,
`Cargo.lock`, `fuzz/Cargo.lock` if needed, `unsafe-budget.toml` and
`crates/secs-policy/**` (only for the exception), `.github/workflows/ci.yml`,
`.github/workflows/release.yml`, `.github/scripts/**` if the gate needs a
flag, `features.toml`, `docs/THIRD_PARTY.md`, `CHANGELOG.md`,
`docs/adr/0019-python-package.md` (only the unsafe bullet, per the first
step).

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test -p secs-policy --locked
cargo deny --all-features check
python -m pip install maturin pytest
cd bindings/python && maturin build --release && python -m pip install --force-reinstall target/wheels/*.whl  # or the workspace target dir maturin reports
python -m pytest bindings/python/tests -q
python3 -m unittest discover -s .github/scripts -p "test_*.py"
python3 .github/scripts/check_feature_tests.py
```

Use a virtual environment in your scratchpad for the Python steps; do not
install into the system Python.

## Report

The forbid finding, the Python API as built, the CI and release jobs (with
pinned SHAs), files changed, and the commands' last lines.
