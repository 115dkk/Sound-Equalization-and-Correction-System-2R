import { HelpTip } from "../../ui/HelpTip";
// The settings that change the correction, in sections grouped by what they
// change: 모드, 보정 범위, 크로스오버, 서브 정렬 (BA), 저음 도착과 울림,
// MIMO. Every change goes through `changeSetting` (calibration_settings_set)
// and only marks the result stale. Detail sits under 고급; every correction
// mode is available in the mode selector. MIMO is chosen beside 보정 계산 too
// (MIMO 함께 계산, P142); its card keeps the details and MIMO만 계산. So is
// XO 추천 포함 (ADR 0028); the crossover card has its switch as well.
import { ComputeControl, ComputeResult } from "./ComputeControl";
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { errorText } from "../../app/jobs/jobText";
import { useNumber, useT } from "../../i18n/react";
import type { BandType } from "../../ipc/bindings/BandType";
import type { CalibrationSettings } from "../../ipc/bindings/CalibrationSettings";
import type { RcBand } from "../../ipc/bindings/RcBand";
import type { SampleRatePolicy } from "../../ipc/bindings/SampleRatePolicy";
import { Advanced, Button, Card, ErrorNotice, IconButton, NumberField, Segmented, Select, Toggle, type Option } from "../../ui";
import { Icon } from "../../ui/Icon";
import { roleCode } from "../../ui/maps/geometry";
import { colorTokens } from "../../ui/plot/series";
import {
  AGREEMENT_MIN_POSITIONS,
  BAND_TYPES,
  INFRA_DEFAULT_HZ,
  LIMITS,
  MAX_EXTRA_BANDS,
  SUB_LIMITS,
  MIMO_MIN_POSITIONS,
  ALLPASS_Q_DEFAULT,
  TRANSITION_FACTORS,
  UPPER_CHOICES,
  infraState,
  layoutOf,
  nextBand,
  rejectionPlace,
  roomVolume,
  subModeChoice,
  subOf,
  subValuesApply,
  upperChoice,
  upperEdits,
  usesSchroeder,
  withBandEdge,
  xoRecInclude,
  type RejectionPlace,
  type UpperChoice,
} from "./settingsView";
import { FAILED_TEXT, MODES, MODE_EFFECT, MODE_NAME, bandTypeText, filterSpec, groupName, modeSpecText, noticeText, rangeNotices, rangeRejectionText, subRoles, xoGroups } from "./text";
import { DESIGN_RATE, rateText, tapsAt } from "../system/outputRate";
import type { BandTypeChoice, CalibrateActions, CalibrateJob, CalibrateView, SettingEdit, SubLayout, SubModeChoice } from "./types";

type Props = { view: CalibrateView; actions: CalibrateActions };
type FailureProps = Props & { onSampleRateSettings: () => void };

function SettingsCard({ id, title, help, children }: { id: string; title: string; help: ReactNode; children: ReactNode }) {
  return (
    <Card title={title} help={help} className="calibrate-settings__card" data-ui={`calibrate.settings.${id}`} id={`calibrate.settings.${id}`}>
      {children}
    </Card>
  );
}

/** The last failure of a section's job: cause, next step and 자세히. A running job shows on its busy button and in the header. */
function JobFailure({ view, actions, onSampleRateSettings, job, id }: FailureProps & { job: CalibrateJob; id: string }) {
  const t = useT();
  if (view.failure?.job === job) {
    const text = errorText(t, view.failure.error);
    const sampleRate = view.failure.error.params.reason === "measurement_sample_rate";
    return <ErrorNotice id={`${id}.failed`}
      cause={text.generic ? t(FAILED_TEXT[job]) : `${t(FAILED_TEXT[job])} ${text.cause}`}
      next={text.next} detail={view.failure.error.detail} code={view.failure.error.code}
      action={view.failure.error.code === "engine_input" && !text.generic ? (
        <Button variant="secondary" data-ui={`${id}.failed.${sampleRate ? "sample_rate" : "measurements"}`} onClick={sampleRate ? onSampleRateSettings : actions.goToMeasurements}>
          {t(sampleRate ? "calibrate.failed.sample_rate_settings" : "calibrate.failed.inspect_measurements")}
        </Button>
      ) : undefined} />;
  }
  return null;
}

/** This input policy applies to every calculation, independently of the filter mode. */
function SampleRateSettings({ view, actions, request }: Props & { request: number }) {
  const t = useT();
  const root = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (request === 0) return;
    const choice = root.current?.querySelector<HTMLButtonElement>('[role="radio"][aria-checked="true"]');
    choice?.focus({ preventScroll: true });
    const content = root.current?.closest<HTMLElement>(".app-content");
    if (root.current && content) content.scrollTop = Math.max(0, root.current.getBoundingClientRect().top - content.getBoundingClientRect().top + content.scrollTop - 16);
  }, [request]);
  return (
    <div ref={root} data-ui="calibrate.settings.input">
      <Advanced key={request} id="calibrate.input.details" label={t("calibrate.sample_rate.title")} defaultOpen={request > 0 || view.settings.sample_rate_policy === "resample"}>
        <Segmented<SampleRatePolicy>
          id="calibrate.sample_rate_policy"
          label={t("calibrate.sample_rate.title")}
          hideLabel
          help={t("calibrate.sample_rate.help")}
          value={view.settings.sample_rate_policy}
          options={[
            { value: "reject", label: t("calibrate.sample_rate.reject") },
            { value: "resample", label: t("calibrate.sample_rate.resample") },
          ]}
          onChange={(policy) => actions.changeSetting({ kind: "sample_rate_policy", policy })}
        />
      </Advanced>
    </div>
  );
}

/* ── 모드 ───────────────────────────────────────────────────────────────── */

function ModeSection({ view, actions }: Props) {
  const t = useT();
  const number = useNumber();
  const s = view.settings;
  return (
    <SettingsCard id="mode" title={t("calibrate.mode.title")} help={<>{t("calibrate.mode.description")}<p>{t(MODE_EFFECT[s.mode])}</p></>}>
      <Segmented
        id="calibrate.mode"
        label={t("calibrate.mode.title")}
        hideLabel
        value={s.mode}
        options={MODES.map((mode) => ({ value: mode, label: t(MODE_NAME[mode]) }))}
        onChange={(mode) => actions.changeSetting({ kind: "mode", mode })}
      />
      <div className="calibrate-mode-text">
        <p className="calibrate-line tabular" data-ui="calibrate.mode.latency">
          {view.modeSpec.max_latency_ms === 0
            ? t("calibrate.mode.latency.zero")
            : t("calibrate.mode.latency.max", { ms: number(view.modeSpec.max_latency_ms) })}
        </p>
        <span className="ui-heading-help" data-ui="calibrate.mode.spec">
          <span id="calibrate.mode.spec.label">{t("calibrate.mode.spec.title")}</span>
          <HelpTip id="calibrate.mode.spec.help" labelledBy="calibrate.mode.spec.label" content={modeSpecText(t, number, view.modeSpec)} />
        </span>
      </div>
      {s.mode === "ultra" && (
        <div className="calibrate-fields">
          <NumberField
            id="calibrate.mode.ultra_ms"
            label={t("calibrate.mode.ultra_ms.label")}
            value={s.ultra_ms}
            min={LIMITS.ultra_ms[0]}
            max={LIMITS.ultra_ms[1]}
            step={10}
            unit="ms"
            showRange
            onChange={(ms) => actions.changeSetting({ kind: "ultra_ms", ms })}
          />
        </div>
      )}
      <ChannelFirOver view={view} />
    </SettingsCard>
  );
}

/**
 * The mode's channel FIR against the DSP's taps per channel, both at the
 * DSP's output rate (ADR 0017): one warning line when it does not fit, with
 * the rate named when it is not 48 kHz.
 */
function ChannelFirOver({ view }: { view: CalibrateView }) {
  const t = useT();
  const number = useNumber();
  const taps = tapsAt(view.modeSpec.fir_taps, view.outputRate);
  if (view.firTaps <= 0 || taps <= view.firTaps) return null;
  const params = { taps: number(taps), limit: number(view.firTaps), rate: rateText(number, view.outputRate), mode: t(MODE_NAME.zero) };
  return (
    <p className="calibrate-hint calibrate-hint--warning calibrate-fir-over" data-ui="calibrate.mode.fir_over">
      <Icon name="warning" size={16} className="calibrate-fir-over__icon" />
      <span>{view.outputRate === DESIGN_RATE ? t("calibrate.mode.fir_over", params) : t("calibrate.mode.fir_over.rate", params)}</span>
    </p>
  );
}

/* ── 보정 범위 ──────────────────────────────────────────────────────────── */

function RoomDims({ settings, change }: { settings: CalibrationSettings; change: (edit: SettingEdit) => void }) {
  const t = useT();
  // Partial dimensions are an edit in progress: only all three set the volume.
  const [dims, setDims] = useState<[number, number, number]>(() => settings.room_dims_m ?? [0, 0, 0]);
  useEffect(() => {
    if (settings.room_dims_m) setDims(settings.room_dims_m);
    else setDims((d) => (d.every((v) => v > 0) ? [0, 0, 0] : d));
  }, [settings.room_dims_m]);
  const set = (index: number, value: number) => {
    const next: [number, number, number] = [...dims];
    next[index] = value;
    setDims(next);
    if (roomVolume(next) !== null || settings.room_dims_m !== null) change({ kind: "room_dims", dims: next });
  };
  const names = ["w", "d", "h"] as const;
  return (
    <fieldset className="calibrate-group" data-ui="calibrate.range.dims">
      <legend className="ui-field__label ui-heading-help"><span id="calibrate.range.dims.label">{t("calibrate.range.dims")}</span><HelpTip id="calibrate.range.dims.help" labelledBy="calibrate.range.dims.label" content={t("calibrate.range.dims.hint")} /></legend>
      <div className="calibrate-fields calibrate-fields--tight">
        {names.map((name, index) => (
          <NumberField
            key={name}
            id={`calibrate.range.dims.${name}`}
            label={t(`calibrate.range.dims.${name}`)}
            value={dims[index]!}
            min={LIMITS.dim[0]}
            max={LIMITS.dim[1]}
            step={0.1}
            digits={1}
            unit="m"
            size={4}
            onChange={(value) => set(index, value)}
          />
        ))}
      </div>
    </fieldset>
  );
}

/** Where the experimental closed-form derivatives apply (D43, D44); two settings, one choice. */
type AnalyticScope = "off" | "peq" | "mimo" | "both";
const ANALYTIC_SCOPES: readonly AnalyticScope[] = ["off", "peq", "mimo", "both"];

function analyticScope(s: CalibrationSettings): AnalyticScope {
  if (s.peq_analytic_derivatives) return s.mimo_iir_analytic_derivatives ? "both" : "peq";
  return s.mimo_iir_analytic_derivatives ? "mimo" : "off";
}

const UPPER_NAME = {
  strict: "calibrate.range.basis.strict",
  lenient: "calibrate.range.basis.lenient",
  agreement: "calibrate.range.basis.agreement",
  manual: "calibrate.range.basis.manual",
} as const satisfies Record<UpperChoice, string>;

const UPPER_EFFECT = {
  strict: "calibrate.range.basis.strict.effect",
  lenient: "calibrate.range.basis.lenient.effect",
  agreement: "calibrate.range.basis.agreement.effect",
  manual: "calibrate.range.basis.manual.effect",
} as const satisfies Record<UpperChoice, string>;

/** Each basis in one sentence by its effect, then what a higher limit costs (ADR 0021 결과). */
function BasisHelp() {
  const t = useT();
  return (
    <>
      <ul>
        {UPPER_CHOICES.map((choice) => (
          <li key={choice}>
            <strong>{t(UPPER_NAME[choice])}</strong>: {t(UPPER_EFFECT[choice], { min: AGREEMENT_MIN_POSITIONS })}
          </li>
        ))}
      </ul>
      <p>{t("calibrate.range.basis.raise")}</p>
    </>
  );
}

/**
 * One basis's controls in the place every basis shares. The others stay laid
 * out but hidden and inert, so the card keeps the height of the tallest and
 * choosing a basis moves nothing below it.
 */
function UpperPanel({ shown, id, children }: { shown: boolean; id: string; children: ReactNode }) {
  return (
    <div className="calibrate-upper__panel" data-ui={id} inert={!shown}>
      {children}
    </div>
  );
}

/** An edge's arrow-key step: 5 Hz below 1 kHz, 50 Hz from there up. */
const bandStep = (hz: number) => (hz < 1000 ? 5 : 50);

/** A refusal of a range edit and the input it concerns, as the card shows it. */
type ShownRejection = { place: RejectionPlace; text: string };

/**
 * The extra correction bands (ADR 0024) under the primary edges and their RC
 * lines: a row per band with its two edges and 빼기, then 보정 구간 추가
 * while there are fewer than four. Each change sends the whole list; the
 * service answers it sorted. Focus follows a band just added to its lower
 * edge, and goes to the add action after a removal.
 */
function ExtraBands({ settings, primaryHi, rejection, change }: { settings: CalibrationSettings; primaryHi: number; rejection: ShownRejection | null; change: (edit: SettingEdit) => Promise<boolean> }) {
  const t = useT();
  const bands = settings.rc_bands;
  const root = useRef<HTMLDivElement>(null);
  const [focus, setFocus] = useState<RcBand | "add" | null>(null);
  useEffect(() => {
    if (focus === null) return;
    const index = focus === "add" ? -1 : bands.findIndex((band) => band.lo_hz === focus.lo_hz && band.hi_hz === focus.hi_hz);
    const target = focus === "add" ? "calibrate.range.extra.add" : index >= 0 ? `calibrate.range.extra.${index + 1}.lower` : null;
    const element = target ? root.current?.querySelector<HTMLElement>(`[data-ui="${target}"]`) : null;
    if (element) {
      element.focus();
      setFocus(null);
    }
  }, [bands, focus]);
  const add = async () => {
    const band = nextBand(settings, primaryHi);
    if (await change({ kind: "rc_bands", bands: [...bands.map((b) => ({ ...b })), band] })) setFocus(band);
  };
  const remove = async (index: number) => {
    if (await change({ kind: "rc_bands", bands: bands.filter((_, i) => i !== index).map((b) => ({ ...b })) })) setFocus("add");
  };
  const refused = (index: number, edge: "lower" | "upper") =>
    rejection?.place.at === "band" && rejection.place.index === index && rejection.place.edge === edge ? rejection.text : undefined;
  return (
    <div ref={root} className="calibrate-extra" data-ui="calibrate.range.extra">
      {bands.map((band, index) => {
        const id = `calibrate.range.extra.${index + 1}`;
        return (
          <div key={index} role="group" aria-labelledby={`${id}.label`} className="calibrate-extra__band" data-ui={id}>
            <div className="calibrate-extra__head">
              <span id={`${id}.label`} className="calibrate-extra__name">
                {t("calibrate.range.extra.band", { n: index + 1 })}
              </span>
              <IconButton icon="close" size="sm" label={t("calibrate.range.extra.remove", { n: index + 1 })} data-ui={`${id}.remove`} onClick={() => void remove(index)} />
            </div>
            <div className="calibrate-fields calibrate-fields--tight">
              {(["lower", "upper"] as const).map((edge) => {
                const hz = edge === "lower" ? band.lo_hz : band.hi_hz;
                return (
                  <NumberField
                    key={edge}
                    id={`${id}.${edge}`}
                    label={t(edge === "lower" ? "calibrate.range.lower" : "calibrate.range.upper")}
                    value={hz}
                    min={LIMITS.rc_band[0]}
                    max={LIMITS.rc_band[1]}
                    step={bandStep(hz)}
                    unit="Hz"
                    rejected={refused(index, edge)}
                    onChange={(value) => void change({ kind: "rc_bands", bands: withBandEdge(bands, index, edge, value) })}
                  />
                );
              })}
            </div>
          </div>
        );
      })}
      {bands.length < MAX_EXTRA_BANDS && (
        <div className="calibrate-actions">
          <span className="ui-heading-help">
            <Button icon="plus" id="calibrate.range.extra.add" data-ui="calibrate.range.extra.add" onClick={() => void add()}>
              {t("calibrate.range.extra.add")}
            </Button>
            <HelpTip id="calibrate.range.extra.help" labelledBy="calibrate.range.extra.add" content={t("calibrate.range.extra.help")} />
          </span>
        </div>
      )}
      {rejection?.place.at === "add" && (
        <p className="ui-field__error" data-ui="calibrate.range.extra.rejected">
          <Icon name="error" size={16} />
          <span>{rejection.text}</span>
        </p>
      )}
    </div>
  );
}

/** `advancedOpen`: gallery and tests start with 고급 open. */
function RangeSection({ view, actions, advancedOpen = false }: Props & { advancedOpen?: boolean | undefined }) {
  const t = useT();
  const number = useNumber();
  const s = view.settings;
  const analytic = analyticScope(s);
  const choice = upperChoice(s);
  const rcLimit = view.lines.rc.find((n) => n.code === "rc_limit");
  const resolved = rcLimit ? Number(rcLimit.params.hz) : Number.NaN;
  const shownHi = Number.isFinite(resolved) ? Math.round(Math.min(LIMITS.rc_hi[1], Math.max(LIMITS.rc_hi[0], resolved))) : 500;
  const notes = rangeNotices(view.lines.rc).map((n) => noticeText(t, number, n)).filter((text): text is string => !!text);
  // The measured positions the project reports (project_info.positions), as MIMO counts them.
  const count = view.positions.length;
  const short = count < AGREEMENT_MIN_POSITIONS;
  const choose = async (next: UpperChoice) => {
    for (const edit of upperEdits(s, next, shownHi)) if (!(await actions.changeSetting(edit))) return;
  };
  const shortText = { count, min: AGREEMENT_MIN_POSITIONS };
  // A refused edge or band (ADR 0024): its reason at the input it concerns; the input keeps the stored value.
  const rejection: ShownRejection | null = view.rangeRejection
    ? (() => {
        const place = rejectionPlace(view.rangeRejection, s);
        return { place, text: rangeRejectionText(t, number, view.rangeRejection, place) };
      })()
    : null;
  const refusedAt = (at: "lower" | "upper" | "basis") => (rejection?.place.at === at ? rejection.text : undefined);
  const basisRefused = refusedAt("basis");
  return (
    <SettingsCard id="range" title={t("calibrate.range.title")} help={t("calibrate.range.description")}>
      <div className="calibrate-fields">
        <NumberField
          id="calibrate.range.lower"
          label={t("calibrate.range.lower")}
          value={s.rc_lo}
          min={LIMITS.rc_lo[0]}
          max={LIMITS.rc_lo[1]}
          step={5}
          unit="Hz"
          showRange
          rejected={refusedAt("lower")}
          onChange={(hz) => actions.changeSetting({ kind: "rc_lo", hz })}
        />
        <Segmented<UpperChoice>
          id="calibrate.range.basis"
          label={t("calibrate.range.basis")}
          help={<BasisHelp />}
          // A chosen agreement stays selectable (one tab stop) and says what to do; otherwise it is off with the reason.
          // A refused 직접 입력 (its starting value not above the lower edge) gives the reason as a field error.
          hint={
            basisRefused ? (
              <span className="ui-field__error" data-ui="calibrate.range.basis.rejected">
                <Icon name="error" size={16} />
                <span>{basisRefused}</span>
              </span>
            ) : !short ? undefined : choice === "agreement" ? (
              <span className="calibrate-hint--warning calibrate-fir-over">
                <Icon name="warning" size={16} className="calibrate-fir-over__icon" />
                <span>{t("calibrate.range.basis.agreement_short", shortText)}</span>
              </span>
            ) : (
              t("calibrate.range.basis.agreement_unavailable", shortText)
            )
          }
          value={choice}
          options={UPPER_CHOICES.map((value) => ({
            value,
            label: t(UPPER_NAME[value]),
            disabled: value === "agreement" && short && choice !== "agreement",
          }))}
          onChange={(next) => void choose(next)}
        />
      </div>
      <div className="calibrate-upper">
        <UpperPanel id="calibrate.range.schroeder" shown={usesSchroeder(choice)}>
          <div className="calibrate-fields">
            <Segmented
              id="calibrate.range.transition"
              label={t("calibrate.range.transition")}
              help={t("calibrate.range.transition.help")}
              value={String(s.rc_hi_transition)}
              options={TRANSITION_FACTORS.map((factor) => ({ value: String(factor), label: `×${number(factor)}` }))}
              onChange={(factor) => actions.changeSetting({ kind: "rc_hi_transition", factor: Number(factor) })}
            />
            <NumberField
              id="calibrate.range.volume"
              label={t("calibrate.range.volume")}
              value={s.room_volume_m3 ?? 0}
              min={LIMITS.volume[0]}
              max={LIMITS.volume[1]}
              step={10}
              digits={1}
              unit="m³"
              help={t("calibrate.range.volume.hint")}
              onChange={(m3) => actions.changeSetting({ kind: "room_volume", m3 })}
            />
            <RoomDims settings={s} change={actions.changeSetting} />
          </div>
        </UpperPanel>
        <UpperPanel id="calibrate.range.agreement" shown={choice === "agreement"}>
          <p className="calibrate-hint">{t("calibrate.range.agreement.note")}</p>
        </UpperPanel>
        <UpperPanel id="calibrate.range.manual" shown={choice === "manual"}>
          <div className="calibrate-fields">
            <NumberField
              id="calibrate.range.upper"
              label={t("calibrate.range.upper")}
              value={s.rc_hi ?? shownHi}
              min={LIMITS.rc_hi[0]}
              max={LIMITS.rc_hi[1]}
              step={50}
              unit="Hz"
              showRange
              rejected={refusedAt("upper")}
              onChange={(hz) => actions.changeSetting({ kind: "rc_hi", hz })}
            />
          </div>
        </UpperPanel>
      </div>
      {/* The RC lines say how the upper limit above was set, so they stay right under it. */}
      {notes.length > 0 && (
        <ul className="calibrate-notes-list" data-ui="calibrate.range.notes">
          {notes.map((note) => (
            <li key={note}>{note}</li>
          ))}
        </ul>
      )}
      <ExtraBands settings={s} primaryHi={shownHi} rejection={rejection} change={actions.changeSetting} />
      {/* An experiment or the low-end protection left on stays in view: the section opens with it. */}
      <Advanced id="calibrate.range.advanced" count={3} defaultOpen={advancedOpen || analytic !== "off" || s.low_protect}>
        <div className="calibrate-fields">
          <NumberField
            id="calibrate.range.boost"
            label={t("calibrate.range.boost")}
            value={s.boost_max_db}
            min={LIMITS.boost[0]}
            max={LIMITS.boost[1]}
            step={1}
            digits={1}
            unit="dB"
            help={t("calibrate.range.boost.hint")}
            onChange={(db) => actions.changeSetting({ kind: "boost_max", db })}
          />
        </div>
        {/* 극저역 보호 (ADR 0022): beside the boost limit it holds below each channel's roll-off. */}
        <Toggle
          id="calibrate.range.low_protect"
          label={t("calibrate.range.low_protect")}
          help={t("calibrate.range.low_protect.help")}
          checked={s.low_protect}
          onChange={(on) => actions.changeSetting({ kind: "low_protect", on })}
        />
        <Segmented<AnalyticScope>
          id="calibrate.analytic"
          label={t("calibrate.analytic.title")}
          help={t("calibrate.analytic.help")}
          hint={analytic !== "off" && analytic !== "peq" && (!s.mimo_on || s.mimo_fir) ? t("calibrate.analytic.mimo_unused") : undefined}
          value={analytic}
          options={ANALYTIC_SCOPES.map((scope) => ({ value: scope, label: t(`calibrate.analytic.${scope}`) }))}
          onChange={(scope) =>
            actions.changeSetting({ kind: "analytic_derivatives", peq: scope === "peq" || scope === "both", mimo_iir: scope === "mimo" || scope === "both" })
          }
        />
      </Advanced>
    </SettingsCard>
  );
}

/* ── 크로스오버 ─────────────────────────────────────────────────────────── */

function XoSection({ view, actions, onSampleRateSettings }: FailureProps) {
  const t = useT();
  const number = useNumber();
  const s = view.settings;
  const groups = xoGroups(view.config);
  const group = view.xoGroup && groups.includes(view.xoGroup) ? view.xoGroup : (groups[0] ?? null);
  if (!group) {
    return (
      <SettingsCard id="xo" title={t("calibrate.xo.title")} help={t("calibrate.xo.description")}>
        <p className="calibrate-hint" data-ui="calibrate.xo.none">
          {t("calibrate.xo.none")}
        </p>
      </SettingsCard>
    );
  }
  const hz = s.xo[group] ?? 80;
  const rec = s.xo_rec[group];
  const recs = groups.filter((g) => s.xo_rec[g]).map((g) => t("calibrate.xo.rec_item", { group: groupName(t, g), hz: number(s.xo_rec[g]!) }));
  const busy = view.running !== null;
  const include = xoRecInclude(s);
  return (
    <SettingsCard id="xo" title={t("calibrate.xo.title")} help={t("calibrate.xo.description")}>
      <div className="calibrate-fields">
        <Select
          id="calibrate.xo.group"
          label={t("calibrate.xo.group")}
          value={group}
          options={groups.map((g) => ({ value: g, label: groupName(t, g) }))}
          onChange={actions.selectXoGroup}
        />
        <NumberField
          id="calibrate.xo.hz"
          label={t("calibrate.xo.hz")}
          value={hz}
          min={LIMITS.xo[0]}
          max={LIMITS.xo[1]}
          step={5}
          unit="Hz"
          hint={<span data-ui="calibrate.xo.recommended">{rec ? t("calibrate.xo.recommended", { hz: number(rec) }) : t("calibrate.xo.recommended.none")}</span>}
          onChange={(value) => actions.changeSetting({ kind: "xo", group, hz: value })}
        />
      </div>
      {/* Right under the value it would overwrite: a manual edit turns it off, and the line under it says so. */}
      <Toggle
        id="calibrate.xo_rec_include"
        label={t("calibrate.xo_rec_include")}
        help={t("calibrate.xo_rec_include.help")}
        hint={view.xoRecTurnedOffByEdit && !include ? t("calibrate.xo_rec_include.off_manual") : undefined}
        checked={include}
        onChange={(on) => actions.changeSetting({ kind: "xo_rec_include", on })}
      />
      <div className="calibrate-actions">
        <Button
          id="calibrate.xo.recommend"
          data-ui="calibrate.xo.recommend"
          busy={view.running === "recommend_xo"}
          disabled={busy || view.positions.length === 0}
          onClick={actions.recommendXo}
        >
          {t("calibrate.xo.recommend")}
        </Button>
        <JobFailure view={view} actions={actions} onSampleRateSettings={onSampleRateSettings} job="recommend_xo" id="calibrate.xo" />
      </div>
      {recs.length > 0 && view.running !== "recommend_xo" && (
        <p className="calibrate-line tabular" data-ui="calibrate.xo.state">
          {t("calibrate.xo.rec_done", { list: recs.join(" · ") })}
          {view.xoRecommendation &&
            ` ${t("calibrate.xo.rec_candidates", { list: view.xoRecommendation.candidates.map((c) => number(c)).join(", ") })}`}
        </p>
      )}
      {/* The values stay; this says they were recommended for inputs that have changed since. */}
      {recs.length > 0 && view.running !== "recommend_xo" && view.results.xo_rec_stale && (
        <p className="calibrate-hint calibrate-hint--warning" data-ui="calibrate.xo.rec_stale">
          {t("calibrate.xo.rec_stale")}
        </p>
      )}
      <Advanced id="calibrate.xo.advanced" count={3}>
        <Toggle
          id="calibrate.xo.peq"
          label={t("calibrate.xo.peq")}
          help={t("calibrate.xo.peq.hint")}
          checked={s.xo_peq_on}
          onChange={(on) => actions.changeSetting({ kind: "xo_peq", on })}
        />
        {s.xo_peq_on && (
          <div className="calibrate-fields">
            <NumberField
              id="calibrate.xo.peq_lo"
              label={t("calibrate.xo.peq_lo")}
              value={s.xo_peq_lo}
              min={LIMITS.xo_peq[0]}
              max={LIMITS.xo_peq[1]}
              step={1}
              unit="Hz"
              help={t("calibrate.xo.peq_lo.hint")}
              onChange={(lo) => actions.changeSetting({ kind: "xo_peq", lo })}
            />
            <NumberField
              id="calibrate.xo.peq_hi"
              label={t("calibrate.xo.peq_hi")}
              value={s.xo_peq_hi}
              min={LIMITS.xo_peq[0]}
              max={LIMITS.xo_peq[1]}
              step={1}
              unit="Hz"
              help={t("calibrate.xo.peq_hi.hint")}
              onChange={(hi) => actions.changeSetting({ kind: "xo_peq", hi })}
            />
          </div>
        )}
      </Advanced>
    </SettingsCard>
  );
}

/* ── 서브 정렬 (BA) ─────────────────────────────────────────────────────── */

const MODE_CHOICES: readonly SubModeChoice[] = ["auto", "fixed", "bundle:A", "bundle:B"];
const CHOICE_TEXT = {
  auto: "calibrate.ba.choice.auto",
  fixed: "calibrate.ba.choice.fixed",
  "bundle:A": "calibrate.ba.choice.bundle_a",
  "bundle:B": "calibrate.ba.choice.bundle_b",
} as const satisfies Record<SubModeChoice, string>;
const LAYOUTS: readonly SubLayout[] = ["values", "dba", "multi"];

function SubSwatch({ token }: { token: string | undefined }) {
  return <span className="calibrate-swatch calibrate-swatch--solid" style={{ borderTopColor: `var(${token ?? "--text-muted"})` }} aria-hidden="true" />;
}

function subSummary(t: ReturnType<typeof useT>, number: ReturnType<typeof useNumber>, settings: CalibrationSettings, role: string): string {
  const e = subOf(settings, role);
  const parts: string[] = [];
  if (e.mode === "bundle") {
    const layout = layoutOf(settings, e.bundle);
    if (layout === "dba") parts.push(t(e.dba_row === "rear" ? "calibrate.ba.summary.dba_rear" : "calibrate.ba.summary.dba_front"));
    if (layout === "multi") parts.push(t("calibrate.ba.summary.multi"));
  }
  const spec = filterSpec(t, number, [e.hp_hz, e.hp_type], [e.lp_hz, e.lp_type], e.peq);
  if (spec) parts.push(spec);
  if (!e.mimo_support) parts.push(t("calibrate.ba.summary.no_mimo"));
  return parts.join(" · ");
}

function SubEditor({ view, actions, role }: Props & { role: string }) {
  const t = useT();
  const s = view.settings;
  const e = subOf(s, role);
  const change = (patch: Extract<SettingEdit, { kind: "sub" }>["patch"]) => actions.changeSetting({ kind: "sub", role, patch });
  const bandOptions: Option<BandTypeChoice>[] = [{ value: "off", label: t("calibrate.ba.filter.off") }, ...BAND_TYPES.map((type) => ({ value: type, label: bandTypeText(type) }))];
  const layout = e.mode === "bundle" ? layoutOf(s, e.bundle) : "values";
  const allpass = [0, 1].map((k) => e.allpass[k] ?? ([0, ALLPASS_Q_DEFAULT] as [number, number]));
  const setAllpass = (k: number, value: [number, number]) =>
    change({ allpass: allpass.map((ap, i) => (i === k ? value : ap)).filter(([hz]) => hz > 0) });
  const peq = [0, 1, 2].map((k) => e.peq[k] ?? ([0, 1, 0] as [number, number, number]));
  const setPeq = (k: number, value: [number, number, number]) => change({ peq: peq.map((band, i) => (i === k ? value : band)).filter(([hz]) => hz > 0) });
  return (
    <div className="calibrate-sub-editor" data-ui="calibrate.ba.editor">
      {e.mode === "bundle" && (
        <div className="calibrate-fields">
          <Select
            id="calibrate.ba.layout"
            label={t("calibrate.ba.layout", { bundle: e.bundle })}
            value={layout}
            options={LAYOUTS.map((value) => ({ value, label: t(`calibrate.ba.layout.${value}`) }))}
            onChange={(value) => actions.changeSetting({ kind: "sub_layout", bundle: e.bundle, layout: value })}
          />
          {layout === "dba" && (
            <>
              <Segmented
                id="calibrate.ba.dba_row"
                label={t("calibrate.ba.dba_row")}
                value={e.dba_row === "rear" ? "rear" : "front"}
                options={[
                  { value: "front", label: t("calibrate.ba.dba_row.front") },
                  { value: "rear", label: t("calibrate.ba.dba_row.rear") },
                ]}
                onChange={(row) => change({ dba_row: row })}
              />
              <NumberField
                id="calibrate.ba.rear_trim"
                label={t("calibrate.ba.rear_trim")}
                value={s.bundle_layouts[e.bundle]?.rear_trim_ms ?? 0}
                min={LIMITS.rear_trim[0]}
                max={LIMITS.rear_trim[1]}
                step={0.1}
                digits={2}
                unit="ms"
                help={t("calibrate.ba.rear_trim.hint")}
                onChange={(ms) => actions.changeSetting({ kind: "rear_trim", bundle: e.bundle, ms })}
              />
            </>
          )}
        </div>
      )}
      {subValuesApply(s, e) ? (
        <fieldset className="calibrate-group" data-ui="calibrate.ba.values">
          <legend className="ui-field__label">{t("calibrate.ba.values")}</legend>
          <div className="calibrate-fields">
            <NumberField id="calibrate.ba.gain" label={t("calibrate.ba.gain")} value={e.gain_db} min={SUB_LIMITS.gain[0]} max={SUB_LIMITS.gain[1]} step={0.5} digits={2} unit="dB" onChange={(gain_db) => change({ gain_db })} />
            <NumberField id="calibrate.ba.delay" label={t("calibrate.ba.delay")} value={e.delay_ms} min={SUB_LIMITS.delay[0]} max={SUB_LIMITS.delay[1]} step={0.1} digits={2} unit="ms" onChange={(delay_ms) => change({ delay_ms })} />
            <Segmented
              id="calibrate.ba.polarity"
              label={t("calibrate.ba.polarity")}
              value={e.polarity < 0 ? "inverted" : "normal"}
              options={[
                { value: "normal", label: t("calibrate.polarity.normal") },
                { value: "inverted", label: t("calibrate.polarity.inverted") },
              ]}
              onChange={(value) => change({ polarity: value === "inverted" ? -1 : 1 })}
            />
          </div>
          {allpass.map(([hz, q], k) => (
            <div key={k} className="calibrate-fields calibrate-fields--tight">
              <NumberField
                id={`calibrate.ba.allpass.${k + 1}.hz`}
                label={t("calibrate.ba.allpass", { n: k + 1 })}
                value={hz}
                min={0}
                max={SUB_LIMITS.apHz[1]}
                step={1}
                unit="Hz"
                size={5}
                hint={k === 0 ? t("calibrate.ba.zero_none") : undefined}
                onChange={(value) => setAllpass(k, [value, q])}
              />
              <NumberField id={`calibrate.ba.allpass.${k + 1}.q`} label="Q" value={q} min={SUB_LIMITS.apQ[0]} max={SUB_LIMITS.apQ[1]} step={0.01} digits={2} size={5} onChange={(value) => setAllpass(k, [hz, value])} />
            </div>
          ))}
        </fieldset>
      ) : (
        <p className="calibrate-hint" data-ui="calibrate.ba.values_auto">
          {t(e.mode === "auto" ? "calibrate.ba.values.auto" : "calibrate.ba.values.layout")}
        </p>
      )}
      <fieldset className="calibrate-group" data-ui="calibrate.ba.fixed">
        <legend className="ui-field__label ui-heading-help"><span id="calibrate.ba.fixed.label">{t("calibrate.ba.fixed")}</span><HelpTip id="calibrate.ba.fixed.help" labelledBy="calibrate.ba.fixed.label" content={t("calibrate.ba.fixed.hint")} /></legend>
        {(["hp", "lp"] as const).map((band) => {
          const on = (band === "hp" ? e.hp_hz : e.lp_hz) > 0;
          return (
            <div key={band} className="calibrate-fields calibrate-fields--tight">
              <Select
                id={`calibrate.ba.${band}_type`}
                label={t(`calibrate.ba.${band}`)}
                value={(on ? (band === "hp" ? e.hp_type : e.lp_type) : "off") as BandTypeChoice}
                options={bandOptions}
                onChange={(type) => actions.changeSetting({ kind: "sub_band_type", role, band, type })}
              />
              {on && (
                <NumberField
                  id={`calibrate.ba.${band}`}
                  label={t("calibrate.ba.filter.hz")}
                  value={band === "hp" ? e.hp_hz : e.lp_hz}
                  min={SUB_LIMITS.filterHz[0]}
                  max={SUB_LIMITS.filterHz[1]}
                  step={1}
                  unit="Hz"
                  size={5}
                  onChange={(hz) => change(band === "hp" ? { hp_hz: hz } : { lp_hz: hz })}
                />
              )}
            </div>
          );
        })}
        {peq.map(([hz, q, gain], k) => (
          <div key={k} className="calibrate-fields calibrate-fields--tight">
            <NumberField
              id={`calibrate.ba.peq.${k + 1}.hz`}
              label={t("calibrate.ba.peq", { n: k + 1 })}
              value={hz}
              min={0}
              max={SUB_LIMITS.apHz[1]}
              step={1}
              unit="Hz"
              size={5}
              hint={k === 0 ? t("calibrate.ba.zero_none") : undefined}
              onChange={(value) => setPeq(k, [value, q, gain])}
            />
            <NumberField id={`calibrate.ba.peq.${k + 1}.q`} label="Q" value={q} min={SUB_LIMITS.apQ[0]} max={SUB_LIMITS.apQ[1]} step={0.01} digits={2} size={5} onChange={(value) => setPeq(k, [hz, value, gain])} />
            <NumberField
              id={`calibrate.ba.peq.${k + 1}.gain`}
              label={t("calibrate.ba.peq.gain")}
              value={gain}
              min={SUB_LIMITS.peqGain[0]}
              max={SUB_LIMITS.peqGain[1]}
              step={0.5}
              digits={1}
              unit="dB"
              size={5}
              onChange={(value) => setPeq(k, [hz, q, value])}
            />
          </div>
        ))}
      </fieldset>
      <Toggle
        id="calibrate.ba.mimo_support"
        label={t("calibrate.ba.mimo_support")}
        help={t("calibrate.ba.mimo_support.hint")}
        checked={e.mimo_support}
        onChange={(on) => change({ mimo_support: on })}
      />
    </div>
  );
}

function BaSection({ view, actions, onSampleRateSettings, editSub }: FailureProps & { editSub?: string | undefined }) {
  const t = useT();
  const number = useNumber();
  const s = view.settings;
  const subs = subRoles(view.config);
  const [editing, setEditing] = useState<string | null>(editSub ?? null);
  if (subs.length < 2) {
    return (
      <SettingsCard id="ba" title={t("calibrate.ba.title")} help={<>{t("calibrate.ba.description")}<p>{t("calibrate.ba.actions.hint")}</p></>}>
        <p className="calibrate-hint" data-ui="calibrate.ba.none">
          {t("calibrate.ba.none", { count: subs.length })}
        </p>
      </SettingsCard>
    );
  }
  const tokens = colorTokens(subs, () => true);
  const role = editing && subs.includes(editing) ? editing : subs[0]!;
  const infra = infraState(s, subs);
  const warnings = view.subWarnings;
  const result = view.results.calibration;
  const canTake = !!result && result.sub_roles.length > 0;
  const busy = view.running !== null;
  const anyFixed = subs.some((r) => subOf(s, r).mode === "fixed");
  return (
    <SettingsCard id="ba" title={t("calibrate.ba.title")} help={<>{t("calibrate.ba.description")}<p>{t("calibrate.ba.actions.hint")}</p></>}>
      <div className="calibrate-sub-modes" data-ui="calibrate.ba.subs">
        {subs.map((r) => (
          <Select
            key={r}
            id={`calibrate.ba.mode.${r}`}
            label={
              <span className="calibrate-sub-name">
                <SubSwatch token={tokens.get(r)} />
                {t("calibrate.ba.mode.label", { code: roleCode(r) })}
              </span>
            }
            value={subModeChoice(subOf(s, r))}
            options={MODE_CHOICES.map((value) => ({ value, label: t(CHOICE_TEXT[value]) }))}
            hint={subSummary(t, number, s, r) || undefined}
            onChange={(mode) => actions.changeSetting({ kind: "sub_mode", role: r, mode })}
          />
        ))}
      </div>
      <div className="calibrate-actions">
        <Button
          id="calibrate.ba.compute_subs"
          data-ui="calibrate.ba.compute_subs"
          busy={view.running === "calibrate_subs"}
          disabled={busy || view.positions.length === 0}
          onClick={actions.calibrateSubs}
        >
          {t("calibrate.ba.compute_subs")}
        </Button>
        <Button id="calibrate.ba.take_values" data-ui="calibrate.ba.take_values" variant="quiet" disabled={busy || !canTake} onClick={actions.takeSubValues}>
          {t("calibrate.ba.take_values")}
        </Button>
        <JobFailure view={view} actions={actions} onSampleRateSettings={onSampleRateSettings} job="calibrate_subs" id="calibrate.ba" />
      </div>
      {!canTake && <p className="ui-field__hint">{t("calibrate.ba.actions.hint_no_result")}</p>}
      {warnings.length > 0 && (
        <ul className="calibrate-warnings" data-ui="calibrate.ba.settings_warnings">
          {warnings.map((w) => (
            <li key={`${w.code}.${w.params.role ?? ""}`}>
              <Icon name="warning" size={16} />
              <span>{noticeText(t, number, w)}</span>
            </li>
          ))}
        </ul>
      )}
      <Advanced id="calibrate.ba.advanced" defaultOpen={editSub !== undefined}>
        <fieldset className="calibrate-group" data-ui="calibrate.ba.infra_group">
          <legend className="ui-field__label">{t("calibrate.ba.infra_title")}</legend>
          <div className="calibrate-fields">
            <Select
              id="calibrate.ba.infra"
              label={t("calibrate.ba.infra")}
              help={t("calibrate.ba.infra.hint")}
              value={infra.role ?? ""}
              options={[{ value: "", label: t("calibrate.ba.infra.none") }, ...subs.map((r) => ({ value: r, label: roleCode(r) }))]}
              onChange={(value) =>
                actions.changeSetting({ kind: "infra", role: value || null, hz: infra.hz || INFRA_DEFAULT_HZ, type: infra.type, subs })
              }
            />
            {infra.role && (
              <>
                <NumberField
                  id="calibrate.ba.infra_hz"
                  label={t("calibrate.ba.infra_hz")}
                  value={infra.hz}
                  min={LIMITS.infra_hz[0]}
                  max={LIMITS.infra_hz[1]}
                  step={1}
                  unit="Hz"
                  size={5}
                  onChange={(hz) => actions.changeSetting({ kind: "infra", role: infra.role, hz, type: infra.type, subs })}
                />
                <Select
                  id="calibrate.ba.infra_type"
                  label={t("calibrate.ba.infra_type")}
                  value={infra.type}
                  options={BAND_TYPES.map((type) => ({ value: type, label: bandTypeText(type) }))}
                  onChange={(type: BandType) => actions.changeSetting({ kind: "infra", role: infra.role, hz: infra.hz, type, subs })}
                />
                <NumberField
                  id="calibrate.ba.infra_boost"
                  label={t("calibrate.ba.infra_boost")}
                  value={s.infra_boost_max_db}
                  min={LIMITS.infra_boost[0]}
                  max={LIMITS.infra_boost[1]}
                  step={1}
                  digits={1}
                  unit="dB"
                  size={5}
                  onChange={(db) => actions.changeSetting({ kind: "infra_boost", db })}
                />
              </>
            )}
          </div>
          {infra.role && (
            <p className="calibrate-line tabular" data-ui="calibrate.ba.infra_note">
              {t("calibrate.ba.infra.note", {
                infra: roleCode(infra.role),
                hz: number(infra.hz),
                others: subs.filter((r) => r !== infra.role).map(roleCode).join(", "),
              })}
            </p>
          )}
        </fieldset>
        <Segmented
          id="calibrate.ba.edit"
          label={t("calibrate.ba.edit")}
          value={role}
          options={subs.map((r) => ({ value: r, label: roleCode(r) }))}
          onChange={setEditing}
        />
        <SubEditor view={view} actions={actions} role={role} />
        {anyFixed && (
          <div className="calibrate-fields">
            <NumberField
              id="calibrate.ba.main_delay"
              label={t("calibrate.ba.main_delay")}
              value={s.sub_main_delay_ms}
              min={LIMITS.main_delay[0]}
              max={LIMITS.main_delay[1]}
              step={0.1}
              digits={2}
              unit="ms"
              help={t("calibrate.ba.main_delay.hint")}
              onChange={(ms) => actions.changeSetting({ kind: "main_delay", ms })}
            />
          </div>
        )}
      </Advanced>
    </SettingsCard>
  );
}

/* ── 저음 도착과 울림 (ADR 0025) ────────────────────────────────────────── */

/**
 * Two official alternatives on the main screen, both off by default. Zero
 * mode does not change phase, so 저음 도착 맞추기 is off there with the
 * reason; the stored choice is kept and comes back with another mode. In
 * Mixed mode its short delay aligns only part of the way, which the switch
 * says while it is on.
 */
function LfSection({ view, actions }: Props) {
  const t = useT();
  const s = view.settings;
  const zero = s.mode === "zero";
  const align = !zero && s.lf_align;
  const alignHint = zero ? (
    <span data-ui="calibrate.lf_align.unavailable">{t("calibrate.lf_align.unavailable_zero", { mode: t(MODE_NAME.zero) })}</span>
  ) : align && s.mode === "mixed" ? (
    <span data-ui="calibrate.lf_align.partial">{t("calibrate.lf_align.mixed", { mode: t(MODE_NAME.mixed) })}</span>
  ) : undefined;
  return (
    <SettingsCard id="lf" title={t("calibrate.lf.title")} help={t("calibrate.lf.description")}>
      <Toggle
        id="calibrate.lf_align"
        label={t("calibrate.lf_align")}
        help={t("calibrate.lf_align.help")}
        hint={alignHint}
        checked={align}
        disabled={zero}
        onChange={(on) => actions.changeSetting({ kind: "lf_align", on })}
      />
      <Toggle
        id="calibrate.mode_decay"
        label={t("calibrate.mode_decay")}
        help={t("calibrate.mode_decay.help")}
        checked={s.mode_decay}
        onChange={(on) => actions.changeSetting({ kind: "mode_decay", on })}
      />
    </SettingsCard>
  );
}

/* ── MIMO ───────────────────────────────────────────────────────────────── */

function MimoSection({ view, actions, onSampleRateSettings }: FailureProps) {
  const t = useT();
  const number = useNumber();
  const s = view.settings;
  const count = view.positions.length;
  const available = count >= MIMO_MIN_POSITIONS;
  const on = available && s.mimo_on;
  const result = view.results.calibration;
  const mimo = view.results.mimo;
  const busy = view.running !== null;
  const state = (): ReactNode => {
    if (view.running === "mimo" || view.runningMimo || view.failure?.job === "mimo") return null;
    if (!result) return <p className="calibrate-hint" data-ui="calibrate.mimo.reason">{t("calibrate.mimo.needs_result")}</p>;
    if (!mimo) return <p className="calibrate-hint" data-ui="calibrate.mimo.state">{t("calibrate.mimo.state.none")}</p>;
    if (mimo.stale) {
      // A changed correction needs 다시 보정하기, which computes MIMO with it; with the correction current only MIMO is behind.
      return (
        <p className="calibrate-hint calibrate-hint--warning" data-ui="calibrate.mimo.state">
          {t(result.stale ? "calibrate.mimo.state.stale.calibration" : "calibrate.mimo.state.stale")}
        </p>
      );
    }
    const parts = [t("calibrate.mimo.state.done", { kind: mimo.kind.toUpperCase(), count: mimo.support_count })];
    if (s.mimo_max_supports > 0) parts.push(t("calibrate.mimo.state.cap", { cap: s.mimo_max_supports }));
    if (mimo.kind === "fir" && view.firBudget > 0) {
      // The supports are designed at 48 kHz; the DSP's budget counts taps at its output rate.
      const taps = tapsAt(mimo.fir_taps_total, view.outputRate);
      const over = taps > view.firBudget;
      const params = { taps: number(taps), budget: number(view.firBudget), rate: rateText(number, view.outputRate) };
      parts.push(
        view.outputRate === DESIGN_RATE
          ? t(over ? "calibrate.mimo.state.budget_over" : "calibrate.mimo.state.budget", params)
          : t(over ? "calibrate.mimo.state.budget_over.rate" : "calibrate.mimo.state.budget.rate", params),
      );
    }
    return (
      <p className="calibrate-line tabular" data-ui="calibrate.mimo.state">
        {parts.join(" · ")}
      </p>
    );
  };
  return (
    <SettingsCard id="mimo" title={t("calibrate.mimo.title")} help={<>{t("calibrate.mimo.description")}<p>{t("calibrate.mimo.compute.help")}</p></>}>
      <Toggle
        id="calibrate.mimo.on"
        label={t("calibrate.mimo.on")}
        help={t("calibrate.mimo.on.hint")}
        hint={!available ? t("calibrate.mimo.unavailable", { count, min: MIMO_MIN_POSITIONS }) : undefined}
        checked={on}
        disabled={!available}
        onChange={(value) => actions.changeSetting({ kind: "mimo_on", on: value })}
      />
      {on && (
        <>
          <ComputeControl view={view} actions={actions} />
          <div className="calibrate-fields">
            <NumberField
              id="calibrate.mimo.fsiso"
              label={t("calibrate.mimo.fsiso")}
              value={s.fsiso}
              min={LIMITS.fsiso[0]}
              max={LIMITS.fsiso[1]}
              step={5}
              unit="Hz"
              help={t("calibrate.mimo.fsiso.hint")}
              onChange={(hz) => actions.changeSetting({ kind: "fsiso", hz })}
            />
          </div>
          <div className="calibrate-actions">
            {/* MIMO alone, on the current correction: the service refuses it on a stale one (다시 보정하기 computes both). */}
            <Button id="calibrate.mimo.compute" data-ui="calibrate.mimo.compute" busy={view.running === "mimo"} disabled={busy || !result || result.stale || view.compute.saving || (view.compute.preference === "gpu" && view.compute.status?.gpu_available === false)} onClick={actions.computeMimo}>
              {t("calibrate.mimo.compute")}
            </Button>
            <JobFailure view={view} actions={actions} onSampleRateSettings={onSampleRateSettings} job="mimo" id="calibrate.mimo" />
          </div>
          {state()}
          {mimo && <ComputeResult report={mimo.compute} />}
          <Advanced id="calibrate.mimo.advanced" count={3}>
            <Toggle
              id="calibrate.mimo.low_auto"
              label={t("calibrate.mimo.low_auto")}
              help={t("calibrate.mimo.low_auto.hint")}
              checked={s.mimo_low === null}
              onChange={(auto) => actions.changeSetting({ kind: "mimo_low", hz: auto ? null : 20 })}
            />
            <div className="calibrate-fields">
              {s.mimo_low !== null && (
                <NumberField
                  id="calibrate.mimo.low"
                  label={t("calibrate.mimo.low")}
                  value={s.mimo_low}
                  min={LIMITS.mimo_low[0]}
                  max={LIMITS.mimo_low[1]}
                  step={5}
                  unit="Hz"
                  showRange
                  onChange={(hz) => actions.changeSetting({ kind: "mimo_low", hz })}
                />
              )}
              <NumberField
                id="calibrate.mimo.max"
                label={t("calibrate.mimo.max")}
                value={s.mimo_max_supports}
                min={LIMITS.mimo_max[0]}
                max={LIMITS.mimo_max[1]}
                step={1}
                help={t("calibrate.mimo.max.hint")}
                hint={t("calibrate.mimo.max.zero")}
                onChange={(value) => actions.changeSetting({ kind: "mimo_max", count: value })}
              />
            </div>
            <Toggle
              id="calibrate.mimo.fir"
              label={t("calibrate.mimo.fir")}
              help={t("calibrate.mimo.fir.hint")}
              checked={s.mimo_fir}
              onChange={(value) => actions.changeSetting({ kind: "mimo_fir", on: value })}
            />
          </Advanced>
        </>
      )}
    </SettingsCard>
  );
}

export function Settings({
  view,
  actions,
  editSub,
  rangeAdvanced,
  sampleRateRequest,
  onSampleRateSettings,
}: FailureProps & { editSub?: string | undefined; rangeAdvanced?: boolean | undefined; sampleRateRequest: number }) {
  const t = useT();
  return (
    <section className="calibrate-settings" aria-labelledby="calibrate.settings.title" data-ui="calibrate.settings">
      <header className="calibrate-settings__head">
        <div className="ui-heading-help">
          <h2 className="calibrate-settings__title" id="calibrate.settings.title">{t("calibrate.settings.title")}</h2>
          <HelpTip id="calibrate.settings.help" labelledBy="calibrate.settings.title" content={t("calibrate.settings.description")} />
        </div>
      </header>
      <SampleRateSettings view={view} actions={actions} request={sampleRateRequest} />
      <ModeSection view={view} actions={actions} />
      <RangeSection view={view} actions={actions} advancedOpen={rangeAdvanced} />
      <XoSection view={view} actions={actions} onSampleRateSettings={onSampleRateSettings} />
      <BaSection view={view} actions={actions} onSampleRateSettings={onSampleRateSettings} editSub={editSub} />
      <LfSection view={view} actions={actions} />
      <MimoSection view={view} actions={actions} onSampleRateSettings={onSampleRateSettings} />
    </section>
  );
}
