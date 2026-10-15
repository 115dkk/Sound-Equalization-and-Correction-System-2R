import { afterEach, expect, it, vi } from "vitest";
import { createTauriIpc } from "./tauri";
import type { JobEvent } from "./bindings/JobEvent";
import type { Channel } from "@tauri-apps/api/core";

afterEach(() => vi.unstubAllGlobals());
it("gives each request its own channel, feeding all messages to one bus", async () => {
  let id = 0;
  const channels: Channel<JobEvent>[] = [];
  vi.stubGlobal("window", { __TAURI_INTERNALS__: {
    transformCallback: () => ++id,
    invoke: async (_command: string, args: { request: { method: string }; events: Channel<JobEvent> }) => {
      channels.push(args.events);
      return { ok: true, data: { method: args.request.method, data: [] } };
    },
  } });
  const ipc = createTauriIpc();
  const events: JobEvent[] = [];
  ipc.events.subscribe((event) => events.push(event));
  await ipc.request({ method: "jobs" }); await ipc.request({ method: "jobs" });
  expect(channels[0]?.id).not.toBe(channels[1]?.id);
  channels[0]?.onmessage({ event: "started", job: 1, kind: "status_watch" });
  channels[1]?.onmessage({ event: "started", job: 2, kind: "measure" });
  expect(events.map((event) => event.job)).toEqual([1, 2]);
});
