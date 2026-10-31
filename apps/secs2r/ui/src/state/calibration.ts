import { recommendXoRequest } from "../ipc/recommendXo";
import { mimoIncluded, xoRecInclude } from "../screens/calibrate/settingsView";
import { subRoles } from "../screens/calibrate/text";
import type { Session } from "./session";

export function calibrationMimo(stores: Session["stores"]): boolean {
  const settings = stores.settings.getSnapshot();
  return !!settings && mimoIncluded(settings, stores.project.getSnapshot()?.positions.length ?? 0);
}

/** Manual and automatic full correction use the same routing and MIMO choice. */
export function startCalibration({ stores, call }: Pick<Session, "stores" | "call">, background = false) {
  const settings = stores.settings.getSnapshot();
  const config = stores.config.getSnapshot();
  if (settings && config && subRoles(config).length > 0 && xoRecInclude(settings)) return call(recommendXoRequest(background), { calibrateOnFailure: true });
  return call({ method: "calibrate", params: { settings: null, subs_only: false, mimo: calibrationMimo(stores), ...(background ? { background: true } : {}) } });
}
