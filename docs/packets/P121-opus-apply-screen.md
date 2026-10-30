# P121 (Opus 5.5): 적용 on the export screen (ADR 0018)

Worktree: `E:\Sound-Equalization-and-Correction-System-2R\.claude\worktrees\apply-levels`
(branch `claude/apply-levels`; P113 and P119 built the backend). Work only
under `apps/secs2r/ui/`. Do not touch Rust, manifests or bindings, and do not
run cargo. Do not commit, do not push. ASTRA rewrites the catalog text
afterwards (P122).

Read first: `CLAUDE.md` (screen rules), `docs/DESIGN.md`,
`docs/adr/0018-applying-to-camilladsp-and-equalizer-apo.md`, the bindings
`ApplySettings.ts`, `ApplyLevel.ts`, `CamillaApply.ts`, `EqApoApply.ts`,
`ApplyStatus.ts`, `CamillaApplyStatus.ts`, `CamillaStatus.ts`,
`DesignMatch.ts`, `EqApoApplyStatus.ts`, `EqApoVariant.ts`,
`IncludeState.ts`, `ApplyCamillaParams.ts`, `ApplyResult.ts`,
`ApplyOutcome.ts`, `AppSettings.ts`, `JobKind.ts`, `JobResult.ts`,
`ErrorCode.ts`, `Request.ts`; `screens/export/**` (how the CamillaDSP row
shows its device fields under it, `ExportItem`, `useExportScreen.ts`,
`types.ts`, fixtures and specimens), `app/jobs/jobText.ts` and
`app/jobs/*` (job tray), `state/session.ts` and `state/effects*.ts` (how
app settings are read and written), `app/prefs.tsx`, and `src/ui/`. Load
the `make-interfaces-feel-better` skill and follow it.

## What it is for

Until now the app only wrote files; the user then loaded them into
CamillaDSP or Equalizer APO by hand. 적용 does that step: it pushes the
current design into a CamillaDSP that is already running (over its
websocket), or writes it into an installed Equalizer APO's config folder and
adds one marked `Include` line to `config.txt`. 관리 (a background service
that keeps it applied) comes later; this packet does not show it.

## Rules (fixed)

1. **Where.** On the export screen, under the CamillaDSP row (beside its
   device fields) and under the Equalizer APO row. Not in a new screen.
2. **Level.** Each of the two rows gets a choice between 파일만 (default,
   today's behaviour) and 적용, stored with `apply_settings_set` (the whole
   `ApplySettings`, read from `AppSettings.apply`). Do not offer 관리 yet.
3. **CamillaDSP with 적용.**
   - An address field (`host:port`, default `127.0.0.1:1234`), validated
     like the local API port field (invalid shows why under the field);
     a one-line note when the host is not loopback.
   - A status line from `apply_status` (refresh on open, after an apply,
     and with a quiet refresh button): not reachable (with the reason),
     or `CamillaDSP <version> · <state>` and whether the loaded config is
     this design, another design (show its title) or unknown; clipped
     samples when non-zero.
   - The row's primary action becomes `CamillaDSP에 적용` (runs
     `apply_camilla` with the row's devices); 내보내기 stays available as a
     secondary action. While the job runs, the job tray shows it like an
     export.
   - The outcome: applied, or rolled back with CamillaDSP's error.
4. **Equalizer APO with 적용.**
   - Status from `apply_status`: not installed; or installed (공식판 / XT /
     unknown) with the config folder, whether SECS 2R's line is present,
     absent or damaged, and the design ID applied there compared with this
     design (this design / another / none).
   - Primary action `Equalizer APO에 적용` (`apply_eqapo`); when the line is
     present, a quiet `config.txt에서 빼기` (`revert_eqapo`) that first asks
     in one confirm sentence (Equalizer APO stops using SECS 2R's filters;
     the files stay in the SECS2R folder).
   - `permission_denied` shows the folder and says the folder needs write
     access (no elevation offered).
5. **Not installed / unsupported.** On non-Windows hosts the Equalizer APO
   적용 choice is disabled with the reason. When Equalizer APO is not
   installed, 적용 stays selectable but its action is disabled with the
   reason.
6. **Wiring.** Extend the export view types and hook with the apply status,
   settings and the three actions (calls through the existing IPC wrapper);
   register job kind `apply` and the error codes `apply_failed`,
   `permission_denied`, `unsupported` in `app/jobs/jobText.ts` (and wherever
   error text is mapped); add the five requests to `ipc/mock.ts`
   (`apply_status`: CamillaDSP unreachable, no Equalizer APO install, but
   a mock toggle the specimens can flip; `apply_settings_set` stores and
   echoes; the jobs finish with an Applied result) and `mockWiring.test.ts`;
   add the `apply` default to the `AppSettings` fixture in
   `state/effects.test.ts`. `npm run typecheck` currently fails on exactly
   these; it must pass.
7. **Copy.** 합니다체 sentences, noun-phrase labels, labels that say the
   effect, UI text at least 14 px, no em-dash; every new string is a key in
   both catalogs with a first ko/en version (list them in the report).
8. **Specimens and tests.** Specimens: CamillaDSP 파일만; 적용 unreachable;
   적용 running this design; running another design with clipped samples;
   rolled back; Equalizer APO not installed; installed XT with the line
   absent; present with this design; damaged; permission denied; revert
   confirm. Tests for the level switch writing `ApplySettings`, the address
   validation, the primary action switching, the revert confirm, and the
   error texts.

## Files

You may edit: `apps/secs2r/ui/src/screens/export/**`, `app/jobs/**`,
`state/**`, `ipc/mock.ts`, `ipc/mockWiring.test.ts`, `ipc/fixtures.ts`,
`src/i18n/ko.json`, `en.json`, gallery registration if needed.

Do not touch: Rust, manifests, bindings, other screens, `.github/`, `docs/`.

## Verification (foreground; do not end the turn before they finish)

```sh
cd apps/secs2r/ui
npm ci
npm run typecheck
npm test
npm run build
```

Node 24 (npm 11); skip `npm ci` if `node_modules` is current.

## Report

Files changed, keys with first ko/en versions, specimens, and the commands'
last lines.
