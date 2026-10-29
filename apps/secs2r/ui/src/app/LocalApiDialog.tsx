// The local API (ADR 0016) on screen: its row in the settings popover and
// the 로컬 API dialog.
//
// Row: a switch (로컬 API), its state on one line (꺼짐, 127.0.0.1:4712에서
// 받는 중, or why it is not listening) and a text button that opens the
// dialog (포트 바꾸기… when the port is taken, which lands on the port).
//
// Dialog: what a program with the token can do, then the state, the
// address, the port (applied with its own button), the token (hidden until
// asked, copied either way, replaced only after a one-line confirm), the
// token file, and a request reference to hand to whoever writes the
// client. 닫기 is the one filled action.
import { useEffect, useId, useRef, useState, type KeyboardEvent, type Ref, type RefObject } from "react";
import type { MessageKey } from "../i18n";
import { useT, type Translate } from "../i18n/react";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Icon, Spinner } from "../ui/Icon";
import { Toggle } from "../ui/controls";
import { ErrorNotice } from "../ui/states";
import { errorText } from "./jobs/jobText";
import {
  PORT_MAX,
  PORT_MIN,
  localApiState,
  parsePort,
  portProblem,
  referenceText,
  type LocalApi,
  type LocalApiState,
  type LocalApiView,
  type PortProblem,
} from "./localApi";
import "./localApi.css";

/** Where focus goes when the dialog opens: 닫기, or the port field (from 포트 바꾸기…). */
export type LocalApiFocus = "info" | "port";

/** Dialog states the gallery draws without clicking through (token shown, confirm open, a typed port). */
export type LocalApiDialogInitial = { tokenShown?: boolean; confirming?: boolean; portDraft?: string };

/** The hidden token: a fixed run of dots, so the length says nothing either. */
const MASK = "•".repeat(16);

/* ── State line ─────────────────────────────────────────────────────────── */

const STATE_TEXT: Record<Exclude<LocalApiState["kind"], "listening" | "port_busy">, MessageKey> = {
  unknown: "frame.local_api.state.unknown",
  unreadable: "frame.local_api.state.unreadable",
  off: "frame.local_api.state.off",
  turning_on: "frame.local_api.state.turning_on",
  turning_off: "frame.local_api.state.turning_off",
  failed: "frame.local_api.state.failed",
};

export function localApiStateText(t: Translate, state: LocalApiState): string {
  if (state.kind === "listening") return t("frame.local_api.state.listening", { address: state.address });
  if (state.kind === "port_busy") return t("frame.local_api.state.port_busy", { port: state.port });
  return t(STATE_TEXT[state.kind]);
}

type Glyph = "on" | "off" | "busy" | "problem";

const GLYPH: Record<LocalApiState["kind"], Glyph> = {
  unknown: "busy",
  unreadable: "problem",
  off: "off",
  turning_on: "busy",
  turning_off: "busy",
  listening: "on",
  port_busy: "problem",
  failed: "problem",
};

/** The state in words with a glyph beside it: a filled dot (on, or dimmed off), a spinner, or a warning sign. */
export function LocalApiStateLine({ view, id }: { view: LocalApiView; id?: string }) {
  const t = useT();
  const state = localApiState(view);
  const glyph = GLYPH[state.kind];
  return (
    <span className={`local-api-state local-api-state--${glyph}`} id={id} data-ui={id} data-state={state.kind} role="status">
      <span className="local-api-state__glyph" aria-hidden="true">
        {glyph === "busy" ? <Spinner size={14} /> : glyph === "problem" ? <Icon name="warning" size={16} /> : <span className="local-api-state__dot" />}
      </span>
      <span className="local-api-state__text">{localApiStateText(t, state)}</span>
    </span>
  );
}

function CallError({ view, id }: { view: LocalApiView; id: string }) {
  const t = useT();
  if (!view.error) return null;
  const text = errorText(t, view.error);
  return <ErrorNotice id={id} cause={text.cause} next={text.next} detail={view.error.detail} code={view.error.code} />;
}

/* ── Popover row ────────────────────────────────────────────────────────── */

export function LocalApiRow({ api, onOpen }: { api: LocalApi; onOpen: (focus: LocalApiFocus) => void }) {
  const t = useT();
  const { view, actions } = api;
  const toPort = localApiState(view).kind === "port_busy";
  return (
    <div className="local-api-row" data-ui="frame.settings.local_api.row">
      <div className="local-api-row__line">
        <Toggle
          id="frame.settings.local_api"
          label={t("frame.settings.local_api")}
          checked={view.status?.enabled ?? false}
          disabled={view.status === null}
          hint={<LocalApiStateLine view={view} />}
          // The switch shows the saved value; a second press while the first
          // is saved is dropped rather than queued behind it.
          onChange={(on) => {
            if (view.pending === null) actions.setEnabled(on);
          }}
        />
        <Button variant="quiet" size="sm" className="local-api-row__open" data-ui="frame.settings.local_api.open" onClick={() => onOpen(toPort ? "port" : "info")}>
          {t(toPort ? "frame.settings.local_api.change_port" : "frame.settings.local_api.open")}
        </Button>
      </div>
      <CallError view={view} id="frame.settings.local_api.error" />
    </div>
  );
}

/* ── Dialog ─────────────────────────────────────────────────────────────── */

function CopyButton({ label, ui, text, copy, buttonRef }: { label: string; ui: string; text: string; copy: (text: string) => void; buttonRef?: Ref<HTMLButtonElement> }) {
  const t = useT();
  return (
    <Button ref={buttonRef} variant="quiet" size="sm" aria-label={label} data-ui={ui} disabled={text === ""} onClick={() => copy(text)}>
      {t("frame.local_api.copy")}
    </Button>
  );
}

export function LocalApiDialog({
  api,
  onClose,
  focus = "info",
  inline,
  initial,
}: {
  api: LocalApi;
  onClose: () => void;
  focus?: LocalApiFocus;
  /** Render in place without a backdrop (gallery specimens). */
  inline?: boolean;
  initial?: LocalApiDialogInitial;
}) {
  const t = useT();
  const { view, actions } = api;
  const status = view.status;
  const state = localApiState(view);
  const port = useRef<HTMLInputElement>(null);

  // The kit dialog focuses 닫기 in its own effect, which runs before this
  // one (a child's effects run first); 포트 바꾸기… then moves on to the port.
  useEffect(() => {
    if (focus !== "port" || inline) return;
    port.current?.focus();
    port.current?.select();
  }, [focus, inline]);

  return (
    <Dialog
      open
      inline={inline}
      id="frame.local_api"
      onClose={onClose}
      title={t("frame.settings.local_api")}
      footer={
        <Button variant="primary" data-ui="frame.local_api.close" data-autofocus onClick={onClose}>
          {t("ui.close")}
        </Button>
      }
    >
      <div className="local-api">
        <p className="local-api__intro">{t("frame.local_api.intro")}</p>
        <CallError view={view} id="frame.local_api.error" />
        <dl className="local-api__fields">
          <div className="local-api__entry">
            <dt className="local-api__key">{t("frame.local_api.status")}</dt>
            <dd className="local-api__value local-api__value--wide local-api__stack">
              <LocalApiStateLine view={view} id="frame.local_api.state" />
              {state.kind === "failed" && (
                <>
                  <p className="local-api__note">{t("frame.local_api.state.failed.next")}</p>
                  {state.detail && (
                    <details className="ui-details">
                      <summary>{t("ui.details")}</summary>
                      <pre className="ui-details__raw">{state.detail}</pre>
                    </details>
                  )}
                </>
              )}
            </dd>
          </div>
          {status && (
            <>
              <div className="local-api__entry">
                <dt className="local-api__key">{t("frame.local_api.address")}</dt>
                <dd className="local-api__value">
                  <code className="local-api__text" data-ui="frame.local_api.address">{status.url}</code>
                </dd>
                <dd className="local-api__actions">
                  <CopyButton label={t("frame.local_api.address.copy")} ui="frame.local_api.address.copy" text={status.url} copy={actions.copy} />
                </dd>
              </div>
              <PortEntry
                saved={status.port}
                busy={view.pending === "port"}
                taken={state.kind === "port_busy"}
                setPort={actions.setPort}
                input={port}
                draft={initial?.portDraft}
              />
              <TokenEntry token={status.token} busy={view.pending === "token"} actions={actions} initial={initial} />
              <div className="local-api__entry">
                <dt className="local-api__key">{t("frame.local_api.token_file")}</dt>
                <dd className="local-api__value">
                  <code className="local-api__text" data-ui="frame.local_api.token_file">{status.token_path}</code>
                </dd>
                <dd className="local-api__actions">
                  <CopyButton label={t("frame.local_api.token_file.copy")} ui="frame.local_api.token_file.copy" text={status.token_path} copy={actions.copy} />
                </dd>
              </div>
            </>
          )}
        </dl>
        {status && <Reference url={status.url} copy={actions.copy} />}
      </div>
    </Dialog>
  );
}

const PROBLEM_TEXT: Record<PortProblem, MessageKey> = {
  empty: "frame.local_api.port.empty",
  not_number: "frame.local_api.port.not_number",
  out_of_range: "frame.local_api.port.out_of_range",
};

/**
 * The port: typed freely, checked as typed (why it cannot be applied shows
 * under the field), applied with 포트 바꾸기 or Enter. Once a new saved
 * port arrives (this change landed, or another program changed it) the
 * field shows it again, without leaving the field.
 */
function PortEntry({
  saved,
  busy,
  taken,
  setPort,
  input,
  draft: initialDraft,
}: {
  saved: number;
  busy: boolean;
  /** The saved port is taken: the hint says what to do instead of giving the range. */
  taken: boolean;
  setPort: (port: number) => void;
  input: RefObject<HTMLInputElement | null>;
  draft: string | undefined;
}) {
  const t = useT();
  const errorId = useId();
  const hintId = useId();
  const [draft, setDraft] = useState<string | null>(initialDraft ?? null);
  const [shownSaved, setShownSaved] = useState(saved);
  if (shownSaved !== saved) {
    setShownSaved(saved);
    setDraft(null);
  }
  const text = draft ?? String(saved);
  const problem = draft === null ? null : portProblem(draft);
  const next = parsePort(text);
  const changed = next !== null && next !== saved;
  const range = t("ui.number.range", { min: PORT_MIN, max: PORT_MAX, unit: "" }).trim();
  const error = problem ? t(PROBLEM_TEXT[problem], { range }) : null;
  const hint = error ? null : taken && !changed ? t("frame.local_api.port.busy_hint") : range;
  const apply = () => {
    if (changed && !busy) setPort(next);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      apply();
    } else if (event.key === "Escape" && draft !== null && draft !== String(saved)) {
      // The first Escape puts the saved port back; the next one closes the dialog.
      event.stopPropagation();
      setDraft(null);
    }
  };
  return (
    <div className="local-api__entry">
      <dt className="local-api__key">
        <label htmlFor="frame.local_api.port">{t("frame.local_api.port")}</label>
      </dt>
      <dd className="local-api__value local-api__value--wide local-api__stack">
        <div className="local-api__port">
          <div className={["ui-number", error && "ui-number--invalid"].filter(Boolean).join(" ")}>
            <input
              ref={input}
              id="frame.local_api.port"
              data-ui="frame.local_api.port"
              className="ui-number__input tabular"
              type="text"
              inputMode="numeric"
              autoComplete="off"
              spellCheck={false}
              size={6}
              value={text}
              aria-invalid={error ? true : undefined}
              aria-describedby={error ? errorId : hintId}
              onChange={(event) => setDraft(event.target.value)}
              onKeyDown={onKeyDown}
            />
          </div>
          <Button data-ui="frame.local_api.port.apply" disabled={!changed} busy={busy} onClick={apply}>
            {t("frame.local_api.port.apply")}
          </Button>
        </div>
        {error && (
          <p className="ui-field__error" id={errorId}>
            <Icon name="error" size={16} />
            <span>{error}</span>
          </p>
        )}
        {hint && (
          <p className="ui-field__hint" id={hintId}>
            {hint}
          </p>
        )}
      </dd>
    </div>
  );
}

/**
 * The token: dots until 보기, copied either way. 새 토큰 만들기 first asks
 * in one line (programs using the old token need the new one); Escape or
 * 취소 there leaves the token as it is.
 */
function TokenEntry({
  token,
  busy,
  actions,
  initial,
}: {
  token: string;
  busy: boolean;
  actions: LocalApi["actions"];
  initial: LocalApiDialogInitial | undefined;
}) {
  const t = useT();
  const confirmId = useId();
  const [shown, setShown] = useState(initial?.tokenShown ?? false);
  const [confirming, setConfirming] = useState(initial?.confirming ?? false);
  const regenerate = useRef<HTMLButtonElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const copy = useRef<HTMLButtonElement>(null);
  // Where focus goes once the confirm line opens or closes (the button that
  // had it is gone): 취소 when it opens, back to 새 토큰 만들기 when it is
  // left, and on to 복사 once the new token is asked for.
  const focusNext = useRef<"cancel" | "regenerate" | "copy" | null>(null);
  useEffect(() => {
    const target = focusNext.current;
    focusNext.current = null;
    if (target === "cancel") cancel.current?.focus();
    else if (target === "regenerate") regenerate.current?.focus();
    else if (target === "copy") copy.current?.focus();
  }, [confirming]);

  const ask = () => {
    focusNext.current = "cancel";
    setConfirming(true);
  };
  const leave = () => {
    focusNext.current = "regenerate";
    setConfirming(false);
  };
  const confirm = () => {
    focusNext.current = "copy";
    setConfirming(false);
    actions.regenerateToken();
  };

  return (
    <div className="local-api__entry">
      <dt className="local-api__key">{t("frame.local_api.token")}</dt>
      <dd className="local-api__value">
        {shown ? (
          <code className="local-api__text local-api__text--token" data-ui="frame.local_api.token.value">
            {token}
          </code>
        ) : (
          <span className="local-api__mask" data-ui="frame.local_api.token.value">
            <span aria-hidden="true">{MASK}</span>
            <span className="visually-hidden">{t("frame.local_api.token.hidden")}</span>
          </span>
        )}
      </dd>
      <dd className="local-api__actions">
        <CopyButton label={t("frame.local_api.token.copy")} ui="frame.local_api.token.copy" text={token} copy={actions.copy} buttonRef={copy} />
        <Button
          variant="quiet"
          size="sm"
          data-ui="frame.local_api.token.show"
          aria-label={t(shown ? "frame.local_api.token.hide.label" : "frame.local_api.token.show.label")}
          disabled={token === ""}
          onClick={() => setShown((value) => !value)}
        >
          {t(shown ? "frame.local_api.token.hide" : "frame.local_api.token.show")}
        </Button>
      </dd>
      <dd className="local-api__value local-api__value--wide">
        {confirming ? (
          <div
            className="local-api__confirm"
            role="group"
            aria-labelledby={confirmId}
            data-ui="frame.local_api.token.confirm_line"
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.stopPropagation();
                leave();
              }
            }}
          >
            <p id={confirmId} className="local-api__confirm-text">
              {t("frame.local_api.token.confirm")}
            </p>
            <div className="local-api__confirm-actions">
              <Button ref={cancel} variant="quiet" size="sm" data-ui="frame.local_api.token.cancel" onClick={leave}>
                {t("ui.dialog.cancel")}
              </Button>
              <Button size="sm" data-ui="frame.local_api.token.confirm" onClick={confirm}>
                {t("frame.local_api.token.regenerate")}
              </Button>
            </div>
          </div>
        ) : (
          <Button ref={regenerate} size="sm" data-ui="frame.local_api.token.regenerate" busy={busy} onClick={ask}>
            {t("frame.local_api.token.regenerate")}
          </Button>
        )}
      </dd>
    </div>
  );
}

/** The request reference: plain text, the same on screen and on the clipboard. */
function Reference({ url, copy }: { url: string; copy: (text: string) => void }) {
  const t = useT();
  const headingId = useId();
  const text = referenceText(url, { what: (key) => t(key), auth: t("frame.local_api.reference.auth") });
  return (
    <section className="local-api__reference" aria-labelledby={headingId}>
      <div className="local-api__reference-head">
        <h3 className="local-api__heading" id={headingId}>
          {t("frame.local_api.reference")}
        </h3>
        <CopyButton label={t("frame.local_api.reference.copy")} ui="frame.local_api.reference.copy" text={text} copy={copy} />
      </div>
      <pre className="local-api__code" data-ui="frame.local_api.reference">
        {text}
      </pre>
    </section>
  );
}
