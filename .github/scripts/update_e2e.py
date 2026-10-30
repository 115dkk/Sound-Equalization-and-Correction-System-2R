#!/usr/bin/env python3
"""Build the old update-test manifests or drive the signed Windows update scenario.

The ``run`` command is guarded for disposable GitHub-hosted Windows runners. It
installs software and a Windows service, so unit tests cover only its pure parts
and step orchestration.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
import urllib.request
from contextlib import contextmanager
from datetime import datetime, timezone
from pathlib import Path, PureWindowsPath

# ``python -I script.py`` omits even the script's directory from sys.path.
# Add this trusted repository directory explicitly so the shared E2E helpers
# remain importable without making downloaded artifact directories import roots.
SCRIPT_DIRECTORY = Path(__file__).resolve().parent
if str(SCRIPT_DIRECTORY) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIRECTORY))

from manage_e2e import (
    CheckError,
    WebDriver,
    command,
    desktop_screenshot,
    find_installed_app,
    kill_installed_process,
    parse_log,
    parse_status,
    powershell,
    require,
    runner_only,
    ui,
    wait_for,
)

ENDPOINT = "http://127.0.0.1:8765/latest.json"
BASE_URL = "http://127.0.0.1:8765/"
OLD_VERSION = "0.0.1"
PRODUCT = "SECS2R"
RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
SHELL_RUN_VALUE = "SECS2RHostShell"
MANIFESTS = (
    Path("apps/secs2r/Cargo.toml"),
    Path("apps/secs2r-host/Cargo.toml"),
    Path("apps/secs2r-host-shell/Cargo.toml"),
)
VERSION_LINE = re.compile(r"^version\.workspace = true(?=\r?$)", re.MULTILINE)
SEMVER = re.compile(
    r"^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)"
    r"(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)
STEPS = (
    "Serve the signed new build",
    "Confine child processes to the local endpoint",
    "Install the old build and service",
    "Download the update through app A",
    "Close app A and install build B",
    "Update the Windows service",
    "Show and dismiss the update notice in app B",
    "Update through the host shell alone",
    "Clean up",
)
STEP_METHODS = (
    "serve",
    "environment",
    "install_old",
    "download_in_app",
    "install_on_close",
    "service_update",
    "notice",
    "shell_update",
    "cleanup",
)
TEST_NOTE = "### Update end-to-end test\n\n- Installed from the runner-local signed update server.\n"


def validate_version(version: str) -> None:
    require(SEMVER.fullmatch(version) is not None, f"invalid semantic version: {version!r}")


def rewritten_manifest(data: bytes, version: str, path: Path) -> bytes:
    """Replace the package's workspace version line exactly once."""
    validate_version(version)
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError as error:
        raise CheckError(f"manifest is not UTF-8: {path}") from error
    matches = list(VERSION_LINE.finditer(text))
    require(len(matches) == 1,
            f"expected one 'version.workspace = true' line in {path}, found {len(matches)}")
    replacement = f'version = "{version}"'
    return VERSION_LINE.sub(replacement, text, count=1).encode("utf-8")


def set_versions(root: Path, version: str) -> list[Path]:
    """Rewrite all three app manifests only after every input validates."""
    root = Path(root)
    pending: list[tuple[Path, bytes]] = []
    for relative in MANIFESTS:
        path = root / relative
        try:
            data = path.read_bytes()
        except OSError as error:
            raise CheckError(f"could not read manifest {path}: {error}") from error
        pending.append((path, rewritten_manifest(data, version, path)))
    for path, data in pending:
        path.write_bytes(data)
    return [path for path, _data in pending]


def build_environment(base: dict[str, str] | None = None) -> dict[str, str]:
    """Return a child environment pinned to the runner-local update endpoint."""
    environment = dict(os.environ if base is None else base)
    environment["SECS2R_UPDATE_ENDPOINT"] = ENDPOINT
    return environment


@contextmanager
def applied_environment(environment: dict[str, str]):
    """Make imported process helpers inherit the same explicit environment."""
    previous = os.environ.copy()
    os.environ.clear()
    os.environ.update(environment)
    try:
        yield
    finally:
        os.environ.clear()
        os.environ.update(previous)


def uninstall_entry_version(reader) -> str:
    """Read the unique SECS 2R DisplayVersion from fakeable uninstall entries."""
    entries = unique_uninstall_entries(reader)
    require(len(entries) == 1, f"expected one SECS 2R uninstall entry, found {len(entries)}")
    version = entries[0].get("DisplayVersion")
    require(isinstance(version, str) and bool(version.strip()),
            f"SECS 2R uninstall entry has no DisplayVersion: {entries[0]!r}")
    return version.strip()


def unique_uninstall_entries(reader) -> list[dict[str, str]]:
    entries = [entry for entry in reader() if entry.get("DisplayName") == "SECS 2R"]
    return list({json.dumps(entry, sort_keys=True): entry for entry in entries}.values())


def detailed_uninstall_entries():
    """Yield update-relevant registry values without guessing an NSIS key name."""
    script = r"""
$rows = @()
$roots = @(
  'Registry::HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'Registry::HKEY_LOCAL_MACHINE\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'Registry::HKEY_LOCAL_MACHINE\Software\Wow6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
foreach ($root in $roots) {
  $rows += @(Get-ItemProperty -Path $root -ErrorAction SilentlyContinue |
    Where-Object { $_.DisplayName -eq 'SECS 2R' } |
    ForEach-Object {
      [ordered]@{
        DisplayName = [string]$_.DisplayName
        DisplayVersion = [string]$_.DisplayVersion
        InstallLocation = [string]$_.InstallLocation
        UninstallString = [string]$_.UninstallString
        QuietUninstallString = [string]$_.QuietUninstallString
      }
    })
}
ConvertTo-Json -InputObject @($rows) -Compress
"""
    raw = powershell(script)
    values = json.loads(raw or "[]")
    if isinstance(values, dict):
        values = [values]
    require(isinstance(values, list), "uninstall registry query did not return an array")
    for value in values:
        require(isinstance(value, dict), f"invalid uninstall registry entry: {value!r}")
        yield value


def installed_entry(reader=detailed_uninstall_entries) -> dict[str, str]:
    entries = unique_uninstall_entries(reader)
    require(len(entries) == 1, f"expected one SECS 2R uninstall entry, found {len(entries)}")
    return entries[0]


def installed_version_or_none(reader=detailed_uninstall_entries) -> str | None:
    entries = unique_uninstall_entries(reader)
    require(len(entries) <= 1, f"expected at most one SECS 2R uninstall entry, found {len(entries)}")
    if not entries:
        return None
    version = entries[0].get("DisplayVersion")
    require(isinstance(version, str) and bool(version.strip()),
            f"SECS 2R uninstall entry has no DisplayVersion: {entries[0]!r}")
    return version.strip()


def uninstaller_path(entry: dict[str, str]) -> Path:
    value = entry.get("UninstallString") or entry.get("QuietUninstallString") or ""
    match = re.match(r'^\s*"([^"\r\n]+\.exe)"(?:\s.*)?$', value, re.IGNORECASE)
    if match is None:
        match = re.match(r"^\s*(.+?\.exe)(?:\s.*)?$", value, re.IGNORECASE)
    require(match is not None, f"unexpected uninstall string: {value!r}")
    windows = PureWindowsPath(match.group(1))
    require(windows.is_absolute(), f"uninstaller is not absolute: {value!r}")
    return Path(str(windows))


def shell_autostart_value(executable: Path) -> str:
    """The Run value `secs_sys::host::shell_autostart_value` writes."""
    return f'"{executable}" --hidden'


def set_shell_logon_start(value: str) -> None:
    import winreg

    with winreg.CreateKeyEx(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_SET_VALUE) as key:
        winreg.SetValueEx(key, SHELL_RUN_VALUE, 0, winreg.REG_SZ, value)


def check_update_json(path: Path, previous: str, target: str) -> dict:
    """Require the persisted update notice to name both installed versions."""
    try:
        value = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, UnicodeError, ValueError) as error:
        raise CheckError(f"could not read update notice {path}: {error}") from error
    require(isinstance(value, dict), f"update notice is not an object: {value!r}")
    require(value.get("from") == previous and value.get("to") == target,
            f"update notice versions do not match {previous} -> {target}: {value!r}")
    return value


def render_report(records: list[dict], facts=()) -> str:
    def escaped(value):
        return str(value).replace("|", "\\|").replace("\r", "").replace("\n", "<br>")

    rows = [
        "## Automatic update end to end (Windows)",
        "",
        "| Step | Result | Evidence |",
        "| --- | --- | --- |",
    ]
    by_number = {record["number"]: record for record in records}
    for number, name in enumerate(STEPS, 1):
        record = by_number.get(number, {"result": "NOT RUN", "detail": ""})
        rows.append(
            f"| {number}. {name} | {record['result']} | {escaped(record['detail'])} |"
        )
    rows += ["", "### Run facts", *[f"- {escaped(fact)}" for fact in facts], ""]
    return "\n".join(rows)


def execute_steps(runner) -> None:
    """Run steps in contract order and always execute the cleanup step."""
    primary = None
    try:
        for number, method_name in enumerate(STEP_METHODS[:-1], 1):
            runner.run_step(number, getattr(runner, method_name))
    except Exception as error:  # preserve the first observed failure after cleanup
        primary = error
    cleanup_error = None
    try:
        runner.run_step(len(STEPS), runner.cleanup)
    except Exception as error:
        cleanup_error = error
    if primary is not None:
        raise primary
    if cleanup_error is not None:
        raise cleanup_error


class UpdateScenario:
    def __init__(self, args, environment: dict[str, str]):
        self.args = args
        self.environment_values = environment
        self.artifacts = args.artifacts
        self.driver = WebDriver()
        self.records: list[dict] = []
        self.facts = [
            f"Old build: {OLD_VERSION}; target build: {args.version}; asset name version: {args.asset_version}.",
            f"Every child process inherited SECS2R_UPDATE_ENDPOINT={ENDPOINT}.",
            "The updater signature remained enabled; latest.json used the new installer's .sig file.",
        ]
        self.server = None
        self.tauri = None
        self.shell = None
        self.handles = []
        self.server_dir = self.artifacts / "update-server"
        self.edge_binary = args.msedgedriver
        self.installed_folders: set[Path] = set()
        self.old_installer = args.old / f"{PRODUCT}-{OLD_VERSION}-windows-x64-setup.exe"
        self.new_installer = args.new / f"{PRODUCT}-{args.asset_version}-windows-x64-setup.exe"
        self.served_installer = self.server_dir / self.new_installer.name
        self.app_data = Path(os.environ["APPDATA"]) / "io.github.115dkk.secs2r"
        self.update_json = self.app_data / "update.json"
        self.host_log = Path(os.environ["ProgramData"]) / "SECS2R" / "host.log"

    def log(self, event: str, **values) -> None:
        with (self.artifacts / "steps.jsonl").open("a", encoding="utf-8") as stream:
            stream.write(json.dumps({"at": time.time(), "event": event, **values}) + "\n")

    def write_report(self) -> None:
        self.args.report.write_text(render_report(self.records, self.facts), encoding="utf-8")

    def run_step(self, number: int, operation) -> None:
        name = STEPS[number - 1]
        record = {"number": number, "result": "PASS", "detail": ""}
        self.log("start", number=number, name=name)
        try:
            detail = operation()
            record["detail"] = detail or "completed"
        except Exception as error:
            record.update(result="FAIL", detail=str(error))
            try:
                desktop_screenshot(self.artifacts / f"{number:02d}-failure.png")
            except Exception as screenshot_error:
                self.log("screenshot_failed", detail=str(screenshot_error))
            raise
        finally:
            self.records.append(record)
            self.log("finish", **record)
            self.write_report()

    def start_background(self, arguments, log_name: str, cwd: Path | None = None):
        output = (self.artifacts / log_name).open("wb")
        self.handles.append(output)
        process = subprocess.Popen(
            [str(argument) for argument in arguments],
            cwd=None if cwd is None else str(cwd),
            env=self.environment_values,
            stdout=output,
            stderr=subprocess.STDOUT,
        )
        return process

    def serve(self) -> str:
        require(self.old_installer.is_file(), f"old installer is missing: {self.old_installer}")
        require(self.new_installer.is_file(), f"new installer is missing: {self.new_installer}")
        new_signature = Path(f"{self.new_installer}.sig")
        require(new_signature.is_file(), f"new installer signature is missing: {new_signature}")
        self.server_dir.mkdir(parents=True, exist_ok=False)
        shutil.copy2(self.new_installer, self.served_installer)
        shutil.copy2(new_signature, Path(f"{self.served_installer}.sig"))
        notes = self.server_dir / "notes.md"
        notes.write_text(TEST_NOTE, encoding="utf-8")
        manifest_script = Path(__file__).with_name("updater_manifest.py")
        pub_date = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
        command([
            sys.executable,
            "-I",
            manifest_script,
            "latest",
            "--only",
            "windows",
            "--version",
            self.args.version,
            "--asset-version",
            self.args.asset_version,
            "--previous",
            OLD_VERSION,
            "--notes-file",
            notes,
            "--assets",
            self.server_dir,
            "--base-url",
            BASE_URL,
            "--pub-date",
            pub_date,
            "--out",
            self.server_dir / "latest.json",
        ])
        self.server = self.start_background(
            [sys.executable, "-I", "-m", "http.server", "8765", "--bind", "127.0.0.1"],
            "update-server.log",
            self.server_dir,
        )
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

        def manifest_ready():
            with opener.open(ENDPOINT, timeout=2) as response:
                require(response.geturl() == ENDPOINT, f"local update endpoint redirected to {response.geturl()}")
                value = json.load(response)
            return value

        manifest = wait_for(
            "runner-local update server",
            manifest_ready,
            lambda value: value.get("version") == self.args.version,
            timeout=20,
        )
        return f"127.0.0.1:8765 served latest.json for {manifest['version']} and {self.served_installer.name}.sig"

    def environment(self) -> str:
        require(os.environ.get("SECS2R_UPDATE_ENDPOINT") == ENDPOINT,
                "the process environment is not pinned to the runner-local endpoint")
        require(self.environment_values.get("SECS2R_UPDATE_ENDPOINT") == ENDPOINT,
                "the child environment is not pinned to the runner-local endpoint")
        return f"parent and explicit child environments use {ENDPOINT}"

    def host_status(self, host: Path) -> dict:
        return parse_status(command([host, "status"], timeout=10).stdout)

    def install_old(self) -> str:
        command([self.old_installer, "/S"], timeout=240)
        folder = find_installed_app()
        self.installed_folders.add(folder)
        version = uninstall_entry_version(detailed_uninstall_entries)
        require(version == OLD_VERSION, f"old installer registered DisplayVersion {version!r}")
        host = folder / "secs2r-host.exe"
        command([host, "install"], timeout=120)
        status = wait_for(
            "old service status",
            lambda: self.host_status(host),
            lambda value: value.get("version") == OLD_VERSION,
            timeout=60,
            interval=1,
        )
        # Turning 관리 on in the app also registers the shell's logon start
        # (apps/secs2r/src/apply.rs). The installer relaunches the shell after an
        # update only when that value exists, and the shell is what asks the
        # service to update itself; `secs2r-host install` alone leaves it unset.
        shell_value = shell_autostart_value(folder / "secs2r-host-shell.exe")
        set_shell_logon_start(shell_value)
        return (f"DisplayVersion={version}; service status version={status['version']}; "
                f"install={folder}; logon start {SHELL_RUN_VALUE}={shell_value}")

    def start_driver(self) -> None:
        require(self.args.tauri_driver.is_file(),
                f"tauri-driver is missing: {self.args.tauri_driver}")
        require(self.edge_binary.is_file(), f"Edge driver is missing: {self.edge_binary}")
        self.tauri = self.start_background(
            [self.args.tauri_driver, "--native-driver", self.edge_binary],
            "tauri-driver.log",
        )
        wait_for(
            "tauri-driver ready",
            lambda: self.driver.request("GET", "/status"),
            lambda value: value.get("ready") is True,
            timeout=30,
        )

    def visible(self, element: str) -> bool:
        return bool(self.driver.execute(
            "const e=arguments[0]; return e.getClientRects().length > 0 "
            "&& getComputedStyle(e).visibility === 'visible';",
            {"element-6066-11e4-a52e-4f735466cecf": element},
        ))

    def download_in_app(self) -> str:
        folder = find_installed_app()
        self.installed_folders.add(folder)
        self.start_driver()
        self.driver.start(folder / "secs2r.exe")

        def ready_badge():
            element = self.driver.find(ui("update.badge"))
            return {
                "element": element,
                "state": self.driver.attribute(element, "data-state"),
                "version": self.driver.attribute(element, "data-version"),
            }

        badge = wait_for(
            "app A ready update badge",
            ready_badge,
            lambda value: value["state"] == "ready" and value["version"] == self.args.version,
            timeout=180,
            interval=0.5,
        )
        require(self.visible(badge["element"]), "ready update badge is not visible")
        return f"{ui('update.badge')} reached data-state=ready and data-version={badge['version']}"

    def installer_processes(self) -> list[dict]:
        script = r"""
$rows = @(Get-CimInstance Win32_Process | Where-Object {
  $_.Name -like 'SECS2R-*-setup.exe' -or $_.ExecutablePath -like '*SECS2R-*-setup.exe'
} | ForEach-Object { [ordered]@{ pid=$_.ProcessId; name=$_.Name; path=$_.ExecutablePath } })
ConvertTo-Json -InputObject $rows -Compress
"""
        raw = powershell(script)
        value = json.loads(raw or "[]")
        if isinstance(value, dict):
            value = [value]
        require(isinstance(value, list), "installer process query did not return an array")
        return value

    def app_processes(self) -> list[dict]:
        """Running `secs2r.exe` processes: the app, not the service or the shell."""
        script = r"""
$rows = @(Get-CimInstance Win32_Process -Filter "Name='secs2r.exe'" | ForEach-Object {
  [ordered]@{ pid=$_.ProcessId; path=$_.ExecutablePath } })
ConvertTo-Json -InputObject $rows -Compress
"""
        value = json.loads(powershell(script) or "[]")
        if isinstance(value, dict):
            value = [value]
        require(isinstance(value, list), "app process query did not return an array")
        return value

    def close_app_window(self) -> None:
        """Send WM_CLOSE to the app's main window, as its title-bar close button does.

        Tauri then runs CloseRequested, the screen's close flow, Destroyed and
        RunEvent::Exit, where the app installs a ready update. WebDriver's Close
        Window acts on the WebView2 page instead; after it, CI run 41099199514
        saw no installation within 240 s. The WebDriver session ends with the app.
        """
        try:
            apps = self.app_processes()
            require(len(apps) == 1, f"expected one running secs2r.exe, found {apps}")
            closed = powershell(f"(Get-Process -Id {int(apps[0]['pid'])}).CloseMainWindow()")
            require(closed == "True", f"secs2r.exe PID {apps[0]['pid']} has no main window to close")
        finally:
            self.driver.session = None

    def install_on_close(self) -> str:
        self.close_app_window()

        def finished():
            version = installed_version_or_none()
            processes = self.installer_processes()
            notice = check_update_json(self.update_json, OLD_VERSION, self.args.version)
            return {"version": version, "installer_processes": processes, "notice": notice,
                    "app_processes": self.app_processes()}

        evidence = wait_for(
            "app close update installation",
            finished,
            lambda value: value["version"] == self.args.version and not value["installer_processes"],
            timeout=240,
            interval=1,
        )
        return (
            f"DisplayVersion={evidence['version']}; no installer process remained; "
            f"{self.update_json} records {OLD_VERSION} -> {self.args.version}"
        )

    def service_update(self) -> str:
        folder = find_installed_app()
        self.installed_folders.add(folder)
        host = folder / "secs2r-host.exe"

        def updated():
            status = self.host_status(host)
            lines = parse_log(self.host_log.read_text(encoding="utf-8"))
            staged = next((line for line in lines if line["event"] == "update_staged"
                           and line["fields"].get("version") == self.args.version), None)
            done = next((line for line in lines if line["event"] == "update_done"
                         and line["fields"].get("version") == self.args.version), None)
            return {"status": status, "staged": staged, "done": done}

        evidence = wait_for(
            "service self-update",
            updated,
            lambda value: value["status"].get("version") == self.args.version
            and value["staged"] is not None and value["done"] is not None,
            timeout=180,
            interval=1,
        )
        return (
            f"service status version={evidence['status']['version']}; "
            f"update_staged at {evidence['staged']['timestamp']}; "
            f"update_done at {evidence['done']['timestamp']}"
        )

    def notice(self) -> str:
        folder = find_installed_app()
        self.installed_folders.add(folder)
        self.driver.start(folder / "secs2r.exe")

        def shown_notice():
            element = self.driver.find(ui("update.notice"))
            text = self.driver.text(element)
            return {"element": element, "text": text, "visible": self.visible(element)}

        notice = wait_for(
            "app B update notice",
            shown_notice,
            lambda value: value["visible"] and self.args.version in value["text"],
            timeout=60,
        )
        dismiss = self.driver.find(ui("update.notice.dismiss"))
        self.driver.click(dismiss)

        def dismissed():
            try:
                element = self.driver.find(ui("update.notice"))
                hidden = not self.visible(element)
            except CheckError:
                hidden = True
            return hidden and not self.update_json.exists()

        wait_for("dismissed update notice", dismissed, timeout=30)
        self.close_app_window()
        # The shell checks for updates only while SECS 2R is not running.
        wait_for("app B exit", self.app_processes, lambda value: not value, timeout=60, interval=1)
        return (
            f"{ui('update.notice')} named {self.args.version}; "
            f"{ui('update.notice.dismiss')} hid it and removed {self.update_json}; "
            f"text={notice['text']!r}"
        )

    def silent_uninstall(self) -> Path:
        entry = installed_entry()
        executable = uninstaller_path(entry)
        command([executable, "/S"], timeout=240)
        wait_for(
            "SECS 2R uninstall entry removal",
            installed_version_or_none,
            lambda value: value is None,
            timeout=60,
            interval=1,
        )
        return executable

    def shell_update(self) -> str:
        uninstaller = self.silent_uninstall()
        command([self.old_installer, "/S"], timeout=240)
        folder = find_installed_app()
        self.installed_folders.add(folder)
        require(uninstall_entry_version(detailed_uninstall_entries) == OLD_VERSION,
                "the second old-build installation did not register version 0.0.1")
        shell = folder / "secs2r-host-shell.exe"
        self.shell = self.start_background([shell, "--hidden"], "host-shell-update.log")

        def updated():
            return {
                "version": installed_version_or_none(),
                "notice": check_update_json(self.update_json, OLD_VERSION, self.args.version),
            }

        evidence = wait_for(
            "host-shell update installation",
            updated,
            lambda value: value["version"] == self.args.version,
            timeout=300,
            interval=1,
        )
        return (
            f"uninstalled with {uninstaller} /S; shell PID {self.shell.pid} alone installed "
            f"DisplayVersion={evidence['version']} and wrote {self.update_json}"
        )

    def stop_tree(self, process) -> None:
        if process is None or process.poll() is not None:
            return
        command(["taskkill.exe", "/PID", str(process.pid), "/T", "/F"], timeout=30, check=False)
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=10)

    def cleanup(self) -> str:
        errors = []
        try:
            self.driver.close()
        except Exception as error:
            errors.append(f"WebDriver close: {error}")
        for folder in list(self.installed_folders):
            for name in ("secs2r.exe", "secs2r-host-shell.exe"):
                try:
                    kill_installed_process(folder / name)
                except Exception as error:
                    errors.append(f"stop {folder / name}: {error}")
        for label, process in (("shell", self.shell),):
            try:
                self.stop_tree(process)
            except Exception as error:
                errors.append(f"stop {label}: {error}")
        try:
            for process in self.installer_processes():
                command(["taskkill.exe", "/PID", str(process["pid"]), "/T", "/F"],
                        timeout=30, check=False)
        except Exception as error:
            errors.append(f"stop installer processes: {error}")
        try:
            if installed_version_or_none() is not None:
                self.silent_uninstall()
        except Exception as error:
            errors.append(f"uninstall: {error}")
        for label, process in (("tauri-driver", self.tauri), ("HTTP server", self.server)):
            try:
                self.stop_tree(process)
            except Exception as error:
                errors.append(f"stop {label}: {error}")
        try:
            if self.edge_binary.is_file():
                kill_installed_process(self.edge_binary)
        except Exception as error:
            errors.append(f"stop Edge driver: {error}")
        for handle in self.handles:
            try:
                handle.close()
            except OSError as error:
                errors.append(f"close log: {error}")
        require(not errors, "; ".join(errors))
        return "uninstalled SECS 2R and stopped the shell, tauri-driver, Edge driver, and local HTTP server"


def run(args) -> None:
    runner_only()
    validate_version(args.version)
    validate_version(args.asset_version)
    args.artifacts.mkdir(parents=True, exist_ok=True)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    environment = build_environment()
    with applied_environment(environment):
        scenario = UpdateScenario(args, environment)
        try:
            execute_steps(scenario)
        finally:
            scenario.write_report()
            summary = os.environ.get("GITHUB_STEP_SUMMARY")
            if summary:
                with open(summary, "a", encoding="utf-8") as stream:
                    stream.write(render_report(scenario.records, scenario.facts))


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    subparsers = result.add_subparsers(dest="command", required=True)
    version = subparsers.add_parser("set-version", help="set the app, shell and service package versions")
    version.add_argument("--version", required=True)
    version.set_defaults(function=lambda args: set_versions(Path(__file__).resolve().parents[2], args.version))

    scenario = subparsers.add_parser(
        "run",
        help="install old build A and observe its signed update to B; GitHub Windows runners only",
    )
    scenario.add_argument("--old", type=Path, required=True)
    scenario.add_argument("--new", type=Path, required=True)
    scenario.add_argument("--version", required=True)
    scenario.add_argument("--asset-version", required=True)
    scenario.add_argument("--tauri-driver", type=Path, required=True)
    scenario.add_argument("--msedgedriver", type=Path, required=True)
    scenario.add_argument("--report", type=Path, required=True)
    scenario.add_argument("--artifacts", type=Path, required=True)
    scenario.set_defaults(function=run)
    return result


def main(argv=None) -> int:
    args = parser().parse_args(argv)
    try:
        args.function(args)
    except (CheckError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
