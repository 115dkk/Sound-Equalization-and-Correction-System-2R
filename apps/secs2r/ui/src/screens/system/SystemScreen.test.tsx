// @vitest-environment happy-dom
import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PrefsScope } from "../../app/prefs";
import { StepHeader } from "../../app/StepHeader";
import { translate } from "../../i18n";
import { renderWith } from "../../test/render";
import { configOf, invalidView, newProjectView, sampleView, speaker } from "./fixtures";
import { systemStepState } from "./state";
import { SystemScreen, systemHeader } from "./SystemScreen";
import type { SystemActions, SystemView } from "./types";

/** Every id docs/M5-reach.md cites for 시스템 (sections 2 and 4), with a role or number filled in. */
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
  "system.patch.L.band",
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
  "system.start_delay",
  "system.sweeps",
  "system.estimate",
  "system.timing",
  "system.timing.role",
  "system.timing.output",
  "system.next",
  "system.confirm",
  "frame.header.blocked",
] as const;

function actionsMock(token: string | null = "undo-7") {
  return {
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
    setStartDelay: vi.fn(),
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

function setup(view: SystemView, { locale = "ko", expert = false }: { locale?: "ko" | "en"; expert?: boolean } = {}) {
  const actions = actionsMock(view.positionsDone.length > 0 ? "undo-7" : null);
  if (expert) {
    render(
      <PrefsScope initial={{ locale, theme: "light", expert: true, accent: "default" }}>
        <SystemScreen view={view} actions={actions} />
      </PrefsScope>,
    );
  } else {
    renderWith(<SystemScreen view={view} actions={actions} />, { locale });
  }
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

  it("sets the start delay and the timing reference", () => {
    const actions = setup(sampleView);
    fireEvent.click(must("system.start_delay.10"));
    expect(actions.setStartDelay).toHaveBeenCalledWith("10 초");
    fireEvent.change(must("system.timing.role"), { target: { value: "C" } });
    expect(actions.setTiming).toHaveBeenCalledWith({ reference: "acoustic", role: "C", output: 1 });
    fireEvent.change(must("system.timing"), { target: { value: "loopback" } });
    expect(actions.setTiming).toHaveBeenLastCalledWith({ reference: "loopback", role: "R", output: 1 });
  });

  it("shows the estimate per position and in total", () => {
    setup(sampleView);
    expect(must("system.estimate").textContent).toContain(translate("ko", "system.duration.min_sec", { min: 2, sec: 31 }));
    expect(must("system.estimate").textContent).toContain(translate("ko", "system.estimate.total", { count: 9, time: "약 22분 40초" }));
  });

  it("shows the band column and the file's role names in the expert view", () => {
    setup(sampleView, { expert: true });
    expect(must("system.patch.SW1.band").textContent).toBe("SUB");
    expect(must("system.speaker").textContent).toContain(translate("ko", "system.speaker.internal", { role: "L" }));
  });

  it("decides the step state like upstream can_advance()", () => {
    expect(systemStepState(sampleView)).toEqual({ status: "done" });
    expect(systemStepState(invalidView)).toEqual({ status: "blocked", reason: "config_invalid", count: 5 });
    expect(systemStepState({ ...newProjectView, config: configOf("custom", [], "") })).toEqual({ status: "blocked", reason: "no_speakers" });
  });

  it("puts the next step in the header", () => {
    const actions = actionsMock();
    const header = systemHeader(sampleView, actions, (key, params) => translate("ko", key, params));
    expect(header.primary?.id).toBe("system.next");
    expect(header.primary?.label).toBe(translate("ko", "frame.step.system.primary"));
    header.primary?.onClick();
    expect(actions.next).toHaveBeenCalled();
  });

  it("draws every reach id in at least one state", () => {
    const seen = new Set<string>();
    const collect = () => document.querySelectorAll("[data-ui]").forEach((node) => seen.add(node.getAttribute("data-ui") ?? ""));
    const custom: SystemView = { ...newProjectView, config: configOf("custom", [speaker("L", 1), speaker("R", 2), speaker("Ext", 3)], "") };
    for (const [view, expert] of [
      [sampleView, true],
      [invalidView, false],
      [custom, false],
    ] as const) {
      const header = systemHeader(view, actionsMock(), (key, params) => translate("ko", key, params));
      const { unmount } = render(
        <PrefsScope initial={{ locale: "ko", theme: "light", expert, accent: "default" }}>
          <StepHeader step="system" state={systemStepState(view)} primary={header.primary} status={header.status} />
          <SystemScreen view={view} actions={actionsMock()} />
        </PrefsScope>,
      );
      collect();
      if (ui("system.dsp.advanced")) fireEvent.click(must("system.dsp.advanced"));
      if (view.positionsDone.length > 0) fireEvent.click(must("system.preset.5.1"));
      collect();
      unmount();
    }
    expect(REACH_IDS.filter((id) => !seen.has(id))).toEqual([]);
  });
});
