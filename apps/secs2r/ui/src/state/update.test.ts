// @vitest-environment happy-dom
import { act, cleanup, renderHook } from "@testing-library/react";
import { createElement, StrictMode, type ReactNode } from "react";
import { afterEach, expect, it, vi } from "vitest";
import { createMockService, updateStates } from "../ipc/mock";
import { SessionProvider } from "./react";
import { createSession } from "./session";
import { useUpdate } from "./update";

const disposals: (() => void)[] = [];
afterEach(() => { cleanup(); for (const dispose of disposals.splice(0)) dispose(); vi.useRealTimers(); vi.restoreAllMocks(); });

function setup(strict = false) {
  vi.useFakeTimers();
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  disposals.push(() => { session.dispose(); mock.dispose(); });
  const wrapper = ({ children }: { children: ReactNode }) => {
    const provider = createElement(SessionProvider, { value: session, children });
    return strict ? createElement(StrictMode, null, provider) : provider;
  };
  return { mock, session, wrapper };
}

it("polls each minute, each second while busy, and stops after unmount", async () => {
  const { mock, wrapper } = setup();
  const { result, unmount } = renderHook(useUpdate, { wrapper });
  await act(async () => {});
  const reads = () => mock.calls.filter((call) => call.method === "update_status").length;
  expect(result.current.status?.phase.kind).toBe("up_to_date");
  const initial = reads();
  await act(() => vi.advanceTimersByTimeAsync(59_999));
  expect(reads()).toBe(initial);
  await act(() => vi.advanceTimersByTimeAsync(1));
  expect(reads()).toBe(initial + 1);
  await act(() => result.current.check());
  expect(result.current.status?.phase.kind).toBe("checking");
  mock.setUpdateStatus(updateStates.downloading);
  await act(() => vi.advanceTimersByTimeAsync(1_000));
  expect(result.current.status?.phase).toMatchObject({ kind: "downloading", received: 40, total: 100 });
  mock.setUpdateStatus(updateStates.readyOnExit);
  await act(() => vi.advanceTimersByTimeAsync(1_000));
  expect(result.current.status?.phase.kind).toBe("ready");
  const ready = reads();
  await act(() => vi.advanceTimersByTimeAsync(59_999));
  expect(reads()).toBe(ready);
  await act(() => vi.advanceTimersByTimeAsync(1));
  expect(reads()).toBe(ready + 1);
  unmount();
  await act(() => vi.advanceTimersByTimeAsync(120_000));
  expect(reads()).toBe(ready + 1);
});

it("downloads, restarts, dismisses and saves policies through the ordered settings writer", async () => {
  const { mock, session, wrapper } = setup();
  mock.setUpdateStatus(updateStates.available);
  const { result } = renderHook(useUpdate, { wrapper });
  await act(async () => {});
  await act(() => result.current.download());
  expect(result.current.status?.phase.kind).toBe("downloading");
  mock.setUpdateStatus(updateStates.readyOnRequest);
  await act(() => vi.advanceTimersByTimeAsync(1_000));
  await act(() => result.current.restart());
  expect(result.current.status?.phase.kind).toBe("installing");
  mock.setUpdateStatus(updateStates.skippedNotice);
  await act(() => vi.advanceTimersByTimeAsync(60_000));
  expect(result.current.status?.notice?.skipped).toBe(true);
  await act(() => result.current.dismiss());
  expect(result.current.status?.notice).toBeNull();
  await act(async () => { expect(await result.current.setPolicy("off")).toBe(true); });
  expect(session.stores.appSettings.getSnapshot()?.updates).toBe("off");
  expect(result.current.status).toMatchObject({ policy: "off", phase: { kind: "off" } });
  await act(() => result.current.check());
  expect(result.current.status).toMatchObject({ policy: "off", phase: { kind: "checking" } });
  // The writer reads the latest complete bundle rather than replacing unrelated settings.
  const latest = await mock.request({ method: "app_settings_get" });
  await mock.request({ method: "app_settings_set", params: { settings: { ...latest, language: "en" } } });
  await act(async () => { expect(await result.current.setPolicy("notify")).toBe(true); });
  expect(session.stores.appSettings.getSnapshot()).toMatchObject({ updates: "notify", language: "en" });
});

it("reports typed action and polling errors and keeps retrying", async () => {
  const { mock, session, wrapper } = setup();
  const { result } = renderHook(useUpdate, { wrapper });
  await act(async () => {});
  await act(() => result.current.download());
  expect(session.errors.getSnapshot()?.error.code).toBe("invalid_request");
  mock.failNext("update_status", "internal", "unavailable");
  await act(() => vi.advanceTimersByTimeAsync(60_000));
  expect(session.errors.getSnapshot()?.error.code).toBe("internal");
  mock.setUpdateStatus(updateStates.portable);
  await act(() => vi.advanceTimersByTimeAsync(60_000));
  expect(result.current.status?.phase.kind).toBe("portable");
  mock.failNext("app_settings_set", "project_io", "cannot save");
  await act(async () => { expect(await result.current.setPolicy("off")).toBe(false); });
  expect(session.errors.getSnapshot()?.error.code).toBe("project_io");
  expect((await mock.request({ method: "app_settings_get" })).updates).toBe("auto");
});

it("ignores a stale status reply after a check action and survives StrictMode remounts", async () => {
  const { mock, wrapper } = setup(true);
  const { result } = renderHook(useUpdate, { wrapper });
  await act(async () => {});
  expect(result.current.status?.phase.kind).toBe("up_to_date");
  const request = mock.request;
  let release!: () => void;
  let delay = true;
  vi.spyOn(mock, "request").mockImplementation(async (params) => {
    const reply = await request(params);
    if (params.method === "update_status" && delay) {
      delay = false;
      await new Promise<void>((resolve) => { release = resolve; });
    }
    return reply;
  });
  await act(() => vi.advanceTimersByTimeAsync(60_000));
  await act(() => result.current.check());
  expect(result.current.status?.phase.kind).toBe("checking");
  await act(async () => { release(); });
  expect(result.current.status?.phase.kind).toBe("checking");
});
