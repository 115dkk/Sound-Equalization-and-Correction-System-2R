// @vitest-environment happy-dom
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { act, useState } from "react";
import { describe, expect, it, vi, type Mock } from "vitest";
import { JobTray } from "../../app/jobs/JobTray";
import { PrefsScope } from "../../app/prefs";
import { T0, replay, staticApi } from "../../gallery/frameSpecimens";
import { translate } from "../../i18n";
import { ipcError } from "../../ipc/mock";
import type { Translate } from "../../i18n/react";
import { renderWith } from "../../test/render";
import type { PlotScale } from "../../ui";
import { CalibrateScreen, calibrateHeader } from "./CalibrateScreen";
import { LOADING_DELAY_MS } from "./ResultGraph";
import * as fx from "./fixtures";
import { calibrateStepState, type CalibrateActions, type CalibrateView } from "./types";

// uPlot needs a canvas; a stub draws the figure, its toolbar and the overlay
// (target handles, XO line, band labels) on a fixed scale, and turns a
// double-click into the plot's double-click callback.
vi.mock("../../ui/plot/LogPlot", () => ({
  LogPlot: (props: {
    id: string;
    label: string;
    curves: readonly { id: string }[];
    overlay?: (scale: PlotScale) => ReactNode;
    toolbar?: ReactNode;
    stale?: boolean;
    onDoubleClick?: (point: { hz: number; value: number }) => void;
  }) => {
    const scale: PlotScale = {
      xToPx: (hz) => Math.log10(hz) * 100,
      pxToX: (px) => 10 ** (px / 100),
      yToPx: (value) => 100 - value,
      pxToY: (px) => 100 - px,
      width: 500,
      height: 300,
      x: [5, 20000],
      y: [-30, 15],
    };
    return (
      <figure data-ui={props.id} aria-label={props.label} className={props.stale ? "plot plot--stale" : "plot"} data-curves={props.curves.map((c) => c.id).join(",")}>
        {props.toolbar}
        <div className="u-over" data-testid="plot-area" onDoubleClick={() => props.onDoubleClick?.({ hz: 300, value: 2 })}>
          {props.overlay?.(scale)}
        </div>
      </figure>
    );
  },
}));

type MockActions = { [K in keyof CalibrateActions]: Mock<CalibrateActions[K]> };

function mockActions(): MockActions {
  return {
    goToMeasurements: vi.fn(),
    goToExport: vi.fn(),
    calibrate: vi.fn(),
    calibrateSubs: vi.fn(),
    recommendXo: vi.fn(),
    computeMimo: vi.fn(),
    changeCompute: vi.fn(),
    refreshCompute: vi.fn(),
    openGpuHelp: vi.fn(),
    selectTab: vi.fn(),
    selectChannel: vi.fn(),
    selectXoGroup: vi.fn(),
    previewXo: vi.fn(),
    changeSetting: vi.fn(async () => true),
    takeSubValues: vi.fn(),
    setTargetPoints: vi.fn(async () => true),
    applyTargetPreset: vi.fn(),
    saveTarget: vi.fn(),
    loadTarget: vi.fn(),
  };
}

const cleanupAll = cleanup;
const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
const must = (id: string) => {
  const element = ui(id);
  if (!element) throw new Error(`missing [data-ui="${id}"]`);
  return element;
};

function setup(view: CalibrateView, options: { locale?: "ko" | "en"; initial?: { targetMenu?: boolean; editSub?: string; latePositions?: boolean } } = {}) {
  const actions = mockActions();
  renderWith(<CalibrateScreen view={view} actions={actions} initial={options.initial} />, { locale: options.locale ?? "ko" });
  return actions;
}

function openAdvanced() {
  for (const toggle of document.querySelectorAll<HTMLButtonElement>(".ui-advanced__toggle[aria-expanded='false']")) fireEvent.click(toggle);
}

function commitNumber(id: string, value: string) {
  const input = must(id) as HTMLInputElement;
  fireEvent.change(input, { target: { value } });
  fireEvent.keyDown(input, { key: "Enter" });
}

const t: Translate = (key, params) => translate("ko", key, params);

const STATES: Record<string, CalibrateView> = {
  empty: fx.emptyView,
  computing: fx.computingView,
  spl: fx.splView,
  gd: fx.gdView,
  xo: fx.xoView,
  ba: fx.baView,
  stale: fx.staleView,
  failed: fx.failedView,
  subsOnly: fx.subsOnlyView,
  mimo: fx.mimoView,
  mimoStale: fx.mimoStaleView,
  onePosition: fx.onePositionView,
  noSubs: fx.noSubsView,
  ultra: fx.ultraView,
  runningMimo: fx.runningMimoView,
  loading: fx.loadingView,
  loadingEmpty: fx.loadingEmptyView,
  noPosition1: fx.noPosition1View,
  readyMimo: fx.readyMimoView,
  readyMimoTwo: fx.readyMimoTwoView,
  computingMimo: fx.computingMimoView,
  mimoFailed: fx.mimoFailedView,
  mimoBehindCorrection: fx.mimoBehindCorrectionView,
};

describe("보정 screen", () => {
  it("renders every fixture state in both languages", () => {
    for (const [name, view] of Object.entries(STATES)) {
      for (const locale of ["ko", "en"] as const) {
        const { unmount } = renderWith(<CalibrateScreen view={view} actions={mockActions()} />, { locale });
        expect(ui("calibrate.verdict"), `${name} ${locale}`).not.toBeNull();
        expect(ui("calibrate.graph"), `${name} ${locale}`).not.toBeNull();
        expect(ui("calibrate.settings"), `${name} ${locale}`).not.toBeNull();
        unmount();
      }
    }
    // Every fixture twice: about 3 s alone, past the 5 s default on a loaded runner.
  }, 30_000);

  it("puts results first: verdict, then the scorecard beside the graph, then the five settings sections in order", () => {
    setup(fx.splView);
    const order = [...document.querySelectorAll("[data-ui]")].map((n) => n.getAttribute("data-ui"));
    const at = (id: string) => order.indexOf(id);
    expect(at("calibrate.verdict")).toBeLessThan(at("calibrate.scorecard.card"));
    expect(at("calibrate.scorecard.card")).toBeLessThan(at("calibrate.graph"));
    const sections = ["mode", "range", "xo", "ba", "mimo"].map((s) => at(`calibrate.settings.${s}`));
    expect(sections.every((v, i) => v > at("calibrate.graph") && (i === 0 || v > sections[i - 1]!))).toBe(true);
  });

  it("shows the verdict numbers and the plan before the first run", () => {
    setup(fx.splView);
    expect(must("calibrate.verdict.within").textContent).toContain("69");
    expect(must("calibrate.verdict.gd").textContent).toContain("41.5");
    expect(must("calibrate.verdict.delay").textContent).toContain("9.8 ms");
    cleanupAll();
    setup(fx.emptyView);
    expect(must("calibrate.verdict.plan").textContent).toContain("3곳에서");
    expect(must("calibrate.plot.preview").textContent).toContain("위치 1");
    expect(ui("calibrate.scorecard.card")).toBeNull();
  });

  it("keeps every engine note available as a sentence in the collapsed notes", () => {
    setup(fx.splView);
    const notes = must("calibrate.verdict.notes") as HTMLDetailsElement;
    expect(notes.tagName).toBe("DETAILS");
    expect(notes.open).toBe(false);
    expect(must("calibrate.verdict.delay").compareDocumentPosition(notes) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // rc_limit is the RC card's line; the per-main phase records and the room modes read as one line each.
    const lines = [...notes.querySelectorAll("li")].map((li) => li.textContent ?? "");
    expect(notes.querySelector("summary")?.textContent).toBe("계산 메모 5개");
    expect(lines).toEqual([
      "FIR 2,048탭에 맞춰 위상 보정 여유를 8.0 ms에서 7.5 ms로 줄였습니다.",
      "위상 보정 세기: L 0.90, R 0.90, C 0.90, Ls 0.90, Rs 0.90, Lrs 0.90, Rrs 0.90, Ltf 0.50, Rtf 0.50, Ltr 0.50, Rtr 0.50",
      "울림이 오래 남는 룸 모드 2곳(34 Hz, 52 Hz)의 부스트를 4.0 dB로 제한했습니다.",
      "BA 평가값 0.84 (낮을수록 좋음)",
      "서브 버스 PEQ 반영 후 BA 평가값 0.84 → 0.80 (낮을수록 좋음)",
    ]);
    // No raw code or parameter list reaches the screen.
    for (const line of lines) expect(line).not.toMatch(/[a-z]+_[a-z]+|[{}[\]]/);
  });

  it("says position 1 is not measured where the channel would be chosen before a run, and that there is nothing to preview", () => {
    setup(fx.noPosition1View);
    expect(ui("calibrate.channel")).toBeNull();
    expect(must("calibrate.channel.none").textContent).toBe("채널위치 1 측정 없음");
    expect(must("calibrate.plot.preview").textContent).toBe("위치 1의 측정이 없어 미리 볼 곡선이 없습니다.");
    cleanupAll();
    // With position 1 measured the box is there and the preview hint says what is shown.
    setup(fx.emptyView);
    expect(ui("calibrate.channel.none")).toBeNull();
    expect((must("calibrate.channel") as HTMLSelectElement).value).toBe("L");
    expect(must("calibrate.plot.preview").textContent).toContain("위치 1");
    expect(must("calibrate.plot.preview").textContent).not.toBe("위치 1의 측정이 없어 미리 볼 곡선이 없습니다.");
  });

  it("keeps the last curves while new ones load: busy at once, dimmed with a status only after the delay", () => {
    vi.useFakeTimers();
    try {
      const actions = mockActions();
      const screenOf = (view: CalibrateView) => (
        <PrefsScope initial={{ locale: "ko", theme: "light", accent: "default" }}>
          <CalibrateScreen view={view} actions={actions} />
        </PrefsScope>
      );
      const { rerender } = render(screenOf(fx.loadingView));
      const box = must("calibrate.graph.plot");
      const status = must("calibrate.graph.loading");
      // At once: busy for readers, the previous channel's curves as they were (a quick reply never flashes).
      expect(box.getAttribute("aria-busy")).toBe("true");
      expect(must("calibrate.plot").className).not.toContain("plot--stale");
      expect(status.textContent).toBe("");
      act(() => vi.advanceTimersByTime(LOADING_DELAY_MS - 1));
      expect(must("calibrate.plot").className).not.toContain("plot--stale");
      act(() => vi.advanceTimersByTime(1));
      // Slow: dimmed like a stale result, and the status says so (outside the busy box, so it is heard).
      expect(must("calibrate.plot").className).toContain("plot--stale");
      expect(status.getAttribute("role")).toBe("status");
      expect(status.textContent).toBe("곡선 불러오는 중");
      expect(box.contains(status)).toBe(false);
      // The curves arrive.
      rerender(screenOf({ ...fx.loadingView, curvesLoading: false }));
      expect(must("calibrate.graph.plot").hasAttribute("aria-busy")).toBe(false);
      expect(must("calibrate.plot").className).not.toContain("plot--stale");
      expect(must("calibrate.graph.loading").textContent).toBe("");
      // Nothing yet to keep: the empty slot, with the status once slow.
      rerender(screenOf(fx.loadingEmptyView));
      expect(ui("calibrate.plot")).toBeNull();
      expect(document.querySelector(".calibrate-graph__loading")).not.toBeNull();
      act(() => vi.advanceTimersByTime(LOADING_DELAY_MS));
      expect(must("calibrate.graph.loading").textContent).toBe("곡선 불러오는 중");
    } finally {
      vi.useRealTimers();
    }
  });

  it("says why a result is stale and why a run failed", () => {
    setup(fx.staleView);
    expect(must("calibrate.verdict.stale").textContent).toContain("보정 설정이 바뀌었습니다");
    cleanupAll();
    const actions = setup(fx.failedView);
    const failed = must("calibrate.verdict.failed");
    expect(failed.getAttribute("role")).toBe("alert");
    expect(failed.querySelector(".ui-error__cause")?.textContent).toContain("SW2");
    expect(failed.querySelector(".ui-error__cause")?.textContent).not.toContain("empty or nonfinite IR");
    expect(failed.querySelector("details")?.textContent).toContain("invalid input: position 2 SW2: empty or nonfinite IR");
    fireEvent.click(must("calibrate.verdict.failed.measurements"));
    expect(actions.goToMeasurements).toHaveBeenCalledOnce();
  });

  it.each(["ko", "en"] as const)("shows generic failures once and offers measurement review only for structured errors (%s)", (locale) => {
    for (const job of ["calibrate", "recommend_xo"] as const) {
      const id = job === "calibrate" ? "calibrate.verdict.failed" : "calibrate.xo.failed";
      setup({ ...fx.emptyView, failure: { job, error: ipcError("engine_input", "unknown solver diagnostic") } }, { locale });
      expect(must(id).querySelector(".ui-error__cause")?.textContent).toBe(translate(locale, `calibrate.failed.${job}`));
      expect(ui(`${id}.measurements`)).toBeNull();
      expect(ui(`${id}.sample_rate`)).toBeNull();
      cleanupAll();
    }
    const actions = setup({ ...fx.emptyView, failure: { job: "recommend_xo", error: ipcError("engine_input", "invalid input: position 1 SW3: sample rate 44100 (expected 48000)", {
      reason: "measurement_sample_rate", position: "1", role: "SW3", actual_hz: "44100", expected_hz: "48000",
    }) } }, { locale });
    const cause = must("calibrate.xo.failed").querySelector(".ui-error__cause")?.textContent ?? "";
    expect(cause.split(translate(locale, "calibrate.failed.recommend_xo"))).toHaveLength(2);
    for (const value of ["SW3", "44,100", "48,000"]) expect(cause).toContain(value);
    expect(must("calibrate.input.details").getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(must("calibrate.xo.failed.sample_rate"));
    expect(must("calibrate.input.details").getAttribute("aria-expanded")).toBe("true");
    expect(document.activeElement).toBe(must("calibrate.sample_rate_policy.reject"));
    expect(actions.goToMeasurements).not.toHaveBeenCalled();
    expect(actions.changeSetting).not.toHaveBeenCalled();
    expect(actions.calibrate).not.toHaveBeenCalled();
    fireEvent.click(must("calibrate.input.details"));
    fireEvent.click(must("calibrate.xo.failed.sample_rate"));
    expect(document.activeElement).toBe(must("calibrate.sample_rate_policy.reject"));
  });

  it("restores the saved conversion policy and forwards an explicit choice without starting a calculation", () => {
    const actions = setup({ ...fx.emptyView, settings: { ...fx.emptyView.settings, sample_rate_policy: "resample" } });
    expect(must("calibrate.input.details").getAttribute("aria-expanded")).toBe("true");
    expect(must("calibrate.sample_rate_policy.resample").getAttribute("aria-checked")).toBe("true");
    fireEvent.click(must("calibrate.sample_rate_policy.reject"));
    expect(actions.changeSetting).toHaveBeenCalledWith({ kind: "sample_rate_policy", policy: "reject" });
    expect(actions.calibrate).not.toHaveBeenCalled();
  });

  it("keeps the experimental derivative choice folded until used and sets both parts in one edit", () => {
    let actions = setup(fx.emptyView);
    expect(must("calibrate.range.advanced").getAttribute("aria-expanded")).toBe("false");
    cleanup();
    actions = setup({ ...fx.emptyView, settings: { ...fx.emptyView.settings, peq_analytic_derivatives: true } });
    expect(must("calibrate.range.advanced").getAttribute("aria-expanded")).toBe("true");
    expect(must("calibrate.analytic.peq").getAttribute("aria-checked")).toBe("true");
    // MIMO is off in this view; only a choice that includes MIMO IIR says that part waits.
    expect(screen.queryByText(translate("ko", "calibrate.analytic.mimo_unused"))).toBeNull();
    fireEvent.click(must("calibrate.analytic.both"));
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "analytic_derivatives", peq: true, mimo_iir: true });
    fireEvent.click(must("calibrate.analytic.off"));
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "analytic_derivatives", peq: false, mimo_iir: false });
    expect(actions.calibrate).not.toHaveBeenCalled();
    cleanup();
    setup({ ...fx.emptyView, settings: { ...fx.emptyView.settings, mimo_iir_analytic_derivatives: true } });
    expect(must("calibrate.analytic.mimo").getAttribute("aria-checked")).toBe("true");
    expect(screen.getByText(translate("ko", "calibrate.analytic.mimo_unused"))).toBeTruthy();
  });

  it("sends unsupported resampling rates to measurements instead of reopening the conversion choice", () => {
    const actions = setup({ ...fx.emptyView, failure: { job: "recommend_xo", error: ipcError("engine_input", "unsupported rate", {
      reason: "unsupported_resampling_rate", position: "1", role: "SW3", actual_hz: "4000", min_hz: "8000", max_hz: "384000",
    }) } });
    expect(ui("calibrate.xo.failed.sample_rate")).toBeNull();
    fireEvent.click(must("calibrate.xo.failed.measurements"));
    expect(actions.goToMeasurements).toHaveBeenCalledOnce();
    expect(must("calibrate.input.details").getAttribute("aria-expanded")).toBe("false");
  });

  it("selects tabs and channels, and on XO the channel's group", () => {
    const actions = setup(fx.splView);
    fireEvent.click(must("calibrate.tab.gd"));
    expect(actions.selectTab).toHaveBeenCalledWith("gd");
    fireEvent.change(must("calibrate.channel"), { target: { value: "R" } });
    expect(actions.selectChannel).toHaveBeenLastCalledWith("R", null);
    const row = must("calibrate.scorecard").querySelector<HTMLElement>('[data-row="C"]')!;
    fireEvent.click(row);
    expect(actions.selectChannel).toHaveBeenLastCalledWith("C", null);
    cleanupAll();
    const xo = setup(fx.xoView);
    fireEvent.change(must("calibrate.channel"), { target: { value: "C" } });
    expect(xo.selectChannel).toHaveBeenLastCalledWith("C", "센터");
  });

  it("disables the channel on BA and offers BA only with two subs", () => {
    setup(fx.baView);
    expect((must("calibrate.channel") as HTMLSelectElement).disabled).toBe(true);
    expect(must("calibrate.ba.table").textContent).toContain("반전");
    expect(must("calibrate.ba.warnings").textContent).toContain("두 필터 사이가 좁아 소리가 거의 나지 않습니다");
    expect(must("calibrate.ba.warnings").textContent).toContain("+13.4 dB");
    cleanupAll();
    setup(fx.noSubsView);
    expect(ui("calibrate.tab.ba")).toBeNull();
    expect(must("calibrate.xo.none").textContent).toBe("서브가 없어 XO가 없습니다.");
    expect(must("calibrate.ba.none").textContent).toContain("0대");
  });

  it("edits the target on SPL: keys move a point, a double-click adds one, the menu applies presets, saves and loads", () => {
    const actions = setup(fx.splView);
    // Two nudges and the key's release store the target once.
    const point = must("calibrate.target.point.2");
    fireEvent.keyDown(point, { key: "ArrowUp" });
    fireEvent.keyDown(must("calibrate.target.point.2"), { key: "ArrowUp" });
    expect(actions.setTargetPoints).not.toHaveBeenCalled();
    fireEvent.keyUp(must("calibrate.target.point.2"), { key: "ArrowUp" });
    expect(actions.setTargetPoints).toHaveBeenCalledTimes(1);
    expect(actions.setTargetPoints).toHaveBeenLastCalledWith([
      [25, 6],
      [60, 6],
      [120, 1],
      [1000, 0],
      [16000, -3],
    ]);
    fireEvent.doubleClick(screen.getByTestId("plot-area"));
    expect(actions.setTargetPoints.mock.lastCall?.[0]).toContainEqual([300, 2]);
    fireEvent.click(must("calibrate.target.menu"));
    const menu = must("calibrate.target.menu.list");
    // The current preset is the checked choice; 사용자 지정 is a state, not a command.
    expect(within(menu).getByRole("menuitemradio", { name: "하만" }).getAttribute("aria-checked")).toBe("true");
    expect(within(menu).getByRole("menuitemradio", { name: "플랫" }).getAttribute("aria-checked")).toBe("false");
    expect(must("calibrate.target.custom").getAttribute("aria-checked")).toBe("false");
    expect((must("calibrate.target.custom") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(must("calibrate.target.preset.flat"));
    expect(actions.applyTargetPreset).toHaveBeenCalledWith("flat");
    fireEvent.click(must("calibrate.target.menu"));
    fireEvent.click(must("calibrate.target.save"));
    expect(actions.saveTarget).toHaveBeenCalled();
    fireEvent.click(must("calibrate.target.menu"));
    fireEvent.click(must("calibrate.target.load"));
    expect(actions.loadTarget).toHaveBeenCalled();
  });

  it("previews the XO while the line moves and stores it on release", () => {
    const actions = setup(fx.xoView);
    const handle = must("calibrate.xo.line.handle");
    fireEvent.keyDown(handle, { key: "ArrowRight" });
    expect(actions.previewXo).toHaveBeenLastCalledWith("프론트", 85);
    expect(actions.changeSetting).not.toHaveBeenCalled();
    fireEvent.keyUp(handle, { key: "ArrowRight" });
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "xo", group: "프론트", hz: 85 });
  });

  it("puts the target back when its save fails, says so next to the graph and saves the same draft again", async () => {
    vi.useFakeTimers();
    try {
      const actions = mockActions();
      const replies: ((saved: boolean) => void)[] = [];
      actions.setTargetPoints.mockImplementation(() => new Promise<boolean>((reply) => replies.push(reply)));
      const screenOf = (view: CalibrateView) => (
        <PrefsScope initial={{ locale: "ko", theme: "light", accent: "default" }}>
          <CalibrateScreen view={view} actions={actions} />
        </PrefsScope>
      );
      const { rerender } = render(screenOf(fx.splView));
      const point = () => must("calibrate.target.point.2").getAttribute("aria-label");
      const stored = point();
      fireEvent.keyDown(must("calibrate.target.point.2"), { key: "ArrowUp" });
      fireEvent.keyUp(must("calibrate.target.point.2"), { key: "ArrowUp" });
      const draft = actions.setTargetPoints.mock.lastCall?.[0];
      expect(draft).toContainEqual([60, 5.5]);
      expect(point()).not.toBe(stored);
      // Saving: the draft stays; the state shows once the save is slow.
      rerender(screenOf({ ...fx.splView, targetEdit: "saving" }));
      expect(ui("calibrate.graph.save")).toBeNull();
      act(() => vi.advanceTimersByTime(LOADING_DELAY_MS));
      expect(must("calibrate.graph.save").textContent).toBe("타겟 저장 중");
      expect(ui("calibrate.graph.save.retry")).toBeNull();
      // Refused: the graph shows the stored target again, the state and 다시 저장 beside it.
      await act(async () => replies[0]!(false));
      rerender(screenOf({ ...fx.splView, targetEdit: "failed" }));
      expect(point()).toBe(stored);
      expect(must("calibrate.graph.save").getAttribute("role")).toBe("status");
      expect(must("calibrate.graph.save").textContent).toContain("타겟 저장 실패");
      fireEvent.click(must("calibrate.graph.save.retry"));
      expect(actions.setTargetPoints).toHaveBeenCalledTimes(2);
      expect(actions.setTargetPoints.mock.lastCall?.[0]).toEqual(draft);
      expect(point()).not.toBe(stored);
      // Stored: the state goes away with the new target.
      await act(async () => replies[1]!(true));
      rerender(screenOf({ ...fx.splView, targetEdit: "idle", target: { ...fx.splView.target, points: draft! } }));
      expect(ui("calibrate.graph.save")).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("puts the XO line and curves back when its save fails and saves the same crossover again", async () => {
    const actions = mockActions();
    actions.changeSetting.mockResolvedValueOnce(false);
    const screenOf = (view: CalibrateView) => (
      <PrefsScope initial={{ locale: "ko", theme: "light", accent: "default" }}>
        <CalibrateScreen view={view} actions={actions} />
      </PrefsScope>
    );
    const { rerender } = render(screenOf(fx.xoView));
    const handle = () => must("calibrate.xo.line.handle");
    fireEvent.keyDown(handle(), { key: "ArrowRight" });
    fireEvent.keyUp(handle(), { key: "ArrowRight" });
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "xo", group: "프론트", hz: 85 });
    await act(async () => {});
    rerender(screenOf({ ...fx.xoView, xoEdit: "failed" }));
    expect(handle().getAttribute("aria-valuenow")).toBe("80");
    // The curves are asked for the stored crossover again.
    expect(actions.selectXoGroup).toHaveBeenLastCalledWith("프론트");
    expect(must("calibrate.graph.save").textContent).toContain("크로스오버 저장 실패");
    fireEvent.click(must("calibrate.graph.save.retry"));
    expect(actions.previewXo).toHaveBeenLastCalledWith("프론트", 85);
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "xo", group: "프론트", hz: 85 });
    expect(handle().getAttribute("aria-valuenow")).toBe("85");
  });

  it("sends every setting through changeSetting", () => {
    const actions = setup(fx.splView);
    fireEvent.click(must("calibrate.mode.linear"));
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "mode", mode: "linear" });
    commitNumber("calibrate.range.lower", "30");
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "rc_lo", hz: 30 });
    fireEvent.click(must("calibrate.range.basis.manual"));
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "rc_hi", hz: 310 });
    commitNumber("calibrate.range.volume", "80");
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "room_volume", m3: 80 });
    commitNumber("calibrate.range.dims.h", "3");
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "room_dims", dims: [5.2, 4.3, 3] });
    commitNumber("calibrate.xo.hz", "90");
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "xo", group: "프론트", hz: 90 });
    fireEvent.change(must("calibrate.xo.group"), { target: { value: "센터" } });
    expect(actions.selectXoGroup).toHaveBeenCalledWith("센터");
    fireEvent.change(must("calibrate.ba.mode.SW1"), { target: { value: "bundle:A" } });
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "sub_mode", role: "SW1", mode: "bundle:A" });
    fireEvent.click(must("calibrate.mimo.on"));
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "mimo_on", on: true });
    openAdvanced();
    commitNumber("calibrate.range.boost", "6");
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "boost_max", db: 6 });
    fireEvent.click(must("calibrate.xo.peq"));
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "xo_peq", on: false });
    fireEvent.change(must("calibrate.ba.infra"), { target: { value: "SW1" } });
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "infra", role: "SW1", hz: 25, type: "LR24", subs: ["SW1", "SW2"] });
    fireEvent.change(must("calibrate.ba.hp_type"), { target: { value: "BW12" } });
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "sub_band_type", role: "SW1", band: "hp", type: "BW12" });
    commitNumber("calibrate.ba.peq.1.hz", "40");
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "sub", role: "SW1", patch: { peq: [[40, 1, 0]] } });
    fireEvent.click(must("calibrate.ba.mimo_support"));
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "sub", role: "SW1", patch: { mimo_support: false } });
  });

  it("edits a fixed sub's values and a group's layout", () => {
    const actions = setup(fx.baView, { initial: { editSub: "SW2" } });
    commitNumber("calibrate.ba.gain", "6");
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "sub", role: "SW2", patch: { gain_db: 6 } });
    fireEvent.click(must("calibrate.ba.polarity.normal"));
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "sub", role: "SW2", patch: { polarity: 1 } });
    commitNumber("calibrate.ba.allpass.1.hz", "60");
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "sub", role: "SW2", patch: { allpass: [[60, 0.71]] } });
    commitNumber("calibrate.ba.main_delay", "1.5");
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "main_delay", ms: 1.5 });
    expect(must("calibrate.ba.settings_warnings").textContent).toContain("SW2");
    cleanupAll();
    const group = setup(fx.subsOnlyView, { initial: { editSub: "SW2" } });
    fireEvent.change(must("calibrate.ba.layout"), { target: { value: "multi" } });
    expect(group.changeSetting).toHaveBeenLastCalledWith({ kind: "sub_layout", bundle: "A", layout: "multi" });
    commitNumber("calibrate.ba.rear_trim", "-1.25");
    expect(group.changeSetting).toHaveBeenLastCalledWith({ kind: "rear_trim", bundle: "A", ms: -1.25 });
    expect(must("calibrate.ba.subs_only").textContent).toBe("서브만 계산한 결과");
    expect(must("calibrate.ba.filters").textContent).toContain("묶음 A DBA");
  });

  it("starts the screen's jobs and disables the others while one runs", () => {
    const actions = setup(fx.mimoView);
    fireEvent.click(must("calibrate.xo.recommend"));
    expect(actions.recommendXo).toHaveBeenCalled();
    fireEvent.click(must("calibrate.ba.compute_subs"));
    expect(actions.calibrateSubs).toHaveBeenCalled();
    fireEvent.click(must("calibrate.ba.take_values"));
    expect(actions.takeSubValues).toHaveBeenCalled();
    fireEvent.click(must("calibrate.mimo.compute"));
    expect(actions.computeMimo).toHaveBeenCalled();
    expect(must("calibrate.mimo.state").textContent).toContain("IIR 서포트 24개");
    cleanupAll();
    setup(fx.runningMimoView);
    expect(must("calibrate.mimo.compute").getAttribute("aria-busy")).toBe("true");
    for (const id of ["calibrate.xo.recommend", "calibrate.ba.compute_subs", "calibrate.ba.take_values"]) {
      expect((must(id) as HTMLButtonElement).disabled, id).toBe(true);
    }
  });

  it("counts the FIR taps at the DSP's output rate before comparing them, and names a rate that is not 48 kHz (ADR 0017)", () => {
    // miniDSP at 96 kHz: supports of 3,072 taps at 48 kHz become 6,144, past the 4,096 budget.
    setup(fx.minidspView);
    const budget = translate("ko", "calibrate.mimo.state.budget_over.rate", { taps: "6,144", budget: "4,096", rate: "96 kHz" });
    expect(must("calibrate.mimo.state").textContent).toContain(budget);
    // The mixed mode's 2,048 taps become 4,096, past 2,048 per channel: one warning line in the mode card.
    const over = must("calibrate.mode.fir_over");
    expect(over.closest("[data-ui='calibrate.settings.mode']")).not.toBeNull();
    expect(over.textContent).toBe(translate("ko", "calibrate.mode.fir_over.rate", { rate: "96 kHz", taps: "4,096", limit: "2,048", mode: "최소 지연" }));
    cleanupAll();
    // The same supports at 48 kHz fit, and the line names no rate.
    setup({ ...fx.minidspView, outputRate: 48000 });
    expect(must("calibrate.mimo.state").textContent).toContain(translate("ko", "calibrate.mimo.state.budget", { taps: "3,072", budget: "4,096" }));
    expect(ui("calibrate.mode.fir_over")).toBeNull();
    cleanupAll();
    // At 48 kHz a DSP with fewer taps than the mode still gets the warning, without a rate.
    setup({ ...fx.minidspView, outputRate: 48000, firTaps: 1024 });
    expect(must("calibrate.mode.fir_over").textContent).toBe(translate("ko", "calibrate.mode.fir_over", { taps: "2,048", limit: "1,024", mode: "최소 지연" }));
    cleanupAll();
    // A mode without FIR and an unknown DSP never warn.
    setup({ ...fx.minidspView, modeSpec: { ...fx.minidspView.modeSpec, fir_taps: 0 } });
    expect(ui("calibrate.mode.fir_over")).toBeNull();
    cleanupAll();
    setup({ ...fx.minidspView, firTaps: 0 });
    expect(ui("calibrate.mode.fir_over")).toBeNull();
  });

  it("requires recovery from an unavailable explicit GPU while Automatic remains calculable", () => {
    const actions = setup(fx.requestedGpuUnavailableView);
    expect((must("calibrate.mimo.compute") as HTMLButtonElement).disabled).toBe(true);
    expect((must("calibrate.compute.preference") as HTMLSelectElement).disabled).toBe(false);
    fireEvent.click(must("calibrate.mimo.compute"));
    expect(actions.computeMimo).not.toHaveBeenCalled();
    cleanupAll();
    setup(fx.runtimeMissingView);
    expect((must("calibrate.mimo.compute") as HTMLButtonElement).disabled).toBe(false);
  });

  it("asks for a result and three positions before MIMO", () => {
    setup({ ...fx.emptyView, settings: { ...fx.emptyView.settings, mimo_on: true } });
    expect(must("calibrate.mimo.reason").textContent).toBe("'보정 계산'을 누르면 MIMO도 함께 계산합니다.");
    cleanupAll();
    setup({ ...fx.splView, positions: [1, 2] });
    const toggle = must("calibrate.mimo.on") as HTMLButtonElement;
    expect(toggle.disabled).toBe(true);
    expect(document.getElementById("calibrate.mimo.on-hint")?.textContent).toBe("측정 위치가 2곳입니다. MIMO는 3곳 이상 필요합니다.");
  });

  it("offers all four modes and shows the saved ultra delay at its own control", () => {
    const actions = setup(fx.splView);
    for (const mode of ["zero", "mixed", "linear", "ultra"]) expect(ui(`calibrate.mode.${mode}`)).not.toBeNull();
    fireEvent.click(must("calibrate.mode.ultra"));
    expect(actions.changeSetting).toHaveBeenCalledWith({ kind: "mode", mode: "ultra" });
    expect(ui("calibrate.verdict.notes")).not.toBeNull();
    cleanupAll();
    const ultraActions = setup(fx.ultraView);
    expect(ui("calibrate.mode.ultra")?.getAttribute("aria-checked")).toBe("true");
    expect(ui("calibrate.mode.ultra_ms")).not.toBeNull();
    commitNumber("calibrate.mode.ultra_ms", "150");
    expect(ultraActions.changeSetting).toHaveBeenCalledWith({ kind: "ultra_ms", ms: 150 });
  });

  it("runs the correction from the result card, beside what it computes from", () => {
    // Before a result: the plan sentence, then 보정 계산 right after it.
    let actions = setup(fx.emptyView);
    expect(must("calibrate.verdict.plan").textContent).toContain("3곳에서 측정한 값으로 보정합니다.");
    const compute = must("calibrate.compute") as HTMLButtonElement;
    expect(compute.textContent).toBe("보정 계산");
    expect(compute.disabled).toBe(false);
    fireEvent.click(compute);
    expect(actions.calibrate).toHaveBeenCalledOnce();
    cleanupAll();
    // No measurement: one sentence and nothing to press.
    setup({ ...fx.emptyView, positions: [] });
    expect(must("calibrate.verdict.plan").textContent).toBe("측정값이 없어 보정할 수 없습니다.");
    expect(ui("calibrate.compute")).toBeNull();
    cleanupAll();
    // While it runs: a spinner and what it corrects from, in place of the button.
    setup(fx.computingView);
    expect(must("calibrate.verdict.computing").textContent).toBe("3점 측정값을 기반으로 보정 중…");
    expect(must("calibrate.verdict.computing").querySelector(".ui-spinner")).not.toBeNull();
    expect(ui("calibrate.compute")).toBeNull();
    cleanupAll();
    // With a result: the figures, then 다시 측정하기 and 다시 보정하기 after them.
    actions = setup(fx.splView);
    const again = must("calibrate.compute") as HTMLButtonElement;
    expect(again.textContent).toBe("다시 보정하기");
    expect(again.closest(".calibrate-run")?.querySelector(".calibrate-stats")).not.toBeNull();
    fireEvent.click(again);
    expect(actions.calibrate).toHaveBeenCalledOnce();
    // Measuring again can change the result; correcting again cannot, so the emphasis is on the first.
    expect(must("calibrate.remeasure").className).toContain("ui-button--primary");
    expect(again.className).not.toContain("ui-button--primary");
    fireEvent.click(must("calibrate.remeasure"));
    expect(actions.goToMeasurements).toHaveBeenCalledOnce();
    // On a current result the way on to 내보내기 comes last, in the go colour.
    const onward = must("calibrate.export");
    expect(onward.textContent).toBe("보정값 내보내기");
    expect(onward.className).toContain("ui-button--go");
    // After 다시 보정하기 and MIMO 함께 계산, which wrap together.
    expect(onward.previousElementSibling).toBe(again.closest(".calibrate-run__compute"));
    expect(again.nextElementSibling).toBeNull();
    fireEvent.click(onward);
    expect(actions.goToExport).toHaveBeenCalledOnce();
    cleanupAll();
    // A stale result is not exported from here: 다시 보정하기 comes first.
    setup(fx.staleView);
    expect(ui("calibrate.export")).toBeNull();
    cleanupAll();
    // Another job of this screen runs: the button stays, off, and the header pill says why.
    setup(fx.runningMimoView);
    expect((must("calibrate.compute") as HTMLButtonElement).disabled).toBe(true);
    expect(must("calibrate.compute").getAttribute("aria-describedby")).toBe("frame.header.calibrate.status");
    cleanupAll();
    // A blocked step (a measurement runs) turns it off and points at the header's reason.
    renderWith(<CalibrateScreen view={{ ...fx.emptyView, measuring: true }} actions={mockActions()} />, { locale: "ko" });
    expect((must("calibrate.compute") as HTMLButtonElement).disabled).toBe(true);
    expect(must("calibrate.compute").getAttribute("aria-describedby")).toBe("frame.header.calibrate.blocked");
  });

  it("has a header with the running job or the stale result and no primary", () => {
    const idle = calibrateHeader(fx.splView, t);
    expect(idle.primary).toBeUndefined();
    expect(idle.status).toBeUndefined();
    renderWith(<>{calibrateHeader(fx.runningMimoView, t).status}</>);
    expect(must("calibrate.running").textContent).toBe("MIMO 계산 중");
    cleanupAll();
    renderWith(<>{calibrateHeader(fx.staleView, t).status}</>);
    expect(must("calibrate.stale").textContent).toBe("다시 계산 필요");
  });

  it("computes the step state like upstream: blocked without measurements or while measuring, done with a current result", () => {
    expect(calibrateStepState({ ...fx.emptyView, positions: [] })).toEqual({ status: "blocked", reason: "no_measurements" });
    expect(calibrateStepState({ ...fx.splView, measuring: true })).toEqual({ status: "blocked", reason: "measuring" });
    expect(calibrateStepState(fx.emptyView)).toEqual({ status: "ready" });
    expect(calibrateStepState(fx.staleView)).toEqual({ status: "ready" });
    expect(calibrateStepState(fx.splView)).toEqual({ status: "done" });
  });
});

/* ── MIMO 함께 계산 (P142) ──────────────────────────────────────────────── */

/** The screen with its settings held in state: changeSetting applies mimo_on, as the service's reply would. */
function SettingsHarness({ initial, actions }: { initial: CalibrateView; actions: MockActions }) {
  const [view, setView] = useState(initial);
  const changeSetting: CalibrateActions["changeSetting"] = async (edit) => {
    await actions.changeSetting(edit);
    if (edit.kind === "mimo_on") setView((v) => ({ ...v, settings: { ...v.settings, mimo_on: edit.on } }));
    return true;
  };
  return <CalibrateScreen view={view} actions={{ ...actions, changeSetting }} />;
}

describe("MIMO 함께 계산", () => {
  it("sits right before 보정 계산, names MIMO in the plan and switches mimo_on", () => {
    const actions = setup(fx.readyMimoView);
    const toggle = must("calibrate.run.mimo") as HTMLButtonElement;
    expect(toggle.getAttribute("role")).toBe("switch");
    expect(toggle.getAttribute("aria-checked")).toBe("true");
    expect(toggle.disabled).toBe(false);
    expect(toggle.textContent).toBe("MIMO 함께 계산");
    expect(toggle.closest(".calibrate-run__mimo")?.nextElementSibling).toBe(must("calibrate.compute"));
    expect(toggle.closest("[data-ui='calibrate.verdict.plan']")).not.toBeNull();
    expect(ui("calibrate.run.mimo.help")).not.toBeNull();
    expect(must("calibrate.verdict.plan").querySelector("p")?.textContent).toBe("3곳에서 측정한 값으로 보정하고 MIMO를 함께 계산합니다.");
    fireEvent.click(toggle);
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "mimo_on", on: false });
    cleanupAll();
    // Off: the plan is the correction's alone.
    setup(fx.emptyView);
    expect(must("calibrate.run.mimo").getAttribute("aria-checked")).toBe("false");
    expect(must("calibrate.verdict.plan").querySelector("p")?.textContent).toBe("3곳에서 측정한 값으로 보정합니다.");
  });

  it("sits right before 다시 보정하기 after a result, after 다시 측정하기", () => {
    const actions = setup(fx.splView);
    const toggle = must("calibrate.run.mimo");
    const slot = toggle.closest(".calibrate-run__mimo");
    // One unit with the button it changes, after 다시 측정하기.
    expect(slot?.previousElementSibling).toBeNull();
    expect(slot?.nextElementSibling).toBe(must("calibrate.compute"));
    expect(slot?.parentElement?.previousElementSibling).toBe(must("calibrate.remeasure"));
    expect(must("calibrate.compute").textContent).toBe("다시 보정하기");
    fireEvent.click(toggle);
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "mimo_on", on: true });
  });

  it("is off below three positions and says why on its own line, and 보정 계산 stays on", () => {
    const actions = setup(fx.readyMimoTwoView);
    const toggle = must("calibrate.run.mimo") as HTMLButtonElement;
    expect(toggle.disabled).toBe(true);
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    const hint = document.getElementById("calibrate.run.mimo-hint");
    expect(hint?.textContent).toBe("측정 위치 3곳 이상 필요");
    expect(hint?.closest(".calibrate-run__mimo")).toBe(toggle.closest(".calibrate-run__mimo"));
    expect(toggle.getAttribute("aria-describedby")).toContain("calibrate.run.mimo-hint");
    expect(must("calibrate.verdict.plan").querySelector("p")?.textContent).toBe("2곳에서 측정한 값으로 보정합니다.");
    fireEvent.click(toggle);
    expect(actions.changeSetting).not.toHaveBeenCalled();
    fireEvent.click(must("calibrate.compute"));
    expect(actions.calibrate).toHaveBeenCalledOnce();
    cleanupAll();
    // In English too, and after a result with one position.
    setup(fx.onePositionView, { locale: "en" });
    expect((must("calibrate.run.mimo") as HTMLButtonElement).disabled).toBe(true);
    expect(document.getElementById("calibrate.run.mimo-hint")?.textContent).toBe("Needs 3 or more positions");
  });

  it("is the MIMO card's switch: either one moves both", async () => {
    const actions = mockActions();
    renderWith(<SettingsHarness initial={fx.splView} actions={actions} />, { locale: "ko" });
    const run = () => must("calibrate.run.mimo").getAttribute("aria-checked");
    const card = () => must("calibrate.mimo.on").getAttribute("aria-checked");
    expect([run(), card()]).toEqual(["false", "false"]);
    await act(async () => { fireEvent.click(must("calibrate.run.mimo")); });
    expect([run(), card()]).toEqual(["true", "true"]);
    await act(async () => { fireEvent.click(must("calibrate.mimo.on")); });
    expect([run(), card()]).toEqual(["false", "false"]);
    expect(actions.changeSetting.mock.calls).toEqual([[{ kind: "mimo_on", on: true }], [{ kind: "mimo_on", on: false }]]);
  });

  it("names MIMO in the busy line and the header pill while 보정 계산 computes it, in the controls' place", () => {
    setup(fx.computingMimoView);
    expect(must("calibrate.verdict.computing").textContent).toBe("3곳의 측정값으로 보정과 MIMO 계산 중…");
    expect(must("calibrate.verdict.computing").className).toBe("calibrate-run__busy");
    expect(ui("calibrate.run.mimo")).toBeNull();
    // The button (the MIMO card's compute device block shares its data-ui).
    expect(document.getElementById("calibrate.compute")).toBeNull();
    // The MIMO card's state line waits for the run.
    expect(ui("calibrate.mimo.state")).toBeNull();
    cleanupAll();
    renderWith(<>{calibrateHeader(fx.computingMimoView, t).status}</>);
    expect(must("calibrate.running").textContent).toBe("보정과 MIMO 계산 중");
    cleanupAll();
    renderWith(<>{calibrateHeader(fx.computingView, t).status}</>);
    expect(must("calibrate.running").textContent).toBe("보정 계산 중");
  });

  it("shows a MIMO failure after the correction in the MIMO card, with the result kept", () => {
    setup(fx.mimoFailedView);
    expect(ui("calibrate.verdict.failed")).toBeNull();
    expect(ui("calibrate.scorecard.card")).not.toBeNull();
    expect(must("calibrate.verdict.mimo").textContent).toContain("계산 실패");
    const failed = must("calibrate.mimo.failed");
    expect(failed.closest("[data-ui='calibrate.settings.mimo']")).not.toBeNull();
    expect(failed.querySelector(".ui-error__cause")?.textContent).toBe("MIMO 계산이 실패했습니다.");
    expect(failed.querySelector("details")?.textContent).toContain("MIMO support design produced nonfinite filters");
    expect(ui("calibrate.mimo.state")).toBeNull();
  });

  it("recomputes MIMO alone after a MIMO-only change, and asks for 다시 보정하기 when the correction changed", () => {
    const actions = setup(fx.mimoStaleView);
    const alone = must("calibrate.mimo.compute") as HTMLButtonElement;
    expect(alone.textContent).toBe("MIMO만 계산");
    expect(alone.disabled).toBe(false);
    expect(must("calibrate.mimo.state").textContent).toBe("MIMO를 계산한 뒤 MIMO 설정이나 보정 결과가 바뀌었습니다. 'MIMO만 계산'을 누르세요.");
    fireEvent.click(alone);
    expect(actions.computeMimo).toHaveBeenCalledOnce();
    cleanupAll();
    setup(fx.mimoBehindCorrectionView);
    expect((must("calibrate.mimo.compute") as HTMLButtonElement).disabled).toBe(true);
    expect(must("calibrate.mimo.state").textContent).toBe("보정 결과부터 다시 계산해야 합니다. '다시 보정하기'를 누르면 MIMO도 함께 계산합니다.");
  });
});

/* ── Scorecard: late bass and MIMO (P110) ───────────────────────────────── */

/** What the screen reader hears in each figure cell of a scorecard row (the channel cell left out). */
function heard(table: string, role: string): string[] {
  const row = must(table).querySelector<HTMLElement>(`[data-row="${role}"]`);
  if (!row) throw new Error(`no row ${role} in ${table}`);
  return [...row.querySelectorAll("td")].slice(1).map((cell) => cell.querySelector(".visually-hidden")?.textContent ?? "");
}

describe("scorecard after MIMO", () => {
  it("shows before → after → MIMO in all three figures with a current MIMO result, and a key that names the third", () => {
    setup(fx.mimoView);
    expect(heard("calibrate.scorecard", "L")).toEqual([
      "보정 전 38, 보정 후 64, MIMO 후 54",
      "보정 전 88.4, 보정 후 47.5, MIMO 후 41.1",
      "보정 전 −23.9, 보정 후 −16.3, MIMO 후 −23.1",
    ]);
    expect(heard("calibrate.scorecard", "R")[2]).toBe("보정 전 −24.0, 보정 후 −17.0, MIMO 후 −21.6");
    // The arrows and the MIMO mark are for the eye; the third value carries the MIMO curve's mark.
    const cell = must("calibrate.scorecard").querySelector('[data-row="C"]')!.querySelectorAll("td")[1]!;
    expect(cell.querySelector('[aria-hidden="true"]')?.textContent).toBe("36 → 62→ 55");
    expect(cell.querySelector(".calibrate-mimo")?.textContent).toBe("55");
    expect(must("calibrate.scorecard.key").textContent).toBe("보정 전 → 보정 후 → MIMO 후");
    expect(must("calibrate.scorecard.key").querySelector(".calibrate-mimo")?.textContent).toBe("MIMO 후");
    expect(must("calibrate.scorecard").querySelector("caption")?.textContent).toContain("MIMO 후");
    cleanupAll();
    setup(fx.mimoView, { locale: "en" });
    expect(heard("calibrate.scorecard", "L")[0]).toBe("Before 38, after 64, after MIMO 54");
    expect(must("calibrate.scorecard.key").textContent).toBe("Before → After → After MIMO");
  });

  it("keeps two values and does not mention MIMO when the MIMO result is stale or absent", () => {
    for (const view of [fx.mimoStaleView, fx.splView, { ...fx.mimoView, results: { ...fx.mimoView.results, mimo_export: "off" as const } }]) {
      setup(view);
      expect(heard("calibrate.scorecard", "L")).toEqual(["보정 전 38, 보정 후 64", "보정 전 88.4, 보정 후 47.5", "보정 전 −23.9, 보정 후 −16.3"]);
      const card = must("calibrate.scorecard.card");
      expect(card.textContent).not.toContain("MIMO");
      expect(card.querySelector(".calibrate-mimo")).toBeNull();
      expect(ui("calibrate.scorecard.key")).toBeNull();
      cleanupAll();
    }
  });

  it("keeps 낮은 순 on the share after correction, not after MIMO", () => {
    setup(fx.mimoView);
    fireEvent.click(must("calibrate.scorecard.sort"));
    const order = [...must("calibrate.scorecard").querySelectorAll<HTMLElement>("tbody tr")].map((row) => row.dataset.row);
    // After correction C 62 < L 64 < R 65; after MIMO L 54 would come first.
    expect(order.slice(0, 3)).toEqual(["C", "L", "R"]);
  });

  it("adds the 늦은 저음 column at the main position, explained once under the table", () => {
    setup(fx.splView);
    const heads = [...must("calibrate.scorecard").querySelectorAll("th[scope=col]")].map((th) => th.textContent);
    expect(heads[3]).toBe("늦은 저음 (dB)20–150 Hz");
    expect(heard("calibrate.scorecard", "C")[2]).toBe("보정 전 −17.0, 보정 후 −16.0");
    expect(must("calibrate.scorecard.late").textContent).toContain("낮을수록 울림이 짧습니다.");
    cleanupAll();
    // A channel without an entry has an empty cell; without any entry there is no column.
    const calibration = fx.splView.results.calibration!;
    setup({ ...fx.splView, results: { ...fx.splView.results, calibration: { ...calibration, late_energy: calibration.late_energy.filter((e) => e.role !== "C") } } });
    const empty = must("calibrate.scorecard").querySelector('[data-row="C"]')!.querySelectorAll("td")[3]!;
    expect(empty.textContent).toBe("");
    cleanupAll();
    setup({ ...fx.splView, results: { ...fx.splView.results, calibration: { ...calibration, late_energy: [] } } });
    expect(must("calibrate.scorecard").querySelectorAll("th[scope=col]")).toHaveLength(3);
    expect(ui("calibrate.scorecard.late")).toBeNull();
    expect(ui("calibrate.scorecard.positions")).toBeNull();
  });

  it("lists the late bass by position, collapsed, only with more than one position, the main position marked", () => {
    setup(fx.mimoView);
    const details = must("calibrate.scorecard.positions") as HTMLDetailsElement;
    expect(details.open).toBe(false);
    expect(details.querySelector("summary")?.textContent).toBe("위치별 늦은 저음");
    const heads = [...must("calibrate.scorecard.positions.table").querySelectorAll("th[scope=col]")].map((th) => th.textContent);
    expect(heads).toEqual(["채널", "위치 1기준", "위치 2", "위치 3"]);
    // Rows in main_roles order; each position before → after → MIMO.
    const rows = [...must("calibrate.scorecard.positions.table").querySelectorAll<HTMLElement>("tbody tr")].map((row) => row.dataset.row);
    expect(rows).toEqual(fx.mimoView.results.calibration!.main_roles);
    expect(heard("calibrate.scorecard.positions.table", "L")).toEqual([
      "보정 전 −23.9, 보정 후 −16.3, MIMO 후 −23.1",
      "보정 전 −22.2, 보정 후 −14.4, MIMO 후 −20.1",
      "보정 전 −24.5, 보정 후 −15.8, MIMO 후 −22.0",
    ]);
    cleanupAll();
    setup(fx.mimoStaleView);
    expect(heard("calibrate.scorecard.positions.table", "L")[1]).toBe("보정 전 −22.2, 보정 후 −14.4");
    cleanupAll();
    setup(fx.onePositionView);
    expect(ui("calibrate.scorecard.card")).not.toBeNull();
    expect(ui("calibrate.scorecard.positions")).toBeNull();
  });

  it("opens the positions from the start when asked (gallery), the way a click does", async () => {
    setup(fx.mimoView, { initial: { latePositions: true } });
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    expect((must("calibrate.scorecard.positions") as HTMLDetailsElement).open).toBe(true);
    // The channel table's height was kept before it opened.
    expect(document.querySelector<HTMLElement>(".calibrate-scorecard__main")!.style.getPropertyValue("--calibrate-score-main-h")).toMatch(/px$/);
  });
});

/* ── RC upper limit basis (ADR 0021, P140) ──────────────────────────────── */

/** Laid out but hidden: inside an inert panel of the upper limit's shared place. */
const inert = (id: string) => must(id).closest("[inert]") !== null;
const checked = (id: string) => must(id).getAttribute("aria-checked") === "true";
const basisHint = () => document.getElementById("calibrate.range.basis-hint")?.textContent ?? null;

describe("RC upper limit basis", () => {
  it("offers four bases and sends each one's edits", async () => {
    let actions = setup(fx.rangeViews.strict);
    expect(checked("calibrate.range.basis.strict")).toBe(true);
    fireEvent.click(must("calibrate.range.basis.lenient"));
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "rc_hi_basis", basis: "lenient" });
    fireEvent.click(must("calibrate.range.basis.agreement"));
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "rc_hi_basis", basis: "agreement" });
    // 직접 입력 starts from the limit in force, the RC line's 154 Hz.
    fireEvent.click(must("calibrate.range.basis.manual"));
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "rc_hi", hz: 154 });
    // The current basis sends nothing.
    actions.changeSetting.mockClear();
    fireEvent.click(must("calibrate.range.basis.strict"));
    expect(actions.changeSetting).not.toHaveBeenCalled();
    cleanupAll();
    // 직접 입력: the field sends the number; a basis sends the basis, then clears the limit.
    actions = setup(fx.rangeViews.manual);
    expect(checked("calibrate.range.basis.manual")).toBe(true);
    commitNumber("calibrate.range.upper", "400");
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "rc_hi", hz: 400 });
    actions.changeSetting.mockClear();
    await act(async () => {
      fireEvent.click(must("calibrate.range.basis.strict"));
    });
    expect(actions.changeSetting.mock.calls).toEqual([[{ kind: "rc_hi_basis", basis: "strict" }], [{ kind: "rc_hi", hz: null }]]);
    // A refused basis keeps the manual limit.
    actions.changeSetting.mockClear();
    actions.changeSetting.mockResolvedValueOnce(false);
    await act(async () => {
      fireEvent.click(must("calibrate.range.basis.lenient"));
    });
    expect(actions.changeSetting.mock.calls).toEqual([[{ kind: "rc_hi_basis", basis: "lenient" }]]);
  });

  it("shows the room volume and the transition only for the Schroeder bases, ×1 by default", () => {
    const actions = setup(fx.rangeViews.strict);
    for (const id of ["calibrate.range.transition", "calibrate.range.volume", "calibrate.range.dims.w"]) expect(inert(id), id).toBe(false);
    expect(inert("calibrate.range.upper")).toBe(true);
    expect(checked("calibrate.range.transition.1")).toBe(true);
    expect(must("calibrate.range.transition").textContent).toBe("×1×2×4");
    fireEvent.click(must("calibrate.range.transition.2"));
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "rc_hi_transition", factor: 2 });
    fireEvent.click(must("calibrate.range.transition.4"));
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "rc_hi_transition", factor: 4 });
    cleanupAll();
    setup(fx.rangeViews.lenientTimes2);
    expect(inert("calibrate.range.transition")).toBe(false);
    expect(checked("calibrate.range.transition.2")).toBe(true);
    cleanupAll();
    setup(fx.rangeViews.strictTimes4);
    expect(checked("calibrate.range.transition.4")).toBe(true);
    cleanupAll();
    for (const view of [fx.rangeViews.agreement, fx.rangeViews.manual]) {
      setup(view);
      expect(inert("calibrate.range.transition")).toBe(true);
      expect(inert("calibrate.range.volume")).toBe(true);
      cleanupAll();
    }
    setup(fx.rangeViews.manual);
    expect(inert("calibrate.range.upper")).toBe(false);
    cleanupAll();
    setup(fx.rangeViews.agreement);
    expect(inert("calibrate.range.agreement")).toBe(false);
    expect(must("calibrate.range.agreement").textContent).toBe(t("calibrate.range.agreement.note"));
  });

  it("keeps the card's height: every basis's controls share one place, the others hidden and inert", () => {
    const panels = () => [...document.querySelectorAll(".calibrate-upper > .calibrate-upper__panel")].map((panel) => [panel.getAttribute("data-ui"), panel.hasAttribute("inert")]);
    setup(fx.rangeViews.strict);
    expect(panels()).toEqual([["calibrate.range.schroeder", false], ["calibrate.range.agreement", true], ["calibrate.range.manual", true]]);
    cleanupAll();
    setup(fx.rangeViews.manual);
    expect(panels()).toEqual([["calibrate.range.schroeder", true], ["calibrate.range.agreement", true], ["calibrate.range.manual", false]]);
  });

  it("turns agreement off below three measured positions with the reason, and keeps a chosen agreement working", () => {
    setup(fx.rangeViews.agreementUnavailable);
    expect((must("calibrate.range.basis.agreement") as HTMLButtonElement).disabled).toBe(true);
    expect(basisHint()).toBe("측정 위치가 2곳입니다. 측정 위치 일치도는 3곳 이상 필요합니다.");
    expect(must("calibrate.range.basis").getAttribute("aria-describedby")).toContain("calibrate.range.basis-hint");
    cleanupAll();
    // Chosen before a position was cleared: still chosen and the group's tab stop, with what to do.
    const actions = setup(fx.rangeViews.agreementShort);
    const chosen = must("calibrate.range.basis.agreement") as HTMLButtonElement;
    expect(chosen.disabled).toBe(false);
    expect(chosen.getAttribute("aria-checked")).toBe("true");
    expect(chosen.tabIndex).toBe(0);
    expect(basisHint()).toBe("측정 위치가 2곳이라 측정 위치 일치도로는 보정을 계산할 수 없습니다. 위치를 3곳 이상 측정하거나 다른 기준을 고르세요.");
    fireEvent.click(must("calibrate.range.basis.strict"));
    expect(actions.changeSetting).toHaveBeenLastCalledWith({ kind: "rc_hi_basis", basis: "strict" });
    cleanupAll();
    // Three positions: on, and no reason.
    setup(fx.rangeViews.strict);
    expect((must("calibrate.range.basis.agreement") as HTMLButtonElement).disabled).toBe(false);
    expect(basisHint()).toBeNull();
    cleanupAll();
    setup(fx.rangeViews.agreementUnavailable, { locale: "en" });
    expect(basisHint()).toBe("Measured positions: 2. Position agreement needs 3 or more.");
  });

  it("states the evidence for each basis in the RC line, and keeps the lines of a manual limit, a missing volume and an older note", () => {
    const lines = (view: CalibrateView) => {
      setup(view);
      const text = [...must("calibrate.range.notes").querySelectorAll("li")].map((li) => li.textContent);
      cleanupAll();
      return text;
    };
    const volume = t("notice.rc_room_volume", { volume_m3: "63" });
    expect(lines(fx.rangeViews.strict)).toEqual(["RC 상한 154 Hz(엄격한 슈뢰더 154 Hz)", volume]);
    expect(lines(fx.rangeViews.strictTimes2)).toEqual(["RC 상한 308 Hz(엄격한 슈뢰더 154 Hz × 2)", volume]);
    expect(lines(fx.rangeViews.strictTimes4)[0]).toBe("RC 상한 616 Hz(엄격한 슈뢰더 154 Hz × 4)");
    expect(lines(fx.rangeViews.lenient)[0]).toBe("RC 상한 200 Hz(너그러운 슈뢰더, 200 Hz 대역 T60 0.50초)");
    expect(lines(fx.rangeViews.lenientTimes2)[0]).toBe("RC 상한 400 Hz(너그러운 슈뢰더 200 Hz × 2, 대역 T60 0.50초)");
    // Agreement needs no room volume: no volume line follows it.
    expect(lines(fx.rangeViews.agreement)).toEqual(["RC 상한 287 Hz(위치 9곳 일치도)"]);
    expect(lines(fx.rangeViews.manual)).toEqual(["RC 상한 300 Hz(직접 입력)"]);
    expect(lines(fx.rangeViews.noVolume)).toEqual(["RC 상한 500 Hz(자동)", t("notice.rc_no_room_volume")]);
    expect(lines(fx.splView)[0]).toBe("RC 상한 310 Hz(자동, 슈뢰더 310 Hz)");
  });
});

/* ── Reach table (docs/M5-reach.md §6–8, P31b rows) ─────────────────────── */

// Every id the reach rows cite, with <role>, <n> and <key> filled in.
const REACH_IDS = [
  "calibrate.compute",
  "calibrate.running",
  "calibrate.stale",
  "calibrate.verdict.plan",
  "calibrate.verdict.within",
  "calibrate.verdict.gd",
  "calibrate.verdict.spread",
  "calibrate.verdict.delay",
  "calibrate.verdict.mimo",
  "calibrate.verdict.notes",
  "calibrate.verdict.stale",
  "calibrate.verdict.failed",
  "calibrate.scorecard",
  "calibrate.scorecard.sort",
  "calibrate.channel",
  "calibrate.tab.spl",
  "calibrate.tab.gd",
  "calibrate.tab.xo",
  "calibrate.tab.ba",
  "calibrate.plot",
  "calibrate.plot.preview",
  "calibrate.plot.filters",
  "calibrate.plot.band.lower",
  "calibrate.plot.band.upper",
  ...["before", "after", "target", "mimo", "measured", "main", "sub", "sum", "align_before", "align_after", "sub_sum"].map((key) => `calibrate.plot.legend.${key}`),
  "calibrate.target.point.1",
  "calibrate.target.menu",
  "calibrate.target.preset.flat",
  "calibrate.target.preset.harman",
  "calibrate.target.preset.cinema",
  "calibrate.target.custom",
  "calibrate.target.save",
  "calibrate.target.load",
  "calibrate.xo.line.handle",
  "calibrate.ba.metrics",
  "calibrate.ba.table",
  "calibrate.ba.filters",
  "calibrate.ba.warnings",
  "calibrate.ba.subs_only",
  "calibrate.mode.zero",
  "calibrate.mode.mixed",
  "calibrate.mode.linear",
  "calibrate.mode.ultra",
  "calibrate.mode.ultra_ms",
  "calibrate.mode.spec",
  "calibrate.range.lower",
  ...["", ".strict", ".lenient", ".agreement", ".manual"].map((option) => `calibrate.range.basis${option}`),
  ...["", ".1", ".2", ".4"].map((option) => `calibrate.range.transition${option}`),
  "calibrate.range.upper",
  "calibrate.range.volume",
  "calibrate.range.dims.w",
  "calibrate.range.dims.d",
  "calibrate.range.dims.h",
  "calibrate.range.boost",
  "calibrate.range.notes",
  "calibrate.xo.group",
  "calibrate.xo.hz",
  "calibrate.xo.recommended",
  "calibrate.xo.recommend",
  "calibrate.xo.state",
  "calibrate.xo.none",
  "calibrate.xo.peq",
  "calibrate.xo.peq_lo",
  "calibrate.xo.peq_hi",
  "calibrate.ba.infra",
  "calibrate.ba.infra_hz",
  "calibrate.ba.infra_type",
  "calibrate.ba.infra_boost",
  "calibrate.ba.infra_note",
  "calibrate.ba.mode.SW1",
  "calibrate.ba.subs",
  "calibrate.ba.edit",
  "calibrate.ba.layout",
  "calibrate.ba.dba_row",
  "calibrate.ba.rear_trim",
  "calibrate.ba.gain",
  "calibrate.ba.delay",
  "calibrate.ba.polarity",
  "calibrate.ba.allpass.1.hz",
  "calibrate.ba.allpass.1.q",
  "calibrate.ba.hp_type",
  "calibrate.ba.hp",
  "calibrate.ba.lp_type",
  "calibrate.ba.lp",
  "calibrate.ba.peq.1.hz",
  "calibrate.ba.peq.1.q",
  "calibrate.ba.peq.1.gain",
  "calibrate.ba.main_delay",
  "calibrate.ba.mimo_support",
  "calibrate.ba.compute_subs",
  "calibrate.ba.take_values",
  "calibrate.ba.settings_warnings",
  "calibrate.ba.none",
  "calibrate.mimo.on",
  "calibrate.mimo.fsiso",
  "calibrate.mimo.low_auto",
  "calibrate.mimo.low",
  "calibrate.mimo.max",
  "calibrate.mimo.fir",
  "calibrate.mimo.compute",
  "calibrate.mimo.reason",
  "calibrate.mimo.state",
  "jobs.cancel.calibrate",
  "jobs.cancel.recommend_xo",
  "jobs.cancel.mimo",
  "jobs.history",
] as const;

/** The ids drawn by one render (every 고급 and the target menu open). */
function idsOf(node: ReactNode): Set<string> {
  const { unmount } = render(<PrefsScope initial={{ locale: "ko", theme: "light", accent: "default" }}>{node}</PrefsScope>);
  openAdvanced();
  const menu = ui("calibrate.target.menu");
  if (menu) fireEvent.click(menu);
  const ids = new Set([...document.querySelectorAll("[data-ui]")].map((n) => n.getAttribute("data-ui") ?? ""));
  unmount();
  return ids;
}

describe("reach table", () => {
  it("draws every id the P31b reach rows cite in at least one state", () => {
    const seen = new Set<string>();
    const add = (ids: Set<string>) => ids.forEach((id) => seen.add(id));
    const screenOf = (view: CalibrateView, editSub?: string) => <CalibrateScreen view={view} actions={mockActions()} initial={{ editSub }} />;
    for (const view of Object.values(STATES)) add(idsOf(screenOf(view)));
    // States that show the rest: a manual upper limit and lower MIMO limit, an infra sub, a group, a fixed sub.
    add(idsOf(screenOf({ ...fx.mimoView, settings: { ...fx.mimoView.settings, rc_hi: 400, mimo_low: 30 } })));
    add(idsOf(screenOf({ ...fx.splView, settings: { ...fx.splView.settings, subs_manual: { SW1: { ...fx.baView.settings.subs_manual.SW2!, mode: "auto", hp_hz: 0, lp_hz: 25 }, SW2: { ...fx.baView.settings.subs_manual.SW2!, mode: "auto", hp_hz: 25, lp_hz: 0 } } } })));
    add(idsOf(screenOf(fx.subsOnlyView, "SW2")));
    add(idsOf(screenOf(fx.baView, "SW2")));
    add(idsOf(screenOf({ ...fx.emptyView, settings: { ...fx.emptyView.settings, mimo_on: true } })));
    // The header's pills, and the job tray's cancels and history (U8).
    for (const view of [fx.splView, fx.staleView, fx.runningMimoView]) add(idsOf(<>{calibrateHeader(view, t).status}</>));
    const jobs = (["calibrate", "recommend_xo", "mimo"] as const).map((kind, i) => ({ type: "event" as const, at: T0 + i, event: { event: "started" as const, job: i + 1, kind } }));
    add(idsOf(<JobTray api={staticApi(replay(jobs), T0 + 5000)} defaultOpen />));
    const missing = REACH_IDS.filter((id) => !seen.has(id));
    expect(missing).toEqual([]);
    // Every fixture and the extra states: past the 5 s default on a loaded runner.
  }, 30_000);
});
