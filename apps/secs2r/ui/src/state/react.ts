import { createContext, createElement, useContext, useSyncExternalStore, type ReactNode } from "react";
import type { LevelMonitorState, Session } from "./session";

const Context = createContext<Session | null>(null);
export function SessionProvider({ value, children }: { value: Session; children: ReactNode }) {
  return createElement(Context.Provider, { value }, children);
}
export function useOptionalSession() {
  return useContext(Context);
}
const noSubscription = () => () => {};
export function useProjectIdentity() {
  const session = useOptionalSession();
  return useSyncExternalStore(session?.host.subscribe ?? noSubscription, () => session?.host.getSnapshot().opened ?? 0);
}
export function useSession() {
  const session = useOptionalSession();
  if (!session) throw new Error("useSession requires SessionProvider");
  return session;
}
export function useHost() {
  const { host } = useSession();
  return useSyncExternalStore(host.subscribe, host.getSnapshot);
}
export function useResource<K extends keyof Session["stores"]>(key: K): ReturnType<Session["stores"][K]["getSnapshot"]> {
  const store = useSession().stores[key];
  return useSyncExternalStore(store.subscribe, store.getSnapshot as () => ReturnType<Session["stores"][K]["getSnapshot"]>);
}
/**
 * One value picked from the level monitor (TD-176). The component renders
 * again only when that value changes, not on every Meter event, so `select`
 * returns a field or a primitive, never a new object.
 */
export function useLevelMonitor<T>(select: (state: LevelMonitorState) => T): T {
  const { monitor } = useSession().stores;
  return useSyncExternalStore(monitor.subscribe, () => select(monitor.getSnapshot()));
}
