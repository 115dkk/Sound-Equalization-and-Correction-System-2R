// Renders a tree inside the providers every screen has: catalogs, toasts,
// a portal layer and (optionally) an IPC.
import { render } from "@testing-library/react";
import type { ReactNode } from "react";
import { PrefsScope, type Prefs } from "../app/prefs";
import type { Locale } from "../i18n";
import { IpcProvider, type Ipc } from "../ipc/context";

export function renderWith(ui: ReactNode, { locale = "ko", ipc }: { locale?: Locale; ipc?: Ipc } = {}) {
  const prefs: Prefs = { locale, theme: "light", expert: false, accent: "default" };
  const tree = <PrefsScope initial={prefs}>{ui}</PrefsScope>;
  return render(ipc ? <IpcProvider value={ipc}>{tree}</IpcProvider> : tree);
}
