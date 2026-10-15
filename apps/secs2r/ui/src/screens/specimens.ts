// The step screens' gallery sections (docs/packets/M5-screens-common.md).
// Each screen packet adds its imports and entries; ids are
// `screen.<step>.<state>` so CI screenshots them one by one.
import type { SectionDef } from "../gallery/Specimen";
import { calibrateSpecimens } from "./calibrate/specimens";
import { connectSpecimens } from "./connect/specimens";
import { exportSpecimens } from "./export/specimens";
import { helpSpecimens } from "./help/specimens";
import { measureSection } from "./measure/specimens";
import { projectSpecimens } from "./project/specimens";
import { systemSpecimens } from "./system/specimens";
import { tutorialSpecimens } from "./tutorial/specimens";

export const screenSections: SectionDef[] = [
  { id: "screen.system", title: "Screen · System", specimens: systemSpecimens },
  { id: "screen.connect", title: "Screen · Connect & check", specimens: connectSpecimens },
  measureSection,
  { id: "screen.calibrate", title: "Screen · Correction", specimens: calibrateSpecimens },
  { id: "screen.export", title: "Screen · Export", specimens: exportSpecimens },
  { id: "screen.help", title: "Help panel", specimens: helpSpecimens },
  { id: "screen.tutorial", title: "Tutorial engine", specimens: tutorialSpecimens },
  { id: "screen.project", title: "Project dialogs", specimens: projectSpecimens },
];
