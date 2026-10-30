// @vitest-environment happy-dom
// The updater on screen (ADR 0020, P134): the rail badge only in its
// phases, 지금 다시 시작 and its confirm, the notice once (and not over a
// confirm), the settings section storing the policy, the fixed ids, and the
// CHANGELOG subset the notice reads.
import { act, cleanup, fireEvent, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { UpdatePhase } from "../../ipc/bindings/UpdatePhase";
import type { UpdateStatus } from "../../ipc/bindings/UpdateStatus";
import { createMockService, updateStates } from "../../ipc/mock";
import { createSession } from "../../state/session";
import { renderWith } from "../../test/render";
import { ConfirmDialog, Dialog } from "../../ui/Dialog";
import { AppFrame } from "../AppFrame";
import type { FrameModel } from "../model";
import { SettingsButton } from "../menus";
import { badgeShown } from "./model";
import { parseNotes, parseSpans } from "./notes";
import { StaticUpdateScope, UpdateScope, type UpdateApi, type UpdateView } from "./scope";
import { RailUpdateBadge, UpdateBadge } from "./UpdateBadge";
import { UpdateNoticeHost } from "./UpdateNotice";

const disposals: (() => void)[] = [];
afterEach(() => {
  cleanup();
  for (const dispose of disposals.splice(0)) dispose();
  vi.useRealTimers();
});

const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);
const need = (id: string) => {
  const node = ui(id);
  if (!node) throw new Error(`missing [data-ui="${id}"]`);
  return node;
};

const URL = "https://github.com/115dkk/Sound-Equalization-and-Correction-System-2R/releases/tag/v1.2.0";

const PHASES: Record<string, UpdatePhase> = {
  off: { kind: "off" },
  idle: { kind: "idle" },
  checking: { kind: "checking" },
  up_to_date: { kind: "up_to_date" },
  available: { kind: "available", version: "1.2.0" },
  downloading: { kind: "downloading", version: "1.2.0", received: 40, total: 100 },
  ready_on_exit: { kind: "ready", version: "1.2.0", install: "on_exit" },
  ready_on_request: { kind: "ready", version: "1.2.0", install: "on_request" },
  installing: { kind: "installing", version: "1.2.0" },
  portable: { kind: "portable", version: "1.2.0", url: URL },
  failed: { kind: "failed", reason: "Could not check for updates" },
};

function makeApi(status: Partial<UpdateStatus> = {}, view: Partial<Omit<UpdateView, "status">> = {}) {
  const actions = {
    check: vi.fn(async () => {}),
    download: vi.fn(async () => {}),
    restart: vi.fn(async () => {}),
    dismiss: vi.fn(async () => {}),
    setPolicy: vi.fn(async () => true),
    openRelease: vi.fn(async (_version: string) => {}),
  };
  const api: UpdateApi = {
    view: { status: { ...updateStates.upToDate, ...status }, userAction: false, dirty: false, busy: false, ...view },
    actions,
  };
  return { api, actions };
}

const frameModel: FrameModel = {
  current: "measure",
  steps: {
    system: { status: "done" },
    connect: { status: "done" },
    measure: { status: "ready" },
    calibrate: { status: "blocked", reason: "no_measurements" },
    export: { status: "blocked", reason: "no_result" },
  },
  project: {
    preset: "2.0",
    subCount: 0,
    outputs: 2,
    positionsDone: 0,
    positionsTotal: 9,
    calibration: "none",
    path: { kind: "rew", state: "ok" },
    fileName: "거실.secs",
    dirty: false,
  },
};

const frame = (
  <AppFrame
    model={frameModel}
    onNavigate={() => {}}
    projectActions={{ onNew: () => {}, onOpen: () => {}, onSave: () => {}, onSaveAs: () => {} }}
    onHelp={() => {}}
    version={{ version: "1.1.0", prerelease: true }}
    tray={<div />}
    shortcuts={false}
  >
    <p>content</p>
  </AppFrame>
);

function live(status: UpdateStatus) {
  vi.useFakeTimers();
  const mock = createMockService({ tickMs: 0, updateStatus: status });
  const session = createSession(mock);
  disposals.push(() => {
    session.dispose();
    mock.dispose();
  });
  return { mock, session };
}

const settle = () => act(() => vi.advanceTimersByTimeAsync(0));

describe("update badge", () => {
  const shown = new Set(["available", "downloading", "ready_on_exit", "ready_on_request", "portable"]);
  it.each(Object.keys(PHASES))("is drawn in %s only when that phase is listed", (name) => {
    const phase = PHASES[name]!;
    const { api } = makeApi({ phase });
    renderWith(<UpdateBadge api={api} />);
    const badge = ui("update.badge");
    expect(Boolean(badge)).toBe(shown.has(name));
    expect(badgeShown(phase, false)).toBe(shown.has(name));
    if (badge) {
      expect(badge.dataset.state).toBe(phase.kind);
      expect(badge.dataset.version).toBe("1.2.0");
    }
  });

  it("shows a failure and the install only when they answer the user's own action", () => {
    for (const name of ["failed", "installing"]) {
      const { api } = makeApi({ phase: PHASES[name]! }, { userAction: true });
      renderWith(<UpdateBadge api={api} />);
      expect(need("update.badge").dataset.state).toBe(PHASES[name]!.kind);
      cleanup();
    }
    const failed = makeApi({ phase: PHASES.failed! }, { userAction: true });
    renderWith(<UpdateBadge api={failed.api} />);
    expect(ui("update.badge")?.dataset.version).toBeUndefined();
    fireEvent.click(need("update.badge.retry"));
    expect(failed.actions.check).toHaveBeenCalledTimes(1);
  });

  it("says a new version is out under 알림만 and downloads it from 받아서 설치", () => {
    const { api, actions } = makeApi({ ...updateStates.available });
    renderWith(<UpdateBadge api={api} />);
    const badge = need("update.badge");
    expect(badge.dataset.state).toBe("available");
    expect(badge.dataset.version).toBe("1.2.0");
    expect(badge.textContent).toContain("새 판 1.2.0 있음");
    const download = need("update.badge.download");
    expect(download.textContent).toBe("받아서 설치");
    fireEvent.click(download);
    expect(actions.download).toHaveBeenCalledTimes(1);
  });

  it("restarts now from 지금 다시 시작, and says 지금 설치 where installing asks for the password", () => {
    const { api, actions } = makeApi({ phase: PHASES.ready_on_exit! });
    renderWith(<UpdateBadge api={api} />);
    expect(need("update.badge").textContent).toContain("다음 실행 때 1.2.0 적용");
    fireEvent.click(need("update.restart"));
    expect(actions.restart).toHaveBeenCalledTimes(1);
    expect(ui("update.restart.confirm")).toBeNull();
    cleanup();
    renderWith(<UpdateBadge api={makeApi({ phase: PHASES.ready_on_request! }).api} />);
    expect(need("update.restart").textContent).toBe("지금 설치");
    expect(need("update.badge").textContent).toContain("1.2.0 설치 준비됨");
  });

  it("asks once before restarting over unsaved changes or running jobs", () => {
    const { api, actions } = makeApi({ phase: PHASES.ready_on_exit! }, { dirty: true, busy: true });
    renderWith(<UpdateBadge api={api} />);
    fireEvent.click(need("update.restart"));
    expect(actions.restart).not.toHaveBeenCalled();
    const confirm = need("update.restart.confirm");
    expect(confirm.textContent).toContain("저장하지 않은 변경 사항은 사라집니다.");
    expect(confirm.textContent).toContain("진행 중인 작업은 멈춥니다.");
    fireEvent.click(need("update.restart.confirm.confirm"));
    expect(actions.restart).toHaveBeenCalledTimes(1);
  });

  it("opens a portable copy's release page through openRelease and draws a plain bar while downloading", () => {
    const portable = makeApi({ phase: { kind: "portable", version: "1.3.0-rc.1", url: URL } });
    renderWith(<UpdateBadge api={portable.api} />);
    const open = need("update.portable");
    expect(open.tagName).toBe("BUTTON");
    expect(open.hasAttribute("href")).toBe(false);
    fireEvent.click(open);
    expect(portable.actions.openRelease).toHaveBeenCalledWith("1.3.0-rc.1");
    cleanup();
    renderWith(<UpdateBadge api={makeApi({ phase: PHASES.downloading! }).api} />);
    const bar = need("update.badge").querySelector('[role="progressbar"]')!;
    expect(bar.getAttribute("aria-valuenow")).toBe("40");
    expect(bar.getAttribute("aria-valuemax")).toBe("100");
  });

  it("shows a failure after 지금 확인 from settings, but not one the user did not ask about", async () => {
    const { mock, session } = live(updateStates.failed);
    renderWith(
      <UpdateScope>
        <RailUpdateBadge />
        <SettingsButton initiallyOpen inline />
      </UpdateScope>,
      { session },
    );
    await settle();
    expect(need("settings.updates.state").dataset.state).toBe("failed");
    expect(ui("update.badge")).toBeNull();
    fireEvent.click(need("update.check"));
    await settle();
    expect(need("settings.updates.state").dataset.state).toBe("checking");
    mock.setUpdateStatus(updateStates.failed);
    await act(() => vi.advanceTimersByTimeAsync(1_000));
    expect(need("update.badge").dataset.state).toBe("failed");
    // Any other result ends it: the next failure is not the user's.
    mock.setUpdateStatus(updateStates.upToDate);
    await act(() => vi.advanceTimersByTimeAsync(60_000));
    mock.setUpdateStatus(updateStates.failed);
    await act(() => vi.advanceTimersByTimeAsync(60_000));
    expect(need("settings.updates.state").dataset.state).toBe("failed");
    expect(ui("update.badge")).toBeNull();
  });
});

describe("update notice", () => {
  it("shows once: dismiss calls dismiss and the notice stays closed", () => {
    const { api, actions } = makeApi({ ...updateStates.skippedNotice });
    function Page() {
      // The first read has no notice yet; each read after it is a new status
      // object with the same notice.
      const [reads, setReads] = useState(0);
      const status = reads === 0 ? { ...api.view.status!, notice: null } : { ...api.view.status! };
      return (
        <>
          <button type="button" data-ui="test.read" onClick={() => setReads(reads + 1)} />
          <UpdateNoticeHost api={{ ...api, view: { ...api.view, status } }} />
        </>
      );
    }
    renderWith(<Page />);
    expect(ui("update.notice")).toBeNull();
    fireEvent.click(need("test.read"));
    const notice = need("update.notice");
    expect(notice.getAttribute("role")).toBe("dialog");
    expect(notice.textContent).toContain("1.2.0 업데이트 완료");
    expect(document.activeElement).toBe(need("update.notice.dismiss"));
    // The release page of the version now running, where the skipped ones are linked.
    fireEvent.click(need("update.notice.release"));
    expect(actions.openRelease).toHaveBeenCalledWith(updateStates.skippedNotice.notice!.to);
    expect(ui("update.notice")).not.toBeNull();
    fireEvent.click(need("update.notice.dismiss"));
    expect(actions.dismiss).toHaveBeenCalledTimes(1);
    expect(ui("update.notice")).toBeNull();
    // The service has not answered yet: the same notice does not come back.
    fireEvent.click(need("test.read"));
    expect(ui("update.notice")).toBeNull();
  });

  it("leaves out the release link when no version was skipped, and dismisses on Escape too", () => {
    const { api, actions } = makeApi({ ...updateStates.notice });
    renderWith(<UpdateNoticeHost api={api} />);
    expect(ui("update.notice.release")).toBeNull();
    fireEvent.keyDown(need("update.notice"), { key: "Escape" });
    expect(actions.dismiss).toHaveBeenCalledTimes(1);
    expect(ui("update.notice")).toBeNull();
  });

  it("waits while a confirm dialog is open and opens once it closes", () => {
    const { api } = makeApi({ ...updateStates.notice });
    function Page() {
      const [confirming, setConfirming] = useState(true);
      return (
        <>
          <ConfirmDialog id="measure.clear" open={confirming} onClose={() => setConfirming(false)} onConfirm={() => {}} title="위치 3 측정 지우기" body="측정 3개가 지워집니다." confirmLabel="측정 지우기" />
          <UpdateNoticeHost api={api} />
        </>
      );
    }
    renderWith(<Page />);
    expect(ui("measure.clear")).not.toBeNull();
    expect(ui("update.notice")).toBeNull();
    fireEvent.click(need("measure.clear.cancel"));
    expect(ui("measure.clear")).toBeNull();
    expect(need("update.notice").textContent).toContain("1.2.0 업데이트 완료");
  });

  it("opens over a dialog of the start and keeps focus when that dialog mounts after it", () => {
    vi.useFakeTimers();
    const { api } = makeApi({ ...updateStates.notice });
    function Page() {
      const [other, setOther] = useState(false);
      return (
        <>
          <button type="button" data-ui="test.open" onClick={() => setOther(true)} />
          <UpdateNoticeHost api={api} />
          <Dialog open={other} id="project.new" title="새 프로젝트" onClose={() => {}} footer={<button type="button">만들기</button>} />
        </>
      );
    }
    renderWith(<Page />);
    fireEvent.click(need("test.open"));
    expect(ui("project.new")).not.toBeNull();
    expect(need("update.notice").closest(".ui-dialog-backdrop")?.classList.contains("ui-dialog-backdrop--above")).toBe(true);
    expect(document.activeElement).toBe(need("update.notice.dismiss"));
    fireEvent.click(need("update.notice.dismiss"));
    act(() => {
      vi.runOnlyPendingTimers();
    });
    // Focus goes into the dialog left under it.
    expect(need("project.new").contains(document.activeElement)).toBe(true);
  });
});

describe("update settings", () => {
  it("stores the chosen policy through setPolicy", async () => {
    const { mock, session } = live(updateStates.upToDate);
    renderWith(
      <UpdateScope>
        <SettingsButton initiallyOpen inline />
      </UpdateScope>,
      { session },
    );
    await settle();
    expect(need("settings.updates.auto").getAttribute("aria-checked")).toBe("true");
    fireEvent.click(need("settings.updates.notify"));
    expect(need("settings.updates.notify").getAttribute("aria-checked")).toBe("true");
    await settle();
    expect(session.stores.appSettings.getSnapshot()?.updates).toBe("notify");
    expect(mock.calls.some((call) => call.method === "app_settings_set" && call.params.settings.updates === "notify")).toBe(true);
    expect(need("settings.updates.notify").getAttribute("aria-checked")).toBe("true");
  });

  it("calls setPolicy, check and download, and shows the version and the last check", () => {
    const { api, actions } = makeApi({ ...updateStates.available });
    renderWith(
      <StaticUpdateScope api={api}>
        <SettingsButton initiallyOpen inline />
      </StaticUpdateScope>,
    );
    fireEvent.click(need("settings.updates.off"));
    expect(actions.setPolicy).toHaveBeenCalledWith("off");
    fireEvent.click(need("update.download"));
    expect(actions.download).toHaveBeenCalledTimes(1);
    fireEvent.click(need("update.check"));
    expect(actions.check).toHaveBeenCalledTimes(1);
    expect(need("settings.updates.state").textContent).toBe("새 판 1.2.0 있음");
    expect(need("settings.updates.meta").textContent).toMatch(/^현재 판 0\.\d+\.\d+.* · 마지막 확인 /);
  });

  it("offers 받아서 설치 only when a version is available, and keeps a failure's reason behind 자세히", () => {
    for (const status of [updateStates.upToDate, updateStates.downloading, updateStates.readyOnExit, updateStates.portable]) {
      renderWith(
        <StaticUpdateScope api={makeApi(status).api}>
          <SettingsButton initiallyOpen inline />
        </StaticUpdateScope>,
      );
      expect(ui("update.download")).toBeNull();
      cleanup();
    }
    const portable = makeApi(updateStates.portable);
    renderWith(
      <StaticUpdateScope api={portable.api}>
        <SettingsButton initiallyOpen inline />
      </StaticUpdateScope>,
    );
    fireEvent.click(need("settings.updates.release"));
    expect(portable.actions.openRelease).toHaveBeenCalledWith("1.2.0");
    cleanup();
    renderWith(
      <StaticUpdateScope api={makeApi(updateStates.failed).api}>
        <SettingsButton initiallyOpen inline />
      </StaticUpdateScope>,
    );
    const reason = need("settings.updates.reason");
    expect(reason.tagName).toBe("DETAILS");
    expect(reason.hasAttribute("open")).toBe(false);
    expect(reason.textContent).toContain("Could not check for updates");
  });
});

describe("update ids in the frame", () => {
  it("puts the badge in the rail's footer, the settings in the settings popover and the notice over the frame", () => {
    const { api } = makeApi({ ...updateStates.readyOnExit, notice: updateStates.skippedNotice.notice });
    renderWith(<StaticUpdateScope api={api}>{frame}</StaticUpdateScope>);
    expect(document.querySelector('.app-rail__footer [data-ui="update.badge"]')).not.toBeNull();
    for (const id of ["update.badge", "update.restart", "update.notice", "update.notice.dismiss", "update.notice.release"]) need(id);
    fireEvent.click(need("update.notice.dismiss"));
    fireEvent.click(need("frame.settings"));
    for (const id of ["settings.updates", "settings.updates.auto", "settings.updates.notify", "settings.updates.off", "update.check"]) need(id);
    expect(screen.getByRole("radiogroup", { name: "업데이트" })).toBe(need("settings.updates"));
  });

  it("draws nothing of the updater without a session", () => {
    renderWith(frame);
    expect(ui("update.badge")).toBeNull();
    fireEvent.click(need("frame.settings"));
    expect(ui("settings.updates")).toBeNull();
  });

  it("reads the updater once for the frame's parts on the mock service", async () => {
    const { mock, session } = live(updateStates.portable);
    renderWith(frame, { session });
    await settle();
    expect(need("update.badge").dataset.state).toBe("portable");
    expect(mock.calls.filter((call) => call.method === "update_status")).toHaveLength(1);
    // The release page goes through the service, which opens the system browser.
    fireEvent.click(need("update.portable"));
    await settle();
    expect(mock.calls).toContainEqual({ method: "update_open_release", params: { version: "1.2.0" } });
  });
});

describe("release notes", () => {
  it("reads headings, bullets with continuation lines and paragraphs", () => {
    const notes = "판 설명입니다.\r\n\r\n### 더한 것\r\n\r\n- 첫째 `secs2r-cli` 줄\r\n  이어지는 줄\r\n- **둘째** [문서](docs/a.md)\r\n\r\n### 고친 것\r\n- 셋째\r\n뒤 문단";
    expect(parseNotes(notes)).toEqual([
      { kind: "paragraph", spans: [{ kind: "text", text: "판 설명입니다." }] },
      { kind: "heading", spans: [{ kind: "text", text: "더한 것" }] },
      {
        kind: "list",
        items: [
          [{ kind: "text", text: "첫째 " }, { kind: "code", text: "secs2r-cli" }, { kind: "text", text: " 줄 이어지는 줄" }],
          [{ kind: "strong", text: "둘째" }, { kind: "text", text: " 문서" }],
        ],
      },
      { kind: "heading", spans: [{ kind: "text", text: "고친 것" }] },
      { kind: "list", items: [[{ kind: "text", text: "셋째" }]] },
      { kind: "paragraph", spans: [{ kind: "text", text: "뒤 문단" }] },
    ]);
    expect(parseNotes("  \n")).toEqual([]);
  });

  it("keeps markup as text", () => {
    expect(parseSpans("<img src=x onerror=alert(1)> **굵게**")).toEqual([
      { kind: "text", text: "<img src=x onerror=alert(1)> " },
      { kind: "strong", text: "굵게" },
    ]);
    const { api } = makeApi({ ...updateStates.notice, notice: { ...updateStates.notice.notice!, notes: "- <b>굵지 않음</b>" } });
    renderWith(<UpdateNoticeHost api={api} />);
    const notes = need("update.notice.notes");
    expect(notes.querySelector("b")).toBeNull();
    expect(notes.querySelector("li")?.textContent).toBe("<b>굵지 않음</b>");
  });
});
