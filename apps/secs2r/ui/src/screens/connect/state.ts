// What the rail shows for 연결·점검, decided like upstream steps 1 and 3:
// a path that is ready (REW answering with Pro, native devices, or the
// sample room) and a level for every channel.
//
// Blocked means an earlier step must act first (no speakers on 시스템).
// Choosing a method, getting it ready and the missing levels are this
// step's own work, so the step stays `ready` and carries the reason
// (`no_path`, `path_not_ready`, `no_levels` with the count) for the steps
// after it to show.
import type { StepState } from "../../app/model";
import { orderRoles } from "../../ui/maps/geometry";
import { levelState } from "../../ui/meters";
import type { ConnectView } from "./types";

/** Whether the chosen path can measure now. */
export function pathReady(view: Pick<ConnectView, "path" | "methods">): boolean {
  switch (view.path) {
    case "mock":
    case "mdat":
      return true;
    case "native":
    case "rew":
      return view.methods?.methods.includes(view.path) ?? false;
    default:
      return false;
  }
}

/** The channels the level check plays, in row order (role order, as upstream ordered_speakers). */
export function levelRoles(view: Pick<ConnectView, "config">): string[] {
  return orderRoles(view.config.speakers.map((speaker) => speaker.role));
}

export function missingLevels(view: Pick<ConnectView, "config" | "levels">): string[] {
  return levelRoles(view).filter((role) => view.levels[role] === undefined);
}

/** Every channel has a level in the recommended range: the check ended without a problem. */
export function levelsPassed(view: Pick<ConnectView, "config" | "levels">): boolean {
  const roles = levelRoles(view);
  return roles.length > 0 && roles.every((role) => levelState(view.levels[role] ?? null) === "ok");
}

export function connectStepState(view: Pick<ConnectView, "path" | "methods" | "config" | "levels">): StepState {
  if (view.config.speakers.length === 0) return { status: "blocked", reason: "no_speakers" };
  if (view.path === "mdat") return { status: "skipped" };
  if (view.path === null) return { status: "ready", reason: "no_path" };
  if (!pathReady(view)) return { status: "ready", reason: "path_not_ready" };
  const missing = missingLevels(view).length;
  if (missing > 0) return { status: "ready", reason: "no_levels", count: missing };
  return { status: "done" };
}
