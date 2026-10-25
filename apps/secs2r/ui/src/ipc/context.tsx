// The typed request function and job event source supplied by Tauri or the mock.
import type { makeClient } from "./client";
import type { JobEventSource } from "./events";

export type RequestFn = ReturnType<typeof makeClient>;
export type Ipc = { request: RequestFn; events: JobEventSource };
