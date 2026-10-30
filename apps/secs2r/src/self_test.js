// The self-test driver (self_test.rs). It runs in the app's own page once the
// page has loaded and sends what a user does through `secs_call`, the command
// the screen uses: a mock-room measurement of the sample project, a
// calibration and two exports. Every answer goes into the report.
(async () => {
  const work = __SECS2R_SELF_TEST_WORK__;
  const audioDevice = __SECS2R_SELF_TEST_AUDIO__;
  const internals = window.__TAURI_INTERNALS__;
  const started = Date.now();
  const steps = [];
  const facts = {};
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

  function eventChannel(onmessage) {
    const id = internals.transformCallback((frame) => {
      const message = frame && typeof frame === "object" && Object.hasOwn(frame, "message")
        ? frame.message : frame;
      onmessage(message);
    });
    return `__CHANNEL__:${id}`;
  }

  async function send(method, params, events) {
    const request = params === undefined ? { method } : { method, params };
    const args = events === undefined ? { request } : { request, events };
    const envelope = await internals.invoke("secs_call", args);
    if (!envelope.ok) {
      throw new Error(`${method}: ${envelope.error.code}: ${envelope.error.message}`);
    }
    return envelope.data.data;
  }

  async function call(method, params) {
    const at = Date.now();
    const data = await send(method, params);
    steps.push({ method, ms: Date.now() - at });
    return data;
  }

  // Starts a job and waits for it, as the screen does, by reading the job list.
  async function job(method, params) {
    const at = Date.now();
    const { job: id } = await send(method, params);
    for (;;) {
      const snapshot = (await send("jobs")).find((entry) => entry.job === id);
      if (snapshot && ["succeeded", "failed", "cancelled"].includes(snapshot.status)) {
        steps.push({ method, ms: Date.now() - at, status: snapshot.status });
        if (snapshot.status !== "succeeded") {
          const error = snapshot.error ? `${snapshot.error.code}: ${snapshot.error.message}` : "no error";
          throw jobError(`${method} job ${snapshot.status}: ${error}`, snapshot.error);
        }
        return;
      }
      if (Date.now() - at > 240000) throw new Error(`${method} job did not finish in 240 s`);
      await sleep(200);
    }
  }

  // A failed job's error, marked for a retry when the backend says a new
  // recording may succeed (a dropped capture cycle, for example).
  function jobError(message, error) {
    const failure = new Error(message);
    failure.retryable = error?.code === "recording_failed" && error.retryable === true;
    return failure;
  }

  // Runs one native audio stage, retrying a retryable recording failure as a
  // user would press the button again: at most three attempts, and every
  // failed attempt stays in the report.
  async function attempts(stage, run) {
    for (let attempt = 1; ; attempt += 1) {
      try {
        return await run();
      } catch (failure) {
        if (!failure.retryable || attempt === 3) throw failure;
        facts.audio.retries.push({ stage, attempt, error: failure.message });
      }
    }
  }

  async function screenRendered() {
    const at = Date.now();
    while (!(document.getElementById("root")?.childElementCount > 0)) {
      if (Date.now() - at > 30000) throw new Error("the screen did not render in 30 s");
      await sleep(100);
    }
    steps.push({ method: "screen", ms: Date.now() - at });
  }

  // The native run is separate from the mock project and never fills default
  // levels. A stored level appears only after the 2.5 s burst was captured and
  // level_check_streaming returned; polling it waits for playback completion.
  async function audioCall(method, params, events) {
    facts.audio.request = params === undefined ? { method } : { method, params };
    const at = Date.now();
    const data = await send(method, params, events);
    steps.push({ method, ms: Date.now() - at });
    return data;
  }

  async function stopMonitor(id) {
    await audioCall("level_monitor_stop");
    const at = Date.now();
    for (;;) {
      const state = (await audioCall("jobs")).find((entry) => entry.job === id);
      if (state && ["succeeded", "cancelled"].includes(state.status)) return;
      if (state?.status === "failed") {
        throw new Error(`level_monitor_stop: ${JSON.stringify(state.error)}`);
      }
      if (Date.now() - at > 30000) throw new Error("level_monitor_stop: no exit in 30 s");
      await sleep(200);
    }
  }

  async function nativeAudio() {
    const audio = facts.audio = {
      ok: false, device: audioDevice, configurations: [], levels: {}, played_dbfs: -20, retries: [],
    };
    await audioCall("project_new", {
      path: `${work}/audio.secs`, preset: "2.0", sub_count: 0, sample: false, tutorial: false,
    });
    const methods = await audioCall("measure_methods");
    audio.methods = methods;
    const output = methods.native_devices.filter((device) => device.name === audioDevice && device.output_channels >= 2);
    const input = methods.native_devices.filter((device) => device.name === audioDevice && device.input_channels >= 2);
    if (!methods.methods.includes("native") || output.length !== 1 || input.length !== 1) {
      throw new Error(`measure_methods: expected one stereo input and output named ${audioDevice}`);
    }
    audio.input_device = input[0].id;
    audio.output_device = output[0].id;
    async function configure(stage, inputChannel, outputChannels) {
      await audioCall("measure_configure", {
        method: "native", input_device: audio.input_device, output_device: audio.output_device,
        input_channel: inputChannel, output_channels: outputChannels,
        sweep_length: 65536, repetitions: 1, sweep_level_dbfs: audio.played_dbfs,
        timing: "none", share_mode: "shared",
      });
      const config = await audioCall("measure_config_get");
      audio.configurations.push({ stage, config });
      if (config.method !== "native" || config.input_device !== audio.input_device ||
          config.output_device !== audio.output_device || config.input_channel !== inputChannel ||
          JSON.stringify(config.roles) !== '["L","R"]' ||
          JSON.stringify(config.output_channels) !== JSON.stringify(outputChannels) ||
          config.sample_rate !== 48000 || config.sweep_length !== 65536 ||
          config.repetitions !== 1 || config.sweep_level_dbfs !== audio.played_dbfs ||
          config.share_mode !== "shared" || config.timing !== "none") {
        throw new Error(`measure_config_get: ${stage} did not retain the requested configuration`);
      }
    }
    for (const [role, channel] of [["L", 1], ["R", 2]]) {
      await configure(`level_${role}`, channel, [1, 2]);
      await attempts(`level_${role}`, async () => {
        const monitorEvents = [];
        const monitorStarted = Date.now();
        const events = eventChannel((message) => monitorEvents.push({ message, at: Date.now() }));
        const { job: id } = await audioCall("level_monitor_start", { role }, events);
        // Let the monitor enter its loop before submitting a burst.
        const at = Date.now();
        for (;;) {
          const state = (await audioCall("jobs")).find((entry) => entry.job === id);
          if (state?.status === "running") break;
          if (state && ["failed", "cancelled", "succeeded"].includes(state.status)) {
            throw jobError(`level_monitor_start: ${JSON.stringify(state)}`, state.error);
          }
          if (Date.now() - at > 30000) throw new Error("level_monitor_start: not running in 30 s");
          await sleep(100);
        }
        if (role === "L") {
          const deadline = Date.now() + 5000;
          for (;;) {
            const meters = monitorEvents.filter(({ message }) => message?.event === "meter" && message.job === id);
            if (meters.length >= 5) {
              audio.idle_meter = { events: meters.length, first_ms: meters[0].at - monitorStarted };
              break;
            }
            if (Date.now() >= deadline) {
              throw new Error(`level_monitor_start: received ${meters.length} idle meter events in 5 s`);
            }
            await sleep(25);
          }
        }
        await audioCall("level_play", { role, master_dbfs: audio.played_dbfs });
        const played = Date.now();
        for (;;) {
          const levels = await audioCall("levels_get");
          if (Object.hasOwn(levels.levels, role)) {
            audio.levels[role] = levels.levels[role];
            // level_signal normalizes signal RMS to master_dbfs; the monitor
            // stores 10*log10(mean(samples^2)), rounded to 0.01 dB. A unity
            // loopback must therefore be within 6 dB of -20 dBFS, not peak dBFS.
            if (!Number.isFinite(audio.levels[role]) || Math.abs(audio.levels[role] - audio.played_dbfs) > 6) {
              throw new Error(`levels_get: ${role} is ${audio.levels[role]} dBFS, expected ${audio.played_dbfs} +/- 6 dB`);
            }
            break;
          }
          const state = (await audioCall("jobs")).find((entry) => entry.job === id);
          if (state && ["failed", "cancelled", "succeeded"].includes(state.status)) {
            // A failed monitor has already ended; the next attempt starts a new one.
            throw jobError(`level_play: ${role} monitor ended without a stored level: ${JSON.stringify(state)}`, state.error);
          }
          if (Date.now() - played > 30000) throw new Error(`level_play: ${role} did not finish in 30 s`);
          await sleep(200);
        }
        await stopMonitor(id);
      });
    }
    // BlackHole never sums channels. Sweep L and R sequentially through the
    // same wire so input 1 captures both roles in one position measurement.
    await configure("sweep", 1, [1, 1]);
    audio.request = { method: "measure_position", params: { position: 1 } };
    await attempts("sweep", () => job("measure_position", { position: 1 }));
    audio.records = await audioCall("measurement_records");
    const record = audio.records.records.find((entry) => entry.position === 1);
    if (!record?.done || !["L", "R"].every((role) => record.roles.includes(role) && record.sources[role] === "measured")) {
      throw new Error("measurement_records: native position 1 must contain measured L and R");
    }
    // Curves are normalized and cannot prove absolute IR amplitude or time.
    // Save the actual native IRs; macos_app_check.py checks their impulse and
    // adds the numeric evidence to this report after the app has exited.
    await audioCall("project_save", { path: null });
    audio.ok = true;
    delete audio.request;
  }

  let ok = false;
  let error = null;
  try {
    await screenRendered();
    const info = await call("bootstrap");
    facts.version = info.version;
    facts.platform = info.platform;
    const methods = await call("measure_methods");
    facts.methods = methods.methods;
    facts.native_devices = methods.native_devices.length;
    facts.native_error = methods.native_error ? methods.native_error.message : null;
    await call("project_new", {
      path: `${work}/sample.secs`, preset: "7.1.4", sub_count: 3, sample: true, tutorial: false,
    });
    await call("measure_configure", { method: "mock" });
    await job("measure_position", { position: 1 });
    await call("project_save", { path: null });
    await job("calibrate", { settings: null, subs_only: false });
    const results = await call("results");
    const calibration = results.calibration;
    if (!calibration) throw new Error("results: no calibration after calibrate");
    if (calibration.stale) throw new Error("results: the new calibration is stale");
    facts.sub_roles = calibration.sub_roles;
    facts.verdict = calibration.verdict;
    await job("export", { kind: "camilla_dsp", folder: `${work}/export/camilla-dsp` });
    await job("export", { kind: "equalizer_apo", folder: `${work}/export/equalizer-apo` });
    if (audioDevice !== null) await nativeAudio();
    ok = true;
  } catch (caught) {
    error = String(caught && caught.message ? caught.message : caught);
    if (facts.audio) facts.audio.error = error;
  }
  await internals.invoke("secs_self_test_report", {
    report: { ok, error, work, ms: Date.now() - started, facts, steps },
  });
})();
