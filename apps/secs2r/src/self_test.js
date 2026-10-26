// The self-test driver (self_test.rs). It runs in the app's own page once the
// page has loaded and sends what a user does through `secs_call`, the command
// the screen uses: a mock-room measurement of the sample project, a
// calibration and two exports. Every answer goes into the report.
(async () => {
  const work = __SECS2R_SELF_TEST_WORK__;
  const internals = window.__TAURI_INTERNALS__;
  const started = Date.now();
  const steps = [];
  const facts = {};
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

  async function send(method, params) {
    const request = params === undefined ? { method } : { method, params };
    const envelope = await internals.invoke("secs_call", { request });
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
          throw new Error(`${method} job ${snapshot.status}: ${error}`);
        }
        return;
      }
      if (Date.now() - at > 240000) throw new Error(`${method} job did not finish in 240 s`);
      await sleep(200);
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
    ok = true;
  } catch (caught) {
    error = String(caught && caught.message ? caught.message : caught);
  }
  await internals.invoke("secs_self_test_report", {
    report: { ok, error, work, ms: Date.now() - started, facts, steps },
  });
})();
