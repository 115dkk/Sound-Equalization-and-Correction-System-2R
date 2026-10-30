// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { PrefsScope } from "../../app/prefs";
import { StepHeader } from "../../app/StepHeader";
import { translate } from "../../i18n";
import type { Translate } from "../../i18n/react";
import { installFakeCanvas } from "../../test/canvas";
import { renderWith } from "../../test/render";
import {
  MEASURE_FIXTURES,
  countdownView,
  doneView,
  emptyView,
  filesView,
  importedView,
  lowSnrView,
  mdatAssignView,
  measuringView,
  noPathView,
  partialView,
  referenceMidView,
  referenceSubView,
  sampleLoadingView,
  tutorialView,
} from "./fixtures";
import { MdatFilesDialog, assignmentProblems } from "./MdatDialogs";
import { MeasureOptions, MeasureScreen, measureHeader, referenceText } from "./MeasureScreen";
import { currentPosition, measureStepState, type MeasureActions, type MeasureView } from "./types";

/**
 * Every id this packet wrote into docs/M5-reach.md (sections 5 and 8).
 * `<n>` and `<role>` stand for any position and role; the test checks one.
 */
export const REACH_IDS = [
  "measure.where",
  "measure.graph.toggle",
  "measure.map",
  "measure.map.position.<n>",
  "measure.plot",
  "measure.plot.axes",
  "measure.plot.legend.role.<role>",
  "measure.plot.legend.position.<n>",
  "measure.graph.empty",
  "measure.positions",
  "measure.positions.<n>",
  "measure.sources",
  "measure.run",
  "measure.run.progress",
  "measure.position.start",
  "measure.next",
  "measure.status",
  "measure.cancel",
  "measure.redo",
  "measure.clear",
  "measure.clear.confirm",
  "measure.clear.undo",
  "measure.options.delay",
  "measure.options.sweep",
  "measure.options.repeats",
  "measure.mdat.load",
  "measure.import.note",
  "measure.files.open",
  "measure.sample.load",
  "measure.warning",
  "measure.mdat.dialog",
  "measure.mdat.row.<n>.position",
  "measure.mdat.row.<n>.role",
  "measure.mdat.problem",
  "measure.mdat.import",
  "measure.files",
  "measure.files.<n>.load",
  "measure.files.<n>.clear",
  "frame.header.blocked",
] as const;

const t: Translate = (key, params) => translate("ko", key, params);

// The real graph and legend draw on a fake canvas (their ids are part of the reach table).
beforeAll(() => {
  installFakeCanvas();
});

function makeActions(): MeasureActions & { [K in keyof MeasureActions]: ReturnType<typeof vi.fn> } {
  return {
    measurePosition: vi.fn(),
    cancelJob: vi.fn(),
    clearPosition: vi.fn(() => Promise.resolve("token-7")),
    undo: vi.fn(() => Promise.resolve()),
    selectPosition: vi.fn(),
    setGraphHidden: vi.fn(),
    setPlotRange: vi.fn(),
    setStartDelay: vi.fn(),
    setSweepLength: vi.fn(),
    setRepetitions: vi.fn(),
    pickMdat: vi.fn(),
    importMdat: vi.fn(),
    closeMdat: vi.fn(),
    loadSample: vi.fn(),
    openCalibrate: vi.fn(),
  } as never;
}

/** The step header and the screen, as the frame draws them. */
function Step({ view, actions }: { view: MeasureView; actions: MeasureActions }) {
  const header = measureHeader(view, actions, t);
  return (
    <>
      <StepHeader step="measure" state={measureStepState(view)} primary={header.primary} status={header.status} />
      <MeasureScreen view={view} actions={actions} />
    </>
  );
}

function renderStep(view: MeasureView, actions = makeActions(), options: { tutorial?: boolean; locale?: "ko" | "en" } = {}) {
  const ui = <Step view={view} actions={actions} />;
  if (options.tutorial) {
    render(
      <PrefsScope initial={{ locale: options.locale ?? "ko", theme: "light", accent: "tutorial" }}>{ui}</PrefsScope>,
    );
  } else {
    renderWith(ui, { locale: options.locale ?? "ko" });
  }
  return actions;
}

const byUi = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${CSS.escape(id)}"]`);
const ui = (id: string) => {
  const node = byUi(id) ?? document.getElementById(id);
  if (!node) throw new Error(`no element ${id}`);
  return node;
};

describe("측정·가져오기 screen", () => {
  it.each(Object.entries(MEASURE_FIXTURES))("renders the %s state in Korean and English", (_name, view) => {
    renderStep(view);
    expect(ui("measure")).toBeTruthy();
    cleanup();
    renderStep(view, makeActions(), { locale: "en" });
    expect(ui("measure.map")).toBeTruthy();
  });

  it("measures the next position, or measures a picked one again", () => {
    let actions = renderStep(partialView);
    expect(ui("measure.position.start").textContent).toContain("이 위치 측정");
    expect(ui("measure.where").textContent).toContain("위치 4 / 9");
    fireEvent.click(ui("measure.position.start"));
    expect(actions.measurePosition).toHaveBeenCalledWith(4);

    cleanup();
    actions = renderStep(lowSnrView);
    expect(ui("measure.position.start").textContent).toContain("이 위치 다시 측정");
    fireEvent.click(ui("measure.position.start"));
    expect(actions.measurePosition).toHaveBeenCalledWith(4);
  });

  it("opens 보정 once every position is measured, and imports on the mdat path", () => {
    let actions = renderStep(doneView);
    expect(currentPosition(doneView)).toBeNull();
    expect(ui("measure.next").textContent).toBe("보정으로");
    fireEvent.click(ui("measure.next"));
    expect(actions.openCalibrate).toHaveBeenCalledOnce();

    cleanup();
    actions = renderStep(importedView);
    const primaries = document.querySelectorAll('[data-ui="measure.mdat.load"]');
    expect(primaries).toHaveLength(1);
    fireEvent.click(ui("measure.mdat.load"));
    expect(actions.pickMdat).toHaveBeenCalledWith(null);
    expect(byUi("measure.options")).toBeNull();
  });

  it("sends untagged mdat rows to the screen's current position, as the hint says", () => {
    const scan = mdatAssignView.mdat!.scan;
    // A file whose titles name no position: the scan guesses 1 for every row.
    const untagged = scan.measurements.slice(0, 2).map((m) => ({ ...m, title: m.title.replace(" P4", ""), position: 1, position_tagged: false }));
    const view: MeasureView = { ...mdatAssignView, mdat: { ...mdatAssignView.mdat!, scan: { ...scan, measurements: untagged } } };
    renderStep(view);
    const current = currentPosition(view) ?? 1;
    const node = ui("measure.mdat.row.1.position");
    const select = node instanceof HTMLSelectElement ? node : node.querySelector("select")!;
    expect(select.value).toBe(String(current));
    expect(current).not.toBe(1);
  });

  it("keeps measuring unavailable until a measurement method is chosen", () => {
    renderStep(noPathView);
    expect((ui("measure.position.start") as HTMLButtonElement).disabled).toBe(true);
    expect(ui("frame.header.blocked").textContent).toContain("측정 방식");
  });

  it("selects positions on the map and in the list, but not while a job runs", () => {
    let actions = renderStep(partialView);
    fireEvent.click(ui("measure.map.position.6"));
    expect(actions.selectPosition).toHaveBeenCalledWith(6);
    fireEvent.click(ui("measure.positions.2"));
    expect(actions.selectPosition).toHaveBeenCalledWith(2);
    fireEvent.keyDown(ui("measure.map"), { key: "ArrowRight" });
    expect(actions.selectPosition).toHaveBeenCalledWith(5);

    cleanup();
    actions = renderStep(measuringView);
    fireEvent.click(ui("measure.map.position.6"));
    fireEvent.click(ui("measure.positions.2"));
    expect(actions.selectPosition).not.toHaveBeenCalled();
  });

  it("hides and shows the graph", () => {
    let actions = renderStep(partialView);
    fireEvent.click(ui("measure.graph.toggle"));
    expect(actions.setGraphHidden).toHaveBeenCalledWith(true);
    cleanup();
    actions = renderStep({ ...partialView, graphHidden: true });
    expect(ui("measure.graph").hidden).toBe(true);
    expect(ui("measure.graph.toggle").textContent).toBe("그래프 보기");
    fireEvent.click(ui("measure.graph.toggle"));
    expect(actions.setGraphHidden).toHaveBeenCalledWith(false);
  });

  it("says what 0 dB is: the reference measurement and its band, or nothing without one (U17)", () => {
    renderStep(referenceMidView);
    expect(ui("measure.plot.reference").textContent).toBe("0 dB = L 위치 1 곡선의 200~2000 Hz 평균");
    // It reads with the legend, below it, inside the graph card.
    expect(ui("measure.plot.legend").compareDocumentPosition(ui("measure.plot.reference")) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(ui("measure.graph").contains(ui("measure.plot.reference"))).toBe(true);
    cleanup();
    renderStep(referenceSubView);
    expect(ui("measure.plot.reference").textContent).toBe("0 dB = SW1 위치 1 곡선의 40~120 Hz 평균");
    cleanup();
    renderStep(referenceSubView, makeActions(), { locale: "en" });
    expect(ui("measure.plot.reference").textContent).toBe("0 dB = average of SW1 at position 1 over 40–120 Hz");
    cleanup();
    renderStep(partialView, makeActions(), { locale: "en" });
    expect(ui("measure.plot.reference").textContent).toBe("0 dB = average of L at position 1 over 200–2000 Hz");
    cleanup();
    // No reference (nothing measured, or a service that sends none): no line.
    renderStep(emptyView);
    expect(byUi("measure.plot.reference")).toBeNull();
    cleanup();
    renderStep({ ...partialView, curves: { ...partialView.curves!, reference: null } });
    expect(byUi("measure.plot")).not.toBeNull();
    expect(byUi("measure.plot.reference")).toBeNull();
    // The role as the screen writes it, at the position the service picked.
    expect(referenceText(t, { role: "Tfl", position: 3, band: "mid" })).toBe("0 dB = Ltf 위치 3 곡선의 200~2000 Hz 평균");
    expect(referenceText(t, null)).toBeNull();
  });

  it("keeps the 0 dB line when the legend hides the reference curve, its channel or its position", () => {
    renderStep(referenceMidView);
    const before = ui("measure.plot.reference").textContent;
    fireEvent.click(ui("measure.plot.legend.role.L"));
    expect(ui("measure.plot.legend.role.L").getAttribute("aria-pressed")).toBe("false");
    expect(ui("measure.plot.reference").textContent).toBe(before);
    fireEvent.click(ui("measure.plot.legend.position.1"));
    expect(ui("measure.plot.legend.position.1").getAttribute("aria-pressed")).toBe("false");
    expect(ui("measure.plot.reference").textContent).toBe(before);
    cleanup();
    // A service answer filtered to other curves still names the reference it shifted them by.
    const filtered = { ...referenceMidView.curves!, curves: referenceMidView.curves!.curves.filter((curve) => curve.role === "SW1") };
    renderStep({ ...referenceMidView, curves: filtered });
    expect(ui("measure.plot.reference").textContent).toBe(before);
  });

  it("draws the axis range P33 keeps for the run and reports the user's pick", () => {
    const actions = renderStep({ ...partialView, plotRange: { x: [20, 2000], y: null } });
    const plot = ui("measure.plot");
    expect(document.getElementById(plot.getAttribute("aria-describedby") ?? "")?.textContent).toContain("20–2k Hz");
    fireEvent.click(ui("measure.plot.axes"));
    fireEvent.click(ui("measure.plot.axes.auto"));
    expect(actions.setPlotRange).toHaveBeenCalledWith({ x: null, y: null });
  });

  it("shows the running position's progress and cancels its job", () => {
    const actions = renderStep(measuringView);
    expect(ui("measure.status").textContent).toContain("위치 4 측정 중");
    expect(ui("measure.run").textContent).toContain("채널 7/13");
    expect(ui("measure.run").textContent).toContain("Lrs 측정함");
    expect(ui("measure.run.progress").getAttribute("aria-valuenow")).toBe("7");
    expect(ui("measure.position.start").getAttribute("aria-busy")).toBe("true");
    fireEvent.click(ui("measure.cancel"));
    expect(actions.cancelJob).toHaveBeenCalledWith(12);
    expect((ui("measure.mdat.load") as HTMLButtonElement).disabled).toBe(true);
    expect((ui("measure.redo") as HTMLButtonElement).disabled).toBe(true);
  });

  it("counts the start delay down", () => {
    renderStep(countdownView);
    expect(ui("measure.run").textContent).toContain("7초 뒤 스윕 시작");
    expect(ui("measure.status").textContent).toContain("7초 뒤 시작");
  });

  it("warns about low S/N on the current position with the service's threshold", () => {
    renderStep(lowSnrView);
    const text = ui("measure.warning").textContent?.replace(/\u00a0/g, " ");
    expect(text).toContain("기준 20 dB");
    expect(text).toContain("SW2 17.2 dB, Rtr 18.6 dB");
    expect(text).toContain("다시 측정하세요");
    cleanup();
    const one = { ...lowSnrView, warnings: [{ position: 4, notices: lowSnrView.warnings[0]!.notices.slice(0, 1) }] };
    renderStep(one);
    expect(ui("measure.warning").textContent).toContain("SW2 신호 대 잡음비 17.2 dB: 기준 20 dB보다 낮습니다.");
  });

  it("clears the last position after confirming and offers undo (U5)", async () => {
    const actions = renderStep(partialView);
    fireEvent.click(ui("measure.redo"));
    const dialog = ui("measure.clear");
    expect(dialog.textContent).toContain("위치 3 측정 지우기");
    expect(dialog.textContent).toContain("채널 13개");
    await act(async () => {
      fireEvent.click(ui("measure.clear.confirm"));
    });
    expect(actions.clearPosition).toHaveBeenCalledWith(3);
    await act(async () => {
      fireEvent.click(ui("measure.clear.undo"));
    });
    expect(actions.undo).toHaveBeenCalledWith("token-7");
  });

  it("changes the start delay, the sweep length and the repetitions", () => {
    const actions = renderStep(partialView);
    fireEvent.change(ui("measure.options.delay"), { target: { value: "10 초" } });
    expect(actions.setStartDelay).toHaveBeenCalledWith("10 초");
    fireEvent.click(ui("measure.options.advanced"));
    fireEvent.change(ui("measure.options.sweep"), { target: { value: "524288" } });
    expect(actions.setSweepLength).toHaveBeenCalledWith(524288);
    // REW's acoustic reference allows one sweep: the choice is not drawn at all.
    expect(byUi("measure.options.repeats")).toBeNull();

    cleanup();
    const loopback: MeasureView = { ...partialView, layout: { ...partialView.layout, timing: { reference: "loopback", role: "", output: 14 } } };
    const other = makeActions();
    renderWith(<MeasureOptions view={loopback} actions={other} busy={false} advancedOpen />);
    fireEvent.click(within(ui("measure.options.repeats")).getByRole("radio", { name: "4회" }));
    expect(other.setRepetitions).toHaveBeenCalledWith(4);

    // Native measurement records one sweep per channel, whatever reference is stored.
    cleanup();
    renderWith(<MeasureOptions view={{ ...loopback, config: { ...loopback.config, method: "native" } }} actions={makeActions()} busy={false} advancedOpen />);
    expect(byUi("measure.options.repeats")).toBeNull();
  });

  it("offers only the sweep lengths the service supplies, here the one place they are set", () => {
    const view: MeasureView = { ...partialView, sweepOptions: [{ samples: 131072, seconds: 2.7 }, { samples: 524288, seconds: 10.9 }], config: { ...partialView.config, sweep_length: 131072 } };
    renderWith(<MeasureOptions view={view} actions={makeActions()} busy={false} advancedOpen />);
    const select = ui("measure.options.sweep") as HTMLSelectElement;
    expect(Array.from(select.options, (option) => option.value)).toEqual(["131072", "524288"]);
  });

  it("assigns mdat measurements: a doubled channel blocks the import until it is fixed", () => {
    const actions = renderStep(mdatAssignView);
    const dialog = ui("measure.mdat.dialog");
    expect(dialog.textContent).toContain("거실 위치 4.mdat 가져오기");
    const importButton = ui("measure.mdat.import") as HTMLButtonElement;
    expect(importButton.disabled).toBe(true);
    expect(ui("measure.mdat.problem").textContent).toContain("Rs");
    expect(ui("measure.mdat.row.8.role").getAttribute("aria-invalid")).toBe("true");
    // The unreadable measurement cannot be assigned.
    expect((ui("measure.mdat.row.15.role") as HTMLSelectElement).disabled).toBe(true);

    fireEvent.change(ui("measure.mdat.row.8.role"), { target: { value: "" } });
    expect(byUi("measure.mdat.problem")).toBeNull();
    expect(importButton.disabled).toBe(false);
    expect(importButton.textContent).toBe("측정 13개 가져오기");
    fireEvent.change(ui("measure.mdat.row.1.position"), { target: { value: "5" } });
    fireEvent.click(importButton);
    const params = actions.importMdat.mock.calls[0]?.[0];
    expect(params.path).toBe("C:/Users/demo/Documents/REW/거실 위치 4.mdat");
    expect(params.assignments).toHaveLength(13);
    expect(params.assignments[0]).toEqual({ measurement: "L P4", role: "L", index: 0, position: 5 });
    expect(params.assignments.some((a: { measurement: string }) => a.measurement === "Rs P4 (다시)")).toBe(false);

    fireEvent.click(within(dialog).getByRole("button", { name: "취소" }));
    expect(actions.closeMdat).toHaveBeenCalledOnce();
  });

  it("imports only the picked position of a Dirac Live project from 파일 관리, but every mdat row", () => {
    const row = (index: number, title: string, role: string, position: number) => ({
      index,
      title,
      role,
      position,
      position_tagged: true,
      looks_like_sub: role.startsWith("SW"),
    });
    const scan = {
      measurements: [row(0, "Left P1", "L", 1), row(1, "Subwoofer 1 P1", "SW1", 1), row(2, "Left P2", "L", 2), row(3, "Subwoofer 1 P2", "SW1", 2)],
      suggestions: [],
      problems: [],
    };
    const dirac = { ...mdatAssignView, mdat: { path: "C:/Users/demo/Dirac/projects/거실.liveproject", position: 2, scan } };
    const actions = renderStep(dirac);
    expect(byUi("measure.mdat.row.3.role")).toBeNull();
    const importButton = ui("measure.mdat.import") as HTMLButtonElement;
    expect(importButton.disabled).toBe(false);
    fireEvent.click(importButton);
    expect(actions.importMdat.mock.calls[0]?.[0].assignments).toEqual([
      { measurement: "Left P2", role: "L", index: 2, position: 2 },
      { measurement: "Subwoofer 1 P2", role: "SW1", index: 3, position: 2 },
    ]);

    // An mdat keeps upstream's rule: every measurement of the file goes to the picked position.
    cleanup();
    renderStep({ ...dirac, mdat: { ...dirac.mdat, path: "C:/Users/demo/Documents/REW/거실.mdat" } });
    expect(byUi("measure.mdat.row.4.role")).toBeTruthy();
    expect((ui("measure.mdat.import") as HTMLButtonElement).disabled).toBe(true);
  });

  it("reports the problems the way upstream does", () => {
    const rows = [
      { role: "L", position: 1 },
      { role: "L", position: 2 },
      { role: "R", position: 2 },
      { role: "R", position: 2 },
    ];
    const readable = [true, true, true, true];
    expect(assignmentProblems(rows, readable, 9).notice).toEqual({ code: "mdat_duplicate_roles", params: { roles: "R P2" } });
    expect(assignmentProblems(rows.slice(0, 2), readable, 9).notice).toBeNull();
    expect(assignmentProblems([{ role: "", position: 1 }], [true], 9).notice?.code).toBe("mdat_nothing_selected");
    // Rows outside the arrangement are skipped, as upstream.
    expect(assignmentProblems([{ role: "L", position: 12 }], [true], 9).notice?.code).toBe("mdat_nothing_selected");
  });

  it("manages files per position: import into a position, clear a position", async () => {
    const actions = renderStep(filesView);
    fireEvent.click(ui("measure.files.open"));
    const dialog = ui("measure.files");
    const rows = within(dialog).getAllByRole("row");
    expect(rows[5]?.textContent).toContain("거실 위치 5.mdat");
    expect(rows[5]?.textContent).toContain("8/13");
    expect(rows[1]?.textContent).toContain("측정함");
    expect((ui("measure.files.4.clear") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(ui("measure.files.4.load"));
    expect(actions.pickMdat).toHaveBeenCalledWith(4);
    fireEvent.click(ui("measure.files.5.clear"));
    await act(async () => {
      fireEvent.click(ui("measure.clear.confirm"));
    });
    expect(actions.clearPosition).toHaveBeenCalledWith(5);
  });

  it("shows the sources of imported positions", () => {
    renderStep(importedView);
    expect(ui("measure.sources").textContent).toBe("불러옴: 거실 7.1.4 위치 1-6.mdat (위치 1–6)");
    cleanup();
    renderStep(filesView);
    expect(ui("measure.sources").textContent).toBe("불러옴: 측정함 (위치 1–3) · 거실 위치 5.mdat (위치 5)");
  });

  it("offers the demo data only with the tutorial accent", () => {
    renderStep(tutorialView);
    expect(byUi("measure.sample.load")).toBeNull();
    cleanup();
    const actions = renderStep(tutorialView, makeActions(), { tutorial: true });
    fireEvent.click(ui("measure.sample.load"));
    expect(actions.loadSample).toHaveBeenCalledOnce();
  });

  it("computes the step state the way upstream can_advance decides", () => {
    expect(measureStepState(emptyView)).toEqual({ status: "ready" });
    expect(measureStepState(partialView)).toEqual({ status: "ready" });
    expect(measureStepState(noPathView)).toEqual({ status: "blocked", reason: "no_path" });
    // The step's own jobs keep it ready.
    expect(measureStepState(measuringView)).toEqual({ status: "ready" });
    expect(measureStepState(sampleLoadingView)).toEqual({ status: "ready" });
    expect(measureStepState(doneView)).toEqual({ status: "done" });
    expect(measureStepState(importedView)).toEqual({ status: "ready" });
    expect(measureStepState({ ...doneView, path: "none" })).toEqual({ status: "done" });
  });

  it("renders every id of the reach table in at least one state", async () => {
    const seen = new Set<string>();
    const collect = () => {
      for (const node of document.querySelectorAll("[data-ui], [id]")) {
        const id = node.getAttribute("data-ui") ?? node.getAttribute("id");
        if (id) seen.add(id);
        const plain = node.getAttribute("id");
        if (plain) seen.add(plain);
      }
    };
    const states: { view: MeasureView; tutorial?: boolean; extra?: ReactNode; then?: () => Promise<void> | void }[] = [
      ...Object.values(MEASURE_FIXTURES).map((view) => ({ view })),
      { view: tutorialView, tutorial: true },
      {
        view: partialView,
        then: async () => {
          fireEvent.click(ui("measure.redo"));
          collect();
          await act(async () => {
            fireEvent.click(ui("measure.clear.confirm"));
          });
        },
      },
      { view: partialView, extra: <MeasureOptions view={{ ...partialView, layout: { ...partialView.layout, timing: { reference: "none", role: "", output: 1 } } }} actions={makeActions()} busy={false} advancedOpen /> },
      {
        view: filesView,
        extra: <MdatFilesDialog open count={9} records={filesView.records} roleCount={13} busy={false} onLoad={() => {}} onClear={() => {}} onClose={() => {}} />,
      },
    ];
    for (const state of states) {
      cleanup();
      const actions = makeActions();
      const ui2 = (
        <>
          <Step view={state.view} actions={actions} />
          {state.extra}
        </>
      );
      if (state.tutorial) render(<PrefsScope initial={{ locale: "ko", theme: "light", accent: "tutorial" }}>{ui2}</PrefsScope>);
      else renderWith(ui2);
      await state.then?.();
      collect();
    }
    const missing = REACH_IDS.map((id) => id.replace("<n>", "1").replace("<role>", "L")).filter((id) => !seen.has(id));
    expect(missing).toEqual([]);
  });
});
