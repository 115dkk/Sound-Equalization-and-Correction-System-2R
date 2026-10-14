// The project dialogs, drawn in place.
import type { SpecimenDef } from "../../gallery/Specimen";
import type { IpcError } from "../../ipc/bindings/IpcError";
import { ProjectDialogs } from "./ProjectDialogs";
import type { ProjectDialogsActions, ProjectDialogsView } from "./types";

export const noopProjectActions: ProjectDialogsActions = {
  createProject: () => {},
  saveAndContinue: () => {},
  discardAndContinue: () => {},
  cancel: () => {},
  startTutorial: () => {},
  openRecent: () => {},
  browseProject: () => {},
};

const saveError: IpcError = { code: "project_io", params: {}, retryable: true, detail: "failed to write C:/Users/demo/Documents/SECS/거실 7.1.4.secs.tmp: The disk is full. (os error 112)" };

const views: Record<string, ProjectDialogsView> = {
  new: { dialog: { kind: "new", initial: { preset: "7.1.4", subCount: 2 } }, busy: false, error: null },
  unsaved: { dialog: { kind: "unsaved", reason: "close", fileName: "거실 7.1.4.secs" }, busy: false, error: null },
  unsavedTutorial: { dialog: { kind: "unsaved", reason: "tutorial", fileName: null }, busy: false, error: saveError },
  tutorial: { dialog: { kind: "tutorial", dirty: true }, busy: false, error: null },
  open: {
    dialog: {
      kind: "open",
      current: "C:/Users/demo/Documents/SECS/거실 7.1.4.secs",
      recent: ["C:/Users/demo/Documents/SECS/거실 7.1.4.secs", "D:/Audio/스튜디오 2.2.secs", "C:/Users/demo/Documents/SECS/측정 20261009-2140.secs"],
    },
    busy: false,
    error: null,
  },
};

const dialog = (key: string) => () => <ProjectDialogs view={views[key]!} actions={noopProjectActions} inline />;

export const projectSpecimens: SpecimenDef[] = [
  { id: "screen.project.new", title: "새 프로젝트 · preset, subs, empty or demo room", render: dialog("new") },
  { id: "screen.project.unsaved", title: "저장 안 한 변경 · before closing", render: dialog("unsaved") },
  { id: "screen.project.unsaved.tutorial", title: "저장 안 한 변경 · before the tutorial, never saved, save failed", render: dialog("unsavedTutorial") },
  { id: "screen.project.tutorial", title: "튜토리얼 시작 · with unsaved changes", render: dialog("tutorial") },
  { id: "screen.project.open", title: "프로젝트 열기 · recent projects", render: dialog("open") },
];
