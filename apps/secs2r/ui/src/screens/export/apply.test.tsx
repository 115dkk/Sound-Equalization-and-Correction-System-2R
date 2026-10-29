// @vitest-environment happy-dom
// 적용 on the export screen (ADR 0018, P121): the level choice, the primary
// action that follows it, the CamillaDSP address, the status lines, the
// revert confirm and the outcomes' texts.
import { fireEvent, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AppFrame } from "../../app/AppFrame";
import { useT, type Translate } from "../../i18n/react";
import { translate } from "../../i18n";
import { renderWith } from "../../test/render";
import { ExportScreen, exportHeader } from "./ExportScreen";
import {
  applyCamillaApplied,
  applyCamillaApplying,
  applyCamillaBadAddress,
  applyCamillaChecking,
  applyCamillaOther,
  applyCamillaPlaceholders,
  applyCamillaRemote,
  applyCamillaRolledBack,
  applyCamillaThis,
  applyCamillaUnreachable,
  applyEqapoAbsent,
  applyEqapoDamaged,
  applyEqapoDenied,
  applyEqapoMissing,
  applyEqapoNotWindows,
  applyEqapoOfficialUnknown,
  applyEqapoOther,
  applyEqapoPresent,
  applyEqapoReverted,
  exportFrame,
  exportReady,
  exportTutorial,
} from "./fixtures";
import { EXPORT_KINDS, type ExportActions, type ExportView } from "./types";

const STATES: Record<string, ExportView> = {
  applyCamillaApplied,
  applyCamillaApplying,
  applyCamillaBadAddress,
  applyCamillaChecking,
  applyCamillaOther,
  applyCamillaPlaceholders,
  applyCamillaRemote,
  applyCamillaRolledBack,
  applyCamillaThis,
  applyCamillaUnreachable,
  applyEqapoAbsent,
  applyEqapoDamaged,
  applyEqapoDenied,
  applyEqapoMissing,
  applyEqapoNotWindows,
  applyEqapoOfficialUnknown,
  applyEqapoOther,
  applyEqapoPresent,
  applyEqapoReverted,
};

function actions() {
  return {
    exportFiles: vi.fn<ExportActions["exportFiles"]>(),
    setMdatScope: vi.fn<ExportActions["setMdatScope"]>(),
    setDesignName: vi.fn<ExportActions["setDesignName"]>(),
    setCamillaDevices: vi.fn<ExportActions["setCamillaDevices"]>(),
    refreshDevices: vi.fn<ExportActions["refreshDevices"]>(),
    changeFolder: vi.fn<ExportActions["changeFolder"]>(),
    openFolder: vi.fn<ExportActions["openFolder"]>(),
    goToCalibrate: vi.fn<ExportActions["goToCalibrate"]>(),
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

/** The screen inside the frame with its header, as the app mounts it. */
function Mounted({ view, act, confirm = false }: { view: ExportView; act: ExportActions; confirm?: boolean }) {
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
      <ExportScreen view={view} actions={act} initialRevertConfirm={confirm} />
    </AppFrame>
  );
}

const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
const t: Translate = (key, params) => translate("ko", key, params);
const button = (id: string) => ui(id) as HTMLButtonElement;
const described = (el: HTMLElement) => document.getElementById(el.getAttribute("aria-describedby") ?? "")?.textContent;

describe("적용", () => {
  it("draws every apply state in both languages with every placeholder filled", () => {
    for (const locale of ["ko", "en"] as const) {
      for (const [name, view] of Object.entries(STATES)) {
        const { unmount } = renderWith(<Mounted view={view} act={actions()} />, { locale });
        expect(ui("export.apply.camilla.panel"), name).not.toBeNull();
        expect(ui("export.apply.eqapo.panel"), name).not.toBeNull();
        expect(document.body.textContent, name).not.toMatch(/\{\w+\}/);
        unmount();
      }
    }
  }, 15_000);

  it("offers 파일만, 적용 and 관리 under the CamillaDSP and Equalizer APO rows and writes the choice", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={exportReady} act={act} />);
    for (const target of ["camilla", "eqapo"] as const) {
      const row = ui(target === "camilla" ? "export.kind.camilla_dsp" : "export.kind.equalizer_apo")!;
      const group = within(row).getByRole("radiogroup", { name: "적용 방식" });
      expect(group).toBe(ui(`export.apply.${target}.level`));
      expect(within(group).getAllByRole("radio").map((radio) => radio.textContent)).toEqual(["파일만", "적용", "관리"]);
      // 파일만 is the default.
      expect(within(group).getByRole("radio", { checked: true }).textContent).toBe("파일만");
      fireEvent.click(within(group).getByRole("radio", { name: "적용" }));
      fireEvent.click(within(group).getByRole("radio", { name: "관리" }));
    }
    expect(act.setApplyLevel.mock.calls).toEqual([["camilla", "apply"], ["camilla", "manage"], ["eqapo", "apply"], ["eqapo", "manage"]]);
    // 파일만 draws nothing else: no address, no status.
    expect(ui("export.apply.camilla.address")).toBeNull();
    expect(ui("export.apply.eqapo.state")).toBeNull();
    // The other filter rows have no choice.
    for (const kind of ["filter_text", "convolution", "before_after_mdat"]) expect(ui(`export.kind.${kind}`)!.querySelector("[data-ui$='.level']"), kind).toBeNull();
    unmount();
    // 관리 stored by another client reads as 관리.
    renderWith(<Mounted view={{ ...applyCamillaThis, apply: { ...applyCamillaThis.apply, settings: { ...applyCamillaThis.apply.settings, camilla: { level: "manage", address: "127.0.0.1:1234" } } } }} act={actions()} />);
    expect(within(ui("export.apply.camilla.level")!).getByRole("radio", { checked: true }).textContent).toBe("관리");
  });

  it("holds Equalizer APO's 적용 off with the reason off Windows, and draws no choice in the tutorial", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={applyEqapoNotWindows} act={act} />);
    const group = ui("export.apply.eqapo.level")!;
    const apply = within(group).getByRole("radio", { name: "적용" }) as HTMLButtonElement;
    expect(apply.disabled).toBe(true);
    expect(document.getElementById(`${group.id}-hint`)?.textContent).toBe("Equalizer APO 적용은 Windows에서만 지원합니다.");
    expect(group.getAttribute("aria-describedby")).toContain(`${group.id}-hint`);
    // CamillaDSP's websocket works anywhere.
    expect((within(ui("export.apply.camilla.level")!).getByRole("radio", { name: "적용" }) as HTMLButtonElement).disabled).toBe(false);
    unmount();
    renderWith(<Mounted view={exportTutorial} act={actions()} />);
    expect(ui("export.apply.camilla.panel")).toBeNull();
    expect(ui("export.apply.eqapo.panel")).toBeNull();
  });

  it("turns the header's primary into CamillaDSP에 적용 and keeps the export in the row as a secondary action", () => {
    // 파일만: the header exports, the row has no button of its own.
    const files = exportHeader(exportReady, actions(), t);
    expect(files.primary).toMatchObject({ id: "export.run.camilla_dsp", label: "CamillaDSP 설정 내보내기", busy: false, disabled: false });
    const act = actions();
    const { unmount } = renderWith(<Mounted view={exportReady} act={act} />);
    expect(ui("export.apply.camilla")).toBeNull();
    expect(ui("export.kind.camilla_dsp")!.querySelector("[data-ui='export.run.camilla_dsp']")).toBeNull();
    fireEvent.click(ui("export.run.camilla_dsp")!);
    expect(act.exportFiles.mock.calls).toEqual([["camilla_dsp"]]);
    unmount();
    // 적용: the header applies; 내보내기 stays in the row.
    const applying = actions();
    const { unmount: unmountApply } = renderWith(<Mounted view={applyCamillaThis} act={applying} />);
    const primary = button("export.apply.camilla");
    expect(primary.textContent).toBe("CamillaDSP에 적용");
    expect(primary.classList.contains("ui-button--primary")).toBe(true);
    expect(primary.closest(".step-header")).not.toBeNull();
    const save = button("export.run.camilla_dsp");
    expect(save.closest("[data-ui='export.kind.camilla_dsp']")).not.toBeNull();
    expect(save.textContent).toBe("CamillaDSP 설정 저장");
    expect(save.classList.contains("ui-button--secondary")).toBe(true);
    fireEvent.click(primary);
    fireEvent.click(save);
    expect(applying.applyCamilla).toHaveBeenCalledTimes(1);
    expect(applying.exportFiles.mock.calls).toEqual([["camilla_dsp"]]);
    unmountApply();
    // Running: the primary spins, every export is off, the pill names the job.
    renderWith(<Mounted view={applyCamillaApplying} act={actions()} />);
    expect(button("export.apply.camilla").getAttribute("aria-busy")).toBe("true");
    // (The staged mdat sits under the closed 고급.)
    for (const kind of EXPORT_KINDS.filter((kind) => kind !== "staged_mdat")) expect(button(`export.run.${kind}`).disabled, kind).toBe(true);
    expect(screen.getByText("CamillaDSP에 적용 중")).toBeTruthy();
  });

  it("applies Equalizer APO from its row, the export quiet beside it, and holds it off with the reason when it is not installed", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={applyEqapoAbsent} act={act} />);
    const row = ui("export.kind.equalizer_apo")!;
    const apply = within(row).getByRole("button", { name: "Equalizer APO에 적용" }) as HTMLButtonElement;
    expect(apply).toBe(ui("export.apply.eqapo"));
    expect(apply.classList.contains("ui-button--secondary")).toBe(true);
    const save = button("export.run.equalizer_apo");
    expect(save.classList.contains("ui-button--quiet")).toBe(true);
    expect(apply.compareDocumentPosition(save) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    fireEvent.click(apply);
    fireEvent.click(save);
    expect(act.applyEqapo).toHaveBeenCalledTimes(1);
    expect(act.exportFiles.mock.calls).toEqual([["equalizer_apo"]]);
    // The header still exports CamillaDSP (its row is 파일만).
    expect(ui("export.run.camilla_dsp")?.closest(".step-header")).not.toBeNull();
    unmount();
    const missing = actions();
    renderWith(<Mounted view={applyEqapoMissing} act={missing} />);
    const off = button("export.apply.eqapo");
    expect(off.disabled).toBe(true);
    expect(described(off)).toBe("Equalizer APO가 없어 적용할 수 없습니다. 설치한 뒤 '다시 확인'을 누르십시오.");
    // 적용 stays chosen.
    expect(within(ui("export.apply.eqapo.level")!).getByRole("radio", { checked: true }).textContent).toBe("적용");
    expect(ui("export.apply.eqapo.state")?.textContent).toBe("미설치");
    fireEvent.click(ui("export.apply.eqapo.refresh")!);
    expect(missing.refreshApplyStatus).toHaveBeenCalledTimes(1);
  });

  it("checks the CamillaDSP address as typed, stores it on Enter or leaving the field, and puts the stored one back on Escape", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={applyCamillaUnreachable} act={act} />);
    const input = screen.getByLabelText("CamillaDSP 주소") as HTMLInputElement;
    expect(input).toBe(ui("export.apply.camilla.address"));
    expect(input.value).toBe("127.0.0.1:1234");
    expect(input.getAttribute("aria-invalid")).toBeNull();
    // Loopback: no note.
    expect(ui("export.apply.camilla.address.remote")).toBeNull();
    fireEvent.change(input, { target: { value: "127.0.0.1:1235" } });
    fireEvent.keyDown(input, { key: "Enter" });
    fireEvent.blur(input);
    expect(act.setCamillaAddress.mock.calls).toEqual([["127.0.0.1:1235"]]);
    expect(act.saveCamillaAddress).toHaveBeenCalledTimes(2);
    unmount();
    // An address without a port: the field says why and the header's primary is off with the reason.
    const bad = actions();
    const { unmount: unmountBad } = renderWith(<Mounted view={applyCamillaBadAddress} act={bad} />);
    const field = ui("export.apply.camilla.address") as HTMLInputElement;
    expect(field.value).toBe("192.168.0.40");
    expect(field.getAttribute("aria-invalid")).toBe("true");
    expect(ui("export.apply.camilla.address.problem")?.textContent).toBe("주소와 포트를 127.0.0.1:1234 형식으로 입력하십시오.");
    expect(described(field)).toBe(ui("export.apply.camilla.address.problem")?.textContent);
    const primary = button("export.apply.camilla");
    expect(primary.disabled).toBe(true);
    expect(ui("export.status.problem")?.textContent).toBe("CamillaDSP 주소 수정 필요");
    expect(described(primary)).toBe("CamillaDSP 주소 수정 필요");
    fireEvent.click(primary);
    expect(bad.applyCamilla).not.toHaveBeenCalled();
    // An invalid address is not the export's business: the row still saves files.
    expect(button("export.run.camilla_dsp").disabled).toBe(false);
    // Escape puts the stored address back without leaving the screen.
    fireEvent.keyDown(field, { key: "Escape" });
    expect(bad.setCamillaAddress.mock.calls).toEqual([["127.0.0.1:1234"]]);
    unmountBad();
    // Another computer: one line on what that means.
    renderWith(<Mounted view={applyCamillaRemote} act={actions()} />);
    expect(ui("export.apply.camilla.address.remote")?.textContent).toBe(t("export.apply.camilla.address.remote"));
    expect(ui("export.apply.camilla.reason")?.textContent).toContain("192.168.0.40:1234의 CamillaDSP에 연결하지 못했습니다.");
  });

  it("says why CamillaDSP does not answer, keeps the raw text behind 자세히, and reads it again quietly", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={applyCamillaUnreachable} act={act} />);
    expect(ui("export.apply.camilla.state")?.textContent).toBe("연결 안 됨");
    expect(ui("export.apply.camilla.reason")?.textContent).toBe(
      "127.0.0.1:1234의 CamillaDSP에 연결하지 못했습니다. CamillaDSP의 실행 여부, 웹소켓 포트와 주소를 확인한 뒤 '다시 확인'을 누르십시오.",
    );
    // The raw text is not the message.
    expect(ui("export.apply.camilla.reason")?.textContent).not.toContain("os error");
    expect(ui("export.apply.camilla.panel")?.querySelector("details pre")?.textContent).toContain("[apply_failed] connect 127.0.0.1:1234");
    const refresh = button("export.apply.camilla.refresh");
    expect(refresh.classList.contains("ui-button--quiet")).toBe(true);
    fireEvent.click(refresh);
    expect(act.refreshApplyStatus).toHaveBeenCalledTimes(1);
    // With file placeholders the row says to choose devices.
    expect(ui("export.apply.camilla.devices_note")).toBeNull();
    unmount();
    const { unmount: unmountChecking } = renderWith(<Mounted view={applyCamillaChecking} act={actions()} />);
    expect(ui("export.apply.camilla.state")?.textContent).toBe("확인 중");
    expect(ui("export.apply.camilla.refresh")).toBeNull();
    unmountChecking();
    renderWith(<Mounted view={applyCamillaPlaceholders} act={actions()} />);
    expect(ui("export.apply.camilla.devices_note")?.textContent).toBe(t("export.apply.camilla.placeholders"));
  });

  it("reads CamillaDSP's version and state, the design it runs, and clipped samples when there are any", () => {
    const { unmount } = renderWith(<Mounted view={applyCamillaThis} act={actions()} />);
    expect(ui("export.apply.camilla.state")?.textContent).toBe("CamillaDSP 4.1.3 · 처리 중");
    expect(ui("export.apply.camilla.design")?.textContent).toBe("이 설계");
    expect(ui("export.apply.camilla.clipped")).toBeNull();
    unmount();
    const { unmount: unmountOther } = renderWith(<Mounted view={applyCamillaOther} act={actions()} />);
    expect(ui("export.apply.camilla.design")?.textContent).toBe("다른 설계");
    // Its title on a line of its own: the title has middle dots of its own.
    expect(ui("export.apply.camilla.design.title")?.textContent).toBe("SECS 2R · 거실 영화 · 표준 모드");
    expect(ui("export.apply.camilla.clipped")?.textContent).toBe("샘플 1,204개");
    unmountOther();
    // An unknown state shows as CamillaDSP wrote it; an unknown design says so.
    const odd: ExportView = {
      ...applyCamillaThis,
      apply: { ...applyCamillaThis.apply, status: { ...applyCamillaThis.apply.status!, camilla: { ...applyCamillaThis.apply.status!.camilla, design: { kind: "unknown" }, status: { ...applyCamillaThis.apply.status!.camilla.status!, state: "Draining" } } } },
    };
    const { unmount: unmountOdd } = renderWith(<Mounted view={odd} act={actions()} />);
    expect(ui("export.apply.camilla.state")?.textContent).toBe("CamillaDSP 4.1.3 · Draining");
    expect(ui("export.apply.camilla.design")?.textContent).toBe("알 수 없음");
    unmountOdd();
    renderWith(<Mounted view={applyCamillaOther} act={actions()} />, { locale: "en" });
    expect(ui("export.apply.camilla.state")?.textContent).toBe("CamillaDSP 4.1.3 · Running");
    expect(ui("export.apply.camilla.clipped")?.textContent).toBe("Samples: 1,204");
  });

  it("reads Equalizer APO's edition, folder, SECS 2R's line and the design applied there against this one", () => {
    const cases: [ExportView, string, string, string][] = [
      [applyEqapoAbsent, "Equalizer APO XT", "SECS 2R 줄 없음", "없음"],
      [applyEqapoPresent, "Equalizer APO XT", "SECS 2R 줄 있음", "이 설계"],
      [applyEqapoOther, "Equalizer APO XT", "SECS 2R 줄 있음", "다른 설계"],
      [applyEqapoOfficialUnknown, "Equalizer APO 공식판", "SECS 2R 줄 있음", "설계 비교 불가"],
      [applyEqapoDamaged, "Equalizer APO XT", "SECS 2R 줄 손상", "다른 설계"],
    ];
    for (const [view, edition, line, design] of cases) {
      const { unmount } = renderWith(<Mounted view={view} act={actions()} />);
      expect(ui("export.apply.eqapo.state")?.textContent, line).toBe(edition);
      expect(ui("export.apply.eqapo.folder")?.textContent, line).toBe(view.apply.status!.eqapo.config_path);
      expect(ui("export.apply.eqapo.line")?.textContent, line).toBe(line);
      expect(ui("export.apply.eqapo.design")?.textContent, line).toBe(design);
      // 빼기 only while the line is there; otherwise what 적용 does to config.txt.
      expect(!!ui("export.apply.eqapo.revert"), line).toBe(view.apply.status!.eqapo.include === "present");
      expect(ui("export.apply.eqapo.line_note")?.textContent ?? null, line).toBe(
        view.apply.status!.eqapo.include === "absent" ? t("export.apply.eqapo.line.absent.note") : view.apply.status!.eqapo.include === "damaged" ? t("export.apply.eqapo.line.damaged.note") : null,
      );
      unmount();
    }
  });

  it("asks once in one sentence before taking SECS 2R's line out of config.txt", () => {
    const act = actions();
    renderWith(<Mounted view={applyEqapoPresent} act={act} />);
    const revert = button("export.apply.eqapo.revert");
    expect(revert.textContent).toBe("config.txt에서 빼기");
    expect(revert.classList.contains("ui-button--quiet")).toBe(true);
    // Right after the line it removes.
    expect(ui("export.apply.eqapo.line")?.nextElementSibling).toBe(revert);
    fireEvent.click(revert);
    const dialog = screen.getByRole("dialog");
    expect(dialog).toBe(ui("export.apply.eqapo.revert.confirm"));
    expect(within(dialog).getByRole("heading").textContent).toBe("config.txt에서 SECS 2R 줄 빼기");
    expect(dialog.querySelector(".ui-dialog__body")?.textContent).toBe("Equalizer APO는 SECS 2R 필터 사용을 중단하며, 필터 파일은 SECS2R 폴더에 남습니다.");
    // Cancel leaves config.txt alone.
    fireEvent.click(ui("export.apply.eqapo.revert.confirm.cancel")!);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(act.revertEqapo).not.toHaveBeenCalled();
    fireEvent.click(revert);
    expect(button("export.apply.eqapo.revert.confirm.confirm").textContent).toBe("config.txt에서 빼기");
    fireEvent.click(ui("export.apply.eqapo.revert.confirm.confirm")!);
    expect(act.revertEqapo).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("opens with the confirm showing for the gallery, and says the line went once it did", () => {
    const { unmount } = renderWith(<Mounted view={applyEqapoPresent} act={actions()} confirm />);
    expect(ui("export.apply.eqapo.revert.confirm")).not.toBeNull();
    unmount();
    renderWith(<Mounted view={applyEqapoReverted} act={actions()} />);
    expect(ui("export.apply.eqapo.outcome")?.textContent).toBe("config.txt의 SECS 2R 줄 제거 완료");
  });

  it("writes each outcome in the row's words: applied, rolled back with CamillaDSP's error, no write access with the folder", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={applyCamillaApplied} act={act} />);
    expect(ui("export.apply.camilla.outcome")?.textContent).toContain("CamillaDSP 적용 완료");
    fireEvent.click(ui("export.apply.camilla.outcome.open")!);
    expect(act.openFolder.mock.calls).toEqual([["C:/Users/demo/AppData/Roaming/SECS2R/applied/camilladsp"]]);
    expect(ui("export.status.problem")).toBeNull();
    expect(screen.getByText("적용 완료")).toBeTruthy();
    unmount();
    // Rolled back: CamillaDSP's own words stay in sight; the header says it failed.
    const { unmount: unmountRolled } = renderWith(<Mounted view={applyCamillaRolledBack} act={actions()} />);
    const rolled = ui("export.apply.camilla.outcome")!;
    expect(rolled.textContent).toContain("CamillaDSP가 새 설정을 읽지 못해 이전 설정으로 되돌렸습니다.");
    expect(ui("export.apply.camilla.outcome.camilla_error")?.textContent).toBe("Reload: Playback error: could not open device 'Speakers (Tesira)': the device is in use by another application");
    expect(rolled.textContent).toContain("오류에 표시된 장치나 설정을 고친 뒤 다시 적용하십시오.");
    expect(screen.getByText("적용 실패")).toBeTruthy();
    unmountRolled();
    // No write access: the folder, and that it needs write access; no elevation offered.
    const { unmount: unmountDenied } = renderWith(<Mounted view={applyEqapoDenied} act={actions()} />);
    const denied = ui("export.apply.eqapo.outcome")!;
    expect(denied.textContent).toContain("Equalizer APO 설정 폴더에 쓸 권한이 없습니다.");
    expect(ui("export.apply.eqapo.outcome.path")?.textContent).toBe("C:/Program Files/EqualizerAPO/config/SECS2R");
    expect(denied.textContent).toContain("현재 계정에 이 폴더의 쓰기 권한을 부여한 뒤 다시 적용하십시오.");
    expect(denied.querySelector("details pre")?.textContent).toBe("[permission_denied] Access is denied. (os error 5)");
    expect(within(denied).queryAllByRole("button")).toEqual([]);
    unmountDenied();
    renderWith(<Mounted view={applyEqapoDenied} act={actions()} />, { locale: "en" });
    expect(ui("export.apply.eqapo.outcome")?.textContent).toContain("You do not have write access to Equalizer APO's config folder.");
  });
});
