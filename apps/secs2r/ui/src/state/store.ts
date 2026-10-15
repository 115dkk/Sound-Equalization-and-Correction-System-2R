import type { IpcError } from "../ipc/bindings/IpcError";

export function createStore<T>(initial: T) {
  let value = initial;
  const listeners = new Set<() => void>();
  return {
    getSnapshot: () => value,
    subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; },
    set(next: T) { if (Object.is(value, next)) return; value = next; for (const listener of listeners) listener(); },
  };
}

/** A generation guards against a slow read overwriting a newer mutation/read. */
export function createResource<T>(read: () => Promise<T>, failed: (error: unknown) => void) {
  const store = createStore<T | null>(null);
  let generation = 0;
  return {
    ...store,
    set(value: T | null) { generation++; store.set(value); },
    async load() {
      const ticket = ++generation;
      try { const value = await read(); if (ticket === generation) store.set(value); }
      catch (error) { if (ticket === generation) failed(error); }
    },
    invalidate() { generation++; },
  };
}

export type Failure = { id: number; error: IpcError };
