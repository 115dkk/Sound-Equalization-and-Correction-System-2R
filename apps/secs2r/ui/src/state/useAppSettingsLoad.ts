import { useSyncExternalStore } from "react";
import type { AppSettings } from "../ipc/bindings/AppSettings";
import { useSession } from "./react";
import type { ResourceLoadState } from "./store";

export type AppSettingsLoadState = ResourceLoadState<AppSettings>;

/** Startup and retry share the resource's generation-guarded load state. */
export function useAppSettingsLoad(): { state: AppSettingsLoadState; retry: () => Promise<void> } {
  const resource = useSession().stores.appSettings;
  const state = useSyncExternalStore(resource.loadState.subscribe, resource.loadState.getSnapshot);
  return { state, retry: resource.load };
}
