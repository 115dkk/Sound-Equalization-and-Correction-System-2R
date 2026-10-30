// The host shell's one window (ADR 0018, 관리; host.html, about 420 × 560):
// what the SECS 2R Host service is doing while SECS 2R itself may be
// closed. The service's state in the header; what it manages (the design,
// CamillaDSP's program and process, Equalizer APO's folder, SECS 2R's line
// and its last repair) with the quiet 관리 끄기 at the head of that list; the
// latest log lines, newest first, each a time and a sentence with the raw
// line behind its disclosure; and the window's actions at the foot: 서비스
// 설치 while it is not installed, SECS 2R 열기, and the quiet 서비스 제거.
// 관리 끄기 and 서비스 제거 are each asked once in one sentence.
import { useId, useState, type ReactNode } from "react";
import { useLocale, useNumber, useT } from "../i18n/react";
import type { MessageKey } from "../i18n";
import type { HostLogLine } from "../ipc/bindings/HostLogLine";
import { Button, ConfirmDialog, EmptyState, ErrorNotice, Icon, Spinner, StatusPill, type PillTone } from "../ui";
import type { HostShellApi } from "./api";
import {
  CAMILLA_TEXT,
  SERVICE_TEXT,
  camillaDetail,
  camillaManaged,
  clockText,
  logSentence,
  managedTargets,
  newestFirst,
  serviceView,
  shortId,
  type Glyph,
  type HostState,
  type ServiceView,
} from "./model";
import { StopManagingConfirm } from "./parts";
import { LOG_LINES, useHostShell, type HostShellAction, type HostShellActions, type HostShellView } from "./useHostShell";
import "./host.css";

/** The page wired to `api`, read every `intervalMs` while the window is visible. */
export function HostShell({ api, intervalMs }: { api: HostShellApi; intervalMs?: number }) {
  const { view, actions } = useHostShell(api, intervalMs === undefined ? {} : { intervalMs });
  return <HostShellPage view={view} actions={actions} />;
}

const SERVICE_TONE: Record<ServiceView, PillTone> = { checking: "busy", not_installed: "neutral", stopped: "warning", unreachable: "warning", running: "success" };

/** What to do about a service that does not run, said where the managed targets would be. */
const SERVICE_NOTE: Partial<Record<ServiceView, MessageKey>> = {
  not_installed: "host.install.why",
  stopped: "host.stopped.next",
  unreachable: "host.unreachable.next",
};

const FAILURE_TEXT: Record<HostShellAction, { cause: MessageKey; next: MessageKey }> = {
  install: { cause: "host.install.failed", next: "host.install.failed.next" },
  uninstall: { cause: "host.uninstall.failed", next: "host.uninstall.failed.next" },
  stop: { cause: "host.stop.failed", next: "host.stop.failed.next" },
  open: { cause: "host.shell.open_failed", next: "host.shell.open_failed.next" },
};

export function HostShellPage({ view, actions }: { view: HostShellView; actions: HostShellActions }) {
  const t = useT();
  const service = serviceView(view.service, view.host);
  const [confirm, setConfirm] = useState<"stop" | "uninstall" | null>(null);
  const running = service === "running" && view.host !== null;
  // One action at a time: the others wait while one is in flight.
  const busy = (action: HostShellAction) => ({ busy: view.busy === action, disabled: view.busy !== null && view.busy !== action });
  return (
    <div className="host-shell" data-ui="host">
      <header className="host-shell__head">
        <div className="host-shell__heading">
          <h1 className="host-shell__title">{t("host.shell.title")}</h1>
          {view.host && <p className="host-shell__version tabular">{t("host.shell.version", { version: view.host.version })}</p>}
        </div>
        <StatusPill tone={SERVICE_TONE[service]} data-ui="host.service" data-state={service} role="status">
          {t(SERVICE_TEXT[service])}
        </StatusPill>
      </header>
      <div className="host-shell__body" data-ui="host.body">
        {view.readError && <ErrorNotice id="host.read_error" tone="warning" cause={t("host.shell.read_failed")} next={t("host.shell.read_failed.next")} detail={view.readError.detail} code={view.readError.code} />}
        {view.failure && (
          <ErrorNotice id="host.failure" cause={t(FAILURE_TEXT[view.failure.action].cause)} next={t(FAILURE_TEXT[view.failure.action].next)} detail={view.failure.error.detail} code={view.failure.error.code} />
        )}
        {running ? (
          <Managed
            host={view.host!}
            now={view.now}
            stop={
              <Button variant="quiet" size="sm" data-ui="host.stop" {...busy("stop")} onClick={() => setConfirm("stop")}>
                {t("host.stop")}
              </Button>
            }
          />
        ) : (
          <ServiceNote service={service} installing={view.busy === "install"} />
        )}
        {running && <Log lines={view.log} now={view.now} />}
      </div>
      <footer className="host-shell__foot">
        <div className="host-shell__actions">
          {/* The filled action: installing while there is no service, else opening SECS 2R (where 관리 is chosen and started). */}
          {service === "not_installed" && (
            <Button variant="primary" data-ui="host.install" {...busy("install")} onClick={actions.install}>
              {t("host.install")}
            </Button>
          )}
          <Button variant={service === "not_installed" ? "secondary" : "primary"} data-ui="host.open_app" {...busy("open")} onClick={actions.openApp}>
            {t("host.shell.open_app")}
          </Button>
        </div>
        {(service === "running" || service === "stopped" || service === "unreachable") && (
          <Button variant="quiet" data-ui="host.uninstall" {...busy("uninstall")} onClick={() => setConfirm("uninstall")}>
            {t("host.uninstall")}
          </Button>
        )}
      </footer>
      <StopManagingConfirm id="host.stop.confirm" open={confirm === "stop"} host={view.host} onClose={() => setConfirm(null)} onConfirm={actions.stopManaging} />
      <UninstallConfirm open={confirm === "uninstall"} onClose={() => setConfirm(null)} onConfirm={actions.uninstall} />
    </div>
  );
}

/** 서비스 제거's confirm: the action in the title and on its button, the consequence in one line. */
export function UninstallConfirm({ open, onClose, onConfirm, inline }: { open: boolean; onClose: () => void; onConfirm: () => void; inline?: boolean }) {
  const t = useT();
  return (
    <ConfirmDialog
      id="host.uninstall.confirm"
      open={open}
      inline={inline}
      title={t("host.uninstall.title")}
      body={t("host.uninstall.body")}
      confirmLabel={t("host.uninstall")}
      onClose={onClose}
      onConfirm={onConfirm}
    />
  );
}

/** A service that does not run: why the service is needed, or what to do; nothing while it is first read (the pill says so). */
function ServiceNote({ service, installing }: { service: ServiceView; installing: boolean }) {
  const t = useT();
  const note = SERVICE_NOTE[service];
  if (!note) return null;
  return (
    <div className="host-note" data-ui="host.note">
      <p>{t(note)}</p>
      {installing && <p className="host-note__next">{t("host.install.waiting")}</p>}
    </div>
  );
}

/* ── What the service manages ───────────────────────────────────────────── */

/** What the service manages, under a heading that carries 관리 끄기 (it ends both at once), or where to start 관리. */
function Managed({ host, now, stop }: { host: HostState; now: number; stop: ReactNode }) {
  const t = useT();
  const titleId = useId();
  const managed = managedTargets(host);
  if (!managed.camilla && !managed.eqapo) {
    return (
      <div className="host-card">
        <EmptyState id="host.nothing" compact title={t("host.shell.nothing")} description={t("host.shell.nothing.next")} />
      </div>
    );
  }
  return (
    <section className="host-section" aria-labelledby={titleId}>
      <div className="host-section__head">
        <h2 className="host-section__title" id={titleId}>
          {t("host.shell.managed")}
        </h2>
        {stop}
      </div>
      {managed.camilla && <CamillaCard host={host} now={now} />}
      {managed.eqapo && <EqApoCard host={host} now={now} />}
    </section>
  );
}

function Card({ id, title, children }: { id: string; title: string; children: ReactNode }) {
  const titleId = useId();
  return (
    <section className="host-card" data-ui={id} aria-labelledby={titleId}>
      <h2 className="host-card__title" id={titleId}>
        {title}
      </h2>
      <dl className="host-facts">{children}</dl>
    </section>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt>{label}</dt>
      <dd>{children}</dd>
    </>
  );
}

/** A state in words after its glyph, as the export screen draws it. */
/** `state` and `pid` mirror the shown state as attributes, for checks that must not read the catalog text. */
function State({ glyph, id, state, pid, children }: { glyph: Glyph; id: string; state?: string; pid?: number | null; children: ReactNode }) {
  return (
    <span className={`host-state host-state--${glyph}`} data-ui={id} data-state={state} data-pid={pid ?? undefined}>
      <span className="host-state__glyph" aria-hidden="true">
        {glyph === "busy" ? <Spinner size={14} /> : glyph === "warning" ? <Icon name="warning" size={16} /> : <span className="host-state__dot" />}
      </span>
      <span className="host-state__text">{children}</span>
    </span>
  );
}

function CamillaCard({ host, now }: { host: HostState; now: number }) {
  const t = useT();
  const locale = useLocale();
  const camilla = host.managed.camilla!;
  const managed = camillaManaged(host);
  const text = CAMILLA_TEXT[managed.kind];
  const detail = camillaDetail(t, managed, (iso) => clockText(iso, now, locale), "host.camilla.given_up.next.shell");
  return (
    <Card id="host.camilla" title={t("host.shell.camilla")}>
      <Fact label={t("export.apply.status")}>
        <State glyph={text.glyph} id="host.camilla.state" state={managed.kind} pid={managed.kind === "running" ? managed.pid : null}>
          {t(text.text)}
          {managed.kind === "running" && managed.pid !== null && <span className="host-state__pid tabular">{t("host.camilla.pid", { pid: managed.pid })}</span>}
        </State>
        {detail && (
          <p className="host-facts__reason" data-ui="host.camilla.reason">
            {detail}
          </p>
        )}
        {managed.kind === "given_up" && managed.reason && <Raw text={managed.reason} />}
      </Fact>
      <Fact label={t("host.shell.exe")}>
        <span className="host-path" data-ui="host.camilla.exe">
          {camilla.exe_path}
        </span>
      </Fact>
      <Fact label={t("host.shell.port")}>
        <span className="tabular">{camilla.port}</span>
      </Fact>
      <Fact label={t("host.shell.design")}>
        <DesignId id={camilla.design_id} />
      </Fact>
    </Card>
  );
}

const LINE_TEXT: Record<HostState["eqapo_include"], { text: MessageKey; glyph: Glyph }> = {
  present: { text: "export.apply.eqapo.line.present", glyph: "on" },
  absent: { text: "export.apply.eqapo.line.absent", glyph: "warning" },
  damaged: { text: "export.apply.eqapo.line.damaged", glyph: "warning" },
};

function EqApoCard({ host, now }: { host: HostState; now: number }) {
  const t = useT();
  const locale = useLocale();
  const eqapo = host.managed.eqapo!;
  const line = LINE_TEXT[host.eqapo_include];
  return (
    <Card id="host.eqapo" title={t("host.shell.eqapo")}>
      <Fact label={t("export.apply.eqapo.line")}>
        <State glyph={line.glyph} id="host.eqapo.line" state={host.eqapo_include}>
          {t(line.text)}
        </State>
      </Fact>
      <Fact label={t("host.eqapo.repair")}>
        <span className="tabular" data-ui="host.eqapo.repair">
          {host.last_repair ? <time dateTime={host.last_repair}>{clockText(host.last_repair, now, locale)}</time> : t("host.eqapo.repair.none")}
        </span>
      </Fact>
      <Fact label={t("export.apply.eqapo.folder")}>
        <span className="host-path" data-ui="host.eqapo.folder">
          {eqapo.config_path}
        </span>
      </Fact>
      <Fact label={t("host.shell.design")}>
        <DesignId id={eqapo.design_id} />
      </Fact>
    </Card>
  );
}

/** The design as its short id; the whole id is the element's title for comparing by hand. */
function DesignId({ id }: { id: string }) {
  return (
    <span className="host-path tabular" title={id}>
      {shortId(id)}
    </span>
  );
}

function Raw({ text }: { text: string }) {
  const t = useT();
  return (
    <details className="ui-details">
      <summary>{t("ui.details")}</summary>
      <pre className="ui-details__raw host-raw">{text}</pre>
    </details>
  );
}

/* ── The log ────────────────────────────────────────────────────────────── */

function Log({ lines, now }: { lines: readonly HostLogLine[]; now: number }) {
  const t = useT();
  const titleId = useId();
  const shown = newestFirst(lines, LOG_LINES);
  return (
    <section className="host-section" aria-labelledby={titleId}>
      <div className="host-section__head">
        <h2 className="host-section__title" id={titleId}>
          {t("host.shell.log")}
        </h2>
      </div>
      {shown.length === 0 ? (
        <p className="host-log__empty" data-ui="host.log.empty">
          {t("host.shell.log.empty")}
        </p>
      ) : (
        <ol className="host-log__list" data-ui="host.log">
          {shown.map((line, index) => (
            <LogEntry key={`${line.timestamp}-${line.event}-${index}`} line={line} now={now} />
          ))}
        </ol>
      )}
    </section>
  );
}

/** A time and a sentence; the raw line opens under it for a bug report. */
function LogEntry({ line, now }: { line: HostLogLine; now: number }) {
  const t = useT();
  const locale = useLocale();
  const number = useNumber();
  const sentence = logSentence(line, number);
  return (
    <li className="host-log__entry">
      <details className="host-log__details">
        <summary className="host-log__summary">
          <Icon name="chevronRight" size={16} className="host-log__chevron" />
          <time className="host-log__time tabular" dateTime={line.timestamp}>
            {clockText(line.timestamp, now, locale)}
          </time>
          <span className={sentence.known ? "host-log__text" : "host-log__text host-log__text--unknown"}>{t(sentence.key, sentence.params)}</span>
        </summary>
        <pre className="ui-details__raw host-raw host-log__raw">{JSON.stringify(line, null, 2)}</pre>
      </details>
    </li>
  );
}
