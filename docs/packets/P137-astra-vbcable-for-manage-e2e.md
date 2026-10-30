# P137 (ASTRA): the managed-host GUI test plays through VB-CABLE instead of Scream

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\apply-manage`
(branch `claude/apply-manage`, PR #122). Do not commit, do not push. Never
install a driver, change a certificate store or run the scenario on this
machine: all of that runs only on GitHub's disposable Windows runners.

## Why

`manage-e2e` needs one virtual WASAPI playback device so that CamillaDSP has
an output. It used Scream 4.0, but on the runner its driver catalog
`scream.cat` fails Authenticode: the signer certificate (CN=Tom Kistner) is
outside its validity period and the signature has no timestamp; its
`devcon-x64.exe` is not signed at all. The maintainer chose (2026-10-30) to
replace Scream with VB-CABLE (VB-Audio Virtual Cable) and to keep requiring
a valid signature on the driver.

## Read first

`CLAUDE.md`; `.github/virtual-audio.toml`, `.github/scripts/virtual_audio.py`
and `test_virtual_audio.py` (how a pinned download is fetched, verified and
installed, the runner guard, endpoint enumeration through WASAPI);
`.github/scripts/manage_e2e.py` (`choose_scream`, the scenario notes, the
report) and `test_manage_e2e.py`; the `manage-e2e` job in
`.github/workflows/ci.yml` (read only; Claude edits the workflow).

External facts, fetch rather than guess, and cite each: the official
VB-CABLE download (https://vb-audio.com/Cable/ and its driver pack URL on
download.vb-audio.com), the latest driver pack's version and file layout,
its licence terms for this use (installing on a CI machine; we never
redistribute the files), the installer's silent command line, whether the
driver catalog is Microsoft-signed (WHQL or attestation) or signed by
VB-Audio, whether a silent install on Windows Server shows a "trust this
publisher" prompt and how established CI projects avoid it (for example
LABSN/sound-ci-helpers), and the render and capture endpoint names it
creates.

## Design (fixed)

1. `.github/virtual-audio.toml` pins VB-CABLE (version, official URL,
   SHA-256 you compute from the official download, format) in place of
   Scream, with a comment naming the licence and that the files are used
   only on disposable runners and never uploaded.
2. `virtual_audio.py`:
   - fetches and verifies the pinned pack;
   - requires `Valid` Authenticode for the installer and the driver catalog
     (report status, message and signer on failure, as the Scream code did);
   - if, and only if, the official installer cannot run silently without
     its publisher in the machine's TrustedPublisher store, adds exactly the
     signer certificate of that verified catalog there before installing,
     and says so in the log;
   - installs silently, waits for the render endpoint, and prints the
     endpoint facts as JSON the way it did for Scream.
   Keep the runner guard first, the PSModulePath handling and the stderr in
   errors. No test signing, no Secure Boot change.
3. `manage_e2e.py`: the device choice finds the VB-CABLE render endpoint
   (rename `choose_scream` to `choose_virtual_output` and update the notes
   and report text). Nothing else in the scenario changes.
4. Tests in `test_virtual_audio.py` and `test_manage_e2e.py` updated for
   the new pin, the signature rule, the optional trust step and the device
   choice.

## Files

You may edit: `.github/virtual-audio.toml`, `.github/scripts/virtual_audio.py`,
`.github/scripts/test_virtual_audio.py`, `.github/scripts/manage_e2e.py`,
`.github/scripts/test_manage_e2e.py`. To compute the SHA-256 you may
download the official pack into the session scratchpad
(`C:\Users\32170336\AppData\Local\Temp\claude\E--Sound-Equalization-and-Correction-System-2R\a241f0ba-0f2d-4875-a552-204969fa6951\scratchpad\vbcable\`)
and hash it there; do not extract or run anything from it on this machine.

## Verification (foreground; do not end the turn before they finish)

```sh
python -m unittest discover -s .github/scripts -p "test_*.py"
python -I .github/scripts/virtual_audio.py --help
```

## Report

The pin (version, URL, SHA-256) with the sources for each external fact,
the licence summary, the install steps as built, whether the trust step is
needed and why, the endpoint names, the step name and command ci.yml should
use, files changed, and the commands' last lines.
