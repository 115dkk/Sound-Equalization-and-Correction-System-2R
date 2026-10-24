// @vitest-environment happy-dom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { createElement, type ReactNode } from "react";
import { createMockService } from "../ipc/mock";
import { IpcFailure } from "../ipc/client";
import { SessionProvider } from "./react";
import { createSession } from "./session";
import { useAppSettingsLoad } from "./useAppSettingsLoad";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it("keeps a successful retry ready when an older settings load fails late", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  const wrapper = ({ children }: { children: ReactNode }) => createElement(SessionProvider, { value: session, children });
  try {
    const request = mock.request;
    let reject!: (error: unknown) => void;
    let delay = true;
    vi.spyOn(mock, "request").mockImplementation(async (params) => {
      if (params.method === "app_settings_get" && delay) {
        delay = false;
        await new Promise<void>((_resolve, fail) => { reject = fail; });
      }
      return request(params);
    });
    const { result } = renderHook(useAppSettingsLoad, { wrapper });
    let startup!: Promise<void>;
    await act(async () => { startup = session.start(); });
    await act(() => result.current.retry());
    const ready = result.current.state;
    expect(ready.status).toBe("ready");
    await act(async () => {
      reject(new IpcFailure({ code: "project_io", params: {}, retryable: false, detail: "late failure" }));
      await startup;
    });
    expect(result.current.state).toBe(ready);
    expect(session.errors.getSnapshot()).toBeNull();
  } finally { session.dispose(); mock.dispose(); }
});

it("exposes a typed app settings failure and becomes ready after retry", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  const wrapper = ({ children }: { children: ReactNode }) => createElement(SessionProvider, { value: session, children });
  try {
    mock.failNext("app_settings_get", "project_io", "settings unavailable");
    const { result } = renderHook(useAppSettingsLoad, { wrapper });
    expect(result.current.state.status).toBe("loading");
    await act(() => session.start());
    expect(result.current.state).toEqual({ status: "failed", error: { code: "project_io", params: {}, retryable: false, detail: "settings unavailable" } });
    const request = mock.request;
    let release!: () => void;
    vi.spyOn(mock, "request").mockImplementation(async (params) => {
      const value = await request(params);
      if (params.method === "app_settings_get") await new Promise<void>((resolve) => { release = resolve; });
      return value;
    });
    let retry!: Promise<void>;
    await act(async () => { retry = result.current.retry(); });
    expect(result.current.state.status).toBe("loading");
    await act(async () => { release(); await retry; });
    expect(result.current.state).toEqual({ status: "ready", value: session.stores.appSettings.getSnapshot() });
    expect(session.stores.appSettings.getSnapshot()).not.toBeNull();
    expect(mock.calls.filter((call) => call.method === "app_settings_get")).toHaveLength(2);
  } finally { session.dispose(); mock.dispose(); }
});

it("stays ready while loaded settings are read again, and when a later read fails", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  const wrapper = ({ children }: { children: ReactNode }) => createElement(SessionProvider, { value: session, children });
  try {
    const { result } = renderHook(useAppSettingsLoad, { wrapper });
    await act(() => session.start());
    expect(result.current.state.status).toBe("ready");
    const request = mock.request;
    let release!: () => void;
    const spy = vi.spyOn(mock, "request").mockImplementation(async (params) => {
      const value = await request(params);
      if (params.method === "app_settings_get") await new Promise<void>((resolve) => { release = resolve; });
      return value;
    });
    let reload!: Promise<void>;
    await act(async () => { reload = session.stores.appSettings.load(); });
    expect(result.current.state.status).toBe("ready");
    await act(async () => { release(); await reload; });
    expect(result.current.state.status).toBe("ready");
    spy.mockRestore();
    mock.failNext("app_settings_get", "project_io", "settings unavailable");
    await act(() => session.stores.appSettings.load());
    expect(result.current.state.status).toBe("ready");
    expect(session.errors.getSnapshot()?.error.code).toBe("project_io");
  } finally { session.dispose(); mock.dispose(); }
});
