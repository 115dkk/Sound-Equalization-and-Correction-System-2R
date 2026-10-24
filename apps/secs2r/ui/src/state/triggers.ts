import type { JobKind } from "../ipc/bindings/JobKind";
import type { Request } from "../ipc/bindings/Request";

const config = ["config_apply_preset", "config_set_sub_count", "config_set_speaker", "config_add_speaker", "config_remove_speaker", "config_renumber"] as const;
const measurements = ["position_clear", "undo", "layout_set", ...config] as const;
const settings = ["calibration_settings_set", "calibration_settings_edit", "sub_take_values"] as const;
const target = ["target_set_points", "target_apply_preset", "target_load"] as const;
const recorded = ["measure", "sample_load", "mdat_import"] as const;
const inputs = [...measurements, ...settings, ...target, "dsp_set", "measure_configure", "level_apply"] as const;

/** Project replacement reloads all project resources, including empty results. */
export const PROJECT_REPLACED = ["project_new", "project_open"] as const;
export const TRIGGERS = {
  project: { mutations: [...inputs, "project_save", "levels_fill_defaults", "export_folder_set"], finished: [...recorded, "calibrate", "mimo"] },
  config: { mutations: [...config, "undo", "measure_configure"], finished: [] },
  notices: { mutations: [...config, "undo", "measure_configure"], finished: [] },
  dsp: { mutations: ["dsp_set"], finished: [] },
  layout: { mutations: ["layout_set", "undo", ...config], finished: [] },
  estimate: { mutations: [...config, "layout_set", "measure_configure", "undo"], finished: [] },
  methods: { mutations: ["rew_launch", "rew_driver_set"], finished: [] },
  records: { mutations: measurements, finished: recorded },
  curves: { mutations: measurements, finished: recorded },
  levels: { mutations: ["level_apply", "levels_fill_defaults", "level_play", ...config], finished: ["level_check"] },
  settings: { mutations: [...settings, ...config, "undo"], finished: ["recommend_xo"] },
  modeSpec: { mutations: settings, finished: [] },
  target: { mutations: target, finished: [] },
  results: { mutations: [...inputs, "app_settings_set", "project_save"], finished: [...recorded, "calibrate", "mimo"] },
  resultCurves: { mutations: [...inputs, "app_settings_set"], finished: [...recorded, "calibrate", "mimo"] },
  lines: { mutations: [...inputs, "app_settings_set"], finished: [...recorded, "calibrate", "mimo"] },
  subWarnings: { mutations: [...settings, ...config, "undo"], finished: ["calibrate"] },
  rew: { mutations: ["rew_launch"], finished: [] },
  folder: { mutations: ["export_folder_set", "project_save"], finished: ["export"] },
  computeStatus: { mutations: ["app_settings_set"], finished: [] },
  appSettings: { mutations: ["app_settings_set", "project_open", "project_save", "project_new"], finished: [] },
} as const satisfies Record<string, { mutations: readonly Request["method"][]; finished: readonly JobKind[] }>;

/** Response-only / event-owned state has no getter in the current protocol. */
export const HOST_TRIGGERS = {
  measurementSettings: { mutations: [...config, "layout_set", "undo"], responses: ["measure_configure", "measure_config_get", "level_apply"] },
} as const;

type ResourceKey = keyof typeof TRIGGERS;
export function triggeredBy(trigger: { mutation: Request["method"] } | { finished: JobKind }): ResourceKey[] {
  return (Object.keys(TRIGGERS) as ResourceKey[]).filter((key) => {
    const row: { mutations: readonly Request["method"][]; finished: readonly JobKind[] } = TRIGGERS[key];
    return "mutation" in trigger
      ? (PROJECT_REPLACED as readonly string[]).includes(trigger.mutation) || row.mutations.includes(trigger.mutation)
      : row.finished.includes(trigger.finished);
  });
}
