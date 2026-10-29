import { useEffect, useMemo, useRef } from "react";
import type { ApplySettings } from "../../ipc/bindings/ApplySettings";
import { IpcFailure } from "../../ipc/client";
import { useHost, useResource, useSession } from "../../state/react";
import { useJobLedger } from "../../state/useJobLedger";
import type { Patch } from "../../state/writers";
import { DESIGN_RATE, countedRate, extraRatesFor } from "../system/outputRate";
import { CAMILLA_EXE_FILE, DEFAULT_APPLY, addressProblem, applyWanted, designNameFor, isFilterKind, mdatScopeFor, withExePath, type ApplyTarget, type ExportActions, type ExportView, type HostFailure } from "./types";

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
  const appSettings = useResource("appSettings");
  const applyStatus = useResource("applyStatus");
  const dsp = useResource("dsp");
  const methodsRead = useRef<typeof session | null>(null);
  useEffect(() => {
    // Session startup normally reads this already. A screen with no snapshot
    // retries once on mount, including StrictMode's repeated setup.
    if (methodsRead.current === session) return;
    methodsRead.current = session;
    if (!session.stores.methods.getSnapshot()) void session.stores.methods.load();
  }, [session]);
  const applySettings = appSettings?.apply ?? DEFAULT_APPLY;
  const windows = bootstrap?.platform.os === "windows";
  const tutorial = project?.tutorial ?? false;
  // 적용's status is read only while a target is set to 적용 (it probes
  // CamillaDSP's websocket and Equalizer APO's install): when the screen
  // opens, when a target turns to 적용, when the address changes, and for a
  // new result (CamillaDSP may now run this design or another).
  const statusWanted = applyWanted({ tutorial, apply: { settings: applySettings, windows } });
  const managing = statusWanted && (applySettings.camilla.level === "manage" || applySettings.eqapo.level === "manage");
  const calibration = results?.calibration;
  useEffect(() => {
    if (statusWanted && (!managing || (document.visibilityState === "visible" && !session.host.getSnapshot().applyChecking))) void session.stores.applyStatus.load();
  }, [session, statusWanted, managing, applySettings.camilla.address, calibration]);
  useEffect(() => {
    if (!managing) return;
    let timer: ReturnType<typeof setInterval> | undefined;
    const refresh = () => {
      // Other refreshes share this busy flag; a slow read never adds polling requests.
      if (document.visibilityState === "visible" && !session.host.getSnapshot().applyChecking) void session.stores.applyStatus.load();
    };
    const restart = () => {
      clearInterval(timer);
      if (document.visibilityState !== "visible") return;
      refresh();
      timer = setInterval(refresh, 3000);
    };
    // The effect above already reads on mount. Showing the page again reads at once.
    if (document.visibilityState === "visible") timer = setInterval(refresh, 3000);
    document.addEventListener("visibilitychange", restart);
    return () => {
      clearInterval(timer);
      document.removeEventListener("visibilitychange", restart);
    };
  }, [session, managing]);
  const actions = useMemo<ExportActions>(() => {
    // The last ApplySettings save: an apply waits for it, so the service applies with the level and address on screen.
    let saving: Promise<unknown> = Promise.resolve();
    const editApply = (patch: Patch<ApplySettings>): Promise<boolean> => {
      const edit = session.writers.apply.edit(patch);
      saving = edit;
      return edit;
    };
    const storedAddress = () => session.stores.appSettings.getSnapshot()?.apply.camilla.address ?? DEFAULT_APPLY.camilla.address;
    const saveAddress = async (): Promise<void> => {
      const draft = session.host.getSnapshot().applyAddressDraft;
      if (draft === null || addressProblem(draft)) return;
      const address = draft.trim();
      if (address !== storedAddress() && !(await editApply((latest) => ({ camilla: { ...latest.camilla, address } })))) return;
      if (session.host.getSnapshot().applyAddressDraft === draft) session.host.set({ applyAddressDraft: null });
    };
    const storedExe = () => session.stores.appSettings.getSnapshot()?.apply.camilla.exe_path ?? "";
    // 관리's executable: typed freely, stored trimmed (an empty field stores none); any text is a path to the service.
    const saveExe = async (): Promise<void> => {
      const draft = session.host.getSnapshot().applyExeDraft;
      if (draft === null) return;
      const path = draft.trim();
      if (path !== storedExe() && !(await editApply((latest) => ({ camilla: withExePath(latest.camilla, path) })))) return;
      if (session.host.getSnapshot().applyExeDraft === draft) session.host.set({ applyExeDraft: null });
    };
    // A host action in flight, and its typed failure kept for the row it was started from (session.call does not raise the global notice for it).
    // The button stays busy until the status has been read again, so the row never shows the state from before the action.
    const hostAction = (action: HostFailure["action"], target: ApplyTarget, call: () => Promise<unknown>) => session.run(async () => {
      const busy = (on: boolean) => session.host.set(action === "install" ? { hostInstalling: on } : { hostStopping: on });
      session.host.set({ hostFailure: null });
      busy(true);
      try {
        await call();
      } catch (error) {
        if (error instanceof IpcFailure) session.host.set({ hostFailure: { action, target, error: error.error } });
        throw error;
      } finally {
        await session.stores.applyStatus.load();
        busy(false);
      }
    });
    // A refusal before any job starts (no write access, not installed, …) has no
    // finished event to land in the row. Never attach an old project's.
    const refused = (target: ApplyTarget, epoch: number, error: unknown) => {
      if (epoch !== session.jobs.epoch || !(error instanceof IpcFailure)) return;
      session.host.set({ applyRuns: { ...session.host.getSnapshot().applyRuns, [target]: { status: "failed", error: error.error } } });
    };
    return {
      exportFiles: (kind) => {
        if (session.stores.project.getSnapshot()?.tutorial) {
          session.host.set({ runs: { ...session.host.getSnapshot().runs, [kind]: { status: "tutorial" } } });
        } else session.run(async () => {
          const current = session.host.getSnapshot();
          const epoch = session.jobs.epoch;
          const name = isFilterKind(kind) ? designNameFor({ designName: current.designName, tutorial: false }) : null;
          const devices = current.camillaDevices;
          // The rates other than the one the files are written at (48 kHz for 자동), as the DSP stands now.
          const choice = session.stores.dsp.getSnapshot()?.fields.output_rate;
          const extra = kind === "convolution" ? extraRatesFor(choice ? countedRate(choice) : DESIGN_RATE, current.extraRates) : [];
          try {
            await session.call({ method: "export", params: {
              kind, folder: "",
              ...(name !== null ? { design_name: name } : {}),
              ...(kind === "camilla_dsp" && (devices.capture.type !== "file" || devices.playback.type !== "file") ? { camilla_devices: devices } : {}),
              ...(kind === "before_after_mdat" ? { mdat_scope: mdatScopeFor(session.stores.results.getSnapshot()?.calibration, current.mdatScope) } : {}),
              ...(extra.length > 0 ? { extra_rates: extra } : {}),
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
      setExtraRates: (rates) => session.host.set({ extraRates: rates }),
      refreshDevices: () => void session.stores.methods.load(),
      changeFolder: () => session.run(async () => {
        const selected = await session.call({ method: "dialog_pick_folder", params: { start: session.stores.folder.getSnapshot()?.folder ?? null } });
        if (selected !== null) await session.call({ method: "export_folder_set", params: { folder: selected } });
      }),
      openFolder: (path) => session.run(() => session.call({ method: "open_folder", params: { path } })),
      goToCalibrate: () => session.navigate("calibrate"),
      goToSystem: () => session.navigate("system"),
      recalibrate: () => session.run(() => session.call({ method: "calibrate", params: { settings: null, subs_only: false } })),
      setApplyLevel: (target, level) => {
        void editApply((latest) => (target === "camilla" ? { camilla: { ...latest.camilla, level } } : { eqapo: { ...latest.eqapo, level } }));
      },
      setCamillaAddress: (text) => session.host.set({ applyAddressDraft: text === storedAddress() ? null : text }),
      saveCamillaAddress: () => void saveAddress(),
      refreshApplyStatus: () => void session.stores.applyStatus.load(),
      applyCamilla: () => session.run(async () => {
        const epoch = session.jobs.epoch;
        // Leaving the address or executable field for this button already saves it; Enter-less keyboard paths save here.
        await saveAddress();
        await saveExe();
        await saving;
        const devices = session.host.getSnapshot().camillaDevices;
        try {
          await session.call({ method: "apply_camilla", params: devices.capture.type !== "file" || devices.playback.type !== "file" ? { camilla_devices: devices } : {} });
        } catch (error) {
          refused("camilla", epoch, error);
          throw error;
        }
      }),
      applyEqapo: () => session.run(async () => {
        const epoch = session.jobs.epoch;
        await saving;
        try {
          await session.call({ method: "apply_eqapo" });
        } catch (error) {
          refused("eqapo", epoch, error);
          throw error;
        }
      }),
      revertEqapo: () => session.run(async () => {
        const epoch = session.jobs.epoch;
        session.host.set({ applyReverting: true });
        try {
          await session.call({ method: "revert_eqapo" });
          if (epoch === session.jobs.epoch) session.host.set({ applyRuns: { ...session.host.getSnapshot().applyRuns, eqapo: { status: "reverted" } } });
        } catch (error) {
          refused("eqapo", epoch, error);
          throw error;
        } finally {
          session.host.set({ applyReverting: false });
          await session.stores.applyStatus.load();
        }
      }),
      setCamillaExe: (text) => session.host.set({ applyExeDraft: text === storedExe() ? null : text }),
      saveCamillaExe: () => void saveExe(),
      pickCamillaExe: () => session.run(async () => {
        const picked = await session.call({ method: "dialog_open_file", params: { kind: CAMILLA_EXE_FILE } });
        if (picked === null) return;
        session.host.set({ applyExeDraft: null });
        await editApply((latest) => ({ camilla: withExePath(latest.camilla, picked) }));
      }),
      installHost: (target) => hostAction("install", target, () => session.call({ method: "host_install" })),
      stopManaging: (target) => hostAction("stop", target, () => session.call({ method: "host_stop_managing" })),
    };
  }, [session]);
  const jobs = ledger.jobs.filter((job) => job.status === "running" || job.status === "cancel_requested");
  const recalibrating = jobs.some((job) => job.kind === "calibrate");
  const exporting = jobs.find((job) => job.kind === "export");
  const request = exporting?.request;
  const applyRequest = jobs.find((job) => job.kind === "apply")?.request;
  return { view: results && settings && project ? {
    results, settings: { mode: results.calibration?.mode ?? settings.mode, ultra_ms: results.calibration?.ultra_ms ?? settings.ultra_ms, mimo_on: settings.mimo_on },
    positions: project.positions.length, rc: lines?.rc ?? [], folder,
    computing: recalibrating, recalibrating,
    exporting: request?.method === "export" ? request.params.kind : null,
    runs: host.runs, mdatScope: host.mdatScope, tutorial: project.tutorial,
    designName: host.designName, camillaDevices: host.camillaDevices,
    nativeDevices: bootstrap?.platform.os === "windows" && !methods?.native_error && methods?.native_devices.length ? methods.native_devices : null,
    outputRate: dsp?.fields.output_rate ?? null, extraRates: host.extraRates,
    apply: {
      settings: applySettings, addressDraft: host.applyAddressDraft, exeDraft: host.applyExeDraft, status: applyStatus, checking: host.applyChecking, windows,
      host: {
        service: applyStatus?.host ?? null, status: applyStatus?.host === "running" ? applyStatus.host_status ?? null : null,
        installing: host.hostInstalling, stopping: host.hostStopping, failure: host.hostFailure, now: Date.now(),
      },
      applying: applyRequest?.method === "apply_camilla" ? "camilla" : applyRequest?.method === "apply_eqapo" ? "eqapo" : null,
      reverting: host.applyReverting, runs: host.applyRuns,
    },
  } : null, actions };
}
