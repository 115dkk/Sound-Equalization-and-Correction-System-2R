// @vitest-environment happy-dom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { createMockService } from "../../ipc/mock";
import { SessionProvider } from "../../state/react";
import { createSession } from "../../state/session";
import { EXPORT_KINDS, MDAT_SCOPES, exportBlock } from "./types";
import { useExportScreen } from "./useExportScreen";

let mock: ReturnType<typeof createMockService>;
let session: ReturnType<typeof createSession>;
const wrapper = ({ children }: { children: ReactNode }) => <SessionProvider value={session}>{children}</SessionProvider>;
async function perform(work: () => unknown) {
  await act(async () => { await work(); await session.settled(); });
}
beforeEach(async () => {
  mock = createMockService({ tickMs: 0 }); session = createSession(mock); await session.start();
  mock.calls.length = 0;
});
afterEach(async () => { cleanup(); await session.monitor(false); session.dispose(); mock.dispose(); vi.restoreAllMocks(); });

describe("useExportScreen", () => {
  it("waits for settings and does no I/O for tutorial exports", async () => {
    const { result } = renderHook(useExportScreen, { wrapper });
    await perform(() => session.stores.project.set({ ...session.stores.project.getSnapshot()!, tutorial: true }));
    for (const kind of EXPORT_KINDS) {
      await perform(() => result.current.actions.exportFiles(kind));
      expect(result.current.view?.runs[kind]).toEqual({ status: "tutorial" });
    }
    expect(mock.calls).toEqual([]);
    await perform(() => session.stores.settings.set(null));
    expect(result.current.view).toBeNull();
  });

  it("exports all kinds through jobs and opens finished folders", async () => {
    await session.call({ method: "calibrate", params: { settings: null, subs_only: false } });
    mock.drain(); await session.stores.results.load();
    const { result } = renderHook(useExportScreen, { wrapper });
    for (const kind of EXPORT_KINDS) {
      if (kind === "filter_text") {
        // The sample result has subs: the service refuses the text before any
        // job starts (the screen keeps the button off; this is the backstop).
        mock.calls.length = 0;
        await perform(() => result.current.actions.exportFiles(kind));
        expect(session.errors.getSnapshot()?.error).toMatchObject({ code: "export_scope", params: { reason: "subs" } });
        expect(result.current.view?.runs[kind]).toBeUndefined();
        expect(result.current.view?.exporting).toBeNull();
        expect(mock.calls.some((call) => call.method === "open_folder")).toBe(false);
        continue;
      }
      await perform(() => result.current.actions.exportFiles(kind));
      expect(mock.calls).toContainEqual({ method: "export", params: { kind, folder: "", ...(kind === "before_after_mdat" ? { mdat_scope: "all" } : {}) } });
      expect(result.current.view?.exporting).toBe(kind);
      await perform(mock.drain);
      const run = result.current.view?.runs[kind];
      expect(run?.status).toBe("done");
      if (run?.status === "done") expect(mock.calls).toContainEqual({ method: "open_folder", params: { path: run.folder } });
      expect(result.current.view?.exporting).toBeNull();
    }
  });

  it("reads the latest scope and results when exporting and keeps the choice after calibration", async () => {
    const { result } = renderHook(useExportScreen, { wrapper });
    const actions = result.current.actions;
    expect(result.current.view?.mdatScope).toBe("all");
    await perform(() => actions.setMdatScope("results"));
    expect(result.current.view?.mdatScope).toBe("results");
    expect(session.host.getSnapshot().mdatScope).toBe("results");
    await perform(() => session.call({ method: "calibrate", params: { settings: null, subs_only: false } }));
    await perform(mock.drain);
    expect(result.current.view?.mdatScope).toBe("results");
    for (const scope of MDAT_SCOPES) {
      await perform(() => actions.setMdatScope(scope));
      expect(result.current.view?.mdatScope).toBe(scope);
      mock.calls.length = 0;
      await perform(() => actions.exportFiles("before_after_mdat"));
      expect(mock.calls).toContainEqual({ method: "export", params: { kind: "before_after_mdat", folder: "", mdat_scope: scope } });
      await perform(mock.drain);
      expect(result.current.view?.runs.before_after_mdat?.status).toBe("done");
    }
    await perform(() => session.call({ method: "calibrate", params: { settings: null, subs_only: false } }));
    await perform(mock.drain);
    expect(result.current.view?.runs).toEqual({});
    expect(result.current.view?.mdatScope).toBe("results");
  });

  it.each(MDAT_SCOPES)("sends measurements without a result when %s was chosen", async (scope) => {
    const { result } = renderHook(useExportScreen, { wrapper });
    expect(result.current.view?.results.calibration).toBeNull();
    await perform(() => result.current.actions.setMdatScope(scope));
    await perform(() => result.current.actions.exportFiles("before_after_mdat"));
    expect(mock.calls).toContainEqual({ method: "export", params: { kind: "before_after_mdat", folder: "", mdat_scope: "measurements" } });
    await perform(mock.drain);
    expect(result.current.view?.runs.before_after_mdat?.status).toBe("done");
    expect(result.current.view?.mdatScope).toBe(scope);
  });

  it.each(EXPORT_KINDS.filter((kind) => kind !== "before_after_mdat"))("never sends mdat_scope for %s", async (kind) => {
    const { result } = renderHook(useExportScreen, { wrapper });
    await perform(() => result.current.actions.setMdatScope("measurements"));
    await perform(() => result.current.actions.exportFiles(kind));
    expect(mock.calls.filter((call) => call.method === "export")).toEqual([{ method: "export", params: { kind, folder: "" } }]);
  });

  it("recalibrates a reopened stale cache and unblocks both mdat formats", async () => {
    await perform(() => session.call({ method: "calibrate", params: { settings: null, subs_only: false } }));
    await perform(mock.drain);
    await perform(() => session.call({ method: "calibration_settings_edit", params: { edit: { kind: "mode", mode: "linear" } } }));
    await perform(() => session.call({ method: "project_save", params: { path: "/projects/stale.secs" } }));
    await perform(() => session.call({ method: "project_open", params: { path: "/projects/stale.secs" } }));
    const { result } = renderHook(useExportScreen, { wrapper });
    const mdatKinds = ["staged_mdat", "before_after_mdat"] as const;
    expect(result.current.view?.results.calibration).toMatchObject({ cached: true, stale: true, mode: "mixed" });
    for (const kind of mdatKinds) {
      expect(exportBlock(result.current.view!, kind)).toBe("stale_cache");
      await perform(() => result.current.actions.exportFiles(kind));
      expect(session.errors.getSnapshot()?.error).toMatchObject({ code: "export_scope", params: { reason: "stale_cache" } });
      expect(result.current.view?.exporting).toBeNull();
    }
    expect(result.current.view?.recalibrating).toBe(false);

    mock.calls.length = 0;
    await perform(result.current.actions.recalibrate);
    expect(mock.calls).toContainEqual({ method: "calibrate", params: { settings: null, subs_only: false } });
    expect(result.current.view?.recalibrating).toBe(true);
    expect(result.current.view?.computing).toBe(true);
    for (const kind of mdatKinds) expect(exportBlock(result.current.view!, kind)).toBe("stale_cache");
    await perform(mock.drain);
    expect(result.current.view?.recalibrating).toBe(false);
    expect(result.current.view?.computing).toBe(false);
    expect(result.current.view?.results.calibration).toMatchObject({ cached: false, stale: false, mode: "linear" });
    for (const kind of mdatKinds) {
      expect(exportBlock(result.current.view!, kind)).toBeNull();
      await perform(() => result.current.actions.exportFiles(kind));
      expect(result.current.view?.exporting).toBe(kind);
      await perform(mock.drain);
      expect(result.current.view?.runs[kind]?.status).toBe("done");
    }
  });

  it.each(["failed", "cancelled"] as const)("clears recalibrating after a %s job without unblocking the stale cache", async (status) => {
    await perform(() => session.call({ method: "calibrate", params: { settings: null, subs_only: false } }));
    await perform(mock.drain);
    await perform(() => session.call({ method: "project_save", params: { path: "/projects/cached.secs" } }));
    await perform(() => session.call({ method: "project_open", params: { path: "/projects/cached.secs" } }));
    await perform(() => session.call({ method: "calibration_settings_edit", params: { edit: { kind: "mode", mode: "linear" } } }));
    const { result } = renderHook(useExportScreen, { wrapper });
    if (status === "failed") mock.failJobs("calibrate", "engine_input");
    await perform(result.current.actions.recalibrate);
    expect(result.current.view?.recalibrating).toBe(true);
    if (status === "cancelled") {
      const job = session.jobs.getSnapshot().jobs.find((job) => job.kind === "calibrate" && job.status === "running")!;
      await perform(() => session.call({ method: "job_cancel", params: { job: job.job } }));
      expect(result.current.view?.recalibrating).toBe(true);
    }
    await perform(mock.drain);
    expect(result.current.view?.recalibrating).toBe(false);
    expect(result.current.view?.results.calibration).toMatchObject({ cached: true, stale: true });
    expect(exportBlock(result.current.view!, "staged_mdat")).toBe("stale_cache");
    expect(exportBlock(result.current.view!, "before_after_mdat")).toBe("stale_cache");
  });

  it("reports failed exports without opening a folder", async () => {
    await session.call({ method: "calibrate", params: { settings: null, subs_only: false } }); mock.drain(); await session.stores.results.load();
    mock.calls.length = 0; mock.failJobs("export", "export_failed");
    const { result } = renderHook(useExportScreen, { wrapper });
    await perform(() => result.current.actions.exportFiles("camilla_dsp"));
    await perform(mock.drain);
    expect(result.current.view?.runs.camilla_dsp).toMatchObject({ status: "failed", error: { code: "export_failed" } });
    expect(mock.calls.some((call) => call.method === "open_folder")).toBe(false);
  });

  it("wires folder selection, cancellation, opening and navigation", async () => {
    const { result } = renderHook(useExportScreen, { wrapper });
    const original = session.stores.folder.getSnapshot()!.folder;
    mock.queueDialog("dialog_pick_folder", null);
    await perform(result.current.actions.changeFolder);
    expect(mock.calls).toContainEqual({ method: "dialog_pick_folder", params: { start: original } });
    expect(mock.calls.some((call) => call.method === "export_folder_set")).toBe(false);
    mock.queueDialog("dialog_pick_folder", "/exports");
    await perform(result.current.actions.changeFolder);
    expect(mock.calls).toContainEqual({ method: "export_folder_set", params: { folder: "/exports" } });
    expect(result.current.view?.folder?.folder).toBe("/exports");
    await perform(() => result.current.actions.openFolder("/exports"));
    expect(mock.calls).toContainEqual({ method: "open_folder", params: { path: "/exports" } });
    await perform(result.current.actions.goToCalibrate);
    expect(session.host.getSnapshot().current).toBe("calibrate");
  });
});
