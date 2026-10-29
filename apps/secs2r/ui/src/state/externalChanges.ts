import { effectsOf } from "./effects";

/** External requests carry only a method, not params or an accepted response.
 * Conservatively reload every resource; never replay UI mutation follow-ups. */
export function externalRefresh(method: string) {
  return {
    replaceProject: method === "project_new" || method === "project_open",
    resources: [...effectsOf({ request: "project_open" }).reload, "measurementSettings"] as const,
    jobs: true,
  };
}

/** One bounded batch for a burst, with no overlapping refreshes. A notification
 * during a read schedules another batch so the last mutation cannot be lost. */
export function coalesceExternalChanges(refresh: (methods: readonly string[]) => Promise<void>, failed: (error: unknown) => void) {
  const methods = new Set<string>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let running = false;
  let disposed = false;
  const schedule = () => {
    if (disposed || running || timer !== undefined || methods.size === 0) return;
    timer = setTimeout(() => {
      timer = undefined;
      const batch = [...methods];
      methods.clear();
      running = true;
      void refresh(batch).catch(failed).finally(() => { running = false; schedule(); });
    }, 25);
  };
  return {
    push(method: string) { if (!disposed) { methods.add(method); schedule(); } },
    dispose() { disposed = true; clearTimeout(timer); timer = undefined; methods.clear(); },
  };
}
