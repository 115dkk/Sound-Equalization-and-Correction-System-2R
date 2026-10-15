import { useEffect, useRef } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** Prevent synchronously: saving is asynchronous, and a cancelled request must not stop jobs.
 * Once answered, destroy the window directly: `close()` would raise another
 * close request, and Tauri's own follow-up `destroy()` needs the same permission. */
export function useWindowClose(requestClose: () => void) {
  const request = useRef(requestClose);
  request.current = requestClose;
  const allowed = useRef(false);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let off: (() => void) | undefined;
    void getCurrentWindow().onCloseRequested((event) => {
      if (allowed.current) return;
      event.preventDefault();
      request.current();
    }).then((unlisten) => { if (disposed) unlisten(); else off = unlisten; });
    return () => { disposed = true; off?.(); };
  }, []);
  return async () => {
    if (!isTauri()) return;
    allowed.current = true;
    try { await getCurrentWindow().destroy(); }
    catch (error) { allowed.current = false; throw error; }
  };
}
