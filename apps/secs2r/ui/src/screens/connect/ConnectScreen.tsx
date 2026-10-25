import { HelpTip } from "../../ui/HelpTip";
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
import type { MessageKey } from "../../i18n";
import type { Translate } from "../../i18n/react";
import { useNumber, useT } from "../../i18n/react";
import type { MeasureDevice } from "../../ipc/bindings/MeasureDevice";
import { Advanced, Button, Card, ErrorNotice, Icon, IconButton, LevelMeter, NumberField, Section, Segmented, Select, StatusPill, type Option, type PillTone } from "../../ui";
import { roleCode, roleKind } from "../../ui/maps/geometry";
import { roleName } from "../system/text";
import { sweepName } from "../measure/format";
import { MicMeterPanel, type MicMeterView } from "./MicMeterPanel";
import { levelRoles, levelsPassed, missingLevels, pathReady } from "./state";
import type { ConnectActions, ConnectPath, ConnectView, RewProblem } from "./types";
import "./connect.css";

const DEFAULT_SWEEP = 262144;

/** REW's audio drivers (`POST /audio/driver`); a name REW reports beyond these (Core Audio and the like) is shown as is. */
const REW_DRIVERS = ["ASIO", "Java"] as const;

/**
 * The REW device the screen shows for one direction: the stored name when REW
 * lists it (REW opens it before each sweep), else the device REW has open now (R7).
 */
function rewDevice(view: ConnectView, kind: "input" | "output"): string | null {
  const details = view.rew.details;
  const devices = (kind === "input" ? details.inputs : details.outputs) ?? [];
  const stored = kind === "input" ? view.settings?.input_device : view.settings?.output_device;
  if (stored && devices.some((d) => d.name === stored)) return stored;
  return (kind === "input" ? details.current?.inputDevice : details.current?.outputDevice) ?? null;
}

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
      if (rew.reachable && rew.pro !== false) return { tone: "success", text: t("connect.method.rew.ready") };
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
  // The sweep-level check holds the device (the service refuses a level play
  // meanwhile): the primary is off and the pill beside the title says why.
  const checking = view.sweepCheck.running;
  const status = checking ? (
    <StatusPill tone="busy" data-ui="connect.status">
      {t("connect.status.sweep_check")}
    </StatusPill>
  ) : roles.length === 0 ? undefined : missing === 0 ? (
      <StatusPill tone="success" data-ui="connect.status">
        {t("connect.status.levels_done")}
      </StatusPill>
    ) : (
      <StatusPill tone="info" data-ui="connect.status">
        {t("connect.status.levels", { done: roles.length - missing, total: roles.length })}
      </StatusPill>
    );
  // The check ended without a problem: the way on is measuring, here where
  // the check was (the rail is not where users look for it); checking all
  // again moves into the level card.
  if (!checking && !view.level.playingAll && levelsPassed(view)) {
    return { primary: { id: "connect.next", label: t("connect.primary.measure"), onClick: actions.next }, status };
  }
  return {
    primary: { id: "connect.level.play_all", label: t("frame.step.connect.primary"), onClick: actions.playAllLevels, busy: view.level.playingAll, disabled: checking },
    status,
  };
}

/* ── 측정 방식 ─────────────────────────────────────────────────────────── */

function MethodCard({ view, actions }: { view: ConnectView; actions: ConnectActions }) {
  const t = useT();
  return (
    <Card title={t("connect.method.title")} help={<>{t("connect.method.description")}<ul>{PATHS.map((path) => <li key={path}><strong>{t(PATH_TEXT[path].name)}</strong>: {t(PATH_TEXT[path].needs)}</li>)}</ul></>} data-ui="connect.method.card">
      <div className={view.path === null ? "connect-methods" : "connect-methods connect-methods--compact"} role="group" aria-label={t("connect.method.title")} data-ui="connect.method">
        {PATHS.map((path) => {
          const status = pathStatus(t, view, path);
          const current = view.path === path;
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
              <StatusPill tone={status.tone} className="connect-method__status">
                {status.text}
              </StatusPill>
            </button>
          );
          return button;
        })}
      </div>
    </Card>
  );
}

function MdatCard() {
  const t = useT();
  return (
    <Card title={t("connect.mdat.title")} help={t("connect.mdat.body")} data-ui="connect.mdat" />
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
  const status = view.methods?.rew ?? { running: false, reachable: false, pro: null };
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
  // REW has no license endpoint: unknown until a sweep succeeds or REW refuses one (R6).
  const pro = !status.reachable ? (
    <StatusPill>{t("connect.rew.pro.unknown")}</StatusPill>
  ) : status.pro === true ? (
    <StatusPill tone="success">{t("connect.rew.pro.ok")}</StatusPill>
  ) : status.pro === false ? (
    <StatusPill tone="warning">{t("connect.rew.pro.no")}</StatusPill>
  ) : (
    <StatusPill>{t("connect.rew.pro.untested")}</StatusPill>
  );
  const unknown = t("connect.rew.unknown");
  const hint: { cause: string; next?: string; detail?: string; tone: "danger" | "warning" } | null =
    launch === "launching"
      ? null
      : problem
        ? { cause: t(PROBLEM_TEXT[problem].cause), next: t(PROBLEM_TEXT[problem].next), tone: "danger", ...(view.rew.problemDetail ? { detail: view.rew.problemDetail } : {}) }
        : status.reachable && status.pro === false
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

/** A REW device by name; REW knows a Java device's channel count only once it has opened it (0 = name only). */
function rewOption(t: Translate, device: MeasureDevice, channels: number): Option<string> {
  return { value: device.name, label: channels > 0 ? t("connect.device.option", { name: device.name, count: channels }) : device.name };
}

function DevicesCard({ view, actions }: { view: ConnectView; actions: ConnectActions }) {
  const t = useT();
  const number = useNumber();
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
  const sweepOptions = (settings?.sweep_lengths ?? []).map((samples) => ({
    value: String(samples),
    label: t("connect.sweep.option", { name: sweepName(samples), seconds: number(samples / Math.max(1, view.sampleRate), 1) }),
  }));
  const cal = view.rew.details.micCalibration;
  // REW mode lists REW's own devices by name and opens the chosen one in REW; the input channel stays REW's (R7).
  const current = view.rew.details.current;
  const rewReachable = !!methods?.rew.reachable;
  const rewList = (devices: readonly MeasureDevice[], kind: "input" | "output", selected: string | null): Option<string>[] => {
    if (devices.length === 0) return [{ value: selected ?? "", label: selected ?? empty }];
    const options = devices.map((d) => rewOption(t, d, kind === "input" ? d.input_channels : d.output_channels));
    if (selected === null) return [{ value: "", label: t("connect.rew.unknown") }, ...options];
    return options.some((o) => o.value === selected) ? options : [...options, { value: selected, label: selected }];
  };
  const rewInput = rew ? rewDevice(view, "input") : null;
  const rewOutput = rew ? rewDevice(view, "output") : null;
  const driver = view.rew.details.driver ?? null;
  const driverOptions: Option<string>[] = driver === null
    ? [{ value: "", label: rewReachable ? t("connect.rew.unknown") : t("connect.device.after_rew") }]
    : [...REW_DRIVERS, ...((REW_DRIVERS as readonly string[]).includes(driver) ? [] : [driver])].map((name) => ({ value: name, label: name }));
  const rewReadout = (id: string, label: string, value: string | null | undefined) => (
    <div className="ui-field" data-ui={id}>
      <span className="ui-field__label" id={`${id}-label`}>{label}</span>
      <div className="ui-control-help-row">
      {value ? (
        <span className="connect-readout connect-readout--name" title={value}>
          {value}
        </span>
      ) : (
        <span className="connect-readout">
          <StatusPill>{methods?.rew.reachable ? t("connect.rew.unknown") : t("connect.device.after_rew")}</StatusPill>
        </span>
      )}
      </div>
    </div>
  );
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
      {rew && (
        <p className="connect-note" data-ui="connect.devices.rew_owned">
          <Icon name="info" size={18} className="connect-note__icon" />
          <span>{t("connect.device.rew_owned")}</span>
        </p>
      )}
      {rew && view.rew.switchingDriver && (
        <p className="connect-note" data-ui="connect.rew_driver.switching" role="status">
          <Icon name="info" size={18} className="connect-note__icon" />
          <span>{t("connect.rew_driver.switching")}</span>
        </p>
      )}
      <div className="connect-fields">
        {rew && (
          <Select
            id="connect.rew_driver"
            label={t("connect.rew_driver")}
            value={driver ?? ""}
            options={driverOptions}
            help={t("connect.rew_driver.help")}
            disabled={!rewReachable || driver === null || view.rew.switchingDriver}
            onChange={(name) => name !== driver && actions.setRewDriver(name)}
          />
        )}
        {rew ? (
          <Select
            id="connect.input_device"
            label={t("connect.input_device")}
            value={rewInput ?? ""}
            options={rewList(inputs, "input", rewInput)}
            disabled={inputs.length === 0 || view.rew.switchingDriver}
            onChange={actions.setInputDevice}
          />
        ) : (
          <Select
            id="connect.input_device"
            label={t("connect.input_device")}
            value={inputId}
            options={listOf(inputs, "input", inputId)}
            disabled={inputs.length === 0}
            onChange={actions.setInputDevice}
          />
        )}
        {rew ? (
          rewReadout("connect.input_channel", t("connect.input_channel"), current?.input)
        ) : (
          <Select
            id="connect.input_channel"
            label={t("connect.input_channel")}
            value={String(settings?.input_channel ?? 1)}
            options={channelOptions}
            disabled={inputs.length === 0}
            onChange={(v) => actions.setInputChannel(Number(v))}
          />
        )}
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
        {rew ? (
          <Select
            id="connect.output_device"
            label={t("connect.output_device")}
            value={rewOutput ?? ""}
            options={rewList(outputs, "output", rewOutput)}
            help={t("connect.output_device.hint")}
            disabled={outputs.length === 0 || view.rew.switchingDriver}
            onChange={actions.setOutputDevice}
          />
        ) : (
          <Select
            id="connect.output_device"
            label={t("connect.output_device")}
            value={outputId}
            options={listOf(outputs, "output", outputId)}
            help={t("connect.output_device.hint")}
            disabled={outputs.length === 0}
            onChange={actions.setOutputDevice}
          />
        )}
        <div className="ui-field" data-ui="connect.sample_rate">
          <span className="ui-field__label">{t("connect.sample_rate")}</span>
          <span className="connect-readout tabular">{t("connect.sample_rate.value", { rate: number(view.sampleRate) })}</span>
        </div>
        <Select
          id="connect.sweep_length"
          label={t("connect.sweep_length")}
          value={String(sweep)}
          options={sweepOptions}
          help={t("connect.sweep_length.hint")}
          onChange={(v) => actions.setSweepLength(Number(v))}
        />
      </div>
      {!rew && (
        <Advanced id="connect.devices.details" label={t("connect.share_mode")} defaultOpen={!!settings?.share_mode && settings.share_mode !== "auto"}>
          <Segmented
            id="connect.share_mode"
            label={t("connect.share_mode")}
            hideLabel
            help={t("connect.share_mode.hint")}
            value={settings?.share_mode === "exclusive" || settings?.share_mode === "shared" ? settings.share_mode : "auto"}
            options={[
              { value: "auto", label: t("connect.share_mode.auto") },
              { value: "exclusive", label: t("connect.share_mode.exclusive") },
              { value: "shared", label: t("connect.share_mode.shared") },
            ]}
            onChange={actions.setShareMode}
          />
        </Advanced>
      )}
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
  if (view.path === "rew") {
    const device = rewDevice(view, "input");
    const input = view.rew.details.current?.input;
    const names = view.rew.details.inputChannelNames ?? [];
    return t("connect.mic.input", { channel: input ?? inputLabel(t, settings.input_channel, names), device: device ?? t("connect.rew.unknown") });
  }
  const device = (view.methods?.native_devices ?? []).find((d) => d.id === settings.input_device);
  const deviceName = view.path === "mock" ? t("connect.mock.device") : device ? device.name : t("connect.device.default");
  return t("connect.mic.input", { channel: inputLabel(t, settings.input_channel, []), device: deviceName });
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
    <Card title={t("connect.level.title")} help={t("connect.level.description")} description={t("connect.level.target")} data-ui="connect.level">
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
              help={t("connect.level.master.hint")}
              onChange={actions.setMasterLevel}
            />
            <div className="connect-level__playing">
              {level.playing ? (
                <StatusPill tone="busy" data-ui="connect.level.playing">
                  {t("connect.level.playing", { code: roleCode(level.playing) })}
                </StatusPill>
              ) : sweepCheck.running ? (
                // Why the play buttons are off.
                <StatusPill tone="busy" data-ui="connect.level.playing">
                  {t("connect.status.sweep_check")}
                </StatusPill>
              ) : (
                <StatusPill data-ui="connect.level.playing">{t("connect.level.idle")}</StatusPill>
              )}
              {/* Once the check passed, the header goes on to measuring; checking all again is here. */}
              {!busy && levelsPassed(view) && (
                <Button variant="secondary" size="sm" data-ui="connect.level.play_all" disabled={!ready} onClick={actions.playAllLevels}>
                  {t("connect.level.again")}
                </Button>
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
              <div className="ui-heading-help"><span className="ui-field__label" id="connect.sweep.title">{t("connect.sweep.title")}</span><HelpTip id="connect.sweep.help" labelledBy="connect.sweep.title" content={t("connect.sweep.hint")} /></div>
              <span className="connect-readout tabular">
                {view.settings?.sweep_level_dbfs == null ? t("connect.sweep.current.default") : t("connect.sweep.current", { value: number(view.settings.sweep_level_dbfs, 1) })}
              </span>
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
