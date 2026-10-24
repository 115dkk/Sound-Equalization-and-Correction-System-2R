import { useEffect, useMemo, useRef } from "react";
import { IpcFailure } from "../../ipc/client";
import { useHost, useResource, useSession } from "../../state/react";
import { levelRoles, pathReady } from "./state";
import type { ConnectActions, ConnectView, RewProblem } from "./types";

export function useConnectScreen(active: boolean): { view: ConnectView | null; actions: ConnectActions } {
  const session = useSession();
  const host = useHost();
  const config = useResource("config");
  const methods = useResource("methods");
  const settings = useResource("measurementSettings");
  const levels = useResource("levels");
  const rew = useResource("rew");
  const queue = useRef(0);
  const cancelWait = useRef<(() => void) | null>(null);
  const sweepRunning = host.jobs.some((job) => job.kind === "level_check" && (job.status === "running" || job.status === "cancel_requested"));
  const ready = host.path !== "mdat" && settings !== null && pathReady({ path: host.path, methods });
  useEffect(() => () => { queue.current++; cancelWait.current?.(); }, []);
  useEffect(() => {
    if (active) session.run(() => session.stores.rew.load());
  }, [active, session]);
  useEffect(() => {
    if (!active || !ready || sweepRunning) return;
    session.run(() => session.monitor(true));
    return () => {
      queue.current++;
      cancelWait.current?.();
      session.host.set({ playing: null, playingAll: false });
      session.run(() => Promise.all([
        session.call({ method: "level_stop" }),
        session.monitor(false),
      ]));
    };
  }, [active, ready, host.path, sweepRunning, settings?.input_device, settings?.input_channel, settings?.output_device, settings?.share_mode, session]);
  useEffect(() => {
    if (methods?.rew.reachable && host.rewLaunch !== "idle") session.host.set({ rewLaunch: "idle", rewProblem: null, rewProblemDetail: null });
  }, [methods?.rew.reachable, host.rewLaunch, session]);
  const actions = useMemo<ConnectActions>(() => {
    const stop = async () => {
      queue.current++;
      cancelWait.current?.();
      session.host.set({ playing: null, playingAll: false });
      await session.call({ method: "level_stop" });
    };
    const play = (roles: string[], all: boolean) => {
      const epoch = ++queue.current;
      cancelWait.current?.();
      session.run(async () => {
        session.host.set({ playingAll: all });
        try {
          await session.call({ method: "level_stop" });
          if (epoch !== queue.current) return;
          await session.monitor(true);
          for (const role of roles) {
            if (epoch !== queue.current) break;
            session.host.set({ playing: role });
            // level_play only enqueues the 2.5 s burst; the monitor job says
            // when it ended (level_play_end), after storing the level. Listen
            // before asking, and give up after 8 s so a lost event cannot stall the queue.
            let release = () => {};
            const ended = new Promise<void>((resolve) => {
              let timer: ReturnType<typeof setTimeout> | undefined;
              let off = () => {};
              const finish = () => { off(); clearTimeout(timer); if (cancelWait.current === finish) cancelWait.current = null; resolve(); };
              release = finish;
              off = session.ipc.events.subscribe((event) => {
                if (event.event === "log" && event.code === "level_play_end" && event.params.role === role) finish();
              });
              timer = setTimeout(finish, 8000);
              cancelWait.current = finish;
            });
            try {
              await session.call({ method: "level_play", params: { role, master_dbfs: session.host.getSnapshot().masterDbfs } });
              if (epoch !== queue.current) break;
              await ended;
            } finally { release(); }
            if (epoch !== queue.current) break;
            await Promise.all([session.stores.levels.load(), session.stores.project.load()]);
          }
        } finally {
          if (epoch === queue.current) session.host.set({ playing: null, playingAll: false });
        }
      });
    };
    return {
      choosePath: (path) => session.run(async () => {
        await stop();
        if (session.host.getSnapshot().path !== path) await session.monitor(false);
        await session.configure({ method: path });
      }),
      refreshMethods: () => session.run(() => session.stores.methods.load()),
      launchRew: () => session.run(async () => {
        session.host.set({ rewLaunch: "checking", rewProblem: null, rewProblemDetail: null });
        try {
          const status = await session.call({ method: "rew_launch" });
          session.host.set({ rewLaunch: status.reachable ? "idle" : "launching" });
          await Promise.all([session.stores.methods.load(), session.stores.rew.load()]);
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
      // On the REW path measure_configure also opens the device in REW; read REW again for its channels.
      setInputDevice: (input_device) => session.run(async () => {
        await session.configure({ input_device });
        if (session.host.getSnapshot().path === "rew") await session.stores.rew.load();
      }),
      setInputChannel: (input_channel) => session.run(() => session.configure({ input_channel })),
      setOutputDevice: (output_device) => session.run(async () => {
        await session.configure({ output_device });
        if (session.host.getSnapshot().path === "rew") await session.stores.rew.load();
      }),
      setSweepLength: (sweep_length) => session.run(() => session.configure({ sweep_length })),
      setShareMode: (share_mode) => session.run(() => session.configure({ share_mode })),
      playLevel: (role) => play([role], false),
      stopLevel: () => session.run(stop),
      playAllLevels: () => {
        const current = session.stores.config.getSnapshot();
        if (current) play(levelRoles({ config: current }), true);
      },
      setMasterLevel: (masterDbfs) => session.host.set({ masterDbfs }),
      skipLevels: () => session.run(() => session.call({ method: "levels_fill_defaults" })),
      checkSweepLevel: (role) => session.run(async () => {
        await stop();
        await session.monitor(false);
        session.host.set({ sweepRole: role, sweepResult: null });
        await session.call({ method: "level_check", params: { role } });
      }),
      applySweepLevel: (level) => session.run(() => session.call({ method: "level_apply", params: { level } })),
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
    rew: { launch: host.rewLaunch, problem: host.rewProblem, problemDetail: host.rewProblemDetail, readingSettings: host.readingRew, switchingDriver: host.switchingRewDriver, details },
    levels: levels?.levels ?? {},
    level: { monitor: host.monitor, reading: host.meter, playing: host.playing, playingAll: host.playingAll, masterDbfs: host.masterDbfs },
    sweepCheck: { running: sweepRunning, result: host.sweepResult, role: host.sweepRole },
  } : null, actions };
}
