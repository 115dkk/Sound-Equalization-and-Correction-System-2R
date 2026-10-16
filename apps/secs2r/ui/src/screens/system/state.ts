// What the rail shows for 시스템, decided like upstream step 2's
// can_advance(): at least one speaker, and config_validate has nothing to say.
import type { StepState } from "../../app/model";
import type { SystemView } from "./types";

export function systemStepState(view: Pick<SystemView, "config" | "notices">): StepState {
  if (view.config.speakers.length === 0 || view.notices.length > 0) return { status: "ready" };
  return { status: "done" };
}
