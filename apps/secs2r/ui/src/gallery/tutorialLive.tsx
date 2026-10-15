import { useEffect, useState } from "react";
import { App } from "../App";
import { usePrefs } from "../app/prefs";
import { createMockService, type MockService } from "../ipc/mock";
import type { SpecimenDef } from "./Specimen";

/** Same App and hooks as production; only the transport is synthetic. */
function TutorialLive({ quest }: { quest: string }) {
  const prefs = usePrefs();
  const [service, setService] = useState<MockService | null>(null);
  useEffect(() => {
    const mock = createMockService({ project: null, tickMs: 40 });
    let disposed = false;
    void (async () => {
      await mock.request({ method: "project_new", params: { path: "", preset: "7.1.4", sub_count: 3, sample: true, tutorial: true } });
      if (quest.startsWith("calibrate.") || quest.startsWith("export.")) {
        await mock.request({ method: "sample_load", params: { positions: null } }); mock.drain();
      }
      if (quest.startsWith("export.")) {
        await mock.request({ method: "calibrate", params: { settings: null, subs_only: false } }); mock.drain();
      }
      if (!disposed) setService(mock);
    })();
    return () => { disposed = true; mock.dispose(); };
  }, [quest]);
  return service ? <App key={`${prefs.locale}.${prefs.theme}`} ipc={service} root={false} initialPrefs={prefs} tutorialStart={quest} /> : null;
}

// Export has no act quest upstream; its first look is the fifth shot.
export const tutorialLiveSpecimens: SpecimenDef[] = [
  ["system", "system.output"], ["connect", "connect.retry"], ["measure", "measure.sample"],
  ["calibrate", "calibrate.compute"], ["export", "export.mdat"],
].map(([step, quest]) => ({ id: `tutorial.live.${step}`, title: `Live tutorial · ${step}`, width: 1280, height: 800, render: () => <TutorialLive quest={quest!} /> }));
