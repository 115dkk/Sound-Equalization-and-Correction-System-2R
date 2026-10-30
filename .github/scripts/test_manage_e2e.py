"""Pure/fake-HTTP tests only; never launch apps, services, installers or drivers."""
from __future__ import annotations

import base64
import io
import json
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path, PureWindowsPath
from unittest import mock

import manage_e2e as m


class WebDriverTest(unittest.TestCase):
    def setUp(self):
        self.requests = []
        self.replies = []
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def handle_request(self):
                data = self.rfile.read(int(self.headers.get("Content-Length", "0")))
                owner.requests.append((self.command, self.path, json.loads(data) if data else None))
                status, reply = owner.replies.pop(0)
                raw = reply if isinstance(reply, bytes) else json.dumps({"value": reply}).encode()
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(raw)))
                self.end_headers()
                self.wfile.write(raw)

            do_POST = do_GET = do_DELETE = handle_request

            def log_message(self, *_args):
                pass

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.client = m.WebDriver(f"http://127.0.0.1:{self.server.server_port}")
        self.client.session = "session-1"

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)

    def test_session_creation_waits_longer_than_other_commands(self):
        seen = []
        original = self.client.opener.open

        def recording_open(request, timeout=None):
            seen.append((request.get_method(), request.full_url.rsplit("/", 1)[-1], timeout))
            return original(request, timeout=timeout)

        self.client.opener.open = recording_open
        self.replies = [(200, {"sessionId": "new", "capabilities": {}}), (200, None)]
        self.client.start(r"C:\Installed\secs2r.exe")
        self.assertEqual(seen[0], ("POST", "session", m.SESSION_TIMEOUT))
        self.assertEqual(seen[1][2], 30)
        self.assertGreaterEqual(m.SESSION_TIMEOUT, 120)

    def test_session_capabilities_and_delete(self):
        self.replies = [(200, {"sessionId": "new", "capabilities": {}}), (200, None), (200, None)]
        self.client.start(r"C:\Installed\secs2r.exe", ["--example"])
        self.client.close()
        self.assertEqual(self.requests, [
            ("POST", "/session", {"capabilities": {"alwaysMatch": {"tauri:options": {
                "application": r"C:\Installed\secs2r.exe", "args": ["--example"], "webviewOptions": {},
            }}}}),
            ("POST", "/session/new/timeouts", {"implicit": 0, "pageLoad": 30000, "script": 10000}),
            ("DELETE", "/session/new", None),
        ])
        self.assertIsNone(self.client.session)

    def test_find_click_clear_keys_text_attribute_execute_and_screenshot(self):
        png = b"\x89PNG\r\n\x1a\nfixture"
        self.replies = [
            (200, {m.ELEMENT: "e1"}), (200, [{m.ELEMENT: "e1"}, {m.ELEMENT: "e2"}]),
            (200, None), (200, None), (200, None), (200, "PID 123"), (200, "true"),
            (200, {"observed": True}), (200, base64.b64encode(png).decode()),
        ]
        selector = m.ui("export.apply.camilla")
        element = self.client.find(selector)
        self.assertEqual(self.client.find_all(selector), ["e1", "e2"])
        self.client.click(element)
        self.client.clear(element)
        self.client.keys(element, "path\ue007")
        self.assertEqual(self.client.text(element), "PID 123")
        self.assertEqual(self.client.attribute(element, "disabled"), "true")
        self.assertEqual(self.client.execute("return arguments[0];", {"observed": True}), {"observed": True})
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "shot.png"
            self.client.screenshot(path)
            self.assertEqual(path.read_bytes(), png)
        prefix = "/session/session-1"
        self.assertEqual(self.requests[:5], [
            ("POST", prefix + "/element", {"using": "css selector", "value": selector}),
            ("POST", prefix + "/elements", {"using": "css selector", "value": selector}),
            ("POST", prefix + "/element/e1/click", {}),
            ("POST", prefix + "/element/e1/clear", {}),
            ("POST", prefix + "/element/e1/value", {"text": "path\ue007"}),
        ])
        self.assertEqual(self.requests[5:], [
            ("GET", prefix + "/element/e1/text", None),
            ("GET", prefix + "/element/e1/attribute/disabled", None),
            ("POST", prefix + "/execute/sync", {"script": "return arguments[0];", "args": [{"observed": True}]}),
            ("GET", prefix + "/screenshot", None),
        ])

    def test_http_and_protocol_errors_preserve_code_and_message(self):
        for status in (404, 200):
            self.replies.append((status, {"error": "no such element", "message": "last selector"}))
            with self.assertRaisesRegex(m.WebDriverError, "no such element: last selector"):
                self.client.find(m.ui("missing"))
        self.replies.append((500, b"not JSON"))
        with self.assertRaisesRegex(m.WebDriverError, "invalid response"):
            self.client.find(m.ui("missing"))

    def test_http_poll_deadline_records_last_driver_error(self):
        self.replies = [(404, {"error": "no such element", "message": "not rendered"})] * 3
        now = [0.0]
        with self.assertRaisesRegex(m.CheckError, "last observed.*not rendered"):
            m.wait_for("render", lambda: self.client.find("#missing"), timeout=1, interval=0.5,
                       clock=lambda: now[0], sleep=lambda seconds: now.__setitem__(0, now[0] + seconds))
        self.assertEqual(len(self.requests), 3)

    def test_invalid_screenshot_is_not_written(self):
        self.replies = [(200, base64.b64encode(b"not png").decode())]
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "shot.png"
            with self.assertRaisesRegex(m.CheckError, "not PNG"):
                self.client.screenshot(path)
            self.assertFalse(path.exists())


class WaitTest(unittest.TestCase):
    def test_false_values_can_be_successful_and_timeout_is_bounded(self):
        self.assertIsNone(m.wait_for("gone", lambda: None, lambda value: value is None))
        now, sleeps = [0.0], []

        def sleep(seconds):
            sleeps.append(seconds)
            now[0] += seconds

        with self.assertRaisesRegex(m.CheckError, "last observed: 'backing_off'"):
            m.wait_for("running", lambda: "backing_off", lambda value: value == "running",
                       timeout=1, interval=0.6, clock=lambda: now[0], sleep=sleep)
        self.assertEqual(sleeps, [0.6, 0.4])

    def test_transient_error_then_success(self):
        observe = mock.Mock(side_effect=[OSError("not ready"), 42])
        self.assertEqual(m.wait_for("ready", observe, sleep=lambda _seconds: None), 42)


class StatusTest(unittest.TestCase):
    def status(self, state="running"):
        return {"version": "0.1", "managed": {"camilla": None, "eqapo": None},
                "camilla": {"state": {"kind": state}, "pid": 123, "restarts_in_window": 1},
                "eqapo_include": "present", "last_repair": None}

    def test_actual_status_shape_and_other_user(self):
        for state in ("running", "backing_off", "given_up", "stopped", "no_session", "other_user"):
            expected = self.status(state)
            self.assertEqual(m.parse_status(json.dumps(expected)), expected)

    def test_refuses_missing_pid_or_unknown_states(self):
        status = self.status()
        status["camilla"]["pid"] = None
        with self.assertRaisesRegex(m.CheckError, "without a PID"):
            m.parse_status(json.dumps(status))
        with self.assertRaisesRegex(m.CheckError, "unknown managed"):
            m.parse_status(json.dumps(self.status("imaginary")))

    def test_log_events_and_string_pid_fields(self):
        lines = [{"timestamp": "2026-10-29T00:00:00Z", "event": name,
                  "fields": {"pid": "123"}} for name in
                 ("launch", "exit", "restart", "repair", "takeover", "port_busy", "given_up", "other_user")]
        self.assertEqual(m.parse_log("\n".join(json.dumps(line) for line in lines) + "\n"), lines)
        with self.assertRaises(m.CheckError):
            m.parse_log('{"event":"launch"}')
        with self.assertRaises(ValueError):
            m.parse_log('{"event":')

    def test_log_line_without_fields_reads_as_empty_fields(self):
        # HostLogLine's serde skips an empty `fields` map.
        self.assertEqual(m.parse_log('{"timestamp":"2026-10-30T19:16:00Z","event":"update_done"}\n'),
                         [{"timestamp": "2026-10-30T19:16:00Z", "event": "update_done", "fields": {}}])
        with self.assertRaises(m.CheckError):
            m.parse_log('{"timestamp":"t","event":"update_done","fields":[]}')


class ConfigTest(unittest.TestCase):
    def test_removes_only_marker_pair_and_preserves_other_bytes(self):
        for ending in (b"\n", b"\r\n"):
            before = b"\xef\xbb\xbf# existing \xec\x84\xa4\xec\xa0\x95" + ending + b"Preamp: -3 dB" + ending
            pair = m.MARKER + ending + m.INCLUDE + ending
            after = before + pair + b"# trailing" + ending
            self.assertEqual(m.remove_managed_lines(after), before + b"# trailing" + ending)
            m.check_config_bytes(before, before + pair)
            with self.assertRaisesRegex(m.CheckError, "outside SECS2R"):
                m.check_config_bytes(before, before.replace(b"-3", b"-2") + pair)

    def test_listener_is_found_by_its_address_columns(self):
        listing = ("  Proto  Local Address          Foreign Address        State           PID\n"
                   "  TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       1012\n"
                   "  TCP    127.0.0.1:1234         0.0.0.0:0              ABHÖREN         4484\n"
                   "  TCP    127.0.0.1:50000        127.0.0.1:1234         TIME_WAIT       0\n")
        self.assertEqual(m.listener_pid(listing, 1234), 4484)
        self.assertEqual(m.listener_pid(listing, 135), 1012)
        self.assertIsNone(m.listener_pid(listing, 4321))
        self.assertIsNone(m.listener_pid(listing.replace("4484", ""), 1234))

    def test_unterminated_last_line_round_trips(self):
        # Equalizer APO's default config.txt has no ending after its last line.
        before = b"Preamp: -6 dB\r\nGraphicEQ: 25 0; 16000 0"
        after = before + b"\r\n" + m.MARKER + b"\r\n" + m.INCLUDE
        self.assertEqual(m.remove_managed_lines(after), before)
        m.check_config_bytes(before, after)

    def test_missing_damaged_and_duplicate_pairs_fail(self):
        for data in (b"Preamp: 0 dB\n", m.MARKER + b"\nwrong\n",
                     (m.MARKER + b"\n" + m.INCLUDE + b"\n") * 2):
            with self.assertRaises(m.CheckError):
                m.remove_managed_lines(data)

    def test_base_is_infinite_and_plays_into_the_virtual_cable(self):
        text = m.base_config()
        self.assertIn('capture:\n    type: Wasapi\n    channels: 2\n    device: "CABLE Input (VB-Audio Virtual Cable)"\n'
                      '    format: F32\n    exclusive: false\n    loopback: true\n', text)
        self.assertNotIn("CABLE Output", text)
        self.assertNotIn("SignalGenerator", text)
        self.assertIn('type: Wasapi\n    channels: 2\n    device: "CABLE Input (VB-Audio Virtual Cable)"\n'
                      '    format: F32\n    exclusive: false\n', text)
        self.assertIn("filters: {}\nmixers: {}\npipeline: []\n", text)
        self.assertNotIn("type: File", text)
        self.assertNotIn("in.raw", text)


class InstallationTest(unittest.TestCase):
    def test_uninstall_lookup_uses_named_entry_and_default_nsis_location(self):
        entries = [{"DisplayName": "Other", "InstallLocation": "C:\\other"},
                   {"DisplayName": "SECS 2R", "InstallLocation": "C:\\Users\\runner\\Programs\\SECS 2R"}]
        self.assertEqual(m.installed_folder(lambda: entries), PureWindowsPath(entries[1]["InstallLocation"]))
        self.assertEqual(m.installed_folder(lambda: [{"DisplayName": "SECS 2R", "": "C:\\app"}]),
                         PureWindowsPath("C:/app"))

    def test_missing_ambiguous_and_relative_registry_entries_fail(self):
        for entries in ([], [{"DisplayName": "SECS 2R"}] * 2,
                        [{"DisplayName": "SECS 2R", "InstallLocation": "relative"}]):
            with self.assertRaises(m.CheckError):
                m.installed_folder(lambda: entries)

    def test_matching_edge_download_only_uses_microsoft(self):
        self.assertEqual(m.edge_download("142.0.3595.10", "142.0.3595.20"),
                         "https://msedgedriver.microsoft.com/142.0.3595.20/edgedriver_win64.zip")
        for driver in ("143.0.3595.20", "142.1.3595.20", "142.0.3596.20", "142.0.3595.20/evil", "142.0"):
            with self.assertRaises(m.CheckError):
                m.edge_download("142.0.3595.10", driver)

    def test_service_binary_path_accepts_only_the_run_command(self):
        expected = PureWindowsPath(r"C:\Program Files\SECS 2R Host\secs2r-host.exe")
        for argument in ("run", '"run"'):
            self.assertEqual(m.service_binary_path(f'"{expected}" {argument}'), expected)
        for value in ('"C:\\host.exe" install', '"C:\\host.exe" run extra', 'C:\\host.exe run'):
            with self.assertRaises(m.CheckError):
                m.service_binary_path(value)

    def test_signature_requires_valid_microsoft_organization(self):
        good = {"status": "Valid", "subject": "CN=Microsoft Corporation, O=Microsoft Corporation, C=US"}
        self.assertTrue(m.microsoft_signature(good))
        for signature in ({**good, "status": "NotTrusted"}, {"status": "Valid", "subject": "O=Other"},
                          {"status": "Valid", "subject": "O=Microsoft Corporation impostor"},
                          {"status": "Valid", "subject": None}):
            self.assertFalse(m.microsoft_signature(signature))

    def test_environment_without_console_fails_explicitly(self):
        facts = {"console_user": "", "process_session": 0, "active_console_session": 0xFFFFFFFF,
                 "elevated": True}
        with self.assertRaisesRegex(m.CheckError, "There is no user in the active console session"):
            m.validate_environment(facts)
        facts.update(console_user="runner", active_console_session=1)
        with self.assertRaisesRegex(m.CheckError, "not in the active console"):
            m.validate_environment(facts)
        facts.update(process_session=1, elevated=False)
        with self.assertRaisesRegex(m.CheckError, "not elevated"):
            m.validate_environment(facts)
        facts["elevated"] = True
        facts["wasapi"] = {"render": [], "capture": []}
        with self.assertRaisesRegex(m.CheckError, "There is no active WASAPI render endpoint"):
            m.validate_environment(facts)
        m.validate_environment(facts, audio=False)
        facts["wasapi"]["render"].append({"id": "render-id", "name": "CABLE Input (VB-Audio Virtual Cable)"})
        m.validate_environment(facts)

    def test_env_takes_no_audio_only_as_a_flag(self):
        self.assertFalse(m.parser().parse_args(["env", "--artifacts", "a", "--report", "r"]).no_audio)
        self.assertTrue(m.parser().parse_args(["env", "--artifacts", "a", "--report", "r", "--no-audio"]).no_audio)
        with self.assertRaises(SystemExit), mock.patch.object(m.sys, "stderr", io.StringIO()):
            m.parser().parse_args(["run", "--artifacts", "a", "--report", "r", "--no-audio"])

    def test_runtime_guard_refuses_a_workstation_and_path_overrides(self):
        with mock.patch.dict(m.os.environ, {}, clear=True):
            with self.assertRaisesRegex(m.CheckError, "disposable"):
                m.runner_only()
        with mock.patch.object(m.sys, "platform", "win32"), mock.patch.dict(m.os.environ, {
            "GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted", "SECS2R_HOST_PIPE": "fake",
        }, clear=True):
            with self.assertRaisesRegex(m.CheckError, "refuses SECS2R_HOST_PIPE"):
                m.runner_only()


class GuiContractTest(unittest.TestCase):
    def scenario(self):
        scenario = object.__new__(m.Scenario)
        scenario.driver = mock.Mock()
        scenario.click = mock.Mock()
        scenario.shown = mock.Mock(side_effect=lambda selector: selector)
        scenario.log = mock.Mock()
        return scenario

    def test_outcome_comes_from_dom_without_callback_instrumentation(self):
        scenario = self.scenario()
        scenario.driver.attribute.return_value = "handed_over"
        self.assertEqual(scenario.gui_apply("camilla", "handed_over"), "handed_over")
        scenario.click.assert_called_once_with("export.apply.camilla")
        scenario.shown.assert_called_once_with(m.ui("export.apply.camilla.outcome") + "[data-outcome]")
        scenario.driver.execute.assert_not_called()
        scenario.driver.attribute.return_value = "applied"
        with self.assertRaisesRegex(m.CheckError, "expected handed_over"):
            scenario.gui_apply("camilla", "handed_over")

    def test_vbcable_is_selected_by_option_value_through_real_controls(self):
        scenario = self.scenario()
        name = "CABLE Input (VB-Audio Virtual Cable)"
        scenario.facts = {"wasapi": {"render": [
            {"name": "CABLE In 16ch (VB-Audio Virtual Cable)", "id": "16ch"},
            {"name": name, "id": "opaque"}], "capture": [
            {"name": "CABLE Output (VB-Audio Virtual Cable)", "id": "capture"}]}}
        scenario.driver.find_all.return_value = ["default", "vbcable"]
        scenario.driver.attribute.side_effect = lambda element, attribute: (
            name if element == "vbcable" else "" if attribute == "value" else "false")
        scenario.driver.execute.return_value = name
        scenario.choose_virtual_output()
        clicks = [call.args[0] for call in scenario.click.call_args_list]
        self.assertEqual(clicks, ["export.devices", "export.devices.capture.backend.wasapi",
                                 "export.devices.capture.loopback", "export.devices.capture.format.F32",
                                 "export.devices.playback.backend.wasapi", "export.devices.playback.format.F32"])
        self.assertEqual(scenario.driver.click.call_args_list, [mock.call("vbcable"), mock.call("vbcable")])
        for side in ("capture", "playback"):
            scenario.shown.assert_any_call("select" + m.ui(f"export.devices.{side}.device"))
            scenario.shown.assert_any_call(m.ui(f"export.devices.{side}.exclusive") + '[aria-checked="false"]')
        for call in scenario.driver.execute.call_args_list:
            self.assertEqual(call.args[0], "return arguments[0].value;")

    def test_missing_or_ambiguous_vbcable_never_uses_another_device(self):
        name = "CABLE Input (VB-Audio Virtual Cable)"
        for names in ([], ["Hardware"], ["Speakers (Scream (WDM))"],
                      ["CABLE In 16ch (VB-Audio Virtual Cable)"],
                      ["CABLE Output (VB-Audio Virtual Cable)"], [name, name],
                      ["CABLE-A Input (VB-Audio Cable A)"], [name + " impostor"]):
            with self.subTest(names=names):
                scenario = self.scenario()
                scenario.facts = {"wasapi": {"render": [{"name": item} for item in names],
                                            "capture": [{"name": name}]}}
                with self.assertRaisesRegex(m.CheckError, "Expected one VB-CABLE"):
                    scenario.choose_virtual_output()
                scenario.click.assert_not_called()


class ReportTest(unittest.TestCase):
    def test_failed_step_never_marks_later_steps_passed(self):
        report = m.render_report([
            {"number": 1, "result": "PASS", "detail": "PID 123"},
            {"number": 2, "result": "FAIL", "detail": "bad | value\nnext"},
        ], ["driver 2.1.0"])
        self.assertIn("| 1. Base CamillaDSP | PASS | PID 123 |", report)
        self.assertIn("FAIL | bad \\| value<br>next", report)
        self.assertIn("| 10. Service removal through the shell | NOT RUN |", report)
        self.assertNotIn("Selector limitations", report)
        self.assertIn("driver 2.1.0", report)

    def test_parser_requires_every_runtime_path_and_help_has_no_side_effects(self):
        args = m.parser().parse_args([
            "run", "--installed-folder", "app", "--camilla", "camilla", "--eqapo-config", "apo",
            "--tauri-driver", "tauri", "--msedgedriver", "edge", "--artifacts", "artifacts", "--report", "report",
        ])
        self.assertEqual(args.installed_folder, Path("app"))
        self.assertIs(args.function, m.run)
        self.assertIn("run", m.parser().format_help())



class WebSocketTest(unittest.TestCase):
    """A one-shot RFC 6455 server on 127.0.0.1, like CamillaDSP's."""

    def serve(self, reply_frames):
        import hashlib
        import socket
        import threading
        server = socket.socket()
        server.bind(("127.0.0.1", 0))
        server.listen(1)
        port = server.getsockname()[1]
        seen = {}

        def run():
            conn, _ = server.accept()
            with conn:
                data = b""
                while b"\r\n\r\n" not in data:
                    data += conn.recv(4096)
                key = [l.split(":", 1)[1].strip() for l in data.decode().split("\r\n") if l.lower().startswith("sec-websocket-key:")][0]
                accept = base64.b64encode(hashlib.sha1((key + m.WEBSOCKET_GUID).encode()).digest()).decode()
                conn.sendall(f"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n".encode())
                head = conn.recv(2)
                length = head[1] & 0x7F
                mask = conn.recv(4)
                payload = bytes(b ^ mask[i % 4] for i, b in enumerate(conn.recv(length)))
                seen["request"] = payload.decode()
                seen["masked"] = bool(head[1] & 0x80)
                for frame in reply_frames:
                    conn.sendall(frame)
                try:
                    conn.recv(64)
                except OSError:
                    pass
            server.close()

        thread = threading.Thread(target=run, daemon=True)
        thread.start()
        return port, seen, thread

    @staticmethod
    def frame(opcode, payload, fin=True):
        return bytes([(0x80 if fin else 0) | opcode, len(payload)]) + payload

    def test_request_is_masked_and_the_text_reply_is_read(self):
        reply = json.dumps({"GetState": {"result": "Ok", "value": "Running"}}).encode()
        port, seen, thread = self.serve([self.frame(0x9, b"hi"), self.frame(0x1, reply[:10], fin=False),
                                         self.frame(0x0, reply[10:])])
        self.assertEqual(m.websocket_exchange(json.dumps("GetState"), port=port), reply.decode())
        thread.join(5)
        self.assertEqual(seen["request"], '"GetState"')
        self.assertTrue(seen["masked"])

    def test_websocket_returns_the_value_and_rejects_errors_and_non_json(self):
        with mock.patch.object(m, "websocket_exchange", return_value='{"GetState": {"result": "Ok", "value": "Running"}}'):
            self.assertEqual(m.websocket("GetState"), "Running")
        with mock.patch.object(m, "websocket_exchange", return_value='{"GetState": {"result": "Error", "value": "x"}}'):
            with self.assertRaises(m.CheckError):
                m.websocket("GetState")
        with mock.patch.object(m, "websocket_exchange", return_value=""):
            with self.assertRaisesRegex(m.CheckError, "not JSON"):
                m.websocket("GetState")

    def test_close_before_reply_fails(self):
        port, _, thread = self.serve([self.frame(0x8, b"")])
        with self.assertRaisesRegex(m.CheckError, "closed before the reply"):
            m.websocket_exchange('"GetState"', port=port)
        thread.join(5)


class ClickTest(unittest.TestCase):
    def test_an_intercepted_click_is_retried_until_it_lands(self):
        class Driver:
            def __init__(self):
                self.clicks = 0

            def find(self, selector):
                return {"selector": selector}

            def attribute(self, element, name):
                return None

            def click(self, element):
                self.clicks += 1
                if self.clicks == 1:
                    raise m.WebDriverError("element click intercepted", "status block")

        scenario = object.__new__(m.Scenario)
        scenario.driver = Driver()
        scenario.log = lambda *args, **kwargs: None
        scenario.click("frame.rail.export")
        self.assertEqual(scenario.driver.clicks, 2)

if __name__ == "__main__":
    unittest.main()
