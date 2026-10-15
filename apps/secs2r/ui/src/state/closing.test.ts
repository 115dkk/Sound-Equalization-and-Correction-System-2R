// @vitest-environment happy-dom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { useWindowClose } from "./closing";

const windowApi = vi.hoisted(() => ({ onCloseRequested: vi.fn(), destroy: vi.fn(async () => {}) }));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => windowApi }));
afterEach(() => { cleanup(); vi.clearAllMocks(); });
it("prevents the first close synchronously and permits only the answered request", async () => {
  let listener!: (event: { preventDefault: () => void }) => void;
  const off = vi.fn();
  windowApi.onCloseRequested.mockImplementation(async (callback) => { listener = callback; return off; });
  const ask = vi.fn();
  const hook = renderHook(() => useWindowClose(ask));
  await act(async () => {});
  const preventDefault = vi.fn();
  listener({ preventDefault });
  expect(preventDefault).toHaveBeenCalledOnce(); expect(ask).toHaveBeenCalledOnce();
  expect(windowApi.destroy).not.toHaveBeenCalled();
  await hook.result.current();
  expect(windowApi.destroy).toHaveBeenCalledOnce();
  listener({ preventDefault });
  expect(preventDefault).toHaveBeenCalledOnce();
  hook.unmount(); expect(off).toHaveBeenCalledOnce();
});
it("unsubscribes if registration finishes after unmount", async () => {
  let finish!: (off: () => void) => void;
  windowApi.onCloseRequested.mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
  const hook = renderHook(() => useWindowClose(vi.fn())); hook.unmount();
  const off = vi.fn(); await act(async () => finish(off)); expect(off).toHaveBeenCalledOnce();
});
