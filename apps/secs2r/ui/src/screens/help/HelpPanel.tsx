// 도움말: a panel at the right edge that leaves the screen usable (upstream
// help_dialog.py was a non-modal 640×720 window). It follows the frame's
// step: title "도움말 · <단계>", the step chip "k / 5", search (Ctrl+F;
// Enter next match, Shift+Enter previous; Esc closes), the step's tabs and
// the text. The parent places it: it fills the right side of the nearest
// positioned ancestor (the app root, or a gallery stage).
import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { STEPS, STEP_TITLE } from "../../app/model";
import { isMac } from "../../app/shortcuts";
import type { MessageKey } from "../../i18n";
import { useT, type Translate } from "../../i18n/react";
import { Button, IconButton, StatusPill } from "../../ui";
import { HELP_TABS, MDAT_CHANNEL_ROWS } from "./content";
import type { HelpActions, HelpBlock, HelpTab, HelpTabId, HelpView } from "./types";
import "./help.css";

export const TAB_LABEL: Record<HelpTabId, MessageKey> = {
  intro: "help.tab.intro",
  steps: "help.tab.steps",
  checks: "help.tab.checks",
  trouble: "help.tab.trouble",
  real: "help.tab.real",
  mdat: "help.tab.mdat",
  camilla: "help.tab.camilla",
};

/** Every visible string of a tab, in reading order (search counts these). */
export function tabTexts(t: Translate, tab: HelpTab): string[] {
  return tab.blocks.flatMap((block): string[] => {
    switch (block.kind) {
      case "lead":
      case "heading":
        return [t(block.text)];
      case "paragraphs":
      case "bullets":
      case "numbered":
      case "code":
        return block.items.map((key) => t(key));
      case "steps":
        return block.items.flatMap((item) => [t(item.do), t(item.see)]);
      case "pairs":
        return block.items.flatMap((item) => [t(item.term), t(item.text)]);
      case "channels":
        return [
          t("help.measure.mdat.table.group"),
          t("help.measure.mdat.table.use"),
          t("help.measure.mdat.table.also"),
          ...MDAT_CHANNEL_ROWS.flatMap((row) => [t(row.group), row.use, row.also]),
        ];
      case "tutorial":
        return [];
    }
  });
}

/** Non-overlapping, case-insensitive occurrences of `query` in `text`. */
export function countMatches(text: string, query: string): number {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return 0;
  const hay = text.toLocaleLowerCase();
  let count = 0;
  for (let at = hay.indexOf(needle); at >= 0; at = hay.indexOf(needle, at + needle.length)) count += 1;
  return count;
}

export function HelpPanel({
  view,
  actions,
  initialTab,
  initialQuery = "",
  initialMatch = 0,
}: {
  view: HelpView;
  actions: HelpActions;
  /** Gallery and tests: the tab to open first (ignored if the step has no such tab). */
  initialTab?: HelpTabId;
  initialQuery?: string;
  initialMatch?: number;
}) {
  const t = useT();
  const titleId = useId();
  const searchId = useId();
  const panelRef = useRef<HTMLElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const tabRefs = useRef<Partial<Record<HelpTabId, HTMLButtonElement | null>>>({});
  const tabs = HELP_TABS[view.step];
  const first = tabs[0]?.id ?? "steps";

  // The tab belongs to the step it was chosen on: a new step opens on its first tab.
  const [chosen, setChosen] = useState<{ step: string; tab: HelpTabId }>({ step: view.step, tab: initialTab ?? first });
  const active = chosen.step === view.step && tabs.some((tab) => tab.id === chosen.tab) ? chosen.tab : first;
  const tab = tabs.find((x) => x.id === active) ?? tabs[0];
  const [query, setQuery] = useState(initialQuery);
  const [match, setMatch] = useState(initialMatch);
  const [frameGap, setFrameGap] = useState<{ top: number; bottom: number } | null>(null);

  const hits = useMemo(() => {
    const out: Partial<Record<HelpTabId, number>> = {};
    for (const x of tabs) out[x.id] = tabTexts(t, x).reduce((sum, text) => sum + countMatches(text, query), 0);
    return out;
  }, [tabs, t, query]);
  const total = hits[active] ?? 0;
  const elsewhere = Object.entries(hits).reduce((sum, [id, n]) => (id === active ? sum : sum + (n ?? 0)), 0);
  const current = total > 0 ? ((match % total) + total) % total : -1;

  const choose = (id: HelpTabId) => {
    setChosen({ step: view.step, tab: id });
    setMatch(0);
    bodyRef.current?.scrollTo({ top: 0 });
  };

  // Beside a frame, the panel sits between the step header and the job tray,
  // so the step's primary action and the tray's cancel stay in reach.
  useLayoutEffect(() => {
    const panel = panelRef.current;
    const parent = panel?.offsetParent;
    if (!panel || !(parent instanceof HTMLElement)) return;
    const header = parent.querySelector<HTMLElement>('[data-ui="frame.header"]');
    const tray = parent.querySelector<HTMLElement>('[data-ui="jobs.tray"]');
    if (!header) return;
    const update = () => {
      const box = parent.getBoundingClientRect();
      const top = Math.round(header.getBoundingClientRect().bottom - box.top + 8);
      const bottom = tray ? Math.round(box.bottom - tray.getBoundingClientRect().top + 8) : 12;
      setFrameGap((prev) => (prev && prev.top === top && prev.bottom === bottom ? prev : { top, bottom }));
    };
    update();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(update);
    observer.observe(header);
    if (tray) observer.observe(tray);
    return () => observer.disconnect();
  }, []);

  // Focus moves into the panel when it opens and back when it closes.
  useEffect(() => {
    const before = document.activeElement;
    panelRef.current?.focus({ preventScroll: true });
    return () => {
      if (before instanceof HTMLElement && panelRef.current?.contains(document.activeElement)) before.focus({ preventScroll: true });
    };
  }, []);

  // Ctrl+F (⌘F) finds in help while the panel is open.
  useEffect(() => {
    const onKey = (event: globalThis.KeyboardEvent) => {
      const mod = isMac() ? event.metaKey : event.ctrlKey;
      if (mod && !event.shiftKey && !event.altKey && (event.code === "KeyF" || event.key.toLowerCase() === "f")) {
        event.preventDefault();
        inputRef.current?.focus();
        inputRef.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // Keep the current match in view inside the body (never scroll the page).
  useLayoutEffect(() => {
    const body = bodyRef.current;
    const mark = body?.querySelector<HTMLElement>("[data-current-match]");
    if (!body || !mark) return;
    const top = mark.getBoundingClientRect().top - body.getBoundingClientRect().top + body.scrollTop;
    if (top < body.scrollTop + 24 || top > body.scrollTop + body.clientHeight - 48) body.scrollTop = Math.max(0, top - body.clientHeight / 3);
  }, [current, active, query]);

  const step = (delta: 1 | -1) => {
    if (total > 0) {
      setMatch((m) => m + delta);
      return;
    }
    // Nothing here: go to the next tab that has the word.
    const order = tabs.map((x) => x.id);
    const start = order.indexOf(active);
    for (let i = 1; i < order.length; i++) {
      const id = order[(start + i * delta + order.length * 2) % order.length];
      if (id && (hits[id] ?? 0) > 0) {
        choose(id);
        return;
      }
    }
  };

  const onPanelKey = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      actions.close();
    }
  };

  const onTabKey = (event: KeyboardEvent<HTMLButtonElement>) => {
    const order = tabs.map((x) => x.id);
    const index = order.indexOf(active);
    const next = { ArrowRight: index + 1, ArrowLeft: index - 1, Home: 0, End: order.length - 1 }[event.key];
    if (next === undefined) return;
    event.preventDefault();
    const id = order[(next + order.length) % order.length];
    if (id) {
      choose(id);
      tabRefs.current[id]?.focus();
    }
  };

  const stepIndex = STEPS.indexOf(view.step) + 1;
  const stepName = t(STEP_TITLE[view.step]);
  const countText = !query.trim()
    ? ""
    : total > 0
      ? t("help.search.count", { current: current + 1, total })
      : elsewhere > 0
        ? t("help.search.elsewhere", { count: elsewhere })
        : t("help.search.none");

  return (
    <aside ref={panelRef} className="help-panel" style={frameGap ?? undefined} data-ui="help" aria-labelledby={titleId} tabIndex={-1} onKeyDown={onPanelKey}>
      <header className="help-panel__head">
        <div className="help-panel__titles">
          <h2 className="help-panel__title" id={titleId}>
            {t("help.title", { step: stepName })}
          </h2>
          <span className="help-panel__chip tabular" data-ui="help.step" title={t("help.step.label", { n: stepIndex, total: STEPS.length })}>
            {t("help.step", { n: stepIndex, total: STEPS.length })}
          </span>
        </div>
        <IconButton icon="close" label={t("help.close")} data-ui="help.close" onClick={actions.close} />
      </header>

      <div className="help-search">
        <label className="help-search__label" htmlFor={searchId}>
          {t("help.search")}
        </label>
        <div className="help-search__box">
          <input
            ref={inputRef}
            id={searchId}
            data-ui="help.search"
            className="help-search__input"
            type="search"
            value={query}
            placeholder={t("help.search.placeholder")}
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => {
              setQuery(event.target.value);
              setMatch(0);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                step(event.shiftKey ? -1 : 1);
              }
            }}
          />
        </div>
        <span className="help-search__count tabular" aria-live="polite">
          {countText}
        </span>
        <IconButton icon="chevronDown" size="sm" label={t("help.search.next")} data-ui="help.search.next" disabled={total + elsewhere === 0} onClick={() => step(1)} />
      </div>

      <div className="help-tabs" role="tablist" aria-label={t("help.tabs")} data-ui="help.tabs">
        {tabs.map((x) => {
          const selected = x.id === active;
          const n = hits[x.id] ?? 0;
          return (
            <button
              key={x.id}
              ref={(node) => {
                tabRefs.current[x.id] = node;
              }}
              type="button"
              role="tab"
              id={`help.tab.${x.id}`}
              data-ui={`help.tab.${x.id}`}
              aria-selected={selected}
              aria-controls={`help.panel.${x.id}`}
              tabIndex={selected ? 0 : -1}
              className="help-tab"
              onClick={() => choose(x.id)}
              onKeyDown={onTabKey}
            >
              <span>{t(TAB_LABEL[x.id])}</span>
              {query.trim() && n > 0 && (
                <span className="help-tab__hits tabular" aria-label={t("help.tab.hits", { count: n })}>
                  {n}
                </span>
              )}
            </button>
          );
        })}
      </div>

      <div ref={bodyRef} className="help-panel__body" role="tabpanel" id={`help.panel.${active}`} aria-labelledby={`help.tab.${active}`} tabIndex={0}>
        {tab && <TabBody tab={tab} step={view.step} query={query} current={current} tutorial={view.tutorial} actions={actions} />}
      </div>

      <footer className="help-panel__foot">
        <span className="ui-kbd">{t("help.keys", { find: isMac() ? "⌘F" : "Ctrl+F" })}</span>
      </footer>
    </aside>
  );
}

function TabBody({
  tab,
  step,
  query,
  current,
  tutorial,
  actions,
}: {
  tab: HelpTab;
  step: string;
  query: string;
  current: number;
  tutorial: boolean;
  actions: HelpActions;
}) {
  const t = useT();
  const needle = query.trim().toLocaleLowerCase();
  // Matches are numbered in reading order as the blocks render.
  let index = 0;
  const mark = (text: string): ReactNode => {
    if (!needle) return text;
    const lower = text.toLocaleLowerCase();
    const parts: ReactNode[] = [];
    let from = 0;
    for (let at = lower.indexOf(needle); at >= 0; at = lower.indexOf(needle, at + needle.length)) {
      if (at > from) parts.push(text.slice(from, at));
      const here = index === current;
      parts.push(
        <mark key={at} className={here ? "help-mark help-mark--current" : "help-mark"} data-current-match={here || undefined}>
          {text.slice(at, at + needle.length)}
        </mark>,
      );
      index += 1;
      from = at + needle.length;
    }
    if (from === 0) return text;
    if (from < text.length) parts.push(text.slice(from));
    return parts;
  };

  const block = (b: HelpBlock, i: number): ReactNode => {
    switch (b.kind) {
      case "lead":
        return (
          <p key={i} className="help-lead">
            {mark(t(b.text))}
          </p>
        );
      case "heading":
        return (
          <h3 key={i} className="help-heading">
            {mark(t(b.text))}
          </h3>
        );
      case "paragraphs":
        return (
          <div key={i} className="help-paragraphs">
            {b.items.map((key) => (
              <p key={key}>{mark(t(key))}</p>
            ))}
          </div>
        );
      case "bullets":
        return (
          <ul key={i} className="help-bullets">
            {b.items.map((key) => (
              <li key={key}>{mark(t(key))}</li>
            ))}
          </ul>
        );
      case "numbered":
        return (
          <ol key={i} className="help-numbered">
            {b.items.map((key) => (
              <li key={key}>{mark(t(key))}</li>
            ))}
          </ol>
        );
      case "steps":
        return (
          <ol key={i} className="help-steps">
            {b.items.map((item, n) => (
              <li key={item.do} className="help-steps__item">
                <span className="help-steps__n tabular" aria-hidden="true">
                  {n + 1}
                </span>
                <div className="help-steps__text">
                  <p className="help-steps__do">{mark(t(item.do))}</p>
                  <p className="help-steps__see">{mark(t(item.see))}</p>
                </div>
              </li>
            ))}
          </ol>
        );
      case "pairs":
        return (
          <dl key={i} className="help-pairs">
            {b.items.map((item) => (
              <div key={item.term} className="help-pairs__item">
                <dt>{mark(t(item.term))}</dt>
                <dd>{mark(t(item.text))}</dd>
              </div>
            ))}
          </dl>
        );
      case "code":
        return (
          <ul key={i} className="help-code">
            {b.items.map((key) => {
              const [label, command] = splitCommand(t(key));
              return (
                <li key={key}>
                  {label && <span className="help-code__label">{mark(label)}</span>}
                  <code className="help-code__command">{mark(command)}</code>
                </li>
              );
            })}
          </ul>
        );
      case "channels":
        return (
          <div key={i} className="help-table-wrap">
            <table className="help-table">
              <caption className="visually-hidden">{t("help.measure.mdat.table")}</caption>
              <thead>
                <tr>
                  <th scope="col">{mark(t("help.measure.mdat.table.group"))}</th>
                  <th scope="col">{mark(t("help.measure.mdat.table.use"))}</th>
                  <th scope="col">{mark(t("help.measure.mdat.table.also"))}</th>
                </tr>
              </thead>
              <tbody>
                {MDAT_CHANNEL_ROWS.map((row) => (
                  <tr key={row.group}>
                    <th scope="row">{mark(t(row.group))}</th>
                    <td className="help-table__names">{mark(row.use)}</td>
                    <td className="help-table__also">{mark(row.also)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        );
      case "tutorial":
        return (
          <div key={i} className="help-tutorial">
            {tutorial ? (
              <StatusPill tone="info" data-ui="help.intro.tutorial">
                {t("help.intro.tutorial_active")}
              </StatusPill>
            ) : (
              <Button variant="secondary" icon="play" data-ui="help.intro.tutorial" onClick={actions.startTutorial}>
                {t("help.intro.tutorial")}
              </Button>
            )}
          </div>
        );
    }
  };

  return (
    <div className="help-doc" data-ui={`help.${step}.${tab.id}`}>
      {tab.blocks.map(block)}
    </div>
  );
}

/** "엔진, Windows: bin\camilladsp.exe …" → label and command (the command keeps any further colons). */
function splitCommand(text: string): [string, string] {
  const at = text.indexOf(": ");
  return at < 0 ? ["", text] : [text.slice(0, at), text.slice(at + 2)];
}
