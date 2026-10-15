// Why the bridge makes a new Channel for every secs_call: the installed Tauri
// Channel, fed the wire frames JavaScriptChannelId::channel_on produces in
// tauri 2.12.3. secs_call builds a Rust channel per request (indices from 0,
// `end` when the last clone drops), so one long-lived JS Channel loses its
// callback after the first request that starts no job, and events of
// concurrent requests collide.
import { Channel } from "@tauri-apps/api/core";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { JobEvent } from "./bindings/JobEvent";

type Frame = { index: number; message: JobEvent } | { index: number; end: true };

function transport() {
  const callbacks = new Map<number, (frame: Frame) => void>();
  let nextId = 1;
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {
      transformCallback(callback: (frame: Frame) => void) {
        const id = nextId++;
        callbacks.set(id, callback);
        return id;
      },
      unregisterCallback(id: number) {
        callbacks.delete(id);
      },
    },
  });
  return {
    callbacks,
    // Each channel_on invocation owns its own Rust AtomicUsize(0), and its
    // last clone sends an end frame on drop (channel.rs:277–332).
    channelOn(id: number) {
      let index = 0;
      return {
        send(message: JobEvent) { callbacks.get(id)?.({ index: index++, message }); },
        drop() { callbacks.get(id)?.({ index, end: true }); },
      };
    },
  };
}

afterEach(() => vi.unstubAllGlobals());

describe("Tauri channel reuse precondition", () => {
  it("delivers multiple events from one retained Rust channel", () => {
    const wire = transport();
    const received: JobEvent[] = [];
    const channel = new Channel<JobEvent>((event) => received.push(event));
    const rust = wire.channelOn(channel.id);
    const started: JobEvent = { event: "started", job: 1, kind: "status_watch" };
    const stopped: JobEvent = { event: "finished", job: 1, status: "succeeded", result: { kind: "stopped" }, error: null };
    rust.send(started);
    rust.send(stopped);
    expect(received).toEqual([started, stopped]);
    rust.drop();
    expect(wire.callbacks.has(channel.id)).toBe(false);
  });

  it("drops the JS callback when an ordinary request's unused Rust channel is dropped", () => {
    const wire = transport();
    const received: JobEvent[] = [];
    const channel = new Channel<JobEvent>((event) => received.push(event));
    wire.channelOn(channel.id).drop(); // e.g. bootstrap, no events
    wire.channelOn(channel.id).send({ event: "started", job: 1, kind: "measure" });
    expect(wire.callbacks.has(channel.id)).toBe(false);
    expect(received).toEqual([]);
  });

  it("does not deliver a second Rust channel's index zero while the first stays alive", () => {
    const wire = transport();
    const received: JobEvent[] = [];
    const channel = new Channel<JobEvent>((event) => received.push(event));
    const watch = wire.channelOn(channel.id);
    const measurement = wire.channelOn(channel.id);
    const started: JobEvent = { event: "started", job: 1, kind: "status_watch" };
    watch.send(started);
    measurement.send({ event: "started", job: 2, kind: "measure" });
    expect(wire.callbacks.has(channel.id)).toBe(true);
    expect(received).toEqual([started]);
  });
});
