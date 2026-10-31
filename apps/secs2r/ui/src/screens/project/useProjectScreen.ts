import { useEffect, useRef, useState } from "react";
import { RECENT_PROJECTS_MAX } from "../../ipc/bindings/rules";
import { IpcFailure } from "../../ipc/client";
import { useResource, useSession } from "../../state/react";
import type { ProjectDialog, ProjectDialogsActions, ProjectDialogsView, UnsavedReason } from "./types";

export function useProjectScreen(close: () => Promise<void>) {
  const session = useSession();
  const project = useResource("project");
  const settings = useResource("appSettings");
  const [view, setView] = useState<ProjectDialogsView>({ dialog: null, busy: false, error: null });
  const waiting = useRef<(() => Promise<void>) | null>(null);
  const previous = useRef<string | null>(null);
  useEffect(() => () => session.setSavePromptOpen(false), [session]);
  const show = (dialog: ProjectDialog | null) => {
    session.setSavePromptOpen(dialog?.kind === "unsaved");
    setView({ dialog, busy: false, error: null });
  };
  const perform = (work: () => Promise<void>) => {
    setView((v) => ({ ...v, busy: true, error: null }));
    void work().then(() => setView((v) => ({ ...v, busy: false }))).catch((error: unknown) => {
      setView((v) => ({ ...v, busy: false, error: error instanceof IpcFailure ? error.error : { code: "internal", params: {}, retryable: false, detail: String(error) } }));
    });
  };
  const guard = (reason: UnsavedReason, work: () => Promise<void>) => {
    if (session.hasPendingProjectEdits() || session.stores.project.getSnapshot()?.dirty) {
      waiting.current = work;
      show({ kind: "unsaved", reason, fileName: project?.name || null });
    } else perform(work);
  };
  async function remember(path: string) {
    if (!path) return;
    // Built on the latest settings when sent, so a pending edit is not undone (TD-68).
    await session.writers.app.edit((latest) => ({ recent_projects: [path, ...latest.recent_projects.filter((p) => p !== path)].slice(0, RECENT_PROJECTS_MAX) }));
  }
  async function save(as = false): Promise<boolean> {
    let path = as ? "" : session.stores.project.getSnapshot()?.path;
    if (!path) path = await session.call({ method: "dialog_save_project", params: { suggested: project?.path ?? "" } }) ?? "";
    if (!path) return false;
    await session.call({ method: "project_save", params: { path } });
    await remember(path); return true;
  }
  async function open(path: string) {
    await session.monitor(false);
    await session.call({ method: "project_open", params: { path } });
    await remember(path); session.navigate("system"); show(null);
  }
  async function tutorial() {
    previous.current = session.stores.project.getSnapshot()?.path || null;
    await session.startTutorial();
    session.navigate("system"); show(null);
  }
  const actions: ProjectDialogsActions = {
    createProject: (input) => perform(async () => {
      const path = await session.call({ method: "dialog_save_project", params: { suggested: "" } });
      if (!path) return;
      await session.monitor(false);
      await session.call({ method: "project_new", params: { path, preset: input.preset, sub_count: input.subCount, sample: input.sample, tutorial: false } });
      await remember(path); session.navigate("system"); show(null);
    }),
    saveAndContinue: () => perform(async () => {
      if (!await save()) return;
      const work = waiting.current; waiting.current = null;
      show(null); await work?.();
    }),
    discardAndContinue: () => perform(async () => { const work = waiting.current; waiting.current = null; show(null); await work?.(); }),
    cancel: () => { waiting.current = null; show(null); },
    startTutorial: () => guard("tutorial", tutorial),
    openRecent: (path) => guard("open", () => open(path)),
    browseProject: () => perform(async () => {
      const path = await session.call({ method: "dialog_open_project" });
      if (path) guard("open", () => open(path));
    }),
  };
  return {
    view, actions,
    menu: {
      onNew: () => guard("new", async () => show({ kind: "new" })),
      onOpen: () => show({ kind: "open", recent: settings?.recent_projects ?? [], current: project?.path || null }),
      onSave: () => perform(async () => { await save(); }),
      onSaveAs: () => perform(async () => { await save(true); }),
    },
    requestClose: () => guard("close", close),
    offerTutorial: () => show({ kind: "tutorial", dirty: project?.dirty ?? false }),
    finishTutorial: () => perform(async () => {
      if (previous.current) await open(previous.current);
      else {
        await session.call({ method: "project_new", params: { path: "", preset: "2.0", sub_count: 0, sample: false, tutorial: false } });
        session.navigate("system");
      }
    }),
  };
}
