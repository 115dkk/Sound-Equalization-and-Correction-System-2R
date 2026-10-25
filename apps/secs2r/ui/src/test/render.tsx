// Renders a tree inside the providers every screen has: catalogs, toasts,
// a portal layer and (optionally) a session.
import { render } from "@testing-library/react";
import type { ReactNode } from "react";
import { PrefsScope, type Prefs } from "../app/prefs";
import type { Locale } from "../i18n";
import { SessionProvider } from "../state/react";
import type { Session } from "../state/session";

export function renderWith(ui: ReactNode, { locale = "ko", session }: { locale?: Locale; session?: Session } = {}) {
  const prefs: Prefs = { locale, theme: "light", accent: "default" };
  const tree = <PrefsScope initial={prefs}>{ui}</PrefsScope>;
  return render(session ? <SessionProvider value={session}>{tree}</SessionProvider> : tree);
}
