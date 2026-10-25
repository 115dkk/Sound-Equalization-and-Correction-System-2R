// The service's no-input responses, recorded by
// crates/secs-service/tests/recorded.rs on a freshly created project
// (ADR 0014). Each file lists `{ params, data }` per request; the mock
// answers with `data` unchanged. Service output: never edit these by hand,
// rerun `SECS_RECORD=1 cargo test -p secs-service --test recorded`.
import type { Request } from "./bindings/Request";
import type { Response } from "./bindings/Response";
import calibrationSettingsGet from "./recorded/calibration_settings_get.json";
import configApplyPreset from "./recorded/config_apply_preset.json";
import modeSpec from "./recorded/mode_spec.json";
import targetApplyPreset from "./recorded/target_apply_preset.json";

type Method = Request["method"];
type ParamsOf<M extends Method> = Extract<Request, { method: M }> extends { params: infer P } ? P : null;
export type RecordedEntry<M extends Method> = { params: ParamsOf<M>; data: Extract<Response, { method: M }>["data"] };
export type RecordedTable = { [M in Method]?: readonly RecordedEntry<M>[] };

export const RECORDED = {
  mode_spec: modeSpec as RecordedEntry<"mode_spec">[],
  config_apply_preset: configApplyPreset as RecordedEntry<"config_apply_preset">[],
  target_apply_preset: targetApplyPreset as RecordedEntry<"target_apply_preset">[],
  calibration_settings_get: calibrationSettingsGet as RecordedEntry<"calibration_settings_get">[],
} satisfies RecordedTable;

/** A copy of the recorded data of the first entry whose params match, if any. */
export function recorded<M extends keyof typeof RECORDED>(
  method: M,
  match: (params: (typeof RECORDED)[M][number]["params"]) => boolean = () => true,
): (typeof RECORDED)[M][number]["data"] | undefined {
  const entries: readonly { params: (typeof RECORDED)[M][number]["params"]; data: (typeof RECORDED)[M][number]["data"] }[] = RECORDED[method];
  const entry = entries.find((e) => match(e.params));
  return entry ? structuredClone(entry.data) : undefined;
}
