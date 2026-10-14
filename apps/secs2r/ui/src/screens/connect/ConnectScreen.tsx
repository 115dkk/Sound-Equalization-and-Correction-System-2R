// 연결·점검: choose how to measure, then check every channel's level
// (upstream steps 1 and 3).
//
// 측정 방식 comes first (자체 측정, REW, 모의 측정, or mdat files only, which
// skips the step). REW gets its connection card; native and REW get the
// device and mic fields; then the level check plays each channel in turn
// while the mic meter shows what arrives.
import { type ReactNode } from "react";
import type { PrimaryAction } from "../../app/StepHeader";
import { errorText } from "../../app/jobs/jobText";
import { usePrefs } from "../../app/prefs";
import type { MessageKey } from "../../i18n";
import type { Translate } from "../../i18n/react";
import { useNumber, useT } from "../../i18n/react";
import type { MeasureDevice } from "../../ipc/bindings/MeasureDevice";
import { Advanced, Button, Card, ErrorNotice, Icon, IconButton, LevelMeter, NumberField, Section, Segmented, Select, StatusPill, Tooltip, type Option, type PillTone } from "../../ui";
import { roleCode, roleKind } from "../../ui/maps/geometry";
import { roleName } from "../system/text";
import { MicMeterPanel, type MicMeterView } from "./MicMeterPanel";
import { levelRoles, missingLevels, pathReady } from "./state";
import type { ConnectActions, ConnectPath, ConnectView, RewProblem } from "./types";
import "./connect.css";

/** REW's sweep lengths (secs-project SWEEP_LENGTHS) in samples; 256k by default. */
export const SWEEP_LENGTHS = [
  { samples: 65536, name: "64k" },
  { samples: 131072, name: "128k" },
  { samples: 262144, name: "256k" },
  { samples: 524288, name: "512k" },
  { samples: 1048576, name: "1M" },
  { samples: 2097152, name: "2M" },
  { samples: 4194304, name: "4M" },
] as const;

export const DEFAULT_SWEEP = 262144;

const PATHS: readonly ConnectPath[] = ["native", "rew", "mock", "mdat"];

const PATH_TEXT: Record<ConnectPath, { name: MessageKey; needs: MessageKey }> = {
  native: { name: "connect.method.native", needs: "connect.method.native.needs" },
  rew: { name: "connect.method.rew", needs: "connect.method.rew.needs" },
  mock: { name: "connect.method.mock", needs: "connect.method.mock.needs" },
  mdat: { name: "connect.method.mdat", needs: "connect.method.mdat.needs" },
};

const PROBLEM_TEXT: Record<RewProblem, { cause: MessageKey; next: MessageKey }> = {
  exe_not_found: { cause: "connect.rew.problem.exe_not_found", next: "connect.rew.problem.manual" },
  cannot_launch: { cause: "connect.rew.problem.cannot_launch", next: "connect.rew.problem.manual" },
  timed_out: { cause: "connect.rew.problem.timed_out", next: "connect.rew.problem.timed_out.next" },
  launch_failed: { cause: "connect.rew.problem.launch_failed", next: "connect.rew.problem.manual" },
};

/** Whether a path is usable now, as a pill. */
function pathStatus(t: Translate, view: ConnectView, path: ConnectPath): { tone: PillTone; text: string } {
  const methods = view.methods;
  switch (path) {
    case "native": {
      if (!methods) return { tone: "busy", text: t("connect.method.checking") };
      if (methods.methods.includes("native")) return { tone: "success", text: t("connect.method.native.ready", { count: methods.native_devices.length }) };
      if (methods.native_error) return { tone: "danger", text: t("connect.method.native.error") };
      return { tone: "warning", text: t("connect.method.native.none") };
    }
    case "rew": {
      if (!methods || view.rew.launch !== "idle") return { tone: "busy", text: t("connect.method.checking") };
      const rew = methods.rew;
      if (rew.reachable && rew.pro) return { tone: "success", text: t("connect.method.rew.ready") };
      if (rew.reachable) return { tone: "warning", text: t("connect.method.rew.not_pro") };
      if (rew.running) return { tone: "warning", text: t("connect.method.rew.no_api") };
      return { tone: "neutral", text: t("connect.method.rew.off") };
    }
    case "mock":
      return { tone: "success", text: t("connect.method.mock.ready", { count: methods?.mock_positions.length ?? 9 }) };
    case "mdat":
      return { tone: "neutral", text: t("connect.method.mdat.status") };
  }
}

export function ConnectScreen({ view, actions }: { view: ConnectView; actions: ConnectActions }) {
  const path = view.path;
  return (
    <div className="connect-screen" data-ui="connect">
      <MethodCard view={view} actions={actions} />
      {path === "mdat" && <MdatCard />}
      {(path === "rew" || path === "native") && (
        <div className="connect-pair">
          {path === "rew" && <RewCard view={view} actions={actions} />}
          <DevicesCard view={view} actions={actions} />
        </div>
      )}
      {path === "mock" && <MockCard view={view} />}
      {path !== null && path !== "mdat" && pathReady(view) && <LevelCard view={view} actions={actions} />}
    </div>
  );
}

/** The step header: plays every channel in turn; on the mdat path, opens 측정·가져오기. */
export function connectHeader(view: ConnectView, actions: ConnectActions, t: Translate): { primary?: PrimaryAction; status?: ReactNode } {
  if (view.path === "mdat") {
    return {
      primary: { id: "connect.next", label: t("connect.primary.next"), onClick: actions.next },
      status: <StatusPill data-ui="connect.status">{t("connect.status.skipped")}</StatusPill>,
    };
  }
  // Until a method is chosen and ready, the next action is in the method or
  // REW card (the REW launch is then the screen's filled button), not here.
  if (view.path === null || !pathReady(view)) return {};
  const roles = levelRoles(view);
  const missing = missingLevels(view).length;
  const status =
    roles.length === 0 ? undefined : missing === 0 ? (
      <StatusPill tone="success" data-ui="connect.status">
        {t("connect.status.levels_done")}
      </StatusPill>
    ) : (
      <StatusPill tone="info" data-ui="connect.status">
        {t("connect.status.levels", { done: roles.length - missing, total: roles.length })}
      </StatusPill>
    );
  return {
    primary: { id: "connect.level.play_all", label: t("frame.step.connect.primary"), onClick: actions.playAllLevels, busy: view.level.playingAll },
    status,
  };
}

/* ── 측정 방식 ─────────────────────────────────────────────────────────── */

function MethodCard({ view, actions }: { view: ConnectView; actions: ConnectActions }) {
  const t = useT();
  return (
    <Card title={t("connect.method.title")} description={view.path === null ? t("connect.method.description") : undefined} data-ui="connect.method.card">
      <div className={view.path === null ? "connect-methods" : "connect-methods connect-methods--compact"} role="group" aria-label={t("connect.method.title")} data-ui="connect.method">
        {PATHS.map((path) => {
          const status = pathStatus(t, view, path);
          const current = view.path === path;
          const needs = t(PATH_TEXT[path].needs);
          const button = (
            <button
              key={path}
              type="button"
              className="connect-method"
              aria-pressed={current}
              data-ui={`connect.method.${path}`}
              onClick={() => !current && actions.choosePath(path)}
            >
              <span className="connect-method__name">{t(PATH_TEXT[path].name)}</span>
              {view.path === null && <span className="connect-method__needs">{needs}</span>}
              <StatusPill tone={status.tone} className="connect-method__status">
                {status.text}
              </StatusPill>
            </button>
          );
          // Once a method is chosen the choices are compact; what each needs moves to its tooltip.
          return view.path === null ? (
            button
          ) : (
            <Tooltip key={path} content={needs}>
              {button}
            </Tooltip>
          );
        })}
      </div>
    </Card>
  );
}

function MdatCard() {
  const t = useT();
  return (
    <Card title={t("connect.mdat.title")} data-ui="connect.mdat">
      <p className="connect-text">{t("connect.mdat.body")}</p>
    </Card>
  );
}

function MockCard({ view }: { view: ConnectView }) {
  const t = useT();
  const methods = view.methods;
  return (
    <Card title={t("connect.mock.title")} data-ui="connect.mock">
      <p className="connect-text">
        {t("connect.mock.body", { channels: methods?.mock_roles.length ?? 13, positions: methods?.mock_positions.length ?? 9 })}
      </p>
    </Card>
  );
}

/* ── REW ───────────────────────────────────────────────────────────────── */

function RewCard({ view, actions }: { view: ConnectView; actions: ConnectActions }) {
  const t = useT();
  const number = useNumber();
  const status = view.methods?.rew ?? { running: false, reachable: false, pro: false };
  const { launch, problem, details } = view.rew;
  const pill =
    launch === "checking" ? (
      <StatusPill tone="busy">{t("connect.rew.checking")}</StatusPill>
    ) : launch === "launching" ? (
      <StatusPill tone="busy">{t("connect.rew.launching")}</StatusPill>
    ) : status.reachable ? (
      <StatusPill tone="success">{t("connect.rew.connected")}</StatusPill>
    ) : status.running ? (
      <StatusPill tone="warning">{t("connect.rew.no_api")}</StatusPill>
    ) : (
      <StatusPill tone="warning">{t("connect.rew.off")}</StatusPill>
    );
  const pro = status.pro ? (
    <StatusPill tone="success">{t("connect.rew.pro.ok")}</StatusPill>
  ) : status.reachable ? (
    <StatusPill tone="warning">{t("connect.rew.pro.no")}</StatusPill>
  ) : (
    <StatusPill>{t("connect.rew.pro.unknown")}</StatusPill>
  );
  const unknown = t("connect.rew.unknown");
  const hint: { cause: string; next?: string; detail?: string; tone: "danger" | "warning" } | null =
    launch === "launching"
      ? null
      : problem
        ? { cause: t(PROBLEM_TEXT[problem].cause), next: t(PROBLEM_TEXT[problem].next), tone: "danger", ...(view.rew.problemDetail ? { detail: view.rew.problemDetail } : {}) }
        : status.reachable && !status.pro
          ? { cause: t("connect.rew.hint.not_pro"), next: t("connect.rew.hint.not_pro.next"), tone: "warning" }
          : status.running && !status.reachable
            ? { cause: t("connect.rew.hint.no_api"), next: t("connect.rew.hint.no_api.next"), tone: "warning" }
            : !status.running
              ? { cause: t("connect.rew.hint.off"), next: t("connect.rew.hint.off.next"), tone: "warning" }
              : null;
  const settings = details.settings;
  return (
    <Card title={t("connect.rew.title")} actions={<span data-ui="connect.rew.status">{pill}</span>} data-ui="connect.rew">
      <dl className="connect-facts">
        <div className="connect-facts__row">
          <dt>{t("connect.rew.executable")}</dt>
          <dd className="connect-facts__path" data-ui="connect.rew.executable">
            {details.executable === null ? t("connect.rew.not_found") : (details.executable ?? unknown)}
          </dd>
        </div>
        <div className="connect-facts__row">
          <dt>{t("connect.rew.version")}</dt>
          <dd className="tabular" data-ui="connect.rew.version">
            {details.version ?? "—"}
          </dd>
        </div>
        <div className="connect-facts__row">
          <dt>{t("connect.rew.pro")}</dt>
          <dd data-ui="connect.rew.pro">{pro}</dd>
        </div>
        <div className="connect-facts__row">
          <dt>{t("connect.rew.blocking")}</dt>
          <dd data-ui="connect.rew.blocking">{details.blocking === true ? t("connect.rew.blocking.on") : details.blocking === false ? t("connect.rew.blocking.off") : "—"}</dd>
        </div>
      </dl>
      <div className="connect-actions">
        <Button data-ui="connect.rew.launch" variant={pathReady(view) ? "secondary" : "primary"} busy={launch !== "idle"} onClick={actions.launchRew}>
          {status.running || problem ? t("connect.rew.recheck") : t("connect.rew.launch")}
        </Button>
        <Button variant="quiet" data-ui="connect.rew.read_settings" busy={view.rew.readingSettings} disabled={!status.reachable} onClick={actions.readRewSettings}>
          {t("connect.rew.read_settings")}
        </Button>
      </div>
      {launch === "launching" && (
        <p className="connect-note" data-ui="connect.rew.hint">
          <Icon name="info" size={18} className="connect-note__icon" />
          <span>{t("connect.rew.hint.launching")}</span>
        </p>
      )}
      {hint && (
        <ErrorNotice id="connect.rew.hint" tone={hint.tone} cause={hint.cause} {...(hint.next ? { next: hint.next } : {})} {...(hint.detail ? { detail: hint.detail } : {})} />
      )}
      {settings && (
        <p className="connect-note tabular" data-ui="connect.rew.settings">
          <Icon name="info" size={18} className="connect-note__icon" />
          <span>
            {t("connect.rew.settings", {
              rate: number(settings.sample_rate),
              length: settings.sweep_length,
              level: settings.level_dbfs === null ? unknown : t("connect.dbfs", { value: number(settings.level_dbfs, 0) }),
            })}
          </span>
        </p>
      )}
    </Card>
  );
}

/* ── 장치와 마이크 ─────────────────────────────────────────────────────── */

function deviceOption(t: Translate, device: MeasureDevice, channels: number): Option<string> {
  return { value: device.id, label: t("connect.device.option", { name: device.name, count: channels }) };
}

function DevicesCard({ view, actions }: { view: ConnectView; actions: ConnectActions }) {
  const t = useT();
  const number = useNumber();
  const { expert } = usePrefs();
  const settings = view.settings;
  const rew = view.path === "rew";
  const methods = view.methods;
  const nativeError = !rew ? (methods?.native_error ?? null) : null;
  const inputs = rew ? (view.rew.details.inputs ?? []) : (methods?.native_devices ?? []).filter((d) => d.input_channels > 0);
  const outputs = rew ? (view.rew.details.outputs ?? []) : (methods?.native_devices ?? []).filter((d) => d.output_channels > 0);
  const empty = rew ? (methods?.rew.reachable ? t("connect.device.unreadable") : t("connect.device.after_rew")) : t("connect.device.none");
  const inputId = settings?.input_device ?? "";
  const outputId = settings?.output_device ?? "";
  const listOf = (devices: readonly MeasureDevice[], kind: "input" | "output", current: string): Option<string>[] => {
    if (devices.length === 0) return [{ value: current, label: empty }];
    const options = devices.map((d) => deviceOption(t, d, kind === "input" ? d.input_channels : d.output_channels));
    const withDefault = rew ? options : [{ value: "", label: t("connect.device.default") }, ...options];
    return withDefault.some((o) => o.value === current) ? withDefault : [...withDefault, { value: current, label: current }];
  };
  const inputDevice = inputs.find((d) => d.id === inputId) ?? (inputId === "" ? inputs[0] : undefined);
  const names = rew ? (view.rew.details.inputChannelNames ?? []) : [];
  const channelCount = Math.max(names.length, inputDevice?.input_channels ?? 8, settings?.input_channel ?? 1, 1);
  const channelOptions = Array.from({ length: channelCount }, (_, i) => ({ value: String(i + 1), label: inputLabel(t, i + 1, names) }));
  const sweep = settings?.sweep_length ?? DEFAULT_SWEEP;
  const sweepOptions = SWEEP_LENGTHS.map((s) => ({
    value: String(s.samples),
    label: t("connect.sweep.option", { name: s.name, seconds: number(s.samples / Math.max(1, view.sampleRate), 1) }),
  }));
  const loopback = settings?.timing === "loopback" && !rew;
  const cal = view.rew.details.micCalibration;
  return (
    <Card title={t("connect.devices.title")} data-ui="connect.devices">
      {nativeError && (
        <ErrorNotice
          id="connect.devices.error"
          cause={errorText(t, nativeError).cause}
          next={errorText(t, nativeError).next}
          detail={nativeError.detail}
          code={nativeError.code}
          action={
            <Button size="sm" data-ui="connect.devices.retry" onClick={actions.refreshMethods}>
              {t("connect.devices.retry")}
            </Button>
          }
        />
      )}
      <div className="connect-fields">
        <Select
          id="connect.input_device"
          label={t("connect.input_device")}
          value={inputId}
          options={listOf(inputs, "input", inputId)}
          disabled={inputs.length === 0}
          onChange={actions.setInputDevice}
        />
        <Select
          id="connect.input_channel"
          label={t("connect.input_channel")}
          value={String(settings?.input_channel ?? 1)}
          options={channelOptions}
          hint={t("connect.input_channel.hint")}
          disabled={inputs.length === 0}
          onChange={(v) => actions.setInputChannel(Number(v))}
        />
        {rew && (
          <div className="ui-field" data-ui="connect.mic_cal">
            <span className="ui-field__label">{t("connect.mic_cal")}</span>
            <span className="connect-readout">
              {cal === "" ? (
                <StatusPill tone="warning">{t("connect.mic_cal.none")}</StatusPill>
              ) : cal ? (
                <StatusPill tone="success" title={cal}>
                  {cal.split(/[\\/]/).pop() ?? cal}
                </StatusPill>
              ) : (
                <StatusPill>{t("connect.rew.unknown")}</StatusPill>
              )}
            </span>
          </div>
        )}
        <Select
          id="connect.output_device"
          label={t("connect.output_device")}
          value={outputId}
          options={listOf(outputs, "output", outputId)}
          hint={t("connect.output_device.hint")}
          disabled={outputs.length === 0}
          onChange={actions.setOutputDevice}
        />
        <div className="ui-field" data-ui="connect.sample_rate">
          <span className="ui-field__label">{t("connect.sample_rate")}</span>
          <span className="connect-readout tabular">{t("connect.sample_rate.value", { rate: number(view.sampleRate) })}</span>
        </div>
        <Select
          id="connect.sweep_length"
          label={t("connect.sweep_length")}
          value={String(sweep)}
          options={sweepOptions}
          hint={t("connect.sweep_length.hint")}
          onChange={(v) => actions.setSweepLength(Number(v))}
        />
        {loopback && (
          <Select
            id="connect.loopback_channel"
            label={t("connect.loopback_channel")}
            value={String(settings?.loopback_input_channel ?? 2)}
            options={channelOptions}
            hint={t("connect.loopback_channel.hint")}
            onChange={(v) => actions.setLoopbackChannel(Number(v))}
          />
        )}
      </div>
      {expert && !rew && (
        <Segmented
          id="connect.share_mode"
          label={t("connect.share_mode")}
          value={settings?.share_mode === "exclusive" || settings?.share_mode === "shared" ? settings.share_mode : "auto"}
          options={[
            { value: "auto", label: t("connect.share_mode.auto") },
            { value: "exclusive", label: t("connect.share_mode.exclusive") },
            { value: "shared", label: t("connect.share_mode.shared") },
          ]}
          onChange={actions.setShareMode}
        />
      )}
      {expert && !rew && <p className="connect-hint">{t("connect.share_mode.hint")}</p>}
    </Card>
  );
}

function inputLabel(t: Translate, n: number, names: readonly string[]): string {
  const name = names[n - 1];
  return name ? t("connect.input.named", { n, name }) : t("connect.input.n", { n });
}

/** "입력 1 · UMIK-1" for the mic meter. */
export function micInput(t: Translate, view: ConnectView): string | null {
  const settings = view.settings;
  if (!settings || view.path === "mdat" || view.path === null) return null;
  const devices = view.path === "rew" ? (view.rew.details.inputs ?? []) : (view.methods?.native_devices ?? []);
  const device = devices.find((d) => d.id === settings.input_device);
  const deviceName = view.path === "mock" ? t("connect.mock.device") : device ? device.name : t("connect.device.default");
  const names = view.path === "rew" ? (view.rew.details.inputChannelNames ?? []) : [];
  return t("connect.mic.input", { channel: inputLabel(t, settings.input_channel, names), device: deviceName });
}

/* ── 레벨 점검 ─────────────────────────────────────────────────────────── */

function LevelCard({ view, actions }: { view: ConnectView; actions: ConnectActions }) {
  const t = useT();
  const number = useNumber();
  const roles = levelRoles(view);
  const missing = missingLevels(view).length;
  const ready = pathReady(view);
  const { level, sweepCheck } = view;
  const busy = level.playing !== null || level.playingAll || sweepCheck.running;
  const meter: MicMeterView = { input: micInput(t, view), monitor: level.monitor, reading: level.reading };
  const firstMain = roles.find((role) => roleKind(role) !== "sub") ?? roles[0] ?? null;
  return (
    <Card title={t("connect.level.title")} description={t("connect.level.description")} data-ui="connect.level">
      <div className="connect-level">
        <ol className="connect-level__rows" aria-label={t("connect.level.rows")}>
          {roles.map((role) => {
            const playing = level.playing === role;
            const stored = view.levels[role];
            const live = playing && level.reading ? level.reading : null;
            const sub = roleKind(role) === "sub";
            return (
              <li key={role} className={["connect-level__row", playing && "connect-level__row--playing"].filter(Boolean).join(" ")} data-ui={`connect.level.row.${role}`}>
                <IconButton
                  icon={playing ? "stop" : "play"}
                  variant="secondary"
                  size="sm"
                  static
                  label={t(playing ? "connect.level.stop.aria" : "connect.level.play.aria", { code: roleCode(role) })}
                  data-ui={`connect.level.play.${role}`}
                  disabled={!ready || (busy && !playing)}
                  onClick={playing ? actions.stopLevel : () => actions.playLevel(role)}
                />
                <LevelMeter
                  id={`connect.level.meter.${role}`}
                  label={
                    <span className="connect-level__label">
                      <span className="connect-level__code">{roleCode(role)}</span>
                      <span className="connect-level__name">{roleName(t, role)}</span>
                      {sub && <span className="connect-level__tone">{t("connect.level.sub_tone")}</span>}
                    </span>
                  }
                  value={live ? live.rms_dbfs : (stored ?? null)}
                  clipping={live?.clipping ?? false}
                  showTicks={false}
                />
              </li>
            );
          })}
        </ol>
        <div className="connect-level__controls">
          <Section title={t("connect.level.output")}>
            <NumberField
              id="connect.level.master"
              label={t("connect.level.master")}
              value={level.masterDbfs}
              min={-40}
              max={0}
              step={0.5}
              digits={1}
              unit="dBFS"
              hint={t("connect.level.master.hint")}
              onChange={actions.setMasterLevel}
            />
            <div className="connect-level__playing">
              {level.playing ? (
                <StatusPill tone="busy" data-ui="connect.level.playing">
                  {t("connect.level.playing", { code: roleCode(level.playing) })}
                </StatusPill>
              ) : (
                <StatusPill data-ui="connect.level.playing">{t("connect.level.idle")}</StatusPill>
              )}
              {(level.playing !== null || level.playingAll) && (
                <Button variant="secondary" size="sm" icon="stop" data-ui="connect.level.stop" onClick={actions.stopLevel}>
                  {t(level.playingAll ? "connect.level.stop_all" : "connect.level.stop")}
                </Button>
              )}
            </div>
          </Section>
          <Section title={t("connect.mic.title")}>
            <MicMeterPanel view={meter} />
          </Section>
        </div>
        <div className="connect-level__extras">
          {missing > 0 && (
            <Section title={t("connect.level.skip.title")}>
              <p className="connect-hint">{t("connect.level.skip.hint", { count: missing })}</p>
              <Button variant="quiet" size="sm" data-ui="connect.level.skip" disabled={busy} onClick={actions.skipLevels}>
                {t("connect.level.skip")}
              </Button>
            </Section>
          )}
          <Advanced id="connect.level.advanced" count={1}>
            <div className="connect-sweep" data-ui="connect.level.sweep">
              <span className="ui-field__label">{t("connect.sweep.title")}</span>
              <span className="connect-readout tabular">
                {view.settings?.sweep_level_dbfs == null ? t("connect.sweep.current.default") : t("connect.sweep.current", { value: number(view.settings.sweep_level_dbfs, 1) })}
              </span>
              <p className="connect-hint">{t("connect.sweep.hint")}</p>
              <Button
                size="sm"
                data-ui="connect.level.sweep.check"
                busy={sweepCheck.running}
                disabled={!ready || firstMain === null || (busy && !sweepCheck.running)}
                onClick={() => firstMain && actions.checkSweepLevel(firstMain)}
              >
                {t("connect.sweep.check", { code: firstMain ? roleCode(firstMain) : "—" })}
              </Button>
              {sweepCheck.result && (
                <>
                  <p className="connect-readout tabular" data-ui="connect.level.sweep.result">
                    {t("connect.sweep.result", {
                      code: roleCode(sweepCheck.role ?? firstMain ?? ""),
                      level: number(sweepCheck.result.rms_dbfs, 1),
                      recommended: number(sweepCheck.result.recommended_sweep_dbfs, 1),
                    })}
                  </p>
                  <Button size="sm" data-ui="connect.level.sweep.apply" disabled={busy} onClick={() => sweepCheck.result && actions.applySweepLevel(sweepCheck.result)}>
                    {t("connect.sweep.apply", { value: number(sweepCheck.result.recommended_sweep_dbfs, 1) })}
                  </Button>
                </>
              )}
            </div>
          </Advanced>
        </div>
      </div>
    </Card>
  );
}
