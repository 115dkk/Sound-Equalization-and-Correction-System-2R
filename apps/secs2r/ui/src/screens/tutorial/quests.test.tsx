// @vitest-environment happy-dom
import { act, cleanup, fireEvent, render, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { SessionProvider, useHost } from "../../state/react";
import { createSession } from "../../state/session";
import { useSystemScreen } from "../system/useSystemScreen";
import { useConnectScreen } from "../connect/useConnectScreen";
import { useMeasureScreen } from "../measure/useMeasureScreen";
import { useCalibrateScreen } from "../calibrate/useCalibrateScreen";
import { useExportScreen } from "../export/useExportScreen";
import type { TutorialView } from "./questHost";
import { afterEach, beforeAll, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { App } from "../../App";
import { createMockService } from "../../ipc/mock";
import { installFakeCanvas } from "../../test/canvas";
import ko from "../../i18n/ko.json";
import en from "../../i18n/en.json";
import { tutorialQuests, type TutorialHost } from "./quests";

beforeAll(() => installFakeCanvas());
afterEach(cleanup);
const noop = () => {};
const host: TutorialHost = {
  system: { setOutput: noop, renumberOutputs: noop, next: noop },
  connect: { choosePath: noop, skipLevels: noop, next: noop },
  measure: { loadSample: noop, openCalibrate: noop, setGraphHidden: noop },
  calibrate: { calibrate: noop, calibrateSubs: noop, selectTab: noop, changeSetting: async () => true }, goTo: noop,
};
function useQuestHost() {
  const state = useHost();
  const system = useSystemScreen();
  const connect = useConnectScreen(false);
  const measure = useMeasureScreen();
  const calibrate = useCalibrateScreen();
  const exporting = useExportScreen();
  return { state, system, connect, measure, calibrate, exporting };
}

const quests = tutorialQuests(host);
const ui = (id: string) => document.querySelector<HTMLElement>(`[data-ui="${id}"]`);

it("ports all 43 upstream entries once, in rail order and upstream order within a step", () => {
  const source = readFileSync("../../../src/secs/wizard/tutorial_quests.py", "utf8");
  const upstream = source.split("\n").flatMap((line, i) => /^    (Quest\(|_next_to\()/.test(line) ? [i + 1] : []);
  expect(quests).toHaveLength(43);
  expect(quests.map((q) => q.upstreamLine).sort((a, b) => a - b)).toEqual(upstream);
  expect(new Set(quests.map((q) => q.id)).size).toBe(43);
  expect(quests.map((q) => q.upstreamLine)).toEqual([
    304, 306, 307, 308, 314, 326, 327, 329, 332,
    288, 289, 293, 296, 297, 302, 316, 317, 324,
    334, 335, 339, 341, 344,
    346, 349, 352, 356, 365, 370, 375, 379, 384, 389, 395, 400, 408, 415, 424, 430, 442, 449,
    451, 453,
  ]);
  // No counterpart is dropped. The two same-step next entries become looks;
  // export has no act upstream. The packet's count of 38 predates this tree.
});

it("resolves every named quest message in both catalogs with matching placeholders", () => {
  for (const q of quests) {
    const keys = [q.title, q.text, q.why, q.hint, q.ok, q.auto?.note, q.busy?.text, `tutorial.q.${q.id}.blocked`].filter((key): key is keyof typeof ko => !!key && (key !== `tutorial.q.${q.id}.blocked` || q.kind === "act"));
    for (const key of keys) {
      expect(ko[key], key).toBeTruthy(); expect(en[key], key).toBeTruthy();
      expect(ko[key].match(/\{[^}]+\}/g) ?? []).toEqual(en[key].match(/\{[^}]+\}/g) ?? []);
      expect(ko[key]).not.toContain("!"); expect(en[key]).not.toContain("!");
    }
  }
});

for (const route of ["action", "auto"] as const) it(`finishes every act through ${route} on the screen hooks and explains blocked states`, async () => {
  const mock = createMockService({ project: null, tickMs: 0 });
  const session = createSession(mock);
  await session.start();
  const wrapper = ({ children }: { children: ReactNode }) => <SessionProvider value={session}>{children}</SessionProvider>;
  const hook = renderHook(useQuestHost, { wrapper });
  const view = (): TutorialView => {
    const h = hook.result.current;
    return { step: h.state.current, system: h.system.view!, connect: h.connect.view!, measure: h.measure.view!, calibrate: h.calibrate.view!, export: h.exporting.view! };
  };
  try {
    for (const entry of quests.filter((q) => q.kind === "act")) {
      await act(async () => {
        await session.call({ method: "project_new", params: { path: "", preset: "7.1.4", sub_count: 3, sample: true, tutorial: true } });
        session.navigate(entry.step);
        if (entry.id === "system.output") await session.call({ method: "config_set_speaker", params: { index: 5, role: null, size: null, band_index: 0, out: 16, pol: null } });
        if (entry.id === "connect.retry") await session.configure({ method: "none" });
        if (entry.id === "measure.next" || entry.step === "calibrate") {
          await session.call({ method: "sample_load", params: { positions: null } }); mock.drain();
        }
      });
      if (entry.id === "measure.next" || entry.step === "calibrate") await waitFor(() => expect(view().calibrate.positions.length).toBeGreaterThan(0));
      if (["calibrate.subs", "calibrate.next"].includes(entry.id)) {
        await act(async () => { await session.call({ method: "calibrate", params: { settings: null, subs_only: false } }); mock.drain(); });
        await waitFor(() => expect(view().calibrate.results.calibration).not.toBeNull());
      }
      await act(async () => { session.host.set({ tab: "spl" }); });
      const h = hook.result.current;
      const actions: TutorialHost = { system: h.system.actions, connect: h.connect.actions, measure: h.measure.actions, calibrate: h.calibrate.actions, goTo: session.navigate };
      const q = tutorialQuests(actions).find((q) => q.id === entry.id)!;
      const before = view();
      const base = q.base?.(before);
      expect(q.done!(before, base), q.id).toBe(false);
      const blocked: TutorialView = { ...before,
        system: { ...before.system, config: { ...before.system.config, speakers: [] } },
        connect: { ...before.connect, path: null, settings: null, methods: null, level: { ...before.connect.level, playingAll: true } },
        measure: { ...before.measure, project: { ...before.measure.project, positions: [], tutorial: false } },
        calibrate: { ...before.calibrate, running: "calibrate", positions: [], measuring: true },
      };
      expect(q.blocked!(blocked), q.id).toBeTruthy();
      expect(q.auto!.run(blocked), q.id).toBe(false);
      await act(async () => {
        if (route === "auto") q.auto!.run(before);
        else switch (q.id) {
          case "system.output": actions.system.setOutput(5, 0, 6); break;
          case "system.next": actions.system.next(); break;
          case "connect.retry": actions.connect.choosePath("mock"); break;
          case "connect.levels": actions.connect.skipLevels(); break;
          case "connect.next": actions.connect.next(); break;
          case "measure.sample": actions.measure.loadSample(); break;
          case "measure.next": actions.measure.openCalibrate(); break;
          case "calibrate.compute": actions.calibrate.calibrate(); break;
          case "calibrate.xo_open": actions.calibrate.selectTab("xo"); break;
          case "calibrate.ba_open": actions.calibrate.selectTab("ba"); break;
          case "calibrate.infra": actions.calibrate.changeSetting({ kind: "infra", role: "SW3", hz: 25, type: "LR24", subs: ["SW1", "SW2", "SW3"] }); break;
          case "calibrate.subs": actions.calibrate.calibrateSubs(); break;
          case "calibrate.next": actions.goTo("export"); break;
        }
      });
      await act(async () => { mock.drain(); });
      await waitFor(() => expect(q.done!(view(), base), q.id).toBe(true));
    }
    expect(session.errors.getSnapshot()).toBeNull();
  } finally { hook.unmount(); session.dispose(); mock.dispose(); }
});

it("resumes at a step that can no longer be left: without measurements, 보정 goes back to the demo data quest", async () => {
  // Levels filled, so 연결·점검 can be left; no measurement, so 측정·가져오기 cannot (보정 is blocked with no_measurements).
  const mock = createMockService({ project: null, tickMs: 0 });
  await mock.request({ method: "project_new", params: { path: "", preset: "7.1.4", sub_count: 3, sample: true, tutorial: true } });
  await mock.request({ method: "levels_fill_defaults" });
  const rendered = render(<App ipc={mock} tutorialStart="calibrate.graph" />);
  try {
    // The app starts and prepares the tutorial first (slow under a full parallel run).
    await waitFor(() => expect(ui("tutorial.coach")?.textContent).toContain(ko["tutorial.q.calibrate.graph.text"]), { timeout: 8000 });
    fireEvent.click(ui("tutorial.coach.pause")!);
    await waitFor(() => expect(ui("tutorial.resume")).not.toBeNull());
    fireEvent.click(ui("tutorial.resume")!);
    await waitFor(() => expect(ui("tutorial.coach")?.textContent).toContain(ko["tutorial.q.measure.sample.text"]));
    expect(ui("tutorial.toast")?.textContent).toBe(ko["tutorial.toast.rewound"].replace("{step}", ko["frame.step.measure"]));
    expect(ui("frame.rail.measure")?.getAttribute("aria-current")).toBe("step");
  } finally { rendered.unmount(); mock.dispose(); }
}, 15000);

it("resumes past finished quests: without levels, 보정 goes back to the 연결·점검 levels quest, not the finished mock-path one", async () => {
  // The tutorial project is on the mock path (connect.retry done) but has no levels, so 연결·점검 cannot be left.
  const mock = createMockService({ project: null, tickMs: 0 });
  await mock.request({ method: "project_new", params: { path: "", preset: "7.1.4", sub_count: 3, sample: true, tutorial: true } });
  const rendered = render(<App ipc={mock} tutorialStart="calibrate.graph" />);
  try {
    await waitFor(() => expect(ui("tutorial.coach")?.textContent).toContain(ko["tutorial.q.calibrate.graph.text"]), { timeout: 8000 });
    fireEvent.click(ui("tutorial.coach.pause")!);
    await waitFor(() => expect(ui("tutorial.resume")).not.toBeNull());
    fireEvent.click(ui("tutorial.resume")!);
    await waitFor(() => expect(ui("tutorial.coach")?.textContent).toContain(ko["tutorial.q.connect.levels.text"]));
    expect(ui("tutorial.toast")?.textContent).toBe(ko["tutorial.toast.rewound"].replace("{step}", ko["frame.step.connect"]));
  } finally { rendered.unmount(); mock.dispose(); }
}, 15000);

it("walks every quest on the real App hooks, checks every target, and reaches the end via Continue", async () => {
  const mock = createMockService({ project: null, tickMs: 0 });
  await mock.request({ method: "project_new", params: { path: "", preset: "7.1.4", sub_count: 3, sample: true, tutorial: true } });
  const rendered = render(<App ipc={mock} />);
  try {
    for (const q of quests) {
      await waitFor(() => expect(ui("tutorial.coach")?.textContent, q.id).toContain(ko[q.text]));
      await waitFor(() => {
        for (const target of q.targets.flatMap((target) => typeof target === "string" ? [target] : [...target])) expect(ui(target), `${q.id}: ${target}`).not.toBeNull();
      });
      fireEvent.click(ui("tutorial.coach.next")!);
      if (q.kind === "act") fireEvent.click(ui("tutorial.coach.next")!);
      if (["measure.sample", "calibrate.compute", "calibrate.subs"].includes(q.id)) {
        await waitFor(() => expect(mock.calls.some((r) => q.id === "measure.sample" ? r.method === "sample_load" : r.method === "calibrate" && r.params.subs_only === (q.id === "calibrate.subs"))).toBe(true));
      }
      await act(async () => { mock.drain(); });
    }
    await waitFor(() => expect(ui("tutorial.end")).not.toBeNull());
    expect(ui("app.error")).toBeNull();
    expect(mock.calls.some((r) => r.method === "sample_load")).toBe(true);
    expect(mock.calls.filter((r) => r.method === "calibrate")).toHaveLength(2);
    expect(mock.calls.some((r) => r.method === "export")).toBe(false);
    fireEvent.click(ui("tutorial.end.restart")!);
    await waitFor(() => expect(ui("tutorial.coach")?.textContent).toContain(ko[quests[0]!.text]));
    const config = await mock.request({ method: "config_get" });
    expect(config.speakers.find((s) => s.role === "SW3")?.bands[0]?.out).toBe(16);
    expect(mock.calls.some((r) => r.method === "rew_launch")).toBe(false);
  } finally { rendered.unmount(); mock.dispose(); }
}, 30000);
