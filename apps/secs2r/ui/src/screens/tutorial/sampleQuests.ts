// Sample quests on the frame's ids, to show the engine working in the
// gallery and the tests. The real quest list (upstream's forty-odd quests on
// the step screens' ids) is P35's.
import type { Quest } from "./types";

/** What the sample quests read: whether help is open and how the correction stands. */
export type SampleView = { helpOpen: boolean; computing: boolean; computed: boolean };

export function sampleQuests(host: { openHelp: () => void; compute: () => void }): Quest<SampleView>[] {
  return [
    {
      id: "sample.rail",
      step: "system",
      kind: "look",
      targets: ["frame.rail"],
      title: "tutorial.sample.rail.title",
      text: "tutorial.sample.rail",
      why: "tutorial.sample.rail.why",
    },
    {
      id: "sample.status",
      step: "system",
      kind: "look",
      targets: ["frame.status"],
      title: "tutorial.sample.status.title",
      text: "tutorial.sample.status",
    },
    {
      id: "sample.help",
      step: "system",
      kind: "act",
      targets: ["frame.help"],
      title: "tutorial.sample.help.title",
      text: "tutorial.sample.help",
      why: "tutorial.sample.help.why",
      done: (view) => view.helpOpen,
      auto: { run: () => host.openHelp(), note: "tutorial.sample.help.note" },
      ok: "tutorial.sample.help.ok",
    },
    {
      id: "sample.tray",
      step: "system",
      kind: "look",
      targets: ["jobs.tray"],
      title: "tutorial.sample.tray.title",
      text: "tutorial.sample.tray",
      why: "tutorial.sample.tray.why",
      terms: ["mimo"],
    },
    {
      id: "sample.compute",
      step: "calibrate",
      kind: "act",
      targets: ["calibrate.primary"],
      title: "tutorial.sample.compute.title",
      text: "tutorial.sample.compute",
      why: "tutorial.sample.compute.why",
      whyTerms: ["dsp"],
      done: (view) => view.computed,
      busy: { when: (view) => view.computing, text: "tutorial.sample.compute.busy" },
      auto: { run: () => host.compute(), note: "tutorial.sample.compute.note" },
      keep: ["jobs.tray"],
    },
  ];
}
