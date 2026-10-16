import { useMemo } from "react";
import { useHost, useResource, useSession } from "../../state/react";
import type { MeasureActions, MeasureView } from "./types";

export function useMeasureScreen(): { view: MeasureView | null; actions: MeasureActions } {
  const session = useSession();
  const host = useHost();
  const project = useResource("project");
  const layout = useResource("layout");
  const estimate = useResource("estimate");
  const config = useResource("measurementSettings");
  const records = useResource("records");
  const curves = useResource("curves");
  const actions = useMemo<MeasureActions>(() => ({
    measurePosition: (position) => session.run(async () => {
      if (!await session.ensureSaved()) return;
      await session.monitor(false);
      await session.call({ method: "measure_position", params: { position } });
    }),
    cancelJob: (job) => session.run(() => session.call({ method: "job_cancel", params: { job } })),
    clearPosition: async (position) => (await session.call({ method: "position_clear", params: { position } })).token,
    undo: async (token) => { await session.call({ method: "undo", params: { token } }); },
    selectPosition: (selected) => session.host.set({ selected }),
    setGraphHidden: (graphHidden) => session.host.set({ graphHidden }),
    setPlotRange: (plotRange) => session.host.set({ plotRange }),
    setStartDelay: (start_delay) => session.run(() => session.call({ method: "layout_set", params: { count: null, start_delay, timing: null, confirm: false } })),
    setSweepLength: (sweep_length) => session.run(() => session.configure({ sweep_length })),
    setRepetitions: (repetitions) => session.run(() => session.configure({ repetitions })),
    pickMdat: (position) => session.run(async () => {
      const path = await session.call({ method: "dialog_pick_mdat" });
      if (path === null) return;
      const scan = await session.call({ method: "mdat_scan", params: { path } });
      session.host.set({ mdat: { path, scan, position } });
    }),
    importMdat: (params) => session.run(async () => {
      await session.monitor(false);
      await session.call({ method: "mdat_import", params });
      session.host.set({ mdat: null });
    }),
    closeMdat: () => session.host.set({ mdat: null }),
    loadSample: () => session.run(async () => {
      await session.monitor(false);
      await session.call({ method: "sample_load", params: { positions: null } });
    }),
    openCalibrate: () => session.navigate("calibrate"),
  }), [session]);
  const running = host.jobs.find((job) => ["measure", "sample_load", "mdat_import", "level_check", "level_monitor"].includes(job.kind) && (job.status === "running" || job.status === "cancel_requested"));
  const request = running ? host.jobRequests[running.job] : undefined;
  return { view: project && layout && config ? {
    project, layout, estimate, config, path: host.path === "mdat" ? "import" : host.path ? "measure" : "none",
    records: records?.records ?? [], curves,
    sweepOptions: config.sweep_lengths.map((samples) => ({ samples, seconds: samples / config.sample_rate })),
    repetitions: config.repetitions,
    playing: host.playing !== null || host.playingAll,
    job: running ? { job: running.job, kind: running.kind, status: running.status, progress: running.progress, position: request?.method === "measure_position" ? request.params.position : null } : null,
    warnings: host.warnings, selected: host.selected, graphHidden: host.graphHidden, plotRange: host.plotRange, mdat: host.mdat,
  } : null, actions };
}
