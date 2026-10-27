// @vitest-environment happy-dom
import { fireEvent, screen } from "@testing-library/react";
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
  exportFrame,
  exportComputingFirst,
  exportDone,
  exportEqapoDone,
  exportExporting,
  exportFailed,
  exportMeasurementsDone,
  exportMeasurementsOnly,
  exportMimoNoSubs,
  exportNoMimo,
  exportNoResult,
  exportNothingMeasured,
  exportRcOnly,
  exportReady,
  exportRecalibrating,
  exportResultsOnly,
  exportStale,
  exportStaleCache,
  exportStaleCacheMeasurements,
  exportTutorial,
} from "./fixtures";
import { canExport, exportStepState, mimoInclusion, type ExportActions, type ExportView } from "./types";
import { translate } from "../../i18n";

/** Every id docs/M5-reach.md §9 and §10 cite for this screen and the frame around it, and the filter formats' rows (P47). */
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
};

function actions() {
  return {
    exportFiles: vi.fn<ExportActions["exportFiles"]>(),
    setMdatScope: vi.fn<ExportActions["setMdatScope"]>(),
    changeFolder: vi.fn<ExportActions["changeFolder"]>(),
    openFolder: vi.fn<ExportActions["openFolder"]>(),
    goToCalibrate: vi.fn<ExportActions["goToCalibrate"]>(),
    recalibrate: vi.fn<ExportActions["recalibrate"]>(),
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
    renderWith(<Mounted view={exportRcOnly} act={act} advanced />);
    for (const kind of ["camilla_dsp", "filter_text", "equalizer_apo", "before_after_mdat", "staged_mdat"]) fireEvent.click(ui(`export.run.${kind}`)!);
    expect(act.exportFiles.mock.calls).toEqual([["camilla_dsp"], ["filter_text"], ["equalizer_apo"], ["before_after_mdat"], ["staged_mdat"]]);
  });

  it("draws the filters for the DSP and the responses for REW as two titled groups between the card title and the rows", () => {
    renderWith(<Mounted view={exportRcOnly} act={actions()} advanced />);
    const files = ui("export.files")!;
    const groups = [...files.querySelectorAll<HTMLElement>("section.export-group")];
    expect(groups.map((group) => document.getElementById(group.getAttribute("aria-labelledby") ?? "")?.textContent)).toEqual(["DSP에 넣을 필터", "REW에서 볼 응답"]);
    expect(groups.map((group) => [...group.querySelectorAll("section[data-ui^='export.kind.']")].map((row) => row.getAttribute("data-ui")))).toEqual([
      ["export.kind.camilla_dsp", "export.kind.filter_text", "export.kind.equalizer_apo"],
      ["export.kind.before_after_mdat", "export.kind.staged_mdat"],
    ]);
    // h2 card, h3 group, h4 row.
    expect(files.querySelector("h2")?.textContent).toBe("파일 내보내기");
    expect([...groups[0]!.querySelectorAll("h3")].map((h) => h.textContent)).toEqual(["DSP에 넣을 필터"]);
    expect([...groups[0]!.querySelectorAll("h4")].map((h) => h.textContent)).toEqual(["CamillaDSP 설정", "필터 값 텍스트", "Equalizer APO 설정"]);
    fireEvent.click(ui("export.kind.camilla_dsp.help")!);
    expect(screen.getByRole("tooltip").textContent).toBe(t("export.kind.camilla_dsp.body"));
  });

  it("turns off a filter format that cannot carry the result and says why in its row", () => {
    const cases: [ExportView, Record<"filter_text" | "equalizer_apo", string | null>][] = [
      // Subs and MIMO (the sample): text blocked by the subs first; Equalizer APO carries both (F12).
      [exportReady, { filter_text: "서브(BA)가 있는 결과는 텍스트로 내보낼 수 없습니다. CamillaDSP나 Equalizer APO 설정으로 내보내세요.", equalizer_apo: null }],
      // Subs, MIMO left out: only the text.
      [exportNoMimo, { filter_text: "서브(BA)가 있는 결과는 텍스트로 내보낼 수 없습니다. CamillaDSP나 Equalizer APO 설정으로 내보내세요.", equalizer_apo: null }],
      // MIMO without subs: the text by MIMO.
      [exportMimoNoSubs, { filter_text: "MIMO를 켠 결과는 텍스트로 내보낼 수 없습니다. 보정에서 MIMO를 끄면 내보낼 수 있습니다.", equalizer_apo: null }],
      // RC alone: everything on.
      [exportRcOnly, { filter_text: null, equalizer_apo: null }],
    ];
    for (const [view, reasons] of cases) {
      const act = actions();
      const { unmount } = renderWith(<Mounted view={view} act={act} advanced />);
      for (const kind of ["filter_text", "equalizer_apo"] as const) {
        const button = ui(`export.run.${kind}`) as HTMLButtonElement;
        const reason = ui(`export.kind.${kind}.blocked`);
        expect(button.disabled, kind).toBe(reasons[kind] !== null);
        expect(reason?.textContent ?? null, kind).toBe(reasons[kind]);
        // The disabled button names its reason; the reason is no alert.
        expect(button.getAttribute("aria-describedby") ? document.getElementById(button.getAttribute("aria-describedby")!) : null).toBe(reason);
        expect(reason?.getAttribute("role") ?? null).toBeNull();
        fireEvent.click(button);
      }
      // CamillaDSP and the mdat exports carry every result.
      for (const kind of ["camilla_dsp", "before_after_mdat", "staged_mdat"]) {
        expect((ui(`export.run.${kind}`) as HTMLButtonElement).disabled, kind).toBe(false);
        expect(ui(`export.kind.${kind}.blocked`), kind).toBeNull();
      }
      expect(act.exportFiles.mock.calls).toEqual((["filter_text", "equalizer_apo"] as const).filter((kind) => reasons[kind] === null).map((kind) => [kind]));
      unmount();
    }
    // With MIMO in it, the Equalizer APO folder is named like the mdat one.
    renderWith(<Mounted view={exportMimoNoSubs} act={actions()} advanced />);
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
      // The action follows its sentence in the same notice and says what it lifts; a peer of the header's primary.
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
    expect((ui("export.run.camilla_dsp") as HTMLButtonElement).disabled).toBe(false);
    expect([...document.querySelectorAll("[data-ui$='.recalibrate']")].map((el) => el.getAttribute("data-ui"))).toEqual([
      "export.kind.before_after_mdat.recalibrate",
      "export.kind.staged_mdat.recalibrate",
    ]);
    unmount();
    // A stale result computed in this session exports mdat as it is; English names the formats too.
    const { unmount: unmountStale } = renderWith(<Mounted view={exportStale} act={actions()} advanced />);
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
      "Equalizer APO의 기존 config.txt를 다른 이름으로 보관한 뒤, 내보낸 config.txt와 secs_mixed 폴더를 config 폴더에 복사하세요. 적용 순서는 'EqAPO 적용 방법.txt'를 확인하세요.",
    );
    fireEvent.click(ui("export.outcome.equalizer_apo.open")!);
    expect(act.openFolder.mock.calls).toEqual([["D:/Audio/거실 보정/SECS_eqapo_RC_BA_mixed_20261014-142233"]]);
    unmount();
    // 초고지연: the files carry the limit (upstream `ultra200`); the folder keeps the bare mode.
    renderWith(<Mounted view={{ ...exportEqapoDone, settings: { ...exportEqapoDone.settings, mode: "ultra", ultra_ms: 200 } }} act={actions()} />, { locale: "en" });
    expect(ui("export.outcome.equalizer_apo.hint")?.textContent).toContain("config.txt and secs_ultra200 folder");
    expect(ui("export.kind.equalizer_apo")?.textContent).toContain("SECS_eqapo_RC_BA_ultra_<save time> folder");
  });

  it("adds the hint only to the Equalizer APO outcome", () => {
    renderWith(<Mounted view={exportDone} act={actions()} />);
    expect(ui("export.outcome.camilla_dsp")).not.toBeNull();
    expect(document.querySelector("[data-ui$='.hint']")).toBeNull();
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
      for (const kind of ["camilla_dsp", "filter_text", "equalizer_apo", "before_after_mdat", "staged_mdat"]) {
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
      const group = screen.getByRole("radiogroup", { name: "저장할 내용" });
      expect(group).toBe(ui("export.mdat.scope"));
      expect(screen.getByRole("radio", { checked: true }).textContent).toBe(chosen);
      expect(ui("export.mdat.file")?.textContent).toBe(file);
      // The choice comes first, the file's name last; every option can be chosen while a result exists.
      const row = ui("export.kind.before_after_mdat")!;
      const order = [...row.querySelectorAll("[data-ui='export.mdat.scope'], .export-item__where")];
      expect(order[0]).toBe(group);
      expect(order.at(-1)).toBe(ui("export.mdat.file"));
      expect(screen.getAllByRole("radio").every((radio) => !(radio as HTMLButtonElement).disabled)).toBe(true);
      fireEvent.click(screen.getByRole("radio", { name: chosen === "보정 결과만" ? "모두" : "보정 결과만" }));
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
    // File names stay Korean in English: they are the names the service writes.
    expect(ui("export.mdat.file")?.textContent).toBe("SECS 원 측정+보정 결과 저지연 모드.mdat");
  });

  it("holds the REW mdat at 원 측정만 without a result and says why", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={exportNoResult} act={act} />);
    const group = ui("export.mdat.scope")!;
    expect(screen.getByRole("radio", { checked: true }).textContent).toBe("원 측정만");
    expect((screen.getByRole("radio", { name: "모두" }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("radio", { name: "보정 결과만" }) as HTMLButtonElement).disabled).toBe(true);
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

  it("runs one export at a time: the running row is busy, every other button is off", () => {
    for (const running of ["filter_text", "equalizer_apo"] as const) {
      const { unmount } = renderWith(<Mounted view={{ ...exportRcOnly, exporting: running }} act={actions()} advanced />);
      for (const kind of ["camilla_dsp", "filter_text", "equalizer_apo", "before_after_mdat", "staged_mdat"]) {
        const button = ui(`export.run.${kind}`) as HTMLButtonElement;
        expect(button.disabled, kind).toBe(true);
        // The header's primary is off, not spinning, while another export runs.
        expect(button.getAttribute("aria-busy"), kind).toBe(kind === running ? "true" : null);
      }
      expect(screen.getByText(running === "filter_text" ? "필터 값 텍스트 저장 중" : "Equalizer APO 설정 저장 중")).toBeTruthy();
      unmount();
    }
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
