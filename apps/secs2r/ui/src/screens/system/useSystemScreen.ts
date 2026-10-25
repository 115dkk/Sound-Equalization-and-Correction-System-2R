import { useMemo } from "react";
import type { DspFields } from "../../ipc/bindings/DspFields";
import type { ConfigSetSpeakerParams } from "../../ipc/bindings/ConfigSetSpeakerParams";
import { useHost, useResource, useSession } from "../../state/react";
import type { SystemActions, SystemView } from "./types";

export function useSystemScreen(configureMeasurement?: () => void): { view: SystemView | null; actions: SystemActions } {
  const session = useSession();
  const host = useHost();
  const config = useResource("config");
  const dsp = useResource("dsp");
  const layout = useResource("layout");
  const project = useResource("project");
  const notices = useResource("notices");
  const estimate = useResource("estimate");
  const settings = useResource("measurementSettings");
  const methods = useResource("methods");
  const rew = useResource("rew");
  const actions = useMemo<SystemActions>(() => {
    const speaker = (index: number, patch: Partial<ConfigSetSpeakerParams>) => session.run(() => session.call({
      method: "config_set_speaker", params: { index, role: null, size: null, band_index: null, out: null, pol: null, ...patch },
    }));
    return {
      configureMeasurement: settings !== null && (host.path === null || (host.path === "native" && methods !== null))
        ? configureMeasurement ?? (() => session.navigate("connect")) : null,
      applyPreset: async (preset) => (await session.call({ method: "config_apply_preset", params: { preset, confirm: true } })).undo_token,
      setSubCount: async (count) => (await session.call({ method: "config_set_sub_count", params: { count, confirm: true } })).undo_token,
      setRole: (index, role) => speaker(index, { role }),
      setSize: (index, size) => speaker(index, { size }),
      setOutput: (index, band_index, out) => speaker(index, { band_index, out }),
      setPolarity: (index, band_index, pol) => speaker(index, { band_index, pol }),
      addSpeaker: () => session.run(() => session.call({ method: "config_add_speaker" })),
      removeSpeaker: (index) => session.run(() => session.call({ method: "config_remove_speaker", params: { index } })),
      renumberOutputs: () => session.run(() => session.call({ method: "config_renumber" })),
      savePatchSheet: () => session.run(async () => {
        const path = await session.call({ method: "dialog_save_file", params: { kind: "patch_sheet", suggested: "secs_patch_sheet.csv" } });
        if (path !== null) await session.call({ method: "patch_sheet_csv", params: { path } });
      }),
      setDspPreset: (preset) => session.run(() => session.writers.dsp.edit({ preset })),
      setDspFields: (fields) => {
        // Controls submit this render's full fields; the writer only needs its edits.
        const patch: Partial<DspFields> = {};
        for (const key of Object.keys(fields) as (keyof DspFields)[]) {
          if (fields[key] !== dsp?.fields[key]) patch[key] = fields[key];
        }
        session.run(() => session.writers.dsp.edit(patch));
      },
      setArrangement: async (count) => (await session.call({ method: "layout_set", params: { count, start_delay: null, timing: null, confirm: true } })).undo_token,
      setTiming: (timing) => session.run(() => session.writers.layout.edit({ timing })),
      undo: async (token) => { await session.call({ method: "undo", params: { token } }); },
      // The label says 연결·점검; on the mdat path that screen shows the skipped note and leads on to 측정·가져오기.
      next: () => session.navigate("connect"),
    };
  }, [session, dsp, configureMeasurement, settings, host.path, methods]);
  // An empty native ID means the system's default output. config.device may
  // still contain a previously selected ID, so it cannot fill this empty value.
  const output = host.path === "rew" ? (rew?.reachable ? rew.output_device : null) : settings?.output_device;
  const nativeDevice = methods?.native_devices.find((device) => device.id === output || device.name === output);
  const rewDevice = rew?.output_devices?.find((device) => device.id === output || device.name === output);
  const state: SystemView["outputDeviceState"] = host.path === "mock" || host.path === "mdat" ? "not_required"
    : settings === null ? "checking"
    : host.path === null ? "unselected"
    : host.path === "rew" ? rew === null ? "checking" : output ? "selected" : "rew_unknown"
    : methods === null ? "checking"
    : methods.native_error || !methods.methods.includes("native") || !methods.native_devices.some((device) => device.output_channels > 0) || (output && (!nativeDevice || nativeDevice.output_channels <= 0)) ? "unavailable"
    : output ? "selected" : "default";
  const outputChannels = state !== "selected" ? null : host.path === "rew" ? rewDevice?.channels : nativeDevice?.output_channels;
  return {
    view: config && dsp && layout && project ? {
      config, dsp, layout, notices: notices ?? [], estimate, outputDeviceState: state, rew: host.path === "rew",
      outputDeviceName: state === "selected" || state === "unavailable" ? (host.path === "rew" ? rewDevice?.name : nativeDevice?.name) ?? output ?? "" : "",
      outputChannels: outputChannels ?? null, outputNames: [], positionsDone: project.positions,
    } : null,
    actions,
  };
}
