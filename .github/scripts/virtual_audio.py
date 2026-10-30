#!/usr/bin/env python3
"""Install pinned Scream only on disposable Windows runners; enumerate WASAPI endpoints."""
from __future__ import annotations

import argparse
import base64
import json
import os
import runpy
import stat
import subprocess
import sys
import time
import zipfile
from pathlib import Path

# Explicit trusted repository path also works when the caller uses Python -I.
_engine = runpy.run_path(str(Path(__file__).with_name("eqapo_engine.py")))
EngineError = _engine["EngineError"]
fetch_file = _engine["fetch_file"]
load_manifest = _engine["load_manifest"]
DEFAULT_MANIFEST = Path(__file__).resolve().parent.parent / "virtual-audio.toml"
MEMBERS = (
    "Install/helpers/devcon-x64.exe",
    "Install/driver/x64/Scream.inf",
    "Install/driver/x64/Scream.sys",
    "Install/driver/x64/scream.cat",
)

# Original interop declarations for the read-only MMDevice API, not registry guesses.
# Contracts: https://learn.microsoft.com/en-us/windows/win32/coreaudio/device-properties
# and the Windows SDK mmdeviceapi.h, propsys.h, FunctionDiscoveryKeys_devpkey.h.
# Interfaces declare only the vtable prefix that is used. PowerShell initializes COM.
ENDPOINT_SOURCE = r"""
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
[ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")] class EnumeratorClass {}
[ComImport, Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface Enumerator { void EnumAudioEndpoints(int flow, uint state, out Devices devices); }
[ComImport, Guid("0BD7A1BE-7A1A-44DB-8397-CC5392387B5E"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface Devices { void GetCount(out uint count); void Item(uint index, out Device device); }
[ComImport, Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface Device {
    void Activate(ref Guid iid, uint context, IntPtr parameters, out IntPtr value);
    void OpenPropertyStore(uint access, out Properties properties);
    void GetId([MarshalAs(UnmanagedType.LPWStr)] out string id);
}
[StructLayout(LayoutKind.Sequential)] struct PropertyKey { public Guid format; public uint id; }
[StructLayout(LayoutKind.Explicit, Size=24)] struct Variant {
    [FieldOffset(0)] public ushort type;
    [FieldOffset(8)] public IntPtr text;
}
[ComImport, Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface Properties {
    void GetCount(out uint count);
    void GetAt(uint index, out PropertyKey key);
    void GetValue(ref PropertyKey key, out Variant value);
}
public class AudioEndpoint { public string id; public string name; }
public static class AudioEndpoints {
    [DllImport("ole32.dll")] static extern int PropVariantClear(ref Variant value);
    public static AudioEndpoint[] Read(int flow) {
        var enumerator = (Enumerator)new EnumeratorClass();
        Devices devices = null;
        try {
            enumerator.EnumAudioEndpoints(flow, 1, out devices); // DEVICE_STATE_ACTIVE
            uint count; devices.GetCount(out count);
            var result = new List<AudioEndpoint>();
            for (uint i = 0; i < count; i++) {
                Device device = null; Properties properties = null;
                var value = new Variant();
                try {
                    devices.Item(i, out device);
                    string id; device.GetId(out id);
                    device.OpenPropertyStore(0, out properties); // STGM_READ
                    var key = new PropertyKey { format = new Guid("A45C254E-DF1C-4EFD-8020-67D146A850E0"), id = 14 };
                    properties.GetValue(ref key, out value);
                    if (value.type != 31) throw new InvalidOperationException("Endpoint friendly name is not VT_LPWSTR");
                    result.Add(new AudioEndpoint { id = id, name = Marshal.PtrToStringUni(value.text) });
                } finally {
                    PropVariantClear(ref value);
                    if (properties != null) Marshal.ReleaseComObject(properties);
                    if (device != null) Marshal.ReleaseComObject(device);
                }
            }
            return result.ToArray();
        } finally {
            if (devices != null) Marshal.ReleaseComObject(devices);
            Marshal.ReleaseComObject(enumerator);
        }
    }
}
"""


def runner_only():
    if not (sys.platform == "win32" and os.environ.get("GITHUB_ACTIONS") == "true"
            and os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted"):
        raise EngineError("Virtual audio installation runs only on a disposable GitHub-hosted Windows runner")


def windows_powershell_env(environ=None):
    """The environment for Windows PowerShell 5.1 started from a PowerShell 7
    step: without PowerShell 7's PSModulePath, which keeps 5.1 from autoloading
    its own modules (Microsoft.PowerShell.Security holds Get-AuthenticodeSignature)."""
    source = os.environ if environ is None else environ
    return {key: value for key, value in source.items() if key.upper() != "PSMODULEPATH"}


def powershell(script):
    preamble = "$ErrorActionPreference='Stop'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new();\n"
    encoded = base64.b64encode((preamble + script).encode("utf-16le")).decode("ascii")
    result = subprocess.run(["powershell.exe", "-NoProfile", "-NonInteractive", "-EncodedCommand", encoded],
                            capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=90,
                            check=False, env=windows_powershell_env())
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip() or "no output"
        raise EngineError(f"PowerShell exited {result.returncode}: {detail}")
    return result.stdout.strip()


def endpoints():
    script = "Add-Type -TypeDefinition @'\n" + ENDPOINT_SOURCE + "\n'@\n"
    script += "@{render=@([AudioEndpoints]::Read(0)); capture=@([AudioEndpoints]::Read(1))} | ConvertTo-Json -Depth 4 -Compress"
    value = json.loads(powershell(script))
    if not all(isinstance(value.get(side), list) for side in ("render", "capture")):
        raise EngineError("WASAPI endpoint enumeration returned an invalid list")
    return value


def scream_endpoint(value):
    matches = [device for device in value["render"] if "scream" in device["name"].lower()]
    if len(matches) != 1:
        raise EngineError(f"Expected one active Scream WASAPI render endpoint, found {len(matches)}: {value}")
    return matches[0]


def extract_package(package, destination):
    # Never extract arbitrary archive paths or execute its batch files/clients.
    if destination.exists() and any(destination.iterdir()):
        raise EngineError(f"extraction destination is not empty: {destination}")
    with zipfile.ZipFile(package) as archive:
        for name in MEMBERS:
            entries = [item for item in archive.infolist() if item.filename == name]
            if len(entries) != 1 or stat.S_ISLNK(entries[0].external_attr >> 16):
                raise EngineError(f"missing, duplicate or linked Scream member: {name}")
        for name in MEMBERS:
            output = destination / name
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_bytes(archive.read(name))
    return destination / MEMBERS[0], destination / MEMBERS[1]


def ps_string(value):
    return "'" + str(value).replace("'", "''") + "'"


def install(args):
    runner_only()  # Before network, services, extraction or driver execution.
    entry = load_manifest(Path(args.manifest), "scream", {"zip"})
    if entry["url"] != f"https://github.com/duncanthrax/scream/releases/download/{entry['version']}/Scream{entry['version']}.zip":
        raise EngineError("Scream must come from its official GitHub release")
    destination = Path(args.dest).resolve()
    package = destination / "download" / f"Scream{entry['version']}.zip"
    fetch_file(entry["url"], entry["sha256"], package)
    devcon, inf = extract_package(package, destination / "driver")
    powershell("foreach ($name in @('AudioEndpointBuilder','Audiosrv')) { "
               "Set-Service -Name $name -StartupType Automatic; Start-Service -Name $name; "
               "(Get-Service $name).WaitForStatus('Running',[TimeSpan]::FromSeconds(30)) }")
    # The driver catalog must carry a valid signature; never enable test signing
    # or change Secure Boot. devcon-x64.exe in release 4.0 has no Authenticode
    # signature (the first real run on a runner reported NotSigned); it is
    # trusted as part of the archive whose SHA-256 is pinned above, as the
    # release's own Install-x64.bat runs it, and its facts are only logged.
    # The catalog read UnknownError on a runner (2026-10-30); the failure now
    # carries Windows' message and the signer so the cause can be judged.
    catalog = inf.with_name("scream.cat")
    for path in (devcon, catalog):
        facts = json.loads(powershell(
            f"$s = Get-AuthenticodeSignature -LiteralPath {ps_string(path)}; "
            "[pscustomobject]@{status=$s.Status.ToString(); message=$s.StatusMessage; "
            "signer=[string]$s.SignerCertificate.Subject; "
            "timestamper=[string]$s.TimeStamperCertificate.Subject} | ConvertTo-Json -Compress"))
        print(f"Authenticode {path.name}: {json.dumps(facts, ensure_ascii=False)}")
        if path == catalog and facts.get("status") != "Valid":
            raise EngineError(f"Scream package signature is not valid: {path}: {facts.get('status')}: "
                              f"{facts.get('message')} (signer: {facts.get('signer')}, "
                              f"timestamper: {facts.get('timestamper')})")
    # Same devcon install command as release 4.0's Install-x64.bat, without pause.
    # A fresh hosted runner has no old Scream device to remove. Do not force a reboot.
    result = subprocess.run([str(devcon), "install", str(inf), "*Scream"], stdin=subprocess.DEVNULL,
                            capture_output=True, text=True, timeout=120, check=False)
    print(result.stdout)
    if result.returncode != 0:
        raise EngineError(f"Scream installation failed or requires reboot ({result.returncode}): {result.stderr}")
    deadline = time.monotonic() + 60
    while True:
        value = endpoints()
        try:
            device = scream_endpoint(value)
            break
        except EngineError:
            if time.monotonic() >= deadline:
                raise
            time.sleep(1)
    print(json.dumps({"version": entry["version"], "sha256": entry["sha256"], "endpoint": device}))


def parser():
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--dest", required=True)
    result.add_argument("--manifest", default=str(DEFAULT_MANIFEST))
    return result


def main(argv=None):
    args = parser().parse_args(argv)
    try:
        install(args)
    except (EngineError, OSError, ValueError, zipfile.BadZipFile, subprocess.SubprocessError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
