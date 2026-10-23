import { useMemo } from "react";
import { useHost, useResource, useSession } from "../../state/react";
import type { CalibrateActions, CalibrateJob, CalibrateView } from "./types";

export function useCalibrateScreen(goToMeasurements?: () => void): { view: CalibrateView | null; actions: CalibrateActions } {
  const session = useSession();
  const host = useHost();
  const appSettings = useResource("appSettings");
  const computeStatus = useResource("computeStatus");
  const results = useResource("results");
  const lines = useResource("lines");
  const curves = useResource("resultCurves");
  const settings = useResource("settings");
  const modeSpec = useResource("modeSpec");
  const target = useResource("target");
  const config = useResource("config");
  const project = useResource("project");
  const measurements = useResource("curves");
  const dsp = useResource("dsp");
  const subWarnings = useResource("subWarnings");
  const actions = useMemo<CalibrateActions>(() => {
    const refresh = () => {
      const h = session.host.getSnapshot();
      session.run(() => session.refreshCurves({ tab: h.tab, channel: h.channel, group: h.xoGroup, xo_hz: h.previewHz, corrected: null }));
    };
    return {
      goToMeasurements: goToMeasurements ?? (() => session.navigate("measure")),
      goToExport: () => session.navigate("export"),
      openGpuHelp: () => session.run(() => session.call({ method: "open_gpu_help" })),
      refreshCompute: () => session.run(() => session.stores.computeStatus.load()),
      changeCompute: (compute) => session.run(async () => {
        const h = session.host.getSnapshot();
        const settings = session.stores.appSettings.getSnapshot();
        if (!settings || h.computeSaving || h.computeLoading || h.computeError || h.jobs.some((job) => ["running", "cancel_requested"].includes(job.status) && !["status_watch", "level_monitor"].includes(job.kind))) return;
        session.host.set({ computeSaving: true });
        try { await session.call({ method: "app_settings_set", params: { settings: { ...settings, compute } } }); }
        finally { session.host.set({ computeSaving: false }); }
      }),
      calibrate: () => session.run(() => session.call({ method: "calibrate", params: { settings: null, subs_only: false } })),
      calibrateSubs: () => session.run(() => session.call({ method: "calibrate", params: { settings: null, subs_only: true } })),
      recommendXo: () => session.run(() => session.call({ method: "recommend_xo" })),
      computeMimo: () => session.run(() => session.call({ method: "mimo", params: { settings: null } })),
      selectTab: (tab) => { session.host.set({ tab, previewHz: null }); refresh(); },
      selectChannel: (channel, group) => {
        session.host.set({ channel, previewHz: null, ...(session.host.getSnapshot().tab === "xo" ? { xoGroup: group } : {}) });
        refresh();
      },
      selectXoGroup: (xoGroup) => { session.host.set({ xoGroup, previewHz: null }); refresh(); },
      previewXo: (xoGroup, previewHz) => { session.host.set({ tab: "xo", xoGroup, previewHz }); refresh(); },
      changeSetting: (edit) => session.run(async () => {
        session.host.set({ previewHz: null });
        await session.call({ method: "calibration_settings_edit", params: { edit } });
      }),
      takeSubValues: () => session.run(() => session.call({ method: "sub_take_values" })),
      setTargetPoints: (points) => session.run(() => session.call({ method: "target_set_points", params: { points } })),
      applyTargetPreset: (preset) => session.run(() => session.call({ method: "target_apply_preset", params: { preset } })),
      saveTarget: () => session.run(async () => {
        const path = await session.call({ method: "dialog_save_file", params: { kind: "target", suggested: "타겟.json" } });
        if (path !== null) await session.call({ method: "target_save", params: { path } });
      }),
      loadTarget: () => session.run(async () => {
        const path = await session.call({ method: "dialog_open_file", params: { kind: "target" } });
        if (path !== null) await session.call({ method: "target_load", params: { path } });
      }),
    };
  }, [session, goToMeasurements]);
  const jobs = host.jobs.filter((job) => job.status === "running" || job.status === "cancel_requested");
  const runningJob = jobs.find((job) => ["calibrate", "recommend_xo", "mimo"].includes(job.kind));
  const request = runningJob ? host.jobRequests[runningJob.job] : undefined;
  const running: CalibrateJob | null = runningJob ? request?.method === "calibrate" && request.params.subs_only ? "calibrate_subs" : runningJob.kind as CalibrateJob : null;
  const measuredRoles = new Set(measurements?.curves.filter((curve) => curve.position === 1).map((curve) => curve.role));
  return { view: results && lines && settings && modeSpec && target && config && project ? {
    compute: { preference: appSettings?.compute ?? "auto", status: computeStatus, loading: host.computeLoading || !appSettings, saving: host.computeSaving, error: host.computeError,
      locked: jobs.some((job) => !["status_watch", "level_monitor"].includes(job.kind)) },
    results, lines, curves: curves?.curves ?? null, settings, modeSpec, target, config, positions: project.positions,
    measuredRoles: config.speakers.map((speaker) => speaker.role).filter((role) => measuredRoles.has(role)),
    firBudget: dsp?.fields.fir_budget ?? 0, xoRecommendation: host.xoRecommendation, running,
    measuring: jobs.some((job) => ["measure", "sample_load", "mdat_import"].includes(job.kind)),
    failure: host.failure,
    staleReason: results.calibration?.stale_reason === "settings" || results.calibration?.stale_reason === "target" || results.calibration?.stale_reason === "measurements" ? results.calibration.stale_reason : null,
    resultMode: results.calibration ? { mode: results.calibration.mode, ultra_ms: results.calibration.ultra_ms } : null,
    tab: host.tab, channel: host.channel, xoGroup: host.xoGroup, subWarnings: subWarnings ?? [],
    curvesLoading: curves?.pending != null && curves.pending.tab === host.tab && curves.pending.channel === host.channel && curves.pending.group === host.xoGroup,
  } : null, actions };
}
