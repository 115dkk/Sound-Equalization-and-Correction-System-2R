import { useMemo } from "react";
import { useHost, useResource, useSession } from "../../state/react";
import { useJobLedger } from "../../state/useJobLedger";
import { mdatScopeFor, type ExportActions, type ExportView } from "./types";

export function useExportScreen(): { view: ExportView | null; actions: ExportActions } {
  const session = useSession();
  const host = useHost();
  const ledger = useJobLedger();
  const results = useResource("results");
  const settings = useResource("settings");
  const project = useResource("project");
  const lines = useResource("lines");
  const folder = useResource("folder");
  const actions = useMemo<ExportActions>(() => ({
    exportFiles: (kind) => {
      if (session.stores.project.getSnapshot()?.tutorial) {
        session.host.set({ runs: { ...session.host.getSnapshot().runs, [kind]: { status: "tutorial" } } });
      } else session.run(() => session.call({ method: "export", params: {
        kind, folder: "",
        ...(kind === "before_after_mdat" ? { mdat_scope: mdatScopeFor(session.stores.results.getSnapshot()?.calibration, session.host.getSnapshot().mdatScope) } : {}),
      } }));
    },
    setMdatScope: (scope) => session.host.set({ mdatScope: scope }),
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
  } : null, actions };
}
