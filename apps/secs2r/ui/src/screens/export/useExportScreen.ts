import { useEffect, useMemo, useRef } from "react";
import { IpcFailure } from "../../ipc/client";
import { useHost, useResource, useSession } from "../../state/react";
import { useJobLedger } from "../../state/useJobLedger";
import { designNameFor, isFilterKind, mdatScopeFor, type ExportActions, type ExportView } from "./types";

const DESIGN_NAME_REASONS = new Set(["design_name_length", "design_name_control", "design_name_character", "design_name_trailing", "design_name_reserved"]);

export function useExportScreen(): { view: ExportView | null; actions: ExportActions } {
  const session = useSession();
  const host = useHost();
  const ledger = useJobLedger();
  const results = useResource("results");
  const settings = useResource("settings");
  const project = useResource("project");
  const lines = useResource("lines");
  const folder = useResource("folder");
  const methods = useResource("methods");
  const bootstrap = useResource("bootstrap");
  const methodsRead = useRef<typeof session | null>(null);
  useEffect(() => {
    // Session startup normally reads this already. A screen with no snapshot
    // retries once on mount, including StrictMode's repeated setup.
    if (methodsRead.current === session) return;
    methodsRead.current = session;
    if (!session.stores.methods.getSnapshot()) void session.stores.methods.load();
  }, [session]);
  const actions = useMemo<ExportActions>(() => ({
    exportFiles: (kind) => {
      if (session.stores.project.getSnapshot()?.tutorial) {
        session.host.set({ runs: { ...session.host.getSnapshot().runs, [kind]: { status: "tutorial" } } });
      } else session.run(async () => {
        const current = session.host.getSnapshot();
        const epoch = session.jobs.epoch;
        const name = isFilterKind(kind) ? designNameFor({ designName: current.designName, tutorial: false }) : null;
        const devices = current.camillaDevices;
        try {
          await session.call({ method: "export", params: {
            kind, folder: "",
            ...(name !== null ? { design_name: name } : {}),
            ...(kind === "camilla_dsp" && (devices.capture.type !== "file" || devices.playback.type !== "file") ? { camilla_devices: devices } : {}),
            ...(kind === "before_after_mdat" ? { mdat_scope: mdatScopeFor(session.stores.results.getSnapshot()?.calibration, current.mdatScope) } : {}),
          } });
        } catch (error) {
          // Name validation fails before a job starts, so no finished event
          // will record it in the row. Never attach an old project's failure.
          if (epoch === session.jobs.epoch && error instanceof IpcFailure && error.error.code === "invalid_request" && DESIGN_NAME_REASONS.has(error.error.params.reason ?? "")) {
            session.host.set({ runs: { ...session.host.getSnapshot().runs, [kind]: { status: "failed", error: error.error } } });
          }
          throw error;
        }
      });
    },
    setMdatScope: (scope) => session.host.set({ mdatScope: scope }),
    setDesignName: (name) => session.host.set({ designName: name }),
    setCamillaDevices: (devices) => session.host.set({ camillaDevices: devices }),
    changeFolder: () => session.run(async () => {
      const selected = await session.call({ method: "dialog_pick_folder", params: { start: session.stores.folder.getSnapshot()?.folder ?? null } });
      if (selected !== null) await session.call({ method: "export_folder_set", params: { folder: selected } });
    }),
    openFolder: (path) => session.run(() => session.call({ method: "open_folder", params: { path } })),
    goToCalibrate: () => session.navigate("calibrate"),
    recalibrate: () => session.run(() => session.call({ method: "calibrate", params: { settings: null, subs_only: false } })),
  }), [session]);
  const jobs = ledger.jobs.filter((job) => job.status === "running" || job.status === "cancel_requested");
  const recalibrating = jobs.some((job) => job.kind === "calibrate");
  const exporting = jobs.find((job) => job.kind === "export");
  const request = exporting?.request;
  return { view: results && settings && project ? {
    results, settings: { mode: results.calibration?.mode ?? settings.mode, ultra_ms: results.calibration?.ultra_ms ?? settings.ultra_ms, mimo_on: settings.mimo_on },
    positions: project.positions.length, rc: lines?.rc ?? [], folder,
    computing: recalibrating, recalibrating,
    exporting: request?.method === "export" ? request.params.kind : null,
    runs: host.runs, mdatScope: host.mdatScope, tutorial: project.tutorial,
    designName: host.designName, camillaDevices: host.camillaDevices,
    nativeDevices: bootstrap?.platform.os === "windows" && !methods?.native_error && methods?.native_devices.length ? methods.native_devices : null,
  } : null, actions };
}
