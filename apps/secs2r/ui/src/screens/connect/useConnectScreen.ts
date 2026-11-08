import { useEffect, useMemo, useRef } from "react";
import { IpcFailure } from "../../ipc/client";
import { useHost, useLevelMonitor, useResource, useSession } from "../../state/react";
import { levelRoles, pathReady } from "./state";
import type { ConnectActions, ConnectState, ConnectView, LiveLevel, RewProblem } from "./types";

/**
 * 연결·점검's state and actions. The level monitor's live fields are not in
 * it: the frame runs this hook with the other screens' hooks, and a Meter
 * event would run them all again (TD-176). `useLiveLevel` reads them where
 * the meter is drawn.
 */
export function useConnectScreen(active: boolean): { view: ConnectState | null; actions: ConnectActions } {
  const session = useSession();
  const host = useHost();
  const playing = useLevelMonitor((level) => level.playing);
  const playingAll = useLevelMonitor((level) => level.playingAll);
  const masterDbfs = useLevelMonitor((level) => level.masterDbfs);
  const config = useResource("config");
  const methods = useResource("methods");
  const settings = useResource("measurementSettings");
  const levels = useResource("levels");
  const rew = useResource("rew");
  const micCalibration = useResource("micCalibration");
  const queue = useRef(0);
  const cancelWait = useRef<(() => void) | null>(null);
  const ready = host.path !== "mdat" && settings !== null && pathReady({ path: host.path, methods });
  useEffect(() => () => { queue.current++; cancelWait.current?.(); }, []);
  useEffect(() => {
    if (active) session.run(() => session.stores.rew.load());
  }, [active, session]);
  useEffect(() => {
    if (active && host.path === "native") session.run(() => session.stores.micCalibration.load());
  }, [active, host.path, session]);
  useEffect(() => {
    if (!active || !ready) return;
    session.run(() => session.monitor(true));
    return () => {
      queue.current++;
      cancelWait.current?.();
      session.stores.monitor.set({ playing: null, playingAll: false });
      session.run(() => Promise.all([
        session.call({ method: "level_stop" }),
        session.monitor(false),
      ]));
    };
  }, [active, ready, host.path, settings?.input_device, settings?.input_channel, settings?.output_device, settings?.share_mode, session]);
  useEffect(() => {
    if (methods?.rew.reachable && host.rewLaunch !== "idle") session.host.set({ rewLaunch: "idle", rewProblem: null, rewProblemDetail: null });
  }, [methods?.rew.reachable, host.rewLaunch, session]);
  const actions = useMemo<ConnectActions>(() => {
    const stop = async () => {
      queue.current++;
      cancelWait.current?.();
      session.stores.monitor.set({ playing: null, playingAll: false });
      await session.call({ method: "level_stop" });
    };
    const play = (roles: string[], all: boolean) => {
      const epoch = ++queue.current;
      cancelWait.current?.();
      // A stop, another play, or the monitor job failing ends the queue; the
      // failure stays on the mic panel until the monitor starts again.
      const ended = () => epoch !== queue.current || session.stores.monitor.getSnapshot().monitorError !== null;
      session.run(async () => {
        session.stores.monitor.set({ playingAll: all });
        try {
          await session.call({ method: "level_stop" });
          if (epoch !== queue.current) return;
          // Also restarts a monitor that failed, which clears its error.
          await session.monitor(true);
          for (const role of roles) {
            if (ended()) break;
            session.stores.monitor.set({ playing: role });
            // level_play only enqueues the 2.5 s burst; the monitor job says
            // when it ended (level_play_end), after storing the level. Listen
            // before asking, and give up after 8 s so a lost event cannot stall the queue.
            // A failed monitor sends no end: its failure ends the wait.
            let release = () => {};
            const burst = new Promise<void>((resolve) => {
              let timer: ReturnType<typeof setTimeout> | undefined;
              let off = () => {};
              let offFailure = () => {};
              const finish = () => { off(); offFailure(); clearTimeout(timer); if (cancelWait.current === finish) cancelWait.current = null; resolve(); };
              release = finish;
              off = session.ipc.events.subscribe((event) => {
                if (event.event === "log" && event.code === "level_play_end" && event.params.role === role) finish();
              });
              offFailure = session.stores.monitor.subscribe(() => { if (session.stores.monitor.getSnapshot().monitorError !== null) finish(); });
              timer = setTimeout(finish, 8000);
              cancelWait.current = finish;
            });
            try {
              await session.call({ method: "level_play", params: { role, master_dbfs: session.stores.monitor.getSnapshot().masterDbfs } });
              if (ended()) break;
              await burst;
            } finally { release(); }
            if (ended()) break;
            await Promise.all([session.stores.levels.load(), session.stores.project.load()]);
          }
        } finally {
          if (epoch === queue.current) session.stores.monitor.set({ playing: null, playingAll: false });
        }
      });
    };
    const micCal = (busy: NonNullable<ConnectView["micCalibration"]["busy"]>, work: (guard: ReturnType<typeof session.projectGuard>) => Promise<unknown>) => session.run(async () => {
      if (session.host.getSnapshot().micCalBusy !== null) return;
      const guard = session.projectGuard();
      session.host.set({ micCalBusy: busy, micCalError: null });
      try { await work(guard); }
      catch (error) {
        if (!(error instanceof IpcFailure)) throw error;
        if (guard.alive()) session.host.set({ micCalError: error.error });
      } finally {
        if (guard.alive()) session.host.set({ micCalBusy: null });
      }
    });
    return {
      pickMicCalibration: () => micCal("pick", async (guard) => {
        const path = await session.call({ method: "dialog_open_file", params: { kind: "mic_calibration" } });
        if (path !== null && guard.alive()) await session.call({ method: "mic_calibration_set", params: { path } });
      }),
      clearMicCalibration: () => micCal("clear", () => session.call({ method: "mic_calibration_clear" })),
      attachMicCalibration: () => micCal("attach", () => session.call({ method: "mic_calibration_attach" })),
      choosePath: (path) => session.run(async () => {
        await stop();
        if (session.host.getSnapshot().path !== path) await session.monitor(false);
        await session.writers.measurement.edit({ method: path });
      }),
      refreshMethods: () => session.run(() => session.stores.methods.load()),
      launchRew: () => session.run(async () => {
        session.host.set({ rewLaunch: "checking", rewProblem: null, rewProblemDetail: null });
        try {
          const status = await session.call({ method: "rew_launch" });
          session.host.set({ rewLaunch: status.reachable ? "idle" : "launching" });
        } catch (error) {
          const reason = error instanceof IpcFailure ? error.error.params.reason : null;
          const problem: RewProblem = reason === "exe_not_found" || reason === "cannot_launch" || reason === "timed_out" ? reason : "launch_failed";
          session.host.set({ rewLaunch: "idle", rewProblem: problem, rewProblemDetail: error instanceof Error ? error.message : String(error) });
          throw error;
        }
      }),
      readRewSettings: () => session.run(async () => {
        session.host.set({ readingRew: true });
        try { await session.stores.rew.load(); }
        finally { session.host.set({ readingRew: false }); }
      }),
      setRewDriver: (driver) => session.run(async () => {
        session.host.set({ switchingRewDriver: true });
        try { await session.call({ method: "rew_driver_set", params: { driver } }); }
        finally { session.host.set({ switchingRewDriver: false }); }
      }),
      // The method the service holds, with the path ("" for the installed REW); a refusal stays at the field.
      setRewExecutable: (rew_executable) => session.run(() => session.writers.measurement.edit({ rew_executable })),
      // On the REW path measure_configure also opens the device in REW; read REW again for its channels.
      setInputDevice: (input_device) => session.run(async () => {
        if (!await session.writers.measurement.edit({ input_device })) return;
        if (session.host.getSnapshot().path === "rew") await session.stores.rew.load();
      }),
      setInputChannel: (input_channel) => session.run(() => session.writers.measurement.edit({ input_channel })),
      setOutputDevice: (output_device) => session.run(async () => {
        if (!await session.writers.measurement.edit({ output_device })) return;
        if (session.host.getSnapshot().path === "rew") await session.stores.rew.load();
      }),
      setShareMode: (share_mode) => session.run(() => session.writers.measurement.edit({ share_mode })),
      playLevel: (role) => play([role], false),
      stopLevel: () => session.run(stop),
      playAllLevels: () => {
        const current = session.stores.config.getSnapshot();
        if (current) play(levelRoles({ config: current }), true);
      },
      setMasterLevel: (masterDbfs) => session.stores.monitor.set({ masterDbfs }),
      skipLevels: () => session.run(() => session.call({ method: "levels_fill_defaults" })),
      next: () => session.navigate("measure"),
    };
  }, [session]);
  const details: ConnectView["rew"]["details"] = rew ? {
    executable: rew.executable_path, version: rew.version, driver: rew.driver, blocking: rew.blocking,
    micCalibration: rew.input_calibration_file,
    inputs: rew.input_devices?.map((device) => ({ id: device.id, name: device.name, input_channels: device.channels, output_channels: 0 })),
    outputs: rew.output_devices?.map((device) => ({ id: device.id, name: device.name, input_channels: 0, output_channels: device.channels })),
    inputChannelNames: rew.input_channel_names ?? undefined,
    current: rew.reachable ? { inputDevice: rew.input_device, outputDevice: rew.output_device, input: rew.input } : undefined,
    settings: rew.sample_rate !== null && rew.sweep?.length != null ? { sample_rate: rew.sample_rate, sweep_length: rew.sweep.length, level_dbfs: rew.level_dbfs } : null,
  } : {};
  return { view: config ? {
    path: host.path, methods, settings, config, sampleRate: settings?.sample_rate ?? 48000,
    rew: { launch: host.rewLaunch, problem: host.rewProblem, problemDetail: host.rewProblemDetail, readingSettings: host.readingRew, switchingDriver: host.switchingRewDriver, details, executableError: host.rewExecutableError },
    micCalibration: { status: micCalibration, busy: host.micCalBusy, error: host.micCalError },
    levels: levels?.levels ?? {},
    level: { playing, playingAll, masterDbfs },
  } : null, actions };
}

/** The level monitor's fields the mic meter draws, read in the component that draws it, which alone renders again on a Meter event (TD-176). */
export function useLiveLevel(): LiveLevel {
  const level = useResource("monitor");
  return useMemo(() => ({ monitor: level.monitor, error: level.monitorError, reading: level.meter }), [level.monitor, level.monitorError, level.meter]);
}

/** 연결·점검's whole view: the hook's state with the live level fields. */
export function withLiveLevel(state: ConnectState, live: LiveLevel): ConnectView {
  return { ...state, level: { ...state.level, ...live } };
}
