import { STEPS, type FrameModel, type StepState } from "./model";
import { useHost, useResource, useSession } from "../state/react";
import type { SystemView } from "../screens/system/types";
import type { ConnectView } from "../screens/connect/types";
import type { MeasureView } from "../screens/measure/types";
import type { CalibrateView } from "../screens/calibrate/types";
import type { ExportView } from "../screens/export/types";
import { systemStepState } from "../screens/system/state";
import { connectStepState, pathReady } from "../screens/connect/state";
import { measureStepState } from "../screens/measure/types";
import { calibrateStepState } from "../screens/calibrate/types";
import { exportStepState } from "../screens/export/types";

export function useFrameModel(views: { system: SystemView | null; connect: ConnectView | null; measure: MeasureView | null; calibrate: CalibrateView | null; export: ExportView | null }) {
  const session = useSession();
  const host = useHost();
  const project = useResource("project");
  const config = useResource("config");
  const layout = useResource("layout");
  const records = useResource("records");
  const result = useResource("results")?.calibration;
  const ready: StepState = { status: "ready" };
  const computing = host.jobs.some((job) => job.kind === "calibrate" && ["running", "cancel_requested"].includes(job.status));
  const model: FrameModel = {
    current: host.current,
    steps: {
      system: views.system ? systemStepState(views.system) : ready,
      connect: views.connect ? connectStepState(views.connect) : ready,
      measure: views.measure ? measureStepState(views.measure) : { status: "blocked", reason: "no_path" },
      calibrate: views.calibrate ? calibrateStepState(views.calibrate) : { status: "blocked", reason: "no_measurements" },
      export: views.export ? exportStepState(views.export) : { status: "blocked", reason: "no_result" },
    },
    project: {
      preset: config?.preset ?? project?.preset ?? null, subCount: config?.summary.sub_count ?? 0,
      outputs: config?.summary.outputs.length ?? 0, positionsDone: records?.records.filter((r) => r.done).length ?? 0,
      positionsTotal: layout?.count ?? 0,
      calibration: computing ? "computing" : host.failure ? "failed" : result ? result.stale ? "stale" : "done" : "none",
      path: { kind: host.path ?? "none", state: pathReady({ path: host.path, methods: views.connect?.methods ?? null }) ? "ok" : "warning" },
      fileName: project?.path ? project.name : null, dirty: project?.dirty ?? false,
      warnings: project?.migration_warnings ?? [], opened: host.opened,
    },
  };
  if (!project) {
    for (const step of STEPS) model.steps[step] = { status: "blocked", reason: "no_project" };
  } else if (views.system) {
    // The system remains editable; its prerequisites gate every later step.
    const { config, notices } = views.system;
    const blocked: StepState | null = config.speakers.length === 0
      ? { status: "blocked", reason: "no_speakers" }
      : notices.length > 0 ? { status: "blocked", reason: "config_invalid", count: notices.length } : null;
    if (blocked) for (const step of STEPS) if (step !== "system" && model.steps[step].status !== "skipped") model.steps[step] = blocked;
  }
  return { model, navigate: session.navigate };
}
