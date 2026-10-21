// @vitest-environment happy-dom
import { cleanup as cleanupScreen, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { StepState } from "../../app/model";
import { PrefsScope } from "../../app/prefs";
import { StepHeader } from "../../app/StepHeader";
import { translate } from "../../i18n";
import { renderWith } from "../../test/render";
import { configOf } from "../system/fixtures";
import { ConnectScreen, connectHeader } from "./ConnectScreen";
import {
  emptyView,
  levelDoneView,
  levelHalfView,
  mdatView,
  mockView,
  nativeErrorView,
  nativeView,
  rewJavaView,
  rewNoApiView,
  rewNotProView,
  rewOffView,
  rewSwitchingView,
  rewUntestedView,
} from "./fixtures";
import { MicMeterPanel } from "./MicMeterPanel";
import { connectStepState } from "./state";
import type { ConnectActions, ConnectView, MeterReading } from "./types";

/** Every id docs/M5-reach.md cites for 연결·점검 (sections 1 and 3), with a role filled in. */
export const REACH_IDS = [
  "connect.method",
  "connect.method.native",
  "connect.method.rew",
  "connect.method.mock",
  "connect.method.mdat",
  "connect.mdat",
  "connect.rew",
  "connect.rew.status",
  "connect.rew.executable",
  "connect.rew.version",
  "connect.rew.pro",
  "connect.rew.blocking",
  "connect.rew.launch",
  "connect.rew.read_settings",
  "connect.rew.settings",
  "connect.rew.hint",
  "connect.devices",
  "connect.devices.retry",
  "connect.rew_driver",
  "connect.input_device",
  "connect.input_channel",
  "connect.mic_cal",
  "connect.output_device",
  "connect.sample_rate",
  "connect.sweep_length",
  "connect.loopback_channel",
  "connect.share_mode",
  "connect.level",
  "connect.level.row.L",
  "connect.level.play.L",
  "connect.level.meter.L",
  "connect.level.master",
  "connect.level.playing",
  "connect.level.stop",
  "connect.level.play_all",
  "connect.level.skip",
  "connect.level.advanced",
  "connect.level.sweep.check",
  "connect.level.sweep.apply",
  "connect.mic",
  "connect.mic.input",
  "connect.mic.meter",
  "connect.mic.peak",
  "connect.mic.state",
  "connect.mic.clear_peak",
  "connect.status",
  "connect.next",
  "frame.mic",
] as const;

function actionsMock() {
  return {
    choosePath: vi.fn(),
    refreshMethods: vi.fn(),
    launchRew: vi.fn(),
    readRewSettings: vi.fn(),
    setRewDriver: vi.fn(),
    setInputDevice: vi.fn(),
    setInputChannel: vi.fn(),
    setOutputDevice: vi.fn(),
    setSweepLength: vi.fn(),
    setLoopbackChannel: vi.fn(),
    setShareMode: vi.fn(),
    playLevel: vi.fn(),
    stopLevel: vi.fn(),
    playAllLevels: vi.fn(),
    setMasterLevel: vi.fn(),
    skipLevels: vi.fn(),
    checkSweepLevel: vi.fn(),
    applySweepLevel: vi.fn(),
    next: vi.fn(),
  } satisfies ConnectActions;
}

const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
const must = (id: string) => {
  const node = ui(id);
  if (!node) throw new Error(`missing [data-ui="${id}"]`);
  return node;
};
const ko = (key: Parameters<typeof translate>[1], params?: Parameters<typeof translate>[2]) => translate("ko", key, params);

function setup(view: ConnectView, { locale = "ko" }: { locale?: "ko" | "en" } = {}) {
  const actions = actionsMock();
  renderWith(<ConnectScreen view={view} actions={actions} />, { locale });
  return actions;
}

const loopbackView: ConnectView = {
  ...nativeView,
  settings: { ...nativeView.settings!, timing: "loopback", loopback_input_channel: 3 },
};

const ALL: [string, ConnectView][] = [
  ["nothing chosen", emptyView],
  ["REW off", rewOffView],
  ["REW without API", rewNoApiView],
  ["REW not Pro", rewNotProView],
  ["native", nativeView],
  ["native error", nativeErrorView],
  ["level half done", levelHalfView],
  ["levels done", levelDoneView],
  ["mock", mockView],
  ["mdat only", mdatView],
];

describe("ConnectScreen", () => {
  it.each(ALL)("renders the %s state in both languages", (_name, view) => {
    for (const locale of ["ko", "en"] as const) {
      const { unmount } = renderWith(<ConnectScreen view={view} actions={actionsMock()} />, { locale });
      expect(ui("connect.method")).not.toBeNull();
      unmount();
    }
  });

  it("chooses a method; choosing the current one again does nothing", () => {
    const actions = setup(nativeView);
    fireEvent.click(must("connect.method.rew"));
    fireEvent.click(must("connect.method.mdat"));
    fireEvent.click(must("connect.method.native"));
    expect(actions.choosePath.mock.calls).toEqual([["rew"], ["mdat"]]);
    // Once chosen, the choices are compact: what each needs moves to a tooltip.
    expect(must("connect.method.rew").textContent).not.toContain(ko("connect.method.rew.needs"));
  });

  it("says what each method needs while nothing is chosen, and whether it is available", () => {
    setup(emptyView);
    fireEvent.click(must("connect.method.card.help"));
    expect(screen.getByRole("tooltip").textContent).toContain(ko("connect.method.native.needs"));
    expect(must("connect.method.native").textContent).toContain(ko("connect.method.native.ready", { count: 4 }));
    expect(must("connect.method.rew").textContent).toContain(ko("connect.method.rew.off"));
    expect(ui("connect.level")).toBeNull();
  });

  it("shows the REW states with a hint for each", () => {
    const off = setup(rewOffView);
    expect(must("connect.rew.status").textContent).toBe(ko("connect.rew.off"));
    expect(must("connect.rew.hint").textContent).toContain(ko("connect.rew.hint.off"));
    expect(must("connect.rew.launch").textContent).toBe(ko("connect.rew.launch"));
    expect((must("connect.rew.read_settings") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(must("connect.rew.launch"));
    expect(off.launchRew).toHaveBeenCalled();
    expect(ui("connect.level")).toBeNull();
  });

  it("tells REW without the API apart from REW without Pro", () => {
    const { unmount } = renderWith(<ConnectScreen view={rewNoApiView} actions={actionsMock()} />);
    expect(must("connect.rew.hint").textContent).toContain(ko("connect.rew.hint.no_api"));
    expect(must("connect.rew.launch").textContent).toBe(ko("connect.rew.recheck"));
    unmount();
    const actions = setup(rewNotProView);
    expect(must("connect.rew.status").textContent).toBe(ko("connect.rew.connected"));
    expect(must("connect.rew.pro").textContent).toBe(ko("connect.rew.pro.no"));
    expect(must("connect.rew.hint").textContent).toContain(ko("connect.rew.hint.not_pro"));
    expect(must("connect.rew.version").textContent).toBe("5.40 beta 92");
    expect(must("connect.mic_cal").textContent).toContain("7103946_90deg.txt");
    expect(must("connect.rew.settings").textContent).toContain("256k");
    fireEvent.click(must("connect.rew.read_settings"));
    expect(actions.readRewSettings).toHaveBeenCalled();
  });

  it("keeps the licence unknown until REW answers a sweep, and still offers REW (R6)", () => {
    setup(rewUntestedView);
    expect(must("connect.rew.pro").textContent).toBe(ko("connect.rew.pro.untested"));
    expect(ui("connect.rew.hint")).toBeNull();
    expect(must("connect.method.rew").textContent).toContain(ko("connect.method.rew.ready"));
  });

  it("lists REW's own devices and shows the one REW opens, never the native ids (R7)", () => {
    setup(rewUntestedView);
    expect(must("connect.devices.rew_owned").textContent).toBe(ko("connect.device.rew_owned"));
    // The project still holds native WASAPI ids; REW does not list them, so the screen shows what REW has open.
    expect((must("connect.input_device") as HTMLSelectElement).value).toBe("ASIO: MOTU Pro Audio");
    expect((must("connect.output_device") as HTMLSelectElement).value).toBe("ASIO: MOTU Pro Audio");
    expect(must("connect.devices").textContent).not.toContain("wasapi:");
    // The input channel stays REW's own choice.
    expect(must("connect.input_channel").querySelector("select")).toBeNull();
    expect(must("connect.input_channel").textContent).toContain("Mic/Line 1");
  });

  it("opens a chosen REW device by name and names Java devices REW has not opened without a channel count", () => {
    const actions = setup(rewJavaView);
    const output = must("connect.output_device") as HTMLSelectElement;
    expect(output.value).toBe("AVR (HDMI)");
    expect([...output.options].map((o) => o.textContent)).toEqual([
      translate("ko", "connect.device.option", { name: "AVR (HDMI)", count: 8 }),
      "Speakers (Realtek)",
    ]);
    fireEvent.change(output, { target: { value: "Speakers (Realtek)" } });
    expect(actions.setOutputDevice).toHaveBeenCalledWith("Speakers (Realtek)");
    fireEvent.change(must("connect.input_device"), { target: { value: "Line In (Realtek)" } });
    expect(actions.setInputDevice).toHaveBeenCalledWith("Line In (Realtek)");
  });

  it("switches REW's driver and keeps a driver name REW reports beyond ASIO and Java", () => {
    const actions = setup(rewJavaView);
    const driver = must("connect.rew_driver") as HTMLSelectElement;
    expect(driver.value).toBe("Java");
    expect([...driver.options].map((o) => o.value)).toEqual(["ASIO", "Java"]);
    expect(driver.disabled).toBe(false);
    fireEvent.change(driver, { target: { value: "ASIO" } });
    expect(actions.setRewDriver).toHaveBeenCalledWith("ASIO");
    cleanupScreen();
    setup({ ...rewJavaView, rew: { ...rewJavaView.rew, details: { ...rewJavaView.rew.details, driver: "CoreAudio" } } });
    expect([...(must("connect.rew_driver") as HTMLSelectElement).options].map((o) => o.value)).toEqual(["ASIO", "Java", "CoreAudio"]);
  });

  it("holds the driver and device fields while REW switches its driver, and before REW answers", () => {
    setup(rewSwitchingView);
    expect(must("connect.rew_driver.switching").textContent).toBe(ko("connect.rew_driver.switching"));
    for (const id of ["connect.rew_driver", "connect.input_device", "connect.output_device"]) {
      expect((must(id) as HTMLSelectElement).disabled).toBe(true);
    }
    cleanupScreen();
    setup(rewOffView);
    const driver = must("connect.rew_driver") as HTMLSelectElement;
    expect(driver.disabled).toBe(true);
    expect(driver.textContent).toBe(ko("connect.device.after_rew"));
    expect(ui("connect.rew_driver.switching")).toBeNull();
  });

  it("says when REW's own devices could not be read", () => {
    const unread = { ...rewUntestedView, rew: { ...rewUntestedView.rew, details: { ...rewUntestedView.rew.details, current: { inputDevice: null, outputDevice: null, input: null } } } };
    setup(unread);
    const input = must("connect.input_device") as HTMLSelectElement;
    expect(input.value).toBe("");
    expect(input.selectedOptions[0]?.textContent).toBe(ko("connect.rew.unknown"));
    expect(must("connect.input_channel").textContent).toBe(ko("connect.input_channel") + ko("connect.rew.unknown"));
  });

  it("explains a failed launch and while REW starts", () => {
    const { unmount } = renderWith(
      <ConnectScreen view={{ ...rewOffView, rew: { ...rewOffView.rew, problem: "exe_not_found", details: { executable: null } } }} actions={actionsMock()} />,
    );
    expect(must("connect.rew.hint").textContent).toContain(ko("connect.rew.problem.exe_not_found"));
    expect(must("connect.rew.hint").textContent).toContain(ko("connect.rew.problem.manual"));
    expect(must("connect.rew.executable").textContent).toBe(ko("connect.rew.not_found"));
    unmount();
    renderWith(<ConnectScreen view={{ ...rewOffView, rew: { ...rewOffView.rew, launch: "launching" } }} actions={actionsMock()} />);
    expect(must("connect.rew.hint").textContent).toBe(ko("connect.rew.hint.launching"));
    expect(must("connect.rew.launch").getAttribute("aria-busy")).toBe("true");
  });

  it("sets the devices, the channel and the sweep length", () => {
    const actions = setup(loopbackView);
    fireEvent.change(must("connect.input_device"), { target: { value: "wasapi:{0.0.1.00000000}.{51f2}" } });
    expect(actions.setInputDevice).toHaveBeenCalledWith("wasapi:{0.0.1.00000000}.{51f2}");
    fireEvent.change(must("connect.input_channel"), { target: { value: "2" } });
    expect(actions.setInputChannel).toHaveBeenCalledWith(2);
    fireEvent.change(must("connect.output_device"), { target: { value: "" } });
    expect(actions.setOutputDevice).toHaveBeenCalledWith("");
    fireEvent.change(must("connect.sweep_length"), { target: { value: "524288" } });
    expect(actions.setSweepLength).toHaveBeenCalledWith(524288);
    fireEvent.change(must("connect.loopback_channel"), { target: { value: "2" } });
    expect(actions.setLoopbackChannel).toHaveBeenCalledWith(2);
    fireEvent.click(must("connect.devices.details"));
    fireEvent.click(must("connect.share_mode.exclusive"));
    expect(actions.setShareMode).toHaveBeenCalledWith("exclusive");
    expect(must("connect.sweep_length").textContent).toContain(translate("ko", "connect.sweep.option", { name: "256k", seconds: "5.5" }));
    expect(must("connect.sample_rate").textContent).toContain("48,000 Hz");
  });

  it("keeps automatic device sharing folded until requested and the loopback channel for a cable reference", () => {
    setup(nativeView);
    expect(ui("connect.share_mode")).toBeNull();
    fireEvent.click(must("connect.devices.details"));
    expect(ui("connect.share_mode")).not.toBeNull();
    expect(ui("connect.loopback_channel")).toBeNull();
  });

  it.each(["exclusive", "shared"] as const)("shows saved %s sharing without changing the setting", (share_mode) => {
    const actions = setup({ ...nativeView, settings: { ...nativeView.settings!, share_mode } });
    expect(must("connect.devices.details").getAttribute("aria-expanded")).toBe("true");
    expect(must(`connect.share_mode.${share_mode}`).getAttribute("aria-checked")).toBe("true");
    expect(actions.setShareMode).not.toHaveBeenCalled();
  });

  it("keeps native sharing controls out of REW device settings", () => {
    setup(rewUntestedView);
    expect(ui("connect.devices.details")).toBeNull();
    expect(ui("connect.share_mode")).toBeNull();
  });

  it("shows a device enumeration error with its cause and a way to read again", () => {
    const actions = setup(nativeErrorView);
    expect(document.body.textContent).toContain(ko("error.device_unavailable"));
    expect(document.body.textContent).toContain("E_ACCESSDENIED");
    fireEvent.click(must("connect.devices.retry"));
    expect(actions.refreshMethods).toHaveBeenCalled();
    expect(must("connect.method.native").textContent).toContain(ko("connect.method.native.error"));
    expect(ui("connect.level")).toBeNull();
  });

  it("plays one channel at a time and stops it", () => {
    const actions = setup(levelHalfView);
    const rows = [...document.querySelectorAll("[data-ui^='connect.level.row.']")].map((row) => row.getAttribute("data-ui"));
    expect(rows.slice(0, 6)).toEqual(["L", "R", "C", "SW1", "SW2", "Ls"].map((role) => `connect.level.row.${role}`));
    expect((must("connect.level.play.L") as HTMLButtonElement).disabled).toBe(true);
    expect(must("connect.level.play.Rs").getAttribute("aria-label")).toBe(ko("connect.level.stop.aria", { code: "Rs" }));
    fireEvent.click(must("connect.level.play.Rs"));
    fireEvent.click(must("connect.level.stop"));
    expect(actions.stopLevel).toHaveBeenCalledTimes(2);
    expect(must("connect.level.playing").textContent).toBe(ko("connect.level.playing", { code: "Rs" }));
    // Stored levels pre-fill the rows; the playing row follows the meter.
    expect(must("connect.level.meter.L").textContent).toContain("−22.4");
    expect(must("connect.level.meter.Rs").textContent).toContain("−24.6");
    expect(must("connect.level.meter.SW1").textContent).toContain(ko("connect.level.sub_tone"));
  });

  it("plays a channel, sets the master output and fills missing levels", () => {
    const actions = setup(nativeView);
    fireEvent.click(must("connect.level.play.C"));
    expect(actions.playLevel).toHaveBeenCalledWith("C");
    const master = must("connect.level.master") as HTMLInputElement;
    fireEvent.change(master, { target: { value: "-15.5" } });
    fireEvent.keyDown(master, { key: "Enter" });
    expect(actions.setMasterLevel).toHaveBeenCalledWith(-15.5);
    fireEvent.click(must("connect.level.skip"));
    expect(actions.skipLevels).toHaveBeenCalled();
  });

  it("finds and applies the recommended sweep level under 고급 (U1)", () => {
    const actions = setup(levelDoneView);
    expect(ui("connect.level.skip")).toBeNull();
    fireEvent.click(must("connect.level.advanced"));
    fireEvent.click(must("connect.level.sweep.check"));
    expect(actions.checkSweepLevel).toHaveBeenCalledWith("L");
    fireEvent.click(must("connect.level.sweep.apply"));
    expect(actions.applySweepLevel).toHaveBeenCalledWith(levelDoneView.sweepCheck.result);
  });

  it("holds the mic peak until it is cleared and names clipping", () => {
    const reading = (rms: number, peak: number): MeterReading => ({ rms_dbfs: rms, peak_dbfs: peak, spl_db: null, clipping: false });
    const view = (r: MeterReading) => ({ input: "입력 1 · UMIK-1", monitor: "receiving" as const, reading: r });
    const { rerender } = renderWith(<MicMeterPanel view={view(reading(-30, -12))} />);
    expect(must("connect.mic.peak").textContent).toBe(ko("connect.mic.peak", { value: "−12.0" }));
    rerender(
      <PrefsScope initial={{ locale: "ko", theme: "light", accent: "default" }}>
        <MicMeterPanel view={view(reading(-28, -0.5))} />
      </PrefsScope>,
    );
    expect(must("connect.mic.peak").textContent).toBe(ko("connect.mic.peak.clip", { value: "−0.5" }));
    fireEvent.click(must("connect.mic.clear_peak"));
    expect(must("connect.mic.peak").textContent).toBe(ko("connect.mic.peak.none"));
    expect(must("connect.mic.state").textContent).toBe(ko("connect.mic.receiving"));
  });

  it("decides the step state from the path and the levels", () => {
    // Choosing and readying a method is this step's own work: ready, not blocked.
    expect(connectStepState(emptyView)).toEqual({ status: "ready", reason: "no_path" });
    expect(connectStepState(rewOffView)).toEqual({ status: "ready", reason: "path_not_ready" });
    expect(connectStepState(rewNotProView)).toEqual({ status: "ready", reason: "path_not_ready" });
    expect(connectStepState(nativeErrorView)).toEqual({ status: "ready", reason: "path_not_ready" });
    expect(connectStepState(nativeView)).toEqual({ status: "ready", reason: "no_levels", count: 13 });
    expect(connectStepState(mockView)).toEqual({ status: "ready", reason: "no_levels", count: 10 });
    expect(connectStepState(levelDoneView)).toEqual({ status: "done" });
    expect(connectStepState(mdatView)).toEqual({ status: "skipped" });
    expect(connectStepState({ ...emptyView, config: configOf("custom", [], "") })).toEqual({ status: "blocked", reason: "no_speakers" });
  });

  it("plays every channel from the header, or goes on to 측정·가져오기 on the mdat path", () => {
    const t = (key: Parameters<typeof translate>[1], params?: Parameters<typeof translate>[2]) => translate("ko", key, params);
    const actions = actionsMock();
    const half = connectHeader(levelHalfView, actions, t);
    expect(half.primary).toMatchObject({ id: "connect.level.play_all", label: t("frame.step.connect.primary"), busy: true });
    half.primary?.onClick();
    expect(actions.playAllLevels).toHaveBeenCalled();
    const mdat = connectHeader(mdatView, actions, t);
    expect(mdat.primary?.id).toBe("connect.next");
    mdat.primary?.onClick();
    expect(actions.next).toHaveBeenCalled();
    expect(connectHeader(emptyView, actions, t).status).toBeUndefined();
  });

  it("turns every level play off while the sweep level is being found, and says why", () => {
    const t = (key: Parameters<typeof translate>[1], params?: Parameters<typeof translate>[2]) => translate("ko", key, params);
    const checking: ConnectView = { ...levelHalfView, level: { ...levelHalfView.level, playing: null, playingAll: false }, sweepCheck: { running: true, result: null, role: "L" } };
    const header = connectHeader(checking, actionsMock(), t);
    expect(header.primary).toMatchObject({ id: "connect.level.play_all", disabled: true, busy: false });
    render(
      <PrefsScope initial={{ locale: "ko", theme: "light", accent: "default" }}>
        <StepHeader step="connect" state={connectStepState(checking)} primary={header.primary} status={header.status} />
        <ConnectScreen view={checking} actions={actionsMock()} />
      </PrefsScope>,
    );
    // The header's play-all is off and described by the pill beside the title.
    const playAll = must("connect.level.play_all") as HTMLButtonElement;
    expect(playAll.disabled).toBe(true);
    expect(document.getElementById(playAll.getAttribute("aria-describedby")!)?.textContent).toBe(ko("connect.status.sweep_check"));
    // Every channel's play is off too, and the card's play state says why.
    for (const button of document.querySelectorAll<HTMLButtonElement>('[data-ui^="connect.level.play."]')) expect(button.disabled, button.dataset.ui).toBe(true);
    expect(must("connect.level.playing").textContent).toBe(ko("connect.status.sweep_check"));
    // Without the check the same view plays.
    expect(connectHeader({ ...checking, sweepCheck: { running: false, result: null, role: null } }, actionsMock(), t).primary?.disabled).toBe(false);
  });

  it("draws every reach id in at least one state", () => {
    const seen = new Set<string>();
    const collect = () => document.querySelectorAll("[data-ui]").forEach((node) => seen.add(node.getAttribute("data-ui") ?? ""));
    const t = (key: Parameters<typeof translate>[1], params?: Parameters<typeof translate>[2]) => translate("ko", key, params);
    const views: ConnectView[] = [rewNotProView, rewOffView, nativeErrorView, loopbackView, levelHalfView, levelDoneView, mdatView, emptyView];
    for (const view of views) {
      const header = connectHeader(view, actionsMock(), t);
      const state: StepState = connectStepState(view);
      const { unmount } = render(
        <PrefsScope initial={{ locale: "ko", theme: "light", accent: "default" }}>
          <StepHeader step="connect" state={state} primary={header.primary} status={header.status} />
          <ConnectScreen view={view} actions={actionsMock()} />
          <MicMeterPanel id="frame.mic" floating view={{ input: null, monitor: "off", reading: null }} onClose={() => {}} />
        </PrefsScope>,
      );
      collect();
      if (ui("connect.level.advanced")) fireEvent.click(must("connect.level.advanced"));
      if (ui("connect.devices.details")) fireEvent.click(must("connect.devices.details"));
      collect();
      unmount();
    }
    expect(REACH_IDS.filter((id) => !seen.has(id))).toEqual([]);
  });
});
