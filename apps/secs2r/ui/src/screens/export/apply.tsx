// 내보내기의 적용 (ADR 0018): under the CamillaDSP row and the Equalizer APO
// row, the choice between 파일만 (today's files) and 적용, and with 적용 what
// it reaches. CamillaDSP: the websocket address, then the running
// CamillaDSP's version and state, the design it runs and its clipped
// samples; the header's primary applies (ExportScreen). Equalizer APO: the
// install and its config folder, SECS 2R's line in config.txt with the
// quiet 빼기 (one confirm sentence), and the design in its SECS2R folder;
// the row's own button applies. Each block ends with how the last apply
// ended. The keys stand in the rows' name column and the values where the
// buttons stand (the export grid's subgrid), like the design name above.
import { useState, type KeyboardEvent, type ReactNode } from "react";
import { errorText } from "../../app/jobs/jobText";
import type { MessageKey } from "../../i18n";
import { useNumber, useT, type Translate } from "../../i18n/react";
import type { CamillaApplyStatus } from "../../ipc/bindings/CamillaApplyStatus";
import type { EqApoApplyStatus } from "../../ipc/bindings/EqApoApplyStatus";
import type { EqApoVariant } from "../../ipc/bindings/EqApoVariant";
import type { IncludeState } from "../../ipc/bindings/IncludeState";
import type { IpcError } from "../../ipc/bindings/IpcError";
import { Button, ConfirmDialog, ErrorNotice, Icon, Segmented, Spinner } from "../../ui";
import { describedBy } from "../../ui/Field";
import { TextBox } from "./fields";
import {
  APPLY_LEVELS,
  addressProblem,
  addressText,
  applyLevel,
  applyProblem,
  eqapoDesign,
  isLoopback,
  type AddressProblem,
  type ApplyRun,
  type ApplyTarget,
  type EqApoDesign,
  type ExportActions,
  type ExportView,
  type ScreenApplyLevel,
} from "./types";

/** The address field's id; the header's primary points at `${ADDRESS_ID}-error` while the address is invalid. */
export const ADDRESS_ID = "export.apply.camilla.address";
/** Why Equalizer APO's 적용 is off (not installed); its button points here. */
export const EQAPO_REASON_ID = "export.apply.eqapo.reason";

const LEVEL_TEXT: Record<ScreenApplyLevel, MessageKey> = { files: "export.apply.level.files", apply: "export.apply.level.apply" };

const ADDRESS_PROBLEM_TEXT: Record<AddressProblem, MessageKey> = {
  format: "export.apply.camilla.address.format",
  port: "export.apply.camilla.address.port",
};

/** CamillaDSP's `ProcessingState` names (`GetState`), lower-cased. Anything else shows as CamillaDSP wrote it. */
const CAMILLA_STATE: Record<string, { text: MessageKey; glyph: Glyph }> = {
  running: { text: "export.apply.camilla.state.running", glyph: "on" },
  starting: { text: "export.apply.camilla.state.starting", glyph: "busy" },
  paused: { text: "export.apply.camilla.state.paused", glyph: "off" },
  inactive: { text: "export.apply.camilla.state.inactive", glyph: "warning" },
  stalled: { text: "export.apply.camilla.state.stalled", glyph: "warning" },
};

const VARIANT_TEXT: Record<EqApoVariant, MessageKey> = {
  official: "export.apply.eqapo.variant.official",
  xt: "export.apply.eqapo.variant.xt",
  unknown: "export.apply.eqapo.variant.unknown",
};

const LINE_TEXT: Record<IncludeState, { text: MessageKey; glyph: Glyph }> = {
  present: { text: "export.apply.eqapo.line.present", glyph: "on" },
  absent: { text: "export.apply.eqapo.line.absent", glyph: "off" },
  damaged: { text: "export.apply.eqapo.line.damaged", glyph: "warning" },
};

const EQAPO_DESIGN_TEXT: Record<EqApoDesign, { text: MessageKey; glyph: Glyph }> = {
  this: { text: "export.apply.design.this", glyph: "on" },
  other: { text: "export.apply.design.other", glyph: "warning" },
  none: { text: "export.apply.design.none", glyph: "off" },
  unknown: { text: "export.apply.eqapo.design.unknown", glyph: "off" },
};

/* ── Pieces ─────────────────────────────────────────────────────────────── */

type Glyph = "on" | "off" | "busy" | "warning" | "danger";

/** A state in words with a glyph before it: a filled dot (success, dimmed, danger), a spinner, or the warning sign. */
function State({ glyph, id, live, children }: { glyph: Glyph; id: string; live?: boolean; children: ReactNode }) {
  return (
    <span className={`export-apply-state export-apply-state--${glyph}`} id={id} data-ui={id} role={live ? "status" : undefined}>
      <span className="export-apply-state__glyph" aria-hidden="true">
        {glyph === "busy" ? <Spinner size={14} /> : glyph === "warning" ? <Icon name="warning" size={16} /> : <span className="export-apply-state__dot" />}
      </span>
      <span className="export-apply-state__text">{children}</span>
    </span>
  );
}

/** One fact: its name in the names' column, its value where the buttons stand. */
function Entry({ label, children }: { label: ReactNode; children: ReactNode }) {
  return (
    <div className="export-apply__entry">
      <dt className="export-apply__key">{label}</dt>
      <dd className="export-apply__value">{children}</dd>
    </div>
  );
}

function Refresh({ target, view, actions }: { target: ApplyTarget; view: ExportView; actions: ExportActions }) {
  const t = useT();
  return (
    <Button size="sm" variant="quiet" static className="export-apply__refresh" data-ui={`export.apply.${target}.refresh`} busy={view.apply.checking} onClick={actions.refreshApplyStatus}>
      {t("export.apply.refresh")}
    </Button>
  );
}

/** The status line before the first answer: being read, or not read yet (then 다시 확인 asks). */
function Unread({ target, view, actions }: { target: ApplyTarget; view: ExportView; actions: ExportActions }) {
  const t = useT();
  return (
    <span className="export-apply__line-value">
      {view.apply.checking ? (
        <State glyph="busy" id={`export.apply.${target}.state`} live>
          {t("export.apply.checking")}
        </State>
      ) : (
        <>
          <State glyph="off" id={`export.apply.${target}.state`} live>
            {t("export.apply.unread")}
          </State>
          <Refresh target={target} view={view} actions={actions} />
        </>
      )}
    </span>
  );
}

/* ── The block ──────────────────────────────────────────────────────────── */

/**
 * 파일만 or 적용 for one target, and with 적용 what it reaches and how the
 * last apply ended. Not drawn in the tutorial, which writes nothing.
 */
export function ApplyBlock({ target, view, actions, initialConfirm = false }: { target: ApplyTarget; view: ExportView; actions: ExportActions; initialConfirm?: boolean }) {
  const level = applyLevel(view, target);
  return (
    <div className="export-apply" data-ui={`export.apply.${target}.panel`}>
      <LevelField target={target} view={view} actions={actions} />
      {level === "apply" && target === "camilla" && <CamillaApply view={view} actions={actions} />}
      {level === "apply" && target === "eqapo" && <EqApoApply view={view} actions={actions} initialConfirm={initialConfirm} />}
      {level === "apply" && view.apply.runs[target] && <ApplyOutcome target={target} run={view.apply.runs[target]!} view={view} actions={actions} />}
    </div>
  );
}

function LevelField({ target, view, actions }: { target: ApplyTarget; view: ExportView; actions: ExportActions }) {
  const t = useT();
  // Equalizer APO lives on Windows only: 적용 stays in sight, off, with the reason.
  const unavailable = target === "eqapo" && !view.apply.windows;
  return (
    <div className="export-apply__level">
      <Segmented<ScreenApplyLevel>
        id={`export.apply.${target}.level`}
        label={t("export.apply.level")}
        size="sm"
        value={applyLevel(view, target)}
        options={APPLY_LEVELS.map((value) => ({ value, label: t(LEVEL_TEXT[value]), disabled: value === "apply" && unavailable }))}
        onChange={(level) => actions.setApplyLevel(target, level)}
        hint={unavailable ? t("export.apply.eqapo.windows_only") : undefined}
        help={t(target === "camilla" ? "export.apply.camilla.help" : "export.apply.eqapo.help")}
      />
    </div>
  );
}

/* ── CamillaDSP ─────────────────────────────────────────────────────────── */

function CamillaApply({ view, actions }: { view: ExportView; actions: ExportActions }) {
  const t = useT();
  const devices = view.camillaDevices;
  const placeholders = devices.capture.type === "file" && devices.playback.type === "file";
  return (
    <>
      <AddressField view={view} actions={actions} />
      {placeholders && (
        // File placeholders read and write files, not sound: said where 적용 is chosen.
        <p className="export-apply__note export-apply__value" data-ui="export.apply.camilla.devices_note">
          <Icon name="info" size={18} />
          <span>{t("export.apply.camilla.placeholders")}</span>
        </p>
      )}
      <dl className="export-apply__facts">
        <CamillaStatus status={view.apply.status?.camilla ?? null} view={view} actions={actions} />
      </dl>
    </>
  );
}

/**
 * `host:port`, typed freely and checked as typed (why it cannot be stored
 * shows under it, and the header's primary stays off); stored on Enter or
 * when the field is left. Escape puts the stored address back. An address
 * off this computer says what that means.
 */
function AddressField({ view, actions }: { view: ExportView; actions: ExportActions }) {
  const t = useT();
  const draft = view.apply.addressDraft;
  const text = addressText(view);
  const problem = draft === null ? null : addressProblem(draft);
  const error = problem ? t(ADDRESS_PROBLEM_TEXT[problem]) : null;
  const remote = !problem && !isLoopback(text);
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      actions.saveCamillaAddress();
    } else if (event.key === "Escape" && draft !== null) {
      // The first Escape puts the stored address back; it does not leave the screen.
      event.stopPropagation();
      actions.setCamillaAddress(view.apply.settings.camilla.address);
    }
  };
  return (
    <div className="export-apply__address" data-ui="export.apply.camilla.address.field">
      <label className="ui-field__label export-apply__key export-apply__key--control" htmlFor={ADDRESS_ID} id={`${ADDRESS_ID}-label`}>
        {t("export.apply.camilla.address")}
      </label>
      <div className="export-apply__value export-apply__control">
        <TextBox
          id={ADDRESS_ID}
          value={text}
          onChange={actions.setCamillaAddress}
          onBlur={actions.saveCamillaAddress}
          onKeyDown={onKeyDown}
          invalid={!!error}
          describedBy={describedBy(ADDRESS_ID, remote, error)}
        />
      </div>
      {error && (
        <p className="ui-field__error export-apply__value" id={`${ADDRESS_ID}-error`} data-ui="export.apply.camilla.address.problem">
          <Icon name="error" size={16} />
          <span>{error}</span>
        </p>
      )}
      {remote && (
        <p className="export-apply__note export-apply__value" id={`${ADDRESS_ID}-hint`} data-ui="export.apply.camilla.address.remote">
          <Icon name="info" size={18} />
          <span>{t("export.apply.camilla.address.remote")}</span>
        </p>
      )}
    </div>
  );
}

function CamillaStatus({ status, view, actions }: { status: CamillaApplyStatus | null; view: ExportView; actions: ExportActions }) {
  const t = useT();
  const number = useNumber();
  if (!status) {
    return (
      <Entry label={t("export.apply.status")}>
        <Unread target="camilla" view={view} actions={actions} />
      </Entry>
    );
  }
  if (!status.reachable || !status.status) {
    const reason = camillaReason(t, status.error, view.apply.settings.camilla.address);
    return (
      <Entry label={t("export.apply.status")}>
        <span className="export-apply__line-value">
          <State glyph="warning" id="export.apply.camilla.state" live>
            {t("export.apply.camilla.unreachable")}
          </State>
          <Refresh target="camilla" view={view} actions={actions} />
        </span>
        <p className="export-apply__reason" id="export.apply.camilla.reason" data-ui="export.apply.camilla.reason">
          {reason.cause} {reason.next}
        </p>
        {status.error && <Details error={status.error} />}
      </Entry>
    );
  }
  const camilla = status.status;
  const state = CAMILLA_STATE[camilla.state.toLowerCase()];
  const clipped = camilla.clipped_samples ?? 0;
  const design = status.design;
  return (
    <>
      <Entry label={t("export.apply.status")}>
        <span className="export-apply__line-value">
          <State glyph={state?.glyph ?? "off"} id="export.apply.camilla.state" live>
            {t("export.apply.camilla.version", { version: camilla.version, state: state ? t(state.text) : camilla.state })}
          </State>
          <Refresh target="camilla" view={view} actions={actions} />
        </span>
      </Entry>
      <Entry label={t("export.apply.camilla.design")}>
        {design.kind === "this" ? (
          <State glyph="on" id="export.apply.camilla.design">{t("export.apply.design.this")}</State>
        ) : design.kind === "other" ? (
          <>
            <State glyph="warning" id="export.apply.camilla.design">{t("export.apply.design.other")}</State>
            {/* Its title on its own line: the service's titles carry their own middle dots. */}
            <p className="export-apply__reason export-apply__title" data-ui="export.apply.camilla.design.title">
              {design.title}
            </p>
          </>
        ) : (
          <State glyph="off" id="export.apply.camilla.design">{t("export.apply.camilla.design.unknown")}</State>
        )}
      </Entry>
      {clipped > 0 && (
        <Entry label={t("export.apply.camilla.clipped")}>
          <State glyph="danger" id="export.apply.camilla.clipped">
            <span className="tabular">{t("export.apply.camilla.clipped.value", { count: number(clipped) })}</span>
          </State>
        </Entry>
      )}
    </>
  );
}

/** Why CamillaDSP did not answer: a connection failure in this screen's words, anything else as its code says. */
function camillaReason(t: Translate, error: IpcError | null, address: string): { cause: string; next: string } {
  if (!error || error.code === "apply_failed") {
    return { cause: t("export.apply.camilla.unreachable.cause", { address }), next: t("export.apply.camilla.unreachable.next") };
  }
  const text = errorText(t, error);
  return { cause: text.cause, next: text.next };
}

/** The raw text behind 자세히, as ErrorNotice keeps it. */
function Details({ error }: { error: IpcError }) {
  const t = useT();
  return (
    <details className="ui-details export-apply__details">
      <summary>{t("ui.details")}</summary>
      <pre className="ui-details__raw">{[`[${error.code}]`, error.detail].filter(Boolean).join(" ")}</pre>
    </details>
  );
}

/* ── Equalizer APO ──────────────────────────────────────────────────────── */

function EqApoApply({ view, actions, initialConfirm }: { view: ExportView; actions: ExportActions; initialConfirm: boolean }) {
  return (
    <dl className="export-apply__facts">
      <EqApoStatus status={view.apply.status?.eqapo ?? null} view={view} actions={actions} initialConfirm={initialConfirm} />
    </dl>
  );
}

function EqApoStatus({ status, view, actions, initialConfirm }: { status: EqApoApplyStatus | null; view: ExportView; actions: ExportActions; initialConfirm: boolean }) {
  const t = useT();
  if (!status) {
    return (
      <Entry label={t("export.apply.eqapo.install")}>
        <Unread target="eqapo" view={view} actions={actions} />
      </Entry>
    );
  }
  if (!status.installed) {
    return (
      <Entry label={t("export.apply.eqapo.install")}>
        <span className="export-apply__line-value">
          <State glyph="off" id="export.apply.eqapo.state" live>
            {t("export.apply.eqapo.not_installed")}
          </State>
          <Refresh target="eqapo" view={view} actions={actions} />
        </span>
        <p className="export-apply__reason" id={EQAPO_REASON_ID} data-ui={EQAPO_REASON_ID}>
          {t("export.apply.eqapo.not_installed.reason")}
        </p>
      </Entry>
    );
  }
  const line = LINE_TEXT[status.include];
  const design = EQAPO_DESIGN_TEXT[eqapoDesign(view)];
  return (
    <>
      <Entry label={t("export.apply.eqapo.install")}>
        <span className="export-apply__line-value">
          <State glyph="on" id="export.apply.eqapo.state" live>
            {t(VARIANT_TEXT[status.variant])}
          </State>
          <Refresh target="eqapo" view={view} actions={actions} />
        </span>
      </Entry>
      {status.config_path && (
        <Entry label={t("export.apply.eqapo.folder")}>
          <span className="export-path" data-ui="export.apply.eqapo.folder">
            {status.config_path}
          </span>
        </Entry>
      )}
      <Entry label={t("export.apply.eqapo.line")}>
        <span className="export-apply__line-value">
          <State glyph={line.glyph} id="export.apply.eqapo.line">
            {t(line.text)}
          </State>
          {status.include === "present" && <Revert view={view} actions={actions} initialConfirm={initialConfirm} />}
        </span>
        {status.include !== "present" && (
          // What 적용 does to config.txt, said before the click; a damaged pair is repaired by it.
          <p className="export-apply__reason" data-ui="export.apply.eqapo.line_note">
            {t(status.include === "absent" ? "export.apply.eqapo.line.absent.note" : "export.apply.eqapo.line.damaged.note")}
          </p>
        )}
      </Entry>
      <Entry label={t("export.apply.eqapo.design")}>
        <State glyph={design.glyph} id="export.apply.eqapo.design">
          {t(design.text)}
        </State>
      </Entry>
    </>
  );
}

/** 빼기: quiet, after the line it removes, and asked once in one sentence. */
function Revert({ view, actions, initialConfirm }: { view: ExportView; actions: ExportActions; initialConfirm: boolean }) {
  const t = useT();
  const [confirming, setConfirming] = useState(initialConfirm);
  return (
    <>
      <Button
        size="sm"
        variant="quiet"
        data-ui="export.apply.eqapo.revert"
        busy={view.apply.reverting}
        disabled={view.apply.applying === "eqapo"}
        onClick={() => setConfirming(true)}
      >
        {t("export.apply.eqapo.revert")}
      </Button>
      <RevertConfirm open={confirming} onClose={() => setConfirming(false)} onConfirm={actions.revertEqapo} />
    </>
  );
}

/** The confirm for 빼기: the action named in the title and on its button, the consequence in one sentence. */
export function RevertConfirm({ open, onClose, onConfirm, inline }: { open: boolean; onClose: () => void; onConfirm: () => void; inline?: boolean }) {
  const t = useT();
  return (
    <ConfirmDialog
      open={open}
      inline={inline}
      id="export.apply.eqapo.revert.confirm"
      title={t("export.apply.eqapo.revert.title")}
      body={t("export.apply.eqapo.revert.body")}
      confirmLabel={t("export.apply.eqapo.revert")}
      onClose={onClose}
      onConfirm={onConfirm}
    />
  );
}

/* ── How it ended ───────────────────────────────────────────────────────── */

const APPLIED_TEXT: Record<ApplyTarget, MessageKey> = { camilla: "export.apply.camilla.applied", eqapo: "export.apply.eqapo.applied" };

function ApplyOutcome({ target, run, view, actions }: { target: ApplyTarget; run: ApplyRun; view: ExportView; actions: ExportActions }) {
  const t = useT();
  const id = `export.apply.${target}.outcome`;
  if (run.status === "applied") {
    return (
      <div className="export-apply__full export-outcome" data-ui={id} role="status">
        <Icon name="check" size={18} weight="semibold" className="export-outcome__icon" />
        <p className="export-outcome__text">{t(APPLIED_TEXT[target])}</p>
        <Button size="sm" variant="quiet" icon="folder" data-ui={`${id}.open`} onClick={() => actions.openFolder(run.folder)}>
          {t("export.folder.open")}
        </Button>
      </div>
    );
  }
  if (run.status === "reverted") {
    return (
      <p className="export-apply__full export-outcome export-outcome--muted" data-ui={id} role="status">
        <Icon name="info" size={18} />
        <span>{t("export.apply.eqapo.reverted")}</span>
      </p>
    );
  }
  if (run.status === "rolled_back") {
    // CamillaDSP's own words are what the user fixes the devices by, so they stay in sight.
    return (
      <div className="export-apply__full" data-ui={id}>
        <ErrorNotice
          cause={t("export.apply.camilla.rolled_back")}
          next={
            <>
              <span className="export-apply__said">
                <span className="export-apply__said-label">{t("export.apply.camilla.said")}</span>
                <code className="export-apply__said-text" data-ui={`${id}.camilla_error`}>
                  {run.error.detail}
                </code>
              </span>
              <span className="export-apply__said-next">{t("export.apply.camilla.rolled_back.next")}</span>
            </>
          }
        />
      </div>
    );
  }
  const text = failureText(t, target, run.error);
  // The folder that could not be written, as the service named it, or Equalizer APO's config folder.
  const path = run.error.params.path ?? (target === "eqapo" && run.error.code === "permission_denied" ? view.apply.status?.eqapo.config_path ?? null : null);
  return (
    <div className="export-apply__full" data-ui={id}>
      <ErrorNotice
        cause={text.cause}
        next={
          <>
            {path && (
              <span className="export-apply__said">
                <span className="export-apply__said-label">{t("export.apply.path")}</span>
                <code className="export-apply__said-text" data-ui={`${id}.path`}>
                  {path}
                </code>
              </span>
            )}
            <span className="export-apply__said-next">{text.next}</span>
          </>
        }
        detail={run.error.detail}
        code={run.error.code}
      />
    </div>
  );
}

/** A failed apply in the row's words: write access to Equalizer APO's folder, else what the code says. */
export function failureText(t: Translate, target: ApplyTarget, error: IpcError): { cause: string; next: string } {
  if (error.code === "permission_denied") {
    return target === "eqapo"
      ? { cause: t("export.apply.eqapo.permission"), next: t("export.apply.eqapo.permission.next") }
      : { cause: t("export.apply.camilla.permission"), next: t("export.apply.camilla.permission.next") };
  }
  if (target === "eqapo" && error.code === "unsupported") return { cause: t("export.apply.eqapo.not_installed.cause"), next: t("export.apply.eqapo.not_installed.next") };
  const text = errorText(t, error);
  return { cause: text.cause, next: text.next };
}

/** Whether the row's apply problem is the missing install (its button points at the reason). */
export function eqapoMissing(view: ExportView): boolean {
  return applyProblem(view, "eqapo")?.problem === "not_installed";
}
