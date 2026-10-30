import { useCallback, useEffect, useRef, useState } from "react";
import type { UpdatePolicy } from "../ipc/bindings/UpdatePolicy";
import type { UpdateStatus } from "../ipc/bindings/UpdateStatus";
import { useResource, useSession } from "./react";

type StatusMethod = "update_status" | "update_check" | "update_download" | "update_notice_dismiss";

/** The updater owns its work; the screen only polls status and sends explicit actions. */
export function useUpdate() {
  const session = useSession();
  const policy = useResource("appSettings")?.updates;
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  const generation = useRef(0);
  const active = useRef(false);
  const polling = useRef(false);
  const actions = useRef(0);
  const request = useCallback(async (method: StatusMethod) => {
    const ticket = ++generation.current;
    if (method !== "update_status") actions.current++;
    try {
      const next = await session.call({ method });
      if (active.current && ticket === generation.current) setStatus(next);
    } catch { /* session.call reports the typed error */ }
    finally { if (method !== "update_status") actions.current--; }
  }, [session]);
  const refresh = useCallback(async () => {
    if (polling.current || actions.current > 0) return;
    polling.current = true;
    try { await request("update_status"); }
    finally { polling.current = false; }
  }, [request]);

  useEffect(() => {
    active.current = true;
    void request("update_status");
    return () => { active.current = false; generation.current++; };
  }, [request]);
  const busy = status?.phase.kind === "checking" || status?.phase.kind === "downloading";
  useEffect(() => {
    const timer = setInterval(() => { void refresh(); }, busy ? 1_000 : 60_000);
    return () => clearInterval(timer);
  }, [refresh, busy]);
  // Policy changes can also come from another settings surface or a local API client.
  useEffect(() => { if (policy !== undefined) void refresh(); }, [policy, refresh]);

  const check = useCallback(() => request("update_check"), [request]);
  const download = useCallback(() => request("update_download"), [request]);
  const dismiss = useCallback(() => request("update_notice_dismiss"), [request]);
  const openRelease = useCallback(async (version: string) => {
    try {
      await session.call({ method: "update_open_release", params: { version } });
    } catch { /* session.call reports the typed error */ }
  }, [session]);
  const restart = useCallback(async () => {
    ++generation.current;
    actions.current++;
    try {
      await session.call({ method: "update_restart" });
      await request("update_status");
    } catch { /* session.call reports the typed error */ }
    finally { actions.current--; }
  }, [session, request]);
  const setPolicy = useCallback(async (updates: UpdatePolicy) => {
    if (!session.stores.appSettings.getSnapshot()) await session.stores.appSettings.load();
    const saved = await session.writers.app.edit({ updates });
    if (saved) await request("update_status");
    return saved;
  }, [session, request]);

  return { status, check, download, restart, dismiss, setPolicy, openRelease };
}
