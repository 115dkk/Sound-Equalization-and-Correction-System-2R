# REAPER convolution script checks

`mock.lua` tests routing, channel rounding, replacement by persistent track tags,
per-source ownership, missing IRs, plug-in/configuration/send failures and cleanup.
It is a model of the host API, not an audio or plug-in compatibility test.
`SECS_LUA` selects Lua 5.4; `SECS_LUAC` selects its compiler. Linux CI installs both
in its existing test job. With neither installed, the two optional Rust tests
print `SKIP` and return; explicitly configured executables must work.

## Real-host check

Use an **existing REAPER installation** and a fresh, disposable directory. Do not
run the harness in your normal REAPER instance. No SWS or other extensions are
needed. The harness refuses to proceed unless the resource directory, project
filename, empty track list and marker file all identify the disposable directory.
It dumps two runs (including complete track chunks), checks that the second run
replaces the first, reloads the unchanged empty project with `noprompt:`, and quits
without saving the generated project. The ordinary REAPER settings are not read
or modified.

PowerShell example, from the repository root (set `$reaper` to an existing install).

```powershell
$repo = (Get-Location).Path
$reaper = 'C:\Program Files\REAPER (x64)\reaper.exe'
$scratch = Join-Path $env:TEMP ('secs-reaper-' + [guid]::NewGuid())
New-Item -ItemType Directory $scratch | Out-Null
$env:SECS_REAPER_TEST_ROOT = $scratch
$env:SECS_REAPER_EXAMPLE = Join-Path $scratch 'bundle'
Set-Content (Join-Path $scratch 'SECS_REAPER_TEST_ONLY') 'disposable test instance'
Set-Content (Join-Path $scratch 'reaper.ini') "[REAPER]`n"
Set-Content (Join-Path $scratch 'empty.rpp') "<REAPER_PROJECT 0.1 7.0 1`n>"
cargo test -p secs-filters --locked reaper_script_uses_routing_order_and_quotes_names
if ($LASTEXITCODE -ne 0) { throw 'Fixture generation failed' }
$harness = Join-Path $repo 'crates\secs-filters\tests\reaper\harness.lua'
& $reaper -newinst -nosplash -cfgfile (Join-Path $scratch 'reaper.ini') (Join-Path $scratch 'empty.rpp') $harness
# Wait for this separate instance to quit before inspecting the dump.
Get-Content (Join-Path $scratch 'dump.txt')
```

A first-launch license or audio-device dialog might need dismissing in the
**disposable instance**. Do not automate dialogs in the user's ordinary instance.
On Linux/macOS use the equivalent environment variables and command arguments
with absolute paths to the installed REAPER and harness.

The fixture has input order `L, R`, output order `R, L, SW1`, and two half-scale
unit impulses. Each dump should contain five tracks:

| Track | Channels | Folder depth | Parent send | Routing |
| --- | --- | --- | --- | --- |
| Scratch source | 2 | 0 | off | channel 2 to path 1; channel 1 to path 2 |
| SECS 2R 거실 | 4 | 1 | on | output bus passes through to parent/master |
| R → SW1 | 2 | 0 | off | channel 1 to output bus channel 3 |
| L → R | 2 | 0 | off | channel 1 to output bus channel 1 |
| SECS 2R Output | 4 | -1 | on | all channels to the folder |

The sends have unity gain, no MIDI and mode 3 (post-FX, pre-fader). ReaVerb has one
`FILELDR` entry, Wet=1 (0 dB), Dry=0, Pre-verb=0, Width=1, Pan=0.5 (centre),
ZL=1 and LL=0 in normalized parameter units; host FX wet is 1. Only the first
input/output FX pin maps to track channel 1. Check the formatted values and
`ITEM0`, `ITEM1`, `pdc`, pin mappings and raw state in the dump. This harness does
not prove the audio transfer function: an audio render comparison is still needed
before claiming sample-accurate playback parity with the WAVs.

## Protocol and implementation notes

- [Official ReaScript reference](https://www.reaper.fm/sdk/reascript/reascripthelp.html)
  documents `TrackFX_SetNamedConfigParm`: ReaVerb accepts `ITEMx` state lines,
  followed by `DONE`. It also documents mono-send bits, channel counts, persistent
  track extension strings, normalized parameters and factory preset index `-2`.
- [ReaVerb IR-loading discussion](https://forum.cockos.com/showthread.php?t=234517)
  links to the [published IR script](https://raw.githubusercontent.com/ReaTeam/ReaScripts/master/Various/amagalma_Create%20Impulse%20Response%20(IR)%20of%20the%20FX%20chain%20of%20the%20selected%20track.lua),
  which uses the file-loader configuration `FILELDR "<absolute path>" 60` and
  then `DONE`. The exporter is independently implemented; no third-party source
  is included.
- [Official effects guide, ReaVerb sections 11.1 and 11.3](https://www.reaper.fm/guides/ReaEffectsGuide.pdf)
  describes wet/dry levels, ZL, file-loader normalization and attenuation.
  The API reference does not specify the numeric file-loader flags. The 60 value
  is the published script's value and the default file-loader flag in the
  [official REAPER 7.84 Linux distribution](https://www.reaper.fm/files/7.x/reaper784_linux_x86_64.tar.xz).
  Static inspection of that distribution confirms the Wet/Dry linear-amplitude
  conversion and parameter names/ranges. It is not a real-host execution test.
- The script resets ReaVerb to the factory preset, requires an empty module list,
  inserts exactly one file loader, verifies the configuration readback, sets the
  required parameters by name and checks their values. It fails and removes its
  new tracks rather than silently retaining an incompatible preset or plug-in.
- Existing source effects are retained. The source's fader/pan is deliberately
  not applied again. Its master/parent send is disabled only after successful
  construction. Existing hardware sends and other user-created sends are not
  changed. A source that is itself a folder parent is rejected to avoid an audio
  feedback loop through the generated folder's parent send. User tracks inserted
  inside the previous generated folder also stop replacement rather than being
  deleted.
- `export_convolution_named(run, folder, design_name)` accepts the optional name.
  The existing `export_convolution(run, folder)` remains compatible and uses
  `보정`. `FilterRun.label` is a mode label, **not** the design name. A service
  caller that has a design name must use the named entry point.
