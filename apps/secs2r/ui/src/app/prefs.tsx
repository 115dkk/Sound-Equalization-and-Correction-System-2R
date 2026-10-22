// Per-user display settings: language, theme, skin, logo, and the
// tutorial accent. The app applies them to <html>; the gallery applies them
// to a subtree so light and dark specimens sit side by side. Persistence
// goes through the service's app settings (P26/P33); here they are state.
import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import type { Locale } from "../i18n";
import type { AppIcon } from "../ipc/bindings/AppIcon";
import { I18nProvider } from "../i18n/react";
import { LayerRoot } from "../ui/layer";
import { ToastProvider } from "../ui/Toast";

export type ThemeChoice = "system" | "light" | "dark";
export type AccentChoice = "default" | "tutorial";
/** The skin set (ADR 0010, docs/skins): the default tokens, Hardware Rack or Signal Matrix. */
export type SkinChoice = "default" | "rack" | "matrix";
export const SKINS: readonly SkinChoice[] = ["default", "rack", "matrix"];

/** `icon` is the logo choice; scopes that leave it out show upstream's logo. `skin` left out is the default skin. */
export type Prefs = { locale: Locale; theme: ThemeChoice; accent: AccentChoice; icon?: AppIcon; skin?: SkinChoice };

type PrefsValue = Prefs & { set: (patch: Partial<Prefs>) => void };

const PrefsContext = createContext<PrefsValue | null>(null);

export function usePrefs(): PrefsValue {
  const value = useContext(PrefsContext);
  if (!value) throw new Error("usePrefs() needs a <PrefsScope> above it");
  return value;
}

/**
 * Provides prefs, catalogs, toasts and a portal layer for one themed scope.
 * `root` applies theme, accent and language to <html>; otherwise to a div.
 */
export function PrefsScope({
  initial,
  onChange,
  root = false,
  className,
  children,
}: {
  initial: Prefs;
  onChange?: (prefs: Prefs) => void;
  root?: boolean;
  className?: string;
  children: ReactNode;
}) {
  const [prefs, setPrefs] = useState(initial);
  const value = useMemo<PrefsValue>(
    () => ({
      ...prefs,
      set: (patch) => {
        const next = { ...prefs, ...patch };
        setPrefs(next);
        onChange?.(next);
      },
    }),
    [prefs, onChange],
  );

  useEffect(() => {
    if (!root) return;
    const html = document.documentElement;
    if (prefs.theme === "system") delete html.dataset.theme;
    else html.dataset.theme = prefs.theme;
    if (prefs.accent === "tutorial") html.dataset.accent = "tutorial";
    else delete html.dataset.accent;
    if (prefs.skin && prefs.skin !== "default") html.dataset.skin = prefs.skin;
    else delete html.dataset.skin;
    html.lang = prefs.locale;
  }, [root, prefs.theme, prefs.accent, prefs.skin, prefs.locale]);

  const body = (
    <I18nProvider locale={prefs.locale}>
      <LayerRoot>
        <ToastProvider>{children}</ToastProvider>
      </LayerRoot>
    </I18nProvider>
  );

  return (
    <PrefsContext.Provider value={value}>
      {root ? (
        body
      ) : (
        <div
          className={["prefs-scope", className].filter(Boolean).join(" ")}
          data-theme={prefs.theme === "system" ? undefined : prefs.theme}
          data-accent={prefs.accent === "tutorial" ? "tutorial" : undefined}
          data-skin={prefs.skin && prefs.skin !== "default" ? prefs.skin : undefined}
          lang={prefs.locale}
        >
          {body}
        </div>
      )}
    </PrefsContext.Provider>
  );
}
