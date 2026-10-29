import { afterEach, expect, it, vi } from "vitest";
import { createTauriIpc } from "./tauri";
import type { JobEvent } from "./bindings/JobEvent";
import type { Channel } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

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

it("forwards the shell's external-change wire method and returns its unlisten callback", async () => {
  let handler!: EventCallback<unknown>;
  const unlisten = vi.fn();
  vi.mocked(listen).mockImplementation(async (_name, callback) => { handler = callback; return unlisten; });
  const ipc = createTauriIpc();
  const changed = vi.fn();
  const off = await ipc.externalChanges!.subscribe(changed);
  expect(listen).toHaveBeenCalledWith("secs://external-change", expect.any(Function));
  handler({ event: "secs://external-change", id: 1, payload: "app_settings_set" });
  handler({ event: "secs://external-change", id: 1, payload: null });
  expect(changed).toHaveBeenCalledExactlyOnceWith("app_settings_set");
  off();
  expect(unlisten).toHaveBeenCalledTimes(1);
});
