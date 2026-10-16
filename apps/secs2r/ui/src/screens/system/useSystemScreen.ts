import { useMemo, useRef } from "react";
import type { DspFields } from "../../ipc/bindings/DspFields";
import type { ConfigSetSpeakerParams } from "../../ipc/bindings/ConfigSetSpeakerParams";
import { useHost, useResource, useSession } from "../../state/react";
import type { SystemActions, SystemView } from "./types";

export function useSystemScreen(): { view: SystemView | null; actions: SystemActions } {
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
  const dspQueue = useRef(Promise.resolve());
  const actions = useMemo<SystemActions>(() => {
    const editDsp = (work: () => Promise<unknown>) => session.run(() => {
      dspQueue.current = dspQueue.current.catch(() => {}).then(async () => { await work(); });
      return dspQueue.current;
    });
    const speaker = (index: number, patch: Partial<ConfigSetSpeakerParams>) => session.run(() => session.call({
      method: "config_set_speaker", params: { index, role: null, size: null, band_index: null, out: null, pol: null, ...patch },
    }));
    return {
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
      setDspPreset: (preset) => editDsp(() => session.call({ method: "dsp_set", params: { preset, fields: null } })),
      setDspFields: (fields) => {
        // Controls submit a complete copy of this render's fields. Preserve only
        // its edits, then rebase them on the previous queued write's result.
        const patch: Partial<DspFields> = {};
        for (const key of Object.keys(fields) as (keyof DspFields)[]) {
          if (fields[key] !== dsp?.fields[key]) patch[key] = fields[key];
        }
        editDsp(() => session.call({ method: "dsp_set", params: { preset: null, fields: { ...(session.stores.dsp.getSnapshot()?.fields ?? fields), ...patch } } }));
      },
      setArrangement: async (count) => (await session.call({ method: "layout_set", params: { count, start_delay: null, timing: null, confirm: true } })).undo_token,
      setStartDelay: (start_delay) => session.run(() => session.call({ method: "layout_set", params: { count: null, start_delay, timing: null, confirm: false } })),
      setTiming: (timing) => session.run(() => session.call({ method: "layout_set", params: { count: null, start_delay: null, timing, confirm: false } })),
      undo: async (token) => { await session.call({ method: "undo", params: { token } }); },
      // The label says 연결·점검; on the mdat path that screen shows the skipped note and leads on to 측정·가져오기.
      next: () => session.navigate("connect"),
    };
  }, [session, dsp]);
  const output = settings?.output_device || config?.device;
  const nativeDevice = methods?.native_devices.find((device) => device.id === output || device.name === output);
  const rewDevice = rew?.output_devices?.find((device) => device.id === output || device.name === output);
  const outputChannels = host.path === "rew" ? rewDevice?.channels : nativeDevice?.output_channels;
  return {
    view: config && dsp && layout && project ? {
      config, dsp, layout, notices: notices ?? [], estimate,
      outputDeviceName: (host.path === "rew" ? rewDevice?.name : nativeDevice?.name) ?? output ?? "",
      outputChannels: outputChannels ?? null, outputNames: [], positionsDone: project.positions,
    } : null,
    actions,
  };
}
