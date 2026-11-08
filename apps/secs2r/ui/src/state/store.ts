import type { IpcError } from "../ipc/bindings/IpcError";
import { IpcFailure, internalError } from "../ipc/client";

export function createStore<T>(initial: T) {
  let value = initial;
  const listeners = new Set<() => void>();
  return {
    getSnapshot: () => value,
    subscribe: (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; },
    set(next: T) { if (Object.is(value, next)) return; value = next; for (const listener of listeners) listener(); },
  };
}

/** A store of named fields set by patch. A patch that changes nothing keeps the snapshot, so subscribers do not render again (TD-176). */
export function createPatchStore<T extends object>(initial: T) {
  const store = createStore(initial);
  return { ...store, set: (patch: Partial<T>) => {
    const current = store.getSnapshot();
    if ((Object.keys(patch) as (keyof T)[]).every((key) => Object.is(current[key], patch[key]))) return;
    store.set({ ...current, ...patch });
  } };
}

export type ResourceLoadState<T> =
  | { status: "loading" }
  | { status: "failed"; error: IpcError }
  | { status: "ready"; value: T };

/** A generation guards against a slow read overwriting a newer mutation/read.
 * `loadState` reports loading and failure only while there is no value: a
 * read of a value already held stays ready, and its failure is reported
 * through `failed` alone (the app keeps running on the held value). */
export function createResource<T>(read: () => Promise<T>, failed: (error: unknown) => void) {
  const store = createStore<T | null>(null);
  const loadState = createStore<ResourceLoadState<T>>({ status: "loading" });
  let generation = 0;
  return {
    ...store, loadState,
    set(value: T | null) {
      generation++; store.set(value);
      loadState.set(value === null ? { status: "loading" } : { status: "ready", value });
    },
    async load() {
      const ticket = ++generation;
      if (store.getSnapshot() === null) loadState.set({ status: "loading" });
      try {
        const value = await read();
        if (ticket === generation) { store.set(value); loadState.set({ status: "ready", value }); }
      } catch (error) {
        if (ticket === generation) {
          const typed = error instanceof IpcFailure ? error.error : internalError(String(error));
          if (store.getSnapshot() === null) loadState.set({ status: "failed", error: typed });
          failed(error);
        }
      }
    },
    invalidate() { generation++; },
  };
}

export type Failure = { id: number; error: IpcError };
