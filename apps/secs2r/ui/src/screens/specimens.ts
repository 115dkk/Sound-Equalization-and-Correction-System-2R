// The step screens' gallery sections (docs/packets/M5-screens-common.md).
// Each screen packet adds one import and one entry; ids are
// `screen.<step>.<state>` so CI screenshots them one by one.
import type { SectionDef } from "../gallery/Specimen";
import { exportSpecimens } from "./export/specimens";
import { helpSpecimens } from "./help/specimens";
import { projectSpecimens } from "./project/specimens";
import { tutorialSpecimens } from "./tutorial/specimens";

export const screenSections: SectionDef[] = [
  { id: "screen.export", title: "Export (P32)", specimens: exportSpecimens },
  { id: "screen.help", title: "Help (P32)", specimens: helpSpecimens },
  { id: "screen.tutorial", title: "Tutorial engine (P32)", specimens: tutorialSpecimens },
  { id: "screen.project", title: "Project dialogs (P32)", specimens: projectSpecimens },
];
