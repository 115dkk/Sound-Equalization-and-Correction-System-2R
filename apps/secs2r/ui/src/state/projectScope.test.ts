import { expect, it } from "vitest";
import { createMockService } from "../ipc/mock";
import { createSession } from "./session";
import { PROJECT_HOST_DEFAULTS, PROJECT_RESOURCES, appResources, replaceProject } from "./projectScope";

/** Every project host field away from its default, and the window's fields set to something a replacement must keep. */
const projectValues = {
  path: "rew", selected: 2, mdat: { kind: "import", path: "/a.mdat" }, warnings: [{ position: 1, notices: [] }],
  channel: "L", xoGroup: "front", previewHz: 90,
  xoRecTurnedOffByEdit: true, xoRecommendation: { groups: [] }, runs: { camilla: { status: "running" } }, mdatScope: "current",
  designName: "living room", applyRuns: { eqapo: { status: "running" } },
  failure: { job: "calibrate", error: { code: "internal", params: {}, retryable: false, detail: "" } },
  rangeRejection: { edit: { kind: "rc_lo", hz: 15 }, error: { code: "invalid_request", params: {}, retryable: false, detail: "" } },
  micCalBusy: "set", micCalError: { code: "internal", params: {}, retryable: false, detail: "" },
} as const;
const appValues = { current: "export", tab: "gd", camillaDevices: { capture: { type: "file" }, playback: { type: "stdout" } }, extraRates: [96000] } as const;

it.each(["request", "external"] as const)("a %s replacement empties every project store and host field and keeps the window's", async (source) => {
  const mock = createMockService({ tickMs: 0 });
  const session = createSession(mock);
  try {
    await session.start(); await session.settled();
    session.host.set({ ...projectValues, ...appValues } as never);
    const opened = session.host.getSnapshot().opened;
    const epoch = session.jobs.epoch;
    const kept = Object.fromEntries(appResources(session.stores).map((key) => [key, session.stores[key].getSnapshot()]));
    expect(session.stores.project.getSnapshot()).not.toBeNull();
    replaceProject(session, source);
    for (const key of PROJECT_RESOURCES) expect(session.stores[key].getSnapshot(), key).toBeNull();
    for (const key of appResources(session.stores)) expect(session.stores[key].getSnapshot(), key).toBe(kept[key]);
    const host = session.host.getSnapshot();
    for (const [key, value] of Object.entries(PROJECT_HOST_DEFAULTS)) expect(host[key as keyof typeof host], key).toEqual(value);
    expect(host.opened).toBe(opened + 1);
    expect(host.designName).toBe("");
    expect(session.jobs.epoch).toBe(epoch + 1);
    expect(host).toMatchObject(appValues);
  } finally { session.dispose(); await session.settled(); mock.dispose(); }
});

it("names every project host field in the defaults and leaves the window's stores out of the table", () => {
  expect(Object.keys(PROJECT_HOST_DEFAULTS)).not.toContain("opened");
  expect(PROJECT_RESOURCES).not.toContain("appSettings");
  expect(PROJECT_RESOURCES).not.toContain("bootstrap");
});
