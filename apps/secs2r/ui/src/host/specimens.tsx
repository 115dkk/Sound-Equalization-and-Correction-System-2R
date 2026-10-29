// The host shell's window (host.html, ADR 0018) in its states, at its
// smallest size (420 × 560): not installed, running and managing both
// targets, CamillaDSP given up with the log that led there, and the confirm
// before 서비스 제거. Fixed views at the gallery's clock, no polling.
import type { ReactNode } from "react";
import type { SpecimenDef } from "../gallery/Specimen";
import { HOST_NOW, LOG_GIVEN_UP, LOG_MANAGING, hostGivenUp, hostManagingRepaired } from "./fixtures";
import { HostShellPage, UninstallConfirm } from "./HostShell";
import type { HostShellActions, HostShellView } from "./useHostShell";

const noop: HostShellActions = { install: () => {}, uninstall: () => {}, stopManaging: () => {}, openApp: () => {} };

const view = (patch: Partial<HostShellView>): HostShellView => ({ service: "not_installed", host: null, log: [], readError: null, busy: null, failure: null, now: HOST_NOW, ...patch });

export const SHELL_NOT_INSTALLED = view({});
export const SHELL_MANAGING = view({ service: "running", host: hostManagingRepaired, log: LOG_MANAGING });
export const SHELL_GIVEN_UP = view({ service: "running", host: hostGivenUp, log: LOG_GIVEN_UP });

/** The window's page filling its stage, as Tauri sizes it. */
function Window({ shell, children }: { shell: HostShellView; children?: ReactNode }) {
  return (
    <div className="gal-fill" style={{ position: "relative" }}>
      <HostShellPage view={shell} actions={noop} />
      {children}
    </div>
  );
}

const shellWindow = (id: string, title: string, render: () => ReactNode): SpecimenDef => ({ id, title, width: 420, height: 560, render });

export const hostShellSpecimens: SpecimenDef[] = [
  shellWindow("host.shell.not-installed", "Host shell · service not installed: why it is needed, 서비스 설치 filled", () => <Window shell={SHELL_NOT_INSTALLED} />),
  shellWindow("host.shell.managing", "Host shell · running, managing CamillaDSP (PID 4812) and Equalizer APO, the line repaired 2 minutes ago", () => <Window shell={SHELL_MANAGING} />),
  shellWindow("host.shell.given-up", "Host shell · CamillaDSP given up after five stops, the log newest first", () => <Window shell={SHELL_GIVEN_UP} />),
  shellWindow("host.shell.uninstall-confirm", "Host shell · 서비스 제거 asks in one sentence", () => (
    <Window shell={SHELL_MANAGING}>
      <div style={{ position: "absolute", inset: 0, zIndex: "var(--z-dialog)", display: "grid", placeItems: "center", padding: "var(--space-6)", background: "var(--scrim)" }}>
        <UninstallConfirm inline open onClose={() => {}} onConfirm={() => {}} />
      </div>
    </Window>
  )),
];
