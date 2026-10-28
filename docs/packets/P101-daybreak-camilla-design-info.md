# P101 (Daybreak): design identity, CamillaDSP labels, bypass, headroom and devices

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\export-bundle`
(branch `claude/export-convolution`, which already holds P100). Work only
there. Do not commit, do not push, do not spawn sub-agents. Claude reviews the
diff and commits.

Read first: `CLAUDE.md`, `docs/adr/0004-parity-and-deviations.md`,
`docs/DEVIATIONS.md`, `crates/secs-camilla/src/lib.rs` (whole file:
`CamillaOptions`, `Device`, `file_devices`, `howto_lines`, the pipeline and
mixer construction, `write_config`), `crates/secs-camilla/tests/`,
`crates/secs-graph/src/plan.rs` (`headroom_db`), `crates/secs-filters/src/lib.rs`
(the EqAPO preamp rule), the convolution export P100 added,
`crates/secs-types/src/ipc.rs` (`ExportParams`, `ExportKind`),
`crates/secs-types/src/chain.rs` (`SignalChain`, `MimoPaths`),
`crates/secs-service/src/export.rs`, `crates/secs-service/src/export_folder.rs`,
`crates/secs-service/src/lib.rs` (`upstream()`), and the CamillaDSP v4
documentation: `https://github.com/HEnquist/camilladsp` README (devices,
`labels`, pipeline `description` and `bypassed`) and the backend documents it
links for Wasapi and Asio. Fetch them; do not guess keys.

## Why

A user replacing Dirac Live Processor with CamillaDSP watches the running
CamillaDSP over its websocket (`GetConfigTitle`, `GetChannelLabels`, the
active config) and needs to tell which project and which design is loaded,
whether the correction is on, and which channel is which. Today the title is
`SECS 2 <mode> (<ms> ms)`, the description holds the roles in prose, devices
are file placeholders, and nothing names the correction steps. Every filter
export also needs one machine-readable record of what it holds.

## Rules (fixed)

1. **Design identity** (new module in `secs-graph`, used by every filter
   writer). `design_id` = lowercase hex SHA-256 of the canonical JSON
   `{"schema":1,"sample_rate":48000,"chain":<SignalChain>,"mimo":<MimoPaths or null>,"lfe":<the LfeChoice as {"gain_db","lp_hz"} or null>}`
   serialized with `serde_json::to_vec` (struct field order, shortest
   round-trip floats). The same design gives the same id in every export
   format. `design_id[..12]` is the short id.
2. **`secs-design.json`** in every filter export folder (CamillaDSP, filter
   text, Equalizer APO, convolution; not the mdat exports), written last in
   the same bundle: `schema` 1, `design_id`, `design_name`, `project_name`,
   `app` `{name, version}` (the app version from `Deps`), `upstream`
   `{repository, commit}` (from `secs_service::upstream()`), `created_utc`
   (RFC 3339), `export_kind` (wire name), `mode`, `latency_ms`, `stages`
   `{rc: bool, ba: bool, xo: bool, mimo: "none"|"fir"|"iir"}` derived from the
   chain and the MIMO paths, `sample_rate`, `inputs`, `outputs`,
   `headroom_db` (rule 4's `H`), `bypass_config` (rule 5's file name or
   null), and `files`: every other file of the bundle as
   `{path (relative, `/` separators), bytes, sha256}`, sorted by path.
3. **Design name and folder.** `ExportParams.design_name: Option<String>`
   (`#[serde(default)]`). Trimmed, 1 to 64 characters, no control
   characters, none of `\ / : * ? " < > |`, not a reserved Windows device
   name, not ending in `.` or space; otherwise `invalid_request` with the
   reason. Without a name the folder is today's timestamped folder and
   `design_name` in the manifest is the mode name (`저지연`, ...). With a name
   the folder is `SECS_<kind>_<name>` under the export base and a new export
   with the same name and kind replaces that folder's contents (build the new
   bundle in a sibling temporary folder, then swap, so a failed export leaves
   the old one). After each filter export, `designs.json` at the export base
   (UTF-8, pretty) lists `{design_name, design_id, export_kind, folder
   (relative), created_utc, mode, mimo}` for every folder under the base that
   holds a readable `secs-design.json`, newest first; rebuild it by scanning
   so a missing or corrupt file heals itself.
4. **Headroom in CamillaDSP.** A `Gain` filter `secs_headroom` of `-H` dB,
   `H = (headroom_db(plan) * 2).ceil() / 2` (the EqAPO preamp rule), applied
   to every input as the first pipeline step, with
   `description: "secs:headroom"`. When `H` is 0 the step is still written
   (gain 0) so its presence is stable.
5. **Bypass.**
   - Each RC correction filter of a main (its FIR and PEQs, the ops in
     `input_ops`) goes in pipeline steps of their own with
     `description: "secs:correction"` and an explicit `bypassed: false`, so a
     controller can switch them off without touching levels, delays,
     polarity, routing or crossovers. If a step today mixes correction
     filters with other filters, split it; the processed signal must stay the
     same.
   - MIMO support paths cannot be switched off by `bypassed` (an unfiltered
     support path would add the raw input to the support output). Their steps
     get `description: "secs:mimo"` and no `bypassed` key.
   - A second config `configs/<name>_bypass.yml` beside the main one: the same
     chain with every RC correction filter removed and without MIMO, keeping
     levels, delays, polarity, routing, bass management, crossovers and LFE,
     with its own headroom step (its own `H`). Its title ends with ` (우회)`
     and its description with ` | secs-design <design_id> bypass`. It shares
     the coefficient folder; it writes no FIR WAV the main config does not.
6. **Title, description, labels.**
   - `title`: `SECS 2R · <project name> · <design name>`; without a project
     name `SECS 2R · <design name>`.
   - `description`: today's text, then ` | secs-design <design_id>`.
   - Channel labels: the input role names on the capture device and the
     output role names on the playback device, where CamillaDSP v4 accepts
     `labels` (check the documentation; if only capture accepts them, write
     capture labels only and say so in the report).
7. **Devices.** `ExportParams.camilla_devices: Option<CamillaDevices>`
   (`#[serde(default)]`): `capture` and `playback`, each one of `File`
   (today's placeholder, the default), `Wasapi {device: Option<String>,
   format, exclusive: bool, loopback: bool (capture only)}` or
   `Asio {device: String}`, with the channel counts taken from the plan.
   Write the keys exactly as the CamillaDSP v4 backend documents spell them,
   and only formats it lists. CamillaDSP's released Windows binaries have no
   ASIO (it needs a build with `asio-backend`); the how-to lines in the YAML
   comment say so when an `Asio` device is written. Update the how-to
   comment so it no longer tells the user to replace file placeholders when
   real devices were chosen.
8. **Parity.** The upstream golden tests (`export_camilladsp_matches_oracle`
   and the others) keep comparing upstream bytes: `CamillaOptions` gains the
   new parts as options whose default reproduces today's bytes exactly
   (`design: None`, `headroom: false`, `labels: false`, `bypass: false`,
   file devices), and those tests keep the defaults. The service always
   turns the new parts on. Add tests for the new output instead of editing
   the goldens.
9. **Registry.** `features.toml`: new entries `export.design_manifest`,
   `export.camilla_bypass`, `export.camilla_devices` (kind `export`, status
   `implemented`, your tests, a note that they are new deviations from
   upstream), and update the `export.camilladsp` note.

## Tests (new, all must pass)

- `secs-graph`: `design_id` is stable for the same input, changes when one
  tap or one delay changes, and is the same for the CamillaDSP, EqAPO, text
  and convolution exports of one result.
- `secs-camilla`: with every option on, the oracle chains give a config that
  parses as YAML, has the title, the description suffix, the labels, the
  headroom step first with `-H`, correction steps with `bypassed: false`,
  MIMO steps without it; the processed signal is unchanged by the step
  split (compare the plan responses or a rendered impulse through the
  convolution renderer before and after); the bypass config has no RC filter
  and no MIMO step and keeps delays, gains, polarity and crossovers; the
  Wasapi and Asio device blocks have the documented keys.
- `secs-camilla`: with the defaults, the existing goldens pass unchanged.
- `secs-service`: exporting the oracle project with a design name twice
  replaces the folder and leaves one entry in `designs.json`; without a name
  the folder is timestamped; `secs-design.json` lists every other file with
  the right hash; an invalid name is `invalid_request` and writes nothing;
  a corrupt `designs.json` is rebuilt.

## Files

You may edit: `crates/secs-graph/**`, `crates/secs-camilla/**`,
`crates/secs-filters/**` (manifest call sites), `crates/secs-types/src/ipc.rs`
and the regenerated `apps/secs2r/ui/src/ipc/bindings/**`,
`crates/secs-service/src/**`, `crates/secs-service/tests/**`,
`apps/secs2r-cli/src/**` (accept `--design-name` and the device options),
`features.toml`, `Cargo.lock`, crate manifests only to add a dependency
(`sha2` is in the workspace).

Do not touch: other files under `apps/secs2r/ui/`, `docs/` (Claude writes
the deviations), the upstream tree, `.github/`, `CHANGELOG.md`.

## Verification (foreground; do not end the turn before they finish)

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python .github/scripts/check_feature_tests.py
python .github/scripts/upstream.py check-tree
```

and `cargo doc --no-deps -p secs-graph -p secs-camilla -p secs-types` with
`RUSTDOCFLAGS=-D warnings`.

## Report

Changed files; the CamillaDSP documentation URLs you used and the exact keys
you took from them; a sample config head (title, description, devices,
the first pipeline steps) for the oracle case with subs and MIMO; the tail of
each verification command; anything left undone.
