import { HelpTip } from "../../ui/HelpTip";
// 시스템: speakers, outputs, the export DSP and the measurement arrangement,
// decided together before anything is connected (upstream steps 2 and 4).
//
// Cards: 구성 (presets, sub count, the output patch sheet, the problems
// config_validate reports), 방 배치도 (the room map synced with the sheet and
// the chosen speaker's wiring), 측정 배치 (arrangement, the time estimate,
// the timing reference) and 내보낼 DSP (U2). The start delay and the sweep
// length are set on 측정·가져오기 only (P58). Changes that clear
// measurements ask first and offer undo (U5).
import { useState, type KeyboardEvent, type ReactNode } from "react";
import type { PrimaryAction } from "../../app/StepHeader";
import type { Translate } from "../../i18n/react";
import { useNumber, useT } from "../../i18n/react";
import type { ConfigSpeaker } from "../../ipc/bindings/ConfigSpeaker";
import type { DspFields } from "../../ipc/bindings/DspFields";
import type { OutputRateChoice } from "../../ipc/bindings/OutputRateChoice";
import {
  Advanced,
  ArrangementMap,
  ArrangementThumb,
  Button,
  Card,
  CellSelect,
  DataTable,
  Icon,
  IconButton,
  NumberField,
  RoomMap,
  Select,
  StatusPill,
  Stepper,
  useConfirmAndUndo,
  type Column,
  type Option,
  type RowGroup,
} from "../../ui";
import { ARRANGEMENT_COUNTS, PRESET_ROLES, ROLES, orderRoles, roleCode, roleKind, type MicPoint } from "../../ui/maps/geometry";
import { OUTPUT_RATES, countedRate, rateChoice, rateText, rateValue } from "./outputRate";
import { DSP_NAME, GROUP_NAME, durationText, isKnownRole, noticeText, outputLabel, roleLabel, roleName } from "./text";
import type { SystemActions, SystemView } from "./types";
import "./system.css";

/** Upstream preset buttons (model.PRESETS) with the 4.0, 4.1, 4.2 and 7.1 presets SECS 2R adds (U13), in order.
 * Grouped as stereo, quad, surround and immersive; a narrow row wraps between groups. */
const PRESET_GROUPS = [
  ["2.0", "2.1", "2.2"],
  ["4.0", "4.1", "4.2"],
  ["5.1", "7.1"],
  ["7.1.4", "9.1.6"],
] as const;
const PRESETS = PRESET_GROUPS.flat();

/** Upstream FIR_TAP_CHOICES (step2_config.py). */
const FIR_TAP_CHOICES = [512, 1024, 2048, 4096, 8192, 16384, 32768, 65536] as const;

const DSP_ORDER = ["camilla", "tesira", "qsys", "minidsp", "custom"] as const;

const ARRANGEMENT_NAME = {
  1: "system.arrangement.1",
  9: "system.arrangement.9",
  13: "system.arrangement.13",
  17: "system.arrangement.17",
} as const;

/** A change that clears measurements (U5). */
type SystemChange = { kind: "preset"; preset: string } | { kind: "subs"; count: number } | { kind: "arrangement"; count: number };

/** The confirm dialog's words for a change: title names the action, the body says what is lost and that it can be undone. */
export function changeCopy(t: Translate, change: SystemChange, positions: number) {
  const body = t("system.confirm.body", { count: positions });
  const confirm = t("system.confirm.action");
  switch (change.kind) {
    case "preset":
      return change.preset === "custom"
        ? { title: t("system.confirm.empty.title"), body, confirm, done: t("system.confirm.empty.done") }
        : { title: t("system.confirm.preset.title", { preset: change.preset }), body, confirm, done: t("system.confirm.preset.done", { preset: change.preset }) };
    case "subs":
      return { title: t("system.confirm.subs.title", { count: change.count }), body, confirm, done: t("system.confirm.subs.done", { count: change.count }) };
    case "arrangement": {
      const name = t(ARRANGEMENT_NAME[change.count as keyof typeof ARRANGEMENT_NAME] ?? "system.arrangement.9");
      return { title: t("system.confirm.arrangement.title", { name }), body, confirm, done: t("system.confirm.arrangement.done", { name }) };
    }
  }
}

const NO_UNDO = Symbol("no undo token");

/** Ask before a change that clears measurements, run it, then offer undo. Without measurements it runs at once. */
function useClearingChange(view: SystemView, actions: SystemActions) {
  const t = useT();
  const [change, setChange] = useState<SystemChange | null>(null);
  const positions = view.positionsDone.length;
  const perform = (next: SystemChange) =>
    next.kind === "preset" ? actions.applyPreset(next.preset) : next.kind === "subs" ? actions.setSubCount(next.count) : actions.setArrangement(next.count);
  const copy = change ? changeCopy(t, change, positions) : null;
  const { request, dialog } = useConfirmAndUndo({
    id: "system.confirm",
    title: copy?.title ?? "",
    body: copy?.body ?? "",
    confirmLabel: copy?.confirm ?? "",
    done: copy?.done ?? "",
    run: async () => {
      if (!change) throw NO_UNDO;
      const token = await perform(change);
      if (!token) throw NO_UNDO;
      return token;
    },
    undo: actions.undo,
    // Failures reach the user through P33's error path; a change that cleared nothing has no undo.
    onError: () => {},
  });
  const ask = (next: SystemChange) => {
    if (positions === 0) {
      void perform(next).catch(() => {});
      return;
    }
    setChange(next);
    request();
  };
  return { ask, dialog };
}

/** Keeps Enter and Space inside a cell control (the sheet's rows also answer to them). */
function CellKeys({ children }: { children: ReactNode }) {
  const keep = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Enter" || event.key === " ") event.stopPropagation();
  };
  return (
    <div className="system-cell" onKeyDown={keep}>
      {children}
    </div>
  );
}

export function SystemScreen({
  view,
  actions,
  initialDspAdvanced = false,
}: {
  view: SystemView;
  actions: SystemActions;
  /** Gallery: draw the DSP card's 고급 unfolded. The fold is the card's own state. */
  initialDspAdvanced?: boolean;
}) {
  const { ask, dialog } = useClearingChange(view, actions);
  const speakers = view.config.speakers;
  const [picked, setPicked] = useState<number | null>(speakers.length > 0 ? 0 : null);
  const selected = picked !== null && picked < speakers.length ? picked : null;
  return (
    <div className="system-screen" data-ui="system">
      <div className="system-grid">
        <ConfigCard view={view} actions={actions} ask={ask} selected={selected} onSelect={setPicked} />
        <RoomCard view={view} selected={selected} onSelect={setPicked} />
        <ArrangementCard view={view} actions={actions} ask={ask} />
        <DspCard view={view} actions={actions} initialAdvanced={initialDspAdvanced} />
      </div>
      {dialog}
    </div>
  );
}

/** The step header: the primary action opens 연결·점검 (disabled by the frame while the step is blocked). */
export function systemHeader(view: SystemView, actions: SystemActions, t: Translate): { primary?: PrimaryAction; status?: ReactNode } {
  // Upstream can_advance(): the step stays open to fix the setup, but the way
  // on waits for speakers and a valid configuration; the pill says why.
  const reason = view.config.speakers.length === 0 ? "frame.blocked.no_speakers.short" : view.notices.length > 0 ? "frame.blocked.config_invalid.short" : null;
  return {
    primary: { id: "system.next", label: t("frame.step.system.primary"), onClick: actions.next, disabled: reason !== null },
    ...(reason ? { status: <StatusPill tone="warning" data-ui="system.status">{t(reason)}</StatusPill> } : {}),
  };
}

/* ── 구성 ──────────────────────────────────────────────────────────────── */

type Row = { index: number; speaker: ConfigSpeaker };

function ConfigCard({
  view,
  actions,
  ask,
  selected,
  onSelect,
}: {
  view: SystemView;
  actions: SystemActions;
  ask: (change: SystemChange) => void;
  selected: number | null;
  onSelect: (index: number) => void;
}) {
  const t = useT();
  const { config } = view;
  const custom = !(PRESETS as readonly string[]).includes(config.preset);
  return (
    <Card className="system-config" title={t("system.config.title")} data-ui="system.config">
      <div className="system-config__top">
        <div className="system-presets" role="group" aria-labelledby="system.preset.label" data-ui="system.preset">
          <span className="ui-field__label" id="system.preset.label">
            {t("system.preset.label")}
          </span>
          <div className="system-presets__list">
            {PRESET_GROUPS.map((group) => (
              <div key={group[0]} className="system-presets__group">
                {group.map((preset) => {
                  const current = config.preset === preset;
                  return (
                    <button
                      key={preset}
                      type="button"
                      className="system-choice system-preset"
                      aria-pressed={current}
                      data-ui={`system.preset.${preset}`}
                      onClick={() => !current && ask({ kind: "preset", preset })}
                    >
                      <span className="system-preset__name tabular">{preset}</span>
                      <span className="system-preset__count tabular">{t("system.preset.count", { count: PRESET_ROLES[preset]?.length ?? 0 })}</span>
                    </button>
                  );
                })}
              </div>
            ))}
            {custom && (
              <span className="system-choice system-preset system-preset--custom" aria-current="true" data-ui="system.preset.custom">
                <span className="system-preset__name">{t("system.preset.custom")}</span>
                <span className="system-preset__count tabular">{t("system.preset.count", { count: config.speakers.length })}</span>
              </span>
            )}
          </div>
        </div>
        <Stepper
          id="system.subs"
          label={t("system.subs.label")}
          value={config.summary.sub_count}
          min={0}
          max={8}
          format={(count) => t("system.subs.value", { count })}
          onChange={(count) => ask({ kind: "subs", count })}
        />
      </div>
      <DeviceLine view={view} configure={actions.configureMeasurement} />
      {view.notices.length > 0 && <NoticeList view={view} />}
      <PatchTable view={view} actions={actions} selected={selected} onSelect={onSelect} />
      <Summary view={view} />
      <div className="system-actions">
        <Button icon="plus" data-ui="system.patch.add" onClick={actions.addSpeaker}>
          {t("system.patch.add")}
        </Button>
        <Button variant="quiet" data-ui="system.patch.renumber" onClick={actions.renumberOutputs}>
          {t("system.patch.renumber")}
        </Button>
        <div className="system-action-help">
          <Button variant="quiet" icon="save" data-ui="system.patch.csv" aria-describedby="system.patch.csv.help-content" onClick={actions.savePatchSheet}>
            {t("system.patch.csv")}
          </Button>
          <HelpTip id="system.patch.csv.help" label={t("system.patch.csv")} content={t("system.patch.csv.help")} />
        </div>
        <Button variant="quiet" data-ui="system.preset.empty" onClick={() => ask({ kind: "preset", preset: "custom" })}>
          {t("system.preset.empty")}
        </Button>
      </div>
    </Card>
  );
}

function DeviceLine({ view, configure }: { view: SystemView; configure: SystemActions["configureMeasurement"] }) {
  const t = useT();
  const state = view.outputDeviceState;
  if (state === "not_required") return null;
  const warning = state === "unselected" || state === "unavailable";
  const channels = view.outputChannels;
  const over = channels !== null && view.config.summary.max_channel > channels;
  const pill = state !== "selected" && state !== "default" ? null : channels === null ? (
      <StatusPill>{t("system.device.unknown")}</StatusPill>
    ) : over ? (
      <StatusPill tone="danger">{t("system.device.over", { count: channels })}</StatusPill>
    ) : (
      <StatusPill tone="success">{t("system.device.channels", { count: channels })}</StatusPill>
    );
  return (
    <div className="system-device" data-ui="system.device">
      <span className="system-device__label">{t("system.device.label")}</span>
      <span className={`system-device__name${warning ? " system-device__name--warning" : ""}`} data-ui="system.device.status">
        {warning && <Icon name="warning" size={16} />}
        {state === "unselected" ? t("system.device.none")
          : state === "unavailable" ? t("system.device.unavailable")
            : state === "default" ? t("connect.device.default")
              : state === "checking" ? t("system.device.checking")
                : state === "rew_unknown" ? t("system.device.rew_unknown")
                  : view.outputDeviceName ?? view.config.device}
      </span>
      {pill}
      {configure && (
        <Button variant="quiet" className="system-device__configure" data-ui="system.device.configure" onClick={configure}>
          {t("system.device.configure")}
        </Button>
      )}
    </div>
  );
}

function NoticeList({ view }: { view: SystemView }) {
  const t = useT();
  const [all, setAll] = useState(false);
  const shown = all ? view.notices : view.notices.slice(0, 4);
  const rest = view.notices.length - shown.length;
  return (
    <div className="system-notices" role="status" data-ui="system.notices">
      <p className="system-notices__title">
        <Icon name="warning" size={18} className="system-notices__icon" />
        <span>{t("system.notices.title", { count: view.notices.length })}</span>
      </p>
      <ul className="system-notices__list">
        {shown.map((notice, index) => {
          const text = noticeText(t, notice);
          return (
            <li key={`${notice.code}-${index}`} className="system-notices__item" data-ui={`system.notices.${notice.code}`}>
              <span className="system-notices__cause">{text.cause}</span>
              {text.next && <span className="system-notices__next">{text.next}</span>}
            </li>
          );
        })}
      </ul>
      {rest > 0 && (
        <Button variant="quiet" size="sm" data-ui="system.notices.more" onClick={() => setAll(true)}>
          {t("system.notices.more", { count: rest })}
        </Button>
      )}
    </div>
  );
}

/** The service stores output numbers 1..=256 (P74, the project's output rule), so no output select lists past it. */
export const OUTPUT_LIMIT = 256;

/**
 * Outputs 1…`count`, never past OUTPUT_LIMIT, and a stored value outside them
 * as one more option (`outside` names it), not a range up to it: a number in
 * an opened project file never decides how many options the screen draws.
 */
function outputChoices(
  t: Translate,
  count: number,
  stored: number,
  names: readonly string[],
  outside: (label: string) => string = (label) => label,
): Option<string>[] {
  const shown = Math.max(0, Math.min(count, OUTPUT_LIMIT));
  const options: Option<string>[] = Array.from({ length: shown }, (_, i) => ({ value: String(i + 1), label: outputLabel(t, i + 1, names) }));
  if (stored < 1 || stored > shown) options.push({ value: String(stored), label: outside(outputLabel(t, stored, names)) });
  return options;
}

function PatchTable({
  view,
  actions,
  selected,
  onSelect,
}: {
  view: SystemView;
  actions: SystemActions;
  selected: number | null;
  onSelect: (index: number) => void;
}) {
  const t = useT();
  const { config, outputChannels, outputNames } = view;
  const duplicates = new Set(config.summary.duplicate_outputs);
  // Upstream lists 1…channels (64 while the device is unknown) and adds the stored value when it is past the end.
  // An unknown device lists up to the largest stored output here, within OUTPUT_LIMIT.
  const count = outputChannels ?? Math.max(64, config.summary.max_channel);
  const outputOptions = (out: number): Option<string>[] =>
    outputChoices(t, count, out, outputNames, (output) => t("system.output.out_of_range", { output }));
  const roleOptions = (role: string): Option<string>[] => {
    const known: Option<string>[] = ROLES.map((r) => ({ value: r, label: roleLabel(t, r) }));
    const custom = isKnownRole(role) ? "custom" : role;
    return [...known, { value: custom, label: role === custom && custom !== "custom" ? t("system.role.custom_named", { role }) : t("system.role.custom") }];
  };

  const columns: Column<Row>[] = [
    {
      key: "role",
      header: t("system.patch.channel"),
      cell: ({ index, speaker }) => (
        <CellKeys>
          <CellSelect
            id={`system.patch.${speaker.role}.role`}
            label={t("system.patch.channel.aria", { code: roleCode(speaker.role) })}
            value={speaker.role}
            options={roleOptions(speaker.role)}
            onChange={(role) => actions.setRole(index, role)}
            display={
              <>
                <span className="system-role__code">{roleCode(speaker.role)}</span>
                <span className="system-role__name">{roleName(t, speaker.role)}</span>
              </>
            }
          />
        </CellKeys>
      ),
    },
    {
      key: "size",
      header: t("system.patch.size"),
      width: "12.5rem",
      cell: ({ index, speaker }) =>
        roleKind(speaker.role) === "sub" ? (
          <StatusPill>{t("system.size.sub")}</StatusPill>
        ) : (
          <CellKeys>
            <CellSelect
              id={`system.patch.${speaker.role}.size`}
              label={t("system.patch.size.aria", { code: roleCode(speaker.role) })}
              value={speaker.size === "large" ? "large" : "small"}
              options={[
                { value: "small", label: t("system.size.small") },
                { value: "large", label: t("system.size.large") },
              ]}
              onChange={(size) => actions.setSize(index, size)}
            />
          </CellKeys>
        ),
    },
    {
      key: "out",
      header: t("system.patch.output"),
      width: "14rem",
      cell: ({ index, speaker }) =>
        speaker.bands.length === 0 ? (
          <span className="system-cell__none">{t("system.patch.no_band")}</span>
        ) : (
          <div className="system-cell__bands">
            {speaker.bands.map((band, b) => {
              const range = outputChannels !== null && (band.out > outputChannels || band.out < 1);
              const duplicate = duplicates.has(band.out);
              const suffix = b === 0 ? "" : `.${b}`;
              return (
                <CellKeys key={b}>
                  <div className="system-out">
                    <CellSelect
                      id={`system.patch.${speaker.role}.out${suffix}`}
                      label={t("system.patch.output.aria", { code: roleCode(speaker.role), band: band.band })}
                      value={String(band.out)}
                      options={outputOptions(band.out)}
                      display={outputLabel(t, band.out, outputNames)}
                      invalid={range || duplicate}
                      onChange={(out) => actions.setOutput(index, b, Number(out))}
                    />
                    {duplicate && <StatusPill tone="danger">{t("system.output.duplicate")}</StatusPill>}
                    {range && <StatusPill tone="danger">{t("system.output.range")}</StatusPill>}
                  </div>
                </CellKeys>
              );
            })}
          </div>
        ),
    },
    {
      key: "pol",
      header: t("system.patch.polarity"),
      width: "6.5rem",
      cell: ({ index, speaker }) => (
        <div className="system-cell__bands">
          {speaker.bands.map((band, b) => {
            const inverted = band.pol === "-";
            return (
              <CellKeys key={b}>
                <button
                  type="button"
                  className="system-polarity"
                  aria-pressed={inverted}
                  aria-label={t("system.polarity.aria", { code: roleCode(speaker.role), band: band.band })}
                  data-ui={`system.patch.${speaker.role}.pol${b === 0 ? "" : `.${b}`}`}
                  onClick={() => actions.setPolarity(index, b, inverted ? "+" : "-")}
                >
                  {inverted ? t("system.polarity.inverted") : t("system.polarity.normal")}
                </button>
              </CellKeys>
            );
          })}
        </div>
      ),
    },
    {
      key: "remove",
      header: <span className="visually-hidden">{t("system.patch.remove")}</span>,
      width: "3.5rem",
      cell: ({ index, speaker }) => (
        <CellKeys>
          <IconButton
            icon="close"
            size="sm"
            label={t("system.patch.remove.aria", { code: roleCode(speaker.role) })}
            data-ui={`system.patch.${speaker.role}.remove`}
            onClick={() => actions.removeSpeaker(index)}
          />
        </CellKeys>
      ),
    },
  ];

  // `id` is the presentation group (front, center, …); `key` is the upstream name (프론트, …) the XO map uses.
  const groups: RowGroup<Row>[] = config.groups.map((group) => ({
    id: group.id,
    label: t("system.group.label", { name: t(GROUP_NAME[group.id] ?? "system.group.other"), count: group.speaker_indices.length }),
    rows: group.speaker_indices.flatMap((index) => {
      const speaker = config.speakers[index];
      return speaker ? [{ index, speaker }] : [];
    }),
  }));

  return (
    <div className="system-patch">
      <DataTable
        id="system.patch"
        caption={t("system.patch.caption")}
        columns={columns}
        groups={groups}
        rowKey={(row) => String(row.index)}
        selectedKey={selected === null ? null : String(selected)}
        onSelect={(row) => onSelect(row.index)}
        empty={t("system.patch.empty")}
      />
    </div>
  );
}

function Summary({ view }: { view: SystemView }) {
  const t = useT();
  const { summary } = view.config;
  const bands = view.config.speakers.reduce((sum, s) => sum + s.bands.length, 0);
  return (
    <div className="system-summary" data-ui="system.summary">
      <span className="tabular">
        {t("system.summary.counts", {
          speakers: summary.speaker_count,
          outputs: bands,
          max: summary.max_channel > 0 ? outputLabel(t, summary.max_channel, view.outputNames) : "—",
        })}
      </span>
      <span className="system-summary__pills">
        {summary.duplicate_outputs.length > 0 ? (
          <StatusPill tone="danger">{t("system.summary.duplicates", { count: summary.duplicate_outputs.length })}</StatusPill>
        ) : (
          <StatusPill tone="success">{t("system.summary.no_duplicates")}</StatusPill>
        )}
        {summary.sub_count > 0 ? (
          <StatusPill tone="success">{t("system.summary.subs", { count: summary.sub_count })}</StatusPill>
        ) : (
          <StatusPill>{t("system.summary.no_subs")}</StatusPill>
        )}
      </span>
    </div>
  );
}

/* ── 방 배치도 ─────────────────────────────────────────────────────────── */

const KIND_NAME = { main: "map.kind.main", sub: "map.kind.sub", top: "map.kind.top", height: "map.kind.height" } as const;

function RoomCard({ view, selected, onSelect }: { view: SystemView; selected: number | null; onSelect: (index: number) => void }) {
  const t = useT();
  const speakers = view.config.speakers.map((s) => ({ role: s.role, outputs: s.bands.map((b) => b.out) }));
  return (
    <Card className="system-room" title={t("system.room.title")} dense data-ui="system.room.card">
      <div className="system-room__body">
        <RoomMap id="system.room" speakers={speakers} selected={selected} onSelect={onSelect} />
        <SpeakerDetail view={view} index={selected} />
      </div>
    </Card>
  );
}

function SpeakerDetail({ view, index }: { view: SystemView; index: number | null }) {
  const t = useT();
  const speaker = index === null ? undefined : view.config.speakers[index];
  if (!speaker) {
    return (
      <p className="system-speaker system-speaker--empty" data-ui="system.speaker">
        {t("system.speaker.empty")}
      </p>
    );
  }
  const kind = roleKind(speaker.role);
  const note = speaker.bands.length > 1 ? "system.speaker.note.bands" : kind === "sub" ? "system.speaker.note.sub" : "system.speaker.note.full";
  return (
    <div className="system-speaker" data-ui="system.speaker" aria-live="polite">
      <div className="system-speaker__head">
        <span className="system-speaker__name" id="system.speaker.name">{roleName(t, speaker.role)}</span>
        <HelpTip id="system.speaker.help" labelledBy="system.speaker.name" content={<>{t(note)}<p>{t("system.speaker.internal", { role: speaker.role })}</p></>} />
        <span className="system-chip">{roleCode(speaker.role)}</span>
        <StatusPill>{t(KIND_NAME[kind])}</StatusPill>
      </div>
      {speaker.bands.length === 0 ? (
        <p className="system-speaker__band">{t("system.patch.no_band")}</p>
      ) : (
        <ul className="system-speaker__bands">
          {speaker.bands.map((band, b) => (
            <li key={b} className="system-speaker__band tabular">
              {t(band.pol === "-" ? "system.speaker.band.inverted" : "system.speaker.band", {
                band: band.band,
                output: outputLabel(t, band.out, view.outputNames),
                input: band.out,
              })}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/* ── 측정 배치 ─────────────────────────────────────────────────────────── */

function ArrangementCard({ view, actions, ask }: { view: SystemView; actions: SystemActions; ask: (change: SystemChange) => void }) {
  const t = useT();
  const { layout } = view;
  const arrangement = layout.arrangements.find((a) => a.count === layout.count);
  const points: MicPoint[] | undefined = arrangement?.points.map((p) => ({ x: p.x_cm, y: p.y_cm, h: p.height > 0 ? 1 : p.height < 0 ? -1 : 0 }));
  const counts = layout.arrangements.length > 0 ? layout.arrangements.map((a) => a.count) : [...ARRANGEMENT_COUNTS];
  return (
    <Card className="system-layout" title={t("system.layout.title")} help={t("system.layout.description")} data-ui="system.layout">
      <div className="system-arrangements" role="group" aria-label={t("system.layout.title")} data-ui="system.arrangement">
        {counts.map((count) => {
          const current = count === layout.count;
          return (
            <button
              key={count}
              type="button"
              className="system-choice system-arrangement"
              aria-pressed={current}
              data-ui={`system.arrangement.${count}`}
              onClick={() => !current && ask({ kind: "arrangement", count })}
            >
              <ArrangementThumb count={count} width={88} />
              <span className="system-arrangement__name">{t(ARRANGEMENT_NAME[count as keyof typeof ARRANGEMENT_NAME] ?? "system.arrangement.9")}</span>
              <span className="system-arrangement__count tabular">{t("system.arrangement.count", { count })}</span>
            </button>
          );
        })}
      </div>
      <div className="system-layout__body">
        <ArrangementMap id="system.arrangement.map" count={layout.count} done={view.positionsDone} points={points} />
        <MeasureOptions view={view} actions={actions} />
      </div>
    </Card>
  );
}

function MeasureOptions({ view, actions }: { view: SystemView; actions: SystemActions }) {
  const t = useT();
  const { layout, config, outputChannels, outputNames } = view;
  const timing = layout.timing;
  // Reference speakers in role order (upstream measurement_roles): every speaker's first band.
  const refs = orderRoles(config.speakers.map((s) => s.role))
    .map((role) => config.speakers.find((s) => s.role === role))
    .flatMap((s) => (s && s.bands[0] ? [{ role: s.role, out: s.bands[0].out }] : []));
  // An empty or vanished reference role falls back to upstream default_ref_role: R, else the first main speaker.
  const mains = refs.filter((r) => roleKind(r.role) !== "sub");
  const refRole = refs.some((r) => r.role === timing.role) ? timing.role : (mains.find((r) => r.role === "R") ?? mains[0] ?? refs[0])?.role ?? "";
  // Upstream's reference output lists the device's channels (8 while it is
  // unknown) and adds a stored value past them as one more item.
  const channels = outputChannels ?? 8;
  const bands = config.speakers.reduce((sum, s) => sum + s.bands.length, 0);
  const timingHint = { acoustic: "system.timing.acoustic.hint", loopback: "system.timing.loopback.hint", none: "system.timing.none.hint" } as const;
  const reference = timing.reference === "loopback" || timing.reference === "none" ? timing.reference : "acoustic";
  // The estimate comes first; the timing reference (REW only) stays folded
  // under 고급 at the end, where the other cards keep theirs.
  return (
    <div className="system-options" data-ui="system.options">
      <div className="system-estimate" data-ui="system.estimate">
        <span className="ui-field__label">{t("system.estimate.label")}</span>
        {view.estimate ? (
          <>
            <span className="system-estimate__main tabular">{t("system.estimate.per_position", { time: durationText(t, view.estimate.per_position_seconds) })}</span>
            <span className="system-estimate__sub tabular">
              {t("system.estimate.total", { count: view.estimate.positions, time: durationText(t, view.estimate.total_seconds) })}
            </span>
          </>
        ) : (
          <span className="system-estimate__sub">—</span>
        )}
        <span className="system-estimate__sub tabular" data-ui="system.sweeps">
          {t("system.estimate.sweeps", { count: bands })}
        </span>
      </div>
      {/* Only REW needs a timing reference: it records each channel on its own.
          The default (the acoustic reference on R, else the first main speaker)
          suits most rooms, so the choice waits folded under 고급. */}
      {view.rew && (
        <Advanced id="system.timing.advanced" count={reference === "none" ? 1 : 2}>
          <div className="system-options__advanced">
            <Select
              id="system.timing"
              label={t("system.timing.label")}
              value={reference}
              options={[
                { value: "acoustic", label: t("system.timing.acoustic") },
                { value: "loopback", label: t("system.timing.loopback") },
                { value: "none", label: t("system.timing.none") },
              ]}
              help={t(timingHint[reference])}
              onChange={(next) => actions.setTiming({ ...timing, reference: next, role: refRole })}
            />
            {reference === "loopback" ? (
              <Select
                id="system.timing.output"
                label={t("system.timing.output")}
                value={String(timing.output)}
                options={outputChoices(t, channels, timing.output, outputNames)}
                onChange={(output) => actions.setTiming({ ...timing, role: refRole, output: Number(output) })}
              />
            ) : reference === "none" ? null : (
              <Select
                id="system.timing.role"
                label={t("system.timing.role")}
                value={refRole}
                options={refs.map((r) => ({ value: r.role, label: t("system.timing.role.option", { code: roleCode(r.role), output: outputLabel(t, r.out, outputNames) }) }))}
                onChange={(role) => actions.setTiming({ ...timing, role })}
              />
            )}
          </div>
        </Advanced>
      )}
    </div>
  );
}

/* ── 내보낼 DSP (U2) ────────────────────────────────────────────────────── */

function DspCard({ view, actions, initialAdvanced }: { view: SystemView; actions: SystemActions; initialAdvanced: boolean }) {
  const t = useT();
  const number = useNumber();
  const { dsp } = view;
  const keys = [...DSP_ORDER.filter((key) => dsp.presets.some((p) => p.key === key)), ...dsp.presets.map((p) => p.key).filter((key) => !(DSP_ORDER as readonly string[]).includes(key))];
  const presetName = (key: string) => {
    const name = DSP_NAME[key];
    return name ? t(name) : (dsp.presets.find((p) => p.key === key)?.name ?? key);
  };
  const edit = (patch: Partial<DspFields>) => actions.setDspFields({ ...dsp.fields, ...patch });
  const taps = FIR_TAP_CHOICES.includes(dsp.fields.fir_taps as (typeof FIR_TAP_CHOICES)[number]) ? [...FIR_TAP_CHOICES] : [...FIR_TAP_CHOICES, dsp.fields.fir_taps].sort((a, b) => a - b);
  // Upstream fir_tap_label: the linear-phase delay of N taps, (N − 1) / 2
  // samples. The DSP's taps run at its output rate (48 kHz for 자동).
  const rate = countedRate(dsp.fields.output_rate);
  const tapLabel = (n: number) => t("system.dsp.taps.option", { taps: number(n), ms: number(((n - 1) / rate / 2) * 1000, 1) });
  return (
    <Card className="system-dsp" title={t("system.dsp.title")} help={<>{t("system.dsp.description")}<p>{t("system.dsp.use")}</p></>} data-ui="system.dsp">
      <Select
        id="system.dsp.device"
        label={t("system.dsp.device")}
        help={t("system.dsp.custom_hint")}
        value={dsp.profile}
        options={keys.map((key) => ({ value: key, label: presetName(key) }))}
        onChange={(key) => actions.setDspPreset(key)}
      />
      <OutputRate fields={dsp.fields} onChange={(output_rate) => edit({ output_rate })} />
      <Advanced id="system.dsp.advanced" count={5} defaultOpen={initialAdvanced}>
        <div className="system-dsp__fields">
          <Select id="system.dsp.fir_taps" label={t("system.dsp.fir_taps")} value={String(dsp.fields.fir_taps)} options={taps.map((n) => ({ value: String(n), label: tapLabel(n) }))} onChange={(v) => edit({ fir_taps: Number(v) })} />
          <NumberField id="system.dsp.fir_budget" label={t("system.dsp.fir_budget")} value={dsp.fields.fir_budget} min={0} max={4194304} step={1024} unit={t("system.dsp.unit.taps")} size={9} onChange={(v) => edit({ fir_budget: v })} />
          <NumberField id="system.dsp.iir_bands" label={t("system.dsp.iir_bands")} value={dsp.fields.iir_bands} min={0} max={64} step={1} unit={t("system.dsp.unit.bands")} size={4} onChange={(v) => edit({ iir_bands: v })} />
          <NumberField id="system.dsp.allpass_bands" label={t("system.dsp.allpass_bands")} value={dsp.fields.allpass_bands} min={0} max={64} step={1} unit={t("system.dsp.unit.bands")} size={4} onChange={(v) => edit({ allpass_bands: v })} />
          <NumberField id="system.dsp.bm_path_delays" label={t("system.dsp.bm_path_delays")} value={dsp.fields.bm_path_delays} min={0} max={64} step={1} unit={t("system.dsp.unit.slots")} size={4} onChange={(v) => edit({ bm_path_delays: v })} />
        </div>
        <p className="system-dsp__note tabular" data-ui="system.dsp.note">
          {t("system.dsp.note", { window: number(dsp.window_ms, 0), resolution: number(dsp.resolution_hz, 1) })}
        </p>
      </Advanced>
    </Card>
  );
}

/**
 * The rate the exported filters are written at (ADR 0017). A PC profile
 * chooses it, 자동 included; a hardware DSP runs at its own rate, so the
 * value stands as text with the reason and no control.
 */
function OutputRate({ fields, onChange }: { fields: DspFields; onChange: (choice: OutputRateChoice) => void }) {
  const t = useT();
  const number = useNumber();
  const choice = fields.output_rate;
  if (!fields.output_rate_editable) {
    return (
      <div className="ui-field system-dsp__rate">
        <span className="ui-field__label">{t("system.dsp.output_rate")}</span>
        <p className="system-dsp__rate-value" data-ui="system.dsp.output_rate">
          <span className="system-dsp__rate-text tabular">{choice === "auto" ? t("system.dsp.output_rate.auto") : rateText(number, choice.fixed)}</span>
          <span className="system-dsp__rate-reason">{t("system.dsp.output_rate.fixed")}</span>
        </p>
      </div>
    );
  }
  return (
    <Select
      id="system.dsp.output_rate"
      label={t("system.dsp.output_rate")}
      help={t("system.dsp.output_rate.help")}
      value={rateValue(choice)}
      options={[
        { value: "auto", label: t("system.dsp.output_rate.auto") },
        ...OUTPUT_RATES.map((rate) => ({ value: String(rate), label: rateText(number, rate) })),
      ]}
      // Said only while 자동 is chosen: what it reads, and what it falls back to.
      hint={choice === "auto" ? t("system.dsp.output_rate.auto.hint") : undefined}
      onChange={(value) => onChange(rateChoice(value))}
    />
  );
}
