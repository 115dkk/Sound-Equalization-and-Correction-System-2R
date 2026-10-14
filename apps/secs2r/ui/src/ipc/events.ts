// Pushed job events (ADR 0003): the service sends Started, Progress, Log and
// Finished over the Tauri channel that came with the job's request. The
// screen collects them on one bus; the job tray and the screens listen to
// it. P33 connects the Tauri channel to `emit`; the mock emits directly.
import type { JobEvent } from "./bindings/JobEvent";

export type JobEventListener = (event: JobEvent) => void;

export interface JobEventSource {
  subscribe(listener: JobEventListener): () => void;
}

export type JobEventBus = JobEventSource & { emit(event: JobEvent): void };

export function createEventBus(): JobEventBus {
  const listeners = new Set<JobEventListener>();
  return {
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    emit(event) {
      for (const listener of [...listeners]) listener(event);
    },
  };
}
