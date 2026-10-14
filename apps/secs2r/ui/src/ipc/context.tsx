// The IPC handles a screen needs: the typed request function and the job
// event source. The app provides the Tauri ones; the gallery and tests
// provide the mock service (ipc/mock.ts).
import { createContext, useContext, type ReactNode } from "react";
import type { makeClient } from "./client";
import type { JobEventSource } from "./events";

export type RequestFn = ReturnType<typeof makeClient>;

export type Ipc = { request: RequestFn; events: JobEventSource };

const IpcContext = createContext<Ipc | null>(null);

export function IpcProvider({ value, children }: { value: Ipc; children: ReactNode }) {
  return <IpcContext.Provider value={value}>{children}</IpcContext.Provider>;
}

export function useIpc(): Ipc {
  const ipc = useContext(IpcContext);
  if (!ipc) throw new Error("useIpc() needs an <IpcProvider> above it");
  return ipc;
}
