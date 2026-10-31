import type { JobKind } from "../ipc/bindings/JobKind";
import type { JobStatus } from "../ipc/bindings/JobStatus";
import type { Request } from "../ipc/bindings/Request";
import type { Session } from "./session";
import type { FinishedJob, JobRequestContext, LedgerJob } from "./jobLedger";
import type { JobEvent } from "../ipc/bindings/JobEvent";
import { mimoIncluded } from "../screens/calibrate/settingsView";

export type Trigger =
  | { request: Request["method"] }
  | { finished: JobKind; status: JobStatus }
  | { log: string };

const config = ["config_apply_preset", "config_set_sub_count", "config_set_speaker", "config_add_speaker", "config_remove_speaker", "config_renumber"] as const;
const measurements = ["position_clear", "undo", "layout_set", ...config] as const;
const settings = ["calibration_settings_set", "calibration_settings_edit", "sub_take_values"] as const;
const target = ["target_set_points", "target_apply_preset", "target_load"] as const;
const recorded = ["measure", "sample_load", "mdat_import"] as const;
const inputs = [...measurements, ...settings, ...target, "dsp_set", "measure_configure"] as const;

/** Project replacement reloads all project resources, including empty results. */
const PROJECT_REPLACED = ["project_new", "project_open"] as const;
const reloads = {
  project: { mutations: [...inputs, "project_save", "levels_fill_defaults", "export_folder_set", "mic_calibration_set", "mic_calibration_clear"], finished: [...recorded, "calibrate", "mimo"] },
  config: { mutations: [...config, "undo", "measure_configure"], finished: [] },
  notices: { mutations: [...config, "undo", "measure_configure"], finished: [] },
  dsp: { mutations: ["dsp_set"], finished: [] },
  layout: { mutations: ["layout_set", "undo", ...config], finished: [] },
  estimate: { mutations: [...config, "layout_set", "measure_configure", "undo"], finished: [] },
  methods: { mutations: ["rew_launch", "rew_driver_set"], finished: [] },
  records: { mutations: measurements, finished: recorded },
  curves: { mutations: [...measurements, "mic_calibration_attach"], finished: recorded },
  micCalibration: { mutations: ["mic_calibration_set", "mic_calibration_clear", "mic_calibration_attach", ...measurements], finished: recorded },
  levels: { mutations: ["levels_fill_defaults", "level_play", ...config], finished: [] },
  settings: { mutations: [...settings, ...config, "undo"], finished: ["recommend_xo"] },
  modeSpec: { mutations: settings, finished: [] },
  target: { mutations: target, finished: [] },
  results: { mutations: [...inputs, "app_settings_set", "project_save", "mic_calibration_attach"], finished: [...recorded, "calibrate", "mimo", "recommend_xo"] },
  resultCurves: { mutations: [...inputs, "app_settings_set", "mic_calibration_attach"], finished: [...recorded, "calibrate", "mimo"] },
  lines: { mutations: [...inputs, "app_settings_set", "mic_calibration_attach"], finished: [...recorded, "calibrate", "mimo"] },
  subWarnings: { mutations: [...settings, ...config, "undo"], finished: ["calibrate"] },
  rew: { mutations: ["rew_launch"], finished: [] },
  folder: { mutations: ["export_folder_set", "project_save"], finished: ["export"] },
  computeStatus: { mutations: ["app_settings_set"], finished: [] },
  appSettings: { mutations: ["app_settings_set", "apply_settings_set", "project_open", "project_save", "project_new"], finished: [] },
} as const satisfies Record<string, { mutations: readonly Request["method"][]; finished: readonly JobKind[] }>;

export type ResourceKey = keyof typeof reloads;
export const projectEdits: readonly Request["method"][] = reloads.project.mutations;
export const refreshInputs: readonly Request["method"][] = [...inputs, "mic_calibration_attach"];
export const refreshMeasurements: readonly JobKind[] = recorded;
export type EffectContext = Pick<Session, "host" | "stores" | "jobs" | "call" | "configure" | "run" | "canContinueRefresh"> & {
  source:
    | { request: Request; data: unknown }
    | { job: LedgerJob; event: FinishedJob; request: JobRequestContext | null }
    | { job: LedgerJob; event: Extract<JobEvent, { event: "log" }>; request: JobRequestContext | null };
  replaceProject: () => void;
};
export type Rule = {
  name: string;
  when: (trigger: Trigger) => boolean;
  reload?: readonly ResourceKey[];
  /** Response/finish state was committed synchronously before any reads. */
  beforeReload?: (ctx: EffectContext) => void;
  apply?: (ctx: EffectContext) => void | Promise<void>;
};
const requested = (...methods: Request["method"][]) => (trigger: Trigger) => "request" in trigger && methods.includes(trigger.request);
/** Preserve the old synchronous response/finish commits, then reload resources,
 * then apply follow-up requests in table order. The session owns epoch checks. */
export const RULES: readonly Rule[] = [
  {
    name: "measurement.response", when: requested("measure_configure", "measure_config_get"),
    beforeReload: ({ source, stores, host }) => {
      if (!("data" in source)) return;
      stores.measurementSettings.set(source.data as import("../ipc/bindings/MeasureConfigResult").MeasureConfigResult);
      const method = stores.measurementSettings.getSnapshot()!.method;
      host.set({ path: method === "none" ? null : method as import("../screens/connect/types").ConnectPath });
    },
  },
  {
    name: "project.replace", when: requested("project_new", "project_open"),
    // The epoch, curve generation and host selection reset before reloads.
    beforeReload: (ctx) => {
      ctx.replaceProject();
      ctx.stores.measurementSettings.set(null);
      ctx.stores.micCalibration.set(null);
      ctx.host.set({ micCalBusy: null, micCalError: null, path: null, selected: null, mdat: null, warnings: [], runs: {}, applyRuns: {}, mdatScope: "all", designName: "", channel: null, xoGroup: null, xoRecommendation: null, xoRecTurnedOffByEdit: false, failure: null, rangeRejection: null, opened: ctx.host.getSnapshot().opened + 1 });
    },
    // Project replacement reads the stored method; tutorial setup owns writes.
    apply: async ({ call }) => { await call({ method: "measure_config_get" }); },
  },
  {
    name: "warnings.replace-position", when: requested("measure_position", "position_clear", "mdat_import"),
    beforeReload: ({ source, host }) => {
      if (!("data" in source)) return;
      const request = source.request;
      if (request.method !== "measure_position" && request.method !== "position_clear" && request.method !== "mdat_import") return;
      const params = request.params;
      const positions = new Set<number>([params.position, ...("assignments" in params ? params.assignments.flatMap((a) => (a.position === null ? [] : [a.position])) : [])]);
      host.set({ warnings: host.getSnapshot().warnings.filter((w) => !positions.has(w.position)) });
    },
  },
  { name: "warnings.replace-sample", when: requested("sample_load"), beforeReload: ({ host }) => { host.set({ warnings: [] }); } },
  {
    name: "rew.driver-response", when: requested("rew_driver_set"),
    beforeReload: ({ source, stores }) => {
      if (!("data" in source)) return;
      stores.rew.set(source.data as import("../ipc/bindings/RewInfoResult").RewInfoResult);
    },
  },
  {
    name: "measurement.refresh", when: requested(...config, "layout_set", "undo"),
    apply: async ({ stores, call }) => { if (stores.measurementSettings.getSnapshot()) await call({ method: "measure_config_get" }); },
  },
  {
    name: "warnings.low-snr", when: (trigger) => "log" in trigger && trigger.log === "measurement_low_snr",
    beforeReload: ({ source, host }) => {
      if (!("event" in source) || source.event.event !== "log" || source.request?.method !== "measure_position") return;
      const { event, request } = source;
      const h = host.getSnapshot();
      const position = request.params.position;
      const old = h.warnings.find((w) => w.position === position);
      host.set({ warnings: [...h.warnings.filter((w) => w.position !== position), { position, notices: [...(old?.notices ?? []), { code: event.code, params: event.params }] }] });
    },
  },
  {
    name: "compute.failure", when: (trigger) => "finished" in trigger && trigger.status === "failed" && ["calibrate", "mimo", "recommend_xo"].includes(trigger.finished),
    beforeReload: ({ source, host }) => {
      if (!("event" in source) || source.event.event !== "finished" || !source.event.error) return;
      const { job, event, request } = source;
      // Cancelled jobs carry an error too; only the failed trigger reaches here.
      host.set({ failure: { job: request?.method === "calibrate" && request.params.subs_only ? "calibrate_subs" : job.kind as "calibrate" | "mimo" | "recommend_xo", error: event.error! } });
    },
  },
  { name: "measurement.finished", when: (trigger) => "finished" in trigger && trigger.finished === "measure", beforeReload: ({ host }) => { host.set({ selected: null }); } },
  {
    name: "xo.result", when: (trigger) => "finished" in trigger,
    beforeReload: ({ source, host }) => {
      if ("event" in source && source.event.event === "finished" && source.event.result?.kind === "xo") host.set({ xoRecommendation: source.event.result.data });
    },
  },
  { name: "export.invalidate", when: (trigger) => "finished" in trigger && trigger.status === "succeeded" && ["calibrate", "mimo"].includes(trigger.finished), beforeReload: ({ host }) => { host.set({ runs: {}, applyRuns: {} }); } },
  {
    name: "export.finished", when: (trigger) => "finished" in trigger,
    beforeReload: ({ source, host, run, call }) => {
      if (!("event" in source) || source.event.event !== "finished" || source.request?.method !== "export") return;
      const { event, request } = source;
      const h = host.getSnapshot();
      if (event.result?.kind === "export") {
        const result = event.result.data;
        host.set({ runs: { ...h.runs, [request.params.kind]: { status: "done", files: result.files, folder: result.folder, rate: result.output_rate, notes: result.notes } } });
        run(() => call({ method: "open_folder", params: { path: result.folder } }));
      } else if (event.status === "failed" && event.error) host.set({ runs: { ...h.runs, [request.params.kind]: { status: "failed", error: event.error } } });
    },
  },
  {
    // 적용 (ADR 0018): the row records how it ended; the status is read again
    // either way, also after an apply another client started.
    name: "apply.finished", when: (trigger) => "finished" in trigger && trigger.finished === "apply",
    beforeReload: ({ source, host, run, stores }) => {
      if (!("event" in source) || source.event.event !== "finished") return;
      const { event, request } = source;
      if (request?.method === "apply_camilla" || request?.method === "apply_eqapo") {
        const target = request.method === "apply_camilla" ? "camilla" : "eqapo";
        const h = host.getSnapshot();
        if (event.result?.kind === "apply") {
          const { outcome, design_id, folder } = event.result.data;
          // 관리 with no CamillaDSP running hands the design to the service, which starts one with it.
          const run = outcome.kind === "rolled_back" ? { status: "rolled_back" as const, error: outcome.error } : { status: "applied" as const, designId: design_id, folder, ...(outcome.kind === "handed_over" ? { handedOver: true as const } : {}) };
          host.set({ applyRuns: { ...h.applyRuns, [target]: run } });
        } else if (event.status === "failed" && event.error) host.set({ applyRuns: { ...h.applyRuns, [target]: { status: "failed", error: event.error } } });
      }
      run(() => stores.applyStatus.load());
    },
  },
  {
    name: "xo.recalibrate", when: (trigger) => "finished" in trigger && trigger.finished === "recommend_xo" && trigger.status === "succeeded",
    // Recommended settings must be reloaded before starting the full calibration.
    // The same MIMO choice 보정 계산 sends (P142): on, with three or more positions.
    apply: async ({ call, stores, source, canContinueRefresh }) => {
      if (!("event" in source)) return;
      const request = source.request;
      if (request?.autoRefreshRevision !== undefined && !canContinueRefresh(request.autoRefreshRevision)) return;
      const settings = stores.settings.getSnapshot();
      const mimo = request?.mimo ?? (!!settings && mimoIncluded(settings, stores.project.getSnapshot()?.positions.length ?? 0));
      await call({ method: "calibrate", params: { settings: null, subs_only: false, mimo, ...(request?.background ? { background: true } : {}) } });
    },
  },
  ...(Object.keys(reloads) as ResourceKey[]).map((key): Rule => {
    const row: { mutations: readonly Request["method"][]; finished: readonly JobKind[] } = reloads[key];
    return {
      name: `reload.${key}`,
      when: (trigger) => "request" in trigger
        ? (PROJECT_REPLACED as readonly string[]).includes(trigger.request) || row.mutations.includes(trigger.request)
        : "finished" in trigger && row.finished.includes(trigger.finished),
      reload: [key],
    };
  }),
];

export function effectsOf(trigger: Trigger): { reload: ResourceKey[]; rules: Rule[] } {
  const rules = RULES.filter((rule) => rule.when(trigger));
  return { reload: [...new Set(rules.flatMap((rule) => rule.reload ?? []))], rules };
}
