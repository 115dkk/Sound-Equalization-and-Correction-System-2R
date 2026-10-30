#!/usr/bin/env python3
"""Drive the installed Windows app and host shell on a disposable GitHub runner.

Only project/measurement/calibration setup invokes secs_call directly. All managed
operations use WebDriver clicks and keystrokes and read stable DOM attributes.
No service paths or application responses are overridden.
Protocol references: https://www.w3.org/TR/webdriver2/ and
https://v2.tauri.app/develop/tests/webdriver/manual-setup/ .
"""
from __future__ import annotations

import argparse
import base64
import ctypes
import hashlib
import json
import os
import re
import runpy
import shutil
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile
from contextlib import contextmanager
from pathlib import Path, PureWindowsPath

ELEMENT = "element-6066-11e4-a52e-4f735466cecf"
MARKER = b"# SECS2R: managed by SECS 2R (this line and the next)"
INCLUDE = b"Include: SECS2R\\SECS.txt"
RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"
WEBVIEW_KEY = r"SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
STEPS = (
    "Base CamillaDSP", "App and design", "Settings through the GUI",
    "Service install through the GUI", "Manage CamillaDSP (takeover)",
    "Restart", "Manage Equalizer APO and repair", "Shell and switch off",
    "Manage with nothing running", "Service removal through the shell",
)


class CheckError(RuntimeError):
    """An observed result failed the scenario's contract."""


class WebDriverError(CheckError):
    def __init__(self, code, message):
        super().__init__(f"{code}: {message}")
        self.code = code


def require(condition, message):
    if not condition:
        raise CheckError(message)


def wait_for(label, observe, accept=bool, timeout=60, interval=0.25,
             clock=time.monotonic, sleep=time.sleep):
    """Bound every poll and retain the last value, including transient errors."""
    deadline = clock() + timeout
    last = None
    while True:
        try:
            last = observe()
            if accept(last):
                return last
        except (OSError, CheckError, ValueError, subprocess.SubprocessError) as error:
            last = str(error)
        remaining = deadline - clock()
        if remaining <= 0:
            raise CheckError(f"{label}: deadline after {timeout}s; last observed: {last!r}")
        sleep(min(interval, remaining))


def ui(name):
    return f'[data-ui="{name}"]'


# Creating a session launches the app and its WebView2; on a fresh runner that
# took longer than the 30 s of other commands (2026-10-30).
SESSION_TIMEOUT = 180


class WebDriver:
    def __init__(self, url="http://127.0.0.1:4444", timeout=30):
        self.url = url.rstrip("/")
        self.timeout = timeout
        self.session = None
        # Local driver traffic must not go through a runner's HTTP proxy.
        self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def request(self, method, path, body=None, timeout=None):
        data = None if body is None else json.dumps(body).encode("utf-8")
        request = urllib.request.Request(self.url + path, data=data, method=method,
                                         headers={"Content-Type": "application/json"})
        failed = False
        try:
            response = self.opener.open(request, timeout=timeout or self.timeout)
        except urllib.error.HTTPError as error:
            response = error
            failed = True
        with response:
            raw = response.read()
        try:
            envelope = json.loads(raw)
            value = envelope["value"]
        except (ValueError, KeyError, TypeError) as error:
            raise WebDriverError("invalid response", repr(raw[:1000])) from error
        if failed or (isinstance(value, dict) and "error" in value):
            if not isinstance(value, dict):
                raise WebDriverError("HTTP error", repr(value))
            raise WebDriverError(value.get("error", "HTTP error"), value.get("message", ""))
        return value

    def command(self, method, path, body=None):
        require(self.session is not None, "WebDriver session is not open")
        return self.request(method, f"/session/{self.session}{path}", body)

    def start(self, application, args=()):
        result = self.request("POST", "/session", {"capabilities": {"alwaysMatch": {
            "tauri:options": {"application": str(application), "args": list(args),
                              "webviewOptions": {}},
        }}}, timeout=SESSION_TIMEOUT)
        self.session = result["sessionId"]
        self.command("POST", "/timeouts", {"implicit": 0, "pageLoad": 30000, "script": 10000})
        return result

    def close(self):
        if self.session is not None:
            try:
                self.command("DELETE", "")
            finally:
                self.session = None

    def find(self, selector):
        return self.command("POST", "/element", {"using": "css selector", "value": selector})[ELEMENT]

    def find_all(self, selector):
        return [entry[ELEMENT] for entry in self.command(
            "POST", "/elements", {"using": "css selector", "value": selector})]

    def element(self, method, element, suffix, body=None):
        return self.command(method, f"/element/{urllib.parse.quote(element, safe='')}/{suffix}", body)

    def click(self, element):
        self.element("POST", element, "click", {})

    def clear(self, element):
        self.element("POST", element, "clear", {})

    def keys(self, element, text):
        self.element("POST", element, "value", {"text": text})

    def text(self, element):
        return self.element("GET", element, "text")

    def attribute(self, element, name):
        return self.element("GET", element, "attribute/" + urllib.parse.quote(name, safe=""))

    def execute(self, script, *args):
        return self.command("POST", "/execute/sync", {"script": script, "args": list(args)})

    def screenshot(self, path):
        raw = base64.b64decode(self.command("GET", "/screenshot"), validate=True)
        require(raw.startswith(b"\x89PNG\r\n\x1a\n"), "WebDriver screenshot is not PNG")
        path.write_bytes(raw)


def parse_status(text):
    status = json.loads(text)
    require(isinstance(status, dict), "host status is not an object")
    require(isinstance(status.get("managed"), dict), "host status has no managed targets")
    process = status.get("camilla", {})
    state = process.get("state", {}).get("kind")
    require(state in {"running", "backing_off", "given_up", "stopped", "no_session", "other_user"},
            f"unknown managed process state: {state!r}")
    require(isinstance(process.get("restarts_in_window"), int), "missing restart count")
    if state == "running":
        require(type(process.get("pid")) is int and process["pid"] > 0, "running without a PID")
    require(status.get("eqapo_include") in {"present", "absent", "damaged"}, "unknown include state")
    return status


def parse_log(text):
    lines = []
    for index, line in enumerate(text.splitlines(), 1):
        if not line.strip():
            continue
        value = json.loads(line)
        require(isinstance(value, dict) and isinstance(value.get("timestamp"), str)
                and isinstance(value.get("event"), str) and isinstance(value.get("fields"), dict),
                f"invalid host.log line {index}")
        lines.append(value)
    return lines


def remove_managed_lines(data):
    """Remove only a complete marker pair; preserve all other bytes and newlines.

    An unterminated include ends the file: SECS2R ended the original's last
    line for the pair, so that line ending goes with it.
    """
    lines = data.splitlines(keepends=True)
    result = []
    index = 0
    pairs = 0
    unterminated = False
    while index < len(lines):
        if lines[index].rstrip(b"\r\n") == MARKER:
            require(index + 1 < len(lines) and lines[index + 1].rstrip(b"\r\n") == INCLUDE,
                    "damaged SECS2R marker pair")
            unterminated = index + 2 == len(lines) and lines[index + 1] == INCLUDE
            index += 2
            pairs += 1
        else:
            result.append(lines[index])
            index += 1
    require(pairs == 1, f"expected one SECS2R marker pair, found {pairs}")
    text = b"".join(result)
    if unterminated and result:
        text = text[:-2] if text.endswith(b"\r\n") else text[:-1] if text.endswith(b"\n") else text
    return text


def listener_pid(listing, port):
    """The PID listening on 127.0.0.1 or 0.0.0.0 `port` in `netstat -ano -p TCP`.

    A connect probe cannot tell: Windows retries a refused loopback SYN for
    about two seconds, so a free port times out a short connect.
    """
    wanted = {f"127.0.0.1:{port}", f"0.0.0.0:{port}"}
    for line in listing.splitlines():
        fields = line.split()
        if (len(fields) >= 4 and fields[0].upper() == "TCP" and fields[1] in wanted
                and fields[2] == "0.0.0.0:0" and fields[-1].isdigit()):
            return int(fields[-1])
    return None


def check_config_bytes(before, after):
    require(remove_managed_lines(after) == before,
            "Equalizer APO changed bytes outside SECS2R's two lines")


VIRTUAL_OUTPUT = "CABLE Input (VB-Audio Virtual Cable)"



def base_config():
    # A plain WASAPI pass-through on the runner's VB-CABLE (format F32, as the
    # app exports it): it reaches Running and exits on the takeover's Exit.
    # With SignalGenerator capture CamillaDSP 4.1.3 never reports Running and
    # stayed alive after Exit, so the service saw the port busy (2026-10-30).
    # Capture is the loopback of CABLE Input, as in the managed configs: the
    # CABLE Output recording device is denied by Windows' microphone privacy
    # for desktop apps on the runner (0x80070005). The cable is silent, so
    # playing its own loopback back does not build up.
    # This config is NEVER substituted for app output.
    return f"""devices:
  samplerate: 48000
  chunksize: 1024
  queuelimit: 2
  capture:
    type: Wasapi
    channels: 2
    device: "{VIRTUAL_OUTPUT}"
    format: F32
    exclusive: false
    loopback: true
  playback:
    type: Wasapi
    channels: 2
    device: "{VIRTUAL_OUTPUT}"
    format: F32
    exclusive: false
filters: {{}}
mixers: {{}}
pipeline: []
"""


def version_parts(version):
    require(isinstance(version, str) and re.fullmatch(r"\d+\.\d+\.\d+\.\d+", version) is not None,
            f"invalid four-part version: {version!r}")
    return tuple(map(int, version.split(".")))


def edge_download(webview_version, driver_version):
    require(version_parts(webview_version)[:3] == version_parts(driver_version)[:3],
            f"WebView2 {webview_version} and msedgedriver {driver_version} builds differ")
    return f"https://msedgedriver.microsoft.com/{driver_version}/edgedriver_win64.zip"


def service_binary_path(command_line):
    match = re.fullmatch(r'"([^"\r\n]+)"\s+(?:run|"run")', command_line.strip())
    require(match is not None, f"unexpected service command line: {command_line!r}")
    return PureWindowsPath(match[1])


def microsoft_signature(signature):
    return signature.get("status") == "Valid" and re.search(
        r"(?:^|,\s*)O=Microsoft Corporation(?:,|$)", signature.get("subject") or "") is not None


def installed_folder(reader):
    """Reader yields uninstall entries (fakeable; no guessed installation path)."""
    entries = [entry for entry in reader() if entry.get("DisplayName") == "SECS 2R"]
    require(len(entries) == 1, f"expected one SECS 2R uninstall entry, found {len(entries)}")
    entry = entries[0]
    location = entry.get("InstallLocation", "").strip().strip('"')
    if not location:
        # Tauri NSIS stores the directory as the key's default value as well.
        location = entry.get("", "").strip().strip('"')
    require(bool(location) and PureWindowsPath(location).is_absolute(),
            f"uninstall entry has no absolute installation directory: {entry!r}")
    return PureWindowsPath(location)


def uninstall_entries():
    import winreg
    root = r"Software\Microsoft\Windows\CurrentVersion\Uninstall"
    for hive in (winreg.HKEY_CURRENT_USER, winreg.HKEY_LOCAL_MACHINE):
        for view in (winreg.KEY_WOW64_64KEY, winreg.KEY_WOW64_32KEY):
            try:
                key = winreg.OpenKey(hive, root, 0, winreg.KEY_READ | view)
            except FileNotFoundError:
                continue
            with key:
                for index in range(winreg.QueryInfoKey(key)[0]):
                    with winreg.OpenKey(key, winreg.EnumKey(key, index)) as child:
                        entry = {}
                        for name in ("DisplayName", "InstallLocation", ""):
                            try:
                                entry[name] = winreg.QueryValueEx(child, name)[0]
                            except FileNotFoundError:
                                pass
                        yield entry


def find_installed_app():
    # HKCU may be shared between WOW64 views: deduplicate identical entries.
    entries = list({json.dumps(e, sort_keys=True): e for e in uninstall_entries()}.values())
    folder = Path(str(installed_folder(lambda: entries)))
    for filename in ("secs2r.exe", "secs2r-cli.exe", "secs2r-host.exe", "secs2r-host-shell.exe"):
        require((folder / filename).is_file(), f"installed companion is missing: {folder / filename}")
    return folder


def render_report(records, notes=()):
    def escaped(value):
        return str(value).replace("|", "\\|").replace("\r", "").replace("\n", "<br>")
    rows = ["## Manage end to end (Windows GUI)", "", "| Step | Result | Evidence |",
            "| --- | --- | --- |"]
    by_number = {record["number"]: record for record in records}
    for number, name in enumerate(STEPS, 1):
        record = by_number.get(number, {"result": "NOT RUN", "detail": ""})
        rows.append(f"| {number}. {name} | {record['result']} | {escaped(record['detail'])} |")
    rows += ["", "### Run facts", *[f"- {escaped(note)}" for note in notes], ""]
    return "\n".join(rows)


def runner_only():
    require(sys.platform == "win32" and os.environ.get("GITHUB_ACTIONS") == "true"
            and os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted",
            "This command runs only on a disposable GitHub-hosted Windows runner")
    for variable in ("SECS2R_HOST_STATE", "SECS2R_HOST_LOG", "SECS2R_HOST_PIPE", "SECS2R_SELF_TEST"):
        require(not os.environ.get(variable), f"real installed-service check refuses {variable}")


def command(args, timeout=30, check=True, env=None):
    result = subprocess.run([str(arg) for arg in args], capture_output=True, text=True,
                            encoding="utf-8", errors="replace", timeout=timeout, check=False, env=env)
    if check:
        require(result.returncode == 0,
                f"{args!r} exited {result.returncode}: {result.stdout}\n{result.stderr}")
    return result


def windows_powershell_env(environ=None):
    """The environment for Windows PowerShell 5.1 started from a PowerShell 7
    step: without PowerShell 7's PSModulePath, which keeps 5.1 from autoloading
    its own modules (Microsoft.PowerShell.Security holds Get-AuthenticodeSignature)."""
    source = os.environ if environ is None else environ
    return {key: value for key, value in source.items() if key.upper() != "PSMODULEPATH"}


def powershell(script, timeout=30):
    preamble = "$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new();\n"
    encoded = base64.b64encode((preamble + script).encode("utf-16le")).decode("ascii")
    return command(["powershell.exe", "-NoProfile", "-NonInteractive", "-EncodedCommand", encoded],
                   timeout=timeout, env=windows_powershell_env()).stdout.strip()


def ps_string(value):
    return "'" + str(value).replace("'", "''") + "'"


def registry_value(hive, key, name, view=None):
    import winreg
    try:
        with winreg.OpenKey(hive, key, 0, winreg.KEY_READ | (view or winreg.KEY_WOW64_64KEY)) as handle:
            return winreg.QueryValueEx(handle, name)[0]
    except FileNotFoundError:
        return None


def webview_version():
    import winreg
    for hive in (winreg.HKEY_CURRENT_USER, winreg.HKEY_LOCAL_MACHINE):
        for view in (winreg.KEY_WOW64_32KEY, winreg.KEY_WOW64_64KEY):
            value = registry_value(hive, WEBVIEW_KEY, "pv", view)
            if value and value != "0.0.0.0":
                version_parts(value)
                return value
    raise CheckError("WebView2 runtime is not registered")


def environment_facts():
    import winreg
    from ctypes import wintypes
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    wts = ctypes.WinDLL("wtsapi32", use_last_error=True)
    kernel.WTSGetActiveConsoleSessionId.restype = wintypes.DWORD
    kernel.ProcessIdToSessionId.argtypes = [wintypes.DWORD, ctypes.POINTER(wintypes.DWORD)]
    session = wintypes.DWORD()
    require(kernel.ProcessIdToSessionId(os.getpid(), ctypes.byref(session)), "ProcessIdToSessionId failed")
    active = kernel.WTSGetActiveConsoleSessionId()
    wts.WTSQuerySessionInformationW.argtypes = [wintypes.HANDLE, wintypes.DWORD, ctypes.c_int,
                                               ctypes.POINTER(ctypes.c_void_p), ctypes.POINTER(wintypes.DWORD)]
    wts.WTSFreeMemory.argtypes = [ctypes.c_void_p]
    pointer, size = ctypes.c_void_p(), wintypes.DWORD()
    user = ""
    if active != 0xFFFFFFFF and wts.WTSQuerySessionInformationW(None, active, 5, ctypes.byref(pointer), ctypes.byref(size)):
        try:
            user = ctypes.wstring_at(pointer)
        finally:
            wts.WTSFreeMemory(pointer)
    return {
        "process_session": session.value, "active_console_session": active,
        "console_user": user,
        "query_user": command(["query.exe", "user"], check=False).stdout.strip(),
        "elevated": bool(ctypes.windll.shell32.IsUserAnAdmin()),
        "EnableLUA": registry_value(winreg.HKEY_LOCAL_MACHINE,
                                    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System", "EnableLUA"),
        "webview2": webview_version(),
        "runner_user_sid": powershell("[Security.Principal.WindowsIdentity]::GetCurrent().User.Value"),
        "wasapi": runpy.run_path(str(Path(__file__).with_name("virtual_audio.py")))["endpoints"](),
    }


def validate_environment(facts):
    require(bool(facts["console_user"]), "There is no user in the active console session.")
    require(facts["process_session"] == facts["active_console_session"],
            "The runner process is not in the active console session; GUI mutations would be rejected")
    require(facts["elevated"], "The runner is not elevated; unattended UAC cannot be automated by WebDriver")
    require(bool(facts["wasapi"]["render"]), "There is no active WASAPI render endpoint.")


def env(args):
    runner_only()
    args.artifacts.mkdir(parents=True, exist_ok=True)
    facts = environment_facts()
    (args.artifacts / "environment.json").write_text(json.dumps(facts, indent=2), encoding="utf-8")
    report = "## Manage GUI environment\n\n```json\n" + json.dumps(facts, indent=2) + "\n```\n"
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(report, encoding="utf-8")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as stream:
            stream.write(report)
    try:
        validate_environment(facts)
    except CheckError as error:
        with args.report.open("a", encoding="utf-8") as stream:
            stream.write(f"\nFAILED: {error}\n")
        if os.environ.get("GITHUB_STEP_SUMMARY"):
            with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as stream:
                stream.write(f"\nFAILED: {error}\n")
        raise


def install_app(installers):
    runner_only()
    choices = list(Path(installers).glob("*-setup.exe"))
    require(len(choices) == 1, f"expected one NSIS installer, found {choices}")
    command([choices[0], "/S"], timeout=240)
    folder = find_installed_app()
    with open(os.environ["GITHUB_ENV"], "a", encoding="utf-8") as stream:
        stream.write(f"SECS_MANAGE_INSTALL={folder}\n")
    print(f"Installed NSIS app: {folder}")


def prepare_edge(destination, artifacts):
    """Official driver only; signature validated before even running --version."""
    runner_only()
    destination, artifacts = Path(destination), Path(artifacts)
    destination.mkdir(parents=True, exist_ok=False)
    artifacts.mkdir(parents=True, exist_ok=True)
    runtime = webview_version()
    # Prefer an exact version. The official major lookup is only a fallback;
    # never accept a different major.minor.build, even if it is newer.
    driver = runtime
    url = edge_download(runtime, driver)
    try:
        response = urllib.request.urlopen(url, timeout=45)
    except urllib.error.HTTPError as error:
        if error.code != 404:
            raise
        lookup = f"https://msedgedriver.microsoft.com/LATEST_RELEASE_{runtime.split('.')[0]}_WINDOWS"
        with urllib.request.urlopen(lookup, timeout=30) as source:
            raw = source.read()
        driver = raw.decode("utf-16" if raw.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig").strip()
        url = edge_download(runtime, driver)
        response = urllib.request.urlopen(url, timeout=45)
    package = destination / "driver.zip"
    with response, package.open("wb") as stream:
        shutil.copyfileobj(response, stream)
    # Extract only one known member; never use archive paths as filesystem paths.
    binary = destination / "msedgedriver.exe"
    with zipfile.ZipFile(package) as archive:
        matches = [info for info in archive.infolist() if info.filename == "msedgedriver.exe"]
        require(len(matches) == 1, "official archive has no unique msedgedriver.exe")
        with archive.open(matches[0]) as source, binary.open("wb") as target:
            shutil.copyfileobj(source, target)
    signature = json.loads(powershell(
        f"$s=Get-AuthenticodeSignature -LiteralPath {ps_string(binary)}; "
        "@{status=$s.Status.ToString(); subject=$s.SignerCertificate.Subject} | ConvertTo-Json -Compress"))
    require(microsoft_signature(signature), f"msedgedriver has no valid Microsoft signature: {signature}")
    actual = command([binary, "--version"]).stdout.strip()
    require(re.search(rf"\b{re.escape(driver)}\b", actual), f"driver binary reports another version: {actual}")
    facts = {"webview2": runtime, "driver": driver, "url": url, "signature": signature, "binary_version": actual}
    (artifacts / "webdriver-version.json").write_text(json.dumps(facts, indent=2), encoding="utf-8")
    with open(os.environ["GITHUB_ENV"], "a", encoding="utf-8") as stream:
        stream.write(f"SECS_MSEDGEDRIVER={binary}\n")
    print(json.dumps(facts, indent=2))


WEBSOCKET_GUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"


def websocket_exchange(text, port=1234, timeout=4.0):
    """One text request and its text reply over RFC 6455, stdlib only.

    The app's CSP forbids ws://, so the check talks to CamillaDSP from here.
    (Windows PowerShell's ClientWebSocket returned an empty string on the
    runner, 2026-10-30, so the client is plain Python.)"""
    key = base64.b64encode(os.urandom(16)).decode("ascii")
    expected = base64.b64encode(hashlib.sha1((key + WEBSOCKET_GUID).encode("ascii")).digest()).decode("ascii")
    with socket.create_connection(("127.0.0.1", port), timeout=timeout) as sock:
        sock.sendall((f"GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\n"
                      f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n").encode("ascii"))
        reader = sock.makefile("rb")
        head = []
        while True:
            line = reader.readline(8192)
            if not line:
                raise CheckError("WebSocket closed during the handshake")
            if line in (b"\r\n", b"\n"):
                break
            head.append(line.decode("latin-1").strip())
            require(len(head) < 64, "WebSocket handshake headers too long")
        require(head and head[0].split(" ")[1:2] == ["101"], f"WebSocket handshake refused: {head[:1]}")
        accept = [line.split(":", 1)[1].strip() for line in head[1:] if line.lower().startswith("sec-websocket-accept:")]
        require(accept == [expected], "WebSocket handshake has a wrong Sec-WebSocket-Accept")

        def send(opcode, payload):
            mask = os.urandom(4)
            header = bytes([0x80 | opcode])
            length = len(payload)
            if length < 126:
                header += bytes([0x80 | length])
            elif length < 1 << 16:
                header += bytes([0x80 | 126]) + length.to_bytes(2, "big")
            else:
                header += bytes([0x80 | 127]) + length.to_bytes(8, "big")
            sock.sendall(header + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(payload)))

        def exact(count):
            data = reader.read(count)
            require(data is not None and len(data) == count, "WebSocket closed before the reply")
            return data

        send(0x1, text.encode("utf-8"))
        message = b""
        while True:
            first, second = exact(2)
            opcode, length = first & 0x0F, second & 0x7F
            if length == 126:
                length = int.from_bytes(exact(2), "big")
            elif length == 127:
                length = int.from_bytes(exact(8), "big")
            require(length <= 1 << 20, "WebSocket response too large")
            mask = exact(4) if second & 0x80 else None
            payload = exact(length)
            if mask:
                payload = bytes(b ^ mask[i % 4] for i, b in enumerate(payload))
            if opcode == 0x8:
                raise CheckError("WebSocket closed before the reply")
            if opcode == 0x9:
                send(0xA, payload)
                continue
            if opcode in (0x1, 0x0):
                message += payload
                require(len(message) <= 1 << 20, "WebSocket response too large")
                if first & 0x80:
                    break
        try:
            send(0x8, (1000).to_bytes(2, "big"))
        except OSError:
            pass
    return message.decode("utf-8")


def websocket(command_name):
    raw = websocket_exchange(json.dumps(command_name))
    try:
        value = json.loads(raw)
    except ValueError:
        raise CheckError(f"CamillaDSP {command_name}: reply is not JSON: {raw[:200]!r}") from None
    reply = value.get(command_name, {}) if isinstance(value, dict) else {}
    require(reply.get("result") == "Ok", f"CamillaDSP {command_name}: {value}")
    return reply.get("value")


def process_facts(pid):
    raw = powershell(
        f"$p=Get-CimInstance Win32_Process -Filter 'ProcessId={int(pid)}'; "
        "if ($null -eq $p) { 'null' } else { "
        "$o=Invoke-CimMethod -InputObject $p -MethodName GetOwnerSid; "
        "@{pid=$p.ProcessId; session=$p.SessionId; path=$p.ExecutablePath; sid=$o.Sid} | ConvertTo-Json -Compress }")
    return json.loads(raw)


def kill_installed_process(executable):
    # The shell auto-starts hidden on install and is single-instance. End only
    # that installed shell before WebDriver launches a debuggable visible one.
    path = ps_string(executable)
    powershell(f"Get-CimInstance Win32_Process | Where-Object {{$_.ExecutablePath -eq {path}}} | "
               "ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction Stop }")


def desktop_screenshot(path):
    powershell("Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; "
               "$r=[Windows.Forms.SystemInformation]::VirtualScreen; "
               "$b=[Drawing.Bitmap]::new($r.Width,$r.Height); $g=[Drawing.Graphics]::FromImage($b); "
               "try { $g.CopyFromScreen($r.Left,$r.Top,0,0,$b.Size); "
               f"$b.Save({ps_string(path)},[Drawing.Imaging.ImageFormat]::Png) "
               "} finally { $g.Dispose(); $b.Dispose() }")


PAGE_PROBLEMS = r"""
const texts = [];
for (const node of document.querySelectorAll('[role="alert"], [data-ui$=".problem"], [data-ui$=".reason"], details')) {
  const text = (node.textContent || '').replace(/\s+/g, ' ').trim();
  if (text && !texts.includes(text)) texts.push(text);
}
return texts.join(' | ').slice(0, 2000);
"""


SETUP_CALL = """
return window.__TAURI_INTERNALS__.invoke('secs_call', {request: arguments[0]}).then(envelope => {
  if (!envelope.ok) throw new Error(JSON.stringify(envelope.error));
  return envelope.data.data;
});
"""


class Scenario:
    def __init__(self, args):
        self.args = args
        self.artifacts = args.artifacts
        self.driver = WebDriver()
        self.records = []
        self.notes = ["Base: shared WASAPI pass-through, VB-CABLE Input loopback -> VB-CABLE Input (silent), no growing output file.",
                      "Applied configs are unchanged: stereo design, VB-CABLE WASAPI loopback -> shared WASAPI, F32.",
                      "tauri-driver 2.1.0 (released 2026-09-26); Edge build/signature in webdriver-version.json.",
                      "Equalizer APO XT: pinned 2.60.4 x64-sse2 Setup.exe --silent; require installer registry values."]
        self.host_root = Path(os.environ["ProgramData"]) / "SECS2R"
        self.host = args.installed_folder / "secs2r-host.exe"
        self.shell = args.installed_folder / "secs2r-host-shell.exe"
        self.app = args.installed_folder / "secs2r.exe"
        self.project = self.artifacts.parent / "manage-project" / "sample.secs"
        self.base = None
        self.tauri = None
        self.handles = []
        self.configs = set()
        self.active_application = None
        self.facts = environment_facts()

    def log(self, event, **values):
        with (self.artifacts / "steps.jsonl").open("a", encoding="utf-8") as stream:
            stream.write(json.dumps({"at": time.time(), "event": event, **values}) + "\n")

    def report(self):
        self.args.report.write_text(render_report(self.records, self.notes), encoding="utf-8")

    def snapshot(self, name):
        path = self.artifacts / f"{name}.png"
        if self.driver.session:
            self.driver.screenshot(path)
        else:
            desktop_screenshot(path)

    @contextmanager
    def step(self, number):
        name = STEPS[number - 1]
        self.log("start", number=number, name=name)
        record = {"number": number, "result": "PASS", "detail": ""}
        try:
            yield record
            self.snapshot(f"{number:02d}")
        except Exception as error:
            record.update(result="FAIL", detail=str(error))
            try:
                self.snapshot(f"{number:02d}-failure")
            except Exception as capture_error:
                self.log("screenshot_failed", detail=str(capture_error))
            raise
        finally:
            self.records.append(record)
            self.log("finish", **record)
            self.report()
            try:
                self.collect()
            except Exception as error:
                self.log("collection_error", detail=str(error))

    def collect(self):
        state_path = self.host_root / "host.json"
        if state_path.is_file():
            try:
                managed = json.loads(state_path.read_text(encoding="utf-8")).get("camilla")
                if managed:
                    self.configs.add(Path(managed["config_path"]))
            except (ValueError, KeyError) as error:
                self.log("state_parse_error", detail=str(error))
        for name in ("host.log", "host.log.1", "host.json"):
            path = self.host_root / name
            if path.is_file():
                shutil.copy2(path, self.artifacts / name)
        for config in self.configs:
            if config.is_file():
                target = self.artifacts / "camilla" / config.parent.parent.name
                target.mkdir(parents=True, exist_ok=True)
                for path in config.parent.glob("*.yml"):
                    shutil.copy2(path, target / path.name)
        config = self.args.eqapo_config / "config.txt"
        if config.is_file():
            shutil.copy2(config, self.artifacts / "config-current.txt")
        # Who holds the CamillaDSP port, for a failed takeover.
        try:
            listing = command(["netstat", "-ano", "-p", "TCP"], timeout=20, check=False).stdout
            rows = [line for line in listing.splitlines() if ":1234 " in line]
            pids = sorted({row.split()[-1] for row in rows if row.split() and row.split()[-1].isdigit()})
            processes = [powershell(f"(Get-Process -Id {pid} -ErrorAction SilentlyContinue | "
                                    "Select-Object Id,ProcessName,Path | ConvertTo-Json -Compress)", timeout=20)
                         for pid in pids]
            (self.artifacts / "port-1234.txt").write_text("\n".join(rows + processes) + "\n", encoding="utf-8")
        except (CheckError, OSError, subprocess.SubprocessError) as error:
            self.log("port_facts_error", detail=str(error))

    def status(self):
        value = parse_status(command([self.host, "status"], timeout=8).stdout)
        self.log("host_status", status=value)
        managed = value["managed"].get("camilla")
        if managed:
            self.configs.add(Path(managed["config_path"]))
        return value

    def logs(self):
        return parse_log((self.host_root / "host.log").read_text(encoding="utf-8"))

    def logged(self, event, pid=None):
        return any(line["event"] == event and (pid is None or line["fields"].get("pid") == str(pid))
                   for line in self.logs())

    def shown(self, selector):
        def rendered():
            element = self.driver.find(selector)
            visible = self.driver.execute(
                "const e = arguments[0]; return e.getClientRects().length > 0 "
                "&& getComputedStyle(e).visibility === 'visible';", {ELEMENT: element})
            return element if visible else None
        return wait_for(selector, rendered)

    def click(self, name):
        # Retried as a whole: right after a project opens the rail re-lays out,
        # and a click then lands on the status block ("element click
        # intercepted", which clicks nothing) or on a replaced element.
        selector = ui(name)
        def attempt():
            element = self.driver.find(selector)
            if self.driver.attribute(element, "disabled") is not None:
                return False
            self.driver.click(element)
            return True
        self.log("click", selector=selector)
        wait_for(f"click {name}", attempt)

    def type(self, name, value):
        element = self.shown(ui(name))
        self.driver.clear(element)
        # React controlled inputs receive real keyboard input, including Enter.
        self.driver.keys(element, value + "\ue007")
        wait_for(f"field {name}", lambda: self.driver.attribute(self.driver.find(ui(name)), "value"),
                 lambda current: current == value)
        self.log("type", selector=ui(name), value=value)

    def choose_virtual_output(self):
        devices = [device for device in self.facts["wasapi"]["render"]
                   if device["name"].casefold() == VIRTUAL_OUTPUT.casefold()]
        require(len(devices) == 1, f"Expected one VB-CABLE render endpoint: {devices}")
        name = devices[0]["name"]
        disclosure = self.shown(ui("export.devices"))
        if self.driver.attribute(disclosure, "aria-expanded") != "true":
            self.click("export.devices")
        for side in ("capture", "playback"):
            base = f"export.devices.{side}"
            self.click(base + ".backend.wasapi")
            if side == "capture":
                loopback = self.shown(ui(base + ".loopback"))
                if self.driver.attribute(loopback, "aria-checked") != "true":
                    self.click(base + ".loopback")
                self.shown(ui(base + '.loopback') + '[aria-checked="true"]')
            exclusive = self.shown(ui(base + ".exclusive"))
            if self.driver.attribute(exclusive, "aria-checked") == "true":
                self.click(base + ".exclusive")
            self.shown(ui(base + ".exclusive") + '[aria-checked="false"]')
            self.click(base + ".format.F32")
            selector = "select" + ui(base + ".device")
            select = self.shown(selector)  # Require real enumeration, never the typed-name fallback.
            def option():
                return next((item for item in self.driver.find_all(selector + " option")
                             if self.driver.attribute(item, "value") == name), None)
            self.driver.click(wait_for(f"VB-CABLE option for {side}", option))
            wait_for(f"VB-CABLE selected for {side}",
                     lambda: self.driver.execute("return arguments[0].value;", {ELEMENT: select}),
                     lambda value: value == name)
        self.log("gui_devices", capture="WASAPI loopback", playback="WASAPI shared", device=name, format="F32")

    def start_session(self, shell=False):
        self.driver.close()
        if self.active_application:
            kill_installed_process(self.active_application)
        executable = self.shell if shell else self.app
        kill_installed_process(executable)
        self.driver.start(executable)
        self.active_application = executable
        self.shown(ui("host" if shell else "frame"))

    def open_project(self):
        self.click("frame.project")
        self.click("frame.project.open")
        # After the setup reload the service may still hold the project: the
        # dialog then marks the recent entry as the open one (a status pill)
        # and disables it. Opening it again is not possible nor needed.
        recent = self.shown(ui("project.open.recent.1"))
        already_open = self.driver.execute(
            "const e = arguments[0]; return e.disabled && e.querySelector('.ui-pill') !== null;",
            {ELEMENT: recent})
        if already_open:
            self.log("project_already_open")
            self.click("project.open.cancel")
        else:
            self.click("project.open.recent.1")
        self.shown(ui("frame.status.file"))
        self.click("frame.rail.export")
        self.shown(ui("export.result.summary"))

    def setup(self):
        def call(method, params=None):
            request = {"method": method}
            if params is not None:
                request["params"] = params
            return self.driver.execute(SETUP_CALL, request)
        def job(method, params):
            identifier = call(method, params)["job"]
            result = wait_for(method, lambda: next((j for j in call("jobs") if j["job"] == identifier), None),
                              lambda j: j and j["status"] in {"succeeded", "failed", "cancelled"}, timeout=300)
            require(result["status"] == "succeeded", f"setup {method}: {result}")
        self.project.parent.mkdir(parents=True, exist_ok=False)
        call("project_new", {"path": str(self.project), "preset": "2.0", "sub_count": 0,
                             "sample": False, "tutorial": False})
        call("measure_configure", {"method": "mock"})
        job("measure_position", {"position": 1})
        job("calibrate", {"settings": None, "subs_only": False})
        results = call("results")
        require(results.get("calibration") and not results["calibration"]["stale"], "setup calibration missing/stale")
        call("project_save", {"path": None})
        settings = call("app_settings_get")
        settings["recent_projects"] = [str(self.project)]
        call("app_settings_set", {"settings": settings})
        # Direct setup calls bypass React's resource cache. A real page reload
        # followed by GUI project-open reloads that cache without injecting state.
        previous = self.driver.execute("return performance.timeOrigin;")
        self.driver.execute("window.location.reload(); return null;")
        wait_for("reload", lambda: self.driver.execute("return performance.timeOrigin;"), lambda value: value != previous)
        self.shown(ui("frame"))

    def page_problems(self):
        """The screen's error texts, including collapsed details, for a report."""
        try:
            return self.driver.execute(PAGE_PROBLEMS)
        except CheckError as error:
            return f"(page problems unreadable: {error})"

    def gui_apply(self, target, outcome=None):
        self.click(f"export.apply.{target}")
        try:
            element = self.shown(ui(f"export.apply.{target}.outcome") + '[data-outcome]')
        except CheckError as error:
            raise CheckError(f"{error}; screen: {self.page_problems()}") from None
        kind = wait_for(f"GUI apply {target}", lambda: self.driver.attribute(element, "data-outcome"),
                        lambda value: value in {"applied", "handed_over"}, timeout=120)
        if outcome:
            require(kind == outcome, f"expected {outcome}, got {kind}")
        self.log("gui_apply_finished", target=target, outcome=kind)
        return kind

    def running(self, previous=None):
        return wait_for("managed CamillaDSP running", self.status,
                        lambda s: s["camilla"]["state"]["kind"] == "running"
                        and s["camilla"]["pid"] != previous)

    def check_pid(self, status, shell=False):
        pid = status["camilla"]["pid"]
        facts = process_facts(pid)
        require(facts and facts["session"] == self.facts["active_console_session"]
                and facts["sid"] == self.facts["runner_user_sid"], f"wrong CamillaDSP user/session: {facts}")
        require(os.path.normcase(facts["path"]) == os.path.normcase(str(self.args.camilla)), f"wrong process binary: {facts}")
        selector = ui("host.camilla.state" if shell else "export.manage.camilla.state") + '[data-state="running"]'
        wait_for(f"GUI PID {pid}", lambda: self.driver.attribute(self.driver.find(selector), "data-pid"),
                 lambda value: value == str(pid))
        self.log("process", **facts)

    def check_websocket(self, status):
        path = status["managed"]["camilla"]["config_path"]
        require(Path(path).is_file(), f"applied config missing: {path}")
        state = wait_for("CamillaDSP GetState", lambda: websocket("GetState"), lambda s: s in {"Running", "Paused"})
        actual = wait_for("CamillaDSP applied config", lambda: websocket("GetConfigFilePath"),
                          lambda p: isinstance(p, str) and os.path.normcase(os.path.abspath(p)) == os.path.normcase(os.path.abspath(path)))
        self.log("websocket", state=state, config=actual)

    def execute(self):
        validate_environment(self.facts)
        require(find_installed_app() == self.args.installed_folder, "installed-folder differs from uninstall registry")
        self.report()
        shutil.copy2(self.args.eqapo_config / "config.txt", self.artifacts / "config-before.txt")
        with self.step(1) as record:
            path = self.artifacts / "base.yml"
            path.write_text(base_config(), encoding="utf-8")
            output = (self.artifacts / "base-camilla.log").open("wb")
            self.handles.append(output)
            self.base = subprocess.Popen([str(self.args.camilla), "-p", "1234", "-a", "127.0.0.1", str(path)],
                                         stdout=output, stderr=subprocess.STDOUT)
            wait_for("base GetState", lambda: websocket("GetState"), lambda value: value == "Running")
            require(self.base.poll() is None, "base CamillaDSP exited; another process may own port 1234")
            require(os.path.normcase(websocket("GetConfigFilePath")) == os.path.normcase(str(path)),
                    "port 1234 does not serve the base configuration")
            record["detail"] = f"base PID {self.base.pid}; VB-CABLE Input loopback -> VB-CABLE Input"
        with self.step(2) as record:
            output = (self.artifacts / "tauri-driver.log").open("wb")
            self.handles.append(output)
            self.tauri = subprocess.Popen([str(self.args.tauri_driver), "--native-driver", str(self.args.msedgedriver)],
                                          stdout=output, stderr=subprocess.STDOUT)
            wait_for("tauri-driver ready", lambda: self.driver.request("GET", "/status"),
                     lambda value: value.get("ready") is True)
            self.start_session()
            self.setup()
            record["detail"] = f"mock-room measurement and calibration saved to {self.project}"
        with self.step(3) as record:
            self.open_project()
            self.choose_virtual_output()
            self.click("export.apply.camilla.level.manage")
            self.type("export.apply.camilla.address", "127.0.0.1:1234")
            self.type("export.manage.camilla.exe", str(self.args.camilla))
            # P123 deliberately disables Manage until the service is installed.
            # Check the real policy here, then require enabled after step 4.
            self.shown(ui("export.manage.camilla.install"))
            button = self.driver.find(ui("export.apply.camilla"))
            require(self.driver.attribute(button, "disabled") is not None,
                    "Manage must be disabled before service installation")
            record["detail"] = "GUI saved VB-CABLE loopback/shared WASAPI and address/executable/manage; action awaits service installation (P123 policy)"
        with self.step(4) as record:
            self.click("export.manage.camilla.install")
            wait_for("SCM RUNNING", lambda: command(["sc.exe", "query", "SECS2RHost"], check=False).stdout,
                     lambda text: re.search(r"STATE\s*:\s*4\b", text) is not None)
            self.shown(ui("export.manage.camilla.service") + '[data-state="running"]')
            status = self.status()
            require(not status["managed"].get("camilla") and not status["managed"].get("eqapo"), "service inherited stale managed state")
            service = json.loads(powershell("Get-CimInstance Win32_Service -Filter \"Name='SECS2RHost'\" | Select-Object PathName,StartName | ConvertTo-Json -Compress"))
            copy = Path(os.environ.get("ProgramW6432", os.environ["ProgramFiles"])) / "SECS 2R Host" / "secs2r-host.exe"
            require(service_binary_path(service["PathName"]) == PureWindowsPath(str(copy)),
                    f"wrong registered binary: {service}")
            require(service["StartName"] == "LocalSystem" and copy.is_file(), f"wrong service account/copy: {service}")
            import winreg
            require(registry_value(winreg.HKEY_CURRENT_USER, RUN_KEY, "SECS2RHostShell") == f'"{self.shell}" --hidden', "missing shell logon command")
            wait_for("Manage enabled", lambda: self.driver.attribute(self.driver.find(ui("export.apply.camilla")), "disabled"), lambda value: value is None)
            record["detail"] = json.dumps(service)
        with self.step(5) as record:
            self.gui_apply("camilla")
            wait_for("base exited", self.base.poll, lambda code: code is not None)
            first = self.running(self.base.pid)
            p1 = first["camilla"]["pid"]
            wait_for("takeover/launch logs", lambda: self.logged("takeover") and self.logged("launch", p1))
            self.check_websocket(first)
            self.check_pid(first)
            self.shown(ui("export.manage.camilla.service") + '[data-state="running"]')
            record["detail"] = f"base {self.base.pid} exited; managed P1={p1}; config/user/session/GUI matched"
        with self.step(6) as record:
            command(["taskkill.exe", "/PID", str(p1), "/F"])
            second = self.running(p1)
            p2 = second["camilla"]["pid"]
            require(second["camilla"]["restarts_in_window"] >= 1, f"restart count missing: {second}")
            wait_for("exit/restart/launch logs", lambda: self.logged("exit", p1) and self.logged("restart") and self.logged("launch", p2))
            self.check_websocket(second)
            self.check_pid(second)
            record["detail"] = f"killed P1={p1}; restarted P2={p2}; restart count/log/GUI/websocket matched"
        with self.step(7) as record:
            config = self.args.eqapo_config / "config.txt"
            before = config.read_bytes()
            (self.artifacts / "config-before.txt").write_bytes(before)
            self.click("export.apply.eqapo.level.manage")
            self.gui_apply("eqapo")
            applied = config.read_bytes()
            check_config_bytes(before, applied)
            (self.artifacts / "config-applied.txt").write_bytes(applied)
            old_repair = self.status()["last_repair"]
            config.write_bytes(remove_managed_lines(applied))
            (self.artifacts / "config-edited.txt").write_bytes(before)
            def repaired():
                content = config.read_bytes()
                check_config_bytes(before, content)
                return content
            after = wait_for("Equalizer APO marker restored", repaired)
            status = wait_for("new last_repair", self.status,
                              lambda s: s["last_repair"] and s["last_repair"] != old_repair and s["eqapo_include"] == "present")
            wait_for("repair log", lambda: self.logged("repair"))
            self.shown(ui("export.manage.eqapo.state") + '[data-state="on"]')
            wait_for("GUI repair timestamp", lambda: self.driver.attribute(self.driver.find(ui("export.manage.eqapo.repair") + " time"), "datetime"),
                     lambda value: value == status["last_repair"])
            (self.artifacts / "config-after.txt").write_bytes(after)
            record["detail"] = f"marker restored; other bytes unchanged; last_repair={status['last_repair']}"
        with self.step(8) as record:
            self.start_session(shell=True)
            self.shown(ui("host.service") + '[data-state="running"]')
            self.shown(ui("host.camilla"))
            self.shown(ui("host.eqapo"))
            self.shown(ui("host.eqapo.line") + '[data-state="present"]')
            self.shown(ui("host.eqapo.repair") + " time[datetime]")
            self.shown(ui("host.log") + " li")
            self.check_pid(second, shell=True)
            self.click("host.stop")
            self.click("host.stop.confirm.confirm")
            wait_for("P2 exited", lambda: process_facts(p2), lambda value: value is None)
            started = time.monotonic()
            def stopped():
                status = self.status()
                require(not status["managed"].get("camilla") and not status["managed"].get("eqapo")
                        and status["camilla"]["pid"] is None, f"management relaunched: {status}")
                return time.monotonic() - started >= 10
            # Do not swallow an invariant failure as a transient wait error.
            deadline = time.monotonic() + 12
            while not stopped():
                require(time.monotonic() < deadline, "stop observation exceeded deadline")
                time.sleep(0.25)
            self.shown(ui("host.nothing"))
            record["detail"] = f"shell showed both targets and P2={p2}; no managed target/relaunch for 10s"
        with self.step(9) as record:
            self.start_session()
            self.open_project()
            self.choose_virtual_output()
            listing = command(["netstat", "-ano", "-p", "TCP"], timeout=20).stdout
            holder = listener_pid(listing, 1234)
            require(holder is None, f"PID {holder} still listens on 1234 before the cold-start check")
            result = self.gui_apply("camilla", "handed_over")
            third = self.running(p2)
            p3 = third["camilla"]["pid"]
            self.check_websocket(third)
            self.check_pid(third)
            record["detail"] = f"GUI outcome {result}; P3={p3}; applied config matched"
        with self.step(10) as record:
            self.start_session(shell=True)
            self.shown(ui("host.service") + '[data-state="running"]')
            self.click("host.uninstall")
            self.click("host.uninstall.confirm.confirm")
            try:
                wait_for("service deleted", lambda: command(["sc.exe", "query", "SECS2RHost"], check=False).returncode,
                         lambda code: code == 1060)
                wait_for("P3 exited", lambda: process_facts(p3), lambda value: value is None)
                wait_for("Program Files copy removed", copy.exists, lambda value: not value)
                wait_for("Run value removed", lambda: registry_value(winreg.HKEY_CURRENT_USER, RUN_KEY, "SECS2RHostShell"),
                         lambda value: value is None)
            except CheckError as error:
                raise CheckError(f"{error}; screen: {self.page_problems()}") from None
            self.shown(ui("host.install"))
            record["detail"] = f"SCM 1060; P3={p3} exited; Program Files copy and HKCU Run value absent"

    def cleanup(self):
        # No service CLI uninstall here: success must come from the GUI. Failed
        # runners are disposed by GitHub, preserving evidence of the failed state.
        for operation in (self.collect, self.driver.close,
                          lambda: kill_installed_process(self.shell),
                          lambda: kill_installed_process(self.app)):
            try:
                operation()
            except Exception as error:
                self.log("cleanup_error", detail=str(error))
        for process in (self.base, self.tauri):
            if process and process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=10)
        for handle in self.handles:
            handle.close()


def run(args):
    runner_only()
    args.artifacts.mkdir(parents=True, exist_ok=True)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    scenario = Scenario(args)
    try:
        scenario.execute()
    finally:
        scenario.cleanup()
        scenario.report()


def parser():
    result = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    subparsers = result.add_subparsers(dest="command", required=True)
    environment = subparsers.add_parser("env", help="report and require an interactive runner console")
    scenario = subparsers.add_parser("run", help="drive the installed app and shell; GitHub Windows runners only")
    for child in (environment, scenario):
        child.add_argument("--artifacts", type=Path, required=True)
        child.add_argument("--report", type=Path, required=True)
    for name in ("installed-folder", "camilla", "eqapo-config", "tauri-driver", "msedgedriver"):
        scenario.add_argument("--" + name, type=Path, required=True)
    environment.set_defaults(function=env)
    scenario.set_defaults(function=run)
    return result


def main(argv=None):
    args = parser().parse_args(argv)
    try:
        args.function(args)
    except (CheckError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
