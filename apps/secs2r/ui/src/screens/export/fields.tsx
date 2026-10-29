// 내보내기의 입력 칸: the design name above the filter exports (DEVIATIONS
// F21) and the CamillaDSP devices inside its row (F20). The kit has no text
// field, so the box borrows the number field's box (`ui-number`): the same
// border, focus ring and skins.
import { useId, useRef, useState, type KeyboardEvent } from "react";
import type { MessageKey } from "../../i18n";
import { useT, type Translate } from "../../i18n/react";
import type { CamillaCaptureDevice } from "../../ipc/bindings/CamillaCaptureDevice";
import type { CamillaDevices } from "../../ipc/bindings/CamillaDevices";
import type { CamillaPlaybackDevice } from "../../ipc/bindings/CamillaPlaybackDevice";
import type { MeasureDevice } from "../../ipc/bindings/MeasureDevice";
import { Icon, Segmented, Select, Toggle, type Option } from "../../ui";
import { Field, describedBy } from "../../ui/Field";
import {
  WASAPI_FORMATS,
  designNameFor,
  designNameProblem,
  deviceProblem,
  devicesFor,
  type CamillaDevice,
  type DesignNameProblem,
  type DeviceSide,
  type ExportActions,
  type ExportView,
} from "./types";

/** The design name field's id; the filter buttons it blocks point at `${DESIGN_NAME_ID}-error`. */
export const DESIGN_NAME_ID = "export.design_name";

const NAME_PROBLEM_TEXT: Record<DesignNameProblem, MessageKey> = {
  length: "export.design_name.problem.length",
  control: "export.design_name.problem.control",
  character: "export.design_name.problem.character",
  trailing: "export.design_name.problem.trailing",
  reserved: "export.design_name.problem.reserved",
};

/* ── A one-line text box ────────────────────────────────────────────────── */

export function TextBox({
  id,
  value,
  onChange,
  onBlur,
  onKeyDown,
  invalid,
  describedBy: described,
  placeholder,
}: {
  id: string;
  value: string;
  onChange: (value: string) => void;
  /** Leaving the box (the CamillaDSP address saves then). */
  onBlur?: () => void;
  onKeyDown?: (event: KeyboardEvent<HTMLInputElement>) => void;
  invalid?: boolean;
  describedBy?: string | undefined;
  placeholder?: string;
}) {
  return (
    <div className={["ui-number", "export-text", invalid && "ui-number--invalid"].filter(Boolean).join(" ")}>
      <input
        id={id}
        data-ui={id}
        className="ui-number__input export-text__input"
        type="text"
        value={value}
        placeholder={placeholder}
        autoComplete="off"
        spellCheck={false}
        aria-invalid={invalid || undefined}
        aria-describedby={described}
        onChange={(event) => onChange(event.target.value)}
        onBlur={onBlur}
        onKeyDown={onKeyDown}
      />
    </div>
  );
}

function TextField({ id, label, value, onChange, hint, error, placeholder }: { id: string; label: string; value: string; onChange: (value: string) => void; hint?: string; error?: string | null; placeholder?: string }) {
  return (
    <Field id={id} label={label} hint={hint} error={error} className="export-text-field">
      <TextBox id={id} value={value} onChange={onChange} invalid={!!error} describedBy={describedBy(id, hint, error)} {...(placeholder ? { placeholder } : {})} />
    </Field>
  );
}

/* ── The design name ────────────────────────────────────────────────────── */

/**
 * One field above the filter exports. Empty, the exports go to timestamped
 * folders; named, to `SECS_<kind>_<name>`, which the next export with the
 * same name replaces whole. That consequence is said beside the field
 * before anything is exported; a name that cannot be a folder says why
 * instead, and the filter buttons stay off.
 */
export function DesignNameField({ view, actions }: { view: ExportView; actions: ExportActions }) {
  const t = useT();
  const problem = designNameProblem(view.designName);
  const name = designNameFor(view);
  const error = problem ? t(NAME_PROBLEM_TEXT[problem]) : null;
  const hint = problem ? null : name !== null ? t("export.design_name.replaces", { name }) : t("export.design_name.empty");
  return (
    <div className="export-name" data-ui="export.design_name.field">
      <label className="ui-field__label export-name__label" htmlFor={DESIGN_NAME_ID} id={`${DESIGN_NAME_ID}-label`}>
        {t("export.design_name")}
      </label>
      <div className="export-name__control">
        <TextBox id={DESIGN_NAME_ID} value={view.designName} onChange={actions.setDesignName} invalid={!!error} describedBy={describedBy(DESIGN_NAME_ID, hint, error)} />
      </div>
      {error && (
        <p className="ui-field__error export-name__message" id={`${DESIGN_NAME_ID}-error`} data-ui="export.design_name.problem">
          <Icon name="error" size={16} />
          <span>{error}</span>
        </p>
      )}
      {hint && (
        <p className="ui-field__hint export-name__message" id={`${DESIGN_NAME_ID}-hint`} data-ui="export.design_name.hint">
          {hint}
        </p>
      )}
    </div>
  );
}

/* ── The CamillaDSP devices ─────────────────────────────────────────────── */

const SIDE_TEXT: Record<DeviceSide, MessageKey> = { capture: "export.devices.capture", playback: "export.devices.playback" };

const BACKEND_TEXT: Record<CamillaDevice["type"], MessageKey> = {
  file: "export.devices.backend.file",
  wasapi: "export.devices.backend.wasapi",
  asio: "export.devices.backend.asio",
};

const BACKENDS = ["file", "wasapi", "asio"] as const satisfies readonly CamillaDevice["type"][];

/** One side as the summary names it: the backend and, for a device, its name. */
export function deviceText(t: Translate, device: CamillaDevice): string {
  switch (device.type) {
    case "file":
      return t("export.devices.backend.file");
    case "wasapi": {
      const name = device.device ?? t("connect.device.default");
      return t("loopback" in device && device.loopback ? "export.devices.summary.loopback" : "export.devices.summary.wasapi", { name });
    }
    case "asio":
      return device.device.trim() === "" ? t("export.devices.summary.asio.none") : t("export.devices.summary.asio", { name: device.device.trim() });
  }
}

/** The closed disclosure's summary: one phrase when both sides read the same (file placeholders), else each side. */
export function devicesSummary(t: Translate, devices: CamillaDevices): string {
  const capture = deviceText(t, devices.capture);
  const playback = deviceText(t, devices.playback);
  return capture === playback ? capture : t("export.devices.summary.pair", { capture, playback });
}

/** A side's backend as it starts: WASAPI on the system default in shared mode and F32 (CamillaDSP's shared-mode format), ASIO without a driver. */
function startDevice(side: "capture", type: CamillaDevice["type"]): CamillaCaptureDevice;
function startDevice(side: "playback", type: CamillaDevice["type"]): CamillaPlaybackDevice;
function startDevice(side: DeviceSide, type: CamillaDevice["type"]): CamillaDevice {
  if (type === "file") return { type: "file" };
  if (type === "asio") return { type: "asio", device: "" };
  return side === "capture"
    ? { type: "wasapi", device: null, format: "F32", exclusive: false, loopback: false }
    : { type: "wasapi", device: null, format: "F32", exclusive: false };
}

function withSide(devices: CamillaDevices, side: DeviceSide, device: CamillaDevice): CamillaDevices {
  // Each side's values are built for that side (startDevice, spreads of its own value).
  return side === "capture" ? { ...devices, capture: device as CamillaCaptureDevice } : { ...devices, playback: device as CamillaPlaybackDevice };
}

/** What each side last had per backend, so trying another backend and coming back keeps a typed name. */
type Remembered = Record<DeviceSide, Partial<Record<"wasapi" | "asio", CamillaDevice>>>;

/**
 * The capture and playback devices of the CamillaDSP configuration, folded
 * under the row. Closed, the toggle reads the current choice; open, each
 * side picks file placeholders, WASAPI or ASIO side by side.
 */
export function CamillaDevicesDisclosure({ view, actions, defaultOpen = false }: { view: ExportView; actions: ExportActions; defaultOpen?: boolean }) {
  const t = useT();
  const [open, setOpen] = useState(defaultOpen);
  const panelId = useId();
  const remembered = useRef<Remembered>({ capture: {}, playback: {} });
  const devices = view.camillaDevices;
  const asio = devices.capture.type === "asio" || devices.playback.type === "asio";
  const set = (side: DeviceSide, device: CamillaDevice) => actions.setCamillaDevices(withSide(devices, side, device));
  const choose = (side: DeviceSide, type: CamillaDevice["type"]) => {
    const current = devices[side];
    if (current.type === type) return;
    if (current.type !== "file") remembered.current[side][current.type] = current;
    const back = type === "file" ? undefined : remembered.current[side][type];
    set(side, back ?? (side === "capture" ? startDevice("capture", type) : startDevice("playback", type)));
  };
  return (
    <div className={["ui-advanced", "export-devices", open && "ui-advanced--open"].filter(Boolean).join(" ")}>
      <button
        type="button"
        id="export.devices"
        data-ui="export.devices"
        className="ui-advanced__toggle"
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        onClick={() => {
          if (!open) actions.refreshDevices();
          setOpen(!open);
        }}
      >
        <Icon name="chevronRight" size={16} className="ui-advanced__chevron" />
        <span>{t("export.devices")}</span>
        {!open && (
          <span className="export-devices__summary" data-ui="export.devices.summary">
            {devicesSummary(t, devices)}
          </span>
        )}
      </button>
      {open && (
        <div id={panelId} className="ui-advanced__panel export-devices__panel">
          <div className="export-devices__sides">
            {(["capture", "playback"] as const).map((side) => (
              <DeviceSideField key={side} side={side} device={devices[side]} nativeDevices={view.nativeDevices} onBackend={(type) => choose(side, type)} onChange={(device) => set(side, device)} />
            ))}
          </div>
          {asio && (
            // Said only when ASIO is chosen: only CamillaDSP's ASIO build can open it.
            <p className="export-devices__note" data-ui="export.devices.asio_note">
              <Icon name="info" size={18} />
              <span>{t("export.devices.asio.note")}</span>
            </p>
          )}
        </div>
      )}
    </div>
  );
}

function DeviceSideField({
  side,
  device,
  nativeDevices,
  onBackend,
  onChange,
}: {
  side: DeviceSide;
  device: CamillaDevice;
  nativeDevices: MeasureDevice[] | null;
  onBackend: (type: CamillaDevice["type"]) => void;
  onChange: (device: CamillaDevice) => void;
}) {
  const t = useT();
  const base = `export.devices.${side}`;
  return (
    <fieldset className="export-device" data-ui={base}>
      <legend className="export-device__legend">{t(SIDE_TEXT[side])}</legend>
      <Segmented
        id={`${base}.backend`}
        label={t("export.devices.backend")}
        hideLabel
        size="sm"
        value={device.type}
        options={BACKENDS.map((value) => ({ value, label: t(BACKEND_TEXT[value]) }))}
        onChange={onBackend}
      />
      {device.type === "wasapi" && <WasapiFields side={side} device={device} nativeDevices={nativeDevices} onChange={onChange} />}
      {device.type === "asio" && (
        <TextField
          id={`${base}.asio`}
          label={t("export.devices.asio")}
          value={device.device}
          error={deviceProblem(device) ? t("export.devices.asio.problem") : null}
          onChange={(name) => onChange({ type: "asio", device: name })}
        />
      )}
    </fieldset>
  );
}

type WasapiDevice = Extract<CamillaDevice, { type: "wasapi" }>;

function WasapiFields({ side, device, nativeDevices, onChange }: { side: DeviceSide; device: WasapiDevice; nativeDevices: MeasureDevice[] | null; onChange: (device: CamillaDevice) => void }) {
  const t = useT();
  const base = `export.devices.${side}`;
  const loopback = "loopback" in device && device.loopback;
  const name = (value: string): string | null => (value.trim() === "" ? null : value);
  return (
    <>
      {nativeDevices ? (
        <Select id={`${base}.device`} label={t("export.devices.device")} value={device.device ?? ""} options={deviceOptions(t, nativeDevices, side, loopback, device.device)} onChange={(value) => onChange({ ...device, device: name(value) })} />
      ) : (
        // No list (not Windows, or the endpoints could not be read): the name as Windows shows it.
        <TextField
          id={`${base}.device`}
          label={t("export.devices.device")}
          value={device.device ?? ""}
          placeholder={t("connect.device.default")}
          hint={t("export.devices.device.typed")}
          onChange={(value) => onChange({ ...device, device: name(value) })}
        />
      )}
      <Segmented
        id={`${base}.format`}
        label={t("export.devices.format")}
        size="sm"
        value={device.format}
        options={WASAPI_FORMATS.map((value) => ({ value, label: value }))}
        onChange={(format) => onChange({ ...device, format })}
      />
      <Toggle
        id={`${base}.exclusive`}
        label={t("export.devices.exclusive")}
        checked={device.exclusive && !loopback}
        disabled={loopback}
        {...(loopback ? { hint: t("export.devices.exclusive.loopback") } : {})}
        onChange={(exclusive) => onChange({ ...device, exclusive })}
      />
      {"loopback" in device && (
        <Toggle
          id={`${base}.loopback`}
          label={t("export.devices.loopback")}
          checked={device.loopback}
          onChange={(on) => {
            // Loopback records an output in shared mode: the list turns to outputs and exclusive goes off.
            const keep = device.device !== null && nativeDevices !== null && devicesFor(nativeDevices, "capture", on).some((d) => d.name === device.device);
            onChange({ ...device, loopback: on, exclusive: on ? false : device.exclusive, device: keep || nativeDevices === null ? device.device : null });
          }}
        />
      )}
    </>
  );
}

/** The system default, the side's devices with their channels, and a stored name the list no longer has. */
function deviceOptions(t: Translate, list: readonly MeasureDevice[], side: DeviceSide, loopback: boolean, current: string | null): Option<string>[] {
  const outputs = side === "playback" || loopback;
  const devices = devicesFor(list, side, loopback);
  const options: Option<string>[] = [
    { value: "", label: t("connect.device.default") },
    ...devices.map((device) => ({ value: device.name, label: t("connect.device.option", { name: device.name, count: outputs ? device.output_channels : device.input_channels }) })),
  ];
  if (current !== null && !devices.some((device) => device.name === current)) options.push({ value: current, label: t("export.devices.device.missing", { name: current }) });
  return options;
}
