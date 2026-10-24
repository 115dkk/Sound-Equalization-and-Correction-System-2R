// The tutorial engine on the frame's ids with the sample quests: a look
// quest, an act quest (armed, list folded, a toast), a busy act quest,
// paused, and the end.
import type { FrameModel, StepId } from "../../app/model";
import { initialJobs, type JobsState } from "../../app/jobs/jobStore";
import { FrameBox, T0, replay } from "../../gallery/frameSpecimens";
import type { SpecimenDef } from "../../gallery/Specimen";
import { EmptyState } from "../../ui";
import { useT } from "../../i18n/react";
import { TutorialAccent } from "../export/specimens";
import { sampleQuests, type SampleView } from "./sampleQuests";
import { TutorialLayer } from "./Tutorial";
import type { TutorialState } from "./types";

const QUESTS = sampleQuests({ openHelp: () => {}, compute: () => {} });

function tutorialFrame(current: StepId, computing = current === "calibrate"): FrameModel {
  return {
    current,
    project: {
      preset: "7.1.4",
      subCount: 3,
      outputs: 14,
      positionsDone: current === "calibrate" ? 1 : 0,
      positionsTotal: 1,
      calibration: computing ? "computing" : current === "calibrate" ? "done" : "none",
      path: { kind: "mock", state: "ok" },
      fileName: "튜토리얼.secs",
      dirty: false,
    },
    steps: {
      system: { status: "done" },
      connect: current === "calibrate" ? { status: "done" } : { status: "ready" },
      measure: current === "calibrate" ? { status: "done" } : { status: "blocked", reason: "no_path" },
      calibrate: current === "calibrate" ? (computing ? { status: "ready" } : { status: "done" }) : { status: "blocked", reason: "no_measurements" },
      export: computing ? { status: "blocked", reason: "computing" } : current === "calibrate" ? { status: "ready" } : { status: "blocked", reason: "no_result" },
    },
  };
}

function Placeholder() {
  const t = useT();
  return <EmptyState title={t("frame.placeholder.title")} description={t("frame.placeholder.body")} />;
}

const computeJob = replay([
  { type: "event", at: T0, event: { event: "started", job: 4, kind: "calibrate" } },
  { type: "event", at: T0 + 1500, event: { event: "progress", job: 4, stage: "engine.design", done: 3, total: 11 } },
]);

function Sample({ initial, view, step = "system", jobs = initialJobs }: { initial: Partial<TutorialState>; view?: Partial<SampleView>; step?: StepId; jobs?: JobsState }) {
  const sample: SampleView = { helpOpen: false, computing: false, computed: false, ...view };
  return (
    <TutorialAccent>
      <TutorialLayer quests={QUESTS} view={sample} step={step} actions={{ goTo: () => {}, finish: () => {}, restart: () => {} }} initial={initial} toastMs={0}>
        <FrameBox frame={tutorialFrame(step, sample.computing)} jobs={jobs} primaryBusy={sample.computing}>
          <Placeholder />
        </FrameBox>
      </TutorialLayer>
    </TutorialAccent>
  );
}

const size = { width: 1280, height: 800 };

export const tutorialSpecimens: SpecimenDef[] = [
  {
    id: "screen.tutorial.look",
    title: "Tutorial · look quest (job tray), 왜? open with a glossary term, list open",
    ...size,
    render: () => <Sample initial={{ index: 3, completed: [0, 1, 2], whyOpen: true }} />,
  },
  {
    id: "screen.tutorial.act",
    title: "Tutorial · act quest (open help) after the first 계속, list folded, click-outside toast",
    ...size,
    render: () => <Sample initial={{ index: 2, completed: [0, 1], armed: true, folded: true, toast: { key: "tutorial.toast.outside_act" } }} />,
  },
  {
    id: "screen.tutorial.busy",
    title: "Tutorial · act quest waiting on a running job (busy)",
    ...size,
    render: () => (
      <Sample initial={{ index: 4, completed: [0, 1, 2, 3], cleared: ["system"] }} view={{ computing: true }} step="calibrate" jobs={computeJob} />
    ),
  },
  {
    id: "screen.tutorial.paused",
    title: "Tutorial · paused (only the resume button stays)",
    ...size,
    render: () => <Sample initial={{ index: 1, completed: [0], paused: true }} />,
  },
  {
    id: "screen.tutorial.end",
    title: "Tutorial · the end (start over or end)",
    ...size,
    render: () => <Sample initial={{ index: 4, completed: [0, 1, 2, 3, 4], cleared: ["system", "calibrate"], finished: true }} step="calibrate" />,
  },
];
