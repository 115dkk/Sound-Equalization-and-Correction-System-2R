import type { FrameModel, StepState } from "./model";
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
      path: { kind: host.path === "mdat" ? "none" : host.path ?? "none", state: views.connect && pathReady(views.connect) ? "ok" : "warning" },
      fileName: project?.path ? project.name : null, dirty: project?.dirty ?? false,
    },
  };
  return { model, navigate: session.navigate };
}
