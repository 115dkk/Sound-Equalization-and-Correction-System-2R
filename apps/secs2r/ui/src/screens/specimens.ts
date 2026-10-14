// The step screens' gallery sections (docs/packets/M5-screens-common.md).
// Each screen packet adds one import and one entry; ids are
// `screen.<step>.<state>` so CI screenshots them one by one.
import type { SectionDef } from "../gallery/Specimen";
import { connectSpecimens } from "./connect/specimens";
import { systemSpecimens } from "./system/specimens";

export const screenSections: SectionDef[] = [
  { id: "screen.system", title: "Screen · System", specimens: systemSpecimens },
  { id: "screen.connect", title: "Screen · Connect & check", specimens: connectSpecimens },
];
