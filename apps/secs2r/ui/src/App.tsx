// M0 placeholder: shows that the shell, the IPC envelope and the catalogs
// work end to end. The real screens arrive in M5 (docs/ARCHITECTURE.md).
import { useEffect, useState } from "react";
import type { BootstrapInfo } from "./ipc/bindings/BootstrapInfo";
import { IpcFailure, request } from "./ipc/client";
import { pickLocale, translate, type MessageKey } from "./i18n";

const locale = pickLocale(navigator.languages);
const t = (key: MessageKey, params?: Record<string, string>) => translate(locale, key, params);

// std::env::consts values as people name them.
const OS_NAMES: Record<string, string> = { windows: "Windows", macos: "macOS", linux: "Linux" };
const ARCH_NAMES: Record<string, string> = { x86_64: "x64", aarch64: "ARM64" };

type State =
  | { kind: "loading" }
  | { kind: "ready"; info: BootstrapInfo }
  | { kind: "failed"; detail: string };

export function App() {
  const [state, setState] = useState<State>({ kind: "loading" });

  useEffect(() => {
    let alive = true;
    request({ method: "bootstrap" })
      .then((info) => alive && setState({ kind: "ready", info }))
      .catch((error: unknown) => {
        const detail = error instanceof IpcFailure ? error.error.detail : String(error);
        if (alive) setState({ kind: "failed", detail });
      });
    return () => {
      alive = false;
    };
  }, []);

  return (
    <main className="shell" lang={locale}>
      <header className="brand">
        <h1>SECS 2R</h1>
        {state.kind === "ready" && (
          <p className="meta">
            <span className="version">
              {t(state.info.prerelease ? "app.prerelease" : "app.release", { version: state.info.version })}
            </span>
            <span>{t("app.upstream", { version: state.info.upstream.version })}</span>
            <span>{t("app.platform", {
                os: OS_NAMES[state.info.platform.os] ?? state.info.platform.os,
                arch: ARCH_NAMES[state.info.platform.arch] ?? state.info.platform.arch,
              })}</span>
          </p>
        )}
      </header>

      {state.kind === "ready" && (
        <section className="notice">
          <h2>{t("app.placeholder.title")}</h2>
          <p>{t("app.placeholder.body")}</p>
        </section>
      )}

      {state.kind === "failed" && (
        <section className="notice notice-error" role="alert">
          <h2>{t("error.bootstrap")}</h2>
          <details>
            <summary>{t("error.details")}</summary>
            <pre>{state.detail}</pre>
          </details>
        </section>
      )}
    </main>
  );
}
