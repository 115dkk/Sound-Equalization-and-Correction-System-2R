// @vitest-environment happy-dom
// 관리 on the export screen (ADR 0018, P123): the level choice, the
// executable field, the service block in each state, the primary that
// applies and hands over, the 관리 끄기 confirm, and Windows only.
import { fireEvent, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AppFrame } from "../../app/AppFrame";
import { HOST_NOW } from "../../host/fixtures";
import { clockText } from "../../host/model";
import { translate } from "../../i18n";
import { useT, type Translate } from "../../i18n/react";
import { renderWith } from "../../test/render";
import { failureText } from "./apply";
import { ExportScreen, exportHeader } from "./ExportScreen";
import {
  exportFrame,
  manageCamillaApplied,
  manageCamillaGivenUp,
  manageCamillaHandedOver,
  manageCamillaInstallFailed,
  manageCamillaInstalling,
  manageCamillaNoExe,
  manageCamillaNotInstalled,
  manageCamillaOtherUser,
  manageCamillaRestarting,
  manageCamillaRunning,
  manageCamillaStopped,
  manageEqapoRepaired,
  manageNotWindows,
} from "./fixtures";
import type { ExportActions, ExportView } from "./types";

const STATES: Record<string, ExportView> = {
  manageCamillaApplied,
  manageCamillaGivenUp,
  manageCamillaHandedOver,
  manageCamillaInstallFailed,
  manageCamillaInstalling,
  manageCamillaNoExe,
  manageCamillaNotInstalled,
  manageCamillaOtherUser,
  manageCamillaRestarting,
  manageCamillaRunning,
  manageCamillaStopped,
  manageEqapoRepaired,
  manageNotWindows,
};

function actions() {
  const names = [
    "exportFiles", "setMdatScope", "setDesignName", "setCamillaDevices", "refreshDevices", "changeFolder", "openFolder", "goToCalibrate", "recalibrate",
    "setApplyLevel", "setCamillaAddress", "saveCamillaAddress", "refreshApplyStatus", "applyCamilla", "applyEqapo", "revertEqapo",
    "setCamillaExe", "saveCamillaExe", "pickCamillaExe", "installHost", "stopManaging",
  ] as const satisfies readonly (keyof ExportActions)[];
  return Object.fromEntries(names.map((name) => [name, vi.fn()])) as unknown as { [K in keyof ExportActions]: ReturnType<typeof vi.fn<ExportActions[K]>> };
}

function Mounted({ view, act }: { view: ExportView; act: ExportActions }) {
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
      <ExportScreen view={view} actions={act} />
    </AppFrame>
  );
}

const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
const button = (id: string) => ui(id) as HTMLButtonElement;
const t: Translate = (key, params) => translate("ko", key, params);
const described = (el: HTMLElement) => (el.getAttribute("aria-describedby") ?? "").split(" ").map((id) => document.getElementById(id)?.textContent ?? "").join(" ").trim();

describe("관리", () => {
  it("draws every 관리 state in both languages with every placeholder filled", () => {
    for (const locale of ["ko", "en"] as const) {
      for (const [name, view] of Object.entries(STATES)) {
        const { unmount } = renderWith(<Mounted view={view} act={actions()} />, { locale });
        expect(ui("export.apply.camilla.panel"), name).not.toBeNull();
        expect(document.body.textContent, name).not.toMatch(/\{\w+\}/);
        unmount();
      }
    }
  }, 15_000);

  it("asks for camilladsp.exe, holds 관리 시작 off without it, and stores what is typed or picked", () => {
    const act = actions();
    renderWith(<Mounted view={manageCamillaNoExe} act={act} />);
    const primary = button("export.apply.camilla");
    expect(primary.textContent).toBe("CamillaDSP 관리 시작");
    expect(primary.disabled).toBe(true);
    expect(ui("export.status.problem")?.textContent).toBe("CamillaDSP 실행 파일 필요");
    expect(described(primary)).toBe("CamillaDSP 실행 파일 필요");
    const field = screen.getByLabelText("CamillaDSP 실행 파일") as HTMLInputElement;
    expect(field.value).toBe("");
    expect(ui("export.manage.camilla.exe.missing")?.textContent).toBe("관리하려면 CamillaDSP 실행 파일(camilladsp.exe)을 고르십시오.");
    expect(described(field)).toBe(ui("export.manage.camilla.exe.missing")?.textContent);
    fireEvent.change(field, { target: { value: "D:/CamillaDSP/camilladsp.exe" } });
    fireEvent.keyDown(field, { key: "Enter" });
    fireEvent.blur(field);
    expect(act.setCamillaExe.mock.calls).toEqual([["D:/CamillaDSP/camilladsp.exe"]]);
    expect(act.saveCamillaExe).toHaveBeenCalledTimes(2);
    fireEvent.click(button("export.manage.camilla.exe.browse"));
    expect(act.pickCamillaExe).toHaveBeenCalledTimes(1);
    expect(act.applyCamilla).not.toHaveBeenCalled();
  });

  it("puts the stored executable back on Escape and shows no hint once there is one", () => {
    const act = actions();
    const draft: ExportView = { ...manageCamillaRunning, apply: { ...manageCamillaRunning.apply, exeDraft: "D:/other.exe" } };
    renderWith(<Mounted view={draft} act={act} />);
    const field = ui("export.manage.camilla.exe") as HTMLInputElement;
    expect(field.value).toBe("D:/other.exe");
    expect(ui("export.manage.camilla.exe.missing")).toBeNull();
    fireEvent.keyDown(field, { key: "Escape" });
    expect(act.setCamillaExe.mock.calls).toEqual([["C:/Program Files/CamillaDSP/camilladsp.exe"]]);
  });

  it("says why the service is needed, installs it from the row, and says what to do when it is stopped", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={manageCamillaNotInstalled} act={act} />);
    expect(ui("export.manage.camilla.service")?.textContent).toBe("설치 안 됨");
    const install = button("export.manage.camilla.install");
    expect(install.textContent).toBe("서비스 설치");
    // Windows's administrator approval is said before the click.
    expect(described(install)).toBe(t("host.install.why"));
    expect(t("host.install.why")).toContain("관리자 승인");
    expect(ui("export.status.problem")?.textContent).toBe("서비스 설치 필요");
    expect(button("export.apply.camilla").disabled).toBe(true);
    fireEvent.click(install);
    expect(act.installHost.mock.calls).toEqual([["camilla"]]);
    // No process line before the service runs.
    expect(ui("export.manage.camilla.state")).toBeNull();
    unmount();
    const { unmount: unmountInstalling } = renderWith(<Mounted view={manageCamillaInstalling} act={actions()} />);
    expect(ui("export.manage.camilla.service")?.textContent).toBe("설치 안 됨");
    expect(button("export.manage.camilla.install").getAttribute("aria-busy")).toBe("true");
    expect(ui("export.manage.camilla.service.reason")?.textContent).toBe("Windows의 관리자 승인 창에서 '예'를 고르십시오.");
    unmountInstalling();
    const { unmount: unmountFailed } = renderWith(<Mounted view={manageCamillaInstallFailed} act={actions()} />);
    const failure = ui("export.manage.camilla.failure")!;
    expect(failure.textContent).toContain("서비스를 설치하지 못했습니다.");
    expect(failure.textContent).toContain(t("host.install.failed.next"));
    expect(failure.querySelector("details pre")?.textContent).toBe("[apply_failed] elevated install: The operation was canceled by the user. (os error 1223)");
    // Only in the row it came from.
    expect(ui("export.manage.eqapo.failure")).toBeNull();
    unmountFailed();
    const stopped = actions();
    renderWith(<Mounted view={manageCamillaStopped} act={stopped} />);
    expect(ui("export.manage.camilla.service")?.textContent).toBe("정지됨");
    expect(ui("export.manage.camilla.service.reason")?.textContent).toBe("Windows의 서비스 앱에서 SECS 2R Host를 시작하십시오.");
    expect(ui("export.status.problem")?.textContent).toBe("서비스 정지됨");
    expect(ui("export.manage.camilla.install")).toBeNull();
    fireEvent.click(ui("export.manage.camilla.refresh")!);
    expect(stopped.refreshApplyStatus).toHaveBeenCalledTimes(1);
  });

  it("reads what the service does with CamillaDSP: nothing, running with its PID, restarting, given up, another user", () => {
    const { unmount } = renderWith(<Mounted view={manageCamillaNoExe} act={actions()} />);
    expect(ui("export.manage.camilla.service")?.textContent).toBe("실행 중");
    expect(ui("export.manage.camilla.state")?.textContent).toBe("관리 안 함");
    expect(ui("export.manage.camilla.stop")).toBeNull();
    unmount();
    const running = actions();
    const { unmount: unmountRunning } = renderWith(<Mounted view={manageCamillaRunning} act={running} />);
    expect(ui("export.manage.camilla.state")?.textContent).toBe("실행 중PID 4812");
    expect(ui("export.manage.camilla.reason")).toBeNull();
    // 관리 시작 is on: it applies again and hands over again.
    const primary = button("export.apply.camilla");
    expect(primary.disabled).toBe(false);
    fireEvent.click(primary);
    expect(running.applyCamilla).toHaveBeenCalledTimes(1);
    unmountRunning();
    const { unmount: unmountRestarting } = renderWith(<Mounted view={manageCamillaRestarting} act={actions()} />);
    expect(ui("export.manage.camilla.state")?.textContent).toBe("다시 시작 대기");
    const until = new Date(HOST_NOW + 4000).toISOString();
    expect(ui("export.manage.camilla.reason")?.textContent).toBe(t("host.camilla.restarting.detail", { time: clockText(until, HOST_NOW, "ko"), count: 2 }));
    unmountRestarting();
    const { unmount: unmountGivenUp } = renderWith(<Mounted view={manageCamillaGivenUp} act={actions()} />);
    expect(ui("export.manage.camilla.state")?.textContent).toBe("다시 시작 중단");
    expect(ui("export.manage.camilla.reason")?.textContent).toBe(`${t("host.camilla.given_up.detail")} ${t("host.camilla.given_up.next")}`);
    // The service's own reason only behind 자세히.
    expect(ui("export.manage.camilla.reason")?.textContent).not.toContain("five exits");
    expect(ui("export.manage.camilla")?.querySelector("details pre")?.textContent).toBe("five exits within five minutes");
    expect(ui("export.manage.camilla.stop")).not.toBeNull();
    unmountGivenUp();
    renderWith(<Mounted view={manageCamillaOtherUser} act={actions()} />);
    expect(ui("export.manage.camilla.state")?.textContent).toBe("다른 사용자 사용 중");
    expect(ui("export.manage.camilla.reason")?.textContent).toBe(t("host.camilla.other_user.detail"));
  });

  it("asks once in one sentence before 관리 끄기, naming what stops", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={manageCamillaRunning} act={act} />);
    const stop = button("export.manage.camilla.stop");
    expect(stop.textContent).toBe("관리 끄기");
    expect(stop.classList.contains("ui-button--quiet")).toBe(true);
    // Right after the state it ends.
    expect(ui("export.manage.camilla.state")?.nextElementSibling).toBe(stop);
    fireEvent.click(stop);
    const dialog = screen.getByRole("dialog");
    expect(dialog).toBe(ui("export.manage.camilla.stop.confirm"));
    expect(within(dialog).getByRole("heading").textContent).toBe("관리 끄기");
    // Both targets are managed: the sentence says both stop.
    expect(dialog.querySelector(".ui-dialog__body")?.textContent).toBe(t("host.stop.body.both"));
    fireEvent.click(ui("export.manage.camilla.stop.confirm.cancel")!);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(act.stopManaging).not.toHaveBeenCalled();
    fireEvent.click(stop);
    expect(button("export.manage.camilla.stop.confirm.confirm").textContent).toBe("관리 끄기");
    fireEvent.click(ui("export.manage.camilla.stop.confirm.confirm")!);
    expect(act.stopManaging.mock.calls).toEqual([["camilla"]]);
    unmount();
    // Only Equalizer APO managed: the sentence is about config.txt.
    const eqapo = actions();
    renderWith(<Mounted view={manageEqapoRepaired} act={eqapo} />);
    fireEvent.click(button("export.manage.eqapo.stop"));
    expect(screen.getByRole("dialog").querySelector(".ui-dialog__body")?.textContent).toBe(t("host.stop.body.eqapo"));
    fireEvent.click(ui("export.manage.eqapo.stop.confirm.confirm")!);
    expect(eqapo.stopManaging.mock.calls).toEqual([["eqapo"]]);
  });

  it("starts Equalizer APO's 관리 from its row and reads its line repair and the last repair time", () => {
    const act = actions();
    const { unmount } = renderWith(<Mounted view={manageEqapoRepaired} act={act} />);
    const run = button("export.apply.eqapo");
    expect(run.textContent).toBe("Equalizer APO 관리 시작");
    expect(run.disabled).toBe(false);
    fireEvent.click(run);
    expect(act.applyEqapo).toHaveBeenCalledTimes(1);
    expect(ui("export.manage.eqapo.state")?.textContent).toBe("켜짐");
    const repaired = manageEqapoRepaired.apply.host.status!.last_repair!;
    expect(ui("export.manage.eqapo.repair")?.textContent).toBe(clockText(repaired, HOST_NOW, "ko"));
    expect(ui("export.manage.eqapo.repair")?.querySelector("time")?.getAttribute("datetime")).toBe(repaired);
    // The 적용 facts stay: the install, the line and the design.
    expect(ui("export.apply.eqapo.line")?.textContent).toBe("SECS 2R 줄 있음");
    // The header's primary still exports CamillaDSP (its row is 파일만).
    expect(ui("export.run.camilla_dsp")?.closest(".step-header")).not.toBeNull();
    unmount();
    // The service not installed: the row's button is off and points at why.
    const missing: ExportView = { ...manageEqapoRepaired, apply: { ...manageEqapoRepaired.apply, host: manageCamillaNotInstalled.apply.host } };
    renderWith(<Mounted view={missing} act={actions()} />);
    const off = button("export.apply.eqapo");
    expect(off.disabled).toBe(true);
    expect(described(off)).toBe(t("host.install.why"));
  });

  it("keeps 관리 off with the reason off Windows, in both rows", () => {
    const { unmount } = renderWith(<Mounted view={manageNotWindows} act={actions()} />);
    for (const target of ["camilla", "eqapo"] as const) {
      const group = ui(`export.apply.${target}.level`)!;
      expect((within(group).getByRole("radio", { name: "관리" }) as HTMLButtonElement).disabled, target).toBe(true);
      expect(group.getAttribute("aria-describedby"), target).toContain(`${group.id}-hint`);
    }
    expect(document.getElementById("export.apply.camilla.level-hint")?.textContent).toBe("관리는 Windows에서만 지원합니다.");
    // CamillaDSP's 적용 stays available, and no service block is drawn.
    expect((within(ui("export.apply.camilla.level")!).getByRole("radio", { name: "적용" }) as HTMLButtonElement).disabled).toBe(false);
    expect(ui("export.manage.camilla")).toBeNull();
    unmount();
    // A 관리 stored by another client off Windows: still chosen, its primary off with the reason.
    const stored: ExportView = { ...manageNotWindows, apply: { ...manageNotWindows.apply, settings: { ...manageNotWindows.apply.settings, camilla: { level: "manage", address: "127.0.0.1:1234" } } } };
    renderWith(<Mounted view={stored} act={actions()} />);
    expect(within(ui("export.apply.camilla.level")!).getByRole("radio", { checked: true }).textContent).toBe("관리");
    expect(button("export.apply.camilla").disabled).toBe(true);
    expect(ui("export.status.problem")?.textContent).toBe("Windows 전용");
    expect(ui("export.manage.camilla")).toBeNull();
    expect(ui("export.manage.camilla.exe")).toBeNull();
  });

  it("says what the service now does once 관리 started, and what to do when camilladsp.exe was missing", () => {
    const { unmount } = renderWith(<Mounted view={manageCamillaApplied} act={actions()} />);
    expect(ui("export.apply.camilla.outcome")?.textContent).toContain("CamillaDSP에 적용했습니다. 앱을 닫아도 서비스가 CamillaDSP를 계속 실행하고, 멈추면 다시 시작합니다.");
    unmount();
    // No CamillaDSP was running: the service starts one with the design; the step counts as done.
    renderWith(<Mounted view={manageCamillaHandedOver} act={actions()} />);
    expect(ui("export.apply.camilla.outcome")?.textContent).toContain("CamillaDSP가 실행 중이 아니어서 서비스가 이 설계로 CamillaDSP를 시작합니다.");
    expect(screen.getByText("적용 완료")).toBeTruthy();
    const text = failureText(t, "camilla", { code: "invalid_request", params: { field: "apply.camilla.exe_path" }, retryable: false, detail: "managed CamillaDSP needs an executable path" });
    expect(text).toEqual({ cause: t("export.manage.camilla.exe.missing.cause"), next: t("export.manage.camilla.exe.missing.next") });
  });
});
