import { Channel, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { JobEvent } from "./bindings/JobEvent";
import { makeClient } from "./client";
import type { Ipc } from "./context";
import { createEventBus } from "./events";

/** Rust closes a request's channel when its last clone (including a job) drops. */
export function createTauriIpc(): Ipc {
  const events = createEventBus();
  const request = makeClient((command, args) => {
    const channel = new Channel<JobEvent>();
    channel.onmessage = events.emit;
    return invoke(command, { ...args, events: channel });
  });
  return { request, events, externalChanges: {
    subscribe: (listener) => listen<string>("secs://external-change", ({ payload }) => {
      if (typeof payload === "string") listener(payload);
    }),
  } };
}
