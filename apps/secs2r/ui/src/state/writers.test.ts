// @vitest-environment happy-dom
// One ordered writer per settings bundle (A-80, TD-68): the writer on its
// own, then each bundle the session owns, through the session and the mock.
import { act, cleanup, renderHook } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { afterEach, describe, expect, it } from "vitest";
import type { Request } from "../ipc/bindings/Request";
import { createMockService } from "../ipc/mock";
import { useProjectScreen } from "../screens/project/useProjectScreen";
import { useSystemScreen } from "../screens/system/useSystemScreen";
import { SessionProvider } from "./react";
import { createSession, type Session } from "./session";
import { createWriter } from "./writers";

afterEach(cleanup);

function deferred<T = void>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

/** A session over the mock whose first `method` request the mock stores at
 * once but whose answer waits for `release()`; `sent` keeps every such request. */
function slowAnswer<M extends Request["method"]>(method: M) {
  const mock = createMockService({ tickMs: 0 });
  const reached = deferred();
  const answer = deferred();
  const sent: Extract<Request, { method: M }>[] = [];
  const session = createSession({ ...mock, request: async (request) => {
    const data = await mock.request(request);
    if ((request.method as Request["method"]) === method) {
      sent.push(request as unknown as Extract<Request, { method: M }>);
      if (sent.length === 1) { reached.resolve(); await answer.promise; }
    }
    return data;
  } });
  return { mock, session, sent, reached: reached.promise, release: () => answer.resolve() };
}

const provider = (session: Session) => ({ children }: { children: ReactNode }) => createElement(SessionProvider, { value: session, children });

describe("ordered writer", () => {
  it("lands two quick edits to different fields", async () => {
    let latest = { theme: "system", compute: "auto" };
    const writer = createWriter({ latest: () => latest, write: async (next) => { latest = next; return latest; } });
    const compute = writer.edit({ compute: "cpu" });
    const theme = writer.edit({ theme: "dark" });
    expect(await Promise.all([compute, theme])).toEqual([true, true]);
    expect(latest).toEqual({ theme: "dark", compute: "cpu" });
  });

  it("builds an edit queued behind a slow one from the slow one's answer", async () => {
    let latest = { theme: "system", recent: ["old"] };
    const entered = deferred();
    const answer = deferred();
    const sent: (typeof latest)[] = [];
    const writer = createWriter({ latest: () => latest, write: async (next) => {
      sent.push(next);
      if (sent.length === 1) {
        entered.resolve(); await answer.promise;
        // The service answers with its own form of what was sent.
        latest = { ...next, recent: ["normalized"] };
      } else latest = next;
      return latest;
    } });
    const first = writer.edit({ theme: "dark" });
    await entered.promise;
    const second = writer.edit((current) => ({ recent: ["new", ...current.recent] }));
    answer.resolve();
    expect(await Promise.all([first, second])).toEqual([true, true]);
    expect(sent).toEqual([{ theme: "dark", recent: ["old"] }, { theme: "dark", recent: ["new", "normalized"] }]);
  });

  it("returns false for a failed edit and still runs the next on the last accepted answer", async () => {
    let latest = { theme: "system", compute: "auto" };
    let fail = true;
    const writer = createWriter({ latest: () => latest, write: async (next) => {
      if (fail) { fail = false; throw new Error("save failed"); }
      latest = next; return latest;
    } });
    const failed = writer.edit({ compute: "gpu" });
    const next = writer.edit({ theme: "dark" });
    expect(await Promise.all([failed, next])).toEqual([false, true]);
    expect(latest).toEqual({ theme: "dark", compute: "auto" });
  });

  it("returns false without writing before the bundle has loaded", async () => {
    let writes = 0;
    const writer = createWriter<{ theme: string }>({ latest: () => null, write: async (next) => { writes++; return next; } });
    expect(await writer.edit({ theme: "dark" })).toBe(false);
    expect(writes).toBe(0);
  });
});

describe("session writers", () => {
  it("app: keeps the compute choice when the theme changes while its save is unanswered (TD-68)", async () => {
    const { mock, session, sent, reached, release } = slowAnswer("app_settings_set");
    try {
      await session.start(); await session.settled();
      const compute = session.writers.app.edit({ compute: "cpu" });
      await reached;
      const theme = session.writers.app.edit({ theme: "dark" });
      release();
      expect(await Promise.all([compute, theme])).toEqual([true, true]);
      await session.settled();
      // The theme went out after the compute answer and was built on it.
      expect(sent.map((request) => request.params.settings)).toMatchObject([{ compute: "cpu", theme: "system" }, { compute: "cpu", theme: "dark" }]);
      expect(await mock.request({ method: "app_settings_get" })).toMatchObject({ compute: "cpu", theme: "dark" });
      expect(session.stores.appSettings.getSnapshot()).toMatchObject({ compute: "cpu", theme: "dark" });
    } finally { release(); session.dispose(); await session.settled(); mock.dispose(); }
  });

  it("app: reports a failed edit once and builds the next on the last accepted settings", async () => {
    const mock = createMockService({ tickMs: 0 });
    const session = createSession(mock);
    try {
      await session.start(); await session.settled();
      const failures: string[] = [];
      const off = session.errors.subscribe(() => {
        const failure = session.errors.getSnapshot();
        if (failure) failures.push(failure.error.code);
      });
      mock.failNext("app_settings_set", "project_io");
      const failed = session.writers.app.edit({ compute: "cpu" });
      const next = session.writers.app.edit({ theme: "dark" });
      await session.settled(); off();
      expect(await Promise.all([failed, next])).toEqual([false, true]);
      expect(failures).toEqual(["project_io"]);
      expect(session.stores.appSettings.getSnapshot()).toMatchObject({ compute: "auto", theme: "dark" });
    } finally { session.dispose(); await session.settled(); mock.dispose(); }
  });

  it("app: remembers at most ten recent projects, the service's bound, on top of the latest settings", async () => {
    const mock = createMockService({ tickMs: 0 });
    const remembered = deferred();
    const session = createSession({ ...mock, request: async (request) => {
      const data = await mock.request(request);
      const sent = request as Request;
      if (sent.method === "app_settings_set" && sent.params.settings.recent_projects[0] === "C:/rooms/new.secs") remembered.resolve();
      return data;
    } });
    const old = Array.from({ length: 10 }, (_, i) => `C:/rooms/old-${i}.secs`);
    await session.start(); await session.settled();
    const hook = renderHook(() => useProjectScreen(async () => {}), { wrapper: provider(session) });
    try {
      expect(await session.writers.app.edit({ compute: "cpu", recent_projects: old })).toBe(true);
      mock.queueDialog("dialog_save_project", "C:/rooms/new.secs");
      await act(async () => { hook.result.current.menu.onSaveAs(); await remembered.promise; await session.settled(); });
      const sent = mock.calls.filter((request): request is Extract<Request, { method: "app_settings_set" }> => request.method === "app_settings_set").at(-1);
      expect(sent?.params.settings).toMatchObject({ compute: "cpu", recent_projects: ["C:/rooms/new.secs", ...old.slice(0, 9)] });
      expect(session.stores.appSettings.getSnapshot()?.recent_projects).toEqual(["C:/rooms/new.secs", ...old.slice(0, 9)]);
    } finally { hook.unmount(); session.dispose(); await session.settled(); mock.dispose(); }
  });

  // Moved from useSystemScreen.test.tsx: the screen's controls submit this
  // render's full fields, so the second edit carries the first one's old value.
  it("dsp: rebases rapid field edits after a delayed full replacement", async () => {
    const { mock, session, sent, reached, release } = slowAnswer("dsp_set");
    await session.start(); await session.settled();
    const hook = renderHook(useSystemScreen, { wrapper: provider(session) });
    try {
      const actions = hook.result.current.actions;
      const fields = hook.result.current.view!.dsp.fields;
      act(() => {
        actions.setDspFields({ ...fields, fir_taps: fields.fir_taps + 1 });
        actions.setDspFields({ ...fields, fir_budget: fields.fir_budget + 1 });
      });
      await reached;
      release();
      await act(async () => { await session.settled(); });
      expect(sent).toHaveLength(2);
      expect(session.stores.dsp.getSnapshot()?.fields).toEqual({ ...fields, fir_taps: fields.fir_taps + 1, fir_budget: fields.fir_budget + 1 });
    } finally { release(); hook.unmount(); session.dispose(); await session.settled(); mock.dispose(); }
  });

  it("dsp: builds a field edit queued behind a preset from that preset's fields", async () => {
    const { mock, session, sent, reached, release } = slowAnswer("dsp_set");
    try {
      await session.start(); await session.settled();
      const dsp = session.stores.dsp.getSnapshot()!;
      const preset = dsp.presets.find((candidate) => candidate.key !== dsp.profile && candidate.key !== "custom")!;
      const selected = session.writers.dsp.edit({ preset: preset.key });
      await reached;
      const patched = session.writers.dsp.edit({ iir_bands: preset.fields.iir_bands + 1 });
      release();
      expect(await Promise.all([selected, patched])).toEqual([true, true]);
      await session.settled();
      const edited = { ...preset.fields, iir_bands: preset.fields.iir_bands + 1 };
      expect(sent.map((request) => request.params)).toEqual([{ preset: preset.key, fields: null }, { preset: null, fields: edited }]);
      // A manual edit makes the profile custom, and a custom profile chooses its output rate (ADR 0017).
      expect(session.stores.dsp.getSnapshot()).toMatchObject({ profile: "custom", fields: { ...edited, output_rate_editable: true } });
    } finally { release(); session.dispose(); await session.settled(); mock.dispose(); }
  });

  it("dsp: counts a queued edit as a pending project edit and drops it when the project is replaced", async () => {
    const { mock, session, sent, reached, release } = slowAnswer("dsp_set");
    try {
      await session.start(); await session.settled();
      const first = session.writers.dsp.edit({ fir_taps: 4096 });
      expect(session.hasPendingProjectEdits()).toBe(true);
      await reached;
      const queued = session.writers.dsp.edit({ fir_budget: 12345 });
      await session.call({ method: "project_new", params: { path: "", preset: "2.0", sub_count: 0, sample: false, tutorial: false } });
      const replacement = session.stores.dsp.getSnapshot();
      release();
      expect(await Promise.all([first, queued])).toEqual([true, false]);
      await session.settled();
      expect(sent).toHaveLength(1);
      expect(session.stores.dsp.getSnapshot()).toEqual(replacement);
      expect(session.hasPendingProjectEdits()).toBe(false);
    } finally { release(); session.dispose(); await session.settled(); mock.dispose(); }
  });

  it("layout: sends start delay and timing edits in order, each with only its own field", async () => {
    const { mock, session, sent, reached, release } = slowAnswer("layout_set");
    try {
      await session.start(); await session.settled();
      const { count, timing } = session.stores.layout.getSnapshot()!;
      const reference = { ...timing, output: timing.output + 1 };
      const delay = session.writers.layout.edit({ start_delay: "10 초" });
      await reached;
      const moved = session.writers.layout.edit({ timing: reference });
      release();
      expect(await Promise.all([delay, moved])).toEqual([true, true]);
      await session.settled();
      expect(sent.map((request) => request.params)).toEqual([
        { count: null, start_delay: "10 초", timing: null, confirm: false },
        { count: null, start_delay: null, timing: reference, confirm: false },
      ]);
      expect(session.stores.layout.getSnapshot()).toMatchObject({ count, start_delay: "10 초", timing: reference });
    } finally { release(); session.dispose(); await session.settled(); mock.dispose(); }
  });

  it("measurement: builds a sweep edit queued behind a method change on the method the service accepted", async () => {
    const { mock, session, sent, reached, release } = slowAnswer("measure_configure");
    try {
      await session.start(); await session.settled();
      const method = session.writers.measurement.edit({ method: "native" });
      await reached;
      const sweep = session.writers.measurement.edit({ sweep_length: 131072 });
      release();
      expect(await Promise.all([method, sweep])).toEqual([true, true]);
      await session.settled();
      expect(sent.map((request) => request.params)).toMatchObject([
        { method: "native", sweep_length: null, input_device: null },
        { method: "native", sweep_length: 131072, input_device: null, timing: null },
      ]);
      expect(session.stores.measurementSettings.getSnapshot()).toMatchObject({ method: "native", sweep_length: 131072 });
    } finally { release(); session.dispose(); await session.settled(); mock.dispose(); }
  });
});
