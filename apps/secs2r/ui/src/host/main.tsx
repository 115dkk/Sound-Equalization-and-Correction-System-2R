// host.html: the SECS 2R Host shell's window (ADR 0018, P126 builds the
// Tauri app around it). It reads nothing of SECS 2R's own settings, so it
// follows the system's language and theme.
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { PrefsScope } from "../app/prefs";
import { pickLocale } from "../i18n";
import "../styles/base.css";
import "../ui";
import { createTauriHostApi } from "./api";
import { HostShell } from "./HostShell";

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <PrefsScope initial={{ locale: pickLocale(navigator.languages), theme: "system", accent: "default" }} root>
        <HostShell api={createTauriHostApi()} />
      </PrefsScope>
    </StrictMode>,
  );
}
