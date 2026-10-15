// @vitest-environment happy-dom
import { fireEvent, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AppFrame } from "../../app/AppFrame";
import { JobTray } from "../../app/jobs/JobTray";
import { initialJobs, jobsReducer } from "../../app/jobs/jobStore";
import { ProjectMenu, SettingsButton } from "../../app/menus";
import { useT, type Translate } from "../../i18n/react";
import { renderWith } from "../../test/render";
import { ExportScreen, exportHeader, rcText } from "./ExportScreen";
import {
  exportFrame,
  exportComputingFirst,
  exportDone,
  exportExporting,
  exportFailed,
  exportNoMimo,
  exportNoResult,
  exportReady,
  exportStale,
  exportTutorial,
} from "./fixtures";
import { canExport, exportStepState, mimoInclusion, type ExportActions, type ExportView } from "./types";
import { translate } from "../../i18n";

/** Every id docs/M5-reach.md §9 and §10 cite for this screen and the frame around it. */
const REACH_IDS = [
  "export.result.summary",
  "export.result.mimo",
  "export.result.rc",
  "export.result.roles",
  "export.result.stale",
  "export.status.stale",
  "export.result.empty",
  "export.go_calibrate",
  "export.folder.path",
  "export.folder.change",
  "export.folder.open",
  "export.run.camilla_dsp",
  "export.run.before_after_mdat",
  "export.run.staged_mdat",
  "export.advanced",
  "export.outcome.camilla_dsp",
  "export.outcome.camilla_dsp.open",
  "export.tutorial",
  "frame",
  "frame.rail",
  "frame.rail.system",
  "frame.rail.connect",
  "frame.rail.measure",
  "frame.rail.calibrate",
  "frame.rail.export",
  "frame.header.blocked",
  "frame.content",
  "frame.status.config",
  "frame.status.positions",
  "frame.status.calibration",
  "frame.status.path",
  "frame.status.file",
  "frame.project",
  "frame.project.new",
  "frame.project.open",
  "frame.project.save",
  "frame.project.save_as",
  "frame.help",
  "frame.settings",
  "frame.settings.language",
  "frame.settings.expert",
  "jobs.tray",
  "jobs.job.export",
  "jobs.cancel.export",
  "jobs.history",
] as const;

const STATES: Record<string, ExportView> = {
  noResult: exportNoResult,
  computingFirst: exportComputingFirst,
  stale: exportStale,
  ready: exportReady,
  exporting: exportExporting,
  done: exportDone,
  failed: exportFailed,
  noMimo: exportNoMimo,
  tutorial: exportTutorial,
};

function actions() {
  return {
    exportFiles: vi.fn<ExportActions["exportFiles"]>(),
    changeFolder: vi.fn<ExportActions["changeFolder"]>(),
    openFolder: vi.fn<ExportActions["openFolder"]>(),
    goToCalibrate: vi.fn<ExportActions["goToCalibrate"]>(),
  };
}

/** The screen inside the frame with its header, as P33 will mount it. */
function Mounted({ view, act, advanced = false }: { view: ExportView; act: ExportActions; advanced?: boolean }) {
  const t = useT();
  const header = exportHeader(view, act, t);
  return (
    <AppFrame
      model={exportFrame(view)}
      onNavigate={() => {}}
      primary={header.primary}
      headerStatus={header.status}
      projectActions={{ onNew: () => {}, onOpen: () => {}, onSave: () => {}, onSaveAs: () => {} }}
      onHelp={() => {}}
      version={{ version: "0.13.0", prerelease: true }}
      tray={<div />}
      shortcuts={false}
    >
      <ExportScreen view={view} actions={act} initialAdvanced={advanced} />
    </AppFrame>
  );
}

const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
const t: Translate = (key, params) => translate("ko", key, params);

describe("내보내기", () => {
  it("renders every state in both languages", () => {
    for (const locale of ["ko", "en"] as const) {
      for (const [name, view] of Object.entries(STATES)) {
        const { unmount } = renderWith(<Mounted view={view} act={actions()} advanced />, { locale });
        expect(ui("export"), name).not.toBeNull();
        unmount();
      }
    }
  });

  it("exports each kind through its button; the header primary exports CamillaDSP", () => {
    const act = actions();
    renderWith(<Mounted view={exportReady} act={act} advanced />);
    fireEvent.click(ui("export.run.camilla_dsp")!);
    fireEvent.click(ui("export.run.before_after_mdat")!);
    fireEvent.click(ui("export.run.staged_mdat")!);
    expect(act.exportFiles.mock.calls).toEqual([["camilla_dsp"], ["before_after_mdat"], ["staged_mdat"]]);
  });

  it("changes and opens the folder, and opens what a run wrote", () => {
    const act = actions();
    renderWith(<Mounted view={exportDone} act={act} />);
    fireEvent.click(ui("export.folder.change")!);
    fireEvent.click(ui("export.folder.open")!);
    fireEvent.click(ui("export.outcome.camilla_dsp.open")!);
    expect(act.changeFolder).toHaveBeenCalledTimes(1);
    expect(act.openFolder.mock.calls).toEqual([["D:/Audio/거실 보정"], ["D:/Audio/거실 보정/camilladsp"]]);
    expect(screen.getByText(/파일 12개 저장/)).toBeTruthy();
  });

  it("disables the exports without a result, while computing and while exporting", () => {
    for (const view of [exportNoResult, exportComputingFirst, exportExporting]) {
      const { unmount } = renderWith(<Mounted view={view} act={actions()} advanced />);
      expect((ui("export.run.staged_mdat") as HTMLButtonElement).disabled).toBe(true);
      expect((ui("export.run.camilla_dsp") as HTMLButtonElement).disabled).toBe(true);
      unmount();
    }
    expect(canExport(exportReady)).toBe(true);
    expect(canExport(exportTutorial)).toBe(true);
  });

  it("shows the header's primary busy only for its own export, and off while another export runs", () => {
    const other = exportHeader(exportExporting, actions(), t);
    expect(other.primary?.busy).toBe(false);
    expect(other.primary?.disabled).toBe(true);
    const own = exportHeader({ ...exportReady, exporting: "camilla_dsp" }, actions(), t);
    expect(own.primary?.busy).toBe(true);
    expect(own.primary?.disabled).toBe(false);
    const idle = exportHeader(exportReady, actions(), t);
    expect(idle.primary?.busy).toBe(false);
    expect(idle.primary?.disabled).toBe(false);
    // In the frame: the primary is off (not spinning) and names the running export as the reason.
    renderWith(<Mounted view={exportExporting} act={actions()} />);
    const primary = ui("export.run.camilla_dsp") as HTMLButtonElement;
    expect(primary.disabled).toBe(true);
    expect(primary.getAttribute("aria-busy")).toBeNull();
    expect(document.getElementById(primary.getAttribute("aria-describedby") ?? "")?.textContent).toContain("REW mdat");
  });

  it("exports a stale result as upstream does, with a warning on the card and in the header", () => {
    const act = actions();
    renderWith(<Mounted view={exportStale} act={act} advanced />);
    expect(ui("frame.header.blocked")).toBeNull();
    expect(ui("export.result.stale")?.textContent).toContain("마지막으로 계산한 결과가 저장됩니다");
    expect(ui("export.status.stale")?.textContent).toBe("다시 계산 필요");
    expect(canExport(exportStale)).toBe(true);
    fireEvent.click(ui("export.run.camilla_dsp")!);
    fireEvent.click(ui("export.run.before_after_mdat")!);
    expect(act.exportFiles.mock.calls).toEqual([["camilla_dsp"], ["before_after_mdat"]]);
  });

  it("sends the way to 보정 when there is nothing current to export", () => {
    const act = actions();
    renderWith(<Mounted view={exportStale} act={act} />);
    fireEvent.click(ui("export.go_calibrate")!);
    expect(act.goToCalibrate).toHaveBeenCalledTimes(1);
  });

  it("shows the failure with its cause, next step and the raw text behind 자세히", () => {
    renderWith(<Mounted view={exportFailed} act={actions()} />);
    expect(screen.getByText("내보내기 파일을 쓰지 못했습니다.")).toBeTruthy();
    expect(screen.getByText(/Access is denied/)).toBeTruthy();
    // The row may be below the fold in a small window: the header says it too.
    expect(screen.getByText("내보내기 실패")).toBeTruthy();
    expect(screen.queryByText("내보냄")).toBeNull();
  });

  it("says nothing is saved in the tutorial", () => {
    renderWith(<Mounted view={exportTutorial} act={actions()} />);
    expect(ui("export.tutorial")?.textContent).toContain("튜토리얼에서는 파일을 저장하지 않습니다.");
    expect(ui("export.outcome.camilla_dsp")?.textContent).toContain("튜토리얼");
    expect(ui("export.folder.change")).toBeNull();
  });

  it("computes the step state: blocked without a result or while computing, done after an export", () => {
    expect(exportStepState(exportNoResult)).toEqual({ status: "blocked", reason: "no_result" });
    expect(exportStepState(exportComputingFirst)).toEqual({ status: "blocked", reason: "computing" });
    expect(exportStepState({ ...exportReady, computing: true })).toEqual({ status: "blocked", reason: "computing" });
    expect(exportStepState(exportStale)).toEqual({ status: "ready" });
    expect(exportStepState(exportReady)).toEqual({ status: "ready" });
    expect(exportStepState(exportFailed)).toEqual({ status: "ready" });
    expect(exportStepState(exportTutorial)).toEqual({ status: "ready" });
    expect(exportStepState(exportDone)).toEqual({ status: "done" });
  });

  it("includes MIMO only when on, computed, current and with three positions", () => {
    expect(mimoInclusion(exportReady)).toEqual({ included: true, kind: "iir", supports: 22, channels: 11 });
    expect(mimoInclusion({ ...exportReady, settings: { ...exportReady.settings, mimo_on: false } })).toEqual({ included: false, reason: "off" });
    expect(mimoInclusion({ ...exportReady, positions: 2 })).toEqual({ included: false, reason: "positions" });
    expect(mimoInclusion({ ...exportReady, results: { ...exportReady.results, mimo: null } })).toEqual({ included: false, reason: "none" });
    expect(mimoInclusion(exportStale)).toEqual({ included: false, reason: "stale" });
  });

  it("writes the RC notices as phrases with whole numbers", () => {
    const t = (key: Parameters<typeof translate>[1], params?: Record<string, string | number>) => translate("ko", key, params);
    const n = (value: number) => String(Math.round(value));
    expect(rcText(t, { code: "rc_limit", params: { hz: "310.4", auto: "true", schroeder_hz: "310.4" } }, n)).toBe("RC 상한 310 Hz (자동 · 슈뢰더 310 Hz)");
    expect(rcText(t, { code: "rc_limit", params: { hz: "250", auto: "false" } }, n)).toBe("RC 상한 250 Hz (직접 정함)");
    expect(rcText(t, { code: "rc_room_volume", params: { volume_m3: "62.5" } }, n)).toBe("방 부피 63 m³로 정함");
    expect(rcText(t, { code: "rc_auto_cap", params: { hz: "500" } }, n)).toBe("자동 상한은 최대 500 Hz");
  });

  it("draws every id the reach table cites in at least one state", () => {
    const seen = new Set<string>();
    const collect = () => {
      for (const node of document.querySelectorAll("[data-ui]")) seen.add(node.getAttribute("data-ui") ?? "");
    };
    for (const view of Object.values(STATES)) {
      const { unmount } = renderWith(<Mounted view={view} act={actions()} advanced />);
      collect();
      unmount();
    }
    // The frame's menus open and the tray with an export running (§10 cites them).
    const jobs = jobsReducer(initialJobs, { type: "event", at: 0, event: { event: "started", job: 1, kind: "export" } });
    renderWith(
      <>
        <ProjectMenu name="거실 7.1.4.secs" dirty actions={{ onNew: () => {}, onOpen: () => {}, onSave: () => {}, onSaveAs: () => {} }} initiallyOpen inline />
        <SettingsButton initiallyOpen inline />
        <JobTray api={{ state: jobs, now: 1000, cancel: () => {}, clearHistory: () => {} }} defaultOpen />
      </>,
    );
    collect();
    expect(REACH_IDS.filter((id) => !seen.has(id))).toEqual([]);
  });
});
