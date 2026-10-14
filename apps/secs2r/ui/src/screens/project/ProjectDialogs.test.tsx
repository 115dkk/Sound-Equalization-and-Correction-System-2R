// @vitest-environment happy-dom
import { fireEvent, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { renderWith } from "../../test/render";
import { ProjectDialogs } from "./ProjectDialogs";
import { splitProjectPath, type ProjectDialog, type ProjectDialogsActions, type ProjectDialogsView, type UnsavedReason } from "./types";

/** Every id docs/M5-reach.md cites for the project dialogs. */
const REACH_IDS = [
  "project.new",
  "project.new.start",
  "project.new.preset",
  "project.new.subs",
  "project.new.create",
  "project.unsaved",
  "project.unsaved.save",
  "project.unsaved.discard",
  "project.unsaved.cancel",
  "project.tutorial",
  "project.tutorial.start",
  "project.open",
  "project.open.recent.1",
  "project.open.browse",
] as const;

function actions() {
  return {
    createProject: vi.fn<ProjectDialogsActions["createProject"]>(),
    saveAndContinue: vi.fn<ProjectDialogsActions["saveAndContinue"]>(),
    discardAndContinue: vi.fn<ProjectDialogsActions["discardAndContinue"]>(),
    cancel: vi.fn<ProjectDialogsActions["cancel"]>(),
    startTutorial: vi.fn<ProjectDialogsActions["startTutorial"]>(),
    openRecent: vi.fn<ProjectDialogsActions["openRecent"]>(),
    browseProject: vi.fn<ProjectDialogsActions["browseProject"]>(),
  };
}

const view = (dialog: ProjectDialog, extra: Partial<ProjectDialogsView> = {}): ProjectDialogsView => ({ dialog, busy: false, error: null, ...extra });
const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);

const DIALOGS: ProjectDialog[] = [
  { kind: "new" },
  ...(["close", "open", "new", "tutorial"] as UnsavedReason[]).map((reason): ProjectDialog => ({ kind: "unsaved", reason, fileName: "거실.secs" })),
  { kind: "unsaved", reason: "close", fileName: null },
  { kind: "tutorial", dirty: false },
  { kind: "tutorial", dirty: true },
  { kind: "open", recent: ["C:/a/거실.secs", "D:/b/스튜디오.secs"], current: "C:/a/거실.secs" },
  { kind: "open", recent: [], current: null },
];

describe("프로젝트 대화상자", () => {
  it("renders every dialog in both languages, and nothing when none is open", () => {
    for (const locale of ["ko", "en"] as const) {
      for (const dialog of DIALOGS) {
        const { unmount } = renderWith(<ProjectDialogs view={view(dialog)} actions={actions()} />, { locale });
        expect(document.querySelector('[role="dialog"]'), `${locale} ${dialog.kind}`).not.toBeNull();
        unmount();
      }
    }
    renderWith(<ProjectDialogs view={{ dialog: null, busy: false, error: null }} actions={actions()} />);
    expect(document.querySelector('[role="dialog"]')).toBeNull();
  });

  it("creates an empty project from the chosen preset and sub count", () => {
    const act = actions();
    renderWith(<ProjectDialogs view={view({ kind: "new" })} actions={act} />);
    fireEvent.click(ui("project.new.preset.5.1")!);
    fireEvent.keyDown(ui("project.new.subs")!, { key: "ArrowUp" });
    fireEvent.click(ui("project.new.create")!);
    expect(act.createProject).toHaveBeenCalledWith({ preset: "5.1", subCount: 2, sample: false });
  });

  it("creates the demo room with its fixed setup", () => {
    const act = actions();
    renderWith(<ProjectDialogs view={view({ kind: "new", initial: { preset: "2.0", subCount: 0 } })} actions={act} />);
    fireEvent.click(ui("project.new.start.sample")!);
    expect((ui("project.new.preset.2.0") as HTMLButtonElement).disabled).toBe(true);
    expect(ui("project.new.subs")?.getAttribute("aria-valuenow")).toBe("3");
    fireEvent.click(ui("project.new.create")!);
    expect(act.createProject).toHaveBeenCalledWith({ preset: "7.1.4", subCount: 3, sample: true });
  });

  it("names the waiting action in the title and the buttons, and routes save, don't save and cancel", () => {
    const act = actions();
    renderWith(<ProjectDialogs view={view({ kind: "unsaved", reason: "open", fileName: "거실 7.1.4.secs" })} actions={act} />);
    expect(screen.getByText("다른 프로젝트를 열기 전에 저장")).toBeTruthy();
    expect(ui("project.unsaved.save")?.textContent).toBe("저장하고 열기");
    expect(ui("project.unsaved.discard")?.textContent).toBe("저장 안 하고 열기");
    fireEvent.click(ui("project.unsaved.save")!);
    fireEvent.click(ui("project.unsaved.discard")!);
    fireEvent.click(ui("project.unsaved.cancel")!);
    fireEvent.keyDown(ui("project.unsaved")!, { key: "Escape" });
    expect(act.saveAndContinue).toHaveBeenCalledTimes(1);
    expect(act.discardAndContinue).toHaveBeenCalledTimes(1);
    expect(act.cancel).toHaveBeenCalledTimes(2);
  });

  it("says what the tutorial does and starts it", () => {
    const act = actions();
    renderWith(<ProjectDialogs view={view({ kind: "tutorial", dirty: true })} actions={act} />);
    expect(screen.getByText(/프로젝트 저장과 내보내기가 되지 않습니다/)).toBeTruthy();
    expect(screen.getByText(/저장할지 고르는 창이 열립니다/)).toBeTruthy();
    fireEvent.click(ui("project.tutorial.start")!);
    expect(act.startTutorial).toHaveBeenCalledTimes(1);
  });

  it("opens a recent project or browses; the open one is marked and not offered", () => {
    const act = actions();
    renderWith(<ProjectDialogs view={view({ kind: "open", recent: ["C:/a/거실.secs", "D:/b/스튜디오.secs"], current: "C:/a/거실.secs" })} actions={act} />);
    expect((ui("project.open.recent.1") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(ui("project.open.recent.2")!);
    fireEvent.click(ui("project.open.browse")!);
    expect(act.openRecent).toHaveBeenCalledWith("D:/b/스튜디오.secs");
    expect(act.browseProject).toHaveBeenCalledTimes(1);
    expect(splitProjectPath("D:\\b\\스튜디오.secs")).toEqual({ name: "스튜디오", folder: "D:\\b" });
  });

  it("shows a failed save with its cause and next step", () => {
    renderWith(
      <ProjectDialogs
        view={view({ kind: "unsaved", reason: "close", fileName: null }, { error: { code: "project_io", params: {}, retryable: true, detail: "disk full" } })}
        actions={actions()}
      />,
    );
    expect(screen.getByText("프로젝트 파일을 읽거나 쓰지 못했습니다.")).toBeTruthy();
    expect(screen.getByText(/저장 위치를 고르는 창이 먼저 열립니다/)).toBeTruthy();
  });

  it("draws every id the reach table cites", () => {
    const seen = new Set<string>();
    for (const dialog of DIALOGS) {
      const { unmount } = renderWith(<ProjectDialogs view={view(dialog)} actions={actions()} />);
      for (const node of document.querySelectorAll("[data-ui]")) seen.add(node.getAttribute("data-ui") ?? "");
      unmount();
    }
    expect(REACH_IDS.filter((id) => !seen.has(id))).toEqual([]);
  });
});
