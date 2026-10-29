// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PrefsScope } from "../../app/prefs";
import { StepHeader } from "../../app/StepHeader";
import { translate } from "../../i18n";
import { renderWith } from "../../test/render";
import { autoRateView, configOf, invalidView, minidspView, newProjectView, sampleView, speaker } from "./fixtures";
import { systemStepState } from "./state";
import { OUTPUT_LIMIT, SystemScreen, systemHeader } from "./SystemScreen";
import type { SystemActions, SystemView } from "./types";

/**
 * Every id docs/M5-reach.md cites for 시스템 (sections 2 and 4), with a role or
 * number filled in. The start delay row there names `measure.options.delay`,
 * which 측정·가져오기 draws (MeasureScreen.test.tsx).
 */
export const REACH_IDS = [
  "system.config",
  "system.preset",
  "system.preset.2.0",
  "system.preset.7.1.4",
  "system.preset.9.1.6",
  "system.preset.custom",
  "system.preset.empty",
  "system.subs",
  "system.device",
  "system.patch",
  "system.patch.L.role",
  "system.patch.L.size",
  "system.patch.L.out",
  "system.patch.L.pol",
  "system.patch.L.remove",
  "system.summary",
  "system.patch.add",
  "system.patch.renumber",
  "system.patch.csv",
  "system.notices",
  "system.notices.more",
  "system.room",
  "system.room.speaker.L",
  "system.room.unassigned.2",
  "system.speaker",
  "system.dsp",
  "system.dsp.device",
  "system.dsp.advanced",
  "system.dsp.fir_taps",
  "system.dsp.fir_budget",
  "system.dsp.iir_bands",
  "system.dsp.allpass_bands",
  "system.dsp.bm_path_delays",
  "system.dsp.note",
  "system.layout",
  "system.arrangement",
  "system.arrangement.1",
  "system.arrangement.9",
  "system.arrangement.13",
  "system.arrangement.17",
  "system.arrangement.map",
  "system.arrangement.map.position.1",
  "system.sweeps",
  "system.estimate",
  "system.timing.advanced",
  "system.timing",
  "system.timing.role",
  "system.timing.output",
  "system.next",
  "system.confirm",
  "frame.header.blocked",
] as const;

function actionsMock(token: string | null = "undo-7") {
  return {
    configureMeasurement: vi.fn(),
    applyPreset: vi.fn(async () => token),
    setSubCount: vi.fn(async () => token),
    setRole: vi.fn(),
    setSize: vi.fn(),
    setOutput: vi.fn(),
    setPolarity: vi.fn(),
    addSpeaker: vi.fn(),
    removeSpeaker: vi.fn(),
    renumberOutputs: vi.fn(),
    savePatchSheet: vi.fn(),
    setDspPreset: vi.fn(),
    setDspFields: vi.fn(),
    setArrangement: vi.fn(async () => token),
    setTiming: vi.fn(),
    undo: vi.fn(async () => {}),
    next: vi.fn(),
  } satisfies SystemActions;
}

const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
const must = (id: string) => {
  const node = ui(id);
  if (!node) throw new Error(`missing [data-ui="${id}"]`);
  return node;
};

function setup(view: SystemView, { locale = "ko" }: { locale?: "ko" | "en" } = {}) {
  const actions = actionsMock(view.positionsDone.length > 0 ? "undo-7" : null);
  renderWith(<SystemScreen view={view} actions={actions} />, { locale });
  return actions;
}

const flush = () => act(async () => {});

describe("SystemScreen", () => {
  it.each([
    ["new project", newProjectView],
    ["sample 7.1.4", sampleView],
    ["problems", invalidView],
  ])("renders the %s state in both languages", (_name, view) => {
    for (const locale of ["ko", "en"] as const) {
      const { unmount } = renderWith(<SystemScreen view={view} actions={actionsMock()} />, { locale });
      expect(ui("system.config")).not.toBeNull();
      expect(ui("system.room")).not.toBeNull();
      expect(ui("system.layout")).not.toBeNull();
      expect(ui("system.dsp")).not.toBeNull();
      unmount();
    }
  });

  it("applies a preset at once when nothing is measured", async () => {
    const actions = setup(newProjectView);
    fireEvent.click(must("system.preset.5.1"));
    await flush();
    expect(actions.applyPreset).toHaveBeenCalledWith("5.1");
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("offers SECS 2R's 4.0, 4.1, 4.2 and 7.1 presets in channel order (U13)", async () => {
    const actions = setup(newProjectView);
    const names = [...document.querySelectorAll(".system-presets__list .system-preset__name")].map((node) => node.textContent);
    expect(names).toEqual(["2.0", "2.1", "2.2", "4.0", "4.1", "4.2", "5.1", "7.1", "7.1.4", "9.1.6"]);
    expect(must("system.preset.4.2").textContent).toContain(translate("ko", "system.preset.count", { count: 6 }));
    expect(must("system.preset.7.1").textContent).toContain(translate("ko", "system.preset.count", { count: 8 }));
    fireEvent.click(must("system.preset.4.1"));
    await flush();
    expect(actions.applyPreset).toHaveBeenCalledWith("4.1");
    fireEvent.click(must("system.preset.7.1"));
    await flush();
    expect(actions.applyPreset).toHaveBeenCalledWith("7.1");
  });

  it("asks before a preset change that clears measurements, then offers undo (U5)", async () => {
    const actions = setup(sampleView);
    fireEvent.click(must("system.preset.5.1"));
    const dialog = screen.getByRole("dialog");
    expect(dialog.textContent).toContain(translate("ko", "system.confirm.preset.title", { preset: "5.1" }));
    expect(dialog.textContent).toContain(translate("ko", "system.confirm.body", { count: 3 }));
    expect(actions.applyPreset).not.toHaveBeenCalled();
    fireEvent.click(must("system.confirm.confirm"));
    await flush();
    expect(actions.applyPreset).toHaveBeenCalledWith("5.1");
    fireEvent.click(must("system.confirm.undo"));
    await flush();
    expect(actions.undo).toHaveBeenCalledWith("undo-7");
  });

  it("cancelling the dialog changes nothing", () => {
    const actions = setup(sampleView);
    fireEvent.click(must("system.arrangement.17"));
    fireEvent.click(must("system.confirm.cancel"));
    expect(actions.setArrangement).not.toHaveBeenCalled();
  });

  it("does not re-apply the preset that is already chosen", () => {
    const actions = setup(sampleView);
    fireEvent.click(must("system.preset.7.1.4"));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(actions.applyPreset).not.toHaveBeenCalled();
  });

  it("changes the sub count and the arrangement through the dialog", async () => {
    const actions = setup(sampleView);
    fireEvent.keyDown(must("system.subs"), { key: "ArrowUp" });
    fireEvent.click(must("system.confirm.confirm"));
    await flush();
    expect(actions.setSubCount).toHaveBeenCalledWith(3);
    fireEvent.click(must("system.arrangement.13"));
    fireEvent.click(must("system.confirm.confirm"));
    await flush();
    expect(actions.setArrangement).toHaveBeenCalledWith(13);
  });

  it("starts from an empty configuration with the custom preset", async () => {
    const actions = setup(newProjectView);
    fireEvent.click(must("system.preset.empty"));
    await flush();
    expect(actions.applyPreset).toHaveBeenCalledWith("custom");
  });

  it("edits a speaker's role, size, output and polarity", () => {
    const actions = setup(sampleView);
    fireEvent.change(must("system.patch.Ls.role"), { target: { value: "Lw" } });
    expect(actions.setRole).toHaveBeenCalledWith(5, "Lw");
    fireEvent.change(must("system.patch.L.size"), { target: { value: "large" } });
    expect(actions.setSize).toHaveBeenCalledWith(0, "large");
    fireEvent.change(must("system.patch.SW2.out"), { target: { value: "16" } });
    expect(actions.setOutput).toHaveBeenCalledWith(4, 0, 16);
    fireEvent.click(must("system.patch.R.pol"));
    expect(actions.setPolarity).toHaveBeenCalledWith(1, 0, "-");
    fireEvent.click(must("system.patch.Tbr.remove"));
    expect(actions.removeSpeaker).toHaveBeenCalledWith(12);
  });

  it("shows subs as a size pill, duplicates and out-of-range outputs, and an inverted band", () => {
    setup(invalidView);
    expect(ui("system.patch.SW1.size")).toBeNull();
    expect(must("system.patch.Ls.out").getAttribute("aria-invalid")).toBe("true");
    expect(must("system.patch.Tbl.out").getAttribute("aria-invalid")).toBe("true");
    expect(must("system.patch.L.out").getAttribute("aria-invalid")).toBeNull();
    expect(must("system.patch.Rb.pol").getAttribute("aria-pressed")).toBe("true");
    expect(must("system.device").textContent).toContain(translate("ko", "system.device.over", { count: 12 }));
  });

  it("bounds the output choices by the device and the service's 256, listing a far stored output as one option (TD-31)", () => {
    const far = 1_000_000_000;
    const farView: SystemView = {
      ...sampleView,
      config: configOf("2.0", [speaker("L", 1), speaker("R", far)], "MOTU 16A"),
      layout: { ...sampleView.layout, timing: { reference: "loopback", role: "R", output: far } },
    };
    const values = (id: string) => [...must(id).querySelectorAll("option")].map((option) => option.value);
    const labels = (id: string) => [...must(id).querySelectorAll("option")].map((option) => option.textContent);
    const upTo = (n: number) => Array.from({ length: n }, (_, i) => String(i + 1));
    const farLabel = translate("ko", "system.output.n", { n: far });

    // The device is not known: the patch lists up to the bound, the reference output upstream's 8.
    setup({ ...farView, outputChannels: null });
    fireEvent.click(must("system.timing.advanced"));
    for (const id of ["system.patch.L.out", "system.patch.R.out", "system.timing.output"]) expect(values(id).length).toBeLessThanOrEqual(OUTPUT_LIMIT + 1);
    expect(OUTPUT_LIMIT).toBe(256);
    expect(values("system.patch.L.out")).toEqual(upTo(OUTPUT_LIMIT));
    expect(values("system.patch.R.out")).toEqual([...upTo(OUTPUT_LIMIT), String(far)]);
    expect(labels("system.patch.R.out").at(-1)).toBe(translate("ko", "system.output.out_of_range", { output: farLabel }));
    expect((must("system.patch.R.out") as HTMLSelectElement).value).toBe(String(far));
    expect(values("system.timing.output")).toEqual([...upTo(8), String(far)]);
    expect(labels("system.timing.output").at(-1)).toBe(farLabel);
    expect((must("system.timing.output") as HTMLSelectElement).value).toBe(String(far));
    cleanup();

    // A known device bounds them by its channels.
    setup({ ...farView, outputChannels: 16 });
    fireEvent.click(must("system.timing.advanced"));
    expect(values("system.patch.L.out")).toEqual(upTo(16));
    expect(values("system.patch.R.out")).toEqual([...upTo(16), String(far)]);
    expect(values("system.timing.output")).toEqual([...upTo(16), String(far)]);
  });

  it("lists the first four notices with cause and next step, the rest behind 외 N개", () => {
    setup(invalidView);
    const notices = must("system.notices");
    expect(notices.querySelectorAll("li")).toHaveLength(4);
    expect(notices.textContent).toContain(translate("ko", "notice.out_duplicate", { out: "6", first_role: "Ls", role: "Rs" }));
    expect(notices.textContent).toContain(translate("ko", "notice.out_duplicate.next"));
    // Role ids become the codes the screen shows (Hfl → Lfh).
    expect(notices.textContent).toContain(translate("ko", "notice.bands_empty", { role: "Lfh" }));
    fireEvent.click(must("system.notices.more"));
    expect(must("system.notices").querySelectorAll("li")).toHaveLength(5);
    expect(ui("system.notices.more")).toBeNull();
  });

  it("runs the sheet actions", () => {
    const actions = setup(sampleView);
    const save = must("system.patch.csv");
    const help = screen.getByRole("button", { name: `${translate("ko", "system.patch.csv")} ${translate("ko", "ui.help")}` });
    fireEvent.click(help);
    expect(screen.getByRole("tooltip").id).toBe(save.getAttribute("aria-describedby"));
    expect(actions.savePatchSheet).not.toHaveBeenCalled();
    fireEvent.click(must("system.patch.add"));
    fireEvent.click(must("system.patch.renumber"));
    fireEvent.click(must("system.patch.csv"));
    expect(actions.addSpeaker).toHaveBeenCalled();
    expect(actions.renumberOutputs).toHaveBeenCalled();
    expect(actions.savePatchSheet).toHaveBeenCalled();
  });

  it("syncs the room map and the sheet: choosing either shows that speaker's wiring", () => {
    setup(sampleView);
    expect(must("system.speaker").textContent).toContain("Left");
    fireEvent.click(must("system.room.speaker.Rs"));
    expect(must("system.speaker").textContent).toContain("Right Surround");
    expect(document.querySelector('[data-row="6"]')?.getAttribute("aria-current")).toBe("true");
    fireEvent.click(document.querySelector('[data-row="2"]')!);
    expect(must("system.speaker").textContent).toContain("Center");
    expect(must("system.speaker").textContent).toContain(translate("ko", "system.speaker.band", { band: "FULL", output: "출력 3", input: 3 }));
  });

  it("chooses the DSP device and edits its fields under 고급 (U2)", () => {
    const actions = setup(sampleView);
    fireEvent.change(must("system.dsp.device"), { target: { value: "tesira" } });
    expect(actions.setDspPreset).toHaveBeenCalledWith("tesira");
    expect(ui("system.dsp.fir_budget")).toBeNull();
    fireEvent.click(must("system.dsp.advanced"));
    fireEvent.change(must("system.dsp.fir_taps"), { target: { value: "4096" } });
    expect(actions.setDspFields).toHaveBeenLastCalledWith({ ...sampleView.dsp.fields, fir_taps: 4096 });
    const iir = must("system.dsp.iir_bands") as HTMLInputElement;
    fireEvent.change(iir, { target: { value: "24" } });
    fireEvent.keyDown(iir, { key: "Enter" });
    expect(actions.setDspFields).toHaveBeenLastCalledWith({ ...sampleView.dsp.fields, iir_bands: 24 });
    expect(must("system.dsp.note").textContent).toContain("1,365");
  });

  it("writes the output rate a PC profile chooses: a fixed rate, or 자동 with its line (ADR 0017)", () => {
    const actions = setup(sampleView);
    const select = must("system.dsp.output_rate") as HTMLSelectElement;
    expect(select.tagName).toBe("SELECT");
    expect(select.value).toBe("48000");
    expect([...select.options].map((option) => option.textContent)).toEqual(["자동", "44.1 kHz", "48 kHz", "88.2 kHz", "96 kHz", "176.4 kHz", "192 kHz"]);
    // The 자동 line is said only while 자동 is chosen.
    expect(document.getElementById("system.dsp.output_rate-hint")).toBeNull();
    fireEvent.change(select, { target: { value: "96000" } });
    expect(actions.setDspFields).toHaveBeenLastCalledWith({ ...sampleView.dsp.fields, output_rate: { fixed: 96000 } });
    fireEvent.change(select, { target: { value: "auto" } });
    expect(actions.setDspFields).toHaveBeenLastCalledWith({ ...sampleView.dsp.fields, output_rate: "auto" });
    cleanup();
    setup(autoRateView);
    expect((must("system.dsp.output_rate") as HTMLSelectElement).value).toBe("auto");
    expect(document.getElementById("system.dsp.output_rate-hint")?.textContent).toBe(translate("ko", "system.dsp.output_rate.auto.hint"));
  });

  it("shows a rate the device fixes as text with its reason, and counts the taps at it", () => {
    const actions = setup(minidspView);
    const rate = must("system.dsp.output_rate");
    expect(rate.tagName).not.toBe("SELECT");
    expect(rate.querySelector("select, input, button")).toBeNull();
    expect(rate.textContent).toBe(`96 kHz${translate("ko", "system.dsp.output_rate.fixed")}`);
    fireEvent.click(must("system.dsp.advanced"));
    // 2048 taps at 96 kHz: (2048 − 1) / 96000 / 2 = 10.7 ms of linear-phase delay, not the 21.3 ms they take at 48 kHz.
    const taps = must("system.dsp.fir_taps") as HTMLSelectElement;
    expect(taps.selectedOptions[0]?.textContent).toBe(translate("ko", "system.dsp.taps.option", { taps: "2,048", ms: "10.7" }));
    expect(actions.setDspFields).not.toHaveBeenCalled();
  });

  it("sets the timing reference folded under 고급 on the REW path, after the estimate", () => {
    const actions = setup(sampleView);
    // The start delay is set on 측정·가져오기 only (P58).
    expect(ui("system.start_delay")).toBeNull();
    const options = must("system.options");
    expect(options.firstElementChild).toBe(must("system.estimate"));
    expect(options.lastElementChild?.contains(must("system.timing.advanced"))).toBe(true);
    expect(ui("system.timing")).toBeNull();
    fireEvent.click(must("system.timing.advanced"));
    fireEvent.change(must("system.timing.role"), { target: { value: "C" } });
    expect(actions.setTiming).toHaveBeenCalledWith({ reference: "acoustic", role: "C", output: 1 });
    fireEvent.change(must("system.timing"), { target: { value: "loopback" } });
    expect(actions.setTiming).toHaveBeenLastCalledWith({ reference: "loopback", role: "R", output: 1 });
  });

  it("hides the timing reference unless REW measures: native keeps arrival times in one recording", () => {
    setup({ ...sampleView, rew: false });
    expect(ui("system.estimate")).not.toBeNull();
    expect(ui("system.timing.advanced")).toBeNull();
    expect(ui("system.timing")).toBeNull();
    expect(ui("system.timing.role")).toBeNull();
  });

  it("shows the estimate per position and in total", () => {
    setup(sampleView);
    expect(must("system.estimate").textContent).toContain(translate("ko", "system.duration.min_sec", { min: 2, sec: 31 }));
    expect(must("system.estimate").textContent).toContain(translate("ko", "system.estimate.total", { count: 9, time: "약 22분 40초" }));
  });

  it("keeps multiband details and file role names available without a global display mode", () => {
    const speaker = invalidView.config.speakers.find((item) => item.bands.length > 1)!;
    setup({ ...newProjectView, config: configOf("custom", [speaker], "") });
    expect(document.querySelectorAll(".system-speaker__band")).toHaveLength(speaker.bands.length);
    expect(document.querySelector(".system-speaker__bands")?.textContent).toContain("LF");
    expect(document.querySelector(".system-speaker__bands")?.textContent).toContain("MF");
    fireEvent.click(must("system.speaker.help"));
    expect(screen.getByRole("tooltip").textContent).toContain(translate("ko", "system.speaker.internal", { role: speaker.role }));
  });

  it("shows the resolved measurement output name instead of its ID", () => {
    setup({ ...sampleView, config: { ...sampleView.config, device: "wasapi:{0.0.0.00000000}.{hdmi}" }, outputDeviceName: "AVR (HDMI)" });
    expect(must("system.device").textContent).toContain("AVR (HDMI)");
    expect(must("system.device").textContent).not.toContain("wasapi:");
  });

  it.each(["unselected", "unavailable"] as const)("opens the supplied setup action for %s measurement output", (outputDeviceState) => {
    const actions = setup({ ...newProjectView, outputDeviceState });
    expect(must("system.device.status").classList.contains("system-device__name--warning")).toBe(true);
    fireEvent.click(must("system.device.configure"));
    expect(actions.configureMeasurement).toHaveBeenCalledOnce();
  });

  it.each(["default", "checking", "rew_unknown", "not_required"] as const)("keeps %s distinct from a missing device and ignores the stored hardware name", (outputDeviceState) => {
    const actions = { ...actionsMock(), configureMeasurement: outputDeviceState === "default" ? vi.fn() : null };
    renderWith(<SystemScreen view={{ ...sampleView, outputDeviceState, outputChannels: null }} actions={actions} />);
    if (outputDeviceState === "not_required") {
      expect(ui("system.device")).toBeNull();
      return;
    }
    expect(must("system.device.status").classList.contains("system-device__name--warning")).toBe(false);
    expect(must("system.device").textContent).not.toContain(sampleView.config.device);
    expect(ui("system.device.configure") !== null).toBe(outputDeviceState === "default");
  });

  it("keeps incomplete setup editable and marks valid setup done", () => {
    expect(systemStepState(sampleView)).toEqual({ status: "done" });
    expect(systemStepState(invalidView)).toEqual({ status: "ready" });
    expect(systemStepState({ ...newProjectView, config: configOf("custom", [], "") })).toEqual({ status: "ready" });
  });

  it("puts the next step in the header", () => {
    const actions = actionsMock();
    const header = systemHeader(sampleView, actions, (key, params) => translate("ko", key, params));
    expect(header.primary?.id).toBe("system.next");
    expect(header.primary?.label).toBe(translate("ko", "frame.step.system.primary"));
    header.primary?.onClick();
    expect(actions.next).toHaveBeenCalled();
    expect(header.primary?.disabled).toBe(false);
    expect(header.status).toBeUndefined();
  });

  it("keeps the way on closed while the setup needs fixing", () => {
    const t = (key: Parameters<typeof translate>[1], params?: Parameters<typeof translate>[2]) => translate("ko", key, params);
    const invalid = systemHeader(invalidView, actionsMock(), t);
    expect(invalid.primary?.disabled).toBe(true);
    render(<>{invalid.status}</>);
    expect(must("system.status").textContent).toContain(translate("ko", "frame.blocked.config_invalid.short"));
    const empty = systemHeader({ ...newProjectView, config: configOf("custom", [], "") }, actionsMock(), t);
    expect(empty.primary?.disabled).toBe(true);
  });

  it("draws every reach id in at least one state", () => {
    const seen = new Set<string>();
    const collect = () => document.querySelectorAll("[data-ui]").forEach((node) => seen.add(node.getAttribute("data-ui") ?? ""));
    const custom: SystemView = { ...newProjectView, config: configOf("custom", [speaker("L", 1), speaker("R", 2), speaker("Ext", 3)], "") };
    const loopback: SystemView = { ...sampleView, layout: { ...sampleView.layout, timing: { reference: "loopback", role: "R", output: 1 } } };
    for (const view of [sampleView, invalidView, custom, loopback]) {
      const header = systemHeader(view, actionsMock(), (key, params) => translate("ko", key, params));
      const { unmount } = render(
        <PrefsScope initial={{ locale: "ko", theme: "light", accent: "default" }}>
          <StepHeader step="system" state={systemStepState(view)} primary={header.primary} status={header.status} />
          <SystemScreen view={view} actions={actionsMock()} />
        </PrefsScope>,
      );
      collect();
      if (ui("system.dsp.advanced")) fireEvent.click(must("system.dsp.advanced"));
      if (ui("system.timing.advanced")) fireEvent.click(must("system.timing.advanced"));
      if (view.positionsDone.length > 0) fireEvent.click(must("system.preset.5.1"));
      collect();
      unmount();
    }
    // Only the missing project blocks 시스템 itself; invalid configuration stays editable.
    const empty = render(<PrefsScope initial={{ locale: "ko", theme: "light", accent: "default" }}><StepHeader step="system" state={{ status: "blocked", reason: "no_project" }} /></PrefsScope>);
    collect();
    empty.unmount();
    expect(REACH_IDS.filter((id) => !seen.has(id))).toEqual([]);
  });
});
