// @vitest-environment happy-dom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { createMockService } from "../../ipc/mock";
import { SessionProvider } from "../../state/react";
import { createSession } from "../../state/session";
import { EXPORT_KINDS } from "./types";
import { useExportScreen } from "./useExportScreen";

let mock: ReturnType<typeof createMockService>;
let session: ReturnType<typeof createSession>;
let pending: Promise<unknown>[];
const wrapper = ({ children }: { children: ReactNode }) => <SessionProvider value={session}>{children}</SessionProvider>;
async function perform(work: () => unknown) {
  await act(async () => { await work(); while (pending.length) await Promise.all(pending.splice(0)); });
}
beforeEach(async () => {
  mock = createMockService({ tickMs: 0 }); session = createSession(mock); await session.start();
  pending = []; vi.spyOn(session, "run").mockImplementation((work) => { pending.push(work().catch(() => {})); });
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
      expect(mock.calls).toContainEqual({ method: "export", params: { kind, folder: "" } });
      expect(result.current.view?.exporting).toBe(kind);
      await perform(mock.drain);
      const run = result.current.view?.runs[kind];
      expect(run?.status).toBe("done");
      if (run?.status === "done") expect(mock.calls).toContainEqual({ method: "open_folder", params: { path: run.folder } });
      expect(result.current.view?.exporting).toBeNull();
    }
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
