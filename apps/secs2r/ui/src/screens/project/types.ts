// The dialogs behind the frame's project menu and closing (upstream
// window.py _confirm_close, open_trial_mode; __init__.py new_project,
// open_project). Upstream had no new-project button and no recent list.
import type { IpcError } from "../../ipc/bindings/IpcError";

/** Presets `project_new` accepts (secs-project PRESETS; an empty layout is not one). */
export const NEW_PROJECT_PRESETS = ["2.0", "2.1", "2.2", "4.0", "4.1", "4.2", "5.1", "7.1.4", "9.1.6"] as const;
export type NewProjectPreset = (typeof NEW_PROJECT_PRESETS)[number];

/** What waits behind the unsaved-changes question. */
export type UnsavedReason = "close" | "open" | "new" | "tutorial";

export type ProjectDialog =
  /** 새 프로젝트: preset and sub count, empty or the demo room, then the save location. */
  | { kind: "new"; initial?: { preset: NewProjectPreset; subCount: number } }
  /** 저장 안 한 변경: before closing, opening another project, creating one or starting the tutorial. `fileName` is null for a project never saved. */
  | { kind: "unsaved"; reason: UnsavedReason; fileName: string | null }
  /** 튜토리얼 시작: what the tutorial does, and its start. `dirty`: the unsaved question comes next. */
  | { kind: "tutorial"; dirty: boolean }
  /** 프로젝트 열기: recent projects (`app_settings_get().recent_projects`, newest first) and a file picker. */
  | { kind: "open"; recent: readonly string[]; current: string | null };

export type ProjectDialogsView = {
  dialog: ProjectDialog | null;
  /** A project call this dialog started is running (`project_new`, `project_save`, `project_open`). */
  busy: boolean;
  /** That call's error (project_io, …), shown in the dialog; null otherwise. */
  error: IpcError | null;
};

export type ProjectDialogsActions = {
  /** `dialog_save_project { suggested }`, then `project_new { path, preset, sub_count, sample }`; P33 adds the path to `recent_projects` (`app_settings_set`). */
  createProject: (input: { preset: NewProjectPreset; subCount: number; sample: boolean }) => void;
  /** `project_save { path: null }` (`dialog_save_project` first when the project has no path), then the waiting action. */
  saveAndContinue: () => void;
  /** The waiting action without saving (closing, opening, creating or starting the tutorial). */
  discardAndContinue: () => void;
  /** Closes the dialog; the waiting action is dropped. Esc and the close button do the same. */
  cancel: () => void;
  /** Host: the unsaved question when dirty, then the tutorial (a separate demo project, `project_new { sample: true }`, the tutorial accent). */
  startTutorial: () => void;
  /** `project_open { path }` (unsaved question first when dirty). */
  openRecent: (path: string) => void;
  /** `dialog_open_project`, then `project_open`. */
  browseProject: () => void;
};

/** "C:/a/b/거실.secs" → name "거실", folder "C:/a/b". */
export function splitProjectPath(path: string): { name: string; folder: string } {
  const at = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  const file = path.slice(at + 1);
  return { name: file.replace(/\.secs$/i, ""), folder: at > 0 ? path.slice(0, at) : "" };
}
