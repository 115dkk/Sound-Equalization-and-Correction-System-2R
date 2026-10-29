// The typed request function and job event source supplied by Tauri or the mock.
import type { makeClient } from "./client";
import type { JobEventSource } from "./events";

export type RequestFn = ReturnType<typeof makeClient>;
export type ExternalChangeSource = {
  subscribe(listener: (method: string) => void): Promise<() => void>;
};
export type Ipc = { request: RequestFn; events: JobEventSource; externalChanges?: ExternalChangeSource };
