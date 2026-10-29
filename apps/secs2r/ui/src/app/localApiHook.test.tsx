// @vitest-environment happy-dom
import { act, cleanup, renderHook, screen, waitFor } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { afterEach, expect, it, vi } from "vitest";
import { createMockService } from "../ipc/mock";
import { createSession, type Session } from "../state/session";
import { SessionProvider } from "../state/react";
import { PrefsScope } from "./prefs";
import { localApiState, useLocalApi } from "./localApi";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });
const wrapper = (session?: Session) => ({ children }: { children: ReactNode }) => createElement(PrefsScope, {
  initial: { locale: "ko", theme: "light", accent: "default" },
  children: session ? createElement(SessionProvider, { value: session, children }) : children,
});

it("keeps gallery usage off and inert without a session", async () => {
  const copy = vi.spyOn(navigator.clipboard, "writeText");
  const { result } = renderHook(() => useLocalApi(true), { wrapper: wrapper() });
  await act(async () => {
    result.current.actions.setEnabled(true);
    result.current.actions.setPort(5000);
    result.current.actions.regenerateToken();
    result.current.actions.copy("unused");
  });
  expect(result.current.view).toMatchObject({ status: { enabled: false, running: false, port: 4712 }, pending: null, error: null });
  expect(copy).not.toHaveBeenCalled();
});

it("reads on opening and reopening, and follows refreshed app settings while open", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  const hook = renderHook(({ open }) => useLocalApi(open), { initialProps: { open: false }, wrapper: wrapper(session) });
  try {
    expect(mock.calls).toEqual([]);
    hook.rerender({ open: true });
    await waitFor(() => expect(hook.result.current.view.status?.port).toBe(4712));
    expect(mock.calls.filter((r) => r.method === "local_api_status")).toHaveLength(1);
    hook.rerender({ open: false });
    await mock.request({ method: "app_settings_set", params: { settings: { ...await mock.request({ method: "app_settings_get" }), local_api: { enabled: true, port: 5000 } } } });
    hook.rerender({ open: true });
    await waitFor(() => expect(hook.result.current.view.status).toMatchObject({ running: true, port: 5000 }));
    await mock.request({ method: "app_settings_set", params: { settings: { ...await mock.request({ method: "app_settings_get" }), local_api: { enabled: false, port: 5001 } } } });
    await act(() => session.stores.appSettings.load());
    await waitFor(() => expect(hook.result.current.view.status).toMatchObject({ running: false, port: 5001 }));
  } finally { hook.unmount(); mock.dispose(); }
});

it("saves only local_api over current service settings through the shared writer", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  await session.stores.appSettings.load();
  const hook = renderHook(() => useLocalApi(true), { wrapper: wrapper(session) });
  try {
    await waitFor(() => expect(hook.result.current.view.status).not.toBeNull());
    const external = { ...await mock.request({ method: "app_settings_get" }), theme: "dark" as const, compute: "cpu" as const, recent_projects: ["/external.secs"], local_api: { enabled: false, port: 5000 } };
    await mock.request({ method: "app_settings_set", params: { settings: external } });
    await act(async () => { hook.result.current.actions.setEnabled(true); await session.settled(); });
    await waitFor(() => expect(hook.result.current.view).toMatchObject({ status: { enabled: true, running: true, port: 5000 }, pending: null }));
    expect(await mock.request({ method: "app_settings_get" })).toEqual({ ...external, local_api: { enabled: true, port: 5000 } });
    await act(async () => { hook.result.current.actions.setPort(5001); await session.settled(); });
    await waitFor(() => expect(hook.result.current.view.status?.url).toBe("http://127.0.0.1:5001"));
    expect(await mock.request({ method: "app_settings_get" })).toEqual({ ...external, local_api: { enabled: true, port: 5001 } });
  } finally { hook.unmount(); mock.dispose(); }
});

it("keeps the saved value while pending, drops concurrent actions and preserves it on failure", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  await session.stores.appSettings.load();
  const hook = renderHook(({ open }) => useLocalApi(open), { initialProps: { open: true }, wrapper: wrapper(session) });
  const request = mock.request;
  let release!: () => void;
  const held = new Promise<void>((resolve) => { release = resolve; });
  vi.spyOn(mock, "request").mockImplementation(async (params) => {
    if (params.method === "app_settings_set") await held;
    return request(params);
  });
  try {
    await waitFor(() => expect(hook.result.current.view.status).not.toBeNull());
    mock.failNext("app_settings_set", "project_io", "save refused");
    await act(async () => {
      hook.result.current.actions.setEnabled(true);
      hook.result.current.actions.setPort(5000);
      hook.result.current.actions.regenerateToken();
    });
    expect(hook.result.current.view).toMatchObject({ status: { enabled: false }, pending: "toggle", error: null });
    // Closing and reopening during the save must not strand pending or discard its answer.
    hook.rerender({ open: false }); hook.rerender({ open: true });
    await act(async () => { release(); await session.settled(); });
    await waitFor(() => expect(hook.result.current.view).toMatchObject({ status: { enabled: false, port: 4712 }, pending: null, error: { code: "project_io" } }));
    expect(mock.calls.filter((r) => r.method === "app_settings_set")).toHaveLength(1);
    expect(mock.calls.some((r) => r.method === "local_api_token_regenerate")).toBe(false);
    await act(async () => { hook.result.current.actions.setEnabled(true); await session.settled(); });
    await waitFor(() => expect(hook.result.current.view).toMatchObject({ status: { running: true }, pending: null, error: null }));
  } finally { release(); hook.unmount(); mock.dispose(); }
});

it("reports status and token failures, retries on reopening and shows the returned token status", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  mock.failNext("local_api_status", "internal", "status failed");
  const hook = renderHook(({ open }) => useLocalApi(open), { initialProps: { open: true }, wrapper: wrapper(session) });
  try {
    await waitFor(() => expect(hook.result.current.view.error?.code).toBe("internal"));
    expect(localApiState(hook.result.current.view)).toEqual({ kind: "unreadable" });
    hook.rerender({ open: false }); hook.rerender({ open: true });
    await waitFor(() => expect(hook.result.current.view.status?.token).toBe("mock-local-api-token"));
    const previous = hook.result.current.view.status;
    mock.failNext("local_api_token_regenerate", "project_io", "token failed");
    await act(async () => { hook.result.current.actions.regenerateToken(); await session.settled(); });
    await waitFor(() => expect(hook.result.current.view.error?.code).toBe("project_io"));
    expect(hook.result.current.view.status).toBe(previous);
    const request = mock.request;
    vi.spyOn(mock, "request").mockImplementation(async (params) => {
      const value = await request(params);
      return params.method === "local_api_token_regenerate" ? { ...value as object, token: "replacement-token" } as typeof value : value;
    });
    await act(async () => { hook.result.current.actions.regenerateToken(); await session.settled(); });
    await waitFor(() => expect(hook.result.current.view).toMatchObject({ status: { token: "replacement-token" }, pending: null, error: null }));
  } finally { hook.unmount(); mock.dispose(); }
});

it("copies through the clipboard and house toast, and reports clipboard failure", async () => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  const copy = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
  const hook = renderHook(() => useLocalApi(true), { wrapper: wrapper(session) });
  try {
    await waitFor(() => expect(hook.result.current.view.status).not.toBeNull());
    await act(async () => { hook.result.current.actions.copy("http://127.0.0.1:4712"); });
    expect(copy).toHaveBeenCalledWith("http://127.0.0.1:4712");
    expect(screen.getByText("클립보드에 복사 완료")).toBeTruthy();
    copy.mockRejectedValueOnce(new Error("clipboard refused"));
    await act(async () => { hook.result.current.actions.copy("secret"); });
    expect(hook.result.current.view.error?.code).toBe("internal");
    expect(screen.queryByText("secret")).toBeNull();
    expect(screen.getAllByText("클립보드에 복사 완료")).toHaveLength(1);
  } finally { hook.unmount(); mock.dispose(); }
});
