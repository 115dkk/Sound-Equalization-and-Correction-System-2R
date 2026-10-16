// @vitest-environment happy-dom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, expect, it } from "vitest";
import type { ReactNode } from "react";
import { createMockService } from "../ipc/mock";
import { SessionProvider } from "../state/react";
import { createSession } from "../state/session";
import { useSystemScreen } from "../screens/system/useSystemScreen";
import { useConnectScreen } from "../screens/connect/useConnectScreen";
import { useMeasureScreen } from "../screens/measure/useMeasureScreen";
import { useFrameModel } from "./useFrameModel";
import { STEPS } from "./model";

let mock: ReturnType<typeof createMockService>;
let session: ReturnType<typeof createSession>;
const wrapper = ({ children }: { children: ReactNode }) => <SessionProvider value={session}>{children}</SessionProvider>;
const useModel = () => useFrameModel({ system: useSystemScreen().view, connect: useConnectScreen(false).view, measure: useMeasureScreen().view, calibrate: null, export: null }).model;
beforeEach(async () => {
  mock = createMockService({ tickMs: 0 }); session = createSession(mock); await session.start();
});
afterEach(async () => { cleanup(); await session.monitor(false); session.dispose(); mock.dispose(); });

it("preserves the mdat path and reports it ready without a device or connect view", async () => {
  const { result } = renderHook(() => useFrameModel({ system: null, connect: null, measure: null, calibrate: null, export: null }), { wrapper });
  await act(async () => { session.host.set({ path: "mdat" }); });
  expect(result.current.model.project.path).toEqual({ kind: "mdat", state: "ok" });
  await act(async () => { session.host.set({ path: "native" }); });
  expect(result.current.model.project.path.state).toBe("warning");
});

it("keeps invalid system configuration editable and blocks later steps", async () => {
  const { result } = renderHook(useModel, { wrapper });
  await act(async () => { session.stores.notices.set([{ code: "out_duplicate", params: { out: "1", first_role: "L", role: "R" } }]); });
  expect(result.current.steps.system).toEqual({ status: "ready" });
  for (const step of STEPS.filter((step) => step !== "system")) expect(result.current.steps[step]).toEqual({ status: "blocked", reason: "config_invalid", count: 1 });
  await act(async () => { session.stores.notices.set([]); });
  expect(result.current.steps.system.status).toBe("done");
  expect(result.current.steps.connect.status).not.toBe("blocked");
});

it("keeps the mdat connect step skipped even while configuration needs repair", async () => {
  const { result } = renderHook(useModel, { wrapper });
  await act(async () => { session.host.set({ path: "mdat" }); session.stores.notices.set([{ code: "out_duplicate", params: {} }]); });
  expect(result.current.steps.connect.status).toBe("skipped");
  expect(result.current.steps.measure.reason).toBe("config_invalid");
});

it("shows every step waiting for a project, even with stale path and views", async () => {
  const { result } = renderHook(useModel, { wrapper });
  await act(async () => { session.host.set({ path: "mdat" }); session.stores.project.set(null); });
  for (const step of STEPS) expect(result.current.steps[step]).toEqual({ status: "blocked", reason: "no_project" });
});
