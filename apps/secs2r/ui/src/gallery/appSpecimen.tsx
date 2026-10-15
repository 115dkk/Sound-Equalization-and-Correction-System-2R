import { useEffect, useMemo } from "react";
import { App } from "../App";
import { usePrefs } from "../app/prefs";
import { createMockService } from "../ipc/mock";
import type { SpecimenDef } from "./Specimen";

function LiveApp() {
  const prefs = usePrefs();
  const service = useMemo(() => createMockService({ project: null, tickMs: 40 }), []);
  useEffect(() => () => service.dispose(), [service]);
  return <App key={`${prefs.locale}.${prefs.theme}`} ipc={service} root={false} initialPrefs={prefs} />;
}

export const appSpecimens: SpecimenDef[] = [{ id: "app.live", title: "Live application · mock service", width: 1280, height: 800, render: () => <LiveApp /> }];
