// One ordered writer per settings bundle (A-80, TD-68). When two screens each
// send a copy of a bundle they read earlier, the later request puts the
// earlier change back. A writer sends its edits one at a time in call order,
// and builds each one from the service's latest answer when it is sent, not
// when it was queued.

/** The fields an edit changes, or a function of the latest answer that returns them. */
export type Patch<T> = Partial<T> | ((latest: T) => Partial<T>);

export type WriterDeps<T> = {
  /** The store snapshot the service last answered; null before it has loaded. */
  latest: () => T | null | Promise<T | null>;
  /** Sends `next`; resolves with the service's answer once `latest` shows it. */
  write: (next: T) => Promise<T>;
};

export type Writer<T> = {
  /** True when the service accepted the edit. A failed edit (its request
   * reports the error) or one with nothing loaded resolves false, and the
   * edits behind it still run. */
  edit(patch: Patch<T>): Promise<boolean>;
};

export function createWriterQueue() {
  let queue: Promise<unknown> = Promise.resolve();
  return {
    enqueue(work: () => Promise<boolean>): Promise<boolean> {
      const done = queue.then(work).catch(() => false);
      queue = done;
      return done;
    },
  };
}

export function createWriter<T>(deps: WriterDeps<T>, queue = createWriterQueue()): Writer<T> {
  return {
    edit(patch) {
      return queue.enqueue(async () => {
        const latest = await deps.latest();
        if (latest === null) return false;
        await deps.write({ ...latest, ...(typeof patch === "function" ? patch(latest) : patch) });
        return true;
      });
    },
  };
}
