// Port of src/secs/wizard/tutorial_quests.py (SECS 2, GPL-3.0).
// Source lines are kept beside each entry so rail reordering is auditable.
import type { MessageKey } from "../../i18n";
import type { ResultTab } from "../../ipc/bindings/ResultTab";
import type { Quest } from "./types";
import { infraState } from "../calibrate/settingsView";
import type { TutorialHost, TutorialView } from "./questHost";
export type { TutorialHost, TutorialView } from "./questHost";

type Entry = Quest<TutorialView> & { upstreamLine: number };
const key = (id: string, field: string) => `tutorial.q.${id}.${field}` as MessageKey;
const subs = (v: TutorialView) => v.system.config.speakers.filter((s) => s.role.startsWith("SW")).map((s) => s.role);
const twoSubs = (v: TutorialView) => subs(v).length >= 2;
const computing = (v: TutorialView) => v.calibrate.running !== null || v.calibrate.measuring;

export function tutorialQuests(host: TutorialHost): Entry[] {
  const look = (id: string, line: number, targets: Quest<TutorialView>["targets"], extra: Partial<Entry> = {}): Entry => ({
    id, step: id.split(".")[0] as Entry["step"], kind: "look", upstreamLine: line,
    targets, title: key(id, "title"), text: key(id, "text"), ...extra,
  });
  const act = (id: string, line: number, targets: Entry["targets"], done: NonNullable<Entry["done"]>, run: (v: TutorialView) => void | false, blocked: (v: TutorialView) => boolean, extra: Partial<Entry> = {}): Entry => ({
    ...look(id, line, targets), kind: "act", done,
    hint: key(id, "hint"), blocked: (v) => blocked(v) ? key(id, "blocked") : null,
    auto: { run: (v) => blocked(v) ? false : run(v), note: key(id, "note") }, ...extra,
  });
  // The engine has an entry/leave lifecycle through animate. Opening a tab
  // here uses the same host setter as the screen, never a simulated click.
  const onTab = (tab: ResultTab): Pick<Entry, "animate"> => ({ animate: { every: 3600, step: () => host.calibrate.selectTab(tab) } });
  const ba = { when: twoSubs, ...onTab("ba") };
  const unavailable = (v: TutorialView) => computing(v) || v.calibrate.positions.length === 0;
  return [
    look("system.presets", 304, ["system.preset", "system.subs"], { terms: ["preset"] }),
    look("system.patch", 306, ["system.patch"], { terms: ["polarity"] }),
    look("system.dsp", 307, ["system.dsp"], { terms: ["dsp", "fir", "iir"] }),
    act("system.output", 308, ["system.patch.SW3.out"], (v) => { const out = v.system.config.speakers.find((s) => s.role === "SW3")?.bands[0]?.out; return out !== undefined && out !== 16; },
      (v) => {
        const index = v.system.config.speakers.findIndex((s) => s.role === "SW3");
        const used = v.system.config.speakers.filter((_, i) => i !== index).flatMap((s) => s.bands.map((b) => b.out));
        const out = used.includes(6) ? Array.from({ length: 64 }, (_, i) => i + 1).find((n) => !used.includes(n)) : 6;
        if (out === undefined) return false;
        host.system.setOutput(index, 0, out);
      }, (v) => !v.system.config.speakers.some((s) => s.role === "SW3" && s.bands.length > 0),
      { when: (v) => v.system.config.speakers.some((s) => s.role === "SW3" && s.bands[0]?.out === 16), why: key("system.output", "why"), whyTerms: ["sweep"], keep: ["system.patch.SW3.out"] }),
    look("system.layout_next", 314, ["system.layout"]),
    look("system.layout", 326, ["system.arrangement.map"]),
    // The timing reference shows only on the REW path (U15), so the quest
    // points at the start delay every path has.
    look("system.timing", 327, ["system.start_delay"]),
    look("system.single", 329, ["system.arrangement.1"], { why: key("system.single", "why"), whyTerms: ["mimo"] }),
    act("system.next", 332, ["system.next"], (v) => v.step !== "system", () => host.system.next(), (v) => v.system.config.speakers.length === 0 || v.system.notices.length > 0, { gate: true }),
    look("connect.sidebar", 288, ["frame.status", "frame.help", "frame.project"], { apart: true }),
    look("connect.rail", 289, ["frame.rail"]),
    look("connect.connection", 293, ["connect.method"], { terms: ["rew", "api", "mdat"] }),
    look("connect.audio", 296, ["connect.method"], { terms: ["calibration", "sweep"] }),
    act("connect.retry", 297, ["connect.method.mock"], (v) => v.connect.path === "mock" && v.connect.settings?.method === "mock", () => host.connect.choosePath("mock"), (v) => !v.connect.methods?.methods.includes("mock"),
      { why: key("connect.retry", "why"), ok: key("connect.retry", "ok") }),
    look("connect.level_next", 302, ["connect.level"]),
    look("connect.master", 316, ["connect.level.master", "connect.level.play_all"]),
    act("connect.levels", 317, ["connect.level.play_all", "connect.level.skip"], (v) => v.system.config.speakers.length > 0 && v.system.config.speakers.every((s) => s.role in v.connect.levels), () => host.connect.skipLevels(), (v) => v.connect.path !== "mock" || !v.connect.settings,
      { why: key("connect.levels", "why"), whyTerms: ["dbfs"], busy: { when: (v) => v.connect.level.playingAll || v.connect.level.playing !== null, text: key("connect.levels", "busy") }, keep: ["connect.level", "connect.mic"] }),
    act("connect.next", 324, ["frame.rail.measure"], (v) => v.step !== "connect", () => host.connect.next(), (v) => !v.connect.settings || v.connect.path === null || v.connect.level.playingAll, { gate: true }),
    look("measure.positions", 334, ["measure.positions"]),
    act("measure.sample", 335, ["measure.sample.load"], (v) => v.measure.project.positions.length > 0, () => { void host.measure.loadSample(); }, (v) => !v.measure.project.tutorial || !!v.measure.job && v.measure.job.kind !== "level_monitor",
      { why: key("measure.sample", "why"), busy: { when: (v) => v.measure.job?.kind === "sample_load", text: key("measure.sample", "busy") }, keep: ["measure.status", "jobs.tray"] }),
    look("measure.graph", 339, ["measure.plot"], { animate: { every: 3600, step: () => host.measure.setGraphHidden(false) } }),
    look("measure.mdat", 341, ["measure.mdat.load"], { terms: ["mdat"] }),
    act("measure.next", 344, ["frame.rail.calibrate"], (v) => v.step !== "measure", () => host.measure.openCalibrate(), (v) => v.measure.project.positions.length === 0 || !!v.measure.job && v.measure.job.kind !== "level_monitor", { gate: true }),
    look("calibrate.mode", 346, ["calibrate.settings.mode", "calibrate.compute"], { apart: true }),
    look("calibrate.graph", 349, ["calibrate.graph"], { terms: ["spl", "gd", "xo", "target"] }),
    look("calibrate.tabs", 352, [["calibrate.tab.spl", "calibrate.tab.gd", "calibrate.tab.xo"], "calibrate.settings"], { terms: ["rc"], apart: true,
      animate: { every: 3600, step: (k) => host.calibrate.selectTab((["spl", "gd", "xo"] as const)[k % 3]!), end: () => host.calibrate.selectTab("spl") } }),
    act("calibrate.compute", 356, ["calibrate.compute"], (v) => !!v.calibrate.results.calibration, () => host.calibrate.calibrate(), unavailable,
      { why: key("calibrate.compute", "why"), busy: { when: computing, text: key("calibrate.compute", "busy") }, keep: ["calibrate.running", "jobs.tray"] }),
    look("calibrate.spl", 365, ["calibrate.graph"], onTab("spl")),
    look("calibrate.metrics", 370, ["calibrate.verdict", "calibrate.plot.filters"], { ...onTab("spl"), terms: ["peq", "fir"], apart: true }),
    look("calibrate.gd", 375, ["calibrate.graph"], onTab("gd")),
    act("calibrate.xo_open", 379, ["calibrate.tab.xo"], (v) => v.calibrate.tab === "xo", () => host.calibrate.selectTab("xo"), (v) => subs(v).length === 0,
      { why: key("calibrate.xo_open", "why"), whyTerms: ["peq"] }),
    look("calibrate.xo", 384, ["calibrate.graph", "calibrate.settings.xo"], { ...onTab("xo"), apart: true }),
    act("calibrate.ba_open", 389, ["calibrate.tab.ba"], (v) => v.calibrate.tab === "ba", () => host.calibrate.selectTab("ba"), (v) => !twoSubs(v), { when: twoSubs, why: key("calibrate.ba_open", "why") }),
    look("calibrate.ba_graph", 395, ["calibrate.graph"], ba),
    look("calibrate.ba_metrics", 400, ["calibrate.ba.metrics", "calibrate.ba.table"], { ...ba, terms: ["gain", "delay", "polarity", "allpass", "dsp"], why: key("calibrate.ba_metrics", "why") }),
    look("calibrate.ba_settings", 408, ["calibrate.ba.subs", "calibrate.ba.edit", "calibrate.ba.editor"], { ...ba, why: key("calibrate.ba_settings", "why"), whyTerms: ["dba"] }),
    act("calibrate.infra", 415, ["calibrate.ba.infra"], (v) => !!infraState(v.calibrate.settings, subs(v)).role,
      (v) => host.calibrate.changeSetting({ kind: "infra", role: subs(v).at(-1)!, hz: 25, type: "LR24", subs: subs(v) }),
      (v) => !twoSubs(v) || computing(v), { ...ba, why: key("calibrate.infra", "why"), whyTerms: ["lp_hp"], keep: ["calibrate.ba.infra_group"] }),
    look("calibrate.ba_filters", 424, ["calibrate.ba.fixed"], { ...ba, terms: ["lp_hp", "peq"] }),
    act("calibrate.subs", 430, ["calibrate.ba.compute_subs"], (v, before) => !!v.calibrate.lines.ba?.subs_only && v.calibrate.lines.ba !== before,
      () => host.calibrate.calibrateSubs(), unavailable, { ...ba, base: (v) => v.calibrate.lines.ba, why: key("calibrate.subs", "why"), busy: { when: computing, text: key("calibrate.subs", "busy") }, ok: key("calibrate.subs", "ok"), keep: ["jobs.tray"] }),
    look("calibrate.ba_after", 442, ["calibrate.graph"], ba),
    act("calibrate.next", 449, ["frame.rail.export"], (v) => v.step !== "calibrate", () => host.goTo("export"), (v) => computing(v) || !v.calibrate.results.calibration, { gate: true }),
    look("export.mdat", 451, ["export.run.before_after_mdat"], { terms: ["mdat", "mimo"] }),
    look("export.camilla", 453, ["export.run.camilla_dsp"], { terms: ["dsp", "fir"] }),
  ];
}
