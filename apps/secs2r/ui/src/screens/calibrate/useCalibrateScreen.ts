import { useMemo } from "react";
import { useHost, useResource, useSession } from "../../state/react";
import { useJobLedger } from "../../state/useJobLedger";
import type { CalibrateActions, CalibrateJob, CalibrateView } from "./types";
import { groupOf, xoGroups } from "./text";
import { DESIGN_RATE, countedRate } from "../system/outputRate";
import { isRangeEdit, mimoIncluded, rangeRejection } from "./settingsView";
import { IpcFailure } from "../../ipc/client";
import { startCalibration } from "../../state/calibration";

const withoutStage = ({ stage: _stage, ...params }: Record<string, string>): Record<string, string> => params;

export function useCalibrateScreen(goToMeasurements?: () => void): { view: CalibrateView | null; actions: CalibrateActions } {
  const session = useSession();
  const host = useHost();
  const ledger = useJobLedger();
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
    const selectedGroup = (channel: string | null) => {
      const config = session.stores.config.getSnapshot();
      if (!config) return null;
      const measured = new Set(session.stores.curves.getSnapshot()?.curves.filter((curve) => curve.position === 1).map((curve) => curve.role));
      const channels = session.stores.results.getSnapshot()?.calibration
        ? session.stores.lines.getSnapshot()?.channels.map((line) => line.role) ?? []
        : config.speakers.map((speaker) => speaker.role).filter((role) => measured.has(role));
      const role = channel && channels.includes(channel) ? channel : channels[0];
      const group = role ? groupOf(config, role) : null;
      return group && xoGroups(config).includes(group) ? group : null;
    };
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
        if (!settings || h.computeSaving || h.computeLoading || h.computeError || session.jobs.getSnapshot().jobs.some((job) => ["running", "cancel_requested"].includes(job.status) && !["status_watch", "level_monitor"].includes(job.kind))) return;
        session.host.set({ computeSaving: true });
        try { await session.writers.app.edit({ compute }); }
        finally { session.host.set({ computeSaving: false }); }
      }),
      calibrate: () => session.run(() => startCalibration(session)),
      calibrateSubs: () => session.run(() => session.call({ method: "calibrate", params: { settings: null, subs_only: true } })),
      recommendXo: () => session.run(() => session.call({ method: "recommend_xo" })),
      computeMimo: () => session.run(() => session.call({ method: "mimo", params: { settings: null } })),
      selectTab: (tab) => {
        session.host.set({ tab, previewHz: null, ...(tab === "xo" ? { xoGroup: selectedGroup(session.host.getSnapshot().channel) } : {}) });
        refresh();
      },
      selectChannel: (channel) => {
        session.host.set({ channel, previewHz: null, ...(session.host.getSnapshot().tab === "xo" ? { xoGroup: selectedGroup(channel) } : {}) });
        refresh();
      },
      selectXoGroup: (xoGroup) => { session.host.set({ xoGroup, previewHz: null }); refresh(); },
      previewXo: (xoGroup, previewHz) => { session.host.set({ tab: "xo", xoGroup, previewHz }); refresh(); },
      changeSetting: async (edit) => {
        // A range edit (or another upper-limit basis) replaces the refusal the card shows; a refusal of
        // a range edit is shown at its input (session.call keeps it out of the global notice).
        const range = isRangeEdit(edit);
        session.host.set({ previewHz: null, ...(range || edit.kind === "rc_hi_basis" ? { rangeRejection: null } : {}) });
        const epoch = session.jobs.epoch;
        try { await session.call({ method: "calibration_settings_edit", params: { edit } }); return true; }
        catch (error) {
          const rejected = range && error instanceof IpcFailure ? rangeRejection(edit, error.error) : null;
          if (rejected && epoch === session.jobs.epoch) session.host.set({ rangeRejection: rejected });
          return false;
        }
      },
      takeSubValues: () => session.run(() => session.call({ method: "sub_take_values" })),
      setTargetPoints: async (points) => {
        try { await session.call({ method: "target_set_points", params: { points } }); return true; }
        catch { return false; }
      },
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
  const jobs = ledger.jobs.filter((job) => job.status === "running" || job.status === "cancel_requested");
  const runningJob = jobs.find((job) => ["calibrate", "recommend_xo", "mimo"].includes(job.kind));
  const request = runningJob?.request;
  const running: CalibrateJob | null = runningJob ? request?.method === "calibrate" && request.params.subs_only ? "calibrate_subs" : runningJob.kind as CalibrateJob : null;
  // The request says whether the run includes MIMO; a job found only in a snapshot (no request) is
  // read from what 보정 계산 sends (`mimoIncluded`) for the settings and positions shown.
  const runningMimo = running === "calibrate" && (request?.method === "calibrate"
    ? request.params.mimo
    : !!settings && !!project && mimoIncluded(settings, project.positions.length));
  // A MIMO failure after the correction was kept belongs to the MIMO card; the job's name says MIMO there.
  const failure = host.failure?.job === "calibrate" && host.failure.error.params.stage === "mimo"
    ? { job: "mimo" as const, error: { ...host.failure.error, params: withoutStage(host.failure.error.params) } }
    : host.failure;
  const measuredRoles = new Set(measurements?.curves.filter((curve) => curve.position === 1).map((curve) => curve.role));
  return { view: results && lines && settings && modeSpec && target && config && project ? {
    compute: { preference: appSettings?.compute ?? "auto", status: computeStatus, loading: host.computeLoading || !appSettings, saving: host.computeSaving, error: host.computeError,
      locked: jobs.some((job) => !["status_watch", "level_monitor"].includes(job.kind)) },
    results, lines, curves: curves?.curves ?? null, settings, modeSpec, target, config, positions: project.positions,
    measuredRoles: config.speakers.map((speaker) => speaker.role).filter((role) => measuredRoles.has(role)),
    firBudget: dsp?.fields.fir_budget ?? 0, firTaps: dsp?.fields.fir_taps ?? 0, outputRate: dsp ? countedRate(dsp.fields.output_rate) : DESIGN_RATE,
    xoRecommendation: host.xoRecommendation, xoRecTurnedOffByEdit: host.xoRecTurnedOffByEdit, running, runningMimo,
    measuring: jobs.some((job) => ["measure", "sample_load", "mdat_import"].includes(job.kind)),
    failure, targetEdit: host.targetEdit, xoEdit: host.xoEdit,
    staleReason: results.calibration?.stale_reason === "settings" || results.calibration?.stale_reason === "target" || results.calibration?.stale_reason === "measurements" || results.calibration?.stale_reason === "config" ? results.calibration.stale_reason : null,
    resultMode: results.calibration ? { mode: results.calibration.mode, ultra_ms: results.calibration.ultra_ms } : null,
    tab: host.tab, channel: host.channel, xoGroup: host.xoGroup, subWarnings: subWarnings ?? [], rangeRejection: host.rangeRejection,
    curvesLoading: curves?.pending != null && curves.pending.tab === host.tab && curves.pending.channel === host.channel && curves.pending.group === host.xoGroup,
  } : null, actions };
}
