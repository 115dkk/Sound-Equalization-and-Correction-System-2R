// @vitest-environment happy-dom
import { fireEvent, screen, within } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { AppFrame } from "../../app/AppFrame";
import { JobTray } from "../../app/jobs/JobTray";
import { initialLedger, ledgerReducer } from "../../state/jobLedger";
import { trayState } from "../../app/jobs/useJobs";
import { ProjectMenu, SettingsButton } from "../../app/menus";
import { useT, type Translate } from "../../i18n/react";
import { renderWith } from "../../test/render";
import { ExportScreen, exportHeader, rcText } from "./ExportScreen";
import {
  DESIGN_NAME_SAMPLES,
  applyCamillaApplied,
  applyCamillaApplying,
  applyCamillaRolledBack,
  applyEqapoAbsent,
  applyEqapoDenied,
  manageEqapoRepaired,
  exportFrame,
  exportComputingFirst,
  exportConvolutionDone,
  exportConvolutionFailed,
  exportDevicesAsio,
  exportDevicesAsioUnnamed,
  exportDevicesTyped,
  exportDevicesWasapi,
  exportDone,
  exportNamed,
  exportEqapoDone,
  exportExporting,
  exportExtraRates,
  exportFailed,
  exportMeasurementsDone,
  exportMeasurementsOnly,
  exportMimoNoSubs,
  exportNoMimo,
  exportNoResult,
  exportNothingMeasured,
  exportRateFallback,
  exportRate96k,
  exportRcOnly,
  exportReady,
  exportRecalibrating,
  exportResultsOnly,
  exportStale,
  exportStaleCache,
  exportStaleCacheMeasurements,
  exportTutorial,
} from "./fixtures";
import { EXPORT_KINDS, EXPORT_TABS, FILTER_KINDS, TAB_KINDS, canExport, defaultExportTab, exportStepState, mimoInclusion, tabOf, tabState, type ApplyRun, type DesignNameProblem, type ExportActions, type ExportTab, type ExportView, type TabState } from "./types";
import type { ExportKind } from "../../ipc/bindings/ExportKind";
import { translate } from "../../i18n";

/** Every id docs/M5-reach.md §9 and §10 cite for this screen and its frame, plus the destinations and filter formats. */
const REACH_IDS = [
  "export.tabs",
  "export.panel",
  ...EXPORT_TABS.map((tab) => `export.tab.${tab}`),
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
  "export.run.filter_text",
  "export.run.equalizer_apo",
  "export.kind.camilla_dsp",
  "export.kind.filter_text",
  "export.kind.equalizer_apo",
  "export.kind.before_after_mdat",
  "export.kind.staged_mdat",
  "export.kind.filter_text.blocked",
  "export.mdat.scope",
  "export.mdat.file",
  "export.advanced",
  "export.outcome.camilla_dsp",
  "export.outcome.camilla_dsp.open",
  "export.outcome.equalizer_apo",
  "export.outcome.equalizer_apo.open",
  "export.outcome.equalizer_apo.hint",
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
  "jobs.tray",
  "jobs.job.export",
  "jobs.cancel.export",
  "jobs.history",
] as const;

const STATES: Record<string, ExportView> = {
  noResult: exportNoResult,
  computingFirst: exportComputingFirst,
  stale: exportStale,
  staleCache: exportStaleCache,
  recalibrating: exportRecalibrating,
  ready: exportReady,
  exporting: exportExporting,
  done: exportDone,
  failed: exportFailed,
  noMimo: exportNoMimo,
  rcOnly: exportRcOnly,
  mimoNoSubs: exportMimoNoSubs,
  eqapoDone: exportEqapoDone,
  tutorial: exportTutorial,
  measurementsOnly: exportMeasurementsOnly,
  resultsOnly: exportResultsOnly,
  measurementsDone: exportMeasurementsDone,
  nothingMeasured: exportNothingMeasured,
  staleCacheMeasurements: exportStaleCacheMeasurements,
  convolutionDone: exportConvolutionDone,
  convolutionFailed: exportConvolutionFailed,
  named: exportNamed,
  nameProblem: { ...exportRcOnly, designName: DESIGN_NAME_SAMPLES.character },
  devicesWasapi: exportDevicesWasapi,
  devicesTyped: exportDevicesTyped,
  devicesAsio: exportDevicesAsio,
  devicesAsioUnnamed: exportDevicesAsioUnnamed,
  rate96k: exportRate96k,
  extraRates: exportExtraRates,
  rateFallback: exportRateFallback,
};

function actions() {
  return {
    selectTab: vi.fn<ExportActions["selectTab"]>(),
    exportFiles: vi.fn<ExportActions["exportFiles"]>(),
    setMdatScope: vi.fn<ExportActions["setMdatScope"]>(),
    setDesignName: vi.fn<ExportActions["setDesignName"]>(),
    setCamillaDevices: vi.fn<ExportActions["setCamillaDevices"]>(),
    setExtraRates: vi.fn<ExportActions["setExtraRates"]>(),
    refreshDevices: vi.fn<ExportActions["refreshDevices"]>(),
    changeFolder: vi.fn<ExportActions["changeFolder"]>(),
    openFolder: vi.fn<ExportActions["openFolder"]>(),
    goToCalibrate: vi.fn<ExportActions["goToCalibrate"]>(),
    goToSystem: vi.fn<ExportActions["goToSystem"]>(),
    recalibrate: vi.fn<ExportActions["recalibrate"]>(),
    setApplyLevel: vi.fn<ExportActions["setApplyLevel"]>(),
    setCamillaAddress: vi.fn<ExportActions["setCamillaAddress"]>(),
    saveCamillaAddress: vi.fn<ExportActions["saveCamillaAddress"]>(),
    refreshApplyStatus: vi.fn<ExportActions["refreshApplyStatus"]>(),
    applyCamilla: vi.fn<ExportActions["applyCamilla"]>(),
    applyEqapo: vi.fn<ExportActions["applyEqapo"]>(),
    revertEqapo: vi.fn<ExportActions["revertEqapo"]>(),
    setCamillaExe: vi.fn<ExportActions["setCamillaExe"]>(),
    saveCamillaExe: vi.fn<ExportActions["saveCamillaExe"]>(),
    pickCamillaExe: vi.fn<ExportActions["pickCamillaExe"]>(),
    installHost: vi.fn<ExportActions["installHost"]>(),
    stopManaging: vi.fn<ExportActions["stopManaging"]>(),
  };
}

/** The screen inside the frame with its header, as P33 will mount it. */
function Mounted({ view, act, advanced = false, devices = false, rates = false }: { view: ExportView; act: ExportActions; advanced?: boolean; devices?: boolean; rates?: boolean }) {
  const t = useT();
  // The tabs work as in the app: the frame keeps the tab the session would.
  const [tab, setTab] = useState(view.tab);
  const shown = { ...view, tab };
  const live = { ...act, selectTab: (next: ExportTab) => { act.selectTab(next); setTab(next); } };
  const header = exportHeader(shown, t);
  return (
    <AppFrame
      model={exportFrame(shown)}
      onNavigate={() => {}}
      headerStatus={header.status}
      projectActions={{ onNew: () => {}, onOpen: () => {}, onSave: () => {}, onSaveAs: () => {} }}
      onHelp={() => {}}
      version={{ version: "0.13.0", prerelease: true }}
      tray={<div />}
      shortcuts={false}
    >
      <ExportScreen view={shown} actions={live} initialAdvanced={advanced} initialDevices={devices} initialExtraRates={rates} />
    </AppFrame>
  );
}

const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
/** Opens the tab that holds `kind`, as a click on it does. */
const show = (kind: ExportKind) => fireEvent.click(ui(`export.tab.${tabOf(kind)}`)!);
const t: Translate = (key, params) => translate("ko", key, params);

describe("내보내기", () => {
  it("renders every state in both languages", () => {
    for (const locale of ["ko", "en"] as const) {
      for (const [name, view] of Object.entries(STATES)) {
        // The device states draw their disclosure open.
        const { unmount } = renderWith(<Mounted view={view} act={actions()} advanced devices={name.startsWith("devices")} />, { locale });
        expect(ui("export"), name).not.toBeNull();
        unmount();
      }
    }
    // Full frames in both languages need room on slower CI runners.
  }, 15_000);

  it("exports each kind through its panel action with exactly one filled button and none in the header", () => {
    const act = actions();
    renderWith(<Mounted view={exportRcOnly} act={act} advanced />);
    for (const kind of EXPORT_KINDS) {
      show(kind);
      const main = TAB_KINDS[tabOf(kind)][0];
      expect([...document.querySelectorAll(".ui-button--primary")]).toEqual([ui(`export.run.${main}`)]);
      expect(document.querySelector(".step-header .ui-button")).toBeNull();
      expect(ui(`export.run.${kind}`)?.closest(".export-item__action")).not.toBeNull();
      expect(ui(`export.run.${kind}`)?.classList.contains("ui-button--secondary")).toBe(kind === "staged_mdat");
      fireEvent.click(ui(`export.run.${kind}`)!);
    }
    expect(act.exportFiles.mock.calls).toEqual([["camilla_dsp"], ["filter_text"], ["equalizer_apo"], ["convolution"], ["before_after_mdat"], ["staged_mdat"]]);
  });

  it("draws destination tabs in EXPORT_TABS order with their names and captions in Korean and English", () => {
    const copy = {
      ko: [["CamillaDSP", "소프트웨어 DSP"], ["Equalizer APO", "Windows 시스템 EQ"], ["컨볼루션 IR", "플레이어의 컨볼버"], ["필터 값 텍스트", "DSP에 직접 입력"], ["REW", "보정 전후 비교"]],
      en: [["CamillaDSP", "Software DSP"], ["Equalizer APO", "Windows system EQ"], ["Convolution IR", "Player convolvers"], ["Filter text", "Typed into a DSP"], ["REW", "Before and after"]],
    };
    for (const locale of ["ko", "en"] as const) {
      const { unmount } = renderWith(<Mounted view={exportRcOnly} act={actions()} />, { locale });
      const tabs = screen.getByRole("tablist", { name: locale === "ko" ? "내보낼 곳" : "Export to" });
      expect(tabs).toBe(ui("export.tabs"));
      const buttons = within(tabs).getAllByRole("tab");
      expect(buttons.map((tab) => tab.getAttribute("data-ui"))).toEqual(EXPORT_TABS.map((tab) => `export.tab.${tab}`));
      expect(buttons.map((tab) => [tab.querySelector(".export-tab__name")?.textContent, tab.querySelector(".export-tab__use")?.textContent])).toEqual(copy[locale]);
      expect(buttons.map((tab) => tab.getAttribute("aria-selected"))).toEqual(["true", "false", "false", "false", "false"]);
      expect(buttons.map((tab) => tab.tabIndex)).toEqual([0, -1, -1, -1, -1]);
      expect(buttons.map((tab) => tab.getAttribute("aria-controls"))).toEqual(["export.panel", null, null, null, null]);
      for (const [index, tab] of buttons.entries()) {
        expect(within(tabs).getByRole("tab", { name: copy[locale][index]!.join(" ") })).toBe(tab);
      }
      unmount();
    }
  });

  it("reads the convolution IR panel like its neighbours: what, where, its button, never blocked by subs or MIMO", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={exportReady} act={act} />);
    show("filter_text");
    expect(ui("export.kind.filter_text.blocked")).not.toBeNull();
    show("convolution");
    const row = ui("export.kind.convolution")!;
    expect(row.querySelector("h3")?.textContent).toBe("컨볼루션 IR");
    expect(row.querySelector(".export-item__where")?.textContent).toBe("SECS_convolution_RC_BA_MIMO_mixed_<저장 시각> 폴더");
    fireEvent.click(ui("export.kind.convolution.help")!);
    expect(screen.getByRole("tooltip").textContent).toBe(t("export.kind.convolution.body"));
    const run = ui("export.run.convolution") as HTMLButtonElement;
    expect(run.textContent).toBe("컨볼루션 IR 저장");
    // Subs and MIMO block the text, not the convolution IR.
    expect(run.disabled).toBe(false);
    expect(ui("export.kind.convolution.blocked")).toBeNull();
    fireEvent.click(run);
    expect(act.exportFiles.mock.calls).toEqual([["convolution"]]);
    unmount();
    // Running: busy in its row and in the header's pill.
    const { unmount: unmountRunning } = renderWith(<Mounted view={{ ...exportReady, tab: "convolution", exporting: "convolution" }} act={actions()} />);
    expect(ui("export.run.convolution")?.getAttribute("aria-busy")).toBe("true");
    expect(screen.getByText("컨볼루션 IR 저장 중")).toBeTruthy();
    unmountRunning();
    // Done: the files, the folder, and where the guide says which file goes where.
    const done = actions();
    const { unmount: unmountDone } = renderWith(<Mounted view={exportConvolutionDone} act={done} />);
    expect(ui("export.outcome.convolution")?.textContent).toContain("파일 29개 저장 → D:/Audio/거실 보정/SECS_convolution_RC_BA_MIMO_mixed_20261014-142233");
    expect(ui("export.outcome.convolution.hint")?.textContent).toBe(t("export.outcome.convolution.hint"));
    expect(ui("export.outcome.convolution.hint")?.textContent).toContain("컨볼루션 적용 방법.txt");
    fireEvent.click(ui("export.outcome.convolution.open")!);
    expect(done.openFolder.mock.calls).toEqual([["D:/Audio/거실 보정/SECS_convolution_RC_BA_MIMO_mixed_20261014-142233"]]);
    unmountDone();
    // Failed: the cause and next step, the raw text behind 자세히, and the header says so.
    renderWith(<Mounted view={exportConvolutionFailed} act={actions()} />);
    expect(ui("export.outcome.convolution")?.textContent).toContain("파일을 내보내지 못했습니다.");
    expect(screen.getByText(/not enough space on the disk/)).toBeTruthy();
    expect(screen.getByText("내보내기 실패")).toBeTruthy();
  });

  it("labels one panel by its chosen tab, draws only that tab's exports, and puts fields under the correct headings", () => {
    renderWith(<Mounted view={exportRcOnly} act={actions()} advanced />);
    const files = ui("export.files")!;
    expect(files.querySelector("h2")?.textContent).toBe("내보낼 곳");
    expect(files.querySelector("section.export-group")).toBeNull();
    expect(files.querySelector("[data-ui^='export.group.']")).toBeNull();
    const titles: Record<ExportTab, string> = { camilla_dsp: "CamillaDSP 설정", equalizer_apo: "Equalizer APO 설정", convolution: "컨볼루션 IR", filter_text: "필터 값 텍스트", rew: "REW mdat" };
    for (const tab of EXPORT_TABS) {
      fireEvent.click(ui(`export.tab.${tab}`)!);
      const panel = screen.getByRole("tabpanel");
      expect(panel).toBe(ui("export.panel"));
      expect(panel.getAttribute("data-tab")).toBe(tab);
      expect(panel.getAttribute("aria-labelledby")).toBe(`export.tab.${tab}`);
      expect(document.getElementById(panel.getAttribute("aria-labelledby")!)).toBe(ui(`export.tab.${tab}`));
      expect(screen.getByRole("tab", { selected: true })).toBe(ui(`export.tab.${tab}`));
      expect([...document.querySelectorAll("section[data-ui^='export.kind.']")].map((section) => section.getAttribute("data-ui"))).toEqual(TAB_KINDS[tab].map((kind) => `export.kind.${kind}`));
      const main = TAB_KINDS[tab][0];
      expect(ui(`export.kind.${main}`)?.querySelector("h3")?.textContent).toBe(titles[tab]);
      expect(panel.querySelectorAll("h3")).toHaveLength(1);
      expect(panel.querySelectorAll("h4")).toHaveLength(tab === "rew" ? 1 : 0);
      if (tab === "rew") {
        expect(ui("export.kind.staged_mdat")?.querySelector("h4")?.textContent).toBe("단계별 mdat");
        expect(ui("export.advanced")?.getAttribute("aria-expanded")).toBe("true");
      }
      expect(panel.querySelectorAll("[data-ui='export.design_name']")).toHaveLength(tab === "rew" ? 0 : 1);
      expect(document.querySelectorAll("[data-ui='export.devices']")).toHaveLength(tab === "camilla_dsp" ? 1 : 0);
      expect(document.querySelectorAll("[data-ui='export.folder']")).toHaveLength(1);
      expect(ui(`export.kind.${main}`)?.contains(ui("export.folder"))).toBe(true);
      if (tab === "camilla_dsp") {
        expect(ui("export.kind.camilla_dsp")?.contains(ui("export.devices"))).toBe(true);
        fireEvent.click(ui("export.kind.camilla_dsp.help")!);
        expect(screen.getByRole("tooltip").textContent).toBe(t("export.kind.camilla_dsp.body"));
        fireEvent.click(ui("export.kind.camilla_dsp.help")!);
      }
    }
    expect(ui("export.run.busy")).toBeNull();
  });

  it("selects and focuses tabs with the arrows, Home and End, wrapping at both ends, and reports clicks", () => {
    const act = actions();
    renderWith(<Mounted view={exportRcOnly} act={act} />);
    ui("export.tab.camilla_dsp")!.focus();
    const moves: [string, ExportTab][] = [
      ["ArrowLeft", "rew"], ["ArrowRight", "camilla_dsp"], ["ArrowRight", "equalizer_apo"],
      ["ArrowLeft", "camilla_dsp"], ["End", "rew"], ["Home", "camilla_dsp"],
    ];
    for (const [key, tab] of moves) {
      fireEvent.keyDown(document.activeElement!, { key });
      expect(act.selectTab).toHaveBeenLastCalledWith(tab);
      expect(document.activeElement).toBe(ui(`export.tab.${tab}`));
      expect(screen.getByRole("tab", { selected: true })).toBe(ui(`export.tab.${tab}`));
      expect(screen.getAllByRole("tab").filter((button) => button.tabIndex === 0)).toEqual([ui(`export.tab.${tab}`)]);
      expect(ui("export.panel")?.getAttribute("data-tab")).toBe(tab);
    }
    expect(act.selectTab.mock.calls).toEqual(moves.map(([, tab]) => [tab]));
    fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
    expect(act.selectTab).toHaveBeenCalledTimes(moves.length);
    fireEvent.click(ui("export.tab.convolution")!);
    expect(act.selectTab).toHaveBeenLastCalledWith("convolution");
    expect(screen.getByRole("tab", { selected: true })).toBe(ui("export.tab.convolution"));
    expect(ui("export.panel")?.getAttribute("data-tab")).toBe("convolution");
  });

  it("turns off a filter format that cannot carry the result and says why in its panel", () => {
    const cases: [ExportView, Record<"filter_text" | "equalizer_apo", string | null>][] = [
      // Subs and MIMO (the sample): text blocked by the subs first; Equalizer APO carries both (F12).
      [exportReady, { filter_text: "서브(BA)가 포함된 결과는 텍스트로 내보낼 수 없습니다. Equalizer APO, CamillaDSP, 컨볼루션 IR은 서브와 MIMO를 지원합니다.", equalizer_apo: null }],
      // Subs, MIMO left out: only the text.
      [exportNoMimo, { filter_text: "서브(BA)가 포함된 결과는 텍스트로 내보낼 수 없습니다. Equalizer APO, CamillaDSP, 컨볼루션 IR은 서브와 MIMO를 지원합니다.", equalizer_apo: null }],
      // MIMO without subs: the text by MIMO.
      [exportMimoNoSubs, { filter_text: "MIMO가 포함된 결과는 텍스트로 내보낼 수 없습니다. Equalizer APO, CamillaDSP, 컨볼루션 IR은 서브와 MIMO를 지원합니다.", equalizer_apo: null }],
      // RC alone: everything on.
      [exportRcOnly, { filter_text: null, equalizer_apo: null }],
    ];
    for (const [view, reasons] of cases) {
      const act = actions();
      const { unmount } = renderWith(<Mounted view={view} act={act} advanced />);
      for (const kind of ["filter_text", "equalizer_apo"] as const) {
        show(kind);
        const button = ui(`export.run.${kind}`) as HTMLButtonElement;
        const reason = ui(`export.kind.${kind}.blocked`);
        expect(button.disabled, kind).toBe(reasons[kind] !== null);
        expect(reason?.textContent ?? null, kind).toBe(reasons[kind]);
        // The disabled button names its reason; the reason is no alert.
        expect(button.getAttribute("aria-describedby") ? document.getElementById(button.getAttribute("aria-describedby")!) : null).toBe(reason);
        expect(reason?.getAttribute("role") ?? null).toBeNull();
        fireEvent.click(button);
      }
      // CamillaDSP, the convolution IR and the mdat exports carry every result.
      for (const kind of ["camilla_dsp", "convolution", "before_after_mdat", "staged_mdat"] as const) {
        show(kind);
        expect((ui(`export.run.${kind}`) as HTMLButtonElement).disabled, kind).toBe(false);
        expect(ui(`export.kind.${kind}.blocked`), kind).toBeNull();
      }
      expect(act.exportFiles.mock.calls).toEqual((["filter_text", "equalizer_apo"] as const).filter((kind) => reasons[kind] === null).map((kind) => [kind]));
      unmount();
    }
    // With MIMO in it, the Equalizer APO folder is named like the mdat one.
    renderWith(<Mounted view={exportMimoNoSubs} act={actions()} advanced />);
    show("equalizer_apo");
    expect(ui("export.kind.equalizer_apo")?.textContent).toContain("SECS_eqapo_RC_BA_MIMO_mixed_<저장 시각> 폴더");
  });

  it("offers 재보정 right after the sentence that blocks both mdat formats for a stale cached result", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={exportStaleCache} act={act} advanced />);
    for (const kind of ["before_after_mdat", "staged_mdat"] as const) {
      const run = ui(`export.run.${kind}`) as HTMLButtonElement;
      const reason = ui(`export.kind.${kind}.blocked`);
      const recalibrate = ui(`export.kind.${kind}.recalibrate`) as HTMLButtonElement | null;
      expect(reason?.textContent, kind).toBe("구 버전에서 측정한 값입니다. mdat으로 내보내려면 다시 보정해야 합니다.");
      expect(run.disabled, kind).toBe(true);
      expect(document.getElementById(run.getAttribute("aria-describedby") ?? ""), kind).toBe(reason);
      // The action follows its sentence in the same notice and remains secondary to the panel's filled action.
      expect(recalibrate?.textContent, kind).toBe("재보정");
      expect(reason?.nextElementSibling, kind).toBe(recalibrate);
      expect(recalibrate?.classList.contains("ui-button--secondary"), kind).toBe(true);
      expect(recalibrate?.disabled, kind).toBe(false);
      expect(recalibrate?.getAttribute("aria-busy"), kind).toBeNull();
      expect(document.getElementById(recalibrate?.getAttribute("aria-describedby") ?? ""), kind).toBe(reason);
      fireEvent.click(recalibrate!);
    }
    // Two buttons with one label: each one's name carries its format.
    expect(screen.getByRole("button", { name: "재보정 REW mdat" })).toBe(ui("export.kind.before_after_mdat.recalibrate"));
    expect(screen.getByRole("button", { name: "재보정 단계별 mdat" })).toBe(ui("export.kind.staged_mdat.recalibrate"));
    expect(act.recalibrate).toHaveBeenCalledTimes(2);
    expect(act.exportFiles).not.toHaveBeenCalled();
    // CamillaDSP carries the cached result; the subs' block on the text has nothing to recalibrate.
    expect([...document.querySelectorAll("[data-ui$='.recalibrate']")].map((el) => el.getAttribute("data-ui"))).toEqual([
      "export.kind.before_after_mdat.recalibrate",
      "export.kind.staged_mdat.recalibrate",
    ]);
    show("camilla_dsp");
    expect((ui("export.run.camilla_dsp") as HTMLButtonElement).disabled).toBe(false);
    show("filter_text");
    expect(ui("export.kind.filter_text.recalibrate")).toBeNull();
    unmount();
    // A stale result computed in this session exports mdat as it is; English names the formats too.
    const { unmount: unmountStale } = renderWith(<Mounted view={exportStale} act={actions()} advanced />);
    show("before_after_mdat");
    expect(document.querySelector("[data-ui$='.recalibrate']")).toBeNull();
    unmountStale();
    renderWith(<Mounted view={exportStaleCache} act={actions()} advanced />, { locale: "en" });
    expect(ui("export.kind.staged_mdat.blocked")?.textContent).toBe("These values were measured with an earlier version. Recalibrate to export mdat.");
    expect(screen.getByRole("button", { name: "Recalibrate REW mdat" })).toBe(ui("export.kind.before_after_mdat.recalibrate"));
    expect(screen.getByRole("button", { name: "Recalibrate Staged mdat" })).toBe(ui("export.kind.staged_mdat.recalibrate"));
  });

  it("keeps 재보정 busy and unclickable while the correction runs, and off while an export runs", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={exportRecalibrating} act={act} advanced />);
    for (const kind of ["before_after_mdat", "staged_mdat"] as const) {
      const recalibrate = ui(`export.kind.${kind}.recalibrate`) as HTMLButtonElement;
      expect(recalibrate.disabled, kind).toBe(true);
      expect(recalibrate.getAttribute("aria-busy"), kind).toBe("true");
      // The spinner joins the label; the label stays.
      expect(recalibrate.textContent, kind).toBe("재보정");
      expect((ui(`export.run.${kind}`) as HTMLButtonElement).disabled, kind).toBe(true);
      fireEvent.click(recalibrate);
    }
    unmount();
    // A CamillaDSP export runs on the cached result: a new one must not replace it mid-export.
    renderWith(<Mounted view={{ ...exportStaleCache, exporting: "camilla_dsp" }} act={act} advanced />);
    const recalibrate = ui("export.kind.before_after_mdat.recalibrate") as HTMLButtonElement;
    expect(recalibrate.disabled).toBe(true);
    expect(recalibrate.getAttribute("aria-busy")).toBeNull();
    fireEvent.click(recalibrate);
    expect(act.recalibrate).not.toHaveBeenCalled();
  });

  it("says where the Equalizer APO files go after they are saved, with the filter files' mode", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={exportEqapoDone} act={act} />);
    expect(ui("export.outcome.equalizer_apo")?.textContent).toContain("파일 14개 저장 → D:/Audio/거실 보정/SECS_eqapo_RC_BA_mixed_20261014-142233");
    expect(ui("export.outcome.equalizer_apo.hint")?.textContent).toBe(
      "Equalizer APO의 기존 config.txt를 다른 이름으로 보관한 뒤, 내보낸 config.txt와 secs_mixed 폴더를 config 폴더에 복사하십시오. 적용 순서는 'EqAPO 적용 방법.txt'를 확인하십시오.",
    );
    fireEvent.click(ui("export.outcome.equalizer_apo.open")!);
    expect(act.openFolder.mock.calls).toEqual([["D:/Audio/거실 보정/SECS_eqapo_RC_BA_mixed_20261014-142233"]]);
    unmount();
    // 초고지연: the files carry the limit (upstream `ultra200`); the folder keeps the bare mode.
    renderWith(<Mounted view={{ ...exportEqapoDone, settings: { ...exportEqapoDone.settings, mode: "ultra", ultra_ms: 200 } }} act={actions()} />, { locale: "en" });
    expect(ui("export.outcome.equalizer_apo.hint")?.textContent).toContain("config.txt and secs_ultra200 folder");
    expect(ui("export.kind.equalizer_apo")?.textContent).toContain("SECS_eqapo_RC_BA_ultra_<save time> folder");
  });

  it("adds a hint only to the Equalizer APO and convolution IR outcomes", () => {
    renderWith(<Mounted view={exportDone} act={actions()} />);
    for (const kind of ["camilla_dsp", "before_after_mdat"] as const) {
      show(kind);
      expect(ui(`export.outcome.${kind}`)).not.toBeNull();
      expect(document.querySelector("[data-ui^='export.outcome.'][data-ui$='.hint']")).toBeNull();
    }
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

  it("disables the exports without a result, while computing and while exporting; the REW mdat's measurements need measurements only", () => {
    // Without a result the REW mdat still writes the original measurements; a running export stops it too.
    const cases: [ExportView, boolean][] = [
      [exportNoResult, true],
      [exportComputingFirst, true],
      [exportExporting, false],
      [{ ...exportRcOnly, computing: true }, false],
    ];
    for (const [view, mdatOn] of cases) {
      const { unmount } = renderWith(<Mounted view={view} act={actions()} advanced />);
      for (const kind of EXPORT_KINDS) {
        show(kind);
        expect((ui(`export.run.${kind}`) as HTMLButtonElement).disabled, kind).toBe(kind === "before_after_mdat" ? !mdatOn : true);
      }
      unmount();
    }
    expect(canExport(exportReady)).toBe(true);
    expect(canExport(exportTutorial)).toBe(true);
    expect(canExport({ ...exportMeasurementsOnly, computing: true }, "before_after_mdat")).toBe(true);
    expect(canExport({ ...exportReady, computing: true }, "before_after_mdat")).toBe(false);
    expect(canExport(exportNothingMeasured, "before_after_mdat")).toBe(false);
    expect(canExport({ ...exportMeasurementsOnly, exporting: "camilla_dsp" }, "before_after_mdat")).toBe(false);
  });

  it("chooses what the REW mdat carries, and its file name says which", () => {
    const names: [ExportView, string, string][] = [
      [exportReady, "모두", "SECS 원 측정+보정 결과 저지연 모드.mdat"],
      [exportMeasurementsOnly, "원 측정만", "SECS 원 측정.mdat"],
      [exportResultsOnly, "보정 결과만", "SECS 보정 결과 저지연 모드.mdat"],
      [{ ...exportResultsOnly, settings: { ...exportResultsOnly.settings, mode: "ultra", ultra_ms: 123.456789 } }, "보정 결과만", "SECS 보정 결과 초고지연 모드 123.457 ms.mdat"],
    ];
    for (const [view, chosen, file] of names) {
      const act = actions();
      const { unmount } = renderWith(<Mounted view={view} act={act} />);
      show("before_after_mdat");
      const group = screen.getByRole("radiogroup", { name: "저장할 내용" });
      expect(group).toBe(ui("export.mdat.scope"));
      // The rows' 적용 choices are radios too: these are the REW mdat's.
      expect(within(group).getByRole("radio", { checked: true }).textContent).toBe(chosen);
      expect(ui("export.mdat.file")?.textContent).toBe(file);
      // The choice comes first, the file's name last; every option can be chosen while a result exists.
      const row = ui("export.kind.before_after_mdat")!;
      const order = [...row.querySelectorAll("[data-ui='export.mdat.scope'], .export-item__where")];
      expect(order[0]).toBe(group);
      expect(order.at(-1)).toBe(ui("export.mdat.file"));
      expect(within(group).getAllByRole("radio").every((radio) => !(radio as HTMLButtonElement).disabled)).toBe(true);
      fireEvent.click(within(group).getByRole("radio", { name: chosen === "보정 결과만" ? "모두" : "보정 결과만" }));
      expect(act.setMdatScope).toHaveBeenLastCalledWith(chosen === "보정 결과만" ? "all" : "results");
      fireEvent.click(ui("export.run.before_after_mdat")!);
      expect(act.exportFiles).toHaveBeenCalledWith("before_after_mdat");
      unmount();
    }
    // The measurements go to their own folder.
    const { unmount } = renderWith(<Mounted view={exportMeasurementsOnly} act={actions()} />);
    expect(ui("export.kind.before_after_mdat")?.querySelector(".export-item__where")?.textContent).toBe("SECS_mdat_measurements_<저장 시각> 폴더");
    unmount();
    renderWith(<Mounted view={exportReady} act={actions()} />, { locale: "en" });
    show("before_after_mdat");
    // File names stay Korean in English: they are the names the service writes.
    expect(ui("export.mdat.file")?.textContent).toBe("SECS 원 측정+보정 결과 저지연 모드.mdat");
  });

  it("holds the REW mdat at 원 측정만 without a result and says why", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={exportNoResult} act={act} />);
    show("before_after_mdat");
    const group = ui("export.mdat.scope")!;
    expect(within(group).getByRole("radio", { checked: true }).textContent).toBe("원 측정만");
    expect((within(group).getByRole("radio", { name: "모두" }) as HTMLButtonElement).disabled).toBe(true);
    expect((within(group).getByRole("radio", { name: "보정 결과만" }) as HTMLButtonElement).disabled).toBe(true);
    const hint = document.getElementById(`${group.id}-hint`);
    expect(hint?.textContent).toBe("보정 결과가 없어 원 측정만 저장할 수 있습니다.");
    expect(group.getAttribute("aria-describedby")).toContain(hint!.id);
    expect(ui("export.mdat.file")?.textContent).toBe("SECS 원 측정.mdat");
    const run = ui("export.run.before_after_mdat") as HTMLButtonElement;
    expect(run.disabled).toBe(false);
    fireEvent.click(run);
    expect(act.exportFiles).toHaveBeenCalledWith("before_after_mdat");
    unmount();
    renderWith(<Mounted view={exportNothingMeasured} act={actions()} />);
    expect(document.getElementById(`${ui("export.mdat.scope")!.id}-hint`)?.textContent).toBe("저장할 측정이 없습니다.");
    expect((ui("export.run.before_after_mdat") as HTMLButtonElement).disabled).toBe(true);
  });

  it("lets an old cache stop the results, not the original measurements", () => {
    renderWith(<Mounted view={exportStaleCacheMeasurements} act={actions()} advanced />);
    expect(ui("export.kind.before_after_mdat.blocked")).toBeNull();
    expect((ui("export.run.before_after_mdat") as HTMLButtonElement).disabled).toBe(false);
    // The staged mdat still needs a new correction.
    expect(ui("export.kind.staged_mdat.blocked")).not.toBeNull();
    expect((ui("export.run.staged_mdat") as HTMLButtonElement).disabled).toBe(true);
  });

  it("runs one export at a time: the running panel is busy, every other button is off", () => {
    for (const running of ["filter_text", "equalizer_apo"] as const) {
      const { unmount } = renderWith(<Mounted view={{ ...exportRcOnly, exporting: running }} act={actions()} advanced />);
      for (const kind of EXPORT_KINDS) {
        show(kind);
        const button = ui(`export.run.${kind}`) as HTMLButtonElement;
        expect(button.disabled, kind).toBe(true);
        // Only the running export's panel button spins; every export stays disabled.
        expect(button.getAttribute("aria-busy"), kind).toBe(kind === running ? "true" : null);
      }
      expect(screen.getByText(running === "filter_text" ? "필터 값 텍스트 저장 중" : "Equalizer APO 설정 저장 중")).toBeTruthy();
      unmount();
    }
  });

  it("shows a panel's filled action busy only for its own export and disabled while any export runs", () => {
    const cases: [ExportView, boolean, boolean][] = [
      [exportExporting, false, true],
      [{ ...exportReady, exporting: "camilla_dsp" }, true, true],
      [exportReady, false, false],
    ];
    for (const [view, busy, disabled] of cases) {
      const act = actions();
      const { unmount } = renderWith(<Mounted view={view} act={act} />);
      show("camilla_dsp");
      const primary = ui("export.run.camilla_dsp") as HTMLButtonElement;
      expect([...document.querySelectorAll(".ui-button--primary")]).toEqual([primary]);
      expect(document.querySelector(".step-header .ui-button")).toBeNull();
      expect(primary.disabled).toBe(disabled);
      expect(primary.getAttribute("aria-busy")).toBe(busy ? "true" : null);
      fireEvent.click(primary);
      expect(act.exportFiles.mock.calls).toEqual(disabled ? [] : [["camilla_dsp"]]);
      expect(exportHeader(view, t)).not.toHaveProperty("primary");
      unmount();
    }
  });

  it("describes the panel's disabled filled action by the header's running export when another export runs", () => {
    renderWith(<Mounted view={exportExporting} act={actions()} />);
    show("camilla_dsp");
    const primary = ui("export.run.camilla_dsp") as HTMLButtonElement;
    expect(primary.disabled).toBe(true);
    expect(primary.getAttribute("aria-busy")).toBeNull();
    expect(primary.getAttribute("aria-describedby")).toBe("frame.header.export.status");
    expect(document.getElementById(primary.getAttribute("aria-describedby")!)?.textContent).toContain("REW mdat");
  });

  it("exports a stale result as upstream does, with a warning on the card and in the header", () => {
    const act = actions();
    renderWith(<Mounted view={exportStale} act={act} advanced />);
    expect(ui("frame.header.blocked")).toBeNull();
    expect(ui("export.result.stale")?.textContent).toContain("마지막으로 계산한 결과가 저장됩니다");
    expect(ui("export.status.stale")?.textContent).toBe("다시 계산 필요");
    expect(canExport(exportStale)).toBe(true);
    fireEvent.click(ui("export.run.camilla_dsp")!);
    show("before_after_mdat");
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
    expect(screen.getByText("파일을 내보내지 못했습니다.")).toBeTruthy();
    expect(screen.getByText(/Access is denied/)).toBeTruthy();
    // The row may be below the fold in a small window: the header says it too.
    expect(screen.getByText("내보내기 실패")).toBeTruthy();
    expect(screen.queryByText("내보냄")).toBeNull();
  });

  it("has no folder in the tutorial and says nothing was saved only on the outcome", () => {
    renderWith(<Mounted view={exportTutorial} act={actions()} />);
    expect(ui("export.folder")).toBeNull();
    expect(document.body.textContent).not.toContain("튜토리얼에서는 파일을 저장하지 않습니다");
    expect(ui("export.outcome.camilla_dsp")?.textContent).toContain("튜토리얼");
    expect(ui("export.folder.change")).toBeNull();
    for (const tab of EXPORT_TABS) {
      fireEvent.click(ui(`export.tab.${tab}`)!);
      expect(ui("export.folder"), tab).toBeNull();
      expect(ui("export.design_name"), tab).toBeNull();
    }
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
    expect(mimoInclusion({ ...exportReady, settings: { ...exportReady.settings, mimo_on: false }, results: { ...exportReady.results, mimo_export: "off" } })).toEqual({ included: false, reason: "off" });
    expect(mimoInclusion({ ...exportReady, positions: 2, results: { ...exportReady.results, mimo_export: "too_few_positions" } })).toEqual({ included: false, reason: "positions" });
    expect(mimoInclusion({ ...exportReady, results: { ...exportReady.results, mimo: null, mimo_export: "none" } })).toEqual({ included: false, reason: "none" });
    expect(mimoInclusion(exportStale)).toEqual({ included: false, reason: "stale" });
  });

  it("writes the RC notices as phrases with whole numbers", () => {
    const t = (key: Parameters<typeof translate>[1], params?: Record<string, string | number>) => translate("ko", key, params);
    const n = (value: number) => String(Math.round(value));
    expect(rcText(t, { code: "rc_limit", params: { hz: "310.4", auto: "true", schroeder_hz: "310.4" } }, n)).toBe("RC 상한 310 Hz(자동, 슈뢰더 310 Hz)");
    expect(rcText(t, { code: "rc_limit", params: { hz: "250", auto: "false" } }, n)).toBe("RC 상한 250 Hz(직접 입력)");
    expect(rcText(t, { code: "rc_room_volume", params: { volume_m3: "62.5" } }, n)).toBe("방 부피 63 m³로 정함");
    expect(rcText(t, { code: "rc_auto_cap", params: { hz: "500" } }, n)).toBe("자동 상한은 최대 500 Hz");
  });

  it("states the RC upper limit's basis and evidence as the 보정 screen's RC line does (ADR 0021)", () => {
    const t = (key: Parameters<typeof translate>[1], params?: Record<string, string | number>) => translate("ko", key, params);
    const n = (value: number, digits = 0) => value.toFixed(digits);
    const line = (params: Record<string, string>) => rcText(t, { code: "rc_limit", params: { auto: "true", ...params } }, n);
    expect(line({ hz: "308", schroeder_hz: "154", basis: "strict", factor: "2" })).toBe("RC 상한 308 Hz(엄격한 슈뢰더 154 Hz × 2)");
    expect(line({ hz: "200", schroeder_hz: "154", basis: "lenient", factor: "1", band_hz: "200", band_t60_s: "0.5" })).toBe("RC 상한 200 Hz(너그러운 슈뢰더, 200 Hz 대역 T60 0.50초)");
    expect(line({ hz: "287", schroeder_hz: "154", basis: "agreement", factor: "1", positions: "9" })).toBe("RC 상한 287 Hz(위치 9곳 일치도)");
    expect(line({ hz: "500", basis: "strict", factor: "1" })).toBe("RC 상한 500 Hz(자동)");
  });

  it("says beside the empty design name where the files go, and beside a name that the next export replaces its folder", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={exportRcOnly} act={act} />);
    const input = ui("export.design_name") as HTMLInputElement;
    expect(screen.getByLabelText("설계 이름")).toBe(input);
    expect(input.value).toBe("");
    expect(ui("export.design_name.hint")?.textContent).toBe("이름을 비워 두면 내보낼 때마다 저장 시각이 붙은 새 폴더를 만듭니다.");
    expect(input.getAttribute("aria-describedby")).toBe("export.design_name-hint");
    expect(input.getAttribute("aria-invalid")).toBeNull();
    // Unnamed, the rows keep their timestamped folders.
    fireEvent.change(input, { target: { value: "거실 영화" } });
    expect(act.setDesignName.mock.calls).toEqual([["거실 영화"]]);
    show("filter_text");
    expect(ui("export.kind.filter_text")?.querySelector(".export-item__where")?.textContent).toBe("SECS_text_RC_BA_mixed_<저장 시각> 폴더");
    unmount();
    // Named: the one sentence before any export, and each filter row names its folder.
    renderWith(<Mounted view={{ ...exportNamed, designName: "  거실 영화 " }} act={actions()} />);
    expect(ui("export.design_name.hint")?.textContent).toBe("같은 형식으로 거실 영화 설계를 다시 내보내면 기존 폴더를 새 폴더로 교체하고, 그 안에 따로 넣은 파일도 삭제합니다.");
    expect(ui("export.design_name.problem")).toBeNull();
    const where = (kind: ExportKind) => {
      show(kind);
      return ui(`export.kind.${kind}`)?.querySelector(".export-item__where")?.textContent;
    };
    expect(where("camilla_dsp")).toBe("SECS_camilla_dsp_거실 영화 폴더 · configs와 coeffs");
    expect(where("filter_text")).toBe("SECS_filter_text_거실 영화 폴더");
    expect(where("equalizer_apo")).toBe("SECS_equalizer_apo_거실 영화 폴더");
    expect(where("convolution")).toBe("SECS_convolution_거실 영화 폴더");
    // The REW mdat takes no name.
    expect(where("before_after_mdat")).toBe("SECS_mdat_RC_BA_mixed_<저장 시각> 폴더");
    for (const kind of FILTER_KINDS) {
      show(kind);
      expect((ui(`export.run.${kind}`) as HTMLButtonElement).disabled, kind).toBe(false);
    }
  });

  it("turns off every filter export and the panel's filled action while the design name cannot be a folder, and says why under the field", () => {
    const messages: Record<DesignNameProblem, string> = {
      length: "설계 이름은 1~64자로 입력해야 합니다.",
      control: "설계 이름에 줄바꿈이나 탭 같은 제어 문자를 넣을 수 없습니다.",
      character: "설계 이름에 \\ / : * ? \" < > | 기호를 넣을 수 없습니다.",
      trailing: "설계 이름은 마침표나 공백으로 끝날 수 없습니다.",
      reserved: "CON, PRN, AUX, NUL, COM1~COM9, LPT1~LPT9는 Windows 장치 이름이므로 설계 이름으로 쓸 수 없습니다. 뒤에 마침표와 글자를 붙여도 쓸 수 없습니다.",
    };
    for (const [problem, name] of Object.entries(DESIGN_NAME_SAMPLES) as [DesignNameProblem, string][]) {
      const act = actions();
      const { unmount } = renderWith(<Mounted view={{ ...exportRcOnly, designName: name }} act={act} advanced />);
      const message = ui("export.design_name.problem")!;
      expect(message.textContent, problem).toBe(messages[problem]);
      expect(message.id).toBe("export.design_name-error");
      // The field is invalid and described by its message, which replaces the hint.
      const input = ui("export.design_name") as HTMLInputElement;
      expect(input.getAttribute("aria-invalid"), problem).toBe("true");
      expect(input.getAttribute("aria-describedby"), problem).toBe("export.design_name-error");
      expect(ui("export.design_name.hint"), problem).toBeNull();
      // Each filter panel has its own name field and filled action, described by that field's message.
      for (const kind of FILTER_KINDS) {
        show(kind);
        const button = ui(`export.run.${kind}`) as HTMLButtonElement;
        expect(button.disabled, `${problem} ${kind}`).toBe(true);
        expect(button.getAttribute("aria-busy")).toBeNull();
        expect([...document.querySelectorAll(".ui-button--primary")]).toEqual([button]);
        expect(document.querySelector(".step-header .ui-button")).toBeNull();
        expect(document.getElementById(button.getAttribute("aria-describedby") ?? "")).toBe(ui("export.design_name.problem"));
        expect(ui("export.design_name.problem")?.textContent).toBe(messages[problem]);
        expect(ui("export.status.problem")?.textContent, problem).toBe("설계 이름 수정 필요");
        fireEvent.click(button);
      }
      // The REW mdat takes no name and stays on.
      for (const kind of ["before_after_mdat", "staged_mdat"] as const) {
        show(kind);
        expect((ui(`export.run.${kind}`) as HTMLButtonElement).disabled, `${problem} ${kind}`).toBe(false);
      }
      expect(act.exportFiles).not.toHaveBeenCalled();
      unmount();
    }
    // A running export keeps the pill on what runs and its own button busy.
    const running = renderWith(<Mounted view={{ ...exportRcOnly, designName: "CON", exporting: "camilla_dsp" }} act={actions()} />);
    expect(ui("export.run.camilla_dsp")?.getAttribute("aria-busy")).toBe("true");
    expect((ui("export.run.camilla_dsp") as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText("CamillaDSP 설정 저장 중")).toBeTruthy();
    running.unmount();
    // An empty field is no problem.
    const empty = renderWith(<Mounted view={{ ...exportRcOnly, designName: "" }} act={actions()} />);
    expect((ui("export.run.camilla_dsp") as HTMLButtonElement).disabled).toBe(false);
    empty.unmount();
    // English names the problem too.
    renderWith(<Mounted view={{ ...exportRcOnly, designName: "거실/영화" }} act={actions()} />, { locale: "en" });
    expect(ui("export.design_name.problem")?.textContent).toBe("Remove these characters from the design name: \\ / : * ? \" < > |");
    expect(ui("export.status.problem")?.textContent).toBe("Design name needs correction");
  });

  it("folds the CamillaDSP devices in its panel, closed, its summary reading the current choice", () => {
    const summaries: [ExportView, string, string][] = [
      [exportReady, "파일 자리표시", "File placeholders"],
      [exportDevicesWasapi, "입력 WASAPI CABLE Output (VB-Audio Virtual Cable), 출력 WASAPI Speakers (Tesira)", "Input WASAPI CABLE Output (VB-Audio Virtual Cable), output WASAPI Speakers (Tesira)"],
      [exportDevicesTyped, "입력 WASAPI 루프백 기본 장치, 출력 WASAPI Speakers (Tesira)", "Input WASAPI loopback Default device, output WASAPI Speakers (Tesira)"],
      [exportDevicesAsio, "입력 파일 자리표시, 출력 ASIO Tesira ASIO", "Input File placeholders, output ASIO Tesira ASIO"],
      [exportDevicesAsioUnnamed, "입력 ASIO 드라이버 이름 없음, 출력 ASIO Tesira ASIO", "Input ASIO driver not specified, output ASIO Tesira ASIO"],
      [{ ...exportReady, camillaDevices: { capture: { type: "asio", device: "Tesira ASIO" }, playback: { type: "asio", device: "Tesira ASIO" } } }, "ASIO Tesira ASIO", "ASIO Tesira ASIO"],
    ];
    for (const [view, ko, en] of summaries) {
      for (const [locale, text] of [["ko", ko], ["en", en]] as const) {
        const { unmount } = renderWith(<Mounted view={view} act={actions()} />, { locale });
        const toggle = ui("export.devices")!;
        expect(toggle.getAttribute("aria-expanded")).toBe("false");
        expect(ui("export.devices.summary")?.textContent, `${locale} ${text}`).toBe(text);
        // The summary is part of the toggle's name; the fields are not drawn while closed.
        expect(toggle.textContent).toBe(`${locale === "ko" ? "입출력 장치" : "Input and output devices"}${text}`);
        expect(ui("export.devices.capture")).toBeNull();
        unmount();
      }
    }
    // Opening shows both sides and drops the summary, which would repeat them.
    renderWith(<Mounted view={exportReady} act={actions()} />);
    fireEvent.click(ui("export.devices")!);
    expect(ui("export.devices")?.getAttribute("aria-expanded")).toBe("true");
    expect(ui("export.devices.summary")).toBeNull();
    expect(screen.getByRole("group", { name: "입력 장치" })).toBe(ui("export.devices.capture"));
    expect(screen.getByRole("group", { name: "출력 장치" })).toBe(ui("export.devices.playback"));
    const panel = ui("export.kind.camilla_dsp")!.querySelector(".export-devices")!;
    expect([...panel.querySelectorAll("[role='radio'][aria-checked='true']")].map((radio) => radio.textContent)).toEqual(["파일 자리표시", "파일 자리표시"]);
    expect(ui("export.devices.asio_note")).toBeNull();
  });

  it("reads the device list again each time the disclosure opens, not when it closes", () => {
    const act = actions();
    renderWith(<Mounted view={exportReady} act={act} />);
    fireEvent.click(ui("export.devices")!);
    expect(act.refreshDevices).toHaveBeenCalledTimes(1);
    fireEvent.click(ui("export.devices")!);
    expect(act.refreshDevices).toHaveBeenCalledTimes(1);
    fireEvent.click(ui("export.devices")!);
    expect(act.refreshDevices).toHaveBeenCalledTimes(2);
  });

  it("says in the panel why the service refused a design name, as the field does", () => {
    const refused: ExportView = {
      ...exportReady,
      runs: { convolution: { status: "failed", error: { code: "invalid_request", detail: "design_name_reserved", retryable: false, params: { reason: "design_name_reserved" } } } },
    };
    renderWith(<Mounted view={refused} act={actions()} />);
    show("convolution");
    const outcome = ui("export.outcome.convolution")!;
    expect(outcome.textContent).toContain("Windows 장치 이름");
    expect(outcome.textContent).not.toContain("다시 시작");
  });

  it("chooses file placeholders, WASAPI or ASIO per side, starting WASAPI at the default device, shared, F32", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={exportReady} act={act} devices />);
    fireEvent.click(ui("export.devices.capture.backend.wasapi")!);
    fireEvent.click(ui("export.devices.playback.backend.asio")!);
    expect(act.setCamillaDevices.mock.calls).toEqual([
      [{ capture: { type: "wasapi", device: null, format: "F32", exclusive: false, loopback: false }, playback: { type: "file" } }],
      [{ capture: { type: "file" }, playback: { type: "asio", device: "" } }],
    ]);
    unmount();
    // With the list: capture lists inputs, playback outputs, each with its channels, after the default.
    const wasapi = actions();
    const { unmount: unmountWasapi } = renderWith(<Mounted view={exportDevicesWasapi} act={wasapi} devices />);
    const options = (id: string) => [...(ui(id) as HTMLSelectElement).options].map((option) => option.textContent);
    expect(options("export.devices.capture.device")).toEqual(["기본 장치", "CABLE Output (VB-Audio Virtual Cable) · 8채널", "Line (Tesira) · 8채널", "UMIK-1 · 2채널"]);
    expect(options("export.devices.playback.device")).toEqual(["기본 장치", "Speakers (Tesira) · 8채널", "CABLE Input (VB-Audio Virtual Cable) · 8채널"]);
    expect((ui("export.devices.playback.device") as HTMLSelectElement).value).toBe("Speakers (Tesira)");
    fireEvent.change(ui("export.devices.playback.device")!, { target: { value: "" } });
    fireEvent.click(ui("export.devices.playback.format.F32")!);
    fireEvent.click(ui("export.devices.capture.exclusive")!);
    expect(wasapi.setCamillaDevices.mock.calls.map(([devices]) => devices)).toEqual([
      { ...exportDevicesWasapi.camillaDevices, playback: { type: "wasapi", device: null, format: "S24", exclusive: true } },
      { ...exportDevicesWasapi.camillaDevices, playback: { type: "wasapi", device: "Speakers (Tesira)", format: "F32", exclusive: true } },
      { ...exportDevicesWasapi.camillaDevices, capture: { type: "wasapi", device: "CABLE Output (VB-Audio Virtual Cable)", format: "F32", exclusive: true, loopback: false } },
    ]);
    // Loopback records an output in shared mode: exclusive goes off, and an input device leaves the choice.
    wasapi.setCamillaDevices.mockClear();
    fireEvent.click(ui("export.devices.capture.loopback")!);
    expect(wasapi.setCamillaDevices.mock.calls[0]?.[0].capture).toEqual({ type: "wasapi", device: null, format: "F32", exclusive: false, loopback: true });
    // Playback has no loopback; no ASIO note without ASIO.
    expect(ui("export.devices.playback.loopback")).toBeNull();
    expect(ui("export.devices.asio_note")).toBeNull();
    unmountWasapi();
    // A stored name the list no longer has stays chosen and says so.
    const { unmount: unmountMissing } = renderWith(
      <Mounted view={{ ...exportDevicesWasapi, camillaDevices: { ...exportDevicesWasapi.camillaDevices, playback: { type: "wasapi", device: "AVR (HDMI)", format: "F32", exclusive: false } } }} act={actions()} devices />,
    );
    expect(options("export.devices.playback.device").at(-1)).toBe("AVR (HDMI) · 목록에 없음");
    expect((ui("export.devices.playback.device") as HTMLSelectElement).value).toBe("AVR (HDMI)");
    unmountMissing();
  });

  it("lets the user type a WASAPI name when there is no device list, and holds exclusive off under loopback", () => {
    const act = actions();
    renderWith(<Mounted view={exportDevicesTyped} act={act} devices />);
    const playback = ui("export.devices.playback.device") as HTMLInputElement;
    expect(playback.tagName).toBe("INPUT");
    expect(playback.value).toBe("Speakers (Tesira)");
    expect(screen.getAllByText("Windows 소리 설정의 장치 이름과 같아야 합니다. 비워 두면 기본 장치를 사용합니다.")).toHaveLength(2);
    const capture = ui("export.devices.capture.device") as HTMLInputElement;
    expect(capture.value).toBe("");
    expect(capture.placeholder).toBe("기본 장치");
    fireEvent.change(capture, { target: { value: "Speakers (Tesira)" } });
    fireEvent.change(playback, { target: { value: "   " } });
    expect(act.setCamillaDevices.mock.calls.map(([devices]) => devices)).toEqual([
      { ...exportDevicesTyped.camillaDevices, capture: { type: "wasapi", device: "Speakers (Tesira)", format: "F32", exclusive: false, loopback: true } },
      // Blank means the default device.
      { ...exportDevicesTyped.camillaDevices, playback: { type: "wasapi", device: null, format: "F32", exclusive: false } },
    ]);
    const exclusive = ui("export.devices.capture.exclusive") as HTMLButtonElement;
    expect(exclusive.disabled).toBe(true);
    expect(exclusive.getAttribute("aria-checked")).toBe("false");
    expect(document.getElementById(exclusive.getAttribute("aria-describedby") ?? "")?.textContent).toBe("루프백 녹음은 공유 모드에서만 가능합니다.");
    expect((ui("export.devices.playback.exclusive") as HTMLButtonElement).disabled).toBe(false);
  });

  it("says when ASIO is chosen that released CamillaDSP builds lack it, and holds CamillaDSP off until each ASIO side names its driver", () => {
    const { unmount } = renderWith(<Mounted view={exportDevicesAsio} act={actions()} devices />);
    expect(ui("export.devices.asio_note")?.textContent).toBe("ASIO는 CamillaDSP의 ASIO 빌드에서만 쓸 수 있습니다. 일반 Windows 빌드에는 ASIO가 없습니다.");
    expect((ui("export.devices.playback.asio") as HTMLInputElement).value).toBe("Tesira ASIO");
    expect((ui("export.run.camilla_dsp") as HTMLButtonElement).disabled).toBe(false);
    expect(ui("export.status.problem")).toBeNull();
    unmount();
    const act = actions();
    renderWith(<Mounted view={exportDevicesAsioUnnamed} act={act} advanced devices />);
    expect(document.querySelectorAll("[data-ui='export.devices.asio_note']")).toHaveLength(1);
    const driver = ui("export.devices.capture.asio") as HTMLInputElement;
    expect(driver.getAttribute("aria-invalid")).toBe("true");
    expect(document.getElementById(driver.getAttribute("aria-describedby") ?? "")?.textContent).toBe("ASIO 드라이버 이름을 입력해야 합니다.");
    const primary = ui("export.run.camilla_dsp") as HTMLButtonElement;
    expect(primary.disabled).toBe(true);
    expect(ui("export.status.problem")?.textContent).toBe("ASIO 드라이버 이름 필요");
    fireEvent.click(primary);
    expect(act.exportFiles).not.toHaveBeenCalled();
    fireEvent.change(driver, { target: { value: "Tesira ASIO" } });
    expect(act.setCamillaDevices).toHaveBeenLastCalledWith({ capture: { type: "asio", device: "Tesira ASIO" }, playback: { type: "asio", device: "Tesira ASIO" } });
    // Only CamillaDSP takes devices: the other exports stay on.
    for (const kind of EXPORT_KINDS.filter((kind) => kind !== "camilla_dsp" && kind !== "filter_text")) {
      show(kind);
      expect((ui(`export.run.${kind}`) as HTMLButtonElement).disabled, kind).toBe(false);
    }
  });

  it("brings back what a side had when the user tries another backend and returns", () => {
    // A controlled host: the screen gets back what it sets.
    function Host() {
      const [view, setView] = useState<ExportView>(exportDevicesWasapi);
      const act = { ...actions(), setCamillaDevices: (camillaDevices: ExportView["camillaDevices"]) => setView((current) => ({ ...current, camillaDevices })) };
      return <Mounted view={view} act={act} devices />;
    }
    renderWith(<Host />);
    fireEvent.click(ui("export.devices.playback.backend.asio")!);
    fireEvent.change(ui("export.devices.playback.asio")!, { target: { value: "Tesira ASIO" } });
    fireEvent.click(ui("export.devices.playback.backend.file")!);
    fireEvent.click(ui("export.devices.playback.backend.wasapi")!);
    expect((ui("export.devices.playback.device") as HTMLSelectElement).value).toBe("Speakers (Tesira)");
    expect(ui("export.devices.playback.format.S24")?.getAttribute("aria-checked")).toBe("true");
    fireEvent.click(ui("export.devices.playback.backend.asio")!);
    expect((ui("export.devices.playback.asio") as HTMLInputElement).value).toBe("Tesira ASIO");
  });

  it("names the rate each filter panel writes when it is not 48 kHz; the REW mdat says nothing (ADR 0017)", () => {
    const { unmount } = renderWith(<Mounted view={exportReady} act={actions()} advanced />);
    for (const kind of EXPORT_KINDS) {
      show(kind);
      expect(document.querySelectorAll("[data-ui$='.rate']"), kind).toHaveLength(0);
    }
    unmount();
    const rows = (view: ExportView) => {
      const { unmount } = renderWith(<Mounted view={view} act={actions()} advanced />);
      const text = EXPORT_KINDS.map((kind) => {
        show(kind);
        return ui(`export.kind.${kind}.rate`)?.textContent ?? null;
      });
      unmount();
      return text;
    };
    const filter = (kind: string) => FILTER_KINDS.some((candidate) => candidate === kind);
    const fixed = translate("ko", "export.rate.fixed", { rate: "96 kHz" });
    expect(fixed).toBe("출력 레이트 96 kHz");
    expect(rows(exportRate96k)).toEqual(EXPORT_KINDS.map((kind) => (filter(kind) ? fixed : null)));
    const auto = translate("ko", "export.rate.auto");
    expect(rows({ ...exportReady, outputRate: "auto" })).toEqual(EXPORT_KINDS.map((kind) => (filter(kind) ? auto : null)));
    expect(rows({ ...exportReady, outputRate: { fixed: 44100 } })[0]).toBe(translate("ko", "export.rate.fixed", { rate: "44.1 kHz" }));
    // Before the DSP arrives the rows say nothing.
    expect(rows({ ...exportReady, outputRate: null }).every((text) => text === null)).toBe(true);
  });

  it("folds the convolution IR's extra rates under its panel, closed, and checks the rates other than the written one", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={exportRate96k} act={act} />);
    show("convolution");
    const toggle = ui("export.extra_rates")!;
    expect(toggle.closest("[data-ui='export.kind.convolution']")).not.toBeNull();
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(ui("export.extra_rates.summary")?.textContent).toBe(translate("ko", "export.extra_rates.none"));
    expect(ui("export.extra_rates.44100")).toBeNull();
    fireEvent.click(toggle);
    const boxes = [...document.querySelectorAll<HTMLInputElement>("input[type='checkbox'][data-ui^='export.extra_rates.']")];
    expect(boxes.map((box) => box.getAttribute("data-ui"))).toEqual([44100, 48000, 88200, 176400, 192000].map((rate) => `export.extra_rates.${rate}`));
    expect(boxes.every((box) => !box.checked)).toBe(true);
    expect(boxes[0]!.closest("label")?.textContent).toBe("44.1 kHz");
    fireEvent.click(ui("export.extra_rates.44100")!);
    expect(act.setExtraRates).toHaveBeenLastCalledWith([44100]);
    unmount();

    const chosen = actions();
    const view = { ...exportExtraRates, extraRates: [96000, ...exportExtraRates.extraRates] };
    renderWith(<Mounted view={view} act={chosen} />);
    // The written rate is never an extra one: 96 kHz stays chosen behind it for when the DSP's rate changes.
    expect(ui("export.extra_rates.summary")?.textContent).toBe("44.1 kHz, 192 kHz");
    fireEvent.click(ui("export.extra_rates")!);
    expect((ui("export.extra_rates.44100") as HTMLInputElement).checked).toBe(true);
    expect((ui("export.extra_rates.192000") as HTMLInputElement).checked).toBe(true);
    expect((ui("export.extra_rates.48000") as HTMLInputElement).checked).toBe(false);
    fireEvent.click(ui("export.extra_rates.192000")!);
    expect(chosen.setExtraRates).toHaveBeenLastCalledWith([96000, 44100]);
    fireEvent.click(ui("export.extra_rates.48000")!);
    expect(chosen.setExtraRates).toHaveBeenLastCalledWith([96000, 44100, 192000, 48000]);
  });

  it("checks the extra rates against 48 kHz under 자동, the rate it falls back to", () => {
    renderWith(<Mounted view={{ ...exportReady, outputRate: "auto" }} act={actions()} rates />);
    show("convolution");
    expect(ui("export.extra_rates.48000")).toBeNull();
    expect(ui("export.extra_rates.96000")).not.toBeNull();
  });

  it("warns on the outcome when 자동 fell back to 48 kHz, with the cause, what to do and the way to 시스템", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={exportRateFallback} act={act} />);
    const warning = ui("export.outcome.camilla_dsp.rate_fallback")!;
    expect(warning.closest("[data-ui='export.outcome.camilla_dsp']")).not.toBeNull();
    expect(warning.textContent).toContain(translate("ko", "export.rate.fallback.unsupported", { rate: "32 kHz" }));
    expect(warning.textContent).toContain(translate("ko", "export.rate.fallback.next"));
    expect(ui("export.outcome.camilla_dsp.rate")).toBeNull();
    fireEvent.click(ui("export.outcome.camilla_dsp.go_system")!);
    expect(act.goToSystem).toHaveBeenCalledOnce();
    unmount();

    const run = exportRateFallback.runs.camilla_dsp!;
    if (run.status !== "done") throw new Error("the fixture's run is done");
    const unread = renderWith(<Mounted view={{ ...exportRateFallback, runs: { camilla_dsp: { ...run, notes: [{ code: "output_rate_fallback", params: {} }] } } }} act={actions()} />);
    expect(ui("export.outcome.camilla_dsp.rate_fallback")?.textContent).toContain(translate("ko", "export.rate.fallback.unread"));
    unread.unmount();

    // 자동 that read the device: the outcome names the rate it wrote, without a warning.
    const read = renderWith(<Mounted view={{ ...exportRateFallback, runs: { camilla_dsp: { ...run, rate: 44100, notes: [] } } }} act={actions()} />);
    expect(ui("export.outcome.camilla_dsp.rate_fallback")).toBeNull();
    expect(ui("export.outcome.camilla_dsp.rate")?.textContent).toBe(translate("ko", "export.rate.written", { rate: "44.1 kHz" }));
    read.unmount();

    // A fixed rate says nothing more after the export: the row already said it.
    renderWith(<Mounted view={{ ...exportRate96k, runs: { camilla_dsp: { ...run, rate: 96000, notes: [] } } }} act={actions()} />);
    expect(ui("export.outcome.camilla_dsp.rate")).toBeNull();
    expect(ui("export.outcome.camilla_dsp.rate_fallback")).toBeNull();
  });

  it("computes tabState for every export kind without marking another destination", () => {
    for (const kind of EXPORT_KINDS) {
      for (const tab of EXPORT_TABS) {
        const own = tab === tabOf(kind);
        expect(tabState({ ...exportRcOnly, exporting: kind }, tab), `${kind} ${tab} busy`).toBe(own ? "busy" : "idle");
        expect(tabState({ ...exportRcOnly, runs: { [kind]: exportDone.runs.camilla_dsp! } }, tab), `${kind} ${tab} done`).toBe(own ? "done" : "idle");
        expect(tabState({ ...exportRcOnly, runs: { [kind]: exportFailed.runs.camilla_dsp! } }, tab), `${kind} ${tab} failed`).toBe(own ? "failed" : "idle");
        expect(tabState({ ...exportRcOnly, runs: { [kind]: { status: "tutorial" } } }, tab), `${kind} ${tab} tutorial`).toBe("idle");
      }
    }
    expect(tabState(exportReady, "filter_text")).toBe("blocked");
    expect(tabState(exportMimoNoSubs, "filter_text")).toBe("blocked");
    expect(tabState(exportRcOnly, "filter_text")).toBe("idle");
    expect(tabState(exportStaleCache, "rew")).toBe("blocked");
    expect(tabState(exportStaleCacheMeasurements, "rew")).toBe("idle");
    expect(tabState(exportStale, "rew")).toBe("idle");
  });

  it("counts apply outcomes only beyond 파일만 and keeps the other tabs unchanged", () => {
    const runs: [ApplyRun, TabState][] = [
      [applyCamillaApplied.apply.runs.camilla!, "done"],
      [applyCamillaRolledBack.apply.runs.camilla!, "failed"],
      [applyEqapoDenied.apply.runs.eqapo!, "failed"],
      [{ status: "reverted" }, "idle"],
    ];
    for (const [target, tab] of [["camilla", "camilla_dsp"], ["eqapo", "equalizer_apo"]] as const) {
      for (const level of ["files", "apply", "manage"] as const) {
        for (const [run, expected] of runs) {
          const view: ExportView = {
            ...exportRcOnly,
            apply: {
              ...exportRcOnly.apply,
              settings: { ...exportRcOnly.apply.settings, [target]: { ...exportRcOnly.apply.settings[target], level } },
              runs: { [target]: run },
            },
          };
          for (const destination of EXPORT_TABS) {
            expect(tabState(view, destination), `${target} ${level} ${run.status} ${destination}`).toBe(destination === tab && level !== "files" ? expected : "idle");
          }
          expect(tabState({ ...view, tutorial: true }, tab)).toBe("idle");
          if (target === "eqapo") expect(tabState({ ...view, apply: { ...view.apply, windows: false } }, tab)).toBe("idle");
        }
      }
    }
  });

  it("keeps tabState priority busy, failed, done, then blocked across exports and applies", () => {
    const done = exportDone.runs.before_after_mdat!;
    const failed = exportFailed.runs.camilla_dsp!;
    const results: ExportView = { ...exportStaleCache, runs: { before_after_mdat: done } };
    const failure: ExportView = { ...results, runs: { ...results.runs, staged_mdat: failed } };
    expect(tabState(exportStaleCache, "rew")).toBe("blocked");
    expect(tabState(results, "rew")).toBe("done");
    expect(tabState(failure, "rew")).toBe("failed");
    expect(tabState({ ...failure, exporting: "before_after_mdat" }, "rew")).toBe("busy");
    expect(tabState({ ...exportReady, runs: { filter_text: done } }, "filter_text")).toBe("done");
    expect(tabState({ ...exportReady, runs: { filter_text: failed } }, "filter_text")).toBe("failed");
    const rolled: ExportView = { ...applyCamillaRolledBack, runs: { camilla_dsp: done } };
    const exportFailedAfterApply: ExportView = { ...applyCamillaApplied, runs: { camilla_dsp: failed } };
    expect(tabState(rolled, "camilla_dsp")).toBe("failed");
    expect(tabState(exportFailedAfterApply, "camilla_dsp")).toBe("failed");
    expect(tabState({ ...rolled, exporting: "camilla_dsp" }, "camilla_dsp")).toBe("busy");
    expect(tabState({ ...exportFailedAfterApply, apply: { ...exportFailedAfterApply.apply, applying: "camilla" } }, "camilla_dsp")).toBe("busy");
  });

  it("marks a running, saved or failed tab with its sign and hidden words in both languages, writes a blocked tab's state as its caption, and leaves idle unmarked", () => {
    const cases: [ExportView, ExportTab, TabState][] = [
      [exportRcOnly, "camilla_dsp", "idle"],
      [exportExporting, "rew", "busy"],
      [applyCamillaApplying, "camilla_dsp", "busy"],
      [{ ...applyEqapoAbsent, apply: { ...applyEqapoAbsent.apply, applying: "eqapo" } }, "equalizer_apo", "busy"],
      [exportFailed, "camilla_dsp", "failed"],
      [applyCamillaRolledBack, "camilla_dsp", "failed"],
      [applyEqapoDenied, "equalizer_apo", "failed"],
      [exportDone, "camilla_dsp", "done"],
      [applyCamillaApplied, "camilla_dsp", "done"],
      [exportConvolutionDone, "convolution", "done"],
      [exportReady, "filter_text", "blocked"],
      [exportMimoNoSubs, "filter_text", "blocked"],
      [exportStaleCache, "rew", "blocked"],
      [exportStaleCacheMeasurements, "rew", "idle"],
    ];
    const words = {
      ko: { busy: "진행 중", failed: "실패", done: "완료", blocked: "이 결과에 사용 불가" },
      en: { busy: "In progress", failed: "Failed", done: "Done", blocked: "Not for this result" },
    };
    for (const locale of ["ko", "en"] as const) {
      for (const [view, tab, state] of cases) {
        const { unmount } = renderWith(<Mounted view={view} act={actions()} />, { locale });
        const button = ui(`export.tab.${tab}`)!;
        expect(tabState(view, tab)).toBe(state);
        expect(button.getAttribute("data-state")).toBe(state);
        // No dot anywhere: a dot's colour alone told a reader nothing (2026-11-11 review).
        expect(button.querySelector(".status-dot")).toBeNull();
        if (state === "idle") {
          expect(button.querySelector(".export-tab__mark")).toBeNull();
          expect(button.querySelector(".visually-hidden")).toBeNull();
          expect(button.querySelector(".export-tab__use")?.textContent).toBe(translate(locale, `export.tab.${tab}.use`));
        } else if (state === "blocked") {
          // The caption gives way to the state, in words a sighted reader sees too.
          expect(button.querySelector(".export-tab__mark")).toBeNull();
          expect(button.querySelector(".visually-hidden")).toBeNull();
          expect(button.querySelector(".export-tab__use")?.textContent).toBe(words[locale].blocked);
          expect(screen.getByRole("tab", { name: `${translate(locale, `export.tab.${tab}`)} ${words[locale].blocked}` })).toBe(button);
        } else {
          const marks = button.querySelectorAll(".export-tab__mark");
          expect(marks).toHaveLength(1);
          expect([...marks[0]!.classList]).toEqual(["export-tab__mark", `export-tab__mark--${state}`]);
          expect(marks[0]!.querySelectorAll("svg")).toHaveLength(1);
          expect(marks[0]!.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
          expect(button.querySelector(".visually-hidden")?.textContent).toBe(words[locale][state]);
          expect(button.querySelector(".export-tab__use")?.textContent).toBe(translate(locale, `export.tab.${tab}.use`));
          expect(screen.getByRole("tab", { name: `${translate(locale, `export.tab.${tab}`)} ${words[locale][state]} ${translate(locale, `export.tab.${tab}.use`)}` })).toBe(button);
        }
        unmount();
      }
    }
  });

  it("keeps a tab busy while its apply is in flight even after its target returns to 파일만, and drops the outcome once it ends there", () => {
    for (const [base, target, tab] of [[applyCamillaApplying, "camilla", "camilla_dsp"], [applyEqapoAbsent, "eqapo", "equalizer_apo"]] as const) {
      const files = { ...base.apply.settings, [target]: { ...base.apply.settings[target], level: "files" } };
      // The job still runs: the tab says so whatever the level was turned to meanwhile.
      const running: ExportView = { ...base, apply: { ...base.apply, applying: target, settings: files } };
      const { unmount } = renderWith(<Mounted view={running} act={actions()} />);
      expect.soft(tabState(running, tab), target).toBe("busy");
      expect.soft(ui(`export.tab.${tab}`)?.getAttribute("data-state"), target).toBe("busy");
      expect.soft(ui(`export.tab.${tab}`)?.querySelector(".export-tab__mark--busy"), target).not.toBeNull();
      unmount();
      // Ended at 파일만: the panel shows no apply outcome there, so the tab shows none either.
      const ended: ExportView = { ...base, apply: { ...base.apply, applying: null, settings: files, runs: { [target]: { status: "applied", designId: "0c1f9e7a", folder: "C:/applied" } } } };
      const { unmount: unmountEnded } = renderWith(<Mounted view={ended} act={actions()} />);
      expect.soft(tabState(ended, tab), target).toBe("idle");
      expect.soft(ui(`export.tab.${tab}`)?.getAttribute("data-state"), target).toBe("idle");
      expect.soft(ui(`export.tab.${tab}`)?.querySelector(".export-tab__mark"), target).toBeNull();
      expect.soft(ui(`export.apply.${target}.outcome`), target).toBeNull();
      unmountEnded();
    }
  });

  it("defaults to CamillaDSP unless Equalizer APO alone is beyond 파일만, including tutorial and non-Windows settings", () => {
    expect(defaultExportTab(exportReady)).toBe("camilla_dsp");
    expect(defaultExportTab(applyEqapoAbsent)).toBe("equalizer_apo");
    expect(defaultExportTab(manageEqapoRepaired)).toBe("equalizer_apo");
    for (const camilla of ["files", "apply", "manage"] as const) {
      for (const eqapo of ["files", "apply", "manage"] as const) {
        const view: ExportView = {
          ...exportReady,
          apply: { ...exportReady.apply, settings: { camilla: { ...exportReady.apply.settings.camilla, level: camilla }, eqapo: { level: eqapo } } },
        };
        expect(defaultExportTab(view), `${camilla} ${eqapo}`).toBe(camilla === "files" && eqapo !== "files" ? "equalizer_apo" : "camilla_dsp");
        expect(defaultExportTab({ ...view, tutorial: true })).toBe("camilla_dsp");
        expect(defaultExportTab({ ...view, apply: { ...view.apply, windows: false } })).toBe("camilla_dsp");
      }
    }
  });

  it("follows the selected tab with the header's design-name and ASIO problem pills", () => {
    const { unmount } = renderWith(<Mounted view={{ ...exportRcOnly, designName: "CON" }} act={actions()} />);
    for (const tab of EXPORT_TABS) {
      fireEvent.click(ui(`export.tab.${tab}`)!);
      expect(ui("export.status.problem")?.textContent ?? null, tab).toBe(tab === "rew" ? null : "설계 이름 수정 필요");
    }
    unmount();
    for (const side of ["capture", "playback"] as const) {
      const { unmount: unmountAsio } = renderWith(<Mounted view={{ ...exportRcOnly, camillaDevices: { ...exportRcOnly.camillaDevices, [side]: { type: "asio", device: "" } } }} act={actions()} />);
      for (const tab of EXPORT_TABS) {
        fireEvent.click(ui(`export.tab.${tab}`)!);
        expect(ui("export.status.problem")?.textContent ?? null, `${side} ${tab}`).toBe(tab === "camilla_dsp" ? "ASIO 드라이버 이름 필요" : null);
        if (tab === "camilla_dsp") {
          const button = ui("export.run.camilla_dsp") as HTMLButtonElement;
          expect(button.disabled).toBe(true);
          expect(button.getAttribute("aria-describedby")).toBe("frame.header.export.status");
          expect(document.getElementById(button.getAttribute("aria-describedby")!)?.textContent).toBe("ASIO 드라이버 이름 필요");
        }
      }
      unmountAsio();
    }
  });

  it("puts export outcomes before apply choices and fields, and the folder before what lands in it", () => {
    for (const kind of EXPORT_KINDS) {
      const { unmount } = renderWith(<Mounted view={{ ...exportRcOnly, tab: tabOf(kind), runs: { [kind]: exportDone.runs.camilla_dsp! } }} act={actions()} advanced />);
      const section = ui(`export.kind.${kind}`)!;
      const outcome = ui(`export.outcome.${kind}`)!;
      const fields = section.querySelectorAll("[data-ui$='.panel'], [data-ui='export.devices'], [data-ui='export.extra_rates'], [data-ui='export.mdat.scope'], [data-ui='export.design_name.field'], [data-ui='export.folder']");
      expect(fields.length, kind).toBe(kind === "staged_mdat" ? 0 : kind === "camilla_dsp" ? 4 : kind === "filter_text" || kind === "before_after_mdat" ? 2 : 3);
      for (const field of fields) {
        expect(outcome.compareDocumentPosition(field) & Node.DOCUMENT_POSITION_FOLLOWING, `${kind} ${field.getAttribute("data-ui")}`).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
      }
      const where = section.querySelector(".export-item__where")!;
      expect(outcome.compareDocumentPosition(where) & Node.DOCUMENT_POSITION_FOLLOWING).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
      if (kind !== "staged_mdat") {
        expect(ui("export.folder")!.compareDocumentPosition(where) & Node.DOCUMENT_POSITION_FOLLOWING, kind).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
      }
      unmount();
    }
  });

  it("puts a format's blocked sentence before its outcome and fields", () => {
    for (const [base, kind] of [[exportReady, "filter_text"], [exportStaleCache, "before_after_mdat"], [exportStaleCache, "staged_mdat"]] as const) {
      const { unmount } = renderWith(<Mounted view={{ ...base, tab: tabOf(kind), runs: { [kind]: exportFailed.runs.camilla_dsp! } }} act={actions()} advanced />);
      const blocked = ui(`export.kind.${kind}.blocked`)!;
      const outcome = ui(`export.outcome.${kind}`)!;
      expect(blocked.compareDocumentPosition(outcome) & Node.DOCUMENT_POSITION_FOLLOWING).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
      const fields = ui(`export.kind.${kind}`)!.querySelectorAll("[data-ui='export.mdat.scope'], [data-ui='export.design_name.field'], [data-ui='export.folder']");
      expect(fields).toHaveLength(kind === "staged_mdat" ? 0 : 2);
      for (const field of fields) {
        expect(blocked.compareDocumentPosition(field) & Node.DOCUMENT_POSITION_FOLLOWING).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
        expect(outcome.compareDocumentPosition(field) & Node.DOCUMENT_POSITION_FOLLOWING).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
      }
      unmount();
    }
  });

  it("explains the default and stored folder origins in the name's help rather than in the folder line", () => {
    for (const [view, text] of [[exportReady, "기본 위치: 프로젝트 옆 SECS_export 폴더 아래 날짜와 프로젝트 이름의 폴더"], [exportDone, "이 프로젝트에 정한 위치"]] as const) {
      const { unmount } = renderWith(<Mounted view={view} act={actions()} />);
      const row = ui("export.folder")!;
      expect(row.textContent).not.toContain(text);
      expect(ui("export.folder.path")?.textContent).toBe(view.folder!.folder);
      expect(ui("export.folder.help")?.closest(".export-folder__name")).not.toBeNull();
      fireEvent.click(ui("export.folder.help")!);
      expect(screen.getByRole("tooltip").textContent).toBe(text);
      expect(row.textContent).not.toContain(text);
      unmount();
    }
  });

  it("draws every id the reach table cites in at least one state", () => {
    const seen = new Set<string>();
    const collect = () => {
      for (const node of document.querySelectorAll("[data-ui]")) seen.add(node.getAttribute("data-ui") ?? "");
      const ids = [...document.querySelectorAll("[id]")].map((node) => node.id);
      expect(ids.filter((id, index) => ids.indexOf(id) !== index)).toEqual([]);
      const screenIds = [...document.querySelectorAll("[data-ui='export'] [data-ui]")].map((node) => node.getAttribute("data-ui"));
      expect(screenIds.filter((id, index) => screenIds.indexOf(id) !== index)).toEqual([]);
    };
    for (const view of Object.values(STATES)) {
      const { unmount } = renderWith(<Mounted view={view} act={actions()} advanced />);
      for (const tab of EXPORT_TABS) {
        fireEvent.click(ui(`export.tab.${tab}`)!);
        collect();
      }
      unmount();
    }
    // The frame's menus open and the tray with an export running (§10 cites them).
    const jobs = trayState(ledgerReducer(initialLedger, { type: "event", at: 0, event: { event: "started", job: 1, kind: "export" } }));
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
